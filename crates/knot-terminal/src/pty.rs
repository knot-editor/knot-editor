//! The PTY engine: runs a program on a pseudo-terminal and feeds its output
//! to a [`Terminal`].
//!
//! Three threads serve each engine. The reader parses output into the
//! terminal under its lock. The writer sends input and the terminal's replies
//! to the PTY, so neither the reader nor the caller blocks on a program that
//! stops reading. The waiter reaps the child and reports its exit once the
//! reader has drained the remaining output.

use std::{
    fmt,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::{Terminal, TerminalEvent, TerminalSize};

/// How long the waiter lets the reader drain output after the child exits.
/// Output never ends while a background job keeps the PTY open, nor on
/// ConPTY.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(200);
const READ_BUFFER: usize = 64 * 1024;

/// A program and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shell {
    pub program: String,
    pub args: Vec<String>,
}

impl Shell {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

/// What [`Pty::spawn`] runs, and where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PtyOptions {
    /// The program to run; the user's shell when `None`. On macOS the
    /// user's shell runs as a login shell under `login`.
    pub shell: Option<Shell>,
    /// Where the program starts; the home directory when `None` or not a
    /// directory.
    pub working_directory: Option<PathBuf>,
    /// Variables set on top of the editor's environment.
    pub env: Vec<(String, String)>,
}

/// What the engine reports, from its threads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PtyEvent {
    /// Output changed the terminal.
    Output,
    /// An effect of the output. Replies never appear here: the engine
    /// writes them to the PTY itself, in stream order.
    Terminal(TerminalEvent),
    /// The program exited, after its last output reached the terminal. No
    /// event follows.
    Exited(ExitStatus),
}

/// How a program ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitStatus {
    pub code: u32,
    /// The signal that ended the program, described by the platform.
    pub signal: Option<String>,
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.signal {
            Some(signal) => write!(f, "terminated by {signal}"),
            None => write!(f, "exit code {}", self.code),
        }
    }
}

impl From<portable_pty::ExitStatus> for ExitStatus {
    fn from(status: portable_pty::ExitStatus) -> Self {
        Self {
            code: status.exit_code(),
            signal: status.signal().map(str::to_owned),
        }
    }
}

type Input = Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>;

/// A running program on a PTY, feeding a terminal. Dropping it shuts the
/// program down without waiting for it; the terminal keeps its screen.
pub struct Pty {
    terminal: Arc<Mutex<Terminal>>,
    master: Box<dyn MasterPty + Send>,
    /// The writer thread's queue; taken on shutdown, which ends the thread.
    input: Input,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
    /// Cleared on shutdown; no event is reported afterwards.
    running: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}

