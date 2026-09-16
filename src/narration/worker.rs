//! Background narration: synthesize chunks, play them in order, and report
//! which word is being spoken.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::plan::Plan;
use super::tts::{self, Engine};
use super::wav::{self, Pcm};

/// Commands the viewer sends to a running narration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Pause,
    Resume,
    /// Jump to a spoken word (index into the plan's words) and carry on there.
    Seek(usize),
    Stop,
}

/// Progress the narration reports back to the viewer.
#[derive(Debug)]
pub enum Ev {
    Started { engine: String },
    Sentence { start: usize, end: usize },
    Word(usize),
    Paused(usize),
    Finished,
    Error(String),
}

const SYNTH_WORKERS: usize = 4;
const POLL: Duration = Duration::from_millis(25);
/// Audio players take a moment to start; shift highlight timing to match.
const PLAYER_START_BIAS: Duration = Duration::from_millis(120);

pub fn spawn(
    plan: Plan,
    engine: Engine,
    events: Sender<Ev>,
    commands: Receiver<Cmd>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || run(plan, engine, events, commands))
}

fn run(plan: Plan, engine: Engine, events: Sender<Ev>, commands: Receiver<Cmd>) {
    let dir = temp_dir();
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        let _ = events.send(Ev::Error(format!("cannot create scratch directory: {err}")));
        return;
    }
    let _ = events.send(Ev::Started {
        engine: engine.label(),
    });

    let chunk_count = plan.chunks.len();
    let tasks: Arc<Mutex<VecDeque<usize>>> = Arc::new(Mutex::new((0..chunk_count).collect()));
    let cancel = Arc::new(AtomicBool::new(false));
    let (result_tx, result_rx) = mpsc::channel::<(usize, Result<Arc<Pcm>, String>)>();
    for _ in 0..chunk_count.min(SYNTH_WORKERS) {
        let tasks = Arc::clone(&tasks);
        let cancel = Arc::clone(&cancel);
        let result_tx = result_tx.clone();
        let engine = engine.clone();
        let plan = plan.clone();
        let dir = dir.clone();
        thread::spawn(move || {
            while !cancel.load(Ordering::Relaxed) {
                let Some(index) = tasks.lock().unwrap().pop_front() else {
                    break;
                };
                let text = plan.chunks[index].text.clone();
                let result = engine.synthesize(&text, &dir, index).map(Arc::new);
                if result_tx.send((index, result)).is_err() {
                    break;
                }
            }
        });
    }
    drop(result_tx);

    // Chunks stay in their slot after playing so `[` can go back without
    // synthesizing again; synthesis already runs ahead of playback, so this
    // does not change what is held in memory.
    let mut slots: Vec<Option<Result<Arc<Pcm>, String>>> = (0..chunk_count).map(|_| None).collect();
    let mut position = (0usize, 0usize); // chunk index, first word within it
    'narrate: loop {
        let (index, from) = position;
        if index >= chunk_count {
            let _ = events.send(Ev::Finished);
            break;
        }
        match await_chunk(&mut slots, index, &commands, &result_rx, &cancel, &events) {
            Wait::Ready => {}
            Wait::Stop => break,
            Wait::Seek(target) => {
                position = locate(&plan, target, index);
                continue 'narrate;
            }
        }
        let Some(Ok(pcm)) = slots[index].clone() else {
            break; // the only failure path already reported the error
        };

        let chunk = &plan.chunks[index];
        let audible = pcm.slice_from(plan.offset_bytes(index, from, pcm.data.len()));
        let file = dir.join(format!("play-{index:04}.wav"));
        if let Err(err) = wav::write_file(&file, &audible) {
            let _ = events.send(Ev::Error(format!("cannot write audio: {err}")));
            break 'narrate;
        }
        let _ = events.send(Ev::Sentence {
            start: chunk.words.start,
            end: chunk.words.end,
        });

        match play(
            &plan,
            index,
            from,
            &file,
            audible.duration_secs(),
            &events,
            &commands,
        ) {
            Outcome::Played => position = (index + 1, 0),
            Outcome::Stopped => break 'narrate,
            Outcome::Seek(target) => position = locate(&plan, target, index),
            Outcome::Paused(word) => match wait_for_resume(&commands, word, &events) {
                Resumed::Stop => break 'narrate,
                Resumed::Resume(target) => position = locate(&plan, target, index),
            },
        }
    }

    cancel.store(true, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Where a spoken word index sits in the plan, keeping the chunk that is
/// already loaded if the index is somehow out of range.
fn locate(plan: &Plan, word: usize, loaded: usize) -> (usize, usize) {
    plan.locate(word).unwrap_or((loaded, 0))
}

/// Outcome of waiting for a chunk to be synthesized.
enum Wait {
    Ready,
    Seek(usize),
    Stop,
}

/// Wait until chunk `index` has been synthesized, buffering results for other
/// chunks. A seek while waiting (the viewer can skip during synthesis, which
/// is slow on Gemini) ends the wait at the requested word.
fn await_chunk(
    slots: &mut [Option<Result<Arc<Pcm>, String>>],
    index: usize,
    commands: &Receiver<Cmd>,
    results: &Receiver<(usize, Result<Arc<Pcm>, String>)>,
    cancel: &AtomicBool,
    events: &Sender<Ev>,
) -> Wait {
    loop {
        if let Some(Err(err)) = &slots[index] {
            let _ = events.send(Ev::Error(err.clone()));
            return Wait::Stop;
        }
        if slots[index].is_some() {
            return Wait::Ready;
        }
        match commands.try_recv() {
            Ok(Cmd::Stop) => return Wait::Stop,
            Ok(Cmd::Seek(target)) => return Wait::Seek(target),
            Err(TryRecvError::Disconnected) => return Wait::Stop,
            _ => {}
        }
        match results.recv_timeout(POLL) {
            Ok((slot, result)) => slots[slot] = Some(result),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if !cancel.load(Ordering::Relaxed) {
                    let _ = events.send(Ev::Error("speech synthesis stopped unexpectedly".into()));
                }
                return Wait::Stop;
            }
        }
    }
}

