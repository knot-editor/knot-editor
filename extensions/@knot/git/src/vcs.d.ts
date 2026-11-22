// The part of `@knot/vcs`'s contracts the Git backend uses. A bundled package
// compiles alone, so it cannot import the provider's TypeScript source for its
// types; this declaration restates the shapes the provider's schemas check on
// every call (see vcs/src/contract.ts).
declare module "@knot/vcs" {
  import type { Contract, FnSchema } from "knot/contract";

  export const vcs: Contract<{
    register: FnSchema<string, void>;
    unregister: FnSchema<string, void>;
    changed: FnSchema<{ backend: string; repository: string }, void>;
  }>;

  /** The contract of the backend `id`, served by that backend. */
  export function backendContract(id: string): Contract<{
    repositories: FnSchema<string, readonly string[]>;
    repositoryOf: FnSchema<string, string | null>;
    baseline: FnSchema<{ repository: string; file: string }, string | null>;
  }>;
}
