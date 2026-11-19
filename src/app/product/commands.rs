//! Workbench and application command declarations.
use super::*;
use crate::app::commands::declarations::{CommandContext, command};
use crate::app::workbench::{PaneId, Slot, SplitDirection, SplitEdge, Workbench};
use knot_protocol::{CommandArgumentValue, CommandOutcome};

#[command("file.new", title = "New Document", description = "Open an untitled document.", id = NEW_COMMAND, participant = Workbench)]
pub(crate) async fn new(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context
        .window(move |window, target, cx| {
            window
                .new_document_in_pane(target.tab.map(|tab| tab.pane), cx)
                .map(|_| ())
                .ok_or(CommandOutcome::InvalidTarget)
        })
        .await
}

/// The terminal commands `@knot/chrome` registers, named here for the menus
/// and keymaps that bind them.
#[cfg(not(test))]
pub(crate) const NEW_TERMINAL_COMMAND: &str = "terminal.new";
#[cfg(not(test))]
pub(crate) const TOGGLE_TERMINAL_COMMAND: &str = "builtin.terminal.toggle";

/// Stand-ins for chrome's terminal commands in the tests of this crate, which
/// load no packages: they open in the test Terminal container, in the
/// focused terminal's directory.
#[cfg(test)]
async fn open_new_terminal(
    context: CommandContext,
    location: operations::TerminalLocation,
) -> Result<(), CommandOutcome> {
    context
        .window(move |window, target, cx| {
            let focused = window
                .workbench
                .read(cx)
                .focused_pane()
                .map(|pane| pane.active_tab().surface_id());
            let directory = match focused {
                Some(TabSurfaceId::Terminal(id)) => KnotWindow::terminal_session_directory(id, cx),
                _ => None,
            };
            window
                .new_terminal_in_pane(target.tab.map(|tab| tab.pane), &location, directory, cx)
                .map(|_| ())
                .ok_or(CommandOutcome::InvalidTarget)
        })
        .await
}

#[cfg(test)]
#[command("terminal.new", title = "New Terminal", description = "Open a terminal.", id = NEW_TERMINAL_COMMAND, participant = Workbench)]
pub(crate) async fn new_terminal(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    open_new_terminal(
        context,
        operations::TerminalLocation::Container(crate::app::terminal::TERMINAL_TOOL_ID.into()),
    )
    .await
}

#[cfg(test)]
#[command("terminal.new-in-center", title = "New Terminal in Editor", description = "Open a terminal in the editor.", id = NEW_TERMINAL_IN_CENTER_COMMAND, participant = Workbench)]
pub(crate) async fn new_terminal_in_center(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    open_new_terminal(context, operations::TerminalLocation::Center).await
}

#[cfg(test)]
#[command("builtin.terminal.toggle", title = "Toggle Terminal", description = "Toggle the Terminal window.", id = TOGGLE_TERMINAL_COMMAND, participant = Workbench)]
pub(crate) async fn toggle_terminal(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| {
            window.toggle_tool(crate::app::terminal::TERMINAL_TOOL_ID, cx);
            Ok(())
        })
        .await
}

#[command("workbench.close-tab", title = "Close Tab", description = "Close the focused tab, or the window when no tab has focus.", id = CLOSE_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn close_tab(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .pending(move |target, completion, cx| {
            let kind = if target.tab.is_some() {
                ProtectedCloseKind::Tab
            } else {
                ProtectedCloseKind::Window
            };
            dispatch_close_to_captured_target(target, kind, completion, cx)
        })
        .await
}

#[command("workbench.reopen-closed-tab", title = "Reopen Closed Tab", description = "Reopen the tab this window closed most recently.", id = REOPEN_CLOSED_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn reopen_closed_tab(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| window.reopen_closed_tab(cx))
        .await
}

#[command("workbench.close-other-tabs", title = "Close Other Tabs", description = "Close every unpinned tab of the focused pane except the focused one.", id = CLOSE_OTHER_TABS_COMMAND, participant = Workbench)]
pub(crate) async fn close_other_tabs(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    close_target(&context, ProtectedCloseKind::OtherTabs).await
}

#[command("workbench.close-tabs-to-right", title = "Close Tabs to the Right", description = "Close every unpinned tab after the focused one in its pane.", id = CLOSE_TABS_TO_RIGHT_COMMAND, participant = Workbench)]
pub(crate) async fn close_tabs_to_right(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    close_target(&context, ProtectedCloseKind::TabsToRight).await
}

#[command("workbench.close-pane", title = "Close Pane", description = "Close every tab of the focused pane.", id = CLOSE_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn close_pane(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    close_target(&context, ProtectedCloseKind::Pane).await
}

#[command("workbench.close-other-panes", title = "Close Other Panes", description = "Close every tab of the other panes in the focused pane's area.", id = CLOSE_OTHER_PANES_COMMAND, participant = Workbench)]
pub(crate) async fn close_other_panes(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    close_target(&context, ProtectedCloseKind::OtherPanes).await
}

