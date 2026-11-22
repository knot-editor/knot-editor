//! Contracts between packages: a provider serves a contract its contract
//! module defines, a consumer that requires the provider imports it and calls
//! it across isolates, and the host validates every argument and result.

use super::extension_restart::{Fixture, start_with_contracts};
use super::*;

const PROVIDER: &str = "@example/provider";
const CONSUMER: &str = "@example/consumer";

const GREETER: &str = r#"
import { defineContract, s } from "knot/contract";
export const greeter = defineContract("greeter", {
  greet: s.fn(s.string(), s.object({ greeting: s.string(), window: s.boolean() })),
  broken: s.fn(s.void(), s.string()),
  fail: s.fn(s.void(), s.void()),
});
"#;

const SERVES_GREETER: &str = r#"
import { commands, windows, workbench } from "knot";
import { greeter } from "./contract.js";
const registration = await greeter.serve({
  async greet(name, context) {
    const window = context.window === null ? null : await windows.get(context.window);
    return { greeting: `hello ${name}`, window: window !== null };
  },
  broken() { return 42; },
  fail() { throw new Error("provider broke"); },
});
await commands.register("provider.dispose", async () => { await registration[Symbol.asyncDispose](); });
await commands.register("provider.self", async () => {
  try { await (await greeter.client()).greet("me"); }
  catch (error) { await workbench.writeClipboardText(error.code); }
});
await commands.register("provider.run", async () => { await commands.invoke("consumer.greet"); });
"#;

/// The provider at a later revision, whose `greet` answers text.
const GREETER_V2: &str = r#"
import { defineContract, s } from "knot/contract";
export const greeter = defineContract("greeter", {
  greet: s.fn(s.string(), s.string()),
  broken: s.fn(s.void(), s.string()),
  fail: s.fn(s.void(), s.void()),
});
"#;

const SERVES_GREETER_V2: &str = r#"
import { greeter } from "./contract.js";
await greeter.serve({ greet: (name) => `v2 ${name}`, broken: () => "", fail() {} });
"#;

const CONSUMES_GREETER: &str = r#"
import { commands, workbench } from "knot";
import { greeter } from "@example/provider";
let held = null;
const show = (value) => workbench.writeClipboardText(JSON.stringify(value ?? null));
const attempt = async (run) => {
  try { await show(await run()); }
  catch (error) { await show({ code: error.code, message: error.message }); }
};
await commands.register("consumer.greet", () => attempt(async () => {
  held = await greeter.client();
  return held.greet("knot");
}));
await commands.register("consumer.held", () => attempt(() => held.greet("again")));
await commands.register("consumer.invalid", () => attempt(async () => (await greeter.client()).greet(1)));
await commands.register("consumer.broken", () => attempt(async () => (await greeter.client()).broken()));
await commands.register("consumer.fail", () => attempt(async () => (await greeter.client()).fail()));
"#;

const OVERRIDES_GREETER: &str = r#"
import { commands } from "knot";
import { greeter } from "@example/provider";
const registration = await greeter.serve({
  greet: (name) => ({ greeting: `hi ${name}`, window: false }),
  broken: () => "",
  fail() {},
});
await commands.register("override.dispose", async () => { await registration[Symbol.asyncDispose](); });
"#;

fn start(cx: &mut TestAppContext) -> Fixture {
    start_with_contracts(
        &[
            (PROVIDER, &[], SERVES_GREETER, Some(GREETER)),
            (CONSUMER, &[PROVIDER], CONSUMES_GREETER, None),
        ],
        cx,
    )
}

fn json(fixture: &Fixture, command: &'static str, cx: &mut TestAppContext) -> serde_json::Value {
    serde_json::from_str(&fixture.clipboard_after(command, cx)).unwrap()
}

fn failure(fixture: &Fixture, command: &'static str, cx: &mut TestAppContext) -> (String, String) {
    let answer = json(fixture, command, cx);
    let field = |name: &str| answer[name].as_str().unwrap_or_default().to_owned();
    (field("code"), field("message"))
}

#[gpui::test]
fn a_call_runs_in_the_provider_package_with_the_callers_window(cx: &mut TestAppContext) {
    let fixture = start(cx);
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::json!({ "greeting": "hello knot", "window": true })
    );
    let (code, message) = failure(&fixture, "consumer.fail", cx);
    assert_eq!(code, "ProviderFailure");
    assert!(message.contains("provider broke"), "{message}");
    assert_eq!(
        fixture.clipboard_after("provider.self", cx),
        "UnsupportedOperation"
    );
}

#[gpui::test]
fn arguments_and_results_outside_the_schema_reject_the_call(cx: &mut TestAppContext) {
    let fixture = start(cx);
    assert_eq!(
        failure(&fixture, "consumer.invalid", cx),
        (
            "ContractViolation".into(),
            "@example/provider greeter: greet: argument: expected string, found number 1 \
             (the provider's schema)"
                .into()
        )
    );
    assert_eq!(
        failure(&fixture, "consumer.broken", cx),
        (
            "ContractViolation".into(),
            "@example/provider greeter: broken: result: expected string, found number 42 \
             (the provider's schema)"
                .into()
        )
    );
}

#[gpui::test]
fn a_consumer_built_against_another_revision_fails_per_call(cx: &mut TestAppContext) {
    let fixture = start(cx);
    fixture.rewrite_file(PROVIDER, "contract.js", GREETER_V2);
    fixture.rewrite(PROVIDER, SERVES_GREETER_V2);
    assert_eq!(
        fixture.restart(PROVIDER, cx).0,
        "Restarted @example/provider"
    );
    // The consumer still holds the first revision's contract module.
    assert_eq!(
        failure(&fixture, "consumer.greet", cx),
        (
            "ContractViolation".into(),
            "@example/provider greeter: greet: result: expected object, found string \"v2 knot\" \
             (the consumer's schema)"
                .into()
        )
    );
}

