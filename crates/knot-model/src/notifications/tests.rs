use std::time::{Duration, SystemTime};

use super::*;

fn spec(message: &str) -> NoticeSpec {
    NoticeSpec {
        message: message.into(),
        source: "Knot".into(),
        ..NoticeSpec::default()
    }
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn messages(popups: &Popups) -> Vec<&str> {
    popups
        .shown
        .iter()
        .map(|notice| notice.message.as_str())
        .collect()
}

#[test]
fn history_keeps_the_last_hundred_notices() {
    let mut store = NotificationStore::default();
    for index in 0..HISTORY_LIMIT + 5 {
        store.raise(spec(&index.to_string()), at(index as u64));
    }
    assert_eq!(store.notices().len(), HISTORY_LIMIT);
    assert_eq!(store.notices()[0].message, "5");
    assert_eq!(store.notices()[HISTORY_LIMIT - 1].message, "104");
}

#[test]
fn auto_dismiss_defaults_to_info_only() {
    let mut store = NotificationStore::default();
    let mut raise = |severity, auto_dismiss| {
        let id = store.raise(
            NoticeSpec {
                severity,
                auto_dismiss,
                ..spec("m")
            },
            at(0),
        );
        store.get(id).unwrap().auto_dismiss
    };
    assert!(raise(Severity::Info, None));
    assert!(!raise(Severity::Warning, None));
    assert!(!raise(Severity::Error, None));
    assert!(!raise(Severity::Info, Some(false)));
    assert!(raise(Severity::Error, Some(true)));
}

#[test]
fn read_state_follows_marking_and_closes_popups() {
    let mut store = NotificationStore::default();
    let first = store.raise(spec("first"), at(0));
    let second = store.raise(spec("second"), at(1));
    assert_eq!(store.unread(), 2);
    assert!(store.mark_read(&[first]));
    assert!(!store.mark_read(&[first]));
    assert_eq!(store.unread(), 1);
    assert_eq!(messages(&store.popups("w", 5)), ["second"]);
    assert!(store.mark_all_read());
    assert!(!store.mark_all_read());
    assert_eq!(store.unread(), 0);
    assert!(store.get(second).unwrap().read);
    assert_eq!(store.popups("w", 5), Popups::default());
}

#[test]
fn dismissing_and_clearing_remove_notices_from_history() {
    let mut store = NotificationStore::default();
    let first = store.raise(spec("first"), at(0));
    store.raise(spec("second"), at(1));
    assert!(store.dismiss(first));
    assert!(!store.dismiss(first));
    assert_eq!(store.notices().len(), 1);
    assert!(store.clear());
    assert!(!store.clear());
    assert!(store.notices().is_empty());
}

#[test]
fn a_targeted_notice_pops_up_only_in_its_window() {
    let mut store = NotificationStore::default();
    store.raise(spec("everywhere"), at(0));
    store.raise(
        NoticeSpec {
            target: Some("w1".into()),
            ..spec("here")
        },
        at(1),
    );
    assert_eq!(messages(&store.popups("w1", 5)), ["here", "everywhere"]);
    assert_eq!(messages(&store.popups("w2", 5)), ["everywhere"]);
    assert_eq!(store.notices().len(), 2);
}

#[test]
fn hiding_a_popup_is_per_window_and_leaves_the_notice_unread() {
    let mut store = NotificationStore::default();
    let id = store.raise(spec("m"), at(0));
    assert!(store.hide_popup(id, "w1"));
    assert!(!store.hide_popup(id, "w1"));
    assert!(!store.hide_popup(99, "w1"));
    assert_eq!(store.popups("w1", 5), Popups::default());
    assert_eq!(messages(&store.popups("w2", 5)), ["m"]);
    assert!(!store.get(id).unwrap().read);
    assert_eq!(store.unread(), 1);
}

#[test]
fn popups_stop_at_the_limit_and_count_the_rest() {
    let mut store = NotificationStore::default();
    for message in ["a", "b", "c", "d", "e"] {
        store.raise(spec(message), at(0));
    }
    let popups = store.popups("w", 3);
    assert_eq!(messages(&popups), ["e", "d", "c"]);
    assert_eq!(popups.more, 2);
    assert_eq!(store.popups("w", 0).more, 5);
    assert_eq!(store.popups("w", 9).more, 0);
}

#[test]
fn positions_parse_and_fall_back_to_the_bottom_right() {
    assert_eq!(Position::parse("topCenter"), Position::TopCenter);
    assert_eq!(Position::parse("bottomLeft"), Position::BottomLeft);
    assert_eq!(Position::parse("middle"), Position::BottomRight);
    assert!(Position::parse("topLeft").is_top());
    assert!(!Position::parse("bottomRight").is_top());
}
