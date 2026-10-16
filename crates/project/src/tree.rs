//! The folder tree as the panel shows it: which folders are open, what each holds, what is selected.
//! Paths here are relative to the open folder; the folder itself is the empty path.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::fs::{DirEntry, EntryKind};

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

    pub fn set_listing(&mut self, dir: PathBuf, entries: Vec<DirEntry>) {
        self.listings.insert(dir, entries);
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
