//! Named extension providers and served contracts: registrations in
//! registration order, owned by the lifecycle that registered them. A name may
//! have several registrations; lookups by name return the latest one or all of
//! them, oldest first. A contract client binds one consumer to one contract
//! registration and lives as long as both.

use std::sync::Arc;

use knot_protocol::{
    ContractClientId, ExtensionId, ExtensionLifecycleId, ProviderInfo, ProviderRegistrationId,
};

use crate::contracts::{ContractBinding, ContractSchema};

/// The contract a registration serves: the package whose contract module
/// defines it, its name there, and the provider's schema.
#[derive(Clone, Debug, PartialEq)]
pub struct ServedContract {
    pub owner: String,
    pub name: String,
    pub schema: Arc<ContractSchema>,
}

impl ServedContract {
    /// The contract as violations and failures name it.
    pub fn label(&self) -> String {
        format!("{} {}", self.owner, self.name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRegistration {
    pub id: ProviderRegistrationId,
    pub name: String,
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub methods: Vec<String>,
    /// Set for a served contract, which name lookups never return.
    pub contract: Option<ServedContract>,
}

impl ProviderRegistration {
    pub fn info(&self) -> ProviderInfo {
        ProviderInfo {
            registration: self.id,
            methods: self.methods.clone(),
        }
    }
}

/// One consumer's binding to a contract registration, with the schemas its
/// calls validate against.
#[derive(Clone, Debug)]
pub struct ContractClient {
    pub id: ContractClientId,
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub registration: ProviderRegistrationId,
    consumer: Arc<ContractSchema>,
    pub binding: ContractBinding,
}

#[derive(Debug, Default)]
pub struct ProviderRegistry {
    next_registration: u64,
    next_client: u64,
    /// Kept in registration order, which identities also follow.
    registrations: Vec<ProviderRegistration>,
    clients: Vec<ContractClient>,
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
        self.push(extension, lifecycle, name, methods, None)
    }

    pub fn serve(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        contract: ServedContract,
    ) -> ProviderRegistrationId {
        let methods = contract.schema.methods().map(str::to_owned).collect();
        self.push(
            extension,
            lifecycle,
            contract.name.clone(),
            methods,
            Some(contract),
        )
    }

    fn push(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        name: String,
        methods: Vec<String>,
        contract: Option<ServedContract>,
    ) -> ProviderRegistrationId {
        self.next_registration += 1;
        let id = ProviderRegistrationId::new(self.next_registration);
        self.registrations.push(ProviderRegistration {
            id,
            name,
            extension,
            lifecycle,
            methods,
            contract,
        });
        id
    }

    /// Removes a registration the lifecycle owns, with the clients bound to
    /// it; false when it owns none under that identity.
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
        self.clients.retain(|client| client.registration != id);
        self.registrations.len() != before
    }

    pub fn registration(&self, id: ProviderRegistrationId) -> Option<&ProviderRegistration> {
        self.registrations
            .iter()
            .find(|registration| registration.id == id)
    }

    /// The most recent provider registration under `name`.
    pub fn get(&self, name: &str) -> Option<&ProviderRegistration> {
        self.registrations
            .iter()
            .rev()
            .find(|registration| registration.contract.is_none() && registration.name == name)
    }

    /// Every provider registration under `name`, oldest first.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a ProviderRegistration> {
        self.registrations.iter().filter(move |registration| {
            registration.contract.is_none() && registration.name == name
        })
    }

    /// The most recent registration serving `owner`'s contract `name`.
    pub fn latest_contract(&self, owner: &str, name: &str) -> Option<&ProviderRegistration> {
        self.registrations.iter().rev().find(|registration| {
            registration
                .contract
                .as_ref()
                .is_some_and(|contract| contract.owner == owner && contract.name == name)
        })
    }

