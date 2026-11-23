//! Line diffs between two texts: the hunks that turn one into the other.
//! Reload's minimal edits and the extension API's `knot.diff` share it, so
//! both agree on what changed.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput, sources::lines};
use knot_protocol::{ByteRange, Decoration, DecorationStyle, GutterMark, GutterStyle};

use crate::editor::ViewDecoration;
use crate::theme::color;

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

/// Which text of a comparison a view shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffSide {
    Original,
    Modified,
}

/// The first line, on `side`, of the hunk nearest `line` in the direction
/// asked, wrapping at the ends, or `None` without hunks. A hunk with no lines
/// on this side starts at the line that follows where they would be.
pub fn adjacent_hunk(hunks: &[LineHunk], side: DiffSide, line: u32, forward: bool) -> Option<u32> {
    let mut starts = hunks.iter().map(|hunk| match side {
        DiffSide::Original => hunk.old.start,
        DiffSide::Modified => hunk.new.start,
    });
    if forward {
        starts.clone().find(|&start| start > line).or(starts.next())
    } else {
        starts
            .clone()
            .rfind(|&start| start < line)
            .or(starts.next_back())
    }
}

/// Where the line `line` of one side's text lies in the other's, so two views
/// scroll together. Lines outside hunks keep their distance from the nearest
/// hunk above; lines inside a hunk map proportionally onto the other side's
/// lines of it. Fractional lines map continuously, except past a hunk with no
/// lines on this side, which the other side's lines of it are skipped over.
pub fn map_line(hunks: &[LineHunk], from: DiffSide, line: f32) -> f32 {
    let mut offset = 0.;
    for hunk in hunks {
        let (this, other) = match from {
            DiffSide::Original => (&hunk.old, &hunk.new),
            DiffSide::Modified => (&hunk.new, &hunk.old),
        };
        if line < this.start as f32 {
            break;
        }
        if line < this.end as f32 {
            let length = (this.end - this.start) as f32;
            let within = (line - this.start as f32) / length;
            return other.start as f32 + within * (other.end - other.start) as f32;
        }
        offset = other.end as f32 - this.end as f32;
    }
    (line + offset).max(0.)
}

