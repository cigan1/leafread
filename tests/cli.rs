//! End-to-end tests for the CLI, exercising piped mode and exit codes.

use std::io::Write;
use std::process::{Command, Stdio};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_leafread"))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn renders_file_to_stdout() {
    let output = binary()
        .args([
            "--no-tui",
            "--no-color",
            "--width",
            "64",
            &fixture("kitchen-sink.md"),
        ])
        .output()
        .expect("run leafread");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Kitchen Sink"), "{stdout}");
    assert!(stdout.contains("┌"), "{stdout}");
    assert!(stdout.contains("∑ᵢ₌₁ⁿ"), "{stdout}");
    assert!(stdout.contains("• First item"), "{stdout}");
    assert!(stdout.contains("☑ Finished task"), "{stdout}");
}

#[test]
fn reads_from_stdin() {
    let mut child = binary()
        .args(["--no-tui", "--no-color", "--width", "40"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn leafread");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"# Piped heading\n\nHello **world**.\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Piped heading"), "{stdout}");
    assert!(stdout.contains("Hello world."), "{stdout}");
}

#[test]
fn color_output_contains_ansi_escapes() {
    let output = binary()
        .args([
            "--no-tui",
            "--color",
            "--width",
            "60",
            &fixture("kitchen-sink.md"),
        ])
        .output()
        .expect("run leafread");
    assert!(output.status.success());
    assert!(output.stdout.contains(&0x1b));
}

#[test]
fn no_color_output_has_no_escapes() {
    let output = binary()
        .args([
            "--no-tui",
            "--no-color",
            "--no-hyperlinks",
            "--width",
            "60",
            &fixture("kitchen-sink.md"),
        ])
        .output()
        .expect("run leafread");
    assert!(output.status.success());
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn missing_file_fails_with_message() {
    let output = binary()
        .args(["--no-tui", "./does-not-exist.md"])
        .output()
        .expect("run leafread");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no such file or directory"), "{stderr}");
}

#[test]
fn help_lists_key_options() {
    let output = binary().arg("--help").output().expect("run leafread");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--theme"), "{stdout}");
    assert!(stdout.contains("--watch"), "{stdout}");
    assert!(stdout.contains("--no-tui"), "{stdout}");
}

#[test]
fn version_is_reported() {
    let output = binary().arg("--version").output().expect("run leafread");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
}
