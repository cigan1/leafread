//! Speech backends (Gemini TTS, macOS `say`, espeak-ng) and audio playback.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use base64::Engine as _;
use serde_json::json;

use super::wav::{self, Pcm};

/// Voice used by the Gemini backend when none is configured.
pub const DEFAULT_VOICE: &str = "Leda";
/// Delivery instruction sent to the Gemini backend.
pub const DEFAULT_STYLE: &str = "Read this aloud in a warm, natural, conversational \
female voice, at an unhurried pace, like a smart colleague talking you through it";
const GEMINI_MODEL: &str = "gemini-2.5-flash-preview-tts";
const GEMINI_ENDPOINT: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent";
const GEMINI_RATE: u32 = 24000;
const SAY_RATE: u32 = 22050;
const CURL_TIMEOUT: &str = "180";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineChoice {
    /// Gemini when a key is available, otherwise the local voice.
    Auto,
    Gemini,
    Say,
    Espeak,
}

impl EngineChoice {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "auto" => Ok(Self::Auto),
            "gemini" => Ok(Self::Gemini),
            "say" => Ok(Self::Say),
            "espeak" | "espeak-ng" => Ok(Self::Espeak),
            other => Err(format!(
                "unknown TTS engine {other:?} (expected auto, gemini, say, or espeak)"
            )),
        }
    }
}

/// A speech backend plus everything needed to call it.
#[derive(Debug, Clone)]
pub enum Engine {
    Gemini {
        voice: String,
        style: String,
        key: String,
    },
    Say {
        voice: Option<String>,
    },
    Espeak {
        voice: Option<String>,
    },
}

impl Engine {
    pub fn label(&self) -> String {
        match self {
            Engine::Gemini { voice, .. } => format!("gemini:{voice}"),
            Engine::Say { voice } => format!("say:{}", voice.as_deref().unwrap_or("system")),
            Engine::Espeak { voice } => {
                format!("espeak:{}", voice.as_deref().unwrap_or("default"))
            }
        }
    }

    /// Synthesize `text` to 16-bit mono PCM. `dir` is a scratch directory the
    /// engine may use, `seq` distinguishes concurrent calls.
    pub fn synthesize(&self, text: &str, dir: &Path, seq: usize) -> Result<Pcm, String> {
        match self {
            Engine::Gemini { voice, style, key } => gemini(text, voice, style, key, dir, seq),
            Engine::Say { voice } => say(text, voice.as_deref(), dir, seq),
            Engine::Espeak { voice } => espeak(text, voice.as_deref(), dir, seq),
        }
    }
}

/// Pick a backend. `voice` overrides the backend default.
pub fn resolve(
    choice: EngineChoice,
    voice: Option<String>,
    style: String,
) -> Result<Engine, String> {
    let gemini = |voice: Option<String>, style: String| -> Result<Engine, String> {
        let key = find_key()
            .ok_or("no Gemini API key: set GEMINI_API_KEY, or put one in ~/.ssh/gemini_key")?;
        Ok(Engine::Gemini {
            voice: voice.unwrap_or_else(|| DEFAULT_VOICE.to_string()),
            style,
            key,
        })
    };
    let engine = match choice {
        EngineChoice::Gemini => gemini(voice, style)?,
        EngineChoice::Say => {
            if find_program(&["say"]).is_none() {
                return Err("`say` is only available on macOS".into());
            }
            Engine::Say { voice }
        }
        EngineChoice::Espeak => {
            if find_program(&["espeak-ng", "espeak"]).is_none() {
                return Err("espeak-ng is not installed".into());
            }
            Engine::Espeak { voice }
        }
        EngineChoice::Auto => {
            if find_key().is_some() && find_program(&["curl"]).is_some() {
                gemini(voice, style)?
            } else {
                local(voice)?
            }
        }
    };
    if !has_player() {
        return Err("no audio player found (looked for afplay, paplay, aplay, ffplay, ...)".into());
    }
    Ok(engine)
}

fn local(voice: Option<String>) -> Result<Engine, String> {
    if cfg!(target_os = "macos") && find_program(&["say"]).is_some() {
        Ok(Engine::Say { voice })
    } else if find_program(&["espeak-ng", "espeak"]).is_some() {
        Ok(Engine::Espeak { voice })
    } else {
        Err("no local speech engine found (install espeak-ng, or set a Gemini API key)".to_string())
    }
}

