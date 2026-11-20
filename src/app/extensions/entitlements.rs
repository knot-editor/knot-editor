//! The application's entitlement decisions: one versioned record in the
//! application state database, so a decision survives restarts.

use std::time::Duration;

use gpui::{App, Global};
use knot_model::extensions::entitlements::{Decision, EntitlementGrants};
use knot_model::extensions::package::Entitlement;

use crate::app::record_store::{RecordStore, StateDatabase};

const VERSION: u32 = 1;
const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

/// The decisions, and the record they persist to when the application has a
/// state database.
#[derive(Default)]
pub(crate) struct ApplicationEntitlements {
    grants: EntitlementGrants,
    record: Option<RecordStore<EntitlementGrants>>,
}

impl Global for ApplicationEntitlements {}

impl ApplicationEntitlements {
    /// Load the persisted decisions; a missing or unreadable record means no
    /// decisions.
    pub(crate) fn open(database: &StateDatabase) -> Self {
        let record = database.record("entitlement_grants", VERSION, WRITE_DEBOUNCE);
        Self {
            grants: record.current().unwrap_or_default(),
            record: Some(record),
        }
    }
}

pub(crate) fn grants(cx: &App) -> EntitlementGrants {
    cx.try_global::<ApplicationEntitlements>()
        .map(|entitlements| entitlements.grants.clone())
        .unwrap_or_default()
}

/// Record `decision` for `entitlements` of `package` and schedule its write.
pub(crate) fn decide(
    package: &str,
    entitlements: &[Entitlement],
    decision: Decision,
    cx: &mut App,
) {
    let state = cx.default_global::<ApplicationEntitlements>();
    state.grants.decide(package, entitlements, decision);
    if let Some(record) = &state.record {
        record.publish(state.grants.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn decisions_survive_reopen(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = StateDatabase::open(path.clone(), cx.background_executor.clone());
        cx.update(|cx| {
            cx.set_global(ApplicationEntitlements::open(&database));
            assert_eq!(grants(cx).decision("@a/b", Entitlement::Process), None);
            decide("@a/b", &[Entitlement::Process], Decision::Denied, cx);
            decide("@c/d", &[Entitlement::Process], Decision::Granted, cx);
            assert_eq!(
                grants(cx).decision("@a/b", Entitlement::Process),
                Some(Decision::Denied)
            );
        });
        database.flush().await.unwrap();
        drop(database);

        let reopened = StateDatabase::open(path, cx.background_executor.clone());
        let state = ApplicationEntitlements::open(&reopened);
        assert_eq!(
            state.grants.decision("@a/b", Entitlement::Process),
            Some(Decision::Denied)
        );
        assert_eq!(
            state.grants.decision("@c/d", Entitlement::Process),
            Some(Decision::Granted)
        );
    }
}
