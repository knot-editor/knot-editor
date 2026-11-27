//! The layout of views a package describes, which composite tabs and editor
//! popovers share: a toolbar of command buttons over rows, columns, and tabs
//! of editors, text frames, and trees.
//!
//! A [`Layout`] owns the views of its members. Each is an ordinary view, so
//! commands, keymaps, and editor-driven packages work in it as anywhere else.
//! A keyed member keeps its view when the package replaces the layout.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{prelude::*, *};
use knot_model::workbench::DocumentId;
use knot_protocol::{Command, CommandArgumentValue, IconRef, ViewId};

use super::convert::ThemePaint as _;
use super::editor::EditorView;
use super::icons::{AppIcon, ref_definition};
use super::model::Buffer;
use super::text_view::TextView;
use super::theme::{color, theme};
use super::tokens;
use super::tree_view::TreeView;
use super::view_controls::{ControlItem, ControlsView};
use super::view_tree::{ViewKind, ViewNode, ViewTree as _};
use super::workbench::{CommandView, ExtensionView};
use super::zoom;
use knot_model::editor::popovers::Extent;

/// An icon button of a composite's toolbar, which runs a command with the
/// composite as its target.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ToolbarButton {
    /// The tooltip.
    pub label: String,
    pub icon: IconRef,
    pub command: String,
    #[serde(default = "no_arguments")]
    pub arguments: CommandArgumentValue,
    /// Shown pressed.
    #[serde(default)]
    pub toggled: bool,
    /// Shown dimmed, and does nothing when clicked.
    #[serde(default)]
    pub disabled: bool,
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
}

/// Runs the command of a clicked control with the window it was clicked in,
/// once the foreground is free.
pub(crate) type RunCommand = Rc<dyn Fn(AnyWindowHandle, Command, &mut App)>;

/// A command runner for a layout that has no controls.
pub(crate) fn ignore_commands() -> RunCommand {
    Rc::new(|_, _, _| {})
}

/// How much of a group's main axis a member takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayoutSize {
    /// A share of the space the pixel members leave, in proportion to the
    /// other weights.
    Weight(f32),
    Pixels(f32),
    /// As much as the member's content takes.
    Auto,
}

impl Default for LayoutSize {
    fn default() -> Self {
        Self::Weight(1.)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Arrangement {
    /// Members side by side.
    Row,
    /// Members stacked.
    Column,
    /// One member shown at a time, chosen from a strip of their titles.
    Tabs,
}

/// A strip that joins line ranges of two editors of the layout, keyed by
/// their members' keys, in color keys of the theme. It is not a view.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectorSpec {
    pub first: String,
    pub second: String,
    pub links: Vec<ConnectorLink>,
}

/// A band that joins lines of the first editor to lines of the second.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectorLink {
    pub first: Range<u32>,
    pub second: Range<u32>,
    /// The theme color key the band is filled with.
    pub color: String,
}

/// How a layout takes its space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Sizing {
    /// The layout fills the space it is given, and `pixels` are window
    /// pixels.
    Fill,
    /// The layout is as tall as its content, up to the space it is given,
    /// and `pixels` are logical pixels the zoom scales.
    Natural,
}

/// The lines an editor or text frame and the rows a tree show before
/// scrolling, unless a member sets its own limit.
const DEFAULT_LIMIT: u32 = 12;

/// Heights of the chrome a layout draws itself, in logical pixels.
pub(crate) const TOOLBAR_HEIGHT: f32 = 37.;
const TAB_STRIP_HEIGHT: f32 = 34.;

/// How wide a connector is unless its member sets a size.
pub(crate) const CONNECTOR_WIDTH: f32 = 32.;

/// A group of members the package describes, in order.
pub(crate) struct GroupSpec {
    pub arrangement: Arrangement,
    pub members: Vec<MemberSpec>,
}

pub(crate) struct MemberSpec {
    /// The name that keeps the member's view when the package replaces the
    /// layout, unique in the layout. A member without one is always new.
    pub key: Option<String>,
    pub size: LayoutSize,
    /// Has focus when the composite opens; a group gives it to its first
    /// view. In tabs it also selects the member.
    pub focused: bool,
    /// The label of the member's tab, for a member of tabs.
    pub title: Option<String>,
    /// The most lines an editor or text frame, or rows a tree, shows before
    /// it scrolls, when the layout is sized to its content.
    pub limit: Option<u32>,
    pub content: ContentSpec,
}

pub(crate) enum ContentSpec {
    /// An editor on `buffer`, which is the text of `document` when given and
    /// an extension buffer otherwise.
    Editor {
        buffer: Entity<Buffer>,
        document: Option<DocumentId>,
        /// Muted text shown while the buffer is empty.
        placeholder: Option<String>,
        /// Whether the editor shows its gutter and current line highlight.
        gutter: bool,
    },
    /// A text frame of the contributed view of this kind.
    Text(String),
    /// A tree of the contributed view of this kind.
    Tree(String),
    Connector(ConnectorSpec),
    /// A wrapping row of checkboxes and buttons.
    Controls(Vec<ControlItem>),
    Group(GroupSpec),
}

