//! Named providers: one package registers, others look up and call across
//! isolates, with the caller's window and lifecycle-owned registrations.

use super::extension_restart::{Fixture, start_allowing_failures};
use super::*;
use crate::app::extensions::load::LoadResult;

const PROVIDER: &str = r#"
import { commands, providers, windows, workbench } from "knot";
const registration = await providers.register("sample.greet", {
  async greet(name, context) {
    const window = context.window === null ? null : await windows.get(context.window);
    return { greeting: `hello ${name}`, window: window !== null };
  },
  fail() { throw new Error("provider broke"); },
});
await commands.register("provider.dispose", async () => { await registration.dispose(); });
await commands.register("provider.self", async () => {
  try { await (await providers.get("sample.greet")).greet("me"); }
  catch (error) { await workbench.writeClipboardText(error.code); }
});
"#;

// A command of the provider's package that runs a consumer's command, which
// calls the provider back while the provider's package is still waiting.
const PROVIDER_RUNS_CONSUMER: &str = r#"
import { commands, providers } from "knot";
await providers.register("sample.greet", { greet(name) { return `hello ${name}`; } });
await commands.register("provider.run", async () => { await commands.invoke("consumer.call"); });
"#;

const CONSUMER_CALLS_BACK: &str = r#"
import { commands, providers, workbench } from "knot";
await commands.register("consumer.call", async () => {
  try { await (await providers.get("sample.greet")).greet("knot"); await workbench.writeClipboardText("called"); }
  catch (error) { await workbench.writeClipboardText(error.code); }
});
await commands.register("consumer.direct", async () => {
  await workbench.writeClipboardText(await (await providers.get("sample.greet")).greet("knot"));
});
"#;

const PROVIDER_V2: &str = r#"
import { providers } from "knot";
await providers.register("sample.greet", { greet(name) { return { greeting: `v2 ${name}` }; } });
"#;

const OVERRIDE: &str = r#"
import { providers } from "knot";
await providers.register("sample.greet", { greet(name) { return { greeting: `hi ${name}` }; } });
"#;

const CONSUMER: &str = r#"
import { commands, providers, workbench } from "knot";
let held = null;
const show = (value) => workbench.writeClipboardText(JSON.stringify(value));
const failure = (error) => show({ code: error.code, message: error.message });
await commands.register("consumer.greet", async () => {
  held = await providers.get("sample.greet");
  await show(held === null ? null : await held.greet("knot"));
});
await commands.register("consumer.held", async () => {
  try { await show(await held.greet("again")); } catch (error) { await failure(error); }
});
await commands.register("consumer.fail", async () => {
  try { await (await providers.get("sample.greet")).fail(); } catch (error) { await failure(error); }
});
await commands.register("consumer.all", async () => {
  const all = await providers.all("sample.greet");
  await show(await Promise.all(all.map((handle) => handle.greet("all"))));
});
"#;

const COMPLETION_FIRST: &str = r#"
import { providers } from "knot";
await providers.register("editor.completion", {
  provideCompletions() { return [{ label: "first", insertText: "first" }]; },
});
"#;

const COMPLETION_SECOND: &str = r#"
import { providers } from "knot";
await providers.register("editor.completion", {
  provideCompletions() { return [{ label: "second", insertText: "second" }]; },
});
"#;

fn json(fixture: &Fixture, command: &'static str, cx: &mut TestAppContext) -> serde_json::Value {
    serde_json::from_str(&fixture.clipboard_after(command, cx)).unwrap()
}

#[gpui::test]
fn a_call_runs_in_the_provider_package_with_the_callers_window(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/provider", &[], PROVIDER),
            ("@example/consumer", &[], CONSUMER),
        ],
        cx,
    );
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::json!({ "greeting": "hello knot", "window": true })
    );
    let failure = json(&fixture, "consumer.fail", cx);
    assert_eq!(failure["code"], "ProviderFailure");
    assert!(
        failure["message"]
            .as_str()
            .unwrap()
            .contains("provider broke"),
        "{failure}"
    );
    assert_eq!(
        fixture.clipboard_after("provider.self", cx),
        "UnsupportedOperation"
    );
}

