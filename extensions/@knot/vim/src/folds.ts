// Folds in one followed editor. The editor owns fold state: collapsed folds
// and the regions that could fold, as line ranges from first line to last.
// Here Vim's "closed fold" is a collapsed one, whose first line is where the
// caret stays, and its keys open, close, and move between them.
import { commands } from "knot";
import type { FoldRange } from "knot";
import type { FoldAction } from "./keys.js";
import type { BufferLines } from "./lines.js";
import { lineStart } from "./motions.js";
import type { Moved, State } from "./normal.js";

/** The outermost collapsed fold that has `line`, hidden or its first. */
export function closedAt(folds: readonly FoldRange[], line: number): FoldRange | null {
  return folds.find((fold) => fold.start <= line && line <= fold.end) ?? null;
}

/** The line that shows for `line`: the first line of the collapsed fold
 * hiding it, or itself. */
export function shownLine(folds: readonly FoldRange[], line: number): number {
  return closedAt(folds, line)?.start ?? line;
}

/** The last line of the closed fold `line` is in, or `line`. */
export function lastLine(folds: readonly FoldRange[], line: number): number {
  return closedAt(folds, line)?.end ?? line;
}

/** The line `n` shown lines down from `line`, a closed fold counting as one;
 * stops at the last line. */
export function linesDown(folds: readonly FoldRange[], line: number, n: number, total: number): number {
  let at = line;
  for (let step = 0; step < n; step++) {
    const next = lastLine(folds, at) + 1;
    if (next >= total) break;
    at = next;
  }
  return at;
}

/** The line `n` shown lines up from `line`; stops at the first line. */
export function linesUp(folds: readonly FoldRange[], line: number, n: number): number {
  let at = shownLine(folds, line);
  for (let step = 0; step < n && at > 0; step++) at = shownLine(folds, at - 1);
  return at;
}

/** A motion that landed in a closed fold lands on its first line instead. */
export async function intoShownLine(state: State, lines: BufferLines, moved: Moved): Promise<Moved> {
  const folds = await state.handle.folds();
  const line = shownLine(folds, moved.to.line);
  if (line === moved.to.line) return moved;
  const to = moved.type === "linewise" ? await lineStart(lines, line) : { line, col: moved.to.col };
  return { ...moved, to };
}

/** The line a fold motion reaches from `line`, or null when there is none:
 * `zj` and `zk` go to the next fold's start and the previous fold's end, `[z`
 * and `]z` to the start and end of the open fold around the line, repeated
 * `n` times. */
export async function foldMotion(
  state: State,
  line: number,
  motion: "fold-next" | "fold-previous" | "fold-start" | "fold-end",
  n: number,
): Promise<number | null> {
  const folds = await state.handle.folds();
  const regions = await state.handle.foldRegions();
  const hidden = (at: number) => folds.some((fold) => fold.start < at && at <= fold.end);
  let at = shownLine(folds, line);
  let moved = false;
  for (let step = 0; step < n; step++) {
    let found: number | undefined;
    const here = closedAt(folds, at);
    if (motion === "fold-next") {
      const after = lastLine(folds, at);
      found = regions.find((region) => region.start > after && !hidden(region.start))?.start;
    } else if (motion === "fold-previous") {
      found = regions.filter((region) => shownLine(folds, region.end) < at).reduce<number | undefined>(
        (best, region) => best === undefined || region.end > best ? region.end : best,
        undefined,
      );
    } else {
      const around = openAround(regions, folds, at);
      if (motion === "fold-start") {
        found = around.filter((region) => region.start < at).at(-1)?.start;
      } else {
        found = around.filter((region) => region.end > (here?.end ?? at)).at(-1)?.end;
      }
    }
    if (found === undefined) break;
    at = shownLine(folds, found);
    moved = true;
  }
  return moved ? at : null;
}

/** The regions around `line` that are not collapsed, outermost first. When
 * `line` is in a collapsed fold, those are the regions around that fold. */
