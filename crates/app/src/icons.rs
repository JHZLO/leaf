//! The icons in front of tree rows and editor tabs, chosen by file type as IntelliJ does: Lucide
//! outlines that come with gpui-kit (ISC), tinted from leaf's palette so they sit on the paper.

use std::borrow::Cow;

use gpui_kit::{assets::IconName, *};

use crate::theme::Palette;

// Only these are embedded beyond gpui-kit's own icons, not the whole catalog.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        File,
        FileCode,
        FileText,
        FileBraces,
        FileCog,
        FileImage,
        FileTerminal,
        FileLock,
        Database,
        Container,
        LocateFixed,
        ChevronsDownUp,
        X,
        Code,
        SquareCode,
        FolderPlus,
        FolderMinus,
    ]
);

/// gpui-kit's icons and leaf's extra ones.
pub struct LeafAssets;

impl AssetSource for LeafAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = ExtraIcons.list(path)?;
        all.extend(gpui_kit::assets::Assets.list(path)?);
        all.sort();
        all.dedup();
        Ok(all)
    }
}

/// Which palette color an icon takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Plain,
    Accent,
    Blue,
    Purple,
    Brown,
    Red,
    Yellow,
    Cyan,
}

impl Tint {
    pub fn color(self, p: &Palette) -> u32 {
        match self {
            Tint::Plain => p.sub,
            Tint::Accent => p.accent,
            Tint::Blue => p.syn_type,
            Tint::Purple => p.syn_keyword,
            Tint::Brown => p.syn_literal,
            Tint::Red => p.ansi[1],
            Tint::Yellow => p.ansi[3],
            Tint::Cyan => p.ansi[6],
        }
    }
}

/// The icon for a folder: open or closed. The open folder's own row takes the accent.
pub fn folder_icon(expanded: bool, root: bool) -> (IconName, Tint) {
    let icon = if expanded {
        IconName::FolderOpen
    } else {
        IconName::Folder
    };
    (icon, if root { Tint::Accent } else { Tint::Plain })
}

/// The icon for a file, from its name: the language's color on a code page, braces for JSON, a
/// cog for settings, and so on. Anything else is a plain page.
pub fn file_icon(name: &str) -> (IconName, Tint) {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" | ".dockerignore" | "docker-compose.yml" | "docker-compose.yaml" => {
            return (IconName::Container, Tint::Blue);
        }
        "makefile" | "justfile" => return (IconName::FileTerminal, Tint::Accent),
        _ => {}
    }
    if name.starts_with(".env") || name.starts_with(".git") {
        return (IconName::FileCog, Tint::Plain);
    }
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension {
        "rs" => (IconName::FileCode, Tint::Brown),
        "kt" | "kts" => (IconName::FileCode, Tint::Purple),
        "java" | "html" | "htm" | "xml" => (IconName::FileCode, Tint::Red),
        "ts" | "tsx" | "py" | "css" | "scss" => (IconName::FileCode, Tint::Blue),
        "js" | "jsx" | "mjs" | "cjs" => (IconName::FileCode, Tint::Yellow),
        "go" | "c" | "h" | "cpp" | "swift" => (IconName::FileCode, Tint::Cyan),
        "sh" | "bash" | "zsh" | "fish" => (IconName::FileTerminal, Tint::Accent),
        "md" | "markdown" => (IconName::FileText, Tint::Accent),
        "txt" | "rst" | "csv" => (IconName::FileText, Tint::Plain),
        "json" | "jsonc" => (IconName::FileBraces, Tint::Yellow),
        "yaml" | "yml" | "toml" | "ini" | "conf" | "properties" | "gradle" | "editorconfig" => {
            (IconName::FileCog, Tint::Cyan)
        }
        "sql" => (IconName::Database, Tint::Blue),
        "lock" => (IconName::FileLock, Tint::Plain),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "bmp" => {
            (IconName::FileImage, Tint::Purple)
        }
        _ => (IconName::File, Tint::Plain),
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings gpui's `test` attribute, which shadows the standard one.
    use gpui_kit::assets::IconName;

    use super::{Tint, file_icon, folder_icon};

    #[test]
    fn files_get_the_icon_of_their_type() {
        assert_eq!(file_icon("main.rs"), (IconName::FileCode, Tint::Brown));
        assert_eq!(file_icon("Booking.KT"), (IconName::FileCode, Tint::Purple));
        assert_eq!(file_icon("README.md"), (IconName::FileText, Tint::Accent));
        assert_eq!(
            file_icon("package.json"),
            (IconName::FileBraces, Tint::Yellow)
        );
        assert_eq!(file_icon("Cargo.lock"), (IconName::FileLock, Tint::Plain));
        assert_eq!(file_icon("query.sql"), (IconName::Database, Tint::Blue));
        assert_eq!(file_icon("Dockerfile"), (IconName::Container, Tint::Blue));
        assert_eq!(file_icon(".gitignore"), (IconName::FileCog, Tint::Plain));
        assert_eq!(file_icon(".env.local"), (IconName::FileCog, Tint::Plain));
        // No extension, or one leaf does not know: a plain page.
        assert_eq!(file_icon("LICENSE"), (IconName::File, Tint::Plain));
        assert_eq!(file_icon("notes.xyz"), (IconName::File, Tint::Plain));
    }

    #[test]
    fn folders_open_and_the_root_stands_out() {
        assert_eq!(folder_icon(false, false), (IconName::Folder, Tint::Plain));
        assert_eq!(
            folder_icon(true, false),
            (IconName::FolderOpen, Tint::Plain)
        );
        assert_eq!(
            folder_icon(true, true),
            (IconName::FolderOpen, Tint::Accent)
        );
    }
}
