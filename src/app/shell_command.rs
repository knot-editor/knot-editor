//! The `knot` command on the shell `PATH`, pointing at the running executable
//! so it stays valid as long as the app stays where it is. On macOS it is a
//! symlink in `/usr/local/bin`, on Linux a symlink in `~/.local/bin`, and on
//! Windows the executable's folder is an entry of the user `Path`.

use std::path::{Path, PathBuf};

mod linux;
mod windows;

/// Where macOS installs the link.
const MACOS_LINK: &str = "/usr/local/bin/knot";

/// Install the command for the running executable. The error is a message for
/// the user and names the manual step.
pub(crate) fn install() -> Result<(), String> {
    let executable = running_executable()?;
    if cfg!(windows) {
        windows::install(&executable)
    } else if cfg!(target_os = "linux") {
        linux::install(&executable)
    } else {
        install_macos(&executable, Path::new(MACOS_LINK))
    }
}

/// Remove the command: on macOS and Linux only when the link points to a
/// Knot executable, on Windows the `Path` entry of the running executable's
/// folder.
pub(crate) fn uninstall() -> Result<(), String> {
    if cfg!(windows) {
        windows::uninstall(&running_executable()?)
    } else if cfg!(target_os = "linux") {
        linux::uninstall()
    } else {
        uninstall_macos(Path::new(MACOS_LINK))
    }
}

/// The running executable. It is not canonicalized on Windows, which would
/// add the `\\?\` prefix that `Path` entries do not use.
fn running_executable() -> Result<PathBuf, String> {
    let executable = std::env::current_exe();
    let executable = if cfg!(windows) {
        executable
    } else {
        executable.and_then(std::fs::canonicalize)
    };
    executable.map_err(|error| format!("Knot cannot find its own executable: {error}"))
}

fn install_macos(executable: &Path, link: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Installing the command is not supported on this platform yet.".into());
    }
    if is_translocated(executable) {
        return Err(
            "Knot is running from a temporary location because it was just downloaded. \
             Move Knot.app to the Applications folder, start it again, and retry."
                .into(),
        );
    }
    let script = install_script(executable, link);
    run_privileged(&script).map_err(|error| {
        format!(
            "{error}\n\nTo install it yourself, run:\n{}",
            manual_install(executable, link)
        )
    })
}

fn uninstall_macos(link: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Removing the command is not supported on this platform yet.".into());
    }
    match std::fs::read_link(link) {
        Err(_) if std::fs::symlink_metadata(link).is_err() => {
            return Err(format!("{} does not exist.", link.display()));
        }
        Err(_) => return Err(format!("{} is not a link to Knot.", link.display())),
        Ok(target) if !is_knot_executable(&target) => {
            return Err(format!(
                "{} is a link to {}, not to Knot.",
                link.display(),
                target.display()
            ));
        }
        Ok(_) => {}
    }
    run_privileged(&uninstall_script(link)).map_err(|error| {
        format!(
            "{error}\n\nTo remove it yourself, run:\nsudo rm {}",
            link.display()
        )
    })
}

/// An executable inside an app bundle, which is what the install links to.
fn is_knot_executable(path: &Path) -> bool {
    std::fs::canonicalize(path)
        .ok()
        .or_else(|| Some(path.to_path_buf()))
        .is_some_and(|path| super::launcher::bundle_of(&path).is_some())
        || std::env::current_exe().is_ok_and(|current| current == path)
}

/// Gatekeeper runs a quarantined app from a randomized read-only copy.
fn is_translocated(executable: &Path) -> bool {
    executable
        .components()
        .any(|component| component.as_os_str() == "AppTranslocation")
}

fn install_script(executable: &Path, link: &Path) -> String {
    let directory = link.parent().unwrap_or(Path::new("/"));
    format!(
        "mkdir -p {} && ln -sf {} {}",
        shell_quote(directory),
        shell_quote(executable),
        shell_quote(link)
    )
}

fn uninstall_script(link: &Path) -> String {
    format!("rm {}", shell_quote(link))
}

fn manual_install(executable: &Path, link: &Path) -> String {
    format!(
        "sudo mkdir -p {} && sudo ln -sf {} {}",
        shell_quote(link.parent().unwrap_or(Path::new("/"))),
        shell_quote(executable),
        shell_quote(link)
    )
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// The AppleScript that runs `script` as administrator.
fn applescript(script: &str) -> String {
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    format!("do shell script \"{escaped}\" with administrator privileges")
}

/// Run a shell script through the system password prompt.
fn run_privileged(script: &str) -> Result<(), String> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(applescript(script))
        .output()
        .map_err(|error| format!("Knot cannot run osascript: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr);
    Err(if message.contains("User canceled") {
        "The password prompt was cancelled.".to_string()
    } else {
        message.trim().to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_script_quotes_every_path() {
        let script = install_script(
            Path::new("/Users/me/Applications/Knot's.app/Contents/MacOS/knot"),
            Path::new("/usr/local/bin/knot"),
        );
        assert_eq!(
            script,
            "mkdir -p '/usr/local/bin' && ln -sf \
             '/Users/me/Applications/Knot'\\''s.app/Contents/MacOS/knot' '/usr/local/bin/knot'"
        );
    }

    #[test]
    fn applescript_escapes_quotes_and_backslashes() {
        assert_eq!(
            applescript(r#"ln -sf 'a\b' "c""#),
            r#"do shell script "ln -sf 'a\\b' \"c\"" with administrator privileges"#
        );
    }

    #[test]
    fn translocated_executables_are_refused_wherever_the_app_would_live() {
        assert!(is_translocated(Path::new(
            "/private/var/folders/x/AppTranslocation/ABC/d/Knot.app/Contents/MacOS/knot"
        )));
        for path in [
            "/Applications/Knot.app/Contents/MacOS/knot",
            "/Users/me/Applications/Knot.app/Contents/MacOS/knot",
        ] {
            assert!(!is_translocated(Path::new(path)));
        }
    }

    #[test]
    fn only_bundle_executables_count_as_knot() {
        assert!(is_knot_executable(Path::new(
            "/Users/me/Applications/Knot.app/Contents/MacOS/knot"
        )));
        assert!(is_knot_executable(Path::new(
            "/Applications/Knot.app/Contents/MacOS/knot"
        )));
        assert!(!is_knot_executable(Path::new("/usr/bin/vim")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn install_refuses_a_translocated_executable_before_asking_for_a_password() {
        let error = install_macos(
            Path::new("/private/var/folders/x/AppTranslocation/ABC/d/Knot.app/Contents/MacOS/knot"),
            Path::new("/nonexistent/knot"),
        )
        .unwrap_err();
        assert!(error.contains("Applications folder"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_leaves_a_link_to_something_else_alone() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("knot");
        std::os::unix::fs::symlink("/usr/bin/vim", &link).unwrap();
        let error = uninstall_macos(&link).unwrap_err();
        assert!(error.contains("not to Knot"));
        assert!(link.symlink_metadata().is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_reports_a_missing_or_regular_file() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("knot");
        assert!(
            uninstall_macos(&link)
                .unwrap_err()
                .contains("does not exist")
        );
        std::fs::write(&link, "").unwrap();
        assert!(uninstall_macos(&link).unwrap_err().contains("not a link"));
    }
}
