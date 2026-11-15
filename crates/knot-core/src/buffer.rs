//! `TextBuffer`: stable-ID piece-table backing store.
//!
//! The buffer has monotonic stable piece IDs, insert / delete / replace /
//! read-range operations, line and UTF-16 queries, and a `Position` /
//! `BufferEdit` surface for stable external ranges.
//!
//! ## Piece tree
//!
//! The pieces live in a persistent B-tree (`tree`) whose nodes summarize their
//! subtree's bytes, line breaks, and UTF-16 units. Offsets, lines, and UTF-16
//! offsets are found by descending the tree, so an edit or a query costs
//! O(log pieces) however many pieces earlier edits created. The tree's nodes
//! are shared with snapshots and copied on write.
//!
//! ## Locators
//!
//! Each piece carries a locator (`locator`), an ordering key that increases in
//! text order and is chosen between its neighbors when the piece is created, so
//! it never changes. The buffer maps every piece id to its locator, and `resolve`
//! finds a piece by seeking the tree by locator, in O(log pieces) rather than a
//! walk over the pieces. Snapshots never resolve positions and do not carry the
//! map.
//!
//! ## Line index
//!
//! There is no document-wide line table. Each backing keeps its own sorted
//! line starts (`backing`), as UTF-8 byte and UTF-16 offsets within it. A
//! piece caches its line-break count and UTF-16 length, computed from its
//! backing's index. Text and every public position stay UTF-8; the UTF-16
//! queries let platform text input, which speaks document-wide UTF-16 offsets,
//! convert in O(log pieces + line length).
//!
//! ## Position tokens
//!
//! A `Position` is an opaque `(PieceId, u32 offset_within_piece)` pair.
//! It is stable across *unrelated* edits: an edit elsewhere in the
//! buffer does not invalidate it. The only failure modes are:
//!
//! - The piece was deleted wholesale → `resolve` returns `None`.
//! - The position sat in the right half of an interior split and
//!   the underlying piece has since been shortened → `resolve` returns
//!   `None`, but the content the position pointed to is recoverable
//!   from the matching `Split` record in the edit log: the new location
//!   is `(Split::new_piece, old_offset - Split::split_offset)`.
//!
//! Default stickiness is *sticky-left*: a `position_at` query at a piece
//! boundary returns the piece ending at that offset (with `offset ==
//! piece.len`), so a subsequent insert at the boundary leaves the
//! position pointing at the same byte it pointed at before. Sticky-right
//! anchors belong to the anchored-range store, which reads the edit log and
//! relocates them.
//!
//! ## Edit log
//!
//! Every `insert` / `delete` (and `replace`, which decomposes into a
//! delete-then-insert pair) pushes one `BufferEdit` event onto the log.
//! Fields are byte offsets into the *pre-edit* logical text, so an observer
//! replaying stale `Position`s can reason about each edit against the buffer
//! state *in which the position was issued*. Each event also records the
//! line and byte column of its edges, which observers such as incremental
//! parsers need and cannot recover once later edits have moved the lines. Sequence numbers are monotonic
//! for the buffer's lifetime; the owner discards entries once every observer
//! has consumed them, so the retained log stays bounded.
//!
//! ## Backing store
//!
//! Immutable backing text backs the piece chain:
//!
//! - `Original`: the buffer's load-time text, shared as an `Arc<str>`. Pieces
//!   referencing it never move; the buffer holds it for its whole lifetime.
//! - `Add`: an append-only sink, stored as sealed `Arc<str>` chunks plus one
//!   mutable tail. Every inserted span is appended to the tail and referenced
//!   by a freshly-issued piece, or by extending the piece that already ends at
//!   the tail's end (see coalescing below). Once the tail reaches
//!   `ADD_CHUNK_SIZE` it is sealed and a new tail starts, so a span never
//!   straddles chunks and old pieces keep their `(Add(chunk), start, len)`
//!   references stable forever.
//!
//! A `Piece` is a reference `(backing, start)` fixed at creation, plus a
//! monotonically-increasing `PieceId` issued from a global counter. Only
//! `len` ever changes: the split rule shortens it, and coalescing extends it.
//!
//! ## Snapshots
//!
//! `snapshot` returns an immutable, `Send` [`TextSnapshot`] for readers off
//! the foreground. It shares the piece tree, the original text, and the sealed
//! chunks, and copies only the tail, so its cost is bounded by the chunk size
//! rather than by the document length or the piece count.
//!
//! ## Split rule
//!
//! Interior edits split a piece; the left half keeps the original id (and
//! original `start`, with `len` shortened to the split offset), the right
//! half gets a fresh id. Degenerate splits (offset 0 or `len`) are elided —
//! the new piece is spliced in directly, no zero-length halves created.
//!
//! ## Coalescing
//!
//! An insert at the end of an `Add` piece whose span ends at the end of the
//! tail extends that piece instead of creating one, so typing grows one piece
//! rather than one per keystroke. Existing positions in the piece keep their
//! offsets, and a sticky-left position at its old end still resolves before
//! the inserted text. A split-shortened piece never qualifies, because its
//! right half still references later bytes of the tail, and neither does a
//! piece in a sealed chunk.
//!
//! ## UTF-8 boundary policy
//!
//! Buffer edits and reads use byte offsets which must land on UTF-8 scalar
//! boundaries. Grapheme-aware caret movement and deletion remain presentation
//! policy above the byte-native core.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use snapshot::TextSnapshot;

