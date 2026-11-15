---
name: review-roadmap
description: Review Knot's roadmap with the user, one item at a time — triage items into Lower priority, drop stale ones, or rewrite them — and sort the Next up queue of work to do next. Use for a roadmap review or reprioritization, not for planning one item (brainstorm-plan) or a documentation audit (review-docs).
---

# Review the roadmap

`docs/roadmap.md` defines the item shape, the section ordering rule, the
[Next up](../../../docs/roadmap.md#next-up) queue, the Lower priority section,
and the next unused ID line. Follow it and `AGENTS.md`. The user decides every
item; you propose.

A review has two sessions. Run the one the user asks for, or both in order:
triage first, then the queue. The user may switch mid-way; follow them.

## Before proposing

1. Read the whole roadmap, and list each item outside Lower priority with its
   section, dependencies, and trigger. Note which items depend on each one.
2. Treat earlier answers in the session as priorities: a user who keeps every
   Vim item does not want the next Vim item proposed.

## Triage session

Walk the main sections and propose, **one item per message**, what to do with
it. Start with the clearest candidates: items whose own text calls them low
priority, then open-ended "when a workflow is selected" items, then items whose
trigger has not occurred and that nothing depends on. Skip items that are
clearly near-term or that the user has already shown they value.

Each proposal names the item and its section, gives two or three short reasons
grounded in its text and in what exists today, states what depends on it, and
ends with one question. Give a lean when you have one, and say so when it is
close. The answers are:

- **Defer:** move it to the end of Lower priority unchanged, except for
  wording the section heading now says (such as "Low priority.").
- **Drop:** delete it. Nothing renumbers.
- **Keep:** leave it. Offer, in one line, to record why in its trigger when
  the user's reason is not in the text.
- **Rewrite, split, or promote:** when the user's answer reshapes the item,
  draft the new text, check its claims against the code first (grep for the
  feature; never assert that something is missing or present without
  looking), and confirm before writing when the change is more than a reword.
  A split takes the next unused ID for the new part. When the real gap is a
  missing building block, propose a new item for it and rewire the
  dependencies.

An ambiguous answer ("ok" to "move it, or drop it?") takes the first option
offered; say which you applied.

After every change:

- Grep `docs/` for the ID of a dropped, split, or renamed item, and fix each
  citation (decisions, other items' descriptions, reports where the sentence
  would mislead) in the same pass. Remove a dropped ID from every *Depends
  on*, and reword what depended on it.
- Keep the ordering rule: an item comes after the items it depends on. A new
  dependency may mean moving an item.
- Bump the next unused ID line when you take an ID.
- Keep lines wrapped as the file wraps them.

Apply each answer to the file as soon as it is given, then propose the next
item in the same message. When the strong candidates run out, say so, list the
weak ones left with your lean, and summarize what was deferred, dropped, and
rewritten.

## Queue session

Sort the work to do next into the Next up list.

1. Gather the candidates: the current queue, items with an active 🚧 plan, and
   items whose text or the user's answers mark them as urgent. Add each
   unfinished dependency of a candidate, since a queued item cannot start
   before it; call out any such gap.
2. Propose one ordered list, each entry with a few words on why it sits
   there: plans already underway first, then small items, then items others
   build on, then dependents. Flag an order the user may regret, such as a
   correctness bug in ordinary editing placed late, but take their call.
3. Iterate on the whole list until the user agrees; restate it in full after
   each change, so an ambiguous instruction gets resolved against a concrete
   list.
4. Write the queue, and remove urgency phrases ("do it now") from the queued
   items' descriptions: the queue states the order, and a description keeps
   only what is missing and what to build.

## Finish

Run `python3 .agents/skills/review-docs/check_links.py`. Commit once per
session, triage then queue, with a message that begins with the words
`Review roadmap`, such as `Review roadmap: defer and drop stale items`.
