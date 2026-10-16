//! What each key does in the tree (IntelliJ semantics). Pure: rows and selection in, a move out.

use std::{
    ops::Range,
    path::{Path, PathBuf},
};

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
        // With nothing selected, the downward keys start at the top and the upward ones at the
        // bottom, so the first press always lands where that key points.
        (NavKey::Down | NavKey::PageDown(_), None) | (NavKey::First, _) => Some(0),
        (NavKey::Up | NavKey::PageUp(_), None) | (NavKey::Last, _) => Some(last),
        (NavKey::Down, Some(i)) => Some((i + 1).min(last)),
        (NavKey::Up, Some(i)) => Some(i.saturating_sub(1)),
        (NavKey::PageDown(n), Some(i)) => Some((i + n).min(last)),
        (NavKey::PageUp(n), Some(i)) => Some(i.saturating_sub(n)),
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

/// Which way a speed search looks for the next name that matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStep {
    /// The text changed: the selected row if it still matches, else the next match below it,
    /// else the first one from the top, so a match anywhere on screen is found.
    Here,
    /// Down: the next match below the selection. Like the arrow keys, it stops at the end.
    Next,
    /// Up: the previous match above the selection.
    Prev,
}

/// Where `query` sits in `name`, as a byte range of `name`, ignoring case.
/// An empty query matches nothing.
pub fn name_match(name: &str, query: &str) -> Option<Range<usize>> {
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if query.is_empty() {
        return None;
    }
    // Lowercasing can turn one character into several; each keeps the bytes it came from,
    // so the range always falls on whole characters of the name.
    let folded: Vec<(char, Range<usize>)> = name
        .char_indices()
        .flat_map(|(at, c)| {
            let bytes = at..at + c.len_utf8();
            c.to_lowercase().map(move |lower| (lower, bytes.clone()))
        })
        .collect();
    folded
        .windows(query.len())
        .find(|window| window.iter().map(|(c, _)| c).eq(query.iter()))
        .map(|window| window[0].1.start..window[window.len() - 1].1.end)
}

