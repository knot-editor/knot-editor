//! Running programs for packages that declare the `process` entitlement.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures::{FutureExt as _, StreamExt as _, select_biased};
use knot_model::extensions::package::Entitlement;
use knot_process::{ExitStatus, Process, ProcessSpec, SpawnError};

use super::*;
use crate::app::extensions::process::{
    ApplicationProcessEnvironment, OwnedProcess, ProcessId, inherited_environment,
};

/// How long `run` keeps collecting output after the process exited, when a
/// helper that outlived it holds the pipes open.
const OUTPUT_IDLE_AFTER_EXIT: Duration = Duration::from_millis(500);

/// Failures of the process operations.
#[derive(Debug, HostError)]
pub(crate) enum ProcessError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "EntitlementNotDeclared", message = "EntitlementNotDeclared")]
    EntitlementNotDeclared,
    #[error(code = "ProgramNotFound", message = "ProgramNotFound")]
    ProgramNotFound,
    #[error(
        code = "WorkingDirectoryNotFound",
        message = "WorkingDirectoryNotFound"
    )]
    WorkingDirectoryNotFound,
    #[error(code = "ProcessFailed", message = "ProcessFailed")]
    ProcessFailed,
    #[error(code = "ProcessInputClosed", message = "ProcessInputClosed")]
    ProcessInputClosed,
    #[error(code = "InvalidUri", message = "InvalidUri")]
    InvalidUri,
}

impl From<AccessError> for ProcessError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

