use super::*;
use crate::scheduler::TEST_WAIT;
use crate::{
    lifecycle::ExtensionState,
    module_graph::{ConfigPhase, ModuleGraph},
};
use knot_protocol::ExtensionLifecycleId;
use knot_protocol::{BufferChange, BufferHandle, BufferSubscriptionId, ExtensionId};
use std::{
    collections::HashSet,
    pin::pin,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use std::{num::NonZeroUsize, sync::mpsc, thread::ThreadId};

fn key(extension: u64) -> ExtensionKey {
    ExtensionKey::new(ExtensionId::new(extension), ExtensionLifecycleId::new(1))
}

fn package_graph(root: &str, entry: &str, files: &[(&str, &str)]) -> ModuleGraph {
    ModuleGraph::new(
        root,
        entry,
        files
            .iter()
            .map(|(path, source)| ((*path).into(), Arc::from(*source)))
            .collect(),
    )
    .unwrap()
}

fn config_graph(entries: &[&str], files: &[(&str, &str)]) -> ModuleGraph {
    ModuleGraph::with_entries(
        "file:///config/knot/",
        entries,
        files
            .iter()
            .map(|(path, source)| ((*path).into(), Arc::from(*source)))
            .collect(),
    )
    .unwrap()
}

#[test]
fn personal_config_runtime_blocks_early_post_init_import() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(95);
    let graph = config_graph(
        &["pre-init.js", "post-init.js"],
        &[
            ("pre-init.js", "import './helper.js';"),
            ("helper.js", "import './post-init.js';"),
            ("post-init.js", "globalThis.startedEarly = true;"),
        ],
    );
    pool.load_personal_config(runtime, IsolateConfig::default(), graph)
        .unwrap();
    let error = pool
        .evaluate_personal_config_phase(runtime, ConfigPhase::PreInit)
        .unwrap()
        .wait()
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::ModuleResolution);
    assert_eq!(error.source(), Some("file:///config/knot/helper.js"));
    assert_eq!(error.line(), Some(1));
    pool.unload(runtime).unwrap();
    pool.shutdown();
}

#[test]
fn personal_config_phase_errors_keep_their_source_and_can_be_unloaded() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    for (index, failing_phase) in [ConfigPhase::PreInit, ConfigPhase::PostInit]
        .into_iter()
        .enumerate()
    {
        let runtime = key(91 + index as u64);
        let graph = config_graph(
            &["pre-init.js", "post-init.js"],
            &[
                (
                    "pre-init.js",
                    if failing_phase == ConfigPhase::PreInit {
                        "\nthrow new Error('pre boom');"
                    } else {
                        "globalThis.ready = true;"
                    },
                ),
                ("post-init.js", "\nthrow new Error('post boom');"),
            ],
        );
        pool.load_personal_config(runtime, IsolateConfig::default(), graph)
            .unwrap();
        let pre = pool
            .evaluate_personal_config_phase(runtime, ConfigPhase::PreInit)
            .unwrap()
            .wait();
        let error = if failing_phase == ConfigPhase::PreInit {
            pre.unwrap_err()
        } else {
            pre.unwrap();
            pool.evaluate_personal_config_phase(runtime, ConfigPhase::PostInit)
                .unwrap()
                .wait()
                .unwrap_err()
        };
        assert_eq!(error.kind(), RuntimeErrorKind::Rejection);
        assert_eq!(error.line(), Some(2));
        assert_eq!(
            error.source(),
            Some(if failing_phase == ConfigPhase::PreInit {
                "file:///config/knot/pre-init.js"
            } else {
                "file:///config/knot/post-init.js"
            })
        );
        pool.unload(runtime).unwrap();
    }
    pool.shutdown();
}

#[test]
fn personal_config_can_have_only_post_init() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(93);
    let graph = config_graph(
        &["post-init.js"],
        &[(
            "post-init.js",
            "await Promise.resolve(); globalThis.postOnly = true;",
        )],
    );
    pool.load_personal_config(runtime, IsolateConfig::default(), graph)
        .unwrap();
    assert!(
        pool.evaluate_personal_config_phase(runtime, ConfigPhase::PreInit)
            .is_err()
    );
    pool.evaluate_personal_config_phase(runtime, ConfigPhase::PostInit)
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(
        pool.execute(runtime, "verify.js", "postOnly")
            .unwrap()
            .wait()
            .unwrap()
            .value
            .as_ref(),
        "true"
    );
    pool.unload(runtime).unwrap();
    pool.shutdown();
}