/// The row a speed search for `query` selects, among the visible rows.
pub fn speed_search(
    rows: &[Row],
    selected: Option<&Path>,
    query: &str,
    step: SearchStep,
) -> Option<PathBuf> {
    let matches = |row: &&Row| name_match(&row.name, query).is_some();
    let current = selected.and_then(|s| rows.iter().position(|r| r.path == s));
    let found = match (step, current) {
        (SearchStep::Here, Some(i)) => rows[i..].iter().chain(&rows[..i]).find(matches),
        (SearchStep::Here | SearchStep::Next, None) => rows.iter().find(matches),
        (SearchStep::Next, Some(i)) => rows[i + 1..].iter().find(matches),
        (SearchStep::Prev, Some(i)) => rows[..i].iter().rev().find(matches),
        (SearchStep::Prev, None) => rows.iter().rev().find(matches),
    };
    found.map(|row| row.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Guide;

    fn row(path: &str, kind: EntryKind, expanded: bool) -> Row {
        let path = PathBuf::from(path);
        let depth = path.components().count() - 1;
        Row {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            depth,
            kind,
            expanded,
            ignored: false,
            loops: false,
            guides: vec![Guide::default(); depth],
        }
    }

    /// docs/ (open) > empty/ (open, nothing inside), guide/ (closed), a.md; src/ (closed); top.md
    fn rows() -> Vec<Row> {
        vec![
            row("docs", EntryKind::Dir, true),
            row("docs/empty", EntryKind::Dir, true),
            row("docs/guide", EntryKind::Dir, false),
            row("docs/a.md", EntryKind::File, false),
            row("src", EntryKind::Dir, false),
            row("top.md", EntryKind::File, false),
        ]
    }

    fn nav(selected: Option<&str>, key: NavKey) -> NavOutcome {
        navigate(&rows(), selected.map(Path::new), key)
    }

    fn select(path: &str) -> NavOutcome {
        NavOutcome {
            select: Some(path.into()),
            ..NavOutcome::default()
        }
    }

    fn expand(path: &str) -> NavOutcome {
        NavOutcome {
            expand: Some(path.into()),
            ..NavOutcome::default()
        }
    }

    fn collapse(path: &str) -> NavOutcome {
        NavOutcome {
            collapse: Some(path.into()),
            ..NavOutcome::default()
        }
    }

    fn nothing() -> NavOutcome {
        NavOutcome::default()
    }

    #[test]
    fn with_nothing_selected_down_starts_at_the_top_and_up_at_the_bottom() {
        assert_eq!(nav(None, NavKey::Down), select("docs"));
        assert_eq!(nav(None, NavKey::Up), select("top.md"));
        assert_eq!(nav(None, NavKey::First), select("docs"));
        assert_eq!(nav(None, NavKey::Last), select("top.md"));
        assert_eq!(nav(None, NavKey::PageDown(3)), select("docs"));
        assert_eq!(nav(None, NavKey::PageUp(3)), select("top.md"));
        assert_eq!(nav(None, NavKey::Right), nothing());
        assert_eq!(nav(None, NavKey::Left), nothing());
    }

    #[test]
    fn a_selection_no_longer_on_screen_counts_as_none() {
        assert_eq!(nav(Some("gone.md"), NavKey::Down), select("docs"));
        assert_eq!(nav(Some("gone.md"), NavKey::Right), nothing());
    }

    #[test]
    fn an_empty_tree_does_nothing() {
        for key in [
            NavKey::Up,
            NavKey::Down,
            NavKey::Right,
            NavKey::Left,
            NavKey::First,
            NavKey::Last,
            NavKey::PageUp(5),
            NavKey::PageDown(5),
        ] {
            assert_eq!(navigate(&[], None, key), nothing());
            assert_eq!(navigate(&[], Some(Path::new("a.md")), key), nothing());
        }
    }

    #[test]
    fn up_and_down_move_one_row_and_stop_at_the_ends() {
        assert_eq!(nav(Some("docs/guide"), NavKey::Down), select("docs/a.md"));
        assert_eq!(nav(Some("docs/guide"), NavKey::Up), select("docs/empty"));
        assert_eq!(nav(Some("docs"), NavKey::Up), select("docs"));
        assert_eq!(nav(Some("top.md"), NavKey::Down), select("top.md"));
    }

    #[test]
    fn right_opens_a_closed_folder_in_place() {
        assert_eq!(nav(Some("src"), NavKey::Right), expand("src"));
        assert_eq!(nav(Some("docs/guide"), NavKey::Right), expand("docs/guide"));
    }

    #[test]
    fn right_steps_into_an_open_folder() {
        assert_eq!(nav(Some("docs"), NavKey::Right), select("docs/empty"));
    }

    #[test]
    fn right_does_nothing_on_an_empty_open_folder_or_a_file() {
        assert_eq!(nav(Some("docs/empty"), NavKey::Right), nothing());
        assert_eq!(nav(Some("docs/a.md"), NavKey::Right), nothing());
        assert_eq!(nav(Some("top.md"), NavKey::Right), nothing());
    }

    #[test]
    fn right_does_not_open_a_folder_that_loops() {
        let mut rows = rows();
        rows[4].loops = true;
        assert_eq!(
            navigate(&rows, Some(Path::new("src")), NavKey::Right),
            nothing()
        );
    }

    #[test]
    fn left_closes_an_open_folder() {
        assert_eq!(nav(Some("docs"), NavKey::Left), collapse("docs"));
        assert_eq!(
            nav(Some("docs/empty"), NavKey::Left),
            collapse("docs/empty")
        );
    }

    #[test]
    fn left_steps_out_to_the_parent() {
        assert_eq!(nav(Some("docs/a.md"), NavKey::Left), select("docs"));
        assert_eq!(nav(Some("docs/guide"), NavKey::Left), select("docs"));
    }

    #[test]
    fn left_does_nothing_on_a_closed_top_level_item() {
        assert_eq!(nav(Some("src"), NavKey::Left), nothing());
        assert_eq!(nav(Some("top.md"), NavKey::Left), nothing());
    }

    #[test]
    fn home_and_end_go_to_the_first_and_last_rows() {
        assert_eq!(nav(Some("docs/a.md"), NavKey::First), select("docs"));
        assert_eq!(nav(Some("docs/a.md"), NavKey::Last), select("top.md"));
    }

    #[test]
    fn names_match_ignoring_case() {
        assert_eq!(name_match("README.md", "read"), Some(0..4));
        assert_eq!(name_match("paths.md", "S.M"), Some(4..7));
        assert_eq!(name_match("paths.md", "x"), None);
        assert_eq!(name_match("paths.md", ""), None);
        assert_eq!(name_match("a.md", "a.md.bak"), None);
    }

    #[test]
    fn a_match_covers_whole_characters_of_the_name() {
        // Hangul takes three bytes a character.
        assert_eq!(name_match("한글.md", "글"), Some(3..6));
        // `İ` lowercases to two characters; a match on either still spans all of `İ`.
        assert_eq!(name_match("İx", "i"), Some(0..2));
        assert_eq!(name_match("İx", "x"), Some(2..3));
    }

    fn search(selected: Option<&str>, query: &str, step: SearchStep) -> Option<PathBuf> {
        speed_search(&rows(), selected.map(Path::new), query, step)
    }

    fn found(path: &str) -> Option<PathBuf> {
        Some(path.into())
    }

    #[test]
    fn typing_selects_the_first_match_from_the_selection_down() {
        // The selected row still matches, so the selection stays.
        assert_eq!(
            search(Some("docs/guide"), "gui", SearchStep::Here),
            found("docs/guide")
        );
        assert_eq!(
            search(Some("docs/empty"), "d", SearchStep::Here),
            found("docs/guide")
        );
        // Only names count: the path `docs/a.md` holds "docs", its name does not.
        assert_eq!(
            search(Some("docs/a.md"), "docs", SearchStep::Here),
            found("docs")
        );
        assert_eq!(search(None, "md", SearchStep::Here), found("docs/a.md"));
        assert_eq!(search(Some("docs"), "zzz", SearchStep::Here), None);
    }

    #[test]
    fn typing_starts_again_from_the_top_past_the_last_match() {
        assert_eq!(
            search(Some("top.md"), "src", SearchStep::Here),
            found("src")
        );
    }

    #[test]
    fn up_and_down_step_through_the_matches_and_stop_at_the_ends() {
        // "d" matches docs, guide, a.md and top.md.
        assert_eq!(
            search(Some("docs"), "d", SearchStep::Next),
            found("docs/guide")
        );
        assert_eq!(
            search(Some("docs/guide"), "d", SearchStep::Next),
            found("docs/a.md")
        );
        assert_eq!(
            search(Some("docs/a.md"), "d", SearchStep::Next),
            found("top.md")
        );
        assert_eq!(search(Some("top.md"), "d", SearchStep::Next), None);
        assert_eq!(
            search(Some("top.md"), "d", SearchStep::Prev),
            found("docs/a.md")
        );
        assert_eq!(search(Some("docs"), "d", SearchStep::Prev), None);
        // With nothing selected they start at the matching end.
        assert_eq!(search(None, "d", SearchStep::Next), found("docs"));
        assert_eq!(search(None, "d", SearchStep::Prev), found("top.md"));
    }

    #[test]
    fn page_keys_move_a_page_and_stop_at_the_ends() {
        assert_eq!(nav(Some("docs"), NavKey::PageDown(2)), select("docs/guide"));
        assert_eq!(
            nav(Some("docs/a.md"), NavKey::PageDown(20)),
            select("top.md")
        );
        assert_eq!(nav(Some("top.md"), NavKey::PageUp(2)), select("docs/a.md"));
        assert_eq!(nav(Some("docs/guide"), NavKey::PageUp(20)), select("docs"));
    }
}
