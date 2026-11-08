use super::*;
use crate::keys::Modifiers;

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

#[test]
fn extension_global_beats_editor_default_and_editor_unbind_leaves_terminal_binding() {
    let mut registry = BindingRegistry::new();
    let personal = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry
        .set(
            BindingOwner::Preset,
            "Cmd-C",
            Some("editor"),
            Some(command("copy")),
        )
        .unwrap();
    registry
        .set(
            BindingOwner::Preset,
            "Cmd-C",
            Some("terminal"),
            Some(command("terminal.copy")),
        )
        .unwrap();
    registry
        .set(personal, "Cmd-C", None, Some(command("personal.copy")))
        .unwrap();
    assert_eq!(
        resolved(&registry, "cmd-c", Some("editor")).as_deref(),
        Some("personal.copy")
    );
    registry.remove(personal, "cmd-c", None).unwrap();
    registry
        .set(personal, "cmd-c", Some("editor"), None)
        .unwrap();
    assert_eq!(
        resolved(&registry, "cmd-c", Some("editor")).as_deref(),
        Some("<unbound>")
    );
    assert_eq!(
        resolved(&registry, "cmd-c", Some("terminal")).as_deref(),
        Some("terminal.copy")
    );
}

#[test]
fn owner_replacement_removal_and_lifecycle_cleanup_reveal_earlier_writes() {
    let mut registry = BindingRegistry::new();
    let extension = BindingOwner::Extension(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    let personal = BindingOwner::Extension(ExtensionId::new(3), ExtensionLifecycleId::new(1));
    registry
        .set(BindingOwner::Preset, "cmd-a", None, Some(command("native")))
        .unwrap();
    registry
        .set(extension, "cmd-a", None, Some(command("first")))
        .unwrap();
    registry
        .set(extension, "cmd-a", None, Some(command("second")))
        .unwrap();
    assert_eq!(
        resolved(&registry, "cmd-a", Some("editor")).as_deref(),
        Some("second")
    );
    registry.set(personal, "cmd-a", None, None).unwrap();
    assert_eq!(
        resolved(&registry, "cmd-a", Some("editor")).as_deref(),
        Some("<unbound>")
    );
    assert!(registry.remove_owner(personal));
    assert_eq!(
        resolved(&registry, "cmd-a", None).as_deref(),
        Some("second")
    );
    assert!(registry.remove_owner(extension));
    assert_eq!(
        resolved(&registry, "cmd-a", None).as_deref(),
        Some("native")
    );
    assert!(!registry.remove_owner(extension));
}

#[test]
fn shadowed_writes_update_the_one_active_map_only_when_visible() {
    let mut registry = BindingRegistry::new();
    let extension = BindingOwner::Extension(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    let personal = BindingOwner::Extension(ExtensionId::new(3), ExtensionLifecycleId::new(1));
    assert!(
        registry
            .set(extension, "cmd-a", None, Some(command("extension")))
            .unwrap()
    );
    assert!(
        registry
            .set(personal, "cmd-a", None, Some(command("personal")))
            .unwrap()
    );
    // A native default beneath extension bindings leaves the active map
    // unchanged.
    assert!(
        !registry
            .set(BindingOwner::Preset, "cmd-a", None, Some(command("native")))
            .unwrap()
    );
    assert_eq!(registry.keys(), vec![canonical_key("cmd-a").unwrap()]);
    // A later write beats an earlier owner's, and removal reveals it again.
    assert!(
        registry
            .set(extension, "cmd-a", None, Some(command("updated")))
            .unwrap()
    );
    assert_eq!(
        resolved(&registry, "cmd-a", Some("editor")).as_deref(),
        Some("updated")
    );
    assert!(registry.remove_owner(extension));
    assert_eq!(
        resolved(&registry, "cmd-a", Some("editor")).as_deref(),
        Some("personal")
    );
    assert!(registry.remove_owner(personal));
    assert_eq!(
        resolved(&registry, "cmd-a", Some("editor")).as_deref(),
        Some("native")
    );
}

#[test]
fn later_extension_and_same_source_conflicts_follow_registration_order() {
    let mut registry = BindingRegistry::new();
    let first = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    let second = BindingOwner::Extension(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    registry
        .set(first, "cmd-z", None, Some(command("first")))
        .unwrap();
    registry
        .set(second, "cmd-z", None, Some(command("second")))
        .unwrap();
    assert_eq!(
        resolved(&registry, "cmd-z", Some("editor")).as_deref(),
        Some("second")
    );
    registry
        .set(first, "cmd-z", Some("editor"), Some(command("specific")))
        .unwrap();
    assert_eq!(
        resolved(&registry, "cmd-z", Some("editor")).as_deref(),
        Some("specific")
    );
    registry
        .set(first, "cmd-z", None, Some(command("latest")))
        .unwrap();
    assert_eq!(
        resolved(&registry, "cmd-z", Some("editor")).as_deref(),
        Some("specific")
    );
    assert_eq!(
        resolved(&registry, "cmd-z", Some("terminal")).as_deref(),
        Some("latest")
    );
}

#[test]
fn declared_kinds_and_sequences_are_validated_and_normalized() {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Preset;
    assert_eq!(
        registry.set(owner, "", None, Some(command("x"))),
        Err(BindingError::Key)
    );
    assert_eq!(
        registry.set(owner, "cmd-foo-bar", None, Some(command("x"))),
        Err(BindingError::Key)
    );
    assert_eq!(
        registry.set(owner, "cmd-a", Some("unknown"), Some(command("x"))),
        Err(BindingError::View)
    );
    assert_eq!(
        registry.declare_extension_kind("editor"),
        Err(BindingError::View)
    );
    registry.declare_extension_kind("outline").unwrap();
    registry
        .set(
            owner,
            "Cmd-K Right",
            Some("outline"),
            Some(command("split")),
        )
        .unwrap();
    assert_eq!(
        registry.keys(),
        vec![if cfg!(target_os = "macos") {
            "cmd-k right"
        } else if cfg!(windows) {
            "win-k right"
        } else {
            "super-k right"
        }]
    );
    assert_eq!(
        resolved(&registry, "cmd-k right", Some("outline")).as_deref(),
        Some("split")
    );
    assert_eq!(resolved(&registry, "cmd-k right", Some("editor")), None);
}

#[test]
fn bare_uppercase_key_preserves_implicit_shift() {
    let mut registry = BindingRegistry::new();
    registry
        .set(BindingOwner::Preset, "A", None, Some(command("shifted")))
        .unwrap();
    assert_eq!(registry.keys(), vec!["shift-a"]);
    assert_eq!(
        resolved(&registry, "shift-a", Some("editor")).as_deref(),
        Some("shifted")
    );
    assert_eq!(resolved(&registry, "a", Some("editor")), None);
}

fn typed(keys: &str) -> Vec<Keystroke> {
    keys.split_whitespace()
        .map(|key| Keystroke::parse(key).unwrap().with_simulated_text())
        .collect()
}

fn matched(
    registry: &BindingRegistry,
    keys: &str,
    view: Option<&str>,
    contexts: &[&str],
) -> (Option<String>, bool) {
    let result = registry.match_typed(&typed(keys), view, contexts);
    (
        result.exact.map(|command| command.name.to_string()),
        result.longer,
    )
}

#[test]
fn typed_keys_match_prefixes_only_where_a_binding_of_them_resolves() {
    let mut registry = BindingRegistry::new();
    registry.declare_extension_kind("outline").unwrap();
    registry
        .set(
            BindingOwner::Preset,
            "cmd-k right",
            Some("editor"),
            Some(command("split")),
        )
        .unwrap();
    registry
        .set(
            BindingOwner::Preset,
            "cmd-j",
            Some("outline"),
            Some(command("copy")),
        )
        .unwrap();
    assert_eq!(
        matched(&registry, "cmd-k", Some("editor"), &[]),
        (None, true)
    );
    assert_eq!(
        matched(&registry, "cmd-k right", Some("editor"), &[]),
        (Some("split".into()), false)
    );
    assert_eq!(
        matched(&registry, "cmd-k", Some("terminal"), &[]),
        (None, false)
    );
    assert_eq!(matched(&registry, "cmd-k", None, &[]), (None, false));
    assert_eq!(
        matched(&registry, "cmd-j", Some("outline"), &[]),
        (Some("copy".into()), false)
    );

    // A sequence bound in a context begins only where the context is on.
    let modal = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry
        .set_in_context(
            modal,
            "z b",
            Some("editor"),
            Some("modal"),
            Some(command("bottom")),
        )
        .unwrap();
    assert_eq!(matched(&registry, "z", Some("editor"), &[]), (None, false));
    assert_eq!(
        matched(&registry, "z", Some("editor"), &["modal"]),
        (None, true)
    );
    assert_eq!(
        matched(&registry, "z b", Some("editor"), &["modal"]),
        (Some("bottom".into()), false)
    );
}

#[test]
fn unbound_keys_match_nothing_in_their_scope() {
    let mut registry = BindingRegistry::new();
    let personal = BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1));
    registry
        .set(BindingOwner::Preset, "cmd-n", None, Some(command("new")))
        .unwrap();
    registry
        .set(personal, "cmd-n", Some("editor"), None)
        .unwrap();
    assert_eq!(
        matched(&registry, "cmd-n", None, &[]),
        (Some("new".into()), false)
    );
    assert_eq!(
        matched(&registry, "cmd-n", Some("terminal"), &[]),
        (Some("new".into()), false)
    );
    assert_eq!(
        matched(&registry, "cmd-n", Some("editor"), &[]),
        (None, false)
    );
}

#[test]
fn a_typed_character_matches_the_binding_of_that_character() {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry
        .set(owner, "?", None, Some(command("help")))
        .unwrap();
    registry
        .set(owner, "shift-a", None, Some(command("upper")))
        .unwrap();
    let question = Keystroke {
        key: "/".into(),
        modifiers: Modifiers::shift(),
        key_char: Some("?".into()),
    };
    assert_eq!(
        registry
            .match_typed(&[question], None, &[])
            .exact
            .map(|c| c.name.to_string()),
        Some("help".into())
    );
    assert_eq!(
        matched(&registry, "shift-a", None, &[]),
        (Some("upper".into()), false)
    );
}

#[test]
fn batch_is_ordered_atomic_and_reports_only_effective_changes() {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1));
    let set = |name: &str| KeybindingMutation::Set {
        key: "cmd-c".into(),
        view: None,
        context: None,
        command: Some(command(name)),
    };
    assert!(
        registry
            .apply_batch(owner, &[set("first"), set("second")])
            .unwrap()
    );
    assert_eq!(
        resolved(&registry, "cmd-c", None).as_deref(),
        Some("second")
    );
    assert!(
        !registry
            .apply_batch(owner, &[set("temporary"), set("second")])
            .unwrap()
    );
    let error = registry
        .apply_batch(
            owner,
            &[
                set("uncommitted"),
                KeybindingMutation::Set {
                    key: "bad-modifier-key".into(),
                    view: None,
                    context: None,
                    command: Some(command("invalid")),
                },
            ],
        )
        .unwrap_err();
    assert!(error.contains("operation 2"));
    assert_eq!(
        resolved(&registry, "cmd-c", None).as_deref(),
        Some("second")
    );
    assert!(
        registry
            .apply_batch(
                owner,
                &[KeybindingMutation::Remove {
                    key: "cmd-c".into(),
                    view: None,
                    context: None,
                }],
            )
            .unwrap()
    );
    assert_eq!(resolved(&registry, "cmd-c", None), None);
}

