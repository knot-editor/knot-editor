use super::*;

fn pair(open: &str, close: &str) -> Bracket {
    Bracket {
        open: open.into(),
        close: close.into(),
        auto_close: true,
        newline: open != close,
        not_in: Vec::new(),
    }
}

fn c_like() -> EditingConfiguration {
    EditingConfiguration {
        line_comments: vec!["//".into(), "///".into(), "//!".into()],
        block_comment: Some(("/*".into(), "*/".into())),
        brackets: vec![
            pair("{", "}"),
            pair("(", ")"),
            pair("[", "]"),
            pair("\"", "\""),
            pair("'", "'"),
            Bracket {
                newline: false,
                ..pair("<", ">")
            },
        ],
        ..Default::default()
    }
}

fn python() -> EditingConfiguration {
    EditingConfiguration {
        line_comments: vec!["#".into()],
        brackets: vec![
            pair("(", ")"),
            pair("[", "]"),
            pair("{", "}"),
            pair("\"", "\""),
            pair("'", "'"),
        ],
        increase_indent: Some(Regex::new(r":\s*$").unwrap()),
        decrease_indent: Some(Regex::new(r"^\s*(else|elif|except|finally)\b").unwrap()),
        ..Default::default()
    }
}

fn markdown() -> EditingConfiguration {
    EditingConfiguration {
        block_comment: Some(("<!--".into(), "-->".into())),
        ..Default::default()
    }
}

fn indentation(reference: Option<usize>, levels: i32) -> Indentation {
    Indentation { reference, levels }
}

/// The indentation of a new line after the last of `lines`.
fn after(configuration: &EditingConfiguration, lines: &[&str]) -> Indentation {
    configuration.indentation_after(lines, lines.len() - 1)
}

/// The indentation of the last of `lines`.
fn at(configuration: &EditingConfiguration, lines: &[&str]) -> Indentation {
    configuration.indentation(lines, lines.len() - 1)
}

fn edit(line: usize, range: Range<usize>, text: &str) -> CommentEdit {
    CommentEdit {
        line,
        range,
        text: text.into(),
    }
}

#[test]
fn a_line_ending_in_an_opener_indents_the_next_line() {
    let rust = c_like();
    assert_eq!(after(&rust, &["fn main() {"]), indentation(Some(0), 1));
    assert_eq!(after(&rust, &["x", "    foo("]), indentation(Some(1), 1));
    assert_eq!(after(&rust, &["let a = [  "]), indentation(Some(0), 1));
    assert_eq!(after(&rust, &["let x = 1;"]), indentation(Some(0), 0));
    // An opener closed on the same line does not indent.
    assert_eq!(after(&rust, &["let v = vec![1];"]), indentation(Some(0), 0));
    // Pairs Enter opens no middle line between, such as Rust's angle
    // brackets, take no part: `>` here is a comparison, not a closer.
    assert_eq!(after(&rust, &["foo<"]), indentation(Some(0), 0));
    assert_eq!(after(&rust, &["if a > b {"]), indentation(Some(0), 1));
}

#[test]
fn a_closer_aligns_with_the_line_holding_its_opener() {
    let rust = c_like();
    let lines = [
        "fn main() {",
        "    if x {",
        "        let v = vec![{ 1 }];",
        "        y(|| {",
        "        });",
        "    }",
        "}",
    ];
    assert_eq!(rust.indentation(&lines, 4), indentation(Some(3), 0));
    assert_eq!(rust.indentation(&lines, 5), indentation(Some(1), 0));
    assert_eq!(rust.indentation(&lines, 6), indentation(Some(0), 0));
    assert_eq!(
        at(&rust, &["const a = [", "    1,", "]);"]),
        indentation(Some(0), 0)
    );
    // `} else {` aligns with its opener and indents the line after it.
    let branches = ["if x {", "    a();", "} else {"];
    assert_eq!(at(&rust, &branches), indentation(Some(0), 0));
    assert_eq!(after(&rust, &branches), indentation(Some(0), 1));
}

#[test]
fn a_closer_without_an_opener_outdents_one_level() {
    let rust = c_like();
    assert_eq!(at(&rust, &["    x();", "}"]), indentation(Some(0), -1));
    assert_eq!(at(&rust, &["}"]), indentation(None, -1));
}

#[test]
fn brackets_in_strings_and_comments_are_ignored() {
    let rust = c_like();
    assert_eq!(after(&rust, &[r#"let s = "{";"#]), indentation(Some(0), 0));
    assert_eq!(
        after(&rust, &[r#"let s = "\"{";"#]),
        indentation(Some(0), 0)
    );
    assert_eq!(after(&rust, &["let c = '(';"]), indentation(Some(0), 0));
    assert_eq!(after(&rust, &["// fn main() {"]), indentation(Some(0), 0));
    assert_eq!(after(&rust, &["foo(); /* { */"]), indentation(Some(0), 0));
    assert_eq!(after(&rust, &["foo(); /* {"]), indentation(Some(0), 0));
    assert_eq!(after(&rust, &["fn f() { // note"]), indentation(Some(0), 1));
    let lines = ["fn f() {", r#"    let s = "}";"#, "    // }", "}"];
    assert_eq!(at(&rust, &lines), indentation(Some(0), 0));
    // A quote unclosed on its line is code, so a lifetime hides nothing.
    assert_eq!(
        after(&rust, &["fn f<'a>(x: &'a str) -> &'a str {"]),
        indentation(Some(0), 1)
    );
}

#[test]
fn indentation_patterns_cover_what_brackets_miss() {
    let python = python();
    assert_eq!(after(&python, &["def f():"]), indentation(Some(0), 1));
    assert_eq!(after(&python, &["if x:  # check"]), indentation(Some(0), 1));
    assert_eq!(after(&python, &["# note:"]), indentation(Some(0), 0));
    assert_eq!(after(&python, &[r#"x = "a:""#]), indentation(Some(0), 0));
    assert_eq!(after(&python, &["x = y"]), indentation(Some(0), 0));
    assert_eq!(
        at(&python, &["if x:", "    y = 1", "else:"]),
        indentation(Some(1), -1)
    );
    assert_eq!(at(&python, &["if x:", "else:"]), indentation(Some(0), 0));
    assert_eq!(
        at(&python, &["    y = 1", "elsewhere = 2"]),
        indentation(Some(0), 0)
    );
}

#[test]
fn blank_lines_are_skipped_when_choosing_the_reference() {
    let rust = c_like();
    assert_eq!(
        after(&rust, &["fn f() {", "", "   "]),
        indentation(Some(0), 1)
    );
    assert_eq!(after(&rust, &["", ""]), indentation(None, 0));
    assert_eq!(
        at(&rust, &["fn f() {", "    a();", "", "}"]),
        indentation(Some(0), 0)
    );
    // A comment line is a reference whose code is empty.
    assert_eq!(
        after(&rust, &["fn f() {", "    // note {"]),
        indentation(Some(1), 0)
    );
}

#[test]
fn a_line_closing_an_earlier_opener_continues_from_the_opener_line() {
    let rust = c_like();
    assert_eq!(
        after(&rust, &["    let x = foo(", "        a,", "        b);"]),
        indentation(Some(0), 0)
    );
    assert_eq!(
        after(&rust, &["foo(bar(", "    a))"]),
        indentation(Some(0), 0)
    );
    assert_eq!(
        after(&rust, &["if foo(", "    a,", ") {"]),
        indentation(Some(0), 1)
    );
    let python = python();
    assert_eq!(
        after(&python, &["def f(", "    a,", "    b):"]),
        indentation(Some(0), 1)
    );
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
