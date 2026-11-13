//! Cold start from a command line on macOS: with no running Knot, a `knot`
//! that lives inside an app bundle asks LaunchServices to start the app, so it
//! runs as a normal application and not as a child of the shell.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use knot_model::entry::LaunchBatch;

/// Set in the environment `open` gives the app, so the app it starts does not
/// ask LaunchServices again.
const STARTED_BY_LAUNCHER: &str = "KNOT_STARTED_BY_LAUNCHER";

/// Start the app bundle that holds this executable for `batch`; whether it did.
/// Only macOS, only from a shell: a start LaunchServices made, a loose binary,
/// and every other platform return false.
pub(crate) fn start_bundle(batch: &LaunchBatch) -> bool {
    if !cfg!(target_os = "macos") || std::env::var_os(STARTED_BY_LAUNCHER).is_some() {
        return false;
    }
    let Some(bundle) = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .and_then(|executable| bundle_of(&executable).map(Path::to_path_buf))
    else {
        return false;
    };
    std::process::Command::new("open")
        .args(open_arguments(&bundle, batch))
        .status()
        .is_ok_and(|status| status.success())
}

/// The `.app` directory an executable at `Contents/MacOS/<name>` belongs to.
pub(crate) fn bundle_of(executable: &Path) -> Option<&Path> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then_some(bundle)
}

/// The arguments of the `open` that starts `bundle` with the batch's paths.
fn open_arguments(bundle: &Path, batch: &LaunchBatch) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-a"),
        bundle.into(),
        OsString::from("--env"),
        format!("{STARTED_BY_LAUNCHER}=1").into(),
        OsString::from("--args"),
    ];
    arguments.extend(
        batch
            .requests()
            .iter()
            .filter_map(|request| request.uri().as_url().to_file_path().ok())
            .map(PathBuf::into_os_string),
    );
    arguments
}

#[cfg(test)]
mod tests {
    use knot_model::entry::OpenRequest;

    use super::*;

    #[test]
    fn bundle_is_found_from_the_executable_inside_it() {
        assert_eq!(
            bundle_of(Path::new("/Applications/Knot.app/Contents/MacOS/knot")),
            Some(Path::new("/Applications/Knot.app"))
        );
    }

    #[test]
    fn loose_executables_have_no_bundle() {
        for path in [
            "/repo/target/debug/knot",
            "/Applications/Knot.app/Contents/Resources/knot",
            "/Applications/Knot/Contents/MacOS/knot",
        ] {
            assert_eq!(bundle_of(Path::new(path)), None, "{path}");
        }
    }

    #[test]
    fn open_arguments_pass_absolute_paths_after_args() {
        let batch = LaunchBatch::new(vec![
            OpenRequest::from_path(Path::new("a b.txt"), Path::new("/work")).unwrap(),
            OpenRequest::from_path(Path::new("/tmp/dir"), Path::new("/work")).unwrap(),
        ]);
        let arguments = open_arguments(Path::new("/Applications/Knot.app"), &batch);
        assert_eq!(
            arguments,
            [
                "-a",
                "/Applications/Knot.app",
                "--env",
                "KNOT_STARTED_BY_LAUNCHER=1",
                "--args",
                "/work/a b.txt",
                "/tmp/dir",
            ]
        );
    }
}
