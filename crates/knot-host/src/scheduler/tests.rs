use super::*;
use std::time::Duration;

fn key(extension: u64, lifecycle: u64) -> ExtensionKey {
    ExtensionKey::new(
        ExtensionId::new(extension),
        ExtensionLifecycleId::new(lifecycle),
    )
}

/// A turn that ran to its next `await` and settled `root`, if any.
fn settled(root: Option<RootId>) -> TurnOutcome {
    TurnOutcome::Completed {
        settled: root.into_iter().collect(),
    }
}

fn admitted() -> (StateMachine<&'static str>, ExtensionKey) {
    let key = key(1, 1);
    let mut scheduler = StateMachine::default();
    scheduler.admit(key).unwrap();
    scheduler.finish_loading(key, Ok(())).unwrap();
    (scheduler, key)
}

#[test]
fn transitions_through_root_and_host_continuation() {
    let (mut scheduler, key) = admitted();
    assert_eq!(scheduler.state(key), Ok(&ExtensionState::Idle));
    let (root, completion) = scheduler.enqueue_root(key, "root").unwrap();
    assert_eq!(scheduler.state(key), Ok(&ExtensionState::Queued));

    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.work, "root");
    assert_eq!(turn.root, Some(root));
    scheduler
        .finish_turn(key, turn.id, TurnOutcome::idle())
        .unwrap();
    assert_eq!(scheduler.state(key), Ok(&ExtensionState::AwaitingHostWork));
    assert!(completion.try_recv().is_err());

    scheduler.enqueue_task(key, Some(root), "response").unwrap();
    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.work, "response");
    scheduler
        .finish_turn(key, turn.id, settled(Some(root)))
        .unwrap();
    assert_eq!(
        completion.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    assert_eq!(scheduler.state(key), Ok(&ExtensionState::Idle));
}

#[test]
fn preserves_global_fifo_and_requeues_after_one_turn() {
    let first = key(1, 1);
    let second = key(2, 1);
    let mut scheduler = StateMachine::default();
    for key in [first, second] {
        scheduler.admit(key).unwrap();
        scheduler.finish_loading(key, Ok(())).unwrap();
    }
    let (first_root, _) = scheduler.enqueue_root(first, "first root").unwrap();
    scheduler.enqueue_root(second, "second root").unwrap();

    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.key, first);
    scheduler
        .enqueue_task(first, Some(first_root), "first continuation")
        .unwrap();
    scheduler
        .finish_turn(first, turn.id, TurnOutcome::idle())
        .unwrap();

    assert_eq!(scheduler.next_turn().unwrap().key, second);
    assert_eq!(scheduler.next_turn().unwrap().key, first);
}

#[test]
fn tasks_run_while_an_invocation_awaits_host_work() {
    let (mut scheduler, key) = admitted();
    let (active, _) = scheduler.enqueue_root(key, "active").unwrap();
    scheduler.enqueue_root(key, "unrelated").unwrap();
    let turn = scheduler.next_turn().unwrap();
    scheduler
        .finish_turn(key, turn.id, TurnOutcome::idle())
        .unwrap();

    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.work, "unrelated");
    scheduler
        .finish_turn(key, turn.id, settled(turn.root))
        .unwrap();
    assert_eq!(scheduler.state(key), Ok(&ExtensionState::AwaitingHostWork));

    scheduler
        .enqueue_task(key, Some(active), "response")
        .unwrap();
    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.work, "response");
}

#[test]
fn rejects_stale_lifecycle_and_duplicate_turns() {
    let (mut scheduler, key) = admitted();
    let (root, _) = scheduler.enqueue_root(key, "root").unwrap();
    let turn = scheduler.next_turn().unwrap();
    let stale = ExtensionKey::new(key.extension, ExtensionLifecycleId::new(2));
    assert!(matches!(
        scheduler.enqueue_root(stale, "stale"),
        Err(SchedulerError::StaleLifecycle { .. })
    ));
    assert!(matches!(
        scheduler.enqueue_task(stale, Some(root), "stale"),
        Err(SchedulerError::StaleLifecycle { .. })
    ));
    scheduler
        .finish_turn(key, turn.id, settled(Some(root)))
        .unwrap();
    assert_eq!(
        scheduler.finish_turn(key, turn.id, TurnOutcome::idle()),
        Err(SchedulerError::DuplicateOrStaleTurn(turn.id))
    );
}

