// Searching: the `/` and `?` prompts, `n`, `N`, `*`, `#`, `g*`, and `g#` as
// motions over the buffer's native `find`, which matches the translated
// pattern as a regular expression. The last pattern is the `/` register; the
// direction is shared by every editor.
import { settings, workbench } from "knot";
import type { ByteRange, EditorViewState } from "knot";
import type { SearchMotion } from "./keys.js";
import type { BufferLines } from "./lines.js";
import type { Position } from "./motions.js";
import type { Moved, State } from "./normal.js";
import { caseSensitive, type Syntax, translate, wordPattern } from "./pattern.js";
import { nextColumn } from "./text.js";

export const REGEX = "vim.regex";
export const IGNORE_CASE = "vim.ignoreCase";
export const SMART_CASE = "vim.smartCase";

declare module "knot" {
  interface Settings {
    /** The syntax of search patterns and replacements: "vim" or "rust". */
    "vim.regex": string;
    /** Whether searches ignore case. */
    "vim.ignoreCase": boolean;
    /** Whether a pattern with an uppercase letter is case-sensitive despite `vim.ignoreCase`. */
    "vim.smartCase": boolean;
  }
}

export async function defineSearchSettings() {
  await settings.define({
    key: REGEX,
    type: "string",
    default: "vim",
    description: "The syntax of search patterns and replacements: \"vim\" or \"rust\", as Find's regex mode reads them.",
  });
  await settings.define({
    key: IGNORE_CASE,
    type: "boolean",
    default: true,
    description: "Ignore case when searching.",
  });
  await settings.define({
    key: SMART_CASE,
    type: "boolean",
    default: true,
    description: "Search case-sensitively when the pattern has an uppercase letter, even while ignoring case.",
  });
}

/** The most matches one backward step reads. */
const READ_LIMIT = 100_000;

/** The pattern syntax and case options in effect for the editor. */
export async function searchSettings(state: State) {
  const scope = { document: state.document, language: state.language, view: "editor" } as const;
  return {
    syntax: ((await settings.get(REGEX, scope)) === "rust" ? "rust" : "vim") as Syntax,
    ignoreCase: (await settings.get(IGNORE_CASE, scope)) !== false,
    smartCase: (await settings.get(SMART_CASE, scope)) !== false,
  };
}

/** A search runs a pattern as the engine reads it, or fails with a message. */
export type Compiled = { readonly query: string; readonly caseSensitive: boolean } | { readonly error: string };

export async function compile(state: State, typed: string, smart: boolean): Promise<Compiled> {
  const { syntax, ignoreCase, smartCase } = await searchSettings(state);
  const translated = translate(typed, syntax);
  if (!translated.ok) return { error: translated.message };
  return {
    query: translated.source,
    caseSensitive: caseSensitive(typed, translated.forcedCase, { ignoreCase, smartCase }, smart),
  };
}

/** The engine's message when `error` is an invalid pattern, else null. */
export function invalidPattern(error: unknown): string | null {
  const host = error as { code?: string; message?: string; details?: { message?: string } };
  return host.code === "InvalidQuery" ? (host.details?.message ?? host.message ?? "Invalid pattern") : null;
}

/** Runs a search motion from `from`; null, with the reason in `state.message`
 * when there is one, when nothing is found. */
