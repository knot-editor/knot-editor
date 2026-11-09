//! One editor session of the product, driven through its key router.
//!
//! A session shows one document's editor focused in a workbench, with or
//! without a window, and takes keys as a window would forward them. Each
//! input settles once the session holds no keys and the extension pool has
//! gone quiet. Keys use the [case](crate::test_support::cases) notation.

use crate::app::documents::DocumentDirty;
use gpui::{
    AppContext, Bounds, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext,
};
use knot_protocol::{Command, CommandArgumentValue, CommandOutcome};
use serde_json::Value;

use super::settle;
use super::setup::product_window;
use crate::app::convert::keystroke_to_gpui;
use crate::app::editor::{CaretShape, Editor, EditorView, Reveal};
use crate::app::keys::Keystroke;
use crate::app::model::{Buffer, BufferModel};
use crate::app::product::{
    ApplicationDocuments, ApplicationProductCommands, ApplicationSessions, DocumentId,
    ProductShell, Workbench,
};
use crate::app::resource::ResourceUri;
use crate::app::settings::{ApplicationSettings, SettingsScope};
use crate::app::status_bar::ApplicationStatusBar;
use crate::test_support::cases::{CARET, parse_keys};

/// The size a session lays its editor out at, as a window would, in rows
/// and cells.
const ROWS: usize = 40;
const COLUMNS: usize = 100;

/// An editor session over one document.
pub struct TestEditor {
    shell: Entity<ProductShell>,
    editor: Entity<Editor>,
    model: Entity<Buffer>,
    document: DocumentId,
    window: Option<(gpui::AnyWindowHandle, Entity<EditorView>)>,
}

/// The draws and host requests one key caused.
#[derive(Clone, Debug)]
pub struct Work {
    pub draws: usize,
    pub requests: Vec<&'static str>,
}

impl TestEditor {
    pub(super) fn open(
        text: &str,
        language: Option<&str>,
        selection: (usize, usize),
        cx: &mut TestAppContext,
    ) -> Self {
        let (document, model) = new_document(text, language, cx);
        Self::over(document, model, selection, cx)
    }

    pub(super) fn over(
        document: DocumentId,
        model: Entity<Buffer>,
        selection: (usize, usize),
        cx: &mut TestAppContext,
    ) -> Self {
        let shell = cx.update(|cx| {
            let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
            cx.new(|cx| ProductShell::new(workbench, cx))
        });
        shell.update(cx, |shell, cx| {
            shell.observe_application(cx);
            shell.activate(cx);
        });
        let editor = active_editor(&shell, cx);
        let session = Self {
            shell,
            editor,
            model,
            document,
            window: None,
        };
        session.show(selection, cx);
        session
    }

    pub(super) fn windowed(text: &str, cx: &mut TestAppContext) -> Self {
        let (document, model) = new_document(text, None, cx);
        let (shell, window) = product_window(document, model.clone(), cx);
        let view = active_view(&shell, cx);
        let editor = cx.read(|cx| view.read(cx).editor().clone());
        cx.update_window(window, |_, window, cx| {
            window.activate_window();
            editor.update(cx, |editor, cx| {
                editor.set_selection(0, 0, Reveal::Nearest, cx)
            });
            shell.update(cx, |shell, cx| shell.focus_active_editor(cx))
        })
        .unwrap();
        cx.refresh().unwrap();
        let session = Self {
            shell,
            editor,
            model,
            document,
            window: Some((window, view)),
        };
        session.settle(cx);
        session
    }

    /// Show `text` in `language`, or detected, in a fresh workbench in place
    /// of the previous document, which is released, with the selection from
    /// `anchor` to `caret`, and settle.
    pub fn load(
        &mut self,
        text: &str,
        language: Option<&str>,
        selection: (usize, usize),
        cx: &mut TestAppContext,
    ) {
        let (document, model) = new_document(text, language, cx);
        self.shell.update(cx, |shell, cx| {
            let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
            cx.global::<ApplicationSessions>()
                .register(cx.weak_entity(), &workbench);
            shell.workbench = workbench;
            cx.notify();
        });
        let previous = std::mem::replace(&mut self.document, document);
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        documents.update(cx, |documents, cx| documents.remove(previous, cx));
        self.model = model;
        self.editor = active_editor(&self.shell, cx);
        self.show(selection, cx);
    }

