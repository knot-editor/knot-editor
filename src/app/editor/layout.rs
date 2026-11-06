//! Row shaping and the geometry shared by caret, selection, hit testing,
//! and painting.

use super::wrap::WrapWidth;
use super::*;

/// Pixels a wrapped row leaves free at the right edge, for the caret and the
/// scrollbar.
const WRAP_MARGIN: f32 = 8.;

/// One visual row: the bytes of a buffer line that it shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub(super) index: usize,
    pub(super) line: usize,
    /// Bytes of the line's content.
    pub(super) range: Range<usize>,
    /// Whether the row ends its line, so it also shows the line's end.
    pub(super) last: bool,
    /// How many cells right of the text area's edge the row's text starts.
    pub(super) indent: usize,
}

impl Row {
    /// Whether the caret at byte `col` of `line` shows on this row.
    pub(super) fn holds(&self, (line, col): (usize, usize)) -> bool {
        self.line == line && self.range.start <= col && (col < self.range.end || self.last)
    }

    /// The first position after this row's positions.
    fn after(&self) -> (usize, usize) {
        if self.last {
            (self.line + 1, 0)
        } else {
            (self.line, self.range.end)
        }
    }

    /// The part of `start..end` that this row shows: whether it begins and
    /// ends here, or `None` when the row shows none of it.
    pub(super) fn span(
        &self,
        start: (usize, usize),
        end: (usize, usize),
    ) -> Option<(Option<usize>, Option<usize>)> {
        let first = (self.line, self.range.start);
        (start < self.after() && end > first).then(|| {
            (
                (start >= first).then_some(start.1),
                (end < self.after()).then_some(end.1),
            )
        })
    }
}

/// A background one decoration paints on one row.
#[derive(Debug, PartialEq)]
pub(super) struct DecorationFill<'a> {
    /// The row's position among the rows painted.
    pub(super) row: usize,
    /// The byte columns of the row's line it covers, or `None` for a band
    /// across the whole text area.
    pub(super) columns: Option<Range<usize>>,
    pub(super) key: &'a str,
}

/// The backgrounds `decorations` paint on `rows`, in paint order: `line`
/// bands, then `background` spans that end at the text on each row, each
/// group with the most severe keys last so they land on top.
pub(super) fn decoration_fills<'a, 'r>(
    decorations: &'a [RenderedDecoration],
    rows: impl IntoIterator<Item = &'r Row>,
) -> Vec<DecorationFill<'a>> {
    let rows: Vec<&Row> = rows.into_iter().collect();
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return Vec::new();
    };
    let lines = first.line..=last.line;
    let mut fills = Vec::new();
    for style in [DecorationStyle::Line, DecorationStyle::Background] {
        let mut painted: Vec<&RenderedDecoration> = decorations
            .iter()
            .filter(|decoration| decoration.style == style && lines.contains(&decoration.line))
            .collect();
        painted.sort_by_key(|decoration| decoration_precedence(&decoration.key));
        for decoration in painted {
            for (position, row) in rows.iter().enumerate() {
                if row.line != decoration.line {
                    continue;
                }
                let columns = if style == DecorationStyle::Line {
                    None
                } else {
                    let start = (decoration.line, decoration.start);
                    let end = (decoration.line, decoration.end);
                    let Some((begins, ends)) = row.span(start, end) else {
                        continue;
                    };
                    Some(begins.unwrap_or(row.range.start)..ends.unwrap_or(row.range.end))
                        .filter(|columns| !columns.is_empty())
                };
                if style == DecorationStyle::Line || columns.is_some() {
                    fills.push(DecorationFill {
                        row: position,
                        columns,
                        key: &decoration.key,
                    });
                }
            }
        }
    }
    fills
}

/// The text size and line height one view lays out with, from the
/// `font.size` setting in its scope and the application zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditorMetrics {
    pub(crate) font_size: f32,
    /// Whole pixels, so rows never land between device pixels.
    pub(crate) line_height: f32,
}

