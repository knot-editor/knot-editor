//! The modal example package driven by real keys and typed text.
use super::*;
use crate::app::editor::{EditorView, Reveal};

fn install_modal(extensions: &Path) {
    install_extension(
        extensions,
        "@example/modal",
        &[],
        &[
            (
                "dist/main.js",
                include_str!(concat!(env!("OUT_DIR"), "/examples/modal/dist/main.js")),
            ),
            (
                "dist/extension.js",
                include_str!(concat!(
                    env!("OUT_DIR"),
                    "/examples/modal/dist/extension.js"
                )),
            ),
            (
                "dist/follow.js",
                include_str!(concat!(env!("OUT_DIR"), "/examples/modal/dist/follow.js")),
            ),
            (
                "dist/motion.js",
                include_str!(concat!(env!("OUT_DIR"), "/examples/modal/dist/motion.js")),
            ),
        ],
    );
}

#[gpui::test]
fn the_modal_example_moves_with_keys_swallows_text_and_leaves_on_escape(cx: &mut TestAppContext) {
    let extensions = tempfile::tempdir().unwrap();
    install_modal(extensions.path());
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(crate::app::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_installed_extensions(extensions.path().to_path_buf(), cx)
    });
    let report = wait_for_extension_report(&host, cx);
    assert!(
        report
            .report
            .entries
            .iter()
            .all(|entry| entry.result == crate::app::extensions::load::LoadResult::Loaded),
    );
    let model = cx.new(|_| BufferModel::from_text("ab\ncd"));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Modal", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model.clone(), cx);
    let editor: Entity<EditorView> = cx.read(|cx| {
        shell
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .editor()
            .unwrap()
            .clone()
    });
    cx.update_window(window, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_selection(0, 0, Reveal::Nearest, cx);
            editor.focus_handle(cx).focus(window);
        })
    })
    .unwrap();
    cx.refresh().unwrap();
    let settle = |cx: &mut TestAppContext| {
        wait_for_api_condition(cx, |cx| !shell.read(cx).holds_keys());
    };

    let entered = dispatch_product_command(&shell, window, "example.modal.enter", cx);
    assert_eq!(wait_for_command(entered, cx), CommandOutcome::Completed);
    cx.simulate_keystrokes(window, "l j");
    cx.simulate_input(window, "x");
    settle(cx);
    assert_eq!(cx.read(|cx| model.read(cx).text()), "ab\ncd");
    cx.read(|cx| assert_eq!(editor.read(cx).selection(cx), (4, 4)));

    cx.simulate_keystrokes(window, "escape");
    cx.simulate_input(window, "y");
    settle(cx);
    assert_eq!(cx.read(|cx| model.read(cx).text()), "ab\ncyd");
}

/// Following uses long-lived handles: the active editor joins at once, and a
/// mouse-style selection leaves the context through a selection listener.
#[gpui::test]
fn following_puts_the_active_editor_in_modal_mode_until_text_is_selected(cx: &mut TestAppContext) {
    let extensions = tempfile::tempdir().unwrap();
    install_modal(extensions.path());
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(crate::app::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_installed_extensions(extensions.path().to_path_buf(), cx)
    });
    wait_for_extension_report(&host, cx);
    let model = cx.new(|_| BufferModel::from_text("ab\ncd"));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Follow", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model, cx);
    let editor: Entity<EditorView> = cx.read(|cx| {
        shell
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .editor()
            .unwrap()
            .clone()
    });
    cx.update_window(window, |_, window, cx| {
        window.activate_window();
        editor.update(cx, |editor, cx| editor.focus_handle(cx).focus(window))
    })
    .unwrap();
    cx.refresh().unwrap();

    let followed = dispatch_product_command(&shell, window, "example.modal.follow", cx);
    assert_eq!(wait_for_command(followed, cx), CommandOutcome::Completed);
    wait_for_api_condition(cx, |cx| editor.read(cx).key_contexts() == ["modal"]);

    let started = std::time::Instant::now();
    editor.update(cx, |editor, cx| {
        editor.set_selection(0, 2, Reveal::Nearest, cx)
    });
    wait_for_api_condition(cx, |cx| editor.read(cx).key_contexts().is_empty());
    println!("selection to cleared context: {:?}", started.elapsed());

    let toggled = dispatch_product_command(&shell, window, "example.modal.follow", cx);
    assert_eq!(wait_for_command(toggled, cx), CommandOutcome::Completed);
    cx.read(|cx| assert!(editor.read(cx).key_contexts().is_empty()));
}
