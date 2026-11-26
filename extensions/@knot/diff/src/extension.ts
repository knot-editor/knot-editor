// The diff view: a composite tab of two editors, either side an open document
// (editable) or a read-only snapshot, with a strip joining their changed
// lines. Both sides are compared natively after each edit, debounced, and the
// result paints bands and wedges on each editor, redraws the strip, and links
// the editors' scrolling. The inline layout keeps only the modified editor and
// shows each hunk's original lines above it as block rows.
import { buffers, commands, diff, documents, icons, keymaps, settings, windows, workbench } from "knot";
import type {
  Buffer,
  CommandContext,
  CompositeGroup,
  CompositeInstance,
  CompositeRegistration,
  Disposable,
  DiffHunk,
  DocumentId,
  EditorHandle,
  ExtensionBuffer,
  ViewNodeTarget,
} from "knot";
import { diffs } from "./contract.js";
import { blocksOf, connectorLinks, decorationsOf, scrollLink, targetLine, type Side } from "./hunks.js";

/** Milliseconds after the last edit before the sides are compared again. */
const DELAY = 150;

/** The key context of a diff's editors, for bindings that apply only in a diff. */
export const DIFF_CONTEXT = "diff";

declare module "knot" {
  interface Settings {
    /** How a diff tab opens: side by side, or inline above the modified text. */
    "diff.layout": "sideBySide" | "inline";
  }
}

export const NEXT_HUNK = "diff.nextHunk";
export const PREVIOUS_HUNK = "diff.previousHunk";
export const TOGGLE_LAYOUT = "diff.toggleLayout";

type Layout = "sideBySide" | "inline";

type SnapshotSpec = { label: string; text: string; language?: string | undefined };
type DocumentSpec = { label: string; document: string };
type SideSpec = SnapshotSpec | DocumentSpec;

/** One side of an open diff. */
type Part = {
  label: string;
  language: string | null;
  buffer: Buffer;
  /** The snapshot's buffer, which this package disposes; null for a document. */
  owned: ExtensionBuffer | null;
  document: DocumentId | null;
  editor: EditorHandle | null;
  view: ViewNodeTarget | null;
  listener: Disposable | null;
};

/** An open diff. */
type Open = {
  id: string;
  composite: CompositeInstance;
  parts: [Part, Part];
  layout: Layout;
  hunks: readonly DiffHunk[];
  timer: ReturnType<typeof setTimeout> | null;
  running: boolean;
  /** A request arrived while a run was in flight, joined into one more run. */
  pending: boolean;
  closed: boolean;
};

const SIDES: readonly Side[] = ["original", "modified"];

function isGone(error: unknown) {
  const code = (error as { code?: string }).code;
  return code === "BufferClosed" || code === "InvalidTarget" || code === "Cancelled" || code === "OwnershipDisposed";
}

/** Awaits `work`, ignoring a failure: the thing it tears down is gone already. */
async function quietly(work: PromiseLike<unknown> | undefined) {
  try {
    await work;
  } catch {
    // Nothing is left to release.
  }
}

function same(left: readonly DiffHunk[], right: readonly DiffHunk[]) {
  return JSON.stringify(left) === JSON.stringify(right);
}

