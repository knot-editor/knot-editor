//! The gutter beside the text: ordered native lanes for extension markers and
//! line numbers.

use super::input::unit_range;
use super::*;
use crate::app::settings::{Folding, LineNumbers};

/// Width of the lane that holds extension markers.
const MARKER_LANE_WIDTH: f32 = 16.;
/// Width of the lane that holds fold chevrons.
const FOLD_LANE_WIDTH: f32 = 18.;
/// Space between the line numbers and the text.
const NUMBER_LANE_PADDING: f32 = 12.;
/// The fewest digits the line-number lane is sized for.
const MIN_NUMBER_DIGITS: usize = 2;

/// A gutter lane, which handles its own clicks and pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GutterLane {
    Markers,
    /// The line numbers and the padding between them and the text.
    Numbers,
    /// The fold chevrons.
    Folds,
}

/// Where each gutter lane sits, as x offsets from the view's left edge.
/// A view without a gutter has a zero-width layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct GutterLayout {
    pub(super) line_numbers: LineNumbers,
    pub(super) folding: Folding,
    /// The marker lane spans `0..markers_width`.
    pub(super) markers_width: f32,
    /// The right edge numbers align to; zero when numbers are off.
    pub(super) numbers_end: f32,
    /// The fold lane spans `numbers_end.max(markers_width)..width`; it is
    /// empty when folding is off.
    pub(super) folds_start: f32,
    pub(super) width: f32,
}

impl GutterLayout {
    /// The lanes for `line_count` lines, with digits `digit_width` wide and
    /// fixed widths multiplied by `scale`, the text's size relative to the
    /// default.
    pub(super) fn new(
        line_numbers: LineNumbers,
        folding: Folding,
        line_count: usize,
        digit_width: f32,
        scale: f32,
    ) -> Self {
        let markers_width = MARKER_LANE_WIDTH * scale;
        let numbers_end = match line_numbers {
            LineNumbers::Off => 0.,
            LineNumbers::On | LineNumbers::Relative => {
                let digits = line_count.max(1).ilog10() as usize + 1;
                markers_width + digits.max(MIN_NUMBER_DIGITS) as f32 * digit_width
            }
        };
        let folds_start = numbers_end.max(markers_width);
        let width = match (folding, numbers_end > 0.) {
            (Folding::Off, true) => numbers_end + NUMBER_LANE_PADDING * scale,
            (Folding::Off, false) => markers_width,
            _ => folds_start + FOLD_LANE_WIDTH * scale,
        };
        Self {
            line_numbers,
            folding,
            markers_width,
            numbers_end,
            folds_start,
            width,
        }
    }

    /// The lane `x` pixels right of the view's left edge falls in.
    pub(super) fn lane_at(&self, x: f32) -> Option<GutterLane> {
        if !(0. ..self.width).contains(&x) {
            None
        } else if x < self.markers_width {
            Some(GutterLane::Markers)
        } else if self.folding != Folding::Off && x >= self.folds_start {
            Some(GutterLane::Folds)
        } else {
            Some(GutterLane::Numbers)
        }
    }

    /// The x of the fold lane's middle.
    pub(super) fn folds_center(&self) -> f32 {
        (self.folds_start + self.width) / 2.
    }

    /// The number shown beside `line`, with the cursor on `cursor_line`.
    pub(super) fn label(&self, line: usize, cursor_line: usize) -> Option<usize> {
        match self.line_numbers {
            LineNumbers::Off => None,
            LineNumbers::Relative if line != cursor_line => Some(line.abs_diff(cursor_line)),
            LineNumbers::On | LineNumbers::Relative => Some(line + 1),
        }
    }
}

impl EditorView {
    /// Lay the gutter out for the current settings, line count, and font.
    pub(super) fn update_gutter_layout(&mut self, window: &Window, cx: &App) {
        if !self.rendering.show_gutter {
            self.gutter = GutterLayout::default();
            return;
        }
        let editor = self.editor.read(cx);
        let scope = editor.settings_scope(self.model.read(cx), cx);
        let line_numbers = settings::editor_line_numbers(&scope, cx);
        let folding = settings::editor_folding(&scope, cx);
        let text = window.text_system();
        let font_size = self.metrics.font_size;
        let digit_width = text
            .advance(
                text.resolve_font(&make_font(false, false)),
                px(font_size),
                '0',
            )
            .map_or(font_size * 0.6, |advance| f32::from(advance.width));
        self.gutter = GutterLayout::new(
            line_numbers,
            folding,
            editor.line_count,
            digit_width,
            self.metrics.scale(),
        );
    }

    /// The gutter lane under a window position.
    pub(super) fn gutter_lane_at(&self, position: Point<Pixels>) -> Option<GutterLane> {
        self.gutter
            .lane_at(f32::from(position.x - self.bounds.origin.x))
    }

