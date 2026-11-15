use knot_protocol::{DecorationStyle, TextEdit};

use super::*;

#[test]
fn recovered_text_is_dirty_against_its_baseline_without_undo_history() {
    let mut model = BufferModel::from_recovered_text("on disk", "recovered");
    assert_eq!(model.text(), "recovered");
    assert!(!model.content_matches_revision(0));
    assert!(model.take_pending_change().is_none());
    assert!(!model.undo().unwrap());

    let unchanged = BufferModel::from_recovered_text("same", "same");
    assert!(unchanged.content_matches_revision(0));
}

#[test]
fn reset_to_text_leaves_a_fresh_buffer_over_the_same_identity() {
    let mut model = BufferModel::from_text("one\ntwo");
    model.set_language(Some("rust".into()), true);
    let source = ContributionSource::fixture();
    let mark = model.create_mark(source, 1).unwrap();
    model.replace(0..3, "ONE").unwrap();
    assert!(model.take_pending_change().is_some());
    let before = model.revision();

    model.reset_to_text("a\nb\nc");
    assert_eq!(model.text(), "a\nb\nc");
    assert!(model.revision() > before, "the revision stays monotonic");
    assert_eq!(model.language(), Some("rust"), "language is kept");
    assert!(!model.undo().unwrap(), "undo cannot cross the reset");
    assert!(!model.redo().unwrap());
    assert_eq!(model.anchored_range_count(), 0, "no anchors survive");
    assert_eq!(model.mark_position(source, mark), None, "no marks survive");
    assert!(model.resolved_contributions().is_empty());
    assert!(
        model.take_pending_change().is_none(),
        "no pending change leaks"
    );
    assert!(
        model.content_matches_revision(model.revision()),
        "the new revision is a usable clean baseline"
    );

    // The reset buffer edits and undoes normally afterwards.
    model.replace(0..1, "z").unwrap();
    assert!(model.undo().unwrap());
    assert_eq!(model.text(), "a\nb\nc");
}

#[test]
fn replacement_is_one_public_commit() {
    let mut model = BufferModel::from_text("abc");
    assert!(model.replace(1..2, "XYZ").unwrap());
    assert_eq!(model.text(), "aXYZc");
    assert_eq!(model.revision(), 1);
    assert_eq!(model.edit_seq(), 2);
    assert_eq!(
        model.buffer.retained_edits(),
        0,
        "stabilized edits are discarded"
    );

    assert!(!model.replace(0..0, "").unwrap());
    assert_eq!(model.revision(), 1);
}

#[test]
fn history_replay_reports_one_edit_covering_its_primitives() {
    let mut model = BufferModel::from_text("<one two three>");
    let edit = |start, end, text: &str| TextEdit {
        range: ByteRange {
            start_byte_offset: start,
            end_byte_offset: end,
        },
        text: text.into(),
    };
    assert!(
        model
            .apply_edits(&[edit(1, 4, "1"), edit(9, 14, "3")], 0)
            .unwrap()
    );
    assert_eq!(model.text(), "<1 two 3>");
    model.pending_changes.clear();

    assert!(model.undo().unwrap());
    assert_eq!(
        model.take_pending_change().unwrap().edits,
        vec![edit(1, 8, "one two three")]
    );
    assert!(model.redo().unwrap());
    assert_eq!(
        model.take_pending_change().unwrap().edits,
        vec![edit(1, 14, "1 two 3")]
    );
    assert_eq!(model.whole_text_reads(), 1, "only the assertion above");
}

#[test]
fn line_queries_and_utf16_conversions() {
    let model = BufferModel::from_text("a😀\r\nβ\n");
    assert_eq!(model.line_count(), 3);
    assert_eq!(model.line_range(0), 0..6);
    assert_eq!(model.read(model.line_range(0)), "a😀\r");
    assert_eq!(model.line_range(1), 7..9);
    assert_eq!(model.line_range(2), 10..10);
    assert_eq!(model.line_of_offset(8), 1);
    assert_eq!(model.utf16_of_offset(7), 5);
    assert_eq!(model.offset_of_utf16(5), 7);
    assert_eq!(model.len(), 10);
    assert!(!model.is_char_boundary(2));
}

#[test]
fn line_ending_follows_the_first_terminator() {
    assert_eq!(BufferModel::from_text("").line_ending(), "\n");
    assert_eq!(BufferModel::from_text("a\r").line_ending(), "\n");
    assert_eq!(BufferModel::from_text("a\nb\r\n").line_ending(), "\n");
    assert_eq!(BufferModel::from_text("a\r\nb\n").line_ending(), "\r\n");
    assert_eq!(BufferModel::from_text("\r\n").line_ending(), "\r\n");
}

#[test]
fn grouped_history_replays_through_revisions_notifications_and_anchors() {
    let mut model = BufferModel::from_text("tail");
    let tail = model.add_view_position(0..4);
    let grouping = EditGrouping {
        context: 7,
        kind: super::super::history::EditGroupKind::Typing,
    };

    assert!(model.replace_grouped(0..0, "a", grouping).unwrap());
    assert!(model.replace_grouped(1..1, "β", grouping).unwrap());
    assert_eq!(model.text(), "aβtail");
    assert_eq!(model.resolve_view_position(tail), Some(3..7));
    assert_eq!(model.revision(), 2);
    assert!(!model.content_matches_revision(1));
    assert!(model.content_matches_revision(2));
    model.pending_changes.clear();

    assert!(model.undo().unwrap());
    assert_eq!(model.text(), "tail");
    assert_eq!(model.resolve_view_position(tail), Some(0..4));
    assert_eq!(model.revision(), 3);
    assert!(model.content_matches_revision(0));
    assert_eq!(
        model.take_pending_change(),
        Some(CommittedBufferChange {
            before_revision: 2,
            revision: 3,
            edits: vec![TextEdit {
                range: ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: 3,
                },
                text: "".into(),
            }],
        })
    );

    assert!(model.redo().unwrap());
    assert_eq!(model.text(), "aβtail");
    assert_eq!(model.resolve_view_position(tail), Some(3..7));
    assert_eq!(model.revision(), 4);
    assert!(model.content_matches_revision(2));
    assert_eq!(
        model.take_pending_change().unwrap().edits,
        vec![TextEdit {
            range: ByteRange {
                start_byte_offset: 0,
                end_byte_offset: 0,
            },
            text: "aβ".into(),
        }]
    );
}

#[test]
fn histories_are_independent_and_divergent_edits_clear_redo() {
    let mut first = BufferModel::from_text("one");
    let mut second = BufferModel::from_text("two");
    first.replace(3..3, "!").unwrap();
    second.replace(3..3, "?").unwrap();

    assert!(first.undo().unwrap());
    assert_eq!(first.text(), "one");
    assert_eq!(second.text(), "two?");
    first.replace(3..3, ".").unwrap();
    assert!(!first.redo().unwrap());
    assert_eq!(first.text(), "one.");
    assert!(second.undo().unwrap());
    assert_eq!(second.text(), "two");
}