impl ContentSpec {
    fn leaf_count(&self) -> usize {
        match self {
            Self::Group(group) => group.leaf_count(),
            Self::Connector(_) | Self::Controls(_) => 0,
            _ => 1,
        }
    }

    fn collect_documents(&self, documents: &mut Vec<DocumentId>) {
        match self {
            Self::Editor {
                document: Some(document),
                ..
            } if !documents.contains(document) => documents.push(*document),
            Self::Group(group) => group.collect_documents(documents),
            _ => {}
        }
    }
}

impl GroupSpec {
    pub(crate) fn leaf_count(&self) -> usize {
        self.members
            .iter()
            .map(|member| member.content.leaf_count())
            .sum()
    }

    /// The documents the editors beneath the group show, each once.
    pub(crate) fn documents(&self) -> Vec<DocumentId> {
        let mut documents = Vec::new();
        self.collect_documents(&mut documents);
        documents
    }

    fn collect_documents(&self, documents: &mut Vec<DocumentId>) {
        for member in &self.members {
            member.content.collect_documents(documents);
        }
    }

    /// Whether an editor member beneath the group has this key.
    pub(crate) fn has_editor_key(&self, key: &str) -> bool {
        self.members.iter().any(|member| match &member.content {
            ContentSpec::Group(group) => group.has_editor_key(key),
            ContentSpec::Editor { .. } => member.key.as_deref() == Some(key),
            _ => false,
        })
    }

    /// The layout-order index of the view that has focus on open, if the
    /// layout names one.
    fn focused_leaf(&self) -> Option<usize> {
        let mut before = 0;
        for member in &self.members {
            let count = member.content.leaf_count();
            if member.focused && count > 0 {
                return Some(before);
            }
            if let ContentSpec::Group(group) = &member.content
                && let Some(inner) = group.focused_leaf()
            {
                return Some(before + inner);
            }
            before += count;
        }
        None
    }
}

/// A view a composite lays out and can give focus to.
#[derive(Clone)]
pub(crate) enum Leaf {
    Editor(Entity<EditorView>),
    Text(Entity<TextView>),
    Tree(Entity<TreeView>),
}

impl Leaf {
    pub(crate) fn editor(&self) -> Option<&Entity<EditorView>> {
        match self {
            Self::Editor(view) => Some(view),
            _ => None,
        }
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Editor(view) => view.focus_handle(cx),
            Self::Text(view) => view.focus_handle(cx),
            Self::Tree(view) => view.focus_handle(cx),
        }
    }

    pub(crate) fn entity_id(&self) -> EntityId {
        match self {
            Self::Editor(view) => view.entity_id(),
            Self::Text(view) => view.entity_id(),
            Self::Tree(view) => view.entity_id(),
        }
    }

    pub(crate) fn view_node(&self, cx: &App) -> ViewNode {
        match self {
            Self::Editor(view) => view.view_node(cx),
            Self::Text(view) => view.view_node(cx),
            Self::Tree(view) => view.view_node(cx),
        }
    }

    pub(crate) fn command_view(&self) -> CommandView {
        match self {
            Self::Editor(view) => CommandView::Editor(view.clone()),
            Self::Text(view) => CommandView::Extension(ExtensionView::Text(view.clone())),
            Self::Tree(view) => CommandView::Extension(ExtensionView::Tree(view.clone())),
        }
    }

    /// The height of the view's content, showing at most `limit` lines or
    /// rows.
    fn natural_height(&self, limit: u32, cx: &App) -> f32 {
        let lines = |view: &Entity<EditorView>| {
            let view = view.read(cx);
            let count = view.model().read(cx).line_count().min(limit as usize);
            count as f32 * view.line_height()
        };
        match self {
            Self::Editor(view) => lines(view),
            Self::Text(view) => lines(view.read(cx).editor()),
            Self::Tree(view) => {
                let rows = view.read(cx).row_count().min(limit as usize);
                rows as f32 * tokens::list::ROW_HEIGHT * zoom::factor(cx)
            }
        }
    }

    fn element(&self) -> AnyElement {
        match self {
            Self::Editor(view) => view.clone().into_any_element(),
            Self::Text(view) => view.clone().into_any_element(),
            Self::Tree(view) => view.clone().into_any_element(),
        }
    }

    /// The contributed view this is, if it is one.
    fn hosted(&self) -> Option<ExtensionView> {
        match self {
            Self::Editor(_) => None,
            Self::Text(view) => Some(ExtensionView::Text(view.clone())),
            Self::Tree(view) => Some(ExtensionView::Tree(view.clone())),
        }
    }
}

/// What a built member shows, which a layout update compares to decide
/// whether a view of the same key is still the right one.
#[derive(Clone, PartialEq)]
enum Source {
    Buffer(Entity<Buffer>),
    Text(String),
    Tree(String),
    Connector,
    Controls,
    Group,
}

