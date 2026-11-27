// The Changes tree: the changed files of the window's repositories, grouped
// as each backend reports them, as a tree of folders or a flat list, embedded
// in the Changes tool window (commit.ts). A file
// opens its changes in the diff view, comparing the sides its group stands
// for; an open diff follows each new snapshot of its repository, moving to
// the group that lists the file when it left its own.
import { commands, documents, settings, workbench, workspaces } from "knot";
import type { CommandContext, Disposable, JsonValue, TreeItem, ViewRegistration, WindowTarget } from "knot";
import { diffs } from "@knot/diff";
import { backendContract } from "./contract.js";
import { isGone, type Repository } from "./repositories.js";
import {
  OPEN_FILE,
  SHOW_FILE_CHANGES,
  TREE_ID,
  childrenOf,
  groupItem,
  noChangesItem,
  noRepositoryItem,
  parseId,
  repositoryItem,
  type Layout,
} from "./rows.js";
import type { Group, Snapshot } from "./status.js";

declare module "knot" {
  interface Settings {
    /** How the Changes view lists files: `tree` of folders or flat `list`. */
    "vcs.changes.layout": string;
  }
}

export const LAYOUT = "vcs.changes.layout";
export const TOGGLE_LAYOUT = "vcs.changes.toggleLayout";

export type ChangesDeps = {
  /** The repositories covering the roots of a window. */
  covering(roots: readonly string[]): Promise<Repository[]>;
  /** What a repository's backend last reported. */
  snapshot(repository: Repository): Snapshot | undefined;
  /** A backend whose contract no longer binds. */
  gone(backend: string): void;
  /** A file of the repository was chosen in the tree of the window. */
  selected(window: WindowTarget, repository: Repository): void;
};

export type Changes = Disposable & {
  /** The embedded tree, which the tool window lays out. */
  readonly tree: ViewRegistration;
  /** A repository's snapshot was replaced, or the shown repositories changed (null). */
  refresh(repository: Repository | null): Promise<void>;
};

/** A diff open from the view, and what it shows. */
type OpenDiff = {
  readonly id: string;
  readonly repository: Repository;
  readonly group: string;
  readonly file: string;
  /** The modified side is the working file's document, not a snapshot. */
  readonly document: boolean;
  /** The tab was kept, not a preview. */
  kept: boolean;
  /** The window it was opened for. */
  readonly target: WindowTarget;
};

type Request = { readonly repository: Repository; readonly group: string; readonly file: string };

function object(value: JsonValue | undefined): Record<string, JsonValue> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, JsonValue>) : null;
}

/**
 * What `vcs.showFileChanges` was asked for, or null for a row gesture that
 * does not open it. A click asks for a preview diff, the other requests for a
 * regular one.
 */
function requestOf(args: JsonValue): (Request & { readonly preview: boolean }) | null {
  let given = object(args);
  let preview = false;
  if (given !== null && "gesture" in given) {
    if (given.gesture !== "click" && given.gesture !== "doubleClick" && given.gesture !== "enter") return null;
    preview = given.gesture === "click";
    given = object(given.arguments);
  }
  const { backend, repository, group, file } = given ?? {};
  if (typeof backend !== "string" || typeof repository !== "string" || typeof group !== "string" || typeof file !== "string") {
    commands.unavailable();
  }
  return { repository: { backend, root: repository }, group, file, preview };
}

const keyOf = ({ repository, group, file }: Request) => JSON.stringify([repository.backend, repository.root, group, file]);

