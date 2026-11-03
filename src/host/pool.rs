//! Product-facing ownership and controls for the pooled extension host.

use std::sync::Arc;

use super::{
    engine::{self, EngineEvent, LifecycleEnd, RuntimePool},
    lifecycle::ExtensionKey,
    module_graph::{ConfigPhase, ModuleGraph},
    protocol::{
        BufferChange, BufferHandle, BufferSubscriptionId, CommandInvocation, CommandInvocationId,
        CompletionRequest, HostRequest, HostResponse, TreeChildrenRequest,
    },
    scheduler::PoolConfig,
};

pub(crate) use super::engine::{
    HeapUsage as ExtensionHeapUsage, RuntimeCommandExecution as ExtensionCommandExecution,
    RuntimeExecution as ExtensionExecution, RuntimePoolError as ExtensionPoolError,
    RuntimeProviderExecution as ExtensionProviderExecution,
    RuntimeResponseError as ExtensionResponseError,
};

/// Configuration for one persistent extension isolate.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExtensionConfig(engine::IsolateConfig);

impl ExtensionConfig {
    /// Names the extension in its console output.
    pub(crate) fn named(name: &str) -> Self {
        Self(engine::IsolateConfig::named(name))
    }

    #[cfg(test)]
    pub(crate) const fn with_heap_limit(heap_limit_bytes: usize) -> Self {
        Self(engine::IsolateConfig::with_heap_limit(heap_limit_bytes))
    }
}

/// Why one admitted extension lifetime ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExtensionExit {
    StartupFailed,
    Unloaded,
    Failed(super::lifecycle::Failure),
    Shutdown,
}

/// Event emitted by the pool for foreground processing.
#[derive(Clone, Debug)]
pub(crate) enum ExtensionEvent {
    Request(HostRequest),
    LifecycleEnded {
        key: ExtensionKey,
        reason: ExtensionExit,
    },
}

/// Unique, awaitable event stream for the application foreground.
pub(crate) struct ExtensionEventInbox(engine::EngineEventInbox);

impl ExtensionEventInbox {
    pub(crate) async fn receive(&mut self) -> Option<ExtensionEvent> {
        self.0.receive().await.map(|event| match event {
            EngineEvent::Request(request) => ExtensionEvent::Request(request),
            EngineEvent::LifecycleEnded { key, reason } => ExtensionEvent::LifecycleEnded {
                key,
                reason: match reason {
                    LifecycleEnd::StartupFailed => ExtensionExit::StartupFailed,
                    LifecycleEnd::Unloaded => ExtensionExit::Unloaded,
                    LifecycleEnd::Failed(failure) => ExtensionExit::Failed(failure),
                    LifecycleEnd::Shutdown => ExtensionExit::Shutdown,
                },
            },
        })
    }

    #[cfg(test)]
    fn receive_timeout(&mut self, timeout: std::time::Duration) -> Option<ExtensionEvent> {
        self.0.receive_timeout(timeout).map(|event| match event {
            EngineEvent::Request(request) => ExtensionEvent::Request(request),
            EngineEvent::LifecycleEnded { key, reason } => ExtensionEvent::LifecycleEnded {
                key,
                reason: match reason {
                    LifecycleEnd::StartupFailed => ExtensionExit::StartupFailed,
                    LifecycleEnd::Unloaded => ExtensionExit::Unloaded,
                    LifecycleEnd::Failed(failure) => ExtensionExit::Failed(failure),
                    LifecycleEnd::Shutdown => ExtensionExit::Shutdown,
                },
            },
        })
    }
}

/// One application-owned bounded worker pool.
pub(crate) struct ExtensionPool {
    engine: RuntimePool,
}

impl ExtensionPool {
    pub(crate) fn new(config: PoolConfig) -> (Arc<Self>, ExtensionEventInbox) {
        let engine = RuntimePool::new(config);
        let inbox = engine
            .take_event_inbox()
            .expect("a new extension pool owns its event inbox");
        (Arc::new(Self { engine }), ExtensionEventInbox(inbox))
    }

    #[cfg(test)]
    pub(crate) fn load(
        &self,
        key: ExtensionKey,
        config: ExtensionConfig,
    ) -> Result<(), ExtensionPoolError> {
        self.engine.load(key, config.0)
    }

    #[cfg(test)]
    pub(crate) fn execute_fixture_module(
        &self,
        key: ExtensionKey,
        specifier: impl Into<Arc<str>>,
        source: impl Into<Arc<str>>,
    ) -> Result<ExtensionExecution, ExtensionPoolError> {
        self.engine.execute_fixture_module(key, specifier, source)
    }