impl Source {
    fn of(content: &ContentSpec) -> Self {
        match content {
            ContentSpec::Editor { buffer, .. } => Self::Buffer(buffer.clone()),
            ContentSpec::Text(kind) => Self::Text(kind.clone()),
            ContentSpec::Tree(kind) => Self::Tree(kind.clone()),
            ContentSpec::Connector(_) => Self::Connector,
            ContentSpec::Controls(_) => Self::Controls,
            ContentSpec::Group(_) => Self::Group,
        }
    }
}

/// The instances of the contributed views among `leaves`.
fn hosted_ids(leaves: &[Leaf], cx: &App) -> Vec<ViewId> {
    leaves
        .iter()
        .filter_map(Leaf::hosted)
        .map(|view| view.instance_id(cx))
        .collect()
}

/// A member of a built group.
struct Member {
    key: Option<String>,
    source: Source,
    size: LayoutSize,
    title: Option<String>,
    limit: u32,
    content: Content,
}

impl Member {
    /// The natural height of the member's content.
    fn extent(&self, cx: &App) -> Extent {
        match &self.content {
            Content::Leaf(leaf) => Extent::Leaf(leaf.natural_height(self.limit, cx)),
            Content::Group(group) => group.read(cx).0.extent(cx),
            Content::Connector(_) | Content::Controls(_) => Extent::Leaf(0.),
        }
    }
}

/// The keyed views of the layout being replaced, which the new one takes
/// from.
type Kept = HashMap<String, (Source, Content)>;

#[derive(Clone)]
enum Content {
    Leaf(Leaf),
    Group(Entity<GroupView>),
    Connector(Entity<ConnectorView>),
    Controls(Entity<ControlsView>),
}

impl Content {
    /// The view that has focus on entering this content: the first, or the
    /// shown member of tabs.
    fn entry_leaf(&self, cx: &App) -> Option<Leaf> {
        match self {
            Self::Leaf(leaf) => Some(leaf.clone()),
            Self::Group(group) => {
                let group = &group.read(cx).0;
                group.members.get(group.shown())?.content.entry_leaf(cx)
            }
            Self::Connector(_) | Self::Controls(_) => None,
        }
    }

    fn element(&self) -> AnyElement {
        match self {
            Self::Leaf(leaf) => leaf.element(),
            Self::Group(group) => group.clone().into_any_element(),
            Self::Connector(connector) => connector.clone().into_any_element(),
            Self::Controls(controls) => controls.clone().into_any_element(),
        }
    }
}

/// A built group. A nested one is a view of its own, so it has an identity
/// in the view tree.
struct Group {
    sizing: Sizing,
    arrangement: Arrangement,
    members: Vec<Member>,
    /// The member tabs show.
    active: usize,
}

struct GroupView(Group);

impl Group {
    /// Build `spec` for the window `window`. A keyed member takes the view
    /// of the same key from `kept` when it shows the same thing. `shown` is
    /// the key of the member tabs showed before, which stays shown if it is
    /// still there.
    fn build(
        spec: GroupSpec,
        window: &str,
        sizing: Sizing,
        run: &RunCommand,
        kept: &mut Kept,
        shown: Option<&str>,
        cx: &mut App,
    ) -> Self {
        let mut active = None;
        let mut named = None;
        let members: Vec<Member> = spec
            .members
            .into_iter()
            .enumerate()
            .map(|(index, member)| {
                let source = Source::of(&member.content);
                let reused = member.key.as_ref().and_then(|key| {
                    if kept.get(key).is_some_and(|(found, _)| *found == source) {
                        kept.remove(key).map(|(_, content)| content)
                    } else {
                        None
                    }
                });
                if member.focused && active.is_none() {
                    active = Some(index);
                }
                if member.key.as_deref().is_some_and(|key| Some(key) == shown) {
                    named = Some(index);
                }
                let content = match (member.content, reused) {
                    (ContentSpec::Group(group), Some(Content::Group(entity))) => {
                        let before = entity.read(cx).0.shown_key();
                        entity.update(cx, |view, cx| {
                            view.0 = Self::build(
                                group,
                                window,
                                sizing,
                                run,
                                kept,
                                before.as_deref(),
                                cx,
                            );
                            cx.notify();
                        });
                        Content::Group(entity)
                    }
                    (
                        ContentSpec::Editor {
                            placeholder,
                            gutter,
                            ..
                        },
                        Some(content),
                    ) => {
                        if let Content::Leaf(Leaf::Editor(view)) = &content {
                            view.update(cx, |view, cx| {
                                view.set_placeholder(placeholder, cx);
                                view.set_show_gutter(gutter, cx);
                            });
                        }
                        content
                    }
                    (_, Some(content)) => content,
                    (
                        ContentSpec::Editor {
                            buffer,
                            placeholder,
                            gutter,
                            ..
                        },
                        None,
                    ) => Content::Leaf(Leaf::Editor(cx.new(|cx| {
                        let mut view = EditorView::new(buffer, cx);
                        view.set_placeholder(placeholder, cx);
                        view.set_show_gutter(gutter, cx);
                        view
                    }))),
                    (ContentSpec::Text(kind), None) => Content::Leaf(Leaf::Text(
                        cx.new(|cx| TextView::new(kind, window.to_owned(), cx)),
                    )),
                    (ContentSpec::Tree(kind), None) => Content::Leaf(Leaf::Tree(
                        cx.new(|cx| TreeView::new(kind, window.to_owned(), cx)),
                    )),
                    (ContentSpec::Connector(spec), None) => {
                        Content::Connector(cx.new(|_| ConnectorView {
                            spec,
                            editors: None,
                        }))
                    }
                    (ContentSpec::Controls(items), _) => {
                        Content::Controls(cx.new(|_| ControlsView::new(items, run.clone())))
                    }
                    (ContentSpec::Group(group), None) => {
                        let group = Self::build(group, window, sizing, run, kept, None, cx);
                        Content::Group(cx.new(|_| GroupView(group)))
                    }
                };
                Member {
                    key: member.key,
                    source,
                    size: member.size,
                    title: member.title,
                    limit: member.limit.unwrap_or(DEFAULT_LIMIT),
                    content,
                }
            })
            .collect();
        Self {
            sizing,
            arrangement: spec.arrangement,
            members,
            active: named.or(active).unwrap_or(0),
        }
    }

