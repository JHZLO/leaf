//! The strings leaf copies: absolute paths, paths relative to the open folder, and `path:line`.

use std::{ffi::OsStr, path::Path};

use unicode_normalization::UnicodeNormalization;

/// A file name as shown and copied. macOS can hand back Hangul in decomposed form (NFD), which
/// pastes as loose jamo elsewhere; APFS finds the file by either form, so NFC is always safe.
pub fn display_name(name: &OsStr) -> String {
    name.to_string_lossy().nfc().collect()
}

/// `root` joined with `rel`, as one NFC string.
pub fn absolute(root: &Path, rel: &Path) -> String {
    root.join(rel).to_string_lossy().nfc().collect()
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
