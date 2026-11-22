import type { Disposable } from "knot";
import { backendContract, vcs } from "./contract.js";

/** Serves `vcs` to backends; resolves with what disposes it. */
export async function start(): Promise<Disposable> {
  const registered = new Set<string>();
  return vcs.serve({
    register(id) {
      registered.add(id);
    },
    unregister(id) {
      registered.delete(id);
    },
    changed() {},
    async backends() {
      // A backend that stopped without unregistering has nothing to bind.
      for (const id of [...registered]) {
        try {
          await backendContract(id).client();
        } catch {
          registered.delete(id);
        }
      }
      return [...registered];
    },
  });
}
