//! Notification popups: which windows show a notice, how a popup ends, what
//! its actions run, and the settings that shape the stack.
use super::*;
use crate::app::{
    notifications::{self, ApplicationNotifications, NoticeAction, NoticeSpec, Severity},
    settings::{
        ApplicationSettings, NOTIFICATIONS_MAX_VISIBLE, NOTIFICATIONS_POPUPS,
        NOTIFICATIONS_POSITION, NOTIFICATIONS_TIMEOUT, STATUS_BAR_VISIBLE,
    },
    status_bar::StatusCommand,
};
use std::time::Duration;

fn spec(message: &str, severity: Severity) -> NoticeSpec {
    NoticeSpec {
        message: message.into(),
        severity,
        source: "Knot".into(),
        ..NoticeSpec::default()
    }
}

fn raise(spec: NoticeSpec, cx: &mut TestAppContext) -> u64 {
    cx.update(|cx| notifications::raise(spec, cx))
}

fn set_setting(key: &str, value: serde_json::Value, cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.default_global::<ApplicationSettings>();
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(key, value, crate::app::settings::SettingsScope::default())
            .unwrap();
    });
}

fn shown(window: &Entity<KnotWindow>, cx: &mut TestAppContext) -> Vec<u64> {
    cx.run_until_parked();
    cx.read(|cx| window.read(cx).notices.shown())
}

fn is_read(id: u64, cx: &mut TestAppContext) -> bool {
    cx.read(|cx| {
        cx.global::<ApplicationNotifications>()
            .0
            .get(id)
            .expect("the notice is in the history")
            .read
    })
}

