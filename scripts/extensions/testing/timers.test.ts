import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";

// Package code calls the timer globals knot.d.ts declares; under node:test
// they are Node's own, which mock.timers drives without waiting. Its mocked
// timeouts ignore refresh(), so tests of debouncing belong in product tests.
beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "setInterval"] }));
afterEach(() => mock.timers.reset());

test("a timeout passes its arguments once its delay has passed", () => {
  const fired: string[] = [];
  const timeout: NodeJS.Timeout = setTimeout((label: string) => fired.push(label), 75, "fired");
  mock.timers.tick(74);
  assert.deepEqual(fired, []);
  mock.timers.tick(1);
  assert.deepEqual(fired, ["fired"]);
  clearTimeout(timeout);
});

test("an interval ticks until cleared", () => {
  let ticks = 0;
  const interval = setInterval(() => ticks++, 10);
  mock.timers.tick(30);
  clearInterval(interval);
  mock.timers.tick(30);
  assert.equal(ticks, 3);
});
