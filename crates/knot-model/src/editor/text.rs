//! Line, position, and boundary queries over the model's text. Positions are
//! `(line, byte_col)` carets or document byte and UTF-16 offsets.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::BufferModel;

/// Byte range of `line`'s content: the line without its terminator, so a
/// `\r` before `\n` is excluded. The caret never sits inside a terminator.
pub fn content_range(model: &BufferModel, line: usize) -> Range<usize> {
    let range = model.line_range(line);
    let crlf = line + 1 < model.line_count()
        && range.end > range.start
        && model.is_char_boundary(range.end - 1)
        && model.read(range.end - 1..range.end) == "\r";
    range.start..range.end - usize::from(crlf)
}

/// Text of `line`'s content.
pub fn line_text(model: &BufferModel, line: usize) -> String {
    model.read(content_range(model, line))
}

/// Last caret column on `line`.
pub fn line_end(model: &BufferModel, line: usize) -> usize {
    content_range(model, line).len()
}

pub fn to_flat_byte(model: &BufferModel, line: usize, byte_col: usize) -> usize {
    model.line_range(line).start + byte_col
}

/// The (line, byte_col) caret for a byte offset, clamped to the text and kept
/// out of line terminators.
pub fn flat_byte_to_position(model: &BufferModel, offset: usize) -> (usize, usize) {
    let offset = offset.min(model.len());
    let line = model.line_of_offset(offset);
    let content = content_range(model, line);
    (line, (offset - content.start).min(content.len()))
}

pub fn to_flat_utf16(model: &BufferModel, line: usize, byte_col: usize) -> usize {
    model.utf16_of_offset(to_flat_byte(model, line, byte_col))
}

pub fn flat_utf16_to_position(model: &BufferModel, offset: usize) -> (usize, usize) {
    flat_byte_to_position(model, model.offset_of_utf16(offset))
}

/// Previous grapheme-cluster boundary before `col` on `line`; 0 if already at
/// the start. This keeps the caret out of emoji ZWJ and modifier sequences,
/// which must be edited as a single user-perceived character.
pub fn prev_boundary(model: &BufferModel, line: usize, col: usize) -> usize {
    if col == 0 {
        return 0;
    }
    line_text(model, line)
        .grapheme_indices(true)
        .take_while(|(start, _)| *start < col)
        .last()
        .map_or(0, |(start, _)| start)
}

/// Next grapheme-cluster boundary after `col` on `line`; line end if at the end.
pub fn next_boundary(model: &BufferModel, line: usize, col: usize) -> usize {
    let text = line_text(model, line);
    text.grapheme_indices(true)
        .map(|(start, _)| start)
        .find(|start| *start > col)
        .unwrap_or(text.len())
}

/// VS Code's default word separators: each run of them is a unit of its own,
/// so `some.thing` is two words around a `.`.
const WORD_SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WordClass {
    Whitespace,
    Separator,
    Word,
}

/// The class of one character for word movement, selection, and whole-word search.
pub fn word_class(character: char) -> WordClass {
    if character.is_whitespace() {
        WordClass::Whitespace
    } else if WORD_SEPARATORS.contains(character) {
        WordClass::Separator
    } else {
        WordClass::Word
    }
}

/// The units double-click selects and word movement stops at, as
/// `(start, end, class)` byte ranges covering `text`: Unicode word segments
/// split wherever the character class changes, with runs of separators and
/// of whitespace merged. Word characters keep Unicode segmentation, so
/// ideographs and emoji stay separate units.
pub fn word_segments(text: &str) -> Vec<(usize, usize, WordClass)> {
    let mut segments: Vec<(usize, usize, WordClass)> = Vec::new();
    for (start, segment) in text.split_word_bound_indices() {
        for (offset, character) in segment.char_indices() {
            let class = word_class(character);
            let at = start + offset;
            let end = at + character.len_utf8();
            match segments.last_mut() {
                Some(last)
                    if last.1 == at
                        && last.2 == class
                        && (class != WordClass::Word || offset > 0) =>
                {
                    last.1 = end
                }
                _ => segments.push((at, end, class)),
            }
        }
    }
    segments
}

/// The nearest word or separator-run end after `caret`, or start before it,
/// skipping whitespace. Words never span lines, so the scan reads lines
/// outward from the caret's line.
pub fn word_boundary(model: &BufferModel, line: usize, caret: usize, forward: bool) -> usize {
    let words = |line: usize| {
        let range = model.line_range(line);
        let text = model.read(range.clone());
        word_segments(&text)
            .into_iter()
            .filter(|(_, _, class)| *class != WordClass::Whitespace)
            .map(|(start, end, _)| (range.start + start, range.start + end))
            .collect::<Vec<_>>()
    };
    if forward {
        (line..model.line_count())
            .find_map(|line| {
                words(line)
                    .into_iter()
                    .map(|(_, end)| end)
                    .find(|end| *end > caret)
            })
            .unwrap_or(model.len())
    } else {
        (0..=line)
            .rev()
            .find_map(|line| {
                words(line)
                    .into_iter()
                    .map(|(start, _)| start)
                    .take_while(|start| *start < caret)
                    .last()
            })
            .unwrap_or(0)
    }
}

/// Convert a UTF-16 offset within `s` to a UTF-8 byte offset.
pub fn utf16_to_byte_col(s: &str, utf16_off: usize) -> usize {
    let mut utf16_count = 0usize;
    for (byte_idx, ch) in s.char_indices() {
        if utf16_count >= utf16_off {
            return byte_idx;
        }
        utf16_count += ch.len_utf16();
    }
    s.len()
}