impl EditorMetrics {
    pub(super) fn new(font_size: f32) -> Self {
        Self {
            font_size,
            line_height: (font_size * 20. / 14.).round().max(1.),
        }
    }

    /// The size relative to the default text, for chrome drawn beside it.
    pub(super) fn scale(self) -> f32 {
        self.font_size / crate::app::typography::TEXT_SIZE
    }
}

impl Editor {
    /// Wrap lines to `wrap`, or stop wrapping with `None`, keeping the top
    /// row's text at the top when the rows change.
    pub(super) fn set_wrap(&mut self, wrap: Option<WrapWidth>, cx: &App) {
        let restored = self.restored_top.take();
        if wrap == self.wrap.width() {
            return;
        }
        let model = self.model.read(cx);
        self.sync_wrap(model);
        let top = restored.unwrap_or_else(|| self.top_offset(model));
        self.wrap
            .set_width(wrap, model.line_count(), |line| line_text(model, line));
        let (line, col) = flat_byte_to_position(model, top);
        self.scroll_top = self.wrap.row_of(line, col) as f32;
        if wrap.is_some() {
            self.scroll_x = 0.;
        }
    }

    /// Lay out as a view `rows` rows tall and `columns` cells wide would,
    /// wrapping when `editor.softWrap` is on, for a test without a window.
    #[cfg(test)]
    pub(crate) fn lay_out_for_test(&mut self, rows: f32, columns: usize, cx: &App) {
        let scope = self.settings_scope(self.model.read(cx), cx);
        let wrap = settings::editor_soft_wrap(&scope, cx).then(|| WrapWidth {
            columns,
            tab_size: settings::editor_tab_size(&scope, cx),
        });
        self.set_wrap(wrap, cx);
        self.lay_out(rows, true);
    }

    /// Visual row `index`, clamped to the last row.
    pub(super) fn row(&self, model: &BufferModel, index: usize) -> Row {
        let (line, sub) = self.wrap.position(index);
        let rows = self.wrap.rows(line);
        Row {
            index: rows.start + sub,
            line,
            range: self.wrap.row_range(line, sub, line_end(model, line)),
            last: rows.start + sub + 1 == rows.end,
            indent: self.wrap.indent(line, sub),
        }
    }

    /// The settings context of this editor's view kind and its buffer's
    /// language.
    pub(super) fn settings_scope(&self, model: &BufferModel, cx: &App) -> SettingsScope {
        SettingsScope {
            language: Some(languages::effective_language(model, cx)),
            view: Some(self.view_kind.to_string()),
            document: model.document().map(|document| document.to_string()),
        }
    }

    /// Columns per tab stop for this editor's view kind and its buffer's
    /// language.
    pub(crate) fn tab_size(&self, model: &BufferModel, cx: &App) -> usize {
        settings::editor_tab_size(&self.settings_scope(model, cx), cx)
    }

    /// One indentation level from the `editor.insertSpaces` and
    /// `editor.tabSize` settings in this editor's scope.
    pub(super) fn indent_unit(&self, model: &BufferModel, cx: &App) -> IndentUnit {
        let scope = self.settings_scope(model, cx);
        IndentUnit {
            tabs: !settings::editor_insert_spaces(&scope, cx),
            size: settings::editor_tab_size(&scope, cx),
        }
    }

    /// The display cell the caret shows at: the caret's row's indentation
    /// plus the cells of the row's text before it, with tabs expanded.
    pub(super) fn caret_column(&self, model: &BufferModel, cx: &App) -> usize {
        let row = self.row(model, self.caret_row());
        let text = line_text(model, row.line);
        let col = self.cursor_col.clamp(row.range.start, row.range.end);
        row.indent + wrap::cells_before(&text[row.range.start..col], self.tab_size(model, cx))
    }

