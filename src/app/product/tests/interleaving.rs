//! A package's code running while another of its invocations awaits.

use super::extension_restart::start_loading;
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
