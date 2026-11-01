//! Language editing configuration: the comment tokens, bracket pairs, and
//! indentation patterns a language registers for native editing commands,
//! and the evaluators those commands call.
//!
//! The evaluators read lines of text, without terminators, and return
//! indentation levels and byte columns, so they need no buffer or view.
//! Brackets and patterns see a line's code: string contents and comments are
//! removed first, so `let s = "{";` and `# note:` do not indent.

use std::ops::Range;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Editing data as a language registration carries it and `languages.get`
/// returns it. Absent fields stay absent so a derived language spreads them
/// unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditingDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) comments: Option<CommentTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) brackets: Option<Vec<BracketPair>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_close_before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) indentation: Option<IndentationPatterns>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommentTokens {
    /// Every line comment token; the first is the one commenting inserts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) line: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) block: Option<(String, String)>,
}

/// A bracket pair as registered; compiling resolves its defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BracketPair {
    pub(crate) open: String,
    pub(crate) close: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_close: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) newline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) not_in: Option<Vec<SyntaxFamily>>,
}

/// A bracket pair with its defaults resolved. A pair whose open equals its
/// close, such as quotes, never takes part in matching or indentation.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Bracket {
    pub(crate) open: String,
    pub(crate) close: String,
    /// Whether typing open inserts close; true unless registered otherwise.
    pub(crate) auto_close: bool,
    /// Whether Enter between the pair opens an indented middle line; by
    /// default, unless open equals close.
    pub(crate) newline: bool,
    /// Syntax at the cursor where auto-closing is suppressed.
    pub(crate) not_in: Vec<SyntaxFamily>,
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
pub(crate) enum SyntaxFamily {
    String,
    Comment,
}

/// Indentation patterns in Rust `regex` syntax.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IndentationPatterns {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) increase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) decrease: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IndentationPattern {
    Increase,
    Decrease,
}

impl IndentationPattern {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Decrease => "decrease",
        }
    }
}

/// An indentation pattern the `regex` crate rejects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InvalidPattern {
    pub(crate) pattern: IndentationPattern,
    pub(crate) message: String,
}

/// A language's editing data with its patterns compiled, as native editing
/// code reads it.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, Default)]
pub(crate) struct EditingConfiguration {
    pub(crate) line_comments: Vec<String>,
    pub(crate) block_comment: Option<(String, String)>,
    pub(crate) brackets: Vec<Bracket>,
    pub(crate) auto_close_before: String,
    pub(crate) increase_indent: Option<Regex>,
    pub(crate) decrease_indent: Option<Regex>,
}

impl EditingDefinition {
    pub(crate) fn compile(&self) -> Result<EditingConfiguration, InvalidPattern> {
        let indentation = self.indentation.clone().unwrap_or_default();
        let compile = |source: Option<String>, pattern| {
            source
                .map(|source| {
                    Regex::new(&source).map_err(|error| InvalidPattern {
                        pattern,
                        message: error.to_string(),
                    })
                })
                .transpose()
        };
        let comments = self.comments.clone().unwrap_or_default();
        Ok(EditingConfiguration {
            line_comments: comments.line,
            block_comment: comments.block,
            brackets: self
                .brackets
                .iter()
                .flatten()
                .map(BracketPair::resolve)
                .collect(),
            auto_close_before: self.auto_close_before.clone().unwrap_or_default(),
            increase_indent: compile(indentation.increase, IndentationPattern::Increase)?,
            decrease_indent: compile(indentation.decrease, IndentationPattern::Decrease)?,
        })
    }
}

/// How many lines above a closer the search for its opener reads.
const OPENER_SEARCH_LINES: usize = 500;

/// The indentation a line should have: `levels` levels deeper than the
/// indentation of line `reference`, or than column zero without one. Levels
/// may be negative; whitespace is the caller's, from `editor.tabSize`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Indentation {
    pub(crate) reference: Option<usize>,
    pub(crate) levels: i32,
}