#[command("workbench.toggle-pin-tab", title = "Toggle Pin Tab", description = "Pin the focused tab before the others, or unpin it.", id = TOGGLE_PIN_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn toggle_pin_tab(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let tab = target.tab()?;
            window
                .workbench
                .update(cx, |workbench, _| workbench.toggle_pin(tab.pane, tab.tab))
                .ok_or(CommandOutcome::InvalidTarget)?;
            cx.notify();
            Ok(())
        })
        .await
}

#[command("workbench.keep-tab-open", title = "Keep Tab Open", description = "Make the focused preview tab a regular tab.", id = KEEP_TAB_OPEN_COMMAND, participant = Workbench)]
pub(crate) async fn keep_tab_open(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let tab = target.tab()?;
            if window.workbench.update(cx, |workbench, _| {
                workbench.promote_preview(tab.pane, tab.tab)
            }) {
                cx.notify();
            }
            Ok(())
        })
        .await
}

#[command("workbench.split-horizontal", title = "Split Right", description = "Split the focused pane to the right.", id = SPLIT_HORIZONTAL_COMMAND, participant = Workbench)]
pub(crate) async fn split_horizontal(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    split(&context, SplitDirection::Horizontal, SplitPlacement::After).await
}

#[command("workbench.split-left", title = "Split Left", description = "Split the focused pane to the left.", id = SPLIT_LEFT_COMMAND, participant = Workbench)]
pub(crate) async fn split_left(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    split(&context, SplitDirection::Horizontal, SplitPlacement::Before).await
}

#[command("workbench.split-up", title = "Split Up", description = "Split the focused pane upward.", id = SPLIT_UP_COMMAND, participant = Workbench)]
pub(crate) async fn split_up(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    split(&context, SplitDirection::Vertical, SplitPlacement::Before).await
}

#[command("workbench.split-vertical", title = "Split Down", description = "Split the focused pane downward.", id = SPLIT_VERTICAL_COMMAND, participant = Workbench)]
pub(crate) async fn split_vertical(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    split(&context, SplitDirection::Vertical, SplitPlacement::After).await
}

#[command("workbench.grow-pane", title = "Grow Pane", description = "Give the focused pane more room.", id = GROW_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn grow_pane(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    resize(&context, true, None).await
}

#[command("workbench.shrink-pane", title = "Shrink Pane", description = "Give the focused pane less room.", id = SHRINK_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn shrink_pane(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    resize(&context, false, None).await
}

macro_rules! resize_along {
    ($($name:literal, $title:literal, $description:literal, $id:ident, $function:ident, $grow:expr, $axis:expr;)*) => {$(
        #[command($name, title = $title, description = $description, id = $id, participant = Workbench)]
        pub(crate) async fn $function(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
            resize(&context, $grow, Some($axis)).await
        }
    )*};
}

resize_along! {
    "workbench.grow-pane-height", "Grow Pane Height", "Give the focused pane more height.", GROW_PANE_HEIGHT_COMMAND, grow_pane_height, true, SplitDirection::Vertical;
    "workbench.shrink-pane-height", "Shrink Pane Height", "Give the focused pane less height.", SHRINK_PANE_HEIGHT_COMMAND, shrink_pane_height, false, SplitDirection::Vertical;
    "workbench.grow-pane-width", "Grow Pane Width", "Give the focused pane more width.", GROW_PANE_WIDTH_COMMAND, grow_pane_width, true, SplitDirection::Horizontal;
    "workbench.shrink-pane-width", "Shrink Pane Width", "Give the focused pane less width.", SHRINK_PANE_WIDTH_COMMAND, shrink_pane_width, false, SplitDirection::Horizontal;
}

#[command("workbench.equalize-panes", title = "Equalize Panes", description = "Give every split pane of the focused pane's area an even share.", id = EQUALIZE_PANES_COMMAND, participant = Workbench)]
pub(crate) async fn equalize_panes(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let pane = target.tab()?.pane;
            window.workbench.update(cx, |workbench, _| {
                if let Some(area) = workbench.pane_area(pane) {
                    workbench.equalize_splits(area);
                }
            });
            cx.notify();
            Ok(())
        })
        .await
}

async fn resize(
    context: &CommandContext,
    grow: bool,
    axis: Option<SplitDirection>,
) -> Result<(), CommandOutcome> {
    context
        .window(move |window, target, cx| {
            let pane = target.tab()?.pane;
            let dock = window.workbench.read(cx).dock_location(pane);
            if !window
                .workbench
                .update(cx, |workbench, _| workbench.resize_pane(pane, grow, axis))
            {
                return Err(CommandOutcome::Unavailable);
            }
            if dock.is_some() {
                window.publish_tool_layout(cx);
            }
            cx.notify();
            Ok(())
        })
        .await
}

#[command("workbench.next-tab", title = "Next Tab", description = "Show the next tab of the focused pane.", id = NEXT_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn next_tab(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.cycle_tab(target.tab()?.pane, true, cx))
        .await
}

