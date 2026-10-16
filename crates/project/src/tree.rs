//! The folder tree as the panel shows it: which folders are open, what each holds, what is selected.
//! Paths here are relative to the open folder; the folder itself is the empty path.

use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
};

use crate::fs::{DirEntry, EntryKind, compare_entries};

/// One visible line of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub kind: EntryKind,
    pub expanded: bool,
    pub ignored: bool,
    /// One per ancestor level, outermost first (`guides[level]`).
    pub guides: Vec<Guide>,
}

/// The indent guide drawn at one ancestor level of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Guide {
    /// Part of the vein: the line from an ancestor of the selection down to the selection's path.
    pub vein: bool,
    /// The vein turns into this row here (this row is on the selection's path).
    pub tick: bool,
}

#[derive(Debug)]
pub struct ProjectTree {
    root: PathBuf,
    listings: HashMap<PathBuf, Vec<DirEntry>>,
    expanded: HashSet<PathBuf>,
    selected: Option<PathBuf>,
}

impl ProjectTree {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            listings: HashMap::new(),
            expanded: HashSet::new(),
            selected: None,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Shows a folder's new contents. A selection that is gone from it moves to a neighbor.
    pub fn set_listing(&mut self, dir: PathBuf, entries: Vec<DirEntry>) {
        let old = self.listings.insert(dir.clone(), entries);
        self.reselect_if_gone(&dir, old.as_deref());
    }

    /// When the selection, or a folder holding it, has vanished from `dir`, the selection moves
    /// to the next item in `dir`, else to the previous one, else up to `dir` itself.
    fn reselect_if_gone(&mut self, dir: &Path, old: Option<&[DirEntry]>) {
        let Some(rest) = self
            .selected
            .as_deref()
            .and_then(|s| s.strip_prefix(dir).ok())
        else {
            return;
        };
        let mut parts = rest.components();
        let Some(Component::Normal(name)) = parts.next() else {
            return;
        };
        let name = name.to_string_lossy();
        let entries = &self.listings[dir];
        if entries.iter().any(|e| e.name == name) {
            return;
        }
        // Where the vanished item would sort now. Its kind decides that (folders come first),
        // so take it from the old listing, or from whether the selection was inside it.
        let gone = old
            .and_then(|old| old.iter().find(|e| e.name == name))
            .cloned()
            .unwrap_or_else(|| DirEntry {
                name: name.to_string(),
                kind: if parts.next().is_some() {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                ignored: false,
                loops: false,
            });
        let at = entries.partition_point(|e| compare_entries(e, &gone).is_lt());
        let neighbor = entries.get(at).or_else(|| entries.get(at.checked_sub(1)?));
        self.selected = match neighbor {
            Some(entry) => Some(dir.join(&entry.name)),
            // The open folder is not a row; with nothing left in it, nothing is selected.
            None if dir.as_os_str().is_empty() => None,
            None => Some(dir.to_path_buf()),
        };
    }

    pub fn listing(&self, dir: &Path) -> Option<&[DirEntry]> {
        self.listings.get(dir).map(Vec::as_slice)
    }

    /// Drops a cached listing; an open folder needs reading again.
    pub fn invalidate(&mut self, dir: &Path) {
        self.listings.remove(dir);
    }

    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    /// Opens a folder. Returns true when its listing has not been read yet.
    pub fn expand(&mut self, dir: &Path) -> bool {
        self.expanded.insert(dir.to_path_buf());
        !self.listings.contains_key(dir)
    }

    /// Closes a folder; a selection inside it moves up to the folder.
    pub fn collapse(&mut self, dir: &Path) {
        self.expanded.remove(dir);
        if self
            .selected
            .as_deref()
            .is_some_and(|s| s != dir && s.starts_with(dir))
        {
            self.selected = Some(dir.to_path_buf());
        }
    }

    pub fn expanded_dirs(&self) -> impl Iterator<Item = &Path> {
        self.expanded.iter().map(PathBuf::as_path)
    }

    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    pub fn select(&mut self, path: Option<PathBuf>) {
        self.selected = path;
    }

    /// The visible rows, depth first, with the vein toward the selection.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        self.push_rows(Path::new(""), 0, &mut rows);
        self.mark_vein(&mut rows);
        rows
    }

    fn push_rows(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let Some(entries) = self.listings.get(dir) else {
            return;
        };
        for entry in entries {
            let path = dir.join(&entry.name);
            let expanded = entry.kind == EntryKind::Dir && self.expanded.contains(&path);
            rows.push(Row {
                path: path.clone(),
                name: entry.name.clone(),
                depth,
                kind: entry.kind,
                expanded,
                ignored: entry.ignored,
                guides: vec![Guide::default(); depth],
            });
            if expanded {
                self.push_rows(&path, depth + 1, rows);
            }
        }
    }