/** Serves the diff contract and the hunk commands; resolves with what stops both. */
export async function start(): Promise<Disposable> {
  const open = new Map<string, Open>();
  let counter = 0;

  await settings.define({
    key: "diff.layout",
    type: "string",
    default: "sideBySide",
    description:
      'How a diff tab opens: "sideBySide" shows the original and modified texts in two editors, "inline" shows the modified text with the original\'s changed lines above it.',
  });
  await icons.define("diff.tab", { lucide: "git-compare" });
  await icons.define("diff.previous", { lucide: "arrow-up" });
  await icons.define("diff.next", { lucide: "arrow-down" });
  await icons.define("diff.layout.sideBySide", { lucide: "rows-2" });
  await icons.define("diff.layout.inline", { lucide: "columns-2" });
  const registration: CompositeRegistration = await workbench.registerComposite({ id: "diff.view" });

  const layout = ({ parts, hunks, layout }: Pick<Open, "parts" | "hunks" | "layout">): CompositeGroup => {
    const source = (part: Part) => (part.owned !== null ? { buffer: part.owned } : { document: part.document! });
    const modified = { type: "editor", key: "modified", focused: true, ...source(parts[1]) } as const;
    if (layout === "inline") return { type: "row", children: [modified] };
    return {
      type: "row",
      children: [
        { type: "editor", key: "original", ...source(parts[0]) },
        { type: "connector", first: "original", second: "modified", links: connectorLinks(hunks) },
        modified,
      ],
    };
  };

  /** The scroll link of a layout: the inline one has a single editor to link. */
  const link = ({ hunks, layout }: Pick<Open, "hunks" | "layout">) =>
    layout === "inline" ? null : scrollLink(hunks);

  /** The buttons; the last one shows the layout it switches to. */
  const toolbar = (layout: Layout) => [
    { label: "Previous Difference", icon: "diff.previous", command: PREVIOUS_HUNK },
    { label: "Next Difference", icon: "diff.next", command: NEXT_HUNK },
    {
      label: layout === "inline" ? "Show Side by Side" : "Show Inline",
      icon: `diff.layout.${layout}`,
      command: TOGGLE_LAYOUT,
    },
  ];

  async function makePart(spec: SideSpec): Promise<Part> {
    if ("document" in spec) {
      const document = await documents.get(spec.document as DocumentId);
      if (document === null) throw new Error(`no open document ${spec.document}`);
      return {
        label: spec.label,
        language: null,
        buffer: document.buffer,
        owned: null,
        document: document.id,
        editor: null,
        view: null,
        listener: null,
      };
    }
    const owned = await buffers.create({ text: spec.text, language: spec.language ?? null, readOnly: true });
    return {
      label: spec.label,
      language: spec.language ?? null,
      buffer: owned,
      owned,
      document: null,
      editor: null,
      view: null,
      listener: null,
    };
  }

  /** Finds the editors of the diff's tab: both sides, or only the modified one inline. */
  async function locate(diffView: Open) {
    for (const window of await windows.list()) {
      const found = windows.findView(window, diffView.composite.target);
      if (found === null) continue;
      const editors = found.view.children.filter((child) => child.kind === "editor");
      const shownParts = diffView.layout === "inline" ? [diffView.parts[1]] : diffView.parts;
      if (diffView.layout === "inline") {
        diffView.parts[0].view = null;
        diffView.parts[0].editor = null;
      }
      for (const [index, part] of shownParts.entries()) {
        const view = editors[index];
        if (view === undefined) continue;
        part.view = view.target;
        part.editor = await workbench.editor(view.target);
        await part.editor.setKeyContext(DIFF_CONTEXT, true);
      }
      return;
    }
  }

  /** Whether the diff's tab is still shown. */
  async function shown(diffView: Open) {
    for (const window of await windows.list()) {
      if (windows.findView(window, diffView.composite.target) !== null) return true;
    }
    return false;
  }

  async function release(diffView: Open) {
    if (diffView.closed) return;
    diffView.closed = true;
    open.delete(diffView.id);
    if (diffView.timer !== null) clearTimeout(diffView.timer);
    for (const part of diffView.parts) {
      await quietly(part.listener?.[Symbol.asyncDispose]());
      await quietly(part.owned?.[Symbol.asyncDispose]());
    }
  }

  /** Paints both editors from the hunks of the revisions compared. */
  async function paint(diffView: Open, revisions: [number, number]) {
    for (const [index, part] of diffView.parts.entries()) {
      // A layout change during the awaits clears the part's editor, so paint the one it had.
      const editor = part.editor;
      if (editor === null) continue;
      const items = await decorationsOf(part.buffer, diffView.hunks, SIDES[index]!);
      await editor.setDecorations(items, { ifRevision: revisions[index]! });
    }
  }

  /** Shows each hunk's original lines above the modified text inline, and none otherwise. */
  async function showBlocks(diffView: Open) {
    const editor = diffView.parts[1].editor;
    if (editor === null) return;
    const blocks = diffView.layout === "inline" ? blocksOf(diffView.hunks, diffView.parts[0].buffer) : [];
    await editor.setBlocks(blocks);
  }

  /** Compares the sides and shows the result; an edit that raced it is compared again by its own notification. */
  async function refresh(diffView: Open) {
    const [original, modified] = diffView.parts;
    const result = await diff(original.buffer, modified.buffer);
    const changed = !same(result.hunks, diffView.hunks);
    diffView.hunks = result.hunks;
    if (changed) {
      await diffView.composite.update({ layout: layout(diffView), scrollLink: link(diffView) });
    }
    await showBlocks(diffView);
    try {
      await paint(diffView, [result.oldRevision!, result.newRevision!]);
    } catch (error) {
      if ((error as { code?: string }).code !== "RevisionConflict") throw error;
    }
  }

  async function run(diffView: Open) {
    if (diffView.running) {
      diffView.pending = true;
      return;
    }
    diffView.running = true;
    try {
      while (!diffView.closed) {
        diffView.pending = false;
        try {
          await refresh(diffView);
        } catch (error) {
          if (!isGone(error)) throw error;
          // A side's buffer or editor was replaced while comparing; a request that raced it compares again.
          if (!(await shown(diffView))) {
            await release(diffView);
            break;
          }
        }
        if (!diffView.pending) break;
      }
    } finally {
      diffView.running = false;
    }
  }

  const kick = (diffView: Open) => void run(diffView).catch(() => {});

  function schedule(diffView: Open) {
    if (diffView.timer !== null) clearTimeout(diffView.timer);
    diffView.timer = setTimeout(() => {
      diffView.timer = null;
      kick(diffView);
    }, DELAY);
  }

  async function listen(diffView: Open, part: Part) {
    await quietly(part.listener?.[Symbol.asyncDispose]());
    part.listener = await part.buffer.onDidChange(() => schedule(diffView));
  }

  const titleOf = ([original, modified]: [Part, Part]) => `${original.label} ↔ ${modified.label}`;

  async function openDiff(options: {
    original: SideSpec;
    modified: SideSpec;
    line?: number | undefined;
    preview?: boolean | undefined;
  }) {
    const parts: [Part, Part] = [await makePart(options.original), await makePart(options.modified)];
    try {
      const { hunks } = await diff(parts[0].buffer, parts[1].buffer);
      const style: Layout = (await settings.get("diff.layout")) === "inline" ? "inline" : "sideBySide";
      const shape = { parts, hunks, layout: style };
      const scroll = link(shape);
      const composite = await registration.open({
        title: titleOf(parts),
        icon: "diff.tab",
        layout: layout(shape),
        ...(scroll === null ? {} : { scrollLink: scroll }),
        toolbar: toolbar(style),
        ...(options.preview === true ? { preview: true } : {}),
      });
      counter += 1;
      const diffView: Open = {
        id: `diff-${counter}`,
        composite,
        parts,
        layout: style,
        hunks,
        timer: null,
        running: false,
        pending: false,
        closed: false,
      };
      open.set(diffView.id, diffView);
      await locate(diffView);
      for (const part of parts) await listen(diffView, part);
      const modified = parts[1];
      if (options.line !== undefined && modified.editor !== null) {
        const line = Math.min(options.line, (await modified.buffer.lineCount()) - 1);
        const { startByteOffset } = await modified.buffer.lineRange(line);
        await modified.editor.setSelection({ anchor: startByteOffset, caret: startByteOffset }, { reveal: "center" });
      }
      kick(diffView);
      return diffView;
    } catch (error) {
      for (const part of parts) await quietly(part.owned?.[Symbol.asyncDispose]());
      throw error;
    }
  }

  function openOf(id: string) {
    const diffView = open.get(id);
    if (diffView === undefined) throw new Error(`no open diff ${id}`);
    return diffView;
  }

  const served = await diffs.serve({
    async open(options) {
      return (await openDiff(options)).id;
    },
    async setText({ id, side, text }) {
      const diffView = openOf(id);
      const part = diffView.parts[SIDES.indexOf(side)]!;
      if (part.owned === null) throw new Error("a document side has no text to replace");
      // A read-only buffer takes no edits, so the side shows a new buffer.
      const replacement = await buffers.create({ text, language: part.language, readOnly: true });
      const old = part.owned;
      part.owned = replacement;
      part.buffer = replacement;
      try {
        await diffView.composite.update({ layout: layout(diffView) });
      } catch (error) {
        part.owned = old;
        part.buffer = old;
        await quietly(replacement[Symbol.asyncDispose]());
        throw error;
      }
      await quietly(old[Symbol.asyncDispose]());
      await locate(diffView);
      await listen(diffView, part);
      kick(diffView);
    },
    async keep(id) {
      await openOf(id).composite.keep();
    },
    async activate(id) {
      await openOf(id).composite.activate();
    },
    async close(id) {
      const diffView = open.get(id);
      if (diffView === undefined) return;
      await quietly(diffView.composite.close());
      await release(diffView);
    },
  });

  /** Moves the caret of the focused side to the next or previous hunk. */
  const go = (direction: "next" | "previous") => async ({ target, editor, buffer }: CommandContext) => {
    if (editor === null || buffer === null) commands.unavailable();
    const diffView = [...open.values()].find((candidate) => candidate.composite.target === target.viewTarget);
    if (diffView === undefined) commands.unavailable();
    const index = diffView.parts.findIndex((part) => part.view !== null && part.view === target.focusedView);
    const { caret } = await editor.selection();
    const line = targetLine(diffView.hunks, SIDES[index < 0 ? 1 : index]!, await buffer.lineAt(caret), direction);
    if (line === null) commands.unavailable();
    const { startByteOffset } = await buffer.lineRange(Math.min(line, (await buffer.lineCount()) - 1));
    await editor.setSelection({ anchor: startByteOffset, caret: startByteOffset }, { reveal: "center" });
  };

  /** Switches the focused diff between side by side and inline. */
  const toggleLayout = async ({ target }: CommandContext) => {
    const diffView = [...open.values()].find((candidate) => candidate.composite.target === target.viewTarget);
    if (diffView === undefined) commands.unavailable();
    const next: Layout = diffView.layout === "inline" ? "sideBySide" : "inline";
    const shape = { parts: diffView.parts, hunks: diffView.hunks, layout: next };
    // A failed update changes nothing, so the layout is recorded once it succeeds.
    await diffView.composite.update({ layout: layout(shape), scrollLink: link(shape), toolbar: toolbar(next) });
    diffView.layout = next;
    await locate(diffView);
    await showBlocks(diffView);
    kick(diffView);
  };

  const navigation = [
    await commands.register(TOGGLE_LAYOUT, toggleLayout, {
      title: "Toggle Diff Layout",
      description: "Switch the diff between side by side and inline.",
    }),
    await commands.register(NEXT_HUNK, go("next"), {
      title: "Go to Next Difference",
      description: "Move the cursor to the next changed region of the diff, wrapping at the end.",
    }),
    await commands.register(PREVIOUS_HUNK, go("previous"), {
      title: "Go to Previous Difference",
      description: "Move the cursor to the previous changed region of the diff, wrapping at the start.",
    }),
  ];
  keymaps.bind("alt-f5", NEXT_HUNK, { view: "editor", context: DIFF_CONTEXT });
  keymaps.bind("alt-shift-f5", PREVIOUS_HUNK, { view: "editor", context: DIFF_CONTEXT });

  // A tab closes without a notice, so each change of the tabs looks for diffs that went.
  const observation = await workbench.observe((event) => {
    if (event.type !== "presentationsChanged" && event.type !== "windowClosed") return;
    for (const diffView of [...open.values()]) {
      void shown(diffView).then((visible) => (visible ? undefined : release(diffView))).catch(() => {});
    }
  });

  return {
    async [Symbol.asyncDispose]() {
      await observation[Symbol.asyncDispose]();
      for (const command of navigation) await command[Symbol.asyncDispose]();
      await served[Symbol.asyncDispose]();
      for (const diffView of [...open.values()]) await release(diffView);
      await registration[Symbol.asyncDispose]();
    },
  };
}
