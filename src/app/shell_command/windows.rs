//! The command on Windows: the executable's folder as an entry of the user
//! `Path` in `HKCU\Environment`, which new terminals read.

use std::path::Path;

pub(super) fn install(executable: &Path) -> Result<(), String> {
    let folder = folder_of(executable)?;
    change_user_path(|path| with_path_entry(path, &folder))
        .map(|_| ())
        .map_err(|error| {
            format!(
                "{error}\n\nTo add it yourself, open \"Edit environment variables for your \
                 account\" and add this folder to Path:\n{folder}"
            )
        })
}

/// Remove exactly the entry of the running executable's folder.
pub(super) fn uninstall(executable: &Path) -> Result<(), String> {
    let folder = folder_of(executable)?;
    let changed = change_user_path(|path| without_path_entry(path, &folder)).map_err(|error| {
        format!(
            "{error}\n\nTo remove it yourself, open \"Edit environment variables for your \
             account\" and remove this folder from Path:\n{folder}"
        )
    })?;
    if changed {
        Ok(())
    } else {
        Err(format!("{folder} is not on your Path."))
    }
}

fn folder_of(executable: &Path) -> Result<String, String> {
    executable
        .parent()
        .and_then(Path::to_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Knot cannot use its folder: {}", executable.display()))
}

/// The entries of a `;`-separated `Path` value, without empty ones.
fn path_entries(path: &str) -> impl Iterator<Item = &str> {
    path.split(';').filter(|entry| !entry.is_empty())
}

/// Windows compares folders case-insensitively, with or without a trailing
/// separator.
fn same_folder(left: &str, right: &str) -> bool {
    let trim = |folder: &str| folder.trim_end_matches(['\\', '/']).to_lowercase();
    trim(left) == trim(right)
}

/// `path` with `folder` appended, or `None` when it already has it.
fn with_path_entry(path: &str, folder: &str) -> Option<String> {
    if path_entries(path).any(|entry| same_folder(entry, folder)) {
        return None;
    }
    Some(
        path_entries(path)
            .chain([folder])
            .collect::<Vec<_>>()
            .join(";"),
    )
}

/// `path` without `folder`, or `None` when it does not have it.
fn without_path_entry(path: &str, folder: &str) -> Option<String> {
    if !path_entries(path).any(|entry| same_folder(entry, folder)) {
        return None;
    }
    Some(
        path_entries(path)
            .filter(|entry| !same_folder(entry, folder))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

#[cfg(not(windows))]
fn change_user_path(_: impl FnOnce(&str) -> Option<String>) -> Result<bool, String> {
    Err("The user Path exists only on Windows.".into())
}

/// Apply `change` to the user `Path` and tell running programs, so that
/// terminals opened from Explorer see it. Returns whether it changed.
#[cfg(windows)]
fn change_user_path(change: impl FnOnce(&str) -> Option<String>) -> Result<bool, String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    let environment = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(|error| format!("Knot cannot open the user environment: {error}"))?;
    let changed = registry::change_path_value(&environment, change)
        .map_err(|error| format!("Knot cannot change the user Path: {error}"))?;
    if changed {
        registry::broadcast_environment_change();
    }
    Ok(changed)
}

#[cfg(windows)]
mod registry {
    use std::io;

    use winreg::enums::{REG_EXPAND_SZ, REG_SZ};
    use winreg::{RegKey, RegValue};

    /// Rewrite the `Path` value of `key`, keeping its type so that entries
    /// such as `%USERPROFILE%\bin` still expand. Returns whether it changed.
    pub(super) fn change_path_value(
        key: &RegKey,
        change: impl FnOnce(&str) -> Option<String>,
    ) -> io::Result<bool> {
        let (path, vtype) = match key.get_raw_value("Path") {
            Ok(value) if matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) => {
                (decode(&value.bytes), value.vtype)
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the Path value is not text",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (String::new(), REG_EXPAND_SZ),
            Err(error) => return Err(error),
        };
        let Some(path) = change(&path) else {
            return Ok(false);
        };
        let bytes = path
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        key.set_raw_value("Path", &RegValue { bytes, vtype })?;
        Ok(true)
    }

    fn decode(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .take_while(|&unit| unit != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    pub(super) fn broadcast_environment_change() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        };
        let environment: Vec<u16> = "Environment".encode_utf16().chain([0]).collect();
        // SAFETY: `environment` is a NUL-terminated UTF-16 string that
        // outlives the call, and the result pointer may be null.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                std::ptr::null_mut(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_a_path_entry_keeps_the_others_and_is_idempotent() {
        assert_eq!(
            with_path_entry(r"C:\bin;%USERPROFILE%\tools;", r"C:\Knot").as_deref(),
            Some(r"C:\bin;%USERPROFILE%\tools;C:\Knot")
        );
        assert_eq!(with_path_entry("", r"C:\Knot").as_deref(), Some(r"C:\Knot"));
        assert_eq!(with_path_entry(r"C:\bin;c:\knot\", r"C:\Knot"), None);
    }

    #[test]
    fn removing_a_path_entry_removes_only_that_folder() {
        assert_eq!(
            without_path_entry(r"C:\bin;C:\Knot\;D:\other", r"c:\knot").as_deref(),
            Some(r"C:\bin;D:\other")
        );
        assert_eq!(without_path_entry(r"C:\bin;C:\Knotty", r"C:\Knot"), None);
    }

    #[cfg(windows)]
    #[test]
    fn changing_the_path_value_keeps_its_type() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, REG_EXPAND_SZ};

        let name = format!(r"Software\KnotTest\{}", uuid::Uuid::new_v4());
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = user.create_subkey(&name).unwrap();
        let add = |path: &str| with_path_entry(path, r"C:\Knot");
        let remove = |path: &str| without_path_entry(path, r"C:\Knot");

        assert!(registry::change_path_value(&key, add).unwrap());
        assert!(!registry::change_path_value(&key, add).unwrap());
        assert_eq!(key.get_raw_value("Path").unwrap().vtype, REG_EXPAND_SZ);
        assert_eq!(key.get_value::<String, _>("Path").unwrap(), r"C:\Knot");

        assert!(registry::change_path_value(&key, remove).unwrap());
        assert_eq!(key.get_value::<String, _>("Path").unwrap(), "");
        user.delete_subkey_all(&name).unwrap();
        let _ = user.delete_subkey(r"Software\KnotTest");
    }
}
