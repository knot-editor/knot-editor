//! HTTP(S) destinations in the displayed terminal frame.

use std::sync::Mutex;

use knot_terminal::{Cell, CellWidth, Point, RenderState, Terminal};

use super::session::lock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkTarget {
    pub(crate) uri: String,
    explicit: bool,
    pub(crate) cells: Vec<(usize, usize)>,
}

impl LinkTarget {
    pub(crate) fn contains(&self, column: usize, row: usize) -> bool {
        self.cells.contains(&(column, row))
    }
}

/// What link detection reads from one displayed cell.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkCell {
    text: String,
    width: CellWidth,
    hidden: bool,
    /// Whether the cell has an OSC 8 hyperlink, and its URI once resolved.
    hyperlink: Option<Option<String>>,
}

/// The displayed cells, collected row by row from a frame.
#[derive(Default)]
pub(crate) struct LinkGridBuilder {
    cells: Vec<LinkCell>,
    wrapped: Vec<bool>,
}

impl LinkGridBuilder {
    pub(crate) fn push(&mut self, cell: &Cell) {
        self.cells.push(LinkCell {
            text: cell.text.to_owned(),
            width: cell.width,
            hidden: cell.style.invisible,
            hyperlink: cell.hyperlink.then_some(None),
        });
    }

    pub(crate) fn end_row(&mut self, wrapped: bool) {
        self.wrapped.push(wrapped);
    }