#[test]
fn package_modules_support_local_imports_and_top_level_await() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(50);
    let graph = package_graph(
        "file:///extensions/%40example/tools/",
        "dist/main.js",
        &[
            (
                "dist/main.js",
                "import * as knot from 'knot'; const { editor } = knot; import { value } from './helper.js'; import { suffix } from '../suffix.mjs'; await Promise.resolve(); globalThis.packageValue = value + suffix + ':' + typeof editor.activeBuffer;",
            ),
            ("dist/helper.js", "export const value = 'ready';"),
            ("suffix.mjs", "export const suffix = '!';"),
        ],
    );
    pool.load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(
        &*pool
            .execute(runtime, "verify.js", "packageValue")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "ready!:function"
    );
    let changed = pool
        .execute_fixture_module(
            runtime,
            "file:///extensions/%40example/tools/other.js",
            "export {};",
        )
        .unwrap()
        .wait()
        .unwrap_err();
    assert!(changed.message().contains("cannot change after load"));
    pool.unload(runtime).unwrap();
    pool.shutdown();
}

#[test]
fn package_module_sources_are_isolated_per_lifecycle() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let first = key(51);
    let second = key(52);
    for (runtime, root, value) in [
        (first, "file:///extensions/%40example/first/", "first"),
        (second, "file:///extensions/%40example/second/", "second"),
    ] {
        let graph = package_graph(
            root,
            "main.js",
            &[
                (
                    "main.js",
                    "import { value } from './shared.js'; globalThis.packageValue = value;",
                ),
                (
                    "shared.js",
                    if value == "first" {
                        "export const value = 'first';"
                    } else {
                        "export const value = 'second';"
                    },
                ),
            ],
        );
        pool.load_package(runtime, IsolateConfig::default(), graph)
            .unwrap()
            .wait()
            .unwrap();
    }
    assert_eq!(
        &*pool
            .execute(first, "check.js", "packageValue")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "first"
    );
    assert_eq!(
        &*pool
            .execute(second, "check.js", "packageValue")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "second"
    );
    pool.unload(first).unwrap();
    pool.unload(second).unwrap();
    pool.shutdown();
}

#[test]
fn package_module_imports_stay_within_the_supplied_graph() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    for (index, (source, expected)) in [
        ("import 'bare-package';", "relative or knot"),
        ("import '../other/secret.js';", "escapes package"),
        (
            "import 'file:///extensions/%40example/other/secret.js';",
            "relative or knot",
        ),
        ("import './missing.js';", "package module not found"),
        ("import 'knot:bootstrap';", "not importable"),
        ("import 'knot:editor';", "relative or knot"),
    ]
    .into_iter()
    .enumerate()
    {
        let runtime = key(60 + index as u64);
        let graph = package_graph(
            "file:///extensions/%40example/tools/",
            "main.js",
            &[("main.js", source)],
        );
        let failure = pool
            .load_package(runtime, IsolateConfig::default(), graph)
            .unwrap()
            .wait()
            .unwrap_err();
        assert_eq!(failure.kind(), RuntimeErrorKind::ModuleResolution);
        assert!(failure.message().contains(expected), "{failure}");
        assert_eq!(
            failure.source(),
            Some("file:///extensions/%40example/tools/main.js")
        );
        assert_eq!(failure.line(), Some(1));
        assert!(failure.column().is_some());
        pool.unload(runtime).unwrap();
    }
    pool.shutdown();
}

#[test]
fn package_module_errors_report_generated_locations_and_dynamic_import_rejection() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let root = "file:///extensions/%40example/tools/";
    let runtime = key(70);
    let graph = package_graph(
        root,
        "main.js",
        &[
            ("main.js", "import './bad.js';"),
            ("bad.js", "export const = 1;"),
        ],
    );
    let syntax = pool
        .load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap_err();
    assert_eq!(syntax.kind(), RuntimeErrorKind::Compilation);
    assert_eq!(
        syntax.source(),
        Some("file:///extensions/%40example/tools/bad.js")
    );
    assert_eq!(syntax.line(), Some(1));
    assert!(syntax.column().is_some());
    pool.unload(runtime).unwrap();

    let runtime = key(71);
    let graph = package_graph(
        root,
        "main.js",
        &[("main.js", "\nthrow new Error('package boom');")],
    );
    let thrown = pool
        .load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap_err();
    assert_eq!(thrown.kind(), RuntimeErrorKind::Rejection);
    assert_eq!(
        thrown.source(),
        Some("file:///extensions/%40example/tools/main.js")
    );
    assert_eq!(thrown.line(), Some(2));
    assert!(thrown.column().is_some());
    pool.unload(runtime).unwrap();

    let runtime = key(72);
    let graph = package_graph(
        root,
        "main.js",
        &[
            ("main.js", "await import('./other.js');"),
            ("other.js", "export {};"),
        ],
    );
    let dynamic = pool
        .load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap_err();
    assert!(
        dynamic
            .message()
            .contains("dynamic imports are unavailable"),
        "{dynamic}"
    );
    pool.unload(runtime).unwrap();

    let runtime = key(73);
    let graph = package_graph(
        root,
        "main.js",
        &[
            ("main.js", "import './child.js';"),
            ("child.js", "\nthrow new Error('child boom');"),
        ],
    );
    let child = pool
        .load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap_err();
    assert_eq!(
        child.source(),
        Some("file:///extensions/%40example/tools/child.js")
    );
    assert_eq!(child.line(), Some(2));
    assert!(child.column().is_some());
    pool.unload(runtime).unwrap();
    pool.shutdown();
}

