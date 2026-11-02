//! Bracket matching: the highlight of the bracket at the caret and its
//! partner, and Jump to Matching Bracket.

use super::*;

/// The brackets matched at a caret, valid while the syntax generation and
/// the caret stay the same.
pub(super) struct BracketCache {
    generation: u64,
    caret: usize,
    matched: Option<[Range<usize>; 2]>,
}

impl EditorView {
    /// The byte ranges of the bracket touching the caret and its partner, as
    /// the highlight paints them.
    pub(super) fn matched_brackets(&self, model: &BufferModel) -> Option<[Range<usize>; 2]> {
        let generation = model.syntax_generation()?;
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        let mut cache = self.bracket_cache.borrow_mut();
        if let Some(cache) = cache
            .as_ref()
            .filter(|cache| cache.generation == generation && cache.caret == caret)
        {
            return cache.matched.clone();
        }
        let matched = model
            .bracket_at(caret)
            .map(|(bracket, partner)| [bracket, partner]);
        *cache = Some(BracketCache {
            generation,
            caret,
            matched: matched.clone(),
        });
        matched
    }

    /// Move the caret before the partner of the bracket it touches, or
    /// before the closer of the innermost pair around it, as VS Code's Go to
    /// Bracket does. Clears the selection.
    pub(super) fn jump_to_matching_bracket(&mut self, cx: &mut Context<Self>) {
        self.break_history_group(cx);
        let model = self.model.read(cx);
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        let target = match model.bracket_at(caret) {
            // Between an adjacent pair, go before the opener.
            Some((bracket, partner)) if partner.start == caret => bracket.start.min(partner.start),
            Some((_, partner)) => partner.start,
            None => match model.enclosing_brackets(caret) {
                Some(pair) => pair.close.start,
                None => return,
            },
        };
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, target);
        self.has_selection = false;
        self.marked_range_utf16 = None;
        self.preferred_x = None;
        self.finish_position_change(cx);
    }
}