#[command("workbench.previous-tab", title = "Previous Tab", description = "Show the previous tab of the focused pane.", id = PREVIOUS_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn previous_tab(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.cycle_tab(target.tab()?.pane, false, cx))
        .await
}

#[command("workbench.next-recent-tab", title = "Next Recently Used Tab", description = "Walk the focused pane's tabs from most to least recently used.", id = NEXT_RECENT_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn next_recent_tab(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.walk_recent_tabs(target.tab()?.pane, true, cx))
        .await
}

#[command("workbench.previous-recent-tab", title = "Previous Recently Used Tab", description = "Walk the focused pane's tabs from least to most recently used.", id = PREVIOUS_RECENT_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn previous_recent_tab(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.walk_recent_tabs(target.tab()?.pane, false, cx))
        .await
}

macro_rules! go_to_tab {
    ($($name:literal, $title:literal, $id:ident, $function:ident, $index:expr;)*) => {$(
        #[command($name, title = $title, palette = false, id = $id, participant = Workbench)]
        pub(crate) async fn $function(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
            context
                .window(|window, target, cx| window.go_to_tab(target.tab()?.pane, $index, cx))
                .await
        }
    )*};
}

go_to_tab! {
    "workbench.go-to-tab-1", "Go to Tab 1", GO_TO_TAB_1_COMMAND, go_to_tab_1, Some(0);
    "workbench.go-to-tab-2", "Go to Tab 2", GO_TO_TAB_2_COMMAND, go_to_tab_2, Some(1);
    "workbench.go-to-tab-3", "Go to Tab 3", GO_TO_TAB_3_COMMAND, go_to_tab_3, Some(2);
    "workbench.go-to-tab-4", "Go to Tab 4", GO_TO_TAB_4_COMMAND, go_to_tab_4, Some(3);
    "workbench.go-to-tab-5", "Go to Tab 5", GO_TO_TAB_5_COMMAND, go_to_tab_5, Some(4);
    "workbench.go-to-tab-6", "Go to Tab 6", GO_TO_TAB_6_COMMAND, go_to_tab_6, Some(5);
    "workbench.go-to-tab-7", "Go to Tab 7", GO_TO_TAB_7_COMMAND, go_to_tab_7, Some(6);
    "workbench.go-to-tab-8", "Go to Tab 8", GO_TO_TAB_8_COMMAND, go_to_tab_8, Some(7);
}

#[command("workbench.go-to-last-tab", title = "Go to Last Tab", description = "Show the last tab of the focused pane.", id = GO_TO_LAST_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn go_to_last_tab(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.go_to_tab(target.tab()?.pane, None, cx))
        .await
}

#[command("workbench.focus-pane-left", title = "Focus Pane Left", description = "Focus the pane to the left.", id = FOCUS_PANE_LEFT_COMMAND, participant = Workbench)]
pub(crate) async fn focus_pane_left(
    context: CommandContext,
    scope: FocusScope,
) -> Result<(), CommandOutcome> {
    focus_toward(context, SplitEdge::Left, scope).await
}

#[command("workbench.focus-pane-right", title = "Focus Pane Right", description = "Focus the pane to the right.", id = FOCUS_PANE_RIGHT_COMMAND, participant = Workbench)]
pub(crate) async fn focus_pane_right(
    context: CommandContext,
    scope: FocusScope,
) -> Result<(), CommandOutcome> {
    focus_toward(context, SplitEdge::Right, scope).await
}

#[command("workbench.focus-pane-up", title = "Focus Pane Up", description = "Focus the pane above.", id = FOCUS_PANE_UP_COMMAND, participant = Workbench)]
pub(crate) async fn focus_pane_up(
    context: CommandContext,
    scope: FocusScope,
) -> Result<(), CommandOutcome> {
    focus_toward(context, SplitEdge::Top, scope).await
}

#[command("workbench.focus-pane-down", title = "Focus Pane Down", description = "Focus the pane below.", id = FOCUS_PANE_DOWN_COMMAND, participant = Workbench)]
pub(crate) async fn focus_pane_down(
    context: CommandContext,
    scope: FocusScope,
) -> Result<(), CommandOutcome> {
    focus_toward(context, SplitEdge::Bottom, scope).await
}

#[command("workbench.move-tab-left", title = "Move Tab Left", description = "Move the focused tab to the pane on the left, splitting when there is none.", id = MOVE_TAB_LEFT_COMMAND, participant = Workbench)]
pub(crate) async fn move_tab_left(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    move_toward(context, SplitEdge::Left).await
}

#[command("workbench.move-tab-right", title = "Move Tab Right", description = "Move the focused tab to the pane on the right, splitting when there is none.", id = MOVE_TAB_RIGHT_COMMAND, participant = Workbench)]
pub(crate) async fn move_tab_right(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    move_toward(context, SplitEdge::Right).await
}

