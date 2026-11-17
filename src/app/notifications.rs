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