    /// The key of the member tabs show, if it has one.
    fn shown_key(&self) -> Option<String> {
        match self.arrangement {
            Arrangement::Tabs => self.members.get(self.active)?.key.clone(),
            _ => None,
        }
    }

    /// Move the keyed members beneath this group into `kept`.
    fn keep(&self, kept: &mut Kept, cx: &App) {
        for member in &self.members {
            if let Some(key) = &member.key {
                kept.insert(key.clone(), (member.source.clone(), member.content.clone()));
            }
            if let Content::Group(group) = &member.content {
                group.read(cx).0.keep(kept, cx);
            }
        }
    }

    /// The editor of the member keyed `key`, at any depth.
    fn keyed_editor(&self, key: &str, cx: &App) -> Option<Entity<EditorView>> {
        self.members
            .iter()
            .find_map(|member| match &member.content {
                Content::Leaf(Leaf::Editor(view)) if member.key.as_deref() == Some(key) => {
                    Some(view.clone())
                }
                Content::Group(group) => group.read(cx).0.keyed_editor(key, cx),
                _ => None,
            })
    }

    /// The connectors beneath this group.
    fn connectors(&self, cx: &App) -> Vec<Entity<ConnectorView>> {
        let mut connectors = Vec::new();
        for member in &self.members {
            match &member.content {
                Content::Connector(connector) => connectors.push(connector.clone()),
                Content::Group(group) => connectors.extend(group.read(cx).0.connectors(cx)),
                Content::Leaf(_) | Content::Controls(_) => {}
            }
        }
        connectors
    }

    /// Point each connector at the editors its keys name.
    fn connect(&self, cx: &mut App) {
        for connector in self.connectors(cx) {
            let editors = {
                let spec = &connector.read(cx).spec;
                self.keyed_editor(&spec.first, cx)
                    .zip(self.keyed_editor(&spec.second, cx))
            };
            connector.update(cx, |connector, cx| {
                connector.editors = editors.map(|(first, second)| [first, second]);
                cx.notify();
            });
        }
    }

    /// Show the members that lead to the view `focus`, so the tabs show it.
    /// Returns whether the view is beneath this group.
    fn reveal(&mut self, focus: EntityId, cx: &mut App) -> bool {
        for (index, member) in self.members.iter().enumerate() {
            let contains = match &member.content {
                Content::Leaf(leaf) => leaf.entity_id() == focus,
                Content::Group(group) => group.update(cx, |view, cx| {
                    let found = view.0.reveal(focus, cx);
                    cx.notify();
                    found
                }),
                Content::Connector(_) | Content::Controls(_) => false,
            };
            if contains {
                if self.arrangement == Arrangement::Tabs {
                    self.active = index;
                }
                return true;
            }
        }
        false
    }

    /// The index of the member that has focus or shows, for the group's node.
    fn shown(&self) -> usize {
        match self.arrangement {
            Arrangement::Tabs => self.active,
            _ => 0,
        }
    }

    /// The views beneath this group in layout order.
    fn leaves(&self, cx: &App) -> Vec<Leaf> {
        let mut leaves = Vec::new();
        for member in &self.members {
            match &member.content {
                Content::Leaf(leaf) => leaves.push(leaf.clone()),
                Content::Group(group) => leaves.extend(group.read(cx).0.leaves(cx)),
                Content::Connector(_) | Content::Controls(_) => {}
            }
        }
        leaves
    }

