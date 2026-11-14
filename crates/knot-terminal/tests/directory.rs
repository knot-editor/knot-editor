use std::path::PathBuf;

use knot_terminal::local_directory;

#[cfg(unix)]
#[test]
fn local_file_uris_and_paths_name_directories() {
    for (reported, expected) in [
        ("file:///tmp/dir", "/tmp/dir"),
        ("file://localhost/tmp/dir", "/tmp/dir"),
        ("file://LOCALHOST/tmp", "/tmp"),
        ("file:///tmp/with%20space/caf%C3%A9", "/tmp/with space/café"),
        ("file:///tmp/100%25", "/tmp/100%"),
        ("/Users/me/src", "/Users/me/src"),
    ] {
        assert_eq!(
            local_directory(reported),
            Some(PathBuf::from(expected)),
            "{reported}"
        );
    }
}

#[cfg(windows)]
#[test]
fn local_file_uris_and_paths_name_directories() {
    for (reported, expected) in [
        ("file:///C:/Users/me", r"C:\Users\me"),
        ("file://localhost/C:/with%20space", r"C:\with space"),
        (r"C:\Users\me", r"C:\Users\me"),
    ] {
        assert_eq!(
            local_directory(reported),
            Some(PathBuf::from(expected)),
            "{reported}"
        );
    }
}

#[test]
fn remote_hosts_other_schemes_and_relative_paths_name_none() {
    for reported in [
        "",
        "file://build-server.example/tmp/dir",
        "kitty-shell-cwd://localhost/tmp",
        "relative/dir",
    ] {
        assert_eq!(local_directory(reported), None, "{reported}");
    }
}
