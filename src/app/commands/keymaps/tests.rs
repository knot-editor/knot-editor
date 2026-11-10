use super::*;
use crate::app::commands::dispatch::*;
use gpui::{KeyContext, TestAppContext};
use knot_protocol::{Command, CommandArgumentValue, ExtensionId, ExtensionLifecycleId};
use std::collections::HashSet;

fn command(name: &str) -> Command {
    Command {
        name: name.into(),
        arguments: CommandArgumentValue::Null,
    }
}

fn resolved(registry: &BindingRegistry, key: &str, view: Option<&str>) -> Option<String> {
    match registry.resolve(&canonical_key(key).unwrap(), view) {
        Some(BindingResolution::Command(command)) => Some(command.name.to_string()),
        Some(BindingResolution::Unbound) => Some("<unbound>".into()),
        None => None,
    }
}

const PLATFORMS: [Platform; 3] = [Platform::MacOs, Platform::Windows, Platform::Linux];

fn preset_registry(platform: Platform) -> BindingRegistry {
    let mut registry = BindingRegistry::new();
    registry.replace_preset(&knot_preset(platform));
    registry
}

#[test]
fn every_preset_binding_is_unique_and_names_a_native_or_bundled_command() {
    let native = inventory::iter::<crate::app::commands::declarations::CommandDescriptor>
        .into_iter()
        .map(|descriptor| descriptor.name)
        .chain(presets::BUNDLED_PACKAGE_COMMANDS.iter().copied())
        .collect::<HashSet<_>>();
    for platform in PLATFORMS {
        let mut seen = HashSet::new();
        for binding in knot_preset(platform) {
            let key = canonical_key(&binding.key)
                .unwrap_or_else(|_| panic!("{platform:?}: invalid key {}", binding.key));
            let view = binding.view;
            assert!(
                seen.insert((key.clone(), view.clone())),
                "{platform:?}: {key} bound twice in {view:?}"
            );
            if let Some(command) = binding.command {
                assert!(
                    native.contains(command.name.as_ref()),
                    "{platform:?}: {key} names unknown command {}",
                    command.name
                );
            }
        }
    }
}

#[test]
fn every_platform_binds_the_pane_layout_chords() {
    for platform in PLATFORMS {
        let registry = preset_registry(platform);
        let secondary = if platform == Platform::MacOs {
            "cmd"
        } else {
            "ctrl"
        };
        for (chord, command_name) in [
            ("left", SPLIT_LEFT_COMMAND),
            ("up", SPLIT_UP_COMMAND),
            ("right", SPLIT_HORIZONTAL_COMMAND),
            ("down", SPLIT_VERTICAL_COMMAND),
            ("=", GROW_PANE_COMMAND),
            ("-", SHRINK_PANE_COMMAND),
            ("0", EQUALIZE_PANES_COMMAND),
            ("secondary-left", FOCUS_PANE_LEFT_COMMAND),
            ("secondary-right", FOCUS_PANE_RIGHT_COMMAND),
            ("secondary-up", FOCUS_PANE_UP_COMMAND),
            ("secondary-down", FOCUS_PANE_DOWN_COMMAND),
            ("shift-left", MOVE_TAB_LEFT_COMMAND),
            ("shift-right", MOVE_TAB_RIGHT_COMMAND),
            ("shift-up", MOVE_TAB_UP_COMMAND),
            ("shift-down", MOVE_TAB_DOWN_COMMAND),
            ("secondary-m", TOGGLE_MAXIMIZE_PANE_COMMAND),
            ("enter", KEEP_TAB_OPEN_COMMAND),
            ("shift-enter", TOGGLE_PIN_TAB_COMMAND),
            ("o", CLOSE_OTHER_TABS_COMMAND),
            ("w", CLOSE_PANE_COMMAND),
        ] {
            let chord = chord.replace("secondary", secondary);
            let key = format!("{secondary}-k {chord}");
            assert_eq!(
                resolved(&registry, &key, Some("editor")).as_deref(),
                Some(command_name),
                "{platform:?}: {key}"
            );
        }
    }
}

