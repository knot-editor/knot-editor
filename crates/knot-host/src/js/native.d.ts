// The private native bindings and hooks the host exposes to its own modules.
// This is the one place host sources may write `any`: the shapes below are
// owned by the Rust side and checked there, so `Native` marks a value that
// crossed from it.

/** A value produced by the host, whose shape the Rust side defines. */
type Native = any;

/** The live frame of the command invocation a continuation belongs to. */
type NativeFrame = Native;

type NativeBindings = {
  readonly platform: "macos" | "windows" | "linux";
  readonly homeUri: string | null;
  log(level: string, message: string): void;
  timer(operation: "start", delay?: number): number;
  timer(operation: "restart", id: number, delay: number): void;
  timer(operation: "cancel", id: number): void;
  keybinding(operation: string, key: string, command: unknown, options: unknown): void;
  request(invocation: unknown, operation: string, ...arguments_: unknown[]): Promise<Native>;
  /** Reads the current frame, or installs one when given an argument. */
  frame(): NativeFrame | null | undefined;
  frame(frame: NativeFrame | null | undefined): void;
  reportCancellationListenerFailure(message: string): void;
};

declare var __knotNativeBindings: NativeBindings | undefined;
declare var __knotDispatchTimer: (id: number) => Promise<void>;
declare var __knotSurfaceCallback: (session: number, argument: string) => Promise<{ result: string } | { error: string }>;
declare var __knotDispatchEditorEvent: (authority: Native, kind: string) => Promise<void> | undefined;
declare var __knotDispatchBufferChange: (subscription: number, change: Native) => Promise<void>;
declare var __knotInvokeCommand: (
  invocation: Native, registration: number, activeHandle: Native, commandArguments: unknown, capture: Native,
) => Promise<Native>;
declare var __knotCancelCommand: (invocation: Native) => boolean;
declare var __knotRequestTreeChildren: (
  registration: number, window: Native, instance: unknown, parentId: Native, generation: Native,
) => Promise<Native>;
declare var __knotCallProvider: (registration: number, method: string, argument: string, window: Native) => Promise<Native>;
declare var __knotDispatchSettingChange: (subscription: number, key: string) => Promise<void>;
declare var __knotDispatchWatchChange: (subscription: number, uris: Native) => Promise<void>;
declare var __knotDispatchApplicationChange: (subscription: number, value: Native) => Promise<void>;
