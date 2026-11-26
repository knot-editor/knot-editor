// The change gutter: every open document with a file is followed, its
// repository is found among the registered backends, and the lines that
// differ from that backend's baseline are marked in the gutter. Marks follow
// edits through a debounced diff against the cached baseline; the baseline
// is fetched again when the backend reports a change, the file moves, or the
// document is saved. File status is pulled for every known repository and
// painted on resources (status.ts), and each window shows the current ref
// (ref.ts).
import { buffers, commands, diff, documents, icons, keymaps, windows, workbench } from "knot";
import type {
  Buffer,
  CommandContext,
  Disposable,
  DiffHunk,
  DocumentId,
  EditorContribution,
  EditorHandle,
  ExtensionBuffer,
} from "knot";
import { diffs } from "@knot/diff";
import { backendContract, vcs } from "./contract.js";
import { contributionsOf, SHOW_HUNK } from "./gutter.js";
import { anchorLines, baselineText, hunkAt, revertEdit } from "./hunk.js";
import { targetLine, type Direction } from "./navigation.js";
import { createDiscovery, isGone, type Repository } from "./repositories.js";
import { createRefs } from "./ref.js";
import { registerChanges } from "./changes.js";
import { createStatuses, defineColors, keyOf } from "./status.js";

/** Milliseconds after the last edit before the marks are diffed again. */
const DELAY = 150;

/** How many times a diff that an edit raced is repeated before the edit's own notification takes over. */
const ATTEMPTS = 5;

/** The popover ID of the hunk popup. */
const POPUP = "vcs.hunk";

export const SHOW_CHANGES = "vcs.showChanges";
export const ROLLBACK_FILE = "vcs.rollbackFile";
export const REVERT_HUNK = "vcs.revertHunk";
export const COPY_HUNK = "vcs.copyHunk";

/** The hunk popup that is open on one of a document's editors. */
type Popup = {
  readonly editor: EditorHandle;
  /** The read-only buffer showing the baseline's lines. */
  readonly baseline: ExtensionBuffer;
  readonly listener: Disposable;
};

