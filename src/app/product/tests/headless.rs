//! Product surfaces driven through a session without a window: keys go
//! through its router, unclaimed keys to the surface with Knot-side focus,
//! and text to that surface's input.
use super::*;
use crate::app::keys::Keystroke;
use crate::app::transient::{TransientOutcome, TransientOwner};
use knot_protocol::{ExtensionId, ExtensionLifecycleId, PickerItem};

fn headless_window(
    documents: &Entity<DocumentCollection>,
    cx: &mut TestAppContext,
) -> (Entity<KnotWindow>, Entity<Buffer>) {
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let shell = cx.update(|cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
        cx.new(|cx| KnotWindow::new(workbench, cx))
    });
    (shell, model)
}

fn press(shell: &Entity<KnotWindow>, keys: &str, cx: &mut TestAppContext) {
    for key in keys.split_whitespace() {
        let keystroke = Keystroke::parse(key).unwrap();
        shell.update(cx, |shell, cx| shell.press_key(keystroke, cx));
        cx.run_until_parked();
    }
}

fn type_text(shell: &Entity<KnotWindow>, text: &str, cx: &mut TestAppContext) {
    shell.update(cx, |shell, cx| shell.type_text(text, cx));
    cx.run_until_parked();
}

fn owner() -> TransientOwner {
    TransientOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(1),
        invocation: None,
    }
}

#[gpui::test]
fn editor_keys_route_and_unclaimed_keys_type(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, model) = headless_window(&documents, cx);
    press(&shell, "x y z backspace left", cx);
    type_text(&shell, "é", cx);
    cx.read(|cx| assert_eq!(model.read(cx).text(), "xéy"));
}

#[gpui::test]
fn the_palette_filters_and_runs_a_command(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, model) = headless_window(&documents, cx);
    shell.update(cx, |shell, cx| shell.open_command_palette(cx));
    type_text(&shell, "split rightx", cx);
    press(&shell, "backspace enter", cx);
    cx.read(|cx| {
        let shell = shell.read(cx);
        assert!(shell.command_palette.is_none());
        assert_eq!(shell.focus, super::super::Focus::Workbench);
        assert_eq!(shell.workbench.read(cx).panes().len(), 2);
    });
    // Focus is back in an editor, so typing edits the document.
    press(&shell, "a", cx);
    cx.read(|cx| assert_eq!(model.read(cx).text(), "a"));
}

#[gpui::test]
fn a_transient_picker_and_input_settle_from_keys(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, model) = headless_window(&documents, cx);
    let items = ["one", "two", "three"]
        .map(|label| PickerItem {
            id: label.to_uppercase(),
            label: label.into(),
            description: None,
            detail: None,
            hint: None,
            icon: None,
        })
        .to_vec();
    let pick = |items: Vec<PickerItem>, cx: &mut TestAppContext| {
        shell
            .update(cx, |shell, cx| {
                shell.begin_picker(owner(), "Pick".into(), items, None, cx)
            })
            .unwrap()
    };

    let mut picked = pick(items.clone(), cx);
    press(&shell, "down down up enter", cx);
    assert_eq!(
        picked.try_recv(),
        Ok(TransientOutcome::Picker("TWO".into()))
    );
    let mut picked = pick(items, cx);
    type_text(&shell, "thr", cx);
    press(&shell, "enter", cx);
    assert_eq!(
        picked.try_recv(),
        Ok(TransientOutcome::Picker("THREE".into()))
    );

    let (mut input, _) = shell
        .update(cx, |shell, cx| {
            shell.begin_input(owner(), "Name".into(), "ab".into(), None, None, cx)
        })
        .unwrap();
    press(&shell, "backspace", cx);
    type_text(&shell, "c", cx);
    press(&shell, "left x", cx);
    press(&shell, "enter", cx);
    assert_eq!(input.try_recv(), Ok(TransientOutcome::Input("axc".into())));

    let (mut input, _) = shell
        .update(cx, |shell, cx| {
            shell.begin_input(owner(), "Name".into(), String::new(), None, None, cx)
        })
        .unwrap();
    press(&shell, "escape", cx);
    assert_eq!(input.try_recv(), Ok(TransientOutcome::Dismissed));
    // A prompt stays outside the value; Backspace deletes text, then
    // dismisses on an empty field.
    let (mut input, _) = shell
        .update(cx, |shell, cx| {
            shell.begin_input(owner(), "Ex".into(), "a".into(), Some(":".into()), None, cx)
        })
        .unwrap();
    press(&shell, "backspace", cx);
    assert_eq!(
        input.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    );
    type_text(&shell, "b", cx);
    press(&shell, "enter", cx);
    assert_eq!(input.try_recv(), Ok(TransientOutcome::Input("b".into())));
    let (mut input, _) = shell
        .update(cx, |shell, cx| {
            shell.begin_input(
                owner(),
                "Ex".into(),
                String::new(),
                Some(":".into()),
                None,
                cx,
            )
        })
        .unwrap();
    press(&shell, "backspace", cx);
    assert_eq!(input.try_recv(), Ok(TransientOutcome::Dismissed));
    // An input without a prompt keeps Backspace as a plain edit.
    let (mut input, _) = shell
        .update(cx, |shell, cx| {
            shell.begin_input(owner(), "Name".into(), String::new(), None, None, cx)
        })
        .unwrap();
    press(&shell, "backspace", cx);
    assert_eq!(
        input.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    );
    press(&shell, "escape", cx);
    // None of those keys reached the editor behind the interactions.
    press(&shell, "q", cx);
    cx.read(|cx| assert_eq!(model.read(cx).text(), "q"));
}

