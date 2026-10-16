//! Directory listings and text files, read and written the way the user left them.

use std::{
    io::{self, Read as _},
    path::{Path, PathBuf},
    time::SystemTime,
};

use ignore::{
    Match,
    gitignore::{Gitignore, GitignoreBuilder},
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
    /// A symlinked folder that leads back to the folder holding it or to one above it.
    /// Expanding it would repeat the tree forever, so the tree keeps it closed.
    pub loops: bool,
}

/// Lists one directory of the tree: folders first, then files, by case-insensitive name.
/// `dir` is relative to `root`. `.git` and `.DS_Store` are left out. Entries matched by a
/// `.gitignore` from `root` down to `dir`, or by `.git/info/exclude`, are kept and marked.
pub fn list_dir(root: &Path, dir: &Path) -> io::Result<Vec<DirEntry>> {
    let abs_dir = root.join(dir);
    let ignores = Ignores::for_dir(root, dir);
    let ancestors = canonical_ancestors(root, dir);
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&abs_dir)? {
        let entry = entry?;
        let name = crate::paths::display_name(&entry.file_name());
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        let path = entry.path();
        // `is_dir` follows symlinks, so a link to a folder lists and expands as a folder.
        let is_dir = path.is_dir();
        let loops = is_dir
            && entry.file_type().is_ok_and(|t| t.is_symlink())
            && std::fs::canonicalize(&path)
                .is_ok_and(|target| ancestors.iter().any(|a| a.starts_with(&target)));
        entries.push(DirEntry {
            // Matched by the NFC name: git on macOS precomposes names the same way.
            ignored: ignores.matched(&abs_dir.join(&name), is_dir),
            name,
            kind: if is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            loops,
        });
    }
    sort_entries(&mut entries);
    Ok(entries)
}

pub(crate) fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by(compare_entries);
}

