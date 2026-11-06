//! A headless product for package behavior tests. The application and its
//! bundled packages start without a window; a session opens over each
//! document and takes keys through its router as a window would forward
//! them. A run settles once the session holds no keys and the extension
//! pool has gone quiet, waiting on finished turns rather than polling.
//!
//! In the key notation, characters are typed and `<name>` presses a named
//! key: `<esc>`, `<cr>`, `<bs>`, `<del>`, `<tab>`, `<s-tab>`, the arrows,
//! `<home>`, `<end>`, `<lt>` for `<`, and `<c-x>` for ctrl with a letter.
use std::time::{Duration, Instant};

use gpui::Keystroke;

use super::editor_api::active_editor_view;
use super::language_indents::CARET;
use super::*;
use crate::app::editor::{Editor, Reveal};
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
    /// Open `text` in `language`, or detected, with the selection from
    /// `anchor` to `caret`, and settle.
    pub(super) fn open(
        text: &str,
        language: Option<&str>,
        (anchor, caret): (usize, usize),
        cx: &mut TestAppContext,
    ) -> Self {
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        let model = cx.new(|_| BufferModel::from_text(text));
        let document = documents.update(cx, |documents, cx| {
            documents.create_untitled("Case", model.clone(), cx)
        });
        if let Some(language) = language {
            cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
        }
        let shell = cx.update(|cx| {
            let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
            cx.new(|cx| ProductShell::new(workbench, cx))
        });
        shell.update(cx, |shell, cx| {
            shell.observe_application(cx);
            shell.activate(cx);
            shell.focus_active_surface(cx);
        });
        let view = active_editor_view(&shell, cx);
        let editor = cx.read(|cx| view.read(cx).editor().clone());
        editor.update(cx, |editor, cx| {
            editor.set_selection(anchor, caret, Reveal::Nearest, cx)
        });
        let session = Self {
            shell,
            editor,
            model,
            document,
        };
        session.lay_out(ROWS, COLUMNS, cx);
        session.settle(cx);
        session
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

    /// End the session and release its document, so later cases do not pay
    /// for them.
    pub(super) fn close(self, cx: &mut TestAppContext) {
        self.shell.update(cx, |shell, cx| shell.end(cx));
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        documents.update(cx, |documents, cx| documents.remove(self.document, cx));
        cx.update(crate::app::application_observation::changed);
        self.settle(cx);
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

/// gpui keystrokes for the key notation.
pub(super) fn parse_keys(keys: &str) -> Vec<String> {
    let mut parsed = Vec::new();
    let mut rest = keys;
    while let Some(character) = rest.chars().next() {
        if character == '<'
            && let Some(end) = rest.find('>')
        {
            let name = &rest[1..end];
            parsed.push(match name {
                "esc" => "escape".to_owned(),
                "cr" => "enter".to_owned(),
                "bs" => "backspace".to_owned(),
                "del" => "delete".to_owned(),
                "tab" => "tab".to_owned(),
                "s-tab" => "shift-tab".to_owned(),
                "lt" => "<".to_owned(),
                "left" | "right" | "up" | "down" | "home" | "end" => name.to_owned(),
                _ => match name.strip_prefix("c-") {
                    Some(letter) => format!("ctrl-{letter}"),
                    None => panic!("unknown key <{name}>"),
                },
            });
            rest = &rest[end + 1..];
        } else {
            parsed.push(match character {
                ' ' => "space".to_owned(),
                upper if upper.is_ascii_uppercase() => {
                    format!("shift-{}", upper.to_ascii_lowercase())
                }
                other => other.to_string(),
            });
            rest = &rest[character.len_utf8()..];
        }
    }
    parsed
}