#[command("workbench.move-tab-up", title = "Move Tab Up", description = "Move the focused tab to the pane above, splitting when there is none.", id = MOVE_TAB_UP_COMMAND, participant = Workbench)]
pub(crate) async fn move_tab_up(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    move_toward(context, SplitEdge::Top).await
}

#[command("workbench.move-tab-down", title = "Move Tab Down", description = "Move the focused tab to the pane below, splitting when there is none.", id = MOVE_TAB_DOWN_COMMAND, participant = Workbench)]
pub(crate) async fn move_tab_down(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    move_toward(context, SplitEdge::Bottom).await
}

#[command("workbench.focus-next-pane", title = "Focus Next Pane", description = "Focus the next editor-area pane in layout order, wrapping.", id = FOCUS_NEXT_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn focus_next_pane(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.cycle_pane_id(true)).await
}

#[command("workbench.focus-previous-pane", title = "Focus Previous Pane", description = "Focus the previous editor-area pane in layout order, wrapping.", id = FOCUS_PREVIOUS_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn focus_previous_pane(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.cycle_pane_id(false)).await
}

#[command("workbench.focus-first-pane", title = "Focus First Pane", description = "Focus the first editor-area pane in layout order.", id = FOCUS_FIRST_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn focus_first_pane(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.first_pane_id()).await
}

#[command("workbench.focus-last-pane", title = "Focus Last Pane", description = "Focus the last editor-area pane in layout order.", id = FOCUS_LAST_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn focus_last_pane(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.last_pane_id()).await
}

#[command("workbench.focus-previously-focused-pane", title = "Focus Previously Focused Pane", description = "Go back to the pane that had focus before this one.", id = FOCUS_PREVIOUS_FOCUSED_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn focus_previously_focused_pane(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.previous_pane_id()).await
}

async fn focus_chosen(
    context: CommandContext,
    choose: impl FnOnce(&Workbench) -> Option<PaneId> + 'static,
) -> Result<(), CommandOutcome> {
    context
        .window(move |window, _, cx| window.focus_chosen_pane(choose, cx))
        .await
}

/// Where directional focus may land: any painted pane, or, with
/// `{ "withinArea": true }`, only panes of the focused pane's area, as Vim's
/// `ctrl-w` keys keep to the center.
#[derive(Clone, Copy, Default)]
pub(crate) struct FocusScope {
    within_area: bool,
}

impl crate::app::commands::declarations::CommandArguments for FocusScope {
    fn decode(value: &CommandArgumentValue) -> Result<Self, CommandOutcome> {
        let invalid = || CommandOutcome::InvalidArgument {
            message: "expected null or { withinArea: boolean }".into(),
        };
        match value {
            CommandArgumentValue::Null => Ok(Self::default()),
            CommandArgumentValue::Object(fields) => match fields.get("withinArea") {
                Some(CommandArgumentValue::Boolean(within_area)) if fields.len() == 1 => Ok(Self {
                    within_area: *within_area,
                }),
                _ => Err(invalid()),
            },
            _ => Err(invalid()),
        }
    }
}

async fn focus_toward(
    context: CommandContext,
    edge: SplitEdge,
    scope: FocusScope,
) -> Result<(), CommandOutcome> {
    context
        .window(move |window, target, cx| {
            window.focus_pane_toward(target.tab()?.pane, edge, scope.within_area, cx)
        })
        .await
}

async fn move_toward(context: CommandContext, edge: SplitEdge) -> Result<(), CommandOutcome> {
    context
        .window(move |window, target, cx| {
            let tab = target.tab()?;
            window.move_tab_toward(tab.pane, tab.tab, edge, cx)
        })
        .await
}

#[command("workbench.toggle-maximize-pane", title = "Toggle Maximize Pane", description = "Show only the focused pane, or the whole layout again.", id = TOGGLE_MAXIMIZE_PANE_COMMAND, participant = Workbench)]
pub(crate) async fn toggle_maximize_pane(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            window.toggle_maximize(target.tab()?.pane, cx);
            Ok(())
        })
        .await
}

#[command("file.open", title = "Open…", description = "Open a file or folder.", id = OPEN_COMMAND, participant = Workbench)]
pub(crate) async fn open(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context.pending(dispatch_open_to_captured_target).await
}

#[command("file.save", title = "Save", description = "Save the current document.", id = SAVE_COMMAND, participant = Workbench)]
pub(crate) async fn save(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    save_to_target(&context, false).await
}

#[command("file.save-as", title = "Save As…", description = "Save the current document under a new name.", id = SAVE_AS_COMMAND, participant = Workbench)]
pub(crate) async fn save_as(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    save_to_target(&context, true).await
}

#[command("file.recover", title = "Recover Unsaved Documents…", description = "Restore or discard unsaved text kept from earlier sessions.", id = RECOVER_COMMAND, participant = Workbench)]
pub(crate) async fn recover(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context
        .window(|window, _, cx| {
            window.start_recovery_listing(cx);
            Ok(())
        })
        .await
}

