//! Line diffs between two texts: the hunks that turn one into the other.
//! Reload's minimal edits and the extension API's `knot.diff` share it, so
//! both agree on what changed.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput, sources::lines};

/// How lines are compared.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiffOptions {
    /// Lines are equal when they match after removing all whitespace, which
    /// also ignores `\r\n` against `\n`.
    pub ignore_whitespace: bool,
}

/// One changed region: the lines of the old text replaced by the lines of the
/// new text. An empty `old` range is an insertion before `old.start`; an empty
/// `new` range is a deletion. Ranges are 0-based, half-open line indices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineHunk {
    pub old: Range<u32>,
    pub new: Range<u32>,
}

/// A line that compares by its text, or by its non-whitespace characters, and
/// reads as its full text to the diff's indentation heuristic.
#[derive(Default)]
struct Line<'a> {
    text: &'a str,
    ignore_whitespace: bool,
}

impl Line<'_> {
    fn significant(&self) -> impl Iterator<Item = char> + '_ {
        self.text
            .chars()
            .filter(|c| !self.ignore_whitespace || !c.is_whitespace())
    }
}

impl PartialEq for Line<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.ignore_whitespace {
            self.significant().eq(other.significant())
        } else {
            self.text == other.text
        }
    }
}

impl Eq for Line<'_> {}

impl Hash for Line<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for c in self.significant() {
            c.hash(state);
        }
    }
}

impl AsRef<[u8]> for Line<'_> {
    fn as_ref(&self) -> &[u8] {
        self.text.as_bytes()
    }
}

/// The hunks, ascending and disjoint, that turn `old` into `new`, comparing
/// lines per `options`.
pub fn line_hunks(old: &str, new: &str, options: DiffOptions) -> Vec<LineHunk> {
    let ignore_whitespace = options.ignore_whitespace;
    let wrap = move |text| Line {
        text,
        ignore_whitespace,
    };
    let mut input = InternedInput::default();
    input.update_before(lines(old).map(wrap));
    input.update_after(lines(new).map(wrap));
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    diff.hunks()
        .map(|hunk| LineHunk {
            old: hunk.before,
            new: hunk.after,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunks(old: &str, new: &str) -> Vec<(Range<u32>, Range<u32>)> {
        line_hunks(old, new, DiffOptions::default())
            .into_iter()
            .map(|hunk| (hunk.old, hunk.new))
            .collect()
    }

    fn hunks_ignoring_whitespace(old: &str, new: &str) -> Vec<(Range<u32>, Range<u32>)> {
        let options = DiffOptions {
            ignore_whitespace: true,
        };
        line_hunks(old, new, options)
            .into_iter()
            .map(|hunk| (hunk.old, hunk.new))
            .collect()
    }

    #[test]
    fn insertion_has_an_empty_old_range() {
        assert_eq!(hunks("a\nc\n", "a\nb\nc\n"), [(1..1, 1..2)]);
    }

    #[test]
    fn deletion_has_an_empty_new_range() {
        assert_eq!(hunks("a\nb\nc\n", "a\nc\n"), [(1..2, 1..1)]);
    }

    #[test]
    fn modification_replaces_lines() {
        assert_eq!(hunks("a\nb\nc\n", "a\nB\nc\n"), [(1..2, 1..2)]);
    }

    #[test]
    fn separate_changes_are_separate_hunks() {
        assert_eq!(
            hunks("a\nb\nc\nd\n", "a\nB\nc\nd\ne\n"),
            [(1..2, 1..2), (4..4, 4..5)]
        );
    }

    #[test]
    fn identical_and_empty_texts_have_no_hunks() {
        assert!(hunks("", "").is_empty());
        assert!(hunks("same\n", "same\n").is_empty());
    }

    #[test]
    fn empty_against_text() {
        assert_eq!(hunks("", "a\nb\n"), [(0..0, 0..2)]);
        assert_eq!(hunks("a\nb\n", ""), [(0..2, 0..0)]);
    }

    #[test]
    fn trailing_newline_changes_the_last_line() {
        assert_eq!(hunks("a\nb", "a\nb\n"), [(1..2, 1..2)]);
        assert_eq!(hunks("a\nb\n", "a\nb"), [(1..2, 1..2)]);
    }

    #[test]
    fn terminators_compare_exactly_by_default() {
        assert_eq!(hunks("a\r\nb\r\n", "a\nb\n"), [(0..2, 0..2)]);
        assert_eq!(hunks("a\r\nb\r\nc\r\n", "a\r\nx\r\nc\r\n"), [(1..2, 1..2)]);
    }

    #[test]
    fn whitespace_changes_are_hunks_by_default() {
        assert_eq!(hunks("a b\nc\n", "a  b\nc\n"), [(0..1, 0..1)]);
    }

    #[test]
    fn ignoring_whitespace_skips_whitespace_only_changes() {
        assert!(hunks_ignoring_whitespace("a b\nc\n", "a  b\n\tc\n").is_empty());
        assert!(hunks_ignoring_whitespace("a\r\nb\r\n", "a\nb\n").is_empty());
        assert!(hunks_ignoring_whitespace("a\nb", "a\nb\n").is_empty());
    }

    #[test]
    fn ignoring_whitespace_keeps_real_changes() {
        assert_eq!(
            hunks_ignoring_whitespace("a b\nc\nd\n", "a  b\nC\nd\n"),
            [(1..2, 1..2)]
        );
    }
}
