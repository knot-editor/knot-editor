//! Literal and regular-expression text search over a buffer.

use std::ops::Range;

use knot_protocol::ByteRange;
use regex_automata::{Input, meta::Regex, util::syntax};

use super::{BufferAccessError, BufferModel};
use crate::editor::{WordClass, word_class};

/// The most matches one search returns, whatever limit it asks for.
pub const MAX_FIND_MATCHES: usize = 100_000;

/// How a search compares text with its query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FindOptions {
    /// Compare characters exactly; otherwise by their simple case fold.
    pub case_sensitive: bool,
    /// Accept a match only at word boundaries, by the editor's word classes.
    pub whole_word: bool,
    /// Read the query as a Rust `regex` pattern in multi-line mode: `^` and
    /// `$` match at line boundaries and `\n` matches across lines. Case
    /// folding is then the pattern engine's own.
    pub regex: bool,
}

/// Why a search failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FindError {
    Access(BufferAccessError),
    /// The regular expression did not parse; carries the parser's message.
    InvalidQuery(String),
}

impl From<BufferAccessError> for FindError {
    fn from(error: BufferAccessError) -> Self {
        Self::Access(error)
    }
}

/// The matches of a search in order, and whether more followed the last.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindMatches {
    pub ranges: Vec<Range<usize>>,
    pub more: bool,
}

impl FindOptions {
    /// The character's simple case fold, which keeps matches one character
    /// long: its lowercase of its uppercase, so `ς`, `σ` and `Σ` agree and
    /// `ſ` meets `s`. A character that case-maps to several (`ß`, `İ`) and
    /// the Turkish dotless `ı` stay themselves.
    fn fold(self, character: char) -> char {
        if self.case_sensitive || character == 'ı' {
            return character;
        }
        let single =
            |mut mapped: Box<dyn Iterator<Item = char>>| match (mapped.next(), mapped.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            };
        let upper = single(Box::new(character.to_uppercase())).unwrap_or(character);
        single(Box::new(upper.to_lowercase())).unwrap_or(character)
    }
}

impl BufferModel {
    /// The non-overlapping matches of `query` inside `range` (the whole
    /// buffer when absent), up to `limit` capped at [`MAX_FIND_MATCHES`].
    /// Matches lie wholly inside the range, and a whole-word match is judged
    /// against the buffer text around the range, not its edges; so are the
    /// anchors and word boundaries of a regular expression, whose empty
    /// matches are kept. An empty query matches nothing.
    pub fn find(
        &self,
        query: &str,
        options: FindOptions,
        range: Option<ByteRange>,
        limit: usize,
    ) -> Result<FindMatches, FindError> {
        let range = self.checked_range(range.unwrap_or(ByteRange {
            start_byte_offset: 0,
            end_byte_offset: self.len(),
        }))?;
        let limit = limit.min(MAX_FIND_MATCHES);
        if options.regex {
            return self.find_regex(query, options, range, limit);
        }
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

    fn find_regex(
        &self,
        pattern: &str,
        options: FindOptions,
        range: Range<usize>,
        limit: usize,
    ) -> Result<FindMatches, FindError> {
        let mut found = FindMatches {
            ranges: Vec::new(),
            more: false,
        };
        if pattern.is_empty() {
            return Ok(found);
        }
        let regex = Regex::builder()
            .syntax(
                syntax::Config::new()
                    .multi_line(true)
                    .case_insensitive(!options.case_sensitive),
            )
            .build(pattern)
            .map_err(|error| FindError::InvalidQuery(invalid_message(&error)))?;
        if limit == 0 {
            return Ok(found);
        }

        // One character of context on each side is all an anchor or a word
        // boundary can look at.
        let window_start = range.start - self.char_before(range.start).map_or(0, char::len_utf8);
        let window_end = range.end + self.char_after(range.end).map_or(0, char::len_utf8);
        let text = self.read(window_start..window_end);
        let span = range.start - window_start..range.end - window_start;
        for matched in regex.find_iter(Input::new(&text).span(span)) {
            if options.whole_word && !bounded(&text, matched.start(), matched.end(), None, None) {
                continue;
            }
            if found.ranges.len() == limit {
                found.more = true;
                break;
            }
            found
                .ranges
                .push(window_start + matched.start()..window_start + matched.end());
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

/// The parser's message for a pattern that did not compile.
fn invalid_message(error: &regex_automata::meta::BuildError) -> String {
    match error.syntax_error() {
        Some(syntax) => syntax.to_string(),
        None => error.to_string(),
    }
}
