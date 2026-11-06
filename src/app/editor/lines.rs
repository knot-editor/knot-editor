//! Line commands on the lines the selection touches: copy, delete, move, and
//! join. Each commits one undo step.

use super::*;

impl Editor {
    /// Duplicate the selected lines below them, moving the selection to the
    /// copy, or above them, leaving it on the copy that now takes their place.
    pub(super) fn copy_lines(&mut self, down: bool, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let lines = self.selected_lines();
        let start = line_start(model, lines.start);
        let end = content_end(model, lines.end - 1);
        let block = model.read(start..end);
        let ending = model.line_ending();
        let state = self.history_view_state(model);
        if down {
            let shift = ending.len() + block.len();
            self.commit_edits(
                vec![(end..end, format!("{ending}{block}"))],
                state.anchor + shift,
                state.caret + shift,
                cx,
            );
        } else {
            self.commit_edits(
                vec![(start..start, format!("{block}{ending}"))],
                state.anchor,
                state.caret,
                cx,
            );
        }
    }

    /// Delete the selected lines with their terminator, leaving the caret in
    /// its column on the line that takes their place.
    pub(super) fn delete_lines(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let lines = self.selected_lines();
        let (range, next) = if lines.end < model.line_count() {
            let range = line_start(model, lines.start)..line_start(model, lines.end);
            (range.clone(), Some((range.start, lines.end)))
        } else if lines.start > 0 {
            let above = lines.start - 1;
            (
                content_end(model, lines.start - 1)..model.len(),
                Some((line_start(model, above), above)),
            )
        } else {
            (0..model.len(), None)
        };
        let caret = next.map_or(0, |(at, line)| {
            at + floor_char_boundary(&line_text(model, line), self.cursor_col)
        });
        self.commit_edits(vec![(range, String::new())], caret, caret, cx);
    }

    /// Swap the selected lines with the line below or above them; nothing at
    /// the document edge. The selection moves with the lines.
    pub(super) fn move_lines(&mut self, down: bool, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let lines = self.selected_lines();
        let start = line_start(model, lines.start);
        let end = content_end(model, lines.end - 1);
        let ending = model.line_ending();
        // A selection ending at column 0 of the next line keeps ending at the
        // start of the line after the moved block.
        let excluded = self
            .selection_range()
            .is_some_and(|(_, end)| end.0 == lines.end)
            .then(|| line_start(model, lines.end));
        let state = self.history_view_state(model);
        let (edits, map): (_, Box<dyn Fn(usize) -> usize>) = if down {
            if lines.end >= model.line_count() {
                return;
            }
            let below = lines.end;
            let text = line_text(model, below);
            let shift = text.len() + ending.len();
            let terminator = if below + 1 < model.line_count() {
                line_start(model, below + 1) - content_end(model, below)
            } else {
                0
            };
            let after_block = end + shift + terminator;
            (
                vec![
                    (start..start, format!("{text}{ending}")),
                    (end..content_end(model, below), String::new()),
                ],
                Box::new(move |offset| {
                    if Some(offset) == excluded {
                        after_block
                    } else {
                        offset + shift
                    }
                }),
            )
        } else {
            if lines.start == 0 {
                return;
            }
            let above = lines.start - 1;
            let text = line_text(model, above);
            let above_start = line_start(model, above);
            let shift = start - above_start;
            let after_block = end - shift + ending.len();
            (
                vec![
                    (above_start..start, String::new()),
                    (end..end, format!("{ending}{text}")),
                ],
                Box::new(move |offset| {
                    if Some(offset) == excluded {
                        after_block
                    } else {
                        offset - shift
                    }
                }),
            )
        };
        let (anchor, caret) = (map(state.anchor), map(state.caret));
        self.commit_edits(edits, anchor, caret, cx);
    }

    /// Join the selected lines, or the caret's line and the next, replacing
    /// each break and the next line's indentation with one space, or with
    /// nothing before a blank line.
    pub(super) fn join_lines(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let (first, last) = match self.selection_range() {
            Some((start, end)) => (start.0, end.0.max(start.0 + 1)),
            None => (self.cursor_line, self.cursor_line + 1),
        };
        let last = last.min(model.line_count() - 1);
        if first >= last {
            return;
        }
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for line in first + 1..=last {
            let previous = line_text(model, line - 1);
            let next = line_text(model, line);
            let trailing = previous.len() - previous.trim_end_matches([' ', '\t']).len();
            let from = (content_end(model, line - 1) - trailing)
                .max(edits.last().map_or(0, |(range, _)| range.end));
            let to = line_start(model, line) + leading_whitespace(&next).len();
            let blank = next.trim().is_empty() || (line == first + 1 && previous.trim().is_empty());
            edits.push((from..to, if blank { "" } else { " " }.into()));
        }
        let state = self.history_view_state(model);
        let (anchor, caret) = if self.has_selection() {
            (
                map_offset(&edits, state.anchor, true),
                map_offset(&edits, state.caret, true),
            )
        } else {
            let join = edits[0].0.start;
            (join, join)
        };
        self.commit_edits(edits, anchor, caret, cx);
    }
}

fn line_start(model: &BufferModel, line: usize) -> usize {
    model.line_range(line).start
}

/// The offset where `line`'s content ends, before its terminator.
fn content_end(model: &BufferModel, line: usize) -> usize {
    content_range(model, line).end
}

/// The largest character boundary of `text` at or before `col`.
fn floor_char_boundary(text: &str, col: usize) -> usize {
    let mut col = col.min(text.len());
    while !text.is_char_boundary(col) {
        col -= 1;
    }
    col
}