/// Replace the byte `range` of line `line` with `text`. A line's edits are
/// ordered and never overlap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommentEdit {
    pub(crate) line: usize,
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Token {
    Open,
    Close,
}

#[cfg_attr(not(test), allow(dead_code))]
impl EditingConfiguration {
    /// The indentation of a new empty line inserted after line `row`.
    ///
    /// The reference is the nearest non-blank line at or above `row`. It
    /// indents the new line one level when its code ends with an opener or
    /// matches `increase_indent`. When it closes a bracket opened on an
    /// earlier line, as the last line of a multi-line call does, the line
    /// holding that opener becomes the reference instead.
    pub(crate) fn indentation_after(&self, lines: &[&str], row: usize) -> Indentation {
        let Some(above) = (0..=row).rev().find(|&row| !lines[row].trim().is_empty()) else {
            return Indentation {
                reference: None,
                levels: 0,
            };
        };
        let code = self.code(lines[above]);
        let opens = self
            .indenting_pairs()
            .any(|pair| code.trim_end().ends_with(pair.open.as_str()))
            || self
                .increase_indent
                .as_ref()
                .is_some_and(|pattern| pattern.is_match(&code));
        let reference = self
            .indenting_pairs()
            .filter_map(|pair| {
                let closer = last_unmatched_closer(&code, pair)?;
                self.find_opener(lines, above, closer, pair)
            })
            .min()
            .unwrap_or(above);
        Indentation {
            reference: Some(reference),
            levels: i32::from(opens),
        }
    }

    /// The indentation of line `row` given its own text and the lines above.
    ///
    /// A line whose code starts with a closer aligns with the line holding
    /// its opener, or outdents one level when the opener is not found. A line
    /// matching `decrease_indent` outdents one level. Any other line indents
    /// as a new line after the line above it would.
    pub(crate) fn indentation(&self, lines: &[&str], row: usize) -> Indentation {
        let base = match row.checked_sub(1) {
            Some(above) => self.indentation_after(lines, above),
            None => Indentation {
                reference: None,
                levels: 0,
            },
        };
        let outdented = Indentation {
            levels: base.levels - 1,
            ..base
        };
        let code = self.code(lines[row]);
        let text = code.trim_start();
        let start = code.len() - text.len();
        if let Some(pair) = self
            .indenting_pairs()
            .find(|pair| text.starts_with(pair.close.as_str()))
        {
            return match self.find_opener(lines, row, start, pair) {
                Some(opener) => Indentation {
                    reference: Some(opener),
                    levels: 0,
                },
                None => outdented,
            };
        }
        if self
            .decrease_indent
            .as_ref()
            .is_some_and(|pattern| pattern.is_match(&code))
        {
            return outdented;
        }
        base
    }

