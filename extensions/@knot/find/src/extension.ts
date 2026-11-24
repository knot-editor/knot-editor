// Find: a bar above the focused editor that highlights every match of its
// text as it is typed and selects the current one. Enter and Shift+Enter in
// the bar, or the next and previous commands anywhere, step between matches.
// Replace adds a second bar beneath it that replaces the current match or
// every match, natively, in one undo step.
import { commands, icons, keymaps, platform, storage, windows, workbench } from "knot";
import type { Buffer, ByteRange, CommandContext, Disposable, Editor, EditorHandle, ViewNodeTarget, ViewTarget } from "knot";
import { endOf, highlights, invalidPattern, matchBefore, matchFrom, rebase, sameRange, summary } from "./search.js";
import type { Options } from "./search.js";

export const FIND = "editor.find";
export const REPLACE = "editor.replace";
export const REPLACE_CURRENT = "editor.replace-current";
export const REPLACE_ALL = "editor.replace-all";
export const NEXT = "editor.find-next";
export const PREVIOUS = "editor.find-previous";
export const CLOSE = "editor.find-close";
export const TOGGLE_CASE = "editor.find-toggle-case";
export const TOGGLE_WORD = "editor.find-toggle-word";
export const TOGGLE_REGEX = "editor.find-toggle-regex";
export const TOGGLE_SCOPE = "editor.find-toggle-selection";

const BAR = "find";
const FIELD = "query";
const REPLACE_BAR = "replace";
const REPLACEMENT = "replacement";
export const MATCH_COLOR = "editor.findMatch";
export const CURRENT_COLOR = "editor.findMatchCurrent";
export const SCOPE_COLOR = "editor.findScope";

// The last query, replacement and options, shared by every editor.
let query = "";
let replacement = "";
const options: Options = { caseSensitive: false, wholeWord: false, regex: false };

/** The storage record holding the query and replacement histories. */
export const HISTORY = "history";
const HISTORY_LIMIT = 100;

// Earlier queries and replacements, newest first, recalled with Up and Down in
// the bar's fields.
let queries: string[] = [];
let replacements: string[] = [];
let saving = Promise.resolve();

/** Move a non-empty `entry` to the front of `list`, within the cap, and save. */
function remember(list: "queries" | "replacements", entry: string) {
  if (entry === "") return;
  const next = [entry, ...(list === "queries" ? queries : replacements).filter((other) => other !== entry)];
  next.length = Math.min(next.length, HISTORY_LIMIT);
  if (list === "queries") queries = next;
  else replacements = next;
  const value = { queries, replacements };
  saving = saving.then(() => storage.write(HISTORY, value)).catch(() => {});
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry) => typeof entry === "string").slice(0, HISTORY_LIMIT) : [];
}

/** The bar of one editor. */
type Session = {
  editor: EditorHandle;
  buffer: Buffer;
  open: boolean;
  /** Whether the Replace bar is open beneath the find bar. */
  replaceOpen: boolean;
  /** The find bar's message, shown again when the bars are reopened. */
  message: string | null;
  /** The Replace bar's message: what the last Replace All did. */
  replaced: string | null;
  /** The range Find in Selection limits the search to, kept current through
   * edits; `null` searches the whole buffer. */
  scope: ByteRange | null;
  /** The buffer revision `scope` is current at. */
  scopeRevision: number;
  /** Edits that arrived while a refresh ran, joined into one more refresh. */
  stale: boolean;
  refreshing: boolean;
  disposables: Disposable[];
};

// Pending until the session follows its editor, so overlapping commands
// share one.
const sessions = new Map<ViewTarget, Promise<Session>>();

function isInvalid(error: unknown) {
  return (error as { code?: string }).code === "InvalidTarget";
}

async function sessionOf(context: CommandContext, editor: Editor, buffer: Buffer): Promise<Session> {
  const info = await windows.get(context.target.windowTarget);
  const target = info?.currentTarget;
  const view = info?.panes.flatMap((pane) => pane.tabs).find((tab) => tab.target === target)?.root;
  if (target == null || view?.kind !== "editor") commands.unavailable();
  let session = sessions.get(target);
  if (session === undefined) {
    session = follow(target, view.target, buffer);
    sessions.set(target, session);
    session.catch(() => sessions.delete(target));
  }
  return session;
}