#[gpui::test]
fn a_disposed_or_restarted_registration_rejects_held_clients(cx: &mut TestAppContext) {
    let fixture = start(cx);
    json(&fixture, "consumer.greet", cx);
    assert_eq!(
        fixture.restart(PROVIDER, cx).0,
        "Restarted @example/provider"
    );
    assert_eq!(failure(&fixture, "consumer.held", cx).0, "ContractNotFound");
    assert_eq!(
        json(&fixture, "consumer.greet", cx)["greeting"],
        "hello knot"
    );

    fixture.clipboard_after("provider.dispose", cx);
    assert_eq!(failure(&fixture, "consumer.held", cx).0, "ContractNotFound");
    assert_eq!(
        failure(&fixture, "consumer.greet", cx),
        (
            "ContractNotFound".into(),
            "no package serves @example/provider greeter".into()
        )
    );
}

#[gpui::test]
fn a_client_binds_the_latest_registration_and_disposal_uncovers_the_previous(
    cx: &mut TestAppContext,
) {
    let fixture = start_with_contracts(
        &[
            (PROVIDER, &[], SERVES_GREETER, Some(GREETER)),
            ("@example/override", &[PROVIDER], OVERRIDES_GREETER, None),
            (
                CONSUMER,
                &[PROVIDER, "@example/override"],
                CONSUMES_GREETER,
                None,
            ),
        ],
        cx,
    );
    assert_eq!(
        json(&fixture, "consumer.greet", cx),
        serde_json::json!({ "greeting": "hi knot", "window": false })
    );
    fixture.clipboard_after("override.dispose", cx);
    assert_eq!(failure(&fixture, "consumer.held", cx).0, "ContractNotFound");
    assert_eq!(
        json(&fixture, "consumer.greet", cx)["greeting"],
        "hello knot"
    );
}

#[gpui::test]
fn a_call_back_into_a_package_waiting_on_the_caller_runs(cx: &mut TestAppContext) {
    let fixture = start(cx);
    assert_eq!(
        json(&fixture, "provider.run", cx),
        serde_json::json!({ "greeting": "hello knot", "window": true })
    );
}

#[gpui::test]
fn importing_a_package_that_is_not_required_fails_to_load(cx: &mut TestAppContext) {
    let fixture = start_with_contracts(
        &[
            (PROVIDER, &[], SERVES_GREETER, Some(GREETER)),
            (CONSUMER, &[], CONSUMES_GREETER, None),
        ],
        cx,
    );
    match fixture.report_entry(CONSUMER, cx) {
        crate::app::extensions::load::LoadResult::Failed(cause) => {
            assert!(cause.contains("or a required package"), "{cause}")
        }
        result => panic!("the consumer loaded: {result:?}"),
    }
}

#[gpui::test]
fn a_package_requiring_a_provider_that_failed_to_start_does_not_load(cx: &mut TestAppContext) {
    let broken = format!("{SERVES_GREETER}\nthrow new Error('broken provider');");
    let fixture = start_with_contracts(
        &[
            (PROVIDER, &[], &broken, Some(GREETER)),
            (CONSUMER, &[PROVIDER], CONSUMES_GREETER, None),
        ],
        cx,
    );
    use crate::app::extensions::load::LoadResult;
    assert!(matches!(
        fixture.report_entry(PROVIDER, cx),
        LoadResult::Failed(_)
    ));
    assert!(!matches!(
        fixture.report_entry(CONSUMER, cx),
        LoadResult::Loaded
    ));
}

/// Personal config serves a contract of an installed package from post-init,
/// without a manifest to require it, so every consumer reaches the override.
#[gpui::test]
fn personal_config_replaces_an_installed_provider_for_every_consumer(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join("post-init.js"), OVERRIDES_GREETER).unwrap();
    install_package(
        extensions.path(),
        PROVIDER,
        &[],
        Some("contract.js"),
        &[("main.js", SERVES_GREETER), ("contract.js", GREETER)],
    );
    install_extension(
        extensions.path(),
        CONSUMER,
        &[PROVIDER],
        &[("main.js", CONSUMES_GREETER)],
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
        r#"{"greeting":"hi knot","window":false}"#
    );
}

/// The authoring example, whose contract and provider modules a consumer
/// outside the package imports and calls.
#[gpui::test]
fn the_authoring_example_serves_and_is_called_across_packages(cx: &mut TestAppContext) {
    const EXAMPLE_CONTRACT: &str =
        include_str!("../../../../scripts/extensions/examples/contract.js");
    const EXAMPLE_PROVIDER: &str = include_str!("../../../../scripts/extensions/examples/serve.js");
    const CALLS_EXAMPLE: &str = r#"
import { commands, workbench } from "knot";
import { greeter } from "@example/provider";
await commands.register("consumer.example", async () => {
  const client = await greeter.client();
  const answer = await client.greet({ name: "knot", shout: true });
  await client.reset();
  await workbench.writeClipboardText(JSON.stringify(answer));
});
"#;
    let fixture = start_with_contracts(
        &[
            (PROVIDER, &[], EXAMPLE_PROVIDER, Some(EXAMPLE_CONTRACT)),
            (CONSUMER, &[PROVIDER], CALLS_EXAMPLE, None),
        ],
        cx,
    );
    assert_eq!(
        json(&fixture, "consumer.example", cx),
        serde_json::json!({ "greeting": "HELLO KNOT", "window": true })
    );
}
