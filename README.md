# leafread

[![PR validation](https://github.com/cigan1/leafread/actions/workflows/pr-validation.yml/badge.svg)](https://github.com/cigan1/leafread/actions/workflows/pr-validation.yml)
[![crates.io](https://img.shields.io/crates/v/leafread.svg)](https://crates.io/crates/leafread)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**leafread is a terminal Markdown reader (CLI/TUI) for developers and terminal
users who read docs, READMEs, and notes without leaving the shell.** It renders
CommonMark + GFM in a full-screen, keyboard-driven reader, or pipes clean ANSI
to stdout.

**The problem:** `cat` and `less` flatten Markdown into plain text — table
structure collapses, code loses its highlighting, diagrams and math become
unreadable noise, and getting a proper preview usually means leaving the
terminal. leafread solves that by rendering Markdown the way it was meant to be
read — bordered tables, syntax-highlighted code, Unicode math, drawn Mermaid
diagrams, and inline images — **so you can keep your whole reading workflow in
the terminal**, with `Tab` walking you through links and `/` searching the
document.

## Quickstart

```sh
cargo install leafread
leafread README.md
```

That is the shortest working path: install, point it at a file. Prebuilt
binaries, Homebrew, and the full option list are below.

## At a glance

| | |
|---|---|
| **What it is** | A terminal Markdown reader (pager/TUI) and renderer |
| **Who it's for** | Developers and terminal users who read docs, READMEs, and notes without leaving the shell |
| **Input** | Files, directories (file browser), or stdin |
| **Output** | Interactive TUI on a terminal; ANSI to stdout when piped |
| **Platforms** | Linux, macOS, Windows (prebuilt Linux/macOS binaries) |
| **Runtime deps** | None — one static-ish binary |
| **Status** | v0.1.0, early. CLI flags may still change before 1.0 |

### Fit

- Reading READMEs, docs, notes, and changelogs in the terminal
- Browsing a folder of Markdown files
- `cat something.md | leafread` in scripts and pipelines
- Previewing GFM tables, task lists, footnotes, and alerts

### Not a fit

- Editing Markdown (use your editor)
- Exporting to HTML/PDF (use [pandoc](https://pandoc.org) or [mdcat](https://github.com/swsnr/mdcat))
- Pixel-perfect Mermaid rendering (diagrams are drawn as Unicode text; unsupported diagram types fall back to source)

## Features

- **Full CommonMark + GFM**: headings, emphasis, links, images, lists, task
  lists, tables (with alignment), strikethrough, autolinks, footnotes,
  GitHub-style alerts (`> [!WARNING]`), description lists, front matter
- **Code blocks** syntax-highlighted via syntect, with a language label and frame
- **Math**: inline `$x^2$` and display `$$…$$` LaTeX rendered to Unicode
  (`α`, `∑`, `∫`, `√`, superscripts, matrices, accents)
- **Mermaid**: `graph`/`flowchart` and `sequenceDiagram` rendered as box-drawing
  diagrams; unsupported types show the source with a badge
- **Inline images**: Kitty, iTerm2, Sixel, and Unicode half-block protocols via
  [ratatui-image], with an alt-text fallback when graphics are unavailable
- **Search**: `/`, live highlighting, `n`/`N` navigation
- **Table of contents**: `t`, jump to any heading
- **Links**: `Tab` to select, `Enter` to open (OSC-8 hyperlinks in piped output),
  relative links to other Markdown files open in place
- **Directory browser**: run it with no arguments to pick a file
- **Live reload**: `w` (or `--watch`) re-renders when the file changes on disk
- **Read aloud**: `p` speaks the page from where you are reading, highlighting
  each word as it is voiced, with the view following along (`s` stops; `--read`
  starts immediately)
- **Themes**: `dark`, `light`, `mono` (modifiers only), `NO_COLOR` respected

[ratatui-image]: https://github.com/ratatui/ratatui-image

## Install

### Prebuilt binaries

Download the tarball for your platform from the
[latest release](https://github.com/cigan1/leafread/releases/latest):

```sh
tar -xzf leafread-<version>-<target>.tar.gz
install -m 755 leafread ~/.local/bin/
```

### cargo

```sh
cargo install leafread
```

### Homebrew

```sh
brew install cigan1/tap/leafread
```

### From source

```sh
git clone https://github.com/cigan1/leafread
cd leafread
cargo build --release
./target/release/leafread README.md
```

Requires Rust 1.86 or newer.

## Usage

```sh
leafread README.md                  # full-screen reader
leafread docs/                      # browse a folder of Markdown files
cat notes.md | leafread             # read from stdin
leafread --no-tui CHANGELOG.md      # plain ANSI to stdout
leafread --width 72 --theme light README.md
leafread --watch README.md          # live reload while you edit
leafread --no-tui notes.md | less -R
```

### Keyboard shortcuts

| Key | Action |
|---|---|
| `j` / `↓`, `k` / `↑` | Scroll line |
| `d` / `u` | Half page down / up |
| `Space` / `b` | Page down / up |
| `g` / `G` | Top / bottom |
| `/` then `Enter` | Search; `n` / `N` next / previous match |
| `t` | Table of contents |
| `l` | Link list |
| `Tab` / `Shift-Tab` | Select next / previous link |
| `Enter` / `o` | Open selected link |
| `f` | File browser |
| `w` | Toggle live reload |
| `r` | Reload now |
| `m` | Toggle front matter |
| `p` | Read aloud from here; pause / resume while reading |
| `s` | Stop reading aloud |
| `?` | Help overlay |
| `q` | Quit |

### Options

```text
leafread [OPTIONS] [PATH]

  PATH                  Markdown file, directory, or `-` for stdin

  -w, --width <N>      Wrap width for piped output (default: terminal width or 100)
  -t, --theme <THEME>  dark (default), light, or mono
      --no-tui         Render to stdout instead of opening the reader UI
      --color          Always emit ANSI colors in piped output
      --no-color       Never emit ANSI colors
      --no-hyperlinks  Do not emit OSC 8 clickable links
  -W, --watch          Reload the document when the file changes
      --front-matter   Show YAML front matter instead of hiding it
      --read           Start reading aloud when the viewer opens
      --tts <ENGINE>   Speech engine: auto (default), gemini, say, espeak
      --voice <NAME>   Voice (default: Leda for gemini, system voice for say)
      --tts-style <T>  Delivery instruction for Gemini, e.g. "slower and calmer"
  -h, --help           Print help
  -V, --version        Print version
```

## Read aloud

Press `p` in the reader to hear the document from the top of your view onwards:
the word being spoken is highlighted, the rest of the spoken sentence is
tinted, and the view scrolls to follow. `p` pauses and resumes; `s` (or `Esc`)
stops. `leafread --read notes.md` starts reading as soon as the viewer opens.

Speech comes from **Gemini TTS** (voice `Leda` by default) when an API key is
available — `GEMINI_API_KEY`, `GOOGLE_API_KEY`, or a key file at
`~/.ssh/gemini_key` or `~/.config/gemini/key`. Without a key it falls back to
the local system voice: macOS `say`, or `espeak-ng` on Linux (plus `curl` for
the Gemini backend and an audio player such as `afplay`, `paplay`, `aplay`, or
`ffplay`). Choose explicitly with `--tts gemini|say|espeak`, and change the
voice with `--voice` (Gemini voices include `Leda` — the default — `Aoede`,
`Sulafat`, `Achernar`, `Callirrhoe`, and `Kore`).

The page is spoken in short chunks that close at sentence boundaries, then
synthesized in the background while the first chunk plays. TTS engines report
no word timings, so within a chunk each word's duration is estimated from its
length; every new chunk resynchronizes the highlight with the voice. Code
blocks, tables, diagrams, and math are skipped, and narration stops with a
status message if the document changes under it.

## Markdown support

| Feature | Status |
|---|---|
| CommonMark core | ✅ |
| GFM tables, task lists, strikethrough, autolinks | ✅ |
| Footnotes | ✅ |
| GitHub alerts (`> [!NOTE]` …) | ✅ |
| Description lists | ✅ |
| YAML front matter | ✅ (hidden; `m` or `--front-matter` to show) |
| Syntax-highlighted code | ✅ |
| Inline/display math (`$…$`, `$$…$$`, ```` ```math ````) | ✅ (Unicode approximation) |
| Mermaid flowcharts & sequence diagrams | ✅ (Unicode rendering) |
| Other Mermaid types (pie, gantt, class, state, ER, …) | ⚠️ source + badge |
| Inline images (local files) | ✅ (Kitty/iTerm2/Sixel/half-blocks) |
| Remote images (`https://…`) | ⚠️ alt text shown, not downloaded |

## How it works

`leafread` parses with [comrak], converts to a small internal document model,
lays that out into styled, wrapped lines (recording TOC entries, link spans, and
image placements as it goes), and then either drives a [ratatui] UI over those
lines or serializes them to ANSI. See [docs/architecture.md](docs/architecture.md)
for the 5-minute tour.

[comrak]: https://github.com/kivikakk/comrak
[ratatui]: https://ratatui.rs

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the dev
setup, the test commands, and what a good pull request looks like. Good first
issues are labelled [`good first issue`](https://github.com/cigan1/leafread/labels/good%20first%20issue).

## Security

Please report vulnerabilities privately — see [SECURITY.md](SECURITY.md).

## Support

Questions and usage help: [GitHub Discussions](https://github.com/cigan1/leafread/discussions).
Bugs and feature requests: [Issues](https://github.com/cigan1/leafread/issues).
See [SUPPORT.md](SUPPORT.md) for what to expect.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this project shall be dual-licensed as
above, without any additional terms or conditions.
