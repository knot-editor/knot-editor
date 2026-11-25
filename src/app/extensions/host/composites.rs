//! Composite view kinds that packages register and the instances they open.
//!
//! A kind is unique across packages and belongs to the lifecycle that
//! registered it. Each instance is a center tab the same lifecycle owns: it
//! closes when the instance is disposed, its kind is unregistered, or the
//! lifecycle ends.

use gpui::{App, Context};
use knot_protocol::{
    BufferHandle, DocumentDestination, ExtensionKey, PresentationTarget, ViewContentKind,
};
use serde::Deserialize;
use std::collections::HashSet;

use super::ProductExtensionHost;
use super::handlers::{AccessError, OpenError};
use crate::app::composite_view::{
    Arrangement, CompositeTab, ContentSpec, GroupSpec, LayoutSize, MemberSpec,
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

/// One member of a layout as the package sends it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutWire {
    /// Names the member across layout updates.
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    size: SizeWire,
    #[serde(default)]
    focused: bool,
    /// The label of a member of tabs.
    #[serde(default)]
    title: Option<String>,
    #[serde(flatten)]
    content: ContentWire,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ContentWire {
    /// An editor on a buffer of `buffers.create` or on a document, by
    /// identity.
    Editor {
        #[serde(default)]
        buffer: Option<u64>,
        #[serde(default)]
        document: Option<String>,
    },
    /// A text frame of the embedded view of this ID.
    Text {
        view: String,
    },
    /// A tree of the embedded view of this ID.
    Tree {
        view: String,
    },
    Row {
        children: Vec<LayoutWire>,
    },
    Column {
        children: Vec<LayoutWire>,
    },
    Tabs {
        children: Vec<LayoutWire>,
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
    fn children(&self) -> Option<(Arrangement, &[LayoutWire])> {
        match &self.content {
            ContentWire::Row { children } => Some((Arrangement::Row, children)),
            ContentWire::Column { children } => Some((Arrangement::Column, children)),
            ContentWire::Tabs { children } => Some((Arrangement::Tabs, children)),
            _ => None,
        }
    }

    /// The root of a layout is a row or column with at least one view
    /// beneath it, at most one member asking for focus per group, sizes that
    /// are positive and finite, a title on exactly the members of tabs, and
    /// keys that are not empty and name one member each.
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        fn check(
            node: &LayoutWire,
            root: bool,
            in_tabs: bool,
            keys: &mut HashSet<String>,
        ) -> Result<usize, &'static str> {
            if let Some(key) = &node.key
                && (key.is_empty() || !keys.insert(key.clone()))
            {
                return Err("a composite key is not empty and names one member");
            }
            let (SizeWire::Weight(value) | SizeWire::Pixels(value)) = node.size;
            if !value.is_finite() || value <= 0. {
                return Err("composite size must be a positive number");
            }
            match (in_tabs, &node.title) {
                (true, None) => return Err("a composite tab needs a title"),
                (false, Some(_)) => return Err("only a member of tabs has a title"),
                _ => {}
            }
            if let ContentWire::Editor { buffer, document } = &node.content
                && buffer.is_some() == document.is_some()
            {
                return Err("a composite editor shows a buffer or a document");
            }
            let group = node.children();
            if root
                && (node.focused
                    || group.is_none_or(|(arrangement, _)| arrangement == Arrangement::Tabs))
            {
                return Err("composite layout must be a row or column");
            }
            let Some((arrangement, children)) = group else {
                return Ok(1);
            };
            if children.iter().filter(|child| child.focused).count() > 1 {
                return Err("a composite group has one focused member");
            }
            children.iter().try_fold(0, |views, child| {
                Ok(views + check(child, false, arrangement == Arrangement::Tabs, keys)?)
            })
        }
        match check(self, true, false, &mut HashSet::new())? {
            0 => Err("composite layout needs a view"),
            _ => Ok(()),
        }
    }

    /// The spec of a checked layout, with each leaf's content from `leaf`.
    fn spec<E>(
        &self,
        leaf: &mut impl FnMut(&ContentWire) -> Result<ContentSpec, E>,
    ) -> Result<MemberSpec, E> {
        let content = match self.children() {
            Some((arrangement, children)) => ContentSpec::Group(GroupSpec {
                arrangement,
                members: children
                    .iter()
                    .map(|child| child.spec(leaf))
                    .collect::<Result<_, _>>()?,
            }),
            None => leaf(&self.content)?,
        };
        Ok(MemberSpec {
            key: self.key.clone(),
            size: match self.size {
                SizeWire::Weight(weight) => LayoutSize::Weight(weight),
                SizeWire::Pixels(pixels) => LayoutSize::Pixels(pixels),
            },
            focused: self.focused,
            title: self.title.clone(),
            content,
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

    /// The spec of `layout`, whose buffers, documents, and views must be
    /// `owner`'s.
    fn composite_spec(
        &self,
        owner: ExtensionKey,
        layout: LayoutWire,
        cx: &App,
    ) -> Result<GroupSpec, OpenError> {
        let mut leaf = |content: &ContentWire| match content {
            ContentWire::Editor {
                buffer: Some(buffer),
                ..
            } => self
                .extension_buffers
                .get(&BufferHandle::new(*buffer))
                .filter(|state| state.owner() == owner)
                .map(|state| ContentSpec::Editor {
                    buffer: state.model.clone(),
                    document: None,
                })
                .ok_or(OpenError::BufferClosed),
            ContentWire::Editor {
                document: Some(identity),
                ..
            } => product::operations::document_model(identity, cx).map(|(document, buffer)| {
                ContentSpec::Editor {
                    buffer,
                    document: Some(document),
                }
            }),
            ContentWire::Editor { .. } => Err(AccessError::InvalidTarget.into()),
            ContentWire::Text { view } => self
                .embedded_view(view, owner, ViewContentKind::Text)
                .map(|_| ContentSpec::Text(view.clone())),
            ContentWire::Tree { view } => self
                .embedded_view(view, owner, ViewContentKind::Tree)
                .map(|_| ContentSpec::Tree(view.clone())),
            ContentWire::Row { .. } | ContentWire::Column { .. } | ContentWire::Tabs { .. } => {
                Err(AccessError::InvalidTarget.into())
            }
        };
        // The root is a row or column, so its spec is a group.
        let ContentSpec::Group(layout) = layout.spec(&mut leaf)?.content else {
            return Err(AccessError::InvalidTarget.into());
        };
        Ok(layout)
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
        let layout = self.composite_spec(owner, layout, &*cx)?;
        let tab = tab.tab()?;
        let (target, window, hosted) =
            product::operations::open_composite(destination, tab, layout, cx)?;
        for view in hosted {
            let kind = view.kind(cx);
            self.attach_view(kind, window.clone(), view, cx);
        }
        self.owned_composites
            .entry(owner)
            .or_default()
            .push(OwnedComposite {
                kind,
                target: target.clone(),
            });
        Ok(target)
    }

    /// Whether `owner` registered an embedded view `id` with `content`.
    fn embedded_view(
        &self,
        id: &str,
        owner: ExtensionKey,
        content: ViewContentKind,
    ) -> Result<(), OpenError> {
        match self.semantics.embedded_content(id, owner) {
            Some(found) if found == content => Ok(()),
            _ => Err(AccessError::InvalidTarget.into()),
        }
    }

    /// Replace what the tab shows and, when `layout` is given, the layout.
    pub(crate) fn update_composite(
        &mut self,
        owner: ExtensionKey,
        target: &PresentationTarget,
        tab: CompositeTabSpec,
        layout: Option<LayoutWire>,
        cx: &mut Context<Self>,
    ) -> Result<(), OpenError> {
        if !self
            .owned_composites
            .get(&owner)
            .is_some_and(|owned| owned.iter().any(|instance| &instance.target == target))
        {
            return Err(AccessError::InvalidTarget.into());
        }
        let layout = layout
            .map(|layout| self.composite_spec(owner, layout, &*cx))
            .transpose()?;
        let (window, hosted) =
            product::operations::update_composite(target, tab.tab()?, layout, cx)?;
        for view in hosted {
            let kind = view.kind(cx);
            self.attach_view(kind, window.clone(), view, cx);
        }
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
