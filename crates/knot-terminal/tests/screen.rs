//! Feeds program output to a terminal and reads the screen back through
//! render-state frames.

use std::sync::Mutex;

use knot_terminal::{
    CellWidth, CursorStyle, Dirty, RenderState, Rgb, Scroll, Style, Terminal, TerminalSize,
    Underline, default_palette,
};

fn size(columns: u16, rows: u16) -> TerminalSize {
    TerminalSize {
        columns,
        rows,
        cell_width: 8,
        cell_height: 16,
    }
}

fn terminal(columns: u16, rows: u16) -> Mutex<Terminal> {
    Mutex::new(Terminal::new(size(columns, rows), 10_000))
}

fn write(terminal: &Mutex<Terminal>, bytes: &[u8]) {
    terminal.lock().unwrap().write(bytes);
}

fn frame(terminal: &Mutex<Terminal>) -> RenderState {
    let mut state = RenderState::new();
    state.update(terminal);
    state
}

/// Each viewport row's text, with empty cells as spaces and trailing spaces
/// removed.
fn screen(state: &mut RenderState) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rows = state.rows();
    while let Some(mut row) = rows.next() {
        let mut line = String::new();
        let mut cells = row.cells();
        while let Some(cell) = cells.next() {
            match (cell.width, cell.text) {
                (CellWidth::SpacerTail | CellWidth::SpacerHead, _) => {}
                (_, "") => line.push(' '),
                (_, text) => line.push_str(text),
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    lines
}

#[derive(Debug, PartialEq)]
struct OwnedCell {
    text: String,
    width: CellWidth,
    style: Style,
    foreground: Option<Rgb>,
    background: Option<Rgb>,
}

fn row_cells(state: &mut RenderState, index: u16) -> Vec<OwnedCell> {
    let mut rows = state.rows();
    while let Some(mut row) = rows.next() {
        if row.index() != index {
            continue;
        }
        let mut owned = Vec::new();
        let mut cells = row.cells();
        while let Some(cell) = cells.next() {
            owned.push(OwnedCell {
                text: cell.text.to_owned(),
                width: cell.width,
                style: cell.style,
                foreground: cell.foreground,
                background: cell.background,
            });
        }
        return owned;
    }
    panic!("no row {index}");
}

fn dirty_rows(state: &mut RenderState) -> Vec<u16> {
    let mut dirty = Vec::new();
    let mut rows = state.rows();
    while let Some(row) = rows.next() {
        if row.dirty() {
            dirty.push(row.index());
        }
    }
    dirty
}

#[test]
fn sgr_styles_and_colors_resolve_per_cell() {
    let terminal = terminal(10, 2);
    write(
        &terminal,
        b"\x1b[1;3;4:3;9;31;48;2;10;20;30mA\x1b[0m\x1b[38;5;196mB\x1b[0m\x1b[7;2;53mC\x1b[0m\x1b[4:2mD\x1b[0mE",
    );
    let mut state = frame(&terminal);
    let palette = default_palette();
    let cells = row_cells(&mut state, 0);

    assert_eq!(cells[0].text, "A");
    assert_eq!(
        cells[0].style,
        Style {
            bold: true,
            italic: true,
            strikethrough: true,
            underline: Underline::Curly,
            ..Style::default()
        }
    );
    assert_eq!(cells[0].foreground, Some(palette[1]));
    assert_eq!(cells[0].background, Some(Rgb::new(10, 20, 30)));

    assert_eq!(cells[1].style, Style::default());
    assert_eq!(cells[1].foreground, Some(palette[196]));
    assert_eq!(cells[1].background, None);

    assert_eq!(
        cells[2].style,
        Style {
            inverse: true,
            faint: true,
            overline: true,
            ..Style::default()
        }
    );
    assert_eq!(cells[2].foreground, None);

    assert_eq!(cells[3].style.underline, Underline::Double);
    assert_eq!(cells[4].style, Style::default());
    assert_eq!((cells[4].foreground, cells[4].background), (None, None));
    assert_eq!(cells[5].text, "");
}

#[test]
fn configured_colors_resolve_and_programs_override_them() {
    let terminal = terminal(10, 2);
    let mut palette = default_palette();
    palette[1] = Rgb::from_packed(0x123456);
    palette[200] = Rgb::from_packed(0xabcdef);
    {
        let mut terminal = terminal.lock().unwrap();
        terminal.set_palette(&palette);
        terminal.set_default_colors(Rgb::from_packed(0xeeeeee), Rgb::from_packed(0x111111));
    }
    write(&terminal, b"\x1b[31mA\x1b[38;5;200mB");
    let mut state = frame(&terminal);
    let cells = row_cells(&mut state, 0);
    assert_eq!(cells[0].foreground, Some(Rgb::from_packed(0x123456)));
    assert_eq!(cells[1].foreground, Some(Rgb::from_packed(0xabcdef)));
    let colors = state.colors();
    assert_eq!(colors.foreground, Rgb::from_packed(0xeeeeee));
    assert_eq!(colors.background, Rgb::from_packed(0x111111));
    assert_eq!(colors.palette, palette);

    // OSC 10 sets the foreground and OSC 4 one palette entry.
    write(
        &terminal,
        b"\x1b]10;rgb:ff/00/00\x1b\\\x1b]4;1;rgb:00/ff/00\x1b\\",
    );
    state.update(&terminal);
    let colors = state.colors();
    assert_eq!(colors.foreground, Rgb::new(0xff, 0, 0));
    assert_eq!(colors.palette[1], Rgb::new(0, 0xff, 0));
    assert_eq!(
        row_cells(&mut state, 0)[0].foreground,
        Some(Rgb::new(0, 0xff, 0))
    );
}

#[test]
fn wide_characters_and_grapheme_clusters() {
    let terminal = terminal(5, 4);
    write(&terminal, "a界b\r\ne\u{301}\r\nabcd界".as_bytes());
    let mut state = frame(&terminal);

    let widths = |cells: &[OwnedCell]| {
        cells
            .iter()
            .map(|cell| (cell.text.clone(), cell.width))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        widths(&row_cells(&mut state, 0))[..4],
        [
            ("a".into(), CellWidth::Narrow),
            ("界".into(), CellWidth::Wide),
            (String::new(), CellWidth::SpacerTail),
            ("b".into(), CellWidth::Narrow),
        ]
    );
    assert_eq!(row_cells(&mut state, 1)[0].text, "e\u{301}");

    // A wide character that does not fit in the last column wraps whole,
    // leaving a spacer head behind.
    assert_eq!(screen(&mut state), ["a界b", "e\u{301}", "abcd", "界"]);
    assert_eq!(row_cells(&mut state, 2)[4].width, CellWidth::SpacerHead);
    assert_eq!(row_cells(&mut state, 3)[0].width, CellWidth::Wide);
}

#[test]
fn scrollback_and_viewport_scrolling() {
    let terminal = terminal(10, 3);
    let output = (0..10)
        .map(|line| format!("line{line}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    write(&terminal, output.as_bytes());
    let mut state = frame(&terminal);
    assert_eq!(screen(&mut state), ["line7", "line8", "line9"]);
    let scrollbar = terminal.lock().unwrap().scrollbar();
    assert_eq!(
        (scrollbar.total, scrollbar.offset, scrollbar.len),
        (10, 7, 3)
    );
    assert_eq!(scrollbar.rows_below(), 0);
    assert_eq!(state.cursor().position, Some((5, 2)));

    terminal.lock().unwrap().scroll_viewport(Scroll::Delta(-2));
    state.update(&terminal);
    assert_eq!(screen(&mut state), ["line5", "line6", "line7"]);
    assert_eq!(terminal.lock().unwrap().scrollbar().rows_below(), 2);
    assert_eq!(state.cursor().position, None);

    terminal.lock().unwrap().scroll_viewport(Scroll::Top);
    state.update(&terminal);
    assert_eq!(screen(&mut state), ["line0", "line1", "line2"]);

    terminal.lock().unwrap().scroll_viewport(Scroll::Bottom);
    state.update(&terminal);
    assert_eq!(screen(&mut state), ["line7", "line8", "line9"]);
    assert_eq!(state.cursor().position, Some((5, 2)));
}

#[test]
fn resizing_reflows_soft_wrapped_lines() {
    let terminal = terminal(10, 4);
    write(&terminal, b"abcdefghijklmno");
    let mut state = frame(&terminal);
    assert_eq!(screen(&mut state), ["abcdefghij", "klmno", "", ""]);

    terminal.lock().unwrap().resize(size(20, 4));
    state.update(&terminal);
    assert_eq!(state.size(), (20, 4));
    assert_eq!(screen(&mut state), ["abcdefghijklmno", "", "", ""]);

    terminal.lock().unwrap().resize(size(5, 4));
    state.update(&terminal);
    assert_eq!(screen(&mut state), ["abcde", "fghij", "klmno", ""]);
}

#[test]
fn cursor_position_shape_and_visibility() {
    let terminal = terminal(10, 3);
    write(&terminal, b"ab\r\ncd");
    let mut state = frame(&terminal);
    let cursor = state.cursor();
    assert_eq!(cursor.position, Some((2, 1)));
    assert_eq!(cursor.style, CursorStyle::Block);
    assert!(cursor.visible);

    for (sequence, style, blinking) in [
        (&b"\x1b[5 q"[..], CursorStyle::Bar, true),
        (b"\x1b[4 q", CursorStyle::Underline, false),
        (b"\x1b[1 q", CursorStyle::Block, true),
        (b"\x1b[0 q", CursorStyle::Block, false),
    ] {
        write(&terminal, sequence);
        state.update(&terminal);
        let cursor = state.cursor();
        assert_eq!((cursor.style, cursor.blinking), (style, blinking));
    }

    write(&terminal, b"\x1b[?25l");
    state.update(&terminal);
    assert!(!state.cursor().visible);

    write(&terminal, "\x1b[?25h\r\n界\x1b[D".as_bytes());
    state.update(&terminal);
    let cursor = state.cursor();
    assert!(cursor.visible);
    assert_eq!(cursor.position, Some((1, 2)));
    assert!(cursor.wide_tail);
}

#[test]
fn dirty_tracking_follows_changes_until_cleaned() {
    let terminal = terminal(10, 4);
    let mut state = frame(&terminal);
    assert_eq!(state.dirty(), Dirty::Full);

    state.clean();
    assert_eq!(state.dirty(), Dirty::Clean);
    assert_eq!(dirty_rows(&mut state), Vec::<u16>::new());
    state.update(&terminal);
    assert_eq!(state.dirty(), Dirty::Clean);

    write(&terminal, b"\x1b[3;1Hx");
    state.update(&terminal);
    assert_eq!(state.dirty(), Dirty::Partial);
    let dirty = dirty_rows(&mut state);
    assert!(dirty.contains(&2), "{dirty:?}");
    assert!(!dirty.contains(&3), "{dirty:?}");

    // An update with no new output keeps what the last frame did not clean.
    state.update(&terminal);
    assert_eq!(state.dirty(), Dirty::Partial);
    assert!(dirty_rows(&mut state).contains(&2));

    state.clean();
    terminal.lock().unwrap().resize(size(12, 4));
    state.update(&terminal);
    assert_eq!(state.dirty(), Dirty::Full);
}

#[test]
fn rows_report_soft_wraps_and_cells_report_hyperlinks() {
    let terminal = terminal(4, 4);
    write(
        &terminal,
        b"abcdef\r\n\x1b]8;;https://example.test\x1b\\li\x1b]8;;\x1b\\nk",
    );
    let mut state = frame(&terminal);
    let mut wrapped = Vec::new();
    let mut linked = Vec::new();
    let mut rows = state.rows();
    while let Some(mut row) = rows.next() {
        wrapped.push(row.wrapped());
        let mut cells = row.cells();
        let mut line = Vec::new();
        while let Some(cell) = cells.next() {
            line.push(cell.hyperlink);
        }
        linked.push(line);
    }
    assert_eq!(wrapped, [true, false, false, false]);
    assert_eq!(linked[2], [true, true, false, false]);
    assert_eq!(linked[0], [false; 4]);
}