#[cfg(unix)]
#[gpui::test]
fn terminal_keys_and_text_reach_the_program(cx: &mut TestAppContext) {
    use crate::app::terminal::{session::TerminalSession, view::TerminalView};
    use alacritty_terminal::{grid::Dimensions, tty};

    let documents = install_globals(cx);
    let (shell, _) = headless_window(&documents, cx);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("keys.hex");
    let expected = "hi\x1b[Aé\r".as_bytes();
    let script = format!(
        "stty raw -echo; printf ready; dd bs=1 count={} 2>/dev/null | od -An -tx1 > '{}'",
        expected.len(),
        destination.display()
    );
    let session = cx.new(|cx| {
        TerminalSession::new_with_options(
            tty::Options {
                shell: Some(tty::Shell::new("/bin/sh".into(), vec!["-c".into(), script])),
                drain_on_exit: true,
                ..Default::default()
            },
            cx,
        )
    });
    let ready = |session: &TerminalSession| {
        session.terminal().is_some_and(|terminal| {
            let terminal = terminal.lock();
            let grid = terminal.grid();
            (0..grid.columns())
                .map(|column| {
                    grid[alacritty_terminal::index::Line(0)]
                        [alacritty_terminal::index::Column(column)]
                    .c
                })
                .collect::<String>()
                .contains("ready")
        })
    };
    wait_for_api_condition(cx, |cx| ready(session.read(cx)));
    cx.update(|cx| {
        let id = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .allocate_id();
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .sessions
            .insert(id, session.clone());
        let view = cx.new(|cx| TerminalView::new(session.clone(), cx));
        shell.update(cx, |shell, cx| {
            let pane = shell.workbench.read(cx).focused_pane_id().unwrap();
            let tab = shell.workbench.update(cx, |workbench, _| {
                workbench.open_terminal_tab(pane, id, view).unwrap()
            });
            shell.activate_tab(pane, tab, cx);
        });
    });
    press(&shell, "h i up", cx);
    type_text(&shell, "é", cx);
    press(&shell, "enter", cx);
    wait_for_api_condition(cx, |cx| {
        matches!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Exited(_)
        )
    });
    let bytes: Vec<u8> = std::fs::read_to_string(destination)
        .unwrap()
        .split_whitespace()
        .map(|hex| u8::from_str_radix(hex, 16).unwrap())
        .collect();
    assert_eq!(bytes, expected);
}
