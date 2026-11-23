import { workbench } from "knot";
import { backendContract, vcs } from "@knot/vcs";
import { baseline } from "./baseline.js";
import { git, isMissingProgram } from "./git.js";
import { status } from "./status.js";
import { repositories, repositoryOf } from "./repositories.js";
import { watch } from "./watch.js";

/** Whether Git runs; without it there is nothing to serve, and the user is told once. */
async function available(): Promise<boolean> {
  try {
    await git("file:///", ["--version"]);
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
    status: (repository) => status(repository).catch(() => []),
  });
  await (await vcs.client()).register("git");
}
