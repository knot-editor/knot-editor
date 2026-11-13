//! Replies, notices, modes, the selection, text, and hyperlinks, observed
//! through program output.

use std::sync::{Arc, Mutex};

use knot_terminal::{
    Mode, Point, RenderState, Rgb, Scroll, Terminal, TerminalEvent, TerminalSize, default_palette,
};

fn size(columns: u16, rows: u16) -> TerminalSize {
    TerminalSize {
        columns,
        rows,
        cell_width: 9,
        cell_height: 18,
    }
}

/// A terminal whose events collect in the returned list.
fn recording(columns: u16, rows: u16) -> (Terminal, Arc<Mutex<Vec<TerminalEvent>>>) {
    let mut terminal = Terminal::new(size(columns, rows), 10_000);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    terminal.set_event_handler(move |event| sink.lock().unwrap().push(event));
    (terminal, events)
}

fn take(events: &Mutex<Vec<TerminalEvent>>) -> Vec<TerminalEvent> {
    std::mem::take(&mut *events.lock().unwrap())
}

fn replies(events: &Mutex<Vec<TerminalEvent>>) -> Vec<String> {
    take(events)
        .into_iter()
        .map(|event| match event {
            TerminalEvent::Reply(bytes) => String::from_utf8(bytes).unwrap(),
            other => panic!("unexpected event: {other:?}"),
        })
        .collect()
}

#[test]
fn queries_are_answered_in_stream_order() {
    let (mut terminal, events) = recording(80, 24);
    for _ in 0..100 {
        terminal.write(b"\x1b[5n\x1b[6n\x1b[c\x1b[18t\x1b[14t\x1b[?2004$p");
    }
    assert_eq!(
        replies(&events),
        [
            "\x1b[0n",
            "\x1b[1;1R",
            "\x1b[?62;22c",
            "\x1b[8;24;80t",
            "\x1b[4;432;720t",
            "\x1b[?2004;2$y",
        ]
        .repeat(100)
    );

    // Size reports follow a resize, and DECRQM follows mode changes.
    terminal.resize(TerminalSize {
        columns: 100,
        rows: 30,
        cell_width: 10,
        cell_height: 20,
    });
    terminal.write(b"\x1b[18t\x1b[14t\x1b[?2004h\x1b[?2004$p");
    assert_eq!(
        replies(&events),
        ["\x1b[8;30;100t", "\x1b[4;600;1000t", "\x1b[?2004;1$y"]
    );

    // With in-band size reports on, a resize reports the new size.
    terminal.write(b"\x1b[?2048h");
    take(&events);
    terminal.resize(size(40, 10));
    assert_eq!(replies(&events), ["\x1b[48;10;40;180;360t"]);
}