    /// Focus the editor, select from `anchor` to `caret`, lay it out, and
    /// settle.
    fn show(&self, (anchor, caret): (usize, usize), cx: &mut TestAppContext) {
        self.shell
            .update(cx, |shell, cx| shell.focus_active_surface(cx));
        self.select(anchor, caret, cx);
        self.lay_out(ROWS, COLUMNS, cx);
        self.settle(cx);
    }

    /// Run until the session holds no keys and every package is quiet.
    pub fn settle(&self, cx: &mut TestAppContext) {
        settle(&self.shell, cx);
    }

    /// Press `keys` without waiting between them, as fast typing arrives,
    /// then settle. A command that waits on input, such as a picker or a
    /// prompt, keeps its package busy, so keys that open one go through
    /// [`Self::press_unsettled`] instead.
    pub fn press(&self, keys: &str, cx: &mut TestAppContext) {
        self.press_unsettled(keys, cx);
        self.settle(cx);
    }

    /// Press `keys` and return at once, for keys that leave a command
    /// waiting on input.
    pub fn press_unsettled(&self, keys: &str, cx: &mut TestAppContext) {
        for key in parse_keys(keys) {
            let keystroke = Keystroke::parse(&key).unwrap();
            match &self.window {
                Some((window, _)) => cx.dispatch_keystroke(*window, keystroke_to_gpui(&keystroke)),
                None => self
                    .shell
                    .update(cx, |shell, cx| shell.press_key(keystroke, cx)),
            }
        }
    }

    /// Type `text` into the window's focused input, such as a picker's.
    pub fn type_input(&self, text: &str, cx: &mut TestAppContext) {
        let (window, _) = self.window.as_ref().expect("typed input needs a window");
        let window = *window;
        cx.simulate_input(window, text);
    }

    /// Run the command `name` without arguments against the focused
    /// surface, settle, and require that it completed.
    pub fn run_command(&self, name: &str, cx: &mut TestAppContext) {
        let target = self
            .shell
            .update(cx, |shell, cx| shell.capture_command_target(cx));
        let mut execution = cx.update(|cx| {
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
        });
        self.settle(cx);
        assert_eq!(
            execution.completion.try_recv(),
            Ok(CommandOutcome::Completed),
            "{name}"
        );
    }

    /// Lay the editor out `rows` rows tall and `columns` cells wide, as a
    /// window would.
    pub fn lay_out(&self, rows: usize, columns: usize, cx: &mut TestAppContext) {
        self.editor.update(cx, |editor, cx| {
            editor.lay_out_for_test(rows as f32, columns, cx)
        });
    }

    /// Select from `anchor` to `caret`.
    pub fn select(&self, anchor: usize, caret: usize, cx: &mut TestAppContext) {
        self.editor.update(cx, |editor, cx| {
            editor.set_selection(anchor, caret, Reveal::Nearest, cx)
        });
    }

    /// The document's text with `‸` at the caret, or at both ends of a
    /// selection.
    pub fn marked(&self, cx: &mut TestAppContext) -> String {
        cx.read(|cx| {
            let mut text = self.model.read(cx).text();
            let (anchor, caret) = self.editor.read(cx).selection(cx);
            text.insert(anchor.max(caret), CARET);
            if anchor != caret {
                text.insert(anchor.min(caret), CARET);
            }
            text
        })
    }

