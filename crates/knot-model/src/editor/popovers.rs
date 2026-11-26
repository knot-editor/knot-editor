//! Editor popovers: non-modal panels floating over an editor's text, anchored
//! to a range of its lines. Each belongs to the lifecycle that opened it and
//! is named by an ID that lifecycle chose; popovers paint in opening order,
//! newest on top. The anchor is held in the buffer's anchored ranges, so
//! edits move it and deleting all of it closes the popover. Presentation
//! state, never recorded.

use std::ops::Range;

use knot_core::anchored_range::AnchoredRangeId;
use knot_protocol::ExtensionKey;

use crate::buffer::BufferModel;

/// How wide a popover is.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PopoverWidth {
    /// The editor's text-area width.
    #[default]
    Editor,
    /// Logical pixels, which zoom scales.
    Pixels(f32),
}

/// What a popover shows where.
#[derive(Clone, Debug, PartialEq)]
pub struct PopoverSpec {
    /// The buffer lines the popover is anchored to, as `start..end` with
    /// `end` exclusive.
    pub anchor: Range<u32>,
    pub width: PopoverWidth,
}

impl PopoverSpec {
    /// Why the spec is invalid: the anchor has no lines, or a pixel width is
    /// not a positive, finite number.
    pub fn problem(&self) -> Option<&'static str> {
        if self.anchor.start >= self.anchor.end {
            return Some("popover anchor must span at least one line");
        }
        if let PopoverWidth::Pixels(pixels) = self.width
            && (!pixels.is_finite() || pixels <= 0.)
        {
            return Some("popover width must be a positive number");
        }
        None
    }

    /// The width in window pixels in a text area `text_width` wide, with
    /// logical pixels multiplied by `zoom`. It never exceeds the text area.
    pub fn resolve_width(&self, text_width: f32, zoom: f32) -> f32 {
        match self.width {
            PopoverWidth::Editor => text_width,
            PopoverWidth::Pixels(pixels) => (pixels * zoom).min(text_width),
        }
        .max(0.)
    }
}

/// The natural height of content: the sum of a column's members, the tallest
/// of a row's or tabs' members, or a view's own height.
#[derive(Clone, Debug, PartialEq)]
pub enum Extent {
    Leaf(f32),
    /// Members side by side, or one shown at a time.
    Max(Vec<Extent>),
    /// Members stacked.
    Sum(Vec<Extent>),
}

impl Extent {
    pub fn height(&self) -> f32 {
        match self {
            Self::Leaf(height) => *height,
            Self::Max(members) => members.iter().map(Self::height).fold(0., f32::max),
            Self::Sum(members) => members.iter().map(Self::height).sum(),
        }
    }
}

/// Where a popover goes in the editor's viewport, vertically.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub top: f32,
    pub height: f32,
}

/// Place content `natural` tall against an anchor spanning `anchor` (top to
/// bottom, from the viewport's top) in a viewport `viewport` tall, leaving
/// `margin` at the viewport's edges. The popover sits below the anchor if it
/// fits, otherwise above, otherwise on the side with more room, clamped to
/// it. `None` while the anchor is wholly out of view or no room is left.
pub fn place(anchor: Range<f32>, viewport: f32, natural: f32, margin: f32) -> Option<Placement> {
    if anchor.end <= 0. || anchor.start >= viewport {
        return None;
    }
    let below = (viewport - anchor.end - margin).max(0.);
    let above = (anchor.start - margin).max(0.);
    let (top, height) = if natural <= below {
        (anchor.end, natural)
    } else if natural <= above {
        (anchor.start - natural, natural)
    } else if below >= above {
        (anchor.end, below)
    } else {
        (anchor.start - above, above)
    };
    (height > 0.).then_some(Placement { top, height })
}

/// One open popover.
#[derive(Clone, Debug)]
pub struct EditorPopover {
    pub owner: ExtensionKey,
    pub id: String,
    pub spec: PopoverSpec,
    anchor: AnchoredRangeId,
    /// Whether the anchored text was empty to begin with, so that an empty
    /// anchor is not a deletion.
    empty: bool,
}

/// The popovers of one editor, in opening order.
#[derive(Debug, Default)]
pub struct EditorPopovers {
    popovers: Vec<EditorPopover>,
}

impl EditorPopovers {
    /// The open popovers, bottom to top.
    pub fn popovers(&self) -> &[EditorPopover] {
        &self.popovers
    }

    pub fn get(&self, owner: ExtensionKey, id: &str) -> Option<&EditorPopover> {
        self.popovers
            .iter()
            .find(|popover| popover.owner == owner && popover.id == id)
    }

    /// Open `owner`'s popover `id` on top of the others, or replace what it
    /// shows in place, keeping its place in the order. The anchor lines clamp
    /// to the buffer's lines.
    pub fn open(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        spec: PopoverSpec,
        buffer: &mut BufferModel,
    ) {
        let bytes = line_bytes(buffer, &spec.anchor);
        let empty = bytes.is_empty();
        let anchor = buffer.add_view_range(bytes);
        match self
            .popovers
            .iter_mut()
            .find(|popover| popover.owner == owner && popover.id == id)
        {
            Some(popover) => {
                buffer.remove_view_position(Some(popover.anchor));
                popover.anchor = anchor;
                popover.empty = empty;
                popover.spec = spec;
            }
            None => self.popovers.push(EditorPopover {
                owner,
                id: id.to_owned(),
                spec,
                anchor,
                empty,
            }),
        }
    }

