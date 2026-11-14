use crate::app::keys::{Keystroke, Modifiers};
use gpui::TestAppContext;

use super::{InputModes, terminal_key_bytes};

#[cfg(unix)]
fn read_line() -> knot_terminal::PtyOptions {
    knot_terminal::PtyOptions {
        shell: Some(knot_terminal::Shell::new(
            "/bin/sh",
            vec!["-c".into(), "read line".into()],
        )),
        ..Default::default()
    }
}

#[gpui::test]
fn windows_space_key_without_key_char_reaches_terminal(_cx: &mut TestAppContext) {
    let space = Keystroke {
        key: "space".into(),
        key_char: None,
        modifiers: Modifiers::default(),
    };
    assert_eq!(terminal_key_bytes(&space, InputModes::default()), b" ");
}

#[cfg(unix)]
#[gpui::test]
fn measured_layout_drives_the_grid_and_frames_paint_cells(cx: &mut TestAppContext) {
    use super::*;

    let session = cx.new(|cx| TerminalSession::new_with_options(read_line(), cx));
    let (view, cx) = cx.add_window_view(|_, cx| TerminalView::new(session.clone(), cx));
    cx.refresh().unwrap();
    let initial = cx.read(|cx| session.read(cx).size());
    cx.simulate_resize(size(px(403.5), px(257.25)));
    cx.refresh().unwrap();
    cx.read(|cx| assert_ne!(session.read(cx).size(), initial));
    cx.update(|window, cx| {
        let view = view.read(cx);
        let layout = view.layout.unwrap();
        let measured = TerminalLayout::measure(window, &base_font(cx), px(14.));
        assert_eq!(layout.cell_width, measured.cell_width);
        assert_eq!(layout.cell_height, measured.cell_height);
        assert_eq!(
            session.read(cx).size(),
            layout.grid_size(view.interaction_bounds)
        );
    });
    let palette = cx.read(|cx| TerminalPalette::new(theme(cx)));
    cx.update(|_, cx| {
        session.read(cx).with_terminal(|terminal| {
            terminal.write(
                "\x1b[2J\x1b[HA界e\u{301}Z\r\n\x1b[1;3;4;9;31mS\x1b[0;7mI\x1b[0;2mF\x1b[0;38;2;1;2;3;48;5;21mC\x1b[0m\x1b[4 q"
                    .as_bytes(),
            )
        });
        let snapshot = view.update(cx, |view, cx| TerminalSnapshot::capture(view, cx));
        let cell = |line: usize, column: usize| {
            snapshot
                .cells
                .iter()
                .find(|cell| cell.line == line && cell.column == column)
        };
        let wide = cell(0, 1).unwrap();
        assert_eq!(wide.text, "界");
        assert!(wide.wide);
        assert!(cell(0, 2).is_none(), "the wide glyph's spacer is skipped");
        assert_eq!(cell(0, 3).unwrap().text, "e\u{301}");

        let styled = cell(1, 0).unwrap();
        assert!(styled.style.bold && styled.style.italic && styled.style.strikethrough);
        assert_eq!(styled.style.underline, knot_terminal::Underline::Single);
        assert_eq!(styled.foreground, palette.palette[1]);
        let inverse = cell(1, 1).unwrap();
        assert_eq!(
            (inverse.foreground, inverse.background),
            (palette.background, palette.foreground)
        );
        assert_eq!(cell(1, 2).unwrap().foreground, dim(palette.foreground));
        let colored = cell(1, 3).unwrap();
        assert_eq!(colored.foreground, Rgb::new(1, 2, 3));
        assert_eq!(colored.background, knot_terminal::default_palette()[21]);

        // An unfocused terminal draws a hollow cursor whatever its shape.
        let cursor = snapshot.cursor.as_ref().unwrap();
        assert_eq!((cursor.line, cursor.column), (1, 4));
        assert_eq!(cursor.style, TerminalCursor::BlockHollow);
    });

    // Dragging within a combining cell selects the whole cluster.
    let (layout, bounds) = cx.read(|cx| {
        let view = view.read(cx);
        (view.layout.unwrap(), view.interaction_bounds)
    });
    let start = layout.origin(bounds, 3, 0) + point(px(0.1), px(0.1));
    cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        start + point(layout.cell_width / 2., px(0.)),
        MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(start, MouseButton::Left, gpui::Modifiers::default());
    cx.read(|cx| {
        assert_eq!(
            session
                .read(cx)
                .with_terminal(|terminal| terminal.selection_text())
                .flatten()
                .as_deref(),
            Some("e\u{301}")
        )
    });
    cx.update(|_, cx| session.update(cx, |session, cx| session.close(cx)));
}