    /// The first row of each line that starts in `rows`, with the number the
    /// gutter shows beside it and whether it is the cursor's line. A wrapped
    /// line shows its number on its first row only.
    pub(super) fn gutter_numbers(
        &self,
        editor: &Editor,
        rows: Range<usize>,
    ) -> Vec<(usize, usize, bool)> {
        if rows.is_empty() {
            return Vec::new();
        }
        let wrap = &editor.wrap;
        let lines = wrap.position(rows.start).0..wrap.position(rows.end - 1).0 + 1;
        lines
            .filter(|line| !wrap.is_hidden(*line))
            .filter_map(|line| {
                let row = wrap.first_row(line);
                let label = self.gutter.label(line, editor.cursor_line)?;
                rows.contains(&row)
                    .then_some((row, label, line == editor.cursor_line))
            })
            .collect()
    }
}

impl Editor {
    /// The marker shown on `line`, if any.
    pub(super) fn gutter_marker(&self, line: usize) -> Option<&RenderedGutterMarker> {
        self.gutter_markers
            .iter()
            .find(|marker| marker.line == line)
    }

    /// Select whole lines from a click on the line numbers: the clicked line,
    /// or with `extend`, from the selection's anchor through the clicked
    /// line. Returns what a drag then extends from by lines.
    pub(super) fn select_lines_from_gutter(
        &mut self,
        line: usize,
        extend: bool,
        cx: &mut Context<Self>,
    ) -> DragAnchor {
        let (start, end) = unit_range(self.model.read(cx), SelectionUnit::Line, line, 0);
        let (fixed_start, fixed_end) = if extend {
            let fixed = if self.has_selection {
                (self.anchor_line, self.anchor_col)
            } else {
                (self.cursor_line, self.cursor_col)
            };
            (fixed, fixed)
        } else {
            (start, end)
        };
        let (anchor, caret) = if start < fixed_start {
            (fixed_end, start)
        } else {
            (fixed_start, end.max(fixed_end))
        };
        self.select_from(anchor, caret, cx);
        DragAnchor {
            unit: SelectionUnit::Line,
            start: fixed_start,
            end: fixed_end,
        }
    }

    /// The line whose background is highlighted: the cursor's, while nothing
    /// is selected, as VS Code's default `renderLineHighlight` does.
    pub(super) fn highlighted_line(&self) -> Option<usize> {
        self.selection_range().is_none().then_some(self.cursor_line)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FOLD_LANE_WIDTH, Folding, GutterLane, GutterLayout, LineNumbers, MARKER_LANE_WIDTH,
        NUMBER_LANE_PADDING,
    };

    #[test]
    fn number_lane_fits_the_digits_of_the_line_count() {
        let width = |lines| GutterLayout::new(LineNumbers::On, Folding::Off, lines, 10., 1.).width;
        assert_eq!(width(1), width(99));
        assert_eq!(width(100) - width(99), 10.);
        assert_eq!(width(999), width(100));
        assert_eq!(width(1000) - width(999), 10.);
        assert_eq!(width(9), MARKER_LANE_WIDTH + 20. + NUMBER_LANE_PADDING);
        assert_eq!(
            GutterLayout::new(LineNumbers::Relative, Folding::Off, 100, 10., 1.).width,
            width(100)
        );
        assert_eq!(
            GutterLayout::new(LineNumbers::Off, Folding::Off, 100, 10., 1.).width,
            MARKER_LANE_WIDTH
        );
        let larger = GutterLayout::new(LineNumbers::On, Folding::Off, 9, 15., 1.5);
        assert_eq!(larger.markers_width, MARKER_LANE_WIDTH * 1.5);
        assert_eq!(
            larger.width,
            (MARKER_LANE_WIDTH + NUMBER_LANE_PADDING) * 1.5 + 30.
        );
    }

    #[test]
    fn fold_lane_follows_the_numbers() {
        let on = GutterLayout::new(LineNumbers::On, Folding::Mouseover, 9, 10., 1.);
        assert_eq!(on.folds_start, on.numbers_end);
        assert_eq!(on.width, on.numbers_end + FOLD_LANE_WIDTH);
        assert_eq!(on.lane_at(on.numbers_end - 1.), Some(GutterLane::Numbers));
        assert_eq!(on.lane_at(on.numbers_end), Some(GutterLane::Folds));
        assert_eq!(on.lane_at(on.width), None);
        let bare = GutterLayout::new(LineNumbers::Off, Folding::Always, 9, 10., 1.);
        assert_eq!(bare.folds_start, MARKER_LANE_WIDTH);
        assert_eq!(bare.width, MARKER_LANE_WIDTH + FOLD_LANE_WIDTH);
        assert_eq!(bare.lane_at(MARKER_LANE_WIDTH), Some(GutterLane::Folds));
    }

    #[test]
    fn labels_follow_the_line_number_mode() {
        let labels = |mode| {
            let layout = GutterLayout::new(mode, Folding::Off, 5, 10., 1.);
            (0..5).map(|line| layout.label(line, 2)).collect::<Vec<_>>()
        };
        assert_eq!(
            labels(LineNumbers::On),
            [Some(1), Some(2), Some(3), Some(4), Some(5)]
        );
        assert_eq!(
            labels(LineNumbers::Relative),
            [Some(2), Some(1), Some(3), Some(1), Some(2)]
        );
        assert_eq!(labels(LineNumbers::Off), [None; 5]);
    }
}