#[test]
fn shown_bindings_prefer_short_view_scoped_and_latest_bindings() {
    let mut registry = BindingRegistry::new();
    let personal = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    for (owner, key, view, name) in [
        (BindingOwner::Preset, "ctrl-k ctrl-s", None, "file.save"),
        (BindingOwner::Preset, "ctrl-s", None, "file.save"),
        (BindingOwner::Preset, "ctrl-b", None, "first"),
        (BindingOwner::Preset, "ctrl-e", None, "first"),
        (BindingOwner::Preset, "ctrl-c", None, "copy"),
        (
            BindingOwner::Preset,
            "ctrl-c",
            Some("terminal"),
            "terminal.copy",
        ),
        (personal, "ctrl-j", Some("editor"), "editor.only"),
    ] {
        registry.set(owner, key, view, Some(command(name))).unwrap();
    }
    registry
        .set(
            personal,
            "ctrl-g",
            None,
            Some(Command {
                name: "with.arguments".into(),
                arguments: CommandArgumentValue::String("x".into()),
            }),
        )
        .unwrap();

    let editor = registry.shown_bindings(Some("editor"));
    assert_eq!(editor["file.save"], "ctrl-s");
    // Equal length: the later write wins.
    assert_eq!(editor["first"], "ctrl-e");
    assert_eq!(editor["copy"], "ctrl-c");
    assert_eq!(editor["editor.only"], "ctrl-j");
    assert!(!editor.contains_key("with.arguments"));

    let terminal = registry.shown_bindings(Some("terminal"));
    assert_eq!(terminal["terminal.copy"], "ctrl-c");
    assert!(!terminal.contains_key("copy"));
    assert!(!terminal.contains_key("editor.only"));
}

