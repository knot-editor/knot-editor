//! Composite view kinds that packages register and the instances they open.
//!
//! A kind is unique across packages and belongs to the lifecycle that
//! registered it. Each instance is a center tab the same lifecycle owns: it
//! closes when the instance is disposed, its kind is unregistered, or the
//! lifecycle ends.

use gpui::{App, Context};
use knot_protocol::{BufferHandle, DocumentDestination, ExtensionKey, PresentationTarget};
use serde::Deserialize;

use super::ProductExtensionHost;
use super::handlers::{AccessError, OpenError};
use crate::app::composite_view::{
    CompositeTab, ContentSpec, GroupSpec, LayoutAxis, LayoutSize, MemberSpec,
};
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

/// A layout as the package sends it: a row or column of editors on buffers
/// and nested rows and columns.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum LayoutWire {
    Editor {
        buffer: u64,
        #[serde(default)]
        size: SizeWire,
        #[serde(default)]
        focused: bool,
    },
    Row {
        children: Vec<LayoutWire>,
        #[serde(default)]
        size: SizeWire,
        #[serde(default)]
        focused: bool,
    },
    Column {
        children: Vec<LayoutWire>,
        #[serde(default)]
        size: SizeWire,
        #[serde(default)]
        focused: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum SizeWire {
    Weight(f32),
    Pixels(f32),
}

impl Default for SizeWire {
    fn default() -> Self {
        Self::Weight(1.)
    }
}

impl LayoutWire {
    /// The root of a layout is a row or a column with at least one editor
    /// beneath it, at most one member asking for focus per group, and sizes
    /// that are positive and finite.
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        fn check(node: &LayoutWire, root: bool) -> Result<usize, &'static str> {
            let (size, focused, children) = match node {
                LayoutWire::Editor { size, focused, .. } => (size, focused, None),
                LayoutWire::Row {
                    children,
                    size,
                    focused,
                }
                | LayoutWire::Column {
                    children,
                    size,
                    focused,
                } => (size, focused, Some(children)),
            };
            let (SizeWire::Weight(value) | SizeWire::Pixels(value)) = size;
            if !value.is_finite() || *value <= 0. {
                return Err("composite size must be a positive number");
            }
            if root && (children.is_none() || *focused) {
                return Err("composite layout must be a row or column");
            }
            let Some(children) = children else {
                return Ok(1);
            };
            let asking = children
                .iter()
                .filter(|child| match child {
                    LayoutWire::Editor { focused, .. }
                    | LayoutWire::Row { focused, .. }
                    | LayoutWire::Column { focused, .. } => *focused,
                })
                .count();
            if asking > 1 {
                return Err("a composite group has one focused member");
            }
            children
                .iter()
                .try_fold(0, |editors, child| Ok(editors + check(child, false)?))
        }
        match check(self, true)? {
            0 => Err("composite layout needs an editor"),
            _ => Ok(()),
        }
    }

    /// The spec of a checked root, with each editor's buffer from `model`.
    fn spec<E>(
        &self,
        model: &mut impl FnMut(BufferHandle) -> Result<gpui::Entity<crate::app::model::Buffer>, E>,
    ) -> Result<MemberSpec, E> {
        let (size, focused, content) = match self {
            Self::Editor {
                buffer,
                size,
                focused,
            } => (
                size,
                focused,
                ContentSpec::Editor(model(BufferHandle::new(*buffer))?),
            ),
            Self::Row {
                children,
                size,
                focused,
            } => (
                size,
                focused,
                ContentSpec::Group(Self::group(LayoutAxis::Row, children, model)?),
            ),
            Self::Column {
                children,
                size,
                focused,
            } => (
                size,
                focused,
                ContentSpec::Group(Self::group(LayoutAxis::Column, children, model)?),
            ),
        };
        Ok(MemberSpec {
            size: match size {
                SizeWire::Weight(weight) => LayoutSize::Weight(*weight),
                SizeWire::Pixels(pixels) => LayoutSize::Pixels(*pixels),
            },
            focused: *focused,
            content,
        })
    }

    fn group<E>(
        axis: LayoutAxis,
        children: &[LayoutWire],
        model: &mut impl FnMut(BufferHandle) -> Result<gpui::Entity<crate::app::model::Buffer>, E>,
    ) -> Result<GroupSpec, E> {
        Ok(GroupSpec {
            axis,
            members: children
                .iter()
                .map(|child| child.spec(model))
                .collect::<Result<_, _>>()?,
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
        layout: LayoutWire,
        cx: &mut Context<Self>,
    ) -> Result<PresentationTarget, OpenError> {
        if self.composite_kinds.get(&kind) != Some(&owner) {
            return Err(AccessError::InvalidTarget.into());
        }
        let mut model = |buffer| {
            self.extension_buffers
                .get(&buffer)
                .filter(|state| state.owner() == owner)
                .map(|state| state.model.clone())
                .ok_or(OpenError::BufferClosed)
        };
        // The root is a row or column, so its spec is a group.
        let ContentSpec::Group(layout) = layout.spec(&mut model)?.content else {
            return Err(AccessError::InvalidTarget.into());
        };
        let tab = tab.tab()?;
        let target = product::operations::open_composite(destination, tab, layout, cx)?;
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
