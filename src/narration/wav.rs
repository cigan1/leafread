//! Minimal WAV read/write for 16-bit mono PCM, the only format the speech
//! backends produce and the players consume.

use std::path::Path;

/// 16-bit mono PCM audio at a known sample rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm {
    pub data: Vec<u8>,
    pub rate: u32,
}

impl Pcm {
    pub fn duration_secs(&self) -> f64 {
        self.data.len() as f64 / (self.rate as f64 * 2.0)
    }

    /// Drop everything before `offset` bytes (rounded down to a whole sample).
    pub fn slice_from(&self, offset: usize) -> Pcm {
        let offset = (offset & !1).min(self.data.len());
        Pcm {
            data: self.data[offset..].to_vec(),
            rate: self.rate,
        }
    }
}

/// The canonical 44-byte RIFF header for 16-bit mono PCM.
fn header(data_len: usize, rate: u32) -> Vec<u8> {
    let byte_rate = rate * 2;
    let mut out = Vec::with_capacity(44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    out
}

/// Serialize PCM to a complete WAV file.
pub fn wav_bytes(pcm: &Pcm) -> Vec<u8> {
    let mut out = header(pcm.data.len(), pcm.rate);
    out.extend_from_slice(&pcm.data);
    out
}

pub fn write_file(path: &Path, pcm: &Pcm) -> std::io::Result<()> {
    std::fs::write(path, wav_bytes(pcm))
}

/// Read the PCM data out of a WAV file, tolerating extra chunks.
pub fn read_file(path: &Path) -> Result<Pcm, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("cannot read audio: {err}"))?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Pcm, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut rate = 0u32;
    let mut bits = 0u16;
    let mut channels = 0u16;
    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let body = offset + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " if size >= 16 => {
                let format = u16::from_le_bytes(bytes[body..body + 2].try_into().unwrap());
                if format != 1 {
                    return Err(format!("unsupported WAV encoding {format}"));
                }
                channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap());
                rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap());
                bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap());
            }
            b"data" => {
                if rate == 0 {
                    return Err("WAV data chunk before format chunk".into());
                }
                if channels != 1 || bits != 16 {
                    return Err(format!(
                        "unsupported WAV layout: {channels} channel(s), {bits}-bit"
                    ));
                }
                return Ok(Pcm {
                    data: bytes[body..end].to_vec(),
                    rate,
                });
            }
            _ => {}
        }
        offset = body + size + (size & 1);
    }
    Err("WAV has no data chunk".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_pcm() {
        let pcm = Pcm {
            data: vec![1, 2, 3, 4, 5, 6],
            rate: 24000,
        };
        let parsed = parse(&wav_bytes(&pcm)).unwrap();
        assert_eq!(parsed, pcm);
    }

    #[test]
    fn duration_is_samples_over_rate() {
        let pcm = Pcm {
            data: vec![0; 24000 * 2],
            rate: 24000,
        };
        assert!((pcm.duration_secs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn slice_from_aligns_to_samples() {
        let pcm = Pcm {
            data: (0..10).collect(),
            rate: 24000,
        };
        assert_eq!(pcm.slice_from(3).data, vec![2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(pcm.slice_from(999).data.is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"not audio").is_err());
    }
}
