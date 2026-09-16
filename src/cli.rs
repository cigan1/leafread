//! Command-line interface.

use std::path::PathBuf;

use clap::Parser;

/// Read Markdown in your terminal.
///
/// With no arguments, opens a file browser in the current directory. Reads
/// from stdin when input is piped or when PATH is `-`.
#[derive(Debug, Parser)]
#[command(name = "leafread", version, about, max_term_width = 100)]
pub struct Cli {
    /// Markdown file, directory, or `-` for stdin
    pub path: Option<PathBuf>,

    /// Wrap width for piped output (default: terminal width or 100)
    #[arg(short, long)]
    pub width: Option<usize>,

    /// Color theme: dark, light, or mono
    #[arg(short, long, default_value = "dark")]
    pub theme: String,

    /// Render to stdout instead of opening the pager UI
    #[arg(long)]
    pub no_tui: bool,

    /// Always emit ANSI colors in piped output
    #[arg(long, conflicts_with = "no_color")]
    pub color: bool,

    /// Never emit ANSI colors
    #[arg(long)]
    pub no_color: bool,

    /// Reload the document when the file changes
    #[arg(short = 'W', long)]
    pub watch: bool,

    /// Show YAML front matter instead of hiding it
    #[arg(long)]
    pub front_matter: bool,

    /// Do not emit OSC 8 clickable hyperlinks
    #[arg(long)]
    pub no_hyperlinks: bool,

    /// Start reading the document aloud when the viewer opens
    #[arg(long)]
    pub read: bool,

    /// Speech engine: auto (Gemini when a key is set, otherwise local), gemini, say, espeak
    #[arg(long, default_value = "auto", value_name = "ENGINE")]
    pub tts: String,

    /// Voice for the speech engine: Leda (default), Aoede, Sulafat, Achernar, Callirrhoe, Kore
    #[arg(long)]
    pub voice: Option<String>,

    /// Delivery instruction for Gemini TTS, e.g. "slower and calmer"
    #[arg(long, value_name = "TEXT")]
    pub tts_style: Option<String>,
}

/// Parse command-line arguments.
pub fn parse() -> Cli {
    Cli::parse()
}
