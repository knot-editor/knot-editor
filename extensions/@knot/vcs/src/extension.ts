// The change gutter: every open document with a file is followed, its
// repository is found among the registered backends, and the lines that
// differ from that backend's baseline are marked in the gutter. Marks follow
// edits through a debounced diff against the cached baseline; the baseline
// is fetched again when the backend reports a change, the file moves, or the
// document is saved. File status is pulled for every known repository and
// painted on resources; see status.ts.
import { commands, diff, documents, keymaps, windows, workbench } from "knot";
import type { Buffer, CommandContext, Disposable, DocumentId, EditorContribution } from "knot";
import { backendContract, vcs } from "./contract.js";
import { contributionsOf } from "./hunks.js";
import { targetLine, type Direction } from "./navigation.js";
import { createDiscovery, isGone, type Repository } from "./repositories.js";
import { createStatuses, defineColors, keyOf } from "./status.js";

/** Milliseconds after the last edit before the marks are diffed again. */
const DELAY = 150;

/** How many times a diff that an edit raced is repeated before the edit's own notification takes over. */
const ATTEMPTS = 5;

/** What the gutter knows about one open document. */
type Follow = {
  readonly buffer: Buffer;
  uri: string | null;
  repository: Repository | null;
  /** The baseline text, null when the file has none. */
  baseline: string | null;
  /** The repository must be found again. */
  resolve: boolean;
  /** The baseline must be fetched again. */
  fetch: boolean;
  /** Marks may be showing. */
  painted: boolean;
  timer: ReturnType<typeof setTimeout> | null;
  running: boolean;
  /** A request arrived while a run was in flight, joined into one more run. */
  pending: boolean;
  closed: boolean;
  disposables: Disposable[];
};

function isBufferGone(error: unknown) {
  const code = (error as { code?: string }).code;
  return code === "BufferClosed" || code === "InvalidTarget" || code === "Cancelled" || code === "OwnershipDisposed";
}

function isConflict(error: unknown) {
  return (error as { code?: string }).code === "RevisionConflict";
}

function same(left: Repository | null, right: Repository | null) {
  return left?.backend === right?.backend && left?.root === right?.root;
}

export const NEXT_CHANGE = "vcs.nextChange";
export const PREVIOUS_CHANGE = "vcs.previousChange";