#[command("editor.set-language-mode", title = "Set Language Mode…", description = "Choose the language of the current document.", id = SET_LANGUAGE_MODE_COMMAND, participant = Workbench)]
pub(crate) async fn set_language_mode(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.start_language_mode(target, cx))
        .await
}

#[command("extensions.restart", title = "Developer: Restart Extension…", description = "Reload one extension package without restarting Knot.", id = RESTART_EXTENSION_COMMAND, participant = Workbench)]
pub(crate) async fn restart_extension(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| {
            window.start_extension_restart(cx);
            Ok(())
        })
        .await
}

#[command("app.about", title = "Knot: About", description = "Show the Knot version and commit.", id = ABOUT_COMMAND, participant = Workbench)]
pub(crate) async fn about(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| {
            window.start_about(cx);
            Ok(())
        })
        .await
}

#[command("app.open-log", title = "Knot: Open Log", description = "Open the application log file.", id = OPEN_LOG_COMMAND, participant = Workbench)]
pub(crate) async fn open_log(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .pending(super::application_log::dispatch_open_log)
        .await
}

#[command("app.open-personal-config", title = "Knot: Open Personal Configuration", description = "Open the post-init.js of the personal configuration directory.", id = OPEN_PERSONAL_CONFIG_COMMAND, participant = Workbench)]
pub(crate) async fn open_personal_config(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .pending(super::application_log::dispatch_open_personal_config)
        .await
}

#[command("app.reveal-log-folder", title = "Knot: Reveal Log Folder", description = "Show the application log folder in the file manager.", id = REVEAL_LOG_FOLDER_COMMAND, participant = Workbench)]
pub(crate) async fn reveal_log_folder(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    let directory =
        crate::app::logging::log_directory().ok_or_else(|| CommandOutcome::HandlerFailure {
            message: "cannot determine the log directory".into(),
        })?;
    context
        .foreground(move |_, cx| {
            cx.reveal_path(&directory);
            Ok(())
        })
        .await
}

#[command("shell.install-command", title = "Shell Command: Install 'knot' command in PATH", description = "Put the knot command on the shell PATH.", id = INSTALL_SHELL_COMMAND, participant = Workbench)]
pub(crate) async fn install_shell_command(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    run_shell_command(
        context,
        "Install failed",
        crate::app::shell_command::install,
    )
    .await
}

#[command("shell.uninstall-command", title = "Shell Command: Uninstall 'knot' command from PATH", description = "Remove the knot command from the shell PATH.", id = UNINSTALL_SHELL_COMMAND, participant = Workbench)]
pub(crate) async fn uninstall_shell_command(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    run_shell_command(
        context,
        "Uninstall failed",
        crate::app::shell_command::uninstall,
    )
    .await
}

/// Run `work` off the foreground and report only its failure.
async fn run_shell_command(
    context: CommandContext,
    failure: &'static str,
    work: fn() -> Result<(), String>,
) -> Result<(), CommandOutcome> {
    context
        .window(move |_, _, cx| {
            let window = window_id(cx.entity_id());
            cx.spawn(async move |_, cx| {
                if let Err(message) = cx.background_spawn(async move { work() }).await {
                    cx.update(|cx| {
                        raise_notice(
                            window,
                            Severity::Error,
                            failure.into(),
                            Some(message),
                            Vec::new(),
                            cx,
                        );
                    });
                }
            })
            .detach();
            Ok(())
        })
        .await
}

#[command("keymaps.show", title = "Show Keybindings", description = "List every key binding, who made it, and what it overrides.", id = SHOW_KEYBINDINGS_COMMAND, participant = Workbench)]
pub(crate) async fn show_keybindings(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            window.start_show_keybindings(target.clone(), cx);
            Ok(())
        })
        .await
}

#[command("editor.show-completions", title = "Show Completions", description = "Show completions at the cursor.", id = SHOW_COMPLETIONS_COMMAND, participant = Workbench)]
pub(crate) async fn show_completions(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context
        .foreground(move |target, cx| {
            Ok(super::super::extensions::host::start_completion(
                target.clone(),
                cx,
            ))
        })
        .await
}

#[command("workbench.show-command-palette", title = "Show Command Palette", description = "Search and run commands.", id = SHOW_COMMAND_PALETTE_COMMAND, participant = Workbench)]
pub(crate) async fn show_command_palette(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context
        .window(move |window, target, cx| {
            window.show_command_palette_from(target.clone(), cx);
            Ok(())
        })
        .await
}

#[command("extensions.show-startup-report", title = "Developer: Show Extension Startup Report", description = "Show which packages started and why any failed.", id = SHOW_EXTENSION_REPORT_COMMAND, participant = Application)]
pub(crate) async fn show_extension_report(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context
        .window(|window, _, cx| {
            window.toggle_extension_report(cx);
            Ok(())
        })
        .await
}

