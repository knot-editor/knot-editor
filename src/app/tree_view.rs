//! Native presentation and interaction state for a semantic extension tree.

use crate::app::convert::ThemePaint as _;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ElementExt as _,
    menu::{PopupMenu, PopupMenuItem},
};

use super::keys::Keystroke;
use super::picker::highlighted;
use knot_model::matching::{MatchStyle, Matcher, Query};

use knot_protocol::{
    Command, CommandArgumentValue, ExtensionId, ExtensionLifecycleId, TreeAction,
    TreeChildrenRequest, TreeChildrenResponse, TreeCollapsibleState, TreeItem, TreeItemKind,
    TreeProviderRegistrationId, ViewId,
};

mod edit;
use edit::TreeEdit;
pub(crate) use edit::{EditSession, EditTarget};

use super::TREE_KEY_CONTEXT;
use super::icons::{AppIcon, icon_definition, ref_definition};
use super::theme::{color, theme};
use knot_model::icons::IconDefinition;

/// An action's face: its icon when it has one, else its label.
fn action_content(action: &TreeAction, cx: &App) -> AnyElement {
    match action.icon.as_ref() {
        Some(icon) => AppIcon::new(ref_definition(icon, false, cx)).into_any_element(),
        None => action.label.clone().into_any_element(),
    }
}

/// An icon-only action names itself in a tooltip.
fn action_tooltip(
    action: &TreeAction,
) -> Option<impl Fn(&mut Window, &mut App) -> AnyView + use<>> {
    action.icon.as_ref()?;
    let label = SharedString::from(action.label.clone());
    Some(move |window: &mut Window, cx: &mut App| {
        gpui_component::tooltip::Tooltip::new(label.clone())
            .text_xs()
            .build(window, cx)
    })
}

pub(crate) static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TREE_GENERATION: AtomicU64 = AtomicU64::new(1);

/// How a row was activated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TreeGesture {
    Click,
    DoubleClick,
    MiddleClick,
    RightClick,
    Enter,
}

