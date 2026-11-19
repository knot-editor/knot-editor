# D041: Terminal titles, bells, and notifications

Status: 🚧 approved, not started. Roadmap item [D041](../roadmap.md).
Builds on [terminal sessions and views](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views)
and [notifications](../architecture/workbench-and-lifecycle.md#notifications).

## Outcome

Three things programs send to a terminal now show up in Knot:

- **Titles** (OSC 0/2) label the terminal's tab.
- **Bells** rung while the terminal's tab isn't shown mark the tab, and mark
  the stripe button of a hidden tool window that holds it.
- **Desktop notifications** (OSC 9 and 777) become notices in the terminal's
  window, with an action that reveals the terminal.

## Design

- **Title vs. name.** While the program has set a non-empty title, the tab
  shows it instead of the default shell label (`zsh (2)`). A name set with
  Rename Session always wins over a title. An empty title goes back to the
  default label. Titles get no ` (N)` suffix and are not persisted: a
  restored session starts a new shell, and restart clears the title. The
  session records whether its name came from Rename, and window layouts keep
  that flag so an explicit name still wins after a restore.
- **Long labels.** Tabs don't truncate labels today, and shell titles such as
  `user@host: ~/src/project/deep/path` would widen terminal tabs without
  limit. Terminal tab labels get a maximum width and end with an ellipsis,
  and a terminal tab's tooltip shows its full label. Document and tool tabs
  are unchanged.
- **Bell marks.** A bell counts only while the terminal's tab is not shown,
  meaning it isn't the active tab of its pane or its tool window is hidden.
  The workbench model keeps the set of marked terminal tabs. A bell on a
  shown tab marks nothing, and any transition that shows a marked tab
  clears it. The tab paints Lucide's `bell-ring` before its label, in the
  `notification.info` color, the way pinned tabs paint `pin`. A hidden tool window holding a
  marked tab shows the stripe dot in the info color, unless a contributed
  badge is already set. A bell in a visible terminal does nothing: no
  visual bell and no system beep.
- **Notifications.** `knot-terminal` registers libghostty's
  desktop-notification callback as a new `TerminalEvent`. Each notification
  raises an `info` notice targeted at the window holding the terminal. The
  notice title is the program's title, or the tab label when the program
  gives none, and the notice message is the body. A "Show Terminal" action
  runs a native command that reveals and focuses the terminal's tab. Notices
  are raised even while the terminal is focused.
- **Routing.** A session turns these engine events into gpui events. The
  application finds the window whose workbench holds the session and applies
  them there. The meaning of each event (mark or not, which label) is model
  code in `knot-model` with plain tests.

## Exclusions

- OS-level desktop notifications, dock bounce, and window attention requests.
- Visual or audible bells.
- Progress reports (OSC 9;4) and the other ConEmu extensions.
- Extension API for titles, bells, or notifications.
- Rate limiting of notifications; the notice store already keeps only the
  last 100.

## Checkpoints

### 1. Titles on terminal tabs ✅

Done and approved. The session resolves its label from an explicit name, then
the program's title, then the default name; layouts record only explicit
names. See [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views).

### 2. Bell marks on tabs and stripe buttons

- **Implementation context:** `crates/knot-model/src/workbench.rs` and
  `workbench/terminals.rs`: a marked-sessions set, `mark_bell(session)`, which
  marks only when the tab isn't shown, clearing on `show_tab` /
  `activate_tab` / tool show, and a query for whether a tool holds a marked
  tab. `src/app/terminal/session.rs` emits a gpui bell event. The
  application subscribes where sessions are registered
  (`ApplicationTerminalSessions` in `src/app/terminal/mod.rs`, used from
  `src/app/product.rs`), finds the window with `find_window` / `window_for`,
  and calls the model. Painting: the tab bell icon in
  `src/app/product/render.rs`, beside the `pin` icon
  (`IconDefinition::lucide("bell-ring")`, `color::NOTIFICATION_INFO`,
  `bell-{tab}` selector), and the stripe dot in
  `src/app/product/stripes.rs` (`render_stripe_button`, next to
  `tool.badge`). Relocating a tab between panes or windows carries its mark.
- **Validation:** model unit tests for marking, not marking a shown tab,
  clearing on show, and tool aggregation. Product tests that ring the bell
  in a background tab and in a hidden Terminal tool window, then assert the
  tab icon and `stripe-badge-*` selectors appear and clear once the tab is
  shown. `just check`.
- **Review gate:** a `Try:` command rings the bell after a delay. The user
  switches away and checks the tab and stripe marks.

### 3. Desktop notifications as notices

- **Implementation context:** `crates/knot-terminal/src/events.rs`:
  register `GHOSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION`
  (`GhosttyTerminalDesktopNotification { title, body }`) as
  `TerminalEvent::Notification { title, body }`. Session to window routing
  as in checkpoint 2. Raise with `KnotWindow::raise_notice`
  (`src/app/product.rs`) or `notifications::raise` with a window target. A
  native command (in `src/app/product/commands.rs`, mirroring the
  `save_as_action` pattern in `src/app/product/save.rs`) takes the session
  and reveals and focuses its tab, and does nothing if the session is gone.
- **Validation:** a `knot-terminal` test that writes OSC 9 and OSC 777 and
  receives the event. Product tests in
  `src/app/product/tests/notifications.rs` or `terminal.rs`: the notice
  carries the expected title, body, and window, and its action shows a
  background terminal. `just check`.
- **Review gate:** a `Try:` command opens a terminal that sends OSC 9 and
  OSC 777 after a delay. The user checks the popup and the action.

### 4. Documentation

Update the terminal section of
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views)
(labels, bell marks, notifications) and replace its "OSC 52, title, and bell
events do not access product features" sentence. Add the title-vs-name rule
to the terminal decision in
[decisions/workbench-and-lifecycle.md](../decisions/workbench-and-lifecycle.md#terminal).
Then remove this plan and the roadmap item.

## Release notes

- Terminal tabs show the title the running program sets.
- Terminal tabs mark a bell rung while they were hidden.
- Notifications sent by terminal programs appear as Knot notices.
