//! Versioned window layouts shared by native window hot exit and extension
//! sessions.
//!
//! A layout records window geometry, the center's split tree, each dock's
//! size, visibility, and split tree, ordered tabs and the selected tab per
//! pane, the focused pane, and each editor tab's view state. It never contains
//! pane, tab, view, or window handles. Every tab records only what its kind
//! needs to recreate a presentation in the same position: a document tab its
//! document identity, resource URI, and explicit language, a tool tab its
//! stable tool ID, and a terminal tab its shell's working directory.
//!
//! Restoring plans a pruned arrangement in one place for both restorers:
//! unresolved documents, unregistered or repeated tools, tabs whose kind the
//! area does not allow, and repeated documents within a pane are dropped,
//! then empty panes and one-child splits collapse.

use gpui::App;
use serde::{Deserialize, Serialize};

use super::{
    documents::DocumentCollection,
    editor::EditorViewState,
    workbench::{
        Area, DockLocation, SplitDirection, TabSurfaceId, TerminalSessionId, Workbench,
        WorkbenchLayout, WorkbenchTab,
    },
};
use knot_model::workbench::TabKind;
pub(crate) use knot_model::workbench::{Arrangement, DockArrangement, PlanNode, Rect};

pub(crate) const WINDOW_LAYOUT_VERSION: u32 = 2;

