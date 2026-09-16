# Architecture

A five-minute tour for contributors. leafread is deliberately small: one
pipeline from bytes to styled lines, with two consumers.

```text
             ┌──────────┐   ┌───────────────┐   ┌───────────────────┐
  source ──▶ │  parser  │──▶│  document     │──▶│  layout engine    │
  (file/     │ (comrak) │   │  model        │   │  wrap, tables,    │
   stdin)    └──────────┘   └───────────────┘   │  frames, theme    │
                                                └─────────┬─────────┘
                                                          │ Rendered
                                      ┌───────────────────┴───────────────────┐
                                      ▼                                       ▼
                              ┌───────────────┐                      ┌───────────────┐
                              │  ansi.rs      │                      │  tui/         │
                              │  stdout       │                      │  ratatui app  │
                              └───────────────┘                      └───────────────┘
```

## Modules

| Path | Responsibility |
|---|---|
| `src/cli.rs` | clap command-line definition |
| `src/main.rs` | input selection (file / directory / stdin), TUI vs pipe dispatch, color policy |
| `src/markdown/parser.rs` | comrak AST → document model; turns image-only paragraphs into block images and display math into math blocks |
| `src/markdown/model.rs` | `Document`, `Block`, `Inline`, plain-text extraction |
| `src/markdown/layout.rs` | the core: word wrapping, indentation, lists, tables, code frames, quotes, alerts, TOC/link/image metadata |
| `src/markdown/syntax.rs` | syntect integration and color mapping |
| `src/markdown/theme.rs` | dark/light/mono palettes and chrome styles |
| `src/math.rs` | LaTeX → Unicode approximation |
| `src/mermaid.rs` | Mermaid flowchart/sequence parsing, rank layout, box-drawing canvas |
| `src/ansi.rs` | styled lines → ANSI escapes, OSC 8 hyperlinks |
| `src/tui/app.rs` | viewer state, key handling, search, watcher |
| `src/tui/ui.rs` | painting: header, content, status, overlays |
| `src/tui/files.rs` | Markdown file browser |
| `src/tui/images.rs` | image loading and protocol cache |
| `src/narration/` | read aloud: speech backends, chunk planning, word timing, background playback |

## The layout engine

Everything funnels through `Wrapper` in `src/markdown/layout.rs`. It takes
styled spans and produces wrapped `Line`s while recording:

- **TOC entries** — heading level, title, and line index,
- **link spans** — `(line, start_col, end_col, url)` per wrapped segment,
- **image placements** — line index, row count, source, alt text.

`Builder` composes blocks. Nested content (quotes, list items, alerts,
footnotes) is rendered by a *sub-builder* at a reduced width, then the resulting
lines are re-prefixed and appended with all metadata shifted. This keeps
nesting logic in one place instead of threading indentation through the wrapper.

Wrapping is grapheme-naive but width-correct: `unicode-width` drives column
accounting, long words are hard-split, and links are recorded with terminal
columns so the ANSI serializer can wrap exactly the right characters in OSC 8
sequences.

### Adding a block type

1. Map the comrak node in `parser.rs`.
2. Add the variant to `model.rs`.
3. Render it in `Builder::render_block`.
4. For containers, use `Builder::sub` + `append_prefixed` so metadata shifts
   correctly.

### Adding an inline style

Handle it in `push_inline` (layout.rs). Styles are patched onto the base style:
`base.patch(theme.inline_code)`, `base.add_modifier(Modifier::BOLD)`, and so on.

## The TUI

`App` owns the parsed `Document`, the `Rendered` lines, and all view state.
`relayout()` re-renders at a given width; it runs on load, on resize, and when
a watched file changes (preserving the scroll fraction).

Images are loaded lazily per placement index and cached as ratatui-image
`StatefulProtocol`s. Only visible placements are handed to the terminal, so
scrolling through a large document does not load every image.

The event loop polls crossterm with a 100 ms timeout so file-system events are
noticed promptly without burning CPU.

## Read aloud

`src/narration/` speaks the document while the viewer highlights the word being
voiced. `layout.rs` records every prose word with the screen segments it
occupies (`Rendered::words`), so the viewer can decorate a line without
re-parsing Markdown; table cells, code, diagrams, and rules never enter that
list. `plan.rs` groups the words into chunks that close at sentence punctuation
(~140 characters), and estimates each word's share of its chunk's duration from
word length plus a pause allowance. `tts.rs` synthesizes a chunk through Gemini
TTS (via `curl`, key from the environment or a key file), macOS `say`, or
`espeak-ng`, and plays the audio with a platform player (`afplay`, `paplay`,
`aplay`, `ffplay`, …). `worker.rs` runs synthesis and playback on a background
thread, synthesizing ahead of playback, and reports the spoken word index to
the TUI over a channel; the viewer resolves that index through
`Rendered::words` to paint the word and follow it. TTS engines expose no word
timings, so chunk boundaries are the resynchronization points — the highlight
can lag or lead slightly inside a chunk but snaps back in step at every
sentence.

## Piped mode

When stdout is not a terminal (or `--no-tui` is passed), the same `Rendered`
lines are serialized by `src/ansi.rs`. Style transitions emit a reset plus the
new attributes; hyperlinks are emitted as OSC 8 only when stdout is a terminal
and `--no-hyperlinks` was not passed. No cursor movement is emitted, so output
pipes cleanly into `less -R`, files, or CI logs.

## Testing strategy

- Parser tests assert the document model for representative Markdown.
- Layout tests assert rendered text (joined spans) for wrapping, tables, lists,
  code frames, footnotes, images, math, and Mermaid.
- Mermaid tests assert diagram structure (boxes, arrows, labels) rather than
  exact art, so layout improvements don't churn tests.
- `tests/cli.rs` runs the real binary over fixtures, including stdin, piped
  color policy, exit codes, and `--help`.
