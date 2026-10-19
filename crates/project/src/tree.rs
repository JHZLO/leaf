//! The folder tree as the panel shows it: which folders are open, what each holds, what is selected.
//! Paths here are relative to the open folder; the folder itself is the empty path, and like
//! IntelliJ it is the first row, so it can be selected (Cmd+N there makes a file at the top).

use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

use crate::fs::{DirEntry, EntryKind, compare_entries, sort_entries};

/// One visible line of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub kind: EntryKind,
    pub expanded: bool,
    pub ignored: bool,
    /// A symlinked folder that leads back up the tree; it never opens.
    pub loops: bool,
}

/// The row a new item named `name` takes in the open folder `dir`: before the first of `dir`'s
/// items that sorts after it, else after everything `dir` shows. A name with a `/` makes a
/// folder first, and sorts as that folder.
pub fn landing(rows: &[Row], dir: &Path, name: &str, kind: EntryKind) -> usize {
    let Some(at) = rows.iter().position(|r| r.path == dir) else {
        return 0;
    };
    let (first, folder) = match name.split_once('/') {
        Some((first, _)) => (first, true),
        None => (name, kind == EntryKind::Dir),
    };
    let entry = |name: &str, kind| DirEntry {
        name: name.to_string(),
        kind,
        ignored: false,
        loops: false,
    };
    let new = entry(first, if folder { EntryKind::Dir } else { kind });
    let depth = rows[at].depth + 1;
    let mut line = at + 1;
    while line < rows.len() && rows[line].depth >= depth {
        let row = &rows[line];
        if row.depth == depth && compare_entries(&new, &entry(&row.name, row.kind)).is_lt() {
            return line;
        }
        line += 1;
    }
    line
}

/// The rows on the way to `path`, outermost first and ending at `path`'s own row: the folder at
/// each depth that holds it, so the row at index `d` is at depth `d`. Empty when `path` has no row.
pub fn route(rows: &[Row], path: &Path) -> Vec<usize> {
    if !rows.iter().any(|r| r.path == path) {
        return Vec::new();
    }
    let mut ancestors: Vec<&Path> = path.ancestors().collect();
    ancestors.reverse();
    // A hidden open folder has no row; every other ancestor of a shown row is shown.
    ancestors
        .into_iter()
        .filter_map(|a| rows.iter().position(|r| r.path == a))
        .collect()
}

#[derive(Debug)]
pub struct ProjectTree {
    root: PathBuf,
    /// The open folder's own row is left out and what it holds starts at the top: a window of
    /// several folders shows the folders side by side, not under a node of its own.
    hide_root: bool,
    listings: HashMap<PathBuf, Vec<DirEntry>>,
    expanded: HashSet<PathBuf>,
    selected: Option<PathBuf>,
}

