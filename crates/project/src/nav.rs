//! What each key does in the tree (IntelliJ semantics). Pure: rows and selection in, a move out.

use std::path::{Path, PathBuf};

use crate::{fs::EntryKind, tree::Row};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    Up,
    Down,
    /// Open a closed folder, or step into an open one.
    Right,
    /// Close an open folder, or step out to the parent.
    Left,
    First,
    Last,
    PageUp(usize),
    PageDown(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavOutcome {
    pub select: Option<PathBuf>,
    pub expand: Option<PathBuf>,
    pub collapse: Option<PathBuf>,
}

pub fn navigate(rows: &[Row], selected: Option<&Path>, key: NavKey) -> NavOutcome {
    let mut outcome = NavOutcome::default();
    if rows.is_empty() {
        return outcome;
    }
    let current = selected.and_then(|s| rows.iter().position(|r| r.path == s));
    let last = rows.len() - 1;
    let target = match (key, current) {
        (NavKey::Down, None) | (NavKey::First, _) => Some(0),
        (NavKey::Up, None) | (NavKey::Last, _) => Some(last),
        (NavKey::Down, Some(i)) => Some((i + 1).min(last)),
        (NavKey::Up, Some(i)) => Some(i.saturating_sub(1)),
        (NavKey::PageDown(n), Some(i)) => Some((i + n).min(last)),
        (NavKey::PageUp(n), Some(i)) => Some(i.saturating_sub(n)),
        (NavKey::PageDown(_), None) | (NavKey::PageUp(_), None) => Some(0),
        (NavKey::Right, Some(i)) => {
            let row = &rows[i];
            if row.kind == EntryKind::Dir && !row.expanded && !row.loops {
                outcome.expand = Some(row.path.clone());
                None
            } else if row.kind == EntryKind::Dir
                && rows.get(i + 1).is_some_and(|next| next.depth > row.depth)
            {
                Some(i + 1)
            } else {
                None
            }
        }
        (NavKey::Left, Some(i)) => {
            let row = &rows[i];
            if row.kind == EntryKind::Dir && row.expanded {
                outcome.collapse = Some(row.path.clone());
                None
            } else {
                rows[..i].iter().rposition(|r| r.depth + 1 == row.depth)
            }
        }
        (NavKey::Right | NavKey::Left, None) => None,
    };
    outcome.select = target.map(|i| rows[i].path.clone());
    outcome
}