#[test]
fn keys_display_in_platform_style() {
    for (key, mac, other) in [
        ("cmd-shift-p", "⇧⌘P", "Shift+Super+P"),
        ("cmd-k right", "⌘K →", "Super+K Right"),
        ("ctrl-alt-enter", "⌃⌥↩", "Ctrl+Alt+Enter"),
        ("ctrl-k ctrl-s", "⌃K ⌃S", "Ctrl+K Ctrl+S"),
        ("f5", "F5", "F5"),
        ("ctrl-space", "⌃Space", "Ctrl+Space"),
    ] {
        let canonical = canonical_key(key).unwrap();
        assert_eq!(display_key_for(&canonical, true), mac, "{key}");
        if !cfg!(windows) {
            assert_eq!(display_key_for(&canonical, false), other, "{key}");
        }
    }
}

fn preset(name: &str, bindings: &[(&str, Option<&str>, Option<&str>)]) -> KeymapPreset {
    KeymapPreset {
        name: name.into(),
        title: name.to_uppercase(),
        bindings: bindings
            .iter()
            .map(|&(key, command, view)| PresetBinding {
                key: key.into(),
                command: command.map(self::command),
                view: view.map(ToOwned::to_owned),
            })
            .collect(),
    }
}

fn preset_owner(lifecycle: u64) -> PresetOwner {
    PresetOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

#[test]
fn latest_preset_registration_wins_and_removal_reveals_the_previous_one() {
    let mut presets = PresetRegistry::new(Vec::new());
    let first = presets.register(
        preset_owner(1),
        preset("emacs", &[("ctrl-x ctrl-s", Some("first"), None)]),
    );
    presets.register(
        preset_owner(2),
        preset("emacs", &[("ctrl-x ctrl-s", Some("second"), None)]),
    );
    presets.register(preset_owner(2), preset("vscode", &[]));
    let command_of = |presets: &PresetRegistry| {
        presets.get("emacs").unwrap().bindings[0]
            .command
            .clone()
            .unwrap()
            .name
            .to_string()
    };
    assert_eq!(command_of(&presets), "second");
    assert_eq!(
        presets
            .list()
            .into_iter()
            .map(|info| (info.name, info.title))
            .collect::<Vec<_>>(),
        [
            ("emacs".to_owned(), "EMACS".to_owned()),
            ("knot".to_owned(), "Knot".to_owned()),
            ("vscode".to_owned(), "VSCODE".to_owned()),
        ]
    );
    presets.remove_lifecycle(preset_owner(2));
    assert_eq!(command_of(&presets), "first");
    assert!(presets.get("vscode").is_none());
    assert!(!presets.unregister(preset_owner(2), first));
    assert!(presets.unregister(preset_owner(1), first));
    assert!(presets.get("emacs").is_none());
}

#[test]
fn knot_is_native_and_the_fallback_for_unknown_selections() {
    let native = vec![PresetBinding::new(
        "cmd-k".into(),
        Some("knot.command"),
        None,
    )];
    let mut presets = PresetRegistry::new(native.clone());
    let knot = presets.get(KNOT_PRESET).unwrap();
    assert_eq!(knot.bindings, native);
    assert_eq!(
        presets.select("missing".into(), false),
        Some(native.clone())
    );
    assert_eq!(presets.select("missing".into(), false), None);
    assert_eq!(presets.select("missing".into(), true), Some(native));
    assert_eq!(
        preset(KNOT_PRESET, &[]).invalid_value(),
        Some("the knot keymap preset cannot be replaced")
    );
    assert!(preset("", &[]).invalid_value().is_some());
    assert!(
        preset("bad", &[("bad-modifier-key", Some("x"), None)])
            .invalid_value()
            .is_some()
    );
    assert!(
        preset("bad", &[("cmd-x", Some(""), None)])
            .invalid_value()
            .is_some()
    );
    assert_eq!(
        preset("good", &[("cmd-x", None, Some("editor"))]).invalid_value(),
        None
    );
}

#[test]
fn replacing_the_preset_keeps_extension_bindings_on_top_and_skips_unknown_views() {
    let mut registry = BindingRegistry::new();
    let personal = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry.replace_preset(
        &preset(
            "a",
            &[
                ("cmd-s", Some("save"), None),
                ("cmd-j", Some("preset.j"), None),
            ],
        )
        .bindings,
    );
    registry
        .set(personal, "cmd-j", None, Some(command("personal.j")))
        .unwrap();
    assert!(
        registry.replace_preset(
            &preset(
                "b",
                &[
                    ("cmd-r", Some("run"), None),
                    ("cmd-j", Some("other.j"), None),
                    ("cmd-o", Some("outline.open"), Some("example.outline")),
                ],
            )
            .bindings
        )
    );
    assert_eq!(resolved(&registry, "cmd-s", None), None);
    assert_eq!(resolved(&registry, "cmd-r", None).as_deref(), Some("run"));
    assert_eq!(
        resolved(&registry, "cmd-j", None).as_deref(),
        Some("personal.j")
    );
    assert_eq!(resolved(&registry, "cmd-o", Some("example.outline")), None);
    assert!(
        !registry.replace_preset(
            &preset(
                "b",
                &[
                    ("cmd-r", Some("run"), None),
                    ("cmd-j", Some("other.j"), None)
                ],
            )
            .bindings
        )
    );
}

#[test]
fn listing_names_each_slot_and_the_owner_that_overrides_it() {
    let mut registry = BindingRegistry::new();
    let package = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry.replace_preset(
        &preset(
            "p",
            &[
                ("ctrl-s", Some("save"), None),
                ("ctrl-c", Some("copy"), None),
                ("ctrl-k", Some("kill"), Some("editor")),
            ],
        )
        .bindings,
    );
    registry
        .set(package, "ctrl-s", None, Some(command("package.save")))
        .unwrap();
    registry
        .set(package, "ctrl-c", Some("terminal"), None)
        .unwrap();
    let rows = registry
        .listing()
        .into_iter()
        .map(|row| {
            (
                row.key,
                row.view,
                row.owner,
                row.command.map(|command| command.name.to_string()),
                row.shadowed_by,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            (
                "ctrl-c".into(),
                None,
                BindingOwner::Preset,
                Some("copy".into()),
                None
            ),
            (
                "ctrl-c".into(),
                Some("terminal".into()),
                package,
                None,
                None
            ),
            (
                "ctrl-k".into(),
                Some("editor".into()),
                BindingOwner::Preset,
                Some("kill".into()),
                None
            ),
            (
                "ctrl-s".into(),
                None,
                package,
                Some("package.save".into()),
                None
            ),
            (
                "ctrl-s".into(),
                None,
                BindingOwner::Preset,
                Some("save".into()),
                Some(package)
            ),
        ]
    );
}

fn resolved_in(
    registry: &BindingRegistry,
    key: &str,
    view: Option<&str>,
    contexts: &[&str],
) -> Option<String> {
    match registry.resolve_in(&canonical_key(key).unwrap(), view, contexts) {
        Some(BindingResolution::Command(command)) => Some(command.name.to_string()),
        Some(BindingResolution::Unbound) => Some("<unbound>".into()),
        None => None,
    }
}

#[test]
fn context_slots_win_by_fact_count_then_latest_write() {
    let mut registry = BindingRegistry::new();
    let modal = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    let other = BindingOwner::Extension(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    registry
        .set(
            BindingOwner::Preset,
            "h",
            Some("editor"),
            Some(command("preset")),
        )
        .unwrap();
    registry
        .set_in_context(
            modal,
            "h",
            Some("editor"),
            Some("normal"),
            Some(command("left")),
        )
        .unwrap();
    // Written later but requiring fewer facts.
    registry
        .set(other, "h", Some("editor"), Some(command("view")))
        .unwrap();
    registry
        .set(other, "h", None, Some(command("global")))
        .unwrap();
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &["normal"]).as_deref(),
        Some("left")
    );
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &[]).as_deref(),
        Some("view")
    );
    assert_eq!(
        resolved_in(&registry, "h", Some("terminal"), &["normal"]).as_deref(),
        Some("global")
    );
    // Among context slots of a view holding both contexts, the latest wins.
    registry
        .set_in_context(
            other,
            "h",
            Some("editor"),
            Some("visual"),
            Some(command("extend")),
        )
        .unwrap();
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &["normal", "visual"]).as_deref(),
        Some("extend")
    );
    registry
        .set_in_context(
            modal,
            "h",
            Some("editor"),
            Some("normal"),
            Some(command("left again")),
        )
        .unwrap();
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &["normal", "visual"]).as_deref(),
        Some("left again")
    );
    // A context slot masks with null, and teardown reveals the earlier write.
    registry
        .set_in_context(other, "h", Some("editor"), Some("normal"), None)
        .unwrap();
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &["normal"]).as_deref(),
        Some("<unbound>")
    );
    assert!(registry.remove_owner(other));
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &["normal"]).as_deref(),
        Some("left again")
    );
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &[]).as_deref(),
        Some("preset")
    );
    assert!(
        registry
            .remove_in_context(modal, "h", Some("editor"), Some("normal"))
            .unwrap()
    );
    assert_eq!(
        resolved_in(&registry, "h", Some("editor"), &["normal"]).as_deref(),
        Some("preset")
    );
}