/// The decorations that mark `hunks` on one side's `text`: a `line` band per
/// hunk, in `diff.removed.background` for lines only the original has,
/// `diff.added.background` for lines only the modified text has, and
/// `diff.modified.background` for lines both have. Where the side has no lines
/// of a hunk, a deletion wedge in the gutter, in the solid `diff.added` or
/// `diff.removed`, marks the boundary the other side's lines belong to.
pub fn hunk_decorations(hunks: &[LineHunk], side: DiffSide, text: &str) -> Vec<ViewDecoration> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(index, _)| index + 1));
    if text.ends_with('\n') || text.is_empty() {
        starts.pop();
    }
    // The byte offset where `line` begins; the end of the text past the last.
    let offset = |line: u32| starts.get(line as usize).copied().unwrap_or(text.len());
    hunks
        .iter()
        .map(|hunk| {
            let (lines, other) = match side {
                DiffSide::Original => (&hunk.old, &hunk.new),
                DiffSide::Modified => (&hunk.new, &hunk.old),
            };
            let range = ByteRange {
                start_byte_offset: offset(lines.start),
                end_byte_offset: offset(lines.end),
            };
            if lines.is_empty() {
                // The wedge takes the color of what the other side holds here.
                let key = match side {
                    DiffSide::Original => color::DIFF_ADDED,
                    DiffSide::Modified => color::DIFF_REMOVED,
                };
                return ViewDecoration {
                    range,
                    decoration: None,
                    gutter: Some(GutterMark::new(key, GutterStyle::Deletion)),
                };
            }
            let key = match (other.is_empty(), side) {
                (false, _) => color::DIFF_MODIFIED_BACKGROUND,
                (true, DiffSide::Original) => color::DIFF_REMOVED_BACKGROUND,
                (true, DiffSide::Modified) => color::DIFF_ADDED_BACKGROUND,
            };
            ViewDecoration {
                range,
                decoration: Some(Decoration::new(key, DecorationStyle::Line)),
                gutter: None,
            }
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

    fn mapped(old: &str, new: &str, from: DiffSide, line: f32) -> f32 {
        map_line(&line_hunks(old, new, DiffOptions::default()), from, line)
    }

    #[test]
    fn the_adjacent_hunk_starts_after_or_before_the_line_and_wraps() {
        // Hunks start at lines 1 and 4 (a deletion) of the modified text.
        let found = line_hunks(
            "a\nb\nc\nd\ne\nf\n",
            "a\nB\nc\nd\nf\n",
            DiffOptions::default(),
        );
        let starts: Vec<u32> = found.iter().map(|hunk| hunk.new.start).collect();
        assert_eq!(starts, [1, 4]);
        let next = |line| adjacent_hunk(&found, DiffSide::Modified, line, true);
        let previous = |line| adjacent_hunk(&found, DiffSide::Modified, line, false);
        assert_eq!(
            (next(0), next(1), next(2), next(4)),
            (Some(1), Some(4), Some(4), Some(1))
        );
        assert_eq!(
            (previous(5), previous(4), previous(2), previous(1)),
            (Some(4), Some(1), Some(1), Some(4))
        );
        // On the original side the hunks start at lines 1 and 4.
        let old = |line| adjacent_hunk(&found, DiffSide::Original, line, true);
        assert_eq!((old(0), old(1), old(4)), (Some(1), Some(4), Some(1)));
        assert_eq!(adjacent_hunk(&[], DiffSide::Original, 0, true), None);
        assert_eq!(adjacent_hunk(&[], DiffSide::Original, 0, false), None);
    }

    #[test]
    fn lines_outside_hunks_keep_their_distance() {
        // One line inserted at line 1 of the modified text.
        let (old, new) = ("a\nc\nd\n", "a\nb\nc\nd\n");
        assert_eq!(mapped(old, new, DiffSide::Original, 0.), 0.);
        assert_eq!(mapped(old, new, DiffSide::Original, 1.), 2.);
        assert_eq!(mapped(old, new, DiffSide::Original, 2.5), 3.5);
        assert_eq!(mapped(old, new, DiffSide::Modified, 0.5), 0.5);
        assert_eq!(mapped(old, new, DiffSide::Modified, 2.), 1.);
        assert_eq!(mapped(old, new, DiffSide::Modified, 3.), 2.);
    }

    #[test]
    fn lines_inside_a_hunk_map_proportionally() {
        // Two lines become four: lines 1..3 against 1..5.
        let (old, new) = ("a\nb\nc\nd\n", "a\nX\nY\nZ\nW\nd\n");
        assert_eq!(mapped(old, new, DiffSide::Original, 1.), 1.);
        assert_eq!(mapped(old, new, DiffSide::Original, 2.), 3.);
        assert_eq!(mapped(old, new, DiffSide::Modified, 2.), 1.5);
        assert_eq!(mapped(old, new, DiffSide::Modified, 5.), 3.);
    }

    #[test]
    fn a_hunk_without_lines_on_this_side_is_skipped_over() {
        let (old, new) = ("a\nd\n", "a\nb\nc\nd\n");
        assert_eq!(mapped(old, new, DiffSide::Original, 1.), 3.);
        assert_eq!(mapped(old, new, DiffSide::Original, 0.5), 0.5);
        assert_eq!(mapped(old, new, DiffSide::Modified, 1.), 1.);
        assert_eq!(mapped(old, new, DiffSide::Modified, 2.), 1.);
    }

    #[test]
    fn mapping_with_empty_sides_and_no_hunks() {
        assert_eq!(map_line(&[], DiffSide::Original, 3.5), 3.5);
        assert_eq!(mapped("", "a\nb\n", DiffSide::Modified, 1.), 0.);
        assert_eq!(mapped("", "a\nb\n", DiffSide::Original, 0.), 2.);
        assert_eq!(mapped("a\nb\n", "", DiffSide::Original, 1.), 0.);
    }

    #[test]
    fn mapping_beyond_the_last_line_keeps_the_offset() {
        let (old, new) = ("a\n", "x\ny\na\n");
        assert_eq!(mapped(old, new, DiffSide::Original, 4.), 6.);
    }

    /// The bands and wedges as (byte range, decoration key, wedge key).
    fn marks(
        old: &str,
        new: &str,
        side: DiffSide,
    ) -> Vec<(Range<usize>, Option<String>, Option<String>)> {
        let hunks = line_hunks(old, new, DiffOptions::default());
        let text = if side == DiffSide::Original { old } else { new };
        hunk_decorations(&hunks, side, text)
            .into_iter()
            .map(|item| {
                (
                    item.range.start_byte_offset..item.range.end_byte_offset,
                    item.decoration.map(|decoration| decoration.color),
                    item.gutter.map(|mark| mark.color),
                )
            })
            .collect()
    }

    fn band(range: Range<usize>, key: &str) -> (Range<usize>, Option<String>, Option<String>) {
        (range, Some(key.to_owned()), None)
    }

    fn wedge(offset: usize, key: &str) -> (Range<usize>, Option<String>, Option<String>) {
        (offset..offset, None, Some(key.to_owned()))
    }

    #[test]
    fn a_replaced_line_is_modified_on_both_sides() {
        let (old, new) = ("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(
            marks(old, new, DiffSide::Original),
            [band(2..4, "diff.modified.background")]
        );
        assert_eq!(
            marks(old, new, DiffSide::Modified),
            [band(2..4, "diff.modified.background")]
        );
    }

    #[test]
    fn an_insertion_is_added_with_a_wedge_on_the_original() {
        let (old, new) = ("a\nc\n", "a\nb\nc\n");
        assert_eq!(
            marks(old, new, DiffSide::Original),
            [wedge(2, "diff.added")]
        );
        assert_eq!(
            marks(old, new, DiffSide::Modified),
            [band(2..4, "diff.added.background")]
        );
    }

    #[test]
    fn a_deletion_is_removed_with_a_wedge_on_the_modified_side() {
        let (old, new) = ("a\nb\nc\n", "a\nc\n");
        assert_eq!(
            marks(old, new, DiffSide::Original),
            [band(2..4, "diff.removed.background")]
        );
        assert_eq!(
            marks(old, new, DiffSide::Modified),
            [wedge(2, "diff.removed")]
        );
    }

    #[test]
    fn an_empty_side_gets_a_wedge_at_the_start() {
        assert_eq!(
            marks("", "a\nb\n", DiffSide::Original),
            [wedge(0, "diff.added")]
        );
        assert_eq!(
            marks("", "a\nb\n", DiffSide::Modified),
            [band(0..4, "diff.added.background")]
        );
        assert_eq!(
            marks("a\n", "", DiffSide::Modified),
            [wedge(0, "diff.removed")]
        );
    }

    #[test]
    fn a_final_line_without_a_newline_ends_at_the_text() {
        let (old, new) = ("a\nb", "a\nb\nc");
        assert_eq!(
            marks(old, new, DiffSide::Original),
            [band(2..3, "diff.modified.background")]
        );
        assert_eq!(
            marks(old, new, DiffSide::Modified),
            [band(2..5, "diff.modified.background")]
        );
        assert_eq!(
            marks("a", "a\nb", DiffSide::Original),
            [band(0..1, "diff.modified.background")]
        );
        assert_eq!(
            marks("a\nb", "a\nb\n", DiffSide::Original),
            [band(2..3, "diff.modified.background")]
        );
        assert_eq!(
            marks("a\n", "a\nb", DiffSide::Original),
            [wedge(2, "diff.added")]
        );
    }
}