/// What to do after a pause.
enum Resumed {
    Resume(usize),
    Stop,
}

/// Wait while paused. Seeking moves the pointer without resuming, and reports
/// the new position so the highlight follows; the returned word is where
/// playback starts again.
fn wait_for_resume(commands: &Receiver<Cmd>, word: usize, events: &Sender<Ev>) -> Resumed {
    let mut word = word;
    loop {
        match commands.recv_timeout(Duration::from_millis(200)) {
            Ok(Cmd::Resume) => return Resumed::Resume(word),
            Ok(Cmd::Seek(target)) => {
                word = target;
                let _ = events.send(Ev::Paused(word));
            }
            Ok(Cmd::Stop) => return Resumed::Stop,
            Ok(Cmd::Pause) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Resumed::Stop,
        }
    }
}

enum Outcome {
    Played,
    Stopped,
    Seek(usize),
    Paused(usize),
}

#[allow(clippy::too_many_arguments)]
fn play(
    plan: &Plan,
    chunk: usize,
    from: usize,
    file: &Path,
    duration: f64,
    events: &Sender<Ev>,
    commands: &Receiver<Cmd>,
) -> Outcome {
    let mut child = match tts::play(file) {
        Ok(child) => child,
        Err(err) => {
            let _ = events.send(Ev::Error(err));
            return Outcome::Stopped;
        }
    };
    let started = Instant::now();
    let mut spoken: Option<usize> = None;
    let mut outcome = Outcome::Played;
    loop {
        match commands.try_recv() {
            Ok(Cmd::Pause) => {
                let word = word_now(plan, chunk, from, duration, started);
                stop_child(&mut child);
                let _ = events.send(Ev::Paused(word));
                return Outcome::Paused(word);
            }
            Ok(Cmd::Stop) => {
                stop_child(&mut child);
                return Outcome::Stopped;
            }
            Ok(Cmd::Seek(target)) => {
                stop_child(&mut child);
                return Outcome::Seek(target);
            }
            Err(TryRecvError::Disconnected) => {
                stop_child(&mut child);
                return Outcome::Stopped;
            }
            _ => {}
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let _ = events.send(Ev::Error(format!("audio player failed ({status})")));
                    outcome = Outcome::Stopped;
                }
                break;
            }
            Ok(None) => {}
            Err(_) => break,
        }
        let word = word_now(plan, chunk, from, duration, started);
        if spoken != Some(word) {
            let _ = events.send(Ev::Word(word));
            spoken = Some(word);
        }
        thread::sleep(POLL);
    }
    if let Outcome::Stopped = outcome {
        stop_child(&mut child);
    }
    outcome
}

fn word_now(plan: &Plan, chunk: usize, from: usize, duration: f64, started: Instant) -> usize {
    let elapsed = started
        .elapsed()
        .saturating_sub(PLAYER_START_BIAS)
        .as_secs_f64();
    plan.word_at(chunk, from, elapsed, duration)
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("leafread-narration-{}", std::process::id()))
}
