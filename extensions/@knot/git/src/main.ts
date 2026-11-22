import { backendContract, vcs } from "@knot/vcs";

// The skeleton: the backend registers and serves a contract that finds no
// repositories until later checkpoints teach it Git.
await backendContract("git").serve({
  repositories: () => [],
  repositoryOf: () => null,
  baseline: () => null,
});
await (await vcs.client()).register("git");
