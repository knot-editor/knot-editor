//! Literal text search over a buffer.

use std::ops::Range;

use knot_protocol::ByteRange;

use super::{BufferAccessError, BufferModel};
use crate::editor::{WordClass, word_class};

/// The most matches one search returns, whatever limit it asks for.
pub const MAX_FIND_MATCHES: usize = 100_000;

/// How a search compares text with its query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FindOptions {
    /// Compare characters exactly; otherwise by their lowercase.
    pub case_sensitive: bool,
    /// Accept a match only at word boundaries, by the editor's word classes.
    pub whole_word: bool,
}

/// The matches of a search in order, and whether more followed the last.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindMatches {
    pub ranges: Vec<Range<usize>>,
    pub more: bool,
}

impl FindOptions {
    fn fold(self, character: char) -> char {
        if self.case_sensitive {
            return character;
        }
        let mut lower = character.to_lowercase();
        match (lower.next(), lower.next()) {
            (Some(single), None) => single,
            _ => character,
        }
    }
}

impl BufferModel {
    /// The non-overlapping matches of `query` inside `range` (the whole
    /// buffer when absent), up to `limit` capped at [`MAX_FIND_MATCHES`].
    /// Matches lie wholly inside the range, and a whole-word match is judged
    /// against the buffer text around the range, not its edges. An empty
    /// query matches nothing.
    pub fn find(
        &self,
        query: &str,
        options: FindOptions,
        range: Option<ByteRange>,
        limit: usize,
    ) -> Result<FindMatches, BufferAccessError> {
        let range = self.checked_range(range.unwrap_or(ByteRange {
            start_byte_offset: 0,
            end_byte_offset: self.len(),
        }))?;
        let limit = limit.min(MAX_FIND_MATCHES);
        let query: Vec<char> = query.chars().map(|c| options.fold(c)).collect();
        let mut found = FindMatches {
            ranges: Vec::new(),
            more: false,
        };
        if query.is_empty() || limit == 0 {
            return Ok(found);
        }

        let text = self.read(range.clone());
        let (before, after) = if options.whole_word {
            (self.char_before(range.start), self.char_after(range.end))
        } else {
            (None, None)
        };
        let mut resume = 0;
        for (start, _) in text.char_indices() {
            if start < resume {
                continue;
            }
            let Some(end) = match_at(&text, start, &query, options) else {
                continue;
            };
            if options.whole_word && !bounded(&text, start, end, before, after) {
                continue;
            }
            if found.ranges.len() == limit {
                found.more = true;
                break;
            }
            found.ranges.push(range.start + start..range.start + end);
            resume = end;
        }
        Ok(found)
    }

    fn char_before(&self, offset: usize) -> Option<char> {
        let mut start = offset.checked_sub(1)?;
        while !self.is_char_boundary(start) {
            start -= 1;
        }
        self.read(start..offset).chars().next()
    }

    fn char_after(&self, offset: usize) -> Option<char> {
        if offset >= self.len() {
            return None;
        }
        let mut end = offset + 1;
        while !self.is_char_boundary(end) {
            end += 1;
        }
        self.read(offset..end).chars().next()
    }
}

/// The end of the match of the folded `query` starting at `start`, if any.
fn match_at(text: &str, start: usize, query: &[char], options: FindOptions) -> Option<usize> {
    let mut length = 0;
    let mut characters = text[start..].chars();
    for &expected in query {
        let character = characters.next()?;
        if options.fold(character) != expected {
            return None;
        }
        length += character.len_utf8();
    }
    Some(start + length)
}

/// Whether the match `start..end` of `text` sits at word boundaries, with
/// `before` and `after` the buffer characters just outside `text`. A match
/// edge that is not itself a word character is a boundary.
fn bounded(
    text: &str,
    start: usize,
    end: usize,
    before: Option<char>,
    after: Option<char>,
) -> bool {
    let is_word = |character: char| word_class(character) == WordClass::Word;
    let first = text[start..].chars().next();
    let last = text[..end].chars().next_back();
    let previous = text[..start].chars().next_back().or(before);
    let next = text[end..].chars().next().or(after);
    let left = !first.is_some_and(is_word) || !previous.is_some_and(is_word);
    let right = !last.is_some_and(is_word) || !next.is_some_and(is_word);
    left && right
}
