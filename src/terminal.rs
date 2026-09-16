//! Terminal probing.
//!
//! The image picker asks the terminal which graphics protocol it speaks by
//! writing a query and reading the reply. Reading that reply is the problem:
//! a terminal that never answers leaves the read blocked forever, and the
//! event loop cannot pick the reply up either — crossterm's reader waits for
//! the rest of a sequence it recognises, so the first keystroke after startup
//! unblocks it and is swallowed. A blocked thread cannot be killed, so the
//! question is asked from a child process, which can.

use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ratatui::crossterm::terminal::{enable_raw_mode, is_raw_mode_enabled};

/// Hidden flag that turns the process into the probing child.
pub const PROBE_FLAG: &str = "--probe-terminal";

/// How long the terminal gets to answer the query. Long enough for a round
/// trip over a slow link, short enough that a terminal which never answers
/// (a bare pty, some CI shells) only delays startup by that much.
const PROBE_TIMEOUT: Duration = Duration::from_millis(600);

/// Safety net: the child must never outlive its parent.
const PROBE_LIFETIME: Duration = Duration::from_secs(3);

/// How long the capability query's thread gets to finish turning raw mode off.
const RAW_MODE_SETTLE: Duration = Duration::from_millis(200);

/// Whether the environment names a terminal that implements the kitty graphics
/// protocol and answers its query silently.
pub fn kitty_graphics_terminal() -> bool {
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    let term = std::env::var("TERM").unwrap_or_default();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    names_kitty_graphics(&term, &term_program)
}

fn names_kitty_graphics(term: &str, term_program: &str) -> bool {
    matches!(term, "xterm-kitty" | "xterm-ghostty") || term_program == "ghostty"
}

/// The capability query runs on a crate thread that turns raw mode off when it
/// is done. Wait for that to land, then turn raw mode on for the viewer: a
/// thread that disables it after the viewer started leaves the terminal
/// reading line-buffered input, where keys only arrive on Enter.
pub fn reclaim_raw_mode() {
    let deadline = Instant::now() + RAW_MODE_SETTLE;
    while is_raw_mode_enabled().unwrap_or(false) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = enable_raw_mode();
}

/// Ask the terminal for a device status report (`CSI 5 n`) and report whether
/// it answered. Only a terminal that answers may be queried for its graphics
/// capabilities; see the module comment.
pub fn answers() -> bool {
    if !io::stdin().is_terminal() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let reply_path =
        std::env::temp_dir().join(format!("leafread-terminal-{}.reply", std::process::id()));
    let _ = fs::remove_file(&reply_path);
    let Ok(mut child) = Command::new(exe)
        .arg(PROBE_FLAG)
        .arg(&reply_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    wait_with_deadline(&mut child, PROBE_TIMEOUT);
    let answered = fs::read(&reply_path).is_ok_and(|reply| reply == b"yes");
    let _ = fs::remove_file(&reply_path);
    answered
}

/// Wait for the child up to `limit`, killing it if it is still reading.
fn wait_with_deadline(child: &mut Child, limit: Duration) {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(_) => break,
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Child half of [`answers`]: read the terminal's reply, then write `yes`/`no`.
///
/// Runs in its own process so the parent can kill it mid-read, and exits the
/// process outright so a blocked reader thread cannot linger.
pub fn answer_probe(reply_path: &Path) -> io::Result<()> {
    std::thread::spawn(|| {
        std::thread::sleep(PROBE_LIFETIME);
        std::process::exit(2);
    });

    let answered = read_status_report().unwrap_or(false);
    // The parent enables raw mode again for the viewer.
    let _ = ratatui::crossterm::terminal::disable_raw_mode();
    fs::write(
        reply_path,
        if answered {
            b"yes".as_slice()
        } else {
            b"no".as_slice()
        },
    )?;
    std::process::exit(0)
}

/// Write the query and read until the terminal reports its status.
fn read_status_report() -> io::Result<bool> {
    ratatui::crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b[5n")?;
    stdout.flush()?;

    let mut seen = Vec::new();
    let mut buffer = [0u8; 64];
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    loop {
        let read = stdin.read(&mut buffer)?;
        if read == 0 {
            return Ok(false);
        }
        seen.extend_from_slice(&buffer[..read]);
        if status_report(&seen) {
            return Ok(true);
        }
    }
}

/// Whether `bytes` contain a device status report: `CSI [ ? ] digits n`.
fn status_report(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while let Some(start) = rest.iter().position(|byte| *byte == 0x1b) {
        rest = &rest[start..];
        let mut index = 1;
        if rest.get(index) != Some(&b'[') {
            rest = &rest[1..];
            continue;
        }
        index += 1;
        if rest.get(index) == Some(&b'?') {
            index += 1;
        }
        let digits = index;
        while rest.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index > digits && rest.get(index) == Some(&b'n') {
            return true;
        }
        rest = &rest[1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_report_matches_a_device_status_reply() {
        assert!(status_report(b"\x1b[0n"));
        assert!(status_report(b"\x1b[12n"));
        assert!(status_report(b"\x1b[?0n"));
        assert!(status_report(b"noise\x1b[0nmore"));
        assert!(status_report(b"\x1b[1;1R\x1b[0n"));
    }

    #[test]
    fn status_report_ignores_other_input() {
        assert!(!status_report(b""));
        assert!(!status_report(b"n"));
        assert!(!status_report(b"hello\n"));
        assert!(!status_report(b"\x1b[0m"));
        assert!(!status_report(b"\x1b[?25l"));
        assert!(!status_report(b"\x1b["));
        assert!(!status_report(b"\x1b[n"));
        assert!(!status_report(b"\x1b[1;1R"));
    }

    #[test]
    fn only_terminals_that_answer_the_kitty_query_silently_are_named() {
        assert!(names_kitty_graphics("xterm-kitty", ""));
        assert!(names_kitty_graphics("xterm-ghostty", ""));
        assert!(names_kitty_graphics("", "ghostty"));
        assert!(!names_kitty_graphics("xterm-256color", "iTerm.app"));
        assert!(!names_kitty_graphics("tmux-256color", "tmux"));
        assert!(!names_kitty_graphics("xterm-256color", "WezTerm"));
        assert!(!names_kitty_graphics("screen-256color", ""));
    }
}
