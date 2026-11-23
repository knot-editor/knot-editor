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

type Marks = (Vec<(usize, String)>, Vec<(usize, String)>);

/// The line bands and gutter wedges of the active diff's two sides, as
/// (line, key) pairs.
fn bands(window: &Entity<KnotWindow>, cx: &TestAppContext) -> [Marks; 2] {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let view = workbench
            .diff_view(workbench.focused_pane().unwrap().active_tab())
            .unwrap();
        view.read(cx).editors().map(|editor| {
            let editor = editor.read(cx).editor().read(cx);
            (
                editor
                    .decorations()
                    .iter()
                    .map(|band| (band.line, band.key.clone()))
                    .collect(),
                editor
                    .gutter_bars()
                    .iter()
                    .map(|wedge| (wedge.line, wedge.key.clone()))
                    .collect(),
            )
        })
    })
}

#[gpui::test]
fn a_diff_tab_bands_each_hunk_on_both_sides(cx: &mut TestAppContext) {
    let (window, _, _) = diff_window(cx);
    let modified = (vec![(1, "diff.modified".to_owned())], vec![]);
    assert_eq!(bands(&window, cx), [modified.clone(), modified]);

    // A line only the modified side has leaves a wedge on the original side.
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, cx| {
            workbench.open_diff_in_center(
                spec("HEAD", None, "one\nthree\n"),
                spec("Working copy", None, "one\ntwo\nthree\n"),
                cx,
            );
        });
    });
    cx.run_until_parked();
    assert_eq!(
        bands(&window, cx),
        [
            (vec![], vec![(1, "diff.added".to_owned())]),
            (vec![(1, "diff.added".to_owned())], vec![]),
        ]
    );
}

/// A window showing one document with `text`, and a diff tab of a `HEAD`
/// snapshot against that document focused beside it.
fn document_diff_window(
    text: &str,
    cx: &mut TestAppContext,
) -> (
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
    crate::app::documents::DocumentId,
    Entity<crate::app::model::Buffer>,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, text).unwrap());
    let (window, native_window) = product_window(document, model.clone(), cx);
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, cx| {
            workbench.open_diff_in_center(
                spec("HEAD", None, "one\ntwo\n"),
                crate::app::diff_view::ModifiedSource::Document {
                    id: document,
                    label: "Working copy".to_owned(),
                    model: model.clone(),
                },
                cx,
            );
        });
        window.focus_active_surface(cx);
    });
    cx.run_until_parked();
    (window, native_window, document, model)
}

fn edit(
    model: &Entity<crate::app::model::Buffer>,
    range: std::ops::Range<usize>,
    text: &str,
    cx: &mut TestAppContext,
) {
    model.update(cx, |model, cx| {
        model.replace(range, text).unwrap();
        cx.notify();
    });
}

fn settle(cx: &mut TestAppContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
}

#[gpui::test]
fn a_document_side_is_editable_shares_the_buffer_and_keeps_the_bands_current(
    cx: &mut TestAppContext,
) {
    let (window, _, document, model) = document_diff_window("one\ntwo\n", cx);
    assert_eq!(bands(&window, cx), [(vec![], vec![]), (vec![], vec![])]);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.view_count(document), 2);
    });

    // An edit of the document, wherever it is made, rebands both sides after
    // the debounce, not before.
    edit(&model, 4..7, "2", cx);
    cx.run_until_parked();
    assert_eq!(bands(&window, cx), [(vec![], vec![]), (vec![], vec![])]);
    settle(cx);
    let modified = (vec![(1, "diff.modified".to_owned())], vec![]);
    assert_eq!(bands(&window, cx), [modified.clone(), modified]);
    assert_eq!(
        texts(&window, cx),
        Some(("one\ntwo\n".to_owned(), "one\n2\n".to_owned()))
    );

    // Changes inside the debounce window coalesce into the last one.
    edit(&model, 4..5, "two", cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    edit(&model, 4..7, "t", cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(
        bands(&window, cx)[1].0,
        vec![(1, "diff.modified".to_owned())]
    );
    settle(cx);
    assert_eq!(model.read_with(cx, |model, _| model.text()), "one\nt\n");
}

#[gpui::test]
async fn closing_the_final_dirty_view_of_a_document_through_its_diff_prompts(
    cx: &mut TestAppContext,
) {
    let (window, native_window, document, _) = document_diff_window("one\n2\n", cx);
    // Close the document's own tab first: the diff still views it.
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.workbench.update(cx, |workbench, _| {
                let tabs = workbench.tabs().map(|tab| tab.id()).collect::<Vec<_>>();
                let pane = workbench.focused_pane_id().unwrap();
                workbench.activate_tab(pane, tabs[0]);
            });
        });
    })
    .unwrap();
    let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert!(!cx.has_pending_prompt());
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.view_count(document), 1);
        assert!(
            workbench
                .focused_pane()
                .unwrap()
                .active_tab()
                .document_id()
                .is_none()
        );
    });

    // Closing the diff, the last view, asks about the dirty document.
    let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::Cancelled
    );
    cx.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).view_count(document), 1);
    });
    let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert!(KnotWindow::documents(cx).read(cx).get(document).is_none());
    });
}