#[test]
fn a_context_needs_a_view_and_a_name() {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    for (view, context) in [(None, "normal"), (Some("editor"), "")] {
        assert_eq!(
            registry.set_in_context(owner, "h", view, Some(context), Some(command("left"))),
            Err(BindingError::Context)
        );
    }
    let error = registry
        .apply_batch(
            owner,
            &[KeybindingMutation::Set {
                key: "h".into(),
                view: None,
                context: Some("normal".into()),
                command: Some(command("left")),
            }],
        )
        .unwrap_err();
    assert!(error.contains("needs a view"), "{error}");
}

#[test]
fn context_slots_beat_an_unbinding_of_their_view_and_list_unshadowed() {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry
        .set_in_context(
            owner,
            "h",
            Some("editor"),
            Some("normal"),
            Some(command("left")),
        )
        .unwrap();
    registry
        .set(owner, "h", None, Some(command("global")))
        .unwrap();
    registry.set(owner, "h", Some("editor"), None).unwrap();
    assert_eq!(matched(&registry, "h", Some("editor"), &[]), (None, false));
    assert_eq!(
        matched(&registry, "h", Some("editor"), &["normal"]),
        (Some("left".into()), false)
    );
    assert_eq!(
        matched(&registry, "h", None, &[]),
        (Some("global".into()), false)
    );
    let listing = registry.listing();
    let context_slot = listing
        .iter()
        .find(|binding| binding.context.as_deref() == Some("normal"))
        .unwrap();
    assert_eq!(context_slot.shadowed_by, None);
    assert!(
        listing
            .iter()
            .filter(|binding| binding.context.is_none() && binding.view.is_some())
            .all(|binding| binding.shadowed_by.is_none())
    );
}

