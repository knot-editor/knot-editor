//! Language editing configuration: the comment tokens, bracket pairs, and
//! indentation patterns a language registers for native editing commands.

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
