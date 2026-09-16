//! Turning on-screen word marks into speakable chunks with word timings.
//!
//! Speech is synthesized one chunk at a time, so each chunk is a natural
//! resynchronization point: when a chunk starts playing, word highlighting is
//! derived from how far playback has advanced through that chunk, weighted by
//! how long each word takes to say.

use std::ops::Range;

use crate::markdown::layout::WordMark;

/// Hard cap on characters per synthesized chunk. Chunks close at sentence
/// punctuation so the voice keeps whole-sentence intonation; the cap only
/// breaks up sentences too long to be spoken in one piece.
pub const MAX_CHUNK_CHARS: usize = 220;

/// Chunks shorter than this keep accumulating sentences, so one-word
/// headings and terse list items do not become choppy little clips.
const MIN_CHUNK_CHARS: usize = 60;

/// A word as it will be spoken, pointing back at its mark in `Rendered::words`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWord {
    pub text: String,
    pub source: usize,
}

/// A run of words synthesized and played as one piece of audio.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// Index range into [`Plan::words`].
    pub words: Range<usize>,
    pub text: String,
    /// Cumulative fractions of the chunk's duration, one entry per word plus a
    /// final `1.0`.
    pub fractions: Vec<f64>,
}

/// Words to speak and how they are grouped into chunks.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub words: Vec<PlannedWord>,
    pub chunks: Vec<Chunk>,
}

impl Plan {
    /// Collect speakable words from `marks` (skipping drawing-only tokens such
    /// as image icons) and group them into chunks of at most `max_chars`.
    /// A chunk closes at a sentence end as soon as it holds `MIN_CHUNK_CHARS`,
    /// so most chunks are exactly one sentence and the voice keeps its natural
    /// intonation; sentences longer than the cap are split at a word boundary.
    pub fn build(marks: &[WordMark], from: usize, max_chars: usize) -> Plan {
        let max_chars = max_chars.max(16);
        let words: Vec<PlannedWord> = marks
            .iter()
            .enumerate()
            .skip(from)
            .filter(|(_, mark)| mark.text.chars().any(char::is_alphanumeric))
            .map(|(source, mark)| PlannedWord {
                text: mark.text.clone(),
                source,
            })
            .collect();

        let min_chars = MIN_CHUNK_CHARS.min(max_chars / 2);
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < words.len() {
            let mut end = start;
            let mut len = 0;
            let mut last_break: Option<usize> = None;
            while end < words.len() {
                let add = words[end].text.chars().count() + usize::from(end > start);
                if end > start && len + add > max_chars {
                    break;
                }
                len += add;
                end += 1;
                if ends_sentence(&words[end - 1].text) {
                    last_break = Some(end);
                    if len >= min_chars {
                        break;
                    }
                }
            }
            let close = match last_break {
                Some(at) if at > start => at,
                _ => end.max(start + 1),
            };
            chunks.push(make_chunk(&words, start..close));
            start = close;
        }
        Plan { words, chunks }
    }

    /// Spoken indices where a sentence begins: where `]` and `[` land. Every
    /// chunk starts one, plus every word that follows sentence punctuation, so
    /// skipping stays a sentence step even inside a merged chunk.
    pub fn sentence_starts(&self) -> Vec<usize> {
        let mut starts: Vec<usize> = self
            .words
            .iter()
            .enumerate()
            .filter(|(index, _)| *index == 0 || ends_sentence(&self.words[index - 1].text))
            .map(|(index, _)| index)
            .collect();
        for chunk in &self.chunks {
            if !starts.contains(&chunk.words.start) {
                starts.push(chunk.words.start);
            }
        }
        starts.sort_unstable();
        starts
    }

    /// Where spoken word `word` sits: the chunk that holds it and the word's
    /// offset within that chunk.
    pub fn locate(&self, word: usize) -> Option<(usize, usize)> {
        let index = self.chunks.partition_point(|chunk| chunk.words.end <= word);
        let chunk = self.chunks.get(index)?;
        (word >= chunk.words.start).then_some((index, word - chunk.words.start))
    }