#[test]
fn failed_package_entry_can_be_rolled_back_and_pending_entry_unloaded() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let mut inbox = pool.take_event_inbox().unwrap();
    let root = "file:///extensions/%40example/tools/";
    let runtime = key(80);
    let graph = package_graph(
        root,
        "main.js",
        &[("main.js", "throw new Error('failed entry');")],
    );
    let failed = pool
        .load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap_err();
    assert!(failed.message().contains("failed entry"));
    pool.unload(runtime).unwrap();
    assert!(
        matches!(inbox.receive_timeout(Duration::from_secs(2)), Some(EngineEvent::LifecycleEnded {
        key: ended,
        reason: LifecycleEnd::Unloaded,
    }) if ended == runtime)
    );

    let runtime = key(81);
    let graph = package_graph(
        root,
        "main.js",
        &[("main.js", "await new Promise(() => {});")],
    );
    let pending = pool
        .load_package(runtime, IsolateConfig::default(), graph)
        .unwrap();
    wait_for_state(&pool, runtime, ExtensionState::AwaitingHostWork);
    pool.unload(runtime).unwrap();
    assert_eq!(
        pending.wait().unwrap_err().kind(),
        RuntimeErrorKind::Cancelled
    );
    assert!(
        matches!(inbox.receive_timeout(Duration::from_secs(2)), Some(EngineEvent::LifecycleEnded {
        key: ended,
        reason: LifecycleEnd::Unloaded,
    }) if ended == runtime)
    );
    pool.shutdown();
}

fn wait_for_state(pool: &RuntimePool, key: ExtensionKey, expected: ExtensionState) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = pool.scheduler_handle.state(key).unwrap();
        if state == expected {
            return;
        }
        assert!(Instant::now() < deadline, "state remained {state:?}");
        std::thread::yield_now();
    }
}

#[test]
fn native_numeric_decoder_rejects_coercion_and_precision_loss() {
    let capsule = RuntimeCapsule::new(key(200), IsolateConfig::default()).unwrap();
    let data = capsule.data.lock().unwrap();
    let data = data.as_ref().unwrap();
    let mut locker = data.isolate.lock();
    let scope = pin!(v8::HandleScope::new(&mut *locker));
    let mut scope = scope.init();
    let context = v8::Local::new(&scope, &data.context);
    let scope = &mut v8::ContextScope::new(&mut scope, context);
    let values = [
        v8::String::new(scope, "1").unwrap().into(),
        v8::Boolean::new(scope, true).into(),
        v8::null(scope).into(),
        v8::undefined(scope).into(),
    ];
    for value in values {
        assert!(matches!(
            value_u64(scope, value, "coordinate"),
            Err(BoundaryError::Type(_))
        ));
    }
    for number in [
        -1.0,
        0.5,
        f64::NAN,
        f64::INFINITY,
        MAX_JS_INTEGER as f64 + 1.0,
    ] {
        let value = v8::Number::new(scope, number);
        assert!(matches!(
            value_u64(scope, value.into(), "coordinate"),
            Err(BoundaryError::Range(_))
        ));
    }
    let maximum = v8::Number::new(scope, MAX_JS_INTEGER as f64);
    assert_eq!(
        value_u64(scope, maximum.into(), "coordinate").unwrap(),
        MAX_JS_INTEGER
    );
}

