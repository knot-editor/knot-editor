//! The contracts between `@knot/vcs` and its backends, run with two test
//! backends and a probe that calls them as `@knot/vcs` does.
use gpui::TestAppContext;
use knot::test_support::{CommandOutcome, TestProduct, TestWindow};
use serde_json::{Value, json};

fn package(name: &str) -> &'static knot_bundled::BundledSource {
    knot_ext_vcs::PACKAGES
        .iter()
        .find(|package| package.manifest.contains(&format!("\"name\": \"{name}\"")))
        .unwrap()
}

/// A backend named `id` that answers with `id` in every result, and stops
/// serving, or withdraws, when told to.
fn backend(id: &'static str) -> knot_bundled::BundledSource {
    let manifest = format!(
        r#"{{ "name": "@knot/test-{id}", "apiVersion": 1, "version": "0.1.0", "main": "dist/main.js", "requires": ["@knot/vcs"] }}"#
    );
    let main = format!(
        r#"
import {{ backendContract, vcs }} from "@knot/vcs";
import {{ commands }} from "knot";
const serving = await backendContract("{id}").serve({{
  repositories: (root) => [`${{root}}/{id}`],
  repositoryOf: (file) => `{id}:${{file}}`,
  baseline: ({{ repository, file }}) => `{id}|${{repository}}|${{file}}`,
  status: () => [],
  ref: () => null,
}});
await (await vcs.client()).register("{id}");
await commands.register("{id}.stop", async () => {{ await serving[Symbol.asyncDispose](); }});
await commands.register("{id}.withdraw", async () => {{ await (await vcs.client()).unregister("{id}"); }});
"#
    );
    knot_bundled::BundledSource {
        manifest: Box::leak(manifest.into_boxed_str()),
        sources: Box::leak(Box::new([(
            "dist/main.js",
            &*Box::leak(main.into_boxed_str()),
        )])),
    }
}

const PROBE_MANIFEST: &str = r#"{ "name": "@knot/test-probe", "apiVersion": 1, "version": "0.1.0", "main": "dist/main.js", "requires": ["@knot/vcs"] }"#;
/// A package that calls the contracts as `@knot/vcs` does and reports the answers.
const PROBE: &str = r#"
import { backendContract, vcs } from "@knot/vcs";
import { commands, workbench } from "knot";
const report = (value) => workbench.writeClipboardText(JSON.stringify(value));
const attempt = async (run) => {
  try { return await run(); } catch (error) { return { error: error.code }; }
};
await commands.register("probe.backends", async () => report(await (await vcs.client()).backends()));
await commands.register("probe.calls", async () => report(await attempt(async () => {
  const answers = {};
  for (const id of ["alpha", "beta"]) {
const client = await backendContract(id).client();
answers[id] = [
  await client.repositories("file:///work"),
  await client.repositoryOf("file:///work/a.txt"),
  await client.baseline({ repository: "file:///work", file: "file:///work/a.txt" }),
];
  }
  return answers;
})));
await commands.register("probe.same", async () => report(backendContract("alpha") === backendContract("alpha")
  && backendContract("alpha") !== backendContract("beta")));
await commands.register("probe.changed", async () => report(await attempt(async () => {
  await (await vcs.client()).changed({ backend: "alpha", repository: "file:///work" });
  return "ok";
})));
"#;

/// `@knot/vcs`, two backends, and a probe that reports what a client of each
/// contract, and of `vcs`, answers.
fn product() -> &'static [knot_bundled::BundledSource] {
    let packages = vec![
        *package("@knot/diff"),
        *package("@knot/vcs"),
        backend("alpha"),
        backend("beta"),
        knot_bundled::BundledSource {
            manifest: PROBE_MANIFEST,
            sources: &[("dist/main.js", PROBE)],
        },
    ];
    Box::leak(packages.into_boxed_slice())
}

fn answer(window: &TestWindow, command: &str, cx: &mut TestAppContext) -> Value {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()));
    assert_eq!(window.run_command(command, cx), CommandOutcome::Completed);
    let text = cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap());
    serde_json::from_str(&text).unwrap()
}

#[gpui::test]
fn a_contract_the_factory_defines_is_owned_by_vcs_and_served_by_each_backend(
    cx: &mut TestAppContext,
) {
    let product = TestProduct::launch(product(), cx);
    let window = product.active_window(cx);

    // Each backend serves its own contract, and two packages that import the
    // factory bind the same ones, so two backends answer at once.
    assert_eq!(
        answer(&window, "probe.calls", cx),
        json!({
            "alpha": [
                ["file:///work/alpha"],
                "alpha:file:///work/a.txt",
                "alpha|file:///work|file:///work/a.txt",
            ],
            "beta": [
                ["file:///work/beta"],
                "beta:file:///work/a.txt",
                "beta|file:///work|file:///work/a.txt",
            ],
        })
    );
    assert_eq!(answer(&window, "probe.same", cx), json!(true));
    assert_eq!(
        answer(&window, "probe.backends", cx),
        json!(["alpha", "beta"])
    );
    assert_eq!(answer(&window, "probe.changed", cx), json!("ok"));
}

#[gpui::test]
fn a_backend_that_stops_serving_or_withdraws_leaves_the_others(cx: &mut TestAppContext) {
    let product = TestProduct::launch(product(), cx);
    let window = product.active_window(cx);

    assert_eq!(
        window.run_command("alpha.stop", cx),
        CommandOutcome::Completed
    );
    assert_eq!(answer(&window, "probe.backends", cx), json!(["beta"]));
    assert_eq!(
        answer(&window, "probe.calls", cx),
        json!({ "error": "ContractNotFound" })
    );

    assert_eq!(
        window.run_command("beta.withdraw", cx),
        CommandOutcome::Completed
    );
    assert_eq!(answer(&window, "probe.backends", cx), json!([]));
}

#[gpui::test]
fn the_git_backend_registers_with_vcs(cx: &mut TestAppContext) {
    let mut packages = knot_ext_vcs::PACKAGES.to_vec();
    packages.push(knot_bundled::BundledSource {
        manifest: PROBE_MANIFEST,
        sources: &[("dist/main.js", PROBE)],
    });
    let product = TestProduct::launch(Box::leak(packages.into_boxed_slice()), cx);
    let window = product.active_window(cx);

    assert_eq!(answer(&window, "probe.backends", cx), json!(["git"]));
}
