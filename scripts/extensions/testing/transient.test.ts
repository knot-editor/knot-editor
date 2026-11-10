import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { workbench } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("an input reports its initial and typed values and shows the returned messages", async () => {
  const window = host.openWindow();
  host.type("12", "x");
  host.answer("showInput", { kind: "accepted", value: "x" });
  const seen: string[] = [];
  const result = await workbench.showInput({
    title: "Line",
    value: "1",
    target: window,
    onChange: (value) => {
      seen.push(value);
      return value === "x" ? "not a number" : undefined;
    },
  });
  assert.deepEqual(result, { kind: "accepted", value: "x" });
  assert.deepEqual(seen, ["1", "12", "x"]);
  assert.deepEqual(host.inputMessages(), [null, null, "not a number"]);
});

test("an input records the completions an object answer carries", async () => {
  const window = host.openWindow();
  host.type("b");
  host.answer("showInput", { kind: "accepted", value: "b" });
  await workbench.showInput({
    title: "Word",
    target: window,
    onChange: (value) => ({
      message: value === "" ? null : "matches",
      items: ["alpha", "beta"]
        .filter((word) => word.startsWith(value))
        .map((word) => ({ label: word, value: word, detail: "word" })),
    }),
  });
  assert.deepEqual(host.inputMessages(), [null, "matches"]);
  assert.deepEqual(host.inputFeedback(), [
    [
      { label: "alpha", value: "alpha", detail: "word" },
      { label: "beta", value: "beta", detail: "word" },
    ],
    [{ label: "beta", value: "beta", detail: "word" }],
  ]);
});
