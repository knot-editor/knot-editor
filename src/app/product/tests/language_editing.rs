//! The editing data the bundled language packages register, read back from
//! the native registry and run through the comment toggle.
use super::*;
use crate::app::extensions::load::LoadResult;
use crate::app::languages::editing::{EditingConfiguration, SyntaxFamily};

const STRING: SyntaxFamily = SyntaxFamily::String;
const COMMENT: SyntaxFamily = SyntaxFamily::Comment;

/// Comment toggling cases for one language. A round trip toggles the first
/// text into the second and the second back into the first; a one-way case
/// only toggles the first into the second.
struct CommentCases {
    round_trips: &'static [(&'static str, &'static str)],
    one_way: &'static [(&'static str, &'static str)],
}

/// A resolved bracket: open, close, auto-close, newline, and not-in.
type Bracket = (
    &'static str,
    &'static str,
    bool,
    bool,
    &'static [SyntaxFamily],
);

const RUST_COMMENTS: CommentCases = CommentCases {
    round_trips: &[
        ("fn main() {}", "// fn main() {}"),
        (
            "    let a = 1;\n  let b = 2;\n\n      c();",
            "  //   let a = 1;\n  // let b = 2;\n\n  //     c();",
        ),
        ("\tx();\n\t\ty();", "\t// x();\n\t// \ty();"),
    ],
    one_way: &[
        ("/// Docs.", "Docs."),
        ("//! Crate docs.", "Crate docs."),
        ("//x", "x"),
        ("//", ""),
        ("  // a\n  /// b\n\n  //! c", "  a\n  b\n\n  c"),
        ("// a\nb", "// // a\n// b"),
        ("/* a */", "// /* a */"),
        ("\n  \n", "\n  \n"),
    ],
};

const RUST_BRACKETS: &[Bracket] = &[
    ("{", "}", true, true, &[]),
    ("[", "]", true, true, &[]),
    ("(", ")", true, true, &[]),
    ("\"", "\"", true, false, &[STRING, COMMENT]),
];

const ECMASCRIPT_COMMENTS: CommentCases = CommentCases {
    round_trips: &[
        ("const a = 1;", "// const a = 1;"),
        (
            "    if (a) {\n  b();\n        c();",
            "  //   if (a) {\n  // b();\n  //       c();",
        ),
        ("<div>{a}</div>", "// <div>{a}</div>"),
    ],
    one_way: &[
        ("//x", "x"),
        ("  // a\n\n  //b", "  a\n\n  b"),
        ("// a\nb", "// // a\n// b"),
        ("/** Docs. */", "// /** Docs. */"),
    ],
};

const ECMASCRIPT_BRACKETS: &[Bracket] = &[
    ("{", "}", true, true, &[]),
    ("[", "]", true, true, &[]),
    ("(", ")", true, true, &[]),
    ("\"", "\"", true, false, &[STRING, COMMENT]),
    ("'", "'", true, false, &[STRING, COMMENT]),
    ("`", "`", true, false, &[STRING]),
];

const PYTHON_COMMENTS: CommentCases = CommentCases {
    round_trips: &[
        ("x = 1", "# x = 1"),
        (
            "def f():\n    return 1\n\n  # odd",
            "# def f():\n#     return 1\n\n#   # odd",
        ),
    ],
    one_way: &[
        ("#x", "x"),
        ("    # a\n\n    #b", "    a\n\n    b"),
        ("\"\"\"Docs.\"\"\"", "# \"\"\"Docs.\"\"\""),
        ("// a", "# // a"),
    ],
};

const PYTHON_BRACKETS: &[Bracket] = &[
    ("{", "}", true, true, &[]),
    ("[", "]", true, true, &[]),
    ("(", ")", true, true, &[]),
    ("\"", "\"", true, false, &[STRING, COMMENT]),
    ("'", "'", true, false, &[STRING, COMMENT]),
];

const TOML_COMMENTS: CommentCases = CommentCases {
    round_trips: &[
        (
            "[package]\nname = \"knot\"",
            "# [package]\n# name = \"knot\"",
        ),
        ("  a = 1\n    b = 2", "  # a = 1\n  #   b = 2"),
    ],
    one_way: &[
        ("#x", "x"),
        ("  # a\n\n  #b", "  a\n\n  b"),
        ("# a\nb = 1", "# # a\n# b = 1"),
    ],
};

const TOML_BRACKETS: &[Bracket] = &[
    ("{", "}", true, false, &[]),
    ("[", "]", true, true, &[]),
    ("\"", "\"", true, false, &[STRING, COMMENT]),
    ("'", "'", true, false, &[STRING, COMMENT]),
];

