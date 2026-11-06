//! A headless product for package behavior tests. The application and its
//! bundled packages start without a window; one session shows each case's
//! document in turn and takes keys through its router as a window would
//! forward them. A run settles once the session holds no keys and the extension
//! pool has gone quiet, waiting on finished turns rather than polling.
//! The key notation is described with the [cases](super::cases).
use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::Keystroke;

use super::cases::{CARET, parse_keys};
use super::editor_api::active_editor_view;
use super::*;
use crate::app::editor::{Editor, EditorViewState, Reveal};
use crate::app::extensions::load::LoadResult;
use crate::app::product::DocumentId;
use crate::app::status_bar::ApplicationStatusBar;

/// The size a session lays its editor out at, as a window would, in rows
/// and cells.
const ROWS: usize = 40;
const COLUMNS: usize = 100;

/// How long a run may take to settle before the test fails.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);

/// Start every bundled package under a personal config whose `post-init.js`
/// is `post_init`, when given, with no installed packages, and require that
/// each one loaded.
pub(super) fn start_bundled_packages_with_post_init(
    post_init: Option<&str>,
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    if let Some(source) = post_init {
        std::fs::write(config.path().join("post-init.js"), source).unwrap();
    }
    let extensions = tempfile::tempdir().unwrap();
    install_globals(cx);
    // Product startup binds keys before packages start, so they can bind theirs.
    cx.update(super::super::launch::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(crate::app::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let report = cx
        .read(crate::app::extensions::host::startup_report)
        .unwrap();
    let failures: Vec<_> = report
        .report
        .entries
        .iter()
        .filter(|entry| entry.result != LoadResult::Loaded)
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
    // Launch opens a window when nothing else does; close it, since a
    // window redraws on every change its session sees.
    for window in cx.windows() {
        cx.update_window(window, |_, window, _| window.remove_window())
            .unwrap();
    }
    cx.run_until_parked();
}

/// Start the bundled packages with an empty personal config.
pub(super) fn start_bundled_packages(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(None, cx);
}

/// Run until `shell` holds no keys and the extension pool is quiet: no
/// package has work in progress, and none finished a turn while the
/// foreground ran what it had.
pub(super) fn settle(shell: &Entity<ProductShell>, cx: &mut TestAppContext) {
    let pool = cx.read(crate::app::extensions::host::pool);
    let Some(pool) = pool else {
        cx.run_until_parked();
        return;
    };
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    loop {
        let before = pool.activity();
        cx.run_until_parked();
        let after = pool.activity();
        let held = cx.read(|cx| shell.read(cx).holds_keys());
        if after == before && !after.busy && !held {
            return;
        }
        if after.finished_turns == before.finished_turns {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                pool.wait_for_turn(after.finished_turns, remaining),
                "the session never settled: {}",
                if held {
                    "it still holds keys"
                } else {
                    "a package is still busy"
                }
            );
        }
    }
}

/// A windowless session over one document, its editor focused.
pub(super) struct Session {
    pub(super) shell: Entity<ProductShell>,
    pub(super) editor: Entity<Editor>,
    pub(super) model: Entity<BufferModel>,
    pub(super) document: DocumentId,
}

impl Session {
    /// Open a session over `text` in `language`, or detected, with the
    /// selection from `anchor` to `caret`, and settle.
    pub(super) fn open(
        text: &str,
        language: Option<&str>,
        selection: (usize, usize),
        cx: &mut TestAppContext,
    ) -> Self {
        let (document, model) = new_document(text, language, cx);
        let shell = cx.update(|cx| {
            let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
            cx.new(|cx| ProductShell::new(workbench, cx))
        });
        shell.update(cx, |shell, cx| {
            shell.observe_application(cx);
            shell.activate(cx);
        });
        let view = active_editor_view(&shell, cx);
        let editor = cx.read(|cx| view.read(cx).editor().clone());
        let session = Self {
            shell,
            editor,
            model,
            document,
        };
        session.show(selection, cx);
        session
    }

    /// Show `text` in `language`, or detected, in a fresh workbench in
    /// place of the previous document, which is released, with the
    /// selection from `anchor` to `caret`, and settle.
    pub(super) fn load(
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
        let view = active_editor_view(&self.shell, cx);
        self.editor = cx.read(|cx| view.read(cx).editor().clone());
        self.show(selection, cx);
    }

    /// Focus the editor, select from `anchor` to `caret`, lay it out, and
    /// settle.
    fn show(&self, (anchor, caret): (usize, usize), cx: &mut TestAppContext) {
        self.shell
            .update(cx, |shell, cx| shell.focus_active_surface(cx));
        self.editor.update(cx, |editor, cx| {
            editor.set_selection(anchor, caret, Reveal::Nearest, cx)
        });
        self.lay_out(ROWS, COLUMNS, cx);
        self.settle(cx);
    }

    pub(super) fn settle(&self, cx: &mut TestAppContext) {
        settle(&self.shell, cx);
    }

    /// Press `keys` in the key notation without waiting between them, as
    /// fast typing arrives, then settle.
    pub(super) fn press(&self, keys: &str, cx: &mut TestAppContext) {
        for key in parse_keys(keys) {
            let keystroke = Keystroke::parse(&key).unwrap();
            self.shell
                .update(cx, |shell, cx| shell.press_key(keystroke, cx));
        }
        self.settle(cx);
    }

    /// Lay the editor out `rows` rows tall and `columns` cells wide, as a
    /// window would.
    pub(super) fn lay_out(&self, rows: usize, columns: usize, cx: &mut TestAppContext) {
        self.editor.update(cx, |editor, cx| {
            editor.lay_out_for_test(rows as f32, columns, cx)
        });
    }

    /// The text with `‸` at the caret, or at both ends of a selection.
    pub(super) fn marked(&self, cx: &mut TestAppContext) -> String {
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

    pub(super) fn contexts(&self, cx: &gpui::App) -> Vec<String> {
        self.editor
            .read(cx)
            .key_contexts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// The session's status bar texts, left side then right side.
    pub(super) fn status(&self, cx: &gpui::App) -> (Vec<String>, Vec<String>) {
        let key = crate::app::product::session_id(self.shell.entity_id());
        let items = cx.global::<ApplicationStatusBar>().0.items(&key, &[]);
        let texts = |side: Vec<crate::app::status_bar::BarItem>| {
            side.into_iter().map(|bar| bar.item.text).collect()
        };
        (texts(items.left), texts(items.right))
    }
}

/// A new untitled document holding `text`, in `language` when given.
fn new_document(
    text: &str,
    language: Option<&str>,
    cx: &mut TestAppContext,
) -> (DocumentId, Entity<BufferModel>) {
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let model = cx.new(|_| BufferModel::from_text(text));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Case", model.clone(), cx)
    });
    if let Some(language) = language {
        cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
    }
    (document, model)
}

/// In typed text, presses Backspace.
pub(super) const BACKSPACE: char = '⌫';

/// What a case does at its selection.
pub(super) enum Action<'a> {
    Command(&'a str),
    /// Type the text one character at a time, pressing Backspace for `⌫`.
    Type(&'a str),
    /// Leave the view as it is.
    Inspect,
}

/// What a run left: the text, the selection, and the brackets the view
/// highlights.
pub(super) struct Outcome {
    pub(super) text: String,
    pub(super) anchor: usize,
    pub(super) caret: usize,
    pub(super) matched: Option<[Range<usize>; 2]>,
}

impl Outcome {
    /// The text with `‸` at the caret, or at both ends of the selection.
    pub(super) fn marked(&self) -> String {
        let mut marked = self.text.clone();
        marked.insert(self.anchor.max(self.caret), CARET);
        if self.anchor != self.caret {
            marked.insert(self.anchor.min(self.caret), CARET);
        }
        marked
    }
}

/// Run `action` on `text` in `language` with the marked selection, or with
/// everything selected, in an editor without a window.
pub(super) fn run(
    cx: &mut TestAppContext,
    language: &str,
    text: &str,
    selection: Option<(usize, usize)>,
    action: Action,
) -> Outcome {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        model.set_document(uuid::Uuid::new_v4());
        model
    });
    cx.update(|cx| crate::app::documents::detect_indentation(&model, cx));
    cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
    let editor = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    let (anchor, caret) = selection.unwrap_or((0, text.len()));
    editor.update(cx, |editor, cx| {
        editor.restore_view_state(
            EditorViewState {
                anchor,
                caret,
                top: 0,
                scroll_x: 0.,
            },
            cx,
        );
        match action {
            Action::Command(command) => {
                editor.execute_editing_command(command, cx);
            }
            Action::Type(text) => {
                for character in text.chars() {
                    if character == BACKSPACE {
                        editor.execute_editing_command("editor.delete-backward", cx);
                    } else {
                        editor.type_text(None, &character.to_string(), cx);
                    }
                }
            }
            Action::Inspect => {}
        }
    });
    cx.run_until_parked();
    let (state, matched) = cx.read(|cx| {
        let editor = editor.read(cx);
        (editor.view_state(cx), editor.matched_brackets_for_test(cx))
    });
    Outcome {
        text: model.read_with(cx, |model, _| model.text()),
        anchor: state.anchor,
        caret: state.caret,
        matched,
    }
}
