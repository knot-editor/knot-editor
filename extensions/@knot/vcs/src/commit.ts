// The Changes tool window and its commit panel: the tree of changes over a
// message editor, the backend's options as checkboxes, and a Commit button, for
// the repository the window's last chosen file belongs to (or its first).
// Each repository has one draft, a persisted buffer cleared by a commit.
import { buffers, commands, history, icons, keymaps, platform, windows, workbench } from "knot";
import type {
  CommandContext,
  CompositeGroup,
  Disposable,
  ExtensionBuffer,
  ViewRegistration,
  WindowInfo,
  WindowTarget,
} from "knot";
import { backendContract } from "./contract.js";
import { SHOW_OUTPUT } from "./output.js";
import { isGone, type Repository } from "./repositories.js";
import { VIEW_ID } from "./rows.js";
import { keyOf } from "./status.js";

export const COMMIT = "vcs.commit";
const COMMIT_FROM_MESSAGE = "vcs.commit.fromMessage";
export const FOCUS_MESSAGE = "vcs.focusCommitMessage";
export const MESSAGE_HISTORY = "vcs.commitMessageHistory";
const MESSAGE_HISTORY_FROM_MESSAGE = "vcs.commitMessageHistory.fromMessage";
export const TOGGLE_OPTION = "vcs.commit.toggleOption";

type Option = { readonly id: string; readonly label: string; readonly tooltip?: string };

/** What a repository's commit panel holds. */
type Draft = {
  readonly repository: Repository;
  readonly buffer: ExtensionBuffer;
  readonly available: readonly Option[];
  /** The ids of the options that are on. */
  options: string[];
  /** The message the options last suggested, which the buffer then holds. */
  suggestion: string | null;
  /** The user's text, kept while a suggestion replaces it. */
  saved: string;
  empty: boolean;
  /** Follows the buffer to notice when it empties or fills. */
  listener: Disposable | null;
};

export type PanelDeps = {
  /** The embedded Changes tree. */
  readonly tree: ViewRegistration;
  /** The repositories covering the roots of a window. */
  covering(roots: readonly string[]): Promise<Repository[]>;
  /** A backend whose contract no longer binds. */
  gone(backend: string): void;
  /** A repository's files changed, so its status must be read again. */
  changed(repository: Repository): void;
  /** The command that toggles the tree between folders and a list. */
  readonly toggleLayout: string;
};

export type Panel = Disposable & {
  /** A file of the repository was chosen in the window's tree. */
  select(window: WindowTarget, repository: Repository): void;
  /** Lays every window out again. */
  render(): Promise<void>;
};

const isInvalidTarget = (error: unknown) => (error as { code?: string }).code === "InvalidTarget";