/** Serves `vcs` to backends and marks changes; resolves with what stops both. */
export async function start(): Promise<Disposable> {
  const registered = new Set<string>();
  const follows = new Map<DocumentId, Follow>();

  const drop = (backend: string) => {
    if (!registered.delete(backend)) return;
    discovery.invalidate(backend);
    for (const follow of follows.values()) {
      if (follow.repository?.backend === backend) invalidate(follow, { resolve: true });
    }
    statuses.request();
  };
  const discovery = createDiscovery(() => [...registered], drop);
  const statuses = createStatuses({
    // The repositories of every root, and those of followed documents outside any root.
    async known() {
      const known = await discovery.covering(await roots());
      for (const { repository } of follows.values()) {
        if (repository === null || !registered.has(repository.backend)) continue;
        if (!known.some((other) => keyOf(other) === keyOf(repository))) known.push(repository);
      }
      return known;
    },
    fetch: async (repository) => (await backendContract(repository.backend).client()).status(repository.root),
    gone: drop,
  });
  await defineColors();

  /** The URIs of every window's roots. */
  const roots = async () => [...new Set((await windows.list()).flatMap((window) => window.workspace.roots.map((root) => root.uri)))];

  async function replace(follow: Follow, items: readonly EditorContribution[], revision: number) {
    await follow.buffer.contributions.replace(items, { ifRevision: revision });
    follow.painted = items.length > 0;
  }

  /** Marks the lines that differ from the baseline, or clears the marks. */
  async function paint(follow: Follow) {
    if (follow.baseline === null && !follow.painted) return;
    for (let attempt = 0; attempt < ATTEMPTS; attempt += 1) {
      try {
        if (follow.baseline === null) {
          const { revision } = await follow.buffer.snapshot({ startByteOffset: 0, endByteOffset: 0 });
          await replace(follow, [], revision);
        } else {
          const { hunks, newRevision } = await diff(follow.baseline, follow.buffer);
          const items = await contributionsOf(follow.buffer, hunks);
          if (items.length > 0 || follow.painted) await replace(follow, items, newRevision!);
        }
        return;
      } catch (error) {
        // An edit raced the diff: its own notification diffs again.
        if (!isConflict(error)) throw error;
      }
    }
  }

  async function update(follow: Follow) {
    if (follow.resolve) {
      follow.resolve = false;
      const next = follow.uri === null ? null : await discovery.find(follow.uri, await roots());
      if (!same(next, follow.repository)) {
        follow.repository = next;
        follow.fetch = true;
        statuses.request();
      }
    }
    if (follow.fetch) {
      follow.fetch = false;
      follow.baseline = null;
      const { repository, uri } = follow;
      if (repository !== null && uri !== null) {
        try {
          follow.baseline = await (await backendContract(repository.backend).client()).baseline({
            repository: repository.root,
            file: uri,
          });
        } catch (error) {
          // A backend that stopped is dropped, and the file looked up again.
          if (!isGone(error)) throw error;
          drop(repository.backend);
          follow.repository = null;
          follow.resolve = true;
          follow.pending = true;
        }
      }
    }
    await paint(follow);
  }

  async function run(follow: Follow) {
    if (follow.running) {
      follow.pending = true;
      return;
    }
    follow.running = true;
    try {
      do {
        follow.pending = false;
        if (follow.closed) break;
        await update(follow);
      } while (follow.pending);
    } catch (error) {
      if (!isBufferGone(error)) throw error;
    } finally {
      follow.running = false;
    }
  }

  /** Runs in the background: a failure is a mark not painted, never a failed callback. */
  const kick = (follow: Follow) => void run(follow).catch(() => {});

  function invalidate(follow: Follow, what: { resolve?: boolean; fetch?: boolean }) {
    if (what.resolve) follow.resolve = true;
    if (what.fetch) follow.fetch = true;
    kick(follow);
  }

  function schedule(follow: Follow) {
    if (follow.timer !== null) clearTimeout(follow.timer);
    follow.timer = setTimeout(() => {
      follow.timer = null;
      kick(follow);
    }, DELAY);
  }

  async function release(follow: Follow) {
    follow.closed = true;
    if (follow.timer !== null) clearTimeout(follow.timer);
    follow.timer = null;
    for (const disposable of follow.disposables) await disposable[Symbol.asyncDispose]().catch(() => {});
    follow.disposables = [];
    try {
      await follow.buffer.contributions[Symbol.asyncDispose]();
    } catch (error) {
      if (!isBufferGone(error)) throw error;
    }
  }

  const resolveAll = () => {
    statuses.request();
    for (const follow of follows.values()) invalidate(follow, { resolve: true });
  };

  const served = await vcs.serve({
    register(id) {
      registered.add(id);
      discovery.invalidate(id);
      resolveAll();
    },
    unregister(id) {
      drop(id);
    },
    changed({ backend, repository }) {
      discovery.invalidate(backend);
      statuses.request(keyOf({ backend, root: repository }));
      for (const follow of follows.values()) {
        const mine = follow.repository?.backend === backend && follow.repository.root === repository;
        if (mine || follow.repository === null) invalidate(follow, { resolve: true, fetch: true });
      }
    },
    async backends() {
      // A backend that stopped without unregistering has nothing to bind.
      for (const id of [...registered]) {
        try {
          await backendContract(id).client();
        } catch {
          drop(id);
        }
      }
      return [...registered];
    },
  });

  const tracker = await documents.track(async (document, tracked) => {
    const follow: Follow = {
      buffer: document.buffer,
      uri: document.resourceUri,
      repository: null,
      baseline: null,
      resolve: true,
      fetch: false,
      painted: false,
      timer: null,
      running: false,
      pending: false,
      closed: false,
      disposables: [],
    };
    follows.set(document.id, follow);
    try {
      follow.disposables.push(await document.buffer.onDidChange(() => schedule(follow)));
    } catch (error) {
      if (!isBufferGone(error)) throw error;
    }
    tracked.onDidChange((now, before) => {
      if (now.resourceUri !== before.resourceUri) {
        follow.uri = now.resourceUri;
        invalidate(follow, { resolve: true, fetch: true });
      } else if (before.dirty && !now.dirty) {
        invalidate(follow, { fetch: true });
      }
    });
    tracked.onDidClose(async () => {
      follows.delete(document.id);
      statuses.request();
      await release(follow);
    });
    kick(follow);
  });

  /** Moves the caret of the focused editor to the next or previous change. */
  const go = (direction: Direction) => async ({ editor, buffer }: CommandContext) => {
    if (editor === null || buffer === null) commands.unavailable();
    const follow = [...follows.values()].find((candidate) => candidate.buffer === buffer);
    if (follow === undefined || follow.baseline === null) commands.unavailable();
    const { hunks } = await diff(follow.baseline, buffer);
    const { caret } = await editor.selection();
    const line = targetLine(hunks, await buffer.lineAt(caret), direction);
    if (line === null) commands.unavailable();
    const { startByteOffset } = await buffer.lineRange(Math.min(line, (await buffer.lineCount()) - 1));
    await editor.setSelection({ anchor: startByteOffset, caret: startByteOffset }, { reveal: "center" });
  };
  const navigation = [
    await commands.register(NEXT_CHANGE, go("next"), {
      title: "Go to Next Change",
      description: "Move the cursor to the next line changed from the version control baseline.",
    }),
    await commands.register(PREVIOUS_CHANGE, go("previous"), {
      title: "Go to Previous Change",
      description: "Move the cursor to the previous line changed from the version control baseline.",
    }),
  ];
  keymaps.bind("alt-f5", NEXT_CHANGE, { view: "editor" });
  keymaps.bind("alt-shift-f5", PREVIOUS_CHANGE, { view: "editor" });

  const observation = await workbench.observe((event) => {
    if (event.type === "workspaceChanged" || event.type === "windowOpened") resolveAll();
  });

  return {
    async [Symbol.asyncDispose]() {
      await observation[Symbol.asyncDispose]();
      for (const command of navigation) await command[Symbol.asyncDispose]();
      await tracker[Symbol.asyncDispose]();
      await statuses[Symbol.asyncDispose]();
      await served[Symbol.asyncDispose]();
    },
  };
}
