//! Child processes with piped, text-only stdio, owned without gpui.
//!
//! A process starts in its own process group. Dedicated threads read its
//! output, write its input, and reap it. Output arrives as text in chunks
//! through bounded channels: a reader thread blocks once a chunk waits
//! unconsumed, so a consumer that stops pulling stalls the child on its pipe
//! instead of growing memory.

use std::{
    fmt,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
};

use futures::{SinkExt as _, channel};

const READ_BUFFER: usize = 64 * 1024;

/// What to run: `command` is resolved through `PATH` in `environment`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessSpec {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// The complete environment of the child.
    pub environment: Vec<(String, String)>,
}

/// Why a process did not start.
#[derive(Debug)]
pub enum SpawnError {
    CommandNotFound(String),
    DirectoryNotFound(PathBuf),
    Io(io::Error),
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandNotFound(command) => write!(f, "command `{command}` was not found"),
            Self::DirectoryNotFound(path) => {
                write!(f, "directory `{}` does not exist", path.display())
            }
            Self::Io(error) => write!(f, "cannot start the process: {error}"),
        }
    }
}

impl std::error::Error for SpawnError {}

/// How a process ended: an exit code, or the signal that ended it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub signal: Option<String>,
}

impl From<std::process::ExitStatus> for ExitStatus {
    fn from(status: std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(&status).map(signal_name);
        #[cfg(not(unix))]
        let signal = None;
        Self {
            code: status.code(),
            signal,
        }
    }
}

/// The name of a signal number, such as `SIGTERM`.
pub fn signal_name(signal: i32) -> String {
    #[cfg(unix)]
    {
        let name = match signal {
            libc::SIGHUP => "SIGHUP",
            libc::SIGINT => "SIGINT",
            libc::SIGQUIT => "SIGQUIT",
            libc::SIGABRT => "SIGABRT",
            libc::SIGKILL => "SIGKILL",
            libc::SIGSEGV => "SIGSEGV",
            libc::SIGPIPE => "SIGPIPE",
            libc::SIGALRM => "SIGALRM",
            libc::SIGTERM => "SIGTERM",
            libc::SIGUSR1 => "SIGUSR1",
            libc::SIGUSR2 => "SIGUSR2",
            _ => return format!("SIG{signal}"),
        };
        name.to_owned()
    }
    #[cfg(not(unix))]
    format!("SIG{signal}")
}

/// The signal number of a name accepted by [`Control::kill`].
pub fn signal_number(name: &str) -> Option<i32> {
    #[cfg(unix)]
    {
        Some(match name {
            "SIGHUP" => libc::SIGHUP,
            "SIGINT" => libc::SIGINT,
            "SIGQUIT" => libc::SIGQUIT,
            "SIGKILL" => libc::SIGKILL,
            "SIGTERM" => libc::SIGTERM,
            "SIGUSR1" => libc::SIGUSR1,
            "SIGUSR2" => libc::SIGUSR2,
            _ => return None,
        })
    }
    #[cfg(not(unix))]
    matches!(name, "SIGKILL" | "SIGTERM").then_some(0)
}

/// The environment `overrides` produce over `base`: a `None` value unsets the
/// variable.
pub fn merge_environment(
    base: &[(String, String)],
    overrides: &[(String, Option<String>)],
) -> Vec<(String, String)> {
    let mut merged = base.to_vec();
    for (name, value) in overrides {
        merged.retain(|(existing, _)| existing != name);
        if let Some(value) = value {
            merged.push((name.clone(), value.clone()));
        }
    }
    merged
}

/// A running process: its output, its exit, and the handle that controls it.
pub struct Process {
    pub stdout: channel::mpsc::Receiver<String>,
    pub stderr: channel::mpsc::Receiver<String>,
    /// Resolves once the process has exited. A process that writes more than
    /// its pipe holds cannot exit before its output is read.
    pub exit: channel::oneshot::Receiver<ExitStatus>,
    pub control: Control,
}

/// Writes to, closes, and signals a process, from any thread.
#[derive(Clone)]
pub struct Control {
    group: u32,
    input: Arc<Mutex<Option<mpsc::Sender<String>>>>,
}

impl Control {
    /// Queues `text` for the process's stdin; false once input is closed.
    pub fn write(&self, text: String) -> bool {
        self.input
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|input| input.send(text).is_ok())
    }

    /// Ends the process's stdin once queued writes are done.
    pub fn close_input(&self) {
        self.input.lock().unwrap().take();
    }

    /// Signals the whole process group.
    pub fn kill(&self, signal: i32) {
        signal_group(self.group, signal);
    }

    /// Ends the whole process group without a chance to clean up.
    pub fn kill_group(&self) {
        Self::kill_process_group(self.group);
    }

    /// Ends the process group led by `group`, which this crate did not
    /// necessarily start.
    pub fn kill_process_group(group: u32) {
        #[cfg(unix)]
        signal_group(group, libc::SIGKILL);
        #[cfg(not(unix))]
        signal_group(group, 0);
    }
}

