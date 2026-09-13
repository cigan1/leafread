//! Inline image loading and protocol caching.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use crate::markdown::layout::ImagePlacement;

pub struct ImageCache {
    picker: Option<Picker>,
    protocols: HashMap<usize, Option<StatefulProtocol>>,
}

impl ImageCache {
    pub fn new(picker: Option<Picker>) -> Self {
        Self {
            picker,
            protocols: HashMap::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.picker.is_some()
    }

    /// Get (loading if necessary) the protocol for an image placement.
    /// `None` means the image could not be loaded and the placeholder should
    /// stay visible.
    pub fn protocol(
        &mut self,
        index: usize,
        placement: &ImagePlacement,
        base_dir: &Path,
    ) -> Option<&mut StatefulProtocol> {
        if !self.protocols.contains_key(&index) {
            let protocol = self.load(placement, base_dir);
            self.protocols.insert(index, protocol);
        }
        self.protocols.get_mut(&index).and_then(|p| p.as_mut())
    }

    fn load(&self, placement: &ImagePlacement, base_dir: &Path) -> Option<StatefulProtocol> {
        let picker = self.picker.as_ref()?;
        let src = placement.src.trim();
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
            return None;
        }
        let decoded = percent_decode(src);
        let path = PathBuf::from(&decoded);
        let path = if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        };
        let image = image::open(&path).ok()?;
        Some(picker.new_resize_protocol(image))
    }

    pub fn clear(&mut self) {
        self.protocols.clear();
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_encoded_paths() {
        assert_eq!(percent_decode("a%20b.png"), "a b.png");
        assert_eq!(percent_decode("plain.png"), "plain.png");
        assert_eq!(percent_decode("100%25.png"), "100%.png");
    }

    #[test]
    fn loads_local_images_and_skips_remote() {
        let dir = std::env::temp_dir().join(format!("leafread-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = image::RgbImage::from_pixel(8, 8, image::Rgb([255, 0, 0]));
        image.save(dir.join("dot.png")).unwrap();

        let mut cache = ImageCache::new(Some(Picker::halfblocks()));
        let local = ImagePlacement {
            line: 0,
            rows: 4,
            src: "dot.png".into(),
            alt: String::new(),
        };
        assert!(cache.protocol(0, &local, &dir).is_some());

        let missing = ImagePlacement {
            line: 1,
            rows: 4,
            src: "nope.png".into(),
            alt: String::new(),
        };
        assert!(cache.protocol(1, &missing, &dir).is_none());

        let remote = ImagePlacement {
            line: 2,
            rows: 4,
            src: "https://example.com/a.png".into(),
            alt: String::new(),
        };
        assert!(cache.protocol(2, &remote, &dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
