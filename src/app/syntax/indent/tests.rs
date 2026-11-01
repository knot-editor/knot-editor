//! The indent captures and properties against small inline queries.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::app::languages::editing::{BracketPair, EditingDefinition};
use crate::app::syntax::{LanguageResolver, SyntaxLanguage, SyntaxState};
use crate::core::buffer::TextBuffer;

const BLOCKS: &str = r#"
(block) @indent.begin
(block "}" @indent.end)
"}" @indent.branch
"#;

const MARKDOWN_INJECTIONS: &str = r#"
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)
"#;

struct Source<'a> {
    lines: Vec<&'a str>,
    starts: Vec<usize>,
}

impl<'a> Source<'a> {
    fn new(text: &'a str) -> Self {
        let lines: Vec<&str> = text.split('\n').collect();
        let starts = lines
            .iter()
            .scan(0, |start, line| {
                let this = *start;
                *start += line.len() + 1;
                Some(this)
            })
            .collect();
        Self { lines, starts }
    }
}

impl LineSource for Source<'_> {
    fn line(&self, row: usize) -> String {
        self.lines[row].into()
    }

    fn line_start(&self, row: usize) -> usize {
        self.starts[row]
    }
}

/// Rust with `indents` and its bracket pairs, and Markdown injecting it.
fn resolver(indents: &str) -> LanguageResolver {
    let pair = |open: &str, close: &str| BracketPair {
        open: open.into(),
        close: close.into(),
        auto_close: None,
        newline: None,
        not_in: None,
    };
    let editing = EditingDefinition {
        brackets: Some(vec![
            pair("{", "}"),
            pair("[", "]"),
            pair("(", ")"),
            pair("\"", "\""),
        ]),
        ..Default::default()
    };
    let rust = Arc::new(
        SyntaxLanguage::new("rust", "", "")
            .unwrap()
            .with_indents(indents)
            .unwrap()
            .with_editing(editing.resolve()),
    );
    let markdown = Arc::new(SyntaxLanguage::new("markdown", "", MARKDOWN_INJECTIONS).unwrap());
    let by_name: HashMap<&str, Arc<SyntaxLanguage>> = [("rust", rust), ("markdown", markdown)]
        .into_iter()
        .collect();
    Arc::new(move |name| by_name.get(name).cloned())
}

/// `text` in `language` with every line reindented at `size` columns per
/// level, in spaces.
fn reindent_in(language: &str, indents: &str, text: &str, size: usize) -> String {
    let lines = Source::new(text);
    reindent_rows(language, indents, text, 0..lines.lines.len(), size)
}