#[test]
fn windows_and_linux_presets_use_control_shortcuts() {
    for platform in [Platform::Windows, Platform::Linux] {
        let registry = preset_registry(platform);
        for (key, command_name) in [
            ("ctrl-shift-p", SHOW_COMMAND_PALETTE_COMMAND),
            ("ctrl-o", OPEN_COMMAND),
            ("ctrl-s", SAVE_COMMAND),
            ("ctrl-shift-s", SAVE_AS_COMMAND),
            ("ctrl-c", COPY_COMMAND),
            ("ctrl-x", CUT_COMMAND),
            ("ctrl-v", PASTE_COMMAND),
            ("ctrl-w", CLOSE_TAB_COMMAND),
            ("ctrl-shift-w", CLOSE_WINDOW_COMMAND),
            ("ctrl-q", QUIT_COMMAND),
            ("ctrl-y", REDO_COMMAND),
            ("ctrl-shift-z", REDO_COMMAND),
            ("ctrl-left", MOVE_WORD_LEFT_COMMAND),
            ("ctrl-right", MOVE_WORD_RIGHT_COMMAND),
            ("ctrl-home", MOVE_DOCUMENT_START_COMMAND),
            ("ctrl-end", MOVE_DOCUMENT_END_COMMAND),
        ] {
            assert_eq!(
                resolved(&registry, key, Some("editor")).as_deref(),
                Some(command_name),
                "{platform:?}: {key}"
            );
        }
        assert!(
            registry.keys().iter().all(|key| !key.contains("cmd-")),
            "{platform:?} binds the platform key"
        );
    }
}

#[test]
fn windows_and_linux_terminals_leave_plain_control_keys_to_the_shell() {
    for platform in [Platform::Windows, Platform::Linux] {
        let registry = preset_registry(platform);
        for key in [
            "ctrl-c",
            "ctrl-v",
            "ctrl-w",
            "ctrl-a",
            "ctrl-z",
            "ctrl-k right",
        ] {
            assert_eq!(
                resolved(&registry, key, Some("terminal")).as_deref(),
                Some("<unbound>"),
                "{platform:?}: {key}"
            );
        }
        assert_eq!(
            resolved(&registry, "ctrl-shift-c", Some("terminal")).as_deref(),
            Some(COPY_COMMAND)
        );
        assert_eq!(
            resolved(&registry, "ctrl-shift-v", Some("terminal")).as_deref(),
            Some(PASTE_COMMAND)
        );
        assert_eq!(
            resolved(&registry, "ctrl-shift-p", Some("terminal")).as_deref(),
            Some(SHOW_COMMAND_PALETTE_COMMAND)
        );
        assert_eq!(resolved(&registry, "ctrl-shift-c", Some("editor")), None);
        // Named keys and ctrl-` are not shell input, so they stay global.
        for (key, command_name) in [
            ("ctrl-tab", NEXT_RECENT_TAB_COMMAND),
            ("ctrl-shift-tab", PREVIOUS_RECENT_TAB_COMMAND),
            ("ctrl-pagedown", NEXT_TAB_COMMAND),
            ("ctrl-pageup", PREVIOUS_TAB_COMMAND),
            ("ctrl-`", TOGGLE_TERMINAL_COMMAND),
            ("ctrl-shift-`", NEW_TERMINAL_COMMAND),
            ("ctrl-alt-b", TOGGLE_RIGHT_DOCK_COMMAND),
            ("ctrl-shift-t", REOPEN_CLOSED_TAB_COMMAND),
            ("alt-1", GO_TO_TAB_1_COMMAND),
            ("alt-9", GO_TO_LAST_TAB_COMMAND),
        ] {
            assert_eq!(
                resolved(&registry, key, Some("terminal")).as_deref(),
                Some(command_name),
                "{platform:?}: {key}"
            );
        }
    }
}

