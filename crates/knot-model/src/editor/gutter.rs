//! Gutter interaction: markers and line-number selection.

use super::*;

impl EditorState {
    /// The marker shown on `line`, if any.
    pub fn gutter_marker(&self, line: usize) -> Option<&RenderedGutterMarker> {
        self.gutter_markers
            .iter()
            .find(|marker| marker.line == line)
    }

    /// Select whole lines from a click on the line numbers: the clicked line,
    /// or with `extend`, from the selection's anchor through the clicked
    /// line. Returns what a drag then extends from by lines.
    pub fn select_lines_from_gutter(
        &mut self,
        line: usize,
        extend: bool,
        buffer: &mut BufferModel,
    ) -> DragAnchor {
        let (start, end) = unit_range(&*buffer, SelectionUnit::Line, line, 0);
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
        self.select_from(anchor, caret, buffer);
        DragAnchor {
            unit: SelectionUnit::Line,
            start: fixed_start,
            end: fixed_end,
        }
    }

    /// The line whose background is highlighted: the cursor's, while nothing
    /// is selected, as VS Code's default `renderLineHighlight` does.
    pub fn highlighted_line(&self) -> Option<usize> {
        self.selection_range().is_none().then_some(self.cursor_line)
    }
}
