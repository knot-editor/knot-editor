# Knot

Knot is a programmable text editor inspired by Emacs, with a modern native
interface. A Rust core on gpui owns the inseparable mechanics: text, input,
rendering, persistence, the workbench, and syntax parsing with tree-sitter.
Everything above that is programmable in JavaScript (TypeScript for bundled
packages), running in V8 isolates off the UI thread: configuration,
keybindings, commands, and extensions. Bundled packages use the same public
API as third-party ones, with no privileged access. Native code never calls
JavaScript synchronously, so the editor stays responsive whatever extensions
do.

Knot is building the real product: correctness comes first. Every completed
slice needs automated tests that cover its model, lifecycle, command, and
persistence paths, so later changes can't silently break it. Prefer focused
tests, and add UI or integration tests only when a lower boundary can't check
the behavior.

No one uses Knot yet: change stored formats, settings, the extension API, and
command names freely, with no migrations or compatibility shims.

## Documentation

- @docs/architecture.md holds the current boundaries. Route a change through
  its subsystem references in `docs/architecture/`; each names its canonical
  source. Read `docs/decisions.md` and its topic files when revisiting or
  adding a design choice, `docs/design.md` for product vision, and
  `docs/roadmap.md` for sequencing.
- Active plans live in `docs/plans/`. Keep a plan updated while implementing
  it, marking finished steps with ✅. Once a plan is complete, move what lasts
  into the current docs and delete the plan.
- When a change alters boundaries, runtime flows, or rationale, update the
  matching doc. [docs/README.md](docs/README.md) says which doc owns what;
  link to it rather than repeating it.

## Rules

- Never create a git worktree (directly or through options like
  `isolation: "worktree"`) without explicit approval.
- When executing multiple steps or tasks, commit each logical step as soon as
  it is done and verified, before starting the next one. Never accumulate
  several steps of work and try to split them into commits at the end.
- Don't use conventional commits.
- Code comments and module documentation must describe the current design and
  behavior only. Do not record migrations, previous locations, roadmap phases,
  or implementation history in code; Git history is the source for that.
- A commit that changes what a user sees (a feature, a fix, a behavior change)
  ends with one `Release-note: <sentence>` trailer per change: plain language
  for a user, one sentence, no item IDs or internal names. The nightly release
  notes are built from these trailers; commits with no user-visible change
  carry none. See [development](docs/development.md#nightly-releases).

- When the user pushes back on an approach or a result, discuss it first and
  agree on the way forward before editing any code or documentation.
- Plan a task, including any roadmap `D###` item, with the `brainstorm-plan`
  skill, and execute a plan with `implement-plan`.
- Whenever the user needs to verify something in the real app, such as a
  manual review gate, write a personal-config `post-init.js` that registers
  `try.*` commands (palette titles starting with `Try:`) exercising the
  change, and tell the user which commands to run. The config directory is
  the one `user_config_root` in `src/app/personal_config.rs` selects
  (`~/Library/Application Support/Knot` on macOS unless `~/.config/knot`
  exists). Look at the existing file first; replace it only when it tests
  finished work.
- At the end of every plan checkpoint, and before reporting Rust work as
  done, run `just check` and fix everything it reports, warnings included. It
  covers what CI covers, including a lint of the product build without test
  features, which test-oriented commands can mask through feature unification.
- Run `cargo fmt` once at the end of Rust work, before committing. Do not use
  `cargo fmt --check` or manually format individual files.

- Keep the design simple: no abstractions "just in case", and no handling of
  extremely unlikely edge cases. This is a text editor used by humans.
- Be brief and direct. The user knows the repo rules; don't recite them or ask
  permission for what was already asked.

