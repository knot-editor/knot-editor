//! The immutable text behind the piece tree, and one line index per backing.
//!
//! Backing text is the load-time original plus the append-only add sink: sealed
//! chunks followed by one growing tail. Each backing keeps its own sorted list
//! of line starts, so a piece's line breaks and UTF-16 length come from binary
//! searches over its backing plus a scan of at most one partial line, however
//! large the backing is. Splitting the original text of a huge file therefore
//! never rescans it.

use std::mem;
use std::sync::Arc;

use super::locator::Locator;
use super::{ADD_CHUNK_SIZE, Backing, Piece};

/// Start of one line within a backing, as UTF-8 byte and UTF-16 code-unit
/// offsets from the backing's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LineStart {
    pub byte: usize,
    pub utf16: usize,
}

/// The text a piece tree refers to, with its line indexes.
pub(super) struct Backings {
    /// Immutable load-time text backing the `Original` pieces.
    pub original: Arc<str>,
    /// Sealed chunks of the add sink, shared with snapshots. Rebuilt only when
    /// a chunk is sealed.
    pub sealed: Arc<[Arc<str>]>,
    /// Unsealed end of the add sink. Appended spans are never overwritten or
    /// moved; only new bytes go at the end.
    pub tail: String,
    /// UTF-16 length of `tail`.
    tail_utf16: usize,
    /// Line starts after each `'\n'`, per backing: the original text, then
    /// every sealed chunk, then the tail. A line start at byte 0 is implicit.
    lines: Vec<Vec<LineStart>>,
}

impl Backings {
    pub fn new(original: Arc<str>) -> Self {
        let mut starts = Vec::new();
        scan_line_starts(&original, LineStart { byte: 0, utf16: 0 }, &mut starts);
        Backings {
            original,
            sealed: Arc::from(Vec::new()),
            tail: String::new(),
            tail_utf16: 0,
            lines: vec![starts, Vec::new()],
        }
    }

    /// The whole text of one backing.
    pub fn text(&self, backing: Backing) -> &str {
        match backing {
            Backing::Original => &self.original,
            Backing::Add(chunk) => self
                .sealed
                .get(chunk as usize)
                .map_or(self.tail.as_str(), |sealed| sealed),
        }
    }

    /// The text a piece references.
    pub fn slice(&self, piece: &Piece) -> &str {
        &self.text(piece.backing)[piece.start..piece.start + piece.len]
    }

    /// Index of the chunk the tail will become.
    pub fn tail_chunk(&self) -> u32 {
        self.sealed.len() as u32
    }

    /// Append `text` to the tail, returning its start within the tail and its
    /// line-break count and UTF-16 length.
    pub fn append(&mut self, text: &str) -> (usize, usize, usize) {
        let start = self.tail.len();
        self.tail.push_str(text);
        let utf16 = utf16_len(text);
        let index = self.lines.last_mut().expect("the tail has a line index");
        let before = index.len();
        scan_line_starts(
            text,
            LineStart {
                byte: start,
                utf16: self.tail_utf16,
            },
            index,
        );
        let breaks = index.len() - before;
        self.tail_utf16 += utf16;
        (start, breaks, utf16)
    }

    /// Seal the tail into an immutable chunk once it reaches
    /// `ADD_CHUNK_SIZE`; the tail's line index moves with it.
    pub fn seal_full_tail(&mut self) {
        if self.tail.len() < ADD_CHUNK_SIZE {
            return;
        }
        let mut chunks = self.sealed.to_vec();
        chunks.push(Arc::from(mem::take(&mut self.tail)));
        self.sealed = chunks.into();
        self.tail_utf16 = 0;
        self.lines.push(Vec::new());
    }

    /// A piece with the given id and locator over `[start, start + len)` of
    /// `backing`, with its summary read from the line index.
    pub fn piece(
        &self,
        id: u64,
        locator: Locator,
        backing: Backing,
        start: usize,
        len: usize,
    ) -> Piece {
        let (breaks, utf16) = self.measure(backing, start, len);
        Piece {
            id,
            locator,
            backing,
            start,
            len,
            breaks,
            utf16,
        }
    }

    /// Line-break count and UTF-16 length of `[start, start + len)`.
    pub fn measure(&self, backing: Backing, start: usize, len: usize) -> (usize, usize) {
        let end = start + len;
        (
            self.breaks_before(backing, end) - self.breaks_before(backing, start),
            self.utf16_at(backing, end) - self.utf16_at(backing, start),
        )
    }

    /// Number of `'\n'` bytes before byte `at` of the backing.
    pub fn breaks_before(&self, backing: Backing, at: usize) -> usize {
        self.index(backing).partition_point(|s| s.byte <= at)
    }

    /// UTF-16 offset of byte `at` within the backing.
    pub fn utf16_at(&self, backing: Backing, at: usize) -> usize {
        let index = self.index(backing);
        let known = index.partition_point(|s| s.byte <= at);
        let base = known
            .checked_sub(1)
            .map_or(LineStart { byte: 0, utf16: 0 }, |i| index[i]);
        base.utf16 + utf16_len(&self.text(backing)[base.byte..at])
    }

    /// The `n`th line start (1-based) of the backing, as a byte offset.
    pub fn nth_line_start(&self, backing: Backing, n: usize) -> usize {
        self.index(backing)[n - 1].byte
    }

    /// The byte offset in `piece`, relative to its start, whose UTF-16 offset
    /// within the piece reaches `units`, rounding up inside a surrogate pair.
    /// `units` must not exceed the piece's UTF-16 length.
    pub fn byte_of_utf16(&self, piece: Piece, units: usize) -> usize {
        let index = self.index(piece.backing);
        let base_utf16 = self.utf16_at(piece.backing, piece.start);
        let target = base_utf16 + units;
        let known = index.partition_point(|s| s.utf16 <= target);
        let (mut byte, mut seen) = match known.checked_sub(1).map(|i| index[i]) {
            Some(line) if line.byte > piece.start => (line.byte, line.utf16),
            _ => (piece.start, base_utf16),
        };
        let end = piece.start + piece.len;
        for ch in self.text(piece.backing)[byte..end].chars() {
            if seen >= target {
                break;
            }
            seen += ch.len_utf16();
            byte += ch.len_utf8();
        }
        byte - piece.start
    }

    fn index(&self, backing: Backing) -> &[LineStart] {
        let slot = match backing {
            Backing::Original => 0,
            Backing::Add(chunk) => chunk as usize + 1,
        };
        &self.lines[slot]
    }

    /// Assert every line index equals a rescan of its text.
    #[cfg(test)]
    pub fn validate(&self) {
        let mut texts = vec![&*self.original];
        texts.extend(self.sealed.iter().map(|chunk| &**chunk));
        texts.push(&self.tail);
        assert_eq!(texts.len(), self.lines.len());
        for (text, index) in texts.into_iter().zip(&self.lines) {
            let mut rebuilt = Vec::new();
            scan_line_starts(text, LineStart { byte: 0, utf16: 0 }, &mut rebuilt);
            assert_eq!(*index, rebuilt);
        }
        assert_eq!(self.tail_utf16, utf16_len(&self.tail));
    }
}

/// Append the start of every line that follows a `'\n'` in `text`, where
/// `text` begins at `base`.
pub(super) fn scan_line_starts(text: &str, base: LineStart, out: &mut Vec<LineStart>) {
    let mut utf16 = base.utf16;
    for (i, ch) in text.char_indices() {
        utf16 += ch.len_utf16();
        if ch == '\n' {
            out.push(LineStart {
                byte: base.byte + i + 1,
                utf16,
            });
        }
    }
}

pub(super) fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}