#[test]
fn macos_preset_binds_cocoa_editing_keys_in_the_editor_only() {
    let registry = preset_registry(Platform::MacOs);
    for (key, command_name) in [
        ("cmd-s", SAVE_COMMAND),
        ("cmd-shift-z", REDO_COMMAND),
        ("alt-left", MOVE_WORD_LEFT_COMMAND),
        ("cmd-up", MOVE_DOCUMENT_START_COMMAND),
        ("ctrl-a", MOVE_LINE_START_LOGICAL_COMMAND),
        ("ctrl-e", MOVE_LINE_END_LOGICAL_COMMAND),
        ("ctrl-f", MOVE_RIGHT_COMMAND),
        ("ctrl-b", MOVE_LEFT_COMMAND),
        ("ctrl-n", MOVE_DOWN_COMMAND),
        ("ctrl-p", MOVE_UP_COMMAND),
        ("ctrl-d", DELETE_FORWARD_COMMAND),
        ("ctrl-h", DELETE_BACKWARD_COMMAND),
        ("ctrl-k", DELETE_TO_LINE_END_COMMAND),
        ("ctrl-j", JOIN_LINES_COMMAND),
        ("cmd-alt-[", FOLD_COMMAND),
        ("cmd-alt-]", UNFOLD_COMMAND),
        ("cmd-left", MOVE_SMART_HOME_COMMAND),
        ("shift-cmd-left", SELECT_SMART_HOME_COMMAND),
    ] {
        assert_eq!(
            resolved(&registry, key, Some("editor")).as_deref(),
            Some(command_name),
            "{key}"
        );
    }
    assert_eq!(
        resolved(&registry, "cmd-c", Some("terminal")).as_deref(),
        Some(COPY_COMMAND)
    );
    for (key, command_name) in [
        ("cmd-}", NEXT_TAB_COMMAND),
        ("cmd-{", PREVIOUS_TAB_COMMAND),
        ("cmd-1", GO_TO_TAB_1_COMMAND),
        ("cmd-8", GO_TO_TAB_8_COMMAND),
        ("cmd-9", GO_TO_LAST_TAB_COMMAND),
        ("ctrl-tab", NEXT_RECENT_TAB_COMMAND),
        ("ctrl-`", TOGGLE_TERMINAL_COMMAND),
        ("ctrl-shift-`", NEW_TERMINAL_COMMAND),
        ("cmd-b", TOGGLE_LEFT_DOCK_COMMAND),
        ("cmd-alt-b", TOGGLE_RIGHT_DOCK_COMMAND),
        ("cmd-j", TOGGLE_BOTTOM_DOCK_COMMAND),
        ("cmd-shift-t", REOPEN_CLOSED_TAB_COMMAND),
    ] {
        assert_eq!(
            resolved(&registry, key, Some("terminal")).as_deref(),
            Some(command_name),
            "{key}"
        );
    }
    for key in ["ctrl-a", "ctrl-c", "ctrl-k", "ctrl-y"] {
        assert_eq!(resolved(&registry, key, Some("terminal")), None, "{key}");
    }
}

#[test]
fn movement_keys_move_by_visual_row_and_only_cocoa_paragraph_keys_ignore_wrapping() {
    for platform in PLATFORMS {
        let registry = preset_registry(platform);
        for (key, command_name) in [
            ("up", MOVE_UP_COMMAND),
            ("shift-up", SELECT_UP_COMMAND),
            ("down", MOVE_DOWN_COMMAND),
            ("shift-down", SELECT_DOWN_COMMAND),
            ("home", MOVE_SMART_HOME_COMMAND),
            ("shift-home", SELECT_SMART_HOME_COMMAND),
            ("end", MOVE_LINE_END_COMMAND),
            ("shift-end", SELECT_LINE_END_COMMAND),
            ("pageup", MOVE_PAGE_UP_COMMAND),
            ("shift-pageup", SELECT_PAGE_UP_COMMAND),
            ("pagedown", MOVE_PAGE_DOWN_COMMAND),
            ("shift-pagedown", SELECT_PAGE_DOWN_COMMAND),
        ] {
            assert_eq!(
                resolved(&registry, key, Some("editor")).as_deref(),
                Some(command_name),
                "{platform:?}: {key}"
            );
        }
        let mut logical = knot_preset(platform)
            .into_iter()
            .filter_map(|binding| {
                let command = binding.command?;
                let name = command.name.to_string();
                name.ends_with("-logical").then_some((binding.key, name))
            })
            .collect::<Vec<_>>();
        logical.sort();
        let expected: Vec<(String, String)> = if platform == Platform::MacOs {
            vec![
                ("ctrl-a".into(), MOVE_LINE_START_LOGICAL_COMMAND.into()),
                ("ctrl-e".into(), MOVE_LINE_END_LOGICAL_COMMAND.into()),
            ]
        } else {
            Vec::new()
        };
        assert_eq!(logical, expected, "{platform:?}");
    }
}

