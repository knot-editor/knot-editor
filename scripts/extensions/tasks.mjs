// Extension tooling tasks. Every directory with a tsconfig.json under
// extensions/@knot is a TypeScript package.
//
//   node tasks.mjs check          type-check the host modules, the contract fixtures, every package, and the tests
//   node tasks.mjs test           compile packages, the host modules, and their tests, then run node:test
//   node tasks.mjs build <dir>    emit one package's dist/ in place
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const tools = dirname(fileURLToPath(import.meta.url));
const root = resolve(tools, "../..");
const tsc = join(tools, "node_modules/typescript/bin/tsc");

function packages() {
  return ["extensions"].flatMap((base) =>
    readdirSync(join(root, base), { withFileTypes: true })
      .filter((scope) => scope.isDirectory())
      .flatMap((scope) =>
        readdirSync(join(root, base, scope.name)).map((name) => join(root, base, scope.name, name)),
      )
      .filter((directory) => existsSync(join(directory, "tsconfig.json"))),
  );
}

function node(...args) {
  nodeWithin(undefined, ...args);
}

// A run that outlives `timeout` milliseconds is killed and fails.
function nodeWithin(timeout, ...args) {
  try {
    execFileSync(process.execPath, args, { stdio: "inherit", timeout });
  } catch (error) {
    if (error.code === "ETIMEDOUT") console.error(`stopped after ${timeout / 1000} s`);
    process.exit(error.status ?? 1);
  }
}

function compile(...args) {
  node(tsc, ...args);
}

function testFiles(directory) {
  return readdirSync(directory, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".test.js"))
    .map((entry) => join(entry.parentPath, entry.name));
}

// The host's own modules: strict TypeScript compiled by knot-host's build
// script, with unit tests next to the crate.
const hostSources = join(root, "crates/knot-host/src/js");

// Host sources never write `any`: the one place for it is the private native
// bindings declaration, where a value from the Rust side is named `Native`.
function checkNoAny() {
  const offenders = [];
  for (const name of readdirSync(hostSources)) {
    if (!name.endsWith(".ts") || name.endsWith(".d.ts")) continue;
    const code = readFileSync(join(hostSources, name), "utf8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/^\s*\/\/.*$/gm, "");
    code.split("\n").forEach((line, index) => {
      if (/\bany\b/.test(line.replace(/(["'`])(?:\\.|(?!\1).)*\1/g, ""))) offenders.push(`${name}:${index + 1}: ${line.trim()}`);
    });
  }
  if (offenders.length > 0) {
    console.error(`host sources must not use \`any\` (use unknown, or Native for host values):\n${offenders.join("\n")}`);
    process.exit(1);
  }
}

const [task, argument] = process.argv.slice(2);
if (task === "check") {
  checkNoAny();
  compile("-p", join(hostSources, "tsconfig.json"), "--noEmit");
  compile("-p", join(tools, "tsconfig.json"));
  compile("-p", join(tools, "tsconfig.test.json"), "--noEmit");
  for (const directory of packages()) compile("-p", join(directory, "tsconfig.json"), "--noEmit");
} else if (task === "test") {
  const build = join(tools, ".test-build");
  rmSync(build, { recursive: true, force: true });
  compile("-p", join(tools, "tsconfig.test.json"));
  const files = ["extensions", "scripts/extensions/testing", "crates/knot-host/js-tests"].flatMap((base) =>
    existsSync(join(build, base)) ? testFiles(join(build, base)) : [],
  );
  // A test that never settles fails by name after ten seconds; the whole run,
  // which takes about fifteen, is cut off after two minutes.
  nodeWithin(
    120_000,
    "--import",
    pathToFileURL(join(tools, "testing/register.mjs")).href,
    "--test",
    "--test-timeout=10000",
    ...files,
  );
} else if (task === "build" && argument !== undefined) {
  compile("-p", join(resolve(process.env.INIT_CWD ?? process.cwd(), argument), "tsconfig.json"));
} else {
  console.error("usage: node tasks.mjs check | test | build <package directory>");
  process.exit(2);
}
