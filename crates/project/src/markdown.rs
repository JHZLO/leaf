//! Markdown editing helpers: continuing lists on Enter, indenting list items, wrapping a selection.
//! Pure: a line (or the lines around a selection) in, an [`Edit`] out; the app applies it.
//! Offsets are bytes, as the editor counts them.

use std::ops::Range;

/// Replace `range` with `text`, then select `selection` (in the text after the edit).
/// Both are relative to the start of the text the helper was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

/// A list item or quote line, split where its text starts.
struct Item<'a> {
    indent: &'a str,
    /// From the indent to where the text starts: `- `, `2. `, `- [x] `, `> `.
    marker: &'a str,
    /// What the next line starts with after the indent: the next number, an unchecked box.
    next: String,
    /// The bullet or number and the spaces after it (not the box): how far its text starts in.
    width: usize,
    list: bool,
}

/// Spaces and tabs at the start of `s`.
fn blank_prefix(s: &str) -> &str {
    &s[..s.len() - s.trim_start_matches([' ', '\t']).len()]
}

fn item(line: &str) -> Option<Item<'_>> {
    let indent = blank_prefix(line);
    let rest = &line[indent.len()..];
    if rest.starts_with('>') {
        let marker = &rest[..rest.len() - rest.trim_start_matches(['>', ' ']).len()];
        return Some(Item {
            indent,
            marker,
            next: marker.to_string(),
            width: marker.len(),
            list: false,
        });
    }
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let (bullet, next_bullet) = if rest.starts_with(['-', '*', '+']) {
        (&rest[..1], rest[..1].to_string())
    } else if (1..=9).contains(&digits) && rest[digits..].starts_with(['.', ')']) {
        let number: u64 = rest[..digits].parse().ok()?;
        let delimiter = &rest[digits..=digits];
        // Zero padding stays as wide as it was: 09. is followed by 10., 01. by 02.
        (
            &rest[..=digits],
            format!("{:0digits$}{delimiter}", number + 1),
        )
    } else {
        return None;
    };
    // A marker needs a space after it; `-foo`, `**bold**` and `1.5` are text.
    let space = blank_prefix(&rest[bullet.len()..]);
    if space.is_empty() {
        return None;
    }
    let width = bullet.len() + space.len();
    let mut len = width;
    let mut next = format!("{next_bullet}{space}");
    let after = &rest[len..];
    if ["[ ]", "[x]", "[X]"].iter().any(|b| after.starts_with(b)) {
        let space = blank_prefix(&after[3..]);
        if !space.is_empty() {
            len += 3 + space.len();
            next.push_str("[ ]");
            next.push_str(space);
        }
    }
    Some(Item {
        indent,
        marker: &rest[..len],
        next,
        width,
        list: true,
    })
}

/// What Enter does on `line` with the cursor at `column`, or `None` for a plain newline.
/// A list item or quote goes on with the same marker on the next line (the next number, an
/// unchecked box). An empty item ends the list instead: its line is cleared.
/// The cursor has to be past the marker; before it, Enter is an ordinary newline.
pub fn enter(line: &str, column: usize) -> Option<Edit> {
    let item = item(line)?;
    let text_start = item.indent.len() + item.marker.len();
    if column < text_start {
        return None;
    }
    if line[text_start..].trim().is_empty() {
        return Some(Edit {
            range: 0..line.len(),
            text: String::new(),
            selection: 0..0,
        });
    }
    let text = format!("\n{}{}", item.indent, item.next);
    let cursor = column + text.len();
    Some(Edit {
        range: column..column,
        text,
        selection: cursor..cursor,
    })
}

/// The indent one nesting level adds in a list, from its lines (the block around the cursor).
/// Where the list already nests, the step between its levels; otherwise the width of its first
/// marker, so a nested item starts under its parent's text as CommonMark wants (`- ` is two
/// spaces, `1. ` three). A list indented with tabs indents with a tab.
pub fn indent_unit<'a>(lines: impl IntoIterator<Item = &'a str>) -> String {
    let items: Vec<Item> = lines
        .into_iter()
        .filter_map(item)
        .filter(|i| i.list)
        .collect();
    if items.iter().any(|i| i.indent.starts_with('\t')) {
        return "\t".into();
    }
    let mut depths: Vec<usize> = items.iter().map(|i| i.indent.len()).collect();
    depths.sort_unstable();
    depths.dedup();
    let width = depths
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .min()
        .or_else(|| items.first().map(|i| i.width))
        .unwrap_or(2);
    " ".repeat(width)
}

