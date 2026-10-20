//! Fuzzy matching for the file finder: which paths match a query, how well, and where.
//!
//! A path matches when the query's characters appear in it in order, ignoring case. Of all the
//! ways they line up, the best scoring one counts: characters in the file name, at the start of
//! a word and right after the previous match score more.

use unicode_normalization::UnicodeNormalization;

/// Every matched character.
const MATCH: i32 = 16;
/// A match in the file name rather than in a folder: `keys` finds `guide/keys.md` before `keys/a.md`.
const FILE_NAME: i32 = 16;
/// A match at the start of a word: after `/`, `_`, `-`, `.` or a space, or where lower case turns upper.
const WORD_START: i32 = 24;
/// A match right after the previous one.
const CONSECUTIVE: i32 = 16;
/// No way to line the query up to this point. Real scores are positive.
const NONE: i32 = -1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// Which of the searched paths.
    pub index: usize,
    pub score: i32,
    /// The matched characters, as char indices into the path, ascending.
    pub positions: Vec<usize>,
}

/// What a search found: the best few, and how many matched in all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub matches: Vec<Match>,
    pub total: usize,
}

/// The best `limit` matches of `query` among `paths`, best first. Ties go to the shorter path,
/// then to the one listed first (the index lists paths alphabetically). An empty query matches
/// every path, in the listed order.
pub fn search(query: &str, paths: &[String], limit: usize) -> Found {
    let mut matcher = Matcher::new(query);
    if matcher.query.is_empty() {
        let matches = (0..paths.len().min(limit))
            .map(|index| Match {
                index,
                score: 0,
                positions: Vec::new(),
            })
            .collect();
        return Found {
            matches,
            total: paths.len(),
        };
    }
    // Scores only; the positions of the few that are shown are worked out again below.
    let mut found = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if let Some(score) = matcher.align(path) {
            found.push((score, matcher.chars.len(), index));
        }
    }
    let order = |a: &(i32, usize, usize), b: &(i32, usize, usize)| {
        b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
    };
    let total = found.len();
    if found.len() > limit {
        found.select_nth_unstable_by(limit, order);
        found.truncate(limit);
    }
    found.sort_unstable_by(order);
    let matches = found
        .into_iter()
        .map(|(score, _, index)| {
            matcher.align(&paths[index]);
            Match {
                index,
                score,
                positions: matcher.positions(),
            }
        })
        .collect();
    Found { matches, total }
}

/// How well `query` matches `path`, and which characters matched; `None` when it does not match.
pub fn score(query: &str, path: &str) -> Option<(i32, Vec<usize>)> {
    let mut matcher = Matcher::new(query);
    let score = matcher.align(path)?;
    Some((score, matcher.positions()))
}

