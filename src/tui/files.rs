//! Markdown file browser for opening a directory.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct FileBrowser {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

impl FileBrowser {
    pub fn new(dir: &Path) -> Self {
        let mut browser = Self {
            dir: dir.to_path_buf(),
            entries: Vec::new(),
            selected: 0,
        };
        browser.refresh();
        browser
    }

    pub fn refresh(&mut self) {
        let mut entries = Vec::new();
        if let Ok(read) = fs::read_dir(&self.dir) {
            for entry in read.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = path.is_dir();
                if is_dir || is_markdown(&path) {
                    entries.push(Entry { name, path, is_dir });
                }
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.entries = entries;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        if !self.entries.is_empty() && self.entries[self.selected].is_dir {
            // Prefer the first file when the selection lands on a directory.
            if let Some(index) = self.entries.iter().position(|e| !e.is_dir) {
                self.selected = index;
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.entries.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.entries.len();
    }

    pub fn page(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as isize;
        let mut index = self.selected as isize + delta;
        index = ((index % len) + len) % len;
        self.selected = index as usize;
    }

    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn enter_dir(&mut self, path: &Path) {
        self.dir = path.to_path_buf();
        self.selected = 0;
        self.refresh();
    }

    pub fn parent(&mut self) -> bool {
        if let Some(parent) = self.dir.parent() {
            let parent = parent.to_path_buf();
            let previous = self.dir.clone();
            self.dir = parent;
            self.selected = 0;
            self.refresh();
            if let Some(index) = self.entries.iter().position(|e| e.path == previous) {
                self.selected = index;
            }
            true
        } else {
            false
        }
    }
}

pub fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdx" | "mdwn" | "text" | "txt")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markdown_extensions() {
        assert!(is_markdown(Path::new("a.md")));
        assert!(is_markdown(Path::new("a.MARKDOWN")));
        assert!(!is_markdown(Path::new("a.png")));
    }

    #[test]
    fn browser_lists_dirs_first_then_markdown() {
        let dir = std::env::temp_dir().join(format!("leafread-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("b.md"), "# b").unwrap();
        fs::write(dir.join("a.md"), "# a").unwrap();
        fs::write(dir.join("ignore.png"), "x").unwrap();

        let browser = FileBrowser::new(&dir);
        let names: Vec<&str> = browser.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["sub", "a.md", "b.md"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