fn signal_group(group: u32, signal: i32) {
    #[cfg(unix)]
    {
        // SAFETY: killpg only sends a signal.
        unsafe { libc::killpg(group as libc::pid_t, signal) };
    }
    #[cfg(not(unix))]
    {
        let _ = signal;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &group.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Process {
    pub fn spawn(spec: &ProcessSpec) -> Result<Self, SpawnError> {
        if !spec.cwd.is_dir() {
            return Err(SpawnError::DirectoryNotFound(spec.cwd.clone()));
        }
        let program = resolve_command(&spec.command, &spec.environment, &spec.cwd)
            .ok_or_else(|| SpawnError::CommandNotFound(spec.command.clone()))?;
        let mut command = Command::new(program);
        command
            .args(&spec.args)
            .env_clear()
            .envs(spec.environment.iter().map(|(k, v)| (k, v)))
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                SpawnError::CommandNotFound(spec.command.clone())
            } else {
                SpawnError::Io(error)
            }
        })?;
        let group = child.id();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let stdin = child.stdin.take().expect("stdin is piped");

        let (input, queued) = mpsc::channel::<String>();
        thread::spawn(move || write_input(stdin, queued));
        let stdout = read_stream(stdout);
        let stderr = read_stream(stderr);
        let (report, exit) = channel::oneshot::channel();
        thread::spawn(move || {
            let status = child.wait().map(ExitStatus::from).unwrap_or(ExitStatus {
                code: None,
                signal: None,
            });
            let _ = report.send(status);
        });
        Ok(Self {
            stdout,
            stderr,
            exit,
            control: Control {
                group,
                input: Arc::new(Mutex::new(Some(input))),
            },
        })
    }
}

fn write_input(mut stdin: std::process::ChildStdin, queued: mpsc::Receiver<String>) {
    // The sender is dropped to close stdin; a broken pipe ends the thread.
    for text in queued {
        if stdin.write_all(text.as_bytes()).is_err() || stdin.flush().is_err() {
            return;
        }
    }
}

/// Reads `source` on its own thread into a channel that holds one chunk, so
/// the thread blocks until the consumer pulls. Once the consumer is gone the
/// thread discards output, so the child never stalls on a pipe nobody reads.
fn read_stream(mut source: impl Read + Send + 'static) -> channel::mpsc::Receiver<String> {
    let (mut sender, receiver) = channel::mpsc::channel(0);
    thread::spawn(move || {
        let mut buffer = vec![0u8; READ_BUFFER];
        let mut pending: Vec<u8> = Vec::new();
        let mut consumed = false;
        loop {
            let count = match source.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            pending.extend_from_slice(&buffer[..count]);
            let text = take_text(&mut pending);
            if !text.is_empty()
                && !consumed
                && futures::executor::block_on(sender.send(text)).is_err()
            {
                consumed = true;
            }
        }
        if !pending.is_empty() && !consumed {
            let text = String::from_utf8_lossy(&pending).into_owned();
            let _ = futures::executor::block_on(sender.send(text));
        }
    });
    receiver
}

/// Decodes the longest prefix of `pending`, replacing invalid bytes and
/// leaving an incomplete code point at the end for the next chunk.
fn take_text(pending: &mut Vec<u8>) -> String {
    let mut text = String::new();
    let mut offset = 0;
    loop {
        match std::str::from_utf8(&pending[offset..]) {
            Ok(valid) => {
                text.push_str(valid);
                offset = pending.len();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                text.push_str(
                    std::str::from_utf8(&pending[offset..offset + valid]).expect("valid prefix"),
                );
                offset += valid;
                match error.error_len() {
                    Some(length) => {
                        text.push('\u{FFFD}');
                        offset += length;
                    }
                    None => break,
                }
            }
        }
    }
    pending.drain(..offset);
    text
}