#[test]
fn every_platform_binds_the_editing_commands_in_the_editor_only() {
    for platform in PLATFORMS {
        let registry = preset_registry(platform);
        let secondary = if platform == Platform::MacOs {
            "cmd"
        } else {
            "ctrl"
        };
        for (key, command_name) in [
            ("enter".to_owned(), INSERT_NEWLINE_COMMAND),
            ("tab".to_owned(), INSERT_TAB_COMMAND),
            ("shift-tab".to_owned(), OUTDENT_LINES_COMMAND),
            (format!("{secondary}-]"), INDENT_LINES_COMMAND),
            (format!("{secondary}-["), OUTDENT_LINES_COMMAND),
            (format!("{secondary}-/"), TOGGLE_LINE_COMMENT_COMMAND),
            ("home".to_owned(), MOVE_SMART_HOME_COMMAND),
            ("shift-home".to_owned(), SELECT_SMART_HOME_COMMAND),
            ("alt-up".to_owned(), MOVE_LINES_UP_COMMAND),
            ("alt-down".to_owned(), MOVE_LINES_DOWN_COMMAND),
            (format!("{secondary}-shift-k"), DELETE_LINES_COMMAND),
            (format!("{secondary}-|"), JUMP_TO_MATCHING_BRACKET_COMMAND),
            ("ctrl-g".to_owned(), presets::GO_TO_LINE_COMMAND),
            (
                if platform == Platform::Linux {
                    "ctrl-shift-alt-up"
                } else {
                    "shift-alt-up"
                }
                .to_owned(),
                COPY_LINES_UP_COMMAND,
            ),
            (
                if platform == Platform::Linux {
                    "ctrl-shift-alt-down"
                } else {
                    "shift-alt-down"
                }
                .to_owned(),
                COPY_LINES_DOWN_COMMAND,
            ),
            (
                if platform == Platform::Linux {
                    "ctrl-shift-a"
                } else {
                    "shift-alt-a"
                }
                .to_owned(),
                TOGGLE_BLOCK_COMMENT_COMMAND,
            ),
        ] {
            assert_eq!(
                resolved(&registry, &key, Some("editor")).as_deref(),
                Some(command_name),
                "{platform:?}: {key}"
            );
            assert_eq!(
                resolved(&registry, &key, Some("terminal")),
                None,
                "{platform:?}: {key} in a terminal"
            );
        }
    }
}

#[gpui::test]
fn rebuilt_map_retains_fixed_config_error_binding(cx: &mut TestAppContext) {
    cx.update(install);
    let quit_key = "secondary-q";
    cx.update(|cx| {
        let owner = BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1));
        assert!(
            cx.global_mut::<ApplicationKeymaps>()
                .0
                .set(owner, quit_key, None, None)
                .unwrap()
        );
        rebuild(cx);
    });
    cx.read(|cx| {
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        let input = [gpui::Keystroke::parse_with_os(quit_key, cx.platform_info().os).unwrap()];
        let contexts = [KeyContext::parse("config-error").unwrap()];
        let (bindings, _) = keymap.bindings_for_input(&input, &contexts);
        assert_eq!(bindings.len(), 1);
        assert!(
            bindings[0]
                .action()
                .as_any()
                .is::<crate::app::commands::dispatch::ProductCommandSource>()
        );
    });
}

