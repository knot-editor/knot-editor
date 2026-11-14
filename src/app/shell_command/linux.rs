//! The command on Linux: a symlink in the user's executable directory
//! (`$XDG_BIN_HOME`, usually `~/.local/bin`), which needs no elevation.

use std::path::{Path, PathBuf};

pub(super) fn install(executable: &Path) -> Result<(), String> {
    let link = link_path()?;
    install_link(executable, &link).map_err(|error| {
        format!(
            "{error}\n\nTo install it yourself, run:\nmkdir -p {} && ln -sf {} {}",
            super::shell_quote(link.parent().unwrap_or(Path::new("/"))),
            super::shell_quote(executable),
            super::shell_quote(&link)
        )
    })
}

pub(super) fn uninstall() -> Result<(), String> {
    uninstall_link(&link_path()?)
}

fn link_path() -> Result<PathBuf, String> {
    directories::BaseDirs::new()
        .and_then(|directories| directories.executable_dir().map(|dir| dir.join("knot")))
        .ok_or_else(|| "Knot cannot find your home folder.".to_string())
}

/// Create or replace the link, but never overwrite a regular file.
fn install_link(executable: &Path, link: &Path) -> Result<(), String> {
    if link
        .symlink_metadata()
        .is_ok_and(|metadata| !metadata.is_symlink())
    {
        return Err(format!("{} exists and is not a link.", link.display()));
    }
    let directory = link.parent().unwrap_or(Path::new("/"));
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Knot cannot create {}: {error}", directory.display()))?;
    let _ = std::fs::remove_file(link);
    symlink(executable, link)
        .map_err(|error| format!("Knot cannot create {}: {error}", link.display()))
}

/// Remove the link only when it points to an executable named `knot`.
fn uninstall_link(link: &Path) -> Result<(), String> {
    match std::fs::read_link(link) {
        Err(_) if link.symlink_metadata().is_err() => {
            return Err(format!("{} does not exist.", link.display()));
        }
        Err(_) => return Err(format!("{} is not a link to Knot.", link.display())),
        Ok(target) if target.file_name().is_none_or(|name| name != "knot") => {
            return Err(format!(
                "{} is a link to {}, not to Knot.",
                link.display(),
                target.display()
            ));
        }
        Ok(_) => {}
    }
    std::fs::remove_file(link).map_err(|error| {
        format!(
            "Knot cannot remove {}: {error}\n\nTo remove it yourself, run:\nrm {}",
            link.display(),
            super::shell_quote(link)
        )
    })
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn install_creates_the_folder_and_replaces_an_old_link() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("bin/knot");
        install_link(Path::new("/old/knot"), &link).unwrap();
        install_link(Path::new("/opt/knot/knot"), &link).unwrap();
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            Path::new("/opt/knot/knot")
        );
    }

    #[test]
    fn install_leaves_a_regular_file_alone() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("knot");
        std::fs::write(&link, "script").unwrap();
        let error = install_link(Path::new("/opt/knot/knot"), &link).unwrap_err();
        assert!(error.contains("not a link"));
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "script");
    }

    #[test]
    fn uninstall_removes_only_a_link_to_knot() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("knot");
        assert!(
            uninstall_link(&link)
                .unwrap_err()
                .contains("does not exist")
        );

        std::os::unix::fs::symlink("/usr/bin/vim", &link).unwrap();
        assert!(uninstall_link(&link).unwrap_err().contains("not to Knot"));
        assert!(link.symlink_metadata().is_ok());
        std::fs::remove_file(&link).unwrap();

        std::fs::write(&link, "").unwrap();
        assert!(uninstall_link(&link).unwrap_err().contains("not a link"));
        std::fs::remove_file(&link).unwrap();

        install_link(Path::new("/opt/knot/knot"), &link).unwrap();
        uninstall_link(&link).unwrap();
        assert!(link.symlink_metadata().is_err());
    }
}
