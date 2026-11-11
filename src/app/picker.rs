//! The native picker shared by the command palette, native pickers, and
//! extension pickers.
//!
//! A picker owns its query field, matching, highlighting, selection, and
//! rows. Items are matched with the shared [`knot_model::matching`] matcher
//! in the styles of the `completion.styles` setting, in tiers: label matches
//! rank above description matches, which rank above matches in an item's
//! hidden match text. Within a tier the matcher's order applies, and ties keep
//! the caller's order, as does an empty query. Events name items by their index in
//! the list the picker was created with. The picker reports the highlighted
//! item whenever it changes, so a consumer can preview it.

use crate::app::convert::ThemePaint as _;
use std::ops::Range;

use gpui::{prelude::*, *};

use super::icons::{AppIcon, ref_definition};
use super::keys::Keystroke;
use knot_model::matching::{Matcher, Query, styles_from_names};

use super::{
    text_input::{TextInput, TextInputEvent},
    theme::{color, theme},
};

const PANEL_WIDTH: f32 = 560.;
const PANEL_MAX_HEIGHT: f32 = 420.;
const LINE_HEIGHT: f32 = 22.;
const ROW_PADDING: f32 = 6.;
const PAGE_ROWS: isize = 10;

/// One row offered to a picker.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PickerEntry {
    pub(crate) label: String,
    /// Muted text beside the label.
    pub(crate) description: Option<String>,
    /// A muted second line.
    pub(crate) detail: Option<String>,
    /// Right-aligned text, such as a keybinding.
    pub(crate) hint: Option<String>,
    /// Painted before the label.
    pub(crate) icon: Option<knot_protocol::IconRef>,
    /// Matched after the label and description but never displayed.
    pub(crate) match_text: Option<String>,
    /// Names the row to an owner that supplies the rows.
    pub(crate) id: Option<String>,
    /// Byte ranges of the label and description a supplied row highlights.
    pub(crate) label_matches: Vec<Range<usize>>,
    pub(crate) description_matches: Vec<Range<usize>>,
}

impl PickerEntry {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PickerEvent {
    /// The highlighted entry changed; `None` when nothing matches.
    Highlighted(Option<usize>),
    Confirmed(usize),
    Dismissed,
    /// A picker whose rows are supplied: the query changed and the owner
    /// should answer with [`Picker::supply`].
    Queried(String),
}

/// One visible row: the entry index and its highlighted byte ranges.
#[derive(Clone, Debug, PartialEq)]
struct PickerMatch {
    entry: usize,
    label: Vec<Range<usize>>,
    description: Vec<Range<usize>>,
}

pub(crate) struct Picker {
    title: Option<SharedString>,
    entries: Vec<PickerEntry>,
    matches: Vec<PickerMatch>,
    selected: usize,
    /// The highlighted entry last reported.
    highlighted: Option<usize>,
    input: Entity<TextInput>,
    scroll: UniformListScrollHandle,
    matcher: Matcher,
    key_context: &'static str,
    /// Whether the owner supplies the rows for each query, shown as given,
    /// instead of the picker matching its entries.
    supplied: bool,
    /// The query the supplied rows answer; `None` before the first answer.
    answered: Option<String>,
    /// Enter was pressed on rows that did not answer the field. The field is
    /// frozen and the highlighted row is confirmed once they do.
    confirm_pending: bool,
    _input_subscription: Subscription,
}

impl Picker {
    pub(crate) fn new(
        title: Option<String>,
        entries: Vec<PickerEntry>,
        key_context: &'static str,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(title, entries, key_context, false, cx)
    }

    /// A picker with no rows until its owner answers the query it reports.
    pub(crate) fn supplied(
        title: Option<String>,
        key_context: &'static str,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(title, Vec::new(), key_context, true, cx)
    }

    fn build(
        title: Option<String>,
        entries: Vec<PickerEntry>,
        key_context: &'static str,
        supplied: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("", cx));
        let subscription = cx.subscribe(
            &input,
            |this: &mut Self, _input, _event: &TextInputEvent, cx| {
                let query = this.input.read(cx).text().to_string();
                if this.supplied {
                    cx.emit(PickerEvent::Queried(query));
                    return;
                }
                this.refilter(&query, cx);
                this.report_highlight(cx);
                cx.notify();
            },
        );
        let mut this = Self {
            title: title.map(Into::into),
            entries,
            matches: Vec::new(),
            selected: 0,
            highlighted: None,
            input,
            scroll: UniformListScrollHandle::new(),
            matcher: Matcher::new(),
            key_context,
            supplied,
            answered: None,
            confirm_pending: false,
            _input_subscription: subscription,
        };
        this.refilter("", cx);
        this.highlighted = this.selected_entry();
        this
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle()
    }

