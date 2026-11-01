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

/// A bracket pair, with the defaults JavaScript validation fills in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BracketPair {
    pub(crate) open: String,
    pub(crate) close: String,
    pub(crate) auto_close: bool,
    /// Whether Enter between the pair opens an indented middle line.
    pub(crate) newline: bool,
    /// Syntax at the cursor where auto-closing is suppressed.
    pub(crate) not_in: Vec<SyntaxFamily>,
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
    pub(crate) brackets: Vec<BracketPair>,
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
            brackets: self.brackets.clone().unwrap_or_default(),
            auto_close_before: self.auto_close_before.clone().unwrap_or_default(),
            increase_indent: compile(indentation.increase, IndentationPattern::Increase)?,
            decrease_indent: compile(indentation.decrease, IndentationPattern::Decrease)?,
        })
    }
}
