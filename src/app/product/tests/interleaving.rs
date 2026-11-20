//! A package's code running while another of its invocations awaits: its
//! loading, and command chains that come back into it.

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
