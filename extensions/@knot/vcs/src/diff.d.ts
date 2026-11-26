// The `diff` contract of `@knot/diff` that this package calls. A bundled
// package compiles alone, so it cannot import the provider's TypeScript
// source for its types; this declaration restates the shape the provider's
// schema checks on every call (see diff/src/contract.ts).
declare module "@knot/diff" {
  import type { Contract, FnSchema } from "knot/contract";

  type Snapshot = { readonly label: string; readonly text: string; readonly language?: string };
  type Open = { readonly label: string; readonly document: string };

  export const diffs: Contract<{
    open: FnSchema<
      { readonly original: Snapshot | Open; readonly modified: Snapshot | Open; readonly line?: number; readonly preview?: boolean },
      string
    >;
    setText: FnSchema<{ readonly id: string; readonly side: "original" | "modified"; readonly text: string }, void>;
    keep: FnSchema<string, void>;
    activate: FnSchema<string, void>;
    close: FnSchema<string, void>;
  }>;
}
