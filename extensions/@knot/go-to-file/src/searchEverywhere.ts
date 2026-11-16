// The one picker: files, open buffers, and the palette's commands under All,
// Buffers, Files, and Actions tabs. Go to File opens it on Files. Choosing a
// buffer or file shows it, choosing an action runs it in the window the
// picker opened from, and `:N` (alone, or after the text) goes to line N.
import { commands, completion, documents, history, windows, workbench, workspaces } from "knot";
import type { Buffer, CommandContext, CommandInfo, DocumentInfo, PickerRow, ViewTarget } from "knot";
import { fileRows } from "./fileRows.js";
import { ACTION_HISTORY, BUFFER_HISTORY, documentOf } from "./recent.js";

/** The commands that open this picker; running one from inside it would nest it. */
const PICKER_COMMANDS = new Set(["workspace.search-everywhere", "workspace.go-to-file", "workbench.show-command-palette"]);

/** Actions the All tab shows, so buffers and files stay visible beside them. */
const ALL_TAB_ACTIONS = 8;

const TABS = [
  { id: "all", label: "All" },
  { id: "buffers", label: "Buffers" },
  { id: "files", label: "Files" },
  { id: "actions", label: "Actions" },
];

type Kind = "buffer" | "file" | "line" | "command";

/** A row ID: what the row is, the line to go to after opening it, and its target. */
function rowId(kind: Kind, line: number | null, target: string): string {
  return `${kind}:${line ?? ""}:${target}`;
}

function parseRowId(id: string): { kind: Kind; line: number | null; target: string } {
  const [, kind, line, target] = /^(\w+):(\d*):([^]*)$/.exec(id)!;
  return { kind: kind as Kind, line: line === "" ? null : Number(line), target: target! };
}

/**
 * A query split into the text to match and the line after its last `:`. A
 * query of only `:` and digits is a jump: it lists nothing but its line row.
 */
export function parseQuery(query: string): { text: string; line: number | null; jump: boolean } {
  const found = /^([^]*):(\d*)$/.exec(query);
  if (found === null) return { text: query, line: null, jump: false };
  return { text: found[1]!, line: found[2] === "" ? null : Number(found[2]), jump: found[1] === "" };
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
      id: rowId("command", null, command.name),
      label: command.title,
      description: command.description,
      hint: command.key,
      labelMatches: index < ordered.length ? ranges : [],
    });
  }
  return rows;
}

/**
 * The open documents matching `text` on their title, most recently active
 * first and the one the window shows last.
 */
async function bufferRows(text: string, current: DocumentInfo | null, line: number | null): Promise<PickerRow[]> {
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
    id: rowId("buffer", line, document.id),
    label: document.title,
    icon: { file: document.title },
    description: document.resourceUri,
    labelMatches: ranges,
  }));
}

/** Puts the caret at the start of the 1-based `line`, clamped to the text. */
async function goToLine(target: ViewTarget, buffer: Buffer | null, line: number) {
  if (buffer === null) return;
  const index = Math.min(Math.max(line, 1), await buffer.lineCount()) - 1;
  const { startByteOffset } = await buffer.lineRange(index);
  const editor = await workbench.editor(target);
  await editor.setSelection({ anchor: startByteOffset, caret: startByteOffset }, { reveal: "center" });
}

/** The tab ID `argument` names, if it names one. */
function requestedTab(argument: CommandContext["arguments"]): string | undefined {
  const tab = (argument as { readonly tab?: unknown } | null)?.tab;
  return TABS.find((candidate) => candidate.id === tab)?.id;
}

export async function searchEverywhere({ target, arguments: argument }: CommandContext) {
  const workspace = await workspaces.get(target.windowTarget);
  const roots = workspace?.roots ?? [];
  const uris = roots.map((root) => root.uri);
  const info = await windows.get(target.windowTarget);
  const shown = info?.statusTarget ?? null;
  const editorTab = info?.panes.flatMap((pane) => pane.tabs).find((tab) => tab.target === shown);
  const current = shown === null ? null : await documentOf(shown);
  const available = (await commands.list(info?.currentTarget ?? shown ?? undefined)).filter((command) => !PICKER_COMMANDS.has(command.name));

  const rows = async (query: string, tab: string | null): Promise<PickerRow[]> => {
    const { text, line, jump } = parseQuery(query);
    const scope = tab ?? "all";
    if (jump) {
      if (editorTab === undefined) return [];
      const label = line === null ? "Go to line…" : `Go to line ${line}`;
      return [{ id: rowId("line", line, ""), label, labelMatches: [] }];
    }
    const buffers = scope === "all" || scope === "buffers" ? await bufferRows(text, current, line) : [];
    let files: PickerRow[] = [];
    if (scope === "all" || scope === "files") {
      files = await fileRows(target.windowTarget, uris, roots.length > 1, text);
      if (scope === "all") {
        const open = new Set((await documents.list()).map((document) => document.resourceUri));
        files = files.filter((row) => !open.has(row.id));
      }
      files = files.map((row) => ({ ...row, id: rowId("file", line, row.id) }));
    }
    let actions: PickerRow[] = [];
    // An empty All tab lists buffers and recent files; actions join once
    // something is typed, and a line has no use for them.
    if ((scope === "all" || scope === "actions") && line === null) {
      actions = await actionRows(available, text);
      if (scope === "all") actions = text.trim() === "" ? [] : actions.slice(0, ALL_TAB_ACTIONS);
    }
    return [...buffers, ...files, ...actions];
  };

  const initialTab = requestedTab(argument);
  const result = await workbench.showPicker({
    title: "Search Everywhere",
    tabs: TABS,
    ...(initialTab === undefined ? {} : { initialTab }),
    onQuery: rows,
  });
  if (result.kind !== "selected") return;
  const { kind, line, target: chosen } = parseRowId(result.id);
  if (kind === "command") {
    await history.record(ACTION_HISTORY, chosen);
    await commands.invoke(chosen);
  } else if (kind === "line") {
    if (shown !== null && line !== null) await goToLine(shown, editorTab?.buffer ?? null, line);
  } else {
    const opened =
      kind === "buffer" ? await documents.present(chosen as DocumentInfo["id"]) : await documents.open(chosen);
    if (line !== null) await goToLine(opened.presentation.target, opened.presentation.buffer, line);
  }
}
