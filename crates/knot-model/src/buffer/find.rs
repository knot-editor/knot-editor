//! Literal and regular-expression text search over a buffer.

use std::ops::Range;

use knot_protocol::ByteRange;
use regex_automata::{
    Input, PatternID,
    meta::Regex,
    util::{interpolate, syntax},
};

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
    /// Keep at most one match starting on each line. After a match spanning
    /// lines, the line it ends on is a candidate again, past the match's end.
    pub per_line: bool,
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

/// The edits of a replacement and whether more matches followed the last.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Replacements {
    pub edits: Vec<knot_protocol::TextEdit>,
    pub more: bool,
}

/// What a replacement did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplaceOutcome {
    /// The matches replaced.
    pub count: usize,
    /// Whether more matches followed the last replaced.
    pub more: bool,
    /// Whether the text, and so the revision, changed: not when every
    /// replacement equals its match.
    pub changed: bool,
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
        let mut lines = PerLine::new(options);
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
            if !lines.accepts(&text, start, end) {
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
        let regex = compile(pattern, options)?;
        if limit == 0 {
            return Ok(found);
        }

        let (window_start, text, span) = self.window(&range);
        let mut lines = PerLine::new(options);
        for matched in regex.find_iter(Input::new(&text).span(span)) {
            if options.whole_word && !bounded(&text, matched.start(), matched.end(), None, None) {
                continue;
            }
            if !lines.accepts(&text, matched.start(), matched.end()) {
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

    /// The text of `range` with one character of context on each side, which
    /// is all an anchor or a word boundary can look at: where it starts in
    /// the buffer, the text, and `range` within it.
    fn window(&self, range: &Range<usize>) -> (usize, String, Range<usize>) {
        let start = range.start - self.char_before(range.start).map_or(0, char::len_utf8);
        let end = range.end + self.char_after(range.end).map_or(0, char::len_utf8);
        (
            start,
            self.read(start..end),
            range.start - start..range.end - start,
        )
    }

    /// The edits that replace the non-overlapping matches of `query` inside
    /// `range` (the whole buffer when absent) with `replacement`, up to
    /// `limit` capped at [`MAX_FIND_MATCHES`]; matching is [`Self::find`]'s.
    /// A literal query inserts the replacement as typed. A pattern expands
    /// `$1`, `${name}` and `$$` from the match's groups, and `\n`, `\t` and
    /// `\\` to a line break, a tab and a backslash; `\u` and `\l` change the
    /// next character to upper or lower case, `\U` and `\L` everything up to
    /// `\E` or `\e` (or the replacement's end), over literal text and
    /// expanded groups alike. The edits are ascending
    /// and disjoint, in the buffer's current coordinates.
    pub fn replace_matches(
        &self,
        query: &str,
        replacement: &str,
        options: FindOptions,
        range: Option<ByteRange>,
        limit: usize,
    ) -> Result<Replacements, FindError> {
        if !options.regex {
            let found = self.find(query, options, range, limit)?;
            return Ok(Replacements {
                edits: found
                    .ranges
                    .into_iter()
                    .map(|range| edit(range, replacement.to_owned()))
                    .collect(),
                more: found.more,
            });
        }

        let range = self.checked_range(range.unwrap_or(ByteRange {
            start_byte_offset: 0,
            end_byte_offset: self.len(),
        }))?;
        let limit = limit.min(MAX_FIND_MATCHES);
        let mut replaced = Replacements {
            edits: Vec::new(),
            more: false,
        };
        if query.is_empty() {
            return Ok(replaced);
        }
        let regex = compile(query, options)?;
        if limit == 0 {
            return Ok(replaced);
        }
        let template = parse_template(replacement);
        let (window_start, text, span) = self.window(&range);
        let mut lines = PerLine::new(options);
        for captures in regex.captures_iter(Input::new(&text).span(span)) {
            let Some(matched) = captures.get_match() else {
                continue;
            };
            if options.whole_word && !bounded(&text, matched.start(), matched.end(), None, None) {
                continue;
            }
            if !lines.accepts(&text, matched.start(), matched.end()) {
                continue;
            }
            if replaced.edits.len() == limit {
                replaced.more = true;
                break;
            }
            let expanded = expand(&template, &captures, &text);
            replaced.edits.push(edit(
                window_start + matched.start()..window_start + matched.end(),
                expanded,
            ));
        }
        Ok(replaced)
    }

    /// Replace the matches [`Self::replace_matches`] finds as one commit that
    /// the revision check `if_revision` (the current one when absent) guards;
    /// the buffer is untouched when nothing matches.
    pub fn find_and_replace(
        &mut self,
        query: &str,
        replacement: &str,
        options: FindOptions,
        range: Option<ByteRange>,
        limit: usize,
        if_revision: Option<u64>,
    ) -> Result<ReplaceOutcome, FindError> {
        let if_revision = if_revision.unwrap_or(self.revision());
        if if_revision != self.revision() {
            return Err(BufferAccessError::RevisionConflict.into());
        }
        let replaced = self.replace_matches(query, replacement, options, range, limit)?;
        let count = replaced.edits.len();
        let changed = self.apply_edits(&replaced.edits, if_revision)?;
        Ok(ReplaceOutcome {
            count,
            more: replaced.more,
            changed,
        })
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

fn compile(pattern: &str, options: FindOptions) -> Result<Regex, FindError> {
    Regex::builder()
        .syntax(
            syntax::Config::new()
                .multi_line(true)
                .case_insensitive(!options.case_sensitive),
        )
        .build(pattern)
        .map_err(|error| FindError::InvalidQuery(invalid_message(&error)))
}

fn edit(range: Range<usize>, text: String) -> knot_protocol::TextEdit {
    knot_protocol::TextEdit {
        range: ByteRange {
            start_byte_offset: range.start,
            end_byte_offset: range.end,
        },
        text,
    }
}

/// Which matches `per_line` keeps.
struct PerLine {
    enabled: bool,
    /// Matches starting before this offset of the searched text are dropped.
    skip_before: usize,
}

impl PerLine {
    fn new(options: FindOptions) -> Self {
        Self {
            enabled: options.per_line,
            skip_before: 0,
        }
    }

    /// Whether the match `start..end` of `text` is kept; a kept match moves
    /// the next candidate to the next line, or past its end when it spans
    /// lines.
    fn accepts(&mut self, text: &str, start: usize, end: usize) -> bool {
        if !self.enabled {
            return true;
        }
        if start < self.skip_before {
            return false;
        }
        self.skip_before = if text[start..end].contains('\n') {
            end
        } else {
            text[start..]
                .find('\n')
                .map_or(text.len(), |newline| start + newline + 1)
        };
        true
    }
}

/// A part of a replacement template.
enum Piece {
    /// Text with `$` references still to expand.
    Text(String),
    /// A case escape.
    Case(char),
}

/// `replacement` split at its case escapes, with `\n`, `\t` and `\\` replaced
/// by their characters; any other backslash stays.
fn parse_template(replacement: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut text = String::with_capacity(replacement.len());
    let mut characters = replacement.chars().peekable();
    while let Some(character) = characters.next() {
        let escaped = match (character, characters.peek()) {
            ('\\', Some('n')) => '\n',
            ('\\', Some('t')) => '\t',
            ('\\', Some('\\')) => '\\',
            ('\\', Some(&case @ ('u' | 'U' | 'l' | 'L' | 'e' | 'E'))) => {
                characters.next();
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Case(case));
                continue;
            }
            _ => {
                text.push(character);
                continue;
            }
        };
        characters.next();
        text.push(escaped);
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    pieces
}

/// The replacement for one match: the template with its groups expanded and
/// its case escapes applied.
fn expand(
    template: &[Piece],
    captures: &regex_automata::util::captures::Captures,
    text: &str,
) -> String {
    let mut expanded = String::new();
    let mut next: Option<char> = None;
    let mut rest: Option<char> = None;
    for piece in template {
        let piece = match piece {
            Piece::Case(case) => {
                match case {
                    'u' | 'l' => next = Some(*case),
                    'U' | 'L' => rest = Some(*case),
                    _ => rest = None,
                }
                continue;
            }
            Piece::Text(piece) => piece,
        };
        let mut part = String::new();
        interpolate::string(
            piece,
            |index, dst| {
                if let Some(group) = captures.get_group(index) {
                    dst.push_str(&text[group.range()]);
                }
            },
            |name| captures.group_info().to_index(PatternID::ZERO, name),
            &mut part,
        );
        for character in part.chars() {
            match next.take().or(rest) {
                Some('u' | 'U') => expanded.extend(character.to_uppercase()),
                Some(_) => expanded.extend(character.to_lowercase()),
                None => expanded.push(character),
            }
        }
    }
    expanded
}