/// The tree's order: folders first, then case-insensitive name, then the exact name so the
/// order never depends on how the disk returned the entries.
pub(crate) fn compare_entries(a: &DirEntry, b: &DirEntry) -> std::cmp::Ordering {
    (a.kind != EntryKind::Dir)
        .cmp(&(b.kind != EntryKind::Dir))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

/// The ignore rules in force inside one folder.
struct Ignores {
    /// Outermost first; a deeper `.gitignore` overrides the ones above it.
    matchers: Vec<Gitignore>,
    /// The folder itself, or one above it, is ignored. Git cannot re-include anything inside
    /// an ignored folder, so every entry is ignored too.
    inherited: bool,
}

impl Ignores {
    fn for_dir(root: &Path, dir: &Path) -> Self {
        let mut exclude = GitignoreBuilder::new(root);
        exclude.add(root.join(".git/info/exclude"));
        let mut ignores = Self {
            matchers: exclude.build().into_iter().collect(),
            inherited: false,
        };
        ignores.push(root);
        let mut current = root.to_path_buf();
        for part in dir.components() {
            current.push(part);
            ignores.inherited = ignores.matched(&current, true);
            ignores.push(&current);
        }
        ignores
    }

    fn push(&mut self, dir: &Path) {
        // A missing or unreadable `.gitignore` simply adds no rules.
        let (matcher, _) = Gitignore::new(dir.join(".gitignore"));
        if !matcher.is_empty() {
            self.matchers.push(matcher);
        }
    }

    fn matched(&self, path: &Path, is_dir: bool) -> bool {
        if self.inherited {
            return true;
        }
        for matcher in self.matchers.iter().rev() {
            match matcher.matched(path, is_dir) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        false
    }
}

/// The real paths of the open folder and of every folder down to `dir`: the tree ancestors of
/// the entries listed in `dir`.
fn canonical_ancestors(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let mut current = root.to_path_buf();
    let mut ancestors: Vec<PathBuf> = std::fs::canonicalize(&current).into_iter().collect();
    for part in dir.components() {
        current.push(part);
        ancestors.extend(std::fs::canonicalize(&current));
    }
    ancestors
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

/// How far to look for a NUL byte to call a file binary (the same probe git uses).
const BINARY_PROBE_BYTES: usize = 8 << 10;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Reads a file for editing. Binary, non-UTF-8 and oversized files are refused. A UTF-8 BOM is
/// stripped and `\r\n` becomes `\n`; both are remembered so saving writes them back.
pub fn read_text(path: &Path) -> Result<TextFile, ReadError> {
    let file = std::fs::File::open(path)?;
    // The stamp comes from the same open file as the text, so it describes what was read.
    let meta = file.metadata()?;
    let stamp = FileStamp {
        modified: meta.modified()?,
        len: meta.len(),
    };
    if stamp.len > MAX_OPEN_BYTES {
        return Err(ReadError::TooLarge(stamp.len));
    }
    let mut bytes = Vec::with_capacity(stamp.len as usize);
    // The file may grow after the size check; never read past the limit.
    file.take(MAX_OPEN_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OPEN_BYTES {
        return Err(ReadError::TooLarge(bytes.len() as u64));
    }
    if bytes[..bytes.len().min(BINARY_PROBE_BYTES)].contains(&0) {
        return Err(ReadError::Binary);
    }
    let bom = bytes.starts_with(UTF8_BOM);
    if bom {
        bytes.drain(..UTF8_BOM.len());
    }
    let text = String::from_utf8(bytes).map_err(|_| ReadError::NotUtf8)?;
    let line_ending = match text.find('\n') {
        Some(i) if text[..i].ends_with('\r') => LineEnding::Crlf,
        _ => LineEnding::Lf,
    };
    let crlf = text.matches("\r\n").count();
    let mixed_line_endings = crlf > 0 && crlf < text.matches('\n').count();
    let text = if crlf > 0 {
        text.replace("\r\n", "\n")
    } else {
        text
    };
    Ok(TextFile {
        text,
        line_ending,
        bom,
        mixed_line_endings,
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

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn names(entries: &[DirEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    fn entry<'a>(entries: &'a [DirEntry], name: &str) -> &'a DirEntry {
        entries.iter().find(|e| e.name == name).unwrap()
    }

    #[test]
    fn lists_folders_first_then_names_ignoring_case() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        for name in ["b.md", "C.md", "a.md", ".env", ".DS_Store", ".gitignore"] {
            write(&root.join(name), "");
        }
        for name in ["zeta", "Alpha", ".git", ".github"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        let entries = list_dir(root, Path::new("")).unwrap();
        assert_eq!(
            names(&entries),
            [
                ".github",
                "Alpha",
                "zeta",
                ".env",
                ".gitignore",
                "a.md",
                "b.md",
                "C.md"
            ]
        );
        assert_eq!(entry(&entries, "zeta").kind, EntryKind::Dir);
        assert_eq!(entry(&entries, "a.md").kind, EntryKind::File);
    }

    #[test]
    fn names_equal_but_for_case_sort_by_exact_name() {
        // APFS is usually case-insensitive, so this case only arrives from other disks.
        let file = |name: &str| DirEntry {
            name: name.to_string(),
            kind: EntryKind::File,
            ignored: false,
            loops: false,
        };
        let mut entries = vec![file("b.md"), file("B.md"), file("a.md")];
        sort_entries(&mut entries);
        assert_eq!(names(&entries), ["a.md", "B.md", "b.md"]);
    }

    #[test]
    fn decomposed_hangul_names_list_composed() {
        let root = tempfile::tempdir().unwrap();
        let nfd = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}.md";
        write(&root.path().join(nfd), "");
        let entries = list_dir(root.path(), Path::new("")).unwrap();
        assert_eq!(names(&entries), ["\u{d55c}\u{ae00}.md"]);
    }

    #[test]
    fn marks_gitignored_entries_but_keeps_them() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        write(&root.join(".gitignore"), "target/\n*.log\n!keep.log\n");
        write(&root.join(".git/info/exclude"), "secret.txt\n");
        write(&root.join("docs/.gitignore"), "draft.md\n!important.log\n");
        for name in [
            "a.log",
            "keep.log",
            "secret.txt",
            "readme.md",
            "target/debug/leaf",
            "docs/draft.md",
            "docs/a.log",
            "docs/important.log",
            "docs/guide.md",
        ] {
            write(&root.join(name), "");
        }
        let ignored = |dir: &str| -> Vec<(String, bool)> {
            list_dir(root, Path::new(dir))
                .unwrap()
                .into_iter()
                .map(|e| (e.name, e.ignored))
                .collect()
        };
        let pairs = |list: &[(&str, bool)]| -> Vec<(String, bool)> {
            list.iter().map(|(n, i)| (n.to_string(), *i)).collect()
        };
        assert_eq!(
            ignored(""),
            pairs(&[
                ("docs", false),
                ("target", true),
                (".gitignore", false),
                ("a.log", true),
                ("keep.log", false),
                ("readme.md", false),
                ("secret.txt", true),
            ])
        );
        assert_eq!(
            ignored("docs"),
            pairs(&[
                (".gitignore", false),
                ("a.log", true),
                ("draft.md", true),
                ("guide.md", false),
                ("important.log", false),
            ])
        );
        // Everything inside an ignored folder is ignored, whatever its name.
        assert_eq!(ignored("target"), pairs(&[("debug", true)]));
        assert_eq!(ignored("target/debug"), pairs(&[("leaf", true)]));
    }

    #[test]
    fn symlinked_folders_that_lead_back_up_are_marked() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = root.path();
        write(&root.join("a/note.md"), "");
        write(&root.join("b/note.md"), "");
        write(&outside.path().join("shared.md"), "");
        symlink(root, root.join("self")).unwrap();
        symlink("..", root.join("a/up")).unwrap();
        symlink("../b", root.join("a/to_b")).unwrap();
        symlink("../a", root.join("b/to_a")).unwrap();
        symlink("note.md", root.join("a/note_link.md")).unwrap();
        symlink(outside.path(), root.join("outside")).unwrap();

        let top = list_dir(root, Path::new("")).unwrap();
        assert!(entry(&top, "self").loops);
        assert!(!entry(&top, "outside").loops);
        assert_eq!(entry(&top, "outside").kind, EntryKind::Dir);
        assert!(!entry(&top, "a").loops);

        let a = list_dir(root, Path::new("a")).unwrap();
        assert!(entry(&a, "up").loops);
        assert!(!entry(&a, "to_b").loops);
        assert_eq!(entry(&a, "note_link.md").kind, EntryKind::File);
        assert!(!entry(&a, "note_link.md").loops);

        // `a/to_b/to_a` is `a` again: an ancestor in the tree, though not on disk.
        let through = list_dir(root, Path::new("a/to_b")).unwrap();
        assert!(entry(&through, "to_a").loops);
    }

    #[test]
    fn reads_lf_text_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        write(&path, "one\ntwo\n");
        let file = read_text(&path).unwrap();
        assert_eq!(file.text, "one\ntwo\n");
        assert_eq!(file.line_ending, LineEnding::Lf);
        assert!(!file.bom);
        assert!(!file.mixed_line_endings);
        assert_eq!(file.stamp, stamp(&path).unwrap());
    }

    #[test]
    fn reads_crlf_text_with_lf_inside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "one\r\ntwo\r\nthree");
        let file = read_text(&path).unwrap();
        assert_eq!(file.text, "one\ntwo\nthree");
        assert_eq!(file.line_ending, LineEnding::Crlf);
        assert!(!file.mixed_line_endings);
    }

    #[test]
    fn text_without_a_newline_is_lf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "one line");
        let file = read_text(&path).unwrap();
        assert_eq!(file.text, "one line");
        assert_eq!(file.line_ending, LineEnding::Lf);
    }

    #[test]
    fn mixed_line_endings_take_the_first_and_are_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "one\ntwo\r\nthree\n");
        let file = read_text(&path).unwrap();
        assert_eq!(file.text, "one\ntwo\nthree\n");
        assert_eq!(file.line_ending, LineEnding::Lf);
        assert!(file.mixed_line_endings);

        write(&path, "one\r\ntwo\n");
        let file = read_text(&path).unwrap();
        assert_eq!(file.line_ending, LineEnding::Crlf);
        assert!(file.mixed_line_endings);
    }

    #[test]
    fn strips_and_remembers_a_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "\u{feff}hello\r\n");
        let file = read_text(&path).unwrap();
        assert_eq!(file.text, "hello\n");
        assert!(file.bom);
        assert_eq!(file.line_ending, LineEnding::Crlf);
    }

    #[test]
    fn a_nul_in_the_first_8_kib_means_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        std::fs::write(&path, b"PK\x03\x04\x00\x00").unwrap();
        assert!(matches!(read_text(&path), Err(ReadError::Binary)));

        let mut late = vec![b'a'; BINARY_PROBE_BYTES];
        late.push(0);
        std::fs::write(&path, &late).unwrap();
        assert_eq!(read_text(&path).unwrap().text.len(), BINARY_PROBE_BYTES + 1);
    }

    #[test]
    fn refuses_text_that_is_not_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("latin1.txt");
        std::fs::write(&path, b"caf\xe9\n").unwrap();
        assert!(matches!(read_text(&path), Err(ReadError::NotUtf8)));
    }

    #[test]
    fn refuses_files_over_the_open_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.log");
        // A sparse file: the size is real, the disk use is not.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_OPEN_BYTES + 1).unwrap();
        assert!(matches!(
            read_text(&path),
            Err(ReadError::TooLarge(len)) if len == MAX_OPEN_BYTES + 1
        ));
        file.set_len(MAX_OPEN_BYTES).unwrap();
        // Exactly at the limit still opens (and the zeros make it binary, not too large).
        assert!(matches!(read_text(&path), Err(ReadError::Binary)));
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            read_text(&dir.path().join("gone.md")),
            Err(ReadError::Io(err)) if err.kind() == io::ErrorKind::NotFound
        ));
    }
}
