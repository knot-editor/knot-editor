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
        "808ddbec934b1f13af79bef9f2564f98e6c14c741850a1532c3c713c80bf5e4f",
    ),
    (
        "x86_64-apple-darwin",
        "4786193ae592aa6c8bf27fc237cc486d0915ea953690b60695a071e41ee0113c",
    ),
    (
        "aarch64-unknown-linux-gnu",
        "cab7582863597e01f7011f04f524884d480a076036e4ccb2756b8721052dfc5b",
    ),
    (
        "x86_64-unknown-linux-gnu",
        "a4a7bf7b8cb12c1e5e4bc505b8e42ca241a3e0976fa17ae00de0767f08d964a6",
    ),
    (
        "x86_64-pc-windows-msvc",
        "ab8afa5cb5661c189d95179070640781966a46b97fd096b5828a830617be349a",
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