#[test]
fn console_methods_accept_any_value_without_failing() {
    let capsule = RuntimeCapsule::new(key(200), IsolateConfig::default()).unwrap();
    let report = capsule
        .execute(
            "console.js",
            r#"
        const cycle = {};
        cycle.self = cycle;
        ["debug", "log", "info", "warn", "error"].every((level) =>
            console[level]("text", 1, { a: [1] }, new Error("boom"), cycle, undefined) === undefined)
    "#,
        )
        .unwrap();
    assert_eq!(&*report.value, "true");
}

#[test]
fn native_dispatch_callbacks_are_not_available_to_extension_code() {
    let capsule = RuntimeCapsule::new(key(201), IsolateConfig::default()).unwrap();
    let report = capsule
        .execute(
            "private-dispatch.js",
            r#"
        ["__knotNativeBindings", "__knotDispatchBufferChange", "__knotInvokeCommand",
         "__knotCancelCommand", "__knotRequestTreeChildren", "__knotRequestCompletions"]
            .every((name) => !Object.hasOwn(globalThis, name))
    "#,
        )
        .unwrap();
    assert_eq!(&*report.value, "true");
}

#[test]
fn process_initialization_is_idempotent() {
    initialize();
    initialize();
}

#[test]
fn capsule_preserves_globals_and_drains_microtasks() {
    let capsule = RuntimeCapsule::new(key(1), IsolateConfig::default()).unwrap();
    capsule
        .execute(
            "first.js",
            "globalThis.count = 41; Promise.resolve().then(() => count++);",
        )
        .unwrap();
    let report = capsule.execute("second.js", "count").unwrap();
    assert_eq!(&*report.value, "42");
}

#[test]
fn scheduled_fixture_modules_resolve_static_imports_and_preserve_state() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(1);
    pool.load(runtime, IsolateConfig::default()).unwrap();

    pool.execute_fixture_module(
        runtime,
        "file:///fixtures/counter.js",
        "export let count = 0; export function increment() { count++; }",
    )
    .unwrap()
    .wait()
    .unwrap();
    pool.execute_fixture_module(
        runtime,
        "file:///fixtures/first.js",
        r#"
            import { count, increment } from "./counter.js";
            increment();
            globalThis.firstModuleCount = count;
        "#,
    )
    .unwrap()
    .wait()
    .unwrap();
    pool.execute_fixture_module(
        runtime,
        "file:///fixtures/second.js",
        r#"
            import { count, increment } from "./counter.js";
            increment();
            globalThis.secondModuleCount = count;
        "#,
    )
    .unwrap()
    .wait()
    .unwrap();

    assert_eq!(
        &*pool
            .execute(
                runtime,
                "verify.js",
                "`${firstModuleCount}:${secondModuleCount}`"
            )
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "1:2"
    );
    pool.shutdown();
}

#[test]
fn stale_lifecycle_cannot_dispatch_a_buffer_change() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let stale = key(48);
    pool.load(stale, IsolateConfig::default()).unwrap();
    pool.unload(stale).unwrap();
    let replacement = ExtensionKey::new(stale.extension, ExtensionLifecycleId::new(2));
    pool.load(replacement, IsolateConfig::default()).unwrap();
    assert!(matches!(
        pool.dispatch_buffer_change(
            stale,
            BufferSubscriptionId::new(1),
            BufferChange {
                buffer: BufferHandle::new(1),
                before_revision: 0,
                revision: 1,
                edits: vec![],
            },
        ),
        Err(RuntimePoolError::Scheduler(
            SchedulerError::StaleLifecycle { .. }
        ))
    ));
    pool.shutdown();
}

