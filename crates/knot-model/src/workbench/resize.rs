//! Split ratios and pane geometry: dividers, keyboard resizing, equalizing,
//! and directional neighbors.

use super::*;

/// A logical-pixel rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

/// Where the bottom dock sits relative to the side docks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BottomDockLayout {
    /// Under the center, between the side docks, which span the full height.
    #[default]
    Contained,
    /// Across the full width, with the side docks above it.
    Full,
}

impl BottomDockLayout {
    /// The layout a setting value names; unknown values are contained.
    pub fn parse(value: &str) -> Self {
        match value {
            "full" => Self::Full,
            _ => Self::Contained,
        }
    }
}

/// The share of a split's extent that one grow or shrink moves.
const RESIZE_STEP: f32 = 0.05;
/// Logical pixels that one grow or shrink adds to or removes from a dock.
const DOCK_RESIZE_STEP: f32 = 40.;

fn find_split(layout: &WorkbenchLayout, boundary: PaneId) -> Option<&WorkbenchLayout> {
    if layout.boundary() == Some(boundary) {
        return Some(layout);
    }
    match layout {
        WorkbenchLayout::Pane(_) => None,
        WorkbenchLayout::Split { first, second, .. } => {
            find_split(first, boundary).or_else(|| find_split(second, boundary))
        }
    }
}

fn equalize(layout: &mut WorkbenchLayout) {
    if let WorkbenchLayout::Split {
        direction,
        ratio,
        first,
        second,
    } = layout
    {
        let first_weight = first.weight(*direction);
        *ratio = first_weight / (first_weight + second.weight(*direction));
        equalize(first);
        equalize(second);
    }
}

/// Lay panes out in `area` from split ratios, ignoring dividers.
fn place(layout: &WorkbenchLayout, area: Rect, result: &mut Vec<(PaneId, Rect)>) {
    let WorkbenchLayout::Split {
        direction,
        ratio,
        first,
        second,
    } = layout
    else {
        if let WorkbenchLayout::Pane(pane) = layout {
            result.push((*pane, area));
        }
        return;
    };
    let (first_area, second_area) = if *direction == SplitDirection::Horizontal {
        let extent = area.width * ratio;
        (
            Rect {
                width: extent,
                ..area
            },
            Rect {
                x: area.x + extent,
                width: area.width - extent,
                ..area
            },
        )
    } else {
        let extent = area.height * ratio;
        (
            Rect {
                height: extent,
                ..area
            },
            Rect {
                y: area.y + extent,
                height: area.height - extent,
                ..area
            },
        )
    };
    place(first, first_area, result);
    place(second, second_area, result);
}

/// The pane beside `from` toward `edge`: of the panes lying entirely on that
/// side, the one overlapping `from` most along the other axis, then the
/// nearest.
pub fn neighbor(rects: &[(PaneId, Rect)], from: PaneId, edge: SplitEdge) -> Option<PaneId> {
    const EPSILON: f32 = 0.5;
    let (_, source) = rects.iter().find(|(pane, _)| *pane == from)?;
    let overlap = |a: f32, a_len: f32, b: f32, b_len: f32| (a + a_len).min(b + b_len) - a.max(b);
    rects
        .iter()
        .filter(|(pane, _)| *pane != from)
        .filter_map(|(pane, rect)| {
            let (distance, shared) = match edge {
                SplitEdge::Left => (
                    source.x - (rect.x + rect.width),
                    overlap(source.y, source.height, rect.y, rect.height),
                ),
                SplitEdge::Right => (
                    rect.x - (source.x + source.width),
                    overlap(source.y, source.height, rect.y, rect.height),
                ),
                SplitEdge::Top => (
                    source.y - (rect.y + rect.height),
                    overlap(source.x, source.width, rect.x, rect.width),
                ),
                SplitEdge::Bottom => (
                    rect.y - (source.y + source.height),
                    overlap(source.x, source.width, rect.x, rect.width),
                ),
            };
            (distance > -EPSILON && shared > EPSILON).then_some((*pane, shared, distance))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.2.total_cmp(&a.2)))
        .map(|(pane, _, _)| pane)
}