/// Tab on a list item: indents the whole item by `unit`, wherever the cursor is in it.
/// `selection` is relative to the line; `None` when the line is not a list item.
pub fn indent(line: &str, selection: Range<usize>, unit: &str) -> Option<Edit> {
    item(line).filter(|i| i.list)?;
    let shift = unit.len();
    Some(Edit {
        range: 0..0,
        text: unit.to_string(),
        selection: selection.start + shift..selection.end + shift,
    })
}

/// Shift+Tab on a list item: removes one `unit` of indent, or what there is when it is less.
/// `None` when the line is not a list item or is not indented.
pub fn outdent(line: &str, selection: Range<usize>, unit: &str) -> Option<Edit> {
    let item = item(line).filter(|i| i.list)?;
    let remove = if item.indent.starts_with(unit) {
        unit.len()
    } else {
        item.indent.len().min(unit.len())
    };
    if remove == 0 {
        return None;
    }
    let shift = |offset: usize| offset - offset.min(remove);
    Some(Edit {
        range: 0..remove,
        text: String::new(),
        selection: shift(selection.start)..shift(selection.end),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    Bold,
    Italic,
}

impl Emphasis {
    fn marker(self) -> &'static str {
        match self {
            Emphasis::Bold => "**",
            Emphasis::Italic => "*",
        }
    }

    /// Whether a run of `n` asterisks carries this emphasis: `**` and `***` are bold, `*` and
    /// `***` italic. So italic inside bold (`***`) can be taken off without touching the bold.
    fn in_run(self, n: usize) -> bool {
        match self {
            Emphasis::Bold => n >= 2,
            Emphasis::Italic => n % 2 == 1,
        }
    }
}

fn leading_stars(s: &str) -> usize {
    s.len() - s.trim_start_matches('*').len()
}

fn trailing_stars(s: &str) -> usize {
    s.len() - s.trim_end_matches('*').len()
}

/// Cmd+B / Cmd+I: wraps the selection in the emphasis markers, or takes them off when the
/// selection already has them, inside it (`**word**` selected) or just around it. With nothing
/// selected it inserts the pair and puts the cursor between (or removes an empty pair).
/// `text` holds the selection's lines; `selection` is relative to it.
pub fn toggle_emphasis(text: &str, selection: Range<usize>, emphasis: Emphasis) -> Edit {
    let marker = emphasis.marker();
    let m = marker.len();
    let selected = &text[selection.clone()];
    let lead = leading_stars(selected);
    let trail = trailing_stars(&selected[lead..]);
    if emphasis.in_run(lead) && emphasis.in_run(trail) {
        let inner = &selected[m..selected.len() - m];
        return Edit {
            range: selection.clone(),
            text: inner.to_string(),
            selection: selection.start..selection.start + inner.len(),
        };
    }
    let before = trailing_stars(&text[..selection.start]);
    let after = leading_stars(&text[selection.end..]);
    if emphasis.in_run(before) && emphasis.in_run(after) {
        let start = selection.start - m;
        return Edit {
            range: start..selection.end + m,
            text: selected.to_string(),
            selection: start..start + selected.len(),
        };
    }
    Edit {
        range: selection.clone(),
        text: format!("{marker}{selected}{marker}"),
        selection: selection.start + m..selection.end + m,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `edit` to `text` and marks the selection with `[` `]`, or the cursor with `|`.
    fn apply(text: &str, edit: Edit) -> String {
        let mut out = text.to_string();
        out.replace_range(edit.range, &edit.text);
        let Range { start, end } = edit.selection;
        if start == end {
            out.insert(start, '|');
        } else {
            out.insert(end, ']');
            out.insert(start, '[');
        }
        out
    }

    /// Enter with the cursor at `|` in `line`.
    fn enter_at(line: &str) -> Option<String> {
        let column = line.find('|').expect("cursor");
        let line = line.replace('|', "");
        enter(&line, column).map(|edit| apply(&line, edit))
    }

    #[test]
    fn enter_continues_bullets_quotes_and_tasks() {
        assert_eq!(enter_at("- item|").as_deref(), Some("- item\n- |"));
        assert_eq!(enter_at("* item|").as_deref(), Some("* item\n* |"));
        assert_eq!(enter_at("+ item|").as_deref(), Some("+ item\n+ |"));
        assert_eq!(enter_at("> said|").as_deref(), Some("> said\n> |"));
        assert_eq!(enter_at("> > deep|").as_deref(), Some("> > deep\n> > |"));
        // A new task starts unchecked, whatever the one above is.
        assert_eq!(
            enter_at("- [ ] todo|").as_deref(),
            Some("- [ ] todo\n- [ ] |")
        );
        assert_eq!(
            enter_at("- [x] done|").as_deref(),
            Some("- [x] done\n- [ ] |")
        );
        assert_eq!(
            enter_at("* [X] done|").as_deref(),
            Some("* [X] done\n* [ ] |")
        );
        // Korean text: offsets are bytes.
        assert_eq!(enter_at("- 한글|").as_deref(), Some("- 한글\n- |"));
    }

    #[test]
    fn enter_numbers_the_next_ordered_item() {
        assert_eq!(enter_at("1. one|").as_deref(), Some("1. one\n2. |"));
        assert_eq!(enter_at("9) nine|").as_deref(), Some("9) nine\n10) |"));
        assert_eq!(enter_at("09. nine|").as_deref(), Some("09. nine\n10. |"));
        assert_eq!(enter_at("01. one|").as_deref(), Some("01. one\n02. |"));
        assert_eq!(
            enter_at("3. [x] done|").as_deref(),
            Some("3. [x] done\n4. [ ] |")
        );
    }

    #[test]
    fn enter_keeps_the_indent_and_the_spacing() {
        assert_eq!(
            enter_at("  - nested|").as_deref(),
            Some("  - nested\n  - |")
        );
        assert_eq!(enter_at("\t1. tab|").as_deref(), Some("\t1. tab\n\t2. |"));
        assert_eq!(enter_at("-   wide|").as_deref(), Some("-   wide\n-   |"));
    }

    #[test]
    fn enter_on_an_empty_item_ends_the_list() {
        assert_eq!(enter_at("- |").as_deref(), Some("|"));
        assert_eq!(enter_at("  - |").as_deref(), Some("|"));
        assert_eq!(enter_at("2. |").as_deref(), Some("|"));
        assert_eq!(enter_at("- [ ] |").as_deref(), Some("|"));
        assert_eq!(enter_at("> |").as_deref(), Some("|"));
        assert_eq!(enter_at("-   |").as_deref(), Some("|"));
    }

    #[test]
    fn enter_splits_an_item_only_past_its_marker() {
        assert_eq!(enter_at("- fo|o").as_deref(), Some("- fo\n- |o"));
        assert_eq!(enter_at("- |foo").as_deref(), Some("- \n- |foo"));
        // At or inside the marker, or in the indent, it is a plain newline.
        assert_eq!(enter_at("|- foo"), None);
        assert_eq!(enter_at("-| foo"), None);
        assert_eq!(enter_at(" | - foo"), None);
        assert_eq!(enter_at("- [|x] foo"), None);
    }

    #[test]
    fn enter_leaves_other_lines_alone() {
        for line in [
            "plain|",
            "|",
            "  indented|",
            "-dash|",
            "**bold**|",
            "*em*|",
            "---|",
            "1.5 apples|",
            "1234567890. too long|",
            "# Heading|",
        ] {
            assert_eq!(enter_at(line), None, "{line}");
        }
    }

    #[test]
    fn the_indent_unit_follows_the_list() {
        assert_eq!(indent_unit(["- a", "- b"]), "  ");
        assert_eq!(indent_unit(["1. a", "2. b"]), "   ");
        assert_eq!(indent_unit(["10. a"]), "    ");
        assert_eq!(indent_unit(["- [ ] a"]), "  ");
        assert_eq!(indent_unit(["- a", "    - b", "- c"]), "    ");
        assert_eq!(indent_unit(["  - a", "    - b"]), "  ");
        assert_eq!(indent_unit(["- a", "\t- b"]), "\t");
        // Text and quotes in the block do not count; with no list at all, two spaces.
        assert_eq!(indent_unit(["Some text", "    code", "- a"]), "  ");
        assert_eq!(indent_unit(["> quote"]), "  ");
        assert_eq!(indent_unit([]), "  ");
    }

    #[test]
    fn tab_indents_and_shift_tab_outdents_a_list_item() {
        let edit = indent("- item", 3..3, "  ").expect("indent");
        assert_eq!(apply("- item", edit), "  - i|tem");
        let edit = indent("1. item", 3..7, "   ").expect("indent");
        assert_eq!(apply("1. item", edit), "   1. [item]");

        let edit = outdent("    - item", 6..6, "  ").expect("outdent");
        assert_eq!(apply("    - item", edit), "  - |item");
        // Less than a unit goes entirely; a cursor inside the removed indent stays at the start.
        let edit = outdent(" - item", 0..0, "  ").expect("outdent");
        assert_eq!(apply(" - item", edit), "|- item");
        let edit = outdent("\t- item", 1..1, "\t").expect("outdent");
        assert_eq!(apply("\t- item", edit), "|- item");
    }

    #[test]
    fn tab_is_left_to_the_editor_outside_list_items() {
        assert_eq!(indent("plain", 0..0, "  "), None);
        assert_eq!(indent("> quote", 0..0, "  "), None);
        assert_eq!(indent("", 0..0, "  "), None);
        assert_eq!(outdent("  plain", 0..0, "  "), None);
        assert_eq!(outdent("- top level", 0..0, "  "), None);
    }

    /// Cmd+B or Cmd+I on `text` with the selection in `[` `]` or the cursor at `|`.
    fn toggle(text: &str, emphasis: Emphasis) -> String {
        let selection = match text.find('|') {
            Some(at) => at..at,
            None => {
                let start = text.find('[').expect("selection");
                start..text.find(']').expect("selection end") - 1
            }
        };
        let text = text.replace(['|', '[', ']'], "");
        let edit = toggle_emphasis(&text, selection, emphasis);
        apply(&text, edit)
    }

    #[test]
    fn bold_and_italic_wrap_the_selection() {
        assert_eq!(toggle("a [word] b", Emphasis::Bold), "a **[word]** b");
        assert_eq!(toggle("a [word] b", Emphasis::Italic), "a *[word]* b");
        assert_eq!(toggle("[한글]", Emphasis::Bold), "**[한글]**");
        // Italic inside bold, and bold inside italic.
        assert_eq!(toggle("**[word]**", Emphasis::Italic), "***[word]***");
        assert_eq!(toggle("*[word]*", Emphasis::Bold), "***[word]***");
    }

    #[test]
    fn bold_and_italic_unwrap_what_has_them() {
        assert_eq!(toggle("a **[word]** b", Emphasis::Bold), "a [word] b");
        assert_eq!(toggle("a [**word**] b", Emphasis::Bold), "a [word] b");
        assert_eq!(toggle("a *[word]* b", Emphasis::Italic), "a [word] b");
        assert_eq!(toggle("a [*word*] b", Emphasis::Italic), "a [word] b");
        assert_eq!(toggle("**[강조]**를", Emphasis::Bold), "[강조]를");
        // From bold and italic together, only the one asked for comes off.
        assert_eq!(toggle("***[word]***", Emphasis::Italic), "**[word]**");
        assert_eq!(toggle("***[word]***", Emphasis::Bold), "*[word]*");
    }

    #[test]
    fn with_nothing_selected_the_pair_goes_in_around_the_cursor() {
        assert_eq!(toggle("a | b", Emphasis::Bold), "a **|** b");
        assert_eq!(toggle("a | b", Emphasis::Italic), "a *|* b");
        assert_eq!(toggle("|", Emphasis::Bold), "**|**");
        // A second press takes the empty pair out again.
        assert_eq!(toggle("a **|** b", Emphasis::Bold), "a | b");
        assert_eq!(toggle("a *|* b", Emphasis::Italic), "a | b");
    }
}
