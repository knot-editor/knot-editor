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

#[test]
fn block_toggle_wraps_and_unwraps_inside_surrounding_whitespace() {
    let rust = c_like();
    let block = |text: &str, edits: &[(Range<usize>, &str)]| {
        assert_eq!(
            rust.toggle_block_comment(text),
            edits
                .iter()
                .map(|(range, text)| (range.clone(), text.to_string()))
                .collect::<Vec<_>>(),
            "{text:?}"
        );
    };
    block("a + b", &[(0..0, "/* "), (5..5, " */")]);
    block("  a\nb\n", &[(2..2, "/* "), (5..5, " */")]);
    block("/* a + b */", &[(0..3, ""), (8..11, "")]);
    block(" /*a*/ ", &[(1..3, ""), (4..6, "")]);
    block("/**/", &[(0..2, ""), (2..4, "")]);
    block("", &[(0..0, "/*  */")]);
    block("  ", &[(0..0, "/*  */")]);
    // A comment only at one end is wrapped again.
    block("/* a", &[(0..0, "/* "), (4..4, " */")]);

    assert_eq!(
        markdown().toggle_block_comment("<!-- x -->"),
        [(0..5, String::new()), (6..10, String::new())]
    );
    let python = EditingConfiguration {
        line_comments: vec!["#".into()],
        ..Default::default()
    };
    assert!(python.toggle_block_comment("a").is_empty());
}

fn pair(open: &str, close: &str) -> Bracket {
    Bracket {
        open: open.into(),
        close: close.into(),
        auto_close: true,
        newline: open != close,
        not_in: Vec::new(),
    }
}

fn brackets() -> EditingConfiguration {
    let mut quote = pair("\"", "\"");
    quote.not_in = vec![SyntaxFamily::String, SyntaxFamily::Comment];
    let mut single = pair("'", "'");
    single.auto_close = false;
    EditingConfiguration {
        brackets: vec![
            pair("{", "}"),
            pair("(", ")"),
            quote,
            single,
            pair("/**", " */"),
        ],
        auto_close_before: ";:.,=}])>".into(),
        ..Default::default()
    }
}

fn close<'a>(
    config: &'a EditingConfiguration,
    before: &str,
    typed: &str,
    after: &str,
    family: Option<SyntaxFamily>,
) -> Option<&'a str> {
    config
        .auto_close(before, typed, after, family)
        .map(|bracket| bracket.close.as_str())
}

#[test]
fn an_opener_closes_before_whitespace_the_line_end_and_listed_characters() {
    let config = brackets();
    assert_eq!(close(&config, "f", "(", "", None), Some(")"));
    assert_eq!(close(&config, "f", "(", " x", None), Some(")"));
    assert_eq!(close(&config, "[", "(", "];", None), Some(")"));
    assert_eq!(close(&config, "f", "(", "x", None), None);
    assert_eq!(close(&config, "", "x", "", None), None);
    assert_eq!(close(&config, "", "", "", None), None);
}

#[test]
fn a_pair_that_does_not_auto_close_still_surrounds() {
    let config = brackets();
    assert_eq!(close(&config, "", "'", "", None), None);
    assert_eq!(
        config.surround("'").map(|pair| pair.close.as_str()),
        Some("'")
    );
    assert_eq!(config.surround("x"), None);
}

#[test]
fn quotes_close_outside_their_syntax_and_after_non_word_characters() {
    let config = brackets();
    assert_eq!(close(&config, "x = ", "\"", "", None), Some("\""));
    assert_eq!(close(&config, "(", "\"", ")", None), Some("\""));
    assert_eq!(close(&config, "don", "\"", "", None), None);
    assert_eq!(close(&config, "a_", "\"", "", None), None);
    assert_eq!(
        close(&config, "x = ", "\"", "", Some(SyntaxFamily::String)),
        None
    );
    assert_eq!(
        close(&config, "// ", "\"", "", Some(SyntaxFamily::Comment)),
        None
    );
    // Brackets have no `notIn`, so they close inside strings.
    assert_eq!(
        close(&config, "\"", "(", " x\"", Some(SyntaxFamily::String)),
        Some(")")
    );
}

#[test]
fn a_multi_character_opener_closes_when_typing_completes_it() {
    let config = brackets();
    assert_eq!(close(&config, "  /*", "*", "", None), Some(" */"));
    assert_eq!(close(&config, "  /", "*", "", None), None);
}
