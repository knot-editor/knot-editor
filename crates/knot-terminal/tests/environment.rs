//! The environment a program inherits from the editor. A test binary of its
//! own, since it changes the process environment.
#![cfg(unix)]

use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use knot_terminal::{Pty, PtyEvent, PtyOptions, Shell, Terminal, TerminalSize};

#[test]
fn programs_do_not_inherit_the_editors_editor_variables() {
    // SAFETY: this binary's only test, and no other thread reads the
    // environment yet.
    unsafe {
        std::env::set_var("EDITOR", "nvim");
        std::env::set_var("VISUAL", "nvim");
        std::env::set_var("KNOT_INHERITED", "kept");
    }
    let size = TerminalSize {
        columns: 80,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    };
    let terminal = Arc::new(Mutex::new(Terminal::new(size, 100)));
    let options = PtyOptions {
        shell: Some(Shell::new(
            "/bin/sh",
            vec![
                "-c".into(),
                "echo \"[${EDITOR-unset}|${VISUAL-unset}|$KNOT_INHERITED|$OWN]\"".into(),
            ],
        )),
        env: vec![("OWN".into(), "set".into())],
        ..PtyOptions::default()
    };
    let (events, receiver) = mpsc::channel();
    let _pty = Pty::spawn(terminal.clone(), &options, move |event| {
        let _ = events.send(event);
    })
    .expect("spawning the program");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let event = receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("the program exits");
        if matches!(event, PtyEvent::Exited(_)) {
            break;
        }
    }
    let text = terminal.lock().unwrap().text();
    assert!(text.contains("[unset|unset|kept|set]"), "{text}");
}