const MARKDOWN_COMMENTS: CommentCases = CommentCases {
    round_trips: &[
        ("Some text", "<!-- Some text -->"),
        ("  - item\n- other", "<!--   - item -->\n<!-- - other -->"),
        ("  a\n\n    b", "  <!-- a -->\n\n  <!--   b -->"),
    ],
    one_way: &[
        ("<!--a-->", "a"),
        ("<!-- -->", ""),
        ("  <!-- a -->  ", "  a  "),
        ("<!-- a -->\nb", "<!-- <!-- a --> -->\n<!-- b -->"),
        ("# Title  ", "<!-- # Title -->  "),
    ],
};

const MARKDOWN_BRACKETS: &[Bracket] = &[
    ("{", "}", true, false, &[]),
    ("[", "]", true, false, &[]),
    ("(", ")", true, false, &[]),
    ("<", ">", true, false, &[]),
    ("`", "`", true, false, &[]),
    ("\"", "\"", false, false, &[]),
    ("'", "'", false, false, &[]),
];

/// Start every bundled package with an empty personal config and no
/// installed packages, and require that each one loaded.
fn start_bundled_packages(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    install_globals(cx);
    install_launch_gate(cx);
    let host = cx.update(crate::app::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Some(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let report = cx
        .read(crate::app::extensions::host::startup_report)
        .unwrap();
    let failures: Vec<_> = report
        .report
        .entries
        .iter()
        .filter(|entry| entry.result != LoadResult::Loaded)
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
}

fn configuration(id: &str, cx: &TestAppContext) -> std::sync::Arc<EditingConfiguration> {
    cx.read(|cx| crate::app::languages::editing_configuration(id, cx))
        .unwrap_or_else(|| panic!("{id} is not registered"))
}

/// Toggle comments on every line of `text` and apply the edits.
fn toggle(configuration: &EditingConfiguration, text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    for edit in configuration.toggle_comments(&lines).into_iter().rev() {
        result[edit.line].replace_range(edit.range, &edit.text);
    }
    result.join("\n")
}

fn assert_language(
    id: &str,
    line: &[&str],
    block: Option<(&str, &str)>,
    cases: &CommentCases,
    brackets: &[Bracket],
    auto_close_before: &str,
    cx: &TestAppContext,
) {
    let configuration = configuration(id, cx);
    assert_eq!(configuration.line_comments, line, "{id} line comments");
    assert_eq!(
        configuration
            .block_comment
            .as_ref()
            .map(|(open, close)| (open.as_str(), close.as_str())),
        block,
        "{id} block comment"
    );
    for &(plain, commented) in cases.round_trips {
        assert_eq!(toggle(&configuration, plain), commented, "{id}: {plain:?}");
        assert_eq!(
            toggle(&configuration, commented),
            plain,
            "{id}: {commented:?}"
        );
    }
    for &(input, expected) in cases.one_way {
        assert_eq!(toggle(&configuration, input), expected, "{id}: {input:?}");
    }
    let resolved: Vec<(&str, &str, bool, bool, &[SyntaxFamily])> = configuration
        .brackets
        .iter()
        .map(|bracket| {
            (
                bracket.open.as_str(),
                bracket.close.as_str(),
                bracket.auto_close,
                bracket.newline,
                bracket.not_in.as_slice(),
            )
        })
        .collect();
    assert_eq!(resolved, brackets, "{id} brackets");
    assert_eq!(
        configuration.auto_close_before, auto_close_before,
        "{id} auto-close-before"
    );
}

#[gpui::test]
fn bundled_languages_register_their_editing_data(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    assert_language(
        "rust",
        &["//", "///", "//!"],
        Some(("/*", "*/")),
        &RUST_COMMENTS,
        RUST_BRACKETS,
        ";:.,=}])>",
        cx,
    );
    for id in [
        "javascript",
        "javascriptreact",
        "typescript",
        "typescriptreact",
    ] {
        assert_language(
            id,
            &["//"],
            Some(("/*", "*/")),
            &ECMASCRIPT_COMMENTS,
            ECMASCRIPT_BRACKETS,
            ";:.,=}])>",
            cx,
        );
    }
    assert_language(
        "python",
        &["#"],
        None,
        &PYTHON_COMMENTS,
        PYTHON_BRACKETS,
        ";:.,=}])>",
        cx,
    );
    assert_language(
        "toml",
        &["#"],
        None,
        &TOML_COMMENTS,
        TOML_BRACKETS,
        ",]}",
        cx,
    );
    assert_language(
        "markdown",
        &[],
        Some(("<!--", "-->")),
        &MARKDOWN_COMMENTS,
        MARKDOWN_BRACKETS,
        ";:.,=}])>",
        cx,
    );

    // Markdown's inline language exists only for injection and has no
    // editing data, so toggling it changes nothing.
    let inline = configuration("markdown_inline", cx);
    assert!(inline.line_comments.is_empty() && inline.block_comment.is_none());
    assert!(inline.brackets.is_empty());
    assert_eq!(toggle(&inline, "*a*"), "*a*");
}
