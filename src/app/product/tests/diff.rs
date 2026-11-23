//! Diff tabs: placement, split, relocation, closure, and what is not restored.
use super::*;
use crate::app::commands::dispatch::{CLOSE_TAB_COMMAND, REOPEN_CLOSED_TAB_COMMAND};
use crate::app::diff_view::SnapshotSpec;
use crate::app::product::relocate::{TabRelocationDestination, TabRelocationSource};
use crate::app::window_layout::{TerminalFacts, WindowLayout};
use crate::app::workbench::{SplitDirection, SplitEdge, SplitPlacement, TabMovePlacement};
use gpui::App;
use knot_model::workbench::{TabMoveDestination, TabSurfaceId};

fn spec(label: &str, language: Option<&str>, text: &str) -> SnapshotSpec {
    SnapshotSpec {
        label: label.to_owned(),
        language: language.map(ToOwned::to_owned),
        text: text.to_owned(),
    }
}

/// A window showing one document, with a diff tab of two snapshots open and
/// focused beside it.
fn diff_window(
    cx: &mut TestAppContext,
) -> (
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
    crate::app::documents::DocumentId,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, cx| {
            workbench.open_diff_in_center(
                spec("HEAD", Some("plaintext"), "one\ntwo\n"),
                spec("Working copy", None, "one\n2\n"),
                cx,
            );
        });
        window.focus_active_surface(cx);
    });
    cx.run_until_parked();
    (window, native_window, document)
}

fn texts(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Option<(String, String)> {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let view = workbench.diff_view(workbench.focused_pane()?.active_tab())?;
        let [original, modified] = view.read(cx).editors();
        Some((
            original.read(cx).model().read(cx).text(),
            modified.read(cx).model().read(cx).text(),
        ))
    })
}

fn capture(window: &Entity<KnotWindow>, cx: &App) -> TabRelocationSource {
    let window = window.read(cx);
    let workbench = window.workbench.read(cx);
    let pane = workbench.focused_pane().unwrap();
    window
        .capture_tab_relocation(pane.id(), pane.active_tab_id(), cx)
        .unwrap()
}

#[gpui::test]
fn a_diff_tab_shows_two_read_only_snapshots_titled_from_both_labels(cx: &mut TestAppContext) {
    let (window, _, document) = diff_window(cx);
    assert_eq!(
        texts(&window, cx),
        Some(("one\ntwo\n".to_owned(), "one\n2\n".to_owned()))
    );
    let models = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        assert!(matches!(tab.surface_id(), TabSurfaceId::Diff(_)));
        assert_eq!(
            workbench.diff_view(tab).unwrap().read(cx).title(),
            "HEAD ↔ Working copy"
        );
        let [original, modified] = workbench.diff_view(tab).unwrap().read(cx).editors();
        assert_eq!(
            original.read(cx).model().read(cx).language(),
            Some("plaintext")
        );
        // The diff is not a view of any document.
        assert_eq!(workbench.view_count(document), 1);
        [original, modified].map(|editor| editor.read(cx).model().clone())
    });
    // Nothing can edit a snapshot.
    for model in models {
        assert!(
            model
                .update(cx, |model, _| model.replace(0..0, "x"))
                .is_err()
        );
    }
}

#[gpui::test]
fn a_diff_splits_into_its_own_view_and_moves_between_panes_and_windows(cx: &mut TestAppContext) {
    let (window, native_window, document) = diff_window(cx);
    let first_view = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .diff_view(workbench.focused_pane().unwrap().active_tab())
            .unwrap()
            .clone()
    });

    let split = window.update(cx, |window, cx| {
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        window.split_pane(pane, SplitDirection::Vertical, SplitPlacement::After, cx)
    });
    assert!(split);
    assert_eq!(
        texts(&window, cx),
        Some(("one\ntwo\n".to_owned(), "one\n2\n".to_owned()))
    );
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let second_view = workbench
            .diff_view(workbench.focused_pane().unwrap().active_tab())
            .unwrap();
        assert_ne!(second_view, &first_view);
    });

    // Moving the tab to the other pane's edge keeps its view.
    let origin = cx.read(|cx| capture(&window, cx));
    let destination = cx.read(|cx| {
        let source = capture(&window, cx);
        TabRelocationDestination {
            window: window.downgrade(),
            workbench: source.workbench,
            tab: TabMoveDestination::At {
                anchor: source.tab,
                placement: TabMovePlacement::Split(SplitEdge::Left),
            },
        }
    });
    let moved_view = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .diff_view(workbench.focused_pane().unwrap().active_tab())
            .unwrap()
            .clone()
    });
    assert_eq!(
        cx.update_window(native_window, |_, _, cx| window.update(cx, |window, cx| {
            window.relocate_tab_to(&origin, &destination, cx)
        }))
        .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        assert_eq!(workbench.diff_view(tab), Some(&moved_view));
        assert_eq!(workbench.view_count(document), 1);
    });

    // Into another window.
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (other, _) = product_window(document, model, cx);
    let origin = cx.read(|cx| capture(&window, cx));
    let destination = cx.read(|cx| {
        let source = capture(&other, cx);
        TabRelocationDestination {
            window: other.downgrade(),
            workbench: source.workbench,
            tab: TabMoveDestination::At {
                anchor: source.tab,
                placement: TabMovePlacement::End,
            },
        }
    });
    assert_eq!(
        cx.update_window(native_window, |_, _, cx| window.update(cx, |window, cx| {
            window.relocate_tab_to(&origin, &destination, cx)
        }))
        .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let workbench = other.read(cx).workbench.read(cx);
        let diff = workbench
            .tabs()
            .find_map(|tab| workbench.diff_view(tab))
            .unwrap();
        assert_eq!(diff, &moved_view);
    });
}

#[gpui::test]
fn closing_a_diff_tab_leaves_the_document_and_is_not_reopened_or_restored(cx: &mut TestAppContext) {
    let (window, native_window, document) = diff_window(cx);
    cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let documents = KnotWindow::documents(cx).read(cx);
        let facts = |_| TerminalFacts {
            directory: None,
            name: None,
        };
        let captured = |tab| WindowLayout::capture_tab(workbench, tab, documents, &facts, cx);
        let tabs = workbench.tabs().collect::<Vec<_>>();
        assert_eq!(tabs.len(), 2);
        assert!(captured(tabs[0]).is_some(), "the document tab is recorded");
        assert!(captured(tabs[1]).is_none(), "the diff tab is not");
        assert_eq!(window.capture_layout(None, cx).documents().len(), 1);
    });

    let view = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .diff_view(workbench.focused_pane().unwrap().active_tab())
            .unwrap()
            .downgrade()
    });
    assert_eq!(
        run_command(&window, native_window, CLOSE_TAB_COMMAND, cx),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert!(workbench.tabs().all(|tab| tab.document_id().is_some()));
        assert_eq!(workbench.view_count(document), 1);
    });
    assert!(view.upgrade().is_none());
    assert_eq!(
        run_command(&window, native_window, REOPEN_CLOSED_TAB_COMMAND, cx),
        CommandOutcome::Unavailable
    );
}

fn run_command(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) -> CommandOutcome {
    let execution = dispatch_product_command(window, native_window, name, cx);
    let outcome = wait_for_command(execution, cx);
    cx.run_until_parked();
    outcome
}
