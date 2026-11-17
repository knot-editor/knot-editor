# D117: Non-modal notifications

Status: 🚧 approved, checkpoints 1 and 2 done. Roadmap item [D117](../roadmap.md).

## Outcome

Knot tells the user things without interrupting them, the way IntelliJ does. A
notice shows as a popup card in a corner of the window and is recorded in a
history that a Notifications panel lists. A bell in the status bar shows the
unread count. Extensions raise notices instead of opening a modal message, and
configuration warnings the user can fix, such as a missing `font.family`, reach
the user instead of only the log. Popups and the panel are designed to look
polished and modern from the first pass, not as a placeholder to refine later.

## Design

### Notices and the store

- A notice has a `message`, an optional `title`, a `severity` (`info`,
  `warning`, `error`), `autoDismiss` (defaults to true for `info` and false for
  `warning` and `error`), optional `actions` (`{ title, command: { name,
  arguments? } }`, the command reference status items use), an optional window
  `target`, a source (the package, or Knot for native notices), and a time.
- The application owns one notification store: the last 100 notices, each read
  or unread, in `knot-model` with plain tests. The store decides history
  eviction, read state, and which notices each window's popups show. The gpui
  global wraps it.
- A notice with a `target` shows its popup only in that window. Without one it
  shows in every window. History is application-wide either way.
- Closing a popup, running one of its actions, or opening the panel marks the
  notice read. Dismissing from the panel removes it from history. Popups are not
  the history: an auto-dismissed popup leaves its notice unread in history.
- Running an action dispatches its command in the popup's window as a status item
  click does, then closes the popup.

### Popups

- The window paints popups natively as an overlay, as it does the palette. Cards
  stack from the chosen corner, newest nearest the edge, with at most
  `notifications.maxVisible` shown. More notices collapse into a "+N more" pill
  that opens the panel.
- `autoDismiss` popups hide after `notifications.timeout`. Pointing at the
  stack pauses every timer in it.
- Look: a rounded card in the theme's chrome radius with a soft shadow; a
  severity icon in the severity's accent color; the title in the strong text and
  the message muted, wrapping to a few lines; actions as compact accent-colored
  buttons in a row under the text; a close button that shows on hover; a slide
  and fade on entry and exit. The card should feel at home in both the Islands
  and the flat theme.
- Settings: `notifications.position` (`bottomRight` by default, `bottomLeft`,
  `topRight`, `topLeft`, `topCenter`), `notifications.timeout`,
  `notifications.maxVisible`, and `notifications.popups` (false records notices
  without popups).
- Color keys with native defaults: `notification.background`,
  `notification.border`, `notification.foreground`,
  `notification.description.foreground`, and `notification.info`,
  `notification.warning`, `notification.error` (accents that default to `info`,
  `warning`, `error`). A theme or user override can restyle popups fully.

### Extension API

- `workbench.showNotice(options)` raises a notice and resolves once it is
  recorded. It does not wait for the user.
- `workbench.showMessage` is removed; `showConfirmation` stays for questions
  that need an answer. Explorer's and Projects' reports become notices.
- The application observation carries the notices: `ApplicationSnapshot` gains
  `notifications`, and a `notificationsChanged` event fires on add, read,
  dismiss, and clear. A package can therefore mirror notices in its own surface,
  and with `notifications.popups` off, replace native popups with its own
  presentation.
- `workbench.markNotificationsRead(ids)`, `workbench.dismissNotification(id)`,
  and `workbench.clearNotifications()` change the store.

### Notifications panel

A bundled `@knot/notifications` package, built only on the public API:

- A dock tool (bottom by default) listing notices newest first, in "Today" and
  "Earlier" groups. Each row has the severity icon, the title, the message as
  the detail line, a relative time, and an unread badge. The row's actions and
  Dismiss are in its context menu, and activating it runs the notice's first
  action. The frame toolbar has Mark All Read and Clear All. An empty panel shows
  a friendly empty state rather than a blank tree.
- A bell status item on the right with the unread count, colored by the most
  severe unread notice, that toggles the panel. `statusBar.hiddenItems` can
  hide it.
- Commands: Show Notifications, Clear Notifications.
- If the tree primitives cannot make the panel look polished (for example,
  messages longer than one detail line, or visible inline action buttons), add
  the missing tree item primitive rather than a native panel.

### Configuration warnings

Every `knot::config` warning also raises a sticky `warning` notice with no
target: setting warnings (`ExtensionHost::report_setting_warning`) and the
missing `font.family` (`typography.rs`). Notices raised before any window exists
wait in the store and pop up when the first window opens. A new Knot: Open
Personal Configuration command opens the `post-init.js` of the personal config
directory, and config warnings carry it as an action along with Open Log. The
fatal config error keeps its diagnostic window, and the startup report keeps
listing setting warnings.

