import { icons, processes, workbench } from "knot";
import { backendContract, vcs } from "@knot/vcs";
import { registerActions, rollback } from "./actions.js";
import { baseline } from "./baseline.js";
import { commit, commitMessage, commitOptions } from "./commit.js";
import { compare } from "./compare.js";
import { isMissingProgram } from "./git.js";
import { ref } from "./ref.js";
import { hunkActions, registerStaging } from "./staging.js";
import { status } from "./status.js";
import { repositories, repositoryOf } from "./repositories.js";
import { watch } from "./watch.js";

/** Whether Git runs; without it there is nothing to serve, and the user is told once. */
async function available(): Promise<boolean> {
  try {
    await processes.run({ command: "git", args: ["--version"] });
    return true;
  } catch (error) {
    if (!isMissingProgram(error)) throw error;
    await workbench.showNotice({
      title: "Git",
      message: "Git was not found on the PATH, so the change gutter has no Git baselines.",
      severity: "warning",
    });
    return false;
  }
}

if (await available()) {
  await icons.define("git.stage", { lucide: "plus" });
  await icons.define("git.unstage", { lucide: "minus" });
  await icons.define("git.discard", { lucide: "undo-2" });
  await icons.define("git.rollback", { lucide: "rotate-ccw" });
  await icons.define("git.stageHunk", { lucide: "plus" });
  await icons.define("git.unstageHunk", { lucide: "check" });
  await registerActions();
  await registerStaging();
  await backendContract("git").serve({
    repositories: async (root) => {
      const found = await repositories(root).catch(() => []);
      await Promise.all(found.map(watch));
      return found;
    },
    repositoryOf: async (directory) => {
      const found = await repositoryOf(directory).catch(() => null);
      if (found !== null) await watch(found);
      return found;
    },
    baseline: ({ repository, file }) => baseline(repository, file).catch(() => null),
    status: (repository) => status(repository).catch(() => ({ files: [], groups: [] })),
    compare: ({ repository, group, file }) => compare(repository, group, file).catch(() => null),
    rollback: ({ repository, path }) => rollback(repository, path),
    commitOptions: () => commitOptions,
    commitMessage: ({ repository, options }) => commitMessage(repository, options),
    commit: ({ repository, message, options }, { window }) => commit(repository, message, options, window),
    hunkActions: () => hunkActions,
    ref: (repository) => ref(repository).catch(() => null),
  });
  await (await vcs.client()).register("git");
}
