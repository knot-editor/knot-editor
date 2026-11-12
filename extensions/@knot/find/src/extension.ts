// Find: a bar above the focused editor that highlights every match of its
// text as it is typed and selects the current one. Enter and Shift+Enter in
// the bar, or the next and previous commands anywhere, step between matches.
import { commands, icons, keymaps, platform, windows, workbench } from "knot";
import type { Buffer, ByteRange, CommandContext, Disposable, Editor, EditorHandle } from "knot";
import { highlights, matchBefore, matchFrom, sameRange, summary } from "./search.js";
import type { Options } from "./search.js";

export const FIND = "editor.find";
export const NEXT = "editor.find-next";
export const PREVIOUS = "editor.find-previous";
export const CLOSE = "editor.find-close";
export const TOGGLE_CASE = "editor.find-toggle-case";
export const TOGGLE_WORD = "editor.find-toggle-word";

const BAR = "find";
const FIELD = "query";
export const MATCH_COLOR = "editor.findMatch";
export const CURRENT_COLOR = "editor.findMatchCurrent";

// The last query and options, shared by every editor.
let query = "";
const options: Options = { caseSensitive: false, wholeWord: false };

/** The bar of one editor. */
type Session = {
  editor: EditorHandle;
  buffer: Buffer;
  open: boolean;
  /** Edits that arrived while a refresh ran, joined into one more refresh. */
  stale: boolean;
  refreshing: boolean;
  disposables: Disposable[];
};

const sessions = new Map<unknown, Session>();

function isInvalid(error: unknown) {
  return (error as { code?: string }).code === "InvalidTarget";
}

async function sessionOf(context: CommandContext, editor: Editor, buffer: Buffer): Promise<Session> {
  const target = (await windows.get(context.target.windowTarget))?.currentTarget;
  if (target == null) commands.unavailable();
  const known = sessions.get(target);
  if (known !== undefined) return known;
  const handle = await workbench.editor(target);
  const session: Session = {
    editor: handle,
    buffer,
    open: false,
    stale: false,
    refreshing: false,
    disposables: [handle],
  };
  sessions.set(target, session);
  handle.onDidClose(() => void sessions.delete(target));
  session.disposables.push(
    await handle.onDidEditBar(async (edit) => {
      if (!session.open || edit.bar !== BAR) return;
      query = edit.fields[FIELD] ?? "";
      await select(session, await handle.selection());
    }),
    await buffer.onDidChange(async () => {
      if (!session.open) return;
      session.stale = true;
      if (session.refreshing) return;
      session.refreshing = true;
      try {
        while (session.stale && session.open) {
          session.stale = false;
          await refresh(session, await handle.selection());
        }
      } finally {
        session.refreshing = false;
      }
    }),
  );
  return session;
}

function selectedRange(selection: { anchor: number; caret: number }): ByteRange {
  return {
    startByteOffset: Math.min(selection.anchor, selection.caret),
    endByteOffset: Math.max(selection.anchor, selection.caret),
  };
}

/** Highlight every match and show the count, with `selection` as the current
 * match when it is one. */
async function refresh(session: Session, selection: { anchor: number; caret: number }) {
  const found = await highlights(session.buffer, query, options);
  const selected = selectedRange(selection);
  const current = found.matches.find((match) => sameRange(match, selected)) ?? null;
  try {
    await session.editor.setDecorations(
      found.matches.map((range) => ({
        range,
        decoration: {
          color: current !== null && sameRange(range, current) ? CURRENT_COLOR : MATCH_COLOR,
          style: "background" as const,
        },
      })),
      { ifRevision: found.revision },
    );
  } catch (error) {
    // An edit raced the search; its own notification searches again.
    if ((error as { code?: string }).code !== "RevisionConflict") throw error;
  }
  await showBar(session, summary(query, found, current));
}

async function showBar(session: Session, message: string | null) {
  try {
    await session.editor.openBar(BAR, {
      fields: [{ id: FIELD, placeholder: "Find" }],
      toggles: [
        {
          id: "case",
          icon: "find.case",
          tooltip: `Match Case (${shortcut("alt-c")})`,
          active: options.caseSensitive,
          command: { name: TOGGLE_CASE },
        },
        {
          id: "word",
          icon: "find.word",
          tooltip: `Whole Word (${shortcut("alt-w")})`,
          active: options.wholeWord,
          command: { name: TOGGLE_WORD },
        },
      ],
      message,
      buttons: [
        { id: "previous", icon: "find.previous", tooltip: "Previous Match", command: { name: PREVIOUS } },
        { id: "next", icon: "find.next", tooltip: "Next Match", command: { name: NEXT } },
        { id: "close", icon: "find.close", tooltip: "Close", command: { name: CLOSE } },
      ],
    });
  } catch (error) {
    if (!isInvalid(error)) throw error;
  }
}

