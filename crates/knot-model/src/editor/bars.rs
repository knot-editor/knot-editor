//! Editor bars: non-modal strips extensions attach to the top of an editor
//! view, described as data. Each bar belongs to the lifecycle that opened it
//! and is named by an ID that lifecycle chose; bars stack in opening order.
//! Field text lives in the owner's text fields; this state keeps what each
//! bar shows and which field, if any, has the editor's focus instead of its
//! text. Presentation state, never recorded.

use knot_protocol::{CommandArgumentValue, ExtensionKey, IconRef};

/// A single-line text field of a bar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BarField {
    /// Names the field within its bar.
    pub id: String,
    /// Shown while the field is empty.
    pub placeholder: String,
}

/// The command a click on a toggle or button runs.
#[derive(Clone, Debug, PartialEq)]
pub struct BarCommand {
    pub name: String,
    pub arguments: CommandArgumentValue,
}

/// An icon button of a bar that runs a command.
#[derive(Clone, Debug, PartialEq)]
pub struct BarButton {
    /// Names the button within its bar; unique among the bar's controls.
    pub id: String,
    pub icon: IconRef,
    pub tooltip: Option<String>,
    pub command: BarCommand,
}

/// An icon button of a bar that shows whether an option is on. The extension
/// flips `active` by updating the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct BarToggle {
    /// Names the toggle within its bar; unique among the bar's controls.
    pub id: String,
    pub icon: IconRef,
    pub tooltip: Option<String>,
    pub active: bool,
    pub command: BarCommand,
}

/// What a bar shows: fields, then toggles, a line of text, and buttons.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarSpec {
    pub fields: Vec<BarField>,
    pub toggles: Vec<BarToggle>,
    pub buttons: Vec<BarButton>,
    /// A line of text after the fields.
    pub message: Option<String>,
}

impl BarSpec {
    /// Why the spec is invalid: a field, toggle, or button ID is empty, or
    /// repeated among the fields or among the toggles and buttons.
    pub fn problem(&self) -> Option<&'static str> {
        let fields = self.fields.iter().map(|field| field.id.as_str());
        let controls = self
            .toggles
            .iter()
            .map(|toggle| toggle.id.as_str())
            .chain(self.buttons.iter().map(|button| button.id.as_str()));
        for (ids, empty, repeated) in [
            (
                fields.collect::<Vec<_>>(),
                "bar field id must not be empty",
                "bar field ids must be unique",
            ),
            (
                controls.collect::<Vec<_>>(),
                "bar toggle and button ids must not be empty",
                "bar toggle and button ids must be unique",
            ),
        ] {
            for (index, id) in ids.iter().enumerate() {
                if id.is_empty() {
                    return Some(empty);
                }
                if ids[..index].contains(id) {
                    return Some(repeated);
                }
            }
        }
        None
    }
}

/// One open bar.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorBar {
    pub owner: ExtensionKey,
    pub id: String,
    pub spec: BarSpec,
}

/// Names one field of one bar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BarFieldRef {
    pub owner: ExtensionKey,
    pub bar: String,
    pub field: String,
}

impl BarFieldRef {
    pub fn new(owner: ExtensionKey, bar: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            owner,
            bar: bar.into(),
            field: field.into(),
        }
    }
}

/// The bars of one editor and the field that has its focus.
#[derive(Debug, Default)]
pub struct EditorBars {
    bars: Vec<EditorBar>,
    focus: Option<BarFieldRef>,
}

impl EditorBars {
    /// The open bars, top to bottom.
    pub fn bars(&self) -> &[EditorBar] {
        &self.bars
    }

    pub fn get(&self, owner: ExtensionKey, id: &str) -> Option<&EditorBar> {
        self.bars
            .iter()
            .find(|bar| bar.owner == owner && bar.id == id)
    }