export async function registerChanges(deps: ChangesDeps): Promise<Changes> {
  const open = new Map<string, OpenDiff>();
  /** The parents whose children were requested, which a refresh asks again. */
  const loaded = new Set<string>();
  let view: ViewRegistration | null = null;

  const layout = async (): Promise<Layout> => ((await settings.get(LAYOUT)) === "list" ? "list" : "tree");

  const find = (id: { backend: string; root: string; group: string }): Group | undefined =>
    deps.snapshot({ backend: id.backend, root: id.root })?.groups.find((group) => group.id === id.group);

  /** A repository's non-empty groups. */
  const groupsOf = (repository: Repository): TreeItem[] => {
    const groups = (deps.snapshot(repository)?.groups ?? []).filter((group) => group.files.length > 0);
    return groups.length === 0 && deps.snapshot(repository) !== undefined
      ? [noChangesItem()]
      : groups.map((group) => groupItem(repository, group));
  };

  async function children(parentId: string | null, window: Parameters<typeof workspaces.get>[0]): Promise<TreeItem[]> {
    if (parentId === null) {
      const roots = ((await workspaces.get(window))?.roots ?? []).map((root) => root.uri);
      const repositories = await deps.covering(roots);
      if (repositories.length === 0) return [noRepositoryItem()];
      if (repositories.length > 1) return repositories.map(repositoryItem);
      return groupsOf(repositories[0]!);
    }
    loaded.add(parentId);
    const id = parseId(parentId);
    if (id === null) return [];
    if (id[0] === "repository") return groupsOf({ backend: id[1], root: id[2] });
    if (id[0] === "file") return [];
    return childrenOf(id, find({ backend: id[1], root: id[2], group: id[3] }), await layout()) ?? [];
  }

  /** Asks the rows to be fetched again. */
  async function invalidate() {
    if (view === null) return;
    await view.invalidate(null).catch(() => {});
    for (const id of [...loaded]) {
      await view.invalidate(id).catch(() => loaded.delete(id));
    }
  }

  async function closeDiff(key: string) {
    const shown = open.get(key);
    open.delete(key);
    if (shown !== undefined) await (await diffs.client()).close(shown.id).catch(() => {});
  }

  async function compare({ repository, group, file }: Request) {
    try {
      return await (await backendContract(repository.backend).client()).compare({ repository: repository.root, group, file });
    } catch (error) {
      if (isGone(error)) deps.gone(repository.backend);
      throw error;
    }
  }

  /** Shows the diff open for `key`, keeping it unless `preview`; false when it is gone. */
  async function reuse(key: string, preview: boolean) {
    const shown = open.get(key);
    if (shown === undefined) return false;
    try {
      const client = await diffs.client();
      if (!preview) {
        await client.keep(shown.id);
        shown.kept = true;
      }
      await client.activate(shown.id);
      return true;
    } catch {
      // A later preview replaced it.
      open.delete(key);
      return false;
    }
  }

  let showing: Promise<unknown> = Promise.resolve();

  /**
   * Opens the file's changes in its group, or shows the diff already open for
   * them. Requests run one at a time, so a second gesture on a row sees the diff
   * the first one is still opening.
   */
  function showFileChanges(context: CommandContext) {
    const run = showing.then(() => openFileChanges(context));
    showing = run.catch(() => {});
    return run;
  }

  async function openFileChanges({ target, arguments: args }: CommandContext) {
    const request = requestOf(args);
    if (request === null) return;
    deps.selected(target.windowTarget, request.repository);
    const { preview } = request;
    const sides = await compare(request);
    if (sides === null) {
      await workbench.showNotice({ title: "Version Control", message: "No changes to show for this file." });
      return;
    }
    if (await reuse(keyOf(request), preview)) return;
    await openDiff(request, sides, preview, target.windowTarget);
  }

  /** Opens the diff of `sides` as a preview or a kept tab, and remembers it. */
  async function openDiff(request: Request, sides: NonNullable<Awaited<ReturnType<typeof compare>>>, preview: boolean, target: WindowTarget) {
    const document = sides.modified === "document";
    const modified = document
      ? { label: "Working Copy", document: (await documents.load(request.file, { target })).id }
      : sides.modified;
    const id = await (await diffs.client()).open({ original: sides.original, modified, ...(preview ? { preview } : {}) });
    open.set(keyOf(request), { id, repository: request.repository, group: request.group, file: request.file, document, kept: !preview, target });
  }

  /**
   * Opens a closed diff's file in the other group that now lists it, such as
   * Staged after the file was staged, in the same kind of tab; nothing happens
   * when no group lists it.
   */
  async function moveDiff(shown: OpenDiff) {
    const group = deps.snapshot(shown.repository)?.groups.find((other) => other.id !== shown.group && other.files.includes(shown.file));
    if (group === undefined) return;
    const request = { repository: shown.repository, group: group.id, file: shown.file };
    if (open.has(keyOf(request))) return;
    const sides = await compare(request);
    if (sides !== null) await openDiff(request, sides, !shown.kept, shown.target);
  }

  /** Brings the diffs of a repository up to the sides its backend now gives. */
  async function followDiffs(repository: Repository | null) {
    for (const [key, shown] of [...open]) {
      if (repository !== null && (shown.repository.backend !== repository.backend || shown.repository.root !== repository.root)) continue;
      try {
        const sides = await compare(shown);
        if (sides === null || (sides.modified === "document") !== shown.document) {
          await closeDiff(key);
          // The file left the group, as staging moves it.
          if (sides === null) await moveDiff(shown);
          continue;
        }
        const client = await diffs.client();
        await client.setText({ id: shown.id, side: "original", text: sides.original.text });
        if (sides.modified !== "document") await client.setText({ id: shown.id, side: "modified", text: sides.modified.text });
      } catch {
        // The tab was closed or the backend is gone.
        open.delete(key);
      }
    }
  }

  const registrations: Disposable[] = [];
  await settings.define({
    key: LAYOUT,
    type: "string",
    default: "tree",
    description: "How the Changes view lists files: `tree` for folders, `list` for a flat list.",
  });
  view = await workbench.registerView({
    id: TREE_ID,
    title: "Changes",
    embedded: true,
    content: { kind: "tree", provider: { getChildren: (parentId, { window }) => children(parentId, window) } },
  });
  registrations.push(view);
  registrations.push(await settings.observe(LAYOUT, invalidate));
  registrations.push(
    await commands.register(SHOW_FILE_CHANGES, showFileChanges, {
      title: "Show File Changes",
      description: "Open the changes of a file in its Changes group.",
      palette: false,
    }),
    await commands.registerForView(TREE_ID, SHOW_FILE_CHANGES, showFileChanges),
    await commands.register(
      OPEN_FILE,
      async ({ target, arguments: args }) => {
        const uri = object(args)?.uri;
        if (typeof uri === "string") await documents.open(uri, { target: target.windowTarget });
      },
      { palette: false },
    ),
    await commands.register(
      TOGGLE_LAYOUT,
      async () => {
        await settings.set(LAYOUT, (await layout()) === "tree" ? "list" : "tree");
      },
      { title: "Changes: Toggle Tree and List", description: "Switch the Changes view between folders and a flat list." },
    ),
  );

  return {
    tree: view,
    async refresh(repository) {
      await invalidate();
      await followDiffs(repository);
    },
    async [Symbol.asyncDispose]() {
      for (const key of [...open.keys()]) await closeDiff(key);
      for (const registration of registrations.reverse()) await registration[Symbol.asyncDispose]().catch(() => {});
    },
  };
}
