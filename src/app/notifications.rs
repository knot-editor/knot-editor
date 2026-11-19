//! The application's notification store, shared by every window. The notices
//! and their history rules live in [`knot_model::notifications`]; windows
//! show their popups.

use std::time::SystemTime;

use gpui::{App, BorrowAppContext as _, Global};

pub(crate) use knot_model::notifications::*;

#[derive(Default)]
pub(crate) struct ApplicationNotifications(pub(crate) NotificationStore);

impl Global for ApplicationNotifications {}

/// Change the store, which redraws every window's popups and lets extensions
/// observe the new history.
pub(crate) fn update<R>(cx: &mut App, change: impl FnOnce(&mut NotificationStore) -> R) -> R {
    if !cx.has_global::<ApplicationNotifications>() {
        cx.set_global(ApplicationNotifications::default());
    }
    let result = cx.update_global::<ApplicationNotifications, _>(|notifications, _| {
        change(&mut notifications.0)
    });
    super::application_observation::changed(cx);
    result
}

/// Record a notice and return its ID.
pub(crate) fn raise(spec: NoticeSpec, cx: &mut App) -> u64 {
    update(cx, |store| store.raise(spec, SystemTime::now()))
}

/// An action that runs the command `name` without arguments.
pub(crate) fn action(title: &str, name: &str) -> NoticeAction {
    NoticeAction {
        title: title.into(),
        command: knot_model::status_bar::StatusCommand {
            name: name.into(),
            arguments: knot_protocol::CommandArgumentValue::Null,
        },
    }
}

/// Record a sticky `warning` notice for a configuration problem the user can
/// fix, with actions that open the personal configuration and the log. It has
/// no target, so every window shows it, and one raised before any window
/// exists pops up when the first opens.
pub(crate) fn raise_config_warning(message: String, cx: &mut App) -> u64 {
    raise(
        NoticeSpec {
            message,
            title: Some("Configuration warning".into()),
            severity: Severity::Warning,
            auto_dismiss: Some(false),
            actions: vec![
                action("Open Personal Configuration", "app.open-personal-config"),
                action("Open Log", "app.open-log"),
            ],
            target: None,
            source: "Knot".into(),
        },
        cx,
    )
}