#[test]
fn fixture_module_failures_are_scoped_and_classified() {
    let capsule = RuntimeCapsule::new(key(1), IsolateConfig::default()).unwrap();

    let invalid = capsule.execute_fixture_module("not a URL", "").unwrap_err();
    assert_eq!(invalid.kind(), RuntimeErrorKind::InvalidModuleSpecifier);

    let invalid_import = capsule
        .execute_fixture_module(
            "file:///fixtures/invalid-import.js",
            "import 'bare-specifier';",
        )
        .unwrap_err();
    assert_eq!(invalid_import.kind(), RuntimeErrorKind::ModuleResolution);
    assert!(
        invalid_import
            .message()
            .contains("invalid fixture module specifier")
    );

    let missing = capsule
        .execute_fixture_module(
            "file:///fixtures/missing-import.js",
            "import './missing.js';",
        )
        .unwrap_err();
    assert_eq!(missing.kind(), RuntimeErrorKind::ModuleResolution);
    assert!(missing.message().contains("fixture module not found"));

    let private = capsule
        .execute_fixture_module(
            "file:///fixtures/private-import.js",
            "import { activeBuffer } from 'knot:bootstrap'; void activeBuffer;",
        )
        .unwrap_err();
    assert_eq!(private.kind(), RuntimeErrorKind::ModuleResolution);
    assert!(private.message().contains("not importable by extensions"));

    let removed = capsule
        .execute_fixture_module(
            "file:///fixtures/removed-import.js",
            "import 'knot:editor';",
        )
        .unwrap_err();
    assert_eq!(removed.kind(), RuntimeErrorKind::ModuleResolution);
    assert!(removed.message().contains("module not found"));

    let syntax = capsule
        .execute_fixture_module("file:///fixtures/syntax.js", "export const = 1;")
        .unwrap_err();
    assert_eq!(syntax.kind(), RuntimeErrorKind::Compilation);
    assert_eq!(syntax.source(), Some("file:///fixtures/syntax.js"));

    let thrown = capsule
        .execute_fixture_module(
            "file:///fixtures/thrown.js",
            "throw new Error('expected module throw');",
        )
        .unwrap_err();
    assert_eq!(thrown.kind(), RuntimeErrorKind::Rejection);
    assert!(thrown.message().contains("expected module throw"));

    let rejected = capsule
        .execute_fixture_module(
            "file:///fixtures/rejected.js",
            "await Promise.reject(new Error('expected module rejection'));",
        )
        .unwrap_err();
    assert_eq!(rejected.kind(), RuntimeErrorKind::Rejection);
    assert!(rejected.message().contains("expected module rejection"));

    assert_eq!(
        &*capsule.execute("recover.js", "6 * 7").unwrap().value,
        "42"
    );
}

#[test]
fn exceptions_and_rejections_are_scoped_to_a_turn() {
    let capsule = RuntimeCapsule::new(key(1), IsolateConfig::default()).unwrap();
    let compilation = capsule.execute("syntax.js", "const =").unwrap_err();
    assert_eq!(compilation.kind(), RuntimeErrorKind::Compilation);
    assert_eq!(compilation.source(), Some("syntax.js"));

    let thrown = capsule
        .execute(
            "throw.js",
            "globalThis.afterThrow = 0; Promise.resolve().then(() => afterThrow = 1); throw new Error('expected throw')",
        )
        .unwrap_err();
    assert_eq!(thrown.kind(), RuntimeErrorKind::Exception);
    assert!(thrown.message().contains("expected throw"));
    assert_eq!(thrown.source(), Some("throw.js"));
    assert_eq!(thrown.line(), Some(1));
    assert_eq!(thrown.column(), Some(74));
    assert!(
        thrown
            .stack()
            .is_some_and(|stack| stack.contains("throw.js"))
    );
    assert_eq!(
        &*capsule
            .execute("after-throw.js", "afterThrow")
            .unwrap()
            .value,
        "1"
    );

    let rejected = capsule
        .execute(
            "reject.js",
            "Promise.reject(new Error('expected rejection one')); Promise.reject(new Error('expected rejection two'))",
        )
        .unwrap_err();
    assert_eq!(rejected.kind(), RuntimeErrorKind::Rejection);
    assert!(rejected.message().contains("expected rejection one"));
    assert!(rejected.message().contains("expected rejection two"));
    assert_eq!(rejected.source(), Some("reject.js"));
    assert_eq!(rejected.line(), Some(1));

    assert_eq!(
        &*capsule.execute("recover.js", "6 * 7").unwrap().value,
        "42"
    );
}

#[test]
fn external_utf16_storage_is_retained_until_isolate_disposal() {
    let text: Arc<[u16]> = "héllo".encode_utf16().collect::<Vec<_>>().into();
    let weak = Arc::downgrade(&text);
    let first = RuntimeCapsule::new(key(1), IsolateConfig::default()).unwrap();
    let second = RuntimeCapsule::new(key(2), IsolateConfig::default()).unwrap();
    first
        .hold_external_utf16("held", Arc::clone(&text))
        .unwrap();
    second
        .hold_external_utf16("held", Arc::clone(&text))
        .unwrap();
    drop(text);
    assert_eq!(weak.strong_count(), 2);
    assert_eq!(&*first.execute("read.js", "held").unwrap().value, "héllo");
    first.dispose();
    assert_eq!(weak.strong_count(), 1);
    second.dispose();
    assert!(weak.upgrade().is_none());
}

