//! Native Manage Entitlements: review and change which entitlements installed
//! packages hold, in the window's transient picker.
//!
//! The picker lists every installed package that declares entitlements, with
//! its decision state. Choosing one toggles between allowed and denied:
//! granting activates a not-loaded package through the restart machinery,
//! while revoking performs a terminating unload, which kills its processes.
//! Bundled packages and personal config are not listed.

use super::*;
use knot_protocol::PickerItem;

pub(super) const MANAGE_TITLE: &str = "Manage Entitlements";

impl KnotWindow {
    /// List entitlement candidates in the captured window and toggle the selection.
    pub(crate) fn start_extension_entitlements(&mut self, cx: &mut Context<Self>) {
        let Some(host) = super::super::extensions::host::entity(cx) else {
            self.raise_notice(
                Severity::Warning,
                "Extensions are unavailable",
                None,
                Vec::new(),
                cx,
            );
            return;
        };
        let candidates = host.update(cx, |host, cx| host.entitlement_candidates(cx));
        let window = cx.weak_entity();
        let task = cx.spawn(async move |this, cx| {
            let candidates = candidates.await;
            if candidates.is_empty() {
                Self::notice(
                    &this,
                    Severity::Info,
                    "No installed packages declare entitlements",
                    None,
                    cx,
                );
                return;
            }
            let items = candidates
                .iter()
                .map(|candidate| {
                    let wants = candidate
                        .entitlements
                        .iter()
                        .map(|entitlement| entitlement.name())
                        .collect::<Vec<_>>()
                        .join(", ");
                    PickerItem {
                        tabs: Vec::new(),
                        detail: None,
                        hint: None,
                        icon: None,
                        selected: false,
                        id: candidate.name.clone(),
                        label: format!("{} {}", candidate.display_name, candidate.version),
                        description: Some(format!("{}: {wants}", candidate.state.describe())),
                    }
                })
                .collect::<Vec<_>>();
            let receiver = this.update(cx, |window, cx| {
                window.begin_owned_picker(None, MANAGE_TITLE.into(), items, None, cx)
            });
            let Ok(Ok(receiver)) = receiver else {
                return;
            };
            let Ok(crate::app::transient::TransientOutcome::Picker(name)) = receiver.await else {
                return;
            };
            let toggled = host.update(cx, |host, cx| {
                host.toggle_entitlement(name.clone(), Some(window), cx)
            });
            let outcome = toggled.await;
            let (severity, title, message) = match outcome.entry.result {
                crate::app::extensions::load::LoadResult::Loaded => {
                    (Severity::Info, format!("Allowed {name}"), None)
                }
                crate::app::extensions::load::LoadResult::NotLoaded(cause) => (
                    if outcome.granted {
                        Severity::Warning
                    } else {
                        Severity::Info
                    },
                    if outcome.granted {
                        format!("Allowed {name}, but it did not load")
                    } else {
                        format!("Denied {name}")
                    },
                    Some(cause),
                ),
                crate::app::extensions::load::LoadResult::Failed(cause) => (
                    Severity::Error,
                    format!("{name} failed"),
                    Some(cause),
                ),
            };
            Self::notice(&this, severity, &title, message, cx);
        });
        self.tasks.push(task);
    }
}