impl From<SpawnError> for ProcessError {
    fn from(error: SpawnError) -> Self {
        match error {
            SpawnError::CommandNotFound(_) => Self::ProgramNotFound,
            SpawnError::DirectoryNotFound(_) => Self::WorkingDirectoryNotFound,
            SpawnError::Io(error) => {
                // `resolve_command` screens for this on Unix, but the program
                // can vanish between the check and the spawn, and other
                // platforms search `PATH` in the spawn itself.
                if error.kind() == std::io::ErrorKind::NotFound {
                    Self::ProgramNotFound
                } else {
                    tracing::warn!(target: "knot::process", "cannot start a process: {error}");
                    Self::ProcessFailed
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessArguments {
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
    /// `NAME=value` entries merged over the inherited environment.
    env: Vec<String>,
    /// Names removed from the inherited environment.
    unset: Vec<String>,
    stdin: Option<String>,
}

impl HostArguments for ProcessArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            command: reader.text(2, "process command must be text")?,
            args: reader.text_list(3)?,
            cwd: reader.optional_text(4, "process cwd must be text")?,
            env: reader.text_list(5)?,
            unset: reader.text_list(6)?,
            stdin: reader.optional_text(7, "process stdin must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessReference {
    process: u64,
}

impl HostArguments for ProcessReference {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            process: reader.integer(2)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessStreamArguments {
    process: u64,
    stream: String,
}

impl HostArguments for ProcessStreamArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            process: reader.integer(2)?,
            stream: reader.text(3, "process stream must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessWriteArguments {
    process: u64,
    text: String,
}

impl HostArguments for ProcessWriteArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            process: reader.integer(2)?,
            text: reader.text(3, "process input must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessKillArguments {
    process: u64,
    signal: Option<String>,
}

impl HostArguments for ProcessKillArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            process: reader.integer(2)?,
            signal: reader.optional_text(3, "process signal must be text")?,
        })
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessExit {
    exit_code: Option<i32>,
    signal: Option<String>,
}

impl From<ExitStatus> for ProcessExit {
    fn from(status: ExitStatus) -> Self {
        Self {
            exit_code: status.code,
            signal: status.signal,
        }
    }
}

impl HostSuccess for ProcessExit {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessRun {
    exit_code: Option<i32>,
    signal: Option<String>,
    stdout: String,
    stderr: String,
}

impl HostSuccess for ProcessRun {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

fn caller(context: &HostContext) -> ExtensionKey {
    ExtensionKey::new(context.request.extension, context.request.lifecycle)
}

fn working_directory(cwd: Option<&str>) -> Result<PathBuf, ProcessError> {
    match cwd {
        Some(uri) => knot_model::resource::ResourceUri::parse(uri)
            .ok()
            .and_then(|uri| uri.as_url().to_file_path().ok())
            .ok_or(ProcessError::InvalidUri),
        None => Ok(directories::BaseDirs::new()
            .map(|base| base.home_dir().to_owned())
            .unwrap_or_else(std::env::temp_dir)),
    }
}

/// Starts the process `arguments` describe for the caller, if it declared the
/// entitlement.
async fn start(
    context: &HostContext,
    arguments: &ProcessArguments,
) -> Result<(ProcessId, OwnedProcess), ProcessError> {
    let key = caller(context);
    let inherited = context.foreground(|host, cx| {
        host.holds_entitlement(key, Entitlement::Process).then(|| {
            cx.try_global::<ApplicationProcessEnvironment>()
                .map(|environment| environment.0.clone())
        })
    })?;
    let Some(inherited) = inherited else {
        return Err(ProcessError::EntitlementNotDeclared);
    };
    let base = inherited_environment(inherited).await;
    let mut overrides: Vec<(String, Option<String>)> = arguments
        .unset
        .iter()
        .map(|name| (name.clone(), None))
        .collect();
    overrides.extend(arguments.env.iter().filter_map(|entry| {
        let (name, value) = entry.split_once('=')?;
        Some((name.to_owned(), Some(value.to_owned())))
    }));
    let spec = ProcessSpec {
        command: arguments.command.clone(),
        args: arguments.args.clone(),
        cwd: working_directory(arguments.cwd.as_deref())?,
        environment: knot_process::merge_environment(&base, &overrides),
    };
    context.foreground(|host, _| {
        let process = Process::spawn(&spec)?;
        Ok::<_, ProcessError>(host.processes.insert(key, process))
    })?
}

/// The process `reference` names, when the caller owns it.
fn owned(context: &HostContext, reference: u64) -> Result<OwnedProcess, ProcessError> {
    let key = caller(context);
    context
        .foreground(|host, _| host.processes.get(ProcessId(reference), key))?
        .ok_or(ProcessError::Access(AccessError::InvalidTarget))
}

#[host_operation("runProcess")]
async fn run_process(
    context: HostContext,
    arguments: ProcessArguments,
) -> Result<ProcessRun, ProcessError> {
    let (id, process) = start(&context, &arguments).await?;
    if let Some(text) = arguments.stdin {
        process.control.write(text);
    }
    process.control.close_input();
    let executor = context.foreground(|_, cx| cx.background_executor().clone())?;
    let mut stdout = process.stdout.lock().await;
    let mut stderr = process.stderr.lock().await;
    let mut output = String::new();
    let mut errors = String::new();
    let (mut out_open, mut err_open) = (true, true);
    let mut status = None;
    let mut exit = process.exit.clone().fuse();
    while out_open || err_open || status.is_none() {
        let exited = status.is_some();
        let idle = async {
            if exited {
                executor.timer(OUTPUT_IDLE_AFTER_EXIT).await;
            } else {
                std::future::pending::<()>().await;
            }
        }
        .fuse();
        let next_out = async {
            if out_open {
                stdout.next().await
            } else {
                std::future::pending().await
            }
        }
        .fuse();
        let next_err = async {
            if err_open {
                stderr.next().await
            } else {
                std::future::pending().await
            }
        }
        .fuse();
        futures::pin_mut!(idle, next_out, next_err);
        select_biased! {
            chunk = next_out => match chunk {
                Some(chunk) => output.push_str(&chunk),
                None => out_open = false,
            },
            chunk = next_err => match chunk {
                Some(chunk) => errors.push_str(&chunk),
                None => err_open = false,
            },
            finished = exit => status = Some(finished),
            () = idle => break,
        }
    }
    drop((stdout, stderr));
    let _ = context.foreground(|host, _| host.processes.remove(id));
    let status = status.unwrap_or(ExitStatus {
        code: None,
        signal: None,
    });
    Ok(ProcessRun {
        exit_code: status.code,
        signal: status.signal,
        stdout: output,
        stderr: errors,
    })
}

#[host_operation("spawnProcess")]
async fn spawn_process(
    context: HostContext,
    arguments: ProcessArguments,
) -> Result<u64, ProcessError> {
    let (ProcessId(id), _) = start(&context, &arguments).await?;
    Ok(id)
}

/// The next chunk of a stream, or `None` once it ended. The child reads ahead
/// only as chunks are taken, so a listener's pace is the child's.
#[host_operation("readProcessOutput")]
async fn read_process_output(
    context: HostContext,
    arguments: ProcessStreamArguments,
) -> Result<Option<String>, ProcessError> {
    let process = owned(&context, arguments.process)?;
    let (stream, ended) = match arguments.stream.as_str() {
        "stdout" => (process.stdout.clone(), process.stdout_ended.clone()),
        "stderr" => (process.stderr.clone(), process.stderr_ended.clone()),
        _ => return Err(AccessError::InvalidTarget.into()),
    };
    let chunk = stream.lock().await.next().await;
    if chunk.is_none() {
        // A stream read to its end after the exit may be the last thing the
        // entry is kept for; `wait_process` covers the other order.
        ended.store(true, Ordering::Release);
        let id = ProcessId(arguments.process);
        let _ = context.foreground(|host, _| host.processes.reap_if_finished(id, &process));
    }
    Ok(chunk)
}

#[host_operation("writeProcessInput")]
async fn write_process_input(
    context: HostContext,
    arguments: ProcessWriteArguments,
) -> Result<(), ProcessError> {
    let process = owned(&context, arguments.process)?;
    if process.control.write(arguments.text) {
        Ok(())
    } else {
        Err(ProcessError::ProcessInputClosed)
    }
}

#[host_operation("closeProcessInput")]
async fn close_process_input(
    context: HostContext,
    arguments: ProcessReference,
) -> Result<(), ProcessError> {
    owned(&context, arguments.process)?.control.close_input();
    Ok(())
}

#[host_operation("killProcess")]
async fn kill_process(
    context: HostContext,
    arguments: ProcessKillArguments,
) -> Result<(), ProcessError> {
    let process = owned(&context, arguments.process)?;
    let signal = knot_process::signal_number(arguments.signal.as_deref().unwrap_or("SIGTERM"))
        .ok_or(AccessError::InvalidTarget)?;
    process.control.kill(signal);
    Ok(())
}

#[host_operation("waitProcess")]
async fn wait_process(
    context: HostContext,
    arguments: ProcessReference,
) -> Result<ProcessExit, ProcessError> {
    let process = owned(&context, arguments.process)?;
    let status = process.exit.clone().await;
    process.control.close_input();
    // An exit delivered after both streams drained may be the last thing the
    // entry is kept for; `read_process_output` covers the other order.
    let id = ProcessId(arguments.process);
    let _ = context.foreground(|host, _| host.processes.reap_if_finished(id, &process));
    Ok(status.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spawn_failure_to_find_the_program_reports_program_not_found() {
        let missing = ProcessError::from(SpawnError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "gone",
        )));
        assert!(matches!(missing, ProcessError::ProgramNotFound));
        let failed = ProcessError::from(SpawnError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        )));
        assert!(matches!(failed, ProcessError::ProcessFailed));
    }
}
