//! The folders one window shows. With one folder, paths are relative to it, as they always were.
//! With several, the first component of a path names its folder, as if one folder held them all
//! (IntelliJ shows them as modules under the window's node), so the rest of leaf keeps working
//! with relative paths and only this table knows where they are on disk.

use std::path::{Component, Path, PathBuf};

use crate::{
    fs::{DirEntry, EntryKind},
    paths,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    roots: Vec<PathBuf>,
    /// The name each folder goes by in paths: its own name, or `name (2)` when two share one.
    keys: Vec<String>,
}

impl Folders {
    /// The folders in the order given, without repeats. `roots` must not be empty.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let mut unique: Vec<PathBuf> = Vec::new();
        for root in roots {
            if !unique.contains(&root) {
                unique.push(root);
            }
        }
        let mut keys: Vec<String> = Vec::new();
        for root in &unique {
            let name = root
                .file_name()
                .map_or_else(|| root.display().to_string(), paths::display_name);
            let key = (1..)
                .map(|n| match n {
                    1 => name.clone(),
                    n => format!("{name} ({n})"),
                })
                .find(|key| !keys.contains(key))
                .unwrap_or(name);
            keys.push(key);
        }
        Self {
            roots: unique,
            keys,
        }
    }

    pub fn single(root: PathBuf) -> Self {
        Self::new(vec![root])
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// The folder the window was opened with; it names the window.
    pub fn first(&self) -> &Path {
        &self.roots[0]
    }

    pub fn several(&self) -> bool {
        self.roots.len() > 1
    }

    /// The name each folder goes by in paths, in order.
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// The window's name: its first folder's.
    pub fn name(&self) -> &str {
        &self.keys[0]
    }

    /// The folder holding `rel` and the path inside it. With several folders the window's own node
    /// (the empty path) is in none of them.
    pub fn split(&self, rel: &Path) -> Option<(&Path, PathBuf)> {
        if !self.several() {
            return Some((&self.roots[0], rel.to_path_buf()));
        }
        let mut parts = rel.components();
        let Some(Component::Normal(first)) = parts.next() else {
            return None;
        };
        let index = self.keys.iter().position(|key| first == key.as_str())?;
        Some((&self.roots[index], parts.as_path().to_path_buf()))
    }

    /// The folder holding `rel`, the path inside it, and the prefix that names the folder in `rel`
    /// (empty with one folder): what file operations take, and how to give their results back.
    pub fn mount(&self, rel: &Path) -> Option<(&Path, PathBuf, PathBuf)> {
        let (root, rest) = self.split(rel)?;
        let prefix = if self.several() {
            rel.components().take(1).collect()
        } else {
            PathBuf::new()
        };
        Some((root, rest, prefix))
    }

    /// `rel` on disk.
    pub fn absolute(&self, rel: &Path) -> Option<PathBuf> {
        let (root, rest) = self.split(rel)?;
        Some(if rest.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(rest)
        })
    }

    /// The path leaf uses for `abs`, when one of the folders holds it (the deepest one, if they
    /// nest).
    pub fn relative(&self, abs: &Path) -> Option<PathBuf> {
        let (index, rest) = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(index, root)| Some((index, abs.strip_prefix(root).ok()?)))
            .max_by_key(|(index, _)| self.roots[*index].components().count())?;
        Some(if self.several() {
            Path::new(&self.keys[index]).join(rest)
        } else {
            rest.to_path_buf()
        })
    }

    /// `rel` inside its own folder: what Copy Relative Path copies (IntelliJ's Copy Path From
    /// Content Root). The same as `rel` with one folder.
    pub fn inside(&self, rel: &Path) -> PathBuf {
        match self.split(rel) {
            Some((_, rest)) => rest,
            None => rel.to_path_buf(),
        }
    }

    /// Whether `rel` is one of the folders themselves (their rows cannot be renamed or trashed).
    pub fn is_folder(&self, rel: &Path) -> bool {
        if self.several() {
            rel.components().count() == 1 && self.split(rel).is_some()
        } else {
            rel.as_os_str().is_empty()
        }
    }

    /// The folders as the window node's entries, in their order.
    pub fn entries(&self) -> Vec<DirEntry> {
        self.keys
            .iter()
            .map(|key| DirEntry {
                name: key.clone(),
                kind: EntryKind::Dir,
                ignored: false,
                loops: false,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two() -> Folders {
        Folders::new(vec![
            PathBuf::from("/Users/me/sql-query"),
            PathBuf::from("/Users/me/tripstore"),
        ])
    }

    #[test]
    fn one_folder_keeps_paths_relative_to_it() {
        let one = Folders::single(PathBuf::from("/Users/me/notes"));
        assert!(!one.several());
        assert_eq!(
            one.absolute(Path::new("docs/a.md")),
            Some(PathBuf::from("/Users/me/notes/docs/a.md"))
        );
        assert_eq!(
            one.absolute(Path::new("")),
            Some(PathBuf::from("/Users/me/notes"))
        );
        assert_eq!(
            one.relative(Path::new("/Users/me/notes/docs/a.md")),
            Some(PathBuf::from("docs/a.md"))
        );
        assert_eq!(
            one.inside(Path::new("docs/a.md")),
            PathBuf::from("docs/a.md")
        );
        assert!(one.is_folder(Path::new("")));
    }

    #[test]
    fn several_folders_are_named_by_the_first_component() {
        let folders = two();
        assert_eq!(folders.name(), "sql-query");
        assert_eq!(
            folders.absolute(Path::new("tripstore/docs/a.md")),
            Some(PathBuf::from("/Users/me/tripstore/docs/a.md"))
        );
        assert_eq!(
            folders.absolute(Path::new("sql-query")),
            Some(PathBuf::from("/Users/me/sql-query"))
        );
        // The window's node and unknown names are on no disk.
        assert_eq!(folders.absolute(Path::new("")), None);
        assert_eq!(folders.absolute(Path::new("other/a.md")), None);
        assert_eq!(
            folders.relative(Path::new("/Users/me/tripstore/docs/a.md")),
            Some(PathBuf::from("tripstore/docs/a.md"))
        );
        assert_eq!(
            folders.relative(Path::new("/Users/me/elsewhere/a.md")),
            None
        );
        // Copied relative to its own folder.
        assert_eq!(
            folders.inside(Path::new("tripstore/docs/a.md")),
            PathBuf::from("docs/a.md")
        );
    }

    #[test]
    fn mount_gives_the_folder_the_rest_and_the_prefix() {
        let folders = two();
        let (root, rest, prefix) = folders.mount(Path::new("tripstore/docs")).unwrap();
        assert_eq!(root, Path::new("/Users/me/tripstore"));
        assert_eq!(rest, PathBuf::from("docs"));
        assert_eq!(prefix.join("a.md"), PathBuf::from("tripstore/a.md"));
        let one = Folders::single(PathBuf::from("/n"));
        let (_, rest, prefix) = one.mount(Path::new("docs")).unwrap();
        assert_eq!((rest, prefix), (PathBuf::from("docs"), PathBuf::new()));
    }

    #[test]
    fn folders_with_one_name_are_told_apart() {
        let folders = Folders::new(vec![
            PathBuf::from("/a/tripstore"),
            PathBuf::from("/b/tripstore"),
            PathBuf::from("/a/tripstore"),
        ]);
        assert_eq!(folders.roots().len(), 2);
        let names: Vec<String> = folders.entries().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["tripstore", "tripstore (2)"]);
        assert_eq!(
            folders.absolute(Path::new("tripstore (2)/x")),
            Some(PathBuf::from("/b/tripstore/x"))
        );
    }

    #[test]
    fn a_nested_folder_takes_its_own_files() {
        let folders = Folders::new(vec![PathBuf::from("/work"), PathBuf::from("/work/inner")]);
        assert_eq!(
            folders.relative(Path::new("/work/inner/a.md")),
            Some(PathBuf::from("inner/a.md"))
        );
        assert_eq!(
            folders.relative(Path::new("/work/b.md")),
            Some(PathBuf::from("work/b.md"))
        );
    }

    #[test]
    fn only_the_folders_themselves_are_folder_rows() {
        let folders = two();
        assert!(folders.is_folder(Path::new("tripstore")));
        assert!(!folders.is_folder(Path::new("tripstore/docs")));
        assert!(!folders.is_folder(Path::new("")));
    }
}
