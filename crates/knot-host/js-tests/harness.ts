// Loads the host's private bootstrap module under Node with its native
// bindings stubbed. The bootstrap installs console and timer globals as it
// loads; they are restored so the test runner keeps its own.
import type * as Bootstrap from "../src/js/bootstrap.js";

export type NativeCall = { operation: string; args: unknown[] };

export type StubOptions = {
  /** Answers `request(invocation, operation, ...args)`; the default answers null. */
  respond?: (call: NativeCall) => unknown;
};

export type Stub = {
  /** Every request the module made, in order. */
  calls: NativeCall[];
  /** The frame the stub holds, as `nativeBindings.frame` sets it. */
  frame: unknown;
  bindings: NativeBindings;
};

export function stubBindings(options: StubOptions = {}): Stub {
  const calls: NativeCall[] = [];
  const stub: Stub = {
    calls,
    frame: undefined,
    bindings: {
      platform: "linux",
      homeUri: null,
      log() {},
      timer: () => 1,
      keybinding() {},
      request: async (_invocation, operation, ...args) => {
        const call = { operation, args };
        calls.push(call);
        return options.respond?.(call) ?? null;
      },
      frame: ((...given: unknown[]) => {
        if (given.length === 0) return stub.frame;
        stub.frame = given[0];
      }) as NativeBindings["frame"],
      reportCancellationListenerFailure() {},
    },
  };
  return stub;
}

const installed = ["setTimeout", "setInterval", "setImmediate", "clearTimeout", "clearInterval", "clearImmediate"] as const;

/** Imports the bootstrap once for the calling test file. */
export async function loadBootstrap(stub: Stub): Promise<typeof Bootstrap> {
  const consoleMethods = (["debug", "log", "info", "warn", "error"] as const).map(
    (name) => [name, console[name]] as const,
  );
  const timers = installed.map((name) => [name, globalThis[name]] as const);
  globalThis.__knotNativeBindings = stub.bindings;
  try {
    return await import("../src/js/bootstrap.js");
  } finally {
    for (const [name, original] of consoleMethods) console[name] = original;
    for (const [name, original] of timers) Object.assign(globalThis, { [name]: original });
  }
}