    /// The caret position on row `index` nearest display cell `column`. A
    /// row that does not end its line leaves its end to the next row.
    pub(super) fn position_at_column(
        &self,
        model: &BufferModel,
        index: usize,
        column: usize,
        cx: &App,
    ) -> (usize, usize) {
        let row = self.row(model, index);
        let text = line_text(model, row.line);
        let col = wrap::index_at_cells(
            &text[row.range.clone()],
            column.saturating_sub(row.indent),
            self.tab_size(model, cx),
            row.last,
        );
        (row.line, row.range.start + col)
    }
}

impl EditorView {
    #[cfg(test)]
    pub(crate) fn metrics(&self) -> EditorMetrics {
        self.metrics
    }

    /// Resolve the metrics for the current settings and zoom. Scrolling is
    /// in rows, so the same line stays at the top when the line height
    /// changes.
    pub(super) fn update_metrics(&mut self, cx: &App) {
        let editor = self.editor.read(cx);
        let font_size = settings::font_size(&editor.settings_scope(self.model.read(cx), cx), cx);
        let metrics = EditorMetrics::new(font_size * crate::app::zoom::factor(cx));
        if metrics != self.metrics {
            self.metrics = metrics;
            self.max_line_width = 0.;
        }
    }

    /// Wrap the editor's lines to the text area's `width` when
    /// `editor.softWrap` is on.
    pub(super) fn update_wrap(&mut self, width: Pixels, window: &Window, cx: &mut App) {
        let editor = self.editor.read(cx);
        let scope = editor.settings_scope(self.model.read(cx), cx);
        let font_size = self.metrics.font_size;
        let text = window.text_system();
        self.cell_width = text
            .advance(
                text.resolve_font(&make_font(false, false)),
                px(font_size),
                '0',
            )
            .map_or(font_size * 0.6, |advance| f32::from(advance.width));
        let wrap = settings::editor_soft_wrap(&scope, cx).then(|| WrapWidth {
            columns: ((f32::from(width) - WRAP_MARGIN) / self.cell_width).max(1.) as usize,
            tab_size: settings::editor_tab_size(&scope, cx),
        });
        self.editor
            .update(cx, |editor, cx| editor.set_wrap(wrap, cx));
    }

    /// How far right of the text area's edge `row`'s text starts.
    pub(super) fn row_indent(&self, row: &Row) -> Pixels {
        px(row.indent as f32 * self.cell_width)
    }

    /// Shape `row`, whose line's content is `text`, with its highlight runs.
    pub(super) fn shape_row(
        &self,
        editor: &Editor,
        row: &Row,
        text: &str,
        window: &Window,
        cx: &App,
    ) -> ShapedLine {
        let runs = slice_runs(
            &self.line_runs(editor, row.line, text, cx),
            row.range.clone(),
        );
        shape_editor_line(
            window,
            text[row.range.clone()].to_owned().into(),
            px(self.metrics.font_size),
            &runs,
            editor.tab_size(self.model.read(cx), cx),
        )
    }

    /// Where `row`'s text starts in the text area, after horizontal scroll,
    /// for row text `text` shaped `shaped_w` wide.
    pub(super) fn row_x(&self, editor: &Editor, row: &Row, text: &str, shaped_w: Pixels) -> Pixels {
        self.row_indent(row)
            + self.line_x_offset(editor, text, shaped_w, self.text_bounds().size.width)
    }

    /// The x of byte `col` of `row`'s line within the text area, after
    /// horizontal scroll.
    pub(super) fn x_in_row(
        &self,
        editor: &Editor,
        row: &Row,
        col: usize,
        window: &Window,
        cx: &App,
    ) -> Pixels {
        let model = self.model.read(cx);
        let text = line_text(model, row.line);
        let shaped = self.shape_row(editor, row, &text, window, cx);
        let row_text = &text[row.range.clone()];
        let col = col.clamp(row.range.start, row.range.end) - row.range.start;
        x_for_index_dir(&shaped, col, row_text, editor.tab_size(model, cx))
            + self.row_x(editor, row, row_text, shaped.width)
    }

