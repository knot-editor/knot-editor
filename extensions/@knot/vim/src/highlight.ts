// The search highlights: every match of the last pattern in each followed
// editor, kept current through edits, from a search until `:noh`. An editor's
// prompt draws its own incremental matches over them while it is open.
import type { ByteRange, Disposable } from "knot";
import type { State } from "./normal.js";
import { compile, invalidPattern } from "./search.js";

export const MATCH_COLOR = "editor.findMatch";
export const CURRENT_COLOR = "editor.findMatchCurrent";
/** The most matches highlighted, as Find's limit. */
const LIMIT = 1000;

/** A pattern as the engine reads it. */
export type Query = { readonly query: string; readonly caseSensitive: boolean };

type Entry = { stale: boolean; refreshing: boolean; subscription: Disposable | null };

export class Highlights {
  private active = false;
  private readonly entries = new Map<State, Entry>();

  /** Keeps `state`'s editor current from now on. */
  async follow(state: State) {
    const entry: Entry = { stale: false, refreshing: false, subscription: null };
    this.entries.set(state, entry);
    entry.subscription = await state.buffer.onDidChange(() => this.changed(state, entry));
    if (this.active) await this.refresh(state);
  }

  unfollow(state: State) {
    this.entries.get(state)?.subscription?.[Symbol.asyncDispose]().catch(() => {});
    this.entries.delete(state);
  }

  /** Shows the last pattern's matches in every editor, until `hide`. */
  async show() {
    this.active = true;
    for (const state of [...this.entries.keys()]) await this.refresh(state);
  }

  /** Clears every editor's highlights, as `:noh` does. */
  async hide() {
    this.active = false;
    for (const state of [...this.entries.keys()]) await this.draw(state, null, null);
  }

  /** Draws `query`'s matches in `state`'s editor, with `current` emphasized;
   * no query clears them. Returns the engine's message for an invalid pattern. */
  async draw(state: State, query: Query | null, current: ByteRange | null): Promise<string | null> {
    try {
      if (query === null) return await this.clear(state);
      let found;
      try {
        found = await state.buffer.find(query.query, { regex: true, caseSensitive: query.caseSensitive, limit: LIMIT });
      } catch (error) {
        const message = invalidPattern(error);
        if (message === null) throw error;
        await this.clear(state);
        return message;
      }
      await state.handle.setDecorations(
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
      // An edit raced the search, which its own notification repeats, or the
      // editor closed.
      const code = (error as { code?: string }).code;
      if (code !== "RevisionConflict" && code !== "InvalidTarget" && code !== "BufferClosed") throw error;
    }
    return null;
  }

  /** Shows the committed pattern's matches again, or none while hidden. */
  async refresh(state: State) {
    const typed = state.shared.registers.get("/")?.text;
    if (!this.active || typed === undefined) return void (await this.draw(state, null, null));
    const compiled = await compile(state, typed, state.shared.search.smart);
    await this.draw(state, "error" in compiled ? null : compiled, null);
  }

  private async clear(state: State): Promise<null> {
    // An empty search reports the current revision without reading text.
    const { revision } = await state.buffer.find("", { limit: 1 });
    await state.handle.setDecorations([], { ifRevision: revision });
    return null;
  }

  /** Refreshes after an edit, joining edits that arrive meanwhile into one more refresh. */
  private async changed(state: State, entry: Entry) {
    if (!this.active) return;
    entry.stale = true;
    if (entry.refreshing) return;
    entry.refreshing = true;
    try {
      while (entry.stale && this.active) {
        entry.stale = false;
        await this.refresh(state);
      }
    } finally {
      entry.refreshing = false;
    }
  }
}

function sameRange(a: ByteRange, b: ByteRange): boolean {
  return a.startByteOffset === b.startByteOffset && a.endByteOffset === b.endByteOffset;
}