impl ProjectTree {
    /// The open folder starts open.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            hide_root: false,
            listings: HashMap::new(),
            expanded: HashSet::from([PathBuf::new()]),
            selected: None,
        }
    }

    /// Leaves the open folder's row out (see `hide_root`).
    pub fn hide_root(&mut self) {
        self.hide_root = true;
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
    /// A symlinked folder that loops back up stays closed and needs no listing.
    pub fn expand(&mut self, dir: &Path) -> bool {
        if self.entry(dir).is_some_and(|e| e.loops) {
            return false;
        }
        self.expanded.insert(dir.to_path_buf());
        !self.listings.contains_key(dir)
    }

    /// The listed entry for `path`, when its folder has been read.
    fn entry(&self, path: &Path) -> Option<&DirEntry> {
        let name = path.file_name()?;
        self.listings
            .get(path.parent()?)?
            .iter()
            .find(|e| OsStr::new(&e.name) == name)
    }

    /// Closes a folder; a selection inside it moves up to the folder.
    /// Closes `dir` and every folder inside it, so opening it again shows only what it holds.
    pub fn collapse(&mut self, dir: &Path) {
        self.expanded.retain(|open| !open.starts_with(dir));
        if self
            .selected
            .as_deref()
            .is_some_and(|s| s != dir && s.starts_with(dir))
        {
            self.selected = Some(dir.to_path_buf());
        }
    }

    /// The open folders inside the open folder, which a session keeps. The open folder itself is
    /// left out: it opens again on its own.
    /// Closes every folder but the open folder itself (IntelliJ's Collapse All). A selection inside
    /// one moves up to its top-level folder.
    pub fn collapse_all(&mut self) {
        self.expanded.retain(|dir| dir.as_os_str().is_empty());
        if let Some(selected) = &self.selected
            && let Some(top) = selected.components().next()
        {
            self.selected = Some(PathBuf::from(top.as_os_str()));
        }
    }

    pub fn expanded_dirs(&self) -> impl Iterator<Item = &Path> {
        self.expanded
            .iter()
            .map(PathBuf::as_path)
            .filter(|dir| !dir.as_os_str().is_empty())
    }

    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    pub fn select(&mut self, path: Option<PathBuf>) {
        self.selected = path;
    }

    /// Selects `path` and opens every folder above it. Returns the folders it opened whose
    /// listings still need reading, outermost first. A folder that loops stops the walk: the
    /// tree never shows what is inside it.
    pub fn reveal(&mut self, path: &Path) -> Vec<PathBuf> {
        // The open folder is among them: it may have been closed.
        let mut above: Vec<&Path> = path.ancestors().skip(1).collect();
        above.reverse();
        let mut unread = Vec::new();
        for dir in above {
            if self.entry(dir).is_some_and(|e| e.loops) {
                break;
            }
            if self.expand(dir) {
                unread.push(dir.to_path_buf());
            }
        }
        self.selected = Some(path.to_path_buf());
        unread
    }

    /// Follows a rename subwy made: the item keeps its open folders, their listings and the
    /// selection, and its folder's listing shows the new name before it is read again.
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        let moved = |path: PathBuf| match path.strip_prefix(from) {
            Ok(rest) if rest.as_os_str().is_empty() => to.to_path_buf(),
            Ok(rest) => to.join(rest),
            Err(_) => path,
        };
        self.listings = std::mem::take(&mut self.listings)
            .into_iter()
            .map(|(dir, entries)| (moved(dir), entries))
            .collect();
        self.expanded = std::mem::take(&mut self.expanded)
            .into_iter()
            .map(moved)
            .collect();
        self.selected = self.selected.take().map(moved);
        let (Some(dir), Some(old), Some(new)) = (from.parent(), from.file_name(), to.file_name())
        else {
            return;
        };
        if let Some(entries) = self.listings.get_mut(dir) {
            if let Some(entry) = entries.iter_mut().find(|e| OsStr::new(&e.name) == old) {
                entry.name = crate::paths::display_name(new);
            }
            sort_entries(entries);
        }
    }

    /// Forgets an item that is gone and everything inside it: a new item by the same name
    /// starts closed, without the old contents.
    pub fn forget(&mut self, path: &Path) {
        if path.as_os_str().is_empty() {
            return;
        }
        self.listings.retain(|dir, _| !dir.starts_with(path));
        self.expanded.retain(|dir| !dir.starts_with(path));
    }

    /// The visible rows, depth first. The open folder is the first row, at depth 0.
    pub fn rows(&self) -> Vec<Row> {
        let root = Path::new("");
        if self.hide_root {
            let mut rows = Vec::new();
            self.push_rows(root, 0, &mut rows);
            return rows;
        }
        let expanded = self.expanded.contains(root);
        let name = self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            crate::paths::display_name,
        );
        let mut rows = vec![Row {
            path: PathBuf::new(),
            name,
            depth: 0,
            kind: EntryKind::Dir,
            expanded,
            ignored: false,
            loops: false,
        }];
        if expanded {
            self.push_rows(root, 1, &mut rows);
        }
        rows
    }

    fn push_rows(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let Some(entries) = self.listings.get(dir) else {
            return;
        };
        for entry in entries {
            let path = dir.join(&entry.name);
            // A folder opened before its listing said it loops still stays closed.
            let expanded =
                entry.kind == EntryKind::Dir && !entry.loops && self.expanded.contains(&path);
            rows.push(Row {
                path: path.clone(),
                name: entry.name.clone(),
                depth,
                kind: entry.kind,
                expanded,
                ignored: entry.ignored,
                loops: entry.loops,
            });
            if expanded {
                self.push_rows(&path, depth + 1, rows);
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
    fn a_vanished_only_top_level_item_moves_the_selection_to_the_open_folder() {
        let mut tree = tree_with(&[("", vec![file("a.md")])]);
        tree.select(Some("a.md".into()));
        tree.set_listing("".into(), vec![]);
        assert_eq!(selected(&tree), Some(""));
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

    /// docs/ (open) > guide/ (open) > a.md, b.md; docs/readme.md; src/ (closed); top.md
    fn sample() -> ProjectTree {
        tree_with(&[
            ("", vec![dir("docs"), dir("src"), file("top.md")]),
            ("docs", vec![dir("guide"), file("readme.md")]),
            ("docs/guide", vec![file("a.md"), file("b.md")]),
        ])
    }

    #[test]
    fn rows_walk_open_folders_depth_first() {
        let rows = sample().rows();
        let summary: Vec<(&str, usize, bool)> = rows
            .iter()
            .map(|r| (r.path.to_str().unwrap(), r.depth, r.expanded))
            .collect();
        assert_eq!(
            summary,
            [
                ("", 0, true),
                ("docs", 1, true),
                ("docs/guide", 2, true),
                ("docs/guide/a.md", 3, false),
                ("docs/guide/b.md", 3, false),
                ("docs/readme.md", 2, false),
                ("src", 1, false),
                ("top.md", 1, false),
            ]
        );
        assert_eq!(rows[0].name, "project");
        assert_eq!(rows[0].kind, EntryKind::Dir);
    }

    #[test]
    fn the_open_folder_closes_and_opens_like_any_folder() {
        let mut tree = sample();
        tree.select(Some("docs/readme.md".into()));
        tree.collapse(Path::new(""));
        assert_eq!(selected(&tree), Some(""));
        assert_eq!(summary(&tree), [("".to_string(), false)]);
        // Its listing stays, so opening it again reads nothing; the folders inside stay closed.
        assert!(!tree.expand(Path::new("")));
        assert_eq!(tree.rows().len(), 4);
        assert_eq!(tree.expanded_dirs().count(), 0);
    }

    #[test]
    fn collapsing_a_folder_closes_the_folders_inside() {
        let mut tree = sample();
        tree.collapse(Path::new("docs"));
        assert!(!tree.expand(Path::new("docs")));
        assert_eq!(
            summary(&tree),
            [
                ("", true),
                ("docs", true),
                ("docs/guide", false),
                ("docs/readme.md", false),
                ("src", false),
                ("top.md", false)
            ]
            .map(|(p, e)| (p.to_string(), e))
            .to_vec()
        );
    }

    /// The route's rows, as paths.
    fn route_to(tree: &ProjectTree, path: &str) -> Vec<String> {
        let rows = tree.rows();
        route(&rows, Path::new(path))
            .into_iter()
            .map(|i| rows[i].path.to_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_route_runs_through_each_folder_on_the_way() {
        let tree = sample();
        assert_eq!(
            route_to(&tree, "docs/guide/b.md"),
            ["", "docs", "docs/guide", "docs/guide/b.md"]
        );
        assert_eq!(route_to(&tree, "docs/guide"), ["", "docs", "docs/guide"]);
        assert_eq!(route_to(&tree, "top.md"), ["", "top.md"]);
        assert_eq!(route_to(&tree, ""), [""]);
        // One row per depth, outermost first.
        let rows = tree.rows();
        for (depth, i) in route(&rows, Path::new("docs/readme.md"))
            .into_iter()
            .enumerate()
        {
            assert_eq!(rows[i].depth, depth);
        }
    }

    #[test]
    fn a_new_item_lands_where_it_will_sort() {
        // docs holds guide/ (open: a.md, b.md) and readme.md.
        let rows = sample().rows();
        let at = |name: &str, kind| {
            let line = landing(&rows, Path::new("docs"), name, kind);
            rows.get(line).map(|r| r.path.to_str().unwrap().to_string())
        };
        assert_eq!(
            at("notes.md", EntryKind::File),
            Some("docs/readme.md".into())
        );
        assert_eq!(at("z.md", EntryKind::File), Some("src".into()));
        // Folders come first, so a new folder goes before guide's files are passed.
        assert_eq!(at("api", EntryKind::Dir), Some("docs/guide".into()));
        assert_eq!(at("zz", EntryKind::Dir), Some("docs/readme.md".into()));
        // A name with a slash makes a folder, and sorts as one: drafts/ before guide/.
        assert_eq!(
            at("drafts/idea.md", EntryKind::File),
            Some("docs/guide".into())
        );
        assert_eq!(landing(&rows, Path::new("gone"), "a", EntryKind::File), 0);
    }

    #[test]
    fn no_route_to_a_path_without_a_row() {
        let tree = sample();
        assert!(route_to(&tree, "src/main.rs").is_empty());
        assert!(route_to(&tree, "gone.md").is_empty());
    }

    #[test]
    fn collapsing_a_folder_lifts_the_selection_inside_it() {
        let mut tree = sample();
        tree.select(Some("docs/guide/a.md".into()));
        tree.collapse(Path::new("docs"));
        assert_eq!(selected(&tree), Some("docs"));
        assert_eq!(tree.rows().len(), 4);
        // A selection outside the folder stays.
        tree.select(Some("top.md".into()));
        tree.expand(Path::new("docs"));
        tree.collapse(Path::new("docs"));
        assert_eq!(selected(&tree), Some("top.md"));
    }

    #[test]
    fn a_hidden_root_puts_its_folders_at_the_top_with_their_routes() {
        let mut tree = tree_with(&[("", vec![dir("a"), dir("b")]), ("a", vec![file("x.md")])]);
        tree.hide_root();
        tree.select(Some("a/x.md".into()));
        let rows = tree.rows();
        let summary: Vec<(&str, usize)> = rows
            .iter()
            .map(|r| (r.path.to_str().unwrap(), r.depth))
            .collect();
        assert_eq!(summary, [("a", 0), ("a/x.md", 1), ("b", 0)]);
        // The route starts at the folder's own row.
        assert_eq!(route(&rows, Path::new("a/x.md")), [0, 1]);
    }

    #[test]
    fn collapse_all_leaves_the_top_level_and_lifts_the_selection() {
        let mut tree = sample();
        tree.select(Some("docs/guide/a.md".into()));
        tree.collapse_all();
        assert_eq!(
            summary(&tree),
            [
                ("", true),
                ("docs", false),
                ("src", false),
                ("top.md", false)
            ]
            .map(|(p, e)| (p.to_string(), e))
            .to_vec()
        );
        assert_eq!(selected(&tree), Some("docs"));
        // Listings stay, so opening a folder again reads nothing.
        assert!(!tree.expand(Path::new("docs")));
    }

    #[test]
    fn expanding_asks_for_a_listing_only_once() {
        let mut tree = sample();
        assert!(tree.expand(Path::new("src")));
        tree.set_listing("src".into(), vec![file("main.rs")]);
        tree.collapse(Path::new("src"));
        assert!(!tree.expand(Path::new("src")));
        tree.invalidate(Path::new("src"));
        assert!(tree.expand(Path::new("src")));
    }

    fn paths(dirs: &[PathBuf]) -> Vec<&str> {
        dirs.iter().map(|d| d.to_str().unwrap()).collect()
    }

    #[test]
    fn revealing_a_file_opens_the_folders_above_it() {
        let mut tree = tree_with(&[("", vec![dir("docs"), file("top.md")])]);
        let unread = tree.reveal(Path::new("docs/guide/deep/a.md"));
        // Outermost first, so each listing can arrive in the order the rows need.
        assert_eq!(paths(&unread), ["docs", "docs/guide", "docs/guide/deep"]);
        assert!(tree.is_expanded(Path::new("docs")));
        assert!(tree.is_expanded(Path::new("docs/guide/deep")));
        assert_eq!(selected(&tree), Some("docs/guide/deep/a.md"));
    }

    #[test]
    fn revealing_reads_only_folders_not_read_yet() {
        let mut tree = sample();
        tree.collapse(Path::new("docs"));
        assert!(tree.reveal(Path::new("docs/guide/b.md")).is_empty());
        assert!(tree.is_expanded(Path::new("docs")));
        assert_eq!(selected(&tree), Some("docs/guide/b.md"));
        assert!(
            tree.rows()
                .iter()
                .any(|r| r.path == Path::new("docs/guide/b.md"))
        );
        // A top-level item has nothing above it to open but the open folder, which may be closed.
        tree.collapse(Path::new(""));
        assert!(tree.reveal(Path::new("top.md")).is_empty());
        assert!(tree.is_expanded(Path::new("")));
        assert_eq!(selected(&tree), Some("top.md"));
    }

    #[test]
    fn revealing_stops_at_a_folder_that_loops() {
        let looping = DirEntry {
            loops: true,
            ..dir("self")
        };
        let mut tree = tree_with(&[("", vec![looping])]);
        assert!(tree.reveal(Path::new("self/inner/a.md")).is_empty());
        assert!(!tree.is_expanded(Path::new("self")));
        assert!(!tree.is_expanded(Path::new("self/inner")));
    }

    fn summary(tree: &ProjectTree) -> Vec<(String, bool)> {
        tree.rows()
            .into_iter()
            .map(|r| (r.path.to_str().unwrap().to_string(), r.expanded))
            .collect()
    }

    #[test]
    fn a_renamed_folder_stays_open_with_the_selection_inside() {
        let mut tree = sample();
        tree.select(Some("docs/guide/b.md".into()));
        tree.renamed(Path::new("docs/guide"), Path::new("docs/manual"));
        assert_eq!(selected(&tree), Some("docs/manual/b.md"));
        assert!(!tree.is_expanded(Path::new("docs/guide")));
        let want = [
            ("", true),
            ("docs", true),
            ("docs/manual", true),
            ("docs/manual/a.md", false),
            ("docs/manual/b.md", false),
            ("docs/readme.md", false),
            ("src", false),
            ("top.md", false),
        ];
        assert_eq!(
            summary(&tree),
            want.map(|(p, e)| (p.to_string(), e)).to_vec()
        );
    }

    #[test]
    fn a_renamed_file_moves_to_its_new_place_in_the_order() {
        let mut tree = tree_with(&[("", vec![file("a.md"), file("b.md"), file("c.md")])]);
        tree.select(Some("a.md".into()));
        tree.renamed(Path::new("a.md"), Path::new("d.md"));
        let names: Vec<String> = tree.rows().into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["project", "b.md", "c.md", "d.md"]);
        assert_eq!(selected(&tree), Some("d.md"));
    }

    #[test]
    fn a_rename_leaves_names_that_only_start_the_same() {
        let mut tree = tree_with(&[
            ("", vec![dir("docs"), dir("docs2")]),
            ("docs", vec![file("a.md")]),
            ("docs2", vec![file("b.md")]),
        ]);
        tree.select(Some("docs2/b.md".into()));
        tree.renamed(Path::new("docs"), Path::new("notes"));
        assert!(tree.is_expanded(Path::new("docs2")));
        assert!(tree.listing(Path::new("docs2")).is_some());
        assert_eq!(selected(&tree), Some("docs2/b.md"));
    }

    #[test]
    fn a_forgotten_folder_comes_back_closed_and_unread() {
        let mut tree = sample();
        tree.forget(Path::new("docs"));
        assert!(!tree.is_expanded(Path::new("docs")));
        assert!(!tree.is_expanded(Path::new("docs/guide")));
        assert!(tree.listing(Path::new("docs/guide")).is_none());
        // The folder holding it keeps its listing until it is read again.
        assert!(tree.listing(Path::new("")).is_some());
        assert!(tree.expand(Path::new("docs")));
    }

    #[test]
    fn forgetting_leaves_names_that_only_start_the_same() {
        let mut tree = tree_with(&[
            ("", vec![dir("docs"), dir("docs2")]),
            ("docs", vec![]),
            ("docs2", vec![]),
        ]);
        tree.forget(Path::new("docs"));
        assert!(tree.is_expanded(Path::new("docs2")));
        assert!(tree.listing(Path::new("docs2")).is_some());
    }

    #[test]
    fn a_looping_symlinked_folder_stays_closed() {
        let looping = DirEntry {
            loops: true,
            ..dir("self")
        };
        let mut tree = tree_with(&[("", vec![dir("docs"), looping.clone()])]);
        assert!(!tree.expand(Path::new("self")));
        assert!(!tree.is_expanded(Path::new("self")));

        // Opened (say, from a session) before the listing said it loops.
        let mut tree = ProjectTree::new(PathBuf::from("/project"));
        tree.expand(Path::new("self"));
        tree.set_listing("".into(), vec![looping]);
        tree.set_listing("self".into(), vec![dir("self")]);
        let rows = tree.rows();
        assert_eq!(rows.len(), 2);
        assert!(rows[1].loops);
        assert!(!rows[1].expanded);
    }
}