fn find_key() -> Option<String> {
    for env in ["GEMINI_API_KEY", "GOOGLE_API_KEY"] {
        if let Ok(value) = std::env::var(env)
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }
    for path in ["~/.ssh/gemini_key", "~/.config/gemini/key"] {
        let Some(home) = std::env::var_os("HOME") else {
            continue;
        };
        let full = PathBuf::from(home).join(path.trim_start_matches("~/"));
        if let Ok(key) = std::fs::read_to_string(&full) {
            let key = key.trim();
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
    }
    None
}

/// Locate an executable by name on `PATH` (appending `.exe` on Windows).
pub fn find_program(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let mut candidate = dir.join(name);
            if cfg!(windows) {
                candidate.set_extension("exe");
            }
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn gemini(
    text: &str,
    voice: &str,
    style: &str,
    key: &str,
    dir: &Path,
    seq: usize,
) -> Result<Pcm, String> {
    if find_program(&["curl"]).is_none() {
        return Err("the Gemini backend needs `curl` on PATH".into());
    }
    let prompt = if style.is_empty() {
        text.to_string()
    } else {
        format!("{style}:\n\n{text}")
    };
    let body = json!({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "voiceConfig": {"prebuiltVoiceConfig": {"voiceName": voice}}
            }
        }
    })
    .to_string();

    let payload = dir.join(format!("request-{seq}.json"));
    let config = dir.join(format!("curl-{seq}.conf"));
    let response = dir.join(format!("response-{seq}.json"));
    write_private(&payload, body.as_bytes())?;
    let endpoint = GEMINI_ENDPOINT.replace("{model}", GEMINI_MODEL);
    write_private(
        &config,
        format!(
            "url = \"{}\"\nheader = \"Content-Type: application/json\"\nheader = \"x-goog-api-key: {}\"\n",
            escape_curl(&endpoint),
            escape_curl(key)
        )
        .as_bytes(),
    )?;

    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(1500 * attempt));
        }
        let output = Command::new("curl")
            .args(["--silent", "--show-error", "--fail-with-body"])
            .args(["--max-time", CURL_TIMEOUT])
            .arg("--config")
            .arg(&config)
            .arg("--data-binary")
            .arg(format!("@{}", payload.display()))
            .arg("--output")
            .arg(&response)
            .output()
            .map_err(|err| format!("cannot run curl: {err}"))?;
        if output.status.success() {
            let bytes = std::fs::read(&response)
                .map_err(|err| format!("cannot read Gemini response: {err}"))?;
            return decode_gemini(&bytes);
        }
        last = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let retryable = ["429", "500", "502", "503", "504"]
            .iter()
            .any(|code| last.contains(code));
        if !retryable {
            break;
        }
    }
    Err(format!("Gemini TTS failed: {last}"))
}

fn escape_curl(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn decode_gemini(bytes: &[u8]) -> Result<Pcm, String> {
    let payload: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|err| format!("bad Gemini response: {err}"))?;
    let data = payload
        .get("candidates")
        .and_then(|candidates| candidates.get(0))
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .and_then(|parts| {
            parts.iter().find_map(|part| {
                part.get("inlineData")
                    .and_then(|inline| inline.get("data"))
                    .and_then(|data| data.as_str())
            })
        })
        .ok_or("Gemini returned no audio")?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|err| format!("bad audio payload: {err}"))?;
    if data.is_empty() {
        return Err("Gemini returned empty audio".into());
    }
    Ok(Pcm {
        data,
        rate: GEMINI_RATE,
    })
}