#[gpui::test]
fn menu_shortcuts_follow_the_effective_global_keymap(cx: &mut TestAppContext) {
    fn menu_key(cx: &gpui::App, command: &str) -> Option<String> {
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        let action = ProductCommandSource {
            name: command.to_owned().into(),
        };
        let keys = keymap
            .bindings_for_action(&action)
            .filter(|binding| {
                binding
                    .predicate()
                    .is_some_and(|predicate| predicate.to_string() == MENU_SHORTCUT_CONTEXT)
            })
            .map(|binding| {
                binding
                    .keystrokes()
                    .iter()
                    .map(|stroke| stroke.unparse(cx.platform_info().os))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        assert!(keys.len() <= 1, "{command} has several menu keys: {keys:?}");
        keys.into_iter().next()
    }
    let key = |source: &str| canonical_key(source).unwrap();
    // Windows and Linux terminals unbind ctrl with a printable character, so
    // their presets show no menu key for those bindings.
    cx.update(|cx| {
        let mut registry = BindingRegistry::new();
        registry.replace_preset(&knot_preset(Platform::MacOs));
        cx.set_global(ApplicationKeymaps(registry));
        rebuild(cx);
    });
    cx.read(|cx| {
        assert_eq!(menu_key(cx, SAVE_COMMAND), Some(key("cmd-s")));
        assert_eq!(menu_key(cx, SPLIT_HORIZONTAL_COMMAND), None);
        assert_eq!(menu_key(cx, MOVE_LEFT_COMMAND), None);
    });

    let personal = BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1));
    cx.update(|cx| {
        let registry = &mut cx.global_mut::<ApplicationKeymaps>().0;
        registry
            .set(personal, "cmd-s", None, Some(command(NEW_COMMAND)))
            .unwrap();
        registry
            .set(personal, "cmd-o", Some("editor"), None)
            .unwrap();
        rebuild(cx);
    });
    cx.read(|cx| {
        assert_eq!(menu_key(cx, SAVE_COMMAND), None);
        // Equally short keys show the latest write.
        assert_eq!(menu_key(cx, NEW_COMMAND), Some(key("cmd-s")));
        assert_eq!(menu_key(cx, OPEN_COMMAND), None);
    });

    cx.update(|cx| {
        cx.global_mut::<ApplicationKeymaps>()
            .0
            .replace_preset(&[PresetBinding::new("cmd-g".into(), Some(SAVE_COMMAND), None)]);
        rebuild(cx);
    });
    cx.read(|cx| {
        assert_eq!(menu_key(cx, SAVE_COMMAND), Some(key("cmd-g")));
        assert_eq!(menu_key(cx, NEW_COMMAND), Some(key("cmd-s")));
    });
}

#[test]
fn every_preset_binds_zoom_and_pc_terminals_leave_it_to_the_shell() {
    for platform in PLATFORMS {
        let registry = preset_registry(platform);
        let secondary = if platform == Platform::MacOs {
            "cmd"
        } else {
            "ctrl"
        };
        for (key, command_name) in [
            ("=", ZOOM_IN_COMMAND),
            ("+", ZOOM_IN_COMMAND),
            ("-", ZOOM_OUT_COMMAND),
            ("0", RESET_ZOOM_COMMAND),
        ] {
            let key = format!("{secondary}-{key}");
            for view in [None, Some("editor")] {
                assert_eq!(
                    resolved(&registry, &key, view).as_deref(),
                    Some(command_name),
                    "{platform:?}: {key} in {view:?}"
                );
            }
            let in_terminal = if platform == Platform::MacOs {
                command_name
            } else {
                "<unbound>"
            };
            assert_eq!(
                resolved(&registry, &key, Some("terminal")).as_deref(),
                Some(in_terminal),
                "{platform:?}: {key} in a terminal"
            );
        }
    }
}