#[command("extensions.show-memory", title = "Developer: Show Extension Memory", description = "Show the process's resident size and each package's JavaScript heap.", id = SHOW_EXTENSION_MEMORY_COMMAND, participant = Workbench)]
pub(crate) async fn show_extension_memory(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let report = super::super::extensions::host::memory_report(cx);
            window
                .new_untitled_in_pane(
                    target.tab.map(|tab| tab.pane),
                    "Extension Memory",
                    &report,
                    cx,
                )
                .map(|_| ())
                .ok_or(CommandOutcome::InvalidTarget)
        })
        .await
}

async fn cycle_picker_tab(context: CommandContext, delta: isize) -> Result<(), CommandOutcome> {
    context
        .window(move |window, _, cx| {
            window
                .cycle_picker_tab(delta, cx)
                .then_some(())
                .ok_or(CommandOutcome::Unavailable)
        })
        .await
}

#[command("picker.next-tab", title = "Next Picker Tab", description = "Show the next scope tab of the open picker.", palette = false, id = PICKER_NEXT_TAB_COMMAND, participant = Application)]
pub(crate) async fn picker_next_tab(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    cycle_picker_tab(context, 1).await
}

#[command("picker.previous-tab", title = "Previous Picker Tab", description = "Show the previous scope tab of the open picker.", palette = false, id = PICKER_PREVIOUS_TAB_COMMAND, participant = Application)]
pub(crate) async fn picker_previous_tab(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    cycle_picker_tab(context, -1).await
}

#[command("window.new", title = "New Blank Window", description = "Open a window without folders.", id = NEW_WINDOW_COMMAND, participant = Application)]
pub(crate) async fn new_window(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    context
        .foreground(|_, cx| {
            open_product_window(None, cx);
            Ok(())
        })
        .await
}

#[command("window.close", title = "Close Window", description = "Close the window and its tabs.", id = CLOSE_WINDOW_COMMAND, participant = Application)]
pub(crate) async fn close_window(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    close_target(&context, ProtectedCloseKind::Window).await
}

#[command("application.quit", title = "Quit Knot", description = "Quit Knot, keeping unsaved text for the next launch.", id = QUIT_COMMAND, participant = Application)]
pub(crate) async fn quit(
    context: CommandContext,
    (): (),
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .foreground(move |target, cx| {
            super::super::commands::dispatch::break_captured_history_group(target, cx);
            Ok(())
        })
        .await?;
    close_target(&context, ProtectedCloseKind::Quit).await
}

#[command("application.hide", title = "Hide Knot", description = "Hide Knot and its windows.", id = HIDE_COMMAND, participant = Application)]
pub(crate) async fn hide(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(|_, cx| {
            cx.hide();
            Ok(())
        })
        .await
}

#[command("application.hide-others", title = "Hide Other Applications", description = "Hide every application but Knot.", id = HIDE_OTHERS_COMMAND, participant = Application)]
pub(crate) async fn hide_others(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(|_, cx| {
            cx.hide_other_apps();
            Ok(())
        })
        .await
}

#[command("application.show-all", title = "Show All Applications", description = "Show the applications that Hide Other Applications hid.", id = SHOW_ALL_COMMAND, participant = Application)]
pub(crate) async fn show_all(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .foreground(|_, cx| {
            cx.unhide_other_apps();
            Ok(())
        })
        .await
}

#[command("window.minimize", title = "Minimize Window", description = "Minimize the window to the Dock.", id = MINIMIZE_WINDOW_COMMAND, participant = Application)]
pub(crate) async fn minimize_window(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    native_window_operation(&context, |window| window.minimize_window()).await
}

#[command("window.zoom", title = "Zoom Window", description = "Zoom the window to fill the screen, or restore its size.", id = ZOOM_WINDOW_COMMAND, participant = Application)]
pub(crate) async fn zoom_window(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    native_window_operation(&context, |window| window.zoom_window()).await
}

async fn native_window_operation(
    context: &CommandContext,
    operation: impl FnOnce(&mut Window) + 'static,
) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            let handle = target
                .native_window(cx)
                .ok_or(CommandOutcome::InvalidTarget)?;
            handle
                .update(cx, |_, native, _| operation(native))
                .map_err(|_| CommandOutcome::InvalidTarget)
        })
        .await
}

async fn split(
    context: &CommandContext,
    direction: SplitDirection,
    placement: SplitPlacement,
) -> Result<(), CommandOutcome> {
    context
        .window(move |window, target, cx| {
            if window.split_pane(target.tab()?.pane, direction, placement, cx) {
                Ok(())
            } else {
                Err(CommandOutcome::InvalidTarget)
            }
        })
        .await
}
async fn save_to_target(
    context: &CommandContext,
    save_as: bool,
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .pending(move |target, completion, cx| {
            dispatch_save_to_captured_target(target, save_as, completion, cx)
        })
        .await
}
async fn close_target(
    context: &CommandContext,
    kind: ProtectedCloseKind,
) -> Result<CommandOutcome, CommandOutcome> {
    context
        .pending(move |target, completion, cx| {
            dispatch_close_to_captured_target(target, kind, completion, cx)
        })
        .await
}