impl Workbench {
    /// Each painted area's rectangle when the workbench fills `bounds`: the
    /// side docks and the bottom dock surround the center as `layout`
    /// arranges them, and the center takes the rest.
    pub fn area_rects(&self, bounds: Rect, layout: BottomDockLayout) -> Vec<(Area, Rect)> {
        let shown = |dock: DockLocation| self.area_shown(Area::Dock(dock));
        let bottom = if shown(DockLocation::Bottom) {
            self.dock_size(DockLocation::Bottom).min(bounds.height)
        } else {
            0.
        };
        // The band the side docks and the center share.
        let upper = match layout {
            BottomDockLayout::Contained => bounds,
            BottomDockLayout::Full => Rect {
                height: bounds.height - bottom,
                ..bounds
            },
        };
        let left = if shown(DockLocation::Left) {
            self.dock_size(DockLocation::Left).min(bounds.width)
        } else {
            0.
        };
        let right = if shown(DockLocation::Right) {
            self.dock_size(DockLocation::Right).min(bounds.width - left)
        } else {
            0.
        };
        let middle = Rect {
            x: bounds.x + left,
            width: bounds.width - left - right,
            ..upper
        };
        let center_height = match layout {
            BottomDockLayout::Contained => middle.height - bottom,
            BottomDockLayout::Full => middle.height,
        };
        let mut rects = vec![(
            Area::Center,
            Rect {
                height: center_height,
                ..middle
            },
        )];
        if left > 0. {
            rects.push((
                Area::Dock(DockLocation::Left),
                Rect {
                    width: left,
                    ..upper
                },
            ));
        }
        if right > 0. {
            rects.push((
                Area::Dock(DockLocation::Right),
                Rect {
                    x: middle.x + middle.width,
                    width: right,
                    ..upper
                },
            ));
        }
        if bottom > 0. {
            let span = match layout {
                BottomDockLayout::Contained => middle,
                BottomDockLayout::Full => bounds,
            };
            rects.push((
                Area::Dock(DockLocation::Bottom),
                Rect {
                    y: bounds.y + bounds.height - bottom,
                    height: bottom,
                    ..span
                },
            ));
        }
        rects
    }

    /// Every painted pane's rectangle when the workbench fills `bounds`.
    pub fn pane_rects(&self, bounds: Rect, layout: BottomDockLayout) -> Vec<(PaneId, Rect)> {
        let mut result = Vec::new();
        for (area, rect) in self.area_rects(bounds, layout) {
            if let Some(layout) = self.area_layout(area) {
                place(layout, rect, &mut result);
            }
        }
        result
    }

    fn split_with_boundary_mut(&mut self, boundary: PaneId) -> Option<&mut WorkbenchLayout> {
        let area = self.pane_area(boundary)?;
        self.tree_mut(area)
            .layout
            .as_mut()?
            .split_with_boundary_mut(boundary)
    }

    pub fn split_ratio(&self, boundary: PaneId) -> Option<f32> {
        let layout = self.area_layout(self.pane_area(boundary)?)?;
        match find_split(layout, boundary)? {
            WorkbenchLayout::Split { ratio, .. } => Some(*ratio),
            WorkbenchLayout::Pane(_) => None,
        }
    }

    /// Set the ratio of the split identified by `boundary`, clamped so
    /// neither side drops below the minimum share.
    pub fn set_split_ratio(&mut self, boundary: PaneId, value: f32) -> bool {
        self.set_split_ratio_bounded(
            boundary,
            value,
            WorkbenchLayout::MIN_RATIO,
            1. - WorkbenchLayout::MIN_RATIO,
        )
    }

    /// Set the ratio of the split identified by `boundary`, clamped to
    /// `[min, max]` instead of [`WorkbenchLayout::clamp_ratio`]. Pointer
    /// drags use this with a pixel-derived floor so the growing pane can
    /// pass the minimum share until the shrinking side hits its own
    /// minimum.
    pub fn set_split_ratio_bounded(
        &mut self,
        boundary: PaneId,
        value: f32,
        min: f32,
        max: f32,
    ) -> bool {
        let Some(WorkbenchLayout::Split { ratio, .. }) = self.split_with_boundary_mut(boundary)
        else {
            return false;
        };
        *ratio = if value.is_finite() {
            value.clamp(min, max)
        } else {
            0.5
        };
        true
    }

