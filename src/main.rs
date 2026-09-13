//! leafread: a terminal Markdown reader.

mod ansi;
mod cli;
mod markdown;
mod math;
mod mermaid;
mod tui;

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};

use markdown::layout::{self, LayoutOptions};
use markdown::syntax::Highlighter;
use markdown::theme::Theme;

enum Input {
    File(PathBuf),
    Dir(PathBuf),
    Stdin(String),
}

fn main() -> Result<()> {
    let cli = cli::parse();
    let theme = Theme::from_name(&cli.theme);
    let stdout_tty = io::stdout().is_terminal();
    let stdin_tty = io::stdin().is_terminal();

    let input = match &cli.path {
        Some(path) if path.as_os_str() == "-" => Input::Stdin(read_stdin()?),
        Some(path) if path.is_dir() => Input::Dir(path.clone()),
        Some(path) if path.exists() => Input::File(path.clone()),
        Some(path) => anyhow::bail!("{}: no such file or directory", path.display()),
        None if !stdin_tty => Input::Stdin(read_stdin()?),
        None => Input::Dir(std::env::current_dir().context("cannot determine current directory")?),
    };

    if stdout_tty && !cli.no_tui {
        return run_tui(input, theme, &cli);
    }
    run_pipe(input, theme, &cli, stdout_tty)
}

fn run_tui(input: Input, theme: Theme, cli: &cli::Cli) -> Result<()> {
    let initial = match input {
        Input::File(path) => {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            tui::app::Initial {
                path: Some(path),
                raw,
                directory: None,
            }
        }
        Input::Dir(path) => tui::app::Initial {
            path: None,
            raw: String::new(),
            directory: Some(path),
        },
        Input::Stdin(raw) => tui::app::Initial {
            path: None,
            raw,
            directory: None,
        },
    };
    tui::run(initial, theme, cli.watch)
}

fn run_pipe(input: Input, theme: Theme, cli: &cli::Cli, stdout_tty: bool) -> Result<()> {
    let (source, base_dir) = match input {
        Input::File(path) => (
            std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?,
            path.parent().map(PathBuf::from),
        ),
        Input::Dir(path) => anyhow::bail!(
            "{} is a directory; piped output needs a markdown file",
            path.display()
        ),
        Input::Stdin(raw) => (raw, None),
    };
    let _ = base_dir;
    let _ = cli.front_matter;

    let width = cli
        .width
        .or_else(|| {
            stdout_tty
                .then(|| {
                    ratatui::crossterm::terminal::size()
                        .ok()
                        .map(|(w, _)| w as usize)
                })
                .flatten()
        })
        .unwrap_or(100)
        .clamp(20, 400);

    let doc = markdown::parser::parse(&source);
    let highlighter = Highlighter::new(theme.syntax_theme, theme.text);
    let options = LayoutOptions {
        width,
        theme: &theme,
        highlighter: &highlighter,
        show_front_matter: cli.front_matter,
        reserve_image_rows: false,
    };
    let rendered = layout::render(&doc, &options);

    let color = color_enabled(cli, stdout_tty);
    let hyperlinks = !cli.no_hyperlinks && stdout_tty;
    let output = ansi::render(
        &rendered.lines,
        &rendered.links,
        ansi::AnsiOptions { color, hyperlinks },
    );

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(output.as_bytes())?;
    handle.flush()?;
    Ok(())
}

fn color_enabled(cli: &cli::Cli, stdout_tty: bool) -> bool {
    if cli.no_color {
        return false;
    }
    if cli.color {
        return true;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return false;
    }
    stdout_tty
}

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("cannot read from stdin")?;
    Ok(buffer)
}