#[test]
fn terminal_paths_settle_every_completion_once() {
    let (mut scheduler, key) = admitted();
    let (_, active) = scheduler.enqueue_root(key, "active").unwrap();
    let (_, queued) = scheduler.enqueue_root(key, "queued").unwrap();
    let turn = scheduler.next_turn().unwrap();
    let failure = Failure::new("boom");
    scheduler
        .finish_turn(key, turn.id, TurnOutcome::Fatal(failure.clone()))
        .unwrap();
    assert_eq!(
        active.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Failed(failure.clone())
    );
    assert_eq!(
        queued.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Failed(failure)
    );
    assert!(active.try_recv().is_err());
    assert!(queued.try_recv().is_err());
}

#[test]
fn stop_and_shutdown_cancel_all_work() {
    let (mut scheduler, first) = admitted();
    let second = key(2, 1);
    scheduler.admit(second).unwrap();
    scheduler.finish_loading(second, Ok(())).unwrap();
    let (_, running) = scheduler.enqueue_root(first, "running").unwrap();
    let (_, queued) = scheduler.enqueue_root(first, "queued").unwrap();
    let (_, other) = scheduler.enqueue_root(second, "other").unwrap();
    let turn = scheduler.next_turn().unwrap();
    scheduler.stop(first).unwrap();
    assert!(running.try_recv().is_err());
    assert!(queued.try_recv().is_err());
    scheduler
        .finish_turn(first, turn.id, TurnOutcome::idle())
        .unwrap();
    for completion in [running, queued] {
        assert_eq!(
            completion.recv_timeout(TEST_WAIT).unwrap(),
            CompletionOutcome::Cancelled
        );
    }

    scheduler.shutdown();
    assert_eq!(
        other.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Cancelled
    );
    assert!(matches!(
        scheduler.enqueue_root(second, "late"),
        Err(SchedulerError::ShuttingDown)
    ));
}

#[test]
fn load_failure_settles_work_queued_during_loading() {
    let key = key(1, 1);
    let mut scheduler = StateMachine::default();
    scheduler.admit(key).unwrap();
    let (_, completion) = scheduler.enqueue_root(key, "root").unwrap();
    let failure = Failure::new("load failed");
    scheduler.finish_loading(key, Err(failure.clone())).unwrap();
    assert_eq!(
        completion.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Failed(failure.clone())
    );
    assert_eq!(scheduler.state(key), Ok(&ExtensionState::Failed(failure)));
}

#[test]
fn replacement_lifecycle_rejects_stale_work() {
    let (mut scheduler, old) = admitted();
    scheduler.stop(old).unwrap();
    let replacement = key(1, 2);
    scheduler.admit(replacement).unwrap();
    scheduler.finish_loading(replacement, Ok(())).unwrap();

    assert!(matches!(
        scheduler.enqueue_root(old, "stale"),
        Err(SchedulerError::StaleLifecycle { current, received })
            if current == replacement.lifecycle && received == old.lifecycle
    ));
    assert_eq!(scheduler.state(replacement), Ok(&ExtensionState::Idle));
}

#[test]
fn roots_queued_during_loading_preserve_fifo_order() {
    let key = key(1, 1);
    let mut scheduler = StateMachine::default();
    scheduler.admit(key).unwrap();
    scheduler.enqueue_root(key, "first").unwrap();
    scheduler.enqueue_root(key, "second").unwrap();
    assert!(scheduler.next_turn().is_none());

    scheduler.finish_loading(key, Ok(())).unwrap();
    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.work, "first");
    scheduler
        .finish_turn(key, turn.id, settled(turn.root))
        .unwrap();
    assert_eq!(scheduler.next_turn().unwrap().work, "second");
}

