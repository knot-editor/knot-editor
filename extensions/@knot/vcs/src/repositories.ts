// Finding the repository of a file across every registered backend. A backend
// is asked for the repositories covering each window root; a file under none
// of them is looked up from its own directory. The deepest repository root
// that contains the file wins, and the backend registered first wins a tie.
import { backendContract } from "./contract.js";

/** The repository a file belongs to, and the backend that owns it. */
export type Repository = { readonly backend: string; readonly root: string };

/** Whether `file` is `root` or lies below it, at a path boundary. */
export function contains(root: string, file: string): boolean {
  const base = root.endsWith("/") ? root : `${root}/`;
  return file === root || file.startsWith(base);
}

/** The directory of a file URI, as a URI. */
export function directoryOf(file: string): string {
  const cut = file.lastIndexOf("/");
  return cut <= 0 ? file : file.slice(0, cut);
}

export function isGone(error: unknown): boolean {
  return (error as { code?: string }).code === "ContractNotFound";
}

export type Discovery = {
  /** Every repository covering one of the window roots. */
  covering(roots: readonly string[]): Promise<Repository[]>;
  /** The repository of `file` given the window roots, or null. */
  find(file: string, roots: readonly string[]): Promise<Repository | null>;
  /** Forgets what a backend answered, or every backend's answers. */
  invalidate(backend?: string): void;
};

/**
 * `backends` lists the registered backend IDs in registration order; `drop`
 * is told of one whose contract no longer binds.
 */
export function createDiscovery(backends: () => string[], drop: (backend: string) => void): Discovery {
  const covering = new Map<string, Promise<string[]>>();
  const nearest = new Map<string, Promise<string | null>>();

  /** Answers of one call, cached on success; a failed backend yields `fallback`. */
  async function ask<T>(
    cache: Map<string, Promise<T>>,
    key: string,
    backend: string,
    call: () => Promise<T>,
    fallback: T,
  ): Promise<T> {
    let answer = cache.get(key);
    if (answer === undefined) {
      answer = call();
      cache.set(key, answer);
    }
    try {
      return await answer;
    } catch (error) {
      cache.delete(key);
      if (isGone(error)) drop(backend);
      return fallback;
    }
  }

  /** Every repository covering one of `roots`, once each, in backend registration order. */
  async function coveringAll(roots: readonly string[]): Promise<Repository[]> {
    const found: Repository[] = [];
    for (const backend of backends()) {
      for (const root of roots) {
        const repositories = await ask(
          covering,
          `${backend}\0${root}`,
          backend,
          async () => (await backendContract(backend).client()).repositories(root),
          [],
        );
        for (const repository of repositories) {
          if (!found.some((known) => known.backend === backend && known.root === repository)) {
            found.push({ backend, root: repository });
          }
        }
      }
    }
    return found;
  }

  return {
    covering: coveringAll,
    async find(file, roots) {
      const found = (await coveringAll(roots)).filter((repository) => contains(repository.root, file));
      const ids = backends();
      if (found.length === 0) {
        const directory = directoryOf(file);
        for (const backend of ids) {
          const repository = await ask(
            nearest,
            `${backend}\0${directory}`,
            backend,
            async () => (await backendContract(backend).client()).repositoryOf(directory),
            null,
          );
          if (repository !== null && contains(repository, file)) found.push({ backend, root: repository });
        }
      }
      let best: Repository | null = null;
      for (const candidate of found) {
        if (best === null || candidate.root.length > best.root.length) best = candidate;
      }
      return best;
    },
    invalidate(backend) {
      for (const cache of [covering, nearest] as Map<string, unknown>[]) {
        for (const key of [...cache.keys()]) {
          if (backend === undefined || key.startsWith(`${backend}\0`)) cache.delete(key);
        }
      }
    },
  };
}
