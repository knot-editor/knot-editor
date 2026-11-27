//! Composite view kinds that packages register and the instances they open.
//!
//! A kind is unique across packages and belongs to the lifecycle that
//! registered it. Each instance is a center tab the same lifecycle owns: it
//! closes when the instance is disposed, its kind is unregistered, or the
//! lifecycle ends.

use crate::app::editor::PopoverContentSpec;
use gpui::{App, Context};
use knot_protocol::{
    BufferHandle, DocumentDestination, ExtensionKey, PresentationTarget, ViewContentKind,
};
use serde::Deserialize;
use std::collections::HashSet;

use super::ProductExtensionHost;
use super::handlers::{AccessError, OpenError};
use crate::app::composite_view::{CompositeTab, ScrollLink};
use crate::app::extensions::semantics::SemanticError;
use crate::app::product;
use crate::app::view_controls::ControlItem;
use crate::app::view_layout::{
    Arrangement, CONNECTOR_WIDTH, ConnectorSpec, ContentSpec, GroupSpec, LayoutSize, MemberSpec,
    ToolbarButton,
};
use knot_model::editor::popovers::{PopoverSpec, PopoverWidth};

/// What `openComposite` opens: an instance of a registered kind.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompositeOpening {
    pub kind: String,
    pub tab: CompositeTabSpec,
    pub layout: LayoutWire,
    pub preview: bool,
}

