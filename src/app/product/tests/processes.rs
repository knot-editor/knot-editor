//! Running programs from packages: the entitlement gate, `processes.run` and
//! `processes.spawn`, and teardown of what a package started.

use super::entitlements::declare;
use super::operations::empty_product_window;
use super::*;
use crate::app::extensions::entitlements;
use crate::app::extensions::host::ProductExtensionHost;
use knot_model::extensions::entitlements::Decision;
use knot_model::extensions::package::Entitlement;

const PACKAGE: &str = "@example/runner";

struct Loaded {
    host: Entity<ProductExtensionHost>,
    _extensions: tempfile::TempDir,
    _storage: tempfile::TempDir,
}

/// Installs a package running `main`, declaring `declared` and, when it
/// declares anything, granted it, and waits for its startup to finish.
fn load(main: &str, declared: &[&str], cx: &mut TestAppContext) -> Loaded {
    let extensions = tempfile::tempdir().unwrap();
    install_extension(extensions.path(), PACKAGE, &[], &[("main.js", main)]);
    declare(extensions.path(), PACKAGE, declared);
    if !declared.is_empty() {
        cx.update(|cx| {
            entitlements::decide(PACKAGE, &[Entitlement::Process], Decision::Granted, cx)
        });
    }
    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let storage = tempfile::tempdir().unwrap();
    host.update(cx, |host, cx| {
        host.set_test_storage_root(storage.path().to_owned(), cx)
    });
    let _window = empty_product_window(&documents, cx);
    let root = extensions.path().to_owned();
    host.update(cx, |host, cx| host.start_installed_extensions(root, cx));
    let report = wait_for_extension_report(&host, cx);
    assert!(
        report
            .report
            .entries
            .iter()
            .all(|entry| entry.result == crate::app::extensions::load::LoadResult::Loaded),
        "{:?}",
        report.report.entries
    );
    Loaded {
        host,
        _extensions: extensions,
        _storage: storage,
    }
}

/// What the package wrote to the clipboard.
fn answer(cx: &mut TestAppContext) -> String {
    cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap())
}

/// Runs `body` as the package's main module, which answers through the clipboard.
fn run_package(body: &str, declared: &[&str], cx: &mut TestAppContext) -> String {
    let main = format!(
        r#"import {{ processes, workbench }} from "knot";
const answer = async (value) => workbench.writeClipboardText(JSON.stringify(value));
{body}"#
    );
    load(&main, declared, cx);
    answer(cx)
}

#[gpui::test]
fn run_collects_output_and_the_exit_status(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let cwd = url::Url::from_directory_path(directory.path()).unwrap();
    let written = run_package(
        &format!(
            r#"
const result = await processes.run({{
  command: "sh",
  args: ["-c", "echo out; echo err >&2; echo $KNOT_X ${{HOME-unset}}; pwd -P; printf in:; cat; exit 3"],
  cwd: "{cwd}",
  env: {{ KNOT_X: "set", HOME: null }},
  stdin: "payload",
}});
await answer(result);
"#
        ),
        &["process"],
        cx,
    );
    let result: serde_json::Value = serde_json::from_str(&written).unwrap();
    let real = std::fs::canonicalize(directory.path()).unwrap();
    assert_eq!(result["exitCode"], 3);
    assert_eq!(result["signal"], serde_json::Value::Null);
    assert_eq!(
        result["stdout"],
        format!("out\nset unset\n{}\nin:payload", real.display())
    );
    assert_eq!(result["stderr"], "err\n");
}

#[gpui::test]
fn a_package_that_did_not_declare_the_entitlement_is_rejected(cx: &mut TestAppContext) {
    let written = run_package(
        r#"
const codes = [];
for (const call of [
  () => processes.run({ command: "true" }),
  () => processes.spawn({ command: "true" }),
]) {
  try { await call(); } catch (error) { codes.push(error.code); }
}
await answer(codes);
"#,
        &[],
        cx,
    );
    assert_eq!(
        written,
        r#"["EntitlementNotDeclared","EntitlementNotDeclared"]"#
    );
}

#[gpui::test]
fn failures_to_start_have_typed_codes(cx: &mut TestAppContext) {
    let written = run_package(
        r#"
const codes = [];
for (const options of [
  { command: "knot-no-such-program" },
  { command: "true", cwd: "file:///knot/no/such/directory" },
  { command: "true", cwd: "https://example.com/" },
]) {
  try { await processes.run(options); } catch (error) { codes.push(error.code); }
}
await answer(codes);
"#,
        &["process"],
        cx,
    );
    assert_eq!(
        written,
        r#"["ProgramNotFound","WorkingDirectoryNotFound","InvalidUri"]"#
    );
}

