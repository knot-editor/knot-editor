// Status bar items for the editor a window describes: the cursor position and
// selection size, the indentation, and the language. Each window follows its
// own `statusTarget` and sets its items as that window's values, so two
// windows show their own editors.
import { commands, documents, languages, settings, workbench, windows } from "knot";
import type {
  Buffer, CommandContext, Disposable, DocumentId, EditorHandle, StatusItem, StatusItemOptions,
  ViewTarget, WindowInfo, WindowTarget,
} from "knot";
import {
  COUNT_LIMIT, columnAt, indentationText, languageText, positionText, selectionSize,
} from "./text.js";

export const INDENTATION_COMMAND = "editor.change-indentation";
const GO_TO_LINE_COMMAND = "editor.go-to-line";
const SET_LANGUAGE_COMMAND = "editor.set-language-mode";

/** The editor one window's items describe. */
type Follow = {
  target: ViewTarget | null;
  document: DocumentId | null;
  buffer: Buffer | null;
  handle: EditorHandle | null;
  selection: Disposable | null;
};

/** The items, each shown only where a window sets its text. */
const ITEMS = {
  position: { tooltip: "Go to Line/Column", command: GO_TO_LINE_COMMAND, priority: 30 },
  indentation: { tooltip: "Select Indentation", command: INDENTATION_COMMAND, priority: 20 },
  language: { tooltip: "Select Language Mode", command: SET_LANGUAGE_COMMAND, priority: 10 },
} as const;
type ItemName = keyof typeof ITEMS;

const options = (name: ItemName, text: string): StatusItemOptions => ({
  text,
  tooltip: ITEMS[name].tooltip,
  command: { name: ITEMS[name].command },
  side: "right",
  priority: ITEMS[name].priority,
});

async function caretText(handle: EditorHandle, buffer: Buffer): Promise<string> {
  const { anchor, caret } = await handle.selection();
  const line = await buffer.lineAt(caret);
  const range = await buffer.lineRange(line);
  const text = await buffer.snapshot(range);
  const column = columnAt(text, Math.min(caret, range.endByteOffset) - range.startByteOffset);
  if (anchor === caret) return positionText(line + 1, column, null);
  const bytes = Math.abs(caret - anchor);
  const selected =
    bytes <= COUNT_LIMIT
      ? await buffer.snapshot({
          startByteOffset: Math.min(anchor, caret),
          endByteOffset: Math.max(anchor, caret),
        })
      : null;
  return positionText(line + 1, column, selectionSize(selected, bytes));
}

export async function start() {
  const items = {} as Record<ItemName, StatusItem>;
  for (const name of Object.keys(ITEMS) as ItemName[]) {
    items[name] = await workbench.registerStatusItem(name, options(name, ""));
  }
  const follows = new Map<WindowTarget, Follow>();

  // Sets one item's text for a window, or clears the window's value so the
  // empty shared one hides the item.
  const show = async (name: ItemName, window: WindowTarget, text: string | null) => {
    if (text === null) await items[name].update(null, { window });
    else await items[name].update(options(name, text), { window });
  };

  const refreshPosition = async (window: WindowTarget) => {
    const follow = follows.get(window);
    if (follow?.handle == null || follow.buffer === null) {
      await show("position", window, null);
      return;
    }
    await show("position", window, await caretText(follow.handle, follow.buffer));
  };

  const refreshDocument = async (window: WindowTarget) => {
    const follow = follows.get(window);
    const document = follow?.document == null ? null : await documents.get(follow.document);
    if (document === null || document === undefined) {
      await show("indentation", window, null);
      await show("language", window, null);
      return;
    }
    await show("indentation", window, indentationText(document.indentation));
    await show("language", window, languageText(document.language, await languages.list()));
  };

  const release = async (follow: Follow) => {
    await follow.selection?.dispose();
    await follow.handle?.dispose();
  };

  // Follows the window's status target, moving the selection listener to a
  // new editor when the target changes.
  const follow = async (info: WindowInfo) => {
    const window = info.target;
    let current = follows.get(window);
    if (current === undefined) {
      current = { target: null, document: null, buffer: null, handle: null, selection: null };
      follows.set(window, current);
    }
    if (current.target !== info.statusTarget) {
      await release(current);
      const tab = info.panes
        .flatMap((pane) => pane.tabs)
        .find((candidate) => candidate.target === info.statusTarget);
      current.target = info.statusTarget;
      current.document = tab?.document ?? null;
      current.buffer = tab?.buffer ?? null;
      current.handle = null;
      current.selection = null;
      if (info.statusTarget !== null) {
        current.handle = await workbench.editor(info.statusTarget);
        current.selection = await current.handle.onDidChangeSelection(() => refreshPosition(window));
      }
    }
    await refreshPosition(window);
    await refreshDocument(window);
  };

  const observation = await workbench.observe(async (event) => {
    switch (event.type) {
      case "windowOpened":
      case "presentationsChanged":
        await follow(event.window);
        break;
      case "windowClosed": {
        const closed = follows.get(event.window);
        follows.delete(event.window);
        if (closed !== undefined) await release(closed);
        break;
      }
      case "documentChanged":
        for (const [window, current] of follows) {
          if (current.document === event.document.id) await refreshDocument(window);
        }
        break;
    }
  });
  for (const window of observation.snapshot.windows) await follow(window);

  await commands.register(INDENTATION_COMMAND, changeIndentation, {
    title: "Change Indentation…",
    description: "Indent with spaces or tabs, change the tab size, or convert the document's indentation.",
  });
}

/** The document the window's status bar describes. */
async function statusDocument(context: CommandContext): Promise<DocumentId> {
  const window = await windows.get(context.target.windowTarget);
  const tab = window?.panes
    .flatMap((pane) => pane.tabs)
    .find((candidate) => candidate.target === window.statusTarget);
  if (tab?.document == null) commands.unavailable();
  return tab.document;
}

/** The indentation actions, in VS Code's order and wording without its
 * separate "Indent Using" entries, which Convert Indentation already covers by
 * also making the style the document's own. All but the size run native
 * commands on the focused editor. */
const ACTIONS = [
  { id: "size", label: "Change Tab Display Size" },
  { id: "detect", label: "Detect Indentation from Content", command: "editor.detect-indentation" },
  { id: "to-spaces", label: "Convert Indentation to Spaces", command: "editor.convert-indentation-to-spaces" },
  { id: "to-tabs", label: "Convert Indentation to Tabs", command: "editor.convert-indentation-to-tabs" },
  { id: "reindent", label: "Reindent Buffer", command: "editor.reindent-buffer" },
  { id: "reindent-selected", label: "Reindent Selected Lines", command: "editor.reindent-selected-lines" },
] as const;

async function changeIndentation(context: CommandContext) {
  const document = await statusDocument(context);
  const action = await workbench.showPicker({
    title: "Select Action",
    items: ACTIONS.map(({ id, label }) => ({ id, label })),
  });
  if (action.kind !== "selected") return;
  const chosen = ACTIONS.find(({ id }) => id === action.id)!;
  if ("command" in chosen) {
    await commands.invoke(chosen.command);
    return;
  }
  const current = await settings.get("editor.tabSize", { document });
  const size = await workbench.showPicker({
    title: "Select Tab Size for Current File",
    items: [1, 2, 3, 4, 5, 6, 7, 8].map((n) => ({
      id: String(n),
      label: String(n),
      ...(n === current ? { description: "Configured Tab Size" } : {}),
    })),
  });
  if (size.kind === "selected") await settings.set("editor.tabSize", Number(size.id), { document });
}
