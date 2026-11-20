//! Entitlement consent at startup, and the same question when restarting.

use super::extension_restart::resolves;
use super::operations::empty_product_window;
use super::*;
use crate::app::extensions::consent::EntitlementConsentView;
use crate::app::extensions::entitlements;
use crate::app::extensions::host::ProductExtensionHost;
use crate::app::extensions::load::LoadResult;
use knot_model::extensions::entitlements::Decision;
use knot_model::extensions::package::Entitlement;

const PLAIN: &str = r#"
import { commands, workbench } from "knot";
await commands.register("entitled.ping", async () => { await workbench.writeClipboardText("ping"); });
"#;

struct Setup {
    host: Entity<ProductExtensionHost>,
    window: Entity<KnotWindow>,
    extensions: tempfile::TempDir,
    _storage: tempfile::TempDir,
}

/// Installs `packages` (name, requirements, declared entitlements) and starts
/// loading them beside an empty product window.
fn begin(packages: &[(&str, &[&str], &[&str])], cx: &mut TestAppContext) -> Setup {
    let extensions = tempfile::tempdir().unwrap();
    for (name, requires, declared) in packages {
        install_extension(extensions.path(), name, requires, &[("main.js", PLAIN)]);
        declare(extensions.path(), name, declared);
    }
    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let storage = tempfile::tempdir().unwrap();
    host.update(cx, |host, cx| {
        host.set_test_storage_root(storage.path().to_owned(), cx)
    });
    let (window, _) = empty_product_window(&documents, cx);
    let setup = Setup {
        host,
        window,
        extensions,
        _storage: storage,
    };
    setup.start(cx);
    setup
}

fn declare(root: &Path, name: &str, declared: &[&str]) {
    let path = root.join(name).join("knot.jsonc");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["entitlements"] = serde_json::json!(declared);
    std::fs::write(path, manifest.to_string()).unwrap();
}

impl Setup {
    fn start(&self, cx: &mut TestAppContext) {
        let root = self.extensions.path().to_owned();
        self.host
            .update(cx, |host, cx| host.start_installed_extensions(root, cx));
    }

    fn consent_window(
        &self,
        cx: &mut TestAppContext,
    ) -> Option<gpui::WindowHandle<EntitlementConsentView>> {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<EntitlementConsentView>())
    }

    fn wait_for_consent(
        &self,
        cx: &mut TestAppContext,
    ) -> gpui::WindowHandle<EntitlementConsentView> {
        crate::test_support::wait_until(cx, |cx| self.consent_window(cx).is_some());
        self.consent_window(cx).unwrap()
    }

    fn finish(&self, cx: &mut TestAppContext) -> Vec<(String, LoadResult)> {
        wait_for_extension_report(&self.host, cx)
            .report
            .entries
            .into_iter()
            .map(|entry| (entry.name.unwrap(), entry.result))
            .collect()
    }

    /// Restart `name` from `window`, answering a confirmation, if one opens,
    /// with `answer`. Returns the entry and whether a prompt was shown.
    fn restart(&self, name: &str, answer: &str, cx: &mut TestAppContext) -> (LoadResult, bool) {
        let result = Arc::new(std::sync::Mutex::new(None));
        let sink = result.clone();
        let window = self.window.downgrade();
        cx.update(|cx| {
            self.host.update(cx, |host, cx| {
                let task = host.restart_package(name.into(), Some(window), cx);
                cx.spawn(async move |_, _| *sink.lock().unwrap() = Some(task.await))
                    .detach();
            })
        });
        let mut prompted = false;
        crate::test_support::wait_until(cx, |cx| {
            if cx.has_pending_prompt() {
                prompted = true;
                cx.simulate_prompt_answer(answer);
            }
            result.lock().unwrap().is_some()
        });
        let entry = result.lock().unwrap().take().unwrap();
        (entry.result, prompted)
    }
}

