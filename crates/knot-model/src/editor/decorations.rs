//! Decorations that belong to one view. Each extension lifecycle owns one set
//! per editor, anchored in the buffer like contributions so edits move it, and
//! only this editor paints it, merged with the buffer's contributions.
//! Presentation state, never recorded.

use knot_protocol::{Decoration, ExtensionKey};

use super::*;
use crate::buffer::ContributionError;

/// One decoration of a view's set: `decoration` painted over `range`.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewDecoration {
    pub range: ByteRange,
    pub decoration: Decoration,
}

/// One lifecycle's decorations on this view, as anchored ranges.
pub(super) struct ViewDecorationSet {
    owner: ExtensionKey,
    items: Vec<(AnchoredRangeId, Decoration)>,
}

impl EditorState {
    /// Atomically replace `owner`'s decorations on this view at buffer
    /// revision `if_revision`; an empty list clears them. Ranges follow the
    /// rules of buffer contributions.
    pub fn set_decorations(
        &mut self,
        owner: ExtensionKey,
        items: &[ViewDecoration],
        if_revision: u64,
        buffer: &mut BufferModel,
    ) -> Result<(), ContributionError> {
        let ranges =
            buffer.check_contribution_ranges(items.iter().map(|item| item.range), if_revision)?;
        self.remove_decorations(owner, buffer);
        if !items.is_empty() {
            let items = ranges
                .into_iter()
                .zip(items)
                .map(|(range, item)| (buffer.add_view_range(range), item.decoration.clone()))
                .collect();
            self.view_decorations
                .push(ViewDecorationSet { owner, items });
        }
        self.project_contributions(buffer);
        Ok(())
    }

    /// Drop `owner`'s decorations on this view, if it has any.
    pub(super) fn remove_decorations(&mut self, owner: ExtensionKey, buffer: &mut BufferModel) {
        self.view_decorations.retain(|set| {
            if set.owner != owner {
                return true;
            }
            for (id, _) in &set.items {
                buffer.remove_view_position(Some(*id));
            }
            false
        });
    }

    /// Release every view decoration's anchor, as the editor goes away.
    pub(super) fn release_decorations(&mut self, buffer: &mut BufferModel) {
        for set in std::mem::take(&mut self.view_decorations) {
            for (id, _) in set.items {
                buffer.remove_view_position(Some(id));
            }
        }
    }

    /// Project the buffer's contributions and this view's decorations into
    /// painted decorations, gutter markers, and click actions. View
    /// decorations paint above buffer ones of the same precedence.
    pub(super) fn project_contributions(&mut self, model: &BufferModel) {
        let mut contributions = model.resolved_contributions();
        for set in &self.view_decorations {
            let source = ContributionSource {
                extension: set.owner.extension,
                lifecycle: set.owner.lifecycle,
            };
            // A deletion that consumed a range's text removed its anchor.
            contributions.extend(set.items.iter().filter_map(|(id, decoration)| {
                let range = model.resolve_view_position(Some(*id))?;
                Some(ResolvedEditorContribution {
                    range: ByteRange {
                        start_byte_offset: range.start,
                        end_byte_offset: range.end,
                    },
                    source,
                    decoration: Some(decoration.clone()),
                    gutter: None,
                    command: None,
                })
            }));
        }
        contributions.sort_by_key(|contribution| {
            contribution
                .decoration
                .as_ref()
                .map_or(0, |decoration| decoration_precedence(&decoration.color))
        });
        (
            self.decorations,
            self.gutter_markers,
            self.contribution_actions,
        ) = project_contributions(model, contributions);
    }
}
