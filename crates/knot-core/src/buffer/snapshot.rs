//! `TextSnapshot`: an immutable, `Send` view of a `TextBuffer`'s text at one
//! moment, for readers off the foreground such as background parsing.
//!
//! A snapshot shares the buffer's piece tree, original text, and sealed add
//! chunks, and owns a copy of the add tail, so later edits never change what
//! it reads. Taking one clones the tree root and the chunk list and copies at
//! most one chunk's worth of tail text, however many pieces the buffer has.

use std::ops::Range;
use std::sync::Arc;

use super::tree::{Bias, Dimension, Tree};
use super::{Backing, Piece, TextBuffer};

/// Immutable text of a `TextBuffer` at the moment `TextBuffer::snapshot` ran.
#[derive(Clone)]
pub struct TextSnapshot {
    original: Arc<str>,
    sealed: Arc<[Arc<str>]>,
    tail: Arc<str>,
    tree: Tree,
}

impl TextBuffer {
    /// Capture the current text as an immutable snapshot.
    ///
    /// Costs a copy of the unsealed add tail, which is shorter than
    /// `ADD_CHUNK_SIZE`; the piece tree, original text, and sealed chunks are
    /// shared.
    pub fn snapshot(&self) -> TextSnapshot {
        TextSnapshot {
            original: Arc::clone(&self.backings.original),
            sealed: Arc::clone(&self.backings.sealed),
            tail: Arc::from(self.backings.tail.as_str()),
            tree: self.tree.clone(),
        }
    }
}

impl TextSnapshot {
    /// Total length of the text, in UTF-8 bytes.
    pub fn len(&self) -> usize {
        self.tree.summary().bytes
    }

    /// Whether the text is empty.
    // Only tests read this; product readers stream chunks.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The contiguous text from `offset` to the end of the piece containing
    /// it; empty at or past the end of the text.
    ///
    /// Concatenating successive chunks reads the whole text, which is the
    /// access pattern of tree-sitter's chunked input callback. `offset` must
    /// land on a UTF-8 char boundary.
    pub fn chunk_at(&self, offset: usize) -> &str {
        let Some(found) = self.tree.seek(Dimension::Bytes, offset, Bias::Right) else {
            return "";
        };
        &self.slice(&found.piece)[offset - found.before.bytes..]
    }

    /// The text in `range` as borrowed slices, in order.
    ///
    /// `range.start <= range.end <= len()` is required, and both ends must
    /// land on UTF-8 char boundaries.
    pub fn chunks(&self, range: Range<usize>) -> impl Iterator<Item = &str> {
        assert!(range.start <= range.end);
        assert!(range.end <= self.len());
        self.tree
            .pieces_from(range.start)
            .map_while(move |(start, piece)| {
                if start >= range.end {
                    return None;
                }
                let end = start + piece.len;
                Some(
                    &self.slice(&piece)[range.start.max(start) - start..range.end.min(end) - start],
                )
            })
    }

    /// Read a byte range of the text into a `String`, with the same
    /// requirements as `chunks`.
    pub fn read_range(&self, range: Range<usize>) -> String {
        let mut out = String::with_capacity(range.len());
        for chunk in self.chunks(range) {
            out.push_str(chunk);
        }
        out
    }

    fn slice(&self, p: &Piece) -> &str {
        let buf = match p.backing {
            Backing::Original => &self.original,
            Backing::Add(chunk) => self
                .sealed
                .get(chunk as usize)
                .map_or(&self.tail, |sealed| sealed),
        };
        &buf[p.start..p.start + p.len]
    }
}

#[cfg(test)]
mod tests;
