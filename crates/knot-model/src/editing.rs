//! Language editing configuration: the comment tokens and bracket pairs a
//! language registers for native editing commands, and the comment toggle
//! those commands call. It reads lines of text, without terminators, and
//! returns byte ranges, so it needs no buffer or view.

use std::ops::Range;

use serde::{Deserialize, Serialize};

/// Editing data as a language registration carries it and `languages.get`
/// returns it. Absent fields stay absent so a derived language spreads them
/// unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditingDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<CommentTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brackets: Option<Vec<BracketPair>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_close_before: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentTokens {
    /// Every line comment token; the first is the one commenting inserts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<(String, String)>,
}

/// A bracket pair as registered; compiling resolves its defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BracketPair {
    pub open: String,
    pub close: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_close: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_in: Option<Vec<SyntaxFamily>>,
}

/// A bracket pair with its defaults resolved. A pair whose open equals its
/// close, such as quotes, never takes part in matching.
#[derive(Clone, Debug, PartialEq)]
pub struct Bracket {
    pub open: String,
    pub close: String,
    /// Whether typing open inserts close; true unless registered otherwise.
    pub auto_close: bool,
    /// Whether Enter between the pair opens an indented middle line; by
    /// default, unless open equals close.
    pub newline: bool,
    /// Syntax at the cursor where auto-closing is suppressed.
    pub not_in: Vec<SyntaxFamily>,
}

