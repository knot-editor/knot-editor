---
name: review-docs
description: Periodically audit Knot's documentation (architecture, decisions, roadmap, plans, guides) for accuracy against the code, ownership, duplication, staleness, and growth, then propose and apply agreed fixes. Use for a documentation review, not for documenting a change while implementing it.
---

# Review the documentation

Documentation has no fixed size budget; this review is what keeps it accurate and proportionate. Follow `docs/README.md` for ownership and the decision entry rules, and `AGENTS.md` for repository rules.

1. **Scope.** If the user names files or an area, review that. Otherwise find the last review with `git log --grep='^Review documentation' -1` and review the docs changed since then (`git diff --stat <commit> -- docs AGENTS.md .agents`), plus anything those changes link to. With no earlier review, review everything, one area at a time.
2. **Snapshot.** Record `wc -w` for each file in scope, and its growth since the last review. Run `python3 .agents/skills/review-docs/check_links.py` from the repository root.
3. **Check** each file in scope:
   - *Accuracy.* Spot-check named types, modules, commands, settings, keys, and flows against the code. Resolve every `D###` against `docs/roadmap.md`: a shipped item cited as future work, or a reference to an item that no longer exists, is stale.
   - *Ownership.* Each fact lives in one home: behavior and flows in `docs/architecture/`, rationale in `docs/decisions/`, contracts in code and `crates/knot-host/src/js/knot.d.ts`, settings in `docs/settings.md`, deferred work in `docs/roadmap.md`. Flag a fact in the wrong home and repeated content that should be a link.
   - *Current state only.* Flag history, migrations, plan progress ("now", "no longer", "was moved", "this plan"), and completed checkpoints that have not been reduced to completion notes.
   - *Decisions.* Every entry meets the entry rules, and a superseded choice has been replaced rather than appended to. Every entry has exactly one matching line in the `docs/decisions.md` index, and that line still states its gist. A topic file that has grown to cover unrelated subjects should split along a real boundary. Apply the sentence test from the entry rules: behavior stated only in a decision moves to its architecture reference or the code first, and behavior stated in both is removed from the decision, which links instead. Never trim rejected alternatives or the measurements that decided a choice: they are the reason the file exists.
   - *Lifecycle.* Completed plans and reports whose roadmap item is done or dropped should be removed once their durable content is preserved. Completed roadmap items should be gone.
   - *Proportion.* Prose that is longer than what it says: restated context, the same point in several places, detail a reader cannot act on.
4. **Report** before editing: the snapshot, then findings grouped by file and ranked by impact. Each finding names the location, the problem, and the proposed fix. Say plainly when an area is in good shape.
5. **Apply** the fixes the user agrees to. Commit each logical group on its own, with a message that begins with the words `Review documentation`, such as `Review documentation for shipped roadmap references`, so the next review can find this one. Run the link checker again before the last commit.
