//! The zones an editor shows in rows reserved between its lines: the content
//! behind each, its measuring, and its painting. What each zone is and where
//! it sits is [`EditorZones`](knot_model::editor::zones::EditorZones) state and
//! the wrap map's rows.
//!
//! A zone's content is the same as a popover's: a toolbar over a layout of
//! views, which join the view tree as children of the editor's. The editor
//! measures the content's natural height, reserves whole rows for it, and
//! paints it over them.

use knot_model::editor::zones::ZoneSpec;
use knot_protocol::ExtensionKey;

use super::popovers::{ContentFocus, PopoverContent};
use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::view_layout::{GroupSpec, ToolbarButton};
use crate::app::view_tree::{ViewKind, ViewNode};
use crate::app::workbench::ExtensionView;

/// Knot closed a zone `owner` opened because the user dismissed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZoneClosed {
    pub owner: ExtensionKey,
    pub id: String,
}

impl EventEmitter<ZoneClosed> for Editor {}

/// What `open_zone` sets: where the zone sits, and what it shows.
pub(crate) struct ZoneContentSpec {
    pub spec: ZoneSpec,
    pub toolbar: Vec<ToolbarButton>,
    pub layout: GroupSpec,
}

/// The content behind one open zone.
pub(super) struct ZoneView {
    owner: ExtensionKey,
    id: String,
    pub(super) content: Entity<PopoverContent>,
    /// Re-measure when a member's content changes.
    _observers: Vec<Subscription>,
}

/// Where one zone paints, relative to the editor's top left.
pub(super) struct ZoneFrame {
    content: Entity<PopoverContent>,
    pub(super) top: f32,
    pub(super) left: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl Editor {
    /// Open `owner`'s zone `id` in the window `window`, or update it in
    /// place. An update keeps the views of keyed members. Returns the
    /// contributed views the content added, which the host binds. The zone
    /// has no rows until the view next measures it.
    pub(crate) fn open_zone(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        window: &str,
        content: ZoneContentSpec,
        cx: &mut Context<Self>,
    ) -> Vec<ExtensionView> {
        let model = self.model.clone();
        if content.spec.focus {
            self.content_focus = Some(ContentFocus::Zone(owner, id.to_owned()));
        }
        model.update(cx, |model, _| {
            self.state.open_zone(owner, id, content.spec, model)
        });
        let kept = self
            .zone_views
            .iter()
            .position(|zone| zone.owner == owner && zone.id == id);
        let (attached, created) = match kept {
            Some(at) => {
                let attached = self.zone_views[at].content.update(cx, |view, cx| {
                    view.update(content.toolbar, content.layout, window, cx)
                });
                (attached, self.zone_views[at].content.clone())
            }
            None => {
                let created =
                    cx.new(|cx| PopoverContent::new(content.toolbar, content.layout, window, cx));
                let attached = created
                    .read(cx)
                    .layout
                    .as_ref()
                    .map_or_else(Vec::new, |layout| layout.hosted_views());
                self.zone_views.push(ZoneView {
                    owner,
                    id: id.to_owned(),
                    content: created.clone(),
                    _observers: Vec::new(),
                });
                (attached, created)
            }
        };
        let observers = created
            .read(cx)
            .leaves_of_layout()
            .iter()
            .map(|leaf| leaf.observe(cx, |_, cx| cx.notify()))
            .collect();
        if let Some(view) = self
            .zone_views
            .iter_mut()
            .find(|zone| zone.owner == owner && zone.id == id)
        {
            view._observers = observers;
        }
        self.sync_zone_views(cx);
        crate::app::extensions::host::application_changed(cx);
        cx.notify();
        attached
    }

    /// Close `owner`'s zone `id`; false when it has none.
    pub(crate) fn close_zone(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let model = self.model.clone();
        let closed = model.update(cx, |model, _| self.state.close_zone(owner, id, model));
        self.sync_zone_views(cx);
        closed
    }

    /// Whether a view of a zone has focus.
    pub(crate) fn zone_focused(&self, cx: &App) -> bool {
        self.zone_views
            .iter()
            .any(|view| view.content.read(cx).inside)
    }

    /// Close the zone with focus, as Escape does, for its owner to hear;
    /// false when no zone has focus.
    pub(crate) fn dismiss_zone(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(view) = self
            .zone_views
            .iter()
            .find(|view| view.content.read(cx).inside)
        else {
            return false;
        };
        let Some(key) = self.state.zones().get(view.owner, &view.id).map(|z| z.key) else {
            return false;
        };
        let model = self.model.clone();
        let closed = model.update(cx, |model, _| self.state.dismiss_zone(key, model));
        self.sync_zone_views(cx);
        closed
    }

    /// Report the zones the user dismissed, move focus back to the text if
    /// one with focus went, drop the content of the zones the state no longer
    /// has, and order the rest as the state does.
    pub(super) fn sync_zone_views(&mut self, cx: &mut Context<Self>) {
        for closed in self.state.take_closed_zones() {
            cx.emit(ZoneClosed {
                owner: closed.owner,
                id: closed.id,
            });
        }
        let open = self.state.zones().zones();
        let lost_focus = self.zone_views.iter().any(|view| {
            view.content.read(cx).inside
                && !open
                    .iter()
                    .any(|zone| zone.owner == view.owner && zone.id == view.id)
        });
        if lost_focus {
            self.content_focus = Some(ContentFocus::Text);
            cx.notify();
        }
        if self.zone_views.len() == open.len()
            && self
                .zone_views
                .iter()
                .zip(open)
                .all(|(view, zone)| view.owner == zone.owner && view.id == zone.id)
        {
            return;
        }
        let mut views = std::mem::take(&mut self.zone_views);
        self.zone_views = open
            .iter()
            .filter_map(|zone| {
                let index = views
                    .iter()
                    .position(|view| view.owner == zone.owner && view.id == zone.id)?;
                Some(views.swap_remove(index))
            })
            .collect();
        crate::app::extensions::host::application_changed(cx);
        cx.notify();
    }

    /// Reserve for each zone the whole rows its content's natural height
    /// takes, at most the viewport's, in rows of `line_height` pixels. Does
    /// nothing before the editor knows its viewport; rows that already fit
    /// are left alone.
    pub(super) fn measure_zones(&mut self, line_height: f32, cx: &mut Context<Self>) {
        let viewport = self.state.viewport_rows().floor();
        if viewport < 1. || line_height <= 0. || self.zone_views.is_empty() {
            return;
        }
        let mut changes = Vec::new();
        for view in &self.zone_views {
            let Some(zone) = self.state.zones().get(view.owner, &view.id) else {
                continue;
            };
            let natural = view.content.read(cx).natural_height(cx);
            // A little under a whole row's height is not a row.
            let rows =
                ((natural / line_height - 1e-3).ceil().max(1.) as usize).min(viewport as usize);
            let current = self
                .state
                .wrap()
                .zones()
                .iter()
                .find(|entry| entry.key == zone.key)
                .map(|entry| entry.rows);
            if current != Some(rows) {
                changes.push((zone.key, rows));
            }
        }
        if changes.is_empty() {
            return;
        }
        let model = self.model.clone();
        model.update(cx, |model, _| {
            for (key, rows) in changes {
                self.state.set_zone_rows(key, rows, model);
            }
        });
        cx.notify();
    }

    /// The content of the zone with wrap-map key `key`.
    fn zone_content(&self, key: u64) -> Option<&Entity<PopoverContent>> {
        let zone = self.state.zones().by_key(key)?;
        self.zone_views
            .iter()
            .find(|view| view.owner == zone.owner && view.id == zone.id)
            .map(|view| &view.content)
    }

    /// The content of `owner`'s zone `id`.
    pub(super) fn zone_content_of(
        &self,
        owner: ExtensionKey,
        id: &str,
    ) -> Option<&Entity<PopoverContent>> {
        self.zone_views
            .iter()
            .find(|view| view.owner == owner && view.id == id)
            .map(|view| &view.content)
    }

    /// The nodes of the open zones, in the state's order, and the one with
    /// focus.
    pub(crate) fn zone_nodes(&self, cx: &App) -> (Vec<ViewNode>, Option<usize>) {
        let nodes = self
            .zone_views
            .iter()
            .map(|zone| {
                zone.content
                    .read(cx)
                    .node(zone.content.entity_id(), ViewKind::Zone, cx)
            })
            .collect();
        let focus = self
            .zone_views
            .iter()
            .position(|zone| zone.content.read(cx).inside);
        (nodes, focus)
    }

    /// The content of the open zones, in the state's order.
    #[cfg(test)]
    pub(crate) fn zone_contents(&self) -> Vec<Entity<PopoverContent>> {
        self.zone_views
            .iter()
            .map(|zone| zone.content.clone())
            .collect()
    }
}

impl EditorView {
    /// Measure the zones against the viewport as of now, before the frame
    /// reads their rows.
    pub(super) fn measure_zones(&mut self, cx: &mut Context<Self>) {
        let line_height = self.metrics.line_height;
        self.editor
            .update(cx, |editor, cx| editor.measure_zones(line_height, cx));
    }