    /// The edits that toggle comments on `lines`; none when every line is
    /// blank or the language has no comment tokens. Blank lines are left
    /// alone.
    ///
    /// When every non-blank line starts with a line comment token, each loses
    /// the longest token it starts with and one following space. Otherwise
    /// every non-blank line gets the first token and a space at the smallest
    /// indentation among them. A language with only a block comment wraps
    /// each line instead, and unwraps when every line is wrapped.
    pub(crate) fn toggle_comments(&self, lines: &[&str]) -> Vec<CommentEdit> {
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
                let start = indent(line);
                let end = line.trim_end().len();
                let body = &line[start..end];
                if body.len() < open.len() + close.len()
                    || !body.starts_with(open.as_str())
                    || !body.ends_with(close.as_str())
                {
                    return None;
                }
                let mut inner = start + open.len();
                let mut outer = end - close.len();
                if inner < outer && line[inner..].starts_with(' ') {
                    inner += 1;
                }
                if inner < outer && line[..outer].ends_with(' ') {
                    outer -= 1;
                }
                Some([
                    CommentEdit {
                        line: row,
                        range: start..inner,
                        text: String::new(),
                    },
                    CommentEdit {
                        line: row,
                        range: outer..end,
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

    /// The pairs that delimit indented blocks: those Enter opens a middle
    /// line between, which excludes quotes.
    fn indenting_pairs(&self) -> impl Iterator<Item = &Bracket> {
        self.brackets
            .iter()
            .filter(|pair| pair.newline && pair.open != pair.close)
    }

    /// The line holding the opener of a `pair` closer at byte `end` of line
    /// `row`'s code, searching back from the closer.
    fn find_opener(&self, lines: &[&str], row: usize, end: usize, pair: &Bracket) -> Option<usize> {
        let mut depth = 0usize;
        for candidate in (row.saturating_sub(OPENER_SEARCH_LINES)..=row).rev() {
            let mut code = self.code(lines[candidate]);
            if candidate == row {
                code.truncate(end);
            }
            for (_, token) in tokens(&code, pair).into_iter().rev() {
                match token {
                    Token::Close => depth += 1,
                    Token::Open if depth == 0 => return Some(candidate),
                    Token::Open => depth -= 1,
                }
            }
        }
        None
    }

    /// A line's code: string contents and comments removed, keeping the
    /// quotes. A quote unclosed on its line counts as code, so a Rust
    /// lifetime or an apostrophe does not hide the rest of the line.
    fn code(&self, line: &str) -> String {
        let mut code = String::new();
        let mut rest = line;
        while let Some(character) = rest.chars().next() {
            if self
                .line_comments
                .iter()
                .any(|token| rest.starts_with(token.as_str()))
            {
                break;
            }
            if let Some((open, close)) = &self.block_comment
                && rest.starts_with(open.as_str())
            {
                match rest[open.len()..].find(close.as_str()) {
                    Some(end) => {
                        rest = &rest[open.len() + end + close.len()..];
                        continue;
                    }
                    None => break,
                }
            }
            if let Some(quote) = self
                .brackets
                .iter()
                .find(|pair| pair.open == pair.close && rest.starts_with(pair.open.as_str()))
                && let Some(end) = closing_quote(&rest[quote.open.len()..], &quote.close)
            {
                code.push_str(&quote.open);
                code.push_str(&quote.close);
                rest = &rest[quote.open.len() + end + quote.close.len()..];
                continue;
            }
            code.push(character);
            rest = &rest[character.len_utf8()..];
        }
        code
    }
}

/// The byte width of a line's leading whitespace.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Where `close` ends a string whose contents start `text`, skipping
/// backslash escapes.
fn closing_quote(text: &str, close: &str) -> Option<usize> {
    let mut characters = text.char_indices();
    while let Some((index, character)) = characters.next() {
        if character == '\\' {
            characters.next();
        } else if text[index..].starts_with(close) {
            return Some(index);
        }
    }
    None
}

/// The opens and closes of `pair` in `code`, with their byte positions.
fn tokens(code: &str, pair: &Bracket) -> Vec<(usize, Token)> {
    let mut tokens = Vec::new();
    let mut position = 0;
    while let Some(character) = code[position..].chars().next() {
        let rest = &code[position..];
        if rest.starts_with(pair.open.as_str()) {
            tokens.push((position, Token::Open));
            position += pair.open.len();
        } else if rest.starts_with(pair.close.as_str()) {
            tokens.push((position, Token::Close));
            position += pair.close.len();
        } else {
            position += character.len_utf8();
        }
    }
    tokens
}

/// The byte position of the last `pair` closer in `code` with no opener
/// before it, as on the line that ends a multi-line call.
fn last_unmatched_closer(code: &str, pair: &Bracket) -> Option<usize> {
    let mut depth = 0usize;
    let mut unmatched = None;
    for (position, token) in tokens(code, pair) {
        match token {
            Token::Open => depth += 1,
            Token::Close if depth == 0 => unmatched = Some(position),
            Token::Close => depth -= 1,
        }
    }
    unmatched
}

#[cfg(test)]
mod tests;