    /// The nested groups beneath this one in layout order, this one's
    /// excluded.
    #[cfg(test)]
    fn groups(&self, cx: &App) -> Vec<Entity<GroupView>> {
        let mut groups = Vec::new();
        for member in &self.members {
            if let Content::Group(group) = &member.content {
                groups.push(group.clone());
                groups.extend(group.read(cx).0.groups(cx));
            }
        }
        groups
    }

    /// The group's node of kind `kind`, whose focus is the member holding
    /// `focused`.
    fn node(&self, id: EntityId, kind: ViewKind, focused: EntityId, cx: &App) -> ViewNode {
        let children: Vec<ViewNode> = self
            .members
            .iter()
            .filter_map(|member| match &member.content {
                Content::Leaf(leaf) => Some(leaf.view_node(cx)),
                Content::Group(group) => Some(group.read(cx).0.node(
                    group.entity_id(),
                    ViewKind::Composite,
                    focused,
                    cx,
                )),
                Content::Connector(_) | Content::Controls(_) => None,
            })
            .collect();
        ViewNode {
            id,
            kind,
            editor: None,
            focus: Some(
                children
                    .iter()
                    .position(|child| child.path_to(focused).is_some())
                    .unwrap_or(self.shown()),
            ),
            children,
        }
    }

    /// The natural height of the group.
    fn extent(&self, cx: &App) -> Extent {
        let scale = self.scale(cx);
        let members = |sized: bool| {
            self.members
                .iter()
                .map(|member| match member.size {
                    LayoutSize::Pixels(pixels) if sized => Extent::Leaf(pixels * scale),
                    _ => member.extent(cx),
                })
                .collect()
        };
        match self.arrangement {
            Arrangement::Column => Extent::Sum(members(true)),
            Arrangement::Row => Extent::Max(members(false)),
            Arrangement::Tabs => Extent::Sum(vec![
                Extent::Leaf(TAB_STRIP_HEIGHT * zoom::factor(cx)),
                Extent::Max(members(false)),
            ]),
        }
    }

    /// What a member's `pixels` are multiplied by.
    fn scale(&self, cx: &App) -> f32 {
        match self.sizing {
            Sizing::Fill => 1.,
            Sizing::Natural => zoom::factor(cx),
        }
    }

    /// A row or column of its members.
    fn render(&self, cx: &App) -> Div {
        let scale = self.scale(cx);
        let border = theme(cx).paint_color(color::BORDER);
        let row = self.arrangement == Arrangement::Row;
        let group = div().flex().size_full().min_w_0().min_h_0();
        let mut group = if row {
            group.flex_row()
        } else {
            group.flex_col()
        };
        for (index, member) in self.members.iter().enumerate() {
            let mut slot = div().min_w_0().min_h_0().flex_shrink(1.);
            match member.size {
                LayoutSize::Weight(_) if !row && self.sizing == Sizing::Natural => {
                    slot = slot.flex_basis(px(member.extent(cx).height()));
                }
                LayoutSize::Weight(weight) => {
                    slot = slot.flex_basis(px(0.));
                    slot.style().flex_grow = Some(weight);
                }
                LayoutSize::Auto => slot = slot.flex_none(),
                LayoutSize::Pixels(pixels) => {
                    slot = slot.flex_none();
                    slot = if row {
                        slot.w(px(pixels * scale))
                    } else {
                        slot.h(px(pixels * scale))
                    };
                }
            }
            if index > 0 {
                slot = if row {
                    slot.border_l_1()
                } else {
                    slot.border_t_1()
                }
                .border_color(border);
            }
            group = group.child(slot.child(div().size_full().child(member.content.element())));
        }
        group
    }
}

/// The strip between two editors, with a band joining each link's lines on
/// one side to its lines on the other.
struct ConnectorView {
    spec: ConnectorSpec,
    /// The editors the spec's keys name, when the layout has them.
    editors: Option<[Entity<EditorView>; 2]>,
}

/// The corners of the band that joins the vertical span `first` on the left
/// edge `x0` to the span `second` on the right edge `x1`, or nothing when
/// both spans lie wholly above or below `visible`.
pub(crate) fn band(
    first: (Pixels, Pixels),
    second: (Pixels, Pixels),
    (x0, x1): (Pixels, Pixels),
    visible: (Pixels, Pixels),
) -> Option<[Point<Pixels>; 4]> {
    if (first.1 < visible.0 && second.1 < visible.0)
        || (first.0 > visible.1 && second.0 > visible.1)
    {
        return None;
    }
    Some([
        point(x0, first.0),
        point(x1, second.0),
        point(x1, second.1),
        point(x0, first.1),
    ])
}