#[cfg(unix)]
#[gpui::test]
fn terminal_font_size_resizes_the_grid(cx: &mut TestAppContext) {
    use super::*;
    use crate::app::settings::FONT_SIZE;

    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let session = cx.new(|cx| TerminalSession::new_with_options(read_line(), cx));
    let (view, cx) = cx.add_window_view(|_, cx| TerminalView::new(session.clone(), cx));
    cx.simulate_resize(size(px(600.), px(400.)));
    cx.refresh().unwrap();
    let grid = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let size = session.read(cx).size();
            (
                view.read(cx).layout.unwrap().font_size,
                size.columns,
                size.rows,
            )
        })
    };
    let (font_size, columns, lines) = grid(cx);
    assert_eq!(font_size, px(14.));

    // An editor-wide size leaves a terminal-scoped one in charge.
    cx.update(|_, cx| {
        let store = &mut cx.global_mut::<ApplicationSettings>().0;
        store
            .set(
                FONT_SIZE,
                28.into(),
                SettingsScope {
                    view: Some("terminal".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        store
            .set(FONT_SIZE, 10.into(), SettingsScope::default())
            .unwrap();
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    let (font_size, larger_columns, larger_lines) = grid(cx);
    assert_eq!(font_size, px(28.));
    assert!(larger_columns < columns && larger_lines < lines);
}

#[cfg(unix)]
#[gpui::test]
fn terminal_font_family_follows_the_terminal_scope(cx: &mut TestAppContext) {
    use super::*;
    use crate::app::settings::FONT_FAMILY;

    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let session = cx.new(|cx| TerminalSession::new_with_options(read_line(), cx));
    let (view, cx) = cx.add_window_view(|_, cx| TerminalView::new(session.clone(), cx));
    cx.simulate_resize(size(px(600.), px(400.)));
    cx.refresh().unwrap();
    let family = |cx: &mut VisualTestContext| cx.read(|cx| base_font(cx).family.to_string());
    let cell_width =
        |cx: &mut VisualTestContext| cx.read(|cx| view.read(cx).layout.unwrap().cell_width);
    assert_eq!(family(cx), knot_model::typography::DEFAULT_FAMILY);
    let set_family = |cx: &mut VisualTestContext, family: &str, view: Option<&str>| {
        let family = family.to_owned();
        let view = view.map(str::to_owned);
        cx.update(|_, cx| {
            cx.global_mut::<ApplicationSettings>()
                .0
                .set(
                    FONT_FAMILY,
                    family.into(),
                    SettingsScope {
                        view,
                        ..Default::default()
                    },
                )
                .unwrap();
        });
        cx.run_until_parked();
        cx.refresh().unwrap();
    };

    // An editor-scoped family leaves terminals alone.
    set_family(cx, "Times New Roman", Some("editor"));
    assert_eq!(family(cx), knot_model::typography::DEFAULT_FAMILY);

    set_family(cx, "Times New Roman", Some("terminal"));
    assert_eq!(family(cx), "Times New Roman");
    assert!(cell_width(cx) > px(0.));

    // An uninstalled family still measures.
    set_family(cx, "No Such Family 7f3a", Some("terminal"));
    assert!(cell_width(cx) > px(0.));
}

#[test]
fn overlay_label_only_shows_for_non_running_sessions() {
    use super::TerminalStatus;
    use super::terminal_overlay_label;
    assert_eq!(terminal_overlay_label(&TerminalStatus::Running), None);
    assert_eq!(
        terminal_overlay_label(&TerminalStatus::Closed),
        Some("closed".to_owned())
    );
    assert_eq!(
        terminal_overlay_label(&TerminalStatus::Exited("exit code 0".into())),
        Some("exit code 0".to_owned())
    );
    assert_eq!(
        terminal_overlay_label(&TerminalStatus::Exited("terminated by Killed: 9".into())),
        Some("terminated by Killed: 9".to_owned())
    );
    assert!(
        terminal_overlay_label(&TerminalStatus::Failed("boom".into()))
            .unwrap()
            .contains("failed")
    );
}

#[cfg(unix)]
#[gpui::test]
fn zoom_scales_the_terminal_font_size(cx: &mut TestAppContext) {
    use super::*;

    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let session = cx.new(|cx| TerminalSession::new_with_options(read_line(), cx));
    let (view, cx) = cx.add_window_view(|_, cx| TerminalView::new(session.clone(), cx));
    cx.simulate_resize(size(px(600.), px(400.)));
    cx.refresh().unwrap();
    let columns = cx.read(|cx| session.read(cx).size().columns);
    cx.update(|_, cx| crate::app::zoom::set_level(2, cx));
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.read(|cx| {
        let font_size = f32::from(view.read(cx).layout.unwrap().font_size);
        assert!((font_size - 14. * 1.44).abs() < 1e-3);
        assert!(session.read(cx).size().columns < columns);
    });
    cx.update(|_, cx| session.update(cx, |session, cx| session.close(cx)));
}
