//! The Knot preset's bindings and the application's preset registry.
//!
//! The `keymap.preset` setting selects the preset. It is resolved when startup
//! completes and again whenever the setting changes. Registering or removing a
//! preset later changes nothing until the next selection change.

use gpui::{App, Global};

use crate::app::keys::{Keystroke, Modifiers};

use super::{ApplicationKeymaps, Platform, PresetBinding, PresetOwner, PresetRegistry, rebuild};
use crate::app::commands::dispatch::*;
use crate::app::settings::{self, ApplicationSettings, KEYMAP_PRESET};
use crate::app::tree_view::SHOW_TREE_CONTEXT_MENU_COMMAND;
use knot_protocol::{ExtensionId, ExtensionLifecycleId};

/// Commands of bundled packages that the Knot preset binds. A key bound to one
/// reports it unavailable while its package is not running.
pub(crate) const GO_TO_LINE_COMMAND: &str = "editor.go-to-line";
#[cfg(test)]
pub(crate) const BUNDLED_PACKAGE_COMMANDS: &[&str] = &[GO_TO_LINE_COMMAND];

pub(crate) struct ApplicationKeymapPresets(pub(crate) PresetRegistry);

impl Default for ApplicationKeymapPresets {
    fn default() -> Self {
        Self(PresetRegistry::new(knot_preset(Platform::current())))
    }
}

impl Global for ApplicationKeymapPresets {}

/// Resolve and install the selected preset. Startup calls this once every
/// preset is registered, and it starts following the selection setting.
pub(crate) fn apply_at_startup(cx: &mut App) {
    apply(cx, true);
    cx.observe_global::<ApplicationSettings>(|cx| apply(cx, false))
        .detach();
}

fn apply(cx: &mut App, force: bool) {
    let selection = settings::text(KEYMAP_PRESET, cx);
    let Some(bindings) = cx
        .default_global::<ApplicationKeymapPresets>()
        .0
        .select(selection, force)
    else {
        return;
    };
    if cx.has_global::<ApplicationKeymaps>()
        && cx
            .global_mut::<ApplicationKeymaps>()
            .0
            .replace_preset(&bindings)
    {
        rebuild(cx);
    }
}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut App,
) {
    if cx.has_global::<ApplicationKeymapPresets>() {
        cx.global_mut::<ApplicationKeymapPresets>()
            .0
            .remove_lifecycle(PresetOwner {
                extension,
                lifecycle,
            });
    }
}