    /// Text runs for `line`, whose content is `text`, from the view's
    /// highlight layers: syntax, then decorations.
    pub(super) fn line_runs(
        &self,
        editor: &Editor,
        line: usize,
        text: &str,
        cx: &App,
    ) -> Vec<TextRun> {
        let theme = theme(cx);
        let syntax = self.syntax_layer(editor, self.model.read(cx), line, theme);
        let decorations = decoration_layers(editor, line, theme);
        let mut layers = vec![syntax.as_slice()];
        layers.extend(decorations.iter().map(Vec::as_slice));
        runs_for(text, &layers, theme)
    }

    /// Shape `line`, whose content is `text`, with its highlight runs.
    #[cfg(test)]
    pub(super) fn shape_line(
        &self,
        line: usize,
        text: &str,
        window: &Window,
        cx: &App,
    ) -> ShapedLine {
        let editor = self.editor.read(cx);
        let runs = self.line_runs(editor, line, text, cx);
        shape_editor_line(
            window,
            text.to_owned().into(),
            px(self.metrics.font_size),
            &runs,
            editor.tab_size(self.model.read(cx), cx),
        )
    }

    /// Query the syntax highlights of `lines` at once, so painting and the
    /// geometry of those lines share one query per syntax generation. A new
    /// query also covers `SYNTAX_OVERSCAN` lines on each side, so small
    /// scrolls reuse it.
    pub(super) fn cache_syntax(&self, model: &BufferModel, lines: Range<usize>) {
        let Some(generation) = model.syntax_generation() else {
            return;
        };
        let lines = lines.start..lines.end.min(model.line_count());
        if lines.is_empty() {
            return;
        }
        let byte_range = |lines: Range<usize>| {
            model.line_range(lines.start).start..model.line_range(lines.end - 1).end
        };
        let mut cache = self.syntax_cache.borrow_mut();
        if cache
            .as_ref()
            .is_some_and(|cache| cache.covers(generation, &byte_range(lines.clone())))
        {
            return;
        }
        let range = byte_range(
            lines.start.saturating_sub(SYNTAX_OVERSCAN)
                ..(lines.end + SYNTAX_OVERSCAN).min(model.line_count()),
        );
        *cache = Some(SyntaxCache {
            generation,
            spans: model.syntax_highlights(range.clone()),
            range,
        });
    }

    /// The syntax highlights on `line`, relative to its start.
    fn syntax_layer(
        &self,
        editor: &Editor,
        model: &BufferModel,
        line: usize,
        theme: &Theme,
    ) -> Vec<HighlightSpan> {
        let Some(generation) = model.syntax_generation() else {
            return Vec::new();
        };
        let range = model.line_range(line);
        if range.is_empty() {
            return Vec::new();
        }
        // Geometry runs between commits and paints; filling the visible
        // lines lets the next paint reuse its query.
        let visible = editor.visible_lines();
        self.cache_syntax(
            model,
            if visible.contains(&line) {
                visible
            } else {
                line..line + 1
            },
        );
        let cache = self.syntax_cache.borrow();
        let Some(cache) = cache
            .as_ref()
            .filter(|cache| cache.covers(generation, &range))
        else {
            return Vec::new();
        };
        let first = cache
            .spans
            .partition_point(|(span, _)| span.end <= range.start);
        cache.spans[first..]
            .iter()
            .take_while(|(span, _)| span.start < range.end)
            .map(|(span, highlight)| HighlightSpan {
                range: span.start.saturating_sub(range.start)..span.end - range.start,
                style: theme.style(*highlight),
            })
            .collect()
    }

    /// The caret's x within the text area, before horizontal scroll.
    pub(super) fn caret_x(&self, editor: &Editor, window: &Window, cx: &App) -> Pixels {
        let row = editor.row(self.model.read(cx), editor.caret_row());
        self.x_in_row(editor, &row, editor.cursor_col, window, cx) + px(editor.scroll_x)
    }