impl ConnectorView {
    /// The bands to paint between the edges `x`, each with its color key,
    /// following both editors' scrolling as of their last paint.
    fn bands(&self, x: (Pixels, Pixels), cx: &App) -> Vec<(&str, [Point<Pixels>; 4])> {
        let Some([first, second]) = &self.editors else {
            return Vec::new();
        };
        let (first, second) = (first.read(cx), second.read(cx));
        let text = first.text_bounds();
        let visible = (text.top(), text.bottom());
        let span = |view: &EditorView, lines: &Range<u32>| {
            (
                view.line_top(lines.start as usize, cx),
                view.line_top(lines.end as usize, cx),
            )
        };
        self.spec
            .links
            .iter()
            .filter_map(|link| {
                let corners = band(
                    span(first, &link.first),
                    span(second, &link.second),
                    x,
                    visible,
                )?;
                Some((link.color.as_str(), corners))
            })
            .collect()
    }
}

impl Render for ConnectorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                let view = view.read(cx);
                let Some([first, _]) = &view.editors else {
                    return;
                };
                let text = first.read(cx).text_bounds();
                let mask = Bounds {
                    origin: point(bounds.origin.x, text.origin.y),
                    size: size(bounds.size.width, text.size.height),
                };
                let bands = view.bands((bounds.left(), bounds.right()), cx);
                let theme = theme(cx);
                window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                    for (key, corners) in bands {
                        let mut path = PathBuilder::fill();
                        path.move_to(corners[0]);
                        for corner in &corners[1..] {
                            path.line_to(*corner);
                        }
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, theme.paint_color(key));
                        }
                    }
                });
            },
        )
        .size_full()
    }
}

impl GroupView {
    /// Show the `index`th member of tabs and return the handle of the view
    /// to give focus.
    fn select(&mut self, index: usize, cx: &mut Context<Self>) -> Option<FocusHandle> {
        let member = self.0.members.get(index)?;
        let handle = member.content.entry_leaf(cx)?.focus_handle(cx);
        self.0.active = index;
        cx.notify();
        Some(handle)
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> Div {
        let theme = theme(cx);
        let entity = cx.entity();
        let strip = div()
            .flex()
            .flex_row()
            .flex_none()
            .h(zoom::rems(TAB_STRIP_HEIGHT))
            .items_end()
            .bg(theme.paint_color(color::TAB_BAR_BACKGROUND))
            .border_b_1()
            .border_color(theme.paint_color(color::BORDER))
            .children(self.0.members.iter().enumerate().map(|(index, member)| {
                let active = index == self.0.active;
                let entity = entity.clone();
                div()
                    .id(("composite-tab", index))
                    .debug_selector(move || format!("composite-tab-{index}"))
                    .px_3()
                    .py_1p5()
                    .text_sm()
                    .cursor_pointer()
                    .border_b_2()
                    .when(active, |tab| {
                        tab.bg(theme.paint_color(color::TAB_ACTIVE_BACKGROUND))
                            .text_color(theme.paint_color(color::TAB_ACTIVE_FOREGROUND))
                            .border_color(theme.paint_color(color::TAB_ACCENT))
                    })
                    .when(!active, |tab| {
                        tab.border_color(gpui::transparent_black())
                            .text_color(theme.paint_color(color::TAB_INACTIVE_FOREGROUND))
                            .hover(|style| {
                                style.bg(theme.paint_color(color::LIST_HOVER_BACKGROUND))
                            })
                    })
                    .child(member.title.clone().unwrap_or_default())
                    .on_click(move |_, window, cx| {
                        let handle = entity.update(cx, |group, cx| group.select(index, cx));
                        if let Some(handle) = handle {
                            window.focus(&handle, cx);
                        }
                    })
            }));
        let shown = self.0.members.get(self.0.active).map(|member| {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(div().size_full().child(member.content.element()))
        });
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(strip)
            .children(shown)
    }
}

impl Render for GroupView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.0.arrangement {
            Arrangement::Tabs => self.render_tabs(cx),
            Arrangement::Row | Arrangement::Column => self.0.render(cx),
        }
    }
}

/// The views of a composite and which one commands act on.
pub(crate) struct Shared {
    leaves: RefCell<Rc<[Leaf]>>,
    focused: Cell<usize>,
}

impl Shared {
    pub(crate) fn leaves(&self) -> Rc<[Leaf]> {
        self.leaves.borrow().clone()
    }

    pub(crate) fn focused_leaf(&self) -> Leaf {
        self.leaves.borrow()[self.focused.get()].clone()
    }
}

/// What replacing a layout did beyond the views themselves.
pub(crate) struct LayoutChange {
    /// The contributed views the new layout added, which the host binds.
    pub attached: Vec<ExtensionView>,
    /// The focused view was removed, so another has focus.
    pub focus_moved: bool,
}

/// A built layout: its groups and views, and which view commands act on.
pub(crate) struct Layout {
    root: Group,
    /// Runs the command of a clicked control.
    run: RunCommand,
    /// The views in layout order, which the layout holds at least one of, and
    /// the index of the one that has focus or had it last.
    shared: Rc<Shared>,
    /// The instances of the contributed views, which the host unbinds when
    /// the layout is released.
    hosted: Vec<ViewId>,
    /// Watches each view's focus; set up on the first render after the layout
    /// changes, which has the window.
    focus_subscriptions: Vec<Subscription>,
}

