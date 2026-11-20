//! A package's code running while another of its invocations awaits: its
//! loading, command chains that come back into it, overlapping commands
//! and children, and requests that outlive their command.

use super::extension_restart::{start_allowing_failures, start_loading};
use super::*;
use crate::app::extensions::host::StartupState;
use crate::app::extensions::load::LoadResult;

// Evaluation parks until the package's own setting listener runs, which its
// own command triggers.
const WAITS: &str = r#"
import { commands, settings, workbench } from "knot";
let release;
const gate = new Promise((resolve) => { release = resolve; });
await settings.define({ key: "waits.poke", type: "boolean", default: false, description: "Poke" });
await settings.observe("waits.poke", () => release());
await commands.register("waits.poke", async () => {
  await settings.set("waits.poke", true);
  await workbench.writeClipboardText("poked");
});
await gate;
await commands.register("waits.loaded", async () => { await workbench.writeClipboardText("loaded"); });
"#;

const FIRST: &str = r#"
import { commands, workbench } from "knot";
await commands.register("first.outer", async () => { await commands.invoke("second.middle"); });
await commands.register("first.inner", async () => { await workbench.writeClipboardText("inner"); });
await commands.register("first.recursive", async () => { await commands.invoke("second.back"); });
"#;

const SECOND: &str = r#"
import { commands, workbench } from "knot";
await commands.register("second.middle", async () => { await commands.invoke("first.inner"); });
await commands.register("second.back", async () => {
  const outcome = await commands.invoke("first.recursive");
  await workbench.writeClipboardText(outcome.kind);
});
"#;

// `gate.wait` parks until `gate.open` of its own package, or the `gate.open`
// setting another package writes, releases it.
const GATE: &str = r#"
import { commands, settings, workbench } from "knot";
let release;
const gate = new Promise((resolve) => { release = resolve; });
await settings.define({ key: "gate.open", type: "boolean", default: false, description: "Open" });
await settings.observe("gate.open", () => release());
await commands.register("gate.wait", async () => {
  await gate;
  await workbench.writeClipboardText("released");
});
await commands.register("gate.open", async () => { release(); });
"#;

const OPENER: &str = r#"
import { commands, settings } from "knot";
await commands.register("opener.open", async () => { await settings.set("gate.open", true); });
"#;

// Children of one parent that each need the other running, and children
// that run until cancelled: their parent settles only once both are.
const PARENT: &str = r#"
import { commands, settings, workbench } from "knot";
const flags = ["inner", "cross", "inner-started", "cross-started"];
for (const flag of flags) {
  await settings.define({ key: `parent.${flag}`, type: "boolean", default: false, description: flag });
}
let release;
const crossRan = new Promise((resolve) => { release = resolve; });
await settings.observe("parent.cross", () => release());
const started = new Set();
for (const flag of ["inner-started", "cross-started"]) {
  await settings.observe(`parent.${flag}`, async () => {
    started.add(flag);
    if (started.size === 2) await workbench.writeClipboardText("both started");
  });
}
await commands.register("parent.both", async () => {
  const outcomes = await Promise.all([commands.invoke("parent.inner"), commands.invoke("child.cross")]);
  await workbench.writeClipboardText(outcomes.map((outcome) => outcome.kind).join(","));
});
await commands.register("parent.inner", async () => { await crossRan; });
await commands.register("parent.hang", async () => {
  await Promise.all([commands.invoke("parent.hang-inner"), commands.invoke("child.hang")]);
});
await commands.register("parent.hang-inner", (context) => new Promise((resolve) => {
  context.signal.addEventListener("abort", () => resolve());
  settings.set("parent.inner-started", true);
}));
"#;

const CHILD: &str = r#"
import { commands, settings } from "knot";
await commands.register("child.cross", async () => { await settings.set("parent.cross", true); });
await commands.register("child.hang", (context) => new Promise((resolve) => {
  context.signal.addEventListener("abort", () => resolve());
  settings.set("parent.cross-started", true);
}));
"#;

const FIRES: &str = r#"
import { commands, workbench } from "knot";
await commands.register("fires.write", () => { workbench.writeClipboardText("late"); });
"#;

fn startup_state(cx: &TestAppContext) -> StartupState {
    cx.read(|cx| {
        crate::app::extensions::host::startup_report(cx)
            .unwrap()
            .state
    })
}

#[gpui::test]
fn a_command_and_a_listener_run_while_their_package_is_loading(cx: &mut TestAppContext) {
    let fixture = start_loading(&[("@example/waits", &[], WAITS)], cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .catalog()
                .borrow()
                .resolve("waits.poke")
                .is_ok()
        })
    });
    assert_ne!(startup_state(cx), StartupState::Complete);
    assert_eq!(fixture.clipboard_after("waits.poke", cx), "poked");
    wait_for_extension_report(&fixture.host, cx);
    assert_eq!(
        fixture.report_entry("@example/waits", cx),
        LoadResult::Loaded
    );
    assert_eq!(fixture.clipboard_after("waits.loaded", cx), "loaded");
}

#[gpui::test]
fn a_chain_comes_back_through_another_registration_but_never_recurses(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/first", &[], FIRST),
            ("@example/second", &[], SECOND),
        ],
        cx,
    );
    assert_eq!(fixture.clipboard_after("first.outer", cx), "inner");
    assert_eq!(
        fixture.clipboard_after("first.recursive", cx),
        "unavailable"
    );
}

fn wait_for_clipboard(text: &str, cx: &mut TestAppContext) {
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .is_some_and(|current| current == text)
        })
    });
}

#[gpui::test]
fn commands_of_two_packages_overlap(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/gate", &[], GATE),
            ("@example/opener", &[], OPENER),
        ],
        cx,
    );
    let waiting = fixture.command("gate.wait", cx);
    let opener = fixture.command("opener.open", cx);
    assert_eq!(wait_for_command(opener, cx), CommandOutcome::Completed);
    assert_eq!(wait_for_command(waiting, cx), CommandOutcome::Completed);
    wait_for_clipboard("released", cx);
}

#[gpui::test]
fn commands_of_one_package_overlap(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(&[("@example/gate", &[], GATE)], cx);
    let waiting = fixture.command("gate.wait", cx);
    let opener = fixture.command("gate.open", cx);
    assert_eq!(wait_for_command(opener, cx), CommandOutcome::Completed);
    assert_eq!(wait_for_command(waiting, cx), CommandOutcome::Completed);
    wait_for_clipboard("released", cx);
}

#[gpui::test]
fn a_command_runs_an_inline_and_a_cross_package_child_at_once(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/parent", &[], PARENT),
            ("@example/child", &[], CHILD),
        ],
        cx,
    );
    assert_eq!(
        fixture.clipboard_after("parent.both", cx),
        "completed,completed"
    );

    let hanging = fixture.command("parent.hang", cx);
    wait_for_clipboard("both started", cx);
    fixture
        .host
        .update(cx, |host, cx| host.cancel_active_test_commands(cx));
    assert_eq!(wait_for_command(hanging, cx), CommandOutcome::Cancelled);
}

#[gpui::test]
fn a_command_completes_without_waiting_for_a_request_it_left_running(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(&[("@example/fires", &[], FIRES)], cx);
    fixture
        .host
        .update(cx, |host, _| host.hold_clipboard_writes());
    let execution = fixture.command("fires.write", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert_eq!(
        cx.read(|cx| fixture.host.read(cx).pending_view_request_count()),
        1
    );
    // The request still runs once its command is gone.
    fixture
        .host
        .update(cx, |host, cx| host.release_view_requests(cx));
    wait_for_clipboard("late", cx);
}
