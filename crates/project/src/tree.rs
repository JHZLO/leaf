//! The folder tree as the panel shows it: which folders are open, what each holds, what is selected.
//! Paths here are relative to the open folder; the folder itself is the empty path.

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

    /// Selects `path` and opens every folder above it. Returns the folders it opened whose
    /// listings still need reading, outermost first. A folder that loops stops the walk: the
    /// tree never shows what is inside it.
    pub fn reveal(&mut self, path: &Path) -> Vec<PathBuf> {
        let mut above: Vec<&Path> = path
            .ancestors()
            .skip(1)
            .filter(|dir| !dir.as_os_str().is_empty())
            .collect();
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

    /// Follows a rename leaf made: the item keeps its open folders, their listings and the
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

    /// docs/ (open) > guide/ (open) > a.md, b.md; docs/readme.md; src/ (closed); top.md
    fn sample() -> ProjectTree {
        tree_with(&[
            ("", vec![dir("docs"), dir("src"), file("top.md")]),
            ("docs", vec![dir("guide"), file("readme.md")]),
            ("docs/guide", vec![file("a.md"), file("b.md")]),
        ])
    }

    const PLAIN: Guide = Guide {
        vein: false,
        tick: false,
    };
    const VEIN: Guide = Guide {
        vein: true,
        tick: false,
    };
    const TICK: Guide = Guide {
        vein: true,
        tick: true,
    };

    fn guides(tree: &ProjectTree) -> Vec<(String, Vec<Guide>)> {
        tree.rows()
            .into_iter()
            .map(|r| (r.path.to_str().unwrap().to_string(), r.guides))
            .collect()
    }

    fn expect(rows: &[(&str, &[Guide])]) -> Vec<(String, Vec<Guide>)> {
        rows.iter()
            .map(|(p, g)| (p.to_string(), g.to_vec()))
            .collect()
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
                ("docs", 0, true),
                ("docs/guide", 1, true),
                ("docs/guide/a.md", 2, false),
                ("docs/guide/b.md", 2, false),
                ("docs/readme.md", 1, false),
                ("src", 0, false),
                ("top.md", 0, false),
            ]
        );
    }

    #[test]
    fn the_vein_runs_from_each_ancestor_down_to_the_child_on_the_path() {
        let mut tree = sample();
        tree.select(Some("docs/guide/b.md".into()));
        assert_eq!(
            guides(&tree),
            expect(&[
                ("docs", &[]),
                ("docs/guide", &[TICK]),
                ("docs/guide/a.md", &[PLAIN, VEIN]),
                ("docs/guide/b.md", &[PLAIN, TICK]),
                ("docs/readme.md", &[PLAIN]),
                ("src", &[]),
                ("top.md", &[]),
            ])
        );
    }

    #[test]
    fn the_vein_reaches_a_selected_folder() {
        let mut tree = sample();
        tree.select(Some("docs/guide".into()));
        assert_eq!(
            guides(&tree),
            expect(&[
                ("docs", &[]),
                ("docs/guide", &[TICK]),
                ("docs/guide/a.md", &[PLAIN, PLAIN]),
                ("docs/guide/b.md", &[PLAIN, PLAIN]),
                ("docs/readme.md", &[PLAIN]),
                ("src", &[]),
                ("top.md", &[]),
            ])
        );
    }

    #[test]
    fn the_vein_passes_over_the_siblings_above_the_path() {
        let mut tree = sample();
        tree.select(Some("docs/readme.md".into()));
        assert_eq!(
            guides(&tree),
            expect(&[
                ("docs", &[]),
                ("docs/guide", &[VEIN]),
                ("docs/guide/a.md", &[VEIN, PLAIN]),
                ("docs/guide/b.md", &[VEIN, PLAIN]),
                ("docs/readme.md", &[TICK]),
                ("src", &[]),
                ("top.md", &[]),
            ])
        );
    }

    #[test]
    fn no_vein_for_a_top_level_selection_or_none() {
        let mut tree = sample();
        let all_plain = |tree: &ProjectTree| {
            tree.rows()
                .iter()
                .all(|r| r.guides.iter().all(|g| *g == PLAIN))
        };
        assert!(all_plain(&tree));
        tree.select(Some("top.md".into()));
        assert!(all_plain(&tree));
        tree.select(Some("docs".into()));
        assert!(all_plain(&tree));
    }

    #[test]
    fn collapsing_a_folder_lifts_the_selection_inside_it() {
        let mut tree = sample();
        tree.select(Some("docs/guide/a.md".into()));
        tree.collapse(Path::new("docs"));
        assert_eq!(selected(&tree), Some("docs"));
        assert_eq!(tree.rows().len(), 3);
        // A selection outside the folder stays.
        tree.select(Some("top.md".into()));
        tree.expand(Path::new("docs"));
        tree.collapse(Path::new("docs"));
        assert_eq!(selected(&tree), Some("top.md"));
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
        // A top-level item has nothing above it to open.
        assert!(tree.reveal(Path::new("top.md")).is_empty());
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
        assert_eq!(names, ["b.md", "c.md", "d.md"]);
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
        assert_eq!(rows.len(), 1);
        assert!(rows[0].loops);
        assert!(!rows[0].expanded);
    }
}