impl Layout {
    /// A layout of `spec` in the window `window`; the spec is a row or column that holds at
    /// least one view.
    pub(crate) fn new(
        spec: GroupSpec,
        window: &str,
        sizing: Sizing,
        run: RunCommand,
        cx: &mut App,
    ) -> Self {
        debug_assert!(spec.leaf_count() > 0 && spec.arrangement != Arrangement::Tabs);
        let focused = spec.focused_leaf().unwrap_or(0);
        let mut root = Group::build(spec, window, sizing, &run, &mut Kept::new(), None, cx);
        root.connect(cx);
        let leaves: Rc<[Leaf]> = root.leaves(cx).into();
        root.reveal(leaves[focused].entity_id(), cx);
        let hosted = hosted_ids(&leaves, cx);
        Self {
            root,
            run,
            shared: Rc::new(Shared {
                leaves: RefCell::new(leaves),
                focused: Cell::new(focused),
            }),
            hosted,
            focus_subscriptions: Vec::new(),
        }
    }

    /// Replace the layout with `spec`, which like the first is a row or
    /// column that holds at least one view. A keyed member keeps its view
    /// when it shows the same thing, and so keeps the view's handle, state,
    /// and focus. When the focused view goes, the layout's own initial focus
    /// takes over.
    pub(crate) fn update(
        &mut self,
        spec: GroupSpec,
        window: &str,
        sizing: Sizing,
        cx: &mut App,
    ) -> LayoutChange {
        debug_assert!(spec.leaf_count() > 0 && spec.arrangement != Arrangement::Tabs);
        let before = self.shared.leaves();
        let focused = self.shared.focused_leaf().entity_id();
        let initial = spec.focused_leaf().unwrap_or(0);
        let mut kept = Kept::new();
        self.root.keep(&mut kept, cx);
        self.root = Group::build(spec, window, sizing, &self.run, &mut kept, None, cx);
        self.root.connect(cx);
        let leaves: Rc<[Leaf]> = self.root.leaves(cx).into();
        let kept_focus = leaves.iter().position(|leaf| leaf.entity_id() == focused);
        let index = kept_focus.unwrap_or(initial);
        self.root.reveal(leaves[index].entity_id(), cx);
        let new = |leaf: &&Leaf| !before.iter().any(|old| old.entity_id() == leaf.entity_id());
        let attached = leaves.iter().filter(new).filter_map(Leaf::hosted).collect();
        let gone: Vec<ViewId> = before
            .iter()
            .filter(|old| {
                !leaves
                    .iter()
                    .any(|leaf| leaf.entity_id() == old.entity_id())
            })
            .filter_map(Leaf::hosted)
            .map(|view| view.instance_id(cx))
            .collect();
        self.hosted = hosted_ids(&leaves, cx);
        *self.shared.leaves.borrow_mut() = leaves;
        self.shared.focused.set(index);
        self.focus_subscriptions.clear();
        detach_views(gone, cx);
        LayoutChange {
            attached,
            focus_moved: kept_focus.is_none(),
        }
    }

    /// The instances of the contributed views, to unbind when the layout is
    /// released.
    pub(crate) fn take_hosted(&mut self) -> Vec<ViewId> {
        std::mem::take(&mut self.hosted)
    }

    /// The contributed views the layout holds, which the host binds.
    pub(crate) fn hosted_views(&self) -> Vec<ExtensionView> {
        self.shared
            .leaves()
            .iter()
            .filter_map(Leaf::hosted)
            .collect()
    }

    /// The editor of the member keyed `key`, at any depth.
    pub(crate) fn keyed_editor(&self, key: &str, cx: &App) -> Option<Entity<EditorView>> {
        self.root.keyed_editor(key, cx)
    }

    /// The views in layout order.
    pub(crate) fn leaves(&self) -> Rc<[Leaf]> {
        self.shared.leaves()
    }

    /// The focus handle of the view commands act on.
    pub(crate) fn focused_handle(&self, cx: &App) -> FocusHandle {
        self.shared.focused_leaf().focus_handle(cx)
    }

    /// Treat the `index`th view as the one with focus, as its gaining focus
    /// does; whether that changed which view has it.
    pub(crate) fn set_focused_leaf(&self, index: usize) -> bool {
        self.shared.focused.replace(index) != index
    }

    pub(crate) fn shared(&self) -> Rc<Shared> {
        self.shared.clone()
    }

