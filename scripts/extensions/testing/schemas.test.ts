import assert from "node:assert/strict";
import { test } from "node:test";
import { defineContract, s } from "knot/contract";

test("schemas are frozen plain descriptors", () => {
  const row = s.object({ label: s.string(), detail: s.optional(s.nullable(s.int())) });
  assert.deepEqual(JSON.parse(JSON.stringify(row)), {
    kind: "object",
    fields: {
      label: { kind: "string" },
      detail: { kind: "optional", schema: { kind: "union", options: [{ kind: "int" }, { kind: "null" }] } },
    },
  });
  assert.ok(Object.isFrozen(row) && Object.isFrozen((row as any).fields));
  assert.deepEqual(JSON.parse(JSON.stringify(s.tuple([s.literal(1), s.enum(["a", "b"])]))), {
    kind: "tuple",
    items: [{ kind: "literal", value: 1 }, { kind: "enum", values: ["a", "b"] }],
  });
  assert.deepEqual(JSON.parse(JSON.stringify(s.record(s.array(s.json())))), {
    kind: "record",
    values: { kind: "array", items: { kind: "json" } },
  });
});

test("a contract carries its name and method descriptors", () => {
  const contract = defineContract("rows", {
    files: s.fn(s.string(), s.array(s.boolean())),
    refresh: s.fn(s.void(), s.void()),
  });
  assert.equal(contract.name, "rows");
  assert.deepEqual(JSON.parse(JSON.stringify(contract.methods)), {
    files: { kind: "fn", argument: { kind: "string" }, result: { kind: "array", items: { kind: "boolean" } } },
    refresh: { kind: "fn", argument: { kind: "void" }, result: { kind: "void" } },
  });
  assert.ok(Object.isFrozen(contract) && Object.isFrozen(contract.methods));
});

test("a field named __proto__ stays a field", () => {
  const fields = JSON.parse('{"__proto__": {"kind": "string"}}');
  assert.throws(() => s.object(fields), TypeError, "a parsed descriptor is not a builder schema");
  const object = s.object({ ["__proto__"]: s.string() }) as any;
  assert.deepEqual(Object.keys(object.fields), ["__proto__"]);
});

test("the builder rejects values that are not schemas of the expected kind", () => {
  const cases: [() => unknown, string][] = [
    [() => s.array({ kind: "string" } as any), "s.array requires a value schema"],
    [() => s.array(s.optional(s.string()) as any), "s.array requires a value schema"],
    [() => s.union([s.void() as any]), "s.union requires a value schema"],
    [() => s.union([] as any), "s.union requires at least one option"],
    [() => s.optional(s.optional(s.int()) as any), "s.optional requires a value schema"],
    [() => s.fn(s.fn(s.void(), s.void()) as any, s.void()), "s.fn argument requires a value schema"],
    [() => s.literal([] as any), "s.literal requires a string, finite number, boolean, or null"],
    [() => s.literal(Number.NaN), "s.literal requires a string, finite number, boolean, or null"],
    [() => s.enum(["a", "a"]), "s.enum requires distinct strings"],
    [() => s.enum([] as any), "s.enum requires distinct strings"],
    [() => s.object([] as any), "s.object requires an object"],
    [() => defineContract("", { m: s.fn(s.void(), s.void()) }), "defineContract requires a contract name"],
    [() => defineContract("c", {}), "defineContract requires at least one method"],
    [() => defineContract("c", { m: s.string() as any }), "defineContract method m requires s.fn"],
  ];
  for (const [build, message] of cases) assert.throws(build, { name: "TypeError", message });
});