    fn mark_vein(&self, rows: &mut [Row]) {
        let Some(selected) = &self.selected else {
            return;
        };
        // The selection's ancestors at each depth, and the row of each one on the path.
        let chain: Vec<PathBuf> = selected
            .ancestors()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect();
        let chain: Vec<PathBuf> = chain.into_iter().rev().collect();
        let index_of = |path: &Path, rows: &[Row]| rows.iter().position(|r| r.path == path);
        for level in 0..chain.len().saturating_sub(1) {
            let (Some(from), Some(to)) = (
                index_of(&chain[level], rows),
                index_of(&chain[level + 1], rows),
            ) else {
                continue;
            };
            for row in rows.iter_mut().take(to + 1).skip(from + 1) {
                if let Some(guide) = row.guides.get_mut(level) {
                    guide.vein = true;
                }
            }
            if let Some(guide) = rows[to].guides.get_mut(level) {
                guide.tick = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            kind: EntryKind::Dir,
            ignored: false,
            loops: false,
        }
    }

    fn file(name: &str) -> DirEntry {
        DirEntry {
            kind: EntryKind::File,
            ..dir(name)
        }
    }

    fn tree_with(listings: &[(&str, Vec<DirEntry>)]) -> ProjectTree {
        let mut tree = ProjectTree::new(PathBuf::from("/project"));
        for (path, entries) in listings {
            if !path.is_empty() {
                tree.expand(Path::new(path));
            }
            tree.set_listing(PathBuf::from(path), entries.clone());
        }
        tree
    }

    fn selected(tree: &ProjectTree) -> Option<&str> {
        tree.selected().map(|p| p.to_str().unwrap())
    }

    #[test]
    fn a_vanished_selection_moves_to_the_next_item() {
        let mut tree = tree_with(&[("", vec![file("a.md"), file("b.md"), file("c.md")])]);
        tree.select(Some("b.md".into()));
        tree.set_listing("".into(), vec![file("a.md"), file("c.md")]);
        assert_eq!(selected(&tree), Some("c.md"));
    }

    #[test]
    fn a_renamed_selection_moves_to_the_item_now_after_it() {
        let mut tree = tree_with(&[("", vec![file("a.md"), file("b.md"), file("c.md")])]);
        tree.select(Some("b.md".into()));
        tree.set_listing("".into(), vec![file("a.md"), file("bb.md"), file("c.md")]);
        assert_eq!(selected(&tree), Some("bb.md"));
    }

    #[test]
    fn a_vanished_last_item_moves_the_selection_to_the_previous_one() {
        let mut tree = tree_with(&[("", vec![dir("docs"), file("a.md"), file("b.md")])]);
        tree.select(Some("b.md".into()));
        tree.set_listing("".into(), vec![dir("docs"), file("a.md")]);
        assert_eq!(selected(&tree), Some("a.md"));
    }

    #[test]
    fn a_vanished_folder_moves_the_selection_past_the_other_folders() {
        let mut tree = tree_with(&[("", vec![dir("a"), dir("b"), file("a.md")])]);
        tree.select(Some("b".into()));
        tree.set_listing("".into(), vec![dir("a"), file("a.md")]);
        // Folders sort first, so the item after folder `b` is the first file.
        assert_eq!(selected(&tree), Some("a.md"));
    }

    #[test]
    fn a_vanished_only_item_moves_the_selection_to_its_folder() {
        let mut tree = tree_with(&[("", vec![dir("docs")]), ("docs", vec![file("a.md")])]);
        tree.select(Some("docs/a.md".into()));
        tree.set_listing("docs".into(), vec![]);
        assert_eq!(selected(&tree), Some("docs"));
    }

    #[test]
    fn a_vanished_only_top_level_item_leaves_nothing_selected() {
        let mut tree = tree_with(&[("", vec![file("a.md")])]);
        tree.select(Some("a.md".into()));
        tree.set_listing("".into(), vec![]);
        assert_eq!(selected(&tree), None);
    }

    #[test]
    fn a_vanished_folder_takes_the_selection_inside_it_to_a_neighbor() {
        let mut tree = tree_with(&[
            ("", vec![dir("docs"), dir("src"), file("z.md")]),
            ("docs", vec![dir("guide")]),
            ("docs/guide", vec![file("a.md")]),
        ]);
        tree.select(Some("docs/guide/a.md".into()));
        tree.set_listing("".into(), vec![dir("src"), file("z.md")]);
        assert_eq!(selected(&tree), Some("src"));
    }

    #[test]
    fn a_vanished_folder_unknown_before_still_sorts_as_a_folder() {
        // The first listing of the open folder arrives after a session restored the selection.
        let mut tree = ProjectTree::new(PathBuf::from("/project"));
        tree.select(Some("b/note.md".into()));
        tree.set_listing("".into(), vec![dir("a"), dir("c"), file("b.md")]);
        assert_eq!(selected(&tree), Some("c"));
    }

    #[test]
    fn a_new_listing_keeps_a_selection_that_is_still_there() {
        let mut tree = tree_with(&[("", vec![file("a.md"), file("b.md")])]);
        tree.select(Some("b.md".into()));
        tree.set_listing("".into(), vec![file("0.md"), file("a.md"), file("b.md")]);
        assert_eq!(selected(&tree), Some("b.md"));
        // A listing of an unrelated folder leaves it alone too.
        tree.set_listing("other".into(), vec![]);
        assert_eq!(selected(&tree), Some("b.md"));
    }
}
