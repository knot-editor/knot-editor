use super::*;

fn owner(lifecycle: u64) -> DecorationOwner {
    DecorationOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

fn uri(text: &str) -> ResourceUri {
    ResourceUri::parse(text).unwrap()
}

fn color(key: &str) -> Decoration {
    Decoration {
        color: Some(key.into()),
        ..Decoration::default()
    }
}

fn propagating(key: &str) -> Decoration {
    Decoration {
        propagate: true,
        ..color(key)
    }
}

fn set(registry: &mut DecorationRegistry, source: u64, text: &str, decoration: Decoration) {
    registry
        .set(owner(1), source, vec![(uri(text), Some(decoration))])
        .unwrap();
}

fn color_of(registry: &DecorationRegistry, text: &str) -> Option<String> {
    registry.lookup(&uri(text)).and_then(|d| d.color.clone())
}

#[test]
fn the_latest_source_wins_among_equals_and_severity_wins_over_both() {
    let mut registry = DecorationRegistry::default();
    let first = registry.create_source(owner(1));
    let second = registry.create_source(owner(1));
    set(&mut registry, first, "file:///a", color("vcs.added"));
    set(&mut registry, second, "file:///a", color("vcs.modified"));
    assert_eq!(color_of(&registry, "file:///a").unwrap(), "vcs.modified");
    set(&mut registry, first, "file:///a", color("warning"));
    assert_eq!(color_of(&registry, "file:///a").unwrap(), "warning");
    set(&mut registry, second, "file:///a", color("error"));
    set(&mut registry, first, "file:///a", color("error"));
    assert_eq!(color_of(&registry, "file:///a").unwrap(), "error");
}

#[test]
fn color_badge_and_tooltip_merge_separately() {
    let mut registry = DecorationRegistry::default();
    let first = registry.create_source(owner(1));
    let second = registry.create_source(owner(1));
    set(
        &mut registry,
        first,
        "file:///a",
        Decoration {
            color: Some("warning".into()),
            badge: Some(Badge::Text("W".into())),
            tooltip: Some("one".into()),
            propagate: false,
        },
    );
    set(
        &mut registry,
        second,
        "file:///a",
        Decoration {
            color: Some("vcs.added".into()),
            badge: Some(Badge::Icon("vcs.added".into())),
            tooltip: Some("two".into()),
            propagate: false,
        },
    );
    let resolved = registry.lookup(&uri("file:///a")).unwrap();
    assert_eq!(resolved.color.as_deref(), Some("warning"));
    // An icon badge and a text badge compete as one field; the badge of the
    // more severe decoration wins.
    assert_eq!(resolved.badge, Some(Badge::Text("W".into())));
    assert_eq!(resolved.tooltip.as_deref(), Some("one\ntwo"));
}

#[test]
fn a_badge_competes_only_with_other_badges() {
    let mut registry = DecorationRegistry::default();
    let first = registry.create_source(owner(1));
    let second = registry.create_source(owner(1));
    set(
        &mut registry,
        first,
        "file:///a",
        Decoration {
            badge: Some(Badge::Text("M".into())),
            ..Decoration::default()
        },
    );
    set(&mut registry, second, "file:///a", color("error"));
    let resolved = registry.lookup(&uri("file:///a")).unwrap();
    assert_eq!(resolved.badge, Some(Badge::Text("M".into())));
    assert_eq!(resolved.color.as_deref(), Some("error"));
}

#[test]
fn propagation_reaches_ancestors_of_the_same_authority_only() {
    let mut registry = DecorationRegistry::default();
    let source = registry.create_source(owner(1));
    set(
        &mut registry,
        source,
        "file:///repo/src/main.rs",
        Decoration {
            badge: Some(Badge::Text("M".into())),
            tooltip: Some("Modified".into()),
            ..propagating("vcs.modified")
        },
    );
    for ancestor in ["file:///repo/src", "file:///repo", "file:///"] {
        let resolved = registry.lookup(&uri(ancestor)).unwrap();
        // Only the color propagates, never the badge or tooltip.
        assert_eq!(
            resolved,
            &ResolvedDecoration {
                color: Some("vcs.modified".into()),
                ..ResolvedDecoration::default()
            }
        );
    }
    assert!(registry.lookup(&uri("file:///repo/other")).is_none());
    assert!(registry.lookup(&uri("mem:///repo")).is_none());

    set(
        &mut registry,
        source,
        "mem://host/a/b",
        propagating("vcs.added"),
    );
    assert!(registry.lookup(&uri("mem://host/a")).is_some());
    assert!(registry.lookup(&uri("mem://other/a")).is_none());
    assert!(registry.lookup(&uri("file:///a")).is_none());
}

#[test]
fn a_direct_color_beats_a_propagated_one_and_a_propagated_one_fills_in() {
    let mut registry = DecorationRegistry::default();
    let source = registry.create_source(owner(1));
    set(
        &mut registry,
        source,
        "file:///repo/a.rs",
        propagating("error"),
    );
    set(&mut registry, source, "file:///repo", color("vcs.added"));
    assert_eq!(color_of(&registry, "file:///repo").unwrap(), "vcs.added");
    // A direct decoration without a color takes the propagated one.
    set(
        &mut registry,
        source,
        "file:///repo",
        Decoration {
            badge: Some(Badge::Text("D".into())),
            ..Decoration::default()
        },
    );
    let resolved = registry.lookup(&uri("file:///repo")).unwrap();
    assert_eq!(resolved.color.as_deref(), Some("error"));
    assert_eq!(resolved.badge, Some(Badge::Text("D".into())));
}

#[test]
fn propagated_colors_meeting_on_an_ancestor_rank_by_severity_then_latest() {
    let mut registry = DecorationRegistry::default();
    let first = registry.create_source(owner(1));
    let second = registry.create_source(owner(1));
    set(&mut registry, first, "file:///r/a", propagating("warning"));
    set(
        &mut registry,
        second,
        "file:///r/b",
        propagating("vcs.added"),
    );
    assert_eq!(color_of(&registry, "file:///r").unwrap(), "warning");
    set(
        &mut registry,
        first,
        "file:///r/a",
        propagating("vcs.modified"),
    );
    assert_eq!(color_of(&registry, "file:///r").unwrap(), "vcs.added");
}

#[test]
fn removal_undoes_propagation() {
    let mut registry = DecorationRegistry::default();
    let source = registry.create_source(owner(1));
    set(&mut registry, source, "file:///r/a", propagating("error"));
    assert!(registry.lookup(&uri("file:///r")).is_some());
    assert_eq!(
        registry.set(owner(1), source, vec![(uri("file:///r/a"), None)]),
        Some(true)
    );
    assert!(registry.lookup(&uri("file:///r")).is_none());
    assert!(registry.lookup(&uri("file:///r/a")).is_none());
}

#[test]
fn set_changes_only_the_named_uris_and_reports_changes() {
    let mut registry = DecorationRegistry::default();
    let source = registry.create_source(owner(1));
    set(&mut registry, source, "file:///a", color("error"));
    set(&mut registry, source, "file:///b", color("warning"));
    assert_eq!(
        registry.set(
            owner(1),
            source,
            vec![(uri("file:///b"), Some(color("warning")))]
        ),
        Some(false)
    );
    set(&mut registry, source, "file:///c", color("info"));
    assert!(registry.lookup(&uri("file:///a")).is_some());
    assert_eq!(registry.clear(owner(1), source), Some(true));
    assert_eq!(registry.clear(owner(1), source), Some(false));
    assert!(registry.lookup(&uri("file:///a")).is_none());
}

#[test]
fn another_owner_cannot_change_a_source() {
    let mut registry = DecorationRegistry::default();
    let source = registry.create_source(owner(1));
    assert_eq!(registry.set(owner(2), source, vec![]), None);
    assert_eq!(registry.clear(owner(2), source), None);
    assert!(!registry.remove_source(owner(2), source));
    assert!(registry.remove_source(owner(1), source));
}

#[test]
fn removing_a_source_or_lifecycle_removes_its_decorations() {
    let mut registry = DecorationRegistry::default();
    let first = registry.create_source(owner(1));
    let second = registry.create_source(owner(2));
    set(&mut registry, first, "file:///a", color("vcs.added"));
    registry
        .set(
            owner(2),
            second,
            vec![(uri("file:///a"), Some(color("vcs.modified")))],
        )
        .unwrap();
    assert_eq!(color_of(&registry, "file:///a").unwrap(), "vcs.modified");
    assert!(registry.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(2)));
    assert_eq!(color_of(&registry, "file:///a").unwrap(), "vcs.added");
    assert!(registry.remove_source(owner(1), first));
    assert!(registry.lookup(&uri("file:///a")).is_none());
    assert!(!registry.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(1)));
}

#[test]
fn badge_text_holds_one_or_two_grapheme_clusters() {
    let text = |text: &str| Decoration {
        badge: Some(Badge::Text(text.into())),
        ..Decoration::default()
    };
    assert!(text("M").is_valid());
    assert!(text("MM").is_valid());
    assert!(text("👨‍👩‍👧é").is_valid());
    assert!(!text("").is_valid());
    assert!(!text("MMM").is_valid());
    assert!(
        !Decoration {
            badge: Some(Badge::Icon(String::new())),
            ..Decoration::default()
        }
        .is_valid()
    );
}