#[gpui::test]
fn spawn_streams_output_and_input(cx: &mut TestAppContext) {
    let written = run_package(
        r#"
const child = await processes.spawn({ command: "cat" });
let text = "";
let seen;
const complete = new Promise((resolve) => { seen = resolve; });
child.onStdout((chunk) => { text += chunk; if (text === "hello world") seen(); });
await child.write("hello ");
await child.write("world");
await child.closeStdin();
await complete;
const exit = await child.exited;
let late = null;
try { await child.write("more"); } catch (error) { late = error.code; }
await answer({ text, exit, late });
"#,
        &["process"],
        cx,
    );
    let result: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(result["text"], "hello world");
    assert_eq!(result["exit"]["exitCode"], 0);
    assert_eq!(result["late"], "ProcessInputClosed");
}

#[gpui::test]
fn output_before_a_listener_attaches_is_held(cx: &mut TestAppContext) {
    let written = run_package(
        r#"
const child = await processes.spawn({ command: "sh", args: ["-c", "printf early; printf warn >&2"] });
await child.exited;
let out = "";
let err = "";
let done;
const both = new Promise((resolve) => { done = resolve; });
child.onStdout((chunk) => { out += chunk; if (out && err) done(); });
child.onStderr((chunk) => { err += chunk; if (out && err) done(); });
await both;
await answer({ out, err });
"#,
        &["process"],
        cx,
    );
    assert_eq!(written, r#"{"out":"early","err":"warn"}"#);
}

#[gpui::test]
fn a_flood_reaches_a_slow_listener_in_full(cx: &mut TestAppContext) {
    let written = run_package(
        r#"
const child = await processes.spawn({
  command: "sh",
  args: ["-c", "head -c 3000000 /dev/zero | tr '\\0' x"],
});
let bytes = 0;
let chunks = 0;
let drained;
const everything = new Promise((resolve) => { drained = resolve; });
child.onStdout(async (chunk) => {
  bytes += chunk.length;
  chunks++;
  await Promise.resolve();
  if (bytes === 3000000) drained();
});
await everything;
const exit = await child.exited;
await answer({ exit, bytes, chunks });
"#,
        &["process"],
        cx,
    );
    let result: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(result["bytes"], 3_000_000);
    assert_eq!(result["exit"]["exitCode"], 0);
    assert!(result["chunks"].as_u64().unwrap() > 1);
}

#[gpui::test]
fn kill_signals_the_process(cx: &mut TestAppContext) {
    let written = run_package(
        r#"
const child = await processes.spawn({ command: "sleep", args: ["300"] });
await child.kill("SIGTERM");
await answer(await child.exited);
"#,
        &["process"],
        cx,
    );
    assert_eq!(written, r#"{"exitCode":null,"signal":"SIGTERM"}"#);
}

/// A package that starts `sleep` through a shell that records its pid, so the
/// tests can look for the process.
fn start_sleeper(cx: &mut TestAppContext) -> (Loaded, std::path::PathBuf, tempfile::TempDir) {
    let marker = tempfile::tempdir().unwrap();
    let pid_file = marker.path().join("pid");
    let main = format!(
        r#"import {{ processes }} from "knot";
await processes.spawn({{ command: "sh", args: ["-c", "echo $$ > {}; sleep 300 & wait"] }});
"#,
        pid_file.display()
    );
    let loaded = load(&main, &["process"], cx);
    (loaded, pid_file, marker)
}

fn recorded_pid(pid_file: &Path, cx: &mut TestAppContext) -> i32 {
    let mut pid = None;
    crate::test_support::wait_until(cx, |_| {
        pid = std::fs::read_to_string(pid_file)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok());
        pid.is_some()
    });
    pid.unwrap()
}

fn is_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes for the process.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[gpui::test]
fn unloading_a_package_kills_its_processes_and_their_helpers(cx: &mut TestAppContext) {
    let (loaded, pid_file, _marker) = start_sleeper(cx);
    let shell = recorded_pid(&pid_file, cx);
    assert!(is_alive(shell));
    loaded
        .host
        .update(cx, |host, cx| host.unload_package_for_test(PACKAGE, cx));
    crate::test_support::wait_until(cx, |_| !is_alive(shell));
    // The shell's `sleep` child is in the same group, so nothing is left of
    // the group.
    assert!(!unsafe_group_alive(shell));
}

fn unsafe_group_alive(group: i32) -> bool {
    // SAFETY: signal 0 only probes for the group.
    unsafe { libc::killpg(group, 0) == 0 }
}

#[gpui::test]
fn restarting_a_package_kills_the_processes_of_the_old_lifecycle(cx: &mut TestAppContext) {
    let (loaded, pid_file, _marker) = start_sleeper(cx);
    let first = recorded_pid(&pid_file, cx);
    std::fs::remove_file(&pid_file).unwrap();
    let outcome = Arc::new(std::sync::Mutex::new(None));
    let sink = outcome.clone();
    cx.update(|cx| {
        loaded.host.update(cx, |host, cx| {
            let task = host.restart_package(PACKAGE.into(), None, cx);
            cx.spawn(async move |_, _| *sink.lock().unwrap() = Some(task.await))
                .detach();
        })
    });
    crate::test_support::wait_until(cx, |_| outcome.lock().unwrap().is_some());
    assert_eq!(
        outcome.lock().unwrap().take().unwrap().result,
        crate::app::extensions::load::LoadResult::Loaded
    );
    let second = recorded_pid(&pid_file, cx);
    assert_ne!(first, second);
    crate::test_support::wait_until(cx, |_| !is_alive(first));
    assert!(is_alive(second));
    loaded
        .host
        .update(cx, |host, cx| host.unload_package_for_test(PACKAGE, cx));
    crate::test_support::wait_until(cx, |_| !is_alive(second));
}

#[gpui::test]
fn the_personal_config_holds_every_entitlement(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("post-init.js"),
        r#"
import { processes, workbench } from "knot";
const result = await processes.run({ command: "sh", args: ["-c", "printf config"] });
await workbench.writeClipboardText(result.stdout);
"#,
    )
    .unwrap();
    install_globals(cx);
    install_launch_gate(cx);
    let host = cx.update(crate::app::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    assert_eq!(answer(cx), "config");
}

#[gpui::test]
fn a_bundled_package_that_declares_the_entitlement_may_run_programs(cx: &mut TestAppContext) {
    let extensions = tempfile::tempdir().unwrap();
    install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let storage = tempfile::tempdir().unwrap();
    host.update(cx, |host, cx| {
        host.set_test_storage_root(storage.path().to_owned(), cx);
        host.set_bundled_packages(&[knot_bundled::BundledSource {
            manifest: r#"{ "name": "@knot/runner", "apiVersion": 1, "version": "1.0.0", "main": "dist/main.js", "entitlements": ["process"] }"#,
            sources: &[(
                "dist/main.js",
                r#"import { processes, workbench } from "knot";
const result = await processes.run({ command: "sh", args: ["-c", "printf bundled"] });
await workbench.writeClipboardText(result.stdout);"#,
            )],
        }]);
        let root = extensions.path().to_owned();
        host.start_installed_extensions(root, cx);
    });
    wait_for_extension_report(&host, cx);
    assert_eq!(answer(cx), "bundled");
}

#[gpui::test]
fn quitting_kills_every_process(cx: &mut TestAppContext) {
    let (loaded, pid_file, _marker) = start_sleeper(cx);
    let shell = recorded_pid(&pid_file, cx);
    assert!(is_alive(shell));
    // `quit_after_storage` starts with this and then quits the application.
    loaded
        .host
        .update(cx, |host, _| host.kill_processes_for_quit());
    crate::test_support::wait_until(cx, |_| !is_alive(shell));
    assert!(!unsafe_group_alive(shell));
}

#[gpui::test]
fn the_checked_example_runs_git_and_follows_lines(cx: &mut TestAppContext) {
    let repository = tempfile::tempdir().unwrap();
    for arguments in [&["init", "-q"][..], &["config", "core.fsmonitor", "false"]] {
        let status = std::process::Command::new("git")
            .args(arguments)
            .current_dir(repository.path())
            .status()
            .unwrap();
        assert!(status.success());
    }
    std::fs::write(repository.path().join("new.txt"), "").unwrap();
    let uri = url::Url::from_directory_path(repository.path()).unwrap();
    let extensions = tempfile::tempdir().unwrap();
    install_extension(
        extensions.path(),
        PACKAGE,
        &[],
        &[
            (
                "main.js",
                &format!(
                    r#"import {{ workbench }} from "knot";
import {{ gitStatus, followLines }} from "./processes.js";
const status = await gitStatus("{uri}");
const lines = [];
const exit = await followLines("sh", ["-c", "printf 'a\nb\ntail'"], (line) => lines.push(line), {{ addEventListener() {{}} }});
await workbench.writeClipboardText(JSON.stringify({{ status, lines, exit }}));"#
                ),
            ),
            (
                "processes.js",
                include_str!("../../../../scripts/extensions/examples/processes.js"),
            ),
        ],
    );
    declare(extensions.path(), PACKAGE, &["process"]);
    cx.update(|cx| entitlements::decide(PACKAGE, &[Entitlement::Process], Decision::Granted, cx));
    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let storage = tempfile::tempdir().unwrap();
    host.update(cx, |host, cx| {
        host.set_test_storage_root(storage.path().to_owned(), cx)
    });
    let _window = empty_product_window(&documents, cx);
    let root = extensions.path().to_owned();
    host.update(cx, |host, cx| host.start_installed_extensions(root, cx));
    wait_for_extension_report(&host, cx);
    let result: serde_json::Value = serde_json::from_str(&answer(cx)).unwrap();
    assert_eq!(result["status"], serde_json::json!(["?? new.txt"]));
    assert_eq!(result["lines"], serde_json::json!(["a", "b", "tail"]));
    assert_eq!(result["exit"]["exitCode"], 0);
}
