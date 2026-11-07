//! Fold regions from a language's `folds` query.
//!
//! Every node a pattern captures as `@fold` that spans several lines is a
//! region from the node's first line through its last. A node that ends at
//! the start of a line ends on the line before. A node that starts its line,
//! such as the indented block of a Python `if`, starts on the line that holds
//! the colon before it, so the header stays visible. A closing bracket
//! that ends the node and starts its last line stays visible too. The query
//! author chooses which nodes fold.

use std::ops::Range;

use tree_sitter::{Language, Node, Query, QueryCursor};

use super::{Layer, StreamingIterator, SyntaxLanguageError, SyntaxQuery, SyntaxState, clamp};
use crate::core::buffer::TextSnapshot;

/// A compiled `folds` query.
pub(crate) struct FoldQuery {
    query: Query,
    capture: u32,
}

impl FoldQuery {
    /// Compile `source` against `grammar`; `None` when it has no patterns.
    pub(super) fn new(
        grammar: &Language,
        source: &str,
    ) -> Result<Option<Self>, SyntaxLanguageError> {
        let query =
            Query::new(grammar, source).map_err(|error| SyntaxLanguageError::InvalidQuery {
                query: SyntaxQuery::Folds,
                message: error.message,
                row: error.row,
                column: error.column,
            })?;
        let capture = query.capture_index_for_name("fold");
        Ok(capture
            .filter(|_| query.pattern_count() > 0)
            .map(|capture| Self { query, capture }))
    }
}

impl SyntaxState {
    /// Whether the buffer's own language has a `folds` query.
    pub(crate) fn has_fold_query(&self) -> bool {
        self.root.language.folds.is_some()
    }

    /// The multi-line regions the `folds` queries of every layer capture, as
    /// line ranges from the first line to one past the last.
    pub(crate) fn fold_regions(&self, text: &TextSnapshot) -> Vec<Range<usize>> {
        let mut regions = Vec::new();
        collect(&self.root, text, &mut regions);
        regions
    }
}

/// Whether only whitespace precedes `node` on its line.
fn starts_line(node: Node, text: &TextSnapshot) -> bool {
    let line_start = node
        .start_byte()
        .saturating_sub(node.start_position().column);
    text.read_range(clamp(line_start..node.start_byte(), text))
        .trim()
        .is_empty()
}

fn collect(layer: &Layer, text: &TextSnapshot, regions: &mut Vec<Range<usize>>) {
    if let (Some(folds), Some(tree)) = (&layer.language.folds, &layer.tree) {
        let mut cursor = QueryCursor::new();
        let node_text = |node: Node| {
            text.chunks(clamp(node.byte_range(), text))
                .map(str::as_bytes)
        };
        let mut matches = cursor.matches(&folds.query, tree.root_node(), node_text);
        while let Some(found) = matches.next() {
            for capture in found
                .captures()
                .iter()
                .filter(|capture| capture.index == folds.capture)
            {
                let node = capture.node;
                let mut first = node.start_position().row;
                if starts_line(node, text)
                    && let Some(previous) = node
                        .prev_sibling()
                        .filter(|previous| previous.kind() == ":")
                {
                    first = previous.end_position().row;
                }
                let end = node.end_position();
                let mut last = if end.column == 0 && end.row > node.start_position().row {
                    end.row - 1
                } else {
                    end.row
                };
                // A closing bracket that ends the node and starts its line
                // stays visible.
                let closes = node
                    .child(node.child_count().saturating_sub(1))
                    .is_some_and(|closer| {
                        !closer.is_named()
                            && matches!(closer.kind(), ")" | "]" | "}")
                            && closer.start_position().row == last
                            && starts_line(closer, text)
                    });
                if closes && last > first {
                    last -= 1;
                }
                if last > first {
                    regions.push(first..last + 1);
                }
            }
        }
    }
    for injection in &layer.injections {
        collect(injection, text, regions);
    }
}
