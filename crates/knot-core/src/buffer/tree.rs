//! A persistent B-tree of pieces with subtree summaries.
//!
//! Leaves hold pieces in text order; internal nodes hold children with the
//! summary of each child's subtree (bytes, line breaks, UTF-16 units). Nodes
//! are `Arc`-shared and mutated through `Arc::make_mut`, so an edit changes
//! nodes in place while no snapshot holds them and copies only the edited
//! path otherwise; cloning a tree is cloning its root.
//!
//! Every position is a byte offset into the text. Pieces are never empty, so
//! a piece boundary is identified by its offset: `insert` adds a piece after
//! the piece that ends at an offset, `modify` changes the piece that ends at
//! or contains an offset, and `remove` drops the pieces between two
//! boundaries. Searches pick the first piece whose cumulative end reaches the
//! target (`Bias::Left`) or passes it (`Bias::Right`).

use std::ops::{Add, AddAssign};
use std::slice;
use std::sync::Arc;

use super::Piece;
use super::locator::Locator;

/// Greatest number of pieces in a leaf.
const LEAF_MAX: usize = 32;
/// Greatest number of children of an internal node.
const INTERNAL_MAX: usize = 16;

/// Totals over a run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub bytes: usize,
    /// Number of `'\n'` bytes.
    pub breaks: usize,
    pub utf16: usize,
}

impl Add for Summary {
    type Output = Summary;

    fn add(self, other: Summary) -> Summary {
        Summary {
            bytes: self.bytes + other.bytes,
            breaks: self.breaks + other.breaks,
            utf16: self.utf16 + other.utf16,
        }
    }
}

impl AddAssign for Summary {
    fn add_assign(&mut self, other: Summary) {
        *self = *self + other;
    }
}

impl Piece {
    pub fn summary(&self) -> Summary {
        Summary {
            bytes: self.len,
            breaks: self.breaks,
            utf16: self.utf16,
        }
    }
}

/// What a search measures.
#[derive(Clone, Copy)]
pub(super) enum Dimension {
    Bytes,
    Breaks,
    Utf16,
}

impl Dimension {
    fn of(self, summary: &Summary) -> usize {
        match self {
            Dimension::Bytes => summary.bytes,
            Dimension::Breaks => summary.breaks,
            Dimension::Utf16 => summary.utf16,
        }
    }
}

/// Which piece a search picks when the target is exactly a piece boundary.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Bias {
    /// The piece ending at the target.
    Left,
    /// The piece starting at the target.
    Right,
}

/// A piece found by a search, with the totals of everything before it.
#[derive(Clone)]
pub(super) struct Located {
    pub piece: Piece,
    pub before: Summary,
}

#[derive(Clone)]
enum Node {
    Leaf(Vec<Piece>),
    Internal(Vec<Child>),
}

#[derive(Clone)]
struct Child {
    summary: Summary,
    /// The greatest locator in the subtree.
    last: Option<Locator>,
    node: Arc<Node>,
}

impl Child {
    /// Recompute the cached values after the subtree changed.
    fn refresh(&mut self) {
        self.summary = self.node.summary();
        self.last = self.node.last_locator();
    }
}

impl Node {
    fn summary(&self) -> Summary {
        match self {
            Node::Leaf(pieces) => pieces
                .iter()
                .fold(Summary::default(), |sum, p| sum + p.summary()),
            Node::Internal(children) => children
                .iter()
                .fold(Summary::default(), |sum, c| sum + c.summary),
        }
    }

    fn entries(&self) -> usize {
        match self {
            Node::Leaf(pieces) => pieces.len(),
            Node::Internal(children) => children.len(),
        }
    }

    fn max_entries(&self) -> usize {
        match self {
            Node::Leaf(_) => LEAF_MAX,
            Node::Internal(_) => INTERNAL_MAX,
        }
    }

    fn last_locator(&self) -> Option<Locator> {
        match self {
            Node::Leaf(pieces) => pieces.last().map(|p| p.locator.clone()),
            Node::Internal(children) => children.last().and_then(|c| c.last.clone()),
        }
    }

    fn child(node: Node) -> Child {
        Child {
            summary: node.summary(),
            last: node.last_locator(),
            node: Arc::new(node),
        }
    }

