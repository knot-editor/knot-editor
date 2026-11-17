// The three lists of Search Everywhere's tabs for one window: files, open
// buffers, and actions. The picker and the `workspace.files`,
// `workspace.buffers`, and `workspace.actions` providers build their rows
// here, with plain row IDs: a file URI, a document ID, a command name.
import { commands, completion, documents, history, windows, workspaces } from "knot";
import type { Buffer, CommandInfo, DocumentInfo, PickerRow, ViewTarget, WindowTarget } from "knot";
import { fileRows } from "./fileRows.js";
import { ACTION_HISTORY, BUFFER_HISTORY, documentOf } from "./recent.js";

/** The commands that open the picker; running one from inside it would nest it. */
const PICKER_COMMANDS = new Set(["workspace.search-everywhere", "workspace.go-to-file", "workbench.show-command-palette"]);

/** What a window offers the lists. */
export interface WindowLists {
  readonly window: WindowTarget;
  readonly roots: readonly string[];
  /** The last focused editor of the window, if any. */
  readonly shown: ViewTarget | null;
  readonly current: DocumentInfo | null;
  readonly available: readonly CommandInfo[];
  /** Whether the window has an editor tab, so a line can be jumped to. */
  readonly hasEditor: boolean;
  readonly editorBuffer: Buffer | null;
}

export async function windowLists(window: WindowTarget): Promise<WindowLists> {
  const workspace = await workspaces.get(window);
  const info = await windows.get(window);
  const shown = info?.statusTarget ?? null;
  const editorTab = info?.panes.flatMap((pane) => pane.tabs).find((tab) => tab.target === shown);
  return {
    window,
    roots: (workspace?.roots ?? []).map((root) => root.uri),
    shown,
    current: shown === null ? null : await documentOf(shown),
    available: (await commands.list(info?.currentTarget ?? shown ?? undefined)).filter(
      (command) => !PICKER_COMMANDS.has(command.name),
    ),
    hasEditor: editorTab !== undefined,
    editorBuffer: editorTab?.buffer ?? null,
  };
}

/** The recent files first, then the other files of the roots matching `text`. */
export function filesRows(lists: WindowLists, text: string): Promise<PickerRow[]> {
  return fileRows(lists.window, lists.roots, lists.roots.length > 1, text);
}

/**
 * The open documents matching `text` on their title, most recently active
 * first and the one the window shows last.
 */
export async function buffersRows(lists: WindowLists, text: string): Promise<PickerRow[]> {
  const { current } = lists;
  const others = (await documents.list()).filter((document) => document.id !== current?.id);
  const ordered = await history.order(BUFFER_HISTORY, others, (document) => document.id);
  const all = current === null ? ordered : [...ordered, current];
  const matches =
    text.trim() === ""
      ? all.map((document) => ({ document, ranges: [] as readonly (readonly [number, number])[] }))
      : (await completion.match(text, all.map((document) => document.title))).map((match) => ({
          document: all[match.index]!,
          ranges: match.ranges,
        }));
  return matches.map(({ document, ranges }) => ({
    id: document.id,
    label: document.title,
    icon: { file: document.title },
    description: document.resourceUri,
    labelMatches: ranges,
  }));
}

/**
 * The actions matching `query` on their title or name, best first, recently
 * run ones first among equals. Only a title match has ranges to highlight.
 */
export async function actionRows(all: readonly CommandInfo[], query: string): Promise<PickerRow[]> {
  const ordered = await history.order(ACTION_HISTORY, all, (command) => command.name);
  // Titles, then names, as one candidate list: the first hit of a command is
  // its best, and the shared matcher ranks both the way it ranks everything.
  const found = await completion.match(query, [
    ...ordered.map((command) => command.title),
    ...ordered.map((command) => command.name),
  ]);
  const rows: PickerRow[] = [];
  const seen = new Set<number>();
  for (const { index, ranges } of found) {
    const at = index % ordered.length;
    if (seen.has(at)) continue;
    seen.add(at);
    const command = ordered[at]!;
    rows.push({
      id: command.name,
      label: command.title,
      description: command.description,
      hint: command.key,
      labelMatches: index < ordered.length ? ranges : [],
    });
  }
  return rows;
}

/** A provider of one list: the rows for a query in the caller's window. */
export function listProvider(rows: (lists: WindowLists, query: string) => Promise<PickerRow[]>) {
  return {
    async rows(query: string, context: { readonly window: WindowTarget | null }): Promise<PickerRow[]> {
      if (context.window === null) return [];
      return rows(await windowLists(context.window), query);
    },
  };
}
