// What `@knot/diff` serves to packages that show a comparison. A caller
// imports this module from "@knot/diff" and lists `@knot/diff` in its
// manifest's `requires`. This module is pure schema code, so it imports only
// "knot/contract".
import { defineContract, s } from "knot/contract";

/** A read-only text, such as a version control baseline. */
const snapshot = s.object({ label: s.string(), text: s.string(), language: s.optional(s.string()) });

/** An open document, which the tab edits live. */
const document = s.object({ label: s.string(), document: s.string() });

/** Served by `@knot/diff`. */
export const diffs = defineContract("diff", {
  /**
   * Opens a tab comparing the original with the modified side and returns its
   * ID. A document side is editable and a snapshot side read-only. `line` is
   * the 0-based line of the modified side the caret starts at. With `preview`
   * the tab is the pane's preview, which the next preview replaces; the
   * replacement closes the diff as closing its tab does.
   */
  open: s.fn(
    s.object({
      original: s.union([snapshot, document]),
      modified: s.union([snapshot, document]),
      line: s.optional(s.int()),
      preview: s.optional(s.boolean()),
    }),
    s.string(),
  ),
  /**
   * Replaces the text of a snapshot side, for example with a new baseline. A
   * document side or a closed diff rejects it.
   */
  setText: s.fn(s.object({ id: s.string(), side: s.enum(["original", "modified"]), text: s.string() }), s.void()),
  /** Makes a preview diff a regular tab; a regular one stays as it is. */
  keep: s.fn(s.string(), s.void()),
  /** Shows the diff's tab and focuses it. */
  activate: s.fn(s.string(), s.void()),
  /** Closes the diff's tab; one already closed is ignored. */
  close: s.fn(s.string(), s.void()),
});