#[test]
fn multiple_task_wakes_enqueue_the_extension_once() {
    let (mut scheduler, key) = admitted();
    let (root, _) = scheduler.enqueue_root(key, "root").unwrap();
    let turn = scheduler.next_turn().unwrap();
    scheduler
        .finish_turn(key, turn.id, TurnOutcome::idle())
        .unwrap();
    scheduler.enqueue_task(key, Some(root), "first").unwrap();
    scheduler.enqueue_task(key, Some(root), "second").unwrap();

    let turn = scheduler.next_turn().unwrap();
    assert_eq!(turn.work, "first");
    assert!(scheduler.next_turn().is_none());
    scheduler
        .finish_turn(key, turn.id, TurnOutcome::idle())
        .unwrap();
    assert_eq!(scheduler.next_turn().unwrap().work, "second");
}

#[test]
fn stopped_running_lifecycle_can_be_replaced_after_its_turn_returns() {
    let (mut scheduler, old) = admitted();
    let (_, completion) = scheduler.enqueue_root(old, "root").unwrap();
    let turn = scheduler.next_turn().unwrap();
    scheduler.stop(old).unwrap();

    assert_eq!(scheduler.state(old), Ok(&ExtensionState::Stopping));
    scheduler
        .finish_turn(old, turn.id, TurnOutcome::idle())
        .unwrap();
    assert_eq!(
        completion.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Cancelled
    );

    let replacement = key(1, 2);
    scheduler.admit(replacement).unwrap();
}

#[test]
fn diagnostics_report_queue_turn_lag_and_worker_movement() {
    let (mut scheduler, key) = admitted();
    let (root, _) = scheduler.enqueue_root(key, "root").unwrap();
    let (queued_root, _) = scheduler.enqueue_root(key, "queued").unwrap();
    scheduler
        .extensions
        .get_mut(&key.extension)
        .unwrap()
        .tasks
        .front_mut()
        .unwrap()
        .enqueued_at -= Duration::from_millis(10);

    let queued = scheduler.diagnostics(2);
    assert_eq!(queued.worker_count, 2);
    assert_eq!(queued.ready_queue_depth, 1);
    assert_eq!(queued.queue_depth, 2);
    assert_eq!(queued.max_queue_depth, 2);
    assert_eq!(queued.extensions[0].state, ExtensionState::Queued);
    assert_eq!(queued.extensions[0].queue_depth, 2);
    assert_eq!(queued.extensions[0].max_queue_depth, 2);

    let root_turn = scheduler.next_turn_on_worker(0).unwrap();
    scheduler
        .finish_turn(key, root_turn.id, TurnOutcome::idle())
        .unwrap();
    scheduler.enqueue_task(key, Some(root), "response").unwrap();
    let second_turn = scheduler.next_turn_on_worker(1).unwrap();
    assert_eq!(second_turn.work, "queued");
    scheduler
        .finish_turn(key, second_turn.id, settled(Some(queued_root)))
        .unwrap();

    let diagnostics = scheduler.diagnostics(2);
    assert_eq!(diagnostics.queue_depth, 1);
    assert_eq!(diagnostics.max_queue_depth, 2);
    assert_eq!(diagnostics.turn_count, 2);
    assert_eq!(diagnostics.worker_movements, 1);
    assert!(diagnostics.max_enqueue_to_start_lag >= Duration::from_millis(10));
    assert_eq!(diagnostics.extensions.len(), 1);
    let extension = &diagnostics.extensions[0];
    assert_eq!(extension.state, ExtensionState::Queued);
    assert_eq!(extension.queue_depth, 1);
    assert_eq!(extension.max_queue_depth, 2);
    assert_eq!(extension.turn_count, 2);
    assert_eq!(extension.worker_movements, 1);
    assert!(extension.max_enqueue_to_start_lag >= Duration::from_millis(10));
}

struct BlockingWork {
    label: &'static str,
    started: mpsc::Sender<&'static str>,
    release: mpsc::Receiver<()>,
}

