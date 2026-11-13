//! Single-instance launch forwarding: the running Knot owns a local socket
//! that command-line launches send their batch to.
//!
//! One message is one batch: one `file:` URI per line, ended by closing the
//! connection.

use std::io::{BufReader, Read, Write};

use interprocess::local_socket::{Listener, ListenerOptions, Name, Stream, prelude::*};
use knot_model::entry::{LaunchBatch, OpenRequest};

/// The most bytes one batch may carry.
const MAX_MESSAGE_BYTES: u64 = 4 * 1024 * 1024;

#[cfg(unix)]
fn default_name() -> std::io::Result<Name<'static>> {
    use interprocess::local_socket::GenericFilePath;
    let path =
        super::record_store::StateDatabase::user_socket_path().map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    path.to_fs_name::<GenericFilePath>()
}

/// Pipe names are machine-global, so the name carries the user.
#[cfg(windows)]
fn default_name() -> std::io::Result<Name<'static>> {
    use interprocess::local_socket::GenericNamespaced;
    let user = std::env::var("USERNAME").unwrap_or_default();
    format!("knot-launch-{user}").to_ns_name::<GenericNamespaced>()
}

/// Send the batch to a running Knot; whether one took it.
pub(crate) fn forward(batch: &LaunchBatch) -> bool {
    default_name().is_ok_and(|name| forward_to(name, batch).is_ok())
}

/// Become the listener, replacing a socket file a dead instance left behind.
pub(crate) fn bind() -> std::io::Result<Listener> {
    bind_to(default_name()?)
}

fn forward_to(name: Name<'_>, batch: &LaunchBatch) -> std::io::Result<()> {
    let mut stream = Stream::connect(name)?;
    stream.write_all(encode(batch).as_bytes())?;
    stream.flush()
}

fn bind_to(name: Name<'_>) -> std::io::Result<Listener> {
    ListenerOptions::new()
        .name(name)
        .try_overwrite(true)
        .create_sync()
}

fn encode(batch: &LaunchBatch) -> String {
    batch
        .requests()
        .iter()
        .map(|request| format!("{}\n", request.uri()))
        .collect()
}

/// Lines that are not valid requests are dropped with a note.
fn decode(message: &str) -> LaunchBatch {
    LaunchBatch::new(
        message
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| match OpenRequest::from_url(line) {
                Ok(request) => Some(request),
                Err(error) => {
                    eprintln!("[knot] cannot open {line}: {error}");
                    None
                }
            })
            .collect(),
    )
}

fn receive(stream: Stream) -> std::io::Result<LaunchBatch> {
    let mut message = String::new();
    BufReader::new(stream)
        .take(MAX_MESSAGE_BYTES)
        .read_to_string(&mut message)?;
    Ok(decode(&message))
}

/// Deliver every batch a client sends, from a thread that lives as long as
/// the process.
pub(crate) fn serve(listener: Listener, deliver: impl Fn(LaunchBatch) + Send + 'static) {
    std::thread::spawn(move || {
        for connection in listener.incoming().filter_map(Result::ok) {
            match receive(connection) {
                Ok(batch) => deliver(batch),
                Err(error) => eprintln!("[knot] launch connection failed: {error}"),
            }
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use interprocess::local_socket::GenericFilePath;

    use super::*;

    fn name(path: &std::path::Path) -> Name<'static> {
        path.to_path_buf().to_fs_name::<GenericFilePath>().unwrap()
    }

    fn batch(uris: &[&str]) -> LaunchBatch {
        LaunchBatch::new(
            uris.iter()
                .map(|uri| OpenRequest::from_url(uri).unwrap())
                .collect(),
        )
    }

    #[test]
    fn a_forwarded_batch_arrives_in_order() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("launch.sock");
        let (sender, receiver) = mpsc::channel();
        serve(bind_to(name(&path)).unwrap(), move |batch| {
            sender.send(batch).unwrap()
        });

        let sent = batch(&["file:///tmp/a%20b.txt", "file:///tmp/dir/"]);
        forward_to(name(&path), &sent).unwrap();

        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), sent);
    }

    #[test]
    fn invalid_lines_are_dropped() {
        let decoded = decode("file:///tmp/a.txt\nnot a uri\nhttps://example.com/x\n\n");
        assert_eq!(decoded, batch(&["file:///tmp/a.txt"]));
    }

    #[test]
    fn a_dead_instances_socket_file_is_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("launch.sock");
        // The standard listener leaves its file behind when dropped.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(forward_to(name(&path), &batch(&[])).is_err());

        let (sender, receiver) = mpsc::channel();
        serve(bind_to(name(&path)).unwrap(), move |batch| {
            sender.send(batch).unwrap()
        });
        let sent = batch(&["file:///tmp/a.txt"]);
        forward_to(name(&path), &sent).unwrap();
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), sent);
    }
}
