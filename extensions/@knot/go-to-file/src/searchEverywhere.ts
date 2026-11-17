// The one picker: files, open buffers, and the palette's commands under All,
// Buffers, Files, and Actions tabs. Go to File opens it on Files. Choosing a
// buffer or file shows it, choosing an action runs it in the window the
// picker opened from, and `:N` (alone, or after the text) goes to line N.
import { commands, documents, history, workbench } from "knot";
import type { Buffer, CommandContext, DocumentInfo, PickerRow, ViewTarget } from "knot";
import { actionRows, buffersRows, filesRows, windowLists } from "./lists.js";
import { ACTION_HISTORY } from "./recent.js";

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
  const lists = await windowLists(target.windowTarget);
  const { shown, hasEditor } = lists;

  const rows = async (query: string, tab: string | null): Promise<PickerRow[]> => {
    const { text, line, jump } = parseQuery(query);
    const scope = tab ?? "all";
    if (jump) {
      if (!hasEditor) return [];
      const label = line === null ? "Go to line…" : `Go to line ${line}`;
      return [{ id: rowId("line", line, ""), label, labelMatches: [] }];
    }
    const wrap = (kind: Kind) => (row: PickerRow) => ({ ...row, id: rowId(kind, kind === "command" ? null : line, row.id) });
    const buffers = scope === "all" || scope === "buffers" ? (await buffersRows(lists, text)).map(wrap("buffer")) : [];
    let files: PickerRow[] = [];
    if (scope === "all" || scope === "files") {
      files = await filesRows(lists, text);
      if (scope === "all") {
        const open = new Set((await documents.list()).map((document) => document.resourceUri));
        files = files.filter((row) => !open.has(row.id));
      }
      files = files.map(wrap("file"));
    }
    let actions: PickerRow[] = [];
    // An empty All tab lists buffers and recent files; actions join once
    // something is typed, and a line has no use for them.
    if ((scope === "all" || scope === "actions") && line === null) {
      actions = await actionRows(lists.available, text);
      if (scope === "all") actions = text.trim() === "" ? [] : actions.slice(0, ALL_TAB_ACTIONS);
      actions = actions.map(wrap("command"));
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
    if (shown !== null && line !== null) await goToLine(shown, lists.editorBuffer, line);
  } else {
    const opened =
      kind === "buffer" ? await documents.present(chosen as DocumentInfo["id"]) : await documents.open(chosen);
    if (line !== null) await goToLine(opened.presentation.target, opened.presentation.buffer, line);
  }
}
