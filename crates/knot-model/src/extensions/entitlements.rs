//! Entitlement decisions and the load policy they imply.
//!
//! A user decides per package identity and entitlement, independent of
//! version, so an update that declares a new entitlement asks only about it.
//! A package with any denied or undecided entitlement does not load. Bundled
//! packages ship in the binary, which grants what they declare.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::extensions::package::{Entitlement, InstalledPackage, PackageOrigin};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Granted,
    Denied,
}

/// Every recorded decision, by canonical package identity.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EntitlementGrants {
    packages: BTreeMap<String, BTreeMap<Entitlement, Decision>>,
}

impl EntitlementGrants {
    pub fn decision(&self, package: &str, entitlement: Entitlement) -> Option<Decision> {
        self.packages.get(package)?.get(&entitlement).copied()
    }

    /// Record `decision` for each of `entitlements`, replacing earlier ones.
    pub fn decide(&mut self, package: &str, entitlements: &[Entitlement], decision: Decision) {
        let decisions = self.packages.entry(package.to_owned()).or_default();
        for entitlement in entitlements {
            decisions.insert(*entitlement, decision);
        }
    }
}

/// An installed package with entitlements the user has not decided on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndecidedPackage {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub entitlements: Vec<Entitlement>,
}

/// The installed packages that declare an entitlement without a decision, in
/// identity order.
pub fn undecided(
    packages: &[InstalledPackage],
    grants: &EntitlementGrants,
) -> Vec<UndecidedPackage> {
    let mut asked = packages
        .iter()
        .filter_map(|package| undecided_package(package, grants))
        .collect::<Vec<_>>();
    asked.sort_by(|left, right| left.name.cmp(&right.name));
    asked
}

/// `package`'s undecided entitlements, when it is installed and has any.
pub fn undecided_package(
    package: &InstalledPackage,
    grants: &EntitlementGrants,
) -> Option<UndecidedPackage> {
    if package.origin == PackageOrigin::Bundled {
        return None;
    }
    let entitlements = package
        .manifest
        .entitlements
        .iter()
        .copied()
        .filter(|entitlement| {
            grants
                .decision(&package.manifest.name, *entitlement)
                .is_none()
        })
        .collect::<Vec<_>>();
    (!entitlements.is_empty()).then(|| UndecidedPackage {
        name: package.manifest.name.clone(),
        display_name: package
            .manifest
            .display_name
            .clone()
            .unwrap_or_else(|| package.manifest.name.clone()),
        version: package.manifest.version.clone(),
        entitlements,
    })
}

/// Why `package` may not load: a declared entitlement is denied or has no
/// decision. Denial is named before an undecided entitlement.
pub fn not_loaded_cause(package: &InstalledPackage, grants: &EntitlementGrants) -> Option<String> {
    if package.origin == PackageOrigin::Bundled {
        return None;
    }
    let decisions = package
        .manifest
        .entitlements
        .iter()
        .map(|entitlement| {
            (
                *entitlement,
                grants.decision(&package.manifest.name, *entitlement),
            )
        })
        .collect::<Vec<_>>();
    if let Some((entitlement, _)) = decisions
        .iter()
        .find(|(_, decision)| *decision == Some(Decision::Denied))
    {
        return Some(format!(
            "not loaded: entitlement `{}` denied",
            entitlement.name()
        ));
    }
    decisions
        .iter()
        .find(|(_, decision)| decision.is_none())
        .map(|(entitlement, _)| {
            format!(
                "not loaded: entitlement `{}` not decided",
                entitlement.name()
            )
        })
}

/// The packages that may not load, with the report cause of each.
pub fn not_loaded(
    packages: &[InstalledPackage],
    grants: &EntitlementGrants,
) -> BTreeMap<String, String> {
    packages
        .iter()
        .filter_map(|package| {
            not_loaded_cause(package, grants).map(|cause| (package.manifest.name.clone(), cause))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::extensions::package::Manifest;

    fn package(
        name: &str,
        origin: PackageOrigin,
        entitlements: &[Entitlement],
    ) -> InstalledPackage {
        InstalledPackage {
            directory: PathBuf::from(name),
            origin,
            manifest: Manifest {
                api_version: 1,
                name: name.into(),
                version: "1.0.0".into(),
                main: "main.js".into(),
                contract: None,
                requires: Vec::new(),
                entitlements: entitlements.to_vec(),
                display_name: None,
                description: None,
                author: None,
                website: None,
                license: None,
            },
            sources: BTreeMap::new(),
        }
    }

    fn installed(name: &str, entitlements: &[Entitlement]) -> InstalledPackage {
        package(name, PackageOrigin::Installed, entitlements)
    }

    #[test]
    fn a_new_package_asks_about_every_declared_entitlement() {
        let packages = [installed("@test/git", &[Entitlement::Process])];
        let grants = EntitlementGrants::default();
        let asked = undecided(&packages, &grants);
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].name, "@test/git");
        assert_eq!(asked[0].display_name, "@test/git");
        assert_eq!(asked[0].entitlements, [Entitlement::Process]);
        assert_eq!(
            not_loaded_cause(&packages[0], &grants).unwrap(),
            "not loaded: entitlement `process` not decided"
        );
    }

    #[test]
    fn a_decision_survives_an_update_and_covers_only_what_it_decided() {
        let mut grants = EntitlementGrants::default();
        grants.decide("@test/git", &[Entitlement::Process], Decision::Granted);
        let mut updated = installed("@test/git", &[Entitlement::Process]);
        updated.manifest.version = "2.0.0".into();
        assert!(undecided(&[updated.clone()], &grants).is_empty());
        assert!(not_loaded_cause(&updated, &grants).is_none());
        assert_eq!(
            undecided_package(&installed("@test/other", &[Entitlement::Process]), &grants)
                .unwrap()
                .name,
            "@test/other"
        );
    }

    #[test]
    fn a_denied_entitlement_blocks_loading_without_asking_again() {
        let mut grants = EntitlementGrants::default();
        grants.decide("@test/git", &[Entitlement::Process], Decision::Denied);
        let packages = [installed("@test/git", &[Entitlement::Process])];
        assert!(undecided(&packages, &grants).is_empty());
        assert_eq!(
            not_loaded(&packages, &grants)["@test/git"],
            "not loaded: entitlement `process` denied"
        );
    }

    #[test]
    fn a_later_decision_replaces_an_earlier_one() {
        let mut grants = EntitlementGrants::default();
        grants.decide("@test/git", &[Entitlement::Process], Decision::Denied);
        grants.decide("@test/git", &[Entitlement::Process], Decision::Granted);
        assert!(
            not_loaded_cause(&installed("@test/git", &[Entitlement::Process]), &grants).is_none()
        );
    }

    #[test]
    fn bundled_and_undeclaring_packages_are_never_asked_about_or_blocked() {
        let grants = EntitlementGrants::default();
        let packages = [
            package("@knot/git", PackageOrigin::Bundled, &[Entitlement::Process]),
            installed("@test/plain", &[]),
        ];
        assert!(undecided(&packages, &grants).is_empty());
        assert!(not_loaded(&packages, &grants).is_empty());
    }

    #[test]
    fn grants_round_trip_through_json() {
        let mut grants = EntitlementGrants::default();
        grants.decide("@test/git", &[Entitlement::Process], Decision::Granted);
        let text = serde_json::to_string(&grants).unwrap();
        assert_eq!(
            serde_json::from_str::<EntitlementGrants>(&text).unwrap(),
            grants
        );
    }
}