    /// The caret position on row `index` nearest `x`, measured within the
    /// text area before horizontal scroll. A row that does not end its line
    /// leaves its end to the next row.
    pub(super) fn column_for_x(
        &self,
        editor: &Editor,
        index: usize,
        x: Pixels,
        window: &Window,
        cx: &App,
    ) -> (usize, usize) {
        let model = self.model.read(cx);
        let row = editor.row(model, index);
        if row.range.is_empty() {
            return (row.line, row.range.start);
        }
        let text = line_text(model, row.line);
        let shaped = self.shape_row(editor, &row, &text, window, cx);
        let row_text = &text[row.range.clone()];
        let tab_size = editor.tab_size(model, cx);
        let offset = self.row_x(editor, &row, row_text, shaped.width) + px(editor.scroll_x);
        let col = row_text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(row.last.then_some(row_text.len()))
            .min_by(|a, b| {
                let distance = |index| {
                    f32::from(
                        (x_for_index_dir(&shaped, index, row_text, tab_size) + offset - x).abs(),
                    )
                };
                distance(*a).total_cmp(&distance(*b))
            })
            .unwrap_or(0);
        (row.line, row.range.start + col)
    }

    /// Per-line horizontal offset for RTL lines: right-align the line within
    /// the pane so a pure-Arabic line starts at the right edge instead of the
    /// left. Returns 0 for LTR lines, `(pane_w - shaped_w).max(0)` for RTL.
    /// The caret, selection, IME preedit, decoration, and hit-test all add
    /// this offset to their x computations so they stay consistent with the
    /// right-aligned text.
    pub(super) fn line_x_offset(
        &self,
        editor: &Editor,
        text: &str,
        shaped_w: Pixels,
        pane_w: Pixels,
    ) -> Pixels {
        (if is_rtl_line(text) {
            (pane_w - shaped_w).max(px(0.))
        } else {
            px(0.)
        }) - px(editor.scroll_x)
    }
}

/// The underline decorations on `line`, one layer per color key and style
/// in the order the model paints them, so a later layer's underline wins
/// where they overlap. Backgrounds paint as quads.
fn decoration_layers(editor: &Editor, line: usize, theme: &Theme) -> Vec<Vec<HighlightSpan>> {
    let underlines = || {
        editor.decorations.iter().filter(move |decoration| {
            decoration.line == line
                && matches!(
                    decoration.style,
                    DecorationStyle::Underline | DecorationStyle::Wavy
                )
        })
    };
    let mut layers: Vec<(&str, DecorationStyle)> = Vec::new();
    for decoration in underlines() {
        let layer = (decoration.key.as_str(), decoration.style);
        layers.retain(|existing| *existing != layer);
        layers.push(layer);
    }
    layers
        .into_iter()
        .map(|(key, style)| {
            let mut ranges: Vec<Range<usize>> = underlines()
                .filter(|decoration| decoration.key == key && decoration.style == style)
                .map(|decoration| decoration.start..decoration.end)
                .collect();
            ranges.sort_unstable_by_key(|range| range.start);
            let mut merged: Vec<Range<usize>> = Vec::new();
            for range in ranges {
                match merged.last_mut() {
                    Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                    _ => merged.push(range),
                }
            }
            let style = theme.decoration(key, style);
            merged
                .into_iter()
                .map(|range| HighlightSpan { range, style })
                .collect()
        })
        .collect()
}

pub(super) fn slice_runs(runs: &[TextRun], range: Range<usize>) -> Vec<TextRun> {
    let mut start = 0;
    runs.iter()
        .filter_map(|run| {
            let run_range = start..start + run.len;
            start = run_range.end;
            let len = run_range
                .end
                .min(range.end)
                .saturating_sub(run_range.start.max(range.start));
            (len > 0).then(|| TextRun { len, ..run.clone() })
        })
        .collect()
}

