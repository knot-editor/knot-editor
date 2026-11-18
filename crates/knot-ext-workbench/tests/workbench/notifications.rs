//! The bundled notifications package: the panel and the bell, driven by
//! notices raised in the launched product.
use gpui::TestAppContext;

use super::bundled::only;
use knot::test_support::{CommandOutcome, TestProduct, wait_until};

const VIEW: &str = "builtin.notifications";

fn start(cx: &mut TestAppContext) -> TestProduct {
    TestProduct::launch(only(&["@knot/notifications"]), cx)
}

fn labels(tree: &knot::test_support::TestTree, cx: &mut TestAppContext) -> Vec<String> {
    tree.labels(cx)
}

#[gpui::test]
fn a_raised_notice_is_listed_and_opening_the_panel_reads_it(cx: &mut TestAppContext) {
    let product = start(cx);
    let window = product.active_window(cx);
    assert_eq!(
        window.run_command("notifications.show", cx),
        CommandOutcome::Completed
    );
    let tree = window.tree(VIEW, cx);
    wait_until(cx, |cx| labels(&tree, cx) == ["No notifications"]);

    let raised = product.raise_notice("disk almost full", "warning", None, cx);
    // A notice without a title is labelled by its source, over its message.
    wait_until(cx, |cx| labels(&tree, cx) == ["Today", "Knot"]);
    assert_eq!(
        product.notice_states(cx),
        [("disk almost full".to_owned(), false)]
    );
    let id = format!("notice:{raised}");
    assert_eq!(tree.detail(&id, cx).as_deref(), Some("disk almost full"));
    assert!(tree.status(&id, cx).is_some());

    // Showing the panel again reads what arrived while it was open.
    assert_eq!(
        window.run_command("notifications.show", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| product.notice_states(cx)[0].1);
}

#[gpui::test]
fn the_stripe_dot_follows_unread_notices_and_severity(cx: &mut TestAppContext) {
    let product = start(cx);
    let window = product.active_window(cx);
    let dot = |cx: &mut TestAppContext| window.stripe_badge(VIEW, cx);
    wait_until(cx, |cx| window.has_tool(VIEW, cx));
    assert_eq!(dot(cx), None);

    product.raise_notice("one", "info", None, cx);
    wait_until(cx, |cx| dot(cx) == Some("info"));
    product.raise_notice("two", "error", None, cx);
    wait_until(cx, |cx| dot(cx) == Some("error"));

    assert_eq!(
        window.run_command("notifications.toggle", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| dot(cx).is_none());
    assert!(product.notice_states(cx).iter().all(|(_, read)| *read));
}

#[gpui::test]
fn rows_run_actions_dismiss_and_clear(cx: &mut TestAppContext) {
    let product = start(cx);
    let window = product.active_window(cx);
    window.run_command("notifications.show", cx);
    let tree = window.tree(VIEW, cx);

    let acted = product.raise_notice(
        "fix me",
        "warning",
        Some(("Open", "notifications.clear")),
        cx,
    );
    product.raise_notice("keep me", "info", None, cx);
    wait_until(cx, |cx| labels(&tree, cx).len() == 3);
    let row = format!("notice:{acted}");
    assert_eq!(
        tree.row_commands(&row, cx),
        ["notifications.run-action", "notifications.dismiss"]
    );

    // Dismiss removes the row's notice from the history.
    tree.invoke(&row, "notifications.dismiss", cx);
    wait_until(cx, |cx| labels(&tree, cx) == ["Today", "Knot"]);
    assert_eq!(product.notice_states(cx), [("keep me".to_owned(), false)]);

    // Activating a row without actions reads it; one with an action runs it
    // (here, Clear Notifications) and reads it.
    let again = product.raise_notice(
        "run me",
        "warning",
        Some(("Clear", "notifications.clear")),
        cx,
    );
    wait_until(cx, |cx| labels(&tree, cx).len() == 3);
    tree.activate_row(&format!("notice:{again}"), cx);
    wait_until(cx, |cx| product.notice_states(cx).is_empty());
    wait_until(cx, |cx| labels(&tree, cx) == ["No notifications"]);
}

#[gpui::test]
fn the_frame_offers_mark_all_read_and_clear_all(cx: &mut TestAppContext) {
    let product = start(cx);
    let window = product.active_window(cx);
    window.run_command("notifications.show", cx);
    let tree = window.tree(VIEW, cx);
    assert_eq!(
        tree.frame_commands(cx),
        ["notifications.mark-all-read", "notifications.clear"]
    );
    product.raise_notice("a", "info", None, cx);
    wait_until(cx, |cx| labels(&tree, cx).len() == 2);
    window.run_command("notifications.mark-all-read", cx);
    wait_until(cx, |cx| product.notice_states(cx)[0].1);
    window.run_command("notifications.clear", cx);
    wait_until(cx, |cx| product.notice_states(cx).is_empty());
}

#[gpui::test]
fn the_more_pill_opens_the_panel_in_its_window(cx: &mut TestAppContext) {
    let product = start(cx);
    let editor = product.windowed_editor("text", cx);
    for message in ["a", "b", "c", "d"] {
        product.raise_notice(message, "warning", None, cx);
    }
    cx.run_until_parked();
    let tree = editor.window(cx).tree(VIEW, cx);
    assert!(!tree.is_attached(cx));
    editor.click_more_notices(cx);
    wait_until(cx, |cx| tree.is_attached(cx));
    wait_until(cx, |cx| {
        product.notice_states(cx).iter().all(|(_, read)| *read)
    });
}

#[gpui::test]
fn popups_give_way_to_the_panel_and_return_when_it_closes(cx: &mut TestAppContext) {
    let product = start(cx);
    let editor = product.windowed_editor("text", cx);
    let window = editor.window(cx);
    let tree = window.tree(VIEW, cx);
    window.run_command("notifications.show", cx);
    wait_until(cx, |cx| tree.is_attached(cx));
    assert!(!editor.popups_shown(cx));

    // A notice that arrives while the panel shows is a row, not a popup.
    product.raise_notice("while open", "error", None, cx);
    wait_until(cx, |cx| labels(&tree, cx).len() == 2);
    assert!(!editor.popups_shown(cx));

    // Closing the panel brings back what is still pending.
    window.run_command("notifications.toggle", cx);
    wait_until(cx, |cx| !tree.is_attached(cx));
    wait_until(cx, |cx| editor.popups_shown(cx));
}