#[test]
fn color_queries_answer_from_the_configured_colors_and_program_overrides() {
    let (mut terminal, events) = recording(80, 24);
    let mut palette = default_palette();
    palette[1] = Rgb::from_packed(0x445566);
    terminal.set_palette(&palette);
    terminal.set_default_colors(Rgb::from_packed(0xd4d4d4), Rgb::from_packed(0x181818));
    terminal.write(b"\x1b]4;1;?\x1b\\\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
    terminal.write(b"\x1b]4;1;#112233\x1b\\\x1b]4;1;?\x1b\\");
    assert_eq!(
        replies(&events),
        [
            "\x1b]4;1;rgb:4444/5555/6666\x1b\\",
            "\x1b]10;rgb:d4d4/d4d4/d4d4\x1b\\",
            "\x1b]11;rgb:1818/1818/1818\x1b\\",
            "\x1b]4;1;rgb:1111/2222/3333\x1b\\",
        ]
    );
}

#[test]
fn clipboard_requests_are_ignored() {
    let (mut terminal, events) = recording(80, 24);
    terminal.write(b"\x1b]52;c;?\x1b\\\x1b]52;c;YQ==\x1b\\\x1b]52;c;?\x07");
    assert_eq!(take(&events), []);
}

#[test]
fn title_bell_and_working_directory_are_reported() {
    let (mut terminal, events) = recording(80, 24);
    terminal.write(b"\x1b]2;build\x1b\\\x07\x1b]7;file://host/tmp/dir\x1b\\\x1b]7;\x1b\\");
    assert_eq!(
        take(&events),
        [
            TerminalEvent::Title("build".into()),
            TerminalEvent::Bell,
            TerminalEvent::WorkingDirectory("file://host/tmp/dir".into()),
            TerminalEvent::WorkingDirectory(String::new()),
        ]
    );
}

#[test]
fn without_a_handler_queries_go_unanswered() {
    let mut terminal = Terminal::new(size(80, 24), 100);
    terminal.write(b"\x1b[6n\x07");
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    terminal.set_event_handler(move |event| sink.lock().unwrap().push(event));
    terminal.write(b"\x1b[5n");
    assert_eq!(replies(&events), ["\x1b[0n"]);
}

#[test]
fn modes_follow_the_program() {
    let (mut terminal, _) = recording(80, 24);
    let modes = [
        Mode::CursorKeys,
        Mode::KeypadKeys,
        Mode::FocusEvents,
        Mode::BracketedPaste,
    ];
    assert!(modes.iter().all(|&mode| !terminal.mode(mode)));
    assert!(!terminal.alternate_screen());
    terminal.write(b"\x1b[?1h\x1b=\x1b[?1004h\x1b[?2004h\x1b[?1049h");
    assert!(modes.iter().all(|&mode| terminal.mode(mode)));
    assert!(terminal.alternate_screen());
    terminal.write(b"\x1b[?1049l\x1b[?1l\x1b>\x1b[?1004l\x1b[?2004l");
    assert!(modes.iter().all(|&mode| !terminal.mode(mode)));
    assert!(!terminal.alternate_screen());

    // Alternate scroll starts on.
    assert!(terminal.mode(Mode::AlternateScroll));
    terminal.write(b"\x1b[?1007l");
    assert!(!terminal.mode(Mode::AlternateScroll));

    assert!(!terminal.mouse_tracking());
    for (set, tracking, format) in [
        (
            &b"\x1b[?1000h\x1b[?1006h"[..],
            Mode::MouseNormal,
            Mode::MouseFormatSgr,
        ),
        (
            b"\x1b[?1002h\x1b[?1005h",
            Mode::MouseButton,
            Mode::MouseFormatUtf8,
        ),
        (
            b"\x1b[?1003h\x1b[?1015h",
            Mode::MouseAny,
            Mode::MouseFormatUrxvt,
        ),
        (
            b"\x1b[?9h\x1b[?1016h",
            Mode::MouseX10,
            Mode::MouseFormatSgrPixels,
        ),
    ] {
        terminal.write(set);
        assert!(terminal.mouse_tracking());
        assert!(
            terminal.mode(tracking) && terminal.mode(format),
            "{tracking:?}"
        );
        terminal.write(b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l");
        assert!(!terminal.mouse_tracking());
    }
}

#[test]
fn selection_joins_soft_wraps_and_follows_scrolling() {
    let (mut terminal, _) = recording(10, 3);
    // "abcdefghijklmno" wraps after ten columns; "next" is its own line.
    terminal.write(b"abcdefghijklmno\r\nnext");
    assert!(!terminal.has_selection());
    assert_eq!(terminal.selection_text(), None);

    terminal.select(Point::new(7, 0), Point::new(2, 1));
    assert_eq!(terminal.selection_text().as_deref(), Some("hijklm"));
    terminal.extend_selection(Point::new(3, 2));
    assert_eq!(terminal.selection_text().as_deref(), Some("hijklmno\nnext"));

    // Selecting backwards keeps the anchor.
    terminal.select(Point::new(4, 1), Point::new(8, 0));
    assert_eq!(terminal.selection_text().as_deref(), Some("ijklmno"));

    // Output scrolls the selected cells into the scrollback; the selection
    // follows them, and extending it from the viewport keeps its anchor.
    terminal.select(Point::new(0, 2), Point::new(3, 2));
    terminal.write(b"\r\nmore\r\nlines");
    assert_eq!(terminal.selection_text().as_deref(), Some("next"));
    terminal.extend_selection(Point::new(4, 2));
    assert_eq!(
        terminal.selection_text().as_deref(),
        Some("next\nmore\nlines")
    );

    // Selecting in a scrolled viewport selects the rows shown.
    terminal.scroll_viewport(Scroll::Top);
    terminal.select(Point::new(0, 0), Point::new(4, 0));
    assert_eq!(terminal.selection_text().as_deref(), Some("abcde"));

    terminal.clear_selection();
    assert!(!terminal.has_selection());
}

#[test]
fn frames_carry_each_row_s_selected_columns() {
    let (mut terminal, _) = recording(10, 3);
    terminal.write(b"abcdefghijklmno\r\nnext");
    terminal.select(Point::new(7, 0), Point::new(2, 1));
    let terminal = Mutex::new(terminal);
    let mut state = RenderState::new();
    state.update(&terminal);
    let mut selected = Vec::new();
    let mut rows = state.rows();
    while let Some(row) = rows.next() {
        selected.push(row.selection());
    }
    assert_eq!(selected, [Some((7, 9)), Some((0, 2)), None]);
}

#[test]
fn text_has_the_scrollback_and_one_line_per_logical_line() {
    let (mut terminal, _) = recording(10, 3);
    terminal.write(b"first\r\nabcdefghijklmno\r\n  indented  \r\nlast\r\n\r\n");
    assert_eq!(terminal.text(), "first\nabcdefghijklmno\n  indented\nlast");
    terminal.write(b"\x1b[?1049h\x1b[Halternate");
    assert_eq!(terminal.text(), "alternate");
    terminal.write(b"\x1b[?1049l");
    assert!(terminal.text().starts_with("first\n"));
}

#[test]
fn hyperlinks_resolve_per_cell() {
    let (mut terminal, _) = recording(20, 3);
    terminal.write(
        b"ab\x1b]8;id=one;https://destination.test/path\x1b\\label\x1b]8;;\x1b\\ \x1b]8;;file:///tmp/x\x1b\\f\x1b]8;;\x1b\\",
    );
    let links: Vec<_> = (0..11)
        .map(|column| terminal.hyperlink(Point::new(column, 0)))
        .collect();
    let destination = Some("https://destination.test/path".to_owned());
    assert_eq!(
        links,
        [
            None,
            None,
            destination.clone(),
            destination.clone(),
            destination.clone(),
            destination.clone(),
            destination,
            None,
            Some("file:///tmp/x".to_owned()),
            None,
            None,
        ]
    );
    assert_eq!(terminal.hyperlink(Point::new(0, 5)), None);
}