/// Tab stops every `tab_size` columns are a rendering projection; source byte
/// offsets stay intact.
pub(super) fn expand_tabs(text: &str, tab_size: usize) -> String {
    let mut out = String::new();
    let mut column = 0;
    for grapheme in text.graphemes(true) {
        if grapheme == "\t" {
            let count = tab_size - column % tab_size;
            out.extend(std::iter::repeat_n(' ', count));
            column += count;
        } else {
            out.push_str(grapheme);
            column += 1;
        }
    }
    out
}

pub(super) fn shape_editor_line(
    window: &Window,
    text: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    tab_size: usize,
) -> ShapedLine {
    if !text.contains('\t') {
        return window.text_system().shape_line(text, font_size, runs, None);
    }
    let mut offset = 0;
    let projected_runs = runs
        .iter()
        .map(|run| {
            let end = offset + run.len;
            let mut projected = run.clone();
            projected.len = expand_tabs(&text[..end], tab_size).len()
                - expand_tabs(&text[..offset], tab_size).len();
            offset = end;
            projected
        })
        .collect::<Vec<_>>();
    window.text_system().shape_line(
        expand_tabs(&text, tab_size).into(),
        font_size,
        &projected_runs,
        None,
    )
}

/// A resolved style over a line-relative byte range.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HighlightSpan {
    pub(super) range: Range<usize>,
    pub(super) style: HighlightStyle,
}