#[test]
fn multiline_unicode_replacement_preserves_other_view_positions_and_commit_text() {
    let prefix = "fn main() {\n    ";
    let selected = "let café = \"👨‍👩‍👧‍👦\";\n    println!(\"中\");";
    let suffix = "\n}\n";
    let mut model = BufferModel::from_text(format!("{prefix}{selected}{suffix}"));
    let start = prefix.len();
    let end = start + selected.len();
    let following_view = model.add_view_position(end..end + suffix.len());
    let replacement = "// e\u{301} and 日本語\n    return;";

    assert!(model.replace(start..end, replacement).unwrap());
    assert_eq!(model.text(), format!("{prefix}{replacement}{suffix}"));
    let new_end = start + replacement.len();
    assert_eq!(
        model.resolve_view_position(following_view),
        Some(new_end..new_end + suffix.len())
    );
    let change = model.take_pending_change().unwrap();
    assert_eq!((change.before_revision, change.revision), (0, 1));
    assert_eq!(change.edits.len(), 1);
    assert_eq!(change.edits[0].range.start_byte_offset, start);
    assert_eq!(change.edits[0].range.end_byte_offset, end);
    assert_eq!(change.edits[0].text, replacement);
    assert!(model.take_pending_change().is_none());
}

#[test]
fn deleting_all_text_and_typing_again_keeps_persistent_view_positions_valid() {
    let mut model = BufferModel::from_text("# Notes\n\n你好 😀\n");
    let position = model.add_view_position(2..model.text().len());
    assert!(model.replace(0..model.text().len(), "").unwrap());
    assert_eq!(model.resolve_view_position(position), Some(0..0));
    assert!(model.replace(0..0, "新しい\n").unwrap());
    let position = model.resolve_view_position(position).unwrap();
    let text = model.text();
    assert!(position.start <= position.end && position.end <= text.len());
    assert!(text.is_char_boundary(position.start));
    assert!(text.is_char_boundary(position.end));
    assert_eq!(model.revision(), 2);
}

#[test]
fn read_only_models_reject_local_and_extension_text_edits() {
    let mut model = BufferModel::from_read_only_text("generated text");
    let extension_edit = TextEdit {
        range: ByteRange {
            start_byte_offset: 0,
            end_byte_offset: 9,
        },
        text: "changed".into(),
    };

    assert_eq!(model.access_policy, BufferAccessPolicy::ReadOnly);
    assert_eq!(
        model.replace(0..9, "changed"),
        Err(BufferAccessError::ReadOnly)
    );
    assert_eq!(
        model.apply_edits(&[extension_edit], 0),
        Err(BufferAccessError::ReadOnly)
    );
    assert_eq!(model.text(), "generated text");
    assert_eq!(model.revision(), 0);
    assert_eq!(model.take_pending_change(), None);
}

