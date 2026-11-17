//! Recalling earlier entries in text inputs: the history rows a prompt lists
//! ahead of its own completions, and the Up/Down recall of a bar field.
//! Entries are given newest first; their owner keeps and caps them.

use std::ops::Range;

use crate::matching::{Matcher, Query, styles_from_names};

/// The entries that match `text`, newest first, as indices into `history`
/// with the matched byte ranges of each. An empty `text` matches every entry.
pub fn matching_entries(history: &[String], text: &str) -> Vec<(usize, Vec<Range<usize>>)> {
    let query = Query::new(text, &styles_from_names::<&str>(&[]));
    let mut matcher = Matcher::new();
    history
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| Some((index, matcher.find(&query, entry)?.ranges)))
        .collect()
}

/// Up and Down recall in one bar field: steps through the entries that start
/// with the text typed before recalling began, and returns to that text
/// past the newest entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Recall {
    /// The typed text recalled entries are filtered by, and the text the
    /// field returns to; set while recalling.
    typed: Option<String>,
    /// The index in the history of the entry the field shows.
    position: Option<usize>,
    /// The text the last step put in the field.
    shown: String,
}

impl Recall {
    /// The text for the field after Up, or `None` when no older entry
    /// matches. `text` is the field's current text.
    pub fn previous(&mut self, history: &[String], text: &str) -> Option<String> {
        self.sync(text);
        let typed = self.typed.as_deref().unwrap_or_default();
        let from = self.position.map_or(0, |position| position + 1);
        let found = (from..history.len()).find(|&index| history[index].starts_with(typed))?;
        self.position = Some(found);
        self.shown = history[found].clone();
        Some(self.shown.clone())
    }

    /// The text for the field after Down, or `None` when it is not
    /// recalling.
    pub fn next(&mut self, history: &[String], text: &str) -> Option<String> {
        self.sync(text);
        let position = self.position?;
        let typed = self.typed.clone().unwrap_or_default();
        match (0..position)
            .rev()
            .find(|&index| history[index].starts_with(&typed))
        {
            Some(found) => {
                self.position = Some(found);
                self.shown = history[found].clone();
            }
            None => {
                self.position = None;
                self.shown = typed;
            }
        }
        Some(self.shown.clone())
    }

    /// Start over when the field no longer shows what the last step put in
    /// it, because the user edited it.
    fn sync(&mut self, text: &str) {
        if self.typed.is_none() || text != self.shown {
            self.typed = Some(text.to_string());
            self.position = None;
            self.shown = text.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Vec<String> {
        ["foo bar", "baz", "foo", "fab"]
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn an_empty_text_lists_every_entry_newest_first() {
        let indices: Vec<_> = matching_entries(&history(), "")
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        assert_eq!(indices, [0, 1, 2, 3]);
    }

    #[test]
    fn typed_text_keeps_matching_entries_in_history_order() {
        let found = matching_entries(&history(), "fo");
        assert_eq!(
            found.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(found[0].1.len(), 1);
        assert_eq!(found[0].1[0], 0..2);
    }

    #[test]
    fn up_and_down_step_through_entries_starting_with_the_typed_text() {
        let history = history();
        let mut recall = Recall::default();
        assert_eq!(recall.previous(&history, "fo").as_deref(), Some("foo bar"));
        assert_eq!(recall.previous(&history, "foo bar").as_deref(), Some("foo"));
        assert_eq!(recall.previous(&history, "foo"), None);
        assert_eq!(recall.next(&history, "foo").as_deref(), Some("foo bar"));
        assert_eq!(recall.next(&history, "foo bar").as_deref(), Some("fo"));
        assert_eq!(recall.next(&history, "fo"), None);
    }

    #[test]
    fn an_empty_field_recalls_every_entry() {
        let history = history();
        let mut recall = Recall::default();
        assert_eq!(recall.previous(&history, "").as_deref(), Some("foo bar"));
        assert_eq!(recall.previous(&history, "foo bar").as_deref(), Some("baz"));
        assert_eq!(recall.next(&history, "baz").as_deref(), Some("foo bar"));
        assert_eq!(recall.next(&history, "foo bar").as_deref(), Some(""));
    }

    #[test]
    fn editing_a_recalled_entry_starts_recalling_afresh() {
        let history = history();
        let mut recall = Recall::default();
        recall.previous(&history, "");
        assert_eq!(recall.previous(&history, "ba").as_deref(), Some("baz"));
        assert_eq!(recall.next(&history, "baz").as_deref(), Some("ba"));
    }

    #[test]
    fn an_empty_history_recalls_nothing() {
        let mut recall = Recall::default();
        assert_eq!(recall.previous(&[], "x"), None);
        assert_eq!(recall.next(&[], "x"), None);
    }
}
