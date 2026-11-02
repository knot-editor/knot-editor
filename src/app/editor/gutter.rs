//! The gutter beside the text: ordered native lanes for extension markers and
//! line numbers.

use super::*;
use crate::app::settings::LineNumbers;

/// Width of the lane that holds extension markers.
const MARKER_LANE_WIDTH: f32 = 16.;
/// Space between the line numbers and the text.
const NUMBER_LANE_PADDING: f32 = 12.;
/// The fewest digits the line-number lane is sized for.
const MIN_NUMBER_DIGITS: usize = 2;

/// Where each gutter lane sits, as x offsets from the view's left edge.
/// A view without a gutter has a zero-width layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct GutterLayout {
    pub(super) line_numbers: LineNumbers,
    /// The marker lane spans `0..markers_width`.
    pub(super) markers_width: f32,
    /// The right edge numbers align to; zero when numbers are off.
    pub(super) numbers_end: f32,
    pub(super) width: f32,
}

impl GutterLayout {
    /// The lanes for `line_count` lines, with digits `digit_width` wide.
    pub(super) fn new(line_numbers: LineNumbers, line_count: usize, digit_width: f32) -> Self {
        let numbers_end = match line_numbers {
            LineNumbers::Off => 0.,
            LineNumbers::On | LineNumbers::Relative => {
                let digits = line_count.max(1).ilog10() as usize + 1;
                MARKER_LANE_WIDTH + digits.max(MIN_NUMBER_DIGITS) as f32 * digit_width
            }
        };
        Self {
            line_numbers,
            markers_width: MARKER_LANE_WIDTH,
            numbers_end,
            width: if numbers_end > 0. {
                numbers_end + NUMBER_LANE_PADDING
            } else {
                MARKER_LANE_WIDTH
            },
        }
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
        let model = self.model.read(cx);
        let line_numbers = settings::editor_line_numbers(&self.settings_scope(model, cx), cx);
        let text = window.text_system();
        let digit_width = text
            .advance(
                text.resolve_font(&make_font(false, false)),
                px(FONT_SIZE),
                '0',
            )
            .map_or(FONT_SIZE * 0.6, |advance| f32::from(advance.width));
        self.gutter = GutterLayout::new(line_numbers, self.line_count, digit_width);
    }

    /// Each line in `lines` with the number the gutter shows beside it and
    /// whether it is the cursor's line.
    pub(super) fn gutter_numbers(&self, lines: Range<usize>) -> Vec<(usize, usize, bool)> {
        lines
            .filter_map(|line| {
                let label = self.gutter.label(line, self.cursor_line)?;
                Some((line, label, line == self.cursor_line))
            })
            .collect()
    }

    /// The line whose background is highlighted: the cursor's, while nothing
    /// is selected, as VS Code's default `renderLineHighlight` does.
    pub(super) fn highlighted_line(&self) -> Option<usize> {
        self.selection_range().is_none().then_some(self.cursor_line)
    }
}

#[cfg(test)]
mod tests {
    use super::{GutterLayout, LineNumbers, MARKER_LANE_WIDTH, NUMBER_LANE_PADDING};

    #[test]
    fn number_lane_fits_the_digits_of_the_line_count() {
        let width = |lines| GutterLayout::new(LineNumbers::On, lines, 10.).width;
        assert_eq!(width(1), width(99));
        assert_eq!(width(100) - width(99), 10.);
        assert_eq!(width(999), width(100));
        assert_eq!(width(1000) - width(999), 10.);
        assert_eq!(width(9), MARKER_LANE_WIDTH + 20. + NUMBER_LANE_PADDING);
        assert_eq!(
            GutterLayout::new(LineNumbers::Relative, 100, 10.).width,
            width(100)
        );
        assert_eq!(
            GutterLayout::new(LineNumbers::Off, 100, 10.).width,
            MARKER_LANE_WIDTH
        );
    }

    #[test]
    fn labels_follow_the_line_number_mode() {
        let labels = |mode| {
            let layout = GutterLayout::new(mode, 5, 10.);
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