/** The notice's Show Output action. */
const showOutput = { title: "Show Output", command: { name: SHOW_OUTPUT } } as const;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function createPanel(deps: PanelDeps): Promise<Panel> {
  const drafts = new Map<string, Promise<Draft>>();
  const chosen = new Map<WindowTarget, Repository>();
  let view: ViewRegistration | null = null;
  let rendering: Promise<unknown> = Promise.resolve();

  const textOf = async (buffer: ExtensionBuffer) => (await buffer.snapshot()).text;

  async function replaceText(draft: Draft, text: string) {
    const { range, revision } = await draft.buffer.snapshot();
    await draft.buffer.applyEdits([{ range, text }], { ifRevision: revision });
  }

  function draftOf(repository: Repository): Promise<Draft> {
    const key = keyOf(repository);
    let draft = drafts.get(key);
    if (draft === undefined) {
      draft = (async () => {
        const client = await backendContract(repository.backend).client();
        const available = await client.commitOptions(repository.root);
        const buffer = await buffers.create({ persist: `commit:${key}` });
        const created: Draft = {
          repository,
          buffer,
          available,
          options: [],
          suggestion: null,
          saved: "",
          empty: (await textOf(buffer)).trim() === "",
          listener: null,
        };
        created.listener = await buffer.onDidChange(async () => {
          const empty = (await textOf(buffer)).trim() === "";
          if (empty === created.empty) return;
          created.empty = empty;
          await render();
        });
        return created;
      })();
      drafts.set(key, draft);
      draft.catch(() => drafts.delete(key));
    }
    return draft;
  }

  /** The repository a window's panel commits, or null when it has none. */
  async function targetOf(window: WindowInfo): Promise<Repository | null> {
    const repositories = await deps.covering(window.workspace.roots.map((root) => root.uri));
    const wanted = chosen.get(window.target);
    return repositories.find((one) => wanted !== undefined && keyOf(one) === keyOf(wanted)) ?? repositories[0] ?? null;
  }

  /** The repository's earlier messages, newest first, kept by the host. */
  const historyOf = (repository: Repository) => `commit-messages:${keyOf(repository)}`;

  const messageKey = (repository: Repository) => `message:${keyOf(repository)}`;

  async function layoutOf(window: WindowInfo): Promise<CompositeGroup> {
    const tree = { type: "tree", key: "changes", view: deps.tree } as const;
    const repository = await targetOf(window);
    if (repository === null) return { type: "column", children: [tree] };
    const draft = await draftOf(repository);
    const several = (await deps.covering(window.workspace.roots.map((root) => root.uri))).length > 1;
    const name = repository.root.replace(/\/+$/, "").split("/").pop() ?? repository.root;
    const key = keyOf(repository);
    return {
      type: "column",
      children: [
        tree,
        {
          type: "editor",
          key: messageKey(repository),
          buffer: draft.buffer,
          placeholder: `${several ? `Commit message for ${name}` : "Commit message"} (${platform === "macos" ? "⌘" : "Ctrl+"}⏎ to commit)`,
          size: { pixels: 96 },
          resize: "commit-message",
          focused: true,
          gutter: false,
          rulers: [72],
        },
        {
          type: "controls",
          items: [
            ...draft.available.map((option) => ({
              type: "checkbox" as const,
              label: option.label,
              checked: draft.options.includes(option.id),
              command: TOGGLE_OPTION,
              arguments: { backend: repository.backend, repository: repository.root, option: option.id },
              ...(option.tooltip === undefined ? {} : { tooltip: option.tooltip }),
            })),
            {
              type: "button" as const,
              label: "Commit",
              primary: true,
              disabled: draft.empty && !draft.options.includes("amend"),
              command: COMMIT,
            },
          ],
        },
      ],
    };
  }

  const toolbar = [
    { label: "Toggle Tree and List", icon: "vcs.changes.layout", command: deps.toggleLayout },
    { label: "Show Version Control Output", icon: "vcs.output", command: SHOW_OUTPUT },
  ];

  async function renderNow() {
    if (view === null) return;
    for (const window of await windows.list()) {
      try {
        await view.update({ layout: await layoutOf(window), toolbar }, { window: window.target });
      } catch (error) {
        if (!isInvalidTarget(error) && (error as { code?: string }).code !== "ViewNotFound") throw error;
      }
    }
    for (const known of [...chosen.keys()]) {
      if ((await windows.get(known).catch(() => null)) === null) chosen.delete(known);
    }
  }

  /** Renders one after another, so an update never overtakes an earlier one. */
  function render(): Promise<void> {
    const run = rendering.then(() => renderNow());
    rendering = run.catch(() => {});
    return run;
  }

  async function windowOf(context: CommandContext) {
    const window = await windows.get(context.target.windowTarget);
    if (window === null) commands.unavailable();
    return window;
  }

  async function toggleOption({ arguments: args }: CommandContext) {
    const { backend, repository, option } = (args ?? {}) as Partial<Record<string, string>>;
    if (backend === undefined || repository === undefined || option === undefined) commands.unavailable();
    const draft = await draftOf({ backend, root: repository });
    draft.options = draft.options.includes(option) ? draft.options.filter((id) => id !== option) : [...draft.options, option];
    try {
      const suggested = await (await backendContract(backend).client()).commitMessage({
        repository,
        options: draft.options,
      });
      if (suggested !== draft.suggestion) {
        if (suggested !== null) {
          if (draft.suggestion === null) draft.saved = await textOf(draft.buffer);
          await replaceText(draft, suggested);
        } else {
          await replaceText(draft, draft.saved);
        }
        draft.suggestion = suggested;
      }
    } catch (error) {
      if (isGone(error)) deps.gone(backend);
      else throw error;
    }
    await render();
  }

  async function commit(context: CommandContext) {
    const window = await windowOf(context);
    const repository = await targetOf(window);
    if (repository === null) commands.unavailable();
    const draft = await draftOf(repository);
    const amending = draft.options.includes("amend");
    const text = await textOf(draft.buffer);
    const notice = (options: { message: string; severity?: "error" }) =>
      workbench.showNotice({ title: "Version Control", target: window.target, actions: [showOutput], ...options });
    if (text.trim() === "" && !amending) {
      await notice({ message: "Enter a commit message." });
      return;
    }
    let result: { summary: string } | null;
    try {
      result = await (await backendContract(repository.backend).client()).commit({
        repository: repository.root,
        message: text,
        options: draft.options,
      });
    } catch (error) {
      if (isGone(error)) deps.gone(repository.backend);
      await notice({ message: `Commit failed: ${message(error)}`, severity: "error" });
      return;
    }
    if (result === null) return;
    draft.options = [];
    draft.suggestion = null;
    draft.saved = "";
    await replaceText(draft, "");
    await draft.buffer.clearPersisted();
    draft.empty = true;
    if (text.trim() !== "") await history.record(historyOf(repository), text.trimEnd());
    deps.changed(repository);
    await render();
    await notice({ message: `Committed ${result.summary}` });
  }

  /** Cmd+Enter is bound in every editor; only the panel's own message commits. */
  async function commitFromMessage(context: CommandContext) {
    if (context.target.tool !== VIEW_ID) commands.unavailable();
    await commit(context);
  }

  async function pickMessage(context: CommandContext) {
    const repository = await targetOf(await windowOf(context));
    if (repository === null) commands.unavailable();
    const earlier = await history.recent(historyOf(repository));
    if (earlier.length === 0) {
      await workbench.showNotice({ title: "Version Control", message: "No earlier commit messages." });
      return;
    }
    const picked = await workbench.showPicker({
      title: "Commit Message History",
      items: earlier.map((text) => {
        const [subject = "", ...rest] = text.split("\n");
        return { id: text, label: subject, ...(rest.some((line) => line.trim() !== "") ? { description: "…" } : {}) };
      }),
    });
    if (picked.kind !== "selected") return;
    await replaceText(await draftOf(repository), picked.id);
  }

  /** Ctrl+M is bound in every editor; only the panel's own message recalls. */
  async function pickMessageFromMessage(context: CommandContext) {
    if (context.target.tool !== VIEW_ID) commands.unavailable();
    await pickMessage(context);
  }

  async function focusMessage(context: CommandContext) {
    const repository = await targetOf(await windowOf(context));
    await view!.focus(
      context.target.windowTarget,
      repository === null ? undefined : { member: messageKey(repository) },
    );
  }

  await icons.define("vcs.changes.layout", { lucide: "list-tree" });
  view = await workbench.registerView({
    id: VIEW_ID,
    title: "Changes",
    content: { kind: "layout" },
    placement: { location: "leftTop", order: 1 },
  });
  const registrations: Disposable[] = [
    view,
    await commands.register(COMMIT, commit, {
      title: "Commit",
      description: "Commit the staged changes with the message of the Changes panel.",
    }),
    await commands.register(COMMIT_FROM_MESSAGE, commitFromMessage, { palette: false }),
    await commands.register(FOCUS_MESSAGE, focusMessage, {
      title: "Focus Commit Message",
      description: "Show the Changes panel and move to its commit message.",
    }),
    await commands.register(MESSAGE_HISTORY, pickMessage, {
      title: "Commit Message History…",
      description: "Replace the commit message with an earlier one of the repository.",
    }),
    await commands.register(MESSAGE_HISTORY_FROM_MESSAGE, pickMessageFromMessage, { palette: false }),
    await commands.register(TOGGLE_OPTION, toggleOption, { palette: false }),
  ];
  const mod = platform === "macos" ? "cmd" : "ctrl";
  keymaps.bind(`${mod}-alt-c`, FOCUS_MESSAGE);
  keymaps.bind("ctrl-m", MESSAGE_HISTORY_FROM_MESSAGE, { view: "editor" });
  keymaps.bind(`${mod}-enter`, COMMIT_FROM_MESSAGE, { view: "editor" });

  return {
    select(window, repository) {
      const previous = chosen.get(window);
      chosen.set(window, repository);
      if (previous === undefined || keyOf(previous) !== keyOf(repository)) void render().catch(() => {});
    },
    render,
    async [Symbol.asyncDispose]() {
      for (const registration of registrations.reverse()) await registration[Symbol.asyncDispose]().catch(() => {});
      for (const draft of drafts.values()) {
        const ready = await draft.catch(() => null);
        await ready?.listener?.[Symbol.asyncDispose]().catch(() => {});
        await Promise.resolve(ready?.buffer[Symbol.asyncDispose]()).catch(() => {});
      }
    },
  };
}