#[test]
fn pooled_capsule_moves_between_workers_and_retains_state() {
    enum Work {
        Execute {
            capsule: Arc<RuntimeCapsule>,
            source: &'static str,
            result: mpsc::Sender<ExecutionReport>,
        },
        Block {
            started: mpsc::Sender<ThreadId>,
            release: mpsc::Receiver<()>,
        },
    }

    initialize();
    let pool = SchedulerPool::new(
        PoolConfig::new(NonZeroUsize::new(2).unwrap()),
        |turn: Turn<Work>| match turn.work {
            Work::Execute {
                capsule,
                source,
                result,
            } => {
                result
                    .send(capsule.execute("movement.js", source).unwrap())
                    .unwrap();
                TurnOutcome::Completed
            }
            Work::Block { started, release } => {
                started.send(std::thread::current().id()).unwrap();
                release.recv_timeout(TEST_WAIT).unwrap();
                TurnOutcome::Completed
            }
        },
    );
    let handle = pool.handle();
    let capsule = Arc::new(RuntimeCapsule::new(key(1), IsolateConfig::default()).unwrap());
    for admitted in [key(1), key(2), key(3)] {
        handle.admit(admitted).unwrap();
        handle.finish_loading(admitted, Ok(())).unwrap();
    }

    let (first_sender, first_result) = mpsc::channel();
    let (_, first_completion) = handle
        .enqueue_root(
            key(1),
            Work::Execute {
                capsule: Arc::clone(&capsule),
                source: "globalThis.count = 1; count",
                result: first_sender,
            },
        )
        .unwrap();
    let first_worker = first_result.recv_timeout(TEST_WAIT).unwrap().worker;
    assert_eq!(
        first_completion.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );

    let blockers: Vec<_> = [key(2), key(3)]
        .into_iter()
        .map(|key| {
            let (started_sender, started) = mpsc::channel();
            let (release, release_receiver) = mpsc::channel();
            let (_, completion) = handle
                .enqueue_root(
                    key,
                    Work::Block {
                        started: started_sender,
                        release: release_receiver,
                    },
                )
                .unwrap();
            (started, release, completion)
        })
        .collect();
    let blocker_workers: Vec<_> = blockers
        .iter()
        .map(|(started, _, _)| started.recv_timeout(TEST_WAIT).unwrap())
        .collect();
    assert_eq!(
        blocker_workers
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    let free_index = blocker_workers
        .iter()
        .position(|worker| *worker != first_worker)
        .unwrap();
    blockers[free_index].1.send(()).unwrap();
    assert_eq!(
        blockers[free_index].2.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );

    let (second_sender, second_result) = mpsc::channel();
    let (_, second_completion) = handle
        .enqueue_root(
            key(1),
            Work::Execute {
                capsule: Arc::clone(&capsule),
                source: "++count",
                result: second_sender,
            },
        )
        .unwrap();
    let second = second_result.recv_timeout(TEST_WAIT).unwrap();
    assert_ne!(second.worker, first_worker);
    assert_eq!(&*second.value, "2");
    assert_eq!(
        second_completion.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );

    blockers[1 - free_index].1.send(()).unwrap();
    assert_eq!(
        blockers[1 - free_index].2.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    pool.shutdown();
}

#[test]
fn movement_stress_alternates_workers_before_disposal() {
    type Command = Option<(&'static str, mpsc::Sender<ExecutionReport>)>;

    let capsule = Arc::new(RuntimeCapsule::new(key(1), IsolateConfig::default()).unwrap());
    capsule.execute("init.js", "globalThis.count = 0").unwrap();
    let mut workers = Vec::new();
    for index in 0..2 {
        let (sender, receiver) = mpsc::channel::<Command>();
        let capsule = Arc::clone(&capsule);
        let worker = std::thread::Builder::new()
            .name(format!("knot-movement-stress-{index}"))
            .spawn(move || {
                while let Some((source, result)) = receiver.recv_timeout(TEST_WAIT).unwrap() {
                    result
                        .send(capsule.execute("movement-stress.js", source).unwrap())
                        .unwrap();
                }
            })
            .unwrap();
        workers.push((sender, worker));
    }

    let mut worker_ids = HashSet::new();
    for index in 0..500 {
        let (result_sender, result) = mpsc::channel();
        workers[index % 2]
            .0
            .send(Some(("++count", result_sender)))
            .unwrap();
        worker_ids.insert(result.recv_timeout(TEST_WAIT).unwrap().worker);
    }
    assert_eq!(worker_ids.len(), 2);
    assert_eq!(
        &*capsule.execute("result.js", "count").unwrap().value,
        "500"
    );
    for (sender, worker) in workers {
        sender.send(None).unwrap();
        worker.join().unwrap();
    }
    capsule.dispose();
    assert_eq!(
        capsule.execute("disposed.js", "count").unwrap_err().kind(),
        RuntimeErrorKind::Disposed
    );
}

#[test]
fn independent_isolates_execute_javascript_concurrently() {
    let pool = RuntimePool::new(PoolConfig::new(NonZeroUsize::new(2).unwrap()));
    let first = key(1);
    let second = key(2);
    pool.load(first, IsolateConfig::default()).unwrap();
    pool.load(second, IsolateConfig::default()).unwrap();
    let barrier = Arc::new(TestGate::default());
    pool.capsule(first)
        .unwrap()
        .install_test_barrier(Arc::clone(&barrier));
    pool.capsule(second)
        .unwrap()
        .install_test_barrier(Arc::clone(&barrier));

    let first_execution = pool
        .execute(first, "first.js", "__knotTestBarrier(); 1")
        .unwrap();
    let second_execution = pool
        .execute(second, "second.js", "__knotTestBarrier(); 2")
        .unwrap();
    let first_report = first_execution.wait().unwrap();
    let second_report = second_execution.wait().unwrap();
    assert_ne!(first_report.worker, second_report.worker);
    assert_eq!((&*first_report.value, &*second_report.value), ("1", "2"));
    assert!(!barrier.timed_out.load(Ordering::Acquire));
    pool.shutdown();
}

#[test]
fn command_arguments_reject_non_json_values_before_dispatch() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(58);
    pool.load(runtime, IsolateConfig::default()).unwrap();
    pool.execute_fixture_module(
        runtime,
        "file:///fixtures/invalid-command-arguments.js",
        r#"
            import * as knot from "knot"; const { commands } = knot;
            const cyclic = {};
            cyclic.self = cyclic;
            for (const value of [NaN, () => {}, cyclic]) {
              let rejected = false;
              try {
                await commands.invoke("knot.fixture.missing", value);
              } catch (error) {
                rejected = error instanceof TypeError;
              }
              if (!rejected) throw new Error("invalid command arguments were accepted");
            }
        "#,
    )
    .unwrap()
    .wait()
    .unwrap();
    assert!(
        pool.receive_request_timeout(Duration::from_millis(20))
            .is_none()
    );
    pool.shutdown();
}

#[test]
fn forced_termination_is_fatal_and_does_not_poison_a_neighbor() {
    let pool = RuntimePool::new(PoolConfig::new(NonZeroUsize::new(2).unwrap()));
    let runaway = key(1);
    let neighbor = key(2);
    pool.load(runaway, IsolateConfig::default()).unwrap();
    pool.load(neighbor, IsolateConfig::default()).unwrap();
    let (entered_sender, entered) = mpsc::channel();
    pool.capsule(runaway)
        .unwrap()
        .install_test_entered_signal(entered_sender);
    let retained: Arc<[u16]> = "worker-owned".encode_utf16().collect::<Vec<_>>().into();
    let retained_weak = Arc::downgrade(&retained);
    pool.capsule(runaway)
        .unwrap()
        .hold_external_utf16("held", retained)
        .unwrap();
    let termination = pool.termination_handle(runaway).unwrap();
    let execution = pool
        .execute(
            runaway,
            "runaway.js",
            "__knotTestEntered(); while (true) {}",
        )
        .unwrap();
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(termination.terminate());
    let error = execution.wait().unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::Terminated);
    assert!(retained_weak.upgrade().is_none());
    assert_eq!(
        &*pool
            .execute(neighbor, "neighbor.js", "6 * 7")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "42"
    );
    assert!(pool.execute(runaway, "late.js", "1").is_err());
    pool.shutdown();
}

#[test]
fn unload_terminates_running_javascript_before_disposal() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(1);
    pool.load(runtime, IsolateConfig::default()).unwrap();
    let (entered_sender, entered) = mpsc::channel();
    pool.capsule(runtime)
        .unwrap()
        .install_test_entered_signal(entered_sender);
    let execution = pool
        .execute(
            runtime,
            "runaway.js",
            "__knotTestEntered(); while (true) {}",
        )
        .unwrap();
    let queued = pool.execute(runtime, "queued.js", "1").unwrap();
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    let started = Instant::now();
    pool.unload(runtime).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    let error = execution.wait().unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::Terminated);
    assert_eq!(
        queued.wait().unwrap_err().kind(),
        RuntimeErrorKind::Cancelled
    );
    assert!(pool.execute(runtime, "late.js", "1").is_err());
    pool.shutdown();
}

