//! Read aloud: speak the document with a word-level highlight that follows the
//! voice.
//!
//! The viewer hands the layout's word marks (and the line it wants to start
//! from) to [`Narration::start`]. Words are grouped into short chunks, each
//! chunk is synthesized to audio in the background, and playback advances a
//! word pointer that the UI uses to highlight and auto-scroll. Chunk
//! boundaries keep the highlight in step even though TTS engines report no
//! word timings: within a chunk the time a word takes is estimated from its
//! length, and each new chunk resynchronizes.

mod plan;
mod tts;
mod wav;
mod worker;

use std::ops::Range;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use crate::markdown::layout::WordMark;

pub use plan::MAX_CHUNK_CHARS;
pub use tts::EngineChoice;

/// How to speak.
#[derive(Debug, Clone)]
pub struct Config {
    pub engine: EngineChoice,
    pub voice: Option<String>,
    pub style: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            engine: EngineChoice::Auto,
            voice: None,
            style: tts::DEFAULT_STYLE.to_string(),
        }
    }
}

/// Where narration is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Preparing,
    Playing,
    Paused,
}

/// Screen segments to decorate for one line: the word being spoken and the
/// sentence (chunk) it belongs to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LineMarks {
    pub word: Vec<(usize, usize)>,
    pub sentence: Vec<(usize, usize)>,
}

/// Viewer-side handle to a running narration.
pub struct Narration {
    state: State,
    commands: Option<Sender<worker::Cmd>>,
    events: Option<Receiver<worker::Ev>>,
    handle: Option<JoinHandle<()>>,
    /// Spoken word index -> index in [`crate::markdown::layout::Rendered::words`].
    spoken: Vec<usize>,
    current: usize,
    sentence: Range<usize>,
    engine: String,
    fingerprint: u64,
    error: Option<String>,
}

impl Default for Narration {
    fn default() -> Self {
        Self::new()
    }
}

impl Narration {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            commands: None,
            events: None,
            handle: None,
            spoken: Vec::new(),
            current: 0,
            sentence: 0..0,
            engine: String::new(),
            fingerprint: 0,
            error: None,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn active(&self) -> bool {
        self.state != State::Idle
    }

    /// Index (into the spoken word list) currently being voiced.
    pub fn current(&self) -> Option<usize> {
        (self.state != State::Idle).then_some(self.current)
    }

    /// The mark in [`crate::markdown::layout::Rendered::words`] for spoken word
    /// `index`, if narration is running.
    pub fn source_index(&self, index: usize) -> Option<usize> {
        self.spoken.get(index).copied()
    }

    pub fn engine_label(&self) -> &str {
        &self.engine
    }

    /// Start speaking `marks` from the first word at or below `from_line`.
    pub fn start(
        &mut self,
        marks: &[WordMark],
        from_line: usize,
        config: &Config,
    ) -> Result<(), String> {
        self.stop();
        let from = marks
            .iter()
            .position(|mark| mark.segments.iter().any(|seg| seg.line >= from_line))
            .ok_or_else(|| "no speakable text below this point".to_string())?;
        let plan = plan::Plan::build(marks, from, MAX_CHUNK_CHARS);
        if plan.chunks.is_empty() {
            return Err("nothing to read aloud here".to_string());
        }
        let engine = tts::resolve(config.engine, config.voice.clone(), config.style.clone())?;
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        self.spoken = plan.words.iter().map(|word| word.source).collect();
        self.current = 0;
        self.sentence = 0..self.spoken.len();
        self.fingerprint = fingerprint(marks);
        self.engine = engine.label();
        self.handle = Some(worker::spawn(plan, engine, event_tx, command_rx));
        self.commands = Some(command_tx);
        self.events = Some(event_rx);
        self.state = State::Preparing;
        Ok(())
    }

