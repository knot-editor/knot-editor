//! The application log: `tracing` records, written to a rotating file in the
//! platform log directory and to stderr when attached to a terminal.
//!
//! Records from the `log` crate, which gpui and other dependencies use, reach
//! the same subscriber. A warning about something the user can fix in settings
//! or the install uses the target `knot::config`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing::Subscriber;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// The environment variable holding an `EnvFilter` directive.
pub(crate) const FILTER_VARIABLE: &str = "KNOT_LOG";
const DEFAULT_FILTER: &str = "info";
const LOG_NAME: &str = "knot";
/// A log at least this large is rotated at startup.
const ROTATE_AT: u64 = 5 * 1024 * 1024;
/// The current log plus its rotated predecessors.
const FILE_COUNT: usize = 10;

/// Keeps the file writer's thread alive until it is flushed.
static GUARD: Mutex<Option<WorkerGuard>> = Mutex::new(None);

/// The directory that holds the log files.
pub(crate) fn log_directory() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        directories::BaseDirs::new().map(|dirs| dirs.home_dir().join("Library/Logs/Knot"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        directories::ProjectDirs::from("", "", "Knot").map(|dirs| {
            dirs.state_dir()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| dirs.data_local_dir().join("logs"))
        })
    }
}

/// The path of the current log file.
pub(crate) fn log_file(directory: &Path) -> PathBuf {
    rotated_path(directory, 0)
}

fn rotated_path(directory: &Path, index: usize) -> PathBuf {
    match index {
        0 => directory.join(format!("{LOG_NAME}.log")),
        n => directory.join(format!("{LOG_NAME}.{n}.log")),
    }
}

/// Shift the log files up by one when the current one is at least
/// `threshold` bytes, dropping the oldest. Missing numbers are skipped.
fn rotate(directory: &Path, threshold: u64) -> io::Result<()> {
    let current = rotated_path(directory, 0);
    match fs::metadata(&current) {
        Ok(metadata) if metadata.len() >= threshold => {}
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    let ignore_missing = |result: io::Result<()>| match result {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    };
    ignore_missing(fs::remove_file(rotated_path(directory, FILE_COUNT - 1)))?;
    for index in (0..FILE_COUNT - 1).rev() {
        ignore_missing(fs::rename(
            rotated_path(directory, index),
            rotated_path(directory, index + 1),
        ))?;
    }
    Ok(())
}

/// The filter `KNOT_LOG` names, or `info` when it is unset or invalid.
fn filter(directive: Option<&str>) -> EnvFilter {
    directive
        .and_then(|directive| EnvFilter::try_new(directive).ok())
        .unwrap_or_else(|| EnvFilter::new(DEFAULT_FILTER))
}

fn layer<S, W>(writer: W, ansi: bool) -> impl Layer<S>
where
    S: Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(ansi)
        .with_target(true)
}

/// A subscriber that writes to `writer`.
#[cfg(test)]
fn subscriber<W>(writer: W, directive: Option<&str>) -> impl Subscriber
where
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    tracing_subscriber::registry()
        .with(layer(writer, false))
        .with(filter(directive))
}

/// Install the application subscriber. Failures to open the file leave
/// stderr output only.
pub(crate) fn install() {
    let directive = std::env::var(FILTER_VARIABLE).ok();
    let file = log_directory().and_then(|directory| open_log(&directory).ok());
    let stderr = io::stderr().is_terminal();
    let (writer, guard) = match file {
        Some(file) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            (Some(writer), Some(guard))
        }
        None => (None, None),
    };
    *GUARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = guard;
    let file_layer = writer.map(|writer: NonBlocking| layer(writer, false));
    let stderr_layer = stderr.then(|| layer(io::stderr, true));
    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .with(filter(directive.as_deref()))
        .try_init();
}

fn open_log(directory: &Path) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    // A failed rotation only lets the current file keep growing.
    let _ = rotate(directory, ROTATE_AT);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file(directory))
}

/// Write the pending records and stop the file writer. Call before the
/// process exits.
pub(crate) fn flush() {
    drop(
        GUARD
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(directory: &Path) -> Vec<String> {
        let mut names = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn rotation_shifts_files_and_drops_the_oldest() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..FILE_COUNT {
            fs::write(rotated_path(directory.path(), index), index.to_string()).unwrap();
        }
        rotate(directory.path(), 1).unwrap();
        assert!(!rotated_path(directory.path(), 0).exists());
        for index in 1..FILE_COUNT {
            assert_eq!(
                fs::read_to_string(rotated_path(directory.path(), index)).unwrap(),
                (index - 1).to_string()
            );
        }
        assert_eq!(names(directory.path()).len(), FILE_COUNT - 1);
    }

    #[test]
    fn rotation_keeps_a_small_log_and_tolerates_gaps() {
        let directory = tempfile::tempdir().unwrap();
        rotate(directory.path(), 10).unwrap();
        fs::write(rotated_path(directory.path(), 0), "tiny").unwrap();
        rotate(directory.path(), 10).unwrap();
        assert_eq!(names(directory.path()), ["knot.log"]);

        fs::write(rotated_path(directory.path(), 0), "0123456789").unwrap();
        fs::write(rotated_path(directory.path(), 3), "old").unwrap();
        rotate(directory.path(), 10).unwrap();
        assert_eq!(names(directory.path()), ["knot.1.log", "knot.4.log"]);
    }

    #[test]
    fn rotation_reports_an_unreadable_directory_without_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("not-a-directory");
        fs::write(&file, "").unwrap();
        assert!(open_log(&file).is_err());
        // A missing directory has nothing to rotate.
        assert!(rotate(&directory.path().join("missing"), 1).is_ok());
    }

    #[test]
    fn filter_directive_parses_and_falls_back_to_info() {
        assert_eq!(filter(None).to_string(), "info");
        assert_eq!(
            filter(Some("knot::fonts=debug,info")).to_string(),
            "knot::fonts=debug,info"
        );
        assert_eq!(filter(Some("=[bad")).to_string(), "info");
    }

    #[test]
    fn records_reach_the_file_with_their_target_and_filter() {
        let directory = tempfile::tempdir().unwrap();
        let file = open_log(directory.path()).unwrap();
        let (writer, guard) = tracing_appender::non_blocking(file);
        tracing::subscriber::with_default(subscriber(writer, Some("warn")), || {
            tracing::warn!(target: "knot::config", "font missing");
            tracing::info!("filtered out");
            tracing::warn!(target: "knot::extension", package = "pkg", "console.warn: hi");
        });
        drop(guard);
        let text = fs::read_to_string(log_file(directory.path())).unwrap();
        assert!(text.contains("WARN knot::config: font missing"), "{text}");
        assert!(
            text.contains("knot::extension: console.warn: hi package=\"pkg\""),
            "{text}"
        );
        assert!(!text.contains("filtered out"), "{text}");
    }
}