    /// Every field of every bar, top to bottom.
    pub fn fields(&self) -> impl Iterator<Item = BarFieldRef> + '_ {
        self.bars.iter().flat_map(|bar| {
            bar.spec
                .fields
                .iter()
                .map(|field| BarFieldRef::new(bar.owner, bar.id.clone(), field.id.clone()))
        })
    }

    /// Open `owner`'s bar `id` below the others, or replace what it shows in
    /// place. Focus leaves a field the new spec drops.
    pub fn open(&mut self, owner: ExtensionKey, id: &str, spec: BarSpec) {
        match self
            .bars
            .iter_mut()
            .find(|bar| bar.owner == owner && bar.id == id)
        {
            Some(bar) => bar.spec = spec,
            None => self.bars.push(EditorBar {
                owner,
                id: id.to_owned(),
                spec,
            }),
        }
        self.drop_stale_focus();
    }

    /// Close `owner`'s bar `id`; false when it has none. Focus in it returns
    /// to the text.
    pub fn close(&mut self, owner: ExtensionKey, id: &str) -> bool {
        let count = self.bars.len();
        self.bars.retain(|bar| bar.owner != owner || bar.id != id);
        self.drop_stale_focus();
        self.bars.len() != count
    }

    /// Close every bar `owner` opened.
    pub fn remove_owner(&mut self, owner: ExtensionKey) {
        self.bars.retain(|bar| bar.owner != owner);
        self.drop_stale_focus();
    }

    /// Give `field` the editor's focus; false when no open bar has it.
    pub fn focus_field(&mut self, field: BarFieldRef) -> bool {
        if !self.contains(&field) {
            return false;
        }
        self.focus = Some(field);
        true
    }

    /// Move focus to the next or previous field across all bars, top to
    /// bottom, wrapping. Does nothing without a focused field.
    pub fn focus_adjacent(&mut self, forward: bool) {
        let Some(focus) = &self.focus else {
            return;
        };
        let fields = self.fields().collect::<Vec<_>>();
        let Some(index) = fields.iter().position(|field| field == focus) else {
            return;
        };
        let count = fields.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        self.focus = Some(fields[next].clone());
    }

    /// Return focus to the text.
    pub fn focus_text(&mut self) {
        self.focus = None;
    }

    /// The field that has the editor's focus; `None` while the text has it.
    pub fn focused(&self) -> Option<&BarFieldRef> {
        self.focus.as_ref()
    }

    /// The bar whose field has the editor's focus.
    pub fn focused_bar(&self) -> Option<&EditorBar> {
        let focus = self.focus.as_ref()?;
        self.get(focus.owner, &focus.bar)
    }

    fn contains(&self, field: &BarFieldRef) -> bool {
        self.get(field.owner, &field.bar)
            .is_some_and(|bar| bar.spec.fields.iter().any(|f| f.id == field.field))
    }

    fn drop_stale_focus(&mut self) {
        if self
            .focus
            .as_ref()
            .is_some_and(|focus| !self.contains(focus))
        {
            self.focus = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use knot_protocol::{ExtensionId, ExtensionLifecycleId};

    use super::*;

    fn owner(id: u64) -> ExtensionKey {
        ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1))
    }

    fn spec(fields: &[&str]) -> BarSpec {
        BarSpec {
            fields: fields
                .iter()
                .map(|id| BarField {
                    id: (*id).to_owned(),
                    placeholder: String::new(),
                })
                .collect(),
            ..BarSpec::default()
        }
    }

    fn order(bars: &EditorBars) -> Vec<(ExtensionKey, &str)> {
        bars.bars()
            .iter()
            .map(|bar| (bar.owner, bar.id.as_str()))
            .collect()
    }

    #[test]
    fn bars_stack_in_opening_order_and_update_in_place() {
        let mut bars = EditorBars::default();
        bars.open(owner(1), "find", spec(&["query"]));
        bars.open(owner(2), "find", spec(&["query"]));
        bars.open(owner(1), "other", spec(&[]));
        bars.open(
            owner(1),
            "find",
            BarSpec {
                message: Some("3 of 17".into()),
                ..spec(&["query"])
            },
        );
        assert_eq!(
            order(&bars),
            [(owner(1), "find"), (owner(2), "find"), (owner(1), "other")]
        );
        assert_eq!(
            bars.get(owner(1), "find").unwrap().spec.message.as_deref(),
            Some("3 of 17")
        );
        assert!(bars.close(owner(2), "find"));
        assert!(!bars.close(owner(2), "find"));
        bars.remove_owner(owner(1));
        assert!(bars.bars().is_empty());
    }

    #[test]
    fn focus_names_a_live_field_and_returns_to_the_text_when_it_goes() {
        let mut bars = EditorBars::default();
        bars.open(owner(1), "find", spec(&["query", "replace"]));
        bars.open(owner(2), "goto", spec(&["line"]));
        assert!(!bars.focus_field(BarFieldRef::new(owner(2), "find", "query")));
        assert!(!bars.focus_field(BarFieldRef::new(owner(1), "find", "missing")));
        assert!(bars.focus_field(BarFieldRef::new(owner(1), "find", "replace")));
        assert_eq!(bars.focused_bar().unwrap().id, "find");

        // An update keeping the field keeps focus; one dropping it does not.
        bars.open(owner(1), "find", spec(&["replace"]));
        assert!(bars.focused().is_some());
        bars.open(owner(1), "find", spec(&["query"]));
        assert_eq!(bars.focused(), None);

        bars.focus_field(BarFieldRef::new(owner(2), "goto", "line"));
        bars.close(owner(1), "find");
        assert!(bars.focused().is_some());
        bars.remove_owner(owner(2));
        assert_eq!(bars.focused(), None);
    }

    #[test]
    fn adjacent_focus_cycles_across_bars_and_wraps() {
        let mut bars = EditorBars::default();
        bars.open(owner(1), "find", spec(&["query", "replace"]));
        bars.open(owner(2), "goto", spec(&["line"]));
        bars.focus_adjacent(true);
        assert_eq!(bars.focused(), None);

        let at = |bar: &EditorBars| bar.focused().map(|f| f.field.clone()).unwrap();
        bars.focus_field(BarFieldRef::new(owner(1), "find", "query"));
        bars.focus_adjacent(true);
        assert_eq!(at(&bars), "replace");
        bars.focus_adjacent(true);
        assert_eq!(at(&bars), "line");
        bars.focus_adjacent(true);
        assert_eq!(at(&bars), "query");
        bars.focus_adjacent(false);
        assert_eq!(at(&bars), "line");
        bars.focus_adjacent(false);
        assert_eq!(at(&bars), "replace");

        bars.close(owner(2), "goto");
        bars.close(owner(1), "find");
        bars.open(owner(1), "one", spec(&["only"]));
        bars.focus_field(BarFieldRef::new(owner(1), "one", "only"));
        bars.focus_adjacent(true);
        assert_eq!(at(&bars), "only");
    }

    #[test]
    fn field_ids_must_be_present_and_unique() {
        assert_eq!(spec(&["a", "b"]).problem(), None);
        assert!(spec(&[""]).problem().is_some());
        assert!(spec(&["a", "a"]).problem().is_some());

        let command = || BarCommand {
            name: "x".into(),
            arguments: CommandArgumentValue::Null,
        };
        let toggle = |id: &str| BarToggle {
            id: id.into(),
            icon: "case".into(),
            tooltip: None,
            active: false,
            command: command(),
        };
        let button = |id: &str| BarButton {
            id: id.into(),
            icon: "next".into(),
            tooltip: None,
            command: command(),
        };
        let with = |toggles, buttons| BarSpec {
            toggles,
            buttons,
            ..spec(&["a"])
        };
        // Controls share a namespace across toggles and buttons, apart from fields.
        assert_eq!(with(vec![toggle("a")], vec![button("b")]).problem(), None);
        assert!(
            with(vec![toggle("a")], vec![button("a")])
                .problem()
                .is_some()
        );
        assert!(with(vec![toggle("")], vec![]).problem().is_some());
        assert!(
            with(vec![], vec![button("b"), button("b")])
                .problem()
                .is_some()
        );
    }
}