    /// The entry index of the selected row.
    pub(crate) fn selected_entry(&self) -> Option<usize> {
        self.matches.get(self.selected).map(|found| found.entry)
    }

    /// Entry indices of the visible rows, best first.
    #[cfg(test)]
    pub(crate) fn visible_entries(&self) -> Vec<usize> {
        self.matches.iter().map(|found| found.entry).collect()
    }

    /// Labels of the visible rows, best first.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn visible_labels(&self) -> Vec<String> {
        self.matches
            .iter()
            .map(|found| self.entries[found.entry].label.clone())
            .collect()
    }

    pub(crate) fn entry(&self, index: usize) -> &PickerEntry {
        &self.entries[index]
    }

    #[cfg(test)]
    pub(crate) fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text(query, cx));
        self.refilter(query, cx);
        self.report_highlight(cx);
        cx.notify();
    }

    fn report_highlight(&mut self, cx: &mut Context<Self>) {
        let highlighted = self.selected_entry();
        if highlighted != self.highlighted {
            self.highlighted = highlighted;
            cx.emit(PickerEvent::Highlighted(highlighted));
        }
    }

    /// The current query.
    pub(crate) fn query(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    /// Show `entries` in the given order as the rows for `query`, unless the
    /// field has changed since. The highlight is reported even when it keeps
    /// its index, because it may now name another row.
    pub(crate) fn supply(
        &mut self,
        query: &str,
        entries: Vec<PickerEntry>,
        cx: &mut Context<Self>,
    ) {
        if !self.supplied || self.query(cx) != query {
            return;
        }
        self.matches = entries
            .iter()
            .enumerate()
            .map(|(entry, item)| PickerMatch {
                entry,
                label: item.label_matches.clone(),
                description: item.description_matches.clone(),
            })
            .collect();
        self.entries = entries;
        self.answered = Some(query.to_owned());
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.highlighted = self.selected_entry();
        cx.emit(PickerEvent::Highlighted(self.highlighted));
        cx.notify();
        if std::mem::take(&mut self.confirm_pending) {
            self.confirm(cx);
        }
    }

    fn refilter(&mut self, query: &str, cx: &App) {
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        if self.supplied {
            return;
        }
        if query.trim().is_empty() {
            self.matches = (0..self.entries.len())
                .map(|entry| PickerMatch {
                    entry,
                    label: Vec::new(),
                    description: Vec::new(),
                })
                .collect();
            return;
        }
        let styles = styles_from_names(&crate::app::settings::strings(
            crate::app::settings::COMPLETION_STYLES,
            cx,
        ));
        let query = Query::new(query, &styles);
        let mut scored = Vec::new();
        for (entry, item) in self.entries.iter().enumerate() {
            let fields = [
                Some(item.label.as_str()),
                item.description.as_deref(),
                item.match_text.as_deref(),
            ];
            let found = fields
                .iter()
                .enumerate()
                .find_map(|(tier, text)| Some((tier, self.matcher.find(&query, (*text)?)?)));
            if let Some((tier, found)) = found {
                let mut row = PickerMatch {
                    entry,
                    label: Vec::new(),
                    description: Vec::new(),
                };
                match tier {
                    0 => row.label = found.ranges,
                    1 => row.description = found.ranges,
                    _ => {}
                }
                scored.push((tier, found.style, std::cmp::Reverse(found.score), row));
            }
        }
        // A stable sort keeps the caller's order among equal scores.
        scored.sort_by_key(|(tier, style, score, _)| (*tier, *style, *score));
        self.matches = scored.into_iter().map(|(.., row)| row).collect();
    }

    pub(crate) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.matches.len();
        self.selected = if count == 0 {
            0
        } else {
            self.selected
                .saturating_add_signed(delta)
                .min(count.saturating_sub(1))
        };
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Center);
        self.report_highlight(cx);
        cx.notify();
    }

    pub(crate) fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.supplied && self.answered.as_deref() != Some(self.query(cx).as_str()) {
            self.confirm_pending = true;
            return;
        }
        if let Some(entry) = self.selected_entry() {
            cx.emit(PickerEvent::Confirmed(entry));
        }
    }

    /// Handle a key the keymap left unclaimed, the query field first; false
    /// when the picker ignores it.
    pub(crate) fn key_down(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.confirm_pending {
            if keystroke.key.eq_ignore_ascii_case("escape") {
                cx.emit(PickerEvent::Dismissed);
            }
            return true;
        }
        if self
            .input
            .update(cx, |input, cx| input.key_down(keystroke, cx))
        {
            return true;
        }
        match keystroke.key.to_lowercase().as_str() {
            "escape" => cx.emit(PickerEvent::Dismissed),
            "enter" => self.confirm(cx),
            "up" => self.move_selection(-1, cx),
            "down" => self.move_selection(1, cx),
            "pageup" => self.move_selection(-PAGE_ROWS, cx),
            "pagedown" => self.move_selection(PAGE_ROWS, cx),
            _ => return false,
        }
        true
    }

    /// Type `text` into the query.
    pub(crate) fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.confirm_pending {
            return;
        }
        self.input
            .update(cx, |input, cx| input.insert_text(None, text, cx));
    }

    #[cfg(test)]
    fn highlights(&self, entry: usize) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
        self.matches
            .iter()
            .find(|found| found.entry == entry)
            .map(|found| (found.label.clone(), found.description.clone()))
            .unwrap_or_default()
    }
}