#[command("workbench.toggle-tool", title = "Toggle Tool…", description = "Choose a tool to show and focus, or to hide when focused.", id = TOGGLE_TOOL_COMMAND, participant = Workbench)]
pub(crate) async fn toggle_tool(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| {
            window.start_toggle_tool(cx);
            Ok(())
        })
        .await
}

#[command("workbench.toggle-status-bar", title = "Toggle Status Bar", description = "Show or hide the status bar.", id = TOGGLE_STATUS_BAR_COMMAND, participant = Workbench)]
pub(crate) async fn toggle_status_bar(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|_, _, cx| {
            let visible =
                crate::app::settings::boolean(crate::app::settings::STATUS_BAR_VISIBLE, cx);
            crate::app::settings::set_boolean(
                crate::app::settings::STATUS_BAR_VISIBLE,
                !visible,
                cx,
            );
            Ok(())
        })
        .await
}

#[command("workspace.add-folder", title = "Add Folder to Workspace…", description = "Add a folder to this window's workspace.", id = ADD_FOLDER_COMMAND, participant = Workbench)]
pub(crate) async fn add_folder(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| {
            window.start_add_folder(cx);
            Ok(())
        })
        .await
}

#[command("workbench.hide-active-tool", title = "Hide Active Tool Window", description = "Hide the focused tool window, or the one used last.", id = HIDE_ACTIVE_TOOL_COMMAND, participant = Workbench)]
pub(crate) async fn hide_active_tool(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.hide_active_tool(target.tab.map(|tab| tab.pane), cx))
        .await
}

#[command("workbench.jump-to-last-tool", title = "Jump to Last Tool Window", description = "Show and focus the tool window used before this one.", id = JUMP_TO_LAST_TOOL_COMMAND, participant = Workbench)]
pub(crate) async fn jump_to_last_tool(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| window.jump_to_last_tool(cx))
        .await
}

#[command("workbench.hide-all-tools", title = "Hide All Tool Windows", description = "Hide every tool window, or show the ones hidden last.", id = HIDE_ALL_TOOLS_COMMAND, participant = Workbench)]
pub(crate) async fn hide_all_tools(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, _, cx| window.hide_all_tools(cx))
        .await
}

#[command("workbench.maximize-tool", title = "Maximize Tool Window", description = "Show only the focused tool window, or the whole layout again.", id = MAXIMIZE_TOOL_COMMAND, participant = Workbench)]
pub(crate) async fn maximize_tool(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.maximize_active_tool(target.tab()?.pane, cx))
        .await
}

macro_rules! stretch_tool {
    ($($name:literal, $title:literal, $description:literal, $id:ident, $function:ident, $edge:expr;)*) => {$(
        #[command($name, title = $title, description = $description, id = $id, participant = Workbench)]
        pub(crate) async fn $function(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
            context
                .window(|window, target, cx| {
                    window.stretch_active_tool(target.tab.map(|tab| tab.pane), $edge, cx)
                })
                .await
        }
    )*};
}

stretch_tool! {
    "workbench.stretch-left", "Stretch to Left", "Move the tool window's edge left.", STRETCH_LEFT_COMMAND, stretch_left, SplitEdge::Left;
    "workbench.stretch-right", "Stretch to Right", "Move the tool window's edge right.", STRETCH_RIGHT_COMMAND, stretch_right, SplitEdge::Right;
    "workbench.stretch-top", "Stretch to Top", "Move the tool window's edge up.", STRETCH_TOP_COMMAND, stretch_top, SplitEdge::Top;
    "workbench.stretch-bottom", "Stretch to Bottom", "Move the tool window's edge down.", STRETCH_BOTTOM_COMMAND, stretch_bottom, SplitEdge::Bottom;
}

macro_rules! move_tool_to_slot {
    ($($name:literal, $title:literal, $id:ident, $function:ident, $slot:expr;)*) => {$(
        #[command($name, title = $title, description = "Move the tool window to another slot.", id = $id, participant = Workbench)]
        pub(crate) async fn $function(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
            context
                .window(|window, target, cx| {
                    window.move_active_tool(target.tab.map(|tab| tab.pane), $slot, cx)
                })
                .await
        }
    )*};
}

move_tool_to_slot! {
    "workbench.move-tool-to-left-top", "Move to Left Top", MOVE_TOOL_TO_LEFT_TOP_COMMAND, move_tool_to_left_top, Slot::LeftTop;
    "workbench.move-tool-to-left-bottom", "Move to Left Bottom", MOVE_TOOL_TO_LEFT_BOTTOM_COMMAND, move_tool_to_left_bottom, Slot::LeftBottom;
    "workbench.move-tool-to-right-top", "Move to Right Top", MOVE_TOOL_TO_RIGHT_TOP_COMMAND, move_tool_to_right_top, Slot::RightTop;
    "workbench.move-tool-to-right-bottom", "Move to Right Bottom", MOVE_TOOL_TO_RIGHT_BOTTOM_COMMAND, move_tool_to_right_bottom, Slot::RightBottom;
    "workbench.move-tool-to-bottom-left", "Move to Bottom Left", MOVE_TOOL_TO_BOTTOM_LEFT_COMMAND, move_tool_to_bottom_left, Slot::BottomLeft;
    "workbench.move-tool-to-bottom-right", "Move to Bottom Right", MOVE_TOOL_TO_BOTTOM_RIGHT_COMMAND, move_tool_to_bottom_right, Slot::BottomRight;
}