export async function searchMotion(
  state: State,
  lines: BufferLines,
  from: Position,
  motion: SearchMotion,
  count: number | null,
): Promise<Moved | null> {
  const { registers } = state.shared;
  let typed: string;
  let forward: boolean;
  let smart = true;
  switch (motion.search) {
    case "prompt": {
      forward = motion.forward;
      const entered = await prompt(state, lines, from, forward);
      if (entered === null) return null;
      if (entered === "") {
        const last = registers.get("/")?.text;
        if (last === undefined) return fail(state, "E35: No previous regular expression");
        typed = last;
        smart = state.shared.search.smart;
      } else {
        typed = entered;
        registers.remember("search", typed);
      }
      break;
    }
    case "repeat": {
      const last = registers.get("/")?.text;
      if (last === undefined) return fail(state, "E35: No previous regular expression");
      typed = last;
      forward = state.shared.search.forward !== motion.reverse;
      smart = state.shared.search.smart;
      // The direction `n` and `N` take stays the one the last search set.
      return find(state, lines, from, typed, forward, smart, count, false);
    }
    case "word": {
      const found = await wordAt(lines, from);
      if (found === null) return fail(state, "E348: No string under cursor");
      forward = motion.forward;
      smart = false;
      typed = wordPattern(found.word, motion.whole, (await searchSettings(state)).syntax);
      registers.remember("search", typed);
      // Both directions start at the word's start, so they leave the word.
      from = { line: from.line, col: found.start };
      break;
    }
  }
  return find(state, lines, from, typed, forward, smart, count, true);
}

/** Steps `count` matches of `typed` from `from`. `remember` makes the pattern
 * and direction the last search, as every search but `n` and `N` does. */
async function find(
  state: State,
  lines: BufferLines,
  from: Position,
  typed: string,
  forward: boolean,
  smart: boolean,
  count: number | null,
  remember: boolean,
): Promise<Moved | null> {
  if (remember) {
    state.shared.registers.set("/", { text: typed, linewise: false });
    state.shared.saveRegisters();
    state.shared.search = { forward, smart };
  }
  await state.shared.highlights.show();
  const compiled = await compile(state, typed, smart);
  if ("error" in compiled) return fail(state, compiled.error);
  const options = { regex: true, caseSensitive: compiled.caseSensitive } as const;
  let position = from;
  let wrapped = false;
  try {
    for (let step = 0; step < (count ?? 1); step++) {
      const next = forward
        ? await matchAfter(state, lines, compiled.query, options, position)
        : await matchBefore(state, lines, compiled.query, options, position);
      if (next === null) return fail(state, `E486: Pattern not found: ${typed}`);
      wrapped ||= next.wrapped;
      position = await lines.position(next.range.startByteOffset);
    }
  } catch (error) {
    const message = invalidPattern(error);
    if (message === null) throw error;
    return fail(state, message);
  }
  if (wrapped) {
    state.message = forward ? "search hit BOTTOM, continuing at TOP" : "search hit TOP, continuing at BOTTOM";
  }
  return { to: position, type: "exclusive", reveal: "center", jump: true };
}

type Options = { readonly regex: true; readonly caseSensitive: boolean };
type Match = { readonly range: ByteRange; readonly wrapped: boolean };

async function bufferEnd(state: State): Promise<number> {
  const last = await state.buffer.lineRange((await state.buffer.lineCount()) - 1);
  return last.endByteOffset;
}

/** The first match starting after `from`, wrapping to the buffer's first. */
async function matchAfter(
  state: State,
  lines: BufferLines,
  query: string,
  options: Options,
  from: Position,
): Promise<Match | null> {
  const end = await bufferEnd(state);
  const text = await lines.text(from.line);
  const here = await lines.offset(from);
  // A match may not start at the caret, and one that starts a character
  // later may overlap what starts at the caret.
  const start = from.col < text.length ? await lines.offset({ line: from.line, col: nextColumn(text, from.col) }) : here + 1;
  if (start <= end) {
    const after = await state.buffer.find(query, {
      ...options, range: { startByteOffset: start, endByteOffset: end }, limit: 1,
    });
    if (after.matches.length > 0) return { range: after.matches[0]!, wrapped: false };
  }
  const first = await state.buffer.find(query, {
    ...options, range: { startByteOffset: 0, endByteOffset: end }, limit: 1,
  });
  return first.matches.length === 0 ? null : { range: first.matches[0]!, wrapped: true };
}

/** The last match starting before `from`, wrapping to the buffer's last. */
async function matchBefore(
  state: State,
  lines: BufferLines,
  query: string,
  options: Options,
  from: Position,
): Promise<Match | null> {
  const at = await lines.offset(from);
  const all = await state.buffer.find(query, {
    ...options, range: { startByteOffset: 0, endByteOffset: await bufferEnd(state) }, limit: READ_LIMIT,
  });
  const before = all.matches.filter((match) => match.startByteOffset < at);
  if (before.length > 0) return { range: before.at(-1)!, wrapped: false };
  return all.matches.length === 0 ? null : { range: all.matches.at(-1)!, wrapped: true };
}