#[test]
fn unload_releases_external_state_before_returning() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(1);
    pool.load(runtime, IsolateConfig::default()).unwrap();
    let text: Arc<[u16]> = "retained".encode_utf16().collect::<Vec<_>>().into();
    let weak = Arc::downgrade(&text);
    pool.capsule(runtime)
        .unwrap()
        .hold_external_utf16("held", Arc::clone(&text))
        .unwrap();
    drop(text);
    assert_eq!(weak.strong_count(), 1);
    pool.unload(runtime).unwrap();
    assert!(weak.upgrade().is_none());
    pool.shutdown();
}

#[test]
fn stale_unload_cannot_dispose_a_replacement_lifecycle() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let old = key(1);
    let replacement = ExtensionKey::new(old.extension, ExtensionLifecycleId::new(2));
    pool.load(old, IsolateConfig::default()).unwrap();
    pool.unload(old).unwrap();
    pool.load(replacement, IsolateConfig::default()).unwrap();

    assert!(matches!(
        pool.unload(old),
        Err(RuntimePoolError::Scheduler(
            SchedulerError::StaleLifecycle { .. }
        ))
    ));
    assert_eq!(
        &*pool
            .execute(replacement, "replacement.js", "6 * 7")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "42"
    );
    pool.shutdown();
}