use backing::Backings;
use locator::Locator;
use tree::{Bias, Dimension, Tree};

mod backing;
mod locator;
mod snapshot;
mod tree;

/// Length at which the add sink's tail is sealed into an immutable chunk.
///
/// Snapshots copy the tail, so this bounds their per-call copy; a single
/// larger insert seals as soon as it is appended.
const ADD_CHUNK_SIZE: usize = 16 * 1024;

/// Monotonically-issued identity for each `TextBuffer` instance.
///
/// This is separate from piece IDs: transactions use it to reject a call made
/// with a different buffer that happens to have the same edit-log sequence.
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

/// Monotonically-increasing piece identifier, issued from a global counter.
///
/// Stable across unrelated edits: a piece's id never changes for the
/// piece's lifetime, and is never reused (the counter only goes up).
pub type PieceId = u64;

/// Opaque, stable token for a location in the logical text.
///
/// `(piece, offset_within_piece)`. Issued by `TextBuffer::position_at`,
/// resolved back to a byte offset by `TextBuffer::resolve`. The token
/// is stable across unrelated edits; failure modes are documented on
/// the module and on `resolve`.
///
/// Field accessors are exposed (rather than the struct being fully
/// opaque) because the anchored-range store needs to *store* tokens keyed by piece id and
/// to apply the deterministic remap described in `Split`; the fields
/// are immutable and the type is `Copy`, so leaking them is safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    piece: PieceId,
    offset: u32,
}

impl Position {
    /// The piece this token is anchored to.
    pub fn piece(&self) -> PieceId {
        self.piece
    }

    /// The offset within `piece`, in UTF-8 bytes. Satisfies `<= piece.len`
    /// for a *valid* token; may exceed it for a *stale* token issued
    /// before the piece was split.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Construct a token from raw `(piece, offset)` fields. Exposed so
    /// callers can build the *remapped* token produced by applying
    /// the split rule to a `Split` record — `(Split::new_piece,
    /// old_offset - Split::split_offset)`. The buffer itself never uses
    /// this; the only in-crate producer of tokens is `position_at`.
    pub fn new(piece: PieceId, offset: u32) -> Self {
        Self { piece, offset }
    }
}

/// A line and a byte column within it, both 0-indexed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextPoint {
    pub line: usize,
    pub column: usize,
}

/// One half of an interior split.
///
/// A `Position` whose `piece` is `old_piece` and whose offset is
/// `> split_offset` is *stale but detectable*: its content has moved to
/// `new_piece` at offset `old_offset - split_offset`. The anchored-range store reads the
/// edit log and applies this transformation; the buffer itself never
/// touches its callers' tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    /// The piece that was shortened by the split (retains its id).
    pub old_piece: PieceId,
    /// The split point within that piece, equal to its post-split length.
    pub split_offset: usize,
    /// The freshly-issued id of the right (longer-offset) half.
    pub new_piece: PieceId,
}