    /// The zones that show in the viewport, over the rows reserved for them:
    /// the text area's width, whatever the horizontal scroll.
    pub(super) fn zone_frames(&self, cx: &App) -> Vec<ZoneFrame> {
        let editor = self.editor.read(cx);
        let line_height = self.metrics.line_height;
        let viewport = f32::from(self.bounds.size.height);
        let width = f32::from(self.text_bounds().size.width);
        editor
            .wrap()
            .zone_frames()
            .filter_map(|(key, rows)| {
                let top = (rows.start as f32 - editor.scroll_top()) * line_height;
                let height = rows.len() as f32 * line_height;
                if top + height <= 0. || top >= viewport {
                    return None;
                }
                Some(ZoneFrame {
                    content: editor.zone_content(key)?.clone(),
                    top,
                    left: self.gutter.width,
                    width,
                    height,
                })
            })
            .collect()
    }

    /// The zones over the text.
    pub(super) fn render_zones(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let theme = theme(cx);
        self.zone_frames(cx)
            .into_iter()
            .map(|frame| {
                div()
                    .absolute()
                    .top(px(frame.top))
                    .left(px(frame.left))
                    .w(px(frame.width))
                    .h(px(frame.height))
                    .occlude()
                    // What the zone's views leave of the wheel scrolls the
                    // editor.
                    .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| this.on_scroll(ev, cx)))
                    .overflow_hidden()
                    .border_y_1()
                    .border_color(theme.paint_color(color::POPOVER_BORDER))
                    .bg(theme.paint_color(color::POPOVER_BACKGROUND))
                    .child(frame.content)
                    .into_any_element()
            })
            .collect()
    }
}