fn say(text: &str, voice: Option<&str>, dir: &Path, seq: usize) -> Result<Pcm, String> {
    let text_file = dir.join(format!("speak-{seq}.txt"));
    let wav_file = dir.join(format!("chunk-{seq}.wav"));
    write_private(&text_file, text.as_bytes())?;
    let mut command = Command::new("say");
    command
        .args(["--file-format=WAVE", "--data-format=LEI16@22050"])
        .arg(format!("--output-file={}", wav_file.display()))
        .arg(format!("--input-file={}", text_file.display()));
    if let Some(voice) = voice {
        command.arg(format!("--voice={voice}"));
    }
    let output = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("cannot run say: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "say failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut pcm = wav::read_file(&wav_file)?;
    if pcm.rate == 0 {
        pcm.rate = SAY_RATE;
    }
    Ok(pcm)
}

fn espeak(text: &str, voice: Option<&str>, dir: &Path, seq: usize) -> Result<Pcm, String> {
    let program = find_program(&["espeak-ng", "espeak"]).ok_or("espeak-ng is not installed")?;
    let text_file = dir.join(format!("speak-{seq}.txt"));
    let wav_file = dir.join(format!("chunk-{seq}.wav"));
    write_private(&text_file, text.as_bytes())?;
    let mut command = Command::new(program);
    command.arg("-s").arg("165").arg("-w").arg(&wav_file);
    if let Some(voice) = voice {
        command.arg("-v").arg(voice);
    }
    command.arg("-f").arg(&text_file);
    let output = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("cannot run espeak-ng: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "espeak-ng failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    wav::read_file(&wav_file)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
    file.write_all(bytes)
        .map_err(|err| format!("cannot write {}: {err}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

/// Whether any supported audio player exists on this machine.
pub fn has_player() -> bool {
    player_command().is_some()
}

/// Start playing `file`, returning the child process so it can be stopped.
pub fn play(file: &Path) -> Result<Child, String> {
    if cfg!(windows) {
        let program = find_program(&["powershell", "powershell.exe", "pwsh"])
            .ok_or("no audio player found")?;
        let quoted = file.display().to_string().replace('\'', "''");
        let script = format!("(New-Object Media.SoundPlayer '{quoted}').PlaySync()");
        return Command::new(program)
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("cannot start audio player: {err}"));
    }
    let (program, args) = player_command()
        .ok_or("no audio player found (looked for afplay, paplay, aplay, ffplay, ...)")?;
    let mut command = Command::new(program);
    command.args(args);
    command.arg(file);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("cannot start audio player: {err}"))
}

fn player_command() -> Option<(PathBuf, Vec<&'static str>)> {
    if cfg!(target_os = "macos") {
        return find_program(&["afplay"]).map(|program| (program, vec![]));
    }
    if cfg!(windows) {
        let program = find_program(&["powershell", "powershell.exe", "pwsh"])?;
        return Some((program, vec!["-NoProfile", "-NonInteractive", "-Command"]));
    }
    let candidates: &[(&str, &[&str])] = &[
        ("pw-play", &[]),
        ("paplay", &[]),
        ("aplay", &["-q"]),
        ("ffplay", &["-nodisp", "-autoexit", "-loglevel", "quiet"]),
        ("mpv", &["--no-video", "--really-quiet"]),
    ];
    for (name, args) in candidates {
        if let Some(program) = find_program(&[name]) {
            return Some((program, args.to_vec()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engine_names() {
        assert_eq!(EngineChoice::parse("auto"), Ok(EngineChoice::Auto));
        assert_eq!(EngineChoice::parse("gemini"), Ok(EngineChoice::Gemini));
        assert_eq!(EngineChoice::parse("espeak-ng"), Ok(EngineChoice::Espeak));
        assert!(EngineChoice::parse("festival").is_err());
    }

    #[test]
    fn decodes_gemini_audio_payload() {
        let pcm = vec![1u8, 2, 3, 4];
        let body = json!({
            "candidates": [{
                "content": {"parts": [{
                    "inlineData": {"data": base64::engine::general_purpose::STANDARD.encode(&pcm)}
                }]}
            }]
        })
        .to_string();
        let decoded = decode_gemini(body.as_bytes()).unwrap();
        assert_eq!(decoded.data, pcm);
        assert_eq!(decoded.rate, GEMINI_RATE);
    }

    #[test]
    fn rejects_gemini_response_without_audio() {
        let body = br#"{"candidates":[{"content":{"parts":[{"text":"hi"}]}}]}"#;
        assert!(decode_gemini(body).is_err());
    }

    #[test]
    fn escapes_curl_config_values() {
        assert_eq!(escape_curl("a\"b\\c"), "a\\\"b\\\\c");
    }
}
