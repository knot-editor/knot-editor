//! The environment extension processes inherit, and the processes a lifecycle
//! owns.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{FutureExt as _, future::Shared};
use gpui::{App, Global};
use knot_process::{Control, ExitStatus, Process};
use knot_protocol::ExtensionKey;

/// How long startup waits for the login shell to print its environment.
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);
const ENVIRONMENT_MARK: &str = "__KNOT_ENVIRONMENT__";

pub(crate) type Environment = Arc<Vec<(String, String)>>;

/// The environment processes inherit, once it is resolved.
pub(crate) struct ApplicationProcessEnvironment(pub Shared<gpui::Task<Environment>>);

impl Global for ApplicationProcessEnvironment {}

/// Starts resolving the environment off the foreground. On macOS and Linux
/// it is the one the user's login shell builds, which a Finder launch lacks;
/// it falls back to Knot's own environment, logging why.
pub(crate) fn start_environment_resolution(cx: &mut App) {
    let task = cx.background_executor().spawn(async {
        Arc::new(resolve_environment(
            std::env::var("SHELL").ok().as_deref(),
            SHELL_TIMEOUT,
        ))
    });
    cx.set_global(ApplicationProcessEnvironment(task.shared()));
}

/// The environment to give a process: resolved at startup, or Knot's own when
/// no resolution was started.
pub(crate) async fn inherited_environment(
    shared: Option<Shared<gpui::Task<Environment>>>,
) -> Environment {
    match shared {
        Some(shared) => shared.await,
        None => Arc::new(own_environment()),
    }
}

fn own_environment() -> Vec<(String, String)> {
    std::env::vars().collect()
}

fn resolve_environment(shell: Option<&str>, timeout: Duration) -> Vec<(String, String)> {
    if cfg!(windows) {
        return own_environment();
    }
    let Some(shell) = shell.filter(|shell| !shell.is_empty()) else {
        tracing::warn!(target: "knot::process", "no $SHELL; extensions inherit Knot's environment");
        return own_environment();
    };
    match login_shell_environment(shell, timeout) {
        Ok(environment) => environment,
        Err(cause) => {
            tracing::warn!(
                target: "knot::process",
                "cannot read the environment of {shell}: {cause}; extensions inherit Knot's environment"
            );
            own_environment()
        }
    }
}

