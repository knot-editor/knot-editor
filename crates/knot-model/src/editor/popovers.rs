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

/// What a popover shows where.
#[derive(Clone, Debug, PartialEq)]
pub struct PopoverSpec {
    /// The buffer lines the popover is anchored to, as `start..end` with
    /// `end` exclusive.
    pub anchor: Range<u32>,
    /// The content's height in logical pixels.
    pub height: f32,
}

impl PopoverSpec {
    /// Why the spec is invalid: the anchor has no lines, or the height is
    /// not a positive, finite number.
    pub fn problem(&self) -> Option<&'static str> {
        if self.anchor.start >= self.anchor.end {
            return Some("popover anchor must span at least one line");
        }
        if !self.height.is_finite() || self.height <= 0. {
            return Some("popover height must be a positive number");
        }
        None
    }
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
            height: 100.,
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
    fn spec_needs_lines_and_a_positive_height() {
        assert_eq!(spec(1..2).problem(), None);
        assert!(spec(2..2).problem().is_some());
        assert!(spec(3..2).problem().is_some());
        for height in [0., -1., f32::NAN, f32::INFINITY] {
            let bad = PopoverSpec {
                height,
                ..spec(0..1)
            };
            assert!(bad.problem().is_some(), "{height}");
        }
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
