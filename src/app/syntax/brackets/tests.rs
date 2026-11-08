use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::app::languages::editing::{BracketPair as RegisteredPair, EditingDefinition};
use crate::app::syntax::{LanguageResolver, SyntaxLanguage};
use knot_core::buffer::TextBuffer;

const MARKDOWN_INJECTIONS: &str = r#"
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)
"#;

/// Rust with its bracket pairs and a quote, and Markdown injecting it.
fn resolver() -> LanguageResolver {
    let pair = |open: &str, close: &str| RegisteredPair {
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
            .with_editing(editing.resolve()),
    );
    let markdown = Arc::new(SyntaxLanguage::new("markdown", "", MARKDOWN_INJECTIONS).unwrap());
    let by_name: HashMap<&str, Arc<SyntaxLanguage>> = [("rust", rust), ("markdown", markdown)]
        .into_iter()
        .collect();
    Arc::new(move |name| by_name.get(name).cloned())
}

/// `text` with one `|` marking an offset, parsed as `language`.
struct Case {
    state: SyntaxState,
    buffer: TextBuffer,
    offset: usize,
}

impl Case {
    fn new(language: &str, marked: &str) -> Self {
        let offset = marked.find('|').expect("a | marker");
        let text = marked.replacen('|', "", 1);
        let resolver = resolver();
        let mut state = SyntaxState::new(resolver(language).unwrap(), resolver);
        let buffer = TextBuffer::from_text(text);
        assert!(state.parse_in_foreground(&buffer.snapshot(), Duration::from_secs(60)));
        Self {
            state,
            buffer,
            offset,
        }
    }

    /// The bracket at the marker and its partner, as their texts' starts.
    fn at(&self) -> Option<(usize, usize)> {
        self.state
            .bracket_at(&self.buffer.snapshot(), self.offset)
            .map(|(bracket, partner)| (bracket.start, partner.start))
    }

    fn enclosing(&self) -> Option<(usize, usize)> {
        self.state
            .enclosing_brackets(&self.buffer.snapshot(), self.offset)
            .map(|pair| (pair.open.start, pair.close.start))
    }
}

fn rust(marked: &str) -> Case {
    Case::new("rust", marked)
}

#[test]
fn a_bracket_on_either_side_of_the_offset_matches() {
    // fn f(a: u8) {}
    assert_eq!(rust("fn f(|a: u8) {}").at(), Some((4, 10)));
    assert_eq!(rust("fn f|(a: u8) {}").at(), Some((4, 10)));
    assert_eq!(rust("fn f(a: u8|) {}").at(), Some((10, 4)));
    assert_eq!(rust("fn f(a: u8)| {}").at(), Some((10, 4)));
    assert_eq!(rust("fn f(a|: u8) {}").at(), None);
}

#[test]
fn the_bracket_before_the_offset_wins() {
    assert_eq!(rust("fn f() {}|").at(), Some((8, 7)));
    assert_eq!(rust("fn f()| {}").at(), Some((5, 4)));
    assert_eq!(rust("fn f(|) {}").at(), Some((4, 5)));
    assert_eq!(rust("fn f() {|}").at(), Some((7, 8)));
}

#[test]
fn nested_brackets_match_their_own_partner() {
    assert_eq!(rust("fn f() { g(|h(1), [2]); }").at(), Some((10, 20)));
    assert_eq!(rust("fn f() { g(h(1)|, [2]); }").at(), Some((14, 12)));
    assert_eq!(rust("fn f() { g(h(1), [2]); |}").at(), Some((23, 7)));
}

#[test]
fn brackets_inside_strings_and_comments_and_quotes_never_match() {
    assert_eq!(rust("fn f() { \"(|\"; }").at(), None);
    assert_eq!(rust("fn f() { // (|\n}").at(), None);
    assert_eq!(rust("fn f() { \"a\"|; }").at(), None);
    assert_eq!(rust("fn f() { |\"a\"; }").at(), None);
}

#[test]
fn an_unclosed_bracket_has_no_partner() {
    assert_eq!(rust("fn f() { g(|1; }").at(), None);
}

#[test]
fn injected_code_matches_in_its_own_language() {
    let case = Case::new("markdown", "# Title\n\n```rust\nfn f(|) {}\n```\n");
    let open = "# Title\n\n```rust\nfn f".len();
    assert_eq!(case.at(), Some((open, open + 1)));
}

#[test]
fn a_document_without_brackets_matches_nothing() {
    assert_eq!(Case::new("markdown", "# (Ti|tle)\n").at(), None);
}

#[test]
fn the_enclosing_pair_is_the_innermost_around_the_offset() {
    // fn f() { g(1, |2); }
    assert_eq!(rust("fn f() { g(1, |2); }").enclosing(), Some((10, 15)));
    assert_eq!(rust("fn f() { g(1, 2);| }").enclosing(), Some((7, 18)));
    assert_eq!(rust("fn f() { g(1, 2); } |").enclosing(), None);
    assert_eq!(rust("fn f() { [1, (2), |3]; }").enclosing(), Some((9, 19)));
}

#[test]
fn the_enclosing_pair_ignores_brackets_in_strings() {
    assert_eq!(rust("fn f() { g(\")\", |1); }").enclosing(), Some((10, 17)));
}