/// A preset table entry: key, command, and view kind. `secondary-` stands for
/// cmd on macOS and ctrl elsewhere.
type PresetEntry = (&'static str, &'static str, Option<&'static str>);

/// The Knot preset for a platform. On Windows and Linux, a global binding
/// whose first keystroke is ctrl with a printable character other than
/// backtick is unbound in terminals, so the shell receives readline and
/// job-control keys, and the terminal copies and pastes with ctrl-shift-c and
/// ctrl-shift-v instead. Named keys such as tab and pageup, and ctrl-`, stay
/// global because shells do not use them.
pub(crate) fn knot_preset(platform: Platform) -> Vec<PresetBinding> {
    let (secondary, platform_entries, os_entries) = match platform {
        Platform::MacOs => ("cmd-", MACOS_BINDINGS, &[][..]),
        Platform::Windows => ("ctrl-", PC_BINDINGS, WINDOWS_BINDINGS),
        Platform::Linux => ("ctrl-", PC_BINDINGS, LINUX_BINDINGS),
    };
    let mut preset = SHARED_BINDINGS
        .iter()
        .chain(platform_entries)
        .chain(os_entries)
        .map(|&(key, command, view)| {
            PresetBinding::new(key.replace("secondary-", secondary), Some(command), view)
        })
        .collect::<Vec<_>>();
    // Terminal programs read function keys.
    preset.push(PresetBinding::new(
        "shift-f10".into(),
        None,
        Some("terminal"),
    ));
    if platform != Platform::MacOs {
        let mut pass_through = preset
            .iter()
            .filter(|binding| binding.view.is_none() && reaches_the_shell(&binding.key))
            .map(|binding| binding.key.clone())
            .collect::<Vec<_>>();
        pass_through.sort();
        pass_through.dedup();
        preset.extend(
            pass_through
                .into_iter()
                .map(|key| PresetBinding::new(key, None, Some("terminal"))),
        );
        preset.extend(
            PC_TERMINAL_BINDINGS
                .iter()
                .map(|&(key, command, view)| PresetBinding::new(key.into(), Some(command), view)),
        );
    }
    preset
}

/// Whether a binding's first keystroke is one a shell reads: plain ctrl with
/// a printable character other than backtick.
fn reaches_the_shell(key: &str) -> bool {
    key.split_whitespace()
        .next()
        .and_then(|stroke| Keystroke::parse(stroke).ok())
        .is_some_and(|stroke| {
            let mut characters = stroke.key.chars();
            stroke.modifiers == Modifiers::control()
                && characters.next().is_some_and(|key| key != '`')
                && characters.next().is_none()
        })
}

const SHARED_BINDINGS: &[PresetEntry] = &[
    ("left", MOVE_LEFT_COMMAND, Some("editor")),
    ("shift-left", SELECT_LEFT_COMMAND, Some("editor")),
    ("right", MOVE_RIGHT_COMMAND, Some("editor")),
    ("shift-right", SELECT_RIGHT_COMMAND, Some("editor")),
    ("up", MOVE_UP_COMMAND, Some("editor")),
    ("shift-up", SELECT_UP_COMMAND, Some("editor")),
    ("down", MOVE_DOWN_COMMAND, Some("editor")),
    ("shift-down", SELECT_DOWN_COMMAND, Some("editor")),
    ("home", MOVE_SMART_HOME_COMMAND, Some("editor")),
    ("shift-home", SELECT_SMART_HOME_COMMAND, Some("editor")),
    ("end", MOVE_LINE_END_COMMAND, Some("editor")),
    ("shift-end", SELECT_LINE_END_COMMAND, Some("editor")),
    ("pageup", MOVE_PAGE_UP_COMMAND, Some("editor")),
    ("shift-pageup", SELECT_PAGE_UP_COMMAND, Some("editor")),
    ("pagedown", MOVE_PAGE_DOWN_COMMAND, Some("editor")),
    ("shift-pagedown", SELECT_PAGE_DOWN_COMMAND, Some("editor")),
    ("enter", INSERT_NEWLINE_COMMAND, Some("editor")),
    ("tab", INSERT_TAB_COMMAND, Some("editor")),
    ("shift-tab", OUTDENT_LINES_COMMAND, Some("editor")),
    ("secondary-]", INDENT_LINES_COMMAND, Some("editor")),
    ("secondary-[", OUTDENT_LINES_COMMAND, Some("editor")),
    ("secondary-/", TOGGLE_LINE_COMMENT_COMMAND, Some("editor")),
    // Every platform reports the shifted backslash as `|` without shift.
    (
        "secondary-|",
        JUMP_TO_MATCHING_BRACKET_COMMAND,
        Some("editor"),
    ),
    ("alt-up", MOVE_LINES_UP_COMMAND, Some("editor")),
    ("alt-down", MOVE_LINES_DOWN_COMMAND, Some("editor")),
    ("secondary-shift-k", DELETE_LINES_COMMAND, Some("editor")),
    ("backspace", DELETE_BACKWARD_COMMAND, Some("editor")),
    ("delete", DELETE_FORWARD_COMMAND, Some("editor")),
    ("ctrl-space", SHOW_COMPLETIONS_COMMAND, Some("editor")),
    ("ctrl-g", GO_TO_LINE_COMMAND, Some("editor")),
    ("shift-f10", SHOW_TREE_CONTEXT_MENU_COMMAND, None),
    ("secondary-n", NEW_COMMAND, None),
    ("ctrl-`", TOGGLE_TERMINAL_COMMAND, None),
    ("ctrl-tab", NEXT_RECENT_TAB_COMMAND, None),
    ("ctrl-shift-tab", PREVIOUS_RECENT_TAB_COMMAND, None),
    ("secondary-shift-t", REOPEN_CLOSED_TAB_COMMAND, None),
    ("secondary-shift-n", NEW_WINDOW_COMMAND, None),
    ("secondary-o", OPEN_COMMAND, None),
    ("secondary-s", SAVE_COMMAND, None),
    ("secondary-shift-s", SAVE_AS_COMMAND, None),
    ("secondary-w", CLOSE_TAB_COMMAND, None),
    ("secondary-shift-w", CLOSE_WINDOW_COMMAND, None),
    ("secondary-k right", SPLIT_HORIZONTAL_COMMAND, None),
    ("secondary-k down", SPLIT_VERTICAL_COMMAND, None),
    ("secondary-k left", SPLIT_LEFT_COMMAND, None),
    ("secondary-k up", SPLIT_UP_COMMAND, None),
    ("secondary-k =", GROW_PANE_COMMAND, None),
    ("secondary-k -", SHRINK_PANE_COMMAND, None),
    ("secondary-k 0", EQUALIZE_PANES_COMMAND, None),
    ("secondary-k secondary-left", FOCUS_PANE_LEFT_COMMAND, None),
    (
        "secondary-k secondary-right",
        FOCUS_PANE_RIGHT_COMMAND,
        None,
    ),
    ("secondary-k secondary-up", FOCUS_PANE_UP_COMMAND, None),
    ("secondary-k secondary-down", FOCUS_PANE_DOWN_COMMAND, None),
    ("secondary-k shift-left", MOVE_TAB_LEFT_COMMAND, None),
    ("secondary-k shift-right", MOVE_TAB_RIGHT_COMMAND, None),
    ("secondary-k shift-up", MOVE_TAB_UP_COMMAND, None),
    ("secondary-k shift-down", MOVE_TAB_DOWN_COMMAND, None),
    (
        "secondary-k secondary-m",
        TOGGLE_MAXIMIZE_PANE_COMMAND,
        None,
    ),
    ("secondary-k enter", KEEP_TAB_OPEN_COMMAND, None),
    ("secondary-k shift-enter", TOGGLE_PIN_TAB_COMMAND, None),
    ("secondary-k o", CLOSE_OTHER_TABS_COMMAND, None),
    ("secondary-k w", CLOSE_PANE_COMMAND, None),
    ("secondary-shift-p", SHOW_COMMAND_PALETTE_COMMAND, None),
    ("secondary-z", UNDO_COMMAND, None),
    ("secondary-shift-z", REDO_COMMAND, None),
    ("secondary-x", CUT_COMMAND, None),
    ("secondary-c", COPY_COMMAND, None),
    ("secondary-v", PASTE_COMMAND, None),
    ("secondary-a", SELECT_ALL_COMMAND, None),
    ("secondary-q", QUIT_COMMAND, None),
    ("secondary-=", ZOOM_IN_COMMAND, None),
    // Keyboards report the shifted `=` as `+`.
    ("secondary-+", ZOOM_IN_COMMAND, None),
    ("secondary--", ZOOM_OUT_COMMAND, None),
    ("secondary-0", RESET_ZOOM_COMMAND, None),
];

/// macOS word, line, and document movement, the Emacs-style keys that every
/// Cocoa text view accepts, and tab navigation.
const MACOS_BINDINGS: &[PresetEntry] = &[
    ("alt-left", MOVE_WORD_LEFT_COMMAND, Some("editor")),
    ("shift-alt-left", SELECT_WORD_LEFT_COMMAND, Some("editor")),
    ("alt-right", MOVE_WORD_RIGHT_COMMAND, Some("editor")),
    ("shift-alt-right", SELECT_WORD_RIGHT_COMMAND, Some("editor")),
    ("cmd-left", MOVE_SMART_HOME_COMMAND, Some("editor")),
    ("shift-cmd-left", SELECT_SMART_HOME_COMMAND, Some("editor")),
    ("cmd-right", MOVE_LINE_END_COMMAND, Some("editor")),
    ("shift-cmd-right", SELECT_LINE_END_COMMAND, Some("editor")),
    ("cmd-up", MOVE_DOCUMENT_START_COMMAND, Some("editor")),
    (
        "shift-cmd-up",
        SELECT_DOCUMENT_START_COMMAND,
        Some("editor"),
    ),
    ("cmd-down", MOVE_DOCUMENT_END_COMMAND, Some("editor")),
    (
        "shift-cmd-down",
        SELECT_DOCUMENT_END_COMMAND,
        Some("editor"),
    ),
    // Cocoa's ctrl-a and ctrl-e reach the paragraph's edges, past wrapping.
    ("ctrl-a", MOVE_LINE_START_LOGICAL_COMMAND, Some("editor")),
    ("ctrl-e", MOVE_LINE_END_LOGICAL_COMMAND, Some("editor")),
    ("ctrl-f", MOVE_RIGHT_COMMAND, Some("editor")),
    ("ctrl-b", MOVE_LEFT_COMMAND, Some("editor")),
    ("ctrl-n", MOVE_DOWN_COMMAND, Some("editor")),
    ("ctrl-p", MOVE_UP_COMMAND, Some("editor")),
    ("ctrl-d", DELETE_FORWARD_COMMAND, Some("editor")),
    ("ctrl-h", DELETE_BACKWARD_COMMAND, Some("editor")),
    ("ctrl-k", DELETE_TO_LINE_END_COMMAND, Some("editor")),
    ("shift-alt-a", TOGGLE_BLOCK_COMMENT_COMMAND, Some("editor")),
    ("shift-alt-up", COPY_LINES_UP_COMMAND, Some("editor")),
    ("shift-alt-down", COPY_LINES_DOWN_COMMAND, Some("editor")),
    ("ctrl-j", JOIN_LINES_COMMAND, Some("editor")),
    ("cmd-alt-[", FOLD_COMMAND, Some("editor")),
    ("cmd-alt-]", UNFOLD_COMMAND, Some("editor")),
    // macOS reports cmd-shift-] as cmd-}.
    ("cmd-}", NEXT_TAB_COMMAND, None),
    ("cmd-{", PREVIOUS_TAB_COMMAND, None),
    ("cmd-1", GO_TO_TAB_1_COMMAND, None),
    ("cmd-2", GO_TO_TAB_2_COMMAND, None),
    ("cmd-3", GO_TO_TAB_3_COMMAND, None),
    ("cmd-4", GO_TO_TAB_4_COMMAND, None),
    ("cmd-5", GO_TO_TAB_5_COMMAND, None),
    ("cmd-6", GO_TO_TAB_6_COMMAND, None),
    ("cmd-7", GO_TO_TAB_7_COMMAND, None),
    ("cmd-8", GO_TO_TAB_8_COMMAND, None),
    ("cmd-9", GO_TO_LAST_TAB_COMMAND, None),
];

/// Windows and Linux word and document movement, the extra Redo key, and tab
/// navigation keys that shells leave alone.
const PC_BINDINGS: &[PresetEntry] = &[
    ("ctrl-left", MOVE_WORD_LEFT_COMMAND, Some("editor")),
    ("shift-ctrl-left", SELECT_WORD_LEFT_COMMAND, Some("editor")),
    ("ctrl-right", MOVE_WORD_RIGHT_COMMAND, Some("editor")),
    (
        "shift-ctrl-right",
        SELECT_WORD_RIGHT_COMMAND,
        Some("editor"),
    ),
    ("ctrl-home", MOVE_DOCUMENT_START_COMMAND, Some("editor")),
    (
        "shift-ctrl-home",
        SELECT_DOCUMENT_START_COMMAND,
        Some("editor"),
    ),
    ("ctrl-end", MOVE_DOCUMENT_END_COMMAND, Some("editor")),
    (
        "shift-ctrl-end",
        SELECT_DOCUMENT_END_COMMAND,
        Some("editor"),
    ),
    ("ctrl-y", REDO_COMMAND, None),
    ("ctrl-pagedown", NEXT_TAB_COMMAND, None),
    ("ctrl-pageup", PREVIOUS_TAB_COMMAND, None),
    ("alt-1", GO_TO_TAB_1_COMMAND, None),
    ("alt-2", GO_TO_TAB_2_COMMAND, None),
    ("alt-3", GO_TO_TAB_3_COMMAND, None),
    ("alt-4", GO_TO_TAB_4_COMMAND, None),
    ("alt-5", GO_TO_TAB_5_COMMAND, None),
    ("alt-6", GO_TO_TAB_6_COMMAND, None),
    ("alt-7", GO_TO_TAB_7_COMMAND, None),
    ("alt-8", GO_TO_TAB_8_COMMAND, None),
    ("alt-9", GO_TO_LAST_TAB_COMMAND, None),
];

/// Windows keys where VS Code's Linux defaults differ.
const WINDOWS_BINDINGS: &[PresetEntry] = &[
    ("shift-alt-a", TOGGLE_BLOCK_COMMENT_COMMAND, Some("editor")),
    ("shift-alt-up", COPY_LINES_UP_COMMAND, Some("editor")),
    ("shift-alt-down", COPY_LINES_DOWN_COMMAND, Some("editor")),
];

/// VS Code's Linux keys for the commands in `WINDOWS_BINDINGS`.
const LINUX_BINDINGS: &[PresetEntry] = &[
    ("ctrl-shift-a", TOGGLE_BLOCK_COMMENT_COMMAND, Some("editor")),
    ("ctrl-shift-alt-up", COPY_LINES_UP_COMMAND, Some("editor")),
    (
        "ctrl-shift-alt-down",
        COPY_LINES_DOWN_COMMAND,
        Some("editor"),
    ),
];

const PC_TERMINAL_BINDINGS: &[PresetEntry] = &[
    ("ctrl-shift-c", COPY_COMMAND, Some("terminal")),
    ("ctrl-shift-v", PASTE_COMMAND, Some("terminal")),
];
