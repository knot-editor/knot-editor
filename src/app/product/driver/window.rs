//! One window, driven through its native window and the input forwarded to it.
//!
//! A window takes keys and text through gpui's window dispatch, as a user
//! types them, and runs commands captured on its focused surface. Its
//! documents, roots, layout, and contributed trees report as plain data.

use std::path::Path;

use gpui::{AnyWindowHandle, AppContext, Entity, Focusable, TestAppContext, VisualTestContext};
use knot_protocol::{Command, CommandArgumentValue, CommandOutcome};
use serde_json::Value;

use crate::app::CommandExecution;
use crate::app::editor::EditorViewState;
use crate::app::keys::Keystroke;
use crate::app::product::{
    ApplicationDocuments, ApplicationProductCommands, KnotWindow, OpenRequest, OpenTarget,
    native_window, test_window_id, window_id, windows,
};
use crate::app::resource::ResourceUri;
use crate::app::status_bar::{ApplicationStatusBar, BarItem};
use crate::app::transient::TransientSurface;
use crate::app::tree_view::TreeView;
use crate::app::workbench::{DockLocation, SplitDirection, SplitPlacement, TabSurfaceId};
use crate::test_support::wait_until;

/// A window, and the native window that renders it when the test asked for
/// one. A window without one takes keys and text as a native window would
/// forward them; painting, pointer input, and native prompts need one.
#[derive(Clone)]
pub struct TestWindow {
    pub(super) window: Entity<KnotWindow>,
    pub(super) native_window: Option<AnyWindowHandle>,
}

impl PartialEq for TestWindow {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
    }
}

impl std::fmt::Debug for TestWindow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "TestWindow({})",
            window_id(self.window.entity_id())
        )
    }
}

/// Run `update` with the application, inside `native_window` when there is one.
fn in_window<R>(
    native_window: Option<AnyWindowHandle>,
    cx: &mut TestAppContext,
    update: impl FnOnce(&mut gpui::App) -> R,
) -> R {
    match native_window {
        Some(native_window) => cx
            .update_window(native_window, |_, _, cx| update(cx))
            .unwrap(),
        None => cx.update(update),
    }
}

/// An open editor popover, as the driver reads it.
#[derive(Debug, PartialEq)]
pub struct TestPopover {
    pub id: String,
    /// The lines of the editor it is anchored to.
    pub anchor: std::ops::Range<u32>,
    /// The tooltips of its toolbar buttons.
    pub buttons: Vec<String>,
    /// The text of each editor it holds.
    pub texts: Vec<String>,
}

/// A command started in a window and not yet waited for.
pub struct TestCommand(CommandExecution);

impl TestCommand {
    /// Wait until the command completes, failing after ten seconds.
    pub fn finish(mut self, cx: &mut TestAppContext) -> CommandOutcome {
        let mut outcome = None;
        wait_until(cx, |_| {
            outcome = self.0.completion.try_recv().ok();
            outcome.is_some()
        });
        outcome.unwrap()
    }
}

/// An editor tab of a window.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorTab {
    /// The identity of the buffer it shows, shared by the tabs of one
    /// document.
    pub buffer: u64,
    pub text: String,
    pub anchor: usize,
    pub caret: usize,
    /// The first line shown.
    pub top: usize,
}

/// One editor of a composite tab, as it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeEditor {
    pub text: String,
    pub caret_line: usize,
    /// The line bands, as line and color key.
    pub bands: Vec<(usize, String)>,
    /// The gutter wedges and bars, as line and color key.
    pub wedges: Vec<(usize, String)>,
    pub scroll_line: f32,
    pub key_contexts: Vec<String>,
    /// The block rows, as the line they sit above and their texts.
    pub blocks: Vec<(usize, Vec<String>)>,
}

/// A tree a package contributes to a window.
pub struct TestTree {
    window: Entity<KnotWindow>,
    native_window: Option<AnyWindowHandle>,
    tree: Entity<TreeView>,
}

impl TestWindow {
    pub(crate) fn of(window: Entity<KnotWindow>, cx: &mut TestAppContext) -> Self {
        let native_window = cx.read(|cx| native_window(window.entity_id(), cx));
        Self {
            window,
            native_window,
        }
    }