/// Maps a document tab's identity and URI to a live document.
type Resolve<'a, D> = dyn FnMut(Option<&str>, Option<&str>) -> Option<D> + 'a;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowLayout {
    pub(crate) version: u32,
    pub(crate) geometry: Option<WindowGeometry>,
    pub(crate) center: Option<LayoutNode>,
    pub(crate) docks: LayoutDocks,
    /// Index of the focused pane in layout order: the center's panes, then
    /// the left, right, and bottom docks'.
    pub(crate) focused: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct LayoutDocks {
    pub(crate) left: LayoutDock,
    pub(crate) right: LayoutDock,
    pub(crate) bottom: LayoutDock,
}

impl LayoutDocks {
    pub(crate) fn dock(&self, dock: DockLocation) -> &LayoutDock {
        match dock {
            DockLocation::Left => &self.left,
            DockLocation::Right => &self.right,
            DockLocation::Bottom => &self.bottom,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct LayoutDock {
    pub(crate) size: f32,
    pub(crate) visible: bool,
    pub(crate) root: Option<LayoutNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum LayoutNode {
    Pane {
        selected: usize,
        tabs: Vec<LayoutTab>,
    },
    Split {
        direction: SplitDirection,
        /// The fraction of the split's extent given to `first`.
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    fn tabs(&self, visit: &mut impl FnMut(&LayoutTab)) {
        match self {
            Self::Pane { tabs, .. } => tabs.iter().for_each(&mut *visit),
            Self::Split { first, second, .. } => {
                first.tabs(visit);
                second.tabs(visit);
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum LayoutTab {
    Document {
        document: Option<String>,
        uri: Option<String>,
        view: EditorViewState,
        /// The document's explicit language; detected languages are not
        /// recorded, so restoration detects them again.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        pinned: bool,
    },
    Tool {
        tool: String,
        #[serde(default, skip_serializing_if = "is_false")]
        pinned: bool,
    },
    Terminal {
        /// The shell's working directory as a file URI.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        pinned: bool,
    },
}

fn is_false(value: &bool) -> bool {
    !value
}

impl LayoutTab {
    fn kind(&self) -> TabKind {
        match self {
            Self::Document { .. } => TabKind::Document,
            Self::Tool { .. } => TabKind::Tool,
            Self::Terminal { .. } => TabKind::Terminal,
        }
    }

    pub(crate) fn is_pinned(&self) -> bool {
        match self {
            Self::Document { pinned, .. }
            | Self::Tool { pinned, .. }
            | Self::Terminal { pinned, .. } => *pinned,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WindowState {
    Windowed,
    Maximized,
    Fullscreen,
}

/// Logical window bounds, presentation state, and the display it was on.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct WindowGeometry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) state: WindowState,
    pub(crate) display: Option<String>,
}

impl WindowGeometry {
    pub(crate) fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// Transported JSON numbers are doubles; offsets and indices decode from their
/// integral values.
fn integral(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if float.fract() == 0. && (0. ..=u64::MAX as f64).contains(&float) => {
                Value::from(float as u64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, integral(value)))
                .collect(),
        ),
        other => other,
    }
}

impl WindowLayout {
    /// Decode a stored layout. An unsupported version or invalid shape yields
    /// `None`, so callers fall back to their layout-free arrangement.
    pub(crate) fn decode(value: &serde_json::Value) -> Option<Self> {
        let value = integral(value.clone());
        if value.get("version").and_then(serde_json::Value::as_u64)
            != Some(u64::from(WINDOW_LAYOUT_VERSION))
        {
            return None;
        }
        let layout: Self = serde_json::from_value(value).ok()?;
        let sizes = DockLocation::ALL.map(|dock| layout.docks.dock(dock).size);
        let geometry = layout.geometry.as_ref().is_none_or(|geometry| {
            [geometry.x, geometry.y, geometry.width, geometry.height]
                .iter()
                .all(|value| value.is_finite())
                && geometry.width > 0.
                && geometry.height > 0.
        });
        (sizes.iter().all(|size| size.is_finite()) && geometry).then_some(layout)
    }

    /// Capture a workbench with its window geometry. `terminal_directory`
    /// reports a terminal session's working directory.
    pub(crate) fn capture(
        workbench: &Workbench,
        geometry: Option<WindowGeometry>,
        documents: &DocumentCollection,
        terminal_directory: &dyn Fn(TerminalSessionId) -> Option<std::path::PathBuf>,
        cx: &App,
    ) -> Self {
        fn node(
            layout: &WorkbenchLayout,
            workbench: &Workbench,
            documents: &DocumentCollection,
            terminal_directory: &dyn Fn(TerminalSessionId) -> Option<std::path::PathBuf>,
            cx: &App,
        ) -> LayoutNode {
            match layout {
                WorkbenchLayout::Pane(id) => {
                    let pane = workbench.pane(*id).expect("layout panes are live");
                    let tabs = pane
                        .tabs()
                        .iter()
                        .map(|tab| {
                            WindowLayout::capture_tab(
                                workbench,
                                tab,
                                documents,
                                terminal_directory,
                                cx,
                            )
                        })
                        .collect();
                    LayoutNode::Pane {
                        selected: pane
                            .tabs()
                            .iter()
                            .position(|tab| tab.id() == pane.active_tab_id())
                            .unwrap_or_default(),
                        tabs,
                    }
                }
                WorkbenchLayout::Split {
                    direction,
                    ratio,
                    first,
                    second,
                } => LayoutNode::Split {
                    direction: *direction,
                    ratio: *ratio,
                    first: Box::new(node(first, workbench, documents, terminal_directory, cx)),
                    second: Box::new(node(second, workbench, documents, terminal_directory, cx)),
                },
            }
        }
        let root = |area| {
            workbench
                .area_layout(area)
                .map(|layout| node(layout, workbench, documents, terminal_directory, cx))
        };
        let dock = |dock: DockLocation| LayoutDock {
            size: workbench.dock_size(dock),
            visible: workbench.dock_visible(dock),
            root: root(Area::Dock(dock)),
        };
        Self {
            version: WINDOW_LAYOUT_VERSION,
            geometry,
            center: root(Area::Center),
            docks: LayoutDocks {
                left: dock(DockLocation::Left),
                right: dock(DockLocation::Right),
                bottom: dock(DockLocation::Bottom),
            },
            focused: workbench.focused_pane_id().and_then(|focused| {
                workbench
                    .pane_order()
                    .iter()
                    .position(|pane| *pane == focused)
            }),
        }
    }

    /// Capture what recreates one tab in place.
    pub(crate) fn capture_tab(
        workbench: &Workbench,
        tab: &WorkbenchTab,
        documents: &DocumentCollection,
        terminal_directory: &dyn Fn(TerminalSessionId) -> Option<std::path::PathBuf>,
        cx: &App,
    ) -> LayoutTab {
        match tab.surface_id() {
            TabSurfaceId::Document(id) => {
                let document = documents.get(id);
                let model = document.map(|document| document.model().read(cx));
                LayoutTab::Document {
                    document: document.map(|document| document.identity().to_string()),
                    uri: document
                        .and_then(|document| document.resource_uri())
                        .map(ToString::to_string),
                    view: workbench
                        .editor(tab)
                        .map(|editor| editor.read(cx).editor().read(cx).view_state(cx))
                        .unwrap_or_default(),
                    language: model
                        .filter(|model| model.language_is_explicit())
                        .and_then(|model| model.language())
                        .map(ToString::to_string),
                    pinned: tab.is_pinned(),
                }
            }
            TabSurfaceId::Tool(id) => LayoutTab::Tool {
                tool: workbench
                    .tool(id)
                    .map(|tool| tool.stable_id.clone())
                    .unwrap_or_default(),
                pinned: tab.is_pinned(),
            },
            TabSurfaceId::Terminal(id) => LayoutTab::Terminal {
                directory: terminal_directory(id)
                    .and_then(|path| url::Url::from_directory_path(path).ok())
                    .map(String::from),
                pinned: tab.is_pinned(),
            },
        }
    }

    /// The roots of the center and the left, right, and bottom docks.
    fn roots(&self) -> impl Iterator<Item = &LayoutNode> {
        [
            self.center.as_ref(),
            self.docks.left.root.as_ref(),
            self.docks.right.root.as_ref(),
            self.docks.bottom.root.as_ref(),
        ]
        .into_iter()
        .flatten()
    }

    /// Distinct document references of this layout's document tabs, as
    /// identity and URI, in layout order.
    pub(crate) fn documents(&self) -> Vec<(Option<String>, Option<String>)> {
        let mut documents = Vec::new();
        for root in self.roots() {
            root.tabs(&mut |tab| {
                if let LayoutTab::Document { document, uri, .. } = tab {
                    let reference = (document.clone(), uri.clone());
                    if !documents.contains(&reference) {
                        documents.push(reference);
                    }
                }
            });
        }
        documents
    }

    /// Whether any tab of this layout presents the given tool.
    pub(crate) fn names_tool(&self, tool: &str) -> bool {
        let mut named = false;
        for root in self.roots() {
            root.tabs(&mut |tab| {
                named |= matches!(tab, LayoutTab::Tool { tool: id, .. } if id == tool);
            });
        }
        named
    }

    /// Plan the pruned arrangement of this layout. `resolve` maps a document
    /// tab's identity and URI to a live document; `has_tool` reports whether
    /// a stable tool ID is registered in the destination window.
    pub(crate) fn plan<D: Copy + PartialEq>(
        &self,
        mut resolve: impl FnMut(Option<&str>, Option<&str>) -> Option<D>,
        has_tool: impl Fn(&str) -> bool,
    ) -> Arrangement<PlannedTab<D>> {
        struct Pruning<'a, D> {
            resolve: &'a mut Resolve<'a, D>,
            has_tool: &'a dyn Fn(&str) -> bool,
            area: Area,
            tools: Vec<String>,
            pane: usize,
            focused: Option<usize>,
        }
        fn prune<D: Copy + PartialEq>(
            node: &LayoutNode,
            state: &mut Pruning<'_, D>,
        ) -> Option<PlanNode<PlannedTab<D>>> {
            match node {
                LayoutNode::Pane { selected, tabs } => {
                    let index = state.pane;
                    state.pane += 1;
                    let mut planned = Vec::new();
                    let mut selection = 0;
                    let mut pinned = 0;
                    // Pinned tabs come first, whatever order the record has.
                    let ordered = tabs
                        .iter()
                        .enumerate()
                        .filter(|(_, tab)| tab.is_pinned())
                        .chain(tabs.iter().enumerate().filter(|(_, tab)| !tab.is_pinned()));
                    for (position, tab) in ordered {
                        if !tab.kind().allows(state.area) {
                            continue;
                        }
                        let kept = match tab {
                            LayoutTab::Document {
                                document,
                                uri,
                                view,
                                language,
                                ..
                            } => (state.resolve)(document.as_deref(), uri.as_deref())
                                .filter(|resolved| {
                                    !planned.iter().any(|tab| {
                                        matches!(tab, PlannedTab::Document { document, .. } if document == resolved)
                                    })
                                })
                                .map(|document| PlannedTab::Document {
                                    document,
                                    view: view.clone(),
                                    language: language.clone(),
                                }),
                            LayoutTab::Tool { tool, .. } => ((state.has_tool)(tool)
                                && !state.tools.contains(tool))
                            .then(|| {
                                state.tools.push(tool.clone());
                                PlannedTab::Tool(tool.clone())
                            }),
                            LayoutTab::Terminal { directory, .. } => Some(PlannedTab::Terminal {
                                directory: directory
                                    .as_deref()
                                    .and_then(|uri| url::Url::parse(uri).ok())
                                    .and_then(|url| url.to_file_path().ok()),
                            }),
                        };
                        if let Some(kept) = kept {
                            if position == *selected {
                                selection = planned.len();
                            }
                            pinned += usize::from(tab.is_pinned());
                            planned.push(kept);
                        }
                    }
                    if planned.is_empty() {
                        return None;
                    }
                    Some(PlanNode::Pane {
                        selected: selection,
                        focused: state.focused == Some(index),
                        pinned,
                        tabs: planned,
                    })
                }
                LayoutNode::Split {
                    direction,
                    ratio,
                    first,
                    second,
                } => match (prune(first, state), prune(second, state)) {
                    (Some(first), Some(second)) => Some(PlanNode::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: Box::new(first),
                        second: Box::new(second),
                    }),
                    (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                    (None, None) => None,
                },
            }
        }
        let mut state = Pruning {
            resolve: &mut resolve,
            has_tool: &has_tool,
            area: Area::Center,
            tools: Vec::new(),
            pane: 0,
            focused: self.focused,
        };
        let center = self
            .center
            .as_ref()
            .and_then(|root| prune(root, &mut state));
        let docks = DockLocation::ALL.map(|dock| {
            let layout = self.docks.dock(dock);
            state.area = Area::Dock(dock);
            DockArrangement {
                root: layout
                    .root
                    .as_ref()
                    .and_then(|root| prune(root, &mut state)),
                size: layout.size,
                visible: layout.visible,
            }
        });
        Arrangement { center, docks }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlannedTab<D> {
    Document {
        document: D,
        view: EditorViewState,
        language: Option<String>,
    },
    Tool(String),
    /// A fresh shell, started in `directory` when it still exists.
    Terminal {
        directory: Option<std::path::PathBuf>,
    },
}

/// A connected display: its persisted identity and logical bounds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayArea {
    pub(crate) uuid: Option<String>,
    pub(crate) bounds: Rect,
}

/// Height of the strip a window must keep on a display so its title bar can
/// still be grabbed, and the minimum reachable width of that strip.
const TITLE_BAR_HEIGHT: f32 = 28.;
const TITLE_BAR_REACH: f32 = 80.;

/// Fit saved bounds to the connected displays. Bounds are kept when enough of
/// their title bar lies on some display; otherwise the window is sized to fit
/// and centered on its saved display, or else on the primary display. Returns
/// the bounds and the index of the display they were fitted to, if any.
pub(crate) fn fit_to_displays(
    saved: &WindowGeometry,
    displays: &[DisplayArea],
    primary: Option<usize>,
) -> (Rect, Option<usize>) {
    let bounds = saved.rect();
    let title = Rect {
        height: TITLE_BAR_HEIGHT.min(bounds.height),
        ..bounds
    };
    let reachable = displays.iter().position(|display| {
        title.intersection(&display.bounds).is_some_and(|visible| {
            visible.width >= TITLE_BAR_REACH.min(bounds.width)
                && visible.height >= title.height.min(TITLE_BAR_HEIGHT / 2.)
        })
    });
    if let Some(index) = reachable {
        return (bounds, Some(index));
    }
    let target = saved
        .display
        .as_ref()
        .and_then(|uuid| {
            displays
                .iter()
                .position(|display| display.uuid.as_ref() == Some(uuid))
        })
        .or(primary)
        .or((!displays.is_empty()).then_some(0));
    let Some(index) = target else {
        return (bounds, None);
    };
    let area = displays[index].bounds;
    let width = bounds.width.min(area.width);
    let height = bounds.height.min(area.height);
    (
        Rect {
            x: area.x + (area.width - width) / 2.,
            y: area.y + (area.height - height) / 2.,
            width,
            height,
        },
        Some(index),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(caret: usize) -> EditorViewState {
        EditorViewState {
            anchor: caret,
            caret,
            top: 0,
            scroll_x: 0.,
            folds: Vec::new(),
        }
    }

    fn document(id: &str) -> serde_json::Value {
        json!({ "kind": "document", "document": id, "uri": null,
                "view": { "anchor": 1, "caret": 1, "top": 0, "scrollX": 0 } })
    }

    fn layout(
        center: serde_json::Value,
        left: serde_json::Value,
        focused: Option<usize>,
    ) -> WindowLayout {
        WindowLayout::decode(&json!({
            "version": 2,
            "geometry": null,
            "center": center,
            "docks": {
                "left": { "size": 240, "visible": true, "root": left },
                "right": { "size": 250, "visible": true, "root": null },
                "bottom": { "size": 220, "visible": false, "root": null },
            },
            "focused": focused,
        }))
        .unwrap()
    }

    fn resolve(document: Option<&str>, uri: Option<&str>) -> Option<u32> {
        match (document, uri) {
            (Some("a"), _) => Some(1),
            (Some("b"), _) => Some(2),
            (_, Some("mem://c")) => Some(3),
            _ => None,
        }
    }

    fn explorer() -> serde_json::Value {
        json!({ "kind": "pane", "selected": 0,
                "tabs": [{ "kind": "tool", "tool": "knot.explorer" }] })
    }

    #[test]
    fn layouts_round_trip_through_json() {
        let original = layout(
            json!({ "kind": "split", "direction": "horizontal", "ratio": 0.25,
                    "first": { "kind": "pane", "selected": 0, "tabs": [document("b")] },
                    "second": { "kind": "pane", "selected": 1,
                                "tabs": [document("a"),
                                         { "kind": "terminal", "directory": "file:///work/",
                                           "pinned": true },
                                         { "kind": "terminal" },
                                         { "kind": "document", "document": "b", "uri": null,
                                           "language": "rust",
                                           "view": { "anchor": 0, "caret": 0, "top": 0, "scrollX": 0 } }] } }),
            explorer(),
            Some(1),
        );
        let encoded = serde_json::to_value(&original).unwrap();
        assert_eq!(WindowLayout::decode(&encoded).unwrap(), original);
        // Extension transport delivers every number as a double.
        let doubles: serde_json::Value =
            serde_json::from_str(&encoded.to_string().replace(":1,", ":1.0,")).unwrap();
        assert_eq!(WindowLayout::decode(&doubles).unwrap(), original);
        let payload = encoded.to_string();
        // Only an explicit language and pinned tabs are recorded.
        assert_eq!(payload.matches("\"language\"").count(), 1);
        assert_eq!(payload.matches("\"pinned\"").count(), 1);
        for forbidden in ["pane\":", "tab\":", "window\":", "presentation"] {
            assert!(!payload.contains(forbidden), "layout leaks {forbidden}");
        }
    }

    #[test]
    fn unsupported_versions_and_invalid_shapes_are_ignored() {
        let valid = serde_json::to_value(layout(json!(null), json!(null), None)).unwrap();
        let mut infinite = valid.clone();
        infinite["docks"]["left"]["size"] = json!(f64::INFINITY);
        for invalid in [
            json!({ "version": 1, "root": null, "focused": null, "geometry": null,
                    "dockSizes": { "left": 240, "right": 240, "bottom": 220 } }),
            json!("layout"),
            json!({ "version": 2, "center": { "kind": "window" }, "focused": null,
                    "geometry": null, "docks": valid["docks"] }),
            json!({ "version": 2, "center": null, "focused": null, "geometry": null }),
            json!({ "version": 2, "center": null, "focused": null, "docks": valid["docks"],
                    "geometry": { "x": 0, "y": 0, "width": 0, "height": 10,
                                  "state": "windowed", "display": null } }),
            infinite,
        ] {
            assert_eq!(WindowLayout::decode(&invalid), None, "{invalid}");
        }
        assert!(WindowLayout::decode(&valid).is_some());
    }

    #[test]
    fn pruning_drops_unknown_tools_unmatched_documents_duplicates_and_misplaced_kinds() {
        let layout = layout(
            json!({ "kind": "pane", "selected": 3, "tabs": [
                document("a"), document("missing"), document("a"),
                { "kind": "document", "document": null, "uri": "mem://c",
                  "language": "rust",
                  "view": { "anchor": 0, "caret": 0, "top": 0, "scrollX": 0 } },
                { "kind": "tool", "tool": "knot.other" },
                { "kind": "terminal" },
                { "kind": "terminal", "directory": "file:///work/" },
                { "kind": "terminal", "directory": "not a uri" },
            ] }),
            json!({ "kind": "split", "direction": "vertical", "ratio": 0.3,
                    "first": { "kind": "pane", "selected": 1, "tabs": [
                        { "kind": "tool", "tool": "knot.explorer" },
                        { "kind": "tool", "tool": "gone.tool" },
                        { "kind": "tool", "tool": "knot.explorer" },
                    ] },
                    "second": { "kind": "pane", "selected": 0, "tabs": [
                        document("b"), { "kind": "terminal" }, { "kind": "tool", "tool": "knot.other" },
                    ] } }),
            Some(2),
        );
        let plan = layout.plan(resolve, |tool| tool != "gone.tool");
        assert_eq!(
            plan.center,
            Some(PlanNode::Pane {
                selected: 1,
                focused: false,
                pinned: 0,
                tabs: vec![
                    PlannedTab::Document {
                        document: 1,
                        view: view(1),
                        language: None,
                    },
                    PlannedTab::Document {
                        document: 3,
                        view: view(0),
                        language: Some("rust".into()),
                    },
                    PlannedTab::Terminal { directory: None },
                    PlannedTab::Terminal {
                        directory: url::Url::parse("file:///work/")
                            .unwrap()
                            .to_file_path()
                            .ok(),
                    },
                    PlannedTab::Terminal { directory: None },
                ],
            })
        );
        assert_eq!(
            plan.docks[0],
            DockArrangement {
                root: Some(PlanNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.3,
                    first: Box::new(PlanNode::Pane {
                        // The selected tool was pruned, so the first tab is selected.
                        selected: 0,
                        focused: false,
                        pinned: 0,
                        tabs: vec![PlannedTab::Tool("knot.explorer".into())],
                    }),
                    // Focus counts panes across areas: the center's, then the left dock's.
                    second: Box::new(PlanNode::Pane {
                        selected: 0,
                        focused: true,
                        pinned: 0,
                        tabs: vec![PlannedTab::Tool("knot.other".into())],
                    }),
                }),
                size: 240.,
                visible: true,
            }
        );
        assert_eq!(
            plan.docks[2],
            DockArrangement {
                root: None,
                size: 220.,
                visible: false,
            }
        );
    }

    #[test]
    fn pinned_tabs_plan_first_and_count_only_kept_tabs() {
        let layout = layout(
            json!({ "kind": "pane", "selected": 2, "tabs": [
                document("a"),
                { "kind": "terminal", "pinned": true },
                { "kind": "document", "document": "missing", "uri": null, "pinned": true,
                  "view": { "anchor": 0, "caret": 0, "top": 0, "scrollX": 0 } },
                { "kind": "document", "document": "b", "uri": null, "pinned": true,
                  "view": { "anchor": 0, "caret": 0, "top": 0, "scrollX": 0 } },
            ] }),
            json!(null),
            Some(0),
        );
        let Some(PlanNode::Pane {
            pinned,
            selected,
            tabs,
            ..
        }) = layout.plan(resolve, |_| true).center
        else {
            panic!("expected a pane");
        };
        assert_eq!(pinned, 2);
        assert_eq!(tabs[0], PlannedTab::Terminal { directory: None });
        assert!(matches!(tabs[1], PlannedTab::Document { document: 2, .. }));
        assert!(matches!(tabs[2], PlannedTab::Document { document: 1, .. }));
        // The selected tab was pruned, so the first kept tab is selected.
        assert_eq!(selected, 0);
    }

    #[test]
    fn pruning_collapses_empty_panes_and_nested_one_child_splits() {
        let layout = layout(
            json!({ "kind": "split", "direction": "vertical", "ratio": 0.5,
                    "first": { "kind": "split", "direction": "horizontal", "ratio": 0.5,
                               "first": { "kind": "pane", "selected": 0,
                                          "tabs": [document("missing")] },
                               "second": { "kind": "split", "direction": "vertical", "ratio": 0.5,
                                           "first": { "kind": "pane", "selected": 9,
                                                      "tabs": [document("b")] },
                                           "second": { "kind": "pane", "selected": 0,
                                                       "tabs": [{ "kind": "tool", "tool": "gone" }] } } },
                    "second": { "kind": "pane", "selected": 0, "tabs": [] } }),
            explorer(),
            Some(7),
        );
        let plan = layout.plan(resolve, |_| false);
        assert_eq!(
            plan.center,
            Some(PlanNode::Pane {
                selected: 0,
                // An out-of-range focus marks no pane; the installer falls back.
                focused: false,
                pinned: 0,
                tabs: vec![PlannedTab::Document {
                    document: 2,
                    view: view(1),
                    language: None,
                }],
            })
        );
        assert_eq!(plan.docks[0].root, None);
    }

    fn display(uuid: &str, x: f32, y: f32, width: f32, height: f32) -> DisplayArea {
        DisplayArea {
            uuid: Some(uuid.into()),
            bounds: Rect {
                x,
                y,
                width,
                height,
            },
        }
    }

    fn saved(x: f32, y: f32, width: f32, height: f32, display: Option<&str>) -> WindowGeometry {
        WindowGeometry {
            x,
            y,
            width,
            height,
            state: WindowState::Windowed,
            display: display.map(Into::into),
        }
    }

    #[test]
    fn saved_bounds_stay_while_their_title_bar_is_reachable() {
        let displays = [
            display("main", 0., 0., 1440., 900.),
            display("side", 1440., 0., 1920., 1080.),
        ];
        let partly = saved(1300., 40., 1000., 700., Some("side"));
        assert_eq!(
            fit_to_displays(&partly, &displays, Some(0)),
            (partly.rect(), Some(0))
        );
        // Only the bottom edge remains visible: the title bar is off screen.
        let hidden = saved(100., -680., 1000., 700., Some("main"));
        assert_eq!(
            fit_to_displays(&hidden, &displays, Some(1)),
            (
                Rect {
                    x: 220.,
                    y: 100.,
                    width: 1000.,
                    height: 700.
                },
                Some(0)
            )
        );
    }

    #[test]
    fn disconnected_displays_center_a_fitted_window_on_the_primary_display() {
        let displays = [display("laptop", 0., 0., 1280., 800.)];
        let geometry = saved(3000., 200., 1600., 1000., Some("external"));
        assert_eq!(
            fit_to_displays(&geometry, &displays, Some(0)),
            (
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 1280.,
                    height: 800.
                },
                Some(0)
            )
        );
        let small = saved(3000., 200., 600., 400., None);
        assert_eq!(
            fit_to_displays(&small, &displays, None),
            (
                Rect {
                    x: 340.,
                    y: 200.,
                    width: 600.,
                    height: 400.
                },
                Some(0)
            )
        );
        assert_eq!(fit_to_displays(&small, &[], None), (small.rect(), None));
    }
}