impl Pty {
    /// Runs the program of `options` on a PTY of the terminal's size.
    ///
    /// The engine takes over the terminal's event handler and calls
    /// `listener` from its threads; the listener must not lock the terminal,
    /// since terminal events arrive while the reader holds the lock.
    pub fn spawn(
        terminal: Arc<Mutex<Terminal>>,
        options: &PtyOptions,
        listener: impl Fn(PtyEvent) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let size = lock(&terminal).size();
        let pair = native_pty_system()
            .openpty(pty_size(size))
            .map_err(io::Error::other)?;
        #[cfg(unix)]
        enable_utf8_input(&*pair.master);
        let mut reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
        let mut writer = writer(&*pair.master)?;
        let mut child = pair
            .slave
            .spawn_command(command(options))
            .map_err(io::Error::other)?;
        // The child holds the only other end, so output ends when it exits.
        drop(pair.slave);
        let killer = child.clone_killer();
        let pid = child.process_id();

        let running = Arc::new(AtomicBool::new(true));
        let exited = Arc::new(AtomicBool::new(false));
        let emit = Arc::new({
            let running = running.clone();
            move |event| {
                if running.load(Ordering::Acquire) {
                    listener(event);
                }
            }
        });
        let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>();
        let input: Input = Arc::new(Mutex::new(Some(input_tx)));
        lock(&terminal).set_event_handler({
            let input = input.clone();
            let emit = emit.clone();
            move |event| match event {
                TerminalEvent::Reply(bytes) => send(&input, bytes),
                event => emit(PtyEvent::Terminal(event)),
            }
        });

        thread::Builder::new()
            .name("terminal writer".into())
            .spawn(move || {
                for bytes in input_rx {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        let (drained_tx, drained_rx) = mpsc::channel::<()>();
        thread::Builder::new()
            .name("terminal reader".into())
            .spawn({
                let terminal = terminal.clone();
                let running = running.clone();
                let emit = emit.clone();
                move || {
                    let mut buffer = vec![0; READ_BUFFER];
                    loop {
                        let count = match reader.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(count) => count,
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                            Err(_) => break,
                        };
                        if !running.load(Ordering::Acquire) {
                            break;
                        }
                        lock(&terminal).write(&buffer[..count]);
                        emit(PtyEvent::Output);
                    }
                    drop(drained_tx);
                }
            })?;

        thread::Builder::new()
            .name("terminal waiter".into())
            .spawn({
                let exited = exited.clone();
                move || {
                    let status = child.wait();
                    exited.store(true, Ordering::Release);
                    let _ = drained_rx.recv_timeout(DRAIN_TIMEOUT);
                    let status = status.map_or(
                        ExitStatus {
                            code: 1,
                            signal: None,
                        },
                        ExitStatus::from,
                    );
                    emit(PtyEvent::Exited(status));
                }
            })?;

        Ok(Self {
            terminal,
            master: pair.master,
            input,
            killer,
            pid,
            running,
            exited,
        })
    }

    /// The terminal the program's output feeds.
    pub fn terminal(&self) -> &Arc<Mutex<Terminal>> {
        &self.terminal
    }

    /// The child's process id, where the platform has one.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Sends `bytes` to the program's input.
    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        let bytes = bytes.into();
        if !bytes.is_empty() {
            send(&self.input, bytes);
        }
    }

    /// Resizes the terminal, then the PTY, which tells the program.
    pub fn resize(&self, size: TerminalSize) {
        let size = {
            let mut terminal = lock(&self.terminal);
            terminal.resize(size);
            terminal.size()
        };
        let _ = self.master.resize(pty_size(size));
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.input.lock().expect("terminal input poisoned").take();
        if self.exited.load(Ordering::Acquire) {
            return;
        }
        // Closing the PTY on exit ends a running shell but not one still
        // starting: on macOS its open of the terminal then blocks forever.
        // The child is `login`, which survives a hangup and may not have
        // started the shell yet, so kill its process group now. A shell that
        // started has its own process group by then and hangs up when the
        // PTY closes.
        #[cfg(target_os = "macos")]
        if let Some(pid) = self.pid {
            // SAFETY: signalling a process group has no memory effects.
            unsafe {
                libc::killpg(pid as libc::pid_t, libc::SIGKILL);
            }
        }
        // A hangup on Unix; termination on Windows. The waiter reaps the
        // child, and the PTY closes when `master` drops.
        let _ = self.killer.kill();
    }
}

fn lock(terminal: &Mutex<Terminal>) -> std::sync::MutexGuard<'_, Terminal> {
    terminal.lock().expect("terminal poisoned")
}

fn send(input: &Input, bytes: Vec<u8>) {
    if let Some(input) = input.lock().expect("terminal input poisoned").as_ref() {
        let _ = input.send(bytes);
    }
}

fn pty_size(size: TerminalSize) -> PtySize {
    let pixels = |cells: u16, cell: u32| (u32::from(cells) * cell).min(u16::MAX.into()) as u16;
    PtySize {
        rows: size.rows,
        cols: size.columns,
        pixel_width: pixels(size.columns, size.cell_width),
        pixel_height: pixels(size.rows, size.cell_height),
    }
}

