# Installed extension examples

`@example/modal` is an installed package written in
TypeScript. Build it with the repository's pinned toolchain, which emits
`dist/` beside `src/`:

```sh
npm --prefix scripts/extensions ci
npm --prefix scripts/extensions run build examples/extensions/@example/modal
```

Then copy the `@example` directory into Knot's per-user `extensions` directory
and restart Knot. On macOS the destination is
`~/Library/Application Support/Knot/extensions/@example`, and
`just install-example-apps-mac` builds and copies it in one step. On Linux it is
`~/.local/share/Knot/extensions/@example`; on Windows use the local app-data
`Knot\extensions\@example` directory. Knot loads only `knot.jsonc` and the
built JavaScript. Use the **Developer: Show Extension Startup Report** command
to inspect load results.

The manifest declares `apiVersion: 1`. See the
[extension author guide](../../docs/extension-api.md) for the API reference,
manifest schema, lifetime rules, and checked examples.

## Modal editing

`@example/modal` is a tiny modal editor on key contexts. **Example: Enter
Modal Mode** puts the focused editor into the `modal` context, where h, j, k,
and l move the caret, other typed text is swallowed by a text binding, and
Escape leaves. **Example: Toggle Modal Mode in Every Editor** uses long-lived editor handles
instead: each editor you activate enters the `modal` context, and selecting
text in one leaves it, through `onDidChangeSelection`. It will be removed once
the bundled Vim mode exists.