/// What `openComposite` and `updateComposite` set on a tab and its toolbar.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CompositeTabSpec {
    pub title: String,
    pub icon: Option<knot_protocol::IconRef>,
    pub resource: Option<String>,
    pub toolbar: Vec<ToolbarButton>,
    pub link: Option<ScrollLink>,
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
            toolbar: self.toolbar,
            link: self.link,
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
    size: Option<SizeWire>,
    #[serde(default)]
    focused: bool,
    /// The label of a member of tabs.
    #[serde(default)]
    title: Option<String>,
    /// Names the size the user drags a pixel-sized member to.
    #[serde(default)]
    resize: Option<String>,
    #[serde(flatten)]
    content: ContentWire,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ContentWire {
    /// An editor on a buffer of `buffers.create` or on a document, by
    /// identity.
    Editor {
        #[serde(default)]
        buffer: Option<u64>,
        #[serde(default)]
        document: Option<String>,
        #[serde(default)]
        max_lines: Option<u32>,
        /// Muted text shown while the buffer is empty.
        #[serde(default)]
        placeholder: Option<String>,
        /// Whether the editor shows its gutter; true when absent.
        #[serde(default)]
        gutter: Option<bool>,
    },
    /// A text frame of the embedded view of this ID.
    Text {
        view: String,
        #[serde(default)]
        max_lines: Option<u32>,
    },
    /// A tree of the embedded view of this ID.
    Tree {
        view: String,
        #[serde(default)]
        max_rows: Option<u32>,
    },
    /// A strip that joins lines of two editors of the layout.
    Connector(ConnectorSpec),
    /// A wrapping row of checkboxes and buttons.
    Controls {
        items: Vec<ControlItem>,
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
    /// are positive and finite, a title on exactly the members of tabs, keys
    /// that are not empty and name one member each, and connectors that join
    /// two editors by key and have no key, focus, or tabs of their own,
    /// and controls that have no key, size, focus, or tabs either and are not
    /// in a popover.
    ///
    /// A popover's layout may hold no view, leaving only its toolbar.
    pub(crate) fn check(&self, popover: bool) -> Result<(), &'static str> {
        #[derive(Default)]
        struct Seen {
            keys: HashSet<String>,
            editors: HashSet<String>,
            connectors: Vec<(String, String)>,
        }
        fn check(
            node: &LayoutWire,
            root: bool,
            in_tabs: bool,
            popover: bool,
            seen: &mut Seen,
        ) -> Result<usize, &'static str> {
            if let Some(key) = &node.key
                && (key.is_empty() || !seen.keys.insert(key.clone()))
            {
                return Err("a composite key is not empty and names one member");
            }
            let (SizeWire::Weight(value) | SizeWire::Pixels(value)) = node.size.unwrap_or_default();
            if !value.is_finite() || value <= 0. {
                return Err("composite size must be a positive number");
            }
            match (in_tabs, &node.title) {
                (true, None) => return Err("a composite tab needs a title"),
                (false, Some(_)) => return Err("only a member of tabs has a title"),
                _ => {}
            }
            if let ContentWire::Editor {
                buffer, document, ..
            } = &node.content
            {
                if buffer.is_some() == document.is_some() {
                    return Err("a composite editor shows a buffer or a document");
                }
                if let Some(key) = &node.key {
                    seen.editors.insert(key.clone());
                }
            }
            if matches!(node.content, ContentWire::Controls { .. }) {
                if node.key.is_some() || node.size.is_some() || node.focused || in_tabs || popover {
                    return Err(
                        "composite controls have no key, size, or focus and are not in tabs or popovers",
                    );
                }
                return Ok(0);
            }
            if let ContentWire::Connector(connector) = &node.content {
                if node.key.is_some() || node.focused || in_tabs {
                    return Err("a composite connector has no key or focus and is not in tabs");
                }
                seen.connectors
                    .push((connector.first.clone(), connector.second.clone()));
                return Ok(0);
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
                Ok(views
                    + check(
                        child,
                        false,
                        arrangement == Arrangement::Tabs,
                        popover,
                        seen,
                    )?)
            })
        }
        let mut seen = Seen::default();
        let views = check(self, true, false, popover, &mut seen)?;
        if seen.connectors.iter().any(|(first, second)| {
            first == second || !seen.editors.contains(first) || !seen.editors.contains(second)
        }) {
            return Err("a composite connector joins two editors of the layout by key");
        }
        match views {
            0 if !popover => Err("composite layout needs a view"),
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
        let size = match (self.size, &self.content) {
            (Some(SizeWire::Weight(weight)), _) => LayoutSize::Weight(weight),
            (Some(SizeWire::Pixels(pixels)), _) => LayoutSize::Pixels(pixels),
            (None, ContentWire::Connector(_)) => LayoutSize::Pixels(CONNECTOR_WIDTH),
            (None, ContentWire::Controls { .. }) => LayoutSize::Auto,
            (None, _) => LayoutSize::default(),
        };
        Ok(MemberSpec {
            key: self.key.clone(),
            size,
            focused: self.focused,
            title: self.title.clone(),
            resize: self.resize.clone(),
            limit: match self.content {
                ContentWire::Editor { max_lines, .. } | ContentWire::Text { max_lines, .. } => {
                    max_lines
                }
                ContentWire::Tree { max_rows, .. } => max_rows,
                _ => None,
            },
            content,
        })
    }
}