fn decision(name: &str, cx: &mut TestAppContext) -> Option<Decision> {
    cx.read(|cx| entitlements::grants(cx).decision(name, Entitlement::Process))
}

fn result<'a>(report: &'a [(String, LoadResult)], name: &str) -> &'a LoadResult {
    &report.iter().find(|(entry, _)| entry == name).unwrap().1
}

#[gpui::test]
fn allowing_loads_the_package_and_remembers_the_decision(cx: &mut TestAppContext) {
    let setup = begin(&[("@example/git", &[], &["process"])], cx);
    let window = setup.wait_for_consent(cx);
    window
        .update(cx, |view, _, cx| {
            let listed = view.listed_for_test();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].name, "@example/git");
            assert_eq!(listed[0].version, "1.0.0");
            assert_eq!(listed[0].entitlements, [Entitlement::Process]);
            view.decide_for_test("@example/git", Decision::Granted, cx);
            view.continue_for_test(cx);
        })
        .unwrap();
    let report = setup.finish(cx);
    assert_eq!(result(&report, "@example/git"), &LoadResult::Loaded);
    assert_eq!(decision("@example/git", cx), Some(Decision::Granted));
}

#[gpui::test]
fn denying_keeps_the_package_unloaded_and_fails_its_dependents(cx: &mut TestAppContext) {
    let setup = begin(
        &[
            ("@example/git", &[], &["process"]),
            ("@example/dependent", &["@example/git"], &[]),
            ("@example/plain", &[], &[]),
        ],
        cx,
    );
    let window = setup.wait_for_consent(cx);
    window
        .update(cx, |view, _, cx| {
            // Only the package with an entitlement is asked about.
            assert_eq!(view.listed_for_test().len(), 1);
            view.decide_for_test("@example/git", Decision::Denied, cx);
            view.continue_for_test(cx);
        })
        .unwrap();
    let report = setup.finish(cx);
    assert_eq!(
        result(&report, "@example/git"),
        &LoadResult::NotLoaded("not loaded: entitlement `process` denied".into())
    );
    assert!(
        matches!(result(&report, "@example/dependent"), LoadResult::Failed(cause) if cause.contains("@example/git") && cause.contains("denied"))
    );
    assert_eq!(result(&report, "@example/plain"), &LoadResult::Loaded);
    assert_eq!(decision("@example/git", cx), Some(Decision::Denied));
}

#[gpui::test]
fn leaving_a_package_undecided_stores_nothing_and_asks_again_next_startup(cx: &mut TestAppContext) {
    let setup = begin(
        &[
            ("@example/git", &[], &["process"]),
            ("@example/other", &[], &["process"]),
        ],
        cx,
    );
    let window = setup.wait_for_consent(cx);
    window
        .update(cx, |view, _, cx| {
            view.decide_for_test("@example/git", Decision::Granted, cx);
            // The window's close button answers with nothing, even after a decision.
            view.close_for_test(cx);
        })
        .unwrap();
    let report = setup.finish(cx);
    for name in ["@example/git", "@example/other"] {
        assert_eq!(
            result(&report, name),
            &LoadResult::NotLoaded("not loaded: entitlement `process` not decided".into())
        );
        assert_eq!(decision(name, cx), None);
    }

    cx.update(crate::app::extensions::consent::close);
    setup.start(cx);
    let window = setup.wait_for_consent(cx);
    window
        .update(cx, |view, _, cx| {
            assert_eq!(view.listed_for_test().len(), 2);
            // Continuing without deciding a package also stores nothing for it.
            view.decide_for_test("@example/other", Decision::Granted, cx);
            view.continue_for_test(cx);
        })
        .unwrap();
    let report = setup.finish(cx);
    assert_eq!(result(&report, "@example/other"), &LoadResult::Loaded);
    assert!(matches!(
        result(&report, "@example/git"),
        LoadResult::NotLoaded(cause) if cause.contains("not decided")
    ));
    assert_eq!(decision("@example/git", cx), None);
}