    pub(crate) fn load_package(
        &self,
        key: ExtensionKey,
        config: ExtensionConfig,
        graph: ModuleGraph,
    ) -> Result<ExtensionExecution, ExtensionPoolError> {
        self.engine.load_package(key, config.0, graph)
    }

    pub(crate) fn load_personal_config(
        &self,
        key: ExtensionKey,
        config: ExtensionConfig,
        graph: ModuleGraph,
    ) -> Result<(), ExtensionPoolError> {
        self.engine.load_personal_config(key, config.0, graph)
    }

    pub(crate) fn evaluate_personal_config_phase(
        &self,
        key: ExtensionKey,
        phase: ConfigPhase,
    ) -> Result<ExtensionExecution, ExtensionPoolError> {
        self.engine.evaluate_personal_config_phase(key, phase)
    }

    #[cfg(test)]
    fn panic_worker_turn(
        &self,
        key: ExtensionKey,
    ) -> Result<ExtensionExecution, ExtensionPoolError> {
        self.engine.panic_worker_turn(key)
    }

    pub(crate) fn invoke_command(
        &self,
        key: ExtensionKey,
        invocation: CommandInvocation,
        active_buffer: Option<BufferHandle>,
    ) -> Result<ExtensionCommandExecution, ExtensionPoolError> {
        self.engine.invoke_command(key, invocation, active_buffer)
    }

    pub(crate) fn request_tree_children(
        &self,
        key: ExtensionKey,
        request: TreeChildrenRequest,
    ) -> Result<ExtensionProviderExecution<super::protocol::TreeChildrenResponse>, ExtensionPoolError>
    {
        self.engine.request_tree_children(key, request)
    }

    pub(crate) fn request_completions(
        &self,
        key: ExtensionKey,
        request: CompletionRequest,
    ) -> Result<ExtensionProviderExecution<super::protocol::CompletionResponse>, ExtensionPoolError>
    {
        self.engine.request_completions(key, request)
    }

    pub(crate) fn cancel_command(
        &self,
        key: ExtensionKey,
        invocation: CommandInvocationId,
    ) -> Result<(), ExtensionPoolError> {
        self.engine.cancel_command(key, invocation)
    }

    pub(crate) fn respond(&self, response: HostResponse) -> Result<(), ExtensionResponseError> {
        self.engine.respond(response)
    }

    pub(crate) fn dispatch_application_change(
        &self,
        key: ExtensionKey,
        subscription: super::protocol::WorkbenchSubscriptionId,
        event: super::protocol::ApplicationEvent,
    ) -> Result<(), ExtensionPoolError> {
        self.engine
            .dispatch_application_change(key, subscription, event)
    }

    pub(crate) fn dispatch_setting_change(
        &self,
        key: ExtensionKey,
        subscription: super::protocol::SettingsSubscriptionId,
        setting: String,
    ) -> Result<(), ExtensionPoolError> {
        self.engine
            .dispatch_setting_change(key, subscription, setting)
    }

    pub(crate) fn dispatch_editor_event(
        &self,
        key: ExtensionKey,
        handle: u64,
        event: super::protocol::EditorEvent,
    ) -> Result<(), ExtensionPoolError> {
        self.engine.dispatch_editor_event(key, handle, event)
    }

    pub(crate) fn dispatch_buffer_change(
        &self,
        key: ExtensionKey,
        subscription: BufferSubscriptionId,
        change: BufferChange,
    ) -> Result<(), ExtensionPoolError> {
        self.engine
            .dispatch_buffer_change(key, subscription, change)
    }

    /// Heap statistics of every live lifecycle, read between turns; `None`
    /// for one running a turn.
    pub(crate) fn heap_usage(&self) -> Vec<(ExtensionKey, Option<ExtensionHeapUsage>)> {
        self.engine.heap_usage()
    }

    #[cfg(test)]
    pub(crate) fn diagnostics(&self) -> super::scheduler::SchedulerDiagnostics {
        self.engine.diagnostics()
    }

    #[allow(
        dead_code,
        reason = "per-turn timeouts (D019) issue it; Restart Extension unloads instead"
    )]
    pub(crate) fn watchdog(
        &self,
        key: ExtensionKey,
    ) -> Result<ExtensionWatchdog, ExtensionPoolError> {
        self.engine
            .termination_handle(key)
            .map(|handle| ExtensionWatchdog { handle })
    }

    pub(crate) fn unload(&self, key: ExtensionKey) -> Result<(), ExtensionPoolError> {
        self.engine.unload(key)
    }

    #[cfg(test)]
    pub(crate) fn shutdown(self) {
        self.engine.shutdown();
    }
}