    /// Grow or shrink a pane by one step. A dock pane changes its dock's
    /// size, unless an `axis` across the dock picks a split inside it; any
    /// other pane changes the ratio of its nearest enclosing split, along
    /// `axis` when given: a horizontal split sets the width and a vertical
    /// one the height. Returns whether anything changed.
    pub fn resize_pane(&mut self, pane: PaneId, grow: bool, axis: Option<SplitDirection>) -> bool {
        let sign = if grow { 1. } else { -1. };
        let Some(area) = self.pane_area(pane) else {
            return false;
        };
        if let Area::Dock(dock) = area
            && axis.is_none_or(|axis| axis == dock.axis())
        {
            let before = self.dock_size(dock);
            self.set_dock_size(dock, before + sign * DOCK_RESIZE_STEP);
            return self.dock_size(dock) != before;
        }
        let Some((boundary, in_second, ratio)) = self.area_layout(area).and_then(|layout| {
            layout
                .ancestors(pane)
                .into_iter()
                .rev()
                .find(|(split, _)| {
                    axis.is_none_or(|axis| {
                        matches!(split, WorkbenchLayout::Split { direction, .. } if *direction == axis)
                    })
                })
                .and_then(|(split, in_second)| match split {
                    WorkbenchLayout::Split { ratio, .. } => {
                        Some((split.boundary()?, in_second, *ratio))
                    }
                    WorkbenchLayout::Pane(_) => None,
                })
        }) else {
            return false;
        };
        let delta = if in_second { -sign } else { sign } * RESIZE_STEP;
        self.set_split_ratio(boundary, ratio + delta);
        self.split_ratio(boundary) != Some(ratio)
    }

    /// Give every pane of each split chain in `area` an even share.
    pub fn equalize_splits(&mut self, area: Area) {
        if let Some(layout) = self.tree_mut(area).layout.as_mut() {
            equalize(layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratios(layout: &WorkbenchLayout) -> Vec<f32> {
        match layout {
            WorkbenchLayout::Pane(_) => Vec::new(),
            WorkbenchLayout::Split {
                ratio,
                first,
                second,
                ..
            } => {
                let mut result = vec![*ratio];
                result.extend(ratios(first));
                result.extend(ratios(second));
                result
            }
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn grow_and_shrink_move_the_nearest_split_and_clamp() {
        let document = DocumentId::new(1);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let left = workbench.focused_pane_id().unwrap();
            let right = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let bottom = workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::After)
                .unwrap();
            assert_eq!(workbench.split_ratio(right), Some(0.5));

            // The bottom pane's nearest split is the vertical one.
            assert!(workbench.resize_pane(bottom, true, None));
            assert!(close(workbench.split_ratio(bottom).unwrap(), 0.45));
            assert_eq!(workbench.split_ratio(right), Some(0.5));
            // The left pane grows its horizontal split.
            assert!(workbench.resize_pane(left, true, None));
            assert!(close(workbench.split_ratio(right).unwrap(), 0.55));
            // The right pane's nearest split is the vertical one.
            assert!(workbench.resize_pane(right, true, None));
            assert!(close(workbench.split_ratio(bottom).unwrap(), 0.5));
            assert!(workbench.resize_pane(left, false, None));
            assert!(close(workbench.split_ratio(right).unwrap(), 0.5));

            for _ in 0..40 {
                workbench.resize_pane(left, true, None);
            }
            assert!(close(
                workbench.split_ratio(right).unwrap(),
                1. - WorkbenchLayout::MIN_RATIO
            ));
            assert!(!workbench.resize_pane(left, true, None));
            assert!(workbench.set_split_ratio(right, -3.));
            assert!(close(
                workbench.split_ratio(right).unwrap(),
                WorkbenchLayout::MIN_RATIO
            ));
            assert!(!workbench.set_split_ratio(left, 0.3));
        }
    }

    #[test]
    fn bounded_split_ratio_passes_point_nine_down_to_the_pixel_floor() {
        let document = DocumentId::new(5);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let _left = workbench.focused_pane_id().unwrap();
            let right = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();

            // The share-based setter keeps both minimums: the growing pane
            // stops at ninety percent.
            assert!(workbench.set_split_ratio(right, 0.97));
            assert!(close(
                workbench.split_ratio(right).unwrap(),
                1. - WorkbenchLayout::MIN_RATIO
            ));

            // A drag across 2000px floors the shrinking side at 120px, so
            // the growing pane passes 0.9 and stops at 0.94.
            let limit = 120. / 2000.;
            assert!(workbench.set_split_ratio_bounded(right, 0.97, limit, 1. - limit));
            assert!(close(workbench.split_ratio(right).unwrap(), 0.94));

            // Dragging back floors the other side at the same minimum.
            assert!(workbench.set_split_ratio_bounded(right, 0.01, limit, 1. - limit));
            assert!(close(workbench.split_ratio(right).unwrap(), limit));
        }
    }

    #[test]
    fn resizing_along_an_axis_skips_splits_of_the_other_one() {
        let document = DocumentId::new(2);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let left = workbench.focused_pane_id().unwrap();
            let right = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let bottom = workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::After)
                .unwrap();

            // The bottom pane widens through the outer horizontal split, past
            // its nearest vertical one.
            assert!(workbench.resize_pane(bottom, true, Some(SplitDirection::Horizontal)));
            assert!(close(workbench.split_ratio(right).unwrap(), 0.45));
            assert_eq!(workbench.split_ratio(bottom), Some(0.5));
            assert!(workbench.resize_pane(bottom, true, Some(SplitDirection::Vertical)));
            assert!(close(workbench.split_ratio(bottom).unwrap(), 0.45));
            // The left pane has no vertical split to change.
            assert!(!workbench.resize_pane(left, true, Some(SplitDirection::Vertical)));
        }
    }