/// Per-edit event in the `TextBuffer`'s edit log.
///
/// `at` and `range` are byte offsets into the buffer's *pre-edit*
/// logical text, so an observer replaying stale `Position`s can reason
/// about each edit against the buffer state in which the tokens were
/// issued. `splits` is normally length 0 (the edit landed on a piece
/// boundary and was elided) or 1 (one interior split); a `delete`
/// spanning two interiors may carry up to 2.
///
/// In addition to the splits, each variant carries enough
/// affected-piece metadata for the anchored-range store to repair
/// stale tokens in `O(affected)` time per edit,
/// without walking the whole piece chain: the anchored-range store keys
/// anchors by `piece_id`, looks up only the pieces this edit touches,
/// and re-binds them via these fields.
#[derive(Clone, Debug)]
pub enum BufferEdit {
    /// Bytes were inserted at `at`; the inserted span has byte length
    /// `inserted_len`. Splits describe any interior piece splits the
    /// edit forced (at most one, at `at`).
    Insert {
        at: usize,
        inserted_len: usize,
        /// Point of `at`.
        at_point: TextPoint,
        /// Point of the end of the inserted span, after the insert.
        end_point: TextPoint,
        /// Position at the end of the inserted span: `(new_piece,
        /// inserted_len)` for a fresh piece, or the extended piece's new
        /// end when the insert coalesced. Sticky-`After` anchors sitting on
        /// a boundary-elided insert relocate here.
        inserted_end: Position,
        /// The piece ending at logical offset `at` *pre-insert*, and
        /// the offset-within-that-piece corresponding to the boundary
        /// `at`. `None` iff `at == 0` (no piece to the left of the
        /// buffer start). A sticky-`After` anchor whose token is
        /// `(left_piece.id, left_piece_offset)` (i.e. sitting exactly
        /// at the boundary) relocates to `inserted_end`;
        /// a sticky-`Before` anchor keeps its token and resolves to
        /// `at` post-edit, matching sticky-`Before` semantics.
        ///
        /// For an interior-split insert, `left_piece` is
        /// `(splits[0].old_piece, splits[0].split_offset)` (the
        /// original piece, now shortened to the split offset) —
        /// equivalent boundary semantics, already covered by the split
        /// remap rule. Stabilize only consults `left_piece` when
        /// `splits` is empty (the boundary-elided case).
        left_piece: Option<(PieceId, usize)>,
        /// The `pieces[0]` id *pre-insert*. Used only when `at == 0`
        /// to relocate sticky-`Before` anchors anchored to the buffer's
        /// first piece at offset 0 — those would otherwise resolve to
        /// `n` post-edit (chain shift), but sticky-`Before` requires them to
        /// stay at `0`.
        pre_first_piece: Option<PieceId>,
        splits: Vec<Split>,
    },
    /// Bytes in `range` were removed. Splits describe any interior
    /// piece splits the edit forced (up to two, at `range.start` and
    /// `range.end`).
    Delete {
        range: Range<usize>,
        /// Point of `range.start`.
        start_point: TextPoint,
        /// Point of `range.end`, before the delete.
        end_point: TextPoint,
        /// Piece ids drained by this delete (the contents of
        /// `[left.insert_at, right.insert_at)` post-both-splits,
        /// pre-drain). Anchors whose token.piece is in this set were
        /// inside the deleted span and are re-bound to a surviving
        /// edge piece below.
        deleted_pieces: Vec<PieceId>,
        /// `(piece_id, post-split_len)` for the drained pieces, in chain
        /// order. The anchored-range store uses this to tell true interior endpoints from
        /// sticky-left tokens exactly at the delete's start/end boundary
        /// when a boundary split was elided.
        deleted_piece_lens: Vec<(PieceId, usize)>,
        /// The piece ending at logical `range.start` (= `s`) post-edit
        /// and its post-edit length. `None` iff `range.start == 0`
        /// (no piece to the left of the delete). Sticky-`Before`
        /// anchors inside `(s, e)` snap to `(left_survivor.0,
        /// left_survivor.1)`, which resolves to `s` post-edit.
        left_survivor: Option<(PieceId, usize)>,
        /// The piece starting at logical `range.start` (= `e` pre-edit,
        /// shifted to `s` post-edit) post-delete. `None` iff the buffer
        /// is empty after the delete. Sticky-`After` anchors inside
        /// `(s, e)` snap to `(right_survivor, 0)`, which resolves to
        /// `s` post-edit.
        right_survivor: Option<PieceId>,
        splits: Vec<Split>,
    },
}

