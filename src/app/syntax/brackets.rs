//! Bracket matching over the syntax tree.
//!
//! A bracket is a leaf token whose text is the `open` or `close` of one of its
//! language's registered pairs whose open and close differ. Its partner is the
//! first sibling in its direction that holds the paired text with the
//! siblings between them balanced, so a bracket inside a string or comment,
//! which is part of a larger leaf, never matches, and a token whose parent
//! lacks the partner, as inside a parse error, has none.

use std::ops::Range;

use tree_sitter::Node;

use super::{Layer, SyntaxState};
use crate::app::languages::editing::Bracket;
use crate::core::buffer::TextSnapshot;

/// The longest bracket token read; longer leaves are never brackets.
const MAX_BRACKET_LEN: usize = 16;

/// A matched pair: the byte ranges of the opener and the closer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BracketPair {
    pub(crate) open: Range<usize>,
    pub(crate) close: Range<usize>,
}

impl SyntaxState {
    /// The pair whose opener or closer touches `offset`: the bracket ending
    /// at `offset` first, else the one starting there. The first range is the
    /// bracket at `offset`, the second its partner.
    pub(crate) fn bracket_at(
        &self,
        text: &TextSnapshot,
        offset: usize,
    ) -> Option<(Range<usize>, Range<usize>)> {
        let before = offset
            .checked_sub(1)
            .and_then(|start| self.bracket_touching(text, start..offset, true));
        before.or_else(|| self.bracket_touching(text, offset..offset + 1, false))
    }

    /// The innermost pair whose opener ends at or before `offset` and whose
    /// closer starts at or after it.
    pub(crate) fn enclosing_brackets(
        &self,
        text: &TextSnapshot,
        offset: usize,
    ) -> Option<BracketPair> {
        self.layers_at(offset).into_iter().find_map(|layer| {
            let tree = layer.tree.as_ref()?;
            let brackets = matching_pairs(&layer.language.editing.brackets);
            let mut node = Some(tree.root_node().descendant_for_byte_range(offset, offset)?);
            while let Some(current) = node {
                if let Some(pair) = enclosing_children(current, text, &brackets, offset) {
                    return Some(pair);
                }
                node = current.parent();
            }
            None
        })
    }

    /// The layers whose parsed ranges contain `offset`, deepest first.
    fn layers_at(&self, offset: usize) -> Vec<&Layer> {
        let mut layers = vec![&self.root];
        let mut layer = &self.root;
        while let Some(injection) = layer
            .injections
            .iter()
            .find(|injection| injection.parses(offset))
        {
            layers.push(injection);
            layer = injection;
        }
        layers.reverse();
        layers
    }

    /// The bracket covering `probe` that ends at `probe.end` when `ending`,
    /// or starts at `probe.start` otherwise, with its partner.
    fn bracket_touching(
        &self,
        text: &TextSnapshot,
        probe: Range<usize>,
        ending: bool,
    ) -> Option<(Range<usize>, Range<usize>)> {
        if probe.end > text.len() {
            return None;
        }
        self.layers_at(probe.start).into_iter().find_map(|layer| {
            let tree = layer.tree.as_ref()?;
            let node = tree
                .root_node()
                .descendant_for_byte_range(probe.start, probe.end)?;
            let touches = if ending {
                node.end_byte() == probe.end
            } else {
                node.start_byte() == probe.start
            };
            if !touches {
                return None;
            }
            let brackets = matching_pairs(&layer.language.editing.brackets);
            let token = token_text(node, text)?;
            let partner = brackets.iter().find_map(|bracket| {
                if token == bracket.open {
                    partner(node, text, bracket, true)
                } else if token == bracket.close {
                    partner(node, text, bracket, false)
                } else {
                    None
                }
            })?;
            Some((node.byte_range(), partner.byte_range()))
        })
    }
}

/// The pairs that take part in matching: those whose open and close differ.
fn matching_pairs(brackets: &[Bracket]) -> Vec<&Bracket> {
    brackets
        .iter()
        .filter(|bracket| bracket.open != bracket.close)
        .collect()
}

/// The text of a leaf short enough to be a bracket; `None` for other nodes,
/// including zero-width nodes error recovery inserts.
fn token_text(node: Node, text: &TextSnapshot) -> Option<String> {
    let range = node.byte_range();
    if node.child_count() != 0
        || node.is_missing()
        || range.is_empty()
        || range.len() > MAX_BRACKET_LEN
        || range.end > text.len()
    {
        return None;
    }
    Some(text.read_range(range))
}

/// The sibling of `node`, a bracket of `bracket`, that pairs with it: the
/// first after it holding `close` when `forward`, else the first before it
/// holding `open`, skipping balanced pairs between them.
fn partner<'tree>(
    node: Node<'tree>,
    text: &TextSnapshot,
    bracket: &Bracket,
    forward: bool,
) -> Option<Node<'tree>> {
    let (same, other) = if forward {
        (&bracket.open, &bracket.close)
    } else {
        (&bracket.close, &bracket.open)
    };
    let step = |node: Node<'tree>| {
        if forward {
            node.next_sibling()
        } else {
            node.prev_sibling()
        }
    };
    let mut depth = 0usize;
    let mut sibling = step(node);
    while let Some(current) = sibling {
        if let Some(token) = token_text(current, text) {
            if &token == same {
                depth += 1;
            } else if &token == other {
                if depth == 0 {
                    return Some(current);
                }
                depth -= 1;
            }
        }
        sibling = step(current);
    }
    None
}

/// The innermost pair among `node`'s children that encloses `offset`.
fn enclosing_children(
    node: Node,
    text: &TextSnapshot,
    brackets: &[&Bracket],
    offset: usize,
) -> Option<BracketPair> {
    let mut walk = node.walk();
    let mut found = None;
    for child in node.children(&mut walk) {
        if child.end_byte() > offset {
            break;
        }
        let Some(token) = token_text(child, text) else {
            continue;
        };
        let Some(bracket) = brackets.iter().find(|bracket| token == bracket.open) else {
            continue;
        };
        if let Some(close) = partner(child, text, bracket, true)
            && close.start_byte() >= offset
        {
            found = Some(BracketPair {
                open: child.byte_range(),
                close: close.byte_range(),
            });
        }
    }
    found
}

#[cfg(test)]
mod tests;
