// The part of `@knot/go-to-file`'s `workspace` contract Vim calls. A bundled
// package compiles alone, so it cannot import the provider's TypeScript
// source for its types; this declaration restates the shape the provider's
// schema checks on every call (see go-to-file/src/contract.ts).
declare module "@knot/go-to-file" {
  import type { IconRef } from "knot";
  import type { Contract, FnSchema } from "knot/contract";

  /** A row of the file list; its ID is the file's URI. */
  export interface FileRow {
    readonly id: string;
    readonly label: string;
    readonly description?: string | null;
    readonly detail?: string | null;
    readonly hint?: string | null;
    readonly icon?: IconRef | null;
    readonly labelMatches?: readonly (readonly [number, number])[] | null;
    readonly descriptionMatches?: readonly (readonly [number, number])[] | null;
  }

  export const workspace: Contract<{ files: FnSchema<string, readonly FileRow[]> }>;
}