/// Asks the user's shell what environment it builds, by running it once and
/// reading what `env` prints.
///
/// A program launched from Finder or the Dock gets a minimal environment, not
/// the one of a terminal: the `PATH` entries from Homebrew, rustup or nvm and
/// the variables from `.zprofile` and `.zshrc` are missing, so the processes
/// Knot starts could not find their tools. Only a shell can run those files.
/// It is `-l` to read `.zprofile` and `-i` to read `.zshrc`, where many people
/// keep their `PATH` setup.
fn login_shell_environment(
    shell: &str,
    timeout: Duration,
) -> Result<Vec<(String, String)>, String> {
    use std::io::Read as _;
    use std::process::{Command, Stdio};

    let script = format!("printf %s {ENVIRONMENT_MARK}; command env -0 2>/dev/null || command env");
    let mut command = Command::new(shell);
    command
        .args(["-l", "-i", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Run the shell in a new session, so it has no terminal. An interactive
    // shell makes itself the terminal's foreground group and leaves a dead one
    // behind when it exits, after which Ctrl-C in a terminal Knot was started
    // from reaches nobody. As session leader the shell is also its own group
    // leader, so the timeout can kill it and its children by pid.
    #[cfg(unix)]
    {
        // SAFETY: the closure runs between fork and exec and only calls
        // `setsid`, which is async-signal-safe.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut command, || {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let group = child.id();
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        let _ = sender.send(output);
    });
    let output = match receiver.recv_timeout(timeout) {
        Ok(output) => output,
        Err(_) => {
            Control::kill_process_group(group);
            let _ = child.wait();
            return Err(format!("no answer within {}s", timeout.as_secs()));
        }
    };
    let status = child.wait().map_err(|error| error.to_string())?;
    let output = String::from_utf8_lossy(&output);
    let environment = parse_environment(&output)
        .ok_or_else(|| format!("unexpected output (exit status {status})"))?;
    if environment.is_empty() {
        return Err("empty environment".into());
    }
    Ok(environment)
}

/// The variables after the marker, from NUL-separated or newline-separated
/// `env` output.
fn parse_environment(output: &str) -> Option<Vec<(String, String)>> {
    let body = &output[output.find(ENVIRONMENT_MARK)? + ENVIRONMENT_MARK.len()..];
    let mut entries: Vec<String> = Vec::new();
    if body.contains('\0') {
        entries.extend(body.split('\0').map(str::to_owned));
    } else {
        for line in body.lines() {
            let starts_entry = line
                .split_once('=')
                .is_some_and(|(name, _)| is_variable_name(name));
            match entries.last_mut() {
                Some(previous) if !starts_entry => {
                    previous.push('\n');
                    previous.push_str(line);
                }
                _ => entries.push(line.to_owned()),
            }
        }
    }
    Some(
        entries
            .into_iter()
            .filter_map(|entry| {
                let (name, value) = entry.split_once('=')?;
                (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
            })
            .collect(),
    )
}

fn is_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Identity of a process within the host.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProcessId(pub u64);

pub(crate) type Stream = Arc<futures::lock::Mutex<futures::channel::mpsc::Receiver<String>>>;

/// A running process owned by one lifecycle.
#[derive(Clone)]
pub(crate) struct OwnedProcess {
    pub owner: ExtensionKey,
    pub control: Control,
    pub stdout: Stream,
    pub stderr: Stream,
    pub exit: Shared<futures::future::BoxFuture<'static, ExitStatus>>,
    /// Whether each output stream was read to its end, shared by every clone.
    pub stdout_ended: Arc<AtomicBool>,
    pub stderr_ended: Arc<AtomicBool>,
}

/// Every live process, by identity. Dropping the registry's entries for a
/// lifecycle kills their process groups.
#[derive(Default)]
pub(crate) struct Processes {
    next: u64,
    live: HashMap<ProcessId, OwnedProcess>,
}

impl Processes {
    pub(crate) fn insert(
        &mut self,
        owner: ExtensionKey,
        process: Process,
    ) -> (ProcessId, OwnedProcess) {
        self.next += 1;
        let id = ProcessId(self.next);
        let Process {
            stdout,
            stderr,
            exit,
            control,
        } = process;
        let exit = exit
            .map(|status| {
                status.unwrap_or(ExitStatus {
                    code: None,
                    signal: None,
                })
            })
            .boxed()
            .shared();
        let owned = OwnedProcess {
            owner,
            control,
            stdout: Arc::new(futures::lock::Mutex::new(stdout)),
            stderr: Arc::new(futures::lock::Mutex::new(stderr)),
            exit,
            stdout_ended: Arc::new(AtomicBool::new(false)),
            stderr_ended: Arc::new(AtomicBool::new(false)),
        };
        self.live.insert(id, owned.clone());
        (id, owned)
    }

    /// The process `id` when `owner` owns it.
    pub(crate) fn get(&self, id: ProcessId, owner: ExtensionKey) -> Option<OwnedProcess> {
        self.live
            .get(&id)
            .filter(|process| process.owner == owner)
            .cloned()
    }

    pub(crate) fn remove(&mut self, id: ProcessId) {
        self.live.remove(&id);
    }

    /// Drops `id` once nothing remains to read: its exit is in and both
    /// streams were read to their ends, so no listener, present or future,
    /// could read more. Anything unread keeps the entry until the lifecycle
    /// ends, since output that arrived before a listener attaches is held
    /// until one does.
    pub(crate) fn reap_if_finished(&mut self, id: ProcessId, process: &OwnedProcess) {
        let drained = process.stdout_ended.load(Ordering::Acquire)
            && process.stderr_ended.load(Ordering::Acquire);
        let exited = process.exit.clone().now_or_never().is_some();
        if drained && exited {
            self.live.remove(&id);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.live.len()
    }

    /// Kills every process, with their helpers.
    pub(crate) fn kill_all(&mut self) {
        for (_, process) in self.live.drain() {
            process.control.kill_group();
        }
    }

    /// Kills every process of `owner`, with their helpers.
    pub(crate) fn kill_owned_by(&mut self, owner: ExtensionKey) {
        self.live.retain(|_, process| {
            if process.owner == owner {
                process.control.kill_group();
                false
            } else {
                true
            }
        });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn parses_nul_and_newline_environments() {
        let nul = format!("noise{ENVIRONMENT_MARK}A=1\0B=two words\0");
        assert_eq!(
            parse_environment(&nul).unwrap(),
            vec![("A".into(), "1".into()), ("B".into(), "two words".into())]
        );
        let lines = format!("{ENVIRONMENT_MARK}A=1\nB=x\ny\nC=3\n");
        assert_eq!(
            parse_environment(&lines).unwrap(),
            vec![
                ("A".into(), "1".into()),
                ("B".into(), "x\ny".into()),
                ("C".into(), "3".into())
            ]
        );
        assert!(parse_environment("no marker").is_none());
    }

    #[test]
    fn reads_the_environment_a_shell_builds() {
        let directory = tempfile::tempdir().unwrap();
        let shell = directory.path().join("shell");
        std::fs::write(
            &shell,
            "#!/bin/sh\nshift 3\nKNOT_FROM_SHELL=yes\nexport KNOT_FROM_SHELL\neval \"$1\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let environment =
            login_shell_environment(shell.to_str().unwrap(), Duration::from_secs(10)).unwrap();
        assert!(
            environment
                .iter()
                .any(|(name, value)| name == "KNOT_FROM_SHELL" && value == "yes")
        );
    }

    #[test]
    fn the_shell_leads_its_own_group() {
        // The timeout kills the shell's process group by the shell's pid, and
        // the shell runs in its own session so it cannot take over Knot's
        // terminal; see `login_shell_environment`.
        let directory = tempfile::tempdir().unwrap();
        let shell = directory.path().join("shell");
        std::fs::write(
            &shell,
            "#!/bin/sh\nshift 3\nPGID=$(ps -o pgid= -p $$ | tr -d ' ')\nexport PGID\nPID=$$\nexport PID\neval \"$1\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let environment =
            login_shell_environment(shell.to_str().unwrap(), Duration::from_secs(10)).unwrap();
        let get = |name: &str| {
            environment
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap()
        };
        assert_eq!(get("PGID"), get("PID"));
    }

    #[test]
    fn falls_back_when_the_shell_does_not_answer() {
        let directory = tempfile::tempdir().unwrap();
        let shell = directory.path().join("shell");
        std::fs::write(&shell, "#!/bin/sh\nsleep 30\n").unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = std::time::Instant::now();
        let error = login_shell_environment(shell.to_str().unwrap(), Duration::from_millis(200))
            .unwrap_err();
        assert!(error.contains("no answer"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
        let environment = resolve_environment(Some("/knot/no/shell"), Duration::from_millis(200));
        assert_eq!(environment, own_environment());
    }
}
