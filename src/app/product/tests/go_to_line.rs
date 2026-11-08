//! The bundled Go to Line package, driven from its key in an editor.
use super::harness::start_bundled_packages;
use super::*;

#[gpui::test]
fn ctrl_g_previews_and_keeps_a_typed_line_in_the_editor(cx: &mut TestAppContext) {
    start_bundled_packages(knot_bundled::PACKAGES, cx);
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let model = cx.new(|_| BufferModel::from_text("one\ntwo\nthree é\nfour\n"));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Lines", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model.clone(), cx);
    let editor = cx.read(|cx| {
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
    let caret = |cx: &gpui::App| editor.read(cx).editor().read(cx).view_state(cx).caret;
    cx.update_window(window, |_, _, cx| {
        shell.update(cx, |shell, cx| shell.focus_active_editor(cx))
    })
    .unwrap();

    cx.simulate_keystrokes(window, "ctrl-g");
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
    cx.simulate_input(window, "3:7");
    // The caret moves while typing, after "three ".
    wait_for_api_condition(cx, |cx| caret(cx) == "one\ntwo\nthree ".len());
    cx.simulate_keystrokes(window, "enter");
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_none());
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(caret(cx), "one\ntwo\nthree ".len()));

    // Escape after a preview restores the caret.
    cx.simulate_keystrokes(window, "ctrl-g");
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
    cx.simulate_input(window, "1");
    wait_for_api_condition(cx, |cx| caret(cx) == 0);
    cx.simulate_keystrokes(window, "escape");
    wait_for_api_condition(cx, |cx| {
        shell.read(cx).transient.is_none() && caret(cx) == "one\ntwo\nthree ".len()
    });
}