/// The executable `command` names, searching `PATH` of `environment` unless
/// the command is a path.
fn resolve_command(command: &str, environment: &[(String, String)], cwd: &Path) -> Option<PathBuf> {
    if command.is_empty() {
        return None;
    }
    #[cfg(not(unix))]
    {
        let _ = (environment, cwd);
        return Some(PathBuf::from(command));
    }
    #[cfg(unix)]
    {
        if command.contains('/') {
            let path = cwd.join(command);
            return is_executable(&path).then_some(path);
        }
        let search = environment
            .iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| value.as_str())?;
        std::env::split_paths(search)
            .map(|directory| directory.join(command))
            .find(|candidate| is_executable(candidate))
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use futures::{StreamExt as _, executor::block_on};

    fn spec(script: &str) -> ProcessSpec {
        ProcessSpec {
            command: "sh".into(),
            args: vec!["-c".into(), script.into()],
            cwd: std::env::temp_dir(),
            environment: vec![("PATH".into(), "/usr/bin:/bin".into())],
        }
    }

    fn collect(mut stream: channel::mpsc::Receiver<String>) -> String {
        block_on(async {
            let mut text = String::new();
            while let Some(chunk) = stream.next().await {
                text.push_str(&chunk);
            }
            text
        })
    }

    #[test]
    fn splits_a_code_point_across_chunks() {
        let mut pending = vec![b'a', 0xE2, 0x82];
        assert_eq!(take_text(&mut pending), "a");
        pending.push(0xAC);
        assert_eq!(take_text(&mut pending), "€");
        let mut invalid = vec![b'x', 0xFF, b'y'];
        assert_eq!(take_text(&mut invalid), "x\u{FFFD}y");
    }

    #[test]
    fn decodes_output_that_splits_a_code_point() {
        let process =
            Process::spawn(&spec(r"printf '\342\202'; sleep 0.2; printf '\254 done'")).unwrap();
        assert_eq!(collect(process.stdout), "€ done");
    }

    #[test]
    fn reports_exit_codes_and_signals() {
        let process = Process::spawn(&spec("exit 3")).unwrap();
        let status = block_on(process.exit).unwrap();
        assert_eq!(status.code, Some(3));
        assert_eq!(status.signal, None);

        let process = Process::spawn(&spec("sleep 30")).unwrap();
        process.control.kill(libc::SIGTERM);
        let status = block_on(process.exit).unwrap();
        assert_eq!(status.code, None);
        assert_eq!(status.signal.as_deref(), Some("SIGTERM"));
    }

    #[test]
    fn round_trips_stdin() {
        let process = Process::spawn(&spec("cat")).unwrap();
        assert!(process.control.write("hello ".into()));
        assert!(process.control.write("world".into()));
        process.control.close_input();
        assert!(!process.control.write("late".into()));
        assert_eq!(collect(process.stdout), "hello world");
        assert_eq!(block_on(process.exit).unwrap().code, Some(0));
    }

    #[test]
    fn keeps_stderr_apart() {
        let process = Process::spawn(&spec("echo out; echo err >&2")).unwrap();
        assert_eq!(collect(process.stderr), "err\n");
        assert_eq!(collect(process.stdout), "out\n");
    }

    #[test]
    fn a_large_output_stalls_the_child_until_pulled() {
        // 8 MiB cannot fit in the pipe and the channel, so the child stays
        // alive until the consumer reads.
        let process = Process::spawn(&spec("head -c 8388608 /dev/zero | tr '\\0' x")).unwrap();
        let mut exit = process.exit;
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(exit.try_recv().unwrap().is_none());
        let text = collect(process.stdout);
        assert_eq!(text.len(), 8 * 1024 * 1024);
        assert_eq!(block_on(exit).unwrap().code, Some(0));
    }

    #[test]
    fn group_kill_reaches_a_grandchild() {
        let marker = tempfile::tempdir().unwrap();
        let pid_file = marker.path().join("pid");
        let script = format!("sleep 60 & echo $! > {}; wait", pid_file.display());
        let process = Process::spawn(&spec(&script)).unwrap();
        let grandchild = loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = text.trim().parse::<i32>()
            {
                break pid;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        process.control.kill_group();
        block_on(process.exit).unwrap();
        for _ in 0..100 {
            // SAFETY: signal 0 only probes for the process.
            if unsafe { libc::kill(grandchild, 0) } != 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the grandchild survived the group kill");
    }

    #[test]
    fn typed_failures_for_a_missing_command_or_directory() {
        let mut missing = spec("true");
        missing.command = "knot-no-such-command".into();
        assert!(matches!(
            Process::spawn(&missing),
            Err(SpawnError::CommandNotFound(_))
        ));
        let mut directory = spec("true");
        directory.cwd = PathBuf::from("/knot/no/such/directory");
        assert!(matches!(
            Process::spawn(&directory),
            Err(SpawnError::DirectoryNotFound(_))
        ));
    }

    #[test]
    fn merges_environment_overrides() {
        let base = vec![
            ("A".to_owned(), "1".to_owned()),
            ("B".to_owned(), "2".to_owned()),
        ];
        let merged = merge_environment(
            &base,
            &[
                ("A".into(), None),
                ("B".into(), Some("3".into())),
                ("C".into(), Some("4".into())),
            ],
        );
        assert_eq!(
            merged,
            vec![("B".into(), "3".into()), ("C".into(), "4".into())]
        );
    }
}
