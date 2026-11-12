//! Compiles the build identity into the binary: `KNOT_VERSION` when the
//! packaging step sets it, else the Cargo version plus `-dev`, and the short
//! git commit.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=KNOT_VERSION");
    println!("cargo:rerun-if-changed=.git/HEAD");
    let version = std::env::var("KNOT_VERSION")
        .ok()
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| format!("{}-dev", std::env::var("CARGO_PKG_VERSION").unwrap()));
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_owned())
        .filter(|commit| !commit.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=KNOT_BUILD_VERSION={version}");
    println!("cargo:rustc-env=KNOT_BUILD_COMMIT={commit}");
}