/// Which backing buffer a piece references.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backing {
    /// The load-time text. Held immutably for the `TextBuffer`'s lifetime.
    Original,
    /// One chunk of the append-only edit sink: a sealed chunk, or the tail
    /// when the index equals the number of sealed chunks. Appended spans
    /// never move.
    Add(u32),
}

/// A reference to a contiguous span of one backing buffer.
///
/// `(backing, start)` is fixed at creation. Only `len` changes, with the
/// summary fields that follow it: the split rule shortens the left half, and
/// coalescing extends a trailing `Add` piece.
#[derive(Clone, Debug)]
struct Piece {
    id: PieceId,
    /// Orders this piece among all others; see `locator`.
    locator: Locator,
    backing: Backing,
    start: usize,
    len: usize,
    /// Number of `'\n'` bytes in the span.
    breaks: usize,
    /// Length of the span in UTF-16 code units.
    utf16: usize,
}

/// A hand-rolled piece table with stable piece IDs.
///
/// See module docs for the backing-store and split-rule policies.
pub struct TextBuffer {
    /// Stable identity of this buffer instance. Kept crate-visible because
    /// `EditTransaction` must bind its recorded byte offsets to one buffer.
    instance_id: u64,
    /// The text the pieces refer to, with a line index per backing.
    backings: Backings,
    /// The pieces in text order. Operations change it in place; snapshots
    /// clone its root.
    tree: Tree,
    /// The locator of every piece in the tree, for `resolve`. Snapshots do not
    /// carry it.
    locators: HashMap<PieceId, Locator>,
    /// Global monotonic piece-id counter. Never reused.
    next_id: PieceId,
    /// Retained edit log. Each `insert` / `delete` pushes one `BufferEdit`.
    /// `replace` decomposes into a delete-then-insert pair and pushes two.
    edits: Vec<BufferEdit>,
    /// Sequence number of `edits[0]`: how many edits were discarded.
    discarded_edits: usize,
}

