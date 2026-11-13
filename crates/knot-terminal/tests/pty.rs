//! Runs real programs through the PTY engine.
#![cfg(unix)]

use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use knot_terminal::{
    ExitStatus, Pty, PtyEvent, PtyOptions, Shell, Terminal, TerminalEvent, TerminalSize,
};

const TIMEOUT: Duration = Duration::from_secs(10);

fn size(columns: u16, rows: u16) -> TerminalSize {
    TerminalSize {
        columns,
        rows,
        cell_width: 8,
        cell_height: 16,
    }
}

fn sh(script: Option<&str>) -> PtyOptions {
    PtyOptions {
        shell: Some(Shell::new(
            "/bin/sh",
            script.map_or_else(Vec::new, |script| vec!["-c".into(), script.into()]),
        )),
        ..PtyOptions::default()
    }
}

fn spawn(options: &PtyOptions) -> (Pty, mpsc::Receiver<PtyEvent>) {
    let terminal = Arc::new(Mutex::new(Terminal::new(size(80, 24), 1_000)));
    let (events, receiver) = mpsc::channel();
    let pty = Pty::spawn(terminal, options, move |event| {
        let _ = events.send(event);
    })
    .expect("spawning the program");
    (pty, receiver)
}

fn text(pty: &Pty) -> String {
    pty.terminal().lock().unwrap().text()
}

/// Waits until a line of the terminal's text satisfies `found`.
fn wait_for_line(pty: &Pty, found: impl Fn(&str) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if text(pty).lines().any(|line| found(line.trim())) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out; the terminal shows:\n{}", text(pty));
}

fn wait_for_exit(events: &mpsc::Receiver<PtyEvent>) -> ExitStatus {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(remaining).expect("the program exits") {
            PtyEvent::Exited(status) => return status,
            PtyEvent::Output | PtyEvent::Terminal(_) => {}
        }
    }
}

#[test]
fn output_is_on_the_screen_when_the_exit_is_reported() {
    let (pty, events) = spawn(&sh(Some("printf 'one\\ntwo\\n'; exit 3")));
    assert_eq!(
        wait_for_exit(&events),
        ExitStatus {
            code: 3,
            signal: None
        }
    );
    assert_eq!(text(&pty), "one\ntwo");
}

#[test]
fn terminal_events_other_than_replies_reach_the_listener() {
    let (_pty, events) = spawn(&sh(Some("printf '\\033]2;done\\007\\007'")));
    let mut seen = Vec::new();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(remaining).expect("the program exits") {
            PtyEvent::Exited(_) => break,
            PtyEvent::Terminal(event) => seen.push(event),
            PtyEvent::Output => {}
        }
    }
    assert_eq!(
        seen,
        [TerminalEvent::Title("done".into()), TerminalEvent::Bell]
    );
}

#[test]
fn a_signal_ends_the_program() {
    let (_pty, events) = spawn(&sh(Some("kill -KILL $$")));
    let status = wait_for_exit(&events);
    assert!(status.signal.is_some(), "{status:?}");
}

#[test]
fn input_round_trips() {
    let (pty, _events) = spawn(&sh(None));
    pty.write("echo $((6 * 7))\r");
    wait_for_line(&pty, |line| line == "42");
}

#[test]
fn replies_reach_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let reply = directory.path().join("reply");
    let (_pty, events) = spawn(&sh(Some(&format!(
        "stty raw -echo; printf '\\033[5n'; head -c 4 > '{}'",
        reply.display()
    ))));
    assert!(wait_for_exit(&events).signal.is_none());
    assert_eq!(std::fs::read(&reply).unwrap(), b"\x1b[0n");
}

#[test]
fn resizing_reaches_the_program() {
    let (pty, _events) = spawn(&sh(None));
    pty.write("stty size\r");
    wait_for_line(&pty, |line| line == "24 80");

    pty.resize(size(100, 30));
    assert_eq!(pty.terminal().lock().unwrap().size(), size(100, 30));
    pty.write("stty size\r");
    wait_for_line(&pty, |line| line == "30 100");
}

#[test]
fn options_set_the_directory_and_environment() {
    let directory = tempfile::tempdir().unwrap();
    let directory = directory.path().canonicalize().unwrap();
    let mut options = sh(Some("pwd -P; echo \"$KNOT_PTY_TEST\""));
    options.working_directory = Some(directory.clone());
    options.env = vec![("KNOT_PTY_TEST".into(), "set".into())];
    let (pty, events) = spawn(&options);
    wait_for_exit(&events);
    assert_eq!(text(&pty), format!("{}\nset", directory.display()));
}

#[test]
fn dropping_ends_a_shell_that_is_still_starting() {
    let (pty, events) = spawn(&PtyOptions::default());
    let pid = pty.pid().expect("a Unix child has a pid") as libc::pid_t;
    let started = Instant::now();
    drop(pty);
    assert!(started.elapsed() < Duration::from_secs(1));

    // The waiter reaps the child once it is gone.
    let deadline = Instant::now() + TIMEOUT;
    // SAFETY: signal 0 only checks that the process exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(Instant::now() < deadline, "the shell is still running");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Nothing is reported after shutdown, the exit included.
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        events
            .try_iter()
            .all(|event| !matches!(event, PtyEvent::Exited(_))),
    );
}
