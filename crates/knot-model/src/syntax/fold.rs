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

use tree_sitter::{Language, Node, Point, Query, QueryCursor};

use super::{Layer, StreamingIterator, SyntaxLanguageError, SyntaxQuery, SyntaxState, clamp};
use knot_core::buffer::TextSnapshot;

/// A compiled `folds` query.
pub struct FoldQuery {
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
    pub fn has_fold_query(&self) -> bool {
        self.root.language.folds.is_some()
    }

    /// The multi-line regions the `folds` queries of every layer capture, as
    /// line ranges from the first line to one past the last.
    pub fn fold_regions(&self, text: &TextSnapshot) -> Vec<Range<usize>> {
        let mut regions = Vec::new();
        collect(&self.root, text, None, &mut regions);
        regions
    }

    /// The regions of [`Self::fold_regions`] whose first line is among
    /// `lines`, querying only the parts of the layers that reach them.
    pub fn fold_regions_starting_in(
        &self,
        text: &TextSnapshot,
        lines: Range<usize>,
    ) -> Vec<Range<usize>> {
        self.fold_regions_and_layers_reached(text, lines).0
    }

    /// [`Self::fold_regions_starting_in`] and how many layers reached `lines`.
    pub(super) fn fold_regions_and_layers_reached(
        &self,
        text: &TextSnapshot,
        lines: Range<usize>,
    ) -> (Vec<Range<usize>>, usize) {
        let mut regions = Vec::new();
        let reached = collect(&self.root, text, Some(&lines), &mut regions);
        regions.retain(|region| lines.contains(&region.start));
        (regions, reached)
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

/// Push the regions of `layer` and its injections, only those of nodes that
/// reach `lines` when given, and return how many layers reached them.
fn collect(
    layer: &Layer,
    text: &TextSnapshot,
    lines: Option<&Range<usize>>,
    regions: &mut Vec<Range<usize>>,
) -> usize {
    // A region may start on the line before its node, at a colon, so the
    // node may start one line past `lines`.
    let rows = lines.map(|lines| Point::new(lines.start, 0)..Point::new(lines.end + 1, 0));
    if let (Some(rows), Some(tree)) = (&rows, &layer.tree) {
        let root = tree.root_node();
        if root.end_position() < rows.start || root.start_position() >= rows.end {
            return 0;
        }
    }
    let mut reached = 1;
    if let (Some(folds), Some(tree)) = (&layer.language.folds, &layer.tree) {
        let mut cursor = QueryCursor::new();
        if let Some(rows) = rows.clone() {
            cursor.set_point_range(rows);
        }
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
        reached += collect(injection, text, lines, regions);
    }
    reached
}