#[gpui::test]
fn a_call_back_into_a_package_waiting_on_the_caller_is_rejected(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/provider", &[], PROVIDER_RUNS_CONSUMER),
            ("@example/consumer", &[], CONSUMER_CALLS_BACK),
        ],
        cx,
    );
    assert_eq!(fixture.clipboard_after("provider.run", cx), "ProviderBusy");
    // Called on its own, the same command reaches the provider.
    assert_eq!(fixture.clipboard_after("consumer.direct", cx), "hello knot");
}

#[gpui::test]
fn get_returns_the_latest_registration_and_all_every_one_in_order(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/provider", &[], PROVIDER),
            ("@example/override", &["@example/provider"], OVERRIDE),
            ("@example/consumer", &[], CONSUMER),
        ],
        cx,
    );
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::json!({ "greeting": "hi knot" })
    );
    assert_eq!(
        json(&fixture, "consumer.all", cx),
        serde_json::json!([
            { "greeting": "hello all", "window": true },
            { "greeting": "hi all" },
        ])
    );
}

#[gpui::test]
fn a_disposed_or_restarted_provider_rejects_held_handles(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/provider", &[], PROVIDER),
            ("@example/consumer", &[], CONSUMER),
        ],
        cx,
    );
    json(&fixture, "consumer.greet", cx);
    fixture.rewrite("@example/provider", PROVIDER_V2);
    assert_eq!(
        fixture.restart("@example/provider", cx).0,
        "Restarted @example/provider"
    );
    assert_eq!(
        json(&fixture, "consumer.held", cx)["code"],
        "ProviderNotFound"
    );
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::json!({ "greeting": "v2 knot" })
    );

    fixture.rewrite("@example/provider", PROVIDER);
    fixture.restart("@example/provider", cx);
    json(&fixture, "consumer.greet", cx);
    fixture.clipboard_after("provider.dispose", cx);
    assert_eq!(
        json(&fixture, "consumer.held", cx)["code"],
        "ProviderNotFound"
    );
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::Value::Null
    );
}

#[gpui::test]
fn a_provider_package_that_failed_to_start_provides_nothing(cx: &mut TestAppContext) {
    let broken = format!("{PROVIDER}\nthrow new Error('broken provider');");
    let fixture = start_allowing_failures(
        &[
            ("@example/provider", &[], &broken),
            ("@example/consumer", &[], CONSUMER),
        ],
        cx,
    );
    assert!(matches!(
        fixture.report_entry("@example/provider", cx),
        LoadResult::Failed(_)
    ));
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::Value::Null
    );
    assert_eq!(json(&fixture, "consumer.all", cx), serde_json::json!([]));
}

#[gpui::test]
fn personal_config_replaces_an_installed_provider_for_every_consumer(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join("post-init.js"), OVERRIDE).unwrap();
    install_extension(
        extensions.path(),
        "@example/provider",
        &[],
        &[("main.js", PROVIDER)],
    );
    install_extension(
        extensions.path(),
        "@example/consumer",
        &[],
        &[("main.js", CONSUMER)],
    );
    install_globals(cx);
    install_launch_gate(cx);
    let host = cx.update(crate::app::extensions::host::install);
    cx.update(|cx| super::super::dispatch_launch_request(None, cx));
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let native_window = cx.windows()[0];
    let window = cx.read(|cx| {
        native_window
            .downcast::<KnotWindow>()
            .unwrap()
            .entity(cx)
            .unwrap()
    });
    let execution = dispatch_product_command(&window, native_window, "consumer.greet", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert_eq!(
        cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        r#"{"greeting":"hi knot"}"#
    );
}

#[gpui::test]
fn every_completion_provider_contributes_not_only_the_latest(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/first", &[], COMPLETION_FIRST),
            ("@example/second", &["@example/first"], COMPLETION_SECOND),
        ],
        cx,
    );
    let execution = fixture.command(
        super::super::super::commands::dispatch::SHOW_COMPLETIONS_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let editor = fixture.editor(cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            editor
                .read(cx)
                .completion_state()
                .is_some_and(|(_, pending, _, _)| pending == 0)
        })
    });
    let items = cx.read(|cx| {
        editor
            .read(cx)
            .completion_surface()
            .unwrap()
            .snapshot()
            .items
            .iter()
            .map(|item| (item.label.clone(), item.provider_label.clone()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        items,
        vec![
            ("first".to_owned(), "@example/first".to_owned()),
            ("second".to_owned(), "@example/second".to_owned()),
        ]
    );
}