/// What `openPopover` sets besides its layout.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PopoverOptions {
    #[serde(default)]
    pub width: PopoverWidth,
    #[serde(default)]
    pub focus: bool,
    pub toolbar: Vec<ToolbarButton>,
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
    pub(super) fn composite_spec(
        &self,
        owner: ExtensionKey,
        layout: LayoutWire,
        cx: &App,
    ) -> Result<GroupSpec, OpenError> {
        let mut leaf = |content: &ContentWire| match content {
            ContentWire::Editor {
                buffer: Some(buffer),
                placeholder,
                gutter,
                ..
            } => self
                .extension_buffers
                .get(&BufferHandle::new(*buffer))
                .filter(|state| state.owner() == owner)
                .map(|state| ContentSpec::Editor {
                    buffer: state.model.clone(),
                    document: None,
                    placeholder: placeholder.clone(),
                    gutter: gutter.unwrap_or(true),
                })
                .ok_or(OpenError::BufferClosed),
            ContentWire::Editor {
                document: Some(identity),
                placeholder,
                gutter,
                ..
            } => product::operations::document_model(identity, cx).map(|(document, buffer)| {
                ContentSpec::Editor {
                    buffer,
                    document: Some(document),
                    placeholder: placeholder.clone(),
                    gutter: gutter.unwrap_or(true),
                }
            }),
            ContentWire::Editor { .. } => Err(AccessError::InvalidTarget.into()),
            ContentWire::Text { view, .. } => self
                .embedded_view(view, owner, ViewContentKind::Text)
                .map(|_| ContentSpec::Text(view.clone())),
            ContentWire::Tree { view, .. } => self
                .embedded_view(view, owner, ViewContentKind::Tree)
                .map(|_| ContentSpec::Tree(view.clone())),
            ContentWire::Connector(connector) => Ok(ContentSpec::Connector(connector.clone())),
            ContentWire::Controls { items } => Ok(ContentSpec::Controls(items.clone())),
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
        opening: CompositeOpening,
        cx: &mut Context<Self>,
    ) -> Result<PresentationTarget, OpenError> {
        let CompositeOpening {
            kind,
            tab,
            layout,
            preview,
        } = opening;
        if self.composite_kinds.get(&kind) != Some(&owner) {
            return Err(AccessError::InvalidTarget.into());
        }
        let layout = self.composite_spec(owner, layout, &*cx)?;
        let tab = tab.tab()?;
        let (target, window, hosted) =
            product::operations::open_composite(destination, tab, layout, preview, cx)?;
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

    /// What a popover anchored to `anchor` shows: the toolbar and layout,
    /// whose buffers, documents, and views must be `owner`'s.
    pub(crate) fn popover_content(
        &self,
        owner: ExtensionKey,
        anchor: std::ops::Range<u32>,
        options: PopoverOptions,
        layout: LayoutWire,
        cx: &App,
    ) -> Result<PopoverContentSpec, OpenError> {
        let spec = PopoverSpec {
            anchor,
            width: options.width,
            focus: options.focus,
        };
        if spec.problem().is_some() {
            return Err(AccessError::InvalidTarget.into());
        }
        Ok(PopoverContentSpec {
            spec,
            toolbar: options.toolbar,
            layout: self.composite_spec(owner, layout, cx)?,
        })
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

    /// Make the preview tab of an instance `owner` opened regular.
    pub(crate) fn keep_composite(
        &mut self,
        owner: ExtensionKey,
        target: &PresentationTarget,
        cx: &mut App,
    ) -> Result<(), OpenError> {
        if !self
            .owned_composites
            .get(&owner)
            .is_some_and(|owned| owned.iter().any(|instance| &instance.target == target))
        {
            return Err(AccessError::InvalidTarget.into());
        }
        product::operations::keep_composite(target, cx)
    }

    /// Show the tab of an instance `owner` opened.
    pub(crate) fn activate_composite(
        &mut self,
        owner: ExtensionKey,
        target: &PresentationTarget,
        cx: &mut App,
    ) -> Result<(), OpenError> {
        if !self
            .owned_composites
            .get(&owner)
            .is_some_and(|owned| owned.iter().any(|instance| &instance.target == target))
        {
            return Err(AccessError::InvalidTarget.into());
        }
        product::operations::activate_composite(target, cx)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_limits_reach_the_spec() {
        let wire: LayoutWire = serde_json::from_str(
            r#"{"type":"column","children":[
                {"type":"editor","buffer":1,"maxLines":5},
                {"type":"tree","view":"t","maxRows":7},
                {"type":"text","view":"x"}]}"#,
        )
        .unwrap();
        let ContentSpec::Group(group) = wire
            .spec(&mut |content| match content {
                ContentWire::Tree { view, .. } | ContentWire::Text { view, .. } => {
                    Ok::<_, ()>(ContentSpec::Text(view.clone()))
                }
                _ => Ok(ContentSpec::Text(String::new())),
            })
            .unwrap()
            .content
        else {
            panic!("a group");
        };
        let limits: Vec<_> = group.members.iter().map(|member| member.limit).collect();
        assert_eq!(limits, [Some(5), Some(7), None]);
    }
}