    /// Track which view has focus, once for the current views. `layout`
    /// finds this layout in its owner and `moved` runs when focus moved to a
    /// different view of it.
    pub(crate) fn watch_focus<T: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<T>,
        layout: impl Fn(&T) -> &Layout + Clone + 'static,
        moved: impl Fn(&mut T, &mut Context<T>) + Clone + 'static,
    ) {
        if !self.focus_subscriptions.is_empty() {
            return;
        }
        for leaf in self.shared.leaves().iter() {
            let handle = leaf.focus_handle(cx);
            let id = leaf.entity_id();
            let (layout, moved) = (layout.clone(), moved.clone());
            self.focus_subscriptions
                .push(cx.on_focus_in(&handle, window, move |this, _, cx| {
                    let layout = layout(this);
                    let index = layout
                        .shared
                        .leaves()
                        .iter()
                        .position(|leaf| leaf.entity_id() == id);
                    if let Some(index) = index
                        && layout.set_focused_leaf(index)
                    {
                        moved(this, cx);
                    }
                }));
        }
    }

    /// Whether a view of the layout has focus, as of `window`.
    pub(crate) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.shared
            .leaves()
            .iter()
            .any(|leaf| leaf.focus_handle(cx).contains_focused(window, cx))
    }

    /// The root's node of kind `kind` with identity `id`, whose focus is the
    /// member holding the view commands act on.
    pub(crate) fn node(&self, id: EntityId, kind: ViewKind, cx: &App) -> ViewNode {
        self.root
            .node(id, kind, self.shared.focused_leaf().entity_id(), cx)
    }

    /// The height of the layout's content, in window pixels.
    pub(crate) fn natural_height(&self, cx: &App) -> f32 {
        self.root.extent(cx).height()
    }

    /// The root group as an element.
    pub(crate) fn render(&self, cx: &App) -> Div {
        self.root.render(cx)
    }

    /// The links of every connector, in layout order, as the lines of each
    /// side and the color key.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn connector_links(&self, cx: &App) -> Vec<(Range<u32>, Range<u32>, String)> {
        self.root
            .connectors(cx)
            .iter()
            .flat_map(|connector| {
                connector
                    .read(cx)
                    .spec
                    .links
                    .iter()
                    .map(|link| (link.first.clone(), link.second.clone(), link.color.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The editors each connector joins, in layout order.
    #[cfg(test)]
    pub(crate) fn connected_editors(&self, cx: &App) -> Vec<Option<[EntityId; 2]>> {
        self.root
            .connectors(cx)
            .iter()
            .map(|connector| {
                connector
                    .read(cx)
                    .editors
                    .as_ref()
                    .map(|editors| editors.each_ref().map(Entity::entity_id))
            })
            .collect()
    }

    /// Show the `index`th tab of the tabs group `group` and return the handle
    /// of the view to give focus.
    #[cfg(test)]
    pub(crate) fn select_tab(
        &self,
        group: usize,
        index: usize,
        cx: &mut App,
    ) -> Option<FocusHandle> {
        let groups = self.root.groups(cx);
        groups
            .get(group)?
            .update(cx, |group, cx| group.select(index, cx))
    }
}

/// Unbind contributed views the foreground no longer shows, once it is free.
pub(crate) fn detach_views(views: Vec<ViewId>, cx: &mut App) {
    if views.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for view in views {
            crate::app::extensions::host::detach_view_instance(view, cx);
        }
    });
}

/// The bar of buttons above a layout, when it has any. A click runs
/// `run` with the window it was in and the button's command, once the
/// foreground is free. `name` prefixes the element IDs and test selectors.
pub(crate) fn toolbar(
    buttons: &[ToolbarButton],
    name: &'static str,
    run: impl Fn(AnyWindowHandle, Command, &mut App) + Clone + 'static,
    cx: &App,
) -> Option<Stateful<Div>> {
    if buttons.is_empty() {
        return None;
    }
    let theme = theme(cx);
    let text = theme.paint_color(color::TEXT);
    let hover = theme.paint_color(color::BUTTON_HOVER_BACKGROUND);
    let pressed = theme.paint_color(color::LIST_SELECTED_BACKGROUND);
    let items = buttons.iter().enumerate().map(|(index, button)| {
        let label = SharedString::from(button.label.clone());
        let command = Command {
            name: button.command.as_str().into(),
            arguments: button.arguments.clone(),
        };
        let run = run.clone();
        div()
            .id((name, index))
            .debug_selector(move || format!("{name}-{index}"))
            .size(zoom::rems(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(zoom::rems(6.))
            .text_color(text)
            .when(button.toggled, |button| button.bg(pressed))
            .when(button.disabled, |button| button.opacity(0.4))
            .when(!button.disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(hover))
                    .on_click(move |_, window, cx| {
                        let handle = window.window_handle();
                        let (run, command) = (run.clone(), command.clone());
                        cx.defer(move |cx| run(handle, command, cx));
                    })
            })
            .child(AppIcon::new(ref_definition(&button.icon, false, cx)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label.clone())
                    .text_xs()
                    .build(window, cx)
            })
    });
    Some(
        div()
            .id(name)
            .flex()
            .flex_row()
            .flex_none()
            .h(zoom::rems(TOOLBAR_HEIGHT))
            .items_center()
            .gap(zoom::rems(4.))
            .px(zoom::rems(8.))
            .py(zoom::rems(5.))
            .bg(theme.paint_color(color::EDITOR_BAR_BACKGROUND))
            .border_b_1()
            .border_color(theme.paint_color(color::BORDER))
            .children(items),
    )
}