    /// The caret's byte offset.
    pub fn caret(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| self.editor.read(cx).selection(cx).1)
    }

    /// The line holding the byte `offset`.
    pub fn line_of(&self, offset: usize, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| {
            let model = self.model.read(cx);
            (0..model.line_count())
                .rev()
                .find(|line| model.line_range(*line).start <= offset)
                .unwrap()
        })
    }

    /// The key contexts packages set on the editor.
    pub fn contexts(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| editor_contexts(&self.editor, cx))
    }

    /// The caret's shape: `bar`, `block`, or `underline`.
    pub fn caret_shape(&self, cx: &mut TestAppContext) -> &'static str {
        match cx.read(|cx| self.editor.read(cx).caret_shape()) {
            CaretShape::Bar => "bar",
            CaretShape::Block => "block",
            CaretShape::Underline => "underline",
        }
    }

    /// The session's status bar texts, left side then right side.
    pub fn status(&self, cx: &mut TestAppContext) -> (Vec<String>, Vec<String>) {
        cx.read(|cx| {
            let key = match &self.window {
                Some((window, _)) => crate::app::product::test_session_id(*window),
                None => crate::app::product::session_id(self.shell.entity_id()),
            };
            let items = cx.global::<ApplicationStatusBar>().0.items(&key, &[]);
            let texts = |side: Vec<crate::app::status_bar::BarItem>| {
                side.into_iter().map(|bar| bar.item.text).collect()
            };
            (texts(items.left), texts(items.right))
        })
    }

    /// Whether a transient surface, such as a picker, is open.
    pub fn transient_open(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.shell.read(cx).transient.is_some())
    }

    /// Scroll the view by `rows` rows, down when positive.
    pub fn scroll_rows(&self, rows: i64, cx: &mut TestAppContext) {
        self.editor
            .update(cx, |editor, cx| editor.scroll_by_rows(rows, cx));
    }

    /// The view's first row and its number of rows.
    pub fn viewport(&self, cx: &mut TestAppContext) -> (usize, usize) {
        cx.read(|cx| self.editor.read(cx).viewport())
    }

    /// The line the view's row `row` belongs to.
    pub fn line_of_row(&self, row: usize, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| self.editor.read(cx).row_start(row, cx).0)
    }

    /// The row showing column `column` of line `line`.
    pub fn row_of(&self, line: usize, column: usize, cx: &mut TestAppContext) -> Option<usize> {
        cx.read(|cx| self.editor.read(cx).row_of_position(line, column, cx))
    }

    /// Collapse lines `first` to `last` into a fold; whether they fold.
    pub fn fold(&self, first: usize, last: usize, cx: &mut TestAppContext) -> bool {
        self.editor
            .update(cx, |editor, cx| editor.fold(first..last + 1, cx))
    }

    /// The collapsed folds as line ranges, first line to one past the last.
    pub fn folds(&self, cx: &mut TestAppContext) -> Vec<std::ops::Range<usize>> {
        cx.read(|cx| self.editor.read(cx).folds())
    }

    /// Set the document's language, as detection or a user would.
    pub fn set_language(&self, language: &str, cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::app::languages::set_language(&self.model, language.into(), cx);
            crate::app::application_observation::changed(cx);
        });
        self.settle(cx);
    }

    fn scope(&self, cx: &mut TestAppContext) -> SettingsScope {
        let identity = cx.read(|cx| {
            cx.global::<ApplicationDocuments>()
                .0
                .read(cx)
                .get(self.document)
                .unwrap()
                .identity()
        });
        SettingsScope {
            language: None,
            view: Some("editor".into()),
            document: Some(identity.to_string()),
        }
    }

    /// The value of the setting `key` for this document's editor.
    pub fn setting(&self, key: &str, cx: &mut TestAppContext) -> Option<Value> {
        let scope = self.scope(cx);
        cx.read(|cx| {
            cx.global::<ApplicationSettings>()
                .0
                .get(key, &scope)
                .cloned()
        })
    }

    /// Give this document its own value of the setting `key`.
    pub fn set_setting(&self, key: &str, value: Value, cx: &mut TestAppContext) {
        let SettingsScope { document, .. } = self.scope(cx);
        cx.update(|cx| {
            cx.default_global::<ApplicationSettings>()
                .0
                .set(
                    key,
                    value,
                    SettingsScope {
                        document,
                        ..Default::default()
                    },
                )
                .unwrap();
            crate::app::application_observation::changed(cx);
        });
        self.settle(cx);
    }

    /// Whether the document has unsaved changes.
    pub fn is_dirty(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            let documents = cx.global::<ApplicationDocuments>().0.read(cx);
            documents.get(self.document).unwrap().is_dirty(cx)
        })
    }

    /// Whether the document is still open.
    pub fn is_open(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            cx.global::<ApplicationDocuments>()
                .0
                .read(cx)
                .get(self.document)
                .is_some()
        })
    }

    /// Open a new untitled tab in the focused pane, and settle.
    pub fn new_tab(&self, cx: &mut TestAppContext) {
        self.shell.update(cx, |shell, cx| shell.new_tab(cx));
        self.settle(cx);
    }

    /// Whether the session's active tab shows this document's editor.
    pub fn is_active(&self, cx: &mut TestAppContext) -> bool {
        active_editor(&self.shell, cx) == self.editor
    }

    /// The text of the session's active editor.
    pub fn active_text(&self, cx: &mut TestAppContext) -> String {
        let editor = active_editor(&self.shell, cx);
        cx.read(|cx| editor.read(cx).model().read(cx).text())
    }

    /// The key contexts of the session's active editor.
    pub fn active_contexts(&self, cx: &mut TestAppContext) -> Vec<String> {
        let editor = active_editor(&self.shell, cx);
        cx.read(|cx| editor_contexts(&editor, cx))
    }

    /// Replace the session's workspace roots.
    pub fn set_roots(&self, uris: &[&str], cx: &mut TestAppContext) {
        let roots = uris
            .iter()
            .map(|uri| ResourceUri::parse(uri).unwrap())
            .collect();
        self.shell
            .update(cx, |shell, cx| shell.replace_workspace_roots(roots, cx));
    }

    /// How many panes the workbench holds.
    pub fn pane_count(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| self.shell.read(cx).workbench.read(cx).panes().len())
    }

    /// The focused pane's index in layout order.
    pub fn focused_pane(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| {
            let workbench = self.shell.read(cx).workbench.read(cx);
            let focused = workbench.focused_pane_id().unwrap();
            workbench
                .panes()
                .iter()
                .position(|pane| pane.id() == focused)
                .unwrap()
        })
    }

    /// The share of its split the pane at `index` in layout order takes.
    pub fn split_ratio(&self, index: usize, cx: &mut TestAppContext) -> Option<f32> {
        cx.read(|cx| {
            let workbench = self.shell.read(cx).workbench.read(cx);
            workbench.split_ratio(workbench.panes()[index].id())
        })
    }

    /// The window's whole editor area and the part showing text.
    pub fn bounds(&self, cx: &mut TestAppContext) -> (Bounds<Pixels>, Bounds<Pixels>) {
        let (_, view) = self.window.as_ref().expect("bounds need a window");
        cx.read(|cx| {
            let view = view.read(cx);
            (view.interaction_bounds(), view.text_bounds())
        })
    }

    /// Press, move, and release the left button through the window, then
    /// settle; a click when `to` is `from`.
    pub fn drag(&self, from: Point<Pixels>, to: Point<Pixels>, cx: &mut TestAppContext) {
        let (window, _) = self.window.as_ref().expect("pointer input needs a window");
        let window = *window;
        let mut window_cx = VisualTestContext::from_window(window, cx);
        window_cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        if to != from {
            window_cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
        }
        window_cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        self.settle(cx);
    }

    /// The draws and host requests each key causes, pressed one at a time
    /// through the window and settled before the next.
    pub fn work_per_key(&self, keys: &[&str], cx: &mut TestAppContext) -> Vec<Work> {
        assert!(self.window.is_some(), "draws need a window");
        crate::app::work_counts::take();
        keys.iter()
            .map(|key| {
                self.press(key, cx);
                let counts = crate::app::work_counts::take();
                Work {
                    draws: counts.draws,
                    requests: counts.requests,
                }
            })
            .collect()
    }
}

/// A new untitled document holding `text`, in `language` when given.
fn new_document(
    text: &str,
    language: Option<&str>,
    cx: &mut TestAppContext,
) -> (DocumentId, Entity<Buffer>) {
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Case", model.clone(), cx)
    });
    if let Some(language) = language {
        cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
    }
    (document, model)
}

fn active_view(shell: &Entity<ProductShell>, cx: &mut TestAppContext) -> Entity<EditorView> {
    cx.read(|cx| {
        shell
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    })
}

fn active_editor(shell: &Entity<ProductShell>, cx: &mut TestAppContext) -> Entity<Editor> {
    let view = active_view(shell, cx);
    cx.read(|cx| view.read(cx).editor().clone())
}

fn editor_contexts(editor: &Entity<Editor>, cx: &gpui::App) -> Vec<String> {
    editor
        .read(cx)
        .key_contexts()
        .into_iter()
        .map(str::to_owned)
        .collect()
}
