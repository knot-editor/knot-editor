//! Large generated documents that typing benchmarks run on: Markdown, Rust,
//! TypeScript, and Python of 5,000 repeated units each, and plain text of
//! the same size as the control without syntax.

/// How many units every document repeats.
pub const UNITS: usize = 5_000;

/// One generated document and the offsets to type at.
pub struct LargeDocument {
    /// The language id, or `None` for plain text.
    pub language: Option<&'static str>,
    pub text: String,
    /// Where typing starts, by label, each just after an identifier or a
    /// word so that typed letters extend it.
    pub spots: Vec<(&'static str, usize)>,
}

/// Every benchmark document, plain text last.
pub fn all() -> Vec<LargeDocument> {
    [
        Some("markdown"),
        Some("rust"),
        Some("typescript"),
        Some("python"),
        None,
    ]
    .into_iter()
    .map(document)
    .collect()
}

/// The benchmark document of `language`, or plain text for `None`.
pub fn document(language: Option<&'static str>) -> LargeDocument {
    // The unit generator and, by label, the text each spot follows.
    type Shape = (fn(usize) -> String, &'static [(&'static str, &'static str)]);
    let (unit, markers): Shape = match language {
        Some("markdown") => (markdown, &[("fence", "let y"), ("prose", "Some")]),
        Some("rust") => (rust, &[("body", "let y")]),
        Some("typescript") => (typescript, &[("body", "const y")]),
        Some("python") => (python, &[("body", "    y")]),
        None => (plain, &[("prose", "Line")]),
        Some(other) => panic!("no benchmark document for {other}"),
    };
    let mut text = String::new();
    let mut unit_starts = Vec::with_capacity(UNITS);
    for i in 0..UNITS {
        unit_starts.push(text.len());
        text.push_str(&unit(i));
    }
    let spot = |index: usize, marker: &str| {
        let start = unit_starts[index];
        start + text[start..].find(marker).unwrap() + marker.len()
    };
    let (first_label, first_marker) = markers[0];
    let mut spots = vec![
        ("start", spot(1, first_marker)),
        (first_label, spot(UNITS / 2, first_marker)),
        ("end", spot(UNITS - 2, first_marker)),
    ];
    spots.extend(
        markers[1..]
            .iter()
            .map(|&(label, marker)| (label, spot(UNITS / 2 + 1, marker))),
    );
    LargeDocument {
        language,
        text,
        spots,
    }
}

impl LargeDocument {
    /// The language id, or `plain text`.
    pub fn name(&self) -> &'static str {
        self.language.unwrap_or("plain text")
    }
}

fn markdown(i: usize) -> String {
    format!("## Section {i}\n\nSome prose.\n\n```rust\nfn f{i}() {{\n    let y = 1;\n}}\n```\n\n")
}

fn rust(i: usize) -> String {
    format!(
        "fn f{i}(x: u32) -> u32 {{\n    let y = x + {i};\n    if y > 3 {{\n        return y;\n    }}\n    y\n}}\n"
    )
}

fn typescript(i: usize) -> String {
    format!(
        "export function f{i}(x: number): number {{\n  const y = x + {i};\n  if (y > 3) {{\n    return y;\n  }}\n  return y;\n}}\n\n"
    )
}

fn python(i: usize) -> String {
    format!("def f{i}(x):\n    y = x + {i}\n    if y > 3:\n        return y\n\n    return y\n\n\n")
}

/// Ten lines, as many as a Markdown unit.
fn plain(i: usize) -> String {
    format!(
        "Line one of part {i}.\n\nLine three of plain prose.\nLine four.\n\nLine six of plain prose.\nLine seven.\nLine eight.\n\nLine ten.\n"
    )
}
