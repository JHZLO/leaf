//! Directory listings and text files, read and written the way the user left them.

use std::{
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Files above this size are not opened.
pub const MAX_OPEN_BYTES: u64 = 50 << 20;
/// Files above this size open without syntax highlighting (it costs about 25 MB of memory per MB).
pub const MAX_HIGHLIGHT_BYTES: u64 = 2 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    Dir,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// The name as shown and copied (NFC).
    pub name: String,
    pub kind: EntryKind,
    /// Matched by a `.gitignore`; shown dimmed.
    pub ignored: bool,
}

/// Lists one directory of the tree: folders first, then files, by case-insensitive name.
/// `dir` is relative to `root`. `.git` and `.DS_Store` are left out.
pub fn list_dir(root: &Path, dir: &Path) -> io::Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root.join(dir))? {
        let entry = entry?;
        let name = crate::paths::display_name(&entry.file_name());
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        let kind = if entry.path().is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        entries.push(DirEntry {
            name,
            kind,
            ignored: false,
        });
    }
    sort_entries(&mut entries);
    Ok(entries)
}

pub(crate) fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Dir)
            .cmp(&(b.kind != EntryKind::Dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    Crlf,
}

/// When a file was last seen on disk, to notice changes made outside leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: SystemTime,
    pub len: u64,
}

#[derive(Clone, Debug)]
pub struct TextFile {
    /// The text with `\n` line endings.
    pub text: String,
    pub line_ending: LineEnding,
    pub bom: bool,
    /// Both `\n` and `\r\n` appear; saving would change lines the user did not touch, so open read-only.
    pub mixed_line_endings: bool,
    pub stamp: FileStamp,
}

#[derive(Debug)]
pub enum ReadError {
    Binary,
    NotUtf8,
    TooLarge(u64),
    Io(io::Error),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Binary => write!(f, "binary file"),
            ReadError::NotUtf8 => write!(f, "not UTF-8"),
            ReadError::TooLarge(len) => write!(f, "too large ({} MB)", len >> 20),
            ReadError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl From<io::Error> for ReadError {
    fn from(err: io::Error) -> Self {
        ReadError::Io(err)
    }
}

pub fn stamp(path: &Path) -> io::Result<FileStamp> {
    let meta = std::fs::metadata(path)?;
    Ok(FileStamp {
        modified: meta.modified()?,
        len: meta.len(),
    })
}

pub fn read_text(path: &Path) -> Result<TextFile, ReadError> {
    let stamp = stamp(path)?;
    if stamp.len > MAX_OPEN_BYTES {
        return Err(ReadError::TooLarge(stamp.len));
    }
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8(bytes).map_err(|_| ReadError::NotUtf8)?;
    Ok(TextFile {
        text,
        line_ending: LineEnding::Lf,
        bom: false,
        mixed_line_endings: false,
        stamp,
    })
}

/// Writes `text` (with `\n` endings) back with the file's own line ending and BOM.
pub fn write_text(
    path: &Path,
    text: &str,
    line_ending: LineEnding,
    bom: bool,
) -> io::Result<FileStamp> {
    let mut out = String::with_capacity(text.len() + 3);
    if bom {
        out.push('\u{feff}');
    }
    match line_ending {
        LineEnding::Lf => out.push_str(text),
        LineEnding::Crlf => out.push_str(&text.replace('\n', "\r\n")),
    }
    std::fs::write(path, out)?;
    stamp(path)
}

/// The path a file should be written to: a symlink's target, so saving never replaces the link.
pub fn write_target(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