    pub fn pause(&mut self) {
        if self.state == State::Playing {
            self.send(worker::Cmd::Pause);
            self.state = State::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == State::Paused {
            self.send(worker::Cmd::Resume);
            self.state = State::Playing;
        }
    }

    /// Pause when playing, resume when paused.
    pub fn toggle_pause(&mut self) {
        match self.state {
            State::Playing => self.pause(),
            State::Paused => self.resume(),
            _ => {}
        }
    }

    /// Stop and join the worker, silencing any audio still playing.
    pub fn stop(&mut self) {
        if self.commands.is_some() {
            self.send(worker::Cmd::Stop);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.commands = None;
        self.events = None;
        self.spoken.clear();
        self.sentence = 0..0;
        self.state = State::Idle;
    }

    fn send(&self, command: worker::Cmd) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(command);
        }
    }

    /// Drain worker events; returns true when something changed for the UI.
    pub fn poll(&mut self) -> bool {
        let mut pending = Vec::new();
        if let Some(events) = &self.events {
            while let Ok(event) = events.try_recv() {
                pending.push(event);
            }
        }
        let changed = !pending.is_empty();
        for event in pending {
            self.apply(event);
        }
        changed
    }

    fn apply(&mut self, event: worker::Ev) {
        match event {
            worker::Ev::Started { engine } => self.engine = engine,
            worker::Ev::Sentence { start, end } => {
                self.sentence = start..end;
                if self.state == State::Preparing {
                    self.state = State::Playing;
                }
            }
            worker::Ev::Word(index) => {
                self.current = index;
                self.state = State::Playing;
            }
            worker::Ev::Paused(index) => {
                self.current = index;
                self.state = State::Paused;
            }
            worker::Ev::Finished => self.finish(),
            worker::Ev::Error(message) => {
                self.error = Some(message);
                self.finish();
            }
        }
    }

    fn finish(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.commands = None;
        self.events = None;
        self.state = State::Idle;
    }

    /// Status bar text while narration is active.
    pub fn status(&self) -> Option<String> {
        match self.state {
            State::Idle => None,
            State::Preparing => Some(format!("◌ preparing speech ({}) · s stop", self.engine)),
            State::Playing => Some("▶ reading aloud · p pause · s stop".to_string()),
            State::Paused => Some("⏸ paused · p resume · s stop".to_string()),
        }
    }

    /// Last failure, consumed once by the viewer for a status message.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Test-only: pretend narration is mid-flight, without a backend.
    #[cfg(test)]
    pub(crate) fn debug_state(
        &mut self,
        spoken: Vec<usize>,
        sentence: Range<usize>,
        current: usize,
    ) {
        self.spoken = spoken;
        self.sentence = sentence;
        self.current = current;
        self.state = State::Playing;
    }

    /// Whether `marks` still describe the same word sequence as when narration
    /// started (a relayout that did not change the document text).
    pub fn matches(&self, marks: &[WordMark]) -> bool {
        self.fingerprint == fingerprint(marks)
    }

    /// What to highlight on `line`, if anything.
    pub fn marks_for_line(&self, marks: &[WordMark], line: usize) -> Option<LineMarks> {
        if !self.active() {
            return None;
        }
        let segments = |spoken: &[usize]| -> Vec<(usize, usize)> {
            spoken
                .iter()
                .filter_map(|index| marks.get(*index))
                .flat_map(|mark| mark.segments.iter())
                .filter(|segment| segment.line == line)
                .map(|segment| (segment.start, segment.end))
                .collect()
        };
        let sentence = if self.sentence.end <= self.spoken.len() {
            segments(&self.spoken[self.sentence.clone()])
        } else {
            Vec::new()
        };
        let word: Vec<(usize, usize)> = self
            .spoken
            .get(self.current)
            .and_then(|index| marks.get(*index))
            .map(|mark| {
                mark.segments
                    .iter()
                    .filter(|segment| segment.line == line)
                    .map(|segment| (segment.start, segment.end))
                    .collect()
            })
            .unwrap_or_default();
        if word.is_empty() && sentence.is_empty() {
            None
        } else {
            Some(LineMarks { word, sentence })
        }
    }
}