#[command("workbench.open-as-editor-tab", title = "Open as Editor Tab", description = "Move the focused terminal tab to the editor.", id = OPEN_AS_EDITOR_TAB_COMMAND, participant = Workbench)]
pub(crate) async fn open_as_editor_tab(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let tab = target.tab()?;
            window.move_tab_to_area(tab.pane, tab.tab, Area::Center, cx)
        })
        .await
}

#[command("workbench.return-to-tool-window", title = "Return to Tool Window", description = "Move the focused terminal tab back to its tool window.", id = RETURN_TO_TOOL_WINDOW_COMMAND, participant = Workbench)]
pub(crate) async fn return_to_tool_window(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let tab = target.tab()?;
            window.return_to_tool_window(tab.pane, tab.tab, cx)
        })
        .await
}

macro_rules! split_and_move {
    ($($name:literal, $title:literal, $description:literal, $id:ident, $function:ident, $edge:expr;)*) => {$(
        #[command($name, title = $title, description = $description, id = $id, participant = Workbench)]
        pub(crate) async fn $function(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
            context
                .window(|window, target, cx| {
                    let tab = target.tab()?;
                    window.split_and_move(tab.pane, tab.tab, $edge, cx)
                })
                .await
        }
    )*};
}

split_and_move! {
    "workbench.split-and-move-right", "Split and Move Right", "Move the focused tab into a new pane on the right.", SPLIT_AND_MOVE_RIGHT_COMMAND, split_and_move_right, SplitEdge::Right;
    "workbench.split-and-move-down", "Split and Move Down", "Move the focused tab into a new pane below.", SPLIT_AND_MOVE_DOWN_COMMAND, split_and_move_down, SplitEdge::Bottom;
}

#[command("workbench.unsplit", title = "Unsplit", description = "Merge the panes of the focused pane's area into it.", id = UNSPLIT_COMMAND, participant = Workbench)]
pub(crate) async fn unsplit(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| window.unsplit_pane(target.tab()?.pane, cx))
        .await
}

#[command("workbench.next-splitter", title = "Go to Next Splitter", description = "Focus the next pane of the focused pane's area, wrapping.", id = NEXT_SPLITTER_COMMAND, participant = Workbench)]
pub(crate) async fn next_splitter(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.cycle_pane_id(true)).await
}

#[command("workbench.previous-splitter", title = "Go to Previous Splitter", description = "Focus the previous pane of the focused pane's area, wrapping.", id = PREVIOUS_SPLITTER_COMMAND, participant = Workbench)]
pub(crate) async fn previous_splitter(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    focus_chosen(context, |workbench| workbench.cycle_pane_id(false)).await
}

#[command("terminal.rename-session", title = "Rename Session", description = "Change the label of the focused terminal tab.", id = RENAME_SESSION_COMMAND, participant = Workbench)]
pub(crate) async fn rename_session(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(|window, target, cx| {
            let tab = target.tab()?;
            window.start_rename_session(tab.pane, tab.tab, cx)
        })
        .await
}

#[command("workbench.reset-layout", title = "Reset Layout", description = "Return docks and tools to their default arrangement.", id = RESET_LAYOUT_COMMAND, participant = Workbench)]
pub(crate) async fn reset_layout(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    context
        .window(move |window, _, cx| {
            window.reset_tool_layout(cx);
            Ok(())
        })
        .await
}

/// Change the application zoom level by `delta` steps, or reset it.
async fn zoom(context: &CommandContext, delta: Option<i32>) -> Result<(), CommandOutcome> {
    context
        .foreground(move |_, cx| {
            let level = delta.map_or(0, |delta| crate::app::zoom::level(cx) + delta);
            crate::app::zoom::set_level(level, cx);
            Ok(())
        })
        .await
}

#[command("workbench.zoom-in", title = "Zoom In", description = "Make text and every surface larger in all windows.", id = ZOOM_IN_COMMAND, participant = Application)]
pub(crate) async fn zoom_in(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    zoom(&context, Some(1)).await
}

#[command("workbench.zoom-out", title = "Zoom Out", description = "Make text and every surface smaller in all windows.", id = ZOOM_OUT_COMMAND, participant = Application)]
pub(crate) async fn zoom_out(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    zoom(&context, Some(-1)).await
}

#[command("workbench.reset-zoom", title = "Reset Zoom", description = "Return every window to 100% zoom.", id = RESET_ZOOM_COMMAND, participant = Application)]
pub(crate) async fn reset_zoom(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    zoom(&context, None).await
}