/// The process's resident memory in bytes, where the platform reports it.
#[cfg(target_os = "macos")]
pub(crate) fn process_resident_bytes() -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: the buffer is a zeroed `proc_taskinfo` of exactly `size` bytes,
    // which is what this flavor writes.
    let written = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    // SAFETY: a full write initialized the structure.
    (written == size).then(|| unsafe { info.assume_init() }.pti_resident_size)
}

/// The process's resident memory in bytes, where the platform reports it.
#[cfg(target_os = "linux")]
pub(crate) fn process_resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: `sysconf` has no preconditions.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    Some(pages * page_size)
}

/// The process's resident memory in bytes, where the platform reports it.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn process_resident_bytes() -> Option<u64> {
    None
}

/// Cloneable, non-V8 watchdog for forcefully interrupting one isolate.
#[allow(
    dead_code,
    reason = "per-turn timeouts (D019) issue it; Restart Extension unloads instead"
)]
pub(crate) struct ExtensionWatchdog {
    handle: engine::TerminationHandle,
}

#[allow(
    dead_code,
    reason = "per-turn timeouts (D019) issue it; Restart Extension unloads instead"
)]
impl ExtensionWatchdog {
    pub(crate) fn terminate(&self) -> bool {
        self.handle.terminate()
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, time::Duration};

    use crate::host::protocol::{ExtensionId, ExtensionLifecycleId};

    use super::*;

    fn key(extension: u64, lifecycle: u64) -> ExtensionKey {
        ExtensionKey::new(
            ExtensionId::new(extension),
            ExtensionLifecycleId::new(lifecycle),
        )
    }

    #[test]
    fn pool_owner_exposes_async_events_diagnostics_and_exact_once_unload() {
        let (pool, mut inbox) = ExtensionPool::new(PoolConfig::new(NonZeroUsize::new(2).unwrap()));
        let first = key(1, 1);
        let second = key(2, 1);
        pool.load(first, ExtensionConfig::default()).unwrap();
        pool.load(second, ExtensionConfig::default()).unwrap();

        let diagnostics = pool.diagnostics();
        assert_eq!(diagnostics.worker_count, 2);
        assert_eq!(diagnostics.extensions.len(), 2);

        let execution = pool
            .execute_fixture_module(
                first,
                "file:///fixtures/pool-owner.js",
                "globalThis.poolOwnerProbe = 42;",
            )
            .unwrap();
        execution.wait().unwrap();

        pool.unload(first).unwrap();
        assert!(matches!(
            inbox.receive_timeout(Duration::from_secs(1)),
            Some(ExtensionEvent::LifecycleEnded {
                key,
                reason: ExtensionExit::Unloaded,
            }) if key == first
        ));
        assert!(pool.unload(first).is_err());
        assert!(inbox.receive_timeout(Duration::from_millis(20)).is_none());

        pool.unload(second).unwrap();
        assert!(matches!(
            inbox.receive_timeout(Duration::from_secs(1)),
            Some(ExtensionEvent::LifecycleEnded { key, .. }) if key == second
        ));
        let pool = Arc::try_unwrap(pool).unwrap_or_else(|_| panic!("pool owner leaked"));
        pool.shutdown();
    }

    #[test]
    fn heap_usage_reports_live_lifecycles_and_drops_disposed_ones() {
        let (pool, mut inbox) = ExtensionPool::new(PoolConfig::single_worker());
        let first = key(1, 1);
        let second = key(2, 1);
        pool.load(first, ExtensionConfig::default()).unwrap();
        pool.load(second, ExtensionConfig::default()).unwrap();
        pool.execute_fixture_module(
            second,
            "file:///fixtures/heap-usage.js",
            "globalThis.retained = new Array(100000).fill(1);",
        )
        .unwrap()
        .wait()
        .unwrap();

        let mut usage = pool.heap_usage();
        usage.sort_by_key(|(key, _)| key.extension.value());
        assert_eq!(
            usage.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            [first, second]
        );
        let first_usage = usage[0].1.expect("an idle isolate reports statistics");
        let second_usage = usage[1].1.expect("an idle isolate reports statistics");
        assert!(first_usage.used > 0 && first_usage.used <= first_usage.total);
        assert!(second_usage.used > first_usage.used + 400_000);

        pool.unload(first).unwrap();
        assert!(matches!(
            inbox.receive_timeout(Duration::from_secs(1)),
            Some(ExtensionEvent::LifecycleEnded { key, .. }) if key == first
        ));
        assert_eq!(
            pool.heap_usage()
                .iter()
                .map(|(key, _)| *key)
                .collect::<Vec<_>>(),
            [second]
        );
        pool.unload(second).unwrap();
        assert!(pool.heap_usage().is_empty());
    }

