// Loaded with `node --import` before tests: resolves "knot" to the fake host's
// API module, "knot/testing" to its controls, and "knot/testing/languages" to
// the shared language-package checks, "knot/contract" to the fake's contracts
// over the host's own schema builder, and "@knot/go-to-file" to that package's
// contract module, all from the compiled tests.
import { registerHooks } from "node:module";
import { pathToFileURL } from "node:url";
import { join } from "node:path";

const buildRoot = join(import.meta.dirname, "../.test-build");
const build = join(buildRoot, "scripts/extensions/testing");
const modules = {
  knot: pathToFileURL(join(build, "knot.js")).href,
  "knot/testing": pathToFileURL(join(build, "host.js")).href,
  "knot/testing/languages": pathToFileURL(join(build, "languages.js")).href,
  "knot/contract": pathToFileURL(join(build, "contract.js")).href,
  "@knot/go-to-file": pathToFileURL(join(buildRoot, "extensions/@knot/go-to-file/src/contract.js")).href,
  "@knot/vcs": pathToFileURL(join(buildRoot, "extensions/@knot/vcs/src/contract.js")).href,
};

registerHooks({
  resolve(specifier, context, next) {
    const url = modules[specifier];
    return url === undefined ? next(specifier, context) : { url, shortCircuit: true };
  },
});
