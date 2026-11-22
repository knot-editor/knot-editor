// The "knot/contract" module: the schema builder `s` and `defineContract`.
// A schema value is its own descriptor, frozen plain data the host compiles
// and validates calls against; nothing here validates values.

const schemas = new WeakSet();

function schema(descriptor) {
  const value = Object.freeze(descriptor);
  schemas.add(value);
  return value;
}

function nested(value, where) {
  if (!schemas.has(value) || value.kind === "optional" || value.kind === "void" || value.kind === "fn") {
    throw new TypeError(`${where} requires a value schema`);
  }
  return value;
}

function scalar(value) {
  return value === null || typeof value === "string" || typeof value === "boolean" ||
    (typeof value === "number" && Number.isFinite(value));
}

function plainObject(value, where) {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${where} requires an object`);
  }
  return value;
}

function list(value, where) {
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
  literal(value) {
    if (!scalar(value)) throw new TypeError("s.literal requires a string, finite number, boolean, or null");
    return schema({ kind: "literal", value });
  },
  enum(values) {
    list(values, "s.enum");
    if (values.length === 0 || values.some((value) => typeof value !== "string") ||
        new Set(values).size !== values.length) {
      throw new TypeError("s.enum requires distinct strings");
    }
    return schema({ kind: "enum", values: Object.freeze([...values]) });
  },
  array: (items) => schema({ kind: "array", items: nested(items, "s.array") }),
  tuple(items) {
    list(items, "s.tuple");
    return schema({ kind: "tuple", items: Object.freeze(items.map((item) => nested(item, "s.tuple"))) });
  },
  record: (values) => schema({ kind: "record", values: nested(values, "s.record") }),
  object(fields) {
    plainObject(fields, "s.object");
    const copy = Object.create(null);
    for (const [name, field] of Object.entries(fields)) {
      copy[name] = schemas.has(field) && field.kind === "optional" ? field : nested(field, "s.object");
    }
    return schema({ kind: "object", fields: Object.freeze(copy) });
  },
  optional: (value) => schema({ kind: "optional", schema: nested(value, "s.optional") }),
  union(options) {
    list(options, "s.union");
    if (options.length === 0) throw new TypeError("s.union requires at least one option");
    return schema({ kind: "union", options: Object.freeze(options.map((option) => nested(option, "s.union"))) });
  },
  nullable: (value) => schema({ kind: "union", options: Object.freeze([nested(value, "s.nullable"), null_]) }),
  fn(argument, result) {
    const part = (value, where) => (value === void_ ? value : nested(value, where));
    return schema({ kind: "fn", argument: part(argument, "s.fn argument"), result: part(result, "s.fn result") });
  },
});

export function defineContract(name, methods) {
  if (typeof name !== "string" || name === "") throw new TypeError("defineContract requires a contract name");
  plainObject(methods, "defineContract");
  const copy = Object.create(null);
  for (const [method, value] of Object.entries(methods)) {
    if (!schemas.has(value) || value.kind !== "fn") {
      throw new TypeError(`defineContract method ${method} requires s.fn`);
    }
    copy[method] = value;
  }
  if (Object.keys(copy).length === 0) throw new TypeError("defineContract requires at least one method");
  return Object.freeze({ name, methods: Object.freeze(copy) });
}
