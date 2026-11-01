//! Indentation: whether a document indents with tabs or spaces, and the edits
//! that indent, outdent, and delete by one indentation unit. Lines are text
//! without terminators and columns are bytes, so this needs no buffer or view.

use std::ops::Range;

/// How a document's text indents its lines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IndentStyle {
    Tabs,
    Spaces,
}

/// How many lines from the start of a document detection reads.
pub(crate) const DETECTION_LINES: usize = 10_000;

/// The style that indents at least twice as many of `lines` as the other.
///
/// A line counts when its leading whitespace starts with a tab, or with two
/// or more spaces; one space is alignment rather than indentation. Blank lines
/// and block-comment continuations such as ` * text` do not count.
pub(crate) fn detect<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<IndentStyle> {
    let (mut tabs, mut spaces) = (0usize, 0usize);
    for line in lines {
        let content = line.trim_start_matches([' ', '\t']);
        if content.is_empty() || content.starts_with('*') {
            continue;
        }
        if line.starts_with('\t') {
            tabs += 1;
        } else if line.starts_with("  ") {
            spaces += 1;
        }
    }
    if tabs > 0 && tabs >= spaces * 2 {
        Some(IndentStyle::Tabs)
    } else if spaces > 0 && spaces >= tabs * 2 {
        Some(IndentStyle::Spaces)
    } else {
        None
    }
}

/// The leading spaces and tabs of `line`.
pub(crate) fn leading_whitespace(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// One level of indentation: a tab, or `size` columns of spaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IndentUnit {
    pub(crate) tabs: bool,
    /// Columns per tab stop.
    pub(crate) size: usize,
}

impl IndentUnit {
    /// The columns `text` occupies from the start of a line, with tabs
    /// advancing to the next tab stop.
    fn width(&self, text: &str) -> usize {
        text.chars().fold(0, |width, character| {
            if character == '\t' {
                width + self.size - width % self.size
            } else {
                width + 1
            }
        })
    }

    /// What Tab inserts after `before`, the line's text before the caret: a
    /// tab, or spaces to the next tab stop.
    pub(crate) fn insertion(&self, before: &str) -> String {
        if self.tabs {
            "\t".into()
        } else {
            " ".repeat(self.size - self.width(before) % self.size)
        }
    }

    /// The column and text that indent `line` by one level. Spaces go after
    /// the existing indentation and round it up to the next tab stop.
    pub(crate) fn indent(&self, line: &str) -> (usize, String) {
        if self.tabs {
            return (0, "\t".into());
        }
        let leading = leading_whitespace(line);
        (leading.len(), self.insertion(leading))
    }

    /// The range that outdents `line` by one level: a leading tab, or leading
    /// spaces back to the previous tab stop. Empty when `line` is not
    /// indented. Either character outdents in either style.
    pub(crate) fn outdent(&self, line: &str) -> Range<usize> {
        if line.starts_with('\t') {
            return 0..1;
        }
        0..self.spaces_to_previous_stop(line.len() - line.trim_start_matches(' ').len())
    }

    /// How many bytes Backspace deletes after `before`, the line's text before
    /// the caret, when that text is only spaces: back to the previous tab stop.
    pub(crate) fn backspace(&self, before: &str) -> Option<usize> {
        (!before.is_empty() && before.bytes().all(|byte| byte == b' '))
            .then(|| self.spaces_to_previous_stop(before.len()))
    }

    fn spaces_to_previous_stop(&self, spaces: usize) -> usize {
        match spaces % self.size {
            0 => spaces.min(self.size),
            partial => partial,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_dominant_style_and_ignores_alignment_and_comments() {
        let tabs = "fn main() {\n\tlet a = 1;\n\tif a {\n\t\tb();\n\t}\n}";
        assert_eq!(detect(tabs.lines()), Some(IndentStyle::Tabs));
        let spaces = "fn main() {\n    let a = 1;\n    if a {\n        b();\n    }\n}";
        assert_eq!(detect(spaces.lines()), Some(IndentStyle::Spaces));

        let mixed = "a\n\tb\n\tc\n    d\n    e";
        assert_eq!(detect(mixed.lines()), None);
        let mostly_tabs = "a\n\tb\n\tc\n\td\n    e";
        assert_eq!(detect(mostly_tabs.lines()), Some(IndentStyle::Tabs));

        assert_eq!(detect("a\nb\n\n   \n\t\nc".lines()), None);
        assert_eq!(detect("".lines()), None);

        let doc_comments = "/**\n * One.\n * Two.\n */\nfn a() {\n\tb();\n}";
        assert_eq!(detect(doc_comments.lines()), Some(IndentStyle::Tabs));
        let aligned = "call(a,\n b,\n c);\n\td();";
        assert_eq!(detect(aligned.lines()), Some(IndentStyle::Tabs));
    }

    #[test]
    fn indents_outdents_and_deletes_by_tab_stops() {
        let spaces = IndentUnit {
            tabs: false,
            size: 4,
        };
        let tabs = IndentUnit {
            tabs: true,
            size: 4,
        };

        assert_eq!(spaces.insertion(""), "    ");
        assert_eq!(spaces.insertion("ab"), "  ");
        assert_eq!(spaces.insertion("\tx"), "   ");
        assert_eq!(tabs.insertion("ab"), "\t");

        assert_eq!(spaces.indent("foo"), (0, "    ".into()));
        assert_eq!(spaces.indent("  foo"), (2, "  ".into()));
        assert_eq!(spaces.indent("\tfoo"), (1, "    ".into()));
        assert_eq!(tabs.indent("    foo"), (0, "\t".into()));

        for unit in [spaces, tabs] {
            assert_eq!(unit.outdent("\t\tfoo"), 0..1);
            assert_eq!(unit.outdent("        foo"), 0..4);
            assert_eq!(unit.outdent("      foo"), 0..2);
            assert_eq!(unit.outdent("  foo"), 0..2);
            assert_eq!(unit.outdent("foo"), 0..0);
        }

        assert_eq!(spaces.backspace("        "), Some(4));
        assert_eq!(spaces.backspace("      "), Some(2));
        assert_eq!(spaces.backspace(" "), Some(1));
        assert_eq!(spaces.backspace("  a "), None);
        assert_eq!(spaces.backspace("\t"), None);
        assert_eq!(spaces.backspace(""), None);
    }
}