#[test]
fn text_bindings_win_by_fact_count_then_latest_write_and_mask_with_null() {
    let mut registry = BindingRegistry::new();
    let modal = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    let other = BindingOwner::Extension(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    let name = |command: Option<CommandName>| command.map(|name| name.to_string());
    assert_eq!(registry.resolve_text("editor", &["normal"]), None);
    registry
        .set_text(
            modal,
            Some("editor"),
            Some("normal"),
            Some("normal.text".into()),
        )
        .unwrap();
    registry
        .set_text(other, Some("editor"), None, Some("editor.text".into()))
        .unwrap();
    assert_eq!(
        name(registry.resolve_text("editor", &["normal"])).as_deref(),
        Some("normal.text")
    );
    assert_eq!(
        name(registry.resolve_text("editor", &[])).as_deref(),
        Some("editor.text")
    );
    // Null masks the lower binding, so the view types natively.
    registry
        .set_text(other, Some("editor"), Some("normal"), None)
        .unwrap();
    assert_eq!(registry.resolve_text("editor", &["normal"]), None);
    let listing = registry.listing();
    let text_rows = listing.iter().filter(|binding| binding.text).count();
    assert_eq!(text_rows, 3);
    assert!(listing.iter().any(|binding| binding.text
        && binding.owner == modal
        && binding.shadowed_by == Some(other)));
    // Teardown reveals the earlier write; removal is per scope.
    assert!(!registry.remove_owner(other));
    assert_eq!(
        name(registry.resolve_text("editor", &["normal"])).as_deref(),
        Some("normal.text")
    );
    registry
        .remove_text(modal, Some("editor"), Some("normal"))
        .unwrap();
    assert_eq!(registry.resolve_text("editor", &["normal"]), None);
    assert!(registry.listing().is_empty());
}

#[test]
fn text_binding_batches_validate_scope_and_command() {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    for (view, context, command, expected) in [
        (None, Some("normal"), Some("t"), "needs a view"),
        (Some("unknown"), None, Some("t"), "unknown view kind"),
        (Some("editor"), None, Some(""), "command name is empty"),
    ] {
        let error = registry
            .apply_batch(
                owner,
                &[KeybindingMutation::SetText {
                    view: view.map(Into::into),
                    context: context.map(Into::into),
                    command: command.map(Into::into),
                }],
            )
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    assert!(
        !registry
            .apply_batch(
                owner,
                &[KeybindingMutation::SetText {
                    view: Some("editor".into()),
                    context: None,
                    command: Some("t".into()),
                }],
            )
            .unwrap()
    );
    assert!(registry.resolve_text("editor", &[]).is_some());
}