    /// The grid, reading the URIs of hyperlinked cells from `terminal`,
    /// which shows the same frame.
    pub(crate) fn finish(mut self, terminal: &Terminal) -> LinkGrid {
        let rows = self.wrapped.len();
        let columns = self.cells.len().checked_div(rows).unwrap_or(0);
        for (index, cell) in self.cells.iter_mut().enumerate() {
            if let Some(uri) = &mut cell.hyperlink {
                *uri = terminal.hyperlink(Point::new(
                    (index % columns) as u16,
                    (index / columns) as u16,
                ));
            }
        }
        let mut grid = LinkGrid {
            columns,
            rows,
            cells: self.cells,
            wrapped: self.wrapped,
            links: Vec::new(),
        };
        grid.find_links();
        grid
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkGrid {
    columns: usize,
    rows: usize,
    cells: Vec<LinkCell>,
    wrapped: Vec<bool>,
    links: Vec<LinkTarget>,
}

pub(crate) fn http_destination(uri: &str) -> Option<String> {
    if uri.chars().any(char::is_control) {
        return None;
    }
    let url = url::Url::parse(uri).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(url.to_string())
}

impl LinkGrid {
    /// The links of the frame `terminal` displays now, read through `state`.
    pub(crate) fn capture(state: &mut RenderState, terminal: &Mutex<Terminal>) -> Self {
        state.update(terminal);
        let mut builder = LinkGridBuilder::default();
        let mut rows = state.rows();
        while let Some(mut row) = rows.next() {
            let wrapped = row.wrapped();
            let mut cells = row.cells();
            while let Some(cell) = cells.next() {
                builder.push(&cell);
            }
            builder.end_row(wrapped);
        }
        builder.finish(&lock(terminal))
    }

    pub(crate) fn links_contain(&self, target: &LinkTarget) -> bool {
        self.links.contains(target)
    }

    pub(crate) fn at(&self, column: usize, row: usize) -> Option<&LinkTarget> {
        self.links.iter().find(|link| link.contains(column, row))
    }

    fn find_links(&mut self) {
        for (index, cell) in self.cells.iter().enumerate() {
            let Some(Some(uri)) = &cell.hyperlink else {
                continue;
            };
            let Some(uri) = http_destination(uri) else {
                continue;
            };
            let coordinate = (index % self.columns, index / self.columns);
            if let Some(target) = self
                .links
                .iter_mut()
                .find(|target| target.explicit && target.uri == uri)
            {
                target.cells.push(coordinate);
            } else {
                self.links.push(LinkTarget {
                    uri,
                    explicit: true,
                    cells: vec![coordinate],
                });
            }
        }
        let mut text = String::new();
        let mut coordinates = Vec::new();
        for row in 0..self.rows {
            for column in 0..self.columns {
                let cell = &self.cells[row * self.columns + column];
                if matches!(cell.width, CellWidth::SpacerTail | CellWidth::SpacerHead) {
                    continue;
                }
                let start = text.len();
                // Explicit destinations, including unsupported schemes, are authoritative.
                if cell.hyperlink.is_some() || cell.hidden || cell.text.is_empty() {
                    text.push(' ');
                } else {
                    text.push_str(&cell.text);
                }
                coordinates.push((start..text.len(), (column, row)));
                if cell.width == CellWidth::Wide && column + 1 < self.columns {
                    coordinates.push((start..text.len(), (column + 1, row)));
                }
            }
            let wrapped = self.wrapped[row];
            if !wrapped || row + 1 == self.rows {
                self.detect(&text, &coordinates, wrapped);
                text.clear();
                coordinates.clear();
            }
        }
    }

    fn detect(
        &mut self,
        text: &str,
        coordinates: &[(std::ops::Range<usize>, (usize, usize))],
        clipped: bool,
    ) {
        let bytes = text.as_bytes();
        for (start, _) in text.char_indices() {
            let tail = &text[start..];
            if !(tail
                .get(..7)
                .is_some_and(|s| s.eq_ignore_ascii_case("http://"))
                || tail
                    .get(..8)
                    .is_some_and(|s| s.eq_ignore_ascii_case("https://")))
            {
                continue;
            }
            if start > 0
                && text[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let length = tail
                .find(|c: char| {
                    c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '`')
                })
                .unwrap_or(tail.len());
            let mut end = start + length;
            if clipped && end == text.len() {
                continue;
            }
            loop {
                if end <= start {
                    break;
                }
                let last = bytes[end - 1] as char;
                let unmatched = match last {
                    ')' => Some(('(', ')')),
                    ']' => Some(('[', ']')),
                    '}' => Some(('{', '}')),
                    _ => None,
                }
                .is_some_and(|(open, close)| {
                    text[start..end].matches(close).count() > text[start..end].matches(open).count()
                });
                if ".,;:!?".contains(last) || unmatched {
                    end -= 1;
                } else {
                    break;
                }
            }
            let Some(uri) = http_destination(&text[start..end]) else {
                continue;
            };
            let cells = coordinates
                .iter()
                .filter(|(range, _)| range.start < end && range.end > start)
                .map(|(_, point)| *point)
                .collect();
            self.links.push(LinkTarget {
                uri,
                explicit: false,
                cells,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_terminal::{Scroll, TerminalSize};

    fn terminal(columns: u16, rows: u16, bytes: &[u8]) -> Mutex<Terminal> {
        let size = TerminalSize {
            columns,
            rows,
            cell_width: 8,
            cell_height: 16,
        };
        let mut terminal = Terminal::new(size, 100);
        terminal.write(bytes);
        Mutex::new(terminal)
    }

    fn capture(terminal: &Mutex<Terminal>) -> LinkGrid {
        LinkGrid::capture(&mut RenderState::new(), terminal)
    }

    fn grid(columns: u16, rows: u16, bytes: &[u8]) -> LinkGrid {
        capture(&terminal(columns, rows, bytes))
    }

    #[test]
    fn explicit_destination_overrides_visible_label() {
        let grid = grid(
            80,
            3,
            b"\x1b]8;id=one;https://destination.test/path\x1b\\https://label.test\x1b]8;;\x1b\\",
        );
        let link = grid.at(0, 0).unwrap();
        assert_eq!(link.uri, "https://destination.test/path");
        assert!(link.contains(17, 0));
        assert_eq!(grid.links.len(), 1);
    }

    #[test]
    fn unsupported_explicit_scheme_blocks_detection_and_controls_are_rejected() {
        let grid = grid(
            80,
            3,
            b"\x1b]8;;file:///tmp/a\x1b\\https://label.test\x1b]8;;\x1b\\",
        );
        assert!(grid.links.is_empty());
        for uri in [
            "file:///tmp/a",
            "javascript:alert(1)",
            "https://test/\nfoo",
            "https://",
        ] {
            assert!(http_destination(uri).is_none(), "{uri:?}");
        }
    }

    #[test]
    fn detects_soft_wraps_but_keeps_hard_lines_separate() {
        let wrapped = grid(12, 4, b"https://example.test/path");
        assert_eq!(wrapped.at(0, 1).unwrap().uri, "https://example.test/path");
        let hard = grid(40, 3, b"https://example.test/\r\nother");
        assert_eq!(hard.at(0, 0).unwrap().uri, "https://example.test/");
        assert!(hard.at(0, 1).is_none());
    }

    #[test]
    fn maps_wide_and_combining_unicode_cells() {
        let grid = grid(16, 4, "界 https://test/界e\u{301}".as_bytes());
        let link = grid.at(3, 0).unwrap();
        assert_eq!(link.uri, "https://test/%E7%95%8Ce%CC%81");
        assert!(link.contains(0, 1));
        assert!(link.contains(1, 1));
        assert!(grid.at(0, 0).is_none());
    }

    #[test]
    fn trims_sentence_punctuation_and_unbalanced_delimiters() {
        let grid = grid(
            100,
            3,
            b"(https://test/a(b)). https://[::1]/x], HTTPS://test/end! wordhttps://ignored.test",
        );
        assert_eq!(
            grid.links
                .iter()
                .map(|link| link.uri.as_str())
                .collect::<Vec<_>>(),
            ["https://test/a(b)", "https://[::1]/x", "https://test/end"]
        );
    }

    #[test]
    fn refuses_detected_destination_clipped_below_viewport() {
        let grid = grid(12, 2, b"https://example.test/long/path");
        // Scrolling removed the prefix; no partial destination is actionable.
        assert!(grid.links.is_empty());
    }

    #[test]
    fn scrolled_view_rejects_cut_off_detected_url_but_keeps_explicit_destination() {
        let detected = terminal(12, 2, b"https://example.test/long/path/continued");
        lock(&detected).scroll_viewport(Scroll::Top);
        assert!(capture(&detected).links.is_empty());
        let explicit = terminal(
            12,
            2,
            b"\x1b]8;;https://example.test/full\x1b\\long visible label continuing below viewport",
        );
        lock(&explicit).scroll_viewport(Scroll::Top);
        assert_eq!(
            capture(&explicit).at(0, 0).unwrap().uri,
            "https://example.test/full"
        );
    }

    #[test]
    fn displayed_scrollback_links_are_found() {
        let terminal = terminal(40, 2, b"https://history.test\r\nnext\r\nlast");
        assert!(capture(&terminal).links.is_empty());
        lock(&terminal).scroll_viewport(Scroll::Top);
        assert_eq!(
            capture(&terminal).at(0, 0).unwrap().uri,
            "https://history.test/"
        );
    }

    #[test]
    fn changed_destinations_change_the_grid() {
        assert_ne!(
            grid(40, 2, b"\x1b]8;;https://a.test\x1b\\label"),
            grid(40, 2, b"\x1b]8;;https://b.test\x1b\\label")
        );
    }
}