/** Select the first match at or after the start of `selection`, and refresh. */
async function select(session: Session, selection: { anchor: number; caret: number }) {
  if (query !== "") {
    const match = await matchFrom(session.buffer, query, options, selectedRange(selection).startByteOffset);
    if (match !== null) {
      await session.editor.setSelection(
        { anchor: match.startByteOffset, caret: match.endByteOffset },
        { reveal: "center" },
      );
      return refresh(session, { anchor: match.startByteOffset, caret: match.endByteOffset });
    }
  }
  await refresh(session, selection);
}

async function open(context: CommandContext) {
  const { editor, buffer } = context;
  if (editor === null || buffer === null) commands.unavailable();
  const session = await sessionOf(context, editor, buffer);
  const selection = await editor.selection();
  let text = query;
  if (selection.anchor !== selection.caret) {
    const selected = (await buffer.snapshot(selectedRange(selection))).text;
    if (!selected.includes("\n")) text = selected;
  }
  query = text;
  session.open = true;
  await showBar(session, null);
  await session.editor.focusBarField(BAR, FIELD, { text });
  await select(session, selection);
}

async function step(context: CommandContext, forward: boolean) {
  const { editor, buffer } = context;
  if (editor === null || buffer === null) commands.unavailable();
  if (query === "") return open(context);
  const selection = selectedRange(await editor.selection());
  const match = forward
    ? await matchFrom(buffer, query, options, selection.endByteOffset)
    : await matchBefore(buffer, query, options, selection.startByteOffset);
  if (match === null) return;
  await editor.setSelection({ anchor: match.startByteOffset, caret: match.endByteOffset }, { reveal: "center" });
  const session = await openSession(context);
  if (session !== undefined) await refresh(session, { anchor: match.startByteOffset, caret: match.endByteOffset });
}

/** Flip an option and search again from the current selection. */
async function toggle(context: CommandContext, flip: () => void) {
  const session = await openSession(context);
  if (session === undefined) return;
  flip();
  await select(session, await session.editor.selection());
}

/** The session of the command's editor while its bar is open. */
async function openSession(context: CommandContext): Promise<Session | undefined> {
  const target = (await windows.get(context.target.windowTarget))?.currentTarget;
  const session = target == null ? undefined : sessions.get(target);
  return session?.open ? session : undefined;
}

async function close(context: CommandContext) {
  const session = await openSession(context);
  if (session === undefined) return;
  session.open = false;
  try {
    await session.editor.closeBar(BAR);
    // An empty search reports the current revision without reading text.
    const { revision } = await session.buffer.find("", { limit: 1 });
    await session.editor.setDecorations([], { ifRevision: revision });
  } catch (error) {
    if (!isInvalid(error)) throw error;
  }
}

/** A key as the tooltip shows it. */
function shortcut(key: string): string {
  return platform === "macos" ? `⌥⌘${key.slice(-1).toUpperCase()}` : `Ctrl+Alt+${key.slice(-1).toUpperCase()}`;
}

export async function start() {
  query = "";
  options.caseSensitive = false;
  options.wholeWord = false;
  await icons.define("find.case", { lucide: "case-sensitive" });
  await icons.define("find.word", { lucide: "whole-word" });
  await icons.define("find.previous", { lucide: "arrow-up" });
  await icons.define("find.next", { lucide: "arrow-down" });
  await icons.define("find.close", { lucide: "x" });
  await commands.register(FIND, open, {
    title: "Find…",
    description: "Search the current file.",
  });
  await commands.register(NEXT, (context) => step(context, true), { title: "Find Next" });
  await commands.register(PREVIOUS, (context) => step(context, false), { title: "Find Previous" });
  await commands.register(CLOSE, close, { title: "Close Find", palette: false });
  await commands.register(TOGGLE_CASE, (context) => toggle(context, () => (options.caseSensitive = !options.caseSensitive)), {
    title: "Find: Toggle Match Case",
    palette: false,
  });
  await commands.register(TOGGLE_WORD, (context) => toggle(context, () => (options.wholeWord = !options.wholeWord)), {
    title: "Find: Toggle Whole Word",
    palette: false,
  });
  const mod = platform === "macos" ? "cmd" : "ctrl";
  keymaps.bind(`${mod}-alt-c`, TOGGLE_CASE, { view: "bar", context: BAR });
  keymaps.bind(`${mod}-alt-w`, TOGGLE_WORD, { view: "bar", context: BAR });
  keymaps.bind("enter", NEXT, { view: "bar", context: BAR });
  keymaps.bind("shift-enter", PREVIOUS, { view: "bar", context: BAR });
  keymaps.bind("escape", CLOSE, { view: "bar", context: BAR });
}
