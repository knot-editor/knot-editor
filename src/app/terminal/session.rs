use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use gpui::{Context, Task};
use knot_terminal::{
    Mode, Pty, PtyEvent, PtyOptions, Shell, Terminal, TerminalEvent, TerminalSize,
};

use super::colors::TerminalPalette;
use crate::app::theme::theme;

/// Rows of history the primary screen keeps.
const SCROLLBACK_LINES: usize = 10_000;

/// The grid before a view first lays the terminal out.
fn initial_size() -> TerminalSize {
    TerminalSize {
        columns: 80,
        rows: 11,
        // The attached view supplies measured pixel dimensions on first layout.
        cell_width: 1,
        cell_height: 1,
    }
}

/// A terminal shared by the PTY's reader thread and the foreground.
pub(crate) type SharedTerminal = Arc<Mutex<Terminal>>;

/// Locks `terminal`. A panic while it was held leaves the screen readable.
pub(crate) fn lock(terminal: &Mutex<Terminal>) -> MutexGuard<'_, Terminal> {
    terminal.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminalStatus {
    Running,
    Exited(String),
    Failed(String),
    Closed,
}

enum TerminalState {
    Running(Pty),
    Stopped {
        terminal: SharedTerminal,
        status: TerminalStatus,
    },
    Failed(String),
    Closed,
}

/// Owns one PTY, terminal, and process independently of any presentation.
pub(crate) struct TerminalSession {
    state: TerminalState,
    size: TerminalSize,
    options: PtyOptions,
    palette: TerminalPalette,
    generation: u64,
    /// The directory the current process last reported, if local.
    reported_directory: Option<PathBuf>,
    _wakeup_task: Option<Task<()>>,
    attachment: Arc<AtomicU64>,
    next_attachment: u64,
    focused: bool,
    mouse_reporting_enabled: bool,
}

pub(crate) struct TerminalAttachment {
    active: Arc<AtomicU64>,
    id: u64,
}