/** What the gutter knows about one open document. */
type Follow = {
  readonly buffer: Buffer;
  readonly id: DocumentId;
  language: string;
  uri: string | null;
  /** The open diff of the baseline against the document, if any. */
  diff: string | null;
  repository: Repository | null;
  /** The baseline text, null when the file has none. */
  baseline: string | null;
  /** The repository must be found again. */
  resolve: boolean;
  /** The baseline must be fetched again. */
  fetch: boolean;
  /** Marks may be showing. */
  painted: boolean;
  /** The hunk popup, if one is open. */
  popup: Popup | null;
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

export const NEXT_HUNK = "vcs.nextHunk";
export const PREVIOUS_HUNK = "vcs.previousHunk";

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
    refs.refresh();
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
    changed: (repository) => void changes.refresh(repository).catch(() => {}),
  });
  await defineColors();
  const refs = await createRefs({
    async fileOf(window) {
      const view = window.statusView === null ? null : windows.findView(window, window.statusView)?.view;
      if (view?.document == null) return null;
      return (await documents.get(view.document))?.resourceUri ?? null;
    },
    find: (file, roots) => discovery.find(file, roots),
    fetch: async (repository) => (await backendContract(repository.backend).client()).ref(repository.root),
    gone: drop,
  });

  const changes = await registerChanges({
    covering: (rootUris) => discovery.covering(rootUris),
    snapshot: (repository) => statuses.snapshot(repository),
    gone: drop,
  });

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
    let fetched = false;
    if (follow.fetch) {
      follow.fetch = false;
      follow.baseline = null;
      // The hunk it shows may have moved.
      await closePopup(follow);
      const { repository, uri } = follow;
      if (repository !== null && uri !== null) {
        try {
          follow.baseline = await (await backendContract(repository.backend).client()).baseline({
            repository: repository.root,
            file: uri,
          });
          fetched = true;
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
    if (fetched || (follow.repository === null && !follow.pending && follow.diff !== null)) await syncDiff(follow);
    await paint(follow);
  }

  /** Closes the follow's diff, if open. */
  async function closeDiff(follow: Follow) {
    const { diff: id } = follow;
    follow.diff = null;
    if (id !== null) await (await diffs.client()).close(id).catch(() => {});
  }

  /** Drops the popup's state once it is closed, whoever closed it. */
  async function forgetPopup(follow: Follow) {
    const { popup } = follow;
    if (popup === null) return;
    follow.popup = null;
    await popup.listener[Symbol.asyncDispose]().catch(() => {});
    await Promise.resolve(popup.baseline[Symbol.asyncDispose]()).catch(() => {});
    await popup.editor[Symbol.asyncDispose]().catch(() => {});
  }

  /** Closes the popup, if open. */
  async function closePopup(follow: Follow) {
    try {
      await follow.popup?.editor.closePopover(POPUP);
    } catch {
      // The editor is gone, and the popover with it.
    }
    await forgetPopup(follow);
  }

  /** Brings an open diff up to the baseline just fetched, or closes it when the file has none. */
  async function syncDiff(follow: Follow) {
    const { diff: id, baseline } = follow;
    if (id === null) return;
    if (baseline === null) return closeDiff(follow);
    try {
      await (await diffs.client()).setText({ id, side: "original", text: baseline });
    } catch {
      // The tab was closed.
      follow.diff = null;
    }
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
    await closePopup(follow);
    await closeDiff(follow);
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
    refs.refresh();
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
      statuses.request();
      refs.refresh();
      for (const follow of follows.values()) {
        const mine = follow.repository?.backend === backend && follow.repository.root === repository;
        if (mine || follow.repository === null) invalidate(follow, { resolve: true, fetch: true });
      }
    },
    statusChanged({ backend, repository }) {
      statuses.request(keyOf({ backend, root: repository }));
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
      id: document.id,
      language: document.language,
      diff: null,
      uri: document.resourceUri,
      repository: null,
      baseline: null,
      resolve: true,
      fetch: false,
      painted: false,
      popup: null,
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
      follow.language = now.language;
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

  /** Opens the popup for `hunk` under it, showing the baseline's lines it replaced. */
  async function openPopup(follow: Follow, { target }: CommandContext, hunk: DiffHunk) {
    const view = target.focusedView;
    if (view === null || follow.baseline === null) commands.unavailable();
    const text = baselineText(follow.baseline, hunk);
    const baseline = await buffers.create({ text, language: follow.language, readOnly: true });
    try {
      if (text !== "") {
        const { revision } = await baseline.snapshot({ startByteOffset: 0, endByteOffset: 0 });
        const end = (await baseline.lineRange((await baseline.lineCount()) - 1)).endByteOffset;
        await baseline.contributions.replace(
          [{ range: { startByteOffset: 0, endByteOffset: end }, decoration: { color: "diff.removed.background", style: "line" } }],
          { ifRevision: revision },
        );
      }
      await closePopup(follow);
      const editor = await workbench.editor(view);
      const { first, last } = anchorLines(hunk);
      const current = follow.buffer;
      const lastLine = (await current.lineCount()) - 1;
      const at = (await current.lineRange(Math.min(hunk.new.start, lastLine))).startByteOffset;
      const here = { range: { startByteOffset: at, endByteOffset: at } };
      const popup: Popup = {
        editor,
        baseline,
        listener: await editor.onDidClosePopover(({ popover }) => {
          if (popover === POPUP && follow.popup === popup) return forgetPopup(follow);
        }),
      };
      follow.popup = popup;
      await editor.openPopover(POPUP, {
        anchor: { start: first, end: last + 1 },
        toolbar: [
          { label: "Previous Hunk", icon: "vcs.previousHunk", command: PREVIOUS_HUNK },
          { label: "Next Hunk", icon: "vcs.nextHunk", command: NEXT_HUNK },
          { label: "Revert Hunk", icon: "vcs.revertHunk", command: REVERT_HUNK, arguments: here },
          { label: "Compare with Last Commit", icon: "vcs.compare", command: SHOW_CHANGES, arguments: here },
          // An insertion has no baseline lines to copy.
          ...(text === "" ? [] : [{ label: "Copy Hunk", icon: "vcs.copyHunk", command: COPY_HUNK, arguments: here }]),
        ],
        layout: {
          type: "column",
          children: text === "" ? [] : [{ type: "editor", buffer: baseline, key: "baseline" }],
        },
      });
    } catch (error) {
      if (follow.popup?.baseline !== baseline) await Promise.resolve(baseline[Symbol.asyncDispose]()).catch(() => {});
      throw error;
    }
  }

  /** The follow, buffer, and hunk of the clicked mark, or of the caret. */
  async function hunkOf({ editor, buffer, arguments: given }: CommandContext) {
    if (editor === null || buffer === null) commands.unavailable();
    const follow = [...follows.values()].find((candidate) => candidate.buffer === buffer);
    if (follow === undefined || follow.baseline === null) commands.unavailable();
    const range = (given as { range?: { startByteOffset?: number } } | null)?.range;
    const offset = typeof range?.startByteOffset === "number" ? range.startByteOffset : (await editor.selection()).caret;
    const { hunks, newRevision } = await diff(follow.baseline, buffer);
    const hunk = hunkAt(hunks, await buffer.lineAt(offset), (await buffer.lineCount()) - 1);
    if (hunk === null) commands.unavailable();
    return { follow, buffer, hunk, baseline: follow.baseline, revision: newRevision! };
  }

  /** Opens the popup of the clicked mark's hunk, or of the hunk at the caret. */
  const showHunk = async (context: CommandContext) => {
    const { follow, hunk } = await hunkOf(context);
    await openPopup(follow, context, hunk);
  };

  /** Replaces the hunk's current lines with its baseline lines. */
  const revertHunk = async (context: CommandContext) => {
    const { buffer, hunk, baseline, revision } = await hunkOf(context);
    await buffer.applyEdits([await revertEdit(baseline, hunk, buffer)], { ifRevision: revision });
  };

  /** Copies the baseline's lines that the hunk replaced. */
  const copyHunk = async (context: CommandContext) => {
    const { hunk, baseline } = await hunkOf(context);
    const text = baselineText(baseline, hunk);
    if (text === "") commands.unavailable();
    await workbench.writeClipboardText(text);
  };

  /** Moves the caret of the focused editor to the next or previous hunk, and the open popup with it. */
  const go = (direction: Direction) => async (context: CommandContext) => {
    const { editor, buffer } = context;
    if (editor === null || buffer === null) commands.unavailable();
    const follow = [...follows.values()].find((candidate) => candidate.buffer === buffer);
    if (follow === undefined || follow.baseline === null) commands.unavailable();
    const { hunks } = await diff(follow.baseline, buffer);
    const { caret } = await editor.selection();
    const line = targetLine(hunks, await buffer.lineAt(caret), direction);
    if (line === null) commands.unavailable();
    const { startByteOffset } = await buffer.lineRange(Math.min(line, (await buffer.lineCount()) - 1));
    await editor.setSelection({ anchor: startByteOffset, caret: startByteOffset }, { reveal: "center" });
    if (follow.popup === null) return;
    const hunk = hunkAt(hunks, line, (await buffer.lineCount()) - 1);
    if (hunk !== null) await openPopup(follow, context, hunk);
  };

  /** Opens the diff of the focused file's baseline against it, at the clicked mark or the caret. */
  const showChanges = async ({ editor, buffer, arguments: given }: CommandContext) => {
    if (editor === null || buffer === null) commands.unavailable();
    const follow = [...follows.values()].find((candidate) => candidate.buffer === buffer);
    if (follow === undefined || follow.baseline === null) commands.unavailable();
    const range = (given as { range?: { startByteOffset?: number } } | null)?.range;
    const offset = typeof range?.startByteOffset === "number" ? range.startByteOffset : (await editor.selection()).caret;
    const line = await buffer.lineAt(offset);
    const { hunks } = await diff(follow.baseline, buffer);
    if (hunks.length === 0) {
      await workbench.showNotice({ title: "Version Control", message: "No changes from the last commit." });
      return;
    }
    await closeDiff(follow);
    const label = await refLabel(follow);
    follow.diff = await (await diffs.client()).open({
      original: { label, text: follow.baseline, language: follow.language },
      modified: { label: "Working Copy", document: follow.id },
      line,
      preview: true,
    });
  };

  /** Restores the focused file to its baseline after a confirmation, and reloads its document. */
  const rollbackFile = async ({ buffer }: CommandContext) => {
    const follow = [...follows.values()].find((candidate) => candidate.buffer === buffer);
    if (follow === undefined || follow.uri === null || follow.repository === null) commands.unavailable();
    const { backend, root } = follow.repository;
    const name = decodeURIComponent(follow.uri.slice(follow.uri.lastIndexOf("/") + 1));
    const document = await documents.get(follow.id);
    const answer = await workbench.showConfirmation({
      title: `Roll back ${name}?`,
      message:
        document?.dirty === true
          ? "Its changes, including unsaved edits, are discarded and cannot be recovered."
          : "Its changes are discarded and cannot be recovered.",
      confirmLabel: "Roll Back",
    });
    if (answer.kind !== "confirmed") return;
    try {
      await (await backendContract(backend).client()).rollback({ repository: root, path: follow.uri });
    } catch (error) {
      if (!isGone(error)) throw error;
      drop(backend);
      return;
    }
    await documents.reload(follow.id);
  };

  /** What the backend shows as the ref of the file's repository. */
  async function refLabel({ repository }: Follow) {
    if (repository === null) return "Baseline";
    try {
      return (await (await backendContract(repository.backend).client()).ref(repository.root))?.label ?? "Baseline";
    } catch (error) {
      if (!isGone(error)) throw error;
      drop(repository.backend);
      return "Baseline";
    }
  }

  await icons.define("vcs.previousHunk", { lucide: "arrow-up" });
  await icons.define("vcs.nextHunk", { lucide: "arrow-down" });
  await icons.define("vcs.revertHunk", { lucide: "undo-2" });
  await icons.define("vcs.copyHunk", { lucide: "copy" });
  await icons.define("vcs.compare", { lucide: "git-compare" });
  const navigation = [
    await commands.register(SHOW_HUNK, showHunk, {
      title: "Show Hunk",
      description: "Show the version control baseline's lines for the hunk at the cursor.",
    }),
    await commands.register(REVERT_HUNK, revertHunk, {
      title: "Revert Hunk",
      description: "Replace the hunk at the cursor with its version control baseline.",
    }),
    await commands.register(COPY_HUNK, copyHunk, {
      title: "Copy Hunk",
      description: "Copy the version control baseline's lines for the hunk at the cursor.",
    }),
    await commands.register(SHOW_CHANGES, showChanges, {
      title: "Compare with Last Commit",
      description: "Open the file's differences from its version control baseline.",
    }),
    await commands.register(ROLLBACK_FILE, rollbackFile, {
      title: "Rollback File",
      description: "Restore the file to its version control baseline, discarding its changes.",
    }),
    await commands.register(NEXT_HUNK, go("next"), {
      title: "Go to Next Hunk",
      description: "Move the cursor to the next hunk from the version control baseline.",
    }),
    await commands.register(PREVIOUS_HUNK, go("previous"), {
      title: "Go to Previous Hunk",
      description: "Move the cursor to the previous hunk from the version control baseline.",
    }),
  ];
  keymaps.bind("alt-f5", NEXT_HUNK, { view: "editor" });
  keymaps.bind("alt-shift-f5", PREVIOUS_HUNK, { view: "editor" });

  const observation = await workbench.observe((event) => {
    if (event.type === "workspaceChanged" || event.type === "windowOpened") resolveAll();
    else if (event.type === "activeTargetChanged") refs.refresh();
  });

  return {
    async [Symbol.asyncDispose]() {
      await observation[Symbol.asyncDispose]();
      for (const command of navigation) await command[Symbol.asyncDispose]();
      await tracker[Symbol.asyncDispose]();
      await statuses[Symbol.asyncDispose]();
      await changes[Symbol.asyncDispose]();
      await refs[Symbol.asyncDispose]();
      await served[Symbol.asyncDispose]();
    },
  };
}