#[gpui::test]
fn recorded_decisions_apply_without_asking(cx: &mut TestAppContext) {
    cx.update(|cx| {
        entitlements::decide(
            "@example/allowed",
            &[Entitlement::Process],
            Decision::Granted,
            cx,
        );
        entitlements::decide(
            "@example/denied",
            &[Entitlement::Process],
            Decision::Denied,
            cx,
        );
    });
    let setup = begin(
        &[
            ("@example/allowed", &[], &["process"]),
            ("@example/denied", &[], &["process"]),
        ],
        cx,
    );
    let report = setup.finish(cx);
    assert!(setup.consent_window(cx).is_none());
    assert_eq!(result(&report, "@example/allowed"), &LoadResult::Loaded);
    assert!(matches!(
        result(&report, "@example/denied"),
        LoadResult::NotLoaded(cause) if cause.contains("denied")
    ));
}

#[gpui::test]
fn restart_asks_about_an_entitlement_added_by_an_update(cx: &mut TestAppContext) {
    let setup = begin(&[("@example/git", &[], &[])], cx);
    assert_eq!(
        result(&setup.finish(cx), "@example/git"),
        &LoadResult::Loaded
    );
    declare(setup.extensions.path(), "@example/git", &["process"]);

    // Dismissing loads nothing and stores nothing.
    let (outcome, prompted) = setup.restart("@example/git", "Dismiss", cx);
    assert!(prompted);
    assert_eq!(
        outcome,
        LoadResult::NotLoaded("not loaded: entitlement `process` not decided".into())
    );
    assert_eq!(decision("@example/git", cx), None);
    assert!(!resolves("entitled.ping", cx));

    // Allowing grants and activates.
    let (outcome, prompted) = setup.restart("@example/git", "Allow", cx);
    assert!(prompted);
    assert_eq!(outcome, LoadResult::Loaded);
    assert_eq!(decision("@example/git", cx), Some(Decision::Granted));
    assert!(resolves("entitled.ping", cx));

    // A decided package restarts without asking.
    let (outcome, prompted) = setup.restart("@example/git", "Allow", cx);
    assert!(!prompted);
    assert_eq!(outcome, LoadResult::Loaded);
}

#[gpui::test]
fn restart_does_not_override_a_denial(cx: &mut TestAppContext) {
    cx.update(|cx| {
        entitlements::decide(
            "@example/git",
            &[Entitlement::Process],
            Decision::Denied,
            cx,
        )
    });
    let setup = begin(&[("@example/git", &[], &["process"])], cx);
    setup.finish(cx);
    let (outcome, prompted) = setup.restart("@example/git", "Allow", cx);
    assert!(!prompted);
    assert_eq!(
        outcome,
        LoadResult::NotLoaded("not loaded: entitlement `process` denied".into())
    );
    assert_eq!(decision("@example/git", cx), Some(Decision::Denied));
    assert!(!resolves("entitled.ping", cx));
}

#[gpui::test]
fn a_denied_package_does_not_open_the_startup_report(cx: &mut TestAppContext) {
    cx.update(|cx| {
        entitlements::decide(
            "@example/git",
            &[Entitlement::Process],
            Decision::Denied,
            cx,
        )
    });
    let setup = begin(&[("@example/git", &[], &["process"])], cx);
    setup.finish(cx);
    cx.update(super::super::launch::open_report_after_failed_startup);
    assert!(
        !setup
            .window
            .read_with(cx, |window, _| window.extension_report_open)
    );

    // A failure still opens it.
    install_extension(
        setup.extensions.path(),
        "@example/broken",
        &[],
        &[("main.js", "throw new Error('broken');")],
    );
    setup.start(cx);
    setup.finish(cx);
    cx.update(super::super::launch::open_report_after_failed_startup);
    assert!(
        setup
            .window
            .read_with(cx, |window, _| window.extension_report_open)
    );
}
