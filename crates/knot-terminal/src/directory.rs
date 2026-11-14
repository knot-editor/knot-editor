//! Reading the working directory a program reports.

use std::path::PathBuf;

use url::Url;

/// The local directory that a
/// [`TerminalEvent::WorkingDirectory`](crate::TerminalEvent::WorkingDirectory)
/// names, or `None` when it names none or names one on another machine.
///
/// A `file://` URI counts when its host is empty, `localhost`, or this
/// machine's host name; its path is percent-decoded. Anything else that is
/// not a URI counts when it is an absolute path.
pub fn local_directory(reported: &str) -> Option<PathBuf> {
    // A Windows path such as `C:\Users` parses as a URI with scheme `c`.
    let Some(url) = Url::parse(reported)
        .ok()
        .filter(|url| url.scheme() == "file")
    else {
        let path = PathBuf::from(reported);
        return path.is_absolute().then_some(path);
    };
    if let Some(host) = url.host_str().filter(|host| !host.is_empty())
        && !host.eq_ignore_ascii_case("localhost")
        && !host_name().is_some_and(|name| host.eq_ignore_ascii_case(&name))
    {
        return None;
    }
    // Only the path names the directory once the host is known to be local.
    Url::parse(&format!("file://{}", url.path()))
        .ok()?
        .to_file_path()
        .ok()
}

#[cfg(unix)]
fn host_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer is writable for its whole length.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return None;
    }
    let name = std::ffi::CStr::from_bytes_until_nul(&buffer).ok()?;
    Some(name.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machines_host_name_is_local() {
        let name = host_name().unwrap();
        assert!(!name.is_empty());
        if cfg!(unix) {
            assert_eq!(
                local_directory(&format!("file://{name}/tmp/dir")),
                Some(PathBuf::from("/tmp/dir"))
            );
            assert_eq!(
                local_directory(&format!("file://{}/tmp", name.to_uppercase())),
                Some(PathBuf::from("/tmp"))
            );
        }
    }
}
