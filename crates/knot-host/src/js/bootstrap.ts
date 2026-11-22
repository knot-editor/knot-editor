import type { PaneTarget, ViewTarget, WindowTarget } from "knot";
import type * as DeclaredContract from "knot/contract";
import { defineContract as defineSchemaContract, s } from "knot/contract";

type Rec = Record<string, unknown>;
/** Options an extension passes; their fields are validated where read. */
type Opts = Record<string, Native>;

const nativeBindings = globalThis.__knotNativeBindings as NativeBindings;
delete (globalThis as { __knotNativeBindings?: NativeBindings }).__knotNativeBindings;

export const platform = nativeBindings.platform;
export const homeUri = nativeBindings.homeUri;

function formatLogArgument(value: unknown) {
  if (typeof value === "string") return value;
  if (value instanceof Error) return value.stack ?? String(value);
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

// V8's console discards its output; this one writes to standard error.
for (const level of ["debug", "log", "info", "warn", "error"] as const) {
  console[level] = (...values: unknown[]) => {
    nativeBindings.log(level, values.map(formatLogArgument).join(" "));
  };
}

// Node-compatible timer globals. The host owns IDs and waits; each expiry
// runs as its own root, and lifecycle teardown cancels every timer.
const TIMEOUT_MAX = 2 ** 31 - 1;
type TimerCallback = (this: unknown, ...args: unknown[]) => unknown;
type TimerEntry = {
  id: number;
  owner: object;
  callback: TimerCallback;
  args: unknown[];
  delay?: number;
  repeat?: boolean;
  immediate?: boolean;
  cleared: boolean;
};
const armedTimers = new Map<number, TimerEntry>();
const timerEntries = new WeakMap<object, TimerEntry>();

function timerDelay(after: unknown) {
  const delay = Number(after) * 1;
  return delay >= 1 && delay <= TIMEOUT_MAX ? delay : 1;
}

function timerCallback(callback: unknown): TimerCallback {
  if (typeof callback === "function") return callback as TimerCallback;
  const error: Error & { code?: string } = new TypeError(
    `The "callback" argument must be of type function. Received ${typeof callback}`,
  );
  error.code = "ERR_INVALID_ARG_TYPE";
  throw error;
}

function cancelTimer(entry: TimerEntry) {
  if (entry.cleared) return;
  entry.cleared = true;
  armedTimers.delete(entry.id);
  nativeBindings.timer("cancel", entry.id);
}

class Timeout {
  #refed = true;
  constructor(callback: TimerCallback, delay: number, args: unknown[], repeat: boolean) {
    const entry: TimerEntry = { id: nativeBindings.timer("start", delay), owner: this, callback, args, delay, repeat, cleared: false };
    timerEntries.set(this, entry);
    armedTimers.set(entry.id, entry);
  }
  refresh() {
    const entry = timerEntries.get(this)!;
    if (!entry.cleared) {
      nativeBindings.timer("restart", entry.id, entry.delay!);
      armedTimers.set(entry.id, entry);
    }
    return this;
  }
  close() { clearTimeout(this); return this; }
  ref() { this.#refed = true; return this; }
  unref() { this.#refed = false; return this; }
  hasRef() { return this.#refed; }
  [Symbol.toPrimitive]() { return timerEntries.get(this)!.id; }
}

class Immediate {
  #refed = true;
  constructor(callback: TimerCallback, args: unknown[]) {
    const entry: TimerEntry = { id: nativeBindings.timer("start"), owner: this, callback, args, immediate: true, cleared: false };
    timerEntries.set(this, entry);
    armedTimers.set(entry.id, entry);
  }
  ref() { this.#refed = true; return this; }
  unref() { this.#refed = false; return this; }
  hasRef() { return this.#refed; }
}

Object.defineProperty(Timeout.prototype, Symbol.dispose, {
  value: function (this: Timeout) { clearTimeout(this); },
  writable: true, configurable: true,
});
Object.defineProperty(Immediate.prototype, Symbol.dispose, {
  value: function (this: Immediate) { clearImmediate(this); },
  writable: true, configurable: true,
});

function clearTimeout(timer: unknown) {
  const entry = typeof timer === "number" || typeof timer === "string"
    ? armedTimers.get(Number(timer))
    : timer !== null && typeof timer === "object" ? timerEntries.get(timer) : undefined;
  if (entry && !entry.immediate) cancelTimer(entry);
}

function clearImmediate(immediate: unknown) {
  const entry = immediate !== null && typeof immediate === "object" ? timerEntries.get(immediate) : undefined;
  if (entry?.immediate) cancelTimer(entry);
}

Object.assign(globalThis, {
  setTimeout: (callback: unknown, delay: unknown, ...args: unknown[]) => new Timeout(timerCallback(callback), timerDelay(delay), args, false),
  setInterval: (callback: unknown, delay: unknown, ...args: unknown[]) => new Timeout(timerCallback(callback), timerDelay(delay), args, true),
  setImmediate: (callback: unknown, ...args: unknown[]) => new Immediate(timerCallback(callback), args),
  clearTimeout,
  clearInterval: clearTimeout,
  clearImmediate,
});

globalThis.__knotDispatchTimer = async (id: number) => {
  const entry = armedTimers.get(id);
  if (!entry) return;
  if (!entry.repeat) {
    armedTimers.delete(id);
    if (entry.immediate) entry.cleared = true;
  }
  let result;
  try {
    result = entry.callback.apply(entry.owner, entry.args);
  } finally {
    // An interval waits again only once its callback has returned, so slow
    // callbacks never stack missed ticks.
    if (entry.repeat && !entry.cleared) nativeBindings.timer("restart", id, entry.delay!);
  }
  await result;
};

export function keybinding(key: string, command: unknown, options?: Opts) {
  if (command !== null && typeof command === "object" && !Array.isArray(command)) {
    const object = command as Rec;
    try {
      command = {
        name: object.name,
        arguments: object.arguments === undefined ? null : validateJsonValue(object.arguments),
      };
    } catch (error) {
      nativeBindings.keybinding("invalid", String(error), undefined, undefined);
      return;
    }
  }
  nativeBindings.keybinding("set", key, command, options);
}

export function removeKeybinding(key: string, options?: Opts) {
  nativeBindings.keybinding("remove", key, undefined, options);
}

export function textBinding(command: unknown, options?: Opts) {
  nativeBindings.keybinding("setText", "", command, options);
}

export function removeTextBinding(options?: Opts) {
  nativeBindings.keybinding("removeText", "", undefined, options);
}

type CodedError = Error & { code?: unknown };

/** The `code` a host error carries, if `error` has one. */
function errorCode(error: unknown): unknown {
  return typeof error === "object" && error !== null ? (error as CodedError).code : undefined;
}

/** The text of a thrown value, stack included when it has one. */
function errorStack(error: unknown): string {
  return String((error as { stack?: unknown } | null | undefined)?.stack ?? error);
}

/** The text of a thrown value, message only when it has one. */
function errorMessage(error: unknown): string {
  return String((error as { message?: unknown } | null | undefined)?.message ?? error);
}

function isObject(value: unknown): value is object {
  return typeof value === "object" && value !== null;
}

/** Whether `target` is a target object registered in `table`. */
function isLive(table: WeakMap<object, unknown>, target: unknown): boolean {
  return typeof target === "object" && target !== null && table.has(target);
}

function hostError(code: string): never {
  const exception: CodedError = new Error(code);
  exception.name = "KnotHostError";
  exception.code = code;
  throw exception;
}

export function ownership(removeLocal: () => void, release: () => Promise<unknown>) {
  let inactive = false;
  let cleanup: Promise<void> | undefined;
  const pending = new Set<Promise<unknown>>();
  return {
    assertActive() { if (inactive) hostError("OwnershipDisposed"); },
    track<T>(promise: Promise<T>) {
      pending.add(promise);
      promise.then(() => pending.delete(promise), () => pending.delete(promise));
      return promise;
    },
    dispose() {
      if (cleanup) return cleanup;
      inactive = true;
      removeLocal();
      const releasePromise = release();
      const earlier = [...pending];
      cleanup = Promise.allSettled([...earlier, releasePromise]).then((results) => {
        const releaseResult = results.at(-1)!;
        if (releaseResult.status === "rejected") throw releaseResult.reason;
      });
      return cleanup;
    },
  };
}

/** A frozen handle released by `await using` or `[Symbol.asyncDispose]()`. */
function disposableHandle(owned: { dispose(): Promise<void> }) {
  return Object.freeze({
    [Symbol.asyncDispose]: () => owned.dispose(),
  });
}

function record(value: unknown, message: string): Rec {
  expectRecord(value, message);
  return value;
}
type Fn = (...args: Native[]) => unknown;
function expectFunction(value: unknown, message: string): asserts value is Fn {
  if (typeof value !== "function") throw new TypeError(message);
}
function expectRecord(value: unknown, message: string): asserts value is Rec {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new TypeError(message);
}
function string(value: unknown, message: string) {
  if (typeof value !== "string") throw new TypeError(message);
  return value;
}
function flag(value: unknown, message: string) {
  if (typeof value !== "boolean") throw new TypeError(message);
  return value;
}
function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}
export function safeInteger(value: unknown) {
  if (typeof value !== "number") throw new TypeError("expected a number");
  if (!isNonNegativeInteger(value)) throw new RangeError("expected a nonnegative safe integer");
  return value;
}
function barId(value: unknown) {
  if (typeof value !== "string" || value === "") throw new TypeError("bar id must be a non-empty string");
  return value;
}
function barCommand(value: unknown) {
  expectRecord(value, "bar command must be an object");
  const name = string(value.name, "bar command name must be a string");
  if (name === "") throw new TypeError("bar command name must not be empty");
  return { name, arguments: value.arguments ?? null };
}
function barControl(value: unknown, kind: unknown) {
  expectRecord(value, `bar ${kind} must be an object`);
  const tooltip = value.tooltip ?? null;
  if (tooltip !== null && typeof tooltip !== "string") throw new TypeError(`bar ${kind} tooltip must be a string`);
  const icon = value.icon;
  if (typeof icon !== "string" && (icon === null || typeof icon !== "object")) {
    throw new TypeError(`bar ${kind} icon must be an icon reference`);
  }
  return {
    id: string(value.id, `bar ${kind} id must be a string`),
    icon,
    tooltip,
    command: barCommand(value.command),
  };
}
function barList(value: unknown, name: unknown) {
  const list = value ?? [];
  if (!Array.isArray(list)) throw new TypeError(`bar ${name} must be an array`);
  return list;
}
function barValue(value: unknown) {
  expectRecord(value, "bar must be an object");
  const fields = barList(value.fields, "fields");
  const message = value.message ?? null;
  if (message !== null && typeof message !== "string") throw new TypeError("bar message must be a string or null");
  return {
    fields: fields.map((field) => {
      expectRecord(field, "bar field must be an object");
      const placeholder = field.placeholder ?? "";
      return {
        id: string(field.id, "bar field id must be a string"),
        placeholder: string(placeholder, "bar field placeholder must be a string"),
        history: historyEntries(field.history),
      };
    }),
    toggles: barList(value.toggles, "toggles").map((toggle) => {
      const control = barControl(toggle, "toggle");
      const active = toggle.active ?? false;
      if (typeof active !== "boolean") throw new TypeError("bar toggle active must be a boolean");
      return { ...control, active };
    }),
    buttons: barList(value.buttons, "buttons").map((button) => barControl(button, "button")),
    message,
  };
}
function foldRange(value: unknown) {
  expectRecord(value, "fold range must be an object");
  const { start, end } = value;
  if (typeof start !== "number" || typeof end !== "number" ||
      !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start >= end) {
    throw new TypeError("a fold needs a non-negative safe integer start below its end");
  }
  return [start, end];
}
function byteRange(value: unknown) {
  expectRecord(value, "range must be an object");
  const startByteOffset = safeInteger(value.startByteOffset);
  const endByteOffset = safeInteger(value.endByteOffset);
  if (startByteOffset > endByteOffset) throw new RangeError("range endpoints must be ordered");
  return { startByteOffset, endByteOffset };
}
function revision(options?: Opts) {
  return safeInteger(record(options, "revision options are required").ifRevision);
}
function arrayItems<T>(items: unknown, convert: (item: unknown) => T): T[] {
  if (!Array.isArray(items)) throw new TypeError("items must be an array");
  return Array.from(items, convert);
}
function historyEntries(value: unknown) {
  if (value == null) return [];
  return arrayItems(value, (entry: unknown) => string(entry, "history entries must be text"));
}
function optionalText(value: unknown) {
  return value == null ? null : string(value, "expected text or null");
}
function iconRef(value: unknown) {
  if (value == null || typeof value === "string") return value ?? null;
  expectRecord(value, "icon must be a name, { file } or { folder }");
  if (value.file != null) return { file: string(value.file, "icon file must be text") };
  return { folder: string(value.folder, "icon folder must be text") };
}
function token<const T extends string>(value: unknown, allowed: readonly T[]): T | null {
  if (value == null) return null;
  if (!allowed.includes(value as T)) throw new TypeError("invalid semantic token");
  return value as T;
}
function decoration(value: unknown) {
  if (value == null) return null;
  expectRecord(value, "decoration must be an object");
  const style = token(value.style, ["background", "line", "underline", "wavy"]);
  if (style === null) throw new TypeError("decoration needs a style");
  return { color: string(value.color, "decoration color must be text"), style };
}
function contribution(value: unknown) {
  expectRecord(value, "contribution must be an object");
  return {
    range: byteRange(value.range),
    decoration: decoration(value.decoration),
    gutter: optionalText(value.gutter),
    command: optionalText(value.command),
  };
}
function viewDecoration(value: unknown) {
  expectRecord(value, "view decoration must be an object");
  const painted = decoration(value.decoration);
  if (painted === null) throw new TypeError("view decoration needs a decoration");
  return { range: byteRange(value.range), decoration: painted, gutter: null, command: null };
}
function iterableItems<T>(value: unknown, convert: (item: unknown) => T): T[] {
  if (value == null || typeof value === "string" ||
      typeof (value as Partial<Iterable<unknown>>)[Symbol.iterator] !== "function") {
    throw new TypeError("provider result must be an iterable of items");
  }
  return Array.from(value as Iterable<unknown>, convert);
}
function treeAction(value: unknown) {
  expectRecord(value, "tree action must be an object");
  return {
    label: string(value.label, "tree action label must be text"),
    command: string(value.command, "tree action command must be text"),
    icon: iconRef(value.icon),
    menuOnly: value.menuOnly == null ? false : flag(value.menuOnly, "tree action menuOnly must be a boolean"),
    showOnHover: value.showOnHover == null ? false : flag(value.showOnHover, "tree action showOnHover must be a boolean"),
    arguments: value.arguments === undefined ? null : validateJsonValue(value.arguments),
  };
}
function treeActions(value: unknown) {
  return value == null ? [] : arrayItems(value, treeAction);
}
function treeItem(value: unknown) {
  expectRecord(value, "tree item must be an object");
  const collapsibleState = token(value.collapsibleState, ["none", "collapsed", "expanded"]);
  if (collapsibleState === null) throw new TypeError("collapsibleState is required");
  const kind = token(value.kind, ["item", "group"]);
  return {
    id: string(value.id, "tree item ID must be text"),
    label: string(value.label, "tree item label must be text"),
    description: optionalText(value.description),
    detail: optionalText(value.detail),
    detailMaxLines: value.detailMaxLines == null ? null : safeInteger(value.detailMaxLines),
    icon: iconRef(value.icon),
    collapsibleState,
    command: optionalText(value.command),
    arguments: value.arguments === undefined ? null : validateJsonValue(value.arguments),
    kind: kind === null ? "item" : kind,
    muted: value.muted == null ? false : flag(value.muted, "tree item muted must be a boolean"),
    bold: value.bold == null ? false : flag(value.bold, "tree item bold must be a boolean"),
    statusText: optionalText(value.statusText),
    badge: optionalText(value.badge),
    actions: treeActions(value.actions),
    draggable: value.draggable == null ? false : flag(value.draggable, "tree item draggable must be a boolean"),
    dropTarget: value.dropTarget == null ? false : flag(value.dropTarget, "tree item dropTarget must be a boolean"),
    resource: optionalText(value.resource),
  };
}

async function requestForInvocation(invocation: Native, operation: string, ...arguments_: unknown[]) {
  try {
    return await nativeBindings.request(
      invocation,
      operation,
      ...arguments_,
    );
  } catch (error) {
    if (errorCode(error) === "Cancelled") liveFrames.get(invocation)?.controller?.abort(error);
    throw error;
  }
}

function request(operation: string, ...arguments_: unknown[]) {
  return requestForInvocation(
    currentFrame()?.invocation ?? null,
    operation,
    ...arguments_,
  );
}

// A registration or subscription is live once its host reply has installed
// the handler, but the application may call or notify it the moment it has
// handled the request. A call or notification that finds no handler waits
// until a registration in flight settles, and resumes in a later task, so the
// code that registered has continued past its `await` first; it waits again
// if its handler is still missing and other registrations are in flight.
let registrationsInFlight = 0;
let registrationWaiters: (() => void)[] = [];

function wakeRegistrationWaiters() {
  const waiters = registrationWaiters;
  registrationWaiters = [];
  for (const resolve of waiters) resolve();
}

async function registerCallable<R>(install: (registration: R) => void, send: () => Promise<R>): Promise<R> {
  registrationsInFlight++;
  try {
    const registration = await send();
    install(registration);
    return registration;
  } finally {
    registrationsInFlight--;
    if (registrationWaiters.length > 0) {
      new Immediate(wakeRegistrationWaiters, []);
    }
  }
}

async function whenRegistered(registered: () => boolean) {
  while (!registered() && registrationsInFlight > 0) {
    await new Promise<void>((resolve) => registrationWaiters.push(resolve));
  }
}

async function callable<V>(table: Map<number, V>, registration: number): Promise<V | undefined> {
  await whenRegistered(() => table.has(registration));
  return table.get(registration);
}

// Runs `run` once the previous run queued on `owner[key]` has settled. A
// subscription's notifications take turns this way, so a slow listener sees
// them in order and, where the application coalesces changes until a
// notification acknowledges them, the changes made meanwhile join the next one.
function inTurn<K extends string, T>(
  owner: { [P in K]?: Promise<unknown> }, key: K, run: () => T | Promise<T>,
): Promise<T> {
  const result = (owner[key] ?? Promise.resolve()).then(run);
  owner[key] = result.catch(() => {});
  return result;
}

const buffers = new Map<Native, Native>();
const commandHandlers = new Map<number, Fn>();
const bufferChangeListeners = new Map<number, { handle: Native; listener: Fn; turn?: Promise<unknown> }>();
const treeProviders = new Map<number, { getChildren: Fn }>();
const providerImplementations = new Map<number, Record<string, unknown>>();
// The frame of the invocation a continuation belongs to travels with the
// continuation itself (V8's continuation-preserved embedder data), so it
// survives awaits and promise reactions. `liveFrames` indexes the frames of
// unfinished command invocations for cancellation.
const liveFrames = new Map<unknown, Frame>();

type Frame = {
  root: Native;
  invocation?: Native;
  controller?: KnotAbortController;
  buffer?: Native;
  capture?: Native;
  target?: Native;
};

function currentFrame(): Frame | null {
  return (nativeBindings.frame() as Frame | null | undefined) ?? null;
}

function withFrame<T>(frame: Frame | null, run: () => T): T {
  const previous = nativeBindings.frame();
  nativeBindings.frame(frame);
  try {
    return run();
  } finally {
    nativeBindings.frame(previous);
  }
}

// The callback a command passes to a native surface it opens (picker
// highlights and queries, input values, tree-edit validation). The surface
// calls it as tasks of this package while the command awaits the surface's
// result; each call runs in the command's frame, so it has the command's
// authority until the surface settles, and a call still running afterwards
// keeps only the command's root. A failing call stops further calls and
// rethrows once the surface settles, so a surface never stays open unowned.
// Calls for which `supersedes` holds overlap, and only the failure of the
// latest of them counts, since the surface drops the others' answers.
type SurfaceState = {
  open: boolean;
  callback: (event: Native) => unknown;
  supersedes: (event: Native) => boolean;
  latest: number;
  failure: { error: unknown } | null;
  frame: Frame | null;
};
const surfaceCallbacks = new Map<number, SurfaceState>();
let nextSurfaceSession = 1;

async function withSurfaceCallback<T>(
  callback: (event: Native) => unknown,
  open: (session: number) => Promise<T>,
  supersedes: (event: Native) => boolean = () => false,
): Promise<T> {
  const session = nextSurfaceSession++;
  const command = currentFrame();
  const state: SurfaceState = { open: true, callback, supersedes, latest: 0, failure: null, frame: null };
  if (command !== null) {
    const frame: Frame = { root: command.root };
    for (const key of ["invocation", "controller", "buffer", "capture", "target"] as const) {
      Object.defineProperty(frame, key, { get: () => (state.open ? command[key] : undefined) });
    }
    state.frame = frame;
  }
  surfaceCallbacks.set(session, state);
  let result: T;
  try {
    result = await open(session);
  } finally {
    state.open = false;
    surfaceCallbacks.delete(session);
  }
  if (state.failure !== null) throw state.failure.error;
  return result;
}

globalThis.__knotSurfaceCallback = async (session: number, argument: string) => {
  const state = surfaceCallbacks.get(session);
  if (state === undefined || state.failure !== null) return { error: "the surface callback stopped" };
  const event = JSON.parse(argument);
  const call = state.supersedes(event) ? ++state.latest : null;
  try {
    const result = await withFrame(state.frame, () => state.callback(event));
    return { result: JSON.stringify(result ?? null) };
  } catch (error) {
    if (call === null || call === state.latest) state.failure ??= { error };
    return { error: errorMessage(error) };
  }
};
type SnapshotTable = { byteAtUtf16: number[]; utf16AtByte: Map<number, number> };
const snapshotTables = new WeakMap<object, SnapshotTable>();
const targetAuthorities = new WeakMap<object, Native>();
const invalidArgumentErrors = new WeakSet<object>();
const unavailableErrors = new WeakSet<object>();
const signalAborters = new WeakMap<object, (reason: unknown) => void>();

type AbortListener = (this: KnotAbortSignal, event: Readonly<{ type: "abort"; target: KnotAbortSignal }>) => unknown;

class KnotAbortSignal {
  #aborted = false;
  #reason: unknown = undefined;
  #listeners = new Set<AbortListener>();
  constructor() { signalAborters.set(this, (reason: unknown) => this.#abort(reason)); }
  get aborted() { return this.#aborted; }
  get reason() { return this.#reason; }
  addEventListener(type: unknown, listener: unknown) {
    if (type !== "abort" || typeof listener !== "function") throw new TypeError("expected an abort listener");
    if (!this.#aborted) this.#listeners.add(listener as AbortListener);
  }
  removeEventListener(type: unknown, listener: unknown) {
    if (type !== "abort" || typeof listener !== "function") throw new TypeError("expected an abort listener");
    this.#listeners.delete(listener as AbortListener);
  }
  throwIfAborted() {
    if (this.#aborted) throw this.#reason;
  }
  #abort(reason: unknown) {
    if (this.#aborted) return;
    this.#aborted = true;
    this.#reason = reason;
    for (const listener of this.#listeners) {
      try { listener.call(this, Object.freeze({ type: "abort", target: this })); }
      catch (error) {
        let message;
        try { message = errorStack(error); } catch { message = "abort listener failed"; }
        nativeBindings.reportCancellationListenerFailure(message);
      }
    }
    this.#listeners.clear();
  }
}

class KnotAbortController {
  signal = Object.freeze(new KnotAbortSignal());
  abort(reason: unknown = new Error("command cancelled")) { signalAborters.get(this.signal)!(reason); }
}

function byteLengthOfCodePoint(codePoint: number) {
  return codePoint <= 0x7f ? 1 : codePoint <= 0x7ff ? 2 : codePoint <= 0xffff ? 3 : 4;
}

function snapshotTable(snapshot: Native) {
  let table = snapshotTables.get(snapshot);
  if (table) return table;

  const byteAtUtf16 = [0];
  const utf16AtByte = new Map<number, number>([[0, 0]]);
  let byteOffset = 0;
  let utf16Offset = 0;
  for (const character of snapshot.text) {
    byteOffset += byteLengthOfCodePoint(character.codePointAt(0)!);
    utf16Offset += character.length;
    byteAtUtf16[utf16Offset] = byteOffset;
    utf16AtByte.set(byteOffset, utf16Offset);
  }
  table = { byteAtUtf16, utf16AtByte };
  snapshotTables.set(snapshot, table);
  return table;
}

function snapshotFromNative(snapshot: Native) {
  return Object.freeze({
    text: snapshot.text,
    range: Object.freeze({
      startByteOffset: snapshot.range.startByteOffset,
      endByteOffset: snapshot.range.endByteOffset,
    }),
    revision: snapshot.revision,
    byteOffsetAtUtf16(offset: Native) {
      safeInteger(offset);
      const offsets = snapshotTable(this).byteAtUtf16;
      const value = Object.hasOwn(offsets, offset) ? offsets[offset] : undefined;
      if (value === undefined) {
        throw new RangeError("UTF-16 offset splits a surrogate pair or is out of bounds");
      }
      return value;
    },
    utf16OffsetAtByte(offset: Native) {
      safeInteger(offset);
      const value = snapshotTable(this).utf16AtByte.get(offset);
      if (value === undefined) {
        throw new RangeError("byte offset splits a UTF-8 scalar or is out of bounds");
      }
      return value;
    },
  });
}

function bufferFor(handle: Native) {
  let buffer = buffers.get(handle);
  if (buffer) return buffer;

  const contributionOwnership = ownership(() => {}, () => request("disposeEditorContributions", handle));
  const contributions = Object.freeze({
    async replace(items: unknown, options?: Opts) {
      contributionOwnership.assertActive();
      const converted = arrayItems(items, contribution);
      const ifRevision = revision(options);
      await contributionOwnership.track(request("replaceEditorContributions", handle, converted, ifRevision));
    },
    [Symbol.asyncDispose]: () => contributionOwnership.dispose(),
  });

  buffer = Object.freeze({
    async snapshot(range: unknown) {
      return snapshotFromNative(await request("snapshot", handle, range == null ? null : byteRange(range)));
    },
    async applyEdits(edits: unknown, options?: Opts) {
      const converted = arrayItems(edits, (edit: unknown) => {
        expectRecord(edit, "edit must be an object");
        return { range: byteRange(edit.range), text: string(edit.text, "edit text must be a string") };
      });
      const resultRevision = await request("applyEdits", handle, converted, revision(options));
      return Object.freeze({ revision: resultRevision });
    },
    lineCount: () => request("lineCount", handle),
    async lineRange(line: unknown) {
      const range = await request("lineRange", handle, safeInteger(line));
      return Object.freeze({ startByteOffset: range.startByteOffset, endByteOffset: range.endByteOffset });
    },
    async lines(first: unknown, last: unknown) {
      return lineItems(await request("lines", handle, safeInteger(first), safeInteger(last)));
    },
    lineAt: (offset: unknown) => request("lineAt", handle, safeInteger(offset)),
    async wordAt(offset: unknown) {
      const word = await request("wordAt", handle, safeInteger(offset));
      return word === null ? null : Object.freeze({
        range: Object.freeze({ startByteOffset: word.range.startByteOffset, endByteOffset: word.range.endByteOffset }),
        kind: word.kind,
      });
    },
    async find(query: unknown, options?: Opts) {
      if (typeof query !== "string") throw new TypeError("find query must be a string");
      const limit = options?.limit ?? 1000;
      if (!isNonNegativeInteger(limit)) throw new TypeError("limit must be a non-negative safe integer");
      const result = await request(
        "find",
        handle,
        query,
        options?.caseSensitive === true,
        options?.wholeWord === true,
        options?.regex === true,
        options?.range == null ? null : byteRange(options.range),
        limit,
        options?.perLine === true,
      );
      return Object.freeze({
        revision: result.revision,
        matches: Object.freeze(result.matches.map((match: Native) => Object.freeze(match))),
        more: result.more,
      });
    },
    async replace(query: unknown, replacement: unknown, options?: Opts) {
      if (typeof query !== "string") throw new TypeError("replace query must be a string");
      if (typeof replacement !== "string") throw new TypeError("replacement must be a string");
      const limit = options?.limit ?? Number.MAX_SAFE_INTEGER;
      if (!isNonNegativeInteger(limit)) throw new TypeError("limit must be a non-negative safe integer");
      const result = await request(
        "replace",
        handle,
        query,
        replacement,
        options?.caseSensitive === true,
        options?.wholeWord === true,
        options?.regex === true,
        options?.range == null ? null : byteRange(options.range),
        limit,
        options?.ifRevision == null ? null : safeInteger(options.ifRevision),
        options?.perLine === true,
      );
      return Object.freeze({ revision: result.revision, count: result.count, more: result.more });
    },
    async createMark(offset: unknown) {
      const id = await request("createMark", handle, safeInteger(offset));
      const owned = ownership(() => {}, () => request("disposeMark", handle, id));
      return Object.freeze({
        position: () => request("markPosition", handle, id),
        [Symbol.asyncDispose]: () => owned.dispose(),
      });
    },
    async onDidChange(listener: unknown) {
      expectFunction(listener, "onDidChange requires a listener");
      const subscription = await registerCallable(
        (subscription: number) => bufferChangeListeners.set(subscription, { handle, listener }),
        () => request("subscribeBufferChanges", handle));
      const owned = ownership(() => bufferChangeListeners.delete(subscription),
        () => request("unsubscribeBufferChanges", subscription));
      return disposableHandle(owned);
    },
    contributions,
  });
  buffers.set(handle, buffer);
  return buffer;
}

function lineItems(items: Native) {
  return Object.freeze(items.map((item: Native) => Object.freeze({
    line: item.line, startByteOffset: item.startByteOffset, text: item.text,
  })));
}

function selectionFromNative(value: Native) {
  const selection: Native = { anchor: value.anchor, caret: value.caret };
  if (value.lines !== undefined) {
    selection.line = value.line;
    selection.lines = lineItems(value.lines);
  }
  return Object.freeze(selection);
}

function linesOption(options?: Opts) {
  if (options?.lines === undefined) return null;
  const lines = options.lines;
  if (!isNonNegativeInteger(lines)) throw new TypeError("lines must be a non-negative safe integer");
  return lines;
}

function editorFor(authority: Native) {
  return Object.freeze({
    async selection(options?: Opts) {
      return selectionFromNative(await request("editorSelection", authority, linesOption(options)));
    },
    async preparePaste(text: unknown, range: unknown) {
      if (typeof text !== "string") throw new TypeError("text must be a string");
      if (range === undefined) return await request("editorPreparePaste", authority, text, null, null);
      if (!Array.isArray(range) || range.length !== 2) throw new TypeError("range must be a [start, end] pair");
      return await request("editorPreparePaste", authority, text, safeInteger(range[0]), safeInteger(range[1]));
    },
    async setSelection(selection: unknown, options?: Opts) {
      expectRecord(selection, "selection must be an object");
      const anchor = safeInteger(selection.anchor);
      const caret = safeInteger(selection.caret);
      const reveal = options?.reveal ?? "nearest";
      if (reveal !== "nearest" && reveal !== "center" && reveal !== "always-center" && reveal !== "top" && reveal !== "bottom") {
        throw new TypeError('reveal must be "nearest", "center", "always-center", "top" or "bottom"');
      }
      await request("editorSetSelection", authority, anchor, caret, reveal);
    },
    async viewport() {
      const value = await request("editorViewport", authority);
      return Object.freeze({ first: value.first, rows: value.rows });
    },
    async rowAt(row: unknown) {
      if (!isNonNegativeInteger(row)) throw new TypeError("row must be a non-negative safe integer");
      const value = await request("editorRowAt", authority, row);
      return Object.freeze({ line: value.line, column: value.column });
    },
    async rowOf(line: unknown, column: unknown) {
      if (!isNonNegativeInteger(line)) throw new TypeError("line must be a non-negative safe integer");
      if (!isNonNegativeInteger(column)) throw new TypeError("column must be a non-negative safe integer");
      return request("editorRowOf", authority, line, column);
    },
    async cellsBefore(line: unknown, column: unknown) {
      if (!isNonNegativeInteger(line)) throw new TypeError("line must be a non-negative safe integer");
      if (!isNonNegativeInteger(column)) throw new TypeError("column must be a non-negative safe integer");
      return request("editorCellsBefore", authority, line, column);
    },
    async columnAtCells(line: unknown, cells: unknown) {
      if (!isNonNegativeInteger(line)) throw new TypeError("line must be a non-negative safe integer");
      if (!isNonNegativeInteger(cells)) throw new TypeError("cells must be a non-negative safe integer");
      return request("editorColumnAtCells", authority, line, cells);
    },
    async fold(range: unknown) {
      const [start, end] = foldRange(range);
      await request("editorFold", authority, start, end);
    },
    async unfold(range: unknown) {
      const [start, end] = foldRange(range);
      await request("editorUnfold", authority, start, end);
    },
    async folds() {
      const folds = await request("editorFolds", authority);
      return Object.freeze(folds.map(({ start, end }: Native) => Object.freeze({ start, end })));
    },
    async foldRegions() {
      const regions = await request("editorFoldRegions", authority);
      return Object.freeze(regions.map(({ start, end }: Native) => Object.freeze({ start, end })));
    },
    async scrollBy(rows: unknown) {
      if (!Number.isSafeInteger(rows)) throw new TypeError("rows must be a safe integer");
      await request("editorScrollBy", authority, rows);
    },
    async setKeyContext(name: unknown, active: unknown) {
      if (typeof name !== "string" || name === "") {
        throw new TypeError("key context must be a non-empty string");
      }
      if (typeof active !== "boolean") throw new TypeError("active must be a boolean");
      await request("editorSetKeyContext", authority, name, active);
    },
    async setDecorations(items: unknown, options?: Opts) {
      const converted = arrayItems(items, viewDecoration);
      await request("editorSetDecorations", authority, converted, revision(options));
    },
    async setCursorStyle(style: unknown) {
      if (style !== null && style !== "bar" && style !== "block" && style !== "underline") {
        throw new TypeError('cursor style must be "bar", "block", "underline" or null');
      }
      await request("editorSetCursorStyle", authority, style);
    },
    async openBar(id: unknown, bar: unknown) {
      await request("editorOpenBar", authority, barId(id), barValue(bar));
    },
    async closeBar(id: unknown) {
      await request("editorCloseBar", authority, barId(id));
    },
    async focusBarField(id: unknown, field: unknown, options?: Opts) {
      const text = options?.text ?? null;
      if (text !== null && typeof text !== "string") throw new TypeError("bar field text must be a string");
      await request("editorFocusBarField", authority, barId(id), string(field, "bar field id must be a string"), text);
    },
    async barFields(id: unknown) {
      return Object.freeze(await request("editorBarFields", authority, barId(id)));
    },
    async beginUndoGroup() {
      const group = await request("editorBeginUndoGroup", authority);
      const owned = ownership(() => {}, () => request("editorEndUndoGroup", group));
      return disposableHandle(owned);
    },
    async record() {
      const id = await request("editorBeginRecording", authority);
      let ended;
      const end = () => (ended ??= request("editorEndRecording", id));
      const owned = ownership(() => {}, () => end());
      let finished = false;
      return Object.freeze({
        async stop() {
          if (finished) return Object.freeze([]);
          finished = true;
          const entries = await end();
          await owned.dispose();
          return Object.freeze(entries.map((entry: unknown) => Object.freeze(entry)));
        },
        [Symbol.asyncDispose]() {
          finished = true;
          return owned.dispose();
        },
      });
    },
    async viewState() {
      const value = await request("editorViewState", authority);
      return Object.freeze({ anchor: value.anchor, caret: value.caret, top: value.top, scrollX: value.scrollX });
    },
    async restoreViewState(state: unknown) {
      expectRecord(state, "view state must be an object");
      const scrollX = state.scrollX;
      if (typeof scrollX !== "number" || !Number.isFinite(scrollX)) {
        throw new TypeError("view state scrollX must be a finite number");
      }
      await request("editorRestoreViewState", authority, safeInteger(state.anchor),
        safeInteger(state.caret), safeInteger(state.top), scrollX);
    },
  });
}

type EditorEntry = {
  closed: boolean;
  listeners: Set<Fn>;
  selectionListeners: Map<Fn, number | null>;
  watching: Promise<unknown> | null;
  barListeners: Set<Fn>;
  watchingBars: Promise<unknown> | null;
  selectionTurn?: Promise<unknown>;
  barsTurn?: Promise<unknown>;
};
const editorHandles = new Map<Native, EditorEntry>();

export async function openEditor(target: unknown) {
  if (!isLive(livePresentations, target)) hostError("InvalidTarget");
  const authority = await request("openEditor", destinationAuthority({ target }));
  const entry: EditorEntry = {
    closed: false, listeners: new Set(), selectionListeners: new Map(), watching: null,
    barListeners: new Set(), watchingBars: null,
  };
  editorHandles.set(authority, entry);
  const owned = ownership(() => {
    editorHandles.delete(authority);
    entry.listeners.clear();
    entry.selectionListeners.clear();
    entry.barListeners.clear();
  }, () => request("releaseEditor", authority));
  return Object.freeze({
    ...editorFor(authority),
    onDidClose(listener: unknown) {
      expectFunction(listener, "onDidClose requires a listener");
      if (entry.closed) {
        (globalThis as unknown as { queueMicrotask(callback: () => void): void }).queueMicrotask(() => listener());
        return disposableHandle({ dispose: async () => {} });
      }
      entry.listeners.add(listener);
      return disposableHandle({ dispose: async () => { entry.listeners.delete(listener); } });
    },
    async onDidChangeSelection(listener: unknown, options?: Opts) {
      expectFunction(listener, "onDidChangeSelection requires a listener");
      const lines = linesOption(options);
      owned.assertActive();
      await registerCallable(() => entry.selectionListeners.set(listener, lines), async () => {
        entry.watching ??= request("watchEditorSelection", authority);
        try { await entry.watching; } catch (error) { entry.watching = null; throw error; }
      });
      return disposableHandle({ dispose: async () => { entry.selectionListeners.delete(listener); } });
    },
    async onDidEditBar(listener: unknown) {
      expectFunction(listener, "onDidEditBar requires a listener");
      owned.assertActive();
      await registerCallable(() => entry.barListeners.add(listener), async () => {
        entry.watchingBars ??= request("watchEditorBars", authority);
        try { await entry.watchingBars; } catch (error) { entry.watchingBars = null; throw error; }
      });
      return disposableHandle({ dispose: async () => { entry.barListeners.delete(listener); } });
    },
    [Symbol.asyncDispose]: () => owned.dispose(),
  });
}

globalThis.__knotDispatchEditorEvent = async (authority, kind) => {
  const entry = editorHandles.get(authority);
  if (!entry) return;
  if (kind === "selection") {
    return inTurn(entry, "selectionTurn", async () => {
      await whenRegistered(() => entry.selectionListeners.size > 0);
      return dispatchSelection(authority, entry);
    });
  }
  if (kind === "bars") {
    return inTurn(entry, "barsTurn", async () => {
      await whenRegistered(() => entry.barListeners.size > 0);
      return dispatchBars(authority, entry);
    });
  }
  if (kind === "closed") {
    editorHandles.delete(authority);
    entry.closed = true;
    const listeners = [...entry.listeners];
    entry.listeners.clear();
    const results = await Promise.allSettled(listeners.map((listener) => listener()));
    const failure = results.find((result) => result.status === "rejected");
    if (failure) throw failure.reason;
  }
};

async function dispatchSelection(authority: Native, entry: EditorEntry) {
  // Acknowledge first, so changes during the listeners queue one more turn.
  // The turn reads the widest window any listener asked for.
  const wanted = [...entry.selectionListeners.values()].filter((lines): lines is number => lines !== null);
  const selection = await request("acknowledgeEditorSelection", authority,
    wanted.length === 0 ? null : Math.max(...wanted));
  if (selection === null) return;
  const event = selectionFromNative(selection);
  const listeners = [...entry.selectionListeners.keys()];
  const results = await Promise.allSettled(listeners.map((listener) => listener(event)));
  const failure = results.find((result) => result.status === "rejected");
  if (failure) throw failure.reason;
}

async function dispatchBars(authority: Native, entry: EditorEntry) {
  // Acknowledge first, so edits during the listeners queue one more turn.
  const edited = await request("acknowledgeEditorBars", authority);
  const calls: Promise<unknown>[] = [];
  for (const { id, fields } of edited as Native[]) {
    const event = Object.freeze({ bar: id, fields: Object.freeze(fields) });
    for (const listener of [...entry.barListeners]) calls.push(Promise.resolve().then(() => listener(event)));
  }
  const results = await Promise.allSettled(calls);
  const failure = results.find((result) => result.status === "rejected");
  if (failure) throw failure.reason;
}

function contextualAuthority(target: unknown) {
  if (target === undefined) return null;
  const frame = currentFrame();
  const authority = targetAuthorities.get(target as object);
  if (authority === undefined || !frame?.target ||
      authority !== targetAuthorities.get(frame.target)) hostError("InvalidTarget");
  return authority;
}

export async function activeBuffer(target?: unknown) {
  const handle = await request("activeBuffer", contextualAuthority(target));
  return handle === null ? null : bufferFor(handle);
}

globalThis.__knotDispatchBufferChange = async (subscription, change) => {
  const entry = await callable(bufferChangeListeners, subscription);
  if (!entry) return;
  await inTurn(entry, "turn", () => entry.listener(Object.freeze({
    buffer: bufferFor(entry.handle),
    beforeRevision: change.beforeRevision,
    revision: change.revision,
    edits: Object.freeze(change.edits.map((edit: Native) => Object.freeze({
      range: Object.freeze({
        startByteOffset: edit.range.startByteOffset,
        endByteOffset: edit.range.endByteOffset,
      }),
      text: edit.text,
    }))),
  })));
};

export async function registerCommand(name: unknown, handler: unknown, options: unknown = {}) {
  if (typeof name !== "string" || typeof handler !== "function") {
    throw new TypeError("commands.register requires a name and handler");
  }
  expectFunction(handler, "commands.register requires a name and handler");
  if (options === null || typeof options !== "object") {
    throw new TypeError("command options must be an object");
  }
  const fields = options as Rec;
  const title = fields.title == null ? name : string(fields.title, "command title must be text");
  const description = optionalText(fields.description);
  if (fields.palette != null && typeof fields.palette !== "boolean") {
    throw new TypeError("command palette option must be a boolean");
  }
  const palette = fields.palette === false ? 0 : 1;
  const registration = await registerCallable(
    (registration: number) => commandHandlers.set(registration, handler),
    () => request("registerCommand", name, title, description, palette));
  const owned = ownership(() => commandHandlers.delete(registration),
    () => request("unregisterCommand", registration));
  return disposableHandle(owned);
}

export async function registerViewCommand(viewKind: unknown, name: unknown, handler: unknown) {
  if (typeof viewKind !== "string" || typeof name !== "string" || typeof handler !== "function") {
    throw new TypeError("commands.registerForView requires a view, command name, and handler");
  }
  expectFunction(handler, "commands.registerForView requires a view, command name, and handler");
  const registration = await registerCallable(
    (registration: number) => commandHandlers.set(registration, handler),
    () => request("registerViewCommand", viewKind, name));
  const owned = ownership(() => commandHandlers.delete(registration),
    () => request("unregisterCommand", registration));
  return disposableHandle(owned);
}

export async function selectedViewText(target: unknown) {
  return request("selectedViewText", contextualAuthority(target));
}

export async function selectedTreeItem(target: unknown) {
  return request("selectedTreeItem", contextualAuthority(target));
}

export async function writeClipboardText(text: unknown) {
  if (typeof text !== "string") {
    throw new TypeError("workbench.writeClipboardText requires text");
  }
  await request("writeClipboardText", text);
}

export function readClipboardText() {
  return request("readClipboardText");
}

function pickerItem(value: unknown) {
  expectRecord(value, "picker item must be an object");
  return {
    id: string(value.id, "picker item ID must be text"),
    label: string(value.label, "picker item label must be text"),
    description: optionalText(value.description),
    detail: optionalText(value.detail),
    hint: optionalText(value.hint),
    icon: iconRef(value.icon),
    selected: value.selected === true,
    tabs: value.tabs == null ? [] : arrayItems(value.tabs, (tab: unknown) => string(tab, "picker item tab must be text")),
  };
}

function pickerTab(value: unknown) {
  expectRecord(value, "picker tab must be an object");
  return {
    id: string(value.id, "picker tab ID must be text"),
    label: string(value.label, "picker tab label must be text"),
    placeholder: optionalText(value.placeholder),
  };
}

function inputCompletion(value: unknown) {
  expectRecord(value, "input completion must be an object");
  return {
    label: string(value.label, "input completion label must be text"),
    value: string(value.value, "input completion value must be text"),
    description: optionalText(value.description),
    hint: optionalText(value.hint),
    icon: iconRef(value.icon),
    matches: value.matches == null ? null : arrayItems(value.matches, textRange),
    descriptionMatches: value.descriptionMatches == null ? null : arrayItems(value.descriptionMatches, textRange),
  };
}

function textRange(value: unknown) {
  if (!Array.isArray(value) || value.length !== 2 || !value.every((unit) => Number.isSafeInteger(unit) && unit >= 0)) {
    throw new TypeError("input completion matches must be pairs of text offsets");
  }
  return [value[0], value[1]];
}

export async function matchCandidates(query: unknown, candidates: unknown, options?: Opts) {
  const text = string(query, "match query must be text");
  const list = arrayItems(candidates, (candidate: unknown) => string(candidate, "match candidate must be text"));
  const styles = options == null ? null : record(options, "match options must be an object").styles;
  const named = styles == null ? null : arrayItems(styles, (style: unknown) => string(style, "match style must be text"));
  const found = await request("matchCandidates", text, list, named);
  return Object.freeze(found.map((match: Native) => Object.freeze({
    index: match.index,
    ranges: Object.freeze(match.ranges.map((range: Native) => Object.freeze([range[0], range[1]]))),
  })));
}

function transientTarget(options: unknown) {
  expectRecord(options, "transient options are required");
  return optionalWindowAuthority(options.target);
}

function pickerRow(value: unknown) {
  const item = pickerItem(value);
  const fields = value as Rec;
  return {
    ...item,
    labelMatches: fields.labelMatches == null ? null : arrayItems(fields.labelMatches, textRange),
    descriptionMatches: fields.descriptionMatches == null
      ? null : arrayItems(fields.descriptionMatches, textRange),
  };
}

export async function showPicker(options: Opts) {
  const target = transientTarget(options);
  const title = string(options.title, "picker title must be text");
  const onQuery = options.onQuery;
  if (onQuery != null) {
    if (options.items != null) throw new TypeError("picker onQuery must be a function and excludes items");
    expectFunction(onQuery, "picker onQuery must be a function and excludes items");
  }
  const items = onQuery == null ? arrayItems(options.items, pickerItem) : [];
  const tabs = options.tabs == null ? [] : arrayItems(options.tabs, pickerTab);
  if (new Set(tabs.map((tab) => tab.id)).size !== tabs.length) {
    throw new TypeError("picker tab IDs must be unique");
  }
  let initialTab = 0;
  if (options.initialTab != null) {
    initialTab = tabs.findIndex((tab) => tab.id === options.initialTab);
    if (initialTab < 0) throw new TypeError("picker initialTab must name one of its tabs");
  }
  const onHighlight = options.onHighlight;
  if (onHighlight == null && onQuery == null && tabs.length === 0) {
    return request("showPicker", title, items, target);
  }
  if (onHighlight != null) expectFunction(onHighlight, "picker onHighlight must be a function");
  const result = await withSurfaceCallback(async (event: Native) => {
    if (event.kind === "query") return arrayItems(await onQuery!(event.query, event.tab), pickerRow);
    await onHighlight!(event.id);
    return null;
  }, (session: number) => request(
    "followPicker", title, items, target, session,
    onQuery == null ? 0 : 1, onHighlight == null ? 0 : 1, tabs, initialTab,
  ), (event: Native) => event.kind === "query");
  return Object.freeze(result);
}

export async function showInput(options: Opts) {
  const target = transientTarget(options);
  const title = string(options.title, "input title must be text");
  const value = optionalText(options.value);
  const prompt = optionalText(options.prompt);
  const history = historyEntries(options.history);
  const placeholder = optionalText(options.placeholder);
  const onChange = options.onChange;
  if (onChange == null) {
    return request("showInput", title, value, prompt, target, history, placeholder);
  }
  expectFunction(onChange, "input onChange must be a function");
  const result = await withSurfaceCallback(async (value: unknown) => {
    const answer = await onChange(value);
    if (typeof answer === "string") return { message: answer, items: [] };
    if (answer == null) return { message: null, items: [] };
    expectRecord(answer, "input onChange must return text, an object, or nothing");
    return {
      message: optionalText(answer.message),
      items: answer.items == null ? [] : arrayItems(answer.items, inputCompletion),
    };
  }, (session: number) => request("followInput", title, value, prompt, target, history, placeholder, session));
  return Object.freeze(result);
}

export async function showConfirmation(options: Opts) {
  const target = transientTarget(options);
  const title = string(options.title, "confirmation title must be text");
  const message = optionalText(options.message);
  const confirmLabel = optionalText(options.confirmLabel);
  const cancelLabel = optionalText(options.cancelLabel);
  return request("showConfirmation", title, message, confirmLabel, cancelLabel, target);
}

const NOTICE_SEVERITIES: readonly unknown[] = ["info", "warning", "error"];
function noticeAction(value: unknown) {
  const action = record(value, "notice action must be an object");
  const command = record(action.command, "notice action command must be an object");
  return {
    title: string(action.title, "notice action title must be text"),
    command: {
      name: string(command.name, "notice action command name must be text"),
      arguments: command.arguments === undefined ? null : validateJsonValue(command.arguments),
    },
  };
}
export async function showNotice(options: Opts) {
  const target = transientTarget(options);
  const severity = options.severity ?? "info";
  if (!NOTICE_SEVERITIES.includes(severity)) {
    throw new TypeError('notice severity must be "info", "warning", or "error"');
  }
  if (options.autoDismiss != null && typeof options.autoDismiss !== "boolean") {
    throw new TypeError("notice autoDismiss must be a boolean");
  }
  return request("showNotice", {
    message: string(options.message, "notice message must be text"),
    title: optionalText(options.title),
    severity,
    autoDismiss: options.autoDismiss ?? null,
    actions: options.actions == null ? [] : arrayItems(options.actions, noticeAction),
  }, target);
}
function noticeId(value: unknown) {
  if (!isNonNegativeInteger(value)) throw new TypeError("notice ID must be an integer");
  return value;
}
export async function markNotificationsRead(ids: unknown) {
  await request("markNotificationsRead", arrayItems(ids, noticeId));
}
export async function dismissNotification(id: unknown) {
  await request("dismissNotification", noticeId(id));
}
export async function clearNotifications() {
  await request("clearNotifications");
}

function statusItemValue(options: unknown) {
  const value = record(options, "status item options must be an object");
  const command = value.command == null ? null : record(value.command, "status item command must be an object");
  const side = value.side ?? "left";
  if (side !== "left" && side !== "right") throw new TypeError('status item side must be "left" or "right"');
  const placement = value.placement ?? "statusBar";
  if (placement !== "statusBar" && placement !== "toolbar") {
    throw new TypeError('status item placement must be "statusBar" or "toolbar"');
  }
  const priority = value.priority ?? 0;
  if (!Number.isSafeInteger(priority)) throw new TypeError("status item priority must be an integer");
  return {
    text: string(value.text, "status item text must be text"),
    icon: iconRef(value.icon),
    tooltip: optionalText(value.tooltip),
    command: command && {
      name: string(command.name, "status item command name must be text"),
      arguments: command.arguments === undefined ? null : validateJsonValue(command.arguments),
    },
    color: optionalText(value.color),
    side,
    placement,
    priority,
  };
}
export async function registerStatusItem(id: unknown, options: unknown) {
  const name = string(id, "status item id must be text");
  if (name.length === 0) throw new TypeError("status item id must be nonempty");
  const registration = await request("registerStatusItem", name, statusItemValue(options));
  const owned = ownership(() => {}, () => request("unregisterStatusItem", registration));
  return Object.freeze({
    id: name,
    async update(next: unknown, scope?: unknown) {
      owned.assertActive();
      const window = scope == null ? null : windowAuthority(record(scope, "status item scope must be an object").window);
      if (next === null && window === null) throw new TypeError("a status item needs a value");
      await owned.track(request("updateStatusItem", registration, next === null ? null : statusItemValue(next), window));
    },
    [Symbol.asyncDispose]: () => owned.dispose(),
  });
}

const MENU_LOCATIONS = ["tab", "content", "menubar.file", "menubar.edit", "menubar.view", "menubar.go"];
export async function addMenuItem(location: unknown, item: unknown) {
  if (typeof location !== "string" || !MENU_LOCATIONS.includes(location)) throw new TypeError(`unknown menu location: ${String(location)}`);
  const value = record(item, "menu item must be an object");
  const command = string(value.command, "menu item command must be text");
  if (command.length === 0) throw new TypeError("menu item command must be nonempty");
  const order = value.order ?? 0;
  if (!Number.isSafeInteger(order)) throw new TypeError("menu item order must be an integer");
  const view = optionalText(value.view);
  const context = optionalText(value.context);
  if (location.startsWith("menubar.") && (view != null || context != null)) {
    throw new TypeError("menu bar items take no view or context");
  }
  if (context != null && view == null) throw new TypeError("a menu item context needs a view");
  const registration = await request("addMenuItem", location, {
    command,
    title: optionalText(value.title),
    arguments: value.arguments === undefined ? null : validateJsonValue(value.arguments),
    group: optionalText(value.group),
    order,
    view,
    context,
  });
  const owned = ownership(() => {}, () => request("removeMenuItem", registration));
  return disposableHandle(owned);
}

export async function listCommands(view: unknown) {
  const target = view === undefined ? undefined : { target: view };
  return Object.freeze((await request("listCommands", destinationAuthority(target))).map((command: unknown) => Object.freeze(command)));
}
export async function invokeCommand(name: unknown, commandArguments: unknown) {
  if (typeof name !== "string") {
    throw new TypeError("commands.invoke requires a command name");
  }
  const arguments_ = commandArguments === undefined ? null : commandArguments;
  const normalized = validateJsonValue(arguments_);
  const dispatch = await request("invokeCommand", name, normalized);
  if (dispatch.kind === "outcome") return Object.freeze(dispatch.outcome);

  const outcome = await invokeRegisteredCommand(
    dispatch.invocation,
    dispatch.registration,
    currentFrame()?.buffer ?? null,
    normalized,
    currentFrame()?.capture ?? null,
  );
  await requestForInvocation(
    dispatch.invocation,
    "completeInlineCommand",
    dispatch.invocation,
    outcome,
  );
  return Object.freeze(outcome);
}

type Json = null | string | number | boolean | readonly Json[] | { readonly [key: string]: Json };

export function validateJsonValue(value: unknown, seen = new Set<object>()): Json {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") {
    if (Number.isFinite(value)) return value;
    throw new TypeError("JSON values require finite numbers");
  }
  if (typeof value !== "object") throw new TypeError("values must be JSON-compatible");
  if (seen.has(value)) throw new TypeError("JSON values cannot contain cycles");
  const prototype = Object.getPrototypeOf(value);
  if ((Array.isArray(value) ? prototype !== Array.prototype : prototype !== null && prototype !== Object.prototype) ||
      typeof (value as { toJSON?: unknown }).toJSON === "function" || Object.getOwnPropertySymbols(value).length !== 0) {
    throw new TypeError("values require plain JSON data");
  }
  seen.add(value);
  let copy: Json[] | Record<string, Json>;
  if (Array.isArray(value)) {
    if (Object.keys(value).some((key) => {
      const index = Number(key);
      return !Number.isInteger(index) || index < 0 || index >= value.length || String(index) !== key;
    })) throw new TypeError("JSON arrays must contain only indexed data");
    const items: Json[] = [];
    copy = items;
    Object.defineProperty(items, "toJSON", { value: undefined });
    for (let index = 0; index < value.length; index++) {
      if (!Object.hasOwn(value, index)) throw new TypeError("JSON arrays must be dense");
      items.push(validateJsonValue(value[index], seen));
    }
  } else {
    const fields: Record<string, Json> = Object.create(null);
    copy = fields;
    for (const key of Object.keys(value)) fields[key] = validateJsonValue((value as Rec)[key], seen);
  }
  seen.delete(value);
  return Object.freeze(copy);
}

export function invalidCommandArguments(message: string): never {
  const error = new Error(string(message, "invalidArguments requires a message"));
  error.name = "InvalidCommandArgumentsError";
  invalidArgumentErrors.add(error);
  throw error;
}

export function unavailableCommand(): never {
  const error = new Error("command unavailable");
  error.name = "UnavailableCommandError";
  unavailableErrors.add(error);
  throw error;
}

async function invokeRegisteredCommand(
  invocation: Native,
  registration: number,
  activeHandle: Native,
  commandArguments: unknown,
  capture: Native,
) {
  const handler = commandHandlers.get(registration)
    ?? await callable(commandHandlers, registration);
  if (!handler) {
    return {
      kind: "handlerFailure",
      message: "Knot command registration is disposed",
    };
  }
  const controller = new KnotAbortController();
  const target = capture === null ? null : Object.freeze({
    window: capture.window,
    windowTarget: windowTarget({ window: capture.window }),
    viewTarget: capture.viewTarget === null ? null : presentationTarget(capture.viewTarget),
    view: capture.view,
    tool: capture.tool,
  });
  if (target) targetAuthorities.set(target, capture.authority);
  const frame = {
    root: currentFrame()?.root ?? null,
    invocation,
    controller,
    buffer: activeHandle,
    capture,
    target,
  };
  liveFrames.set(invocation, frame);
  try {
    try {
      await withFrame(frame, () => handler(Object.freeze({
        target,
        buffer: activeHandle === null ? null : bufferFor(activeHandle),
        editor: capture?.editor ? editorFor(capture.authority) : null,
        arguments: validateJsonValue(commandArguments),
        signal: controller.signal,
      })));
      return controller.signal.aborted
        ? { kind: "cancelled" }
        : { kind: "completed" };
    } catch (error) {
      if (controller.signal.aborted) {
        return { kind: "cancelled" };
      }
      if (isObject(error) && unavailableErrors.has(error)) return { kind: "unavailable" };
      const message = errorStack(error);
      if (isObject(error) && invalidArgumentErrors.has(error)) {
        return { kind: "invalidArgument", message };
      }
      return { kind: "handlerFailure", message };
    }
  } finally {
    liveFrames.delete(invocation);
  }
}

globalThis.__knotInvokeCommand = (
  invocation,
  registration,
  activeHandle,
  commandArguments,
  capture,
) => invokeRegisteredCommand(
  invocation,
  registration,
  activeHandle,
  commandArguments,
  capture,
);

// Whether the command is running in this isolate.
globalThis.__knotCancelCommand = (invocation: Native) => {
  const frame = liveFrames.get(invocation);
  frame?.controller?.abort(new Error("command cancelled"));
  return frame !== undefined;
};

export async function registerView(contribution: unknown) {
  const value = record(contribution, "workbench.registerView requires a contribution object");
  const id = string(value.id, "view id must be text");
  if (id.length === 0 || id.startsWith("knot.") || /\s/.test(id)
      || ["editor", "terminal"].includes(id)) {
    throw new TypeError("view id must be a unique, non-reserved namespaced string");
  }
  const title = string(value.title, "view title must be text");
  // A view without content is a container: a tool window for tabs opened
  // into it.
  const content = value.content === undefined
    ? { kind: "container" }
    : record(value.content, "view content must be an object");
  let provider: { getChildren: Fn } | null = null;
  if (content.kind === "container" && value.content !== undefined) {
    throw new TypeError("view content kind must be \"tree\" or \"text\"");
  }
  if (content.kind === "tree") {
    const candidate = record(content.provider, "tree content requires a provider object");
    if (typeof candidate.getChildren !== "function") {
      throw new TypeError("tree provider requires a getChildren method");
    }
    provider = candidate as { getChildren: Fn };
  } else if (content.kind !== "text" && content.kind !== "container") {
    throw new TypeError("view content kind must be \"tree\" or \"text\"");
  }
  const newTab = optionalText(value.newTab);
  if (newTab !== null && content.kind !== "container") {
    throw new TypeError("view newTab applies only to a container");
  }
  const drop = optionalText(value.drop);
  if (drop !== null && content.kind !== "tree") {
    throw new TypeError("view drop applies only to a tree");
  }
  const actions = treeActions(value.actions);
  const language = optionalText(value.language);
  if (language !== null && content.kind !== "text") {
    throw new TypeError("view language applies only to text content");
  }
  let gutter = false;
  if (value.gutter !== undefined) {
    if (typeof value.gutter !== "boolean") {
      throw new TypeError("view gutter must be a boolean");
    }
    if (content.kind !== "text") {
      throw new TypeError("view gutter applies only to text content");
    }
    gutter = value.gutter;
  }
  let location = "leftTop";
  let order = null;
  let visible = true;
  if (value.placement !== undefined) {
    const placement = record(value.placement, "view placement must be an object");
    if (placement.location !== undefined) {
      location = string(placement.location, "view placement location must be text");
      if (
        !["leftTop", "leftBottom", "rightTop", "rightBottom", "bottomLeft", "bottomRight"].includes(
          location,
        )
      ) {
        throw new TypeError("view placement location is invalid");
      }
    }
    if (placement.order !== undefined) {
      order = safeInteger(placement.order);
    }
    if (placement.visible !== undefined) {
      if (typeof placement.visible !== "boolean") {
        throw new TypeError("view visible must be a boolean");
      }
      visible = placement.visible;
    }
  }
  const registration = await registerCallable(
    (registration: number) => { if (provider !== null) treeProviders.set(registration, provider); },
    () => request(
    "registerView",
    id,
    title,
    location,
    order,
    visible ? 1 : 0,
    content.kind,
    actions,
    language,
    gutter ? 1 : 0,
    newTab,
    drop,
  ));
  const owned = ownership(() => treeProviders.delete(registration),
    () => request("unregisterView", registration));
  return Object.freeze({
    id,
    async show(window: unknown) {
      owned.assertActive();
      await owned.track(request("showView", registration, optionalWindowAuthority(window)));
    },
    async hide(window: unknown) {
      owned.assertActive();
      await owned.track(request("hideView", registration, optionalWindowAuthority(window)));
    },
    async focus(window: unknown) {
      owned.assertActive();
      await owned.track(request("focusView", registration, optionalWindowAuthority(window)));
    },
    async toggle(window: unknown) {
      owned.assertActive();
      await owned.track(request("toggleView", registration, optionalWindowAuthority(window)));
    },
    async setBadge(badge: unknown) {
      owned.assertActive();
      if (badge !== null && badge !== "info" && badge !== "error") {
        throw new TypeError("view badge must be info, error, or null");
      }
      await owned.track(request("setViewBadge", registration, badge));
    },
    async invalidate(parentId: unknown, window: unknown) {
      owned.assertActive();
      const parent = optionalText(parentId);
      await owned.track(request(
        "invalidateView",
        registration,
        parent,
        optionalWindowAuthority(window),
      ));
    },
    async reveal(idPath: unknown, window: unknown) {
      owned.assertActive();
      const path = arrayItems(idPath, (id: unknown) => string(id, "reveal IDs must be text"));
      await owned.track(request("revealViewItem", registration, path, optionalWindowAuthority(window)));
    },
    async editLabel(id: unknown, options?: Opts) {
      owned.assertActive();
      return owned.track(editTree(registration, false, string(id, "tree item ID must be text"), options));
    },
    async editNewItem(parentId: unknown, options?: Opts) {
      owned.assertActive();
      return owned.track(editTree(registration, true, string(parentId, "tree item ID must be text"), options));
    },
    async setText(text: unknown, window: unknown) {
      owned.assertActive();
      await owned.track(request(
        "replaceViewText",
        registration,
        string(text, "view text must be text"),
        optionalWindowAuthority(window),
      ));
    },
    [Symbol.asyncDispose]: () => owned.dispose(),
  });
}

// An inline edit passes each value to `validate`, whose message is shown
// under the field.
async function editTree(registration: number, newRow: unknown, id: unknown, options?: Opts) {
  const fields = options == null ? {} : record(options, "edit options must be an object");
  const value = optionalText(fields.value) ?? "";
  const selection = fields.selection == null ? null : textRange(fields.selection);
  const validate = fields.validate;
  if (validate != null && typeof validate !== "function") {
    throw new TypeError("edit validate must be a function");
  }
  const edit = (session: number | null) => request(
    "editTreeItem", registration, newRow ? 1 : 0, id, value, selection,
    iconRef(fields.icon), session, null,
  );
  const result = validate == null ? await edit(null) : await withSurfaceCallback(async (current: unknown) => {
    const message = await validate(current);
    return { message: message == null ? null : string(message, "edit validate must return text or nothing") };
  }, edit);
  return Object.freeze(result);
}

globalThis.__knotRequestTreeChildren = async (registration, window, instance, parentId, generation) => {
  const provider = treeProviders.get(registration)
    ?? await callable(treeProviders, registration);
  if (!provider) {
    return { error: "tree data provider is disposed" };
  }
  try {
    return { items: iterableItems(await provider.getChildren(parentId, Object.freeze({
      window: windowTarget({ window }),
      instance: string(instance, "view instance must be text"),
    })), treeItem) };
  } catch (error) {
    return { error: errorStack(error) };
  }
};

globalThis.__knotCallProvider = async (registration: number, method: string, argument: string, window: Native) => {
  const implementation = providerImplementations.get(registration)
    ?? await callable(providerImplementations, registration);
  if (!implementation) return { error: "the provider is disposed" };
  try {
    const context = Object.freeze({ window: window === null ? null : windowTarget({ window }) });
    if (!isMethod(implementation, method)) throw new TypeError(`provider has no method ${method}`);
    const handler = implementation[method] as Fn;
    const result = await handler.call(implementation, JSON.parse(argument), context);
    return { result: JSON.stringify(validateJsonValue(result === undefined ? null : result)) };
  } catch (error) {
    return { error: errorMessage(error) };
  }
};

// The "knot/contract" module of one owner, the package whose modules import
// it: the shared builder, whose contracts carry the owner and bind through
// the host.
export function contractModule(owner: string): Pick<typeof DeclaredContract, "s" | "defineContract"> {
  const names = new Set<string>();
  function defineContract<const M extends DeclaredContract.ContractMethods>(
    name: string,
    methods: M,
  ): DeclaredContract.Contract<M> {
    const contract = defineSchemaContract(name, methods);
    if (names.has(contract.name)) throw new TypeError(`contract ${contract.name} is defined twice in ${owner}`);
    names.add(contract.name);
    const descriptor = JSON.stringify(contract.methods);
    return Object.freeze({
      name: contract.name,
      methods: contract.methods,
      serve: (implementation: unknown) => serveContract(owner, contract.name, descriptor, contract.methods, implementation),
      client: () => contractClient(owner, contract.name, descriptor, contract.methods),
    }) as unknown as DeclaredContract.Contract<M>;
  }
  return Object.freeze({ s, defineContract });
}

// The contracts Knot itself interprets, owned by `knot`. Any package serves
// them; native code asks every registration (completion) or the latest one
// (paste) and validates each result against the descriptor served here, which
// a provider cannot alter.
const knotContracts = contractModule("knot");
const { s: schema } = knotContracts;

export const completionContract = knotContracts.defineContract("completion", {
  provideCompletions: schema.fn(
    schema.object({
      document: schema.string(),
      revision: schema.int(),
      cursorByteOffset: schema.int(),
      prefix: schema.string(),
    }),
    schema.array(schema.object({ label: schema.string(), insertText: schema.string() })),
  ),
});

const pasteContracts = new Map<string, unknown>();
export function pasteContract(languageId: unknown) {
  if (typeof languageId !== "string" || languageId === "") {
    throw new TypeError("pasteContract requires a language ID");
  }
  let contract = pasteContracts.get(languageId);
  if (contract === undefined) {
    contract = knotContracts.defineContract(`paste.${languageId}`, {
      providePaste: schema.fn(
        schema.object({
          document: schema.string(),
          revision: schema.int(),
          text: schema.string(),
          range: schema.tuple([schema.int(), schema.int()]),
          selectedText: schema.string(),
        }),
        schema.nullable(schema.string()),
      ),
    });
    pasteContracts.set(languageId, contract);
  }
  return contract;
}

// A method the implementation defines, own or from its class, not one every
// object inherits such as `toString`.
function isMethod(implementation: Rec, method: string) {
  const handler = implementation[method];
  return typeof handler === "function" &&
    !(method in Object.prototype && handler === (Object.prototype as Rec)[method]);
}

// A served contract dispatches like a provider: the host validates each call
// and runs the method through `__knotCallProvider`.
async function serveContract(
  owner: string, name: string, descriptor: string, methods: DeclaredContract.ContractMethods, implementation: unknown,
) {
  if (implementation === null || typeof implementation !== "object") {
    throw new TypeError(`serve requires an object with the methods of ${name}`);
  }
  for (const method of Object.keys(methods)) {
    if (!isMethod(implementation as Rec, method)) {
      throw new TypeError(`serve requires a ${method} method for ${name}`);
    }
  }
  const registration = await registerCallable(
    (registration: number) => providerImplementations.set(registration, implementation as Rec),
    () => request("serveContract", owner, name, descriptor));
  const owned = ownership(() => providerImplementations.delete(registration),
    () => request("unregisterProvider", registration));
  return disposableHandle(owned);
}

// A client calls the registration the host bound it to; each call carries the
// window of the command it runs in.
async function contractClient(owner: string, name: string, descriptor: string, methods: DeclaredContract.ContractMethods) {
  const client = await request("contractClient", owner, name, descriptor);
  const handle: Record<string, (...arguments_: unknown[]) => Promise<unknown>> = Object.create(null);
  for (const [method, schema] of Object.entries(methods)) {
    const returnsVoid = schema.result.kind === "void";
    handle[method] = async (...arguments_: unknown[]) => {
      if (arguments_.length > 1) throw new TypeError("contract methods take at most one argument");
      const argument = JSON.stringify(validateJsonValue(arguments_[0] ?? null));
      const window = currentFrame()?.capture?.window ?? null;
      const result = JSON.parse(await request("callContract", client, method, argument, window));
      return returnsVoid ? undefined : result;
    };
  }
  return Object.freeze(handle);
}

const liveWindows = new WeakMap<object, Native>();
const livePanes = new WeakMap<object, Native>();
const livePresentations = new WeakMap<object, Native>();
const windowTargets = new Map<Native, WindowTarget>();
const panesByKey = new Map<string, PaneTarget>();
const presentationsByKey = new Map<string, ViewTarget>();

function windowTarget(value: Native): WindowTarget {
  let target = windowTargets.get(value.window);
  if (!target) {
    // The brand in the declaration is type-level only; a target is a bare frozen object.
    target = Object.freeze({}) as WindowTarget;
    liveWindows.set(target, value);
    windowTargets.set(value.window, target);
  }
  return target;
}
function paneTarget(value: Native): PaneTarget {
  const key = `${value.window.window}/${value.workbench}/${value.pane}`;
  let target = panesByKey.get(key);
  if (!target) {
    target = Object.freeze({ window: windowTarget(value.window) }) as PaneTarget;
    livePanes.set(target, value);
    panesByKey.set(key, target);
  }
  return target;
}
function presentationTarget(value: Native): ViewTarget {
  const key = `${value.pane.window.window}/${value.pane.workbench}/${value.pane.pane}/${value.tab}/${value.presentation}`;
  let target = presentationsByKey.get(key);
  if (!target) {
    const pane = paneTarget(value.pane);
    target = Object.freeze({ window: pane.window, pane }) as ViewTarget;
    livePresentations.set(target, value);
    presentationsByKey.set(key, target);
  }
  return target;
}
function windowAuthority(target: unknown) {
  const value = isObject(target) ? liveWindows.get(target) : undefined;
  if (!value) hostError("InvalidTarget");
  return value.window;
}
function optionalWindowAuthority(target: unknown) {
  return target == null ? null : windowAuthority(target);
}
function documentFromNative(value: Native) {
  return Object.freeze({ ...value, buffer: bufferFor(value.buffer) });
}
function workspaceFromNative(value: Native) {
  return Object.freeze({ window: windowTarget(value.window), generation: value.generation,
    roots: Object.freeze(value.roots.map((root: Native) => Object.freeze(root))) });
}
function windowFromNative(value: Native) {
  return Object.freeze({ target: windowTarget(value.target),
    currentTarget: value.currentTarget === null ? null : presentationTarget(value.currentTarget),
    statusTarget: value.statusTarget === null ? null : presentationTarget(value.statusTarget),
    workspace: workspaceFromNative(value.workspace),
    panes: Object.freeze(value.panes.map((pane: Native) => Object.freeze({
      target: paneTarget(pane.target), selected: presentationTarget(pane.selected),
      tabs: Object.freeze(pane.tabs.map(presentationFromNative)),
    }))),
  });
}
function presentationFromNative(tab: Native) {
  return Object.freeze({
    target: presentationTarget(tab.target), kind: tab.kind, document: tab.document,
    buffer: tab.buffer === null ? null : bufferFor(tab.buffer), tool: tab.tool,
    directory: tab.directory,
  });
}
function destinationAuthority(options?: unknown) {
  if (options === undefined) return null;
  const target = record(options, "destination options must be an object").target;
  if (target === undefined) return null;
  if (!isObject(target)) hostError("InvalidTarget");
  const window = liveWindows.get(target);
  if (window) return { kind: "window", window: window.window };
  const pane = livePanes.get(target);
  if (pane) return { kind: "pane", window: pane.window.window, workbench: pane.workbench, pane: pane.pane };
  const view = livePresentations.get(target);
  if (view) return { kind: "view", window: view.pane.window.window, workbench: view.pane.workbench,
    pane: view.pane.pane, tab: view.tab, presentation: view.presentation };
  hostError("InvalidTarget");
}
function documentPresentationFromNative(value: Native) {
  const presentation = value.presentation;
  return Object.freeze({ document: documentFromNative(value.document),
    presentation: Object.freeze({ target: presentationTarget(presentation.target), kind: presentation.kind, tool: null, directory: null,
      document: presentation.document, buffer: bufferFor(presentation.buffer) }) });
}
export async function closePresentation(target: unknown, options?: Opts) {
  if (!isLive(livePresentations, target)) hostError("InvalidTarget");
  const discard = options === undefined ? undefined : record(options, "close options must be an object").discard;
  if (discard !== undefined && typeof discard !== "boolean") throw new TypeError("discard must be a boolean");
  return Object.freeze(await request("closePresentation", destinationAuthority({ target }), discard ?? null));
}
export async function createUntitled(options?: Opts) {
  return documentPresentationFromNative(await request("createUntitled", destinationAuthority(options)));
}
export async function presentDocument(id: unknown, options?: Opts) {
  return documentPresentationFromNative(await request("presentDocument", string(id, "document identity must be text"), destinationAuthority(options)));
}
export async function openDocument(uri: unknown, options?: Opts) {
  const preview = options === undefined ? undefined : record(options, "destination options must be an object").preview;
  if (preview !== undefined && typeof preview !== "boolean") throw new TypeError("preview must be a boolean");
  const focus = options === undefined ? undefined : options.focus;
  if (focus !== undefined && typeof focus !== "boolean") throw new TypeError("focus must be a boolean");
  const value = await request("openDocument", string(uri, "resource URI must be text"), destinationAuthority(options),
    preview ?? null, focus ?? null);
  return documentPresentationFromNative(value);
}

export async function openTerminal(options?: Opts) {
  const fields = options === undefined ? {} : record(options, "terminal options must be an object");
  const { cwd, location } = fields;
  if (location !== undefined && (typeof location !== "string" || location.length === 0)) {
    throw new TypeError("terminal location must be \"editor\" or a container view ID");
  }
  const tab = await request("openTerminal", cwd === undefined ? null : string(cwd, "terminal directory must be text"),
    destinationAuthority(options), location ?? null);
  return presentationFromNative(tab);
}

export async function reloadDocument(id: unknown) {
  await request("reloadDocument", string(id, "document identity must be text"));
}
export async function saveDocument(id: unknown, options?: Opts) {
  const uri = options === undefined ? undefined : record(options, "save options must be an object").uri;
  await request("saveDocument", string(id, "document identity must be text"),
    uri === undefined ? null : string(uri, "resource URI must be text"));
}
export async function setDocumentLanguage(id: unknown, language: unknown) {
  await request("setDocumentLanguage", string(id, "document identity must be text"),
    optionalText(language));
}

function textList(value: unknown, message: string) {
  return value === undefined ? [] : arrayItems(value, (item: unknown) => string(item, message));
}
function languageId(value: unknown) {
  const id = string(value, "language id must be text");
  if (id.length === 0 || /\s/.test(id)) {
    throw new TypeError("language id must be nonempty and contain no whitespace");
  }
  return id;
}
function languagePatterns(value: Rec) {
  const fileExtensions = textList(value.fileExtensions, "file extensions must be text");
  if (fileExtensions.some((extension) => extension.length === 0 || extension.startsWith("."))) {
    throw new TypeError("file extensions must be nonempty and omit the leading dot");
  }
  const fileNames = textList(value.fileNames, "file names must be text");
  const interpreters = textList(value.interpreters, "interpreters must be text");
  return [fileExtensions, fileNames, interpreters];
}
export async function associateLanguage(id: unknown, patterns: unknown) {
  const language = languageId(id);
  const claimed = languagePatterns(record(patterns, "languages.associate requires patterns"));
  const association = await request("associateLanguage", language, ...claimed);
  const owned = ownership(() => {}, () => request("unregisterLanguage", association));
  return disposableHandle(owned);
}
function editingToken(value: unknown, message: string) {
  const token = string(value, message);
  if (token.length === 0 || token.trim() !== token) {
    throw new TypeError("comment and bracket tokens must be nonempty without surrounding whitespace");
  }
  return token;
}
function bracketPair(value: unknown) {
  expectRecord(value, "brackets must be objects");
  const pair: {
    open: string; close: string; autoClose?: boolean; newline?: boolean; notIn?: ("string" | "comment")[];
  } = {
    open: editingToken(value.open, "bracket open must be text"),
    close: editingToken(value.close, "bracket close must be text"),
  };
  for (const key of ["autoClose", "newline"] as const) {
    const flagValue = value[key];
    if (flagValue === undefined) continue;
    if (typeof flagValue !== "boolean") throw new TypeError(`bracket ${key} must be a boolean`);
    pair[key] = flagValue;
  }
  if (value.notIn !== undefined) {
    pair.notIn = arrayItems(value.notIn, (family: unknown): "string" | "comment" => {
      if (family !== "string" && family !== "comment") {
        throw new TypeError('bracket notIn entries must be "string" or "comment"');
      }
      return family;
    });
  }
  return pair;
}
// Absent fields stay absent, so languages.get returns them as registered.
function languageEditing(value: Rec) {
  const editing: {
    comments?: { line?: string[]; block?: string[] };
    brackets?: ReturnType<typeof bracketPair>[];
    autoCloseBefore?: string;
  } = {};
  if (value.comments !== undefined) {
    const comments = record(value.comments, "language comments must be an object");
    const target: { line?: string[]; block?: string[] } = {};
    editing.comments = target;
    if (comments.line !== undefined) {
      target.line = arrayItems(comments.line,
        (item: unknown) => editingToken(item, "line comment tokens must be text"));
    }
    if (comments.block !== undefined) {
      const block = arrayItems(comments.block,
        (item: unknown) => editingToken(item, "block comment tokens must be text"));
      if (block.length !== 2) throw new TypeError("a block comment must be [open, close]");
      target.block = block;
    }
  }
  if (value.brackets !== undefined) editing.brackets = arrayItems(value.brackets, bracketPair);
  if (value.autoCloseBefore !== undefined) {
    editing.autoCloseBefore = string(value.autoCloseBefore, "autoCloseBefore must be text");
  }
  return editing;
}
export async function registerLanguage(definition: unknown) {
  const value = record(definition, "languages.register requires a language object");
  const id = languageId(value.id);
  const name = string(value.name, "language name must be text");
  const grammar = string(value.grammar, "language grammar must be text");
  const [fileExtensions, fileNames, interpreters] = languagePatterns(value);
  const queries = record(value.queries, "language queries must be an object");
  const highlights = string(queries.highlights, "highlights query must be text");
  const injections = queries.injections === undefined
    ? ""
    : string(queries.injections, "injections query must be text");
  const indents = queries.indents === undefined
    ? ""
    : string(queries.indents, "indents query must be text");
  const folds = queries.folds === undefined
    ? ""
    : string(queries.folds, "folds query must be text");
  const editing = languageEditing(value);
  const registration = await request("registerLanguage", id, name, grammar, fileExtensions,
    fileNames, interpreters, highlights, injections, indents, folds, editing);
  const owned = ownership(() => {}, () => request("unregisterLanguage", registration));
  return disposableHandle(owned);
}
const settingTypes = {
  boolean: (value: unknown) => typeof value === "boolean",
  integer: Number.isSafeInteger,
  number: Number.isFinite,
  string: (value: unknown) => typeof value === "string",
  strings: (value: unknown) => Array.isArray(value) && value.every((item) => typeof item === "string"),
};
const settingListeners = new Map<number, Fn>();
const watchListeners = new Map<number, Fn>();
function settingKey(value: unknown) {
  if (typeof value !== "string" || value.length === 0) throw new TypeError("setting key must be nonempty text");
  return value;
}
function settingsScope(value: unknown) {
  if (value == null) return [null, null, null];
  expectRecord(value, "settings scope must be an object");
  return [optionalText(value.language), optionalText(value.view), optionalText(value.document)];
}
export async function defineSetting(definition: unknown) {
  const value = record(definition, "settings.define requires a definition object");
  const key = settingKey(value.key);
  const type = string(value.type, "setting type must be text");
  if (!Object.hasOwn(settingTypes, type)) {
    throw new TypeError("setting type must be boolean, integer, number, string, or strings");
  }
  if (!settingTypes[type as keyof typeof settingTypes](value.default)) throw new TypeError(`setting default must be of type ${type}`);
  const description = string(value.description, "setting description must be text");
  await request("defineSetting", key, type, value.default, description);
}
export async function setSettings(keyOrValues: unknown, valueOrScope?: unknown, scope?: unknown) {
  let values;
  if (typeof keyOrValues === "string") {
    values = { [settingKey(keyOrValues)]: valueOrScope };
  } else {
    values = record(keyOrValues, "settings.set requires a key or an object of values");
    scope = valueOrScope;
    Object.keys(values).forEach(settingKey);
  }
  const converted = Object.create(null);
  for (const key of Object.keys(values)) {
    if (values[key] === undefined) throw new TypeError("setting values must be JSON values");
    converted[key] = validateJsonValue(values[key]);
  }
  await request("setSettings", converted, ...settingsScope(scope));
}
export async function getSetting(key: unknown, facts: unknown) {
  const value = await request("getSetting", settingKey(key), ...settingsScope(facts));
  return value === null ? undefined : (validateJsonValue(value) as Native);
}
export async function resetSetting(key: unknown, scope: unknown) {
  await request("resetSetting", settingKey(key), ...settingsScope(scope));
}
export async function observeSetting(key: unknown, listener: unknown) {
  settingKey(key);
  expectFunction(listener, "settings.observe requires a listener");
  const subscription = await registerCallable(
    (subscription: number) => settingListeners.set(subscription, listener),
    () => request("observeSetting", key));
  const owned = ownership(() => settingListeners.delete(subscription),
    () => request("unobserveSetting", subscription));
  return disposableHandle(owned);
}
globalThis.__knotDispatchSettingChange = async (subscription, key) => {
  const listener = await callable(settingListeners, subscription);
  if (listener) await listener(key);
};
function deepFreeze<T>(value: T): T {
  if (value !== null && typeof value === "object") {
    Object.values(value).forEach(deepFreeze);
    Object.freeze(value);
  }
  return value;
}
export async function getLanguage(id: unknown) {
  return deepFreeze(await request("getLanguage", string(id, "language id must be text")));
}
export async function listLanguages() {
  return Object.freeze((await request("listLanguages")).map((language: Native) => Object.freeze({
    ...language,
    fileExtensions: Object.freeze(language.fileExtensions),
    fileNames: Object.freeze(language.fileNames),
    interpreters: Object.freeze(language.interpreters),
  })));
}
const hexColor = /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
function themeColor(value: unknown, name: unknown) {
  if (typeof value !== "string" || !hexColor.test(value)) {
    throw new TypeError(`theme color ${name} must be #rrggbb or #rrggbbaa`);
  }
  return value;
}
function syntaxStyle(value: unknown, capture: string) {
  expectRecord(value, `syntax style ${capture} must be an object`);
  const style: {
    color?: string; fontStyle?: "normal" | "italic"; fontWeight?: number;
    underline?: boolean; strikethrough?: boolean;
  } = {};
  for (const [field, entry] of Object.entries(value)) {
    if (entry === undefined) continue;
    if (field === "color") style.color = themeColor(entry, capture);
    else if (field === "fontStyle") {
      if (entry !== "normal" && entry !== "italic") {
        throw new TypeError(`syntax style ${capture} fontStyle must be "normal" or "italic"`);
      }
      style.fontStyle = entry;
    } else if (field === "fontWeight") {
      if (typeof entry !== "number" || !Number.isInteger(entry) || entry < 100 || entry > 900) {
        throw new TypeError(`syntax style ${capture} fontWeight must be an integer from 100 to 900`);
      }
      style.fontWeight = entry;
    } else if (field === "underline" || field === "strikethrough") {
      if (typeof entry !== "boolean") throw new TypeError(`syntax style ${capture} ${field} must be boolean`);
      style[field] = entry;
    } else {
      throw new TypeError(`syntax style ${capture} has unknown field ${field}`);
    }
  }
  return style;
}
export async function registerTheme(definition: unknown) {
  const value = record(definition, "themes.register requires a theme object");
  const name = string(value.name, "theme name must be text");
  if (name.length === 0) throw new TypeError("theme name must be nonempty");
  if (value.appearance !== "dark" && value.appearance !== "light") {
    throw new TypeError('theme appearance must be "dark" or "light"');
  }
  const colors: Record<string, string> = {};
  for (const [key, color] of Object.entries(record(value.colors ?? {}, "theme colors must be an object"))) {
    if (color !== undefined) colors[key] = themeColor(color, key);
  }
  const syntax: Record<string, ReturnType<typeof syntaxStyle>> = {};
  for (const [capture, style] of Object.entries(record(value.syntax ?? {}, "theme syntax must be an object"))) {
    if (style !== undefined) syntax[capture] = syntaxStyle(style, capture);
  }
  const chrome: Record<string, number> = {};
  for (const [key, entry] of Object.entries(record(value.chrome ?? {}, "theme chrome must be an object"))) {
    if (entry === undefined) continue;
    if (typeof entry !== "number" || !Number.isFinite(entry)) {
      throw new TypeError(`theme chrome ${key} must be a number`);
    }
    chrome[key] = entry;
  }
  const registration = await request("registerTheme",
    { name, appearance: value.appearance, colors, syntax, chrome });
  const owned = ownership(() => {}, () => request("unregisterTheme", registration));
  return disposableHandle(owned);
}
export async function getTheme(name: unknown) {
  const theme = await request("getTheme", string(name, "theme name must be text"));
  if (theme === null) return null;
  const syntax = Object.fromEntries(Object.entries(theme.syntax)
    .map(([capture, style]) => [capture, Object.freeze(style)]));
  return Object.freeze({ ...theme, colors: Object.freeze(theme.colors), syntax: Object.freeze(syntax) });
}
export async function listThemes() {
  return Object.freeze((await request("listThemes")).map((theme: Native) => Object.freeze(theme)));
}
export async function defineColor(definition: unknown) {
  const value = record(definition, "themes.defineColor requires a definition object");
  const key = string(value.key, "color key must be text");
  if (key.length === 0) throw new TypeError("color key must be nonempty");
  const description = string(value.description, "color description must be text");
  const defaults = record(value.defaults, "color defaults must be an object");
  const fallback = (appearance: "dark" | "light") => {
    const entry = string(defaults[appearance], `color default ${appearance} must be text`);
    if (entry.length === 0 || (entry.startsWith("#") && !hexColor.test(entry))) {
      throw new TypeError(`color default ${appearance} must be #rrggbb, #rrggbbaa, or a color key`);
    }
    return entry;
  };
  await request("defineColor", key, description, fallback("dark"), fallback("light"));
}
function iconSpec(value: unknown, name: unknown) {
  const spec = record(value, `icon ${name} must be a definition object`);
  return Object.fromEntries(Object.entries(spec).filter(([, entry]) => entry !== undefined));
}
export async function defineIcon(name: unknown, definition: unknown) {
  const key = string(name, "icon name must be text");
  await request("defineIcon", key, iconSpec(definition, key));
}
export async function overrideIcon(name: unknown, definition: unknown) {
  const key = string(name, "icon name must be text");
  const registration = await request("registerIconOverride", key, iconSpec(definition, key));
  const owned = ownership(() => {}, () => request("unregisterIconOverride", registration));
  return disposableHandle(owned);
}
export async function associateIcons(patterns: unknown) {
  const value = record(patterns, "icons.associate requires patterns");
  const claims = ["files", "extensions", "folders"].map((field) => {
    const entries = record(value[field] ?? {}, `icon associations of ${field} must be an object`);
    for (const name of Object.values(entries)) string(name, "associated icon names must be text");
    return entries;
  });
  const registration = await request("registerIconAssociation", ...claims);
  const owned = ownership(() => {}, () => request("unregisterIconAssociation", registration));
  return disposableHandle(owned);
}
export async function registerIconPack(definition: unknown) {
  const value = record(definition, "icons.registerPack requires a pack object");
  const name = string(value.name, "icon pack name must be text");
  const icons: Record<string, ReturnType<typeof iconSpec>> = {};
  for (const [key, spec] of Object.entries(record(value.icons, "icon pack icons must be an object"))) {
    if (spec !== undefined) icons[key] = iconSpec(spec, key);
  }
  const registration = await request("registerIconPack", name, icons);
  const owned = ownership(() => {}, () => request("unregisterIconPack", registration));
  return disposableHandle(owned);
}
export async function listIconPacks() {
  return Object.freeze(await request("listIconPacks"));
}
function presetCommand(command: unknown) {
  if (command === null) return null;
  if (typeof command === "string") return { name: command, arguments: null };
  const value = record(command, "keymap preset command must be a name, a command object, or null");
  return {
    name: string(value.name, "keymap preset command name must be text"),
    arguments: value.arguments === undefined ? null : validateJsonValue(value.arguments),
  };
}
export async function registerKeymapPreset(definition: unknown) {
  const value = record(definition, "keymaps.registerPreset requires a preset object");
  const name = string(value.name, "keymap preset name must be text");
  const title = value.title === undefined ? name : string(value.title, "keymap preset title must be text");
  if (!Array.isArray(value.bindings)) throw new TypeError("keymap preset bindings must be an array");
  const bindings = value.bindings.map((binding: unknown) => {
    const entry = record(binding, "keymap preset binding must be an object");
    if (entry.command === undefined) throw new TypeError("keymap preset binding requires a command or null");
    const view = entry.view ?? null;
    if (view !== null) string(view, "keymap preset binding view must be text");
    return {
      key: string(entry.key, "keymap preset binding key must be text"),
      command: presetCommand(entry.command),
      view,
    };
  });
  const registration = await request("registerKeymapPreset", { name, title, bindings });
  const owned = ownership(() => {}, () => request("unregisterKeymapPreset", registration));
  return disposableHandle(owned);
}
export async function getKeymapPreset(name: unknown) {
  const preset = await request("getKeymapPreset", string(name, "keymap preset name must be text"));
  if (preset === null) return null;
  const bindings = preset.bindings.map((binding: Native) => Object.freeze({
    ...binding,
    command: binding.command === null ? null : Object.freeze(binding.command),
  }));
  return Object.freeze({ ...preset, bindings: Object.freeze(bindings) });
}
export async function listKeymapPresets() {
  return Object.freeze((await request("listKeymapPresets")).map((preset: Native) => Object.freeze(preset)));
}
export async function listGrammars() {
  return Object.freeze(await request("listGrammars"));
}

export async function listDocuments() {
  return Object.freeze((await request("listDocuments")).map(documentFromNative));
}
export async function getDocument(id: unknown) {
  const value = await request("getDocument", string(id, "document identity must be text"));
  return value === null ? null : documentFromNative(value);
}
export async function listWindows() {
  return Object.freeze((await request("listWindows")).map(windowFromNative));
}
export async function lastFocusedDocument(target: unknown) {
  const value = await request("getWindow", windowAuthority(target));
  if (value === null || value.lastFocusedDocument === null) return null;
  return getDocument(value.lastFocusedDocument);
}
export async function getWindow(target: unknown) {
  const value = await request("getWindow", windowAuthority(target));
  return value === null ? null : windowFromNative(value);
}
function workspaceDestination(options?: Opts) {
  if (options === undefined) return null;
  const target = record(options, "workspace options must be an object").target;
  return target === undefined ? null : windowAuthority(target);
}
export async function openFolder(uri: unknown, options?: Opts) {
  return workspaceFromNative(await request("openFolder", string(uri, "resource URI must be text"), workspaceDestination(options)));
}
export async function replaceWorkspaceRoots(roots: unknown, options?: Opts) {
  const uris = arrayItems(roots, (uri: unknown) => string(uri, "workspace root URI must be text"));
  return workspaceFromNative(await request("replaceWorkspaceRoots", uris, workspaceDestination(options)));
}
export async function pickFolder(options?: Opts) {
  const value = await request("pickFolder", workspaceDestination(options));
  return Object.freeze(value.kind === "selected"
    ? { kind: "selected", uri: value.uri }
    : { kind: "dismissed" });
}
export function readFileText(uri: unknown) {
  return request("readFileText", string(uri, "resource URI must be text"));
}
export async function listFiles(uri: unknown) {
  const entries = await request("listFiles", string(uri, "resource URI must be text"));
  return Object.freeze(entries.map((entry: Native) => Object.freeze({ uri: entry.uri, name: entry.name, kind: entry.kind, ignored: entry.ignored })));
}
export async function searchFiles(uri: unknown, query: unknown, options?: Opts) {
  const limit = options == null ? null : record(options, "search options must be an object").limit ?? null;
  if (limit !== null && (typeof limit !== "number" || !Number.isInteger(limit) || limit < 1)) throw new TypeError("search limit must be a positive integer");
  const directories = typeof uri === "string" ? [uri] : arrayItems(uri, (item: unknown) => string(item, "resource URI must be text"));
  const found = await request("searchFiles", directories, string(query, "search query must be text"), limit);
  return Object.freeze({
    results: Object.freeze(found.results.map((hit: Native) => Object.freeze({
      uri: hit.uri,
      directory: hit.directory,
      path: hit.path,
      name: hit.name,
      ranges: Object.freeze(hit.ranges.map((range: Native) => Object.freeze([range[0], range[1]]))),
    }))),
    more: found.more,
  });
}
export function isFileIgnored(uri: unknown) {
  return request("isFileIgnored", string(uri, "resource URI must be text"));
}
export async function watchFiles(uri: unknown, listener: unknown, options?: Opts) {
  string(uri, "resource URI must be text");
  expectFunction(listener, "files.watch requires a listener");
  const recursive = options === undefined
    ? false
    : record(options, "watch options must be an object").recursive ?? false;
  if (typeof recursive !== "boolean") throw new TypeError("watch recursive must be a boolean");
  const subscription = await registerCallable(
    (subscription: number) => watchListeners.set(subscription, listener),
    () => request("watchFiles", uri, recursive));
  const owned = ownership(() => watchListeners.delete(subscription),
    () => request("unwatchFiles", subscription));
  return disposableHandle(owned);
}
globalThis.__knotDispatchWatchChange = async (subscription, uris) => {
  const listener = await callable(watchListeners, subscription);
  if (listener) await listener(Object.freeze([...uris]));
};
const processSignals = new Set(["SIGHUP", "SIGINT", "SIGQUIT", "SIGKILL", "SIGTERM", "SIGUSR1", "SIGUSR2"]);
function processArguments(options: Opts, allowStdin: unknown) {
  const value = record(options, "process options must be an object");
  const command = string(value.command, "process command must be text");
  const args = value.args === undefined
    ? []
    : arrayItems(value.args, (argument: unknown) => string(argument, "process arguments must be text"));
  const cwd = value.cwd === undefined ? null : string(value.cwd, "process cwd must be a file URI");
  const env = [];
  const unset = [];
  if (value.env !== undefined) {
    for (const [name, entry] of Object.entries(record(value.env, "process env must be an object"))) {
      if (name === "" || name.includes("=")) throw new TypeError("process env names must be nonempty and without =");
      if (entry === null) unset.push(name);
      else env.push(`${name}=${string(entry, "process env values must be text or null")}`);
    }
  }
  let stdin = null;
  if (allowStdin && value.stdin !== undefined) stdin = string(value.stdin, "process stdin must be text");
  return [command, args, cwd, env, unset, stdin];
}
export async function runProcess(options: Opts) {
  const result = await request("runProcess", ...processArguments(options, true));
  return Object.freeze({
    exitCode: result.exitCode,
    signal: result.signal,
    stdout: result.stdout,
    stderr: result.stderr,
  });
}
// A stream reads its next chunk only after every listener finished with the
// previous one, so a slow listener stalls the child on its pipe. Output that
// arrives while no listener is attached waits in the application.
function processStream(id: unknown, stream: string) {
  const listeners = new Set<Fn>();
  let running = false;
  let finish!: () => void;
  const ended = new Promise<void>((resolve) => { finish = resolve; });
  const pump = () => withFrame(null, async () => {
    while (listeners.size > 0) {
      let chunk;
      try {
        chunk = await request("readProcessOutput", id, stream);
      } catch (error) {
        // The handle is gone: its process finished and drained, or its
        // lifecycle ended. Either way the stream is over.
        if (errorCode(error) === "InvalidTarget") { finish(); break; }
        throw error;
      }
      if (chunk === null) { finish(); break; }
      for (const listener of [...listeners]) {
        try { await listener(chunk); } catch (error) { void Promise.reject(error); }
      }
    }
    running = false;
  });
  const listen = (listener: unknown) => {
    expectFunction(listener, "process stream requires a listener");
    listeners.add(listener);
    if (!running) {
      running = true;
      pump();
    }
    return disposableHandle({ dispose: () => { listeners.delete(listener); return Promise.resolve(); } });
  };
  return { listen, ended };
}
export async function spawnProcess(options: Opts) {
  const id = await request("spawnProcess", ...processArguments(options, false));
  const exited = withFrame(null, () => request("waitProcess", id))
    .then((status: Native) => Object.freeze({ exitCode: status.exitCode, signal: status.signal }));
  exited.catch(() => {});
  const stdout = processStream(id, "stdout");
  const stderr = processStream(id, "stderr");
  return Object.freeze({
    onStdout: stdout.listen,
    onStderr: stderr.listen,
    stdoutEnded: stdout.ended,
    stderrEnded: stderr.ended,
    write: (text: unknown) => request("writeProcessInput", id, string(text, "process input must be text")),
    closeStdin: () => request("closeProcessInput", id),
    kill: (signal: unknown) => {
      if (signal !== undefined && !processSignals.has(signal as string)) throw new TypeError("unsupported process signal");
      return request("killProcess", id, signal ?? null);
    },
    [Symbol.asyncDispose]: () => request("disposeProcess", id),
    exited,
  });
}
export async function statFile(uri: unknown) {
  const kind = await request("statFile", string(uri, "resource URI must be text"));
  return kind === null ? null : Object.freeze({ kind });
}
export function createFile(uri: unknown) {
  return request("createFile", string(uri, "resource URI must be text"));
}
export function writeFileText(uri: unknown, text: unknown) {
  return request("writeFileText", string(uri, "resource URI must be text"),
    string(text, "file text must be text"));
}
export function createDirectory(uri: unknown) {
  return request("createDirectory", string(uri, "resource URI must be text"));
}
export function renameFile(from: unknown, to: unknown) {
  return request("renameFile",
    string(from, "resource URI must be text"), string(to, "resource URI must be text"));
}
export function copyFile(from: unknown, to: unknown) {
  return request("copyFile",
    string(from, "resource URI must be text"), string(to, "resource URI must be text"));
}
export async function revealFile(uri: unknown) {
  await request("revealFile", string(uri, "resource URI must be text"));
}
export async function deleteFile(uri: unknown, options?: Opts) {
  const permanent = options === undefined
    ? false
    : record(options, "delete options must be an object").permanent ?? false;
  if (typeof permanent !== "boolean") throw new TypeError("delete permanent must be a boolean");
  await request("deleteFile", string(uri, "resource URI must be text"), permanent);
}
export async function openWindow(options?: Opts) {
  const roots = options === undefined
    ? undefined
    : record(options, "window options must be an object").roots;
  const uris = roots === undefined ? [] : arrayItems(roots, (uri: unknown) => string(uri, "workspace root URI must be text"));
  return windowFromNative(await request("openWindow", uris));
}
export async function focusWindow(target: unknown) {
  await request("focusWindow", windowAuthority(target));
}
function sessionEntry(value: unknown) {
  expectRecord(value, "session entry must be an object");
  const entry = {
    uri: optionalText(value.uri),
    document: optionalText(value.document),
    capture: optionalText(value.capture),
  };
  if (entry.uri === null && entry.document === null) {
    throw new TypeError("session entry requires a uri or document");
  }
  return entry;
}
function optionalIndex(value: unknown) {
  return value == null ? null : safeInteger(value);
}
export async function replaceWindows(options: Opts) {
  expectRecord(options, "replaceWindows requires options");
  const suspend = arrayItems(options.suspend, (value: unknown) => {
    expectRecord(value, "suspended document must be an object");
    return {
      document: string(value.document, "document identity must be text"),
      capture: optionalText(value.capture),
    };
  });
  const windows = arrayItems(options.windows, (value: unknown) => {
    expectRecord(value, "destination window must be an object");
    return {
      roots: arrayItems(value.roots, (uri: unknown) => string(uri, "workspace root URI must be text")),
      entries: arrayItems(value.entries, sessionEntry),
      selected: optionalIndex(value.selected),
      layout: value.layout == null ? null : validateJsonValue(value.layout),
    };
  });
  const release = options.release == null ? [] : arrayItems(options.release, windowAuthority);
  const value = await request("replaceWindows", {
    suspend, windows, release, focus: optionalIndex(options.focus),
  });
  return Object.freeze({
    protected: value.protected,
    windows: Object.freeze(value.windows.map((window: Native) => Object.freeze({
      window: windowTarget(window.window),
      entries: Object.freeze(window.entries.map((entry: Native) => Object.freeze(entry))),
      layout: window.layout,
    }))),
  });
}
export async function captureLayout(target: unknown) {
  return validateJsonValue(await request("captureLayout", windowAuthority(target))) as Native;
}
export async function getWorkspace(target: unknown) {
  const value = await request("getWorkspace", windowAuthority(target));
  return value === null ? null : workspaceFromNative(value);
}

const applicationListeners = new Map<number, Fn>();
function applicationSnapshotFromNative(value: Native) {
  return Object.freeze({
    documents: Object.freeze(value.documents.map(documentFromNative)),
    windows: Object.freeze(value.windows.map(windowFromNative)),
    activeWindow: value.activeWindow === null ? null : windowTarget(value.activeWindow),
    activeTarget: value.activeTarget === null ? null : presentationTarget(value.activeTarget),
    notifications: noticesFromNative(value.notifications),
    launched: value.launched,
  });
}
function noticesFromNative(notices: Native) {
  return Object.freeze(notices.map((notice: Native) => Object.freeze({
    ...notice,
    actions: Object.freeze(notice.actions.map((action: Native) => Object.freeze(action))),
    target: notice.target === null ? null : windowTarget(notice.target),
  })));
}
export async function observeWorkbench(listener: unknown) {
  expectFunction(listener, "observe requires a listener");
  const value = await registerCallable(
    (value: Native) => applicationListeners.set(value.subscription, listener),
    () => request("observeWorkbench"));
  const owned = ownership(() => applicationListeners.delete(value.subscription),
    () => request("unobserveWorkbench", value.subscription));
  return Object.freeze({ snapshot: applicationSnapshotFromNative(value.snapshot), watermark: value.watermark,
    [Symbol.asyncDispose]: () => owned.dispose() });
}
globalThis.__knotDispatchApplicationChange = async (subscription, value) => {
  const listener = await callable(applicationListeners, subscription);
  if (!listener) return;
  let payload;
  switch (value.type) {
    case "documentOpened": case "documentChanged": payload = { document: documentFromNative(value.document) }; break;
    case "documentMoved": payload = { document: documentFromNative(value.document), from: value.from }; break;
    case "documentClosed": payload = { document: value.document }; break;
    case "windowOpened": case "presentationsChanged": payload = { window: windowFromNative(value.window) }; break;
    case "windowClosed": case "viewStateChanged": payload = { window: windowTarget(value.window) }; break;
    case "launched": payload = {}; break;
    case "notificationsChanged": payload = { notifications: noticesFromNative(value.notifications) }; break;
    case "workspaceChanged": payload = { workspace: workspaceFromNative(value.workspace) }; break;
    case "activeTargetChanged": payload = {
      activeWindow: value.activeWindow === null ? null : windowTarget(value.activeWindow),
      activeTarget: value.activeTarget === null ? null : presentationTarget(value.activeTarget),
    }; break;
    default: throw new Error("unknown application event");
  }
  await listener(Object.freeze({ type: value.type, sequence: value.sequence, ...payload }));
};

export async function storageRead(key: unknown) {
  const result = await request("storageRead", string(key, "storage key must be text"));
  return Object.freeze(result.found ? { found: true, value: validateJsonValue(result.value) } : { found: false });
}
export async function storageWrite(key: unknown, value: unknown) {
  await request("storageWrite", string(key, "storage key must be text"), validateJsonValue(value));
}
export async function storageDelete(key: unknown) {
  await request("storageDelete", string(key, "storage key must be text"));
}
export async function storageList() {
  return Object.freeze(await request("storageList"));
}

const HISTORY_LIMIT = 50;
function historyKey(name: unknown) {
  return `history/${string(name, "history name must be text")}`;
}
async function readHistory(name: unknown) {
  const stored = await storageRead(historyKey(name));
  return stored.found && Array.isArray(stored.value) ? stored.value.filter((id) => typeof id === "string") : [];
}
export async function historyRecent(name: unknown) {
  return Object.freeze(await readHistory(name));
}
export async function historyRecord(name: unknown, id: unknown) {
  const recorded = string(id, "history ID must be text");
  const ids = await readHistory(name);
  await storageWrite(historyKey(name), [recorded, ...ids.filter((other) => other !== recorded)].slice(0, HISTORY_LIMIT));
}
export async function historyForget(name: unknown, id: unknown) {
  if (id === undefined) {
    await storageDelete(historyKey(name));
    return;
  }
  const forgotten = string(id, "history ID must be text");
  const ids = await readHistory(name);
  if (ids.includes(forgotten)) await storageWrite(historyKey(name), ids.filter((other) => other !== forgotten));
}
export async function historyOrder(name: unknown, items: unknown, key: unknown = (item: Rec) => item.id) {
  if (!Array.isArray(items)) throw new TypeError("history items must be an array");
  expectFunction(key, "history key must be a function");
  const places = new Map((await readHistory(name)).map((id, place) => [id, place]));
  const recent: [number, unknown][] = [];
  const rest: unknown[] = [];
  for (const item of items) {
    const place = places.get(key(item) as string);
    if (place === undefined) rest.push(item);
    else recent.push([place, item]);
  }
  recent.sort((left, right) => left[0] - right[0]);
  return Object.freeze([...recent.map(([, item]) => item), ...rest]);
}

function captureFromNative(value: Native) {
  return Object.freeze(value);
}
export async function recoveryList() {
  return Object.freeze((await request("recoveryList")).map(captureFromNative));
}
export async function recoveryRetain(document: unknown) {
  return captureFromNative(await request("recoveryRetain", string(document, "document identity must be text")));
}
export async function recoveryRelease(id: unknown) {
  return await request("recoveryRelease", string(id, "capture identity must be text"));
}
export async function recoveryRestore(id: unknown, options?: Opts) {
  const value = await request("recoveryRestore", string(id, "capture identity must be text"), destinationAuthority(options));
  const presentation = documentPresentationFromNative(value.presentation);
  return Object.freeze(value.outcome === "restored"
    ? { outcome: "restored", resource: value.resource, ...presentation }
    : { outcome: "preserved", owner: value.owner, ...presentation });
}