impl TextBuffer {
    /// Build a `TextBuffer` from load-time text.
    ///
    /// The text becomes the `Original` backing; an initial single piece
    /// references the whole span. The `Add` sink starts empty. No `\r`
    /// normalization is performed — the bytes are stored verbatim.
    pub fn from_text(text: impl Into<Arc<str>>) -> Self {
        let original = text.into();
        let len = original.len();
        let backings = Backings::new(original);
        let mut buf = TextBuffer {
            instance_id: NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed),
            backings,
            tree: Tree::new(),
            locators: HashMap::new(),
            next_id: 1,
            edits: Vec::new(),
            discarded_edits: 0,
        };
        if len > 0 {
            let locator = Locator::between(None, None);
            let piece = buf
                .backings
                .piece(buf.next_id, locator.clone(), Backing::Original, 0, len);
            buf.locators.insert(piece.id, locator);
            buf.next_id += 1;
            buf.tree = Tree::from_piece(piece);
        }
        buf
    }

    /// Build an empty buffer.
    pub fn new() -> Self {
        Self::from_text("")
    }

    /// Total length of the logical text, in UTF-8 bytes.
    pub fn len(&self) -> usize {
        self.tree.summary().bytes
    }

    /// Stable identity of this `TextBuffer` instance.
    ///
    /// This is crate-visible rather than public API: it guards recorded edit
    /// transactions against being replayed against another buffer.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Whether the logical text is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read a byte range of the logical text into a `String`.
    ///
    /// `range` is inclusive-start / exclusive-end byte offsets into the
    /// logical text. `range.start <= range.end <= len()` is required,
    /// and both ends must land on UTF-8 char boundaries (debug-asserted).
    pub fn read_range(&self, range: Range<usize>) -> String {
        assert!(range.start <= range.end);
        assert!(range.end <= self.len());
        debug_assert!(self.is_char_boundary(range.start));
        debug_assert!(self.is_char_boundary(range.end));

        let mut out = String::with_capacity(range.end - range.start);
        for chunk in self.chunks(range) {
            out.push_str(chunk);
        }
        out
    }

    /// The logical text in `range` as borrowed piece slices, in order.
    fn chunks(&self, range: Range<usize>) -> impl Iterator<Item = &str> {
        self.tree
            .pieces_from(range.start)
            .map_while(move |(start, piece)| {
                if start >= range.end {
                    return None;
                }
                let end = start + piece.len;
                Some(
                    &self.backings.slice(&piece)
                        [range.start.max(start) - start..range.end.min(end) - start],
                )
            })
    }

    /// Insert `text` at byte offset `at`.
    ///
    /// `at` must satisfy `0 <= at <= len()` and land on a UTF-8 char
    /// boundary (debug-asserted). The inserted text itself must be valid
    /// UTF-8 (Rust guarantees this via `&str`).
    pub fn insert(&mut self, at: usize, text: &str) {
        assert!(at <= self.len());
        debug_assert!(self.is_char_boundary(at));

        if text.is_empty() {
            return;
        }

        let at_point = self.point_of_offset(at);

        // Make `at` a piece boundary. Splits are recorded so they can be
        // attached to the BufferEdit event.
        let pre_first_piece = self.first_piece().map(|p| p.id);
        let mut splits = Vec::new();
        self.split_at(at, &mut splits);

        // The piece ending at logical `at` pre-insert (and the
        // offset-within-that-piece corresponding to the boundary `at`).
        // For interior-split inserts this is `(splits[0].old_piece,
        // splits[0].split_offset)` (post-split, the old_piece is
        // shortened to the split offset); stabilize relies only on the
        // splits list in that case, so this is dormant but kept
        // consistent.
        let left = self.piece_ending_at(at);
        let left_piece = left.as_ref().map(|p| (p.id, p.len));

        // Append the span to the add tail and record its placement.
        let tail_chunk = self.backings.tail_chunk();
        let (add_start, breaks, utf16) = self.backings.append(text);

        let coalesce = splits.is_empty()
            && matches!(
                left,
                Some(Piece { backing: Backing::Add(chunk), start, len, .. })
                    if chunk == tail_chunk && start + len == add_start
            );
        let inserted_end = if coalesce {
            self.tree.modify(at, |piece, _| {
                piece.len += text.len();
                piece.breaks += breaks;
                piece.utf16 += utf16;
                Position::new(piece.id, piece.len as u32)
            })
        } else {
            let id = self.next_id;
            self.next_id += 1;
            let locator = Locator::between(
                left.as_ref().map(|p| &p.locator),
                self.piece_starting_at(at).as_ref().map(|p| &p.locator),
            );
            self.locators.insert(id, locator.clone());
            let piece = Piece {
                id,
                locator,
                backing: Backing::Add(tail_chunk),
                start: add_start,
                len: text.len(),
                breaks,
                utf16,
            };
            self.tree.insert(at, piece);
            Position::new(id, text.len() as u32)
        };
        self.backings.seal_full_tail();

        let end_point = self.point_of_offset(at + text.len());

        // Record the edit event. `at` and `inserted_len` are byte
        // offsets into the pre-edit logical text.
        self.edits.push(BufferEdit::Insert {
            at,
            inserted_len: text.len(),
            at_point,
            end_point,
            inserted_end,
            left_piece,
            pre_first_piece,
            splits,
        });
    }

    /// Delete the byte range `[start, end)` from the logical text.
    ///
    /// Both ends must land on UTF-8 char boundaries (debug-asserted).
    /// Deleting an empty range is a no-op.
    pub fn delete(&mut self, range: Range<usize>) {
        assert!(range.start <= range.end);
        assert!(range.end <= self.len());
        debug_assert!(self.is_char_boundary(range.start));
        debug_assert!(self.is_char_boundary(range.end));

        if range.is_empty() {
            return;
        }
        let start_point = self.point_of_offset(range.start);
        let end_point = self.point_of_offset(range.end);

        // Split at both ends, then drop the pieces in between. Both calls
        // share one `splits` vec so the ordering in the BufferEdit event is
        // "left-end-split, then right-end-split".
        let mut splits = Vec::new();
        self.split_at(range.start, &mut splits);
        self.split_at(range.end, &mut splits);

        // Collect the affected-piece metadata stabilization needs to
        // repair stale tokens in O(affected) without walking the chain.
        // `left_survivor` is the piece ending at `range.start` (= `s`)
        // post-edit, which (if a split happened at `s`) is the original
        // piece now shortened to the split offset. `right_survivor` is the
        // piece that pre-edit starts at `range.end` and post-edit shifts to
        // start at `s`. `None` iff the delete reaches the buffer's start or
        // end respectively (no left/right edge survives).
        let removed = self.tree.remove(range.start, range.end);
        for piece in &removed {
            self.locators.remove(&piece.id);
        }
        let deleted_piece_lens: Vec<(PieceId, usize)> =
            removed.iter().map(|p| (p.id, p.len)).collect();
        let deleted_pieces: Vec<PieceId> = deleted_piece_lens.iter().map(|(id, _)| *id).collect();
        let left_survivor = self.piece_ending_at(range.start).map(|p| (p.id, p.len));
        let right_survivor = self
            .tree
            .seek(Dimension::Bytes, range.start, Bias::Right)
            .map(|found| found.piece.id);

        // Record the edit event. `range` is the pre-edit byte span.
        self.edits.push(BufferEdit::Delete {
            range,
            start_point,
            end_point,
            deleted_pieces,
            deleted_piece_lens,
            left_survivor,
            right_survivor,
            splits,
        });
    }

    /// Replace `[range.start, range.end)` with `text`.
    ///
    /// Implemented as delete-then-insert; `range` and `at` follow the same
    /// UTF-8 boundary rules as those operations. Replacing an empty range is
    /// equivalent to `insert(range.start, text)`.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        assert!(range.start <= range.end);
        assert!(range.end <= self.len());
        debug_assert!(self.is_char_boundary(range.start));
        debug_assert!(self.is_char_boundary(range.end));

        if range.is_empty() {
            self.insert(range.start, text);
            return;
        }
        self.delete(range.clone());
        self.insert(range.start, text);
    }

    // ---- Line index ------------------------------------------------

    /// Number of lines. The buffer always has at least one line (line 0,
    /// possibly empty), so this returns `>= 1`. A trailing `'\n'` creates one
    /// extra empty line.
    pub fn line_count(&self) -> usize {
        self.tree.summary().breaks + 1
    }

    /// Byte offset of the start of `line` (0-indexed). `line` must be
    /// `< line_count()`.
    pub fn line_start(&self, line: usize) -> usize {
        assert!(
            line < self.line_count(),
            "line {line} out of range ({})",
            self.line_count()
        );
        if line == 0 {
            return 0;
        }
        // The piece holding the `line`th line break.
        let found = self
            .tree
            .seek(Dimension::Breaks, line, Bias::Left)
            .expect("the line break exists");
        let piece = found.piece;
        // The line break's index among the backing's line starts.
        let nth =
            self.backings.breaks_before(piece.backing, piece.start) + (line - found.before.breaks);
        found.before.bytes + self.backings.nth_line_start(piece.backing, nth) - piece.start
    }

    /// 0-indexed line number of the line containing byte offset `offset`.
    /// `offset` must satisfy `0 <= offset <= len()`. `len()` maps to the
    /// last line.
    pub fn line_of_offset(&self, offset: usize) -> usize {
        assert!(offset <= self.len());
        let Some(found) = self.tree.seek(Dimension::Bytes, offset, Bias::Left) else {
            return 0;
        };
        let piece = found.piece;
        let start = piece.start;
        let within = offset - found.before.bytes;
        found.before.breaks + self.backings.breaks_before(piece.backing, start + within)
            - self.backings.breaks_before(piece.backing, start)
    }

    /// Line and byte column of `offset`, which must satisfy
    /// `0 <= offset <= len()`.
    pub fn point_of_offset(&self, offset: usize) -> TextPoint {
        let line = self.line_of_offset(offset);
        TextPoint {
            line,
            column: offset - self.line_start(line),
        }
    }

    /// UTF-16 code-unit offset of byte offset `offset`, which must be a char
    /// boundary `<= len()`.
    pub fn utf16_of_offset(&self, offset: usize) -> usize {
        debug_assert!(self.is_char_boundary(offset));
        let Some(found) = self.tree.seek(Dimension::Bytes, offset, Bias::Left) else {
            return 0;
        };
        let piece = found.piece;
        let within = offset - found.before.bytes;
        found.before.utf16 + self.backings.utf16_at(piece.backing, piece.start + within)
            - self.backings.utf16_at(piece.backing, piece.start)
    }

    /// Byte offset of UTF-16 code-unit offset `utf16`. An offset inside a
    /// surrogate pair rounds up to the end of its scalar, and an offset past
    /// the end clamps to `len()`.
    pub fn offset_of_utf16(&self, utf16: usize) -> usize {
        let Some(found) = self.tree.seek(Dimension::Utf16, utf16, Bias::Left) else {
            return self.len();
        };
        found.before.bytes
            + self
                .backings
                .byte_of_utf16(found.piece, utf16 - found.before.utf16)
    }

    // ---- Position tokens -------------------------------------

    /// Issue a `Position` token for byte offset `at` in the current
    /// logical text.
    ///
    /// `at` must satisfy `0 <= at <= len()`. Returns `None` iff the
    /// buffer is empty (no piece exists to anchor the token).
    ///
    /// Sticky-left policy: if `at` lies on a piece boundary, the
    /// token is anchored to the piece *ending* at `at`, with
    /// `offset == piece.len`. A subsequent insert at `at` is therefore
    /// boundary-elided (no split), and the position keeps pointing at
    /// the same byte it pointed at before. To point *past* an inserted
    /// span (sticky-right), the anchored-range store issues its own token *after* the
    /// edit, or remaps via the edit log.
    pub fn position_at(&self, at: usize) -> Option<Position> {
        assert!(at <= self.len());
        let found = self.tree.seek(Dimension::Bytes, at, Bias::Left)?;
        Some(Position {
            piece: found.piece.id,
            offset: (at - found.before.bytes) as u32,
        })
    }

    /// Resolve a `Position` token back to a byte offset.
    ///
    /// Returns `Some(byte_offset)` iff the position is *valid*: the
    /// piece still exists in the chain and `offset <= piece.len` in its
    /// *current* length. Returns `None` if:
    ///
    /// - The anchor piece has been deleted (no piece with `id` remains).
    /// - The anchor piece exists but `offset > piece.len` — the
    ///   position sat in the right half of an interior split and
    ///   is now *stale but detectable*. The anchored-range store reads the matching
    ///   `Split` from the edit log and remaps to
    ///   `(Split::new_piece, offset - Split::split_offset)`.
    pub fn resolve(&self, p: Position) -> Option<usize> {
        let (start, piece) = self.tree.find(self.locators.get(&p.piece)?)?;
        let off = p.offset as usize;
        (off <= piece.len).then_some(start + off)
    }

    // ---- Edit log -------------------------------------------------

    /// High-water mark of the edit log: the sequence number that will be
    /// assigned to the *next* edit (zero on a fresh buffer, incremented
    /// by one per `insert` / `delete`).
    pub fn edit_seq(&self) -> usize {
        self.discarded_edits + self.edits.len()
    }

    /// Slice of all edits with sequence number `>= seq`. `seq` is the
    /// high-water mark returned by `edit_seq`; an observer keeps a
    /// cursor, polls `edits_since(cursor)` each cycle, then advances
    /// its cursor by the returned slice's length.
    ///
    /// `seq` must not precede discarded edits. A `seq` past the end yields
    /// an empty slice.
    pub fn edits_since(&self, seq: usize) -> &[BufferEdit] {
        assert!(
            seq >= self.discarded_edits,
            "edits before {} were discarded (requested {seq})",
            self.discarded_edits
        );
        let start = (seq - self.discarded_edits).min(self.edits.len());
        &self.edits[start..]
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn retained_edits(&self) -> usize {
        self.edits.len()
    }

    /// Discard every edit with sequence number `< seq`, once all observers
    /// have consumed them. `edit_seq` is unaffected.
    pub fn discard_edits_before(&mut self, seq: usize) {
        let count = seq
            .saturating_sub(self.discarded_edits)
            .min(self.edits.len());
        self.edits.drain(..count);
        self.discarded_edits += count;
    }

    // ---- Internals -------------------------------------------------------

    /// The first piece, if the buffer is not empty.
    fn first_piece(&self) -> Option<Piece> {
        self.tree
            .seek(Dimension::Bytes, 0, Bias::Left)
            .map(|found| found.piece)
    }

    /// The piece ending at byte `at`, which must be a piece boundary; `None`
    /// at offset 0.
    fn piece_ending_at(&self, at: usize) -> Option<Piece> {
        if at == 0 {
            return None;
        }
        self.tree
            .seek(Dimension::Bytes, at, Bias::Left)
            .map(|found| found.piece)
    }

    /// The piece starting at byte `at`, which must be a piece boundary; `None`
    /// at the end of the text.
    fn piece_starting_at(&self, at: usize) -> Option<Piece> {
        self.tree.pieces_from(at).next().map(|(_, piece)| piece)
    }

    /// Whether `offset` is a UTF-8 scalar boundary in the logical text.
    ///
    /// This is crate-visible so the editor host can validate untrusted byte
    /// ranges before calling the buffer's assertion-based mutation and read
    /// operations.
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        if offset == 0 || offset == self.len() {
            return true;
        }
        // Piece boundaries are char boundaries by construction; inside a
        // piece, defer to its backing, which is valid UTF-8.
        self.tree
            .seek(Dimension::Bytes, offset, Bias::Right)
            .is_some_and(|found| {
                let within = offset - found.before.bytes;
                within == 0 || self.backings.slice(&found.piece).is_char_boundary(within)
            })
    }

    /// Make byte offset `at` a piece boundary.
    ///
    /// `at` must satisfy `0 <= at <= len()`.
    ///
    /// Degenerate splits — `at` at the start or end of an existing piece —
    /// are elided by the split rule: no zero-length halves are created.
    /// Otherwise the containing piece is split, the left half retains its id
    /// (with `len` shortened to the split offset) and the right half is
    /// inserted as a fresh piece behind it.
    ///
    /// If a strict interior split occurs, a `Split` record is pushed onto
    /// `splits` so the caller can attach it to the `BufferEdit` event it
    /// is constructing. Boundary-elided splits push nothing.
    fn split_at(&mut self, at: usize, splits: &mut Vec<Split>) {
        if at == 0 || at == self.len() {
            return;
        }
        let found = self
            .tree
            .seek(Dimension::Bytes, at, Bias::Left)
            .expect("`at` lies within the text");
        let piece = found.piece;
        let left_len = at - found.before.bytes;
        if left_len == piece.len {
            return;
        }

        let left = self.backings.piece(
            piece.id,
            piece.locator.clone(),
            piece.backing,
            piece.start,
            left_len,
        );
        let right_id = self.next_id;
        self.next_id += 1;
        let successor = self.piece_starting_at(at + piece.len - left_len);
        let right_locator =
            Locator::between(Some(&piece.locator), successor.as_ref().map(|p| &p.locator));
        self.locators.insert(right_id, right_locator.clone());
        let right = Piece {
            id: right_id,
            locator: right_locator,
            backing: piece.backing,
            start: piece.start + left_len,
            len: piece.len - left_len,
            breaks: piece.breaks - left.breaks,
            utf16: piece.utf16 - left.utf16,
        };
        self.tree.modify(at, |p, _| *p = left);
        self.tree.insert(at, right);
        splits.push(Split {
            old_piece: piece.id,
            split_offset: left_len,
            new_piece: right_id,
        });
    }

    /// The pieces in text order, for tests.
    #[cfg(test)]
    fn pieces(&self) -> Vec<Piece> {
        self.tree.pieces_from(0).map(|(_, piece)| piece).collect()
    }

    /// Assert the invariants of the tree and the line indexes: node sizes,
    /// summaries, and every piece's cached summary against its text.
    #[cfg(test)]
    fn validate(&self) {
        self.tree.validate();
        self.backings.validate();
        let mut ids = std::collections::HashSet::new();
        for piece in self.pieces() {
            assert!(ids.insert(piece.id), "duplicate piece id {}", piece.id);
            assert_eq!(self.locators.get(&piece.id), Some(&piece.locator));
            let text = self.backings.slice(&piece);
            assert_eq!(piece.breaks, text.matches('\n').count());
            assert_eq!(piece.utf16, backing::utf16_len(text));
        }
        assert_eq!(self.locators.len(), ids.len(), "map holds deleted pieces");
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------------------------------
// Unit tests
// ----------------------------------------------------------------------

#[cfg(test)]
mod differential_tests;
#[cfg(test)]
mod tests;