## Exclusions

- Progress reporting with a handle, percentage, and cancellation moves to its
  own roadmap item, [D200](../roadmap.md), with the workspace file index scan as
  its first native consumer and language-server indexing later.
- Converting native warning prompts to notices stays in D181; terminal
  notifications stay in D041.
- No presenter provider: a package replaces popups by turning them off and
  observing notices. A new native layout would be another `position` value.
- Notice history does not persist across restarts.

## Deferred decisions

- The exact timeout and `maxVisible` defaults, and the animation timings, are
  tuned at the checkpoint 1 review.
- Whether the panel needs new tree item primitives is decided at the checkpoint
  3 review.

## Checkpoints

### 1. Native store and popups ✅

Approved. The `knot-model` store, the application global, the popup overlay,
the four `notifications.*` settings ([settings](../settings.md)), and the
`notification.*` color keys are in place; every bundled theme sets the keys.
`notifications.timeout` is in seconds (default 8) and `notifications.maxVisible`
defaults to 3. A timed-out popup hides in its own window only; closing one marks
the notice read everywhere. Still open: the "+N more" pill paints but opens
nothing until checkpoint 4 wires it to the panel.

### 2. Extension API ✅

Approved. `workbench.showNotice` and the store operations, the snapshot's
`notifications` and the `notificationsChanged` event, and the removal of
`showMessage` are in place ([extension guide](../extension-api.md#tell-the-user-without-interrupting)).
Explorer and Projects report failures as `warning` notices. A notice without a
`target` shows in every window, so those reports no longer stay in the
command's window. The personal `post-init.js` carries the `Try:` commands.

### 3. Popup countdown ring ✅ (awaiting review)

Deliverables: the close button is always visible, replacing the hover-only
button. On an auto-dismissing popup a thin ring in the severity accent drains
around it over `notifications.timeout`; a sticky popup shows the plain button.
Pointing at the stack freezes every ring along with its timer, and leaving
resumes both.

Validation: a window test that a ring's progress follows the timer, freezes
while the stack is hovered, resumes after, and is absent on a sticky popup.

Review gate: the user watches popups of each severity and `autoDismiss` setting
with the `Try: Notice ...` commands (the personal `post-init.js` still has
them), including pointing at the stack. Hovering now resumes the remaining time
rather than restarting the timeout.

Implementation context:
- `src/app/product/notifications.rs` owns the cards, timers, and hover pause.
- Track elapsed time per card rather than only the timer task, so the ring and
  the pause agree.

### 4. Notifications panel

Deliverables: the `@knot/notifications` package with the dock tool, groups,
rows, context menu actions, toolbar actions, empty state, bell status item,
commands, and the "+N more" pill opening the panel; any tree primitive the look
needs.

Validation: package tests against the fake host; a product test driving the
package through the real host (raise, see the row and the bell count, mark read,
dismiss, clear).

Review gate: the user reviews the panel and bell in the real app with the
`Try:` commands.

Implementation context:
- Mirror a small bundled package such as `extensions/@knot/go-to-line`
  (`knot.jsonc`, `src`, `test`, `tsconfig.json`) and register it in
  `crates/knot-ext-workbench/build.rs`.
- Views through `workbench.registerView` (`ViewContribution`, `TreeItem` with
  `detail`, `icon`, `badge`, `statusText`, `actions`); the bell through
  `workbench.registerStatusItem`, as `@knot/chrome` does.
- A new tree primitive goes through `src/app/tree_view.rs`,
  `src/app/extensions/semantics.rs`, and `knot.d.ts`.

### 5. Configuration warnings and docs

Deliverables: notices at every `knot::config` warning site, held until the first
window; the Open Personal Configuration command; architecture, decision, and
roadmap updates.

Validation: product tests that a setting warning and a missing font raise one
sticky notice each, including before the first window, with working actions.

Review gate: the user triggers a bad setting and a missing font through `Try:`
commands.

Implementation context:
- Sites: `report_setting_warning` in `src/app/extensions/host.rs` and the
  `font.family` warning in `src/app/typography.rs` (find them by searching for
  `knot::config`); `src/app/product/launch.rs`'s fatal error is excluded.
- The command beside `app.open-log` in `src/app/product/commands.rs` and
  `src/app/product/application_log.rs`; the directory from `user_config_root`
  in `src/app/personal_config.rs`.
- Docs: replace "The status bar carries no operation messages…" in
  `docs/architecture/workbench-and-lifecycle.md` with a Notifications section;
  add notification surfaces and color keys to `docs/architecture/semantic-ui.md`;
  update the failure-notices and application-log entries in
  `docs/decisions/platform.md` and `docs/decisions.md`; close D117 in the
  roadmap and update D181 to build on notices.