impl TreeGesture {
    fn name(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::DoubleClick => "doubleClick",
            Self::MiddleClick => "middleClick",
            Self::RightClick => "rightClick",
            Self::Enter => "enter",
        }
    }

    /// Whether the gesture expands or collapses an expandable row.
    fn toggles(self) -> bool {
        matches!(self, Self::Click | Self::Enter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TreeProviderIdentity {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub registration: TreeProviderRegistrationId,
}

#[derive(Clone)]
pub(crate) enum TreeViewEvent {
    RequestChildren {
        provider: TreeProviderIdentity,
        request: TreeChildrenRequest,
    },
    InvokeCommand {
        provider: TreeProviderIdentity,
        command: String,
        arguments: CommandArgumentValue,
        /// The session showing the tree.
        window: String,
        /// The tree, which must still have focus there.
        view: ViewId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TreeViewRegistrationError {
    WrongView,
    ProviderInUse,
    ProviderNotFound,
}

#[derive(Clone, Debug, Default)]
struct ChildrenState {
    items: Vec<TreeItem>,
    generation: u64,
    loading: bool,
    error: Option<String>,
}

#[derive(Clone)]
enum VisibleRow {
    Item {
        item: Box<TreeItem>,
        parent_id: Option<String>,
        depth: usize,
    },
    /// The new row an edit is typing, first among its parent's children.
    Draft {
        icon: Option<knot_protocol::IconRef>,
        depth: usize,
    },
    Status {
        depth: usize,
        label: SharedString,
        error: bool,
    },
}

/// Keyboard action mode: the row whose actions are being navigated, or frame
/// actions when no row qualifies.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionFocus {
    row: Option<String>,
    index: usize,
}

/// Typeahead over the loaded rows: the typed query, the rows it matches in
/// display order with the label ranges to highlight, and the current match.
struct SpeedSearch {
    query: String,
    matches: Vec<String>,
    ranges: HashMap<String, Vec<std::ops::Range<usize>>>,
    current: usize,
}

/// The character a key types into a search, if it types one.
fn typed_character(keystroke: &Keystroke) -> Option<char> {
    let modifiers = &keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }
    let text = match (&keystroke.key_char, keystroke.key.as_str()) {
        (Some(text), _) => text.clone(),
        (None, "space") => " ".to_owned(),
        (None, key) if key.chars().count() == 1 && modifiers.shift => key.to_uppercase(),
        (None, key) if key.chars().count() == 1 => key.to_owned(),
        _ => return None,
    };
    let mut chars = text.chars();
    let character = chars.next()?;
    (chars.next().is_none() && !character.is_control()).then_some(character)
}

/// A row's ID and its bounds when last painted.
type RowBounds = Option<(String, Bounds<Pixels>)>;

/// The open context menu of a row's actions.
struct ContextMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _subscriptions: [Subscription; 2],
}

/// Foreground-owned cache and presentation state for one native tree surface.
pub(crate) struct TreeView {
    view_id: String,
    window: String,
    instance_id: ViewId,
    provider: Option<TreeProviderIdentity>,
    frame_actions: Vec<TreeAction>,
    children: HashMap<Option<String>, ChildrenState>,
    expanded: HashSet<String>,
    selected: Option<String>,
    /// The ID path of a reveal waiting for rows to load.
    pending_reveal: Option<Vec<String>>,
    search: Option<SpeedSearch>,
    edit: Option<TreeEdit>,
    action_focus: Option<ActionFocus>,
    context_menu: Option<ContextMenu>,
    /// The selected row and its bounds when last painted.
    selected_bounds: Rc<RefCell<RowBounds>>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl TreeView {
    pub(crate) fn kind(&self) -> &str {
        &self.view_id
    }

    pub(crate) fn new(
        view_id: impl Into<String>,
        window: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            view_id: view_id.into(),
            window: window.into(),
            instance_id: ViewId::new(NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed)),
            provider: None,
            frame_actions: Vec::new(),
            children: HashMap::new(),
            expanded: HashSet::new(),
            selected: None,
            pending_reveal: None,
            search: None,
            edit: None,
            action_focus: None,
            context_menu: None,
            selected_bounds: Rc::default(),
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
        }
    }

    pub(crate) fn instance_id(&self) -> ViewId {
        self.instance_id
    }

    pub(crate) fn selected_item_id(&self) -> Option<String> {
        self.selected.clone()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn selected_label(&self) -> Option<String> {
        let selected = self.selected.as_deref()?;
        self.children
            .values()
            .flat_map(|children| &children.items)
            .find(|item| item.id == selected)
            .map(|item| item.label.clone())
    }

    #[cfg(test)]
    pub(crate) fn select_item(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if !self
            .children
            .values()
            .flat_map(|children| &children.items)
            .any(|item| item.id == id)
        {
            return false;
        }
        self.selected = Some(id.to_owned());
        cx.notify();
        true
    }

    pub(crate) fn handle_command(
        &self,
        command: &Command,
        cx: &mut Context<Self>,
    ) -> super::commands::dispatch::CommandClaim {
        use super::commands::dispatch::{ApplicationProductCommands, COPY_COMMAND, CommandClaim};

        if command.name.as_ref() == COPY_COMMAND && self.selected.is_none() {
            return CommandClaim::Declined;
        }
        let catalog = cx
            .global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog();
        let Some(handler) = catalog
            .borrow()
            .resolve_view(self.instance_id, command.name.as_ref())
        else {
            return CommandClaim::Declined;
        };
        CommandClaim::Extension {
            handler,
            view: Some(self.instance_id),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn activate_row_for_test(&mut self, id: &str, cx: &mut Context<Self>) {
        self.activate(id, TreeGesture::Click, cx);
    }

    /// Activate row `id` with the gesture named as the API names it.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn activate_row_as_for_test(
        &mut self,
        id: &str,
        gesture: &str,
        cx: &mut Context<Self>,
    ) {
        let gesture = [
            TreeGesture::Click,
            TreeGesture::DoubleClick,
            TreeGesture::MiddleClick,
            TreeGesture::RightClick,
            TreeGesture::Enter,
        ]
        .into_iter()
        .find(|candidate| candidate.name() == gesture)
        .expect("a known gesture");
        self.activate(id, gesture, cx);
    }

    /// The ID of the first visible row labelled `label`.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn row_id_for_test(&self, label: &str) -> Option<String> {
        self.visible_rows().into_iter().find_map(|row| match row {
            VisibleRow::Item { item, .. } if item.label == label => Some(item.id),
            _ => None,
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn muted_for_test(&self, id: &str) -> bool {
        self.item(id).is_some_and(|item| item.muted)
    }

    /// The ID of the selected row.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn selected_id_for_test(&self) -> Option<String> {
        self.selected.clone()
    }

    #[cfg(test)]
    pub(crate) fn owns_provider(
        &self,
        registration: TreeProviderRegistrationId,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    ) -> bool {
        self.provider.is_some_and(|provider| {
            provider.registration == registration
                && provider.extension == extension
                && provider.lifecycle == lifecycle
        })
    }

    pub(crate) fn register_provider(
        &mut self,
        view_id: &str,
        provider: TreeProviderIdentity,
        frame_actions: Vec<TreeAction>,
        cx: &mut Context<Self>,
    ) -> Result<(), TreeViewRegistrationError> {
        if view_id != self.view_id {
            return Err(TreeViewRegistrationError::WrongView);
        }
        if self.provider.is_some() {
            return Err(TreeViewRegistrationError::ProviderInUse);
        }
        self.provider = Some(provider);
        self.frame_actions = frame_actions;
        self.invalidate(provider.registration, None, cx)?;
        Ok(())
    }

    pub(crate) fn invalidate(
        &mut self,
        registration: TreeProviderRegistrationId,
        parent_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), TreeViewRegistrationError> {
        let provider = self
            .provider
            .filter(|provider| provider.registration == registration)
            .ok_or(TreeViewRegistrationError::ProviderNotFound)?;
        let generation = NEXT_TREE_GENERATION.fetch_add(1, Ordering::Relaxed);
        if parent_id.is_none() {
            // Keep stale rows visible while the refresh is in flight so
            // selection and keyboard navigation stay usable. Group children
            // still refetch when the new root response arrives.
            self.action_focus = None;
        }
        let state = self.children.entry(parent_id.clone()).or_default();
        state.generation = generation;
        state.loading = true;
        state.error = None;
        cx.emit(TreeViewEvent::RequestChildren {
            provider,
            request: TreeChildrenRequest {
                registration,
                window: self.window.clone(),
                view: self.instance_id,
                parent_id,
                generation,
            },
        });
        cx.notify();
        Ok(())
    }

    pub(crate) fn unregister_provider(
        &mut self,
        registration: TreeProviderRegistrationId,
        cx: &mut Context<Self>,
    ) -> Result<(), TreeViewRegistrationError> {
        if self
            .provider
            .is_none_or(|provider| provider.registration != registration)
        {
            return Err(TreeViewRegistrationError::ProviderNotFound);
        }
        self.clear_provider(cx);
        Ok(())
    }

    /// Reload root children for a still-registered provider.
    pub(crate) fn refresh_root(&mut self, cx: &mut Context<Self>) {
        if let Some(provider) = self.provider {
            let _ = self.invalidate(provider.registration, None, cx);
        }
    }

    #[cfg(test)]
    pub(crate) fn is_loading(&self) -> bool {
        self.children.values().any(|state| state.loading)
    }

    #[cfg(test)]
    pub(crate) fn root_generation(&self) -> u64 {
        self.children[&None].generation
    }

    #[cfg(test)]
    pub(crate) fn lifecycle_state(&self) -> (bool, usize, bool, usize) {
        (
            self.provider.is_some(),
            self.children.len(),
            self.is_loading(),
            self.children
                .values()
                .filter(|state| state.error.is_some())
                .count(),
        )
    }

    #[cfg(test)]
    pub(crate) fn root_labels(&self) -> Vec<String> {
        self.children
            .get(&None)
            .map(|state| state.items.iter().map(|item| item.label.clone()).collect())
            .unwrap_or_default()
    }

    pub(crate) fn remove_lifecycle(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        cx: &mut Context<Self>,
    ) {
        if self.provider.is_some_and(|provider| {
            provider.extension == extension && provider.lifecycle == lifecycle
        }) {
            self.clear_provider(cx);
        }
    }

    fn clear_provider(&mut self, cx: &mut Context<Self>) {
        self.provider = None;
        self.frame_actions.clear();
        self.children.clear();
        self.expanded.clear();
        self.selected = None;
        self.pending_reveal = None;
        self.search = None;
        self.edit = None;
        self.action_focus = None;
        cx.notify();
    }

    /// Match the query against the visible rows and move to the best match,
    /// or keep the current one when `keep_current` and it still matches.
    fn refresh_search(&mut self, keep_current: bool) {
        let Some(search) = self.search.take() else {
            return;
        };
        let labels = self
            .visible_rows()
            .into_iter()
            .filter_map(|row| match row {
                VisibleRow::Item { item, .. } => Some((item.id, item.label)),
                VisibleRow::Draft { .. } | VisibleRow::Status { .. } => None,
            })
            .collect::<Vec<_>>();
        let styles = [MatchStyle::Substring, MatchStyle::Flex];
        // Speed search ignores case, so the query is never smart-cased.
        let query = Query::new(&search.query.to_lowercase(), &styles);
        let ranked = Matcher::new().rank(
            &query,
            &labels
                .iter()
                .map(|(_, label)| label.as_str())
                .collect::<Vec<_>>(),
        );
        let best = ranked.first().map(|(index, _)| labels[*index].0.clone());
        let mut found = ranked.iter().map(|(index, _)| *index).collect::<Vec<_>>();
        found.sort_unstable();
        let matches = found
            .iter()
            .map(|index| labels[*index].0.clone())
            .collect::<Vec<_>>();
        let ranges = ranked
            .into_iter()
            .map(|(index, found)| (labels[index].0.clone(), found.ranges))
            .collect();
        let previous = keep_current
            .then(|| search.matches.get(search.current))
            .flatten()
            .filter(|id| matches.contains(id))
            .cloned();
        let current = previous
            .or(best)
            .and_then(|id| matches.iter().position(|candidate| *candidate == id))
            .unwrap_or(0);
        self.search = Some(SpeedSearch {
            query: search.query,
            matches,
            ranges,
            current,
        });
    }

    /// Select the current search match and scroll it into view.
    fn select_search_match(&mut self) {
        let Some(id) = self
            .search
            .as_ref()
            .and_then(|search| search.matches.get(search.current).cloned())
        else {
            return;
        };
        let position = self
            .visible_rows()
            .iter()
            .position(|row| matches!(row, VisibleRow::Item { item, .. } if item.id == id));
        self.selected = Some(id);
        if let Some(flat) = position {
            self.scroll.scroll_to_item(flat, ScrollStrategy::Center);
        }
    }

    /// Handle a key while the search is active or starting; false when the
    /// key belongs to ordinary navigation.
    fn search_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let key = keystroke.key.to_lowercase();
        if self.search.is_some() {
            match key.as_str() {
                "escape" => {
                    self.search = None;
                }
                "backspace" => {
                    let search = self.search.as_mut().expect("search is active");
                    search.query.pop();
                    if search.query.is_empty() {
                        self.search = None;
                    } else {
                        self.refresh_search(false);
                        self.select_search_match();
                    }
                }
                "up" | "down" => {
                    let search = self.search.as_mut().expect("search is active");
                    let count = search.matches.len();
                    if count > 0 {
                        search.current = if key == "down" {
                            (search.current + 1) % count
                        } else {
                            (search.current + count - 1) % count
                        };
                        self.select_search_match();
                    }
                }
                // Enter and the other navigation keys act on the selected row
                // and end the search.
                "enter" | "left" | "right" | "home" | "end" | "pageup" | "pagedown" | "tab" => {
                    self.search = None;
                    return false;
                }
                _ => {
                    let Some(character) = typed_character(keystroke) else {
                        return false;
                    };
                    self.search
                        .as_mut()
                        .expect("search is active")
                        .query
                        .push(character);
                    self.refresh_search(false);
                    self.select_search_match();
                }
            }
            cx.notify();
            return true;
        }
        match typed_character(keystroke) {
            Some(character) if !character.is_whitespace() => {
                self.search = Some(SpeedSearch {
                    query: character.to_string(),
                    matches: Vec::new(),
                    ranges: HashMap::new(),
                    current: 0,
                });
                self.refresh_search(false);
                self.select_search_match();
                cx.notify();
                true
            }
            _ => false,
        }
    }

    /// Expand each ancestor named by `path`, wait for its children, then
    /// select and scroll to the last ID. A path that names a missing row
    /// stops where it breaks.
    pub(crate) fn reveal(&mut self, path: Vec<String>, cx: &mut Context<Self>) {
        self.pending_reveal = (!path.is_empty()).then_some(path);
        self.advance_reveal(cx);
    }

    /// Continue the pending reveal as far as the loaded rows allow.
    fn advance_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.pending_reveal.clone() else {
            return;
        };
        let Some(provider) = self.provider else {
            self.pending_reveal = None;
            return;
        };
        let mut parent: Option<String> = None;
        for (index, id) in path.iter().enumerate() {
            let Some(state) = self.children.get(&parent) else {
                // An ancestor that was never loaded: load it and resume when
                // its children arrive.
                if parent.is_some() {
                    let _ = self.invalidate(provider.registration, parent, cx);
                } else {
                    self.pending_reveal = None;
                }
                return;
            };
            if state.loading {
                return;
            }
            if state.error.is_some() || !state.items.iter().any(|item| &item.id == id) {
                self.pending_reveal = None;
                return;
            }
            if index + 1 == path.len() {
                self.pending_reveal = None;
                self.selected = Some(id.clone());
                self.action_focus = None;
                let position = self
                    .visible_rows()
                    .iter()
                    .position(|row| matches!(row, VisibleRow::Item { item, .. } if &item.id == id));
                if let Some(flat) = position {
                    self.scroll.scroll_to_item(flat, ScrollStrategy::Center);
                }
                cx.notify();
                return;
            }
            self.expanded.insert(id.clone());
            parent = Some(id.clone());
        }
    }

    pub(crate) fn apply_response(
        &mut self,
        response: TreeChildrenResponse,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(provider) = self.provider else {
            return false;
        };
        if provider.registration != response.registration
            || response.window != self.window
            || response.view != self.instance_id
        {
            return false;
        }
        // Resolve which nested rows need their own children before borrowing
        // the parent state mutably. Groups always reload so a refreshed tree
        // reflects their current children; expanded items load once.
        let children_to_load = match &response.result {
            Ok(items) => items
                .iter()
                .filter(|item| {
                    item.kind == TreeItemKind::Group
                        || (item.collapsible_state == TreeCollapsibleState::Expanded
                            && !self.children.contains_key(&Some(item.id.clone())))
                })
                .map(|item| item.id.clone())
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        let Some(state) = self.children.get_mut(&response.parent_id) else {
            return false;
        };
        if state.generation != response.generation {
            return false;
        }
        state.loading = false;
        match response.result {
            Ok(items) => {
                for item in &items {
                    if item.kind != TreeItemKind::Group
                        && item.collapsible_state == TreeCollapsibleState::Expanded
                    {
                        self.expanded.insert(item.id.clone());
                    }
                }
                state.items = items;
                state.error = None;
            }
            Err(error) => state.error = Some(error.message),
        }
        for id in children_to_load {
            let _ = self.invalidate(provider.registration, Some(id), cx);
        }
        self.advance_reveal(cx);
        self.refresh_search(true);
        self.drop_orphaned_edit(cx);
        cx.notify();
        true
    }

    fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        let mut visited = HashSet::new();
        self.append_children(None, 0, &mut visited, &mut rows);
        rows
    }

    fn append_children(
        &self,
        parent_id: Option<&str>,
        depth: usize,
        visited: &mut HashSet<String>,
        rows: &mut Vec<VisibleRow>,
    ) {
        let key = parent_id.map(str::to_owned);
        let Some(state) = self.children.get(&key) else {
            return;
        };
        if let Some(TreeEdit {
            target: EditTarget::New { parent, icon },
            message,
            ..
        }) = &self.edit
            && parent_id == Some(parent.as_str())
        {
            rows.push(VisibleRow::Draft {
                icon: icon.clone(),
                depth,
            });
            if let Some(message) = message {
                rows.push(VisibleRow::Status {
                    depth,
                    label: message.clone(),
                    error: true,
                });
            }
        }
        for item in &state.items {
            if !visited.insert(item.id.clone()) {
                continue;
            }
            rows.push(VisibleRow::Item {
                item: Box::new(item.clone()),
                parent_id: key.clone(),
                depth,
            });
            if let Some(TreeEdit {
                target: EditTarget::Label(id),
                message: Some(message),
                ..
            }) = &self.edit
                && *id == item.id
            {
                rows.push(VisibleRow::Status {
                    depth,
                    label: message.clone(),
                    error: true,
                });
            }
            if item.kind == TreeItemKind::Group || self.expanded.contains(&item.id) {
                self.append_children(Some(&item.id), depth + 1, visited, rows);
            }
        }
        if state.loading {
            rows.push(VisibleRow::Status {
                depth,
                label: "Loading…".into(),
                error: false,
            });
        } else if let Some(error) = &state.error {
            rows.push(VisibleRow::Status {
                depth,
                label: format!("Error: {error}").into(),
                error: true,
            });
        }
    }

    fn item(&self, id: &str) -> Option<&TreeItem> {
        self.children
            .values()
            .flat_map(|state| &state.items)
            .find(|item| item.id == id)
    }

    fn activate(&mut self, id: &str, gesture: TreeGesture, cx: &mut Context<Self>) {
        self.selected = Some(id.to_owned());
        let Some(item) = self.item(id).cloned() else {
            return;
        };
        // Group headers organize rows and expose actions; they are not
        // activation targets themselves.
        if item.kind == TreeItemKind::Group {
            cx.notify();
            return;
        }
        match item.collapsible_state {
            TreeCollapsibleState::None => {
                if let (Some(provider), Some(command)) = (self.provider, item.command) {
                    let arguments = CommandArgumentValue::Object(
                        [
                            (
                                "gesture".to_owned(),
                                CommandArgumentValue::String(gesture.name().to_owned()),
                            ),
                            ("arguments".to_owned(), item.arguments),
                        ]
                        .into(),
                    );
                    cx.emit(TreeViewEvent::InvokeCommand {
                        provider,
                        command,
                        arguments,
                        window: self.window.clone(),
                        view: self.instance_id,
                    });
                }
            }
            _ if !gesture.toggles() => {}
            _ if self.expanded.remove(id) => {}
            _ => {
                self.expanded.insert(id.to_owned());
                if !self.children.contains_key(&Some(id.to_owned()))
                    && let Some(provider) = self.provider
                {
                    let _ = self.invalidate(provider.registration, Some(id.to_owned()), cx);
                }
            }
        }
        self.refresh_search(true);
        cx.notify();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn visible_labels(&self) -> Vec<String> {
        self.visible_rows()
            .iter()
            .filter_map(|row| match row {
                VisibleRow::Item { item, .. } => Some(item.label.clone()),
                VisibleRow::Draft { .. } | VisibleRow::Status { .. } => None,
            })
            .collect()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn actions_for_row_for_test(&self, id: &str) -> Vec<TreeAction> {
        self.actions_for_row(id)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn frame_actions_for_test(&self) -> Vec<TreeAction> {
        self.frame_actions.clone()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn status_for_test(&self, id: &str) -> Option<(Option<String>, Option<String>)> {
        self.item(id)
            .map(|item| (item.status_text.clone(), item.badge.clone()))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn description_for_test(&self, id: &str) -> Option<String> {
        self.item(id).and_then(|item| item.description.clone())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn detail_for_test(&self, id: &str) -> Option<String> {
        self.item(id).and_then(|item| item.detail.clone())
    }

    #[cfg(test)]
    pub(crate) fn action_focus_for_test(&self) -> Option<(Option<String>, usize)> {
        self.action_focus
            .as_ref()
            .map(|focus| (focus.row.clone(), focus.index))
    }

    /// Every action of a row, for its context menu.
    fn actions_for_row(&self, id: &str) -> Vec<TreeAction> {
        self.item(id)
            .map(|item| item.actions.clone())
            .unwrap_or_default()
    }

    /// The actions a row shows as buttons.
    fn inline_actions_for_row(&self, id: &str) -> Vec<TreeAction> {
        let mut actions = self.actions_for_row(id);
        actions.retain(|action| !action.menu_only);
        actions
    }

    fn focused_actions(&self) -> Vec<TreeAction> {
        let Some(focus) = &self.action_focus else {
            return Vec::new();
        };
        match &focus.row {
            Some(row) => self.inline_actions_for_row(row),
            None => self.frame_actions.clone(),
        }
    }

    /// Enter keyboard action mode for the current row, falling back to the
    /// frame's toolbar when that row exposes no actions.
    fn enter_action_focus(&mut self, cx: &mut Context<Self>) {
        if let Some(selected) = self.selected.clone()
            && !self.inline_actions_for_row(&selected).is_empty()
        {
            self.action_focus = Some(ActionFocus {
                row: Some(selected),
                index: 0,
            });
            cx.notify();
            return;
        }
        if !self.frame_actions.is_empty() {
            self.action_focus = Some(ActionFocus {
                row: None,
                index: 0,
            });
            cx.notify();
        }
    }

    pub(crate) fn invoke_action(&mut self, action: TreeAction, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        cx.emit(TreeViewEvent::InvokeCommand {
            provider,
            command: action.command,
            arguments: action.arguments,
            window: self.window.clone(),
            view: self.instance_id,
        });
    }

    /// Select row `id` and open its context menu. A row with no actions has
    /// no menu, so the click activates it as `rightClick`; with a menu the
    /// focus moves to the menu, away from the tree the command needs.
    fn right_click(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_context_menu(id, position, window, cx) {
            self.activate(id, TreeGesture::RightClick, cx);
        }
    }

    /// Select row `id` and open a menu of its actions at `position`, which
    /// takes focus and returns it to the tree when dismissed. False when the
    /// row has no actions.
    pub(crate) fn open_context_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let actions = self.actions_for_row(id);
        if actions.is_empty() {
            return false;
        }
        self.selected = Some(id.to_owned());
        self.action_focus = None;
        let tree = cx.entity().downgrade();
        let focus = self.focus.clone();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            actions
                .into_iter()
                .fold(menu.action_context(focus), |menu, action| {
                    let tree = tree.clone();
                    menu.item(
                        PopupMenuItem::new(action.label.clone()).on_click(move |_, _, cx| {
                            let _ =
                                tree.update(cx, |tree, cx| tree.invoke_action(action.clone(), cx));
                        }),
                    )
                })
        });
        let menu_focus = menu.focus_handle(cx);
        let subscriptions = [
            cx.subscribe_in(&menu, window, |tree, _, _: &DismissEvent, _, cx| {
                tree.close_context_menu(cx)
            }),
            cx.on_blur(&menu_focus, window, |tree, _, cx| {
                tree.close_context_menu(cx)
            }),
        ];
        menu_focus.focus(window, cx);
        crate::app::components::track_popup(&menu_focus, cx);
        self.context_menu = Some(ContextMenu {
            menu,
            position,
            _subscriptions: subscriptions,
        });
        cx.notify();
        true
    }

    /// Open the selected row's context menu below it; false when no row is
    /// selected or it has no actions.
    pub(crate) fn open_selected_context_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selected) = self.selected.clone() else {
            return false;
        };
        let position = self
            .selected_bounds
            .borrow()
            .as_ref()
            .filter(|(row, _)| *row == selected)
            .map_or_else(Point::default, |(_, bounds)| bounds.bottom_left());
        self.open_context_menu(&selected, position, window, cx)
    }

    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    #[cfg(test)]
    pub(crate) fn context_menu_open(&self) -> bool {
        self.context_menu.is_some()
    }

    /// Handle a key the keymap left unclaimed; false when the tree ignores
    /// it.
    pub(crate) fn key_down(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.edit_key(keystroke, cx) {
            return true;
        }
        let key = keystroke.key.to_lowercase();
        if let Some(focus) = self.action_focus.clone() {
            let actions = self.focused_actions();
            if !actions.is_empty() {
                match key.as_str() {
                    "up" => {
                        self.action_focus = Some(ActionFocus {
                            row: focus.row,
                            index: focus.index.saturating_sub(1),
                        });
                        cx.notify();
                    }
                    "down" => {
                        self.action_focus = Some(ActionFocus {
                            row: focus.row,
                            index: (focus.index + 1).min(actions.len() - 1),
                        });
                        cx.notify();
                    }
                    "enter" | "space" => {
                        let action = actions[focus.index.min(actions.len() - 1)].clone();
                        self.invoke_action(action, cx);
                    }
                    "escape" | "left" => {
                        self.action_focus = None;
                        cx.notify();
                    }
                    _ => return false,
                }
                return true;
            }
            // The focused row disappeared (for example after a refresh); fall
            // through so the arrow still moves the row cursor.
            self.action_focus = None;
        }

        if self.search_key(keystroke, cx) {
            return true;
        }
        let rows = self.visible_rows();
        // Keep each navigable item's flat list index so scrolling follows the
        // rendered rows even when transient loading/error status rows appear.
        let item_rows = rows
            .iter()
            .enumerate()
            .filter_map(|(flat, row)| match row {
                VisibleRow::Item {
                    item, parent_id, ..
                } => Some((flat, item.id.as_str(), parent_id.as_deref())),
                VisibleRow::Draft { .. } | VisibleRow::Status { .. } => None,
            })
            .collect::<Vec<_>>();
        if item_rows.is_empty() {
            return false;
        }
        let current = self
            .selected
            .as_deref()
            .and_then(|selected| item_rows.iter().position(|(_, id, _)| *id == selected));
        let index = match key.as_str() {
            "up" => current.unwrap_or(1).saturating_sub(1),
            "down" => (current.map_or(0, |index| index + 1)).min(item_rows.len() - 1),
            "left" => {
                let Some(index) = current else {
                    return false;
                };
                let (_, id, parent) = item_rows[index];
                if self.expanded.remove(id) {
                    self.refresh_search(true);
                    cx.notify();
                } else if let Some(parent) = parent {
                    self.selected = Some(parent.to_owned());
                }
                return true;
            }
            "right" => {
                let Some(index) = current else {
                    // With no row cursor, enter the frame's own actions.
                    self.enter_action_focus(cx);
                    return true;
                };
                let id = item_rows[index].1;
                let collapsible = self
                    .item(id)
                    .is_some_and(|item| item.collapsible_state != TreeCollapsibleState::None);
                if collapsible {
                    self.activate(id, TreeGesture::Enter, cx);
                } else {
                    self.selected = Some(id.to_owned());
                    self.enter_action_focus(cx);
                }
                return true;
            }
            "enter" => {
                let index = current.unwrap_or(0);
                self.activate(item_rows[index].1, TreeGesture::Enter, cx);
                return true;
            }
            _ => return false,
        };
        let (flat, id, _) = item_rows[index];
        self.selected = Some(id.to_owned());
        self.action_focus = None;
        self.scroll.scroll_to_item(flat, ScrollStrategy::Center);
        cx.notify();
        true
    }

    /// The disclosure icon's name; none for a row that cannot expand.
    fn disclosure_name(item: &TreeItem, expanded: bool) -> Option<&'static str> {
        if item.kind == TreeItemKind::Group || item.collapsible_state == TreeCollapsibleState::None
        {
            None
        } else if expanded {
            Some("tree.expanded")
        } else {
            Some("tree.collapsed")
        }
    }

    fn disclosure(item: &TreeItem, expanded: bool, cx: &App) -> AppIcon {
        match Self::disclosure_name(item, expanded) {
            Some(name) => AppIcon::new(icon_definition(name, cx)),
            None => AppIcon::blank(),
        }
    }

    /// The row's icon, in its `.open` form when the row is expanded.
    fn icon_definition(item: &TreeItem, expanded: bool, cx: &App) -> Option<IconDefinition> {
        Some(ref_definition(item.icon.as_ref()?, expanded, cx))
    }

    fn icon(item: &TreeItem, expanded: bool, cx: &App) -> Option<AppIcon> {
        Self::icon_definition(item, expanded, cx).map(AppIcon::new)
    }
}

impl EventEmitter<TreeViewEvent> for TreeView {}

impl Focusable for TreeView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TreeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.visible_rows();
        let selected = self.selected.clone();
        let expanded = self.expanded.clone();
        let action_focus = self.action_focus.clone();
        let frame_actions = self.frame_actions.clone();
        let selected_bounds = self.selected_bounds.clone();
        let search_ranges = self
            .search
            .as_ref()
            .map(|search| search.ranges.clone())
            .unwrap_or_default();
        let search_bar = self.search.as_ref().map(|search| {
            (
                search.query.clone(),
                search.matches.len(),
                search.current + 1,
            )
        });
        let context_menu = self
            .context_menu
            .as_ref()
            .map(|open| (open.menu.clone(), open.position));
        let entity = cx.entity();
        let border = theme(cx).paint_color(color::BORDER);
        let button_text = theme(cx).paint_color(color::TEXT);
        let button_focused = theme(cx).paint_color(color::LIST_SELECTED_BACKGROUND);
        let button = theme(cx).paint_color(color::BUTTON_BACKGROUND);
        let button_hover = theme(cx).paint_color(color::BUTTON_HOVER_BACKGROUND);
        let mut key_context = KeyContext::default();
        key_context.add(TREE_KEY_CONTEXT);
        key_context.add(super::commands::keymaps::view_context(&self.view_id));

        let toolbar = if frame_actions.is_empty() {
            None
        } else {
            let frame_focused = action_focus
                .as_ref()
                .is_some_and(|focus| focus.row.is_none());
            let mut bar = div()
                .id("extension-tree-frame-actions")
                .w_full()
                .px_2()
                .py_1()
                .flex()
                .flex_row()
                .gap_2()
                .border_b_1()
                .border_color(border);
            for (action_index, action) in frame_actions.into_iter().enumerate() {
                let focused = frame_focused
                    && action_focus
                        .as_ref()
                        .is_some_and(|focus| focus.index == action_index);
                let target = action.clone();
                let entity = entity.clone();
                bar = bar.child(
                    div()
                        .id(("extension-tree-frame-action", action_index))
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .text_xs()
                        .text_color(button_text)
                        .bg(if focused { button_focused } else { button })
                        .hover(|style| style.bg(button_hover))
                        .child(action_content(&action, cx))
                        .when_some(action_tooltip(&action), |button, tooltip| {
                            button.tooltip(tooltip)
                        })
                        .on_click(move |_, window, cx| {
                            entity.update(cx, |tree, cx| {
                                tree.focus_handle(cx).focus(window, cx);
                                tree.invoke_action(target.clone(), cx);
                            });
                            cx.stop_propagation();
                        }),
                );
            }
            Some(bar)
        };

        // Rows share one height, so a view where any row has a second line
        // shows all of them at two-line height.
        let edit_label = self.edit.as_ref().and_then(|edit| match &edit.target {
            EditTarget::Label(id) => Some((id.clone(), edit.input.clone())),
            EditTarget::New { .. } => None,
        });
        let edit_draft = self.edit.as_ref().and_then(|edit| match &edit.target {
            EditTarget::New { .. } => Some(edit.input.clone()),
            EditTarget::Label(_) => None,
        });
        let two_lines = rows.iter().any(|row| match row {
            VisibleRow::Item { item, .. } => item.detail.is_some(),
            _ => false,
        });
        let list = uniform_list("extension-tree", rows.len(), move |range, _, cx| {
            let theme = theme(cx);
            range
                .map(|index| match &rows[index] {
                    VisibleRow::Item { item, depth, .. } => {
                        let id = item.id.clone();
                        let is_selected = selected.as_deref() == Some(&item.id);
                        let is_group = item.kind == TreeItemKind::Group;
                        let row_action_focus = action_focus.as_ref().and_then(|focus| {
                            (focus.row.as_deref() == Some(item.id.as_str())).then_some(focus.index)
                        });
                        let mut row = div()
                            .id(("extension-tree-row", index))
                            .debug_selector(move || format!("extension-tree-row-{index}"))
                            .w_full()
                            .overflow_hidden()
                            .pl(crate::app::zoom::rems(8. + *depth as f32 * 14.))
                            .pr_2()
                            .py_1()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .text_sm()
                            .when(two_lines, |row| row.h(crate::app::zoom::rems(44.)))
                            .when(is_group, |row| {
                                row.bg(theme.paint_color(color::LIST_GROUP_BACKGROUND))
                                    .text_color(theme.paint_color(color::TEXT_MUTED))
                            })
                            .when(item.muted && !is_selected, |row| {
                                row.text_color(theme.paint_color(color::TEXT_MUTED))
                            })
                            .when(is_selected, |row| {
                                row.bg(theme.paint_color(color::LIST_SELECTED_BACKGROUND))
                                    .text_color(theme.paint_color(color::LIST_SELECTED_FOREGROUND))
                            })
                            // A selected row keeps its highlight while hovered.
                            .when(!is_selected, |row| {
                                row.hover(|style| {
                                    style.bg(theme.paint_color(color::LIST_HOVER_BACKGROUND))
                                })
                            })
                            .child(Self::disclosure(item, expanded.contains(&item.id), cx))
                            .children(Self::icon(item, expanded.contains(&item.id), cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        match (
                                            edit_label
                                                .as_ref()
                                                .filter(|(label, _)| *label == item.id),
                                            search_ranges.get(&item.id),
                                        ) {
                                            (Some((_, input)), _) => div().child(input.clone()),
                                            (None, Some(ranges)) => {
                                                div().truncate().child(highlighted(
                                                    item.label.clone(),
                                                    ranges,
                                                    theme.paint_color(color::LIST_MATCH_FOREGROUND),
                                                ))
                                            }
                                            (None, None) => {
                                                div().truncate().child(item.label.clone())
                                            }
                                        },
                                    )
                                    .when_some(item.detail.clone(), |column, detail| {
                                        column.child(
                                            div()
                                                .truncate()
                                                .text_xs()
                                                .text_color(theme.paint_color(color::TEXT_MUTED))
                                                .child(detail),
                                        )
                                    }),
                            )
                            .when_some(item.description.clone(), |row, description| {
                                row.child(
                                    div()
                                        .text_color(theme.paint_color(color::TEXT_MUTED))
                                        .child(description),
                                )
                            });
                        if is_selected {
                            let selected_bounds = selected_bounds.clone();
                            let id = item.id.clone();
                            row = row.on_prepaint(move |bounds, _, _| {
                                *selected_bounds.borrow_mut() = Some((id, bounds));
                            });
                        }
                        if let Some(status) = item.status_text.clone() {
                            row = row.child(
                                div()
                                    .text_color(theme.paint_color(color::TEXT_MUTED))
                                    .child(status),
                            );
                        }
                        if let Some(badge) = &item.badge {
                            row = row.child(
                                div()
                                    .size(crate::app::zoom::rems(6.0))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(theme.paint_color(badge)),
                            );
                        }
                        for (action_index, action) in item
                            .actions
                            .iter()
                            .filter(|action| !action.menu_only)
                            .enumerate()
                        {
                            let focused = row_action_focus == Some(action_index);
                            let target = action.clone();
                            let entity = entity.clone();
                            row = row.child(
                                div()
                                    .id(("extension-tree-action", index * 1024 + action_index))
                                    .px_1()
                                    .rounded_sm()
                                    .text_xs()
                                    .text_color(theme.paint_color(color::TEXT_ACCENT))
                                    .bg(theme.paint_color(if focused {
                                        color::LIST_SELECTED_BACKGROUND
                                    } else {
                                        color::BUTTON_BACKGROUND
                                    }))
                                    .hover(|style| {
                                        style.bg(theme.paint_color(color::BUTTON_HOVER_BACKGROUND))
                                    })
                                    .child(action_content(action, cx))
                                    .when_some(action_tooltip(action), |button, tooltip| {
                                        button.tooltip(tooltip)
                                    })
                                    .on_click(move |_, window, cx| {
                                        entity.update(cx, |tree, cx| {
                                            tree.focus_handle(cx).focus(window, cx);
                                            tree.invoke_action(target.clone(), cx);
                                        });
                                        cx.stop_propagation();
                                    }),
                            );
                        }
                        row.on_click({
                            let entity = entity.clone();
                            let id = id.clone();
                            move |event, window, cx| {
                                let gesture = if event.click_count() >= 2 {
                                    TreeGesture::DoubleClick
                                } else {
                                    TreeGesture::Click
                                };
                                entity.update(cx, |tree, cx| {
                                    tree.focus_handle(cx).focus(window, cx);
                                    tree.activate(&id, gesture, cx);
                                });
                            }
                        })
                        .on_mouse_down(MouseButton::Middle, {
                            let entity = entity.clone();
                            let id = id.clone();
                            move |_, window, cx| {
                                entity.update(cx, |tree, cx| {
                                    tree.focus_handle(cx).focus(window, cx);
                                    tree.activate(&id, TreeGesture::MiddleClick, cx);
                                });
                                cx.stop_propagation();
                            }
                        })
                        .on_mouse_down(MouseButton::Right, {
                            let entity = entity.clone();
                            move |event, window, cx| {
                                entity.update(cx, |tree, cx| {
                                    tree.focus_handle(cx).focus(window, cx);
                                    tree.right_click(&id, event.position, window, cx);
                                });
                                cx.stop_propagation();
                            }
                        })
                        .into_any_element()
                    }
                    VisibleRow::Draft { icon, depth } => div()
                        .id(("extension-tree-draft", index))
                        .debug_selector(move || format!("extension-tree-draft-{index}"))
                        .w_full()
                        .pl(crate::app::zoom::rems(8. + *depth as f32 * 14.))
                        .pr_2()
                        .py_1()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .when(two_lines, |row| row.h(crate::app::zoom::rems(44.)))
                        .child(AppIcon::blank())
                        .children(
                            icon.as_ref()
                                .map(|icon| AppIcon::new(ref_definition(icon, false, cx))),
                        )
                        .children(edit_draft.clone().map(|input| div().flex_1().child(input)))
                        .into_any_element(),
                    VisibleRow::Status {
                        depth,
                        label,
                        error,
                    } => div()
                        .id(("extension-tree-status", index))
                        .debug_selector(move || format!("extension-tree-status-{index}"))
                        .w_full()
                        .pl(crate::app::zoom::rems(22. + *depth as f32 * 14.))
                        .pr_2()
                        .py_1()
                        .text_sm()
                        // Rows share one height, so a long message is cut
                        // short rather than wrapped over the rows below.
                        .truncate()
                        .when(two_lines, |row| row.h(crate::app::zoom::rems(44.)))
                        .text_color(theme.paint_color(if *error {
                            color::ERROR
                        } else {
                            color::TEXT_MUTED
                        }))
                        .child(label.clone())
                        .into_any_element(),
                })
                .collect()
        })
        .flex_1()
        .track_scroll(&self.scroll);

        // The tree holds focus while its context menu or edit field does, so
        // the session keeps the tree as the target of what they run.
        div()
            .track_focus(&self.focus)
            .key_context(key_context)
            .size_full()
            .flex()
            .flex_col()
            .when_some(toolbar, |container, toolbar| container.child(toolbar))
            .child(list)
            .when_some(search_bar, |container, (query, matches, current)| {
                let theme = theme(cx);
                container.child(
                    div()
                        .id("extension-tree-speed-search")
                        .w_full()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .border_t_1()
                        .border_color(border)
                        .text_sm()
                        .child(div().flex_1().truncate().child(query))
                        .child(
                            div()
                                .text_color(theme.paint_color(color::TEXT_MUTED))
                                .child(if matches == 0 {
                                    "no matches".to_owned()
                                } else {
                                    format!("{current}/{matches}")
                                }),
                        ),
                )
            })
            .when_some(context_menu, |container, (menu, position)| {
                container.child(
                    deferred(
                        anchored()
                            .position(position)
                            .snap_to_window_with_margin(px(8.))
                            .child(menu),
                    )
                    .with_priority(1),
                )
            })
    }
}

#[crate::app::commands::declarations::command("tree.show-context-menu", title = "Show Context Menu", description = "Show the actions of the selected tree row.", id = SHOW_TREE_CONTEXT_MENU_COMMAND, participant = Workbench)]
pub(crate) async fn show_context_menu(
    context: crate::app::commands::declarations::CommandContext,
    (): (),
) -> Result<(), knot_protocol::CommandOutcome> {
    use crate::app::workbench::{CommandView, ExtensionView};
    use knot_protocol::CommandOutcome;
    context
        .foreground(|target, cx| {
            let Some(CommandView::Extension(ExtensionView::Tree(tree))) =
                target.command_view(cx).filter(|_| target.focused)
            else {
                return Err(CommandOutcome::Unavailable);
            };
            let window = target.native_window(cx).ok_or(CommandOutcome::Unavailable)?;
            let opened = window
                .update(cx, |_, window, cx| {
                    tree.update(cx, |tree, cx| tree.open_selected_context_menu(window, cx))
                })
                .map_err(|_| CommandOutcome::InvalidTarget)?;
            opened.then_some(()).ok_or(CommandOutcome::Unavailable)
        })
        .await
}

#[cfg(test)]
mod tests;
