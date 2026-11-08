# Documentation

Current sources of truth:

- [architecture.md](architecture.md) is the short system overview: boundaries,
  ownership, dependency direction, and cross-cutting invariants. It links to
  focused [subsystem references](architecture/) for runtime flows and behavioral
  constraints; load only the references relevant to the task.
- [decisions.md](decisions.md) records validated choices and rationale, with
  links to subsystem references and essential supporting measurements.
- [design.md](design.md) describes product vision and long-term principles.
- [roadmap.md](roadmap.md) records the prioritized work and the unordered
  register of deferred work.
- [plans/](plans/) contains active executable slice plans.
- [reports/](reports/) contains analyses that support deferred roadmap items;
  remove a report once its item is completed or dropped.
- [settings.md](settings.md) lists every native and bundled-package setting
  with its type, default, and scope.
- [extension-api.md](extension-api.md) is the extension-author guide for setup and common
  workflows; precise API contracts live beside the TypeScript declarations.
- [development.md](development.md) covers local checks, CI, and the Rust
  toolchain.

Completed plans and exploration reports are retained in Git history. Preserve
durable behavior in the architecture references and rationale and essential
measurements in decisions before removing a completed plan.

Keep each fact in its authoritative home. Summaries may repeat enough context
for orientation, then link to the owner rather than reproducing its contract.
Architecture references retain ownership, cross-boundary flows, and constraints
that are difficult to infer from one module. Constants, field-level API rules,
and local algorithms belong beside code and tests. Decisions retain the choice,
its reason, and evidence needed to assess it. Plans retain unfinished work and
review gates; reduce completed checkpoints to completion notes and current links.

No document has a size budget. The `review-docs` skill in `.agents/skills`
periodically checks the documentation against the code and these ownership
rules, and keeps it proportionate.

## Decisions

[decisions.md](decisions.md) is an index with one line per decision, grouped by
topic; the entries live in topic files under [decisions/](decisions/). A task
that revisits a choice reads the index, then only the topic files it touches.

- Record a choice only when a reasonable alternative was rejected and a future
  change might revisit it. Small or obvious choices stay in plans and commit
  messages.
- An entry states the choice, why it was made, the rejected alternatives that
  matter, and the measurements that decided it. Keep it as short as the
  reasoning allows.
- Leave behavior out: keys, wording, defaults, edge-case rules, and how a
  feature works belong in architecture references or code. Link them instead.
  A sentence belongs in a decision only if removing it would lose a reason, a
  rejected alternative, or deciding evidence; describe behavior only as far as
  the reason needs it. Behavior a decision explains must also be stated in its
  owner, so the architecture never depends on reading decisions.
- Revise the existing section when a choice extends or changes a topic, and
  replace a superseded choice rather than appending to it. Never record
  history; Git keeps it.
- Add or update the entry's index line in the same change. Put a new entry in
  the topic file that owns its subject, and start a new topic file only when
  none fits.
- Keep section headings and file names stable, since other documents link to
  them.
