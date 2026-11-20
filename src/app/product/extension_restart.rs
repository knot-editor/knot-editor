//! Native Restart Extension: stop and reload one package from the window's
//! transient picker.
//!
//! The picker lists every bundled and installed package in the retained startup
//! report, including failed ones. Personal config is not listed; restarting
//! Knot is its recovery path.

use super::*;
use crate::app::extensions::load::LoadResult;
use knot_protocol::PickerItem;

pub(super) const RESTART_TITLE: &str = "Restart Extension";

impl KnotWindow {
    /// List restartable packages in the captured window and restart the selection.
    pub(crate) fn start_extension_restart(&mut self, cx: &mut Context<Self>) {
        let Some(report) = super::super::extensions::host::startup_report(cx) else {
            self.raise_notice(
                Severity::Warning,
                "Extensions are unavailable",
                None,
                Vec::new(),
                cx,
            );
            return;
        };
        let items = report
            .report
            .entries
            .iter()
            .filter_map(|entry| {
                let name = entry.name.clone()?;
                Some(PickerItem {
                    tabs: Vec::new(),
                    detail: None,
                    hint: None,
                    icon: None,
                    selected: false,
                    id: name.clone(),
                    label: name,
                    description: Some(match &entry.result {
                        LoadResult::Loaded => "loaded".into(),
                        LoadResult::NotLoaded(cause) => cause.clone(),
                        LoadResult::Failed(cause) => format!("failed: {cause}"),
                    }),
                })
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            cx.notify();
            return;
        }
        let Ok(receiver) = self.begin_owned_picker(None, RESTART_TITLE.into(), items, None, cx)
        else {
            return;
        };
        let window = cx.weak_entity();
        let task = cx.spawn(async move |this, cx| {
            let Ok(TransientOutcome::Picker(name)) = receiver.await else {
                return;
            };
            let restart = cx.update(|cx| {
                super::super::extensions::host::entity(cx).map(|host| {
                    host.update(cx, |host, cx| {
                        host.restart_package(name.clone(), Some(window), cx)
                    })
                })
            });
            let Some(restart) = restart else { return };
            let entry = restart.await;
            let (severity, title, message) = match entry.result {
                LoadResult::Loaded => (Severity::Info, format!("Restarted {name}"), None),
                LoadResult::NotLoaded(cause) => (
                    Severity::Warning,
                    format!("{name} was not restarted"),
                    Some(cause),
                ),
                LoadResult::Failed(cause) => (
                    Severity::Error,
                    format!("{name} failed to restart"),
                    Some(cause),
                ),
            };
            Self::notice(&this, severity, &title, message, cx);
        });
        self.tasks.push(task);
    }
}