/// Case folding for matching. One char in, one char out keeps char indices aligned with the path.
fn fold(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// One query, reused across paths so the tables are allocated once per search.
struct Matcher {
    /// Folded, NFC (the index's form, and what the macOS input methods type), without spaces.
    query: Vec<char>,
    /// The same query as bytes when it is ASCII: the quick check can then skip UTF-8 decoding,
    /// since no byte of a non-ASCII character equals an ASCII one.
    ascii: Option<Vec<u8>>,
    /// The path being aligned, folded.
    chars: Vec<char>,
    bonus: Vec<i32>,
    /// The first and the last path char each query char can sit on; outside them nothing fits.
    lo: Vec<usize>,
    hi: Vec<usize>,
    /// `scores[i * n + j]`: the best score with query char `i` matched at path char `j`.
    scores: Vec<i32>,
    /// Where query char `i - 1` sits on that best line-up, to read the positions back.
    from: Vec<usize>,
    /// The path char the last query char matched on the best line-up.
    end: usize,
}

impl Matcher {
    fn new(query: &str) -> Self {
        let query: Vec<char> = query
            .nfc()
            .filter(|c| !c.is_whitespace())
            .map(fold)
            .collect();
        let ascii = query
            .iter()
            .all(char::is_ascii)
            .then(|| query.iter().map(|&c| c as u8).collect());
        Self {
            query,
            ascii,
            chars: Vec::new(),
            bonus: Vec::new(),
            lo: Vec::new(),
            hi: Vec::new(),
            scores: Vec::new(),
            from: Vec::new(),
            end: 0,
        }
    }

    /// The query's characters in order somewhere in `path`; most paths fail here, cheaply.
    fn contains(&self, path: &str) -> bool {
        match &self.ascii {
            Some(query) => {
                let bytes = path.as_bytes();
                let mut from = 0;
                for &want in query {
                    match bytes[from..]
                        .iter()
                        .position(|b| b.to_ascii_lowercase() == want)
                    {
                        Some(at) => from += at + 1,
                        None => return false,
                    }
                }
                true
            }
            None => {
                let mut want = self.query.iter();
                let mut next = want.next();
                for c in path.chars() {
                    match next {
                        Some(&w) if fold(c) == w => next = want.next(),
                        Some(_) => {}
                        None => break,
                    }
                }
                next.is_none()
            }
        }
    }

    /// Scores the best line-up of the query in `path`, keeping the tables for [`Self::positions`].
    fn align(&mut self, path: &str) -> Option<i32> {
        if !self.contains(path) {
            return None;
        }
        self.chars.clear();
        self.bonus.clear();
        let mut prev = None;
        let mut name_start = 0;
        for (j, c) in path.chars().enumerate() {
            let word_start = match prev {
                None => true,
                Some(p) => {
                    matches!(p, '/' | '_' | '-' | '.' | ' ')
                        || (p.is_lowercase() && c.is_uppercase())
                }
            };
            if c == '/' {
                name_start = j + 1;
            }
            self.chars.push(fold(c));
            self.bonus.push(if word_start {
                MATCH + WORD_START
            } else {
                MATCH
            });
            prev = Some(c);
        }
        for bonus in &mut self.bonus[name_start..] {
            *bonus += FILE_NAME;
        }

        let (n, m) = (self.chars.len(), self.query.len());
        // `contains` passed, so these walks find every query char.
        self.lo.clear();
        let mut j = 0;
        for &want in &self.query {
            while self.chars[j] != want {
                j += 1;
            }
            self.lo.push(j);
            j += 1;
        }
        self.hi.clear();
        self.hi.resize(m, 0);
        let mut j = n;
        for i in (0..m).rev() {
            j -= 1;
            while self.chars[j] != self.query[i] {
                j -= 1;
            }
            self.hi[i] = j;
        }

        self.scores.clear();
        self.scores.resize(m * n, NONE);
        // Only cells with a score are read back, and each of those is written below.
        if self.from.len() < m * n {
            self.from.resize(m * n, 0);
        }
        for j in self.lo[0]..=self.hi[0] {
            if self.chars[j] == self.query[0] {
                self.scores[j] = self.bonus[j];
            }
        }
        for i in 1..m {
            let (row, prev_row, want) = (i * n, (i - 1) * n, self.query[i]);
            // The best of the previous row two or more chars back: a match after a gap.
            let (mut gap, mut gap_at) = (NONE, 0);
            let mut k = self.lo[i - 1];
            for j in self.lo[i]..=self.hi[i] {
                while k + 2 <= j {
                    if self.scores[prev_row + k] > gap {
                        (gap, gap_at) = (self.scores[prev_row + k], k);
                    }
                    k += 1;
                }
                if self.chars[j] != want {
                    continue;
                }
                // `lo[i] > lo[i - 1]`, so `j >= 1`.
                let after = self.scores[prev_row + j - 1];
                let (base, at) = if after > NONE && after + CONSECUTIVE >= gap {
                    (after + CONSECUTIVE, j - 1)
                } else {
                    (gap, gap_at)
                };
                if base > NONE {
                    self.scores[row + j] = base + self.bonus[j];
                    self.from[row + j] = at;
                }
            }
        }
        let last = (m - 1) * n;
        let (mut best, mut end) = (NONE, 0);
        for j in self.lo[m - 1]..=self.hi[m - 1] {
            if self.scores[last + j] > best {
                (best, end) = (self.scores[last + j], j);
            }
        }
        self.end = end;
        (best > NONE).then_some(best)
    }

    /// The matched char indices of the last [`Self::align`] that matched.
    fn positions(&self) -> Vec<usize> {
        let (n, m) = (self.chars.len(), self.query.len());
        let mut positions = vec![0; m];
        let mut j = self.end;
        for i in (0..m).rev() {
            positions[i] = j;
            j = self.from[i * n + j];
        }
        positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(query: &str, paths: &[&str]) -> Vec<String> {
        let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        search(query, &paths, 50)
            .matches
            .into_iter()
            .map(|m| paths[m.index].clone())
            .collect()
    }

    #[test]
    fn matches_characters_in_order_ignoring_case() {
        let (_, positions) = score("rdme", "README.md").unwrap();
        assert_eq!(positions, [0, 3, 4, 5]);
        assert!(score("RDME", "readme.md").is_some());
        assert!(score("ba", "ab.md").is_none());
        assert!(score("abc", "ab").is_none());
        assert!(score("x", "").is_none());
    }

    #[test]
    fn spaces_in_the_query_are_ignored() {
        let (_, positions) = score("guide keys", "docs/guide/keys.md").unwrap();
        assert_eq!(positions, [5, 6, 7, 8, 9, 11, 12, 13, 14]);
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        let (_, positions) = score("\u{ae00}m", "docs/\u{d55c}\u{ae00}.md").unwrap();
        assert_eq!(positions, [6, 8]);
    }

    #[test]
    fn file_name_matches_beat_folder_matches() {
        assert_eq!(
            order("keys", &["keys/a.md", "docs/guide/keys.md"]),
            ["docs/guide/keys.md", "keys/a.md"]
        );
    }

    #[test]
    fn word_starts_beat_letters_inside_words() {
        assert_eq!(
            order("fm", &["leaf/farm.txt", "leaf/foo_moo.txt"]),
            ["leaf/foo_moo.txt", "leaf/farm.txt"]
        );
        assert_eq!(
            order("ps", &["src/perhaps.rs", "src/ProjectSearch.rs"]),
            ["src/ProjectSearch.rs", "src/perhaps.rs"]
        );
        // The line-up on word starts in the file name wins over the first one found.
        let (_, positions) = score("dk", "docs/kit/dark_keys.md").unwrap();
        assert_eq!(positions, [9, 14]);
    }

    #[test]
    fn consecutive_matches_beat_scattered_ones() {
        assert_eq!(order("ab", &["xaxb.md", "xxab.md"]), ["xxab.md", "xaxb.md"]);
    }

    #[test]
    fn ties_go_to_the_shorter_path_then_the_first_listed() {
        assert_eq!(
            order("a", &["b/a.md", "c/a.md", "a.md"]),
            ["a.md", "b/a.md", "c/a.md"]
        );
    }

    #[test]
    fn keeps_the_best_up_to_the_limit() {
        let paths: Vec<String> = ["x/main.rs", "main.rs", "a/b/main.rs", "domain.rs"]
            .iter()
            .map(|p| p.to_string())
            .collect();
        let found = search("main", &paths, 2);
        // All four match; two are kept.
        assert_eq!(found.total, 4);
        let found = found.matches;
        let indices: Vec<usize> = found.iter().map(|m| m.index).collect();
        assert_eq!(indices, [1, 0]);
        assert_eq!(found[0].positions, [0, 1, 2, 3]);
        assert_eq!(found[1].positions, [2, 3, 4, 5]);
    }

    #[test]
    fn an_empty_query_lists_paths_in_order() {
        let paths: Vec<String> = ["a.md", "b.md", "c.md"]
            .iter()
            .map(|p| p.to_string())
            .collect();
        let found = search("  ", &paths, 2);
        assert_eq!(found.total, 3);
        let found = found.matches;
        let indices: Vec<usize> = found.iter().map(|m| m.index).collect();
        assert_eq!(indices, [0, 1]);
        assert!(found.iter().all(|m| m.positions.is_empty()));
    }

    /// Run with `cargo test --release -p subwy-project -- --ignored --nocapture fuzzy`.
    #[test]
    #[ignore]
    fn fifty_thousand_paths_per_keystroke() {
        const WORDS: [&str; 16] = [
            "src",
            "docs",
            "crates",
            "components",
            "tests",
            "editor",
            "project",
            "panel",
            "theme",
            "finder",
            "keymap",
            "session",
            "markdown",
            "preview",
            "status",
            "bar",
        ];
        const EXTENSIONS: [&str; 5] = ["rs", "md", "ts", "json", "toml"];
        let mut seed = 0x2545_f491_u64;
        let mut next = |bound: usize| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) as usize % bound
        };
        let paths: Vec<String> = (0..50_000)
            .map(|_| {
                let depth = 1 + next(5);
                let mut path: Vec<String> = (0..depth)
                    .map(|_| WORDS[next(WORDS.len())].to_string())
                    .collect();
                let name = format!(
                    "{}_{}.{}",
                    WORDS[next(WORDS.len())],
                    WORDS[next(WORDS.len())],
                    EXTENSIONS[next(EXTENSIONS.len())]
                );
                path.push(name);
                path.join("/")
            })
            .collect();
        for query in [
            "f",
            "fi",
            "fin",
            "find",
            "finder",
            "finder.rs",
            "srcedpan",
            "zzz",
        ] {
            // The best of a few runs: other work on the machine only ever adds time.
            let mut best = f64::MAX;
            let mut shown = 0;
            for _ in 0..5 {
                let start = std::time::Instant::now();
                shown = search(query, &paths, 12).matches.len();
                best = best.min(start.elapsed().as_secs_f64() * 1000.);
            }
            println!("{query:>10}: {best:>6.2} ms, {shown} shown");
        }
    }
}
