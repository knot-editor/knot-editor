//! Reloading a document from changed file content: what a check of the file
//! found, and the minimal line edits that turn the buffer into the file.

use imara_diff::{Algorithm, Diff, InternedInput};
use knot_protocol::{ByteRange, TextEdit};

use crate::filesystem::ResourceVersion;

/// What reading a persisted document's file found.
#[derive(Debug, Eq, PartialEq)]
pub enum FileCheck {
    /// The provider version is the recorded one: Knot's own save, or no change.
    Unchanged,
    /// The version moved but the content is the saved content, as after a
    /// `touch`; only the recorded version needs to change.
    VersionOnly,
    /// The file holds different content.
    Changed,
}

/// Compare a read of the file with the document's recorded version and its
/// saved content.
pub fn check_file(
    recorded: &ResourceVersion,
    version: &ResourceVersion,
    file_text: &str,
    saved_text: &str,
) -> FileCheck {
    if recorded == version {
        FileCheck::Unchanged
    } else if file_text == saved_text {
        FileCheck::VersionOnly
    } else {
        FileCheck::Changed
    }
}

/// The edits, ascending and disjoint in `old`'s byte offsets, that change
/// `old` into `new`. Only the lines the diff reports as changed are touched,
/// so anchored positions elsewhere stay in place.
pub fn line_edits(old: &str, new: &str) -> Vec<TextEdit> {
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let old_starts = line_starts(old);
    let new_starts = line_starts(new);
    diff.hunks()
        .map(|hunk| {
            let before =
                old_starts[hunk.before.start as usize]..old_starts[hunk.before.end as usize];
            let after = new_starts[hunk.after.start as usize]..new_starts[hunk.after.end as usize];
            TextEdit {
                range: ByteRange {
                    start_byte_offset: before.start,
                    end_byte_offset: before.end,
                },
                text: new[after].to_owned(),
            }
        })
        .collect()
}

/// The byte offset where each line starts, plus the text length.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(index, _)| index + 1));
    if starts.last() != Some(&text.len()) {
        starts.push(text.len());
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, edits: &[TextEdit]) -> String {
        let mut text = old.to_owned();
        for edit in edits.iter().rev() {
            text.replace_range(
                edit.range.start_byte_offset..edit.range.end_byte_offset,
                &edit.text,
            );
        }
        text
    }

    fn assert_turns_into(old: &str, new: &str) -> Vec<TextEdit> {
        let edits = line_edits(old, new);
        assert_eq!(apply(old, &edits), new);
        edits
    }

    #[test]
    fn untouched_lines_are_not_edited() {
        let edits = assert_turns_into("a\nb\nc\nd\n", "a\nB\nc\nd\ne\n");
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].range.start_byte_offset, 2);
        assert_eq!(edits[0].range.end_byte_offset, 4);
        assert_eq!(edits[0].text, "B\n");
    }

    #[test]
    fn crlf_lines_diff_by_line() {
        let edits = assert_turns_into("a\r\nb\r\nc\r\n", "a\r\nx\r\nc\r\n");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].text, "x\r\n");
    }

    #[test]
    fn trailing_newline_changes() {
        assert_turns_into("a\nb", "a\nb\n");
        assert_turns_into("a\nb\n", "a\nb");
        assert_turns_into("a\n", "a");
    }

    #[test]
    fn empty_files() {
        assert_turns_into("", "a\nb\n");
        assert_turns_into("a\nb\n", "");
        assert!(line_edits("", "").is_empty());
        assert!(line_edits("same\n", "same\n").is_empty());
    }

    #[test]
    fn check_compares_version_before_content() {
        let one = ResourceVersion::new([1]);
        let two = ResourceVersion::new([2]);
        assert_eq!(check_file(&one, &one, "x", "y"), FileCheck::Unchanged);
        assert_eq!(check_file(&one, &two, "x", "x"), FileCheck::VersionOnly);
        assert_eq!(check_file(&one, &two, "x", "y"), FileCheck::Changed);
    }
}