    /// Close `owner`'s popover `id`; false when it has none.
    pub fn close(&mut self, owner: ExtensionKey, id: &str, buffer: &mut BufferModel) -> bool {
        let count = self.popovers.len();
        self.retain(buffer, |popover| popover.owner != owner || popover.id != id);
        self.popovers.len() != count
    }

    /// Close every popover `owner` opened.
    pub fn remove_owner(&mut self, owner: ExtensionKey, buffer: &mut BufferModel) {
        self.retain(buffer, |popover| popover.owner != owner);
    }

    /// Close every popover, as the editor goes away.
    pub fn release(&mut self, buffer: &mut BufferModel) {
        self.retain(buffer, |_| false);
    }

    /// Close the popovers whose anchor an edit deleted; returns them.
    pub fn close_deleted(&mut self, buffer: &mut BufferModel) -> Vec<(ExtensionKey, String)> {
        let deleted = self
            .popovers
            .iter()
            .filter(|popover| {
                buffer
                    .resolve_view_position(Some(popover.anchor))
                    .is_none_or(|bytes| bytes.is_empty() && !popover.empty)
            })
            .map(|popover| (popover.owner, popover.id.clone()))
            .collect::<Vec<_>>();
        self.retain(buffer, |popover| {
            !deleted
                .iter()
                .any(|(owner, id)| *owner == popover.owner && *id == popover.id)
        });
        deleted
    }

    /// The buffer lines `popover` is anchored to now, as `start..end`.
    pub fn anchor_lines(
        &self,
        popover: &EditorPopover,
        buffer: &BufferModel,
    ) -> Option<Range<u32>> {
        let bytes = buffer.resolve_view_position(Some(popover.anchor))?;
        let start = buffer.line_of_offset(bytes.start) as u32;
        let end = if bytes.end > bytes.start {
            buffer.line_of_offset(bytes.end - 1) as u32 + 1
        } else {
            start + 1
        };
        Some(start..end)
    }

    fn retain(&mut self, buffer: &mut BufferModel, keep: impl Fn(&EditorPopover) -> bool) {
        self.popovers.retain(|popover| {
            let kept = keep(popover);
            if !kept {
                buffer.remove_view_position(Some(popover.anchor));
            }
            kept
        });
    }
}

/// The bytes of the lines `lines`, clamped to the buffer, through the end of
/// the last line's break.
fn line_bytes(buffer: &BufferModel, lines: &Range<u32>) -> Range<usize> {
    let last = buffer.line_count().saturating_sub(1);
    let first = (lines.start as usize).min(last);
    let end = (lines.end as usize).saturating_sub(1).clamp(first, last);
    buffer.line_range(first).start..buffer.line_range(end).end
}

#[cfg(test)]
mod tests {
    use knot_protocol::{ExtensionId, ExtensionLifecycleId};

    use super::*;