#[test]
fn heap_exhaustion_probe_child() {
    if std::env::var_os("KNOT_RUN_HEAP_EXHAUSTION_PROBE").is_none() {
        return;
    }
    let pool = RuntimePool::new(PoolConfig::new(NonZeroUsize::new(2).unwrap()));
    let exhausted = key(1);
    let neighbor = key(2);
    // A limit near V8's minimum working set can fail a background compiler's
    // allocation, which is fatal for the process rather than reaching the
    // near-heap-limit callback.
    pool.load(exhausted, IsolateConfig::with_heap_limit(16 * 1024 * 1024))
        .unwrap();
    pool.load(neighbor, IsolateConfig::default()).unwrap();
    let error = pool
        .execute(
            exhausted,
            "heap.js",
            "let value = ''; while (true) { value += '0123456789abcdef'; }",
        )
        .unwrap()
        .wait()
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::HeapLimitExceeded);
    assert!(pool.execute(exhausted, "late.js", "1").is_err());
    assert_eq!(
        &*pool
            .execute(neighbor, "neighbor.js", "6 * 7")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "42"
    );
    pool.shutdown();
}

fn used_heap(pool: &RuntimePool, runtime: ExtensionKey) -> usize {
    pool.heap_usage()
        .into_iter()
        .find(|(key, _)| *key == runtime)
        .and_then(|(_, usage)| usage)
        .expect("an idle isolate reports its heap")
        .used
}

#[test]
fn startup_garbage_is_released_once_the_package_settles() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(1);
    let graph = package_graph(
        "file:///extensions/%40example/burst/",
        "main.js",
        &[(
            "main.js",
            "{ const a = []; for (let i = 0; i < 100000; i++) a.push({ i, s: 'x' + i }); } await Promise.resolve(); globalThis.kept = 'kept';",
        )],
    );
    pool.load_package(runtime, IsolateConfig::default(), graph)
        .unwrap()
        .wait()
        .unwrap();
    // The burst allocates about 9 MiB that no turn would otherwise collect.
    assert!(used_heap(&pool, runtime) < 2 * 1024 * 1024);
    assert_eq!(
        &*pool
            .execute(runtime, "kept.js", "kept")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "kept"
    );
    pool.shutdown();
}

#[test]
fn a_long_allocating_turn_runs_normally_and_its_garbage_is_released() {
    let pool = RuntimePool::new(PoolConfig::single_worker());
    let runtime = key(1);
    pool.load(runtime, IsolateConfig::default()).unwrap();
    let report = pool
        .execute(
            runtime,
            "long.js",
            "const start = Date.now(); let rounds = 0;
             while (Date.now() - start < 150) {
               const a = []; for (let i = 0; i < 20000; i++) a.push({ i, s: 'x' + i });
               rounds++;
             }
             globalThis.rounds = rounds; rounds > 0 ? 'done' : 'none'",
        )
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(&*report.value, "done");
    assert!(used_heap(&pool, runtime) < 2 * 1024 * 1024);
    assert_eq!(
        &*pool
            .execute(runtime, "rounds.js", "rounds > 0")
            .unwrap()
            .wait()
            .unwrap()
            .value,
        "true"
    );
    pool.shutdown();
}