    /// What one more idle package costs: isolate creation time, resident size
    /// per isolate, and heap per isolate, each loaded with an empty module.
    #[test]
    #[ignore = "measurement: run in release with --ignored --nocapture"]
    fn measure_isolate_cost() {
        const COUNT: u64 = 50;
        fn mib(bytes: f64) -> String {
            format!("{:.2} MiB", bytes / (1024.0 * 1024.0))
        }
        fn median(mut values: Vec<usize>) -> usize {
            values.sort_unstable();
            values[values.len() / 2]
        }
        let (pool, _inbox) = ExtensionPool::new(PoolConfig::single_worker());
        let mut next_extension = 0;
        let mut load = |source: &str| {
            let runtime = key(next_extension, 1);
            next_extension += 1;
            let graph = ModuleGraph::new(
                "file:///measure/",
                "main.js",
                [("main.js".to_owned(), Arc::from(source))].into(),
            )
            .unwrap();
            let start = std::time::Instant::now();
            let execution = pool
                .load_package(runtime, ExtensionConfig::default(), graph)
                .unwrap();
            let created = start.elapsed();
            execution.wait().unwrap();
            (runtime, created, start.elapsed())
        };
        // The first isolate pays V8's process-wide setup.
        let (_, first, _) = load("export {};");
        println!("first isolate creation: {first:?}");
        for (name, source) in [
            ("empty package", "export {};"),
            (
                "package allocating ~8 MiB transiently at startup",
                "{ const a = []; for (let i = 0; i < 100000; i++) a.push({ i, s: 'x' + i }); }",
            ),
        ] {
            let resident_before = process_resident_bytes().unwrap_or(0);
            let loaded = (0..COUNT).map(|_| load(source)).collect::<Vec<_>>();
            let resident_after = process_resident_bytes().unwrap_or(0);
            let mut created = loaded
                .iter()
                .map(|(_, created, _)| *created)
                .collect::<Vec<_>>();
            let mut started = loaded
                .iter()
                .map(|(_, _, started)| *started)
                .collect::<Vec<_>>();
            created.sort_unstable();
            started.sort_unstable();
            let keys = loaded.iter().map(|(key, ..)| *key).collect::<Vec<_>>();
            let usage = pool
                .heap_usage()
                .into_iter()
                .filter(|(key, _)| keys.contains(key))
                .filter_map(|(_, usage)| usage)
                .collect::<Vec<_>>();
            println!("{name}, {COUNT} isolates:");
            println!(
                "  creation median {:?}, p90 {:?}; through startup median {:?}, p90 {:?}",
                created[created.len() / 2],
                created[created.len() * 9 / 10],
                started[started.len() / 2],
                started[started.len() * 9 / 10],
            );
            println!(
                "  resident size per isolate: {}",
                mib(resident_after.saturating_sub(resident_before) as f64 / COUNT as f64)
            );
            println!(
                "  heap per isolate (median): used {}, total {}, external {}",
                mib(median(usage.iter().map(|usage| usage.used).collect()) as f64),
                mib(median(usage.iter().map(|usage| usage.total).collect()) as f64),
                mib(median(usage.iter().map(|usage| usage.external).collect()) as f64),
            );
        }
    }

    #[test]
    fn process_resident_size_is_reported() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(process_resident_bytes().unwrap() > 1024 * 1024);
        }
    }

    #[test]
    fn watchdog_is_lifecycle_scoped_and_v8_free_to_call() {
        let (pool, _inbox) = ExtensionPool::new(PoolConfig::single_worker());
        let runtime = key(1, 1);
        pool.load(runtime, ExtensionConfig::with_heap_limit(32 * 1024 * 1024))
            .unwrap();
        let watchdog = pool.watchdog(runtime).unwrap();
        assert!(watchdog.terminate());
        assert!(!watchdog.terminate());
        drop(watchdog);
        drop(pool);
    }

    #[test]
    fn worker_panic_finalizes_only_the_failed_lifecycle() {
        let (pool, mut inbox) = ExtensionPool::new(PoolConfig::single_worker());
        let failed = key(1, 1);
        let neighbor = key(2, 1);
        pool.load(failed, ExtensionConfig::default()).unwrap();
        pool.load(neighbor, ExtensionConfig::default()).unwrap();

        let execution = pool.panic_worker_turn(failed).unwrap();
        assert!(execution.wait().is_err());
        assert!(matches!(
            inbox.receive_timeout(Duration::from_secs(1)),
            Some(ExtensionEvent::LifecycleEnded {
                key,
                reason: ExtensionExit::Failed(_),
            }) if key == failed
        ));
        assert!(pool.watchdog(failed).is_err());

        pool.execute_fixture_module(
            neighbor,
            "file:///fixtures/panic-neighbor.js",
            "globalThis.neighborSurvived = true;",
        )
        .unwrap()
        .wait()
        .unwrap();
        pool.unload(neighbor).unwrap();
    }
}