impl Drop for TerminalAttachment {
    fn drop(&mut self) {
        let _ = self
            .active
            .compare_exchange(self.id, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

/// Divider marking where carried output ends and the new session begins.
const RESTART_DIVIDER: &str = "--- terminal restarted ---";

/// Writes `text`, the lines of a previous session, into a fresh terminal,
/// followed by a divider. The cursor ends below the divider, so new shell
/// output continues underneath; lines longer than the grid wrap again.
fn replay(terminal: &mut Terminal, text: &str) {
    if text.is_empty() {
        return;
    }
    let mut bytes = Vec::new();
    for line in text.lines() {
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(RESTART_DIVIDER.as_bytes());
    bytes.extend_from_slice(b"\r\n");
    terminal.write(&bytes);
}

impl TerminalSession {
    pub(crate) fn new(working_directory: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let options = PtyOptions {
            // Tests may exit without dropping a session. A plain shell hangs
            // up with its PTY, whereas the platform login wrapper survives
            // and is orphaned.
            shell: cfg!(test).then(|| {
                Shell::new(
                    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" },
                    Vec::new(),
                )
            }),
            working_directory,
            env: Vec::new(),
        };
        Self::new_with_options(options, cx)
    }

    pub(crate) fn new_with_options(mut options: PtyOptions, cx: &mut Context<Self>) -> Self {
        options.env.extend([
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]);
        let mut this = Self {
            state: TerminalState::Closed,
            size: initial_size(),
            options,
            palette: TerminalPalette::new(theme(cx)),
            generation: 0,
            reported_directory: None,
            _wakeup_task: None,
            attachment: Arc::new(AtomicU64::new(0)),
            next_attachment: 0,
            focused: false,
            mouse_reporting_enabled: true,
        };
        this.start(None, cx);
        this
    }

    /// Starts a process on a fresh terminal that first shows `carried`.
    fn start(&mut self, carried: Option<String>, cx: &mut Context<Self>) {
        crate::app::allow_thread_wakeups(cx);
        self.generation += 1;
        self.reported_directory = None;
        let generation = self.generation;
        let mut terminal = Terminal::new(self.size, SCROLLBACK_LINES);
        self.palette.apply(&mut terminal);
        if let Some(text) = carried {
            replay(&mut terminal, &text);
        }
        // Output only repaints; the other events wait in order for the
        // foreground. One pending wakeup covers any number of them.
        let (wakeups, mut wakeup_rx) = tokio::sync::mpsc::channel(1);
        let events = Arc::new(Mutex::new(Vec::new()));
        let queue = events.clone();
        let listener = move |event| {
            if !matches!(event, PtyEvent::Output) {
                queue
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(event);
            }
            let _ = wakeups.try_send(());
        };
        self.state = match Pty::spawn(Arc::new(Mutex::new(terminal)), &self.options, listener) {
            Ok(pty) => TerminalState::Running(pty),
            Err(error) => TerminalState::Failed(error.to_string()),
        };
        self._wakeup_task = Some(cx.spawn(async move |this, cx| {
            while wakeup_rx.recv().await.is_some() {
                let pending =
                    std::mem::take(&mut *events.lock().unwrap_or_else(PoisonError::into_inner));
                if this
                    .update(cx, |session, cx| {
                        session.apply_events(generation, pending);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn apply_events(&mut self, generation: u64, events: Vec<PtyEvent>) {
        if self.generation != generation {
            return;
        }
        for event in events {
            match event {
                PtyEvent::Exited(status) => self.finish_exit(status.to_string()),
                PtyEvent::Terminal(TerminalEvent::WorkingDirectory(reported)) => {
                    self.reported_directory = knot_terminal::local_directory(&reported);
                }
                // Titles and bells have no product behavior; the engine
                // answers queries itself.
                PtyEvent::Terminal(_) | PtyEvent::Output => {}
            }
        }
    }

    pub(crate) fn mouse_reporting_enabled(&self) -> bool {
        self.mouse_reporting_enabled
    }

    pub(crate) fn toggle_mouse_reporting(&mut self, cx: &mut Context<Self>) {
        self.mouse_reporting_enabled = !self.mouse_reporting_enabled;
        cx.notify();
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the attached view has focus in an active window.
    pub(crate) fn focused(&self) -> bool {
        self.focused
    }

    fn owns_attachment(&self, attachment: &TerminalAttachment) -> bool {
        Arc::ptr_eq(&self.attachment, &attachment.active)
            && self.attachment.load(Ordering::Acquire) == attachment.id
    }

    pub(crate) fn send_input(
        &self,
        attachment: &TerminalAttachment,
        generation: u64,
        bytes: Vec<u8>,
    ) {
        if self.generation == generation && self.owns_attachment(attachment) {
            self.send(bytes);
        }
    }

    pub(crate) fn set_focus(&mut self, attachment: &TerminalAttachment, focused: bool) {
        if !self.owns_attachment(attachment) || self.focused == focused {
            return;
        }
        self.focused = focused;
        if self
            .with_terminal(|terminal| terminal.mode(Mode::FocusEvents))
            .unwrap_or(false)
        {
            self.send(if focused { b"\x1b[I" } else { b"\x1b[O" }.to_vec());
        }
    }

    fn finish_exit(&mut self, status: String) {
        if let TerminalState::Running(pty) = &self.state {
            self.state = TerminalState::Stopped {
                terminal: pty.terminal().clone(),
                status: TerminalStatus::Exited(status),
            };
        }
    }

    pub(crate) fn attach(&mut self) -> Option<TerminalAttachment> {
        if self.attachment.load(Ordering::Acquire) != 0 {
            return None;
        }
        self.next_attachment += 1;
        let id = self.next_attachment;
        self.attachment.store(id, Ordering::Release);
        Some(TerminalAttachment {
            active: self.attachment.clone(),
            id,
        })
    }

    pub(crate) fn terminal(&self) -> Option<&SharedTerminal> {
        match &self.state {
            TerminalState::Running(pty) => Some(pty.terminal()),
            TerminalState::Stopped { terminal, .. } => Some(terminal),
            TerminalState::Failed(_) | TerminalState::Closed => None,
        }
    }

    /// Runs `f` on the locked terminal, if the session has one.
    pub(crate) fn with_terminal<R>(&self, f: impl FnOnce(&mut Terminal) -> R) -> Option<R> {
        self.terminal().map(|terminal| f(&mut lock(terminal)))
    }

    pub(crate) fn status(&self) -> TerminalStatus {
        match &self.state {
            TerminalState::Running(_) => TerminalStatus::Running,
            TerminalState::Stopped { status, .. } => status.clone(),
            TerminalState::Failed(error) => TerminalStatus::Failed(error.clone()),
            TerminalState::Closed => TerminalStatus::Closed,
        }
    }

    pub(crate) fn size(&self) -> TerminalSize {
        self.size
    }

    pub(crate) fn send(&self, bytes: Vec<u8>) {
        if let TerminalState::Running(pty) = &self.state {
            pty.write(bytes);
        }
    }

    pub(crate) fn resize(&mut self, size: TerminalSize) {
        if self.size == size {
            return;
        }
        self.size = size;
        if let TerminalState::Running(pty) = &self.state {
            pty.resize(size);
        }
    }

    /// Makes `palette` the terminal's default colors.
    pub(crate) fn set_palette(&mut self, palette: TerminalPalette) {
        if self.palette != palette {
            self.palette = palette;
            self.with_terminal(|terminal| palette.apply(terminal));
        }
    }

    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self._wakeup_task = None;
        self.state = TerminalState::Closed;
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn working_directory(&self) -> Option<&std::path::Path> {
        self.options.working_directory.as_deref()
    }

    /// The directory the shell last reported, otherwise the one it is in
    /// now where the platform can report it, otherwise the directory the
    /// session started in. A remote directory counts as unreported.
    pub(crate) fn current_directory(&self) -> Option<PathBuf> {
        if let Some(reported) = &self.reported_directory {
            return Some(reported.clone());
        }
        match &self.state {
            TerminalState::Running(pty) => pty.pid().and_then(super::process::working_directory),
            _ => None,
        }
        .or_else(|| self.options.working_directory.clone())
    }

    pub(crate) fn restart(&mut self, cx: &mut Context<Self>) {
        let carried = self.with_terminal(|terminal| terminal.text());
        self.close(cx);
        self.start(carried, cx);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[gpui::test]
    fn an_old_generation_exit_cannot_stop_the_replacement(cx: &mut gpui::TestAppContext) {
        let session = cx.new(|cx| {
            TerminalSession::new_with_options(
                PtyOptions {
                    shell: Some(Shell::new("/bin/sh", Vec::new())),
                    ..PtyOptions::default()
                },
                cx,
            )
        });
        session.update(cx, |session, cx| {
            let generation = session.generation;
            session.restart(cx);
            assert_eq!(session.status(), TerminalStatus::Running);
            session.apply_events(
                generation,
                vec![PtyEvent::Exited(knot_terminal::ExitStatus {
                    code: 0,
                    signal: None,
                })],
            );
            assert_eq!(session.status(), TerminalStatus::Running);
            session.close(cx);
        });
    }
}