function openAround(regions: readonly FoldRange[], folds: readonly FoldRange[], line: number): FoldRange[] {
  const closed = closedAt(folds, line);
  return regions.filter((region) =>
    region.start <= line && line <= region.end
    && !folds.some((fold) => fold.start === region.start && fold.end === region.end)
    && (closed === null || (region.start <= closed.start && closed.end <= region.end))
  );
}

/** The caret's line. */
async function caretLine(state: State): Promise<number> {
  return state.buffer.lineAt((await state.handle.selection()).caret);
}

/** Runs the fold keys other than `zf` over a selection, which visual mode
 * handles. */
export async function runFold(state: State, action: FoldAction, count: number | null) {
  const { handle } = state;
  const line = await caretLine(state);
  const closes = action === "close" || action === "close-all" || action === "toggle" || action === "toggle-all"
    || action === "close-every" || action === "create-lines";
  if (closes) await enableFolds(state);
  switch (action) {
    case "open":
    case "delete":
      await open(state, line, count ?? 1);
      break;
    case "open-all":
    case "delete-all":
    case "reveal":
      await openAll(state, line);
      break;
    case "close":
      await close(state, line, count ?? 1);
      break;
    case "close-all":
      await closeAll(state, line);
      break;
    case "toggle":
      if (closedAt(await handle.folds(), line) !== null) await open(state, line, count ?? 1);
      else await close(state, line, count ?? 1);
      break;
    case "toggle-all":
      if (closedAt(await handle.folds(), line) !== null) await openAll(state, line);
      else await closeAll(state, line);
      break;
    case "open-every":
    case "delete-every":
      await commands.invoke("editor.unfold-all");
      break;
    case "close-every":
      await commands.invoke("editor.fold-all");
      break;
    case "disable":
      await disableFolds(state);
      break;
    case "enable":
      await enableFolds(state);
      break;
    case "toggle-enabled":
      if (state.foldsOff === null) await disableFolds(state);
      else await enableFolds(state);
      break;
    case "create-lines": {
      const last = Math.min((await state.buffer.lineCount()) - 1, line + (count ?? 1) - 1);
      await createFold(state, line, last);
      break;
    }
    case "create-selection":
      break;
  }
}

/** Collapses the lines `first` through `last` as a fold, unless it is one
 * line. */
export async function createFold(state: State, first: number, last: number) {
  await enableFolds(state);
  if (last > first) await state.handle.fold({ start: first, end: last });
}

/** `zo`: expands the outermost collapsed fold at the line, `levels` times. */
async function open(state: State, line: number, levels: number) {
  for (let level = 0; level < levels; level++) {
    const closed = closedAt(await state.handle.folds(), line);
    if (closed === null) return;
    await state.handle.unfold(closed);
  }
}

/** `zO`: expands every collapsed fold that has the line. */
async function openAll(state: State, line: number) {
  for (const fold of await state.handle.folds()) {
    if (fold.start <= line && line <= fold.end) await state.handle.unfold(fold);
  }
}

/** `zc`: collapses the innermost fold around what shows of the line, then the
 * one around that, `levels` times. */
async function close(state: State, line: number, levels: number) {
  for (let level = 0; level < levels; level++) {
    const folds = await state.handle.folds();
    const around = openAround(await state.handle.foldRegions(), folds, line);
    const region = around.at(-1);
    if (region === undefined) return;
    await state.handle.fold(region);
  }
}

/** `zC`: collapses every fold around the line. */
async function closeAll(state: State, line: number) {
  for (const region of await state.handle.foldRegions()) {
    if (region.start <= line && line <= region.end) await state.handle.fold(region);
  }
}

/** `zn`: expands every fold, remembering which were collapsed. */
async function disableFolds(state: State) {
  if (state.foldsOff !== null) return;
  state.foldsOff = await state.handle.folds();
  await commands.invoke("editor.unfold-all");
}

/** `zN`: collapses the folds `zn` expanded. Other fold keys also turn
 * folds back on, as in Vim. */
export async function enableFolds(state: State) {
  const folds = state.foldsOff;
  if (folds === null) return;
  state.foldsOff = null;
  for (const fold of folds) await state.handle.fold(fold);
}