impl BracketPair {
    fn resolve(&self) -> Bracket {
        Bracket {
            open: self.open.clone(),
            close: self.close.clone(),
            auto_close: self.auto_close.unwrap_or(true),
            newline: self.newline.unwrap_or(self.open != self.close),
            not_in: self.not_in.clone().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyntaxFamily {
    String,
    Comment,
}

/// A language's editing data with its defaults resolved, as native editing
/// code reads it.
#[derive(Clone, Debug, Default)]
pub struct EditingConfiguration {
    pub line_comments: Vec<String>,
    pub block_comment: Option<(String, String)>,
    pub brackets: Vec<Bracket>,
    pub auto_close_before: String,
}

impl EditingDefinition {
    pub fn resolve(&self) -> EditingConfiguration {
        let comments = self.comments.clone().unwrap_or_default();
        EditingConfiguration {
            line_comments: comments.line,
            block_comment: comments.block,
            brackets: self
                .brackets
                .iter()
                .flatten()
                .map(BracketPair::resolve)
                .collect(),
            auto_close_before: self.auto_close_before.clone().unwrap_or_default(),
        }
    }
}

/// Replace the byte `range` of line `line` with `text`. A line's edits are
/// ordered and never overlap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentEdit {
    pub line: usize,
    pub range: Range<usize>,
    pub text: String,
}

impl EditingConfiguration {
    /// The edits that toggle comments on `lines`; none when every line is
    /// blank or the language has no comment tokens. Blank lines are left
    /// alone.
    ///
    /// When every non-blank line starts with a line comment token, each loses
    /// the longest token it starts with and one following space. Otherwise
    /// every non-blank line gets the first token and a space at the smallest
    /// indentation among them. A language with only a block comment wraps
    /// each line instead, and unwraps when every line is wrapped.
    pub fn toggle_comments(&self, lines: &[&str]) -> Vec<CommentEdit> {
        let filled: Vec<(usize, &str)> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(row, line)| (row, *line))
            .collect();
        let Some(column) = filled.iter().map(|(_, line)| indent(line)).min() else {
            return Vec::new();
        };
        if let Some(token) = self.line_comments.first() {
            let uncommented: Option<Vec<_>> = filled
                .iter()
                .map(|&(row, line)| {
                    let start = indent(line);
                    let token = self
                        .line_comments
                        .iter()
                        .filter(|token| line[start..].starts_with(token.as_str()))
                        .max_by_key(|token| token.len())?;
                    let mut end = start + token.len();
                    if line[end..].starts_with(' ') {
                        end += 1;
                    }
                    Some(CommentEdit {
                        line: row,
                        range: start..end,
                        text: String::new(),
                    })
                })
                .collect();
            return uncommented.unwrap_or_else(|| {
                filled
                    .iter()
                    .map(|&(row, _)| CommentEdit {
                        line: row,
                        range: column..column,
                        text: format!("{token} "),
                    })
                    .collect()
            });
        }
        let Some((open, close)) = &self.block_comment else {
            return Vec::new();
        };
        let unwrapped: Option<Vec<_>> = filled
            .iter()
            .map(|&(row, line)| {
                let (open, close) = unwrap_block(line, open, close)?;
                Some([
                    CommentEdit {
                        line: row,
                        range: open,
                        text: String::new(),
                    },
                    CommentEdit {
                        line: row,
                        range: close,
                        text: String::new(),
                    },
                ])
            })
            .collect();
        match unwrapped {
            Some(edits) => edits.into_iter().flatten().collect(),
            None => filled
                .iter()
                .flat_map(|&(row, line)| {
                    let end = line.trim_end().len();
                    [
                        CommentEdit {
                            line: row,
                            range: column..column,
                            text: format!("{open} "),
                        },
                        CommentEdit {
                            line: row,
                            range: end..end,
                            text: format!(" {close}"),
                        },
                    ]
                })
                .collect(),
        }
    }

    /// The edits, relative to `text`, that toggle a block comment around it,
    /// ignoring surrounding whitespace: unwrap it when it starts and ends with
    /// the delimiters, else wrap it. Whitespace alone becomes an empty
    /// comment with two spaces inside. None when the language has no block
    /// comment.
    pub fn toggle_block_comment(&self, text: &str) -> Vec<(Range<usize>, String)> {
        let Some((open, close)) = &self.block_comment else {
            return Vec::new();
        };
        let start = indent(text);
        let end = text.trim_end().len();
        if start >= end {
            return vec![(0..0, format!("{open}  {close}"))];
        }
        match unwrap_block(text, open, close) {
            Some((open, close)) => vec![(open, String::new()), (close, String::new())],
            None => vec![
                (start..start, format!("{open} ")),
                (end..end, format!(" {close}")),
            ],
        }
    }
}

impl EditingConfiguration {
    /// The pair whose closer to insert after the caret when `typed` is typed with
    /// `before` and `after` the caret on its line, at a caret inside
    /// `family`. A pair applies when it auto-closes, the text before the
    /// caret and `typed` end with its opener, the character after the caret
    /// is whitespace, the end of the line, or in `autoCloseBefore`, `family`
    /// is not in its `notIn`, and, for a pair whose opener is its closer,
    /// no word character precedes the opener. The longest opener wins.
    pub fn auto_close(
        &self,
        before: &str,
        typed: &str,
        after: &str,
        family: Option<SyntaxFamily>,
    ) -> Option<&Bracket> {
        if typed.is_empty()
            || !after
                .chars()
                .next()
                .is_none_or(|next| next.is_whitespace() || self.auto_close_before.contains(next))
        {
            return None;
        }
        let text = format!("{before}{typed}");
        self.brackets
            .iter()
            .filter(|bracket| {
                bracket.auto_close
                    && text.ends_with(&bracket.open)
                    && (bracket.open.ends_with(typed) || typed.ends_with(&bracket.open))
                    && family.is_none_or(|family| !bracket.not_in.contains(&family))
                    && (bracket.open != bracket.close
                        || !text[..text.len() - bracket.open.len()]
                            .chars()
                            .next_back()
                            .is_some_and(is_word_character))
            })
            .max_by_key(|bracket| bracket.open.len())
    }

    /// The pair whose opener is `typed`, which wraps a selection when typed
    /// over it, whether or not the pair auto-closes.
    pub fn surround(&self, typed: &str) -> Option<&Bracket> {
        self.brackets.iter().find(|bracket| bracket.open == typed)
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The ranges of `open` and `close`, each with one space inside, when
/// `text` without surrounding whitespace starts and ends with them.
fn unwrap_block(text: &str, open: &str, close: &str) -> Option<(Range<usize>, Range<usize>)> {
    let start = indent(text);
    let end = text.trim_end().len();
    let body = &text[start..end];
    if body.len() < open.len() + close.len() || !body.starts_with(open) || !body.ends_with(close) {
        return None;
    }
    let mut inner = start + open.len();
    let mut outer = end - close.len();
    if inner < outer && text[inner..].starts_with(' ') {
        inner += 1;
    }
    if inner < outer && text[..outer].ends_with(' ') {
        outer -= 1;
    }
    Some((start..inner, outer..end))
}

/// The byte width of a line's leading whitespace.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
mod tests;
