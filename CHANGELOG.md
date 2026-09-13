# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-09-13

Initial release.

### Added

- Interactive terminal reader (ratatui) with scrolling, half-page, and page
  navigation.
- Piped mode: renders ANSI to stdout when output is not a terminal, with
  `--width`, `--color`/`--no-color`, and OSC 8 hyperlinks.
- Full CommonMark + GFM: tables with alignment, task lists, footnotes,
  strikethrough, autolinks, description lists, and GitHub-style alerts.
- Syntax-highlighted fenced code blocks via syntect, with language labels and
  framed blocks.
- LaTeX math (inline `$…$`, display `$$…$$`, ```` ```math ````) rendered to
  Unicode, including fractions, roots, scripts, matrices, and accents.
- Mermaid `graph`/`flowchart` and `sequenceDiagram` rendered as Unicode
  box-drawing diagrams; unsupported diagram types fall back to source with a
  badge.
- Inline images for local files via Kitty, iTerm2, Sixel, and Unicode
  half-block protocols (ratatui-image), with alt-text fallback.
- Search (`/`) with live highlighting and `n`/`N` navigation.
- Table of contents (`t`) and link navigation (`Tab`, `Enter`, `l`).
- Directory browser when starting without arguments or on a directory.
- Live reload on file change (`w` toggle, `--watch` flag).
- Themes: `dark`, `light`, `mono`; `NO_COLOR` respected.
- YAML front matter parsing (hidden by default; `--front-matter` to show).

[Unreleased]: https://github.com/cigan1/leafread/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/cigan1/leafread/releases/tag/v0.1.0
