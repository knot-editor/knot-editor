# D187: File watches start off the foreground

Status: 🚧 checkpoint 1 implemented, awaiting review. Roadmap item [D187](../roadmap.md).
References: [watching](../architecture/documents-and-persistence.md#watching),
[workspace file index](../architecture/documents-and-persistence.md#workspace-file-index),
[detecting external changes](../architecture/documents-and-persistence.md#detecting-external-changes),
[file watching decisions](../decisions/documents-and-persistence.md#file-watching).

## Goal

Starting or releasing a file watch never blocks the foreground. On macOS,
`notify` starts an FSEvents stream synchronously (`FSEventStreamStart` waits on
`fseventsd`, about 100 ms) and its `stop()` joins the stream's thread. Today
every watch user pays that on the foreground: the workspace file index (one
recursive watch per root), document watches (one non-recursive watch per
document's parent directory; FSEvents backs non-recursive watches too), and
extension `files.watch`. Switching to a multi-root project or restoring a
session with documents in several directories no longer freezes the window,
and no change between a request and the stream running is missed.

## Design

- **The local provider starts and stops asynchronously.**
  `LocalFileSystemProvider::watch` keeps its synchronous validation (normalize,
  canonicalize) so its errors still reach the caller, then returns a guard at
  once and builds the `notify` watcher on a thread of its own. A guard dropped
  before the start finishes discards the watcher when it arrives. Dropping a
  guard stops the watcher on a background thread. The registry and its
  covering logic are unchanged.
- **Providers report when a watch is live.** `WatchEvent` gains `Started`. The
  local provider sends it once the stream runs; the memory provider, and a
  local provider without OS events, send it immediately. A start that fails
  after `watch` returned logs the error and still sends `Started`: the watch
  stays silent rather than leaving anyone waiting.
- **The registry gives each subscription a one-time live signal.** It records
  whether each provider watch has started. A subscription to a running watch
  is live immediately; one that joins a starting watch becomes live when the
  provider reports `Started`. The signal precedes every batch of that
  subscription.
- **Each user waits for it where a gap would matter.**
  - The file index scans once, after its subscription is live, then applies
    batches in order. A fresh root's first query waits about 100 ms longer;
    no root is ever scanned twice to cover the start.
  - A document watch rechecks its document once when live, covering a change
    between reading the file and the stream starting. On an already-running
    watch the recheck finds the version unchanged.
  - `files.watch` resolves its promise only once the watch is live, so an
    extension that awaits it and then reads the disk misses nothing, and
    receives no synthetic batch. The explorer awaits its root watches
    sequentially on its worker; that latency is off the foreground.

## Exclusions

- No FSEvents history replay (`sinceWhen`); `notify` does not expose it.
- No recovery for a failed start beyond logging; the index then lags until its
  next full scan.
- No change to coalescing, sharing, or batch routing.

## Deferred decisions

None.

## Checkpoints

### 1. Asynchronous provider watches and the live signal 🔍

Deliverables:

- ✅ `WatchEvent::Started`; the local provider's asynchronous start and stop;
  the memory provider's immediate `Started`, or a held one after
  `defer_watch_starts` until `start_watches` (test support).
- ✅ The registry's per-watch started state and per-subscription live signal
  (`WatchSubscription::live`), delivered before any batch, immediately for a
  running watch. Events before a subscription is live are not reported, which
  also covers platform events that race ahead of `Started`.
- ✅ Model tests: a subscription to a fresh watch becomes live only after
  `Started`; one sharing a running watch is live at once; the live signal
  precedes batches; a guard dropped before its local start completes leaves no
  watcher; with OS events, a write made right after the live signal is
  reported.

Implementation context:

- `crates/knot-model/src/filesystem/mod.rs`: `WatchEvent`, `WatchGuard`,
  `FileSystemProvider::watch` contract docs.
- `crates/knot-model/src/filesystem/local.rs` `watch`: split validation from
  the `notify::recommended_watcher` + `watch` call, which moves to a spawned
  thread; the guard owns shared state (the watcher once built, a dropped flag)
  and stops the watcher off the calling thread on drop. The `os_events: false`
  path sends `Started` immediately.
- `crates/knot-model/src/filesystem/memory.rs` `watch`: send `Started` after
  registering the watcher.
- `crates/knot-model/src/watch.rs`: `ProviderWatch` gains a started flag;
  `absorb` handles `Started`; `Subscriber` holds the live signal (for example a
  `futures::channel::oneshot` sender returned to the caller through
  `WatchSubscription`), fired from the delivery thread so ordering with
  batches holds. Existing registry tests in the same file are the pattern.
- The registry no longer blocks under its state lock, since the provider call
  is quick.

Validation: the new tests in `watch.rs`, existing filesystem and registry
tests, `just check`.

Review gate: the provider and registry contract (event, live signal shape,
failure behavior) before users move onto it.

### 2. Watch users wait for live

Deliverables:

- The file index lease awaits live before its single scan.
- Document watches recheck once when live.
- `files.watch` resolves once live.
- Docs: [watching](../architecture/documents-and-persistence.md#watching) and
  [workspace file index](../architecture/documents-and-persistence.md#workspace-file-index)
  describe the asynchronous start, the live signal, and scan-after-live;
  `knot.d.ts` and [extension guide](../extension-api.md) state that the
  `files.watch` promise resolves once the watch is live; a
  [file watching](../decisions/documents-and-persistence.md#file-watching)
  entry records why the index waits rather than scanning twice; the roadmap
  item is removed when the plan completes.
- A `try.*` post-init command set for a manual check: switching to a project
  with several roots and opening documents from several directories, with no
  visible stall.

Tests:

- File index: a lease's queries resolve after the first scan, which starts
  only once live (memory provider: immediate); a change made between lease and
  live is indexed.
- Document watch: a document changed on disk between opening and the watch
  going live is reloaded.
- `files.watch`: the existing product test still passes; the fake host in
  `scripts/extensions/testing` keeps resolving immediately.

Implementation context:

- `src/app/file_index.rs` `lease`: move `scanned.scan(policy)` behind awaiting
  the subscription's live signal inside the background task.
- `src/app/product/document_watch.rs` `sync`: the per-document task awaits live,
  calls `recheck(id, cx)`, then loops over batches.
- `src/app/extensions/host.rs` `watch_files` returns the live signal with the
  id; `src/app/extensions/host/handlers.rs` `watch_files` awaits it after the
  foreground call, before returning the id.
- Tests: `src/app/product/tests/files.rs` (index and `files.watch`),
  `src/app/product/tests/document_watch.rs`.

Validation: product tests, extension tests, `just check`.

Review gate: manual check in the real app with the `Try:` commands.
