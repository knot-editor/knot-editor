//! Named extension providers: registrations in registration order, owned by
//! the lifecycle that registered them. A name may have several registrations;
//! lookups by name return the latest one or all of them, oldest first.

use knot_protocol::{ExtensionId, ExtensionLifecycleId, ProviderInfo, ProviderRegistrationId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRegistration {
    pub id: ProviderRegistrationId,
    pub name: String,
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub methods: Vec<String>,
}

impl ProviderRegistration {
    pub fn info(&self) -> ProviderInfo {
        ProviderInfo {
            registration: self.id,
            methods: self.methods.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ProviderRegistry {
    next_registration: u64,
    /// Kept in registration order, which identities also follow.
    registrations: Vec<ProviderRegistration>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        name: String,
        methods: Vec<String>,
    ) -> ProviderRegistrationId {
        self.next_registration += 1;
        let id = ProviderRegistrationId::new(self.next_registration);
        self.registrations.push(ProviderRegistration {
            id,
            name,
            extension,
            lifecycle,
            methods,
        });
        id
    }

    /// Removes a registration the lifecycle owns; false when it owns none
    /// under that identity.
    pub fn unregister(
        &mut self,
        id: ProviderRegistrationId,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    ) -> bool {
        let before = self.registrations.len();
        self.registrations.retain(|registration| {
            !(registration.id == id
                && registration.extension == extension
                && registration.lifecycle == lifecycle)
        });
        self.registrations.len() != before
    }

    pub fn registration(&self, id: ProviderRegistrationId) -> Option<&ProviderRegistration> {
        self.registrations
            .iter()
            .find(|registration| registration.id == id)
    }

    /// The most recent registration under `name`.
    pub fn get(&self, name: &str) -> Option<&ProviderRegistration> {
        self.registrations
            .iter()
            .rev()
            .find(|registration| registration.name == name)
    }

    /// Every registration under `name`, oldest first.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a ProviderRegistration> {
        self.registrations
            .iter()
            .filter(move |registration| registration.name == name)
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        self.registrations.retain(|registration| {
            (registration.extension, registration.lifecycle) != (extension, lifecycle)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(value: u64) -> (ExtensionId, ExtensionLifecycleId) {
        (ExtensionId::new(value), ExtensionLifecycleId::new(1))
    }

    fn register(
        registry: &mut ProviderRegistry,
        owner: (ExtensionId, ExtensionLifecycleId),
        name: &str,
    ) -> ProviderRegistrationId {
        registry.register(owner.0, owner.1, name.into(), vec!["rows".into()])
    }

    #[test]
    fn get_returns_the_latest_registration_and_all_keeps_registration_order() {
        let mut registry = ProviderRegistry::new();
        let first = register(&mut registry, owner(1), "workspace.files");
        let other = register(&mut registry, owner(2), "editor.completion");
        let second = register(&mut registry, owner(2), "workspace.files");

        assert_eq!(
            registry.get("workspace.files").map(|found| found.id),
            Some(second)
        );
        assert_eq!(
            registry
                .all("workspace.files")
                .map(|found| found.id)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(
            registry.get("editor.completion").map(|found| found.id),
            Some(other)
        );
        assert!(registry.get("missing").is_none());
        assert_eq!(registry.all("missing").count(), 0);
    }

    #[test]
    fn disposing_the_latest_registration_uncovers_the_previous_one() {
        let mut registry = ProviderRegistry::new();
        let first = register(&mut registry, owner(1), "workspace.files");
        let second = register(&mut registry, owner(2), "workspace.files");

        let (extension, lifecycle) = owner(1);
        assert!(
            !registry.unregister(second, extension, lifecycle),
            "only the owner unregisters"
        );
        let (extension, lifecycle) = owner(2);
        assert!(registry.unregister(second, extension, lifecycle));
        assert!(!registry.unregister(second, extension, lifecycle));

        assert_eq!(
            registry.get("workspace.files").map(|found| found.id),
            Some(first)
        );
        assert!(registry.registration(second).is_none());
    }

    #[test]
    fn removing_a_lifecycle_drops_only_its_registrations() {
        let mut registry = ProviderRegistry::new();
        let kept = register(&mut registry, owner(1), "workspace.files");
        register(&mut registry, owner(2), "workspace.files");
        register(&mut registry, owner(2), "workspace.buffers");

        let (extension, lifecycle) = owner(2);
        registry.remove_lifecycle(extension, lifecycle);

        assert_eq!(
            registry.get("workspace.files").map(|found| found.id),
            Some(kept)
        );
        assert!(registry.get("workspace.buffers").is_none());
        let restarted = registry.register(
            extension,
            ExtensionLifecycleId::new(2),
            "workspace.files".into(),
            Vec::new(),
        );
        assert!(
            restarted.value() > kept.value(),
            "identities are never reused"
        );
    }
}
