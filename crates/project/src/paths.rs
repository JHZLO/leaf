//! The strings subwy copies: absolute paths, paths relative to the open folder, and `path:line`.

use std::{ffi::OsStr, path::Path};

use unicode_normalization::UnicodeNormalization;

/// A file name as shown and copied. macOS can hand back Hangul in decomposed form (NFD), which
/// pastes as loose jamo elsewhere; APFS finds the file by either form, so NFC is always safe.
pub fn display_name(name: &OsStr) -> String {
    name.to_string_lossy().nfc().collect()
}

/// `path` with the home folder written as `~`, the way a shell prompt shows it.
pub fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `root` joined with `rel`, as one NFC string. The open folder itself (`rel` empty) is `root`
/// without the trailing slash `join` would add.
pub fn absolute(root: &Path, rel: &Path) -> String {
    let path = if rel.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    path.to_string_lossy().nfc().collect()
}

/// `rel` with forward slashes and no trailing slash; the open folder itself is `.`.
pub fn relative(rel: &Path) -> String {
    let text: String = rel.to_string_lossy().nfc().collect();
    if text.is_empty() {
        ".".to_string()
    } else {
        text.trim_end_matches('/').to_string()
    }
}

/// `rel:line`, or `rel:start-end` when the selection spans lines. Lines count from 1.
pub fn with_lines(rel: &Path, start: u32, end: u32) -> String {
    if start == end {
        format!("{}:{start}", relative(rel))
    } else {
        format!("{}:{start}-{end}", relative(rel))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    /// "한글" as macOS may hand it back: decomposed into jamo (NFD).
    const HANGUL_NFD: &str = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}";
    const HANGUL_NFC: &str = "\u{d55c}\u{ae00}";

    #[test]
    fn decomposed_hangul_names_become_composed() {
        let name = format!("{HANGUL_NFD}.md");
        assert_eq!(display_name(OsStr::new(&name)), format!("{HANGUL_NFC}.md"));
        assert_eq!(display_name(OsStr::new("plain.md")), "plain.md");
    }

    #[test]
    fn absolute_paths_join_the_root_in_nfc() {
        let root = Path::new("/Users/me/notes");
        assert_eq!(
            absolute(root, Path::new("docs/a.md")),
            "/Users/me/notes/docs/a.md"
        );
        let rel = format!("docs/{HANGUL_NFD}.md");
        assert_eq!(
            absolute(root, Path::new(&rel)),
            format!("/Users/me/notes/docs/{HANGUL_NFC}.md")
        );
        // The open folder's own row copies the folder, with no trailing slash.
        assert_eq!(absolute(root, Path::new("")), "/Users/me/notes");
    }

    #[test]
    fn relative_paths_have_no_trailing_slash_and_the_root_is_a_dot() {
        assert_eq!(relative(Path::new("docs/a.md")), "docs/a.md");
        assert_eq!(relative(Path::new("docs/guide")), "docs/guide");
        assert_eq!(relative(Path::new("docs/guide/")), "docs/guide");
        assert_eq!(relative(Path::new("")), ".");
        // Spaces stay as they are; whoever pastes decides on quoting.
        assert_eq!(relative(Path::new("my docs/a b.md")), "my docs/a b.md");
        let rel = format!("{HANGUL_NFD}/a.md");
        assert_eq!(relative(Path::new(&rel)), format!("{HANGUL_NFC}/a.md"));
    }

    #[test]
    fn a_line_or_a_span_of_lines_follows_the_path() {
        assert_eq!(with_lines(Path::new("docs/a.md"), 12, 12), "docs/a.md:12");
        assert_eq!(
            with_lines(Path::new("docs/a.md"), 12, 18),
            "docs/a.md:12-18"
        );
        assert_eq!(with_lines(Path::new("a.md"), 1, 1), "a.md:1");
        let rel = format!("{HANGUL_NFD}.md");
        assert_eq!(
            with_lines(Path::new(&rel), 3, 4),
            format!("{HANGUL_NFC}.md:3-4")
        );
    }
}