impl EventEmitter<PickerEvent> for Picker {}

impl Focusable for Picker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle()
    }
}

pub(crate) fn highlighted(text: String, ranges: &[Range<usize>], color: Hsla) -> StyledText {
    let style = HighlightStyle {
        color: Some(color),
        font_weight: Some(FontWeight::BOLD),
        ..HighlightStyle::default()
    };
    StyledText::new(text).with_highlights(ranges.iter().map(|range| (range.clone(), style)))
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let has_detail = self.entries.iter().any(|entry| entry.detail.is_some());
        let zoom = crate::app::zoom::factor(cx);
        let row_height = (LINE_HEIGHT * if has_detail { 2. } else { 1. } + ROW_PADDING) * zoom;
        let rows = self.matches.len();
        let list_height = (rows as f32 * row_height).min(PANEL_MAX_HEIGHT * zoom);
        let list = uniform_list("picker-rows", rows, move |range, _, cx| {
            let picker = entity.read(cx);
            let theme = theme(cx);
            let muted = theme.paint_color(color::TEXT_MUTED);
            let matched = theme.paint_color(color::LIST_MATCH_FOREGROUND);
            range
                .map(|index| {
                    let found = &picker.matches[index];
                    let entry = &picker.entries[found.entry];
                    let is_selected = index == picker.selected;
                    let confirmed = found.entry;
                    let entity = entity.clone();
                    let line =
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .when_some(entry.icon.as_ref(), |line, icon| {
                                line.child(
                                    div()
                                        .debug_selector(move || format!("picker-icon-{index}"))
                                        .child(AppIcon::new(ref_definition(icon, false, cx))),
                                )
                            })
                            .child(div().flex_shrink_0().child(highlighted(
                                entry.label.clone(),
                                &found.label,
                                matched,
                            )))
                            .when_some(entry.description.clone(), |line, description| {
                                line.child(
                                    div().flex_1().truncate().text_sm().text_color(muted).child(
                                        highlighted(description, &found.description, matched),
                                    ),
                                )
                            })
                            .when(entry.description.is_none(), |line| {
                                line.child(div().flex_1())
                            })
                            .when_some(entry.hint.clone(), |line, hint| {
                                line.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_sm()
                                        .text_color(muted)
                                        .child(hint),
                                )
                            });
                    div()
                        .id(("picker-row", index))
                        .h(px(row_height))
                        .px_3()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .overflow_hidden()
                        .when(is_selected, |row| {
                            row.bg(theme.paint_color(color::LIST_SELECTED_BACKGROUND))
                                .text_color(theme.paint_color(color::LIST_SELECTED_FOREGROUND))
                        })
                        .when(!is_selected, |row| {
                            row.hover(|style| {
                                style.bg(theme.paint_color(color::LIST_HOVER_BACKGROUND))
                            })
                        })
                        .child(line)
                        .when_some(entry.detail.clone(), |row, detail| {
                            row.child(div().truncate().text_sm().text_color(muted).child(detail))
                        })
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |_, cx| cx.emit(PickerEvent::Confirmed(confirmed)));
                        })
                })
                .collect()
        })
        .track_scroll(&self.scroll)
        .h(px(list_height));

        div()
            .id("picker")
            .w(px(PANEL_WIDTH * zoom))
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(theme(cx).paint_color(color::POPOVER_BORDER))
            .bg(theme(cx).paint_color(color::POPOVER_BACKGROUND))
            .text_color(theme(cx).paint_color(color::TEXT))
            .shadow_lg()
            .key_context(self.key_context)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(self.title.clone(), |panel, title| {
                panel.child(div().px_3().pt_2().text_sm().child(title))
            })
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme(cx).paint_color(color::BORDER))
                    .child(self.input.clone()),
            )
            .when(rows == 0, |panel| {
                panel.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_color(theme(cx).paint_color(color::TEXT_MUTED))
                        .child("No matching items"),
                )
            })
            .child(list)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::app::keys::Keystroke;
    use gpui::{AppContext as _, Entity, TestAppContext};

    use super::{Picker, PickerEntry, PickerEvent};

    fn entry(label: &str, description: Option<&str>) -> PickerEntry {
        PickerEntry {
            description: description.map(str::to_owned),
            ..PickerEntry::new(label)
        }
    }

    fn picker(entries: Vec<PickerEntry>, cx: &mut TestAppContext) -> Entity<Picker> {
        cx.new(|cx| Picker::new(Some("Pick".into()), entries, "picker", cx))
    }

    fn labels(picker: &Picker) -> Vec<&str> {
        picker
            .visible_entries()
            .into_iter()
            .map(|entry| picker.entries[entry].label.as_str())
            .collect()
    }

    const COMMANDS: &[&str] = &[
        "Close Tab",
        "Close Window",
        "Copy",
        "Cut",
        "New Document",
        "New Terminal",
        "New Window",
        "Open…",
        "Paste",
        "Recover Unsaved Documents…",
        "Restart Extension…",
        "Save",
        "Save As…",
        "Set Language Mode…",
        "Show Command Palette",
        "Split Down",
        "Split Right",
        "Toggle Terminal Mouse Reporting",
    ];

    #[gpui::test]
    fn ranks_the_expected_command_first(cx: &mut TestAppContext) {
        let picker = picker(
            COMMANDS.iter().map(|label| entry(label, None)).collect(),
            cx,
        );
        for (query, expected) in [
            ("save", "Save"),
            ("saveas", "Save As…"),
            ("sa a", "Save As…"),
            ("nt", "New Terminal"),
            ("new term", "New Terminal"),
            ("nw", "New Window"),
            ("copy", "Copy"),
            ("spr", "Split Right"),
            ("sd", "Split Down"),
            ("lang", "Set Language Mode…"),
            ("palette", "Show Command Palette"),
            ("rest", "Restart Extension…"),
            ("tmr", "Toggle Terminal Mouse Reporting"),
        ] {
            picker.update(cx, |picker, cx| {
                picker.set_query(query, cx);
                assert_eq!(labels(picker).first(), Some(&expected), "query {query:?}");
            });
        }
    }

    #[gpui::test]
    fn the_completion_styles_setting_selects_how_rows_match(cx: &mut TestAppContext) {
        let picker = picker(
            ["set", "xset", "reset", "toggle word"]
                .iter()
                .map(|label| entry(label, None))
                .collect(),
            cx,
        );
        let query = |query: &str, styles: serde_json::Value, cx: &mut TestAppContext| {
            cx.update(|cx| {
                cx.default_global::<crate::app::settings::ApplicationSettings>()
                    .0
                    .set(
                        crate::app::settings::COMPLETION_STYLES,
                        styles,
                        Default::default(),
                    )
                    .unwrap();
            });
            picker.update(cx, |picker, cx| {
                picker.set_query(query, cx);
                labels(picker)
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(query("set", serde_json::json!(["prefix"]), cx), ["set"]);
        assert_eq!(
            query("set", serde_json::json!(["substring"]), cx),
            ["set", "xset", "reset"]
        );
        assert_eq!(
            query("word tog", serde_json::json!(["orderless"]), cx),
            ["toggle word"]
        );
        assert_eq!(
            query("set", serde_json::json!(["prefix", "substring"]), cx),
            ["set", "xset", "reset"]
        );
    }

    #[gpui::test]
    fn ranks_paths_by_their_file_name(cx: &mut TestAppContext) {
        let paths = [
            "src/app/product/commands.rs",
            "src/app/commands/catalog.rs",
            "src/app/command_palette.rs",
            "docs/architecture/commands.md",
        ];
        let picker = picker(paths.iter().map(|path| entry(path, None)).collect(), cx);
        picker.update(cx, |picker, cx| {
            picker.set_query("catalog", cx);
            assert_eq!(labels(picker), ["src/app/commands/catalog.rs"]);
            picker.set_query("cmdpal", cx);
            assert_eq!(labels(picker).first(), Some(&"src/app/command_palette.rs"));
        });
    }

    #[gpui::test]
    fn empty_query_and_ties_keep_the_callers_order(cx: &mut TestAppContext) {
        let picker = picker(
            vec![
                entry("beta", None),
                entry("alpha", None),
                entry("alpha", None),
            ],
            cx,
        );
        picker.update(cx, |picker, cx| {
            assert_eq!(picker.visible_entries(), [0, 1, 2]);
            picker.set_query("alpha", cx);
            assert_eq!(picker.visible_entries(), [1, 2]);
            picker.set_query("   ", cx);
            assert_eq!(picker.visible_entries(), [0, 1, 2]);
        });
    }

    #[gpui::test]
    fn label_matches_rank_above_description_and_hidden_text(cx: &mut TestAppContext) {
        let hidden = PickerEntry {
            match_text: Some("files.recent".into()),
            ..PickerEntry::new("Open Recent")
        };
        let picker = picker(
            vec![
                hidden,
                entry("Projects", Some("recent folders")),
                entry("Recent Files", None),
            ],
            cx,
        );
        picker.update(cx, |picker, cx| {
            picker.set_query("recent", cx);
            // Label matches rank above the description match, and the prefix
            // match leads the other label match.
            assert_eq!(labels(picker), ["Recent Files", "Open Recent", "Projects"]);
            picker.set_query("files.rec", cx);
            assert_eq!(labels(picker), ["Open Recent"]);
            assert_eq!(picker.highlights(0), (Vec::new(), Vec::new()));
        });
    }

    #[gpui::test]
    #[allow(clippy::single_range_in_vec_init)]
    fn highlights_cover_matched_bytes_in_multibyte_text(cx: &mut TestAppContext) {
        let picker = picker(vec![entry("Café crème", Some("naïve 日本語"))], cx);
        picker.update(cx, |picker, cx| {
            picker.set_query("cafe", cx);
            assert_eq!(picker.highlights(0).0, [0..5]);
            picker.set_query("cre", cx);
            assert_eq!(picker.highlights(0).0, [6..10]);
            picker.set_query("日本", cx);
            let (label, description) = picker.highlights(0);
            assert!(label.is_empty());
            assert_eq!(description, [7..13]);
        });
    }

    #[gpui::test]
    #[allow(clippy::single_range_in_vec_init)]
    fn supplied_rows_keep_their_order_and_ranges_and_stale_answers_drop(cx: &mut TestAppContext) {
        let picker = cx.new(|cx| Picker::supplied(Some("Pick".into()), "picker", cx));
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&picker, move |_, event, _| {
                observed.borrow_mut().push(event.clone())
            })
        });
        let row = |label: &str, matches: Vec<std::ops::Range<usize>>| PickerEntry {
            id: Some(label.to_owned()),
            label_matches: matches,
            ..PickerEntry::new(label)
        };
        picker.update(cx, |picker, cx| {
            picker.supply("", vec![row("zed", vec![]), row("alpha", vec![0..2])], cx);
            assert_eq!(labels(picker), ["zed", "alpha"]);
            assert_eq!(picker.highlights(1).0, [0..2]);
        });
        // Typing reports the query and keeps the rows until it is answered.
        picker.update(cx, |picker, cx| {
            picker.set_query("a", cx);
            assert_eq!(labels(picker), ["zed", "alpha"]);
        });
        picker.update(cx, |picker, cx| {
            // An answer to an earlier query is dropped.
            picker.supply("", vec![row("stale", vec![])], cx);
            assert_eq!(labels(picker), ["zed", "alpha"]);
            // The matcher plays no part: a row the query cannot match shows.
            picker.supply("a", vec![row("beta", vec![]), row("omega", vec![])], cx);
            assert_eq!(labels(picker), ["beta", "omega"]);
            picker.move_selection(1, cx);
            picker.confirm(cx);
        });
        assert_eq!(
            *events.borrow(),
            [
                PickerEvent::Highlighted(Some(0)),
                PickerEvent::Queried("a".into()),
                PickerEvent::Highlighted(Some(0)),
                PickerEvent::Highlighted(Some(1)),
                PickerEvent::Confirmed(1),
            ]
        );
        assert_eq!(
            picker.read_with(cx, |picker, _| picker.entry(1).id.clone()),
            Some("omega".into())
        );
    }

    #[gpui::test]
    fn enter_on_rows_for_older_text_waits_for_the_answer(cx: &mut TestAppContext) {
        let picker = cx.new(|cx| Picker::supplied(Some("Pick".into()), "picker", cx));
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&picker, move |_, event, _| {
                observed.borrow_mut().push(event.clone())
            })
        });
        let row = |label: &str| PickerEntry {
            id: Some(label.to_owned()),
            ..PickerEntry::new(label)
        };
        picker.update(cx, |picker, cx| {
            picker.supply("", vec![row("old")], cx);
            picker.insert_text("ab", cx);
            picker.confirm(cx);
            // Typing after the held Enter does not reach the field.
            picker.insert_text("zz", cx);
            assert_eq!(picker.query(cx), "ab");
        });
        picker.update(cx, |picker, cx| {
            // An answer to an older query does not release it.
            picker.supply("", vec![row("stale")], cx);
            picker.supply("ab", vec![row("new")], cx);
        });
        assert!(events.borrow().contains(&PickerEvent::Confirmed(0)));
        assert_eq!(labels_of(&picker, cx), ["new"]);
    }

    fn labels_of(picker: &Entity<Picker>, cx: &mut TestAppContext) -> Vec<String> {
        picker.read_with(cx, |picker, _| picker.visible_labels())
    }

    #[gpui::test]
    fn confirms_the_selection_and_blocks_without_matches(cx: &mut TestAppContext) {
        let picker = picker(
            vec![entry("Alpha", Some("first")), entry("Beta", Some("second"))],
            cx,
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&picker, move |_, event, _| {
                observed.borrow_mut().push(event.clone())
            })
        });
        picker.update(cx, |picker, cx| {
            picker.move_selection(10, cx);
            assert_eq!(picker.selected_entry(), Some(1));
            picker.set_query("second", cx);
            assert_eq!(picker.selected_entry(), Some(1));
            picker.confirm(cx);
            picker.set_query("zzz", cx);
            assert_eq!(picker.selected_entry(), None);
            picker.confirm(cx);
            picker.move_selection(1, cx);
            assert_eq!(picker.selected, 0);
        });
        assert_eq!(
            *events.borrow(),
            [
                PickerEvent::Highlighted(Some(1)),
                PickerEvent::Confirmed(1),
                PickerEvent::Highlighted(None),
            ]
        );
    }

    #[gpui::test]
    fn keys_edit_the_query_and_move_and_confirm_the_selection(cx: &mut TestAppContext) {
        let picker = picker(
            vec![
                entry("Alpha", None),
                entry("Beta", None),
                entry("Gamma", None),
            ],
            cx,
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&picker, move |_, event, _| {
                observed.borrow_mut().push(event.clone())
            })
        });
        let press = |key: &str, cx: &mut TestAppContext| {
            let keystroke = Keystroke::parse(key).unwrap();
            picker.update(cx, |picker, cx| picker.key_down(&keystroke, cx))
        };
        assert!(press("down", cx));
        assert!(press("up", cx));
        picker.update(cx, |picker, cx| picker.insert_text("mx", cx));
        assert!(press("backspace", cx));
        cx.read(|cx| assert_eq!(labels(picker.read(cx)), ["Gamma"]));
        assert!(press("enter", cx));
        assert!(!press("a", cx));
        assert!(press("escape", cx));
        assert_eq!(
            *events.borrow(),
            [
                PickerEvent::Highlighted(Some(1)),
                PickerEvent::Highlighted(Some(0)),
                PickerEvent::Highlighted(None),
                PickerEvent::Highlighted(Some(2)),
                PickerEvent::Confirmed(2),
                PickerEvent::Dismissed,
            ]
        );
    }
}
