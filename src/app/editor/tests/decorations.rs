use std::ops::Range;

use super::super::{
    BufferModel, ContributionSource, ResolvedEditorContribution,
    layout::{Row, decoration_fills},
    project_contributions,
};
use knot_protocol::{ByteRange, Decoration, DecorationStyle};

use DecorationStyle::{Background, Line};

fn row(line: usize, range: Range<usize>, last: bool) -> Row {
    Row {
        index: 0,
        line,
        range,
        last,
        indent: 0,
    }
}

/// The backgrounds `decorations` on `text` paint on `rows`, as each fill's
/// row position, columns, and key.
fn fills(
    text: &str,
    decorations: &[(Range<usize>, &str, DecorationStyle)],
    rows: &[Row],
) -> Vec<(usize, Option<Range<usize>>, String)> {
    let model = BufferModel::from_text(text);
    let contributions = decorations
        .iter()
        .map(|(range, key, style)| ResolvedEditorContribution {
            range: ByteRange {
                start_byte_offset: range.start,
                end_byte_offset: range.end,
            },
            source: ContributionSource::fixture(),
            decoration: Some(Decoration::new(key, *style)),
            gutter: None,
            command: None,
        })
        .collect();
    let (decorations, ..) = project_contributions(&model, contributions);
    decoration_fills(&decorations, rows)
        .into_iter()
        .map(|fill| (fill.row, fill.columns, fill.key.to_owned()))
        .collect()
}

#[test]
fn a_background_ends_at_the_text_on_each_row_it_crosses() {
    // "one two three" wraps after "one two ".
    let rows = [row(0, 0..8, false), row(0, 8..13, true), row(1, 0..1, true)];
    assert_eq!(
        fills("one two three\nx", &[(4..11, "match", Background)], &rows),
        [
            (0, Some(4..8), "match".into()),
            (1, Some(8..11), "match".into())
        ]
    );
    // Continuing onto the next line still stops at the text, not the edge.
    assert_eq!(
        fills("one two three\nx", &[(4..15, "match", Background)], &rows),
        [
            (0, Some(4..8), "match".into()),
            (1, Some(8..13), "match".into()),
            (2, Some(0..1), "match".into()),
        ]
    );
}

#[test]
fn a_line_decoration_bands_every_row_of_its_lines_including_blank_ones() {
    let text = "a b c\n\nd\ne";
    let rows = [
        row(0, 0..2, false),
        row(0, 2..5, true),
        row(1, 0..0, true),
        row(2, 0..1, true),
        row(3, 0..1, true),
    ];
    // From the start of line 0 to the start of line 2 covers lines 0 and 1.
    assert_eq!(
        fills(text, &[(0..7, "diff", Line)], &rows),
        [
            (0, None, "diff".into()),
            (1, None, "diff".into()),
            (2, None, "diff".into()),
        ]
    );
    // Ending inside line 2 includes it.
    assert_eq!(
        fills(text, &[(1..8, "diff", Line)], &rows)
            .iter()
            .map(|fill| fill.0)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
}

#[test]
fn an_empty_line_decoration_bands_its_line_and_an_empty_background_paints_nothing() {
    let rows = [row(0, 0..2, true), row(1, 0..2, true)];
    assert_eq!(
        fills("ab\ncd", &[(4..4, "diff", Line)], &rows),
        [(1, None, "diff".into())]
    );
    assert_eq!(fills("ab\ncd", &[(4..4, "match", Background)], &rows), []);
    // A background over a blank line paints nothing on it.
    let rows = [row(0, 0..2, true), row(1, 0..0, true), row(2, 0..2, true)];
    assert_eq!(
        fills("ab\n\ncd", &[(0..6, "match", Background)], &rows),
        [
            (0, Some(0..2), "match".into()),
            (2, Some(0..2), "match".into())
        ]
    );
}

#[test]
fn bands_paint_beneath_backgrounds_and_severity_keys_paint_last() {
    let rows = [row(0, 0..3, true)];
    assert_eq!(
        fills(
            "abc",
            &[
                (0..3, "error", Background),
                (0..3, "warning", Line),
                (0..3, "match", Background),
                (0..3, "diff", Line),
            ],
            &rows,
        ),
        [
            (0, None, "diff".into()),
            (0, None, "warning".into()),
            (0, Some(0..3), "match".into()),
            (0, Some(0..3), "error".into()),
        ]
    );
}
