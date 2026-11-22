// The schema builder `s` and `defineContract`. A schema value is its own
// descriptor, frozen plain data the host compiles and validates calls
// against; nothing here validates values. Extensions import it through the
// bootstrap's `contractModule`, which qualifies each contract with the
// package defining it and adds `serve` and `client`.

import type * as Declared from "knot/contract";

type Scalar = string | number | boolean | null;
type ValueDescriptor =
  | { readonly kind: "string" | "int" | "number" | "boolean" | "null" | "json" }
  | { readonly kind: "literal"; readonly value: Scalar }
  | { readonly kind: "enum"; readonly values: readonly string[] }
  | { readonly kind: "array"; readonly items: ValueDescriptor }
  | { readonly kind: "tuple"; readonly items: readonly ValueDescriptor[] }
  | { readonly kind: "record"; readonly values: ValueDescriptor }
  | { readonly kind: "object"; readonly fields: Readonly<Record<string, ValueDescriptor | OptionalDescriptor>> }
  | { readonly kind: "union"; readonly options: readonly ValueDescriptor[] };
type OptionalDescriptor = { readonly kind: "optional"; readonly schema: ValueDescriptor };
type VoidDescriptor = { readonly kind: "void" };
type FnDescriptor = {
  readonly kind: "fn";
  readonly argument: ValueDescriptor | VoidDescriptor;
  readonly result: ValueDescriptor | VoidDescriptor;
};
type Descriptor = ValueDescriptor | OptionalDescriptor | VoidDescriptor | FnDescriptor;

const schemas = new WeakSet<object>();

function schema<D extends Descriptor>(descriptor: D): D {
  const value = Object.freeze(descriptor);
  schemas.add(value);
  return value;
}

function descriptorOf(value: unknown): Descriptor | undefined {
  return typeof value === "object" && value !== null && schemas.has(value) ? (value as Descriptor) : undefined;
}

function nested(value: unknown, where: string): ValueDescriptor {
  const descriptor = descriptorOf(value);
  if (descriptor === undefined || descriptor.kind === "optional" || descriptor.kind === "void" ||
      descriptor.kind === "fn") {
    throw new TypeError(`${where} requires a value schema`);
  }
  return descriptor;
}

function scalar(value: unknown): value is Scalar {
  return value === null || typeof value === "string" || typeof value === "boolean" ||
    (typeof value === "number" && Number.isFinite(value));
}

function plainObject(value: unknown, where: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${where} requires an object`);
  }
  return value as Record<string, unknown>;
}

function list(value: unknown, where: string): readonly unknown[] {
  if (!Array.isArray(value)) throw new TypeError(`${where} requires an array`);
  return value;
}

const string = schema({ kind: "string" });
const int = schema({ kind: "int" });
const number = schema({ kind: "number" });
const boolean = schema({ kind: "boolean" });
const null_ = schema({ kind: "null" });
const json = schema({ kind: "json" });
const void_ = schema({ kind: "void" });

export const s = Object.freeze({
  string: () => string,
  int: () => int,
  number: () => number,
  boolean: () => boolean,
  null: () => null_,
  json: () => json,
  void: () => void_,
  literal(value: unknown) {
    if (!scalar(value)) throw new TypeError("s.literal requires a string, finite number, boolean, or null");
    return schema({ kind: "literal", value });
  },
  enum(input: unknown) {
    const values = list(input, "s.enum");
    if (values.length === 0 || values.some((value) => typeof value !== "string") ||
        new Set(values).size !== values.length) {
      throw new TypeError("s.enum requires distinct strings");
    }
    return schema({ kind: "enum", values: Object.freeze([...values] as string[]) });
  },
  array: (items: unknown) => schema({ kind: "array", items: nested(items, "s.array") }),
  tuple(input: unknown) {
    const items = list(input, "s.tuple");
    return schema({ kind: "tuple", items: Object.freeze(items.map((item) => nested(item, "s.tuple"))) });
  },
  record: (values: unknown) => schema({ kind: "record", values: nested(values, "s.record") }),
  object(input: unknown) {
    const fields = plainObject(input, "s.object");
    const copy: Record<string, ValueDescriptor | OptionalDescriptor> = Object.create(null);
    for (const [name, field] of Object.entries(fields)) {
      const descriptor = descriptorOf(field);
      copy[name] = descriptor?.kind === "optional" ? descriptor : nested(field, "s.object");
    }
    return schema({ kind: "object", fields: Object.freeze(copy) });
  },
  optional: (value: unknown) => schema({ kind: "optional", schema: nested(value, "s.optional") }),
  union(input: unknown) {
    const options = list(input, "s.union");
    if (options.length === 0) throw new TypeError("s.union requires at least one option");
    return schema({ kind: "union", options: Object.freeze(options.map((option) => nested(option, "s.union"))) });
  },
  nullable: (value: unknown) =>
    schema({ kind: "union", options: Object.freeze([nested(value, "s.nullable"), null_]) }),
  fn(argument: unknown, result: unknown) {
    const part = (value: unknown, where: string) => (value === void_ ? void_ : nested(value, where));
    return schema({ kind: "fn", argument: part(argument, "s.fn argument"), result: part(result, "s.fn result") });
  },
}) satisfies typeof Declared.s;

export function defineContract<const M extends Declared.ContractMethods>(
  name: string,
  methods: M,
): Pick<Declared.Contract<M>, "name" | "methods"> {
  if (typeof name !== "string" || name === "") throw new TypeError("defineContract requires a contract name");
  const input = plainObject(methods, "defineContract");
  const copy: Record<string, FnDescriptor> = Object.create(null);
  for (const [method, value] of Object.entries(input)) {
    const descriptor = descriptorOf(value);
    if (descriptor?.kind !== "fn") throw new TypeError(`defineContract method ${method} requires s.fn`);
    copy[method] = descriptor;
  }
  if (Object.keys(copy).length === 0) throw new TypeError("defineContract requires at least one method");
  return Object.freeze({ name, methods: Object.freeze(copy) as unknown as M });
}

// Fails the build when this module's exports and the public declaration
// diverge in either direction.
// (Values of a symbol type are the declaration's private brands.)
type ExportNames<T> = { [K in keyof T]: T[K] extends symbol ? never : K }[keyof T];
type Matches<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
const exportsMatch: Matches<ExportNames<typeof import("./contract")>, ExportNames<typeof Declared>> = true;
void exportsMatch;