/// Order-sensitive hash of the word sequence, used to notice document changes.
fn fingerprint(marks: &[WordMark]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for mark in marks {
        for byte in mark.text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::layout::Segment;

    fn marks(words: &[(&str, usize)]) -> Vec<WordMark> {
        words
            .iter()
            .map(|(text, line)| WordMark {
                text: text.to_string(),
                segments: vec![Segment {
                    line: *line,
                    start: 0,
                    end: text.chars().count(),
                }],
            })
            .collect()
    }

    #[test]
    fn idle_narration_has_no_status_or_marks() {
        let narration = Narration::new();
        assert!(!narration.active());
        assert!(narration.status().is_none());
        assert!(narration.marks_for_line(&marks(&[("hi", 0)]), 0).is_none());
    }

    #[test]
    fn fingerprint_notices_word_changes() {
        let a = marks(&[("alpha", 0), ("beta", 0)]);
        let b = marks(&[("alpha", 0), ("gamma", 0)]);
        let c = marks(&[("alpha", 0)]);
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&a), fingerprint(&c));
    }

    #[test]
    fn start_fails_without_text_below_the_line() {
        let mut narration = Narration::new();
        let err = narration
            .start(&marks(&[("hi", 0)]), 10, &Config::default())
            .unwrap_err();
        assert!(err.contains("no speakable text"), "{err}");
        assert!(!narration.active());
    }

    #[test]
    fn marks_report_the_spoken_word_and_its_sentence() {
        let mut narration = Narration::new();
        narration.debug_state(vec![0, 1, 2], 0..3, 1);
        let marks = marks(&[("alpha", 0), ("beta", 1), ("gamma", 1)]);
        let line = narration.marks_for_line(&marks, 1).unwrap();
        assert_eq!(line.word, vec![(0, 4)], "beta is the spoken word");
        assert_eq!(line.sentence, vec![(0, 4), (0, 5)], "whole chunk is hinted");
        assert!(narration.marks_for_line(&marks, 2).is_none());
        assert!(
            narration
                .marks_for_line(&marks, 9)
                .is_none_or(|marks| marks.word.is_empty() && marks.sentence.is_empty())
        );
    }

    /// End-to-end check of the audio pipeline. Plays a few seconds aloud and
    /// needs a real backend, so it is not part of the normal test run:
    /// `cargo test -- --ignored narration::tests::speaks_end_to_end`
    #[test]
    #[ignore = "plays audio through the speakers"]
    fn speaks_end_to_end() {
        let source = "leafread renders Markdown in the terminal. \
                      Pressing p reads the page aloud, one highlighted word at a time.";
        let marks = crate::markdown::layout::render(
            &crate::markdown::parser::parse(source),
            &crate::markdown::layout::LayoutOptions {
                width: 60,
                theme: &crate::markdown::theme::Theme::dark(),
                highlighter: &crate::markdown::syntax::Highlighter::plain(
                    ratatui::style::Style::default(),
                ),
                show_front_matter: false,
                reserve_image_rows: false,
            },
        );
        assert!(!marks.words.is_empty());

        let mut narration = Narration::new();
        narration
            .start(&marks.words, 0, &Config::default())
            .expect("start narration");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let mut words: Vec<usize> = Vec::new();
        let mut finished = false;
        while std::time::Instant::now() < deadline {
            if narration.poll() {
                if let Some(error) = narration.take_error() {
                    panic!("narration failed: {error}");
                }
                if narration.state() == State::Playing
                    && let Some(current) = narration.current()
                    && words.last() != Some(&current)
                {
                    words.push(current);
                }
                if !narration.active() {
                    finished = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        narration.stop();
        assert!(finished, "narration did not finish in time");
        assert!(words.len() > 5, "expected several words, saw {words:?}");
        assert!(
            words.windows(2).all(|pair| pair[1] >= pair[0]),
            "word progress went backwards: {words:?}"
        );
    }
}