    /// Move the upper half out when the node holds more than it may.
    fn split_if_full(&mut self) -> Option<Child> {
        if self.entries() <= self.max_entries() {
            return None;
        }
        let mid = self.entries() / 2;
        Some(Node::child(match self {
            Node::Leaf(pieces) => Node::Leaf(pieces.split_off(mid)),
            Node::Internal(children) => Node::Internal(children.split_off(mid)),
        }))
    }

    fn collect(&self, out: &mut Vec<Piece>) {
        match self {
            Node::Leaf(pieces) => out.extend_from_slice(pieces),
            Node::Internal(children) => {
                for child in children {
                    child.node.collect(out);
                }
            }
        }
    }
}

/// The pieces of a text in order, with a summary of the whole.
#[derive(Clone)]
pub(super) struct Tree {
    root: Arc<Node>,
    summary: Summary,
}

impl Tree {
    pub fn new() -> Self {
        Tree {
            root: Arc::new(Node::Leaf(Vec::new())),
            summary: Summary::default(),
        }
    }

    pub fn from_piece(piece: Piece) -> Self {
        Tree {
            summary: piece.summary(),
            root: Arc::new(Node::Leaf(vec![piece])),
        }
    }

    /// Totals over the whole text.
    pub fn summary(&self) -> Summary {
        self.summary
    }

    /// The first piece in text order whose cumulative end in `dimension`
    /// reaches (`Bias::Left`) or passes (`Bias::Right`) `target`.
    pub fn seek(&self, dimension: Dimension, target: usize, bias: Bias) -> Option<Located> {
        let reached = |end: &Summary| match bias {
            Bias::Left => dimension.of(end) >= target,
            Bias::Right => dimension.of(end) > target,
        };
        let mut node = &*self.root;
        let mut before = Summary::default();
        loop {
            match node {
                Node::Leaf(pieces) => {
                    for piece in pieces {
                        let end = before + piece.summary();
                        if reached(&end) {
                            return Some(Located {
                                piece: piece.clone(),
                                before,
                            });
                        }
                        before = end;
                    }
                    return None;
                }
                Node::Internal(children) => {
                    let mut next = None;
                    for child in children {
                        let end = before + child.summary;
                        if reached(&end) {
                            next = Some(&*child.node);
                            break;
                        }
                        before = end;
                    }
                    node = next?;
                }
            }
        }
    }

