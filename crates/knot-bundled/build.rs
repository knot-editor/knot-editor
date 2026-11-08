//! Compiles the TypeScript extension packages with the lockfile-pinned compiler
//! in `scripts/extensions`. Bundled packages under `extensions/@knot` are
//! embedded through a generated table that `src/lib.rs` includes. Each package
//! is recompiled only when its own inputs or a shared input changed.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .canonicalize()
        .unwrap();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let tools = root.join("scripts/extensions");
    let bundled = root.join("extensions/@knot");

    let shared = [
        tools.join("package-lock.json"),
        tools.join("tsconfig.base.json"),
        root.join("crates/knot-host/src/js/knot.d.ts"),
    ];
    println!("cargo::rerun-if-changed={}", bundled.display());
    for input in &shared {
        println!("cargo::rerun-if-changed={}", input.display());
    }

    install_tools(&tools);

    let mut shared_fingerprint = String::new();
    for input in &shared {
        fingerprint(input, &mut shared_fingerprint);
    }

    let mut table = String::from("pub const PACKAGES: &[BundledSource] = &[\n");
    for package in sorted_entries(&bundled)
        .into_iter()
        .filter(|path| path.is_dir())
    {
        let name = package.file_name().unwrap().to_str().unwrap();
        let dist = out.join("extensions").join(name).join("dist");
        let stamp = out.join("extensions").join(format!("{name}.fingerprint"));
        let mut current = shared_fingerprint.clone();
        fingerprint(&package.join("src"), &mut current);
        fingerprint(&package.join("tsconfig.json"), &mut current);
        if !dist.exists() || fs::read_to_string(&stamp).ok().as_deref() != Some(&current) {
            // A failed compile leaves no stamp, so the next build retries.
            let _ = fs::remove_file(&stamp);
            compile(&tools, &package, &dist);
            fs::write(&stamp, &current).unwrap();
        }

        let mut modules = Vec::new();
        collect_modules(&dist, &dist, &mut modules);
        writeln!(
            table,
            "    BundledSource {{\n        manifest: include_str!({:?}),\n        sources: &[",
            package.join("knot.jsonc").to_str().unwrap()
        )
        .unwrap();
        for (relative, path) in modules {
            writeln!(
                table,
                "            ({:?}, include_str!({:?})),",
                format!("dist/{relative}"),
                path.to_str().unwrap()
            )
            .unwrap();
        }
        table.push_str("        ],\n    },\n");
    }
    table.push_str("];\n");
    fs::write(out.join("bundled_packages.rs"), table).unwrap();
}

/// Installs the pinned toolchain when it is missing or older than the lockfile.
fn install_tools(tools: &Path) {
    let lockfile = tools.join("package-lock.json");
    let installed = tools.join("node_modules/.package-lock.json");
    let modified = |path: &Path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    };
    if modified(&installed).is_some_and(|time| Some(time) >= modified(&lockfile)) {
        return;
    }
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    run(
        Command::new(npm)
            .args(["ci", "--ignore-scripts", "--no-audit", "--no-fund"])
            .current_dir(tools),
        "npm ci",
    );
}

fn compile(tools: &Path, package: &Path, dist: &Path) {
    if dist.exists() {
        fs::remove_dir_all(dist).unwrap();
    }
    run(
        Command::new("node")
            .arg(tools.join("node_modules/typescript/bin/tsc"))
            .arg("-p")
            .arg(package.join("tsconfig.json"))
            .arg("--outDir")
            .arg(dist),
        &format!("tsc for {}", package.display()),
    );
}

/// Appends the path, size, and modification time of `path` (recursively for a
/// directory) to `into`.
fn fingerprint(path: &Path, into: &mut String) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        for entry in sorted_entries(path) {
            fingerprint(&entry, into);
        }
    } else {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());
        writeln!(into, "{} {} {modified}", path.display(), metadata.len()).unwrap();
    }
}

fn run(command: &mut Command, description: &str) {
    let output = command.output().unwrap_or_else(|error| {
        panic!("cannot run {description}: {error}; building Knot requires Node.js and npm")
    });
    if !output.status.success() {
        panic!(
            "{description} failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn collect_modules(root: &Path, directory: &Path, modules: &mut Vec<(String, PathBuf)>) {
    for path in sorted_entries(directory) {
        if path.is_dir() {
            collect_modules(root, &path, modules);
        } else if path.extension().is_some_and(|extension| extension == "js") {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/");
            modules.push((relative, path));
        }
    }
}

fn sorted_entries(directory: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