fn command(options: &PtyOptions) -> CommandBuilder {
    let mut command = match &options.shell {
        Some(shell) => {
            let mut command = CommandBuilder::new(&shell.program);
            command.args(&shell.args);
            command
        }
        None => default_shell(),
    };
    if let Some(directory) = &options.working_directory {
        command.cwd(directory);
    }
    // Startup notification belongs to the editor, not to programs it runs.
    command.env_remove("XDG_ACTIVATION_TOKEN");
    command.env_remove("DESKTOP_STARTUP_ID");
    // The editor's own environment may come from a shell that exported
    // these. zsh picks its vi keymap from them before reading any startup
    // file, so a shell here would edit differently than in a terminal
    // started from the desktop; its startup files set them again.
    command.env_remove("EDITOR");
    command.env_remove("VISUAL");
    for (name, value) in &options.env {
        command.env(name, value);
    }
    command
}

/// The user's shell as a login shell under `login`, so that it is a login
/// session on the terminal.
#[cfg(target_os = "macos")]
fn default_shell() -> CommandBuilder {
    let shell = CommandBuilder::new_default_prog().get_shell();
    let name = shell.rsplit('/').next().unwrap_or(&shell);
    // `login` looks for `.hushlogin` only in the working directory, so
    // check the home directory here and pass `-q` for it.
    let hushlogin = std::env::var_os("HOME")
        .is_some_and(|home| PathBuf::from(home).join(".hushlogin").exists());
    let mut command = CommandBuilder::new("/usr/bin/login");
    // -f skips authentication for the logged-in user, -l keeps the
    // directory and argv[0], -p keeps the environment, and -q acts as if
    // `.hushlogin` exists. Since -l keeps argv[0], zsh's `exec -a` gives
    // the shell the leading `-` of a login shell.
    command.args([
        if hushlogin { "-qflp" } else { "-flp" },
        &user_name(),
        "/bin/zsh",
        "-fc",
        &format!("exec -a -{name} {shell}"),
    ]);
    command
}

#[cfg(target_os = "macos")]
fn user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| {
        // SAFETY: the entry is read before any other passwd call.
        unsafe {
            let entry = libc::getpwuid(libc::getuid());
            if entry.is_null() {
                return String::new();
            }
            std::ffi::CStr::from_ptr((*entry).pw_name)
                .to_string_lossy()
                .into_owned()
        }
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_shell() -> CommandBuilder {
    CommandBuilder::new(CommandBuilder::new_default_prog().get_shell())
}

#[cfg(windows)]
fn default_shell() -> CommandBuilder {
    CommandBuilder::new("powershell")
}

/// Marks the PTY's input as UTF-8, so the line discipline erases whole
/// characters.
#[cfg(unix)]
fn enable_utf8_input(master: &dyn MasterPty) {
    let Some(fd) = master.as_raw_fd() else {
        return;
    };
    // SAFETY: `fd` is the open master and `termios` a valid buffer.
    unsafe {
        let mut termios = std::mem::zeroed::<libc::termios>();
        if libc::tcgetattr(fd, &mut termios) == 0 {
            termios.c_iflag |= libc::IUTF8;
            libc::tcsetattr(fd, libc::TCSANOW, &termios);
        }
    }
}

/// Writes to the PTY through a duplicate of its descriptor: portable-pty's
/// own writer sends a newline and EOF when dropped, which would run a
/// half-typed command as the shell shuts down.
#[cfg(unix)]
fn writer(master: &dyn MasterPty) -> io::Result<Box<dyn Write + Send>> {
    use std::os::fd::BorrowedFd;

    let fd = master
        .as_raw_fd()
        .ok_or_else(|| io::Error::other("the PTY has no descriptor"))?;
    // SAFETY: `master` keeps the descriptor open for this call.
    let fd = unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?;
    Ok(Box::new(std::fs::File::from(fd)))
}

#[cfg(windows)]
fn writer(master: &dyn MasterPty) -> io::Result<Box<dyn Write + Send>> {
    master.take_writer().map_err(io::Error::other)
}
