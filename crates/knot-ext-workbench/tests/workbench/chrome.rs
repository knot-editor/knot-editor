//! The bundled chrome package, following the editor a window describes.
use gpui::TestAppContext;

use super::bundled::only;
use super::projects::file_uri;
use knot::test_support::{CommandOutcome, TestProduct, wait_until};

#[gpui::test]
fn the_items_follow_the_selection_and_the_document_of_the_status_target(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/chrome"]), cx);
    let editor = product.windowed_editor("one\ntwo\nthree é\n", cx);
    let right = |cx: &mut TestAppContext| editor.status(cx).1;
    wait_until(cx, |cx| right(cx).len() == 3);
    assert_eq!(right(cx), ["Ln 1, Col 1", "Spaces: 4", "Plain Text"]);

    editor.select(6, 6, cx);
    wait_until(cx, |cx| right(cx)[0] == "Ln 2, Col 3");
    editor.select(0, 3, cx);
    wait_until(cx, |cx| right(cx)[0] == "Ln 1, Col 4 (3 selected)");

    // A document-scoped indentation write reaches the bar without a listener.
    editor.set_setting("editor.insertSpaces", false.into(), cx);
    wait_until(cx, |cx| right(cx)[1] == "Tab Size: 4");
}

#[gpui::test]
fn the_tab_menu_closes_and_splits_the_clicked_tab(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/chrome"]), cx);
    let editor = product.windowed_editor("one", cx);
    wait_until(cx, |cx| !editor.tab_menu(0, cx).is_empty());
    assert_eq!(
        editor.tab_menu(0, cx),
        [
            vec![
                "Close Tab",
                "Close Other Tabs",
                "Close Tabs to the Right",
                "Close All"
            ],
            vec!["Split Right", "Split Down"],
        ]
    );

    // Closing to the right of the first tab leaves it, though the last tab
    // had focus.
    editor.new_tab(cx);
    editor.new_tab(cx);
    assert_eq!(editor.tab_titles(cx).len(), 3);
    editor.right_click_tab(0, cx);
    editor.press_in_menu("down down down enter", cx);
    wait_until(cx, |cx| editor.tab_titles(cx).len() == 1);
}

#[gpui::test]
fn the_editor_menu_offers_the_clipboard_at_the_pointer_and_at_the_caret(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/chrome"]), cx);
    let editor = product.windowed_editor("one two", cx);
    wait_until(cx, |cx| !editor.content_menu(cx).is_empty());
    assert_eq!(editor.content_menu(cx), [vec!["Cut", "Copy", "Paste"]]);

    // A right-click outside the selection moves the caret to the click, then
    // Paste (the third item) acts on that editor.
    editor.select(0, 3, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("!".into()));
    editor.right_click_content(cx);
    assert_eq!(editor.caret(cx), 7);
    editor.press_in_menu("down down down enter", cx);
    assert_eq!(editor.active_text(cx), "one two!");

    // Shift+F10 opens the menu at the caret, and Cut removes the selection.
    editor.select(0, 3, cx);
    editor.press("<s-f10>", cx);
    editor.press_in_menu("down enter", cx);
    assert_eq!(editor.active_text(cx), " two!");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("one".to_owned())
    );
}

const TERMINAL: &str = "builtin.terminal";

fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap()
}

#[gpui::test]
fn new_terminal_opens_in_the_terminal_window_in_the_root_of_the_last_document(
    cx: &mut TestAppContext,
) {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(second.join("src")).unwrap();
    std::fs::write(second.join("src/lib.rs"), "").unwrap();
    let product = TestProduct::launch(only(&["@knot/chrome"]), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(&first), &file_uri(&second)], cx);
    window.open_path(&second.join("src/lib.rs"), cx);

    wait_until(cx, |cx| {
        window.run_command("terminal.new", cx) == CommandOutcome::Completed
    });
    wait_until(cx, |cx| !window.terminals(TERMINAL, cx).is_empty());
    let terminals = window.terminals(TERMINAL, cx);
    assert_eq!(terminals.len(), 1);
    assert!(terminals[0].0, "the terminal is in the Terminal window");
    assert_eq!(
        terminals[0].1.as_deref().map(canonical),
        Some(canonical(&second))
    );
    assert!(window.tool_shown(TERMINAL, cx));

    // From a focused terminal, the next one starts where that shell is.
    window.run_command("terminal.new", cx);
    wait_until(cx, |cx| window.terminals(TERMINAL, cx).len() == 2);
    assert_eq!(
        window.terminals(TERMINAL, cx)[1]
            .1
            .as_deref()
            .map(canonical),
        Some(canonical(&second))
    );
}

#[gpui::test]
fn new_terminal_in_editor_opens_in_the_center(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let product = TestProduct::launch(only(&["@knot/chrome"]), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(temp.path())], cx);
    window.run_command("terminal.new-in-center", cx);
    wait_until(cx, |cx| !window.terminals(TERMINAL, cx).is_empty());
    let terminals = window.terminals(TERMINAL, cx);
    assert_eq!(terminals.len(), 1);
    assert!(!terminals[0].0, "the terminal is in the center");
    assert_eq!(
        terminals[0].1.as_deref().map(canonical),
        Some(canonical(temp.path()))
    );
}

#[gpui::test]
fn showing_the_empty_terminal_window_opens_a_terminal_and_toggle_hides_it(cx: &mut TestAppContext) {
    let product = TestProduct::launch(only(&["@knot/chrome"]), cx);
    let window = product.active_window(cx);
    wait_until(cx, |cx| window.has_tool(TERMINAL, cx));
    assert!(!window.tool_shown(TERMINAL, cx));

    window.click_tool_button(TERMINAL, cx);
    wait_until(cx, |cx| window.terminals(TERMINAL, cx).len() == 1);
    assert!(window.tool_shown(TERMINAL, cx));

    // The toggle hides the focused window, keeping its shell, and shows it again.
    window.run_command("builtin.terminal.toggle", cx);
    wait_until(cx, |cx| !window.tool_shown(TERMINAL, cx));
    assert_eq!(window.terminals(TERMINAL, cx).len(), 1);
    window.run_command("builtin.terminal.toggle", cx);
    wait_until(cx, |cx| window.tool_shown(TERMINAL, cx));
    assert_eq!(window.terminals(TERMINAL, cx).len(), 1);
}

#[gpui::test]
fn terminals_in_the_terminal_window_and_the_editor_return_after_a_restart(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let product = TestProduct::launch(only(&["@knot/chrome"]), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(temp.path())], cx);
    window.run_command("terminal.new", cx);
    wait_until(cx, |cx| window.terminals(TERMINAL, cx).len() == 1);
    window.run_command("terminal.new-in-center", cx);
    wait_until(cx, |cx| window.terminals(TERMINAL, cx).len() == 2);

    product.relaunch_restoring(&[&window], cx);
    let restored = product.active_window(cx);
    wait_until(cx, |cx| restored.terminals(TERMINAL, cx).len() == 2);
    let mut homes = restored
        .terminals(TERMINAL, cx)
        .into_iter()
        .map(|(in_window, _)| in_window)
        .collect::<Vec<_>>();
    homes.sort();
    assert_eq!(homes, [false, true]);
}