fn blocking_work(
    label: &'static str,
    started: &mpsc::Sender<&'static str>,
) -> (BlockingWork, mpsc::Sender<()>) {
    let (release, released) = mpsc::channel();
    (
        BlockingWork {
            label,
            started: started.clone(),
            release: released,
        },
        release,
    )
}

fn blocking_executor(work: Turn<BlockingWork>) -> TurnOutcome {
    work.work.started.send(work.work.label).unwrap();
    work.work.release.recv_timeout(TEST_WAIT).unwrap();
    settled(work.root)
}

#[test]
fn one_worker_serializes_independent_extensions() {
    let pool = SchedulerPool::new(PoolConfig::single_worker(), blocking_executor);
    assert_eq!(pool.worker_count(), 1);
    let handle = pool.handle();
    let first = key(1, 1);
    let second = key(2, 1);
    for key in [first, second] {
        handle.admit(key).unwrap();
        handle.finish_loading(key, Ok(())).unwrap();
    }
    let (started, starts) = mpsc::channel();
    let (first_work, release_first) = blocking_work("first", &started);
    let (second_work, release_second) = blocking_work("second", &started);
    let (_, first_done) = handle.enqueue_root(first, first_work).unwrap();
    let (_, second_done) = handle.enqueue_root(second, second_work).unwrap();

    assert_eq!(
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        "first"
    );
    assert!(starts.try_recv().is_err());
    release_first.send(()).unwrap();
    assert_eq!(
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        "second"
    );
    release_second.send(()).unwrap();
    assert_eq!(
        first_done.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    assert_eq!(
        second_done.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    pool.shutdown();
}

#[test]
fn two_workers_run_independent_extensions_concurrently() {
    let config = PoolConfig::with_manual_timers(NonZeroUsize::new(2).unwrap());
    let pool = SchedulerPool::new(config, blocking_executor);
    assert_eq!(pool.worker_count(), 2);
    let handle = pool.handle();
    let first = key(1, 1);
    let second = key(2, 1);
    for key in [first, second] {
        handle.admit(key).unwrap();
        handle.finish_loading(key, Ok(())).unwrap();
    }
    let (started, starts) = mpsc::channel();
    let (first_work, release_first) = blocking_work("first", &started);
    let (second_work, release_second) = blocking_work("second", &started);
    let (_, first_done) = handle.enqueue_root(first, first_work).unwrap();
    let (_, second_done) = handle.enqueue_root(second, second_work).unwrap();

    let mut labels = [
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
    ];
    labels.sort_unstable();
    assert_eq!(labels, ["first", "second"]);
    release_first.send(()).unwrap();
    release_second.send(()).unwrap();
    assert_eq!(
        first_done.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    assert_eq!(
        second_done.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    pool.shutdown();
}

#[test]
fn two_workers_never_run_one_extension_concurrently() {
    let pool = SchedulerPool::new(
        PoolConfig::with_manual_timers(NonZeroUsize::new(2).unwrap()),
        |turn: Turn<BlockingWork>| {
            let outcome = if turn.work.label == "root" {
                TurnOutcome::idle()
            } else {
                settled(turn.root)
            };
            turn.work.started.send(turn.work.label).unwrap();
            turn.work.release.recv_timeout(TEST_WAIT).unwrap();
            outcome
        },
    );
    let handle = pool.handle();
    let key = key(1, 1);
    handle.admit(key).unwrap();
    handle.finish_loading(key, Ok(())).unwrap();
    let (started, starts) = mpsc::channel();
    let (root_work, release_root) = blocking_work("root", &started);
    let (root, completed) = handle.enqueue_root(key, root_work).unwrap();
    assert_eq!(starts.recv_timeout(Duration::from_secs(2)).unwrap(), "root");
    let (continuation_work, release_continuation) = blocking_work("continuation", &started);
    handle
        .enqueue_task(key, Some(root), continuation_work)
        .unwrap();

    assert_eq!(starts.try_recv(), Err(mpsc::TryRecvError::Empty));
    release_root.send(()).unwrap();
    assert_eq!(
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        "continuation"
    );
    release_continuation.send(()).unwrap();
    assert_eq!(
        completed.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    pool.shutdown();
}

#[test]
fn configured_worker_count_stays_constant_as_extensions_grow() {
    let pool = SchedulerPool::new(
        PoolConfig::with_manual_timers(NonZeroUsize::new(2).unwrap()),
        |_: Turn<()>| TurnOutcome::idle(),
    );
    let handle = pool.handle();
    for extension in 1..=32 {
        let key = key(extension, 1);
        handle.admit(key).unwrap();
        handle.finish_loading(key, Ok(())).unwrap();
    }

    assert_eq!(pool.worker_count(), 2);
    let diagnostics = pool.diagnostics();
    assert_eq!(diagnostics.worker_count, 2);
    assert_eq!(diagnostics.extensions.len(), 32);
    assert!(
        diagnostics
            .extensions
            .iter()
            .all(|extension| extension.state == ExtensionState::Idle)
    );
    pool.shutdown();
}

#[test]
fn many_lifecycles_preserve_queued_tasks_and_delayed_continuations() {
    let mut scheduler = StateMachine::default();
    for lifecycle in 1..=3 {
        let keys = (1..=12)
            .map(|extension| key(extension, lifecycle))
            .collect::<Vec<_>>();
        let mut pending = Vec::new();
        for key in &keys {
            scheduler.admit(*key).unwrap();
            scheduler.finish_loading(*key, Ok(())).unwrap();
            let (root, completed) = scheduler.enqueue_root(*key, "await host").unwrap();
            let (next_root, next_completed) = scheduler.enqueue_root(*key, "next root").unwrap();
            pending.push((root, next_root, completed, next_completed));
        }
        assert_eq!(scheduler.diagnostics(2).queue_depth, 24);
        for key in &keys {
            let turn = scheduler.next_turn_on_worker(0).unwrap();
            assert_eq!(turn.key, *key);
            assert_eq!(turn.work, "await host");
            scheduler
                .finish_turn(*key, turn.id, TurnOutcome::idle())
                .unwrap();
        }
        // The isolate moves on to the next task while the first waits.
        for (key, (_, next_root, _, _)) in keys.iter().zip(&pending) {
            let turn = scheduler.next_turn_on_worker(0).unwrap();
            assert_eq!(turn.key, *key);
            assert_eq!(turn.work, "next root");
            scheduler
                .finish_turn(*key, turn.id, settled(Some(*next_root)))
                .unwrap();
        }
        assert!(scheduler.next_turn().is_none());
        assert_eq!(scheduler.diagnostics(2).queue_depth, 0);

        // Host replies can arrive after every isolate has yielded, in any order.
        for (key, (root, _, _, _)) in keys.iter().zip(&pending).rev() {
            scheduler
                .enqueue_task(*key, Some(*root), "host reply")
                .unwrap();
        }
        for (key, (root, _, _, _)) in keys.iter().zip(&pending).rev() {
            let turn = scheduler.next_turn_on_worker(1).unwrap();
            assert_eq!(turn.key, *key);
            assert_eq!(turn.work, "host reply");
            scheduler
                .finish_turn(*key, turn.id, settled(Some(*root)))
                .unwrap();
        }
        assert!(scheduler.next_turn().is_none());
        for (_, _, first, second) in pending {
            assert_eq!(
                first.recv_timeout(TEST_WAIT).unwrap(),
                CompletionOutcome::Completed
            );
            assert_eq!(
                second.recv_timeout(TEST_WAIT).unwrap(),
                CompletionOutcome::Completed
            );
        }
        assert_eq!(scheduler.diagnostics(2).queue_depth, 0);
        for key in keys {
            scheduler.stop(key).unwrap();
        }
    }
    let diagnostics = scheduler.diagnostics(2);
    assert_eq!(diagnostics.worker_count, 2);
    assert_eq!(diagnostics.turn_count, 108);
    assert_eq!(diagnostics.worker_movements, 36);
}

#[test]
fn yielding_turn_frees_a_worker_for_queued_extensions() {
    struct GatedWork {
        key: ExtensionKey,
        settles: bool,
        started: mpsc::Sender<ExtensionKey>,
        release: mpsc::Receiver<()>,
    }
    let pool = SchedulerPool::new(
        PoolConfig::with_manual_timers(NonZeroUsize::new(2).unwrap()),
        |turn: Turn<GatedWork>| {
            turn.work.started.send(turn.work.key).unwrap();
            turn.work.release.recv_timeout(TEST_WAIT).unwrap();
            if turn.work.settles {
                settled(turn.root)
            } else {
                TurnOutcome::idle()
            }
        },
    );
    let handle = pool.handle();
    let keys = (1..=4)
        .map(|extension| key(extension, 1))
        .collect::<Vec<_>>();
    for key in &keys {
        handle.admit(*key).unwrap();
        handle.finish_loading(*key, Ok(())).unwrap();
    }
    let (started, starts) = mpsc::channel();
    let mut releases = Vec::new();
    let mut completions = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let (release, released) = mpsc::channel();
        let work = GatedWork {
            key: *key,
            settles: index != 0,
            started: started.clone(),
            release: released,
        };
        let (root, completion) = handle.enqueue_root(*key, work).unwrap();
        releases.push(release);
        completions.push((root, completion));
    }
    let first_two = [
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
    ];
    assert!(first_two.contains(&keys[0]));
    assert!(first_two.contains(&keys[1]));
    assert_eq!(handle.diagnostics().queue_depth, 2);
    releases[0].send(()).unwrap();
    assert_eq!(
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        keys[2]
    );
    assert_eq!(
        handle.state(keys[0]).unwrap(),
        ExtensionState::AwaitingHostWork
    );
    releases[1].send(()).unwrap();
    assert_eq!(
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        keys[3]
    );
    releases[2].send(()).unwrap();
    releases[3].send(()).unwrap();
    for (_, completed) in completions.iter_mut().skip(1) {
        assert_eq!(
            completed.recv_timeout(TEST_WAIT).unwrap(),
            CompletionOutcome::Completed
        );
    }

    let (release_reply, released_reply) = mpsc::channel();
    handle
        .enqueue_task(
            keys[0],
            Some(completions[0].0),
            GatedWork {
                key: keys[0],
                settles: true,
                started,
                release: released_reply,
            },
        )
        .unwrap();
    assert_eq!(
        starts.recv_timeout(Duration::from_secs(2)).unwrap(),
        keys[0]
    );
    release_reply.send(()).unwrap();
    assert_eq!(
        completions[0].1.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    assert_eq!(pool.diagnostics().turn_count, 5);
    pool.shutdown();
}

#[test]
fn executor_panic_fails_only_its_extension() {
    let pool = SchedulerPool::new(PoolConfig::single_worker(), |turn: Turn<&'static str>| {
        assert_ne!(turn.work, "panic");
        settled(turn.root)
    });
    let handle = pool.handle();
    let failed_key = key(1, 1);
    let neighbor = key(2, 1);
    for key in [failed_key, neighbor] {
        handle.admit(key).unwrap();
        handle.finish_loading(key, Ok(())).unwrap();
    }
    let (_, failed) = handle.enqueue_root(failed_key, "panic").unwrap();
    let (_, completed) = handle.enqueue_root(neighbor, "complete").unwrap();

    assert!(matches!(
        failed.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Failed(failure)
            if failure.message() == "turn executor panicked"
    ));
    assert_eq!(
        completed.recv_timeout(TEST_WAIT).unwrap(),
        CompletionOutcome::Completed
    );
    let diagnostics = pool.diagnostics();
    assert_eq!(diagnostics.worker_count, 1);
    assert_eq!(diagnostics.turn_count, 2);
    assert!(matches!(
        diagnostics
            .extensions
            .iter()
            .find(|extension| extension.key == failed_key)
            .map(|extension| &extension.state),
        Some(ExtensionState::Failed(failure))
            if failure.message() == "turn executor panicked"
    ));
    pool.shutdown();
}
