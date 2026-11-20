# Writing extensions for Knot

The [TypeScript declarations](../../crates/knot-host/src/js/knot.d.ts) are the API version 1
reference for parameters, defaults, errors, completion, and lifetime rules.
This guide covers setup and common workflows. Knot runs JavaScript; TypeScript
is the recommended authoring language and compiles before installation.

## Topics

- [Commands, keys, and menus](commands-and-keys.md): register commands, bind keys and modal contexts, compose commands, add menu items.
- [Tool windows, prompts, and status](ui.md): tool windows and trees, transient prompts, editor bars, status items, notices.
- [Languages, themes, and icons](languages-and-themes.md): register languages, themes and color keys, icons.
- [Settings](settings.md): read and write settings.
- [Sharing between packages](packages.md): providers (including the names Knot asks), identity and lifetime, native contributors.
- [Application state, files, and storage](application.md): inspect and change the application, read and change files, cleanup and failures, durable storage, document captures, project sessions.

## Package and check your code

Install a directory containing `knot.jsonc` and built `.js`/`.mjs` sources under
the platform's local application-data `Knot/extensions` root. For example:

```json
{
  "name": "@example/tools",
  "apiVersion": 1,
  "version": "1.0.0",
  "main": "dist/main.js"
}
```

Use the [manifest schema](../schemas/knot.schema.json) for tooling and the
[bundled packages](../../extensions/@knot/) for directory layout. The host
accepts API version 1 before evaluation; package version and dependency ordering
are separate. Static relative imports stay inside captured package sources;
dynamic imports reject. Startup diagnostics report rejected or failed packages
while independent packages continue.

A package that needs a capability beyond the editor API lists it in
`"entitlements"`; the only one is `"process"`, which lets the package run
programs. When Knot starts and an installed package has an entitlement the user
has not decided on, because the package is new or an update added it, Knot asks
before loading any package. A package the user denied does not load, and its
startup report entry says why. Entitlements are consent, not a sandbox: a
package runs with the whole public API. Bundled packages declare them the same
way, and shipping in Knot grants them.

Personal config has no manifest. Its optional `pre-init.js` and `post-init.js`
entries share one lifecycle around installed-package startup; config failure
stops product startup. Directory selection and loading are described in
[host startup](../architecture/extension-host.md#package-admission-and-startup).

Run `scripts/check-extensions.sh` with Node/npm to check declarations and every
TypeScript package with the lockfile-pinned compiler, then run package tests. The
[checked examples](../../scripts/extensions/examples/) cover commands, providers,
cleanup, errors, and keybindings. This check runs in CI; Rust/V8 tests separately
validate runtime behavior.

## Write and test in TypeScript

A TypeScript package keeps `src/*.ts`, `test/*.test.ts`, and a `tsconfig.json`
beside its manifest; Knot loads only the emitted `dist/` modules, one per source
file. The bundled [project registry](../../extensions/@knot/projects/) shows the layout.
In this repository, packages extend the shared
[base configuration](../../scripts/extensions/tsconfig.base.json), which sees only
ES2022 and the `knot` declarations. Import sibling modules by their emitted
name, such as `./model.js`. `npm --prefix scripts/extensions run build <dir>`
emits one package's `dist/`; Cargo builds bundled packages itself.

`console.debug`, `log`, `info`, `warn`, and `error` write to the
[application log](../development.md#application-log) at the matching level (`log`
is `info`), with the target `knot::extension` and the package name as a
`package` field (`personal-config` for your config). Run Knot's **Open Log**
command to read them, or start Knot from a terminal to see them on standard
error. Use them to inspect events while writing a package.

Node's timer globals work as in Node: `setTimeout`, `setInterval`,
`setImmediate`, their `clear*` counterparts, and `Timeout.refresh()`, which
suits debouncing. Each callback runs as its own turn behind work already queued,
so a delay is a minimum, and unloading the package cancels its timers. Package
tests get Node's own timers, which `mock.timers` drives, though its mocked
timeouts ignore `refresh()`.

Keep policy in plain functions over plain data and keep mutable state out of
module scope: export `start()` to create state and register contributions, and
make `main.ts` only `await start()`. Tests then exercise policy directly and run
whole flows from a fresh start.

`npm --prefix scripts/extensions test` compiles every package's sources and tests
and runs them with `node:test`. Under test, `"knot"` resolves to an
[in-memory host](../../scripts/extensions/testing/host.ts) typed against the
declarations, and tests import its controls from `"knot/testing"`: arrange
windows, documents, and records; queue prompt answers; run commands and emit
application events; make the next call to an operation fail with a host error;
and inspect records, captures, messages, and views. Each test file runs in its
own process; call `reset()` before each test. For files, tests seed and remove
them with `seedFile` and `removeFile`, whose changes call watch listeners
immediately and uncoalesced, and list reveals with `revealed()`; the fake
matches `files.hidden` and `files.ignored` as plain names, reads no ignore
files, and applies rename and delete rules to its open documents. The fake
implements only what tests need, and anything else throws `not faked`. It does not reproduce host
validation, scheduling, or durability, so behavior crossing the native boundary
still needs product tests.