    /// Every registration serving `owner`'s contract `name`, oldest first.
    pub fn contracts<'a>(
        &'a self,
        owner: &'a str,
        name: &'a str,
    ) -> impl Iterator<Item = &'a ProviderRegistration> {
        self.registrations.iter().filter(move |registration| {
            registration
                .contract
                .as_ref()
                .is_some_and(|contract| contract.owner == owner && contract.name == name)
        })
    }

    /// Binds a consumer with schema `consumer` to a contract registration,
    /// reusing its binding with the same schema. None when the registration
    /// serves no contract.
    pub fn bind(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        registration: ProviderRegistrationId,
        consumer: ContractSchema,
    ) -> Option<ContractClientId> {
        let provider = self.registration(registration)?.contract.as_ref()?;
        if let Some(existing) = self.clients.iter().find(|client| {
            (client.extension, client.lifecycle, client.registration)
                == (extension, lifecycle, registration)
                && *client.consumer == consumer
        }) {
            return Some(existing.id);
        }
        let consumer = Arc::new(consumer);
        let binding = ContractBinding::new(provider.schema.clone(), Some(consumer.clone()));
        self.next_client += 1;
        let id = ContractClientId::new(self.next_client);
        self.clients.push(ContractClient {
            id,
            extension,
            lifecycle,
            registration,
            consumer,
            binding,
        });
        Some(id)
    }

    /// A client of `extension`'s and the registration it is bound to, while
    /// both are live.
    pub fn client(
        &self,
        id: ContractClientId,
        extension: ExtensionId,
    ) -> Option<(&ContractClient, &ProviderRegistration)> {
        let client = self
            .clients
            .iter()
            .find(|client| client.id == id && client.extension == extension)?;
        Some((client, self.registration(client.registration)?))
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        self.registrations.retain(|registration| {
            (registration.extension, registration.lifecycle) != (extension, lifecycle)
        });
        let registrations = &self.registrations;
        self.clients.retain(|client| {
            (client.extension, client.lifecycle) != (extension, lifecycle)
                && registrations
                    .iter()
                    .any(|registration| registration.id == client.registration)
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

    fn schema(result: &str) -> ContractSchema {
        ContractSchema::parse(&format!(
            r#"{{"rows":{{"kind":"fn","argument":{{"kind":"void"}},"result":{{"kind":"{result}"}}}}}}"#
        ))
        .unwrap()
    }

    fn serve(
        registry: &mut ProviderRegistry,
        owner: (ExtensionId, ExtensionLifecycleId),
        name: &str,
    ) -> ProviderRegistrationId {
        registry.serve(
            owner.0,
            owner.1,
            ServedContract {
                owner: "@example/provider".into(),
                name: name.into(),
                schema: Arc::new(schema("string")),
            },
        )
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

    #[test]
    fn contracts_are_found_by_owner_and_name_and_never_by_provider_name() {
        let mut registry = ProviderRegistry::new();
        let first = serve(&mut registry, owner(1), "rows");
        let second = serve(&mut registry, owner(2), "rows");
        let provider = register(&mut registry, owner(1), "rows");

        assert_eq!(registry.get("rows").map(|found| found.id), Some(provider));
        assert_eq!(registry.all("rows").count(), 1);
        assert_eq!(
            registry
                .latest_contract("@example/provider", "rows")
                .map(|found| found.id),
            Some(second)
        );
        assert!(registry.latest_contract("@example/other", "rows").is_none());
        assert_eq!(
            registry
                .contracts("@example/provider", "rows")
                .map(|found| found.id)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        let (extension, lifecycle) = owner(2);
        registry.unregister(second, extension, lifecycle);
        assert_eq!(
            registry
                .latest_contract("@example/provider", "rows")
                .map(|found| found.id),
            Some(first)
        );
    }

    #[test]
    fn a_client_lives_as_long_as_its_consumer_and_its_registration() {
        let mut registry = ProviderRegistry::new();
        let (provider, provider_lifecycle) = owner(1);
        let (consumer, consumer_lifecycle) = owner(2);
        let served = serve(&mut registry, owner(1), "rows");
        let named = register(&mut registry, owner(1), "rows");
        assert!(
            registry
                .bind(consumer, consumer_lifecycle, named, schema("string"))
                .is_none()
        );

        let client = registry
            .bind(consumer, consumer_lifecycle, served, schema("string"))
            .unwrap();
        assert_eq!(
            registry.bind(consumer, consumer_lifecycle, served, schema("string")),
            Some(client),
            "the same consumer schema reuses its binding"
        );
        let stale = registry
            .bind(consumer, consumer_lifecycle, served, schema("int"))
            .unwrap();
        assert_ne!(stale, client);
        let (found, registration) = registry.client(client, consumer).unwrap();
        assert!(!found.binding.checks_both());
        assert_eq!(registration.id, served);
        assert!(
            registry
                .client(stale, consumer)
                .unwrap()
                .0
                .binding
                .checks_both()
        );
        assert!(
            registry.client(client, provider).is_none(),
            "a client belongs to its consumer"
        );

        registry.unregister(served, provider, provider_lifecycle);
        assert!(registry.client(client, consumer).is_none());

        let served = serve(&mut registry, owner(1), "rows");
        let client = registry
            .bind(consumer, consumer_lifecycle, served, schema("string"))
            .unwrap();
        registry.remove_lifecycle(consumer, consumer_lifecycle);
        assert!(registry.client(client, consumer).is_none());
        let client = registry
            .bind(
                consumer,
                ExtensionLifecycleId::new(2),
                served,
                schema("string"),
            )
            .unwrap();
        registry.remove_lifecycle(provider, provider_lifecycle);
        assert!(registry.client(client, consumer).is_none());
    }
}
