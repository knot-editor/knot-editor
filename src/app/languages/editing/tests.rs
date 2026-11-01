use super::*;

fn c_like() -> EditingConfiguration {
    EditingConfiguration {
        line_comments: vec!["//".into(), "///".into(), "//!".into()],
        block_comment: Some(("/*".into(), "*/".into())),
        ..Default::default()
    }
}

fn markdown() -> EditingConfiguration {
    EditingConfiguration {
        block_comment: Some(("<!--".into(), "-->".into())),
        ..Default::default()
    }
}

fn edit(line: usize, range: Range<usize>, text: &str) -> CommentEdit {
    CommentEdit {
        line,
        range,
        text: text.into(),
    }
}

#[test]
fn commenting_inserts_the_first_token_at_the_smallest_indentation() {
    let rust = c_like();
    assert_eq!(
        rust.toggle_comments(&["    a", "  b", "", "      c"]),
        [
            edit(0, 2..2, "// "),
            edit(1, 2..2, "// "),
            edit(3, 2..2, "// ")
        ]
    );
    // Mixed commented and uncommented lines comment every line.
    assert_eq!(
        rust.toggle_comments(&["// a", "b"]),
        [edit(0, 0..0, "// "), edit(1, 0..0, "// ")]
    );
}

#[test]
fn uncommenting_strips_the_longest_token_and_one_space() {
    let rust = c_like();
    assert_eq!(
        rust.toggle_comments(&["  /// doc", "  //x", "", "//!  y", "    //"]),
        [
            edit(0, 2..6, ""),
            edit(1, 2..4, ""),
            edit(3, 0..4, ""),
            edit(4, 4..6, ""),
        ]
    );
}

#[test]
fn a_block_only_language_wraps_and_unwraps_each_line() {
    let markdown = markdown();
    assert_eq!(
        markdown.toggle_comments(&["  a", "b  "]),
        [
            edit(0, 0..0, "<!-- "),
            edit(0, 3..3, " -->"),
            edit(1, 0..0, "<!-- "),
            edit(1, 1..1, " -->"),
        ]
    );
    assert_eq!(
        markdown.toggle_comments(&["<!-- a -->", "  <!--b-->  ", "<!-- -->"]),
        [
            edit(0, 0..5, ""),
            edit(0, 6..10, ""),
            edit(1, 2..6, ""),
            edit(1, 7..10, ""),
            edit(2, 0..5, ""),
            edit(2, 5..8, ""),
        ]
    );
    // A line comment token wins over the block comment.
    assert_eq!(c_like().toggle_comments(&["/* a */"])[0].text, "// ");
}

#[test]
fn nothing_to_toggle_without_tokens_or_text() {
    assert!(
        EditingConfiguration::default()
            .toggle_comments(&["a"])
            .is_empty()
    );
    assert!(c_like().toggle_comments(&["", "  "]).is_empty());
}