/// Compose `line`'s text runs from ordered highlight layers.
///
/// Each layer's spans are sorted and non-overlapping, on char boundaries, and
/// may extend past the line, which clips them. Layers apply in order, so a
/// later layer's color and font override an earlier one's and its underline
/// joins them. Bytes no span covers use the theme's foreground.
pub(super) fn runs_for(line: &str, layers: &[&[HighlightSpan]], theme: &Theme) -> Vec<TextRun> {
    if line.is_empty() {
        return Vec::new();
    }
    let mut boundaries = vec![0, line.len()];
    for span in layers.iter().flat_map(|layer| layer.iter()) {
        boundaries.extend([
            span.range.start.min(line.len()),
            span.range.end.min(line.len()),
        ]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut cursors = vec![0; layers.len()];
    let mut runs: Vec<(usize, HighlightStyle)> = Vec::new();
    for segment in boundaries.windows(2) {
        let (start, end) = (segment[0], segment[1]);
        let mut style = HighlightStyle::default();
        for (layer, cursor) in layers.iter().zip(&mut cursors) {
            while layer
                .get(*cursor)
                .is_some_and(|span| span.range.end <= start)
            {
                *cursor += 1;
            }
            if let Some(span) = layer.get(*cursor)
                && span.range.start <= start
            {
                style = style.highlight(span.style);
            }
        }
        match runs.last_mut() {
            Some((len, last)) if *last == style => *len += end - start,
            _ => runs.push((end - start, style)),
        }
    }
    runs.into_iter()
        .map(|(len, style)| {
            let mut font = make_font(false, false);
            font.weight = style.font_weight.unwrap_or(font.weight);
            font.style = style.font_style.unwrap_or(font.style);
            TextRun {
                len,
                font,
                color: style.color.unwrap_or(theme.color(color::EDITOR_FOREGROUND)),
                background_color: None,
                underline: style.underline,
                strikethrough: style.strikethrough,
            }
        })
        .collect()
}

/// Heuristic: is this line's base direction right-to-left? Scans for the
/// first strong directional character (a letter) and checks whether it
/// falls in an RTL Unicode block (Arabic or Hebrew). Lines starting with
/// LTR content (code, comments, markdown) return false even if they
/// contain embedded RTL runs — the base direction stays LTR.
pub(super) fn is_rtl_line(s: &str) -> bool {
    for ch in s.chars() {
        if !ch.is_alphabetic() {
            continue;
        }
        return char_is_strong_rtl(ch);
    }
    false
}

/// Is this character a strong RTL character (Arabic/Hebrew block)?
pub(super) fn char_is_strong_rtl(ch: char) -> bool {
    let c = ch as u32;
    (0x0590..=0x05FF).contains(&c)   // Hebrew
        || (0x0600..=0x06FF).contains(&c)      // Arabic
        || (0x0700..=0x074F).contains(&c)      // Syriac
        || (0x0750..=0x077F).contains(&c)      // Arabic Supplement
        || (0x08A0..=0x08FF).contains(&c)      // Arabic Extended-A
        || (0xFB1D..=0xFB4F).contains(&c)      // Hebrew presentation forms
        || (0xFB50..=0xFDFF).contains(&c)      // Arabic presentation forms-A
        || (0xFE70..=0xFEFF).contains(&c) // Arabic presentation forms-B
}

/// Direction-aware `x_for_index`. gpui's `ShapedLine::x_for_index` walks
/// glyphs in visual order assuming increasing logical indices — correct for
/// pure LTR, broken for RTL where Core Text reorders glyphs so visual order
/// has DECREASING logical indices.
///
/// This function detects the direction of the character AT `index` (the char
/// after the boundary) rather than relying on per-run heuristics, because
/// gpui merges CTRuns by font — a single `ShapedRun` can contain both RTL
/// and LTR glyphs (e.g. an Arabic line with embedded English, where the
/// surrounding spaces share the same font as the English text).
///
/// For an LTR character at `index`: the caret is at the LEFT edge of that
/// character's glyph (find the glyph with `index == target`).
/// For an RTL character at `index`: the caret is at the LEFT edge of the
/// glyph with the LARGEST index < `index` (the char before the boundary
/// in logical order, which sits to the RIGHT in visual order).
///
/// Special cases: `index == 0` returns `s.width` for RTL-base lines (right
/// edge) or `px(0.)` for LTR; `index >= s.len` is the mirror.
///
/// Uses only public fields: `LineLayout.runs`, `ShapedRun.glyphs`,
/// `ShapedGlyph.index`, `ShapedGlyph.position`, `LineLayout.width`,
/// `LineLayout.len`.
pub(super) fn x_for_index_dir(
    s: &ShapedLine,
    index: usize,
    line_str: &str,
    tab_size: usize,
) -> Pixels {
    let expanded;
    let (index, line_str) = if line_str.contains('\t') {
        let mapped = expand_tabs(&line_str[..index], tab_size).len();
        expanded = expand_tabs(line_str, tab_size);
        (mapped, expanded.as_str())
    } else {
        (index, line_str)
    };
    let rtl_base = is_rtl_line(line_str);
    if index == 0 {
        return if rtl_base { s.width } else { px(0.) };
    }
    if index >= s.len {
        return if rtl_base { px(0.) } else { s.width };
    }

    let rtl = line_str[index..]
        .chars()
        .next()
        .map(char_is_strong_rtl)
        .unwrap_or(rtl_base);

    let all_glyphs: Vec<&ShapedGlyph> = s.runs.iter().flat_map(|r| r.glyphs.iter()).collect();

    if !rtl {
        // A grapheme boundary can fall after a single glyph representing an
        // entire emoji/ZWJ cluster. In that case there is no glyph *at* the
        // boundary: use the next glyph's left edge, or the line's right edge
        // at end-of-line. Falling back to the preceding glyph's position
        // would paint the caret on the emoji's left edge.
        return all_glyphs
            .iter()
            .filter(|g| g.index >= index)
            .min_by_key(|g| g.index)
            .map_or(s.width, |g| g.position.x);
    }

    let mut best: Option<Pixels> = None;
    let mut best_idx: i64 = -1;
    for g in &all_glyphs {
        if (g.index as i64) < (index as i64) && (g.index as i64) > best_idx {
            best_idx = g.index as i64;
            best = Some(g.position.x);
        }
    }
    best.unwrap_or(if rtl_base { px(0.) } else { s.width })
}