    fn owner(id: u64) -> ExtensionKey {
        ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1))
    }

    fn spec(anchor: Range<u32>) -> PopoverSpec {
        PopoverSpec {
            anchor,
            width: PopoverWidth::Editor,
        }
    }

    fn buffer(text: &str) -> BufferModel {
        BufferModel::from_text(text)
    }

    fn order(popovers: &EditorPopovers) -> Vec<(ExtensionKey, &str)> {
        popovers
            .popovers()
            .iter()
            .map(|popover| (popover.owner, popover.id.as_str()))
            .collect()
    }

    #[test]
    #[allow(clippy::reversed_empty_ranges)]
    fn spec_needs_lines_and_a_positive_width() {
        assert_eq!(spec(1..2).problem(), None);
        assert!(spec(2..2).problem().is_some());
        assert!(spec(3..2).problem().is_some());
        for pixels in [0., -1., f32::NAN, f32::INFINITY] {
            let bad = PopoverSpec {
                width: PopoverWidth::Pixels(pixels),
                ..spec(0..1)
            };
            assert!(bad.problem().is_some(), "{pixels}");
        }
    }

    #[test]
    fn width_scales_with_zoom_and_clamps_to_the_text_area() {
        let pixels = |pixels| PopoverSpec {
            width: PopoverWidth::Pixels(pixels),
            ..spec(0..1)
        };
        assert_eq!(spec(0..1).resolve_width(500., 2.), 500.);
        assert_eq!(pixels(300.).resolve_width(500., 1.), 300.);
        assert_eq!(pixels(300.).resolve_width(500., 1.5), 450.);
        assert_eq!(pixels(300.).resolve_width(500., 2.), 500.);
        assert_eq!(pixels(300.).resolve_width(120., 1.), 120.);
        assert_eq!(pixels(300.).resolve_width(0., 1.), 0.);
    }

    #[test]
    fn extents_sum_columns_and_take_the_tallest_of_rows() {
        use Extent::*;
        assert_eq!(Leaf(30.).height(), 30.);
        assert_eq!(Sum(vec![Leaf(30.), Leaf(20.)]).height(), 50.);
        assert_eq!(Max(vec![Leaf(30.), Leaf(20.)]).height(), 30.);
        let nested = Sum(vec![
            Leaf(10.),
            Max(vec![Leaf(30.), Sum(vec![Leaf(20.), Leaf(25.)])]),
        ]);
        assert_eq!(nested.height(), 55.);
        assert_eq!(Max(vec![]).height(), 0.);
    }

    #[test]
    fn a_popover_goes_below_above_or_on_the_roomier_side() {
        let place = |anchor: Range<f32>, natural| place(anchor, 400., natural, 10.);
        // Below the anchor's bottom when it fits.
        assert_eq!(
            place(100.0..120., 100.),
            Some(Placement {
                top: 120.,
                height: 100.
            })
        );
        // Above when it does not fit below but does above.
        assert_eq!(
            place(300.0..320., 150.),
            Some(Placement {
                top: 150.,
                height: 150.
            })
        );
        // Neither: the side with more room, clamped to it.
        assert_eq!(
            place(100.0..120., 500.),
            Some(Placement {
                top: 120.,
                height: 270.
            })
        );
        assert_eq!(
            place(280.0..300., 500.),
            Some(Placement {
                top: 10.,
                height: 270.
            })
        );
    }

    #[test]
    fn a_popover_hides_while_its_anchor_is_out_of_view() {
        assert_eq!(place(-40.0..-20., 400., 50., 10.), None);
        assert_eq!(place(400.0..420., 400., 50., 10.), None);
        // A partly visible anchor still shows it.
        assert!(place(-10.0..10., 400., 50., 10.).is_some());
        // An anchor at the bottom leaves no room below, and none above is
        // needed when the content fits there.
        assert_eq!(
            place(380.0..400., 400., 50., 10.),
            Some(Placement {
                top: 330.,
                height: 50.
            })
        );
        // No room on either side.
        assert_eq!(place(0.0..400., 400., 50., 10.), None);
    }

    #[test]
    fn popovers_stack_in_opening_order_and_update_in_place() {
        let mut buffer = buffer("a\nb\nc\n");
        let mut popovers = EditorPopovers::default();
        popovers.open(owner(1), "hunk", spec(0..1), &mut buffer);
        popovers.open(owner(2), "hunk", spec(1..2), &mut buffer);
        popovers.open(owner(1), "hover", spec(2..3), &mut buffer);
        popovers.open(owner(1), "hunk", spec(1..3), &mut buffer);
        assert_eq!(
            order(&popovers),
            [(owner(1), "hunk"), (owner(2), "hunk"), (owner(1), "hover")]
        );
        let hunk = popovers.get(owner(1), "hunk").unwrap();
        assert_eq!(popovers.anchor_lines(hunk, &buffer), Some(1..3));
        // Replacing an anchor leaves exactly one per popover.
        let anchors = buffer.anchored_range_count();
        popovers.open(owner(1), "hunk", spec(0..1), &mut buffer);
        assert_eq!(buffer.anchored_range_count(), anchors);

        assert!(popovers.close(owner(2), "hunk", &mut buffer));
        assert!(!popovers.close(owner(2), "hunk", &mut buffer));
        popovers.remove_owner(owner(1), &mut buffer);
        assert!(popovers.popovers().is_empty());
        assert_eq!(buffer.anchored_range_count(), 0);
    }

    #[test]
    fn the_anchor_follows_edits_and_deleting_it_closes_the_popover() {
        let mut buffer = buffer("a\nb\nc\nd\n");
        let mut popovers = EditorPopovers::default();
        popovers.open(owner(1), "hunk", spec(1..3), &mut buffer);
        let lines = |popovers: &EditorPopovers, buffer: &BufferModel| {
            popovers.anchor_lines(popovers.get(owner(1), "hunk").unwrap(), buffer)
        };

        buffer.replace(0..0, "new\n").unwrap();
        assert_eq!(lines(&popovers, &buffer), Some(2..4));
        assert!(popovers.close_deleted(&mut buffer).is_empty());

        // Deleting the anchored lines (now "b\nc\n") deletes the anchor.
        let start = buffer.line_range(2).start;
        let end = buffer.line_range(3).end;
        buffer.replace(start..end, "").unwrap();
        assert_eq!(
            popovers.close_deleted(&mut buffer),
            [(owner(1), "hunk".to_owned())]
        );
        assert!(popovers.popovers().is_empty());
        assert_eq!(buffer.anchored_range_count(), 0);
    }

    #[test]
    fn anchor_lines_clamp_to_the_buffer() {
        let mut buffer = buffer("a\nb");
        let mut popovers = EditorPopovers::default();
        popovers.open(owner(1), "far", spec(5..9), &mut buffer);
        let far = popovers.get(owner(1), "far").unwrap();
        assert_eq!(popovers.anchor_lines(far, &buffer), Some(1..2));
        popovers.release(&mut buffer);
        assert_eq!(buffer.anchored_range_count(), 0);
    }
}