    /// The pieces in text order from the piece containing byte `at`.
    pub fn pieces_from(&self, at: usize) -> Pieces<'_> {
        let mut stack = Vec::new();
        let mut offset = 0;
        let mut node = &*self.root;
        let leaf = loop {
            match node {
                Node::Internal(children) => {
                    let mut rest = children.iter();
                    let mut next = None;
                    for child in rest.by_ref() {
                        if offset + child.summary.bytes > at {
                            next = Some(&*child.node);
                            break;
                        }
                        offset += child.summary.bytes;
                    }
                    let Some(child) = next else {
                        return Pieces {
                            stack: Vec::new(),
                            leaf: [].iter(),
                            offset: self.summary.bytes,
                        };
                    };
                    stack.push(rest);
                    node = child;
                }
                Node::Leaf(pieces) => {
                    let mut leaf = pieces.iter();
                    loop {
                        let before_piece = leaf.clone();
                        match leaf.next() {
                            Some(piece) if offset + piece.len <= at => offset += piece.len,
                            _ => {
                                leaf = before_piece;
                                break;
                            }
                        }
                    }
                    break leaf;
                }
            }
        };
        Pieces {
            stack,
            leaf,
            offset,
        }
    }

    /// The piece with `locator` and its start offset.
    pub fn find(&self, locator: &Locator) -> Option<(usize, Piece)> {
        let mut node = &*self.root;
        let mut offset = 0;
        loop {
            match node {
                Node::Leaf(pieces) => {
                    for piece in pieces {
                        if piece.locator == *locator {
                            return Some((offset, piece.clone()));
                        }
                        offset += piece.len;
                    }
                    return None;
                }
                Node::Internal(children) => {
                    let mut next = None;
                    for child in children {
                        if child.last.as_ref().is_some_and(|last| last >= locator) {
                            next = Some(&*child.node);
                            break;
                        }
                        offset += child.summary.bytes;
                    }
                    node = next?;
                }
            }
        }
    }

    /// Insert `piece` after the piece that ends at byte `at`, or first when
    /// `at` is 0.
    pub fn insert(&mut self, at: usize, piece: Piece) {
        let summary = piece.summary();
        let root = Arc::make_mut(&mut self.root);
        if let Some(sibling) = Self::insert_into(root, at, piece) {
            let left = Node::child(std::mem::replace(root, Node::Leaf(Vec::new())));
            *root = Node::Internal(vec![left, sibling]);
        }
        self.summary += summary;
    }

    fn insert_into(node: &mut Node, at: usize, piece: Piece) -> Option<Child> {
        match node {
            Node::Leaf(pieces) => {
                let mut end = 0;
                let mut index = 0;
                for p in pieces.iter() {
                    end += p.len;
                    if end > at {
                        break;
                    }
                    index += 1;
                }
                pieces.insert(index, piece);
            }
            Node::Internal(children) => {
                let (index, local) = Self::child_at(children, at);
                let child = &mut children[index];
                let sibling = Self::insert_into(Arc::make_mut(&mut child.node), local, piece);
                child.refresh();
                if let Some(sibling) = sibling {
                    children.insert(index + 1, sibling);
                }
            }
        }
        node.split_if_full()
    }

    /// The child whose range reaches byte `at` first, and `at` relative to it.
    fn child_at(children: &[Child], at: usize) -> (usize, usize) {
        let mut start = 0;
        for (index, child) in children.iter().enumerate() {
            let end = start + child.summary.bytes;
            if end >= at || index + 1 == children.len() {
                return (index, at - start);
            }
            start = end;
        }
        unreachable!("an internal node has children")
    }

    /// Change the piece that ends at or contains byte `at` (`at > 0`), passing
    /// it and its start offset to `edit`. The piece's summary fields may
    /// change; its identity and backing span start must not.
    pub fn modify<R>(&mut self, at: usize, edit: impl FnOnce(&mut Piece, usize) -> R) -> R {
        fn go<R>(
            node: &mut Node,
            at: usize,
            start: usize,
            edit: impl FnOnce(&mut Piece, usize) -> R,
        ) -> R {
            match node {
                Node::Leaf(pieces) => {
                    let mut piece_start = start;
                    for piece in pieces {
                        if piece_start + piece.len >= at {
                            return edit(piece, piece_start);
                        }
                        piece_start += piece.len;
                    }
                    unreachable!("modify({at}) past the end of the text")
                }
                Node::Internal(children) => {
                    let (index, local) = Tree::child_at(children, at - start);
                    let child_start = at - local;
                    let child = &mut children[index];
                    let result = go(Arc::make_mut(&mut child.node), at, child_start, edit);
                    child.refresh();
                    result
                }
            }
        }
        assert!(at > 0, "no piece ends at 0");
        let result = go(Arc::make_mut(&mut self.root), at, 0, edit);
        self.summary = self.root.summary();
        result
    }

    /// Remove the pieces within `[start, end)`, which must lie on piece
    /// boundaries, and return them in order.
    pub fn remove(&mut self, start: usize, end: usize) -> Vec<Piece> {
        let mut removed = Vec::new();
        if start >= end {
            return removed;
        }
        Self::remove_from(Arc::make_mut(&mut self.root), start, end, &mut removed);
        loop {
            let only = match &*self.root {
                Node::Internal(children) if children.len() == 1 => Arc::clone(&children[0].node),
                Node::Internal(children) if children.is_empty() => Arc::new(Node::Leaf(Vec::new())),
                _ => break,
            };
            self.root = only;
        }
        self.summary = self.root.summary();
        removed
    }

    fn remove_from(node: &mut Node, start: usize, end: usize, removed: &mut Vec<Piece>) {
        match node {
            Node::Leaf(pieces) => {
                let mut offset = 0;
                pieces.retain(|piece| {
                    let piece_start = offset;
                    offset += piece.len;
                    let inside = piece_start >= start && piece_start < end;
                    if inside {
                        removed.push(piece.clone());
                    }
                    !inside
                });
            }
            Node::Internal(children) => {
                let mut offset = 0;
                let mut gone = vec![false; children.len()];
                for (index, child) in children.iter_mut().enumerate() {
                    let child_start = offset;
                    let child_end = offset + child.summary.bytes;
                    offset = child_end;
                    if child_end <= start || child_start >= end {
                        continue;
                    }
                    if start <= child_start && child_end <= end {
                        child.node.collect(removed);
                        gone[index] = true;
                        continue;
                    }
                    Self::remove_from(
                        Arc::make_mut(&mut child.node),
                        start.max(child_start) - child_start,
                        end.min(child_end) - child_start,
                        removed,
                    );
                    child.refresh();
                    gone[index] = child.node.entries() == 0;
                }
                let mut index = 0;
                children.retain(|_| {
                    index += 1;
                    !gone[index - 1]
                });
                Self::merge_small(children);
            }
        }
    }

    /// Merge neighbors where one is under half full and both fit one node.
    fn merge_small(children: &mut Vec<Child>) {
        let mut index = 0;
        while index + 1 < children.len() {
            let (left, right) = (&children[index], &children[index + 1]);
            let combined = left.node.entries() + right.node.entries();
            let small = left.node.entries().min(right.node.entries()) < left.node.max_entries() / 2;
            if !small || combined > left.node.max_entries() {
                index += 1;
                continue;
            }
            let right = children.remove(index + 1);
            let left = &mut children[index];
            match (Arc::make_mut(&mut left.node), &*right.node) {
                (Node::Leaf(into), Node::Leaf(from)) => into.extend_from_slice(from),
                (Node::Internal(into), Node::Internal(from)) => into.extend_from_slice(from),
                _ => unreachable!("siblings have the same depth"),
            }
            left.summary += right.summary;
            left.last = right.last;
        }
    }

    /// Number of levels, counting the leaves.
    #[cfg(test)]
    pub fn height(&self) -> usize {
        let mut node = &*self.root;
        let mut height = 1;
        while let Node::Internal(children) = node {
            node = &children[0].node;
            height += 1;
        }
        height
    }

    /// Assert the tree's structural invariants: uniform depth, node sizes in
    /// bounds, non-empty pieces, and summaries equal to the sum of their
    /// children.
    #[cfg(test)]
    pub fn validate(&self) {
        fn go(node: &Node, root: bool) -> usize {
            assert!(node.entries() <= node.max_entries(), "node over capacity");
            match node {
                Node::Leaf(pieces) => {
                    assert!(root || !pieces.is_empty(), "empty leaf");
                    assert!(pieces.iter().all(|p| p.len > 0), "empty piece");
                    1
                }
                Node::Internal(children) => {
                    assert!(children.len() >= if root { 2 } else { 1 }, "thin node");
                    let depths: Vec<usize> = children
                        .iter()
                        .map(|child| {
                            assert_eq!(child.summary, child.node.summary(), "stale summary");
                            assert_eq!(child.last, child.node.last_locator(), "stale locator");
                            go(&child.node, false)
                        })
                        .collect();
                    assert!(depths.windows(2).all(|w| w[0] == w[1]), "uneven depth");
                    depths[0] + 1
                }
            }
        }
        go(&self.root, true);
        assert_eq!(self.summary, self.root.summary(), "stale tree summary");
        let pieces: Vec<Piece> = self.pieces_from(0).map(|(_, piece)| piece).collect();
        assert!(
            pieces.windows(2).all(|w| w[0].locator < w[1].locator),
            "locators out of order"
        );
    }
}

/// Pieces in text order, each with its start offset.
pub(super) struct Pieces<'a> {
    stack: Vec<slice::Iter<'a, Child>>,
    leaf: slice::Iter<'a, Piece>,
    offset: usize,
}

impl Iterator for Pieces<'_> {
    type Item = (usize, Piece);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(piece) = self.leaf.next() {
                let start = self.offset;
                self.offset += piece.len;
                return Some((start, piece.clone()));
            }
            let child = loop {
                match self.stack.last_mut()?.next() {
                    Some(child) => break child,
                    None => {
                        self.stack.pop();
                    }
                }
            };
            let mut node = &*child.node;
            // Descend to the leftmost leaf below the next child.
            loop {
                match node {
                    Node::Internal(children) => {
                        let mut rest = children.iter();
                        let first = rest.next().expect("an internal node has children");
                        self.stack.push(rest);
                        node = &first.node;
                    }
                    Node::Leaf(pieces) => {
                        self.leaf = pieces.iter();
                        break;
                    }
                }
            }
        }
    }
}
