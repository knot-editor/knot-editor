// What `@knot/go-to-file` offers other packages: the rows of the picker's
// lists for a query, in the window of the command that calls. A package
// imports `workspace` from "@knot/go-to-file" and lists `@knot/go-to-file`
// in its manifest's `requires`. This module is pure schema code, so it
// imports only "knot/contract".
import { defineContract, s } from "knot/contract";

const ranges = s.array(s.tuple([s.int(), s.int()]));
const text = s.nullable(s.string());

/** One row of a list; a file's row has its URI as ID, a buffer's its document ID,
 * an action's its command name. */
export const row = s.object({
  id: s.string(),
  label: s.string(),
  description: s.optional(text),
  detail: s.optional(text),
  hint: s.optional(text),
  icon: s.optional(s.nullable(s.union([s.string(), s.object({ file: s.string() }), s.object({ folder: s.string() })]))),
  labelMatches: s.optional(s.nullable(ranges)),
  descriptionMatches: s.optional(s.nullable(ranges)),
});

const list = s.fn(s.string(), s.array(row));

/** The lists of Search Everywhere's tabs. */
export const workspace = defineContract("workspace", {
  /** The files of the window's roots matching the query, recently active ones first. */
  files: list,
  /** The open documents matching the query, most recently active first. */
  buffers: list,
  /** The commands available in the window matching the query. */
  actions: list,
});