#[test]
fn read_only_models_keep_non_text_operations_available() {
    let mut model = BufferModel::from_read_only_text("generated text");
    let position = model.add_view_position(0..9);

    model
        .replace_contributions(
            ContributionSource::fixture(),
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: 9,
                },
                decoration: Some(Decoration::new("info", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            0,
        )
        .unwrap();

    assert_eq!(
        model.snapshot(None).unwrap().text.to_utf8(),
        "generated text"
    );
    assert_eq!(model.resolve_view_position(position), Some(0..9));
    assert_eq!(model.resolved_contributions().len(), 1);
    model.close();
    assert!(!model.is_open());
}

#[test]
fn contribution_sets_stabilize_and_replace_atomically() {
    let mut model = BufferModel::from_text("abc def");
    let source = ContributionSource {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(3),
    };
    model
        .replace_contributions(
            source,
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 4,
                    end_byte_offset: 7,
                },
                decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            0,
        )
        .unwrap();

    assert!(model.replace(0..0, "++").unwrap());
    assert_eq!(
        model.resolved_contributions(),
        [ResolvedEditorContribution {
            range: ByteRange {
                start_byte_offset: 6,
                end_byte_offset: 9,
            },
            source,
            decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
            gutter: None,
            command: None,
        }]
    );

    assert_eq!(
        model.replace_contributions(source, &[], 0),
        Err(ContributionError::RevisionConflict)
    );
    assert_eq!(model.resolved_contributions().len(), 1);
    assert_eq!(
        model.replace_contributions(
            source,
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 7,
                    end_byte_offset: 99,
                },
                decoration: Some(Decoration::new("error", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            model.revision(),
        ),
        Err(ContributionError::InvalidRange)
    );
    assert_eq!(model.resolved_contributions().len(), 1);

    model
        .replace_contributions(source, &[], model.revision())
        .unwrap();
    assert!(model.resolved_contributions().is_empty());
}

#[test]
fn replacing_one_contribution_source_preserves_other_sources() {
    let mut model = BufferModel::from_text("abcdef");
    let extension_source = ContributionSource {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(3),
    };
    for (source, range, decoration) in [
        (ContributionSource::fixture(), 0..2, "info"),
        (extension_source, 2..4, "warning"),
    ] {
        model
            .replace_contributions(
                source,
                &[EditorContribution {
                    range: ByteRange {
                        start_byte_offset: range.start,
                        end_byte_offset: range.end,
                    },
                    decoration: Some(Decoration::new(decoration, DecorationStyle::Wavy)),
                    gutter: None,
                    command: None,
                }],
                model.revision(),
            )
            .unwrap();
    }

    model
        .replace_contributions(
            extension_source,
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 4,
                    end_byte_offset: 6,
                },
                decoration: Some(Decoration::new("error", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            model.revision(),
        )
        .unwrap();

    assert_eq!(model.contributions.sets.len(), 2);
    assert_eq!(model.contributions.metadata.len(), 2);
    assert_eq!(model.anchored_ranges.len(), 2);
    assert_eq!(
        model.resolved_contributions(),
        [
            ResolvedEditorContribution {
                range: ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: 2,
                },
                source: ContributionSource::fixture(),
                decoration: Some(Decoration::new("info", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            },
            ResolvedEditorContribution {
                range: ByteRange {
                    start_byte_offset: 4,
                    end_byte_offset: 6,
                },
                source: extension_source,
                decoration: Some(Decoration::new("error", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            },
        ]
    );
}

#[test]
fn empty_buffer_contributions_are_rejected_recoverably() {
    let mut model = BufferModel::from_text("");
    let source = ContributionSource::fixture();

    assert_eq!(
        model.replace_contributions(
            source,
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: 0,
                },
                decoration: Some(Decoration::new("info", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            0,
        ),
        Err(ContributionError::InvalidRange)
    );
    assert!(model.contributions.sets.is_empty());
    assert!(model.contributions.metadata.is_empty());
    assert!(model.anchored_ranges.is_empty());
}

#[test]
fn contribution_ownership_rejects_stale_lifecycles_and_cleans_up() {
    let mut model = BufferModel::from_text("abc");
    let extension = ExtensionId::new(7);
    let source = ContributionSource {
        extension,
        lifecycle: ExtensionLifecycleId::new(3),
    };
    let stale_source = ContributionSource {
        extension,
        lifecycle: ExtensionLifecycleId::new(2),
    };
    model
        .replace_contributions(
            source,
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: 3,
                },
                decoration: Some(Decoration::new("error", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            0,
        )
        .unwrap();

    assert_eq!(
        model.dispose_contributions(stale_source),
        Err(ContributionError::NotFound)
    );
    assert_eq!(model.resolved_contributions().len(), 1);
    assert!(!model.remove_lifecycle_state(extension, ExtensionLifecycleId::new(2)));
    assert!(model.remove_lifecycle_state(extension, ExtensionLifecycleId::new(3)));
    assert!(model.resolved_contributions().is_empty());
}

#[test]
fn stabilization_prunes_consumed_contribution_metadata() {
    let mut model = BufferModel::from_text("abc def");
    let source = ContributionSource::fixture();
    model
        .replace_contributions(
            source,
            &[EditorContribution {
                range: ByteRange {
                    start_byte_offset: 5,
                    end_byte_offset: 6,
                },
                decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
                gutter: None,
                command: None,
            }],
            0,
        )
        .unwrap();

    assert!(model.replace(4..7, "").unwrap());

    assert!(model.resolved_contributions().is_empty());
    assert!(model.contributions.metadata.is_empty());
    assert!(model.contributions.sets[&source].anchored_ranges.is_empty());
    assert!(model.anchored_ranges.is_empty());
}

#[test]
fn decoration_precedence_is_application_owned_and_deterministic() {
    let mut model = BufferModel::from_text("abc");
    for (source, decoration) in [
        (ContributionSource::fixture(), "warning"),
        (
            ContributionSource {
                extension: ExtensionId::new(7),
                lifecycle: ExtensionLifecycleId::new(3),
            },
            "info",
        ),
        (
            ContributionSource {
                extension: ExtensionId::new(8),
                lifecycle: ExtensionLifecycleId::new(1),
            },
            "diff.add",
        ),
    ] {
        model
            .replace_contributions(
                source,
                &[EditorContribution {
                    range: ByteRange {
                        start_byte_offset: 0,
                        end_byte_offset: 3,
                    },
                    decoration: Some(Decoration::new(decoration, DecorationStyle::Wavy)),
                    gutter: None,
                    command: None,
                }],
                0,
            )
            .unwrap();
    }

    assert_eq!(
        model
            .resolved_contributions()
            .into_iter()
            .filter_map(|contribution| contribution.decoration.map(|decoration| decoration.color))
            .collect::<Vec<_>>(),
        ["diff.add", "info", "warning"]
    );
}

#[test]
fn checked_reads_reject_invalid_utf8_ranges_without_touching_the_buffer() {
    let model = BufferModel::from_text("aé中");

    assert_eq!(
        model.read_checked(ByteRange {
            start_byte_offset: 1,
            end_byte_offset: 3,
        }),
        Ok("é".into())
    );
    assert_eq!(
        model.read_checked(ByteRange {
            start_byte_offset: 2,
            end_byte_offset: 3,
        }),
        Err(BufferAccessError::InvalidRange)
    );
    assert_eq!(
        model.read_checked(ByteRange {
            start_byte_offset: 4,
            end_byte_offset: 3,
        }),
        Err(BufferAccessError::InvalidRange)
    );
    assert_eq!(model.text(), "aé中");
    assert_eq!(model.revision(), 0);
}

#[test]
fn checked_reads_reject_closed_buffers() {
    let mut model = BufferModel::from_text("abc");
    model.snapshot(None).unwrap();
    model.close();
    assert!(matches!(
        model.snapshot(None),
        Err(BufferAccessError::Closed)
    ));

    assert_eq!(
        model.read_checked(ByteRange {
            start_byte_offset: 0,
            end_byte_offset: 3,
        }),
        Err(BufferAccessError::Closed)
    );
}

#[test]
fn snapshots_and_atomic_edits_use_public_revisions() {
    let mut model = BufferModel::from_text("aé中z");
    let snapshot = model
        .snapshot(Some(ByteRange {
            start_byte_offset: 1,
            end_byte_offset: 6,
        }))
        .unwrap();
    assert_eq!(snapshot.text.to_utf8(), "é中");
    assert_eq!(snapshot.revision, 0);

    assert!(
        model
            .apply_edits(
                &[
                    TextEdit {
                        range: ByteRange {
                            start_byte_offset: 0,
                            end_byte_offset: 1,
                        },
                        text: "A".into(),
                    },
                    TextEdit {
                        range: ByteRange {
                            start_byte_offset: 6,
                            end_byte_offset: 7,
                        },
                        text: "Z".into(),
                    },
                ],
                0,
            )
            .unwrap()
    );
    assert_eq!(model.text(), "Aé中Z");
    assert_eq!(model.revision(), 1);
}

#[test]
fn edits_reject_invalid_batches_without_partial_mutation() {
    let mut model = BufferModel::from_text("abcdef");
    let overlapping = [
        TextEdit {
            range: ByteRange {
                start_byte_offset: 1,
                end_byte_offset: 3,
            },
            text: "X".into(),
        },
        TextEdit {
            range: ByteRange {
                start_byte_offset: 2,
                end_byte_offset: 4,
            },
            text: "Y".into(),
        },
    ];
    assert_eq!(
        model.apply_edits(&overlapping, 0),
        Err(BufferAccessError::InvalidEditBatch)
    );
    assert_eq!(model.text(), "abcdef");
    assert_eq!(model.revision(), 0);

    assert_eq!(
        model.apply_edits(&[], 1),
        Err(BufferAccessError::RevisionConflict)
    );
}

#[test]
fn semantically_empty_batches_do_not_commit() {
    let mut model = BufferModel::from_text("abcdef");
    let edits = [TextEdit {
        range: ByteRange {
            start_byte_offset: 1,
            end_byte_offset: 3,
        },
        text: "bc".into(),
    }];
    assert!(!model.apply_edits(&edits, 0).unwrap());
    assert!(!model.apply_edits(&[], 0).unwrap());
    assert_eq!(model.text(), "abcdef");
    assert_eq!(model.revision(), 0);
}

#[test]
fn snapshots_preserve_unicode_and_reject_split_scalar_boundaries() {
    let text = "ASCII 中 العربية e\u{301} 👩‍💻";
    let model = BufferModel::from_text(text);
    let full = model.snapshot(None).unwrap();
    assert_eq!(full.text.to_utf8(), text);

    let emoji_start = text.find('👩').unwrap();
    let emoji_end = text.len();
    assert_eq!(
        model
            .snapshot(Some(ByteRange {
                start_byte_offset: emoji_start,
                end_byte_offset: emoji_end,
            }))
            .unwrap()
            .text
            .to_utf8(),
        "👩‍💻"
    );
    assert_eq!(
        model.snapshot(Some(ByteRange {
            start_byte_offset: emoji_start + 1,
            end_byte_offset: emoji_end,
        })),
        Err(BufferAccessError::InvalidRange)
    );
}

#[test]
fn snapshot_cache_reuses_storage_and_invalidates_on_edit() {
    let mut model = BufferModel::from_text("abc");
    let first = model.snapshot(None).unwrap();
    let second = model.snapshot(None).unwrap();
    let (SnapshotText(first), SnapshotText(second)) = (first.text, second.text);
    assert!(Arc::ptr_eq(&first, &second));

    assert!(model.replace(1..2, "B").unwrap());
    let third = model.snapshot(None).unwrap();
    let SnapshotText(third) = third.text;
    assert!(!Arc::ptr_eq(&first, &third));
    assert_eq!(String::from_utf16(&third).unwrap(), "aBc");
}

fn rust_syntax() -> Option<SyntaxState> {
    use crate::syntax::SyntaxLanguage;
    let highlights = "\"fn\" @keyword\n(identifier) @variable\n(line_comment) @comment\n";
    let language = Arc::new(SyntaxLanguage::new("rust", highlights, "").unwrap());
    Some(SyntaxState::new(language, Arc::new(|_| None)))
}

fn syntax_names(model: &BufferModel) -> Vec<(String, &'static str)> {
    model
        .syntax_highlights(0..model.len())
        .into_iter()
        .map(|(range, highlight)| (model.read(range), highlight.name()))
        .collect()
}

#[test]
fn syntax_follows_every_commit_path() {
    let fresh = |text: &str| {
        let mut model = BufferModel::from_text(text);
        model.set_syntax(rust_syntax());
        syntax_names(&model)
    };
    let mut model = BufferModel::from_text("fn a() {}");
    model.set_syntax(rust_syntax());
    assert_eq!(
        syntax_names(&model),
        [("fn".into(), "keyword"), ("a".into(), "variable")]
    );

    model.replace(0..0, "// x\n").unwrap();
    let expected = fresh("// x\nfn a() {}");
    assert_eq!(expected[0], ("// x".into(), "comment"));
    assert_eq!(syntax_names(&model), expected);

    let edits = [TextEdit {
        range: ByteRange {
            start_byte_offset: 8,
            end_byte_offset: 9,
        },
        text: "bc".into(),
    }];
    assert!(model.apply_edits(&edits, model.revision()).unwrap());
    assert_eq!(syntax_names(&model), fresh("// x\nfn bc() {}"));

    assert!(model.undo().unwrap());
    assert_eq!(syntax_names(&model), expected);
    assert!(model.redo().unwrap());
    assert_eq!(syntax_names(&model), fresh("// x\nfn bc() {}"));
    assert!(model.start_syntax_parse().is_none());
}

#[test]
fn reset_to_text_rebuilds_the_syntax_tree() {
    let mut model = BufferModel::from_text("fn a() {}");
    model.set_syntax(rust_syntax());
    let generation = model.syntax_generation();
    let language = Arc::clone(model.syntax_language().unwrap());

    model.reset_to_text("// x\nfn b() {}");
    assert_ne!(model.syntax_generation(), generation, "a new tree");
    assert!(Arc::ptr_eq(model.syntax_language().unwrap(), &language));
    let mut fresh = BufferModel::from_text("// x\nfn b() {}");
    fresh.set_syntax(rust_syntax());
    assert_eq!(syntax_names(&model), syntax_names(&fresh));
    assert!(model.start_syntax_parse().is_none());
}

fn over_budget_model() -> BufferModel {
    let mut model = BufferModel::from_text("fn a() { let x = 1; }\n".repeat(2_000));
    model.set_syntax_budget(Duration::ZERO);
    model.set_syntax(rust_syntax());
    model
}

#[test]
fn an_over_budget_parse_continues_as_a_job_rebased_over_later_commits() {
    let mut model = over_budget_model();
    assert!(model.syntax_highlights(0..2).is_empty());
    let generation = model.syntax_generation();
    let parse = model.start_syntax_parse().unwrap();
    assert!(
        model.start_syntax_parse().is_none(),
        "one parse runs at a time"
    );

    model.replace(0..0, "// c\n").unwrap();
    assert!(model.finish_syntax_parse(parse.run().unwrap()));
    assert_ne!(model.syntax_generation(), generation);
    assert!(
        !model.syntax_is_current(),
        "the rebased tree lags the commit"
    );
    while let Some(parse) = model.start_syntax_parse() {
        assert!(model.finish_syntax_parse(parse.run().unwrap()));
    }
    assert!(model.syntax_is_current());
    let names: Vec<_> = model
        .syntax_highlights(0..12)
        .into_iter()
        .map(|(range, highlight)| (model.read(range), highlight.name()))
        .collect();
    assert_eq!(
        names,
        [
            ("// c".into(), "comment"),
            ("fn".into(), "keyword"),
            ("a".into(), "variable")
        ]
    );
}

#[test]
fn a_parse_of_replaced_or_closed_syntax_is_rejected() {
    let mut model = over_budget_model();
    let parse = model.start_syntax_parse().unwrap();
    let parsed = {
        let mut other = over_budget_model();
        other.start_syntax_parse().unwrap().run().unwrap()
    };
    assert!(!model.finish_syntax_parse(parsed), "another state's result");

    model.set_syntax(rust_syntax());
    let generation = model.syntax_generation();
    assert!(
        parse.run().is_none(),
        "replacing the state cancels its parse"
    );
    let stale = model.start_syntax_parse().unwrap().run().unwrap();
    model.set_syntax(rust_syntax());
    assert!(!model.finish_syntax_parse(stale));
    assert_ne!(model.syntax_generation(), generation);

    let parsed = model.start_syntax_parse().unwrap().run().unwrap();
    model.close();
    assert!(!model.finish_syntax_parse(parsed));
    assert_eq!(model.syntax_generation(), None);
}

#[test]
fn closing_or_clearing_syntax_releases_it() {
    let mut model = BufferModel::from_text("fn a() {}");
    model.set_syntax(rust_syntax());
    let parsed = model.syntax_generation();
    model.set_syntax(rust_syntax());
    assert_ne!(model.syntax_generation(), parsed);
    model.set_syntax(None);
    assert_eq!(model.syntax_generation(), None);
    model.set_syntax(rust_syntax());
    model.close();
    assert_eq!(model.syntax_generation(), None);
}

#[test]
fn a_view_batch_is_one_undo_step_that_restores_its_selection() {
    let mut model = BufferModel::from_text("one\ntwo\nthree");
    let view = |anchor, caret| HistoryViewState {
        context: 7,
        anchor,
        caret,
    };
    assert!(
        model
            .apply_edits_from_view(
                vec![
                    (0..0, "  ".into()),
                    (4..4, "  ".into()),
                    (8..8, "  ".into())
                ],
                HistoryViewChange {
                    before: view(0, 9),
                    after: view(0, 15),
                },
            )
            .unwrap()
    );
    assert_eq!(model.text(), "  one\n  two\n  three");
    assert_eq!(
        model.take_pending_change().map(|change| change.edits.len()),
        Some(3)
    );

    assert!(model.undo().unwrap());
    assert_eq!(model.text(), "one\ntwo\nthree");
    assert_eq!(model.history_view_restore().unwrap().state, view(0, 9));
    assert!(model.redo().unwrap());
    assert_eq!(model.text(), "  one\n  two\n  three");
    assert_eq!(model.history_view_restore().unwrap().state, view(0, 15));

    let unchanged = model
        .apply_edits_from_view(
            vec![(0..2, "  ".into())],
            HistoryViewChange {
                before: view(0, 0),
                after: view(0, 0),
            },
        )
        .unwrap();
    assert!(!unchanged);
}

#[test]
fn indentation_is_detected_from_the_leading_lines() {
    let model = BufferModel::from_text("a\r\n\tb\r\n\tc");
    assert_eq!(model.detect_indent_style(), Some(IndentStyle::Tabs));
    let model = BufferModel::from_text("");
    assert_eq!(model.detect_indent_style(), None);
}

fn group_owner(lifecycle: u64) -> ExtensionKey {
    ExtensionKey::new(ExtensionId::new(1), ExtensionLifecycleId::new(lifecycle))
}

fn typing(context: u64) -> EditGrouping {
    EditGrouping {
        context,
        kind: crate::history::EditGroupKind::Typing,
    }
}

fn caret(context: u64, at: usize) -> HistoryViewState {
    HistoryViewState {
        context,
        anchor: at,
        caret: at,
    }
}

fn view_change(context: u64, before: usize, after: usize) -> HistoryViewChange {
    HistoryViewChange {
        before: caret(context, before),
        after: caret(context, after),
    }
}

#[test]
fn an_open_undo_group_joins_batches_typing_and_amends_into_one_step() {
    let mut model = BufferModel::from_text("one two three");
    let owner = group_owner(1);
    assert!(model.begin_undo_group(owner, None));
    model
        .apply_edits_from_view(vec![(4..7, String::new())], view_change(7, 4, 4))
        .unwrap();
    model.break_history_group();
    model
        .replace_grouped_from_view(4..4, "x", typing(7), view_change(7, 4, 5))
        .unwrap();
    model.break_history_group();
    model
        .replace_grouped_from_view(5..5, "y", typing(7), view_change(7, 5, 6))
        .unwrap();
    model
        .amend_last_edit_from_view(vec![(6..6, "!".into())], caret(7, 7))
        .unwrap();
    model
        .replace_from_view(0..0, ">", view_change(7, 7, 8))
        .unwrap();
    assert_eq!(model.text(), ">one xy! three");
    assert!(model.end_undo_group(owner, None));

    assert!(model.undo().unwrap());
    assert_eq!(model.text(), "one two three");
    assert!(!model.undo().unwrap());
    assert!(model.redo().unwrap());
    assert_eq!(model.text(), ">one xy! three");
}

#[test]
fn an_open_undo_group_records_its_first_and_last_view_state() {
    let owner = group_owner(1);
    let mut model = BufferModel::from_text("ab");
    assert!(model.begin_undo_group(owner, None));
    model
        .replace_from_view(0..0, "x", view_change(7, 0, 1))
        .unwrap();
    model
        .replace_from_view(1..1, "y", view_change(7, 1, 2))
        .unwrap();
    model.end_undo_group(owner, None);
    model.undo().unwrap();
    assert_eq!(model.history_view_restore().unwrap().state, caret(7, 0));
    model.redo().unwrap();
    assert_eq!(model.history_view_restore().unwrap().state, caret(7, 2));

    let mut model = BufferModel::from_text("ab");
    assert!(model.begin_undo_group(owner, None));
    model
        .replace_from_view(0..0, "x", view_change(7, 0, 1))
        .unwrap();
    model
        .replace_from_view(1..1, "y", view_change(8, 1, 2))
        .unwrap();
    model.end_undo_group(owner, None);
    model.undo().unwrap();
    assert_eq!(model.text(), "ab");
    assert!(model.history_view_restore().is_none());
}

#[test]
fn an_undo_group_opened_by_a_view_restores_its_selection_around_unrecorded_commits() {
    let owner = group_owner(1);
    let mut model = BufferModel::from_text("one two");
    assert!(model.begin_undo_group(owner, Some(caret(7, 5))));
    model
        .apply_edits(
            &[TextEdit {
                range: ByteRange {
                    start_byte_offset: 4,
                    end_byte_offset: 7,
                },
                text: String::new(),
            }],
            0,
        )
        .unwrap();
    model
        .replace_from_view(4..4, "x", view_change(7, 4, 5))
        .unwrap();
    assert!(model.end_undo_group(owner, Some(caret(7, 4))));

    model.undo().unwrap();
    assert_eq!(model.text(), "one two");
    assert_eq!(model.history_view_restore().unwrap().state, caret(7, 5));
    model.redo().unwrap();
    assert_eq!(model.history_view_restore().unwrap().state, caret(7, 4));
}

#[test]
fn undo_group_rules_for_ownership_emptiness_and_replay() {
    let mut model = BufferModel::from_text("a");
    let owner = group_owner(1);
    let other = group_owner(2);
    assert!(model.begin_undo_group(owner, None));
    assert!(!model.begin_undo_group(other, None));
    assert!(!model.end_undo_group(other, None));
    assert!(model.end_undo_group(owner, None));
    // An empty group leaves no step.
    assert!(!model.undo().unwrap());

    model.replace_grouped(1..1, "b", typing(1)).unwrap();
    model.break_history_group();
    assert!(model.begin_undo_group(owner, None));
    model.replace_grouped(2..2, "c", typing(1)).unwrap();
    model.replace_grouped(3..3, "d", typing(1)).unwrap();
    // Undo ends the open group and replays it as one step.
    assert!(model.undo().unwrap());
    assert_eq!(model.text(), "ab");
    assert!(!model.end_undo_group(owner, None));
    // A forward edit after the step no longer joins it.
    model.replace_grouped(2..2, "e", typing(1)).unwrap();
    assert!(!model.redo().unwrap());

    assert!(model.begin_undo_group(owner, None));
    model.close();
    assert!(!model.end_undo_group(owner, None));
}

#[test]
fn marks_follow_edits_and_vanish_with_their_text() {
    let mut model = BufferModel::from_text("hello world");
    let source = ContributionSource::fixture();
    let other = ContributionSource {
        extension: ExtensionId::new(2),
        lifecycle: ExtensionLifecycleId::new(1),
    };
    let mark = model.create_mark(source, 6).unwrap();
    assert_eq!(model.mark_position(source, mark), Some(6));
    // Another lifecycle cannot read or remove it.
    assert_eq!(model.mark_position(other, mark), None);
    model.dispose_mark(other, mark);
    assert_eq!(model.mark_position(source, mark), Some(6));

    // It stays with the character after it: text inserted before or at it
    // moves it, text inserted after does not.
    assert!(model.replace(0..0, ">> ").unwrap());
    assert_eq!(model.mark_position(source, mark), Some(9));
    assert!(model.replace(9..9, "big ").unwrap());
    assert_eq!(model.mark_position(source, mark), Some(13));
    assert!(model.replace(14..14, "!").unwrap());
    assert_eq!(model.mark_position(source, mark), Some(13));

    // A deletion before it moves it back; undo restores it with the text.
    assert!(model.replace(0..3, "").unwrap());
    assert_eq!(model.mark_position(source, mark), Some(10));
    model.undo().unwrap();
    assert_eq!(model.mark_position(source, mark), Some(13));
    model.redo().unwrap();

    // Deleting the text it sat in removes it.
    let end = model.len();
    assert!(model.replace(0..end, "").unwrap());
    assert_eq!(model.mark_position(source, mark), None);
}

#[test]
fn marks_validate_offsets_and_end_with_disposal_or_their_lifecycle() {
    let mut model = BufferModel::from_text("aé");
    let source = ContributionSource::fixture();
    assert_eq!(
        model.create_mark(source, 2),
        Err(ContributionError::InvalidRange)
    );
    assert_eq!(
        model.create_mark(source, 4),
        Err(ContributionError::InvalidRange)
    );
    let first = model.create_mark(source, 0).unwrap();
    let second = model.create_mark(source, 3).unwrap();
    model.dispose_mark(source, first);
    assert_eq!(model.mark_position(source, first), None);
    model.dispose_mark(source, first);
    assert_eq!(model.mark_position(source, second), Some(3));
    assert!(!model.remove_lifecycle_state(source.extension, source.lifecycle));
    assert_eq!(model.mark_position(source, second), None);

    // An empty buffer has no text to mark.
    let mut empty = BufferModel::from_text("");
    assert_eq!(
        empty.create_mark(source, 0),
        Err(ContributionError::InvalidRange)
    );
}

#[test]
fn line_content_excludes_terminators_and_rejects_missing_lines() {
    let model = BufferModel::from_text("one\r\ntwo\nthree");
    assert_eq!(model.line_content_range(0), Some(0..3));
    assert_eq!(model.line_content_range(1), Some(5..8));
    assert_eq!(model.line_content_range(2), Some(9..14));
    assert_eq!(model.line_content_range(3), None);
}

#[test]
fn line_at_offset_requires_a_character_boundary_within_the_text() {
    let model = BufferModel::from_text("a\u{e9}\nb");
    assert_eq!(model.line_at_offset(0), Some(0));
    assert_eq!(model.line_at_offset(2), None);
    assert_eq!(model.line_at_offset(4), Some(1));
    assert_eq!(model.line_at_offset(5), Some(1));
    assert_eq!(model.line_at_offset(6), None);
}

#[test]
fn subscriptions_belong_to_their_lifecycle_and_buffer() {
    let (extension, other) = (ExtensionId::new(1), ExtensionId::new(2));
    let lifecycle = ExtensionLifecycleId::new(1);
    let (first, second) = (BufferHandle::new(1), BufferHandle::new(2));
    let mut registry = BufferSubscriptionRegistry::new();
    let a = registry.subscribe(first, extension, lifecycle);
    let b = registry.subscribe(second, other, lifecycle);
    assert!(!registry.unsubscribe(a, other, lifecycle));
    assert_eq!(registry.for_buffer(first).count(), 1);
    registry.remove_buffer(first);
    assert_eq!(registry.for_buffer(first).count(), 0);
    registry.remove_lifecycle(other, lifecycle);
    assert!(!registry.unsubscribe(b, other, lifecycle));
}

fn found(model: &BufferModel, query: &str, options: FindOptions) -> Vec<Range<usize>> {
    model.find(query, options, None, 1000).unwrap().ranges
}

fn insensitive() -> FindOptions {
    FindOptions::default()
}

fn sensitive() -> FindOptions {
    FindOptions {
        case_sensitive: true,
        ..FindOptions::default()
    }
}

fn whole_word() -> FindOptions {
    FindOptions {
        whole_word: true,
        ..FindOptions::default()
    }
}

#[test]
fn find_is_literal_and_does_not_overlap() {
    let model = BufferModel::from_text("a.b a.b aaa");
    assert_eq!(found(&model, "a.b", sensitive()), vec![0..3, 4..7]);
    assert_eq!(found(&model, "aa", sensitive()), vec![8..10]);
    assert_eq!(
        found(&model, "a\nb", sensitive()),
        Vec::<Range<usize>>::new()
    );
    let lines = BufferModel::from_text("one\ntwo\nthree");
    assert_eq!(found(&lines, "e\nt", sensitive()), vec![2..5]);
}

#[test]
fn find_folds_case_unless_sensitive() {
    let model = BufferModel::from_text("Foo foo FOO");
    assert_eq!(found(&model, "foo", insensitive()), vec![0..3, 4..7, 8..11]);
    assert_eq!(found(&model, "FOO", insensitive()), vec![0..3, 4..7, 8..11]);
    assert_eq!(found(&model, "foo", sensitive()), vec![4..7]);
}

#[test]
fn find_reports_byte_offsets_in_multibyte_text() {
    let model = BufferModel::from_text("é ÉCOLE 😀 école");
    // "é" is two bytes, "😀" four.
    assert_eq!(found(&model, "é", insensitive()), vec![0..2, 3..5, 15..17]);
    assert_eq!(found(&model, "é", sensitive()), vec![0..2, 15..17]);
    assert_eq!(found(&model, "😀", sensitive()), vec![10..14]);
    assert_eq!(found(&model, "école", insensitive()), vec![3..9, 15..21]);
}

#[test]
fn find_whole_word_uses_the_editor_word_classes() {
    let model = BufferModel::from_text("cat concat cat.cat cat_ (cat) cats");
    assert_eq!(
        found(&model, "cat", whole_word()),
        vec![0..3, 11..14, 15..18, 25..28]
    );
    // A query that ends in a separator needs no boundary after it.
    let calls = BufferModel::from_text("foo(a) foo(b) xfoo(c)");
    assert_eq!(found(&calls, "foo(", whole_word()), vec![0..4, 7..11]);
}

#[test]
fn find_whole_word_looks_beyond_a_range_edge() {
    let model = BufferModel::from_text("foo foo");
    let tail = ByteRange {
        start_byte_offset: 1,
        end_byte_offset: 7,
    };
    // The "oo" at 1..3 is the tail of a word, not a word.
    let matches = model.find("oo", whole_word(), Some(tail), 10).unwrap();
    assert!(matches.ranges.is_empty());
    let cut = ByteRange {
        start_byte_offset: 0,
        end_byte_offset: 2,
    };
    assert!(
        model
            .find("fo", whole_word(), Some(cut), 10)
            .unwrap()
            .ranges
            .is_empty()
    );
}

#[test]
fn find_empty_query_matches_nothing() {
    let model = BufferModel::from_text("abc");
    assert_eq!(found(&model, "", insensitive()), Vec::<Range<usize>>::new());
    assert_eq!(
        BufferModel::from_text("").find("a", insensitive(), None, 5),
        Ok(FindMatches {
            ranges: vec![],
            more: false,
        })
    );
}

#[test]
fn find_limit_reports_more() {
    let model = BufferModel::from_text("a a a");
    let two = model.find("a", insensitive(), None, 2).unwrap();
    assert_eq!(two.ranges, vec![0..1, 2..3]);
    assert!(two.more);
    let exact = model.find("a", insensitive(), None, 3).unwrap();
    assert_eq!(exact.ranges.len(), 3);
    assert!(!exact.more);
    assert!(
        model
            .find("a", insensitive(), None, 0)
            .unwrap()
            .ranges
            .is_empty()
    );
}

#[test]
fn find_range_limits_matches_and_is_checked() {
    let model = BufferModel::from_text("ab ab ab é");
    let range = |start, end| {
        Some(ByteRange {
            start_byte_offset: start,
            end_byte_offset: end,
        })
    };
    assert_eq!(
        model
            .find("ab", insensitive(), range(3, 8), 10)
            .unwrap()
            .ranges,
        vec![3..5, 6..8]
    );
    // A match cut by the range edge is not a match.
    assert!(
        model
            .find("ab", insensitive(), range(3, 4), 10)
            .unwrap()
            .ranges
            .is_empty()
    );
    assert_eq!(
        model.find("ab", insensitive(), range(9, 20), 10),
        Err(FindError::Access(BufferAccessError::InvalidRange))
    );
    assert_eq!(
        model.find("ab", insensitive(), range(0, 10), 10),
        Err(FindError::Access(BufferAccessError::InvalidRange)),
        "10 splits the two-byte é"
    );
    assert_eq!(
        model.find("ab", insensitive(), range(5, 3), 10),
        Err(FindError::Access(BufferAccessError::InvalidRange))
    );
}

fn pattern() -> FindOptions {
    FindOptions {
        regex: true,
        ..FindOptions::default()
    }
}

fn pattern_sensitive() -> FindOptions {
    FindOptions {
        case_sensitive: true,
        ..pattern()
    }
}

fn span(start: usize, end: usize) -> Option<ByteRange> {
    Some(ByteRange {
        start_byte_offset: start,
        end_byte_offset: end,
    })
}

#[test]
fn find_regex_anchors_match_at_line_boundaries() {
    let model = BufferModel::from_text("one\ntwo\nthree");
    assert_eq!(found(&model, "^t", pattern()), vec![4..5, 8..9]);
    assert_eq!(found(&model, "e$", pattern()), vec![2..3, 12..13]);
    assert_eq!(found(&model, "e\\nt", pattern()), vec![2..5]);
}

#[test]
fn find_regex_folds_case_unless_sensitive() {
    let model = BufferModel::from_text("Foo foo FOO é É");
    assert_eq!(found(&model, "f(o)+", pattern()), vec![0..3, 4..7, 8..11]);
    assert_eq!(found(&model, "f(o)+", pattern_sensitive()), vec![4..7]);
    assert_eq!(found(&model, "é", pattern()), vec![12..14, 15..17]);
}

#[test]
fn find_regex_whole_word_checks_each_match() {
    let model = BufferModel::from_text("cat concat cat.cat cats");
    let options = FindOptions {
        whole_word: true,
        ..pattern()
    };
    assert_eq!(found(&model, "cat", options), vec![0..3, 11..14, 15..18]);
    assert_eq!(
        found(&model, "cats?", options),
        vec![0..3, 11..14, 15..18, 19..23]
    );
}

#[test]
fn find_regex_judges_anchors_and_boundaries_against_the_surrounding_text() {
    let model = BufferModel::from_text("ab\nab ab");
    // 3..5 starts a line; 6..8 does not, and `\b` sees the space before it.
    assert_eq!(
        model.find("^ab", pattern(), span(3, 8), 10).unwrap().ranges,
        vec![3..5]
    );
    assert_eq!(
        model.find("^ab", pattern(), span(1, 8), 10).unwrap().ranges,
        vec![3..5]
    );
    assert_eq!(
        model
            .find("\\bb", pattern(), span(1, 8), 10)
            .unwrap()
            .ranges,
        Vec::<Range<usize>>::new()
    );
    // `$` at the range edge needs a real line end after it.
    assert_eq!(
        model.find("a$", pattern(), span(0, 1), 10).unwrap().ranges,
        Vec::<Range<usize>>::new()
    );
    assert_eq!(
        model.find("b$", pattern(), span(0, 2), 10).unwrap().ranges,
        vec![1..2]
    );
    // A match cannot run past the range.
    assert_eq!(
        model.find("a+b", pattern(), span(3, 4), 10).unwrap().ranges,
        Vec::<Range<usize>>::new()
    );
}

#[test]
fn find_regex_keeps_empty_matches() {
    let model = BufferModel::from_text("a\n\nbé");
    assert_eq!(found(&model, "^", pattern()), vec![0..0, 2..2, 3..3]);
    assert_eq!(
        found(&model, "x*", pattern()),
        vec![0..0, 1..1, 2..2, 3..3, 4..4, 6..6]
    );
}

#[test]
fn find_regex_limit_reports_more() {
    let model = BufferModel::from_text("a1 b2 c3");
    let two = model.find("[a-z]\\d", pattern(), None, 2).unwrap();
    assert_eq!(two.ranges, vec![0..2, 3..5]);
    assert!(two.more);
    assert!(!model.find("[a-z]\\d", pattern(), None, 3).unwrap().more);
}

#[test]
fn find_regex_rejects_an_invalid_pattern_with_the_parser_message() {
    let model = BufferModel::from_text("abc");
    let Err(FindError::InvalidQuery(message)) = model.find("(", pattern(), None, 10) else {
        panic!("an unclosed group is invalid");
    };
    assert!(message.contains("unclosed group"), "{message}");
    // The same text is a fine literal query, and an empty pattern matches nothing.
    assert!(model.find("(", insensitive(), None, 10).is_ok());
    assert!(
        model
            .find("", pattern(), None, 10)
            .unwrap()
            .ranges
            .is_empty()
    );
}

#[test]
fn find_on_a_closed_buffer_fails() {
    let mut model = BufferModel::from_text("ab");
    model.close();
    assert_eq!(
        model.find("a", insensitive(), None, 1),
        Err(FindError::Access(BufferAccessError::Closed))
    );
}

#[test]
fn find_folds_case_by_simple_unicode_folding() {
    let model = BufferModel::from_text("Σ σ ς ſ s \u{212A} k ß ss SS İ i ı I");
    // Greek sigmas agree, including the final form.
    assert_eq!(found(&model, "σ", insensitive()).len(), 3);
    assert_eq!(found(&model, "ς", insensitive()).len(), 3);
    assert_eq!(found(&model, "σ", sensitive()).len(), 1);
    // The long s and the Kelvin sign fold to their ASCII letters.
    assert_eq!(found(&model, "s", insensitive()).len(), 6);
    assert_eq!(found(&model, "k", insensitive()).len(), 2);
    // Folding never changes length: ß is not ss, and the Turkish letters
    // keep to themselves.
    let sharp_s = model.text().find('ß').unwrap();
    assert_eq!(
        found(&model, "ß", insensitive()),
        vec![sharp_s..sharp_s + 2]
    );
    assert_eq!(found(&model, "İ", insensitive()).len(), 1);
    assert_eq!(found(&model, "ı", insensitive()).len(), 1);
    assert_eq!(found(&model, "i", insensitive()).len(), 2);
}

/// The text after replacing the matches of `query`, and whether more remained.
fn replaced(
    text: &str,
    query: &str,
    replacement: &str,
    options: FindOptions,
    range: Option<ByteRange>,
    limit: usize,
) -> (String, bool) {
    let model = BufferModel::from_text(text);
    let replacements = model
        .replace_matches(query, replacement, options, range, limit)
        .unwrap();
    let mut result = text.to_owned();
    for edit in replacements.edits.iter().rev() {
        result.replace_range(
            edit.range.start_byte_offset..edit.range.end_byte_offset,
            &edit.text,
        );
    }
    (result, replacements.more)
}

fn replaced_all(text: &str, query: &str, replacement: &str, options: FindOptions) -> String {
    replaced(text, query, replacement, options, None, usize::MAX).0
}

#[test]
fn replace_literal_inserts_the_replacement_as_typed() {
    let options = FindOptions::default();
    assert_eq!(
        replaced_all("a.b a.b", "a.b", "$1\\n", options),
        "$1\\n $1\\n"
    );
    assert_eq!(replaced_all("Foo foo", "foo", "bar", options), "bar bar");
    let sensitive = FindOptions {
        case_sensitive: true,
        ..options
    };
    assert_eq!(replaced_all("Foo foo", "foo", "bar", sensitive), "Foo bar");
    let word = FindOptions {
        whole_word: true,
        ..options
    };
    assert_eq!(
        replaced_all("cat concat cat", "cat", "dog", word),
        "dog concat dog"
    );
    assert_eq!(replaced_all("abc", "b", "", options), "ac");
}

#[test]
fn replace_regex_expands_groups() {
    assert_eq!(
        replaced_all("john smith", "(\\w+) (\\w+)", "$2, $1", pattern()),
        "smith, john"
    );
    assert_eq!(
        replaced_all(
            "john smith",
            "(?<first>\\w+) (?<last>\\w+)",
            "${last}_${first}",
            pattern()
        ),
        "smith_john"
    );
    assert_eq!(replaced_all("a", "a", "$$1", pattern()), "$1");
    assert_eq!(replaced_all("abc", "b", "[$0]", pattern()), "a[b]c");
    // A group that did not take part, or does not exist, expands to nothing.
    assert_eq!(replaced_all("ac", "a(b)?c", "<$1>", pattern()), "<>");
    assert_eq!(replaced_all("a", "a", "<$7>", pattern()), "<>");
    // Braces end a name where a bare name would run on.
    assert_eq!(replaced_all("a", "(a)", "$1x", pattern()), "");
    assert_eq!(replaced_all("a", "(a)", "${1}x", pattern()), "ax");
}

#[test]
fn replace_regex_expands_escapes() {
    assert_eq!(replaced_all("a b", " ", "\\n", pattern()), "a\nb");
    assert_eq!(replaced_all("a b", " ", "\\t", pattern()), "a\tb");
    assert_eq!(replaced_all("a b", " ", "\\\\", pattern()), "a\\b");
    assert_eq!(replaced_all("a b", " ", "\\x", pattern()), "a\\xb");
    assert_eq!(replaced_all("a b", " ", "\\\\n", pattern()), "a\\nb");
}

#[test]
fn replace_regex_empty_matches_prefix_every_line() {
    assert_eq!(
        replaced_all("one\ntwo\n", "^", "// ", pattern()),
        "// one\n// two\n// "
    );
    assert_eq!(replaced_all("ab", "x*", "-", pattern()), "-a-b-");
}

#[test]
fn replace_regex_whole_word_and_case() {
    let word = FindOptions {
        whole_word: true,
        ..pattern()
    };
    assert_eq!(replaced_all("cat concat", "c\\w*t", "X", word), "X X");
    assert_eq!(replaced_all("cat concat", "cat", "X", word), "X concat");
    assert_eq!(replaced_all("Cat cat", "cat", "X", pattern()), "X X");
    assert_eq!(
        replaced_all("Cat cat", "cat", "X", pattern_sensitive()),
        "Cat X"
    );
}

#[test]
fn replace_stays_inside_a_range() {
    let options = FindOptions::default();
    assert_eq!(
        replaced("aaa aaa", "a", "b", options, span(4, 7), usize::MAX).0,
        "aaa bbb"
    );
    // Anchors are judged against the text around the range.
    assert_eq!(
        replaced(
            "one\ntwo\nthree",
            "^",
            "> ",
            pattern(),
            span(4, 7),
            usize::MAX
        )
        .0,
        "one\n> two\nthree"
    );
    assert_eq!(
        replaced("one\ntwo", "^", "> ", pattern(), span(5, 7), usize::MAX).0,
        "one\ntwo"
    );
}

#[test]
fn replace_limits_and_reports_more() {
    let options = FindOptions::default();
    assert_eq!(
        replaced("a a a", "a", "b", options, None, 2),
        ("b b a".to_owned(), true)
    );
    assert_eq!(
        replaced("a a", "a", "b", options, None, 2),
        ("b b".to_owned(), false)
    );
    assert_eq!(
        replaced("a a", "a", "b", options, None, 0),
        ("a a".to_owned(), false)
    );
    assert_eq!(
        replaced("a a a", "a", "b", pattern(), None, 1),
        ("b a a".to_owned(), true)
    );
}

#[test]
fn replace_with_no_match_or_an_empty_query_makes_no_edits() {
    let model = BufferModel::from_text("abc");
    for options in [FindOptions::default(), pattern()] {
        let none = model.replace_matches("x", "y", options, None, 10).unwrap();
        assert!(none.edits.is_empty() && !none.more);
        let empty = model.replace_matches("", "y", options, None, 10).unwrap();
        assert!(empty.edits.is_empty() && !empty.more);
    }
}

#[test]
fn replace_reports_invalid_patterns_and_ranges() {
    let model = BufferModel::from_text("abc");
    assert!(matches!(
        model.replace_matches("(", "y", pattern(), None, 10),
        Err(FindError::InvalidQuery(_))
    ));
    assert!(matches!(
        model.replace_matches("a", "y", FindOptions::default(), span(0, 9), 10),
        Err(FindError::Access(_))
    ));
}

#[test]
fn find_and_replace_commits_one_undoable_batch() {
    let mut model = BufferModel::from_text("a a a");
    let revision = model.revision();
    let outcome = model
        .find_and_replace("a", "bb", FindOptions::default(), None, 2, Some(revision))
        .unwrap();
    assert_eq!(
        outcome,
        ReplaceOutcome {
            count: 2,
            more: true,
            changed: true
        }
    );
    assert_eq!(model.text(), "bb bb a");
    assert_eq!(model.revision(), revision + 1);
    model.undo().unwrap();
    assert_eq!(model.text(), "a a a");
}

#[test]
fn find_and_replace_without_a_change_keeps_the_revision() {
    let mut model = BufferModel::from_text("a a");
    let revision = model.revision();
    let none = model
        .find_and_replace("x", "y", FindOptions::default(), None, 9, None)
        .unwrap();
    assert_eq!((none.count, none.changed), (0, false));
    let same = model
        .find_and_replace("a", "a", FindOptions::default(), None, 9, None)
        .unwrap();
    assert_eq!((same.count, same.changed), (2, false));
    assert_eq!(model.revision(), revision);
}

#[test]
fn find_and_replace_rejects_stale_revisions_and_read_only_buffers() {
    let mut model = BufferModel::from_text("a");
    let stale = model.revision() + 1;
    assert_eq!(
        model.find_and_replace("a", "b", FindOptions::default(), None, 9, Some(stale)),
        Err(FindError::Access(BufferAccessError::RevisionConflict))
    );
    let mut generated = BufferModel::from_read_only_text("a");
    assert_eq!(
        generated.find_and_replace("a", "b", FindOptions::default(), None, 9, None),
        Err(FindError::Access(BufferAccessError::ReadOnly))
    );
    assert_eq!(generated.text(), "a");
}
