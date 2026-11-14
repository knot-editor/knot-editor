//! Links the static libghostty-vt library.
//!
//! The library comes prebuilt from this repository's `libghostty-vt-<version>`
//! release, published by `.github/workflows/libghostty.yml`, so building needs
//! no Zig. The archive for the target is downloaded with `curl` once, checked
//! against its pinned SHA-256, and extracted into `OUT_DIR`.
//! `KNOT_LIBGHOSTTY_DIR` names an install tree (`include/`, `lib/`) to link
//! instead, for offline builds or to try another libghostty build.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const VERSION: &str = "1.0.1";
const RELEASE_URL: &str = "https://github.com/knot-editor/knot-editor/releases/download";

/// The SHA-256 of each target's release archive.
const ARCHIVES: &[(&str, &str)] = &[
    (
        "aarch64-apple-darwin",
        "f00d4173544e09d04e2b302a6f3d68d65e155b6de06062cf3a2e8635d3c05f03",
    ),
    (
        "x86_64-apple-darwin",
        "a5ec10bb3fe55e856a82e78ccefc00b824d956af96e3efa7f13bd1a854c7fa18",
    ),
    (
        "aarch64-unknown-linux-gnu",
        "f8d84b888fb210a566ab5a9b9d5b5969f3ac932aefde774bd74ae8f4e810964d",
    ),
    (
        "x86_64-unknown-linux-gnu",
        "2f968d4808e8074b33a94c940597576bcee0776edf692918bb5d94012a08b867",
    ),
    (
        "x86_64-pc-windows-msvc",
        "b246b4123488d022fda16cbed0474a2bab9fe6fe628d3d3870ecd78493902093",
    ),
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=KNOT_LIBGHOSTTY_DIR");
    let target = env::var("TARGET").expect("TARGET is set by Cargo");
    let windows = target.contains("windows");
    let library = if windows {
        "ghostty-vt-static.lib"
    } else {
        "libghostty-vt.a"
    };

    let install = match env::var_os("KNOT_LIBGHOSTTY_DIR") {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            println!(
                "cargo:rerun-if-changed={}",
                dir.join("lib").join(library).display()
            );
            dir
        }
        None => download(&target),
    };
    let lib = install.join("lib");
    assert!(
        lib.join(library).is_file(),
        "{} has no {library}",
        lib.display()
    );

    println!("cargo:rustc-link-search=native={}", lib.display());
    if windows {
        println!("cargo:rustc-link-lib=static=ghostty-vt-static");
        println!("cargo:rustc-link-lib=dylib=ntdll");
    } else {
        println!("cargo:rustc-link-lib=static=ghostty-vt");
    }
    println!("cargo:include={}", install.join("include").display());
}

/// Downloads, verifies, and extracts the target's archive into `OUT_DIR`,
/// returning the install tree. A previous extraction of the same archive is
/// reused.
fn download(target: &str) -> PathBuf {
    let Some((_, sha256)) = ARCHIVES.iter().find(|(name, _)| *name == target) else {
        panic!(
            "no prebuilt libghostty-vt for {target}; build libghostty-vt {VERSION} \
             with Zig and set KNOT_LIBGHOSTTY_DIR to its install prefix"
        );
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let install = out.join("libghostty-vt");
    let stamp = install.join(".sha256");
    if fs::read_to_string(&stamp).is_ok_and(|stamp| stamp == *sha256) {
        return install;
    }

    let name = format!("libghostty-vt-{VERSION}-{target}");
    let url = format!("{RELEASE_URL}/libghostty-vt-{VERSION}/{name}.tar.gz");
    let archive = out.join(format!("{name}.tar.gz"));
    let status = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--retry", "3", "--output"])
        .arg(&archive)
        .arg(&url)
        .status()
        .unwrap_or_else(|error| panic!("cannot run curl to download {url}: {error}"));
    assert!(
        status.success(),
        "downloading {url} failed; set KNOT_LIBGHOSTTY_DIR to build offline"
    );

    let bytes = fs::read(&archive).expect("read the downloaded archive");
    let actual = hex(&Sha256::digest(&bytes));
    if actual != *sha256 {
        let _ = fs::remove_file(&archive);
        panic!("{url} has SHA-256 {actual}, expected {sha256}");
    }

    let extract = out.join("extract");
    remove_dir(&extract);
    remove_dir(&install);
    unpack(&bytes[..], &extract);
    fs::rename(extract.join(&name), &install).expect("move the extracted library");
    remove_dir(&extract);
    let _ = fs::remove_file(&archive);
    fs::write(&stamp, sha256).expect("record the extracted archive");
    install
}

fn unpack(archive: impl Read, destination: &Path) {
    tar::Archive::new(flate2::read::GzDecoder::new(archive))
        .unpack(destination)
        .expect("extract the libghostty-vt archive");
}

fn remove_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap_or_else(|error| {
            panic!("cannot remove {}: {error}", path.display());
        });
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
