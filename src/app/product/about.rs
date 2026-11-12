//! Native Knot: About: show the build version and commit in the window's
//! transient picker.

use super::*;
use crate::app::build_info;
use knot_protocol::PickerItem;

pub(super) const ABOUT_TITLE: &str = "About Knot";

impl KnotWindow {
    pub(crate) fn start_about(&mut self, cx: &mut Context<Self>) {
        let row = |id: &str, label: &str, description: &str| PickerItem {
            icon: None,
            id: id.into(),
            label: label.into(),
            description: Some(description.into()),
            detail: None,
            hint: None,
        };
        let items = vec![
            row("version", "Version", build_info::VERSION),
            row("commit", "Commit", build_info::COMMIT),
        ];
        if self
            .begin_owned_picker(None, ABOUT_TITLE.into(), items, None, cx)
            .is_err()
        {
            cx.notify();
        }
    }
}
