use std::collections::HashMap;

use super::*;
use knot_core::buffer::TextBuffer;

const RUST_HIGHLIGHTS: &str = r#"
(identifier) @variable
((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))
(function_item name: (identifier) @function)
(call_expression function: (identifier) @function.call)
(type_identifier) @type
(primitive_type) @type.builtin
["fn" "let" "if" "else" "struct" "impl"] @keyword
"return" @keyword.return
(line_comment) @comment
(block_comment) @comment
(string_literal) @string
(escape_sequence) @string.escape
(integer_literal) @number
"#;

const MARKDOWN_HIGHLIGHTS: &str = r#"
(atx_heading) @markup.heading
(fenced_code_block) @markup.raw.block
(fenced_code_block_delimiter) @punctuation.delimiter
(info_string) @label
(block_quote_marker) @markup.quote
"#;

const MARKDOWN_INJECTIONS: &str = r#"
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)
((html_block) @injection.content
  (#set! injection.language "html")
  (#set! injection.include-children))
((inline) @injection.content
  (#set! injection.language "markdown_inline"))
"#;

const MARKDOWN_INLINE_HIGHLIGHTS: &str = r#"
(emphasis) @markup.italic
(strong_emphasis) @markup.strong
(code_span) @markup.raw
"#;

const HTML_HIGHLIGHTS: &str = r#"
(tag_name) @tag
(attribute_name) @tag.attribute
"#;

const HTML_INJECTIONS: &str = r#"
((script_element (raw_text) @injection.content)
  (#set! injection.language "javascript"))
((style_element (raw_text) @injection.content)
  (#set! injection.language "css"))
"#;

const JAVASCRIPT_HIGHLIGHTS: &str = r#"
["const" "let" "function" "return"] @keyword
(identifier) @variable
(number) @number
"#;

const CSS_HIGHLIGHTS: &str = r#"
(tag_name) @tag
(property_name) @property
"#;

/// Long enough that every test can finish a foreground parse.
const UNBOUNDED: Duration = Duration::from_secs(60);

fn language(highlights: &str) -> Arc<SyntaxLanguage> {
    Arc::new(SyntaxLanguage::new("rust", highlights, "").unwrap())
}

/// Rust, Markdown, HTML, and the languages they inject, by grammar name and
/// by the `rs` and `js` fence aliases.
fn languages() -> LanguageResolver {
    let compile = |grammar, highlights, injections| {
        Arc::new(SyntaxLanguage::new(grammar, highlights, injections).unwrap())
    };
    let rust = compile("rust", RUST_HIGHLIGHTS, "");
    let javascript = compile("javascript", JAVASCRIPT_HIGHLIGHTS, "");
    let by_name: HashMap<&str, Arc<SyntaxLanguage>> = [
        ("rust", Arc::clone(&rust)),
        ("rs", rust),
        ("javascript", Arc::clone(&javascript)),
        ("js", javascript),
        (
            "markdown",
            compile("markdown", MARKDOWN_HIGHLIGHTS, MARKDOWN_INJECTIONS),
        ),
        (
            "markdown_inline",
            compile("markdown_inline", MARKDOWN_INLINE_HIGHLIGHTS, ""),
        ),
        ("html", compile("html", HTML_HIGHLIGHTS, HTML_INJECTIONS)),
        ("css", compile("css", CSS_HIGHLIGHTS, "")),
        ("typescript", compile("typescript", "", "")),
        ("python", compile("python", "", "")),
    ]
    .into_iter()
    .collect();
    Arc::new(move |name| by_name.get(name).cloned())
}

/// A buffer and its syntax state, edited together the way a model commit
/// feeds the edit log to the state.
struct Fixture {
    buffer: TextBuffer,
    state: SyntaxState,
    /// The language a fresh parse of the same text uses.
    root: &'static str,
}

impl Fixture {
    fn new(text: &str) -> Self {
        Self::of("rust", text)
    }

    /// `text` parsed as the language `languages` names `root`.
    fn of(root: &'static str, text: &str) -> Self {
        let resolver = languages();
        let language = resolver(root).unwrap();
        Self::parsed(text, SyntaxState::new(language, resolver), root)
    }

    fn with_query(text: &str, highlights: &str) -> Self {
        let state = SyntaxState::new(language(highlights), languages());
        Self::parsed(text, state, "rust")
    }

    fn parsed(text: &str, mut state: SyntaxState, root: &'static str) -> Self {
        let buffer = TextBuffer::from_text(text);
        assert!(state.parse_in_foreground(&buffer.snapshot(), UNBOUNDED));
        Self {
            buffer,
            state,
            root,
        }
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        let seq = self.buffer.edit_seq();
        self.buffer.replace(range, text);
        self.state.edit(self.buffer.edits_since(seq));
    }

    fn reparse(&mut self) {
        assert!(
            self.state
                .parse_in_foreground(&self.buffer.snapshot(), UNBOUNDED)
        );
    }

    fn text(&self) -> String {
        self.buffer.read_range(0..self.buffer.len())
    }

    fn highlights(&self) -> Vec<(String, &'static str)> {
        self.highlights_in(0..self.buffer.len())
    }

    fn highlights_in(&self, range: Range<usize>) -> Vec<(String, &'static str)> {
        self.state
            .highlights(&self.buffer.snapshot(), range)
            .into_iter()
            .map(|(range, highlight)| (self.buffer.read_range(range), highlight.name()))
            .collect()
    }

    /// The highlights of a fresh parse of the current text.
    fn fresh(&self) -> Vec<(String, &'static str)> {
        Fixture::of(self.root, &self.text()).highlights()
    }
}

fn named(spans: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    spans
        .iter()
        .map(|(text, name)| (text.to_string(), *name))
        .collect()
}

#[test]
fn language_errors_name_the_grammar_or_query_location() {
    assert_eq!(
        SyntaxLanguage::new("cobol", "", "").err(),
        Some(SyntaxLanguageError::UnknownGrammar)
    );
    let Err(SyntaxLanguageError::InvalidQuery {
        query, row, column, ..
    }) = SyntaxLanguage::new("rust", "(identifier) @variable\n(not_a_node) @x", "")
    else {
        panic!("expected an invalid query");
    };
    assert_eq!((query, row, column), (SyntaxQuery::Highlights, 1, 1));
    let Err(SyntaxLanguageError::InvalidQuery { query, .. }) =
        SyntaxLanguage::new("rust", "", "(not_a_node) @injection.content")
    else {
        panic!("expected an invalid query");
    };
    assert_eq!(query, SyntaxQuery::Injections);
}

#[test]
fn captures_paint_in_cursor_order_by_priority() {
    let fixture = Fixture::with_query(
        "fn main() {}",
        r#"
        ((identifier) @constant (#set! priority 90))
        (function_item) @keyword
        (identifier) @variable
        (function_item name: (identifier) @function)
        "#,
    );
    // The later `@function` pattern beats `@variable` on the same node, the
    // inner identifier beats its enclosing item, and the low-priority
    // `@constant` loses to everything despite coming first.
    assert_eq!(
        fixture.highlights(),
        named(&[
            ("fn ", "keyword"),
            ("main", "function"),
            ("() {}", "keyword")
        ])
    );
}

#[test]
fn an_inner_node_beats_an_outer_one_starting_at_the_same_byte() {
    // The cursor yields an ancestor's capture before its descendant's when
    // both start at the same byte, even when the descendant's pattern comes
    // first.
    let fixture = Fixture::with_query(
        "fn main() {}",
        r#"
        "fn" @keyword
        (function_item) @function
        "#,
    );
    assert_eq!(
        fixture.highlights(),
        named(&[("fn", "keyword"), (" main() {}", "function")])
    );
}

#[test]
fn predicates_and_unknown_captures_filter_highlights() {
    let fixture = Fixture::with_query(
        "let MAX = lower;",
        r#"
        (identifier) @spell
        ((identifier) @constant (#match? @constant "^[A-Z]+$"))
        "#,
    );
    assert_eq!(fixture.highlights(), named(&[("MAX", "constant")]));
}

#[test]
fn highlights_are_limited_and_clipped_to_the_requested_range() {
    let fixture = Fixture::new("let abc = 1;\nlet d = 2;\n");
    assert_eq!(fixture.highlights_in(5..7), named(&[("bc", "variable")]));
    assert_eq!(
        fixture.highlights_in(13..23),
        named(&[("let", "keyword"), ("d", "variable"), ("2", "number")])
    );
}

#[test]
fn an_edited_tree_paints_shifted_highlights_before_reparsing() {
    let mut fixture = Fixture::new("fn a() {}");
    let generation = fixture.state.generation();
    fixture.replace(0..0, "\n\n");
    assert!(fixture.state.generation() > generation);
    assert_eq!(
        fixture
            .state
            .highlights(&fixture.buffer.snapshot(), 0..fixture.buffer.len())[0]
            .0,
        2..4
    );
    fixture.reparse();
    assert_eq!(fixture.highlights(), fixture.fresh());
}

#[test]
fn incremental_highlights_match_a_fresh_parse_after_random_edits() {
    let mut fixture = Fixture::new(
        "struct Point { x: i32 }\n\nfn main() {\n    let p = make(1, \"a\\n\");\n    \
         // done\n    if p { return 2; }\n}\n",
    );
    let fragments = [
        "", "x", " ", "\n", "fn ", "let ", "\"", "\\n", "/*", "*/", "// ", "{", "}", "(", ")", ";",
        "return ", "MAX", "42", "é", "call(1)",
    ];
    let mut rng: u64 = 0x5eed_cafe;
    let mut next = |bound: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % bound as u64) as usize
    };
    for step in 0..25 {
        let text = fixture.text();
        let boundaries: Vec<usize> = (0..=text.len())
            .filter(|offset| text.is_char_boundary(*offset))
            .collect();
        let a = boundaries[next(boundaries.len())];
        let b = boundaries[next(boundaries.len())];
        let range = a.min(b)..a.max(b).min(a.min(b) + 12);
        let range = range.start..range.end.max(range.start);
        let range = if text.is_char_boundary(range.end) {
            range
        } else {
            range.start..range.start
        };
        fixture.replace(range, fragments[next(fragments.len())]);
        // Parse after a run of commits as well as after single ones.
        if next(3) > 0 {
            fixture.reparse();
            assert_eq!(
                fixture.highlights(),
                fixture.fresh(),
                "step {step}: {:?}",
                fixture.text()
            );
        }
    }
}

#[test]
fn a_background_result_is_rebased_over_edits_committed_while_it_ran() {
    let mut fixture = Fixture::new("fn a() {}\n");
    fixture.replace(0..0, "fn b() { let x = 1; }\n");
    let job = fixture
        .state
        .start_background_parse(fixture.buffer.snapshot());
    // The running parse owns the reparse.
    assert!(
        !fixture
            .state
            .parse_in_foreground(&fixture.buffer.snapshot(), UNBOUNDED)
    );
    assert!(!fixture.state.needs_parse());

    fixture.replace(0..0, "// note\n");
    let end = fixture.buffer.len();
    fixture.replace(end..end, "fn c() {}\n");
    let parsed = std::thread::spawn(move || job.run().unwrap())
        .join()
        .unwrap();
    assert!(fixture.state.finish_background_parse(parsed));

    // Rebased onto the current text but not yet a parse of it.
    assert!(fixture.state.needs_parse());
    let shifted = fixture.highlights();
    assert!(shifted.contains(&("b".into(), "function")));
    assert!(shifted.contains(&("a".into(), "function")));
    fixture.reparse();
    assert_eq!(fixture.highlights(), fixture.fresh());
}

#[test]
fn a_background_result_without_later_edits_is_current() {
    let buffer = TextBuffer::from_text("fn a() {}\n");
    let mut state = SyntaxState::new(language(RUST_HIGHLIGHTS), languages());
    let job = state.start_background_parse(buffer.snapshot());
    assert!(
        state
            .highlights(&buffer.snapshot(), 0..buffer.len())
            .is_empty()
    );
    assert!(state.finish_background_parse(job.run().unwrap()));
    assert!(!state.needs_parse());
    assert!(state.parse_in_foreground(&buffer.snapshot(), Duration::ZERO));
    assert!(
        !state
            .highlights(&buffer.snapshot(), 0..buffer.len())
            .is_empty()
    );
}

#[test]
fn a_result_for_another_state_is_rejected() {
    let buffer = TextBuffer::from_text("fn a() {}\n");
    let mut replaced = SyntaxState::new(language(RUST_HIGHLIGHTS), languages());
    let job = replaced.start_background_parse(buffer.snapshot());

    let mut state = SyntaxState::new(language(RUST_HIGHLIGHTS), languages());
    let pending = state.start_background_parse(buffer.snapshot());
    let generation = state.generation();
    assert!(!state.finish_background_parse(job.run().unwrap()));
    assert_eq!(state.generation(), generation);
    assert!(
        state
            .highlights(&buffer.snapshot(), 0..buffer.len())
            .is_empty()
    );

    let parsed = pending.run().unwrap();
    let duplicate = ParsedTree {
        id: parsed.id,
        root: parsed.root.clone(),
    };
    assert!(state.finish_background_parse(parsed));
    assert!(!state.finish_background_parse(duplicate));
}

#[test]
fn dropping_a_state_cancels_its_background_parse() {
    let buffer = TextBuffer::from_text("fn f() { let x = call(1, \"s\"); }\n".repeat(20_000));
    let mut state = SyntaxState::new(language(RUST_HIGHLIGHTS), languages());
    let job = state.start_background_parse(buffer.snapshot());
    drop(state);
    assert!(job.run().is_none());
}

#[test]
fn a_parse_over_budget_is_cancelled_and_can_restart() {
    let source = "fn f() { let x = call(1, \"s\"); }\n".repeat(2_000);
    let buffer = TextBuffer::from_text(source);
    let mut state = SyntaxState::new(language(RUST_HIGHLIGHTS), languages());
    assert!(!state.parse_in_foreground(&buffer.snapshot(), Duration::ZERO));
    assert!(state.needs_parse());
    assert!(state.parse_in_foreground(&buffer.snapshot(), UNBOUNDED));
    assert_eq!(
        state.highlights(&buffer.snapshot(), 0..2),
        vec![(0..2, Highlight::named("keyword"))]
    );
}

fn has(spans: &[(String, &'static str)], text: &str, name: &str) -> bool {
    spans
        .iter()
        .any(|(span, found)| span == text && *found == name)
}

#[test]
fn injected_layers_paint_over_their_host() {
    let fixture = Fixture::of(
        "markdown",
        "# Title\n\nSome *em* text.\n\n```rust\nfn main() {}\n```\n",
    );
    assert_eq!(
        fixture.highlights(),
        named(&[
            ("# Title\n", "markup.heading"),
            ("*em*", "markup.italic"),
            ("```", "punctuation.delimiter"),
            ("rust", "label"),
            ("\n", "markup.raw.block"),
            ("fn", "keyword"),
            (" ", "markup.raw.block"),
            ("main", "function"),
            ("() {}\n", "markup.raw.block"),
            ("```", "punctuation.delimiter"),
            ("\n", "markup.raw.block"),
        ])
    );
    // A query range inside the fence still reaches the injected layer.
    let start = fixture.text().find("fn").unwrap();
    assert_eq!(
        fixture.highlights_in(start..start + 7),
        named(&[
            ("fn", "keyword"),
            (" ", "markup.raw.block"),
            ("main", "function")
        ])
    );
}

#[test]
fn injections_exclude_the_named_children_of_their_content() {
    // The inline emphasis spans a block continuation, which stays out of the
    // inline layer.
    let fixture = Fixture::of("markdown", "> some *em\n> more* text\n");
    assert_eq!(
        fixture.highlights(),
        named(&[
            ("> ", "markup.quote"),
            ("*em\n", "markup.italic"),
            ("more*", "markup.italic"),
        ])
    );
}

#[test]
fn html_injects_javascript_and_css() {
    let fixture = Fixture::of(
        "html",
        "<p a=\"1\">x</p><script>let a = 2;</script><style>p { color: red; }</style>\n",
    );
    let spans = fixture.highlights();
    assert!(has(&spans, "a", "tag.attribute"));
    assert!(has(&spans, "let", "keyword"));
    assert!(has(&spans, "2", "number"));
    assert!(has(&spans, "color", "property"));
}

#[test]
fn injections_nest() {
    // Markdown hands the HTML block to HTML, which hands the script to
    // JavaScript.
    let fixture = Fixture::of(
        "markdown",
        "<div>\n<script>let x = 1;</script>\n</div>\n\nafter *b*\n",
    );
    let spans = fixture.highlights();
    assert!(has(&spans, "div", "tag"));
    assert!(has(&spans, "let", "keyword"));
    assert!(has(&spans, "*b*", "markup.italic"));
}

#[test]
fn an_unresolved_injection_leaves_its_host_highlights() {
    let fixture = Fixture::of("markdown", "```cobol\nfoo\n```\n");
    assert!(fixture.state.root.injections.is_empty());
    assert!(has(&fixture.highlights(), "\nfoo\n", "markup.raw.block"));
}

#[test]
fn injecting_a_region_into_its_own_language_stops_at_the_depth_limit() {
    let language = Arc::new(
        SyntaxLanguage::new(
            "rust",
            "\"fn\" @keyword",
            "((source_file) @injection.content
              (#set! injection.language \"rust\")
              (#set! injection.include-children))",
        )
        .unwrap(),
    );
    let resolver: LanguageResolver = {
        let language = Arc::clone(&language);
        Arc::new(move |_| Some(Arc::clone(&language)))
    };
    let fixture = Fixture::parsed("fn a() {}", SyntaxState::new(language, resolver), "rust");
    let mut depth = 0;
    let mut layer = &fixture.state.root;
    while let [injection] = layer.injections.as_slice() {
        depth += 1;
        layer = injection;
    }
    assert_eq!(depth, MAX_INJECTION_DEPTH);
    assert_eq!(fixture.highlights(), named(&[("fn", "keyword")]));
}

#[test]
fn edits_that_create_and_remove_injections_match_a_fresh_parse() {
    let mut fixture = Fixture::of("markdown", "Intro *text*.\n\nlast line\n");
    let steps: &[(&str, &str)] = &[
        // Open a fence: everything after becomes Rust.
        ("\n\n", "\n\n```rust\nfn a() {}\n"),
        // Close it, so the last line is Markdown again.
        ("fn a() {}\n", "fn a() {}\n```\n"),
        // Change the fence's language.
        ("```rust", "```js"),
        ("fn a() {}", "let a = 1;"),
        // An HTML block with a script, then break the script open.
        ("last line", "<div>\n<script>let b = 2;</script>\n</div>"),
        ("</script>", "</scrip>"),
        ("</scrip>", "</script>"),
        // Remove the fence entirely.
        ("```js\nlet a = 1;\n```\n", ""),
        ("Intro", "> Quoted *a\n> b* intro"),
    ];
    for (find, replace) in steps {
        let text = fixture.text();
        let start = text
            .find(find)
            .unwrap_or_else(|| panic!("{find:?} in {text:?}"));
        fixture.replace(start..start + find.len(), replace);
        fixture.reparse();
        assert_eq!(
            fixture.highlights(),
            fixture.fresh(),
            "{:?}",
            fixture.text()
        );
    }
}

/// Apply random edits from `fragments`, comparing incremental highlights to
/// a fresh parse after most of them.
fn assert_random_edits_match_a_fresh_parse(mut fixture: Fixture, fragments: &[&str], steps: usize) {
    let mut rng: u64 = 0x5eed_cafe;
    let mut next = |bound: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % bound as u64) as usize
    };
    for step in 0..steps {
        let text = fixture.text();
        let boundaries: Vec<usize> = (0..=text.len())
            .filter(|offset| text.is_char_boundary(*offset))
            .collect();
        let a = boundaries[next(boundaries.len())];
        let b = boundaries[next(boundaries.len())];
        let range = a.min(b)..a.max(b).min(a.min(b) + 12);
        let range = if text.is_char_boundary(range.end) {
            range
        } else {
            range.start..range.start
        };
        fixture.replace(range, fragments[next(fragments.len())]);
        // Parse after a run of commits as well as after single ones.
        if next(3) > 0 {
            fixture.reparse();
            assert_eq!(
                fixture.highlights(),
                fixture.fresh(),
                "step {step}: {:?}",
                fixture.text()
            );
        }
    }
}

#[test]
fn random_markdown_edits_match_a_fresh_parse() {
    assert_random_edits_match_a_fresh_parse(
        Fixture::of(
            "markdown",
            "# Title\n\nSome *em* and `code`.\n\n```rust\nfn main() {}\n```\n\n\
             > quoted **strong**\n> more\n\n<div>\n<script>let x = 1;</script>\n</div>\n",
        ),
        &[
            "",
            "x",
            " ",
            "\n",
            "\n\n",
            "*",
            "**",
            "`",
            "```",
            "```rust\n",
            "```js\n",
            "> ",
            "# ",
            "fn a() {}",
            "let b = 2;",
            "<script>",
            "</script>",
            "<div>\n",
            "é",
        ],
        25,
    );
}

#[test]
fn random_html_edits_match_a_fresh_parse() {
    assert_random_edits_match_a_fresh_parse(
        Fixture::of(
            "html",
            "<p a=\"1\">x</p>\n<script>let a = 2;</script>\n<style>p { color: red; }</style>\n",
        ),
        &[
            "",
            "x",
            " ",
            "\n",
            "<",
            ">",
            "</",
            "script",
            "style",
            "<script>",
            "</script>",
            "let c = 3;",
            "{",
            "}",
            ";",
            "color: blue;",
        ],
        25,
    );
}

#[test]
fn a_background_parse_finds_injections_and_is_rebased() {
    let mut fixture = Fixture::of("markdown", "Intro.\n");
    fixture.replace(7..7, "\n```rust\nfn a() {}\n```\n");
    let job = fixture
        .state
        .start_background_parse(fixture.buffer.snapshot());
    // Typing inside the fence while the parse runs.
    let inside = fixture.text().find("a()").unwrap();
    fixture.replace(inside..inside, "b");
    let parsed = std::thread::spawn(move || job.run().unwrap())
        .join()
        .unwrap();
    assert!(fixture.state.finish_background_parse(parsed));
    assert!(has(&fixture.highlights(), "fn", "keyword"));
    fixture.reparse();
    assert_eq!(fixture.highlights(), fixture.fresh());
    assert!(has(&fixture.highlights(), "ba", "function"));
}

/// Parse and query costs that set `FOREGROUND_BUDGET`. Run with
/// `cargo test --release --lib syntax_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn syntax_costs_on_large_rust_files() {
    let unit = "/// Documented.\nfn item(value: &str) -> Result<usize, String> {\n    \
                let parsed = value.parse::<usize>().map_err(|e| e.to_string())?;\n    \
                if parsed > 10 { return Ok(parsed * 2); }\n    Ok(parsed)\n}\n\n";
    let percentile = |samples: &mut Vec<Duration>, p: usize| {
        samples.sort();
        samples[samples.len() * p / 100]
    };
    for units in [500, 10_000] {
        let source = unit.repeat(units);
        let lines = source.lines().count();
        let mut fixture = Fixture {
            buffer: TextBuffer::from_text(source),
            state: SyntaxState::new(language(RUST_HIGHLIGHTS), languages()),
            root: "rust",
        };
        let started = Instant::now();
        fixture.reparse();
        println!(
            "{lines} lines, {} KiB: initial parse {:?}",
            fixture.buffer.len() / 1024,
            started.elapsed()
        );

        // Extend an identifier, which keeps the text valid, then type
        // characters that leave it invalid until the statement completes.
        let middle = fixture.buffer.len() / 2;
        let mut offset = fixture.text()[middle..].find("parsed =").unwrap() + middle + 6;
        for (label, typed) in [("valid", "x"), ("invalid", "(")] {
            let mut typing = Vec::new();
            for _ in 0..300 {
                fixture.replace(offset..offset, typed);
                offset += 1;
                let started = Instant::now();
                fixture.reparse();
                typing.push(started.elapsed());
            }
            println!(
                "  {label} typing reparse p50 {:?} p99 {:?}",
                percentile(&mut typing, 50),
                percentile(&mut typing, 99)
            );
        }

        let mut queries = Vec::new();
        for _ in 0..200 {
            let start = fixture.buffer.line_start(lines / 2);
            let end = fixture.buffer.line_start(lines / 2 + 60);
            let started = Instant::now();
            std::hint::black_box(
                fixture
                    .state
                    .highlights(&fixture.buffer.snapshot(), start..end),
            );
            queries.push(started.elapsed());
        }
        println!(
            "  60-line highlight query p50 {:?} p99 {:?}",
            percentile(&mut queries, 50),
            percentile(&mut queries, 99)
        );
    }
}

/// Reparse costs after an Enter and a typed `}` on large files, which set the
/// indent budget. Run with
/// `cargo test --release -p knot-model indent_reparse_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn indent_reparse_costs_on_large_files() {
    const RUST: &str = "/// Documented.\nfn item(value: &str) -> Result<usize, String> {\n    \
        let parsed = value.parse::<usize>().map_err(|e| e.to_string())?;\n    \
        if parsed > 10 { return Ok(parsed * 2); }\n    Ok(parsed)\n}\n\n";
    const TYPESCRIPT: &str = "export class Item {\n  private values: number[] = [];\n  \
        add(value: number): void {\n    if (value > 10) {\n      this.values.push(value);\n    }\n  }\n  \
        total(): number {\n    return this.values.reduce((a, b) => a + b, 0);\n  }\n}\n\n";
    const PYTHON: &str = "class Item:\n    def add(self, value):\n        if value > 10:\n            \
        self.values.append(value)\n        \n        return value\n\n    \
        def total(self):\n        return sum(self.values)\n\n\n";
    const MARKDOWN: &str = "## Section\n\nA paragraph with *emphasis* and `code`.\n\n```rust\n\
        fn item(value: &str) -> usize {\n    if value.is_empty() { return 0; }\n    value.len()\n}\n```\n\n\
        - a list item\n- another item\n\n";
    // (root, unit, line of the unit to press Enter at the end of, line and text of
    // the closing or branching token typed at the end of its line)
    let cases: [(&str, &str, usize, usize, &str); 4] = [
        ("rust", RUST, 3, 3, "}"),
        ("typescript", TYPESCRIPT, 3, 4, "}"),
        ("python", PYTHON, 2, 4, "else:"),
        ("markdown", MARKDOWN, 6, 7, "}"),
    ];
    for (root, unit, enter_line, brace_line, closing) in cases {
        let unit_lines = unit.lines().count();
        for units in [1_000, 5_000] {
            let source = unit.repeat(units);
            let lines = source.lines().count();
            let resolver = languages();
            let language = resolver(root).unwrap();
            let mut fixture = Fixture::parsed(&source, SyntaxState::new(language, resolver), root);
            for (label, line, typed) in [
                ("Enter", enter_line, "\n    "),
                ("close", brace_line, closing),
            ] {
                let mut samples = Vec::new();
                for sample in 0..200 {
                    let unit_index = (sample * 37 + 11) % units;
                    let next = fixture
                        .buffer
                        .line_start(unit_index * unit_lines + line + 1);
                    let offset = next - 1;
                    fixture.replace(offset..offset, typed);
                    let started = Instant::now();
                    fixture.reparse();
                    samples.push(started.elapsed());
                    fixture.replace(offset..offset + typed.len(), "");
                    fixture.reparse();
                }
                samples.sort();
                println!(
                    "{root:10} {lines:6} lines {label:5} median {:?} p99 {:?} max {:?}",
                    samples[samples.len() / 2],
                    samples[samples.len() * 99 / 100],
                    samples[samples.len() - 1]
                );
            }
        }
    }
}

/// Parse and query costs with many injections. Run with
/// `cargo test --release --lib injection_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn injection_costs_on_a_large_markdown_file() {
    let unit = "## Section\n\nA paragraph with *emphasis*, `code`, and a [link](url).\n\
                Another line of the same paragraph.\n\n```rust\nfn item(value: &str) -> usize {\n    \
                value.len()\n}\n```\n\n- a list item with **strong** text\n- another item\n\n";
    let percentile = |samples: &mut Vec<Duration>, p: usize| {
        samples.sort();
        samples[samples.len() * p / 100]
    };
    for units in [100, 2_000] {
        let source = unit.repeat(units);
        let lines = source.lines().count();
        let resolver = languages();
        let mut fixture = Fixture {
            buffer: TextBuffer::from_text(source),
            state: SyntaxState::new(resolver("markdown").unwrap(), resolver),
            root: "markdown",
        };
        let started = Instant::now();
        fixture.reparse();
        println!(
            "{lines} lines, {} KiB: initial parse {:?}",
            fixture.buffer.len() / 1024,
            started.elapsed()
        );
        let middle = fixture.buffer.len() / 2;
        for (label, anchor) in [
            ("first paragraph", "A paragraph"),
            ("middle paragraph", "A paragraph"),
            ("middle fence", "value.len"),
        ] {
            let from = if label.starts_with("first") {
                0
            } else {
                middle
            };
            let start = fixture.text()[from..].find(anchor).unwrap() + from + 1;
            let mut typing = Vec::new();
            for offset in start..start + 300 {
                fixture.replace(offset..offset, "x");
                let started = Instant::now();
                fixture.reparse();
                typing.push(started.elapsed());
            }
            println!(
                "  typing in the {label}: p50 {:?} p99 {:?}",
                percentile(&mut typing, 50),
                percentile(&mut typing, 99)
            );
        }
        let mut queries = Vec::new();
        for _ in 0..200 {
            let start = fixture.buffer.line_start(lines / 2);
            let end = fixture.buffer.line_start(lines / 2 + 60);
            let started = Instant::now();
            std::hint::black_box(
                fixture
                    .state
                    .highlights(&fixture.buffer.snapshot(), start..end),
            );
            queries.push(started.elapsed());
        }
        println!(
            "  60-line highlight query p50 {:?} p99 {:?}",
            percentile(&mut queries, 50),
            percentile(&mut queries, 99)
        );
    }
}

/// The layers of `layer` and every layer injected into it.
fn layer_count(layer: &Layer) -> usize {
    1 + layer.injections.iter().map(layer_count).sum::<usize>()
}

/// The foreground stages of one typed character on the large Markdown
/// benchmark document, by spot: the whole commit with and without syntax,
/// and, apart, the syntax edit, the layer clone, the foreground attempt that
/// follows a commit, starting a background parse, the background parse
/// itself, the rebase of its result over one later key, and a highlight
/// query over 60 lines. Run with
/// `cargo test --release -p knot-model typing_stage_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn typing_stage_costs_on_the_large_markdown_document() {
    use crate::buffer::BufferModel;
    use crate::large_documents::document;

    const KEYS: usize = 100;
    fn report(label: &str, samples: &mut [Duration]) {
        samples.sort();
        println!(
            "  {label:30} p50 {:>10.3?} p99 {:>10.3?} max {:>10.3?}",
            samples[samples.len() / 2],
            samples[samples.len() * 99 / 100],
            samples[samples.len() - 1]
        );
    }
    fn timed<T>(samples: &mut Vec<Duration>, run: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = run();
        samples.push(started.elapsed());
        result
    }

    let markdown = document(Some("markdown"));
    let plain = document(None);
    let resolver = languages();
    let mut fixture = Fixture::of("markdown", &markdown.text);
    let mut model = BufferModel::from_text(markdown.text.as_str());
    model.set_syntax(Some(SyntaxState::new(
        resolver("markdown").unwrap(),
        Arc::clone(&resolver),
    )));
    let parse = model.start_syntax_parse().unwrap();
    assert!(model.finish_syntax_parse(parse.run().unwrap()));
    let mut plain_model = BufferModel::from_text(plain.text.as_str());
    println!(
        "markdown {} lines, {} layers; plain text {} lines",
        markdown.text.lines().count(),
        layer_count(&fixture.state.root),
        plain.text.lines().count()
    );

    // Plain text types at its middle spot alongside Markdown prose.
    let plain_spots = plain.spots.iter().chain([&plain.spots[1]]);
    for (&(label, spot), &(_, plain_spot)) in markdown.spots.iter().zip(plain_spots) {
        println!(" typing at {label}");
        let mut commit = Vec::new();
        let mut plain_commit = Vec::new();
        for key in 0..KEYS {
            let offset = spot + key;
            timed(&mut commit, || model.replace(offset..offset, "x").unwrap());
            let parse = model.start_syntax_parse().unwrap();
            assert!(model.finish_syntax_parse(parse.run().unwrap()));
            let offset = plain_spot + key;
            timed(&mut plain_commit, || {
                plain_model.replace(offset..offset, "x").unwrap()
            });
        }
        report("commit, markdown", &mut commit);
        report("commit, plain text", &mut plain_commit);

        let [
            mut edit,
            mut clone,
            mut attempt,
            mut start,
            mut background,
            mut finish,
            mut query,
        ] = std::array::from_fn(|_| Vec::new());
        for key in 0..KEYS {
            let offset = spot + 2 * key;
            let seq = fixture.buffer.edit_seq();
            fixture.buffer.replace(offset..offset, "x");
            let edits = fixture.buffer.edits_since(seq).to_vec();
            timed(&mut edit, || fixture.state.edit(&edits));
            timed(&mut clone, || {
                std::hint::black_box(fixture.state.root.clone())
            });
            let snapshot = fixture.buffer.snapshot();
            timed(&mut attempt, || {
                fixture
                    .state
                    .parse_in_foreground(&snapshot, FOREGROUND_BUDGET)
            });
            let parse = timed(&mut start, || {
                fixture.state.start_background_parse(snapshot)
            });
            let parsed = timed(&mut background, || parse.run().unwrap());
            // The next key lands while the parse runs, so its result is
            // rebased over that key and the foreground tries again.
            fixture.replace(offset + 1..offset + 1, "x");
            timed(&mut finish, || {
                fixture.state.finish_background_parse(parsed);
                fixture
                    .state
                    .parse_in_foreground(&fixture.buffer.snapshot(), FOREGROUND_BUDGET)
            });
            fixture.reparse();
            let line = fixture.buffer.line_of_offset(offset);
            let range = fixture.buffer.line_start(line)..fixture.buffer.line_start(line + 60);
            timed(&mut query, || {
                std::hint::black_box(fixture.state.highlights(&fixture.buffer.snapshot(), range))
            });
        }
        report("syntax edit", &mut edit);
        report("layer clone", &mut clone);
        report("foreground attempt", &mut attempt);
        report("start background parse", &mut start);
        report("background parse (off thread)", &mut background);
        report("rebase + foreground attempt", &mut finish);
        report("60-line highlight query", &mut query);
    }
}

/// The syntax family at the `|` in `marked`, parsed as `root`.
fn family(root: &'static str, marked: &str) -> Option<SyntaxFamily> {
    let offset = marked.find('|').unwrap();
    let fixture = Fixture::of(root, &marked.replacen('|', "", 1));
    fixture.state.family_at(&fixture.buffer.snapshot(), offset)
}

#[test]
fn a_string_contains_only_offsets_strictly_inside_it() {
    assert_eq!(
        family("rust", "let s = \"a|b\";"),
        Some(SyntaxFamily::String)
    );
    assert_eq!(
        family("rust", "let s = \"|ab\";"),
        Some(SyntaxFamily::String)
    );
    assert_eq!(family("rust", "let s = |\"ab\";"), None);
    assert_eq!(family("rust", "let s = \"ab\"|;"), None);
    assert_eq!(family("rust", "|let s;"), None);
}

#[test]
fn a_line_comment_contains_its_end() {
    assert_eq!(
        family("rust", "let s; // a|b\n"),
        Some(SyntaxFamily::Comment)
    );
    assert_eq!(
        family("rust", "let s; // ab|\nlet t;"),
        Some(SyntaxFamily::Comment)
    );
    assert_eq!(family("rust", "let s; // ab|"), Some(SyntaxFamily::Comment));
    assert_eq!(family("rust", "let s; |// ab"), None);
}

#[test]
fn a_block_comment_ending_in_its_closer_does_not_contain_its_end() {
    let rust = Arc::new(
        SyntaxLanguage::new("rust", RUST_HIGHLIGHTS, "")
            .unwrap()
            .with_editing(EditingConfiguration {
                block_comment: Some(("/*".into(), "*/".into())),
                ..Default::default()
            }),
    );
    let at = |marked: &str| {
        let offset = marked.find('|').unwrap();
        let fixture = Fixture::parsed(
            &marked.replacen('|', "", 1),
            SyntaxState::new(Arc::clone(&rust), languages()),
            "rust",
        );
        fixture.state.family_at(&fixture.buffer.snapshot(), offset)
    };
    assert_eq!(at("let s; /* a| */"), Some(SyntaxFamily::Comment));
    assert_eq!(at("let s; /* a */|"), None);
    assert_eq!(at("let s; /* é */|"), None);
}

#[test]
fn injected_code_reports_its_own_strings() {
    assert_eq!(
        family("markdown", "# T\n\n```rust\nlet s = \"a|b\";\n```\n"),
        Some(SyntaxFamily::String)
    );
    assert_eq!(family("markdown", "# \"a|b\"\n"), None);
}