    /// The word (index into [`Plan::words`]) being spoken `elapsed` seconds
    /// into a chunk of `duration`, starting from word `from` inside it.
    pub fn word_at(&self, chunk: usize, from: usize, elapsed: f64, duration: f64) -> usize {
        let Some(chunk) = self.chunks.get(chunk) else {
            return 0;
        };
        let last = chunk.words.len().saturating_sub(1);
        let from = from.min(last);
        let base = chunk.fractions[from];
        let progress = if duration > 0.0 {
            (elapsed / duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let fraction = base + progress * (1.0 - base);
        let local = chunk
            .fractions
            .partition_point(|x| *x <= fraction)
            .saturating_sub(1)
            .clamp(from, last);
        chunk.words.start + local
    }

    /// Byte offset into a chunk's PCM where word `from` (offset within the
    /// chunk) begins; used to resume playback mid-chunk.
    pub fn offset_bytes(&self, chunk: usize, from: usize, len: usize) -> usize {
        let Some(chunk) = self.chunks.get(chunk) else {
            return 0;
        };
        let fraction = chunk
            .fractions
            .get(from)
            .copied()
            .unwrap_or_else(|| chunk.fractions.last().copied().unwrap_or(0.0));
        ((fraction * len as f64) as usize) & !1
    }
}

fn make_chunk(words: &[PlannedWord], range: Range<usize>) -> Chunk {
    let text = words[range.clone()]
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let mut text = text;
    if !ends_sentence(&text) {
        text.push('.');
    }
    let mut weights: Vec<f64> = words[range.clone()]
        .iter()
        .map(|word| weight(&word.text))
        .collect();
    let total: f64 = weights.iter().sum();
    let mut fractions = Vec::with_capacity(weights.len() + 1);
    let mut running = 0.0;
    for weight in weights.drain(..) {
        fractions.push(running / total);
        running += weight;
    }
    fractions.push(1.0);
    Chunk {
        words: range,
        text,
        fractions,
    }
}

/// Rough speaking cost of a word: length in letters plus a pause for trailing
/// punctuation. Good enough to keep a word-level highlight in step.
fn weight(text: &str) -> f64 {
    let letters = text
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .count()
        .max(1) as f64;
    let trimmed = text.trim_end_matches(['"', '\'', ')', ']', '\u{201d}', '\u{2019}']);
    match trimmed.chars().last() {
        Some('.') | Some('!') | Some('?') | Some('\u{2026}') => letters + 3.0,
        Some(',') | Some(';') | Some(':') | Some('\u{2014}') => letters + 2.0,
        _ => letters,
    }
}

fn ends_sentence(text: &str) -> bool {
    let trimmed = text.trim_end_matches(['"', '\'', ')', ']', '\u{201d}', '\u{2019}']);
    matches!(
        trimmed.chars().last(),
        Some('.') | Some('!') | Some('?') | Some('\u{2026}')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::layout::Segment;

    fn marks(texts: &[&str]) -> Vec<WordMark> {
        texts
            .iter()
            .enumerate()
            .map(|(i, text)| WordMark {
                text: text.to_string(),
                segments: vec![Segment {
                    line: 0,
                    start: i,
                    end: i + 1,
                }],
            })
            .collect()
    }

    #[test]
    fn long_sentences_stay_whole() {
        let sentence = "A sentence that is fairly long but still short enough to be \
spoken as one natural piece of audio without cutting it in half.";
        assert!(sentence.len() > 100 && sentence.len() < MAX_CHUNK_CHARS);
        let words: Vec<&str> = sentence.split(' ').collect();
        let plan = Plan::build(&marks(&words), 0, MAX_CHUNK_CHARS);
        assert_eq!(plan.chunks.len(), 1, "{:?}", plan.chunks);
    }

    #[test]
    fn separate_sentences_become_separate_chunks() {
        let first = "The first sentence is long enough to stand on its own as a chunk.";
        let second = "And this second sentence becomes the next chunk right after it.";
        let plan = Plan::build(&marks(&[first, second]), 0, MAX_CHUNK_CHARS);
        assert_eq!(plan.chunks.len(), 2);
        assert_eq!(plan.chunks[0].text, first);
        assert_eq!(plan.chunks[1].text, second);
    }

    #[test]
    fn short_sentences_merge() {
        let plan = Plan::build(&marks(&["Hi.", "Ok.", "Sure."]), 0, MAX_CHUNK_CHARS);
        assert_eq!(plan.chunks.len(), 1);
        assert_eq!(plan.chunks[0].text, "Hi. Ok. Sure.");
    }

    #[test]
    fn sentences_longer_than_the_cap_split_at_word_boundaries() {
        let words = vec!["alpha"; 80];
        let plan = Plan::build(&marks(&words), 0, MAX_CHUNK_CHARS);
        assert!(plan.chunks.len() > 1);
        for chunk in &plan.chunks {
            assert!(
                chunk.text.chars().count() <= MAX_CHUNK_CHARS + 1,
                "{}",
                chunk.text
            );
        }
    }

    #[test]
    fn chunks_close_at_sentence_boundaries() {
        let plan = Plan::build(
            &marks(&["One", "sentence.", "Two", "more", "words", "here."]),
            0,
            20,
        );
        assert_eq!(plan.chunks.len(), 2);
        assert_eq!(plan.chunks[0].text, "One sentence.");
        assert_eq!(plan.chunks[1].text, "Two more words here.");
        assert_eq!(plan.chunks[0].words, 0..2);
        assert_eq!(plan.chunks[1].words, 2..6);
    }

    #[test]
    fn long_unpunctuated_runs_split_at_the_limit() {
        let words = vec!["alpha"; 40];
        let plan = Plan::build(&marks(&words), 0, 20);
        assert!(plan.chunks.len() > 1);
        for chunk in &plan.chunks {
            assert!(chunk.text.chars().count() <= 21, "{}", chunk.text);
        }
        // every word is spoken exactly once, in order
        let mut seen = 0;
        for chunk in &plan.chunks {
            assert_eq!(chunk.words.start, seen);
            seen = chunk.words.end;
        }
        assert_eq!(seen, plan.words.len());
    }

    #[test]
    fn terms_without_punctuation_get_a_period() {
        let plan = Plan::build(&marks(&["Install", "leafread"]), 0, 40);
        assert_eq!(plan.chunks[0].text, "Install leafread.");
    }

    #[test]
    fn drawing_only_tokens_are_skipped_but_remembered() {
        let plan = Plan::build(&marks(&["\u{1f5bc}", "alt", "text"]), 0, 40);
        assert_eq!(plan.words.len(), 2);
        assert_eq!(plan.words[0].source, 1);
        assert_eq!(plan.words[1].source, 2);
    }

    #[test]
    fn starts_from_the_requested_word() {
        let plan = Plan::build(&marks(&["a.", "b.", "c."]), 2, 40);
        assert_eq!(plan.chunks.len(), 1);
        assert_eq!(plan.chunks[0].text, "c.");
    }

    #[test]
    fn fractions_are_monotonic_and_normalized() {
        let plan = Plan::build(&marks(&["One", "two,", "three."]), 0, 40);
        let fractions = &plan.chunks[0].fractions;
        assert_eq!(fractions.len(), 4);
        assert_eq!(fractions[0], 0.0);
        assert_eq!(*fractions.last().unwrap(), 1.0);
        assert!(fractions.windows(2).all(|w| w[0] < w[1]), "{fractions:?}");
    }

    #[test]
    fn word_at_tracks_progress_through_a_chunk() {
        let plan = Plan::build(&marks(&["alpha.", "beta.", "gamma."]), 0, 60);
        let chunk = 0;
        assert_eq!(plan.word_at(chunk, 0, 0.0, 3.0), 0);
        assert_eq!(plan.word_at(chunk, 0, 2.9, 3.0), 2);
        assert_eq!(plan.word_at(chunk, 0, 9.0, 3.0), 2, "clamps at the end");
        assert_eq!(plan.word_at(chunk, 1, 0.0, 2.0), 1, "resume starts at word");
    }

    #[test]
    fn offset_bytes_is_sample_aligned() {
        let plan = Plan::build(&marks(&["alpha", "beta", "gamma"]), 0, 60);
        assert_eq!(plan.offset_bytes(0, 0, 1000), 0);
        let mid = plan.offset_bytes(0, 1, 1001);
        assert!(mid > 0 && mid % 2 == 0 && mid < 1001, "{mid}");
        assert!(
            plan.offset_bytes(0, 1, 1000) < plan.offset_bytes(0, 2, 1000),
            "later words start later"
        );
    }

    #[test]
    fn sentence_starts_mark_sentences_and_chunk_boundaries() {
        let plan = Plan::build(
            &marks(&["Hi.", "Ok.", "Sure.", "A", "new", "paragraph."]),
            0,
            60,
        );
        // "Hi. Ok. Sure." merges into one chunk, and its later sentences are
        // still landing spots.
        assert_eq!(plan.sentence_starts(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn sentence_starts_include_a_chunk_that_opens_without_punctuation() {
        let plan = Plan::build(&marks(&["Install", "leafread", "then", "read."]), 0, 16);
        assert_eq!(plan.chunks.len(), 2, "{:?}", plan.chunks);
        assert_eq!(plan.sentence_starts(), vec![0, 2]);
    }

    #[test]
    fn locate_finds_the_chunk_holding_a_word() {
        let plan = Plan::build(&marks(&["One.", "Two.", "Three.", "Four.", "Five."]), 0, 12);
        assert_eq!(plan.chunks.len(), 3, "{:?}", plan.chunks);
        assert_eq!(plan.locate(0), Some((0, 0)));
        assert_eq!(plan.locate(1), Some((0, 1)));
        assert_eq!(plan.locate(2), Some((1, 0)));
        assert_eq!(plan.locate(4), Some((2, 0)));
        assert_eq!(plan.locate(5), None, "past the last word");
    }
}
