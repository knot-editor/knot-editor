// Vim's jumplist, without host calls. Entries are opaque to it, and the
// caller says which line each one is on now, or null once its text is gone.
// As in Vim, a jump appends the position it left at the end, wherever
// `ctrl-o` had stepped back to, and an earlier entry on the same line is
// dropped, so the list never holds one line twice.

/** How many entries the list keeps; recording more drops the oldest. */
export const LIMIT = 100;

export type LineOf<T> = (entry: T) => number | null;

export class JumpList<T> {
  readonly entries: T[] = [];
  /** The entry `ctrl-o` and `ctrl-i` count from; the list's length while no
   * step back is in progress. */
  index = 0;

  /**
   * Records a jump from `entry`, on `line`, and ends any step back. Returns
   * the entries no longer held: ones whose text is gone, the earlier one on
   * the same line, and the oldest past the limit.
   */
  push(entry: T, line: number, lineOf: LineOf<T>): T[] {
    const removed = this.remove((other) => {
      const at = lineOf(other);
      return at === null || at === line;
    });
    this.entries.push(entry);
    while (this.entries.length > LIMIT) removed.push(this.entries.shift()!);
    this.index = this.entries.length;
    return removed;
  }

  /**
   * Steps `count` entries back (negative) or forward, as `ctrl-o` and
   * `ctrl-i` do, and returns the entry to jump to, or null when the list
   * holds none that far. Stepping back from the end first records the
   * current position, `current` on `line`, so `ctrl-i` comes back to it.
   * `removed` lists entries no longer held, which may include `current`.
   */
  step(
    count: number,
    current: T,
    line: number,
    lineOf: LineOf<T>,
  ): { readonly target: T | null; readonly removed: T[] } {
    const removed = this.remove((entry) => lineOf(entry) === null);
    const outside = () => this.index + count < 0 || this.index + count >= this.entries.length;
    if (count === 0 || outside()) return { target: null, removed: [...removed, current] };
    if (this.index === this.entries.length) {
      removed.push(...this.push(current, line, lineOf));
      this.index--;
      if (this.index + count < 0) return { target: null, removed };
    } else {
      removed.push(current);
    }
    this.index += count;
    return { target: this.entries[this.index]!, removed };
  }

  /** Removes the entries `drop` matches, keeping `index` on the same entry. */
  private remove(drop: (entry: T) => boolean): T[] {
    const removed: T[] = [];
    for (let at = this.entries.length - 1; at >= 0; at--) {
      if (!drop(this.entries[at]!)) continue;
      removed.push(...this.entries.splice(at, 1));
      if (at < this.index) this.index--;
    }
    return removed;
  }
}