fn bounds(
    native_window: gpui::AnyWindowHandle,
    selector: &'static str,
    cx: &mut TestAppContext,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.run_until_parked();
    cx.update_window(native_window, |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    VisualTestContext::from_window(native_window, cx).debug_bounds(selector)
}

fn click(
    native_window: gpui::AnyWindowHandle,
    point: gpui::Point<gpui::Pixels>,
    cx: &mut TestAppContext,
) {
    let mut window_cx = VisualTestContext::from_window(native_window, cx);
    window_cx.simulate_click(point, Modifiers::default());
}

fn new_window(
    documents: &Entity<DocumentCollection>,
    cx: &mut TestAppContext,
) -> (Entity<KnotWindow>, gpui::AnyWindowHandle) {
    let document = cx.update(|cx| create_untitled_document(documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    product_window(document, model, cx)
}

#[gpui::test]
fn a_notice_pops_up_in_the_windows_it_targets(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (first, first_native) = new_window(&documents, cx);
    let (second, _) = new_window(&documents, cx);
    let first_key = super::super::test_window_id(first_native);

    let everywhere = raise(spec("everywhere", Severity::Warning), cx);
    let only_first = raise(
        NoticeSpec {
            target: Some(first_key),
            ..spec("first only", Severity::Warning)
        },
        cx,
    );
    assert_eq!(shown(&first, cx), [only_first, everywhere]);
    assert_eq!(shown(&second, cx), [everywhere]);
    // History is application-wide either way.
    let recorded = cx.read(|cx| cx.global::<ApplicationNotifications>().0.notices().len());
    assert_eq!(recorded, 2);
}

#[gpui::test]
fn a_notice_raised_before_any_window_pops_up_when_one_opens(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let id = raise(spec("early", Severity::Warning), cx);
    let (window, _) = new_window(&documents, cx);
    assert_eq!(shown(&window, cx), [id]);
}

#[gpui::test]
fn an_info_popup_times_out_and_leaves_its_notice_unread(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = new_window(&documents, cx);
    set_setting(NOTIFICATIONS_TIMEOUT, serde_json::json!(5), cx);
    let info = raise(spec("info", Severity::Info), cx);
    let sticky = raise(spec("sticky", Severity::Warning), cx);
    assert_eq!(shown(&window, cx), [sticky, info]);
    assert!(bounds(native_window, "notice-1", cx).is_some());

    cx.executor().advance_clock(Duration::from_secs(4));
    assert_eq!(shown(&window, cx), [sticky, info]);
    cx.executor().advance_clock(Duration::from_millis(1100));
    assert_eq!(shown(&window, cx), [sticky]);
    // The card fades out before it leaves the stack.
    assert_eq!(cx.read(|cx| window.read(cx).notices.painted()), 2);
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| window.read(cx).notices.painted()), 1);
    assert!(!is_read(info, cx));
    assert!(!is_read(sticky, cx));
}

#[gpui::test]
fn pointing_at_the_stack_pauses_its_timers(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = new_window(&documents, cx);
    set_setting(NOTIFICATIONS_TIMEOUT, serde_json::json!(5), cx);
    let id = raise(spec("info", Severity::Info), cx);
    let card = bounds(native_window, "notice-stack", cx).expect("the stack is painted");

    VisualTestContext::from_window(native_window, cx).simulate_mouse_move(
        card.center(),
        None,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(30));
    assert_eq!(shown(&window, cx), [id]);

    VisualTestContext::from_window(native_window, cx).simulate_mouse_move(
        gpui::point(px(1.), px(1.)),
        None,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(6));
    assert_eq!(shown(&window, cx), Vec::<u64>::new());
}

#[gpui::test]
fn closing_a_popup_marks_its_notice_read_everywhere(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (first, first_native) = new_window(&documents, cx);
    let (second, _) = new_window(&documents, cx);
    let id = raise(spec("sticky", Severity::Error), cx);
    let close = bounds(first_native, "notice-close-1", cx).expect("the close button is painted");
    click(first_native, close.center(), cx);
    assert!(is_read(id, cx));
    assert_eq!(shown(&first, cx), Vec::<u64>::new());
    assert_eq!(shown(&second, cx), Vec::<u64>::new());
    // The notice stays in the history.
    assert_eq!(
        cx.read(|cx| cx.global::<ApplicationNotifications>().0.notices().len()),
        1
    );
}

#[gpui::test]
fn an_action_runs_its_command_in_the_popups_window_and_closes_it(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = new_window(&documents, cx);
    let id = raise(
        NoticeSpec {
            actions: vec![NoticeAction {
                title: "Hide bar".into(),
                command: StatusCommand {
                    name: "workbench.toggle-status-bar".into(),
                    arguments: knot_protocol::CommandArgumentValue::Null,
                },
            }],
            ..spec("sticky", Severity::Warning)
        },
        cx,
    );
    assert!(cx.read(|cx| crate::app::settings::boolean(STATUS_BAR_VISIBLE, cx)));
    let action = bounds(native_window, "notice-action-1-0", cx).expect("the action is painted");
    click(native_window, action.center(), cx);
    cx.run_until_parked();
    assert!(!cx.read(|cx| crate::app::settings::boolean(STATUS_BAR_VISIBLE, cx)));
    assert!(is_read(id, cx));
    assert_eq!(shown(&window, cx), Vec::<u64>::new());
}

#[gpui::test]
fn the_limit_collapses_older_popups_into_a_count(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = new_window(&documents, cx);
    set_setting(NOTIFICATIONS_MAX_VISIBLE, serde_json::json!(2), cx);
    let ids = ["a", "b", "c", "d"].map(|message| raise(spec(message, Severity::Warning), cx));
    assert_eq!(shown(&window, cx), [ids[3], ids[2]]);
    assert_eq!(cx.read(|cx| window.read(cx).notices.more()), 2);
    assert!(bounds(native_window, "notice-more", cx).is_some());

    // Closing a card lets the next older one in.
    let close = bounds(native_window, "notice-close-4", cx).unwrap();
    click(native_window, close.center(), cx);
    assert_eq!(shown(&window, cx), [ids[2], ids[1]]);
    assert_eq!(cx.read(|cx| window.read(cx).notices.more()), 1);
}

#[gpui::test]
fn the_position_decides_the_corner_the_stack_grows_from(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (_, native_window) = new_window(&documents, cx);
    raise(spec("sticky", Severity::Warning), cx);
    let window_bounds = cx
        .update_window(native_window, |_, window, _| window.bounds())
        .unwrap();
    let corner = |position: &str, cx: &mut TestAppContext| {
        set_setting(NOTIFICATIONS_POSITION, serde_json::json!(position), cx);
        let card = bounds(native_window, "notice-stack", cx).unwrap();
        (
            card.center().x > window_bounds.center().x,
            card.center().y > window_bounds.center().y,
        )
    };
    assert_eq!(corner("bottomRight", cx), (true, true));
    assert_eq!(corner("bottomLeft", cx), (false, true));
    assert_eq!(corner("topRight", cx), (true, false));
    assert_eq!(corner("topLeft", cx), (false, false));
    let (_, below) = corner("topCenter", cx);
    assert!(!below);
}

#[gpui::test]
fn disabling_popups_records_notices_without_showing_them(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = new_window(&documents, cx);
    set_setting(NOTIFICATIONS_POPUPS, serde_json::json!(false), cx);
    let id = raise(spec("quiet", Severity::Error), cx);
    assert_eq!(shown(&window, cx), Vec::<u64>::new());
    assert_eq!(cx.read(|cx| window.read(cx).notices.painted()), 0);
    assert!(!is_read(id, cx));

    // Turning them on shows what is still unread.
    set_setting(NOTIFICATIONS_POPUPS, serde_json::json!(true), cx);
    assert_eq!(shown(&window, cx), [id]);
    assert!(bounds(native_window, "notice-1", cx).is_some());
}
