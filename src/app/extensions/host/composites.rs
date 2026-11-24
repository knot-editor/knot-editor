//! Composite view kinds that packages register and the instances they open.
//!
//! A kind is unique across packages and belongs to the lifecycle that
//! registered it. Each instance is a center tab the same lifecycle owns: it
//! closes when the instance is disposed, its kind is unregistered, or the
//! lifecycle ends.

use gpui::{App, Context};
use knot_protocol::{BufferHandle, DocumentDestination, ExtensionKey, PresentationTarget};

use super::ProductExtensionHost;
use super::handlers::{AccessError, OpenError};
use crate::app::composite_view::CompositeTab;
use crate::app::extensions::semantics::SemanticError;
use crate::app::product;

/// What `openComposite` and `updateComposite` set on a tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CompositeTabSpec {
    pub title: String,
    pub icon: Option<knot_protocol::IconRef>,
    pub resource: Option<String>,
}

impl CompositeTabSpec {
    fn tab(self) -> Result<CompositeTab, OpenError> {
        Ok(CompositeTab {
            title: self.title,
            icon: self.icon,
            resource: self
                .resource
                .map(|uri| {
                    crate::app::resource::ResourceUri::parse(&uri)
                        .map_err(|_| OpenError::InvalidUri)
                })
                .transpose()?,
        })
    }
}

pub(super) struct OwnedComposite {
    kind: String,
    target: PresentationTarget,
}

impl ProductExtensionHost {
    pub(crate) fn register_composite_kind(
        &mut self,
        owner: ExtensionKey,
        kind: String,
    ) -> Result<(), SemanticError> {
        if kind.is_empty() || kind.starts_with("knot.") || kind.chars().any(char::is_whitespace) {
            return Err(SemanticError::InvalidViewId);
        }
        if self.composite_kinds.contains_key(&kind) {
            return Err(SemanticError::ViewIdInUse);
        }
        self.composite_kinds.insert(kind, owner);
        Ok(())
    }

    /// Remove a kind of the owner and close its instances.
    pub(crate) fn unregister_composite_kind(
        &mut self,
        owner: ExtensionKey,
        kind: &str,
        cx: &mut App,
    ) {
        if self.composite_kinds.get(kind) != Some(&owner) {
            return;
        }
        self.composite_kinds.remove(kind);
        let instances = self.owned_composites.entry(owner).or_default();
        let (closing, kept) = std::mem::take(instances)
            .into_iter()
            .partition(|instance| instance.kind == kind);
        *instances = kept;
        for instance in closing {
            product::operations::close_presentation_unattended(instance.target, cx);
        }
    }

    pub(crate) fn open_composite(
        &mut self,
        owner: ExtensionKey,
        destination: DocumentDestination,
        kind: String,
        tab: CompositeTabSpec,
        buffer: BufferHandle,
        cx: &mut Context<Self>,
    ) -> Result<PresentationTarget, OpenError> {
        if self.composite_kinds.get(&kind) != Some(&owner) {
            return Err(AccessError::InvalidTarget.into());
        }
        let model = self
            .extension_buffers
            .get(&buffer)
            .filter(|state| state.owner() == owner)
            .ok_or(OpenError::BufferClosed)?
            .model
            .clone();
        let tab = tab.tab()?;
        let target = product::operations::open_composite(destination, tab, model, cx)?;
        self.owned_composites
            .entry(owner)
            .or_default()
            .push(OwnedComposite {
                kind,
                target: target.clone(),
            });
        Ok(target)
    }

    pub(crate) fn update_composite(
        &mut self,
        owner: ExtensionKey,
        target: &PresentationTarget,
        tab: CompositeTabSpec,
        cx: &mut App,
    ) -> Result<(), OpenError> {
        if !self
            .owned_composites
            .get(&owner)
            .is_some_and(|owned| owned.iter().any(|instance| &instance.target == target))
        {
            return Err(AccessError::InvalidTarget.into());
        }
        product::operations::update_composite(target, tab.tab()?, cx)?;
        Ok(())
    }

    /// End the kinds and instances one lifecycle owns.
    pub(super) fn release_composites_of(&mut self, owner: ExtensionKey, cx: &mut App) {
        self.composite_kinds
            .retain(|_, kind_owner| *kind_owner != owner);
        for instance in self.owned_composites.remove(&owner).unwrap_or_default() {
            product::operations::close_presentation_unattended(instance.target, cx);
        }
    }
}
