// API version 1 contract example: the module a package's manifest names in
// `contract`. It is pure schema code, importing only "knot/contract" and its
// own relative modules, so a consumer evaluates it without running the
// provider.
import { defineContract, s } from "knot/contract";

export const greeter = defineContract("greeter", {
  /** A greeting for `name`; `shout` upper-cases it. */
  greet: s.fn(
    s.object({ name: s.string(), shout: s.optional(s.boolean()) }),
    s.object({ greeting: s.string(), window: s.boolean() }),
  ),
  /** Forgets every greeting; no argument, no result. */
  reset: s.fn(s.void(), s.void()),
});