/** Sets the message a failed search leaves and fails the motion. */
function fail(state: State, message: string): null {
  state.message = message;
  return null;
}

/** Asks for a pattern, highlighting its matches and showing the first from
 * `from` as it is typed; null when dismissed, else the text, which is empty
 * to search for the last pattern again. The editor's view is as it was
 * afterwards. */
async function prompt(state: State, lines: BufferLines, from: Position, forward: boolean): Promise<string | null> {
  const view = await state.handle.viewState();
  const input = await workbench.showInput({
    title: forward ? "Search Forward" : "Search Backward",
    prompt: forward ? "/" : "?",
    history: state.shared.registers.entries("search"),
    onChange: (text) => preview(state, lines, from, forward, text, view),
  });
  await state.handle.restoreViewState(view);
  if (!state.mode.startsWith("visual")) state.caret = (await state.handle.selection()).caret;
  if (input.kind === "dismissed") {
    await state.shared.highlights.refresh(state);
    return null;
  }
  return input.value;
}

/** Highlights the matches of the pattern typed so far and moves the caret to
 * the first one from `from`, or back to the view it started from when there
 * is none. Returns the message of an invalid pattern. A visual selection
 * keeps its end. */
async function preview(
  state: State,
  lines: BufferLines,
  from: Position,
  forward: boolean,
  text: string,
  view: EditorViewState,
): Promise<string | null> {
  const { highlights } = state.shared;
  if (text === "") {
    await state.handle.restoreViewState(view);
    await highlights.refresh(state);
    return null;
  }
  const compiled = await compile(state, text, true);
  if ("error" in compiled) {
    await highlights.draw(state, null, null);
    return compiled.error;
  }
  const options = { regex: true, caseSensitive: compiled.caseSensitive } as const;
  let match: Match | null;
  try {
    match = forward
      ? await matchAfter(state, lines, compiled.query, options, from)
      : await matchBefore(state, lines, compiled.query, options, from);
  } catch (error) {
    const message = invalidPattern(error);
    if (message === null) throw error;
    await highlights.draw(state, null, null);
    return message;
  }
  const message = await highlights.draw(state, compiled, match?.range ?? null);
  if (match === null) {
    await state.handle.restoreViewState(view);
  } else if (!state.mode.startsWith("visual")) {
    state.caret = match.range.startByteOffset;
    await state.handle.setSelection(
      { anchor: state.caret, caret: state.caret },
      { reveal: "nearest" },
    );
  }
  return message;
}

/** The word `*` and `#` search for: the keyword at the caret or the next one
 * on its line, else the non-blank text from the caret on; `start` is its
 * first column. */
async function wordAt(
  lines: BufferLines,
  from: Position,
): Promise<{ readonly word: string; readonly start: number } | null> {
  const text = await lines.text(from.line);
  const keyword = /[\p{L}\p{N}_]/u;
  const characters = [...text].map((char, index, all) => ({
    char,
    col: all.slice(0, index).join("").length,
  }));
  const here = characters.findIndex(({ col, char }) => from.col >= col && from.col < col + char.length);
  const wordIn = (isWord: (char: string) => boolean): { word: string; start: number } | null => {
    let first = here;
    while (first < characters.length && !isWord(characters[first]!.char)) first++;
    if (first >= characters.length) return null;
    let low = first;
    while (low > 0 && isWord(characters[low - 1]!.char)) low--;
    let high = first;
    while (high < characters.length && isWord(characters[high]!.char)) high++;
    return {
      word: characters.slice(low, high).map(({ char }) => char).join(""),
      start: characters[low]!.col,
    };
  };
  if (here === -1) return null;
  return wordIn((char) => keyword.test(char)) ?? wordIn((char) => !/\s/.test(char));
}