async function follow(target: ViewTarget, view: ViewNodeTarget, buffer: Buffer): Promise<Session> {
  const handle = await workbench.editor(view);
  const session: Session = {
    editor: handle,
    buffer,
    open: false,
    replaceOpen: false,
    message: null,
    replaced: null,
    scope: null,
    scopeRevision: 0,
    stale: false,
    refreshing: false,
    disposables: [handle],
  };
  handle.onDidClose(() => void sessions.delete(target));
  session.disposables.push(
    await handle.onDidEditBar(async (edit) => {
      if (!session.open) return;
      session.replaced = null;
      if (edit.bar === REPLACE_BAR) {
        replacement = edit.fields[REPLACEMENT] ?? "";
        return showReplaceBar(session);
      }
      if (edit.bar !== BAR) return;
      query = edit.fields[FIELD] ?? "";
      await select(session, await handle.selection());
    }),
    await buffer.onDidChange(async (change) => {
      if (!session.open) return;
      if (session.scope !== null && change.revision > session.scopeRevision) {
        session.scope = rebase(session.scope, change.edits);
        session.scopeRevision = change.revision;
      }
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
  let found;
  try {
    found = await highlights(session.buffer, query, options, session.scope);
  } catch (error) {
    // A newer edit shortened the buffer under the scope; its own notification
    // searches again.
    if (session.scope !== null && (error as { code?: string }).code === "InvalidRange") return;
    const invalid = invalidPattern(error);
    if (invalid === null) throw error;
    await clearHighlights(session);
    return showBars(session, invalid);
  }
  const selected = selectedRange(selection);
  const current = found.matches.find((match) => sameRange(match, selected)) ?? null;
  try {
    // Later decorations paint above earlier ones, so the scope goes first.
    const scope =
      session.scope === null
        ? []
        : [{ range: session.scope, decoration: { color: SCOPE_COLOR, style: "background" as const } }];
    await session.editor.setDecorations(
      [
        ...scope,
        ...found.matches.map((range) => ({
          range,
          decoration: {
            color: current !== null && sameRange(range, current) ? CURRENT_COLOR : MATCH_COLOR,
            style: "background" as const,
          },
        })),
      ],
      { ifRevision: found.revision },
    );
  } catch (error) {
    // An edit raced the search; its own notification searches again.
    if ((error as { code?: string }).code !== "RevisionConflict") throw error;
  }
  await showBars(session, summary(query, found, current));
}

async function clearHighlights(session: Session) {
  try {
    // An empty search reports the current revision without reading text.
    const revision = await revisionOf(session.buffer);
    await session.editor.setDecorations([], { ifRevision: revision });
  } catch (error) {
    if (!isInvalid(error) && (error as { code?: string }).code !== "RevisionConflict") throw error;
  }
}

/** The buffer's current revision, which an empty search reports without
 * reading text. */
async function revisionOf(buffer: Buffer): Promise<number> {
  return (await buffer.find("", { limit: 1 })).revision;
}

async function showBars(session: Session, message: string | null) {
  session.message = message;
  await showBar(session, message);
  if (session.replaceOpen) await showReplaceBar(session);
}

async function showReplaceBar(session: Session) {
  try {
    await session.editor.openBar(REPLACE_BAR, {
      fields: [{ id: REPLACEMENT, placeholder: "Replace", history: replacements }],
      message: session.replaced,
      buttons: [
        { id: "replace", icon: "find.replace", tooltip: "Replace (Enter)", command: { name: REPLACE_CURRENT } },
        {
          id: "replace-all",
          icon: "find.replace-all",
          tooltip: `Replace All (${platform === "macos" ? "⌘" : "Ctrl+"}Enter)`,
          command: { name: REPLACE_ALL },
        },
      ],
    });
  } catch (error) {
    if (!isInvalid(error)) throw error;
  }
}

async function showBar(session: Session, message: string | null) {
  try {
    await session.editor.openBar(BAR, {
      fields: [{ id: FIELD, placeholder: "Find", history: queries }],
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
        {
          id: "regex",
          icon: "find.regex",
          tooltip: `Use Regular Expression (${shortcut("alt-r")})`,
          active: options.regex,
          command: { name: TOGGLE_REGEX },
        },
        {
          id: "scope",
          icon: "find.selection",
          tooltip: `Find in Selection (${shortcut("alt-s")})`,
          active: session.scope !== null,
          command: { name: TOGGLE_SCOPE },
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
    let match: ByteRange | null = null;
    try {
      match = await matchFrom(session.buffer, query, options, selectedRange(selection).startByteOffset, session.scope);
    } catch (error) {
      // `refresh` reports an invalid pattern.
      if (invalidPattern(error) === null) throw error;
    }
    if (match !== null) return reveal(session, match);
  }
  await refresh(session, selection);
}

/** Select `match`, centered, and refresh. */
async function reveal(session: Session, match: ByteRange) {
  const selection = { anchor: match.startByteOffset, caret: match.endByteOffset };
  await session.editor.setSelection(selection, { reveal: "center" });
  await refresh(session, selection);
}

async function open(context: CommandContext, withReplace: boolean) {
  const { editor, buffer } = context;
  if (editor === null || buffer === null) commands.unavailable();
  const session = await sessionOf(context, editor, buffer);
  const selection = await editor.selection();
  let text = query;
  if (selection.anchor !== selection.caret) {
    const selected = (await buffer.snapshot(selectedRange(selection))).text;
    if (!selected.includes("\n")) text = selected;
    else if (!session.open) await setScope(session, selectedRange(selection));
  }
  query = text;
  session.open = true;
  await showBar(session, null);
  if (withReplace && !session.replaceOpen) {
    session.replaceOpen = true;
    await showReplaceBar(session);
    await session.editor.focusBarField(REPLACE_BAR, REPLACEMENT, { text: replacement });
  }
  await session.editor.focusBarField(BAR, FIELD, { text });
  await select(session, selection);
}

async function setScope(session: Session, scope: ByteRange | null) {
  session.scope = scope;
  if (scope !== null) session.scopeRevision = await revisionOf(session.buffer);
}

async function step(context: CommandContext, forward: boolean) {
  const { editor, buffer } = context;
  if (editor === null || buffer === null) commands.unavailable();
  if (query === "") return open(context, false);
  remember("queries", query);
  const selection = selectedRange(await editor.selection());
  const scope = (await openSession(context))?.scope ?? null;
  let match: ByteRange | null;
  try {
    match = forward
      ? await matchFrom(buffer, query, options, selection.endByteOffset, scope)
      : await matchBefore(buffer, query, options, selection.startByteOffset, scope);
  } catch (error) {
    if (invalidPattern(error) !== null) return;
    throw error;
  }
  if (match === null) {
    const session = await openSession(context);
    if (session !== undefined) await showBars(session, session.message);
    return;
  }
  await editor.setSelection({ anchor: match.startByteOffset, caret: match.endByteOffset }, { reveal: "center" });
  const session = await openSession(context);
  if (session !== undefined) await refresh(session, { anchor: match.startByteOffset, caret: match.endByteOffset });
}

/** Flip an option and search again from the current selection. */
async function toggle(context: CommandContext, flip: () => void) {
  const session = await openSession(context);
  if (session === undefined) return;
  flip();
  session.replaced = null;
  await select(session, await session.editor.selection());
}

/** Limit the search to the selection, or to the caret's line when nothing is
 * selected, or lift the limit. */
async function toggleScope(context: CommandContext) {
  let session = await openSession(context);
  if (session === undefined) {
    // Opening turns the scope on for a multi-line selection; any other
    // selection is scoped below.
    await open(context, false);
    session = await openSession(context);
    if (session === undefined || session.scope !== null) return;
  }
  session.replaced = null;
  const selection = await session.editor.selection();
  let scope: ByteRange | null = null;
  if (session.scope === null) {
    scope =
      selection.anchor === selection.caret
        ? await session.buffer.lineRange(await session.buffer.lineAt(selection.caret))
        : selectedRange(selection);
  }
  await setScope(session, scope);
  await select(session, selection);
}

/** Replace the selection when it is a match, then select the next match; with
 * any other selection, only select the next match. */
async function replaceCurrent(context: CommandContext) {
  const session = await openSession(context);
  if (session === undefined || query === "") return;
  remember("queries", query);
  remember("replacements", replacement);
  const { buffer, editor } = session;
  const selection = await editor.selection();
  const selected = selectedRange(selection);
  session.replaced = null;
  try {
    const found = await buffer.find(query, { ...options, range: selected, limit: 1 });
    if (found.matches.length === 0 || !sameRange(found.matches[0]!, selected)) return select(session, selection);
    const before = await endOf(buffer);
    const { revision } = await buffer.replace(query, replacement, { ...options, range: selected, limit: 1 });
    // Only the match changed, so the buffer's growth is the replacement's.
    const growth = (await endOf(buffer)) - before;
    growScope(session, growth, revision);
    const match = await matchFrom(buffer, query, options, selected.endByteOffset + growth, session.scope);
    if (match !== null) return reveal(session, match);
  } catch (error) {
    if (invalidPattern(error) === null) throw error;
  }
  await refresh(session, await editor.selection());
}

/** Replace every match, as one undo step, and report how many. */
async function replaceAll(context: CommandContext) {
  const session = await openSession(context);
  if (session === undefined || query === "") return;
  remember("queries", query);
  remember("replacements", replacement);
  try {
    const before = await endOf(session.buffer);
    const scoped = session.scope === null ? options : { ...options, range: session.scope };
    const { count, more, revision } = await session.buffer.replace(query, replacement, scoped);
    growScope(session, (await endOf(session.buffer)) - before, revision);
    session.replaced = `Replaced ${count}${more ? "+" : ""}`;
  } catch (error) {
    if (invalidPattern(error) === null) throw error;
    session.replaced = null;
  }
  await refresh(session, await session.editor.selection());
}

/** Account for replacements inside the scope, which changed its length by
 * `growth`, without waiting for their change notification, which then finds
 * the scope current. A notification that came first already did. */
function growScope(session: Session, growth: number, revision: number) {
  if (session.scope === null || session.scopeRevision >= revision) return;
  session.scope = { ...session.scope, endByteOffset: session.scope.endByteOffset + growth };
  session.scopeRevision = revision;
}

/** The session of the command's editor while its bar is open. A session that
 * failed to follow its editor never opened. */
async function openSession(context: CommandContext): Promise<Session | undefined> {
  const target = (await windows.get(context.target.windowTarget))?.currentTarget;
  const session = target == null ? undefined : await sessions.get(target)?.catch(() => undefined);
  return session?.open ? session : undefined;
}

async function close(context: CommandContext) {
  const session = await openSession(context);
  if (session === undefined) return;
  session.open = false;
  session.replaceOpen = false;
  session.replaced = null;
  session.scope = null;
  try {
    await session.editor.closeBar(BAR);
    await session.editor.closeBar(REPLACE_BAR);
    await clearHighlights(session);
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
  replacement = "";
  options.caseSensitive = false;
  options.wholeWord = false;
  options.regex = false;
  const saved = await storage.read(HISTORY);
  const record = saved.found ? (saved.value as { queries?: unknown; replacements?: unknown } | null) : null;
  queries = strings(record?.queries);
  replacements = strings(record?.replacements);
  await icons.define("find.case", { lucide: "case-sensitive" });
  await icons.define("find.word", { lucide: "whole-word" });
  await icons.define("find.regex", { lucide: "regex" });
  await icons.define("find.selection", { lucide: "scan-text" });
  await icons.define("find.replace", { lucide: "replace" });
  await icons.define("find.replace-all", { lucide: "replace-all" });
  await icons.define("find.previous", { lucide: "arrow-up" });
  await icons.define("find.next", { lucide: "arrow-down" });
  await icons.define("find.close", { lucide: "x" });
  await commands.register(FIND, (context) => open(context, false), {
    title: "Find…",
    description: "Search the current file.",
  });
  await commands.register(REPLACE, (context) => open(context, true), {
    title: "Replace…",
    description: "Search the current file and replace matches.",
  });
  await commands.register(REPLACE_CURRENT, replaceCurrent, { title: "Replace", palette: false });
  await commands.register(REPLACE_ALL, replaceAll, { title: "Replace All", palette: false });
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
  await commands.register(TOGGLE_REGEX, (context) => toggle(context, () => (options.regex = !options.regex)), {
    title: "Find: Toggle Regular Expression",
    palette: false,
  });
  await commands.register(TOGGLE_SCOPE, toggleScope, {
    title: "Find in Selection",
    description: "Limit the search to the selection, or the current line.",
  });
  const mod = platform === "macos" ? "cmd" : "ctrl";
  for (const context of [BAR, REPLACE_BAR]) {
    keymaps.bind(`${mod}-alt-c`, TOGGLE_CASE, { view: "bar", context });
    keymaps.bind(`${mod}-alt-w`, TOGGLE_WORD, { view: "bar", context });
    keymaps.bind(`${mod}-alt-r`, TOGGLE_REGEX, { view: "bar", context });
    keymaps.bind(`${mod}-alt-s`, TOGGLE_SCOPE, { view: "bar", context });
    keymaps.bind("escape", CLOSE, { view: "bar", context });
  }
  keymaps.bind("enter", NEXT, { view: "bar", context: BAR });
  keymaps.bind("shift-enter", PREVIOUS, { view: "bar", context: BAR });
  keymaps.bind("enter", REPLACE_CURRENT, { view: "bar", context: REPLACE_BAR });
  keymaps.bind(`${mod}-enter`, REPLACE_ALL, { view: "bar", context: REPLACE_BAR });
}