    #[test]
    fn equalize_gives_each_pane_of_a_chain_an_even_share() {
        let document = DocumentId::new(3);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let first = workbench.focused_pane_id().unwrap();
            let second = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let third = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::After)
                .unwrap();
            workbench.set_split_ratio(second, 0.8);
            workbench.set_split_ratio(third, 0.2);
            workbench.equalize_splits(Area::Center);
            let layout = workbench.center().unwrap();
            // first | (second | (third / fourth)): thirds across, halves down.
            let found = ratios(layout);
            assert!(close(found[0], 1. / 3.), "{found:?}");
            assert!(close(found[1], 0.5), "{found:?}");
            assert!(close(found[2], 0.5), "{found:?}");
            assert!(workbench.pane(first).is_some());
        }
    }

    #[test]
    fn collapsing_a_pane_keeps_its_siblings_ratios() {
        let document = DocumentId::new(4);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let first = workbench.focused_pane_id().unwrap();
            let second = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let third = workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::After)
                .unwrap();
            workbench.set_split_ratio(third, 0.3);
            workbench.set_split_ratio(second, 0.7);
            let tab = workbench.pane(first).unwrap().active_tab_id();
            workbench.remove_tab(first, tab).unwrap();
            assert_eq!(workbench.split_ratio(third), Some(0.3));
            assert_eq!(ratios(workbench.center().unwrap()), vec![0.3]);
        }
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn neighbors_lie_on_the_side_and_overlap_most_along_the_other_axis() {
        // A left dock, a large pane split right into a top and bottom pair,
        // and a bottom dock across everything but the left dock.
        let [dock, main, top, bottom, panel] = [1, 2, 3, 4, 5].map(PaneId);
        let rects = [
            (dock, rect(0., 0., 200., 800.)),
            (main, rect(200., 0., 500., 600.)),
            (top, rect(700., 0., 300., 200.)),
            (bottom, rect(700., 200., 300., 400.)),
            (panel, rect(200., 600., 800., 200.)),
        ];
        assert_eq!(neighbor(&rects, main, SplitEdge::Right), Some(bottom));
        assert_eq!(neighbor(&rects, main, SplitEdge::Left), Some(dock));
        assert_eq!(neighbor(&rects, main, SplitEdge::Bottom), Some(panel));
        assert_eq!(neighbor(&rects, main, SplitEdge::Top), None);
        assert_eq!(neighbor(&rects, top, SplitEdge::Bottom), Some(bottom));
        assert_eq!(neighbor(&rects, top, SplitEdge::Left), Some(main));
        assert_eq!(neighbor(&rects, top, SplitEdge::Right), None);
        assert_eq!(neighbor(&rects, panel, SplitEdge::Top), Some(main));
        assert_eq!(neighbor(&rects, panel, SplitEdge::Left), Some(dock));
        assert_eq!(neighbor(&rects, dock, SplitEdge::Right), Some(main));
        assert_eq!(neighbor(&rects, PaneId(9), SplitEdge::Right), None);
    }

    #[test]
    fn pane_rects_lay_docks_around_the_center() {
        let workbench = &mut Workbench::empty();
        let (left, _) = workbench.open_document_in_center(DocumentId::new(5));
        let right = workbench
            .split_pane(left, SplitDirection::Horizontal, SplitPlacement::After)
            .unwrap();
        workbench.set_split_ratio(right, 0.25);
        let explorer = workbench.register_tool(
            crate::workbench::tools::LEFT_TOOL_ID,
            "Explorer",
            DockLocation::Left,
        );
        let (dock, _) = workbench.show_tool(explorer).unwrap();
        let other = workbench.register_tool("knot.other", "Other", DockLocation::Bottom);
        let (bottom, _) = workbench.show_tool(other).unwrap();
        workbench.set_dock_size(DockLocation::Left, 200.);
        workbench.set_dock_size(DockLocation::Bottom, 100.);
        let bounds = rect(0., 0., 1000., 500.);
        let rects = workbench.pane_rects(bounds, BottomDockLayout::Contained);
        assert_eq!(
            rects,
            [
                (left, rect(200., 0., 200., 400.)),
                (right, rect(400., 0., 600., 400.)),
                (dock, rect(0., 0., 200., 500.)),
                (bottom, rect(200., 400., 800., 100.)),
            ]
        );
        assert_eq!(neighbor(&rects, bottom, SplitEdge::Left), Some(dock));
        assert_eq!(neighbor(&rects, left, SplitEdge::Bottom), Some(bottom));
        // A full-width bottom dock runs under the side dock, which stops above it.
        let rects = workbench.pane_rects(bounds, BottomDockLayout::Full);
        assert_eq!(
            rects,
            [
                (left, rect(200., 0., 200., 400.)),
                (right, rect(400., 0., 600., 400.)),
                (dock, rect(0., 0., 200., 400.)),
                (bottom, rect(0., 400., 1000., 100.)),
            ]
        );
        assert_eq!(neighbor(&rects, bottom, SplitEdge::Left), None);
        assert_eq!(neighbor(&rects, dock, SplitEdge::Bottom), Some(bottom));
        // A hidden dock gives its room back and is never a neighbor.
        workbench.set_dock_visible(DockLocation::Left, false);
        let rects = workbench.pane_rects(bounds, BottomDockLayout::Contained);
        assert_eq!(rects[0], (left, rect(0., 0., 250., 400.)));
        assert_eq!(neighbor(&rects, left, SplitEdge::Left), None);
    }

    #[test]
    fn dock_panes_resize_their_dock_and_splits_across_it() {
        let workbench = &mut Workbench::empty();
        let (pane, _) = workbench.open_document_in_center(DocumentId::new(6));
        let first = workbench.register_tool(
            crate::workbench::tools::LEFT_TOOL_ID,
            "Explorer",
            DockLocation::Left,
        );
        let (dock, tab) = workbench.show_tool(first).unwrap();
        let second = workbench.register_tool("knot.other", "Other", DockLocation::Left);
        workbench.show_tool(second).unwrap();
        let source = workbench.capture_tab_move(dock, tab).unwrap();
        let lower = workbench
            .relocate_tab(
                source,
                TabMoveDestination::At {
                    anchor: source,
                    placement: TabMovePlacement::Split(SplitEdge::Bottom),
                },
            )
            .unwrap()
            .destination
            .pane_id;
        let before = workbench.dock_size(DockLocation::Left);
        assert!(workbench.resize_pane(lower, true, None));
        assert_eq!(workbench.dock_size(DockLocation::Left), before + 40.);
        assert!(workbench.resize_pane(lower, true, Some(SplitDirection::Vertical)));
        assert!(close(workbench.split_ratio(lower).unwrap(), 0.45));
        assert_eq!(workbench.dock_size(DockLocation::Left), before + 40.);
        // An unsplit center pane has nothing to resize.
        assert!(!workbench.resize_pane(pane, true, None));
        // Equalizing stays in one area.
        workbench.set_split_ratio(lower, 0.3);
        workbench.equalize_splits(Area::Center);
        assert_eq!(workbench.split_ratio(lower), Some(0.3));
        workbench.equalize_splits(Area::Dock(DockLocation::Left));
        assert_eq!(workbench.split_ratio(lower), Some(0.5));
    }
}
