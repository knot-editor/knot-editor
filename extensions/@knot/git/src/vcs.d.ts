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
    statusChanged: FnSchema<{ backend: string; repository: string }, void>;
    log: FnSchema<{ backend: string; repository: string; command: string; output: string; failed: boolean }, void>;
  }>;

  /** The contract of the backend `id`, served by that backend. */
  export function backendContract(id: string): Contract<{
    repositories: FnSchema<string, readonly string[]>;
    repositoryOf: FnSchema<string, string | null>;
    baseline: FnSchema<{ repository: string; file: string }, string | null>;
    status: FnSchema<
      string,
      {
        readonly files: readonly {
          file: string;
          status: "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";
        }[];
        readonly groups: readonly {
          id: string;
          label: string;
          files: readonly string[];
          actions: readonly {
            label: string;
            icon?: string;
            command: string;
            on: readonly ("group" | "folder" | "file")[];
            menuOnly?: boolean;
            showOnHover?: boolean;
          }[];
        }[];
      }
    >;
    compare: FnSchema<
      { repository: string; group: string; file: string },
      {
        original: { label: string; text: string };
        modified: "document" | { label: string; text: string };
      } | null
    >;
    rollback: FnSchema<{ repository: string; path: string }, void>;
    commitOptions: FnSchema<string, readonly { id: string; label: string; tooltip?: string }[]>;
    commitMessage: FnSchema<{ repository: string; options: readonly string[] }, string | null>;
    commit: FnSchema<
      { repository: string; message: string; options: readonly string[] },
      { summary: string } | null
    >;
    ref: FnSchema<string, { label: string; tooltip?: string } | null>;
  }>;
}
