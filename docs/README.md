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

## Decisions

`decisions.md` is read when a task revisits a choice, not in every session, so
it must stay short enough to read whole.

- Record a choice only when a reasonable alternative was rejected and a future
  change might revisit it. Small or obvious choices stay in plans and commit
  messages.
- An entry states the choice, why it was made, the rejected alternatives that
  matter, and at most one or two measurements that decided it. Keep it to one
  or two short paragraphs, roughly 150 words.
- Leave behavior out: keys, wording, defaults, edge-case rules, and how a
  feature works belong in architecture references or code. Link them instead.
- Revise the existing section when a choice extends or changes a topic, and
  replace a superseded choice rather than appending to it. Never record
  history; Git keeps it.
- Keep the whole file under 5,000 words (`wc -w docs/decisions.md`). If an
  addition would exceed it, condense or merge existing entries in the same
  change.
- Keep section headings stable, since other documents link to them.
