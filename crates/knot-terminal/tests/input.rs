//! Mouse reports in each tracking mode and format, and keys in the
//! protocols a program requests.

use knot_terminal::{
    Key, KeyEvent, Modifiers, MouseAction, MouseButton, MouseEvent, Terminal, TerminalSize,
};

/// A 300×2100 grid of 8×16-pixel cells after the program wrote `setup`.
fn terminal(setup: &[u8]) -> Terminal {
    let size = TerminalSize {
        columns: 300,
        rows: 2100,
        cell_width: 8,
        cell_height: 16,
    };
    let mut terminal = Terminal::new(size, 0);
    terminal.write(setup);
    terminal
}

/// The report of `action` with `button` at the middle of a cell.
fn report(
    terminal: &Terminal,
    action: MouseAction,
    button: Option<MouseButton>,
    modifiers: Modifiers,
    (column, row): (u16, u16),
) -> Vec<u8> {
    let event = MouseEvent {
        action,
        button,
        modifiers,
        x: f32::from(column) * 8. + 4.,
        y: f32::from(row) * 16. + 8.,
    };
    terminal.encode_mouse(&event, action == MouseAction::Motion && button.is_some())
}

fn press(terminal: &Terminal, button: MouseButton, cell: (u16, u16)) -> Vec<u8> {
    report(
        terminal,
        MouseAction::Press,
        Some(button),
        Modifiers::default(),
        cell,
    )
}

#[test]
fn tracking_modes_choose_which_events_are_reported() {
    let none = Modifiers::default();
    let left = Some(MouseButton::Left);
    assert!(press(&terminal(b""), MouseButton::Left, (2, 4)).is_empty());
    // X10 reports presses only.
    let x10 = terminal(b"\x1b[?9h");
    assert_eq!(press(&x10, MouseButton::Left, (2, 4)), b"\x1b[M #%");
    assert!(report(&x10, MouseAction::Release, left, none, (2, 4)).is_empty());
    for (setup, drag, motion) in [
        (&b"\x1b[?1000h"[..], false, false),
        (b"\x1b[?1002h", true, false),
        (b"\x1b[?1003h", true, true),
    ] {
        let terminal = terminal(setup);
        assert_eq!(
            report(&terminal, MouseAction::Release, left, none, (2, 4)),
            b"\x1b[M##%"
        );
        let dragged = report(&terminal, MouseAction::Motion, left, none, (2, 4));
        assert_eq!(dragged == b"\x1b[M@#%", drag, "{setup:?}");
        let moved = report(&terminal, MouseAction::Motion, None, none, (2, 4));
        assert_eq!(moved == b"\x1b[MC#%", motion, "{setup:?}");
    }
}

#[test]
fn buttons_wheels_and_modifiers_in_each_format() {
    let modifiers = Modifiers {
        shift: true,
        alt: true,
        control: true,
        super_key: false,
    };
    let sgr = terminal(b"\x1b[?1000;1006h");
    for (button, code) in [
        (MouseButton::Left, 0),
        (MouseButton::Middle, 1),
        (MouseButton::Right, 2),
        (MouseButton::WheelUp, 64),
        (MouseButton::WheelDown, 65),
        (MouseButton::WheelLeft, 66),
        (MouseButton::WheelRight, 67),
        (MouseButton::Back, 128),
        (MouseButton::Forward, 129),
    ] {
        assert_eq!(
            press(&sgr, button, (2, 4)),
            format!("\x1b[<{code};3;5M").as_bytes()
        );
    }
    assert_eq!(
        report(
            &sgr,
            MouseAction::Release,
            Some(MouseButton::Right),
            modifiers,
            (2, 4)
        ),
        b"\x1b[<30;3;5m"
    );
    let legacy = terminal(b"\x1b[?1000h");
    assert_eq!(
        report(
            &legacy,
            MouseAction::Press,
            Some(MouseButton::Left),
            modifiers,
            (2, 4)
        ),
        b"\x1b[M<#%"
    );
    let urxvt = terminal(b"\x1b[?1000;1015h");
    assert_eq!(press(&urxvt, MouseButton::Left, (2, 4)), b"\x1b[32;3;5M");
    // SGR pixels reports the zero-based pixel position within the grid.
    let pixels = terminal(b"\x1b[?1000;1016h");
    assert_eq!(press(&pixels, MouseButton::Left, (2, 4)), b"\x1b[<0;20;72M");
}

#[test]
fn coordinates_past_a_format_limit_are_not_reported() {
    let legacy = terminal(b"\x1b[?1000h");
    assert_eq!(
        press(&legacy, MouseButton::Left, (222, 222)),
        [27, 91, 77, 32, 255, 255]
    );
    for cell in [(223, 0), (0, 223)] {
        assert!(press(&legacy, MouseButton::Left, cell).is_empty());
    }
    let utf8 = terminal(b"\x1b[?1000;1005h");
    assert_eq!(
        press(&utf8, MouseButton::Left, (222, 2014)),
        "\x1b[M \u{ff}\u{7ff}".as_bytes()
    );
    let sgr = terminal(b"\x1b[?1000;1006h");
    assert_eq!(
        press(&sgr, MouseButton::Left, (299, 2099)),
        b"\x1b[<0;300;2100M"
    );
}

#[test]
fn keys_follow_the_protocol_the_program_pushes_and_pops() {
    let event = KeyEvent {
        key: Some(Key::Enter),
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
        ..KeyEvent::default()
    };
    let mut terminal = terminal(b"");
    assert_eq!(terminal.encode_key(&event), b"\x1b[27;5;13~");
    terminal.write(b"\x1b[>1u");
    assert_eq!(terminal.encode_key(&event), b"\x1b[13;5u");
    terminal.write(b"\x1b[<u");
    assert_eq!(terminal.encode_key(&event), b"\x1b[27;5;13~");
    let up = KeyEvent {
        key: Some(Key::Up),
        ..KeyEvent::default()
    };
    assert_eq!(terminal.encode_key(&up), b"\x1b[A");
    terminal.write(b"\x1b[?1h");
    assert_eq!(terminal.encode_key(&up), b"\x1bOA");
}
