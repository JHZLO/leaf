//! The icons in front of tree rows, chosen by file type as IntelliJ does: Lucide outlines that come
//! with gpui-kit (ISC). They take no color of their own; color in the tree is the route's.

use std::borrow::Cow;

use gpui_kit::{assets::IconName, *};

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
        Settings,
    ]
);

/// gpui-kit's icons and subwy's extra ones.
pub struct SubwyAssets;

impl AssetSource for SubwyAssets {
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

/// The icon for a folder: open or closed.
pub fn folder_icon(expanded: bool) -> IconName {
    if expanded {
        IconName::FolderOpen
    } else {
        IconName::Folder
    }
}

/// The icon for a file, from its name: a code page, braces for JSON, a cog for settings, and so on.
/// Anything else is a plain page.
pub fn file_icon(name: &str) -> IconName {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" | ".dockerignore" | "docker-compose.yml" | "docker-compose.yaml" => {
            return IconName::Container;
        }
        "makefile" | "justfile" => return IconName::FileTerminal,
        _ => {}
    }
    if name.starts_with(".env") || name.starts_with(".git") {
        return IconName::FileCog;
    }
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension {
        "rs" | "kt" | "kts" | "java" | "html" | "htm" | "xml" | "ts" | "tsx" | "py" | "css"
        | "scss" | "js" | "jsx" | "mjs" | "cjs" | "go" | "c" | "h" | "cpp" | "swift" => {
            IconName::FileCode
        }
        "sh" | "bash" | "zsh" | "fish" => IconName::FileTerminal,
        "md" | "markdown" | "txt" | "rst" | "csv" => IconName::FileText,
        "json" | "jsonc" => IconName::FileBraces,
        "yaml" | "yml" | "toml" | "ini" | "conf" | "properties" | "gradle" | "editorconfig" => {
            IconName::FileCog
        }
        "sql" => IconName::Database,
        "lock" => IconName::FileLock,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "bmp" => IconName::FileImage,
        _ => IconName::File,
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings gpui's `test` attribute, which shadows the standard one.
    use gpui_kit::assets::IconName;

    use super::{file_icon, folder_icon};

    #[test]
    fn files_get_the_icon_of_their_type() {
        assert_eq!(file_icon("main.rs"), IconName::FileCode);
        assert_eq!(file_icon("Booking.KT"), IconName::FileCode);
        assert_eq!(file_icon("README.md"), IconName::FileText);
        assert_eq!(file_icon("package.json"), IconName::FileBraces);
        assert_eq!(file_icon("Cargo.lock"), IconName::FileLock);
        assert_eq!(file_icon("query.sql"), IconName::Database);
        assert_eq!(file_icon("Dockerfile"), IconName::Container);
        assert_eq!(file_icon(".gitignore"), IconName::FileCog);
        assert_eq!(file_icon(".env.local"), IconName::FileCog);
        // No extension, or one subwy does not know: a plain page.
        assert_eq!(file_icon("LICENSE"), IconName::File);
        assert_eq!(file_icon("notes.xyz"), IconName::File);
    }

    #[test]
    fn folders_open() {
        assert_eq!(folder_icon(false), IconName::Folder);
        assert_eq!(folder_icon(true), IconName::FolderOpen);
    }
}