fn reindent_rows(
    language: &str,
    indents: &str,
    text: &str,
    rows: Range<usize>,
    size: usize,
) -> String {
    let resolver = resolver(indents);
    let mut state = SyntaxState::new(resolver(language).unwrap(), resolver);
    let buffer = TextBuffer::from_text(text);
    assert!(state.parse_in_foreground(&buffer.snapshot(), Duration::from_secs(60)));
    let source = Source::new(text);
    let changes: HashMap<usize, usize> = state
        .reindent(&buffer.snapshot(), &source, rows, size)
        .into_iter()
        .collect();
    source
        .lines
        .iter()
        .enumerate()
        .map(|(row, line)| match changes.get(&row) {
            Some(&columns) => " ".repeat(columns) + line.trim_start_matches([' ', '\t']),
            None => (*line).to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn reindent(indents: &str, text: &str) -> String {
    reindent_in("rust", indents, text, 4)
}

/// The indentation `row` of `text` gets.
fn line_indent(indents: &str, text: &str, row: usize) -> Option<LineIndent> {
    let resolver = resolver(indents);
    let mut state = SyntaxState::new(resolver("rust").unwrap(), resolver);
    let buffer = TextBuffer::from_text(text);
    assert!(state.parse_in_foreground(&buffer.snapshot(), Duration::from_secs(60)));
    state.line_indent(&buffer.snapshot(), &Source::new(text), row, 4)
}

#[test]
fn begin_indents_the_lines_inside_a_node_and_branch_takes_its_close_back() {
    let text = "fn a() {\nlet x = 1;\nif x {\n\ty();\n      }\n}";
    assert_eq!(
        reindent(BLOCKS, text),
        "fn a() {\n    let x = 1;\n    if x {\n        y();\n    }\n}"
    );
}

#[test]
fn a_row_adds_at_most_one_level() {
    // The closure's block and the call's block both start on the first
    // line, so its body is one level deeper, not two.
    let text = "fn a() { f(|| {\nb();\n}) }";
    assert_eq!(reindent(BLOCKS, text), "fn a() { f(|| {\n    b();\n}) }");
}

#[test]
fn a_blank_line_indents_from_the_last_node_of_the_previous_line() {
    let text = "fn a() {\n    if x {\n        y();\n    }\n\n}";
    // After a closing brace, the line belongs to the outer block.
    assert_eq!(line_indent(BLOCKS, text, 4), Some(LineIndent::Columns(4)));
    let text = "fn a() {\n\n}";
    assert_eq!(line_indent(BLOCKS, text, 1), Some(LineIndent::Columns(4)));
    // A trailing comment does not count as the last node.
    let text = "fn a() { // note\n\n}";
    assert_eq!(line_indent(BLOCKS, text, 1), Some(LineIndent::Columns(4)));
    // Without `@indent.end` the line stays inside the inner block.
    let text = "fn a() {\n    if x {\n        y();\n    }\n\n}";
    assert_eq!(
        line_indent("(block) @indent.begin", text, 4),
        Some(LineIndent::Columns(8))
    );
}

#[test]
fn several_rows_leave_blank_lines_alone_but_a_single_row_indents_one() {
    let text = "fn a() {\n\nb();\n}";
    assert_eq!(reindent(BLOCKS, text), "fn a() {\n\n    b();\n}");
    assert_eq!(
        reindent_rows("rust", BLOCKS, text, 1..2, 4),
        "fn a() {\n    \nb();\n}"
    );
}

#[test]
fn dedent_takes_back_the_level_of_a_node_starting_on_an_earlier_row() {
    let indents = r#"
[(impl_item) (where_clause)] @indent.begin
(impl_item (where_clause) @indent.dedent)
(impl_item (declaration_list) @indent.branch)
["where" "}"] @indent.branch
"#;
    let text = "impl<T> A for T\nwhere\nT: B,\n{\n}";
    assert_eq!(
        reindent(indents, text),
        "impl<T> A for T\nwhere\n    T: B,\n{\n}"
    );
}

#[test]
fn ignore_keeps_the_lines_inside_a_string() {
    let indents = &format!("{BLOCKS}(string_literal) @indent.ignore");
    let text = "fn a() {\nlet s = \"one\n  two\n three\";\n}";
    assert_eq!(
        reindent(indents, text),
        "fn a() {\n    let s = \"one\n  two\n three\";\n}"
    );
    assert_eq!(line_indent(indents, text, 2), Some(LineIndent::Keep));
}

#[test]
fn auto_moves_a_block_comment_with_its_first_line() {
    let indents = &format!("{BLOCKS}(block_comment) @indent.auto");
    let text = "fn a() {\n/*\n * x\n */\n}";
    assert_eq!(
        reindent(indents, text),
        "fn a() {\n    /*\n     * x\n     */\n}"
    );
    assert_eq!(line_indent(indents, text, 2), Some(LineIndent::Follow(1)));
    // Without its first line, a comment's lines stay where they are.
    assert_eq!(reindent_rows("rust", indents, text, 2..4, 4), text);
    // Moving left keeps the alignment too.
    let text = "fn a() {\n        /*\n         * x\n         */\n}";
    assert_eq!(
        reindent(indents, text),
        "fn a() {\n    /*\n     * x\n     */\n}"
    );
}

#[test]
fn zero_puts_a_line_at_column_0() {
    let indents = &format!("{BLOCKS}((identifier) @indent.zero (#eq? @indent.zero \"label\"))");
    let text = "fn a() {\n    label;\n    b();\n}";
    assert_eq!(reindent(indents, text), "fn a() {\nlabel;\n    b();\n}");
}

const ALIGN: &str = r#"
((arguments) @indent.align
  (#set! indent.open_delimiter "(")
  (#set! indent.close_delimiter ")"))
"#;

#[test]
fn align_aligns_after_the_open_delimiter_or_hangs_one_level() {
    assert_eq!(reindent(ALIGN, "call(a,\nb);"), "call(a,\n     b);");
    assert_eq!(reindent(ALIGN, "call(\nb);"), "call(\n    b);");
    // Earlier rows' new indentation moves the delimiter.
    let indents = format!("{ALIGN}{BLOCKS}");
    assert_eq!(
        reindent(&indents, "fn a() {\ncall(a,\nb);\n}"),
        "fn a() {\n    call(a,\n         b);\n}"
    );
    let increment = r#"
((arguments) @indent.align
  (#set! indent.open_delimiter "(")
  (#set! indent.increment 0))
"#;
    assert_eq!(reindent(increment, "call(a,\nb);"), "call(a,\n    b);");
}

#[test]
fn align_indents_a_closing_line_that_would_match_the_next_block() {
    let indents = r#"
((parameters) @indent.align
  (#set! indent.open_delimiter "(")
  (#set! indent.close_delimiter ")")
  (#set! indent.avoid_last_matching_next))
(block) @indent.begin
"}" @indent.branch
"#;
    assert_eq!(
        reindent(indents, "fn f(\na: u32) {\nx();\n}"),
        "fn f(\n        a: u32) {\n    x();\n}"
    );
    assert_eq!(
        reindent(indents, "fn f(a: u32,\nb: u32) {\nx();\n}"),
        "fn f(a: u32,\n     b: u32) {\n    x();\n}"
    );
}

#[test]
fn immediate_and_start_at_same_line_indent_from_a_single_line_node() {
    let text = "fn a() {\nfoo();\n}";
    let both = r#"
(block) @indent.begin
"}" @indent.branch
((call_expression) @indent.begin
  (#set! indent.immediate)
  (#set! indent.start_at_same_line))
"#;
    assert_eq!(reindent(both, text), "fn a() {\n        foo();\n}");
    for one in ["indent.immediate", "indent.start_at_same_line"] {
        let indents = format!("{BLOCKS}((call_expression) @indent.begin (#set! {one}))");
        assert_eq!(reindent(&indents, text), "fn a() {\n    foo();\n}", "{one}");
    }
}

#[test]
fn node_predicates_compare_kinds() {
    let text = "fn a() {\nfoo();\nbar;\n}";
    let has_parent = r#"
(block) @indent.begin
"}" @indent.branch
((identifier) @indent.zero (#has-parent? @indent.zero call_expression))
"#;
    assert_eq!(reindent(has_parent, text), "fn a() {\nfoo();\n    bar;\n}");
    let not_has_parent = r#"
(block) @indent.begin
"}" @indent.branch
((identifier) @indent.zero (#not-has-parent? @indent.zero call_expression))
"#;
    assert_eq!(
        reindent(not_has_parent, text),
        "fn a() {\n    foo();\nbar;\n}"
    );
    let kind = r#"
(block) @indent.begin
"}" @indent.branch
((_) @indent.zero (#kind-eq? @indent.zero identifier))
"#;
    assert_eq!(reindent(kind, text), "fn a() {\nfoo();\nbar;\n}");
    let not_kind = r#"
(block) @indent.begin
"}" @indent.branch
((_) @indent.zero (#not-kind-eq? @indent.zero identifier block))
"#;
    assert_eq!(
        reindent(not_kind, text),
        "fn a() {\n    foo();\n    bar;\n}"
    );
}

#[test]
fn unsupported_predicates_and_bad_queries_fail_to_compile() {
    let compile = |indents| {
        SyntaxLanguage::new("rust", "", "")
            .unwrap()
            .with_indents(indents)
    };
    let Err(SyntaxLanguageError::InvalidQuery {
        query,
        message,
        row,
        column,
    }) = compile("(block) @indent.begin\n  ((identifier) @x (#lua-match? @x \"^a\"))")
    else {
        panic!("compiled");
    };
    assert_eq!((query, row, column), (SyntaxQuery::Indents, 1, 2));
    assert!(message.contains("lua-match?"), "{message}");
    assert!(matches!(
        compile("(not_a_node) @indent.begin"),
        Err(SyntaxLanguageError::InvalidQuery {
            query: SyntaxQuery::Indents,
            ..
        })
    ));
    assert!(compile("  ").is_ok());
}

#[test]
fn languages_without_an_indent_query_leave_lines_alone() {
    let text = "fn a() {\nb();\n}";
    assert_eq!(line_indent("", text, 1), None);
    assert_eq!(reindent("", text), text);
}

#[test]
fn an_injected_layer_indents_from_its_first_line_with_its_own_query() {
    let text = "# Title\n\n```rust\nfn a() {\nb();\n}\n```\nplain\n";
    assert_eq!(
        reindent_in("markdown", BLOCKS, text, 4),
        "# Title\n\n```rust\nfn a() {\n    b();\n}\n```\nplain\n"
    );
    let text = "- item\n\n  ```rust\n  fn a() {\n  b();\n  }\n  ```\n";
    assert_eq!(
        reindent_in("markdown", BLOCKS, text, 4),
        "- item\n\n  ```rust\n  fn a() {\n      b();\n  }\n  ```\n"
    );
}

#[test]
fn an_unclosed_bracket_in_a_parse_error_indents_the_lines_after_it() {
    // `fn main() {` alone parses as an ERROR holding the brace.
    assert_eq!(
        line_indent(BLOCKS, "fn main() {\n", 1),
        Some(LineIndent::Columns(4))
    );
    assert_eq!(
        line_indent(BLOCKS, "fn main() {\n    a();\n", 2),
        Some(LineIndent::Columns(4))
    );
    // An unclosed `if` takes the function's closing brace, leaving the
    // function's opening brace unclosed in the error.
    let text = "fn f() {\n    let x = 1;\n    if x {\n\n    b();\n}\n";
    assert_eq!(line_indent(BLOCKS, text, 3), Some(LineIndent::Columns(8)));
    assert_eq!(
        reindent(BLOCKS, "fn f() {\nif x {\na();\n}\n"),
        "fn f() {\n    if x {\n        a();\n    }\n"
    );
    // Closed brackets and quotes inside the error do not count.
    assert_eq!(
        line_indent(BLOCKS, "fn main() { let s = \"(\"; a[0];\n", 1),
        Some(LineIndent::Columns(4))
    );
}