    /// Whether the window is still open.
    pub fn is_open(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| windows(cx).contains(&self.window))
    }

    /// The dot on the stripe button of the tool window `view`: "info" or
    /// "error", `None` without one.
    pub fn stripe_badge(&self, view: &str, cx: &mut TestAppContext) -> Option<&'static str> {
        use crate::app::workbench::ToolBadge;
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let tool = workbench.tool(workbench.tool_by_stable_id(view)?)?;
            tool.badge.map(|badge| match badge {
                ToolBadge::Info => "info",
                ToolBadge::Error => "error",
            })
        })
    }

    /// Start the command `name` on the focused surface.
    pub fn start_command(&self, name: &str, cx: &mut TestAppContext) -> TestCommand {
        self.start_command_with(name, CommandArgumentValue::Null, cx)
    }

    /// Run the command `name` with a `uri` argument on the focused surface
    /// until it completes.
    pub fn run_command_on(&self, name: &str, uri: &str, cx: &mut TestAppContext) -> CommandOutcome {
        let arguments = CommandArgumentValue::Object(
            [(
                "uri".to_owned(),
                CommandArgumentValue::String(uri.to_owned()),
            )]
            .into(),
        );
        self.start_command_with(name, arguments, cx).finish(cx)
    }

    /// Run the command `name` with an object of text `arguments` on the
    /// focused surface until it completes.
    pub fn run_command_with(
        &self,
        name: &str,
        arguments: &[(&str, &str)],
        cx: &mut TestAppContext,
    ) -> CommandOutcome {
        let arguments = CommandArgumentValue::Object(
            arguments
                .iter()
                .map(|(key, value)| {
                    (
                        (*key).to_owned(),
                        CommandArgumentValue::String((*value).to_owned()),
                    )
                })
                .collect(),
        );
        self.start_command_with(name, arguments, cx).finish(cx)
    }

    fn start_command_with(
        &self,
        name: &str,
        arguments: CommandArgumentValue,
        cx: &mut TestAppContext,
    ) -> TestCommand {
        let target = in_window(self.native_window, cx, |cx| {
            self.window.update(cx, |window, cx| {
                window.focus_active_surface(cx);
                window.capture_command_target(cx)
            })
        });
        TestCommand(cx.update(|cx| {
            let dispatcher = cx.global::<ApplicationProductCommands>().0.clone();
            dispatcher.update(cx, |dispatcher, cx| {
                dispatcher.dispatch(
                    Command {
                        name: name.into(),
                        arguments,
                    },
                    target,
                    cx,
                )
            })
        }))
    }

    /// Run the command `name` on the focused surface until it completes.
    pub fn run_command(&self, name: &str, cx: &mut TestAppContext) -> CommandOutcome {
        self.start_command(name, cx).finish(cx)
    }

    /// The labels of the tab menu of the center pane's tab at `index`, by
    /// group, as the tab menu would show them.
    pub fn tab_menu(&self, index: usize, cx: &mut TestAppContext) -> Vec<Vec<String>> {
        in_window(self.native_window, cx, |cx| {
            self.window.update(cx, |window, cx| {
                let (pane, tab) = {
                    let workbench = window.workbench.read(cx);
                    let id = workbench.center_pane().unwrap();
                    let pane = workbench.pane(id).unwrap();
                    (id, pane.tabs()[index].id())
                };
                let target = window.capture_tab_target(pane, tab, cx).unwrap();
                KnotWindow::menu_groups(crate::app::menus::MenuLocation::Tab, &target, cx)
                    .into_iter()
                    .map(|group| group.into_iter().map(|entry| entry.title).collect())
                    .collect()
            })
        })
    }

    /// Run the command `name` against the center pane's tab at `index`, as
    /// its tab menu would, and wait until it completes.
    pub fn run_tab_command(
        &self,
        index: usize,
        name: &str,
        cx: &mut TestAppContext,
    ) -> CommandOutcome {
        let target = in_window(self.native_window, cx, |cx| {
            self.window.update(cx, |window, cx| {
                let (pane, tab) = {
                    let workbench = window.workbench.read(cx);
                    let id = workbench.center_pane().unwrap();
                    let pane = workbench.pane(id).unwrap();
                    (id, pane.tabs()[index].id())
                };
                window.capture_tab_target(pane, tab, cx).unwrap()
            })
        });
        TestCommand(cx.update(|cx| {
            let dispatcher = cx.global::<ApplicationProductCommands>().0.clone();
            dispatcher.update(cx, |dispatcher, cx| {
                dispatcher.dispatch(
                    Command {
                        name: name.into(),
                        arguments: CommandArgumentValue::Null,
                    },
                    target,
                    cx,
                )
            })
        }))
        .finish(cx)
    }

    /// Press `keys`, such as `cmd-a enter`, in the window.
    pub fn press(&self, keys: &str, cx: &mut TestAppContext) {
        match self.native_window {
            Some(native_window) => cx.simulate_keystrokes(native_window, keys),
            None => {
                for key in keys.split_whitespace() {
                    let keystroke = Keystroke::parse(key).unwrap();
                    self.window
                        .update(cx, |window, cx| window.press_key(keystroke, cx));
                    cx.run_until_parked();
                }
            }
        }
        cx.run_until_parked();
    }

    /// Press each of `keys`, such as `j l`, after the command the one before
    /// it ran has finished, as a person typing leaves time for it. A package
    /// that keeps a request open does not hold this up, as it does the
    /// editor's settling.
    pub fn press_each(&self, keys: &str, cx: &mut TestAppContext) {
        for key in keys.split_whitespace() {
            self.press(key, cx);
            wait_until(cx, |cx| !cx.read(|cx| self.window.read(cx).holds_keys()));
        }
    }

    /// Type `text` into the window's focused input.
    pub fn type_input(&self, text: &str, cx: &mut TestAppContext) {
        match self.native_window {
            Some(native_window) => cx.simulate_input(native_window, text),
            None => self
                .window
                .update(cx, |window, cx| window.type_text(text, cx)),
        }
        cx.run_until_parked();
    }

    /// Whether a transient surface, such as a picker or an input, is open.
    pub fn transient_open(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.window.read(cx).transient.is_some())
    }

    /// The labels of the rows an open picker shows, best first; empty when
    /// no picker is open.
    pub fn picker_labels(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            match self
                .window
                .read(cx)
                .transient
                .as_ref()
                .and_then(|active| active.surface.as_ref())
            {
                Some(TransientSurface::Picker(picker)) => picker.read(cx).visible_labels(),
                _ => Vec::new(),
            }
        })
    }

    /// Wait until a command opens a text input, failing if it raises a
    /// prompt instead.
    pub fn wait_for_input(&self, cx: &mut TestAppContext) {
        wait_until(cx, |cx| {
            assert!(
                !cx.has_pending_prompt(),
                "the command raised a prompt instead of an input"
            );
            cx.read(|cx| {
                matches!(
                    self.window
                        .read(cx)
                        .transient
                        .as_ref()
                        .and_then(|active| active.surface.as_ref()),
                    Some(TransientSurface::Input(_))
                )
            })
        });
    }

    /// The workspace roots, in order.
    pub fn roots(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            self.window
                .read(cx)
                .workspace
                .snapshot()
                .roots()
                .iter()
                .map(|root| root.uri.to_string())
                .collect()
        })
    }

    /// The color key and tooltip decorating the resource `uri`, which is what
    /// its document tab shows.
    pub fn resource_decoration(
        &self,
        uri: &str,
        cx: &mut TestAppContext,
    ) -> Option<(Option<String>, Option<String>)> {
        let uri = ResourceUri::parse(uri).unwrap();
        cx.read(|cx| {
            let decoration = crate::app::resource_decorations::decoration_of(&uri, cx)?;
            Some((decoration.color, decoration.tooltip))
        })
    }

    /// Replace the workspace roots with `uris`.
    pub fn set_roots(&self, uris: &[&str], cx: &mut TestAppContext) {
        self.window.update(cx, |window, cx| {
            let roots = uris
                .iter()
                .map(|uri| ResourceUri::parse(uri).unwrap())
                .collect();
            window.replace_workspace_roots(roots, cx);
            cx.notify();
        });
        cx.run_until_parked();
    }

    /// Wait until the workspace roots are `uris`.
    pub fn wait_for_roots(&self, uris: &[&str], cx: &mut TestAppContext) {
        wait_until(cx, |cx| self.roots(cx) == uris);
    }

    /// Open the file at `path` on the focused surface, as Open does, and
    /// require that it opened.
    pub fn open_path(&self, path: &Path, cx: &mut TestAppContext) {
        let request = OpenRequest::from_path(path, Path::new("/")).unwrap();
        let target = cx.read(|cx| open_target(&self.window, cx));
        let mut opened = self.window.update(cx, |window, cx| {
            window.start_open_operation(request, target, false, cx)
        });
        let mut result = None;
        wait_until(cx, |_| {
            result = opened.try_recv().ok();
            result.is_some()
        });
        assert!(result.unwrap().is_ok(), "{} did not open", path.display());
    }

    /// Each terminal of the window with the directory its shell is in, and
    /// whether it is in the tool window `tool`.
    pub fn terminals(
        &self,
        tool: &str,
        cx: &mut TestAppContext,
    ) -> Vec<(bool, Option<std::path::PathBuf>)> {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let tool = workbench.tool_by_stable_id(tool);
            workbench
                .tab_snapshots()
                .into_iter()
                .filter_map(|tab| match tab.surface {
                    TabSurfaceId::Terminal(id) => Some((
                        tool.is_some() && workbench.pane_tool(tab.pane_id) == tool,
                        KnotWindow::terminal_session_directory(id, cx),
                    )),
                    _ => None,
                })
                .collect()
        })
    }

    /// Whether a tool window `stable_id` is registered.
    pub fn has_tool(&self, stable_id: &str, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            self.window
                .read(cx)
                .workbench
                .read(cx)
                .tool_by_stable_id(stable_id)
                .is_some()
        })
    }

    /// The text of the text frame `view`, or `None` when there is no such frame.
    pub fn frame_text(&self, view: &str, cx: &mut TestAppContext) -> Option<String> {
        cx.read(
            |cx| match &self.window.read(cx).contributed_views.get(view)?.content {
                crate::app::workbench::ExtensionView::Text(text) => Some(text.read(cx).text(cx)),
                _ => None,
            },
        )
    }

    fn layout_tool(
        &self,
        view: &str,
        cx: &TestAppContext,
    ) -> Entity<crate::app::layout_view::LayoutView> {
        cx.read(|cx| {
            self.window
                .read(cx)
                .contributed_views
                .get(view)
                .expect("a contributed view")
                .layout()
                .clone()
        })
    }

    /// The text of each editor of the layout tool window `view`, in layout order.
    pub fn tool_editors(&self, view: &str, cx: &mut TestAppContext) -> Vec<String> {
        let tool = self.layout_tool(view, cx);
        cx.read(|cx| {
            tool.read(cx)
                .leaves()
                .iter()
                .filter_map(|leaf| leaf.editor())
                .map(|editor| editor.read(cx).model().read(cx).text())
                .collect()
        })
    }

    /// The placeholder of the first editor of the layout tool window `view`.
    pub fn tool_placeholder(&self, view: &str, cx: &mut TestAppContext) -> Option<String> {
        let tool = self.layout_tool(view, cx);
        cx.read(|cx| {
            let leaves = tool.read(cx).leaves();
            let editor = leaves.iter().find_map(|leaf| leaf.editor())?;
            editor.read(cx).placeholder().map(str::to_owned)
        })
    }

    /// The commands of the toolbar buttons of the layout tool window `view`.
    pub fn tool_toolbar(&self, view: &str, cx: &mut TestAppContext) -> Vec<String> {
        let tool = self.layout_tool(view, cx);
        cx.read(|cx| {
            tool.read(cx)
                .toolbar()
                .iter()
                .map(|button| button.command.clone())
                .collect()
        })
    }

    /// Give focus to the first editor of the layout tool window `view`.
    pub fn focus_tool_editor(&self, view: &str, cx: &mut TestAppContext) {
        let tool = self.layout_tool(view, cx);
        let handle = cx.read(|cx| {
            let leaves = tool.read(cx).leaves();
            leaves
                .iter()
                .find(|leaf| leaf.editor().is_some())
                .expect("an editor")
                .focus_handle(cx)
        });
        cx.update(|cx| {
            tool.read(cx).set_focused_leaf_holding(&handle, cx);
            self.window
                .update(cx, |window, cx| window.focus_view(&handle, cx));
        });
        super::park_packages(cx);
    }

    /// Whether the first editor of the layout tool window `view` has focus.
    pub fn tool_editor_focused(&self, view: &str, cx: &mut TestAppContext) -> bool {
        let tool = self.layout_tool(view, cx);
        let handle = cx.read(|cx| {
            let leaves = tool.read(cx).leaves();
            leaves
                .iter()
                .find(|leaf| leaf.editor().is_some())
                .expect("an editor")
                .focus_handle(cx)
        });
        let native = self.native_window.expect("a windowed product");
        cx.update_window(native, |_, window, _| handle.is_focused(window))
            .unwrap()
    }

    /// Whether the tool window `stable_id` is shown.
    pub fn tool_shown(&self, stable_id: &str, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            workbench
                .tool_by_stable_id(stable_id)
                .is_some_and(|tool| workbench.tool_visible(tool))
        })
    }

    /// Click the stripe button of the tool window `stable_id`.
    pub fn click_tool_button(&self, stable_id: &str, cx: &mut TestAppContext) {
        in_window(self.native_window, cx, |cx| {
            self.window.update(cx, |window, cx| {
                let tool = window
                    .workbench
                    .read(cx)
                    .tool_by_stable_id(stable_id)
                    .expect("the tool window is registered");
                window.click_stripe_button(tool, cx);
            })
        });
        cx.run_until_parked();
    }

    /// Open an untitled document in the center.
    pub fn new_document(&self, cx: &mut TestAppContext) {
        self.window
            .update(cx, |window, cx| window.new_document_in_pane(None, cx));
        cx.run_until_parked();
    }

    /// Append `text` to the document active in the normal group.
    pub fn append(&self, text: &str, cx: &mut TestAppContext) {
        let model = cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let pane = workbench.center_pane().unwrap();
            let tab = workbench.pane(pane).unwrap().active_tab();
            workbench.editor(tab).unwrap().read(cx).model().clone()
        });
        model.update(cx, |model, cx| {
            let len = model.text().len();
            model.replace(len..len, text).unwrap();
            cx.notify();
        });
        cx.run_until_parked();
    }

    /// The text of each document tab, in layout order.
    pub fn document_texts(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            let documents = cx.global::<ApplicationDocuments>().0.read(cx);
            self.window
                .read(cx)
                .workbench
                .read(cx)
                .tab_snapshots()
                .into_iter()
                .filter_map(|tab| match tab.surface {
                    TabSurfaceId::Document(document) => documents
                        .get(document)
                        .map(|document| document.model().read(cx).text()),
                    _ => None,
                })
                .collect()
        })
    }

    /// The document tabs of the center pane as titles with whether each is a
    /// preview.
    pub fn document_tabs(&self, cx: &mut TestAppContext) -> Vec<(String, bool)> {
        cx.read(|cx| {
            let documents = cx.global::<ApplicationDocuments>().0.read(cx);
            let workbench = self.window.read(cx).workbench.read(cx);
            let Some(pane) = workbench
                .center_pane()
                .and_then(|pane| workbench.pane(pane))
            else {
                return Vec::new();
            };
            pane.tabs()
                .iter()
                .filter_map(|tab| {
                    let document = documents.get(tab.document_id()?)?;
                    Some((document.title().to_owned(), tab.is_preview()))
                })
                .collect()
        })
    }

    /// Whether the center pane's active tab is the focused one.
    pub fn editor_focused(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            workbench.focused_pane_id() == workbench.center_pane()
        })
    }

    /// Whether the window's center holds a pane.
    pub fn has_center_pane(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            self.window
                .read(cx)
                .workbench
                .read(cx)
                .center_pane()
                .is_some()
        })
    }

    /// The window's status bar texts, left side then right side.
    pub fn status_bar(&self, cx: &mut TestAppContext) -> (Vec<String>, Vec<String>) {
        cx.read(|cx| {
            let key = match &self.native_window {
                Some(native_window) => test_window_id(*native_window),
                None => window_id(self.window.entity_id()),
            };
            let items = cx.global::<ApplicationStatusBar>().0.items(&key, &[]);
            let texts = |side: Vec<BarItem>| side.into_iter().map(|bar| bar.item.text).collect();
            (texts(items.left), texts(items.right))
        })
    }

    /// The editor tabs, in layout order.
    pub fn editors(&self, cx: &mut TestAppContext) -> Vec<EditorTab> {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            workbench
                .tabs()
                .filter_map(|tab| workbench.editor(tab))
                .map(|view| {
                    let view = view.read(cx);
                    let state = view.editor().read(cx).view_state(cx);
                    EditorTab {
                        buffer: view.model().entity_id().as_u64(),
                        text: view.model().read(cx).text(),
                        anchor: state.anchor,
                        caret: state.caret,
                        top: state.top,
                    }
                })
                .collect()
        })
    }

    /// The composite view of the center's first composite tab.
    fn first_composite(
        &self,
        cx: &gpui::App,
    ) -> Option<Entity<crate::app::composite_view::CompositeView>> {
        let workbench = self.window.read(cx).workbench.read(cx);
        workbench
            .tabs()
            .find_map(|tab| workbench.composite_view(tab).cloned())
    }

    /// The tab title of each composite tab, in layout order.
    pub fn composite_titles(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            workbench
                .tabs()
                .filter_map(|tab| workbench.composite_view(tab))
                .map(|view| view.read(cx).tab().title.clone())
                .collect()
        })
    }

    /// Whether each composite tab of the center pane is its preview, in layout
    /// order.
    pub fn composite_previews(&self, cx: &mut TestAppContext) -> Vec<bool> {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let Some(pane) = workbench
                .center_pane()
                .and_then(|pane| workbench.pane(pane))
            else {
                return Vec::new();
            };
            pane.tabs()
                .iter()
                .filter(|tab| matches!(tab.surface_id(), TabSurfaceId::Composite(_)))
                .map(|tab| tab.is_preview())
                .collect()
        })
    }

    /// The editors of the first composite tab, in layout order.
    pub fn composite_editors(&self, cx: &mut TestAppContext) -> Vec<CompositeEditor> {
        cx.read(|cx| {
            let view = self.first_composite(cx).expect("a composite tab");
            view.read(cx)
                .leaves()
                .iter()
                .filter_map(|leaf| leaf.editor())
                .map(|view| {
                    let view = view.read(cx);
                    let editor = view.editor().read(cx);
                    CompositeEditor {
                        text: view.model().read(cx).text(),
                        caret_line: editor.caret().0,
                        bands: editor
                            .decorations()
                            .iter()
                            .map(|band| (band.line, band.key.clone()))
                            .collect(),
                        wedges: editor
                            .gutter_bars()
                            .iter()
                            .map(|wedge| (wedge.line, wedge.key.clone()))
                            .collect(),
                        scroll_line: editor.scroll_line(),
                        key_contexts: editor
                            .key_contexts()
                            .into_iter()
                            .map(str::to_owned)
                            .collect(),
                        blocks: editor
                            .wrap()
                            .blocks()
                            .map(|block| {
                                (
                                    block.line,
                                    block.lines.iter().map(|line| line.text.clone()).collect(),
                                )
                            })
                            .collect(),
                    }
                })
                .collect()
        })
    }

    /// The links of the first composite tab's connectors, as the lines of
    /// each side and the color key.
    pub fn composite_links(
        &self,
        cx: &mut TestAppContext,
    ) -> Vec<(std::ops::Range<u32>, std::ops::Range<u32>, String)> {
        cx.read(|cx| {
            let view = self.first_composite(cx).expect("a composite tab");
            view.read(cx).connector_links(cx)
        })
    }

    /// The lines of the first composite tab's toolbar buttons' commands.
    pub fn composite_toolbar(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            let view = self.first_composite(cx).expect("a composite tab");
            view.read(cx)
                .tab()
                .toolbar
                .iter()
                .map(|button| button.command.clone())
                .collect()
        })
    }

    /// Scroll editor `index` of the first composite tab to `line`.
    pub fn scroll_composite_editor(&self, index: usize, line: f32, cx: &mut TestAppContext) {
        let editor = cx.read(|cx| {
            let view = self.first_composite(cx).expect("a composite tab");
            let leaves = view.read(cx).leaves();
            let editor = leaves
                .iter()
                .filter_map(|leaf| leaf.editor())
                .nth(index)
                .expect("an editor");
            editor.read(cx).editor().clone()
        });
        editor.update(cx, |editor, cx| editor.scroll_to_line(line, cx));
        cx.run_until_parked();
    }

    /// Give focus to editor `index` of the first composite tab and select its tab.
    pub fn focus_composite_editor(&self, index: usize, cx: &mut TestAppContext) {
        let view = cx
            .read(|cx| self.first_composite(cx))
            .expect("a composite tab");
        let (position, handle) = cx.read(|cx| {
            let leaves = view.read(cx).leaves();
            let (position, leaf) = leaves
                .iter()
                .enumerate()
                .filter(|(_, leaf)| leaf.editor().is_some())
                .nth(index)
                .expect("an editor");
            (position, leaf.focus_handle(cx))
        });
        view.update(cx, |view, cx| view.set_focused_leaf(position, cx));
        let located = cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let pane = workbench.center_pane()?;
            let tab = workbench.pane(pane)?.tabs().iter().find(|tab| {
                workbench
                    .composite_view(tab)
                    .is_some_and(|candidate| candidate == &view)
            })?;
            Some((pane, tab.id()))
        });
        let (pane, tab) = located.expect("the composite's tab");
        self.window
            .update(cx, |window, cx| window.activate_tab(pane, tab, cx));
        cx.update(|cx| {
            self.window
                .update(cx, |window, cx| window.focus_view(&handle, cx))
        });
        self.window
            .update(cx, |window, cx| window.focus_active_surface(cx));
        super::park_packages(cx);
    }

    /// The text of editor `index` of the first composite tab, with `‸` at its
    /// caret, or at both ends of a selection.
    pub fn composite_marked(&self, index: usize, cx: &mut TestAppContext) -> String {
        cx.read(|cx| {
            let view = self.first_composite(cx).expect("a composite tab");
            let view = view.read(cx);
            let editor = view
                .leaves()
                .iter()
                .filter_map(|leaf| leaf.editor())
                .nth(index)
                .expect("an editor")
                .read(cx);
            let (anchor, caret) = editor.editor().read(cx).selection(cx);
            let mut text = editor.editor().read(cx).model().read(cx).text();
            text.insert(anchor.max(caret), crate::test_support::cases::CARET);
            if anchor != caret {
                text.insert(anchor.min(caret), crate::test_support::cases::CARET);
            }
            text
        })
    }

    /// The gutter's bar lane of the editor tab at `index`, in layout order,
    /// as line, style, and color key; a wedge drawn below its line is
    /// reported on the line after it.
    pub fn gutter_bars(
        &self,
        index: usize,
        cx: &mut TestAppContext,
    ) -> Vec<(usize, &'static str, String)> {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let view = workbench
                .tabs()
                .filter_map(|tab| workbench.editor(tab))
                .nth(index)
                .expect("an editor tab");
            view.read(cx)
                .editor()
                .read(cx)
                .gutter_bars()
                .iter()
                .map(|mark| {
                    let style = match mark.style {
                        knot_protocol::GutterStyle::Dot => "dot",
                        knot_protocol::GutterStyle::Bar => "bar",
                        knot_protocol::GutterStyle::Deletion => "deletion",
                    };
                    (mark.line + usize::from(mark.below), style, mark.key.clone())
                })
                .collect()
        })
    }

    /// The first editor tab's open popovers: the lines each is anchored to,
    /// its toolbar tooltips, and the text of each editor it holds.
    pub fn popovers(&self, cx: &mut TestAppContext) -> Vec<TestPopover> {
        cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let view = workbench
                .tabs()
                .find_map(|tab| workbench.editor(tab))
                .expect("an editor tab");
            let view = view.read(cx);
            let editor = view.editor().read(cx);
            let model = view.model().read(cx);
            let popovers = editor.popovers().popovers();
            editor
                .popover_contents()
                .into_iter()
                .zip(popovers)
                .map(|(content, popover)| {
                    let content = content.read(cx);
                    TestPopover {
                        id: popover.id.clone(),
                        anchor: editor.popovers().anchor_lines(popover, model).unwrap(),
                        buttons: content.toolbar_labels(),
                        texts: content
                            .leaves()
                            .iter()
                            .filter_map(|leaf| leaf.editor())
                            .map(|view| view.read(cx).model().read(cx).text())
                            .collect(),
                    }
                })
                .collect()
        })
    }

    /// Click the toolbar button of the first editor's popover with the tooltip `label`.
    pub fn click_popover_button(&self, label: &str, cx: &mut TestAppContext) {
        let native_window = self.native_window.expect("a native window");
        let index = self.popovers(cx)[0]
            .buttons
            .iter()
            .position(|button| button == label)
            .expect("a toolbar button");
        let selector: &'static str = Box::leak(format!("popover-toolbar-{index}").into_boxed_str());
        cx.refresh().unwrap();
        let mut visual = VisualTestContext::from_window(native_window, cx);
        let at = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
        visual.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
    }

    /// Click the bar lane of the gutter on `line` of the first editor tab.
    pub fn click_gutter_bar(&self, line: usize, cx: &mut TestAppContext) {
        let native_window = self.native_window.expect("a native window");
        let at = cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let view = workbench
                .tabs()
                .find_map(|tab| workbench.editor(tab))
                .expect("an editor tab");
            view.read(cx).click_point_for_test(Some(true), line)
        });
        let mut visual = VisualTestContext::from_window(native_window, cx);
        visual.simulate_click(at, gpui::Modifiers::default());
        visual.run_until_parked();
    }

    /// Split the normal group's pane, moving focus to the new pane beside it.
    pub fn split(&self, cx: &mut TestAppContext) {
        self.window.update(cx, |window, cx| {
            let pane = window.workbench.read(cx).center_pane().unwrap();
            window.workbench.update(cx, |workbench, cx| {
                workbench.split_pane(pane, SplitDirection::Horizontal, SplitPlacement::After, cx);
            });
        });
        cx.run_until_parked();
    }

    /// Move the tool window `stable_id` to `slot` (such as `rightTop`), as a
    /// drag of its stripe button does.
    pub fn move_tool(&self, stable_id: &str, slot: &str, cx: &mut TestAppContext) {
        let slot = serde_json::from_value(serde_json::json!(slot)).expect("a slot name");
        self.window.update(cx, |window, cx| {
            let tool = window
                .workbench
                .read(cx)
                .tool_by_stable_id(stable_id)
                .expect("a registered tool window");
            window.move_tool_to_slot(tool, slot, cx);
        });
        cx.run_until_parked();
    }

    /// Size the dock `dock`, `left`, `right`, or `bottom`, to `size`.
    pub fn set_dock_size(&self, dock: &str, size: f32, cx: &mut TestAppContext) {
        let location = dock_location(dock);
        self.window.update(cx, |window, cx| {
            window
                .workbench
                .update(cx, |workbench, _| workbench.set_side_extent(location, size));
        });
        cx.run_until_parked();
    }

    /// Select from `anchor` to `caret` in the focused editor and scroll line
    /// `top` to its top.
    pub fn set_view(&self, anchor: usize, caret: usize, top: usize, cx: &mut TestAppContext) {
        let editor = cx.read(|cx| {
            self.window
                .read(cx)
                .workbench
                .read(cx)
                .focused_editor()
                .unwrap()
                .read(cx)
                .editor()
                .clone()
        });
        editor.update(cx, |editor, cx| {
            editor.restore_view_state(
                EditorViewState {
                    anchor,
                    caret,
                    top,
                    scroll_x: 0.,
                    folds: Vec::new(),
                },
                cx,
            )
        });
        cx.run_until_parked();
    }

    /// The window's layout as packages read it.
    pub fn layout(&self, cx: &mut TestAppContext) -> Value {
        let target = knot_protocol::WindowTarget {
            window: window_id(self.window.entity_id()),
        };
        let layout = cx
            .update(|cx| crate::app::product::sessions::capture_layout(&target, cx))
            .unwrap();
        serde_json::to_value(layout).unwrap()
    }

    /// Focus the center pane's active document.
    pub fn focus_document(&self, cx: &mut TestAppContext) {
        let located = cx.read(|cx| {
            let workbench = self.window.read(cx).workbench.read(cx);
            let pane = workbench.center_pane()?;
            Some((pane, workbench.pane(pane)?.active_tab_id()))
        });
        let (pane, tab) = located.expect("a normal pane");
        self.window
            .update(cx, |window, cx| window.activate_tab(pane, tab, cx));
        cx.run_until_parked();
    }

    /// The tree contributed as view `id`, waiting until it attaches.
    pub fn tree(&self, id: &str, cx: &mut TestAppContext) -> TestTree {
        let mut tree = None;
        wait_until(cx, |cx| {
            tree = cx.read(|cx| {
                let window = self.window.read(cx);
                window
                    .contributed_views
                    .get(id)
                    .map(|view| view.tree().clone())
                    .or_else(|| {
                        // An embedded tree lives in the layout that hosts it.
                        window
                            .contributed_views
                            .values()
                            .filter(|view| {
                                matches!(
                                    view.content,
                                    crate::app::workbench::ExtensionView::Layout(..)
                                )
                            })
                            .flat_map(|view| view.layout().read(cx).hosted_views())
                            .find_map(|hosted| match &hosted {
                                crate::app::workbench::ExtensionView::Tree(tree)
                                    if hosted.kind(cx) == id =>
                                {
                                    Some(tree.clone())
                                }
                                _ => None,
                            })
                    })
            });
            tree.is_some()
        });
        TestTree {
            window: self.window.clone(),
            native_window: self.native_window,
            tree: tree.unwrap(),
        }
    }
}

impl TestTree {
    /// The labels of the visible rows, in order.
    pub fn labels(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| self.tree.read(cx).visible_labels())
    }

    /// Whether a row has the id `row`.
    pub fn has_row(&self, row: &str, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.tree.read(cx).status_for_test(row).is_some())
    }

    /// The status the row `row` shows.
    pub fn status(&self, row: &str, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| {
            self.tree
                .read(cx)
                .status_for_test(row)
                .and_then(|(status, _)| status)
        })
    }

    /// The decoration painted on the row `row`: its color key, badge, and
    /// tooltip. A badge reads `icon:name` or `text:M`.
    pub fn decoration(
        &self,
        row: &str,
        cx: &mut TestAppContext,
    ) -> Option<(Option<String>, Option<String>, Option<String>)> {
        use crate::app::resource_decorations::Badge;
        cx.read(|cx| {
            let decoration = self.tree.read(cx).decoration_for_test(row, cx)?;
            let badge = decoration.badge.map(|badge| match badge {
                Badge::Icon(name) => format!("icon:{name}"),
                Badge::Text(text) => format!("text:{text}"),
            });
            Some((decoration.color, badge, decoration.tooltip))
        })
    }

    /// Whether the row at flat position `index` painted a decoration badge.
    pub fn paints_decoration_badge(&self, index: usize, cx: &mut TestAppContext) -> bool {
        let native_window = self.native_window.expect("the window is open");
        cx.refresh().unwrap();
        VisualTestContext::from_window(native_window, cx)
            .debug_bounds(Box::leak(
                format!("extension-tree-decoration-{index}").into_boxed_str(),
            ))
            .is_some()
    }

    /// The second line the row `row` shows.
    pub fn detail(&self, row: &str, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).detail_for_test(row))
    }

    /// The description the row `row` shows.
    pub fn description(&self, row: &str, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).description_for_test(row))
    }

    /// The commands of the row `row`'s actions.
    pub fn row_commands(&self, row: &str, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            self.tree
                .read(cx)
                .actions_for_row_for_test(row)
                .into_iter()
                .map(|action| action.command)
                .collect()
        })
    }

    /// The commands of the actions beside the rows.
    pub fn frame_commands(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| {
            self.tree
                .read(cx)
                .frame_actions_for_test()
                .into_iter()
                .map(|action| action.command)
                .collect()
        })
    }

    /// The selected row's label.
    pub fn selected(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).selected_label())
    }

    /// Whether a tab of the window shows the tree.
    pub fn is_attached(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.located(cx).is_some())
    }

    /// Whether the tree is the focused pane's active tab.
    pub fn is_focused(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            let focus = self.tree.read(cx).focus_handle(cx);
            let workbench = self.window.read(cx).workbench.read(cx);
            workbench.focused_pane().is_some_and(|pane| {
                workbench
                    .command_view(pane.active_tab())
                    .is_some_and(|view| view.focus_handle(cx) == focus)
            })
        })
    }

    /// Activate the tree's tab and focus the tree, as a click would.
    pub fn focus(&self, cx: &mut TestAppContext) {
        self.activate_tab(cx);
        in_window(self.native_window, cx, |cx| {
            let handle = self.tree.read(cx).focus_handle(cx);
            // A tree in a layout tool window is its layout's focused view.
            let window = self.window.read(cx);
            for view in window.contributed_views.values() {
                if matches!(
                    view.content,
                    crate::app::workbench::ExtensionView::Layout(..)
                ) {
                    view.layout().read(cx).set_focused_leaf_holding(&handle, cx);
                }
            }
            self.window
                .update(cx, |window, cx| window.focus_view(&handle, cx));
        });
        cx.refresh().unwrap();
        cx.run_until_parked();
    }

    /// Activate the row `row`, as Enter on it would.
    pub fn activate_row(&self, row: &str, cx: &mut TestAppContext) {
        self.activate_tab(cx);
        in_window(self.native_window, cx, |cx| {
            self.tree
                .update(cx, |tree, cx| tree.activate_row_for_test(row, cx));
        });
        cx.run_until_parked();
    }

    /// Whether an inline edit is open in the tree.
    pub fn is_editing(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.tree.read(cx).is_editing())
    }

    /// The text of the open inline edit.
    pub fn edit_text(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).edit_text_for_test(cx))
    }

    /// The message shown under the open inline edit.
    pub fn edit_message(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).edit_message_for_test())
    }

    /// The ID of the first visible row labelled `label`.
    pub fn row_id(&self, label: &str, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).row_id_for_test(label))
    }

    /// Whether the row `row` is painted dimmed.
    pub fn is_muted(&self, row: &str, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.tree.read(cx).muted_for_test(row))
    }

    /// The selected row's ID.
    pub fn selected_id(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).selected_id_for_test())
    }

    /// Activate the row labelled `label` with a gesture (`click`,
    /// `doubleClick`, `middleClick`, `rightClick`, or `enter`).
    pub fn gesture(&self, label: &str, gesture: &str, cx: &mut TestAppContext) {
        let row = self
            .row_id(label, cx)
            .unwrap_or_else(|| panic!("no visible row labelled {label}"));
        self.focus(cx);
        in_window(self.native_window, cx, |cx| {
            self.tree.update(cx, |tree, cx| {
                tree.activate_row_as_for_test(&row, gesture, cx)
            });
        });
        cx.run_until_parked();
    }

    /// Invoke the row `row`'s action running `command`, as a click on it
    /// would.
    pub fn invoke(&self, row: &str, command: &str, cx: &mut TestAppContext) {
        let action = cx
            .read(|cx| self.tree.read(cx).actions_for_row_for_test(row))
            .into_iter()
            .find(|action| action.command == command)
            .unwrap_or_else(|| panic!("{row} has no {command} action"));
        self.focus(cx);
        in_window(self.native_window, cx, |cx| {
            self.tree
                .update(cx, |tree, cx| tree.invoke_action(action, cx));
        });
    }

    fn located(
        &self,
        cx: &gpui::App,
    ) -> Option<(crate::app::workbench::PaneId, crate::app::workbench::TabId)> {
        let focus = self.tree.read(cx).focus_handle(cx);
        let workbench = self.window.read(cx).workbench.read(cx);
        workbench
            .panes()
            .iter()
            .filter(|pane| workbench.area_shown(pane.area()))
            .find_map(|pane| {
                pane.tabs()
                    .iter()
                    .find(|tab| workbench.tab_holds(tab, &focus, cx))
                    .map(|tab| (pane.id(), tab.id()))
            })
    }

    fn activate_tab(&self, cx: &mut TestAppContext) {
        let (pane, tab) = cx.read(|cx| self.located(cx)).expect("the tree has a tab");
        self.window
            .update(cx, |window, cx| window.activate_tab(pane, tab, cx));
        cx.run_until_parked();
    }
}

fn open_target(window: &Entity<KnotWindow>, cx: &gpui::App) -> OpenTarget {
    let target_window = window.downgrade();
    let window = window.read(cx);
    let workbench = window.workbench.read(cx);
    // A window whose center has no pane opens into one.
    let tab = workbench.focused_pane().map(|pane| {
        let tab = pane.active_tab();
        crate::app::commands::dispatch::TargetTab {
            pane: pane.id(),
            tab: tab.id(),
            surface: tab.surface_id(),
            presentation: tab.presentation(),
        }
    });
    OpenTarget {
        window: target_window,
        workbench: window.workbench.downgrade(),
        tab,
    }
}

fn dock_location(dock: &str) -> DockLocation {
    match dock {
        "left" => DockLocation::Left,
        "right" => DockLocation::Right,
        "bottom" => DockLocation::Bottom,
        other => panic!("no dock {other}"),
    }
}
