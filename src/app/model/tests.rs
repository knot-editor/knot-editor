use super::*;

#[gpui::test]
fn handles_are_monotonic_and_invalidated_buffers_stay_invalid(cx: &mut gpui::TestAppContext) {
    let mut registry = BufferRegistry::new();
    let first_handle = registry.register(WeakEntity::new_invalid());
    let second_handle = registry.register(WeakEntity::new_invalid());
    assert_ne!(first_handle, second_handle);

    registry.invalidate(first_handle);
    assert!(cx.read(|cx| registry.resolve(first_handle, cx)).is_err());

    let third_handle = registry.register(WeakEntity::new_invalid());
    assert_ne!(third_handle, first_handle);
    assert_ne!(third_handle, second_handle);
}

#[gpui::test]
fn opening_the_same_model_reuses_its_handle(cx: &mut gpui::TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("text")));
    let other = cx.new(|_| Buffer::new(BufferModel::from_text("other")));
    let mut registry = BufferRegistry::new();
    let first = registry.open(&model);
    assert_eq!(registry.open(&model), first);
    assert_eq!(registry.handle_for(&model), Some(first));
    assert_ne!(registry.open(&other), first);
    assert_eq!(registry.buffers.len(), 2);
}

#[test]
fn buffer_subscriptions_are_scoped_by_buffer_and_lifecycle() {
    let mut registry = BufferSubscriptionRegistry::new();
    let first_buffer = BufferHandle::new(1);
    let second_buffer = BufferHandle::new(2);
    let extension = ExtensionId::new(7);
    let first_lifecycle = ExtensionLifecycleId::new(3);
    let second_lifecycle = ExtensionLifecycleId::new(4);
    let first = registry.subscribe(first_buffer, extension, first_lifecycle);
    let second = registry.subscribe(first_buffer, extension, second_lifecycle);
    registry.subscribe(second_buffer, ExtensionId::new(8), first_lifecycle);

    assert!(!registry.unsubscribe(first, extension, second_lifecycle));
    assert_eq!(registry.for_buffer(first_buffer).count(), 2);

    registry.remove_lifecycle(extension, first_lifecycle);
    assert_eq!(
        registry.for_buffer(first_buffer).collect::<Vec<_>>(),
        [BufferSubscription {
            id: second,
            buffer: first_buffer,
            extension,
            lifecycle: second_lifecycle,
        }]
    );
    assert_eq!(registry.for_buffer(second_buffer).count(), 1);

    registry.remove_buffer(second_buffer);
    assert_eq!(registry.for_buffer(second_buffer).count(), 0);
}

#[gpui::test]
fn an_over_budget_parse_continues_in_the_background(cx: &mut gpui::TestAppContext) {
    use std::{sync::Arc, time::Duration};

    use crate::app::syntax::SyntaxLanguage;

    let language = SyntaxLanguage::new("rust", "\"fn\" @keyword\n(line_comment) @comment\n", "");
    let syntax = SyntaxState::new(Arc::new(language.unwrap()), Arc::new(|_| None));
    let buffer = cx.new(|_| {
        let mut model = BufferModel::from_text("fn a() { let x = 1; }\n".repeat(2_000));
        model.set_syntax_budget(Duration::ZERO);
        Buffer::new(model)
    });
    buffer.update(cx, |buffer, cx| buffer.set_syntax(Some(syntax), cx));
    assert!(buffer.read_with(cx, |buffer, _| buffer.syntax_highlights(0..2).is_empty()));

    // A commit notifies while the parse runs; the result is rebased and the
    // driver parses again until the tree is current.
    buffer.update(cx, |buffer, cx| {
        buffer.replace(0..0, "// c\n").unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    buffer.read_with(cx, |buffer, _| {
        assert!(buffer.syntax_is_current());
        let names: Vec<_> = buffer
            .syntax_highlights(0..7)
            .into_iter()
            .map(|(range, highlight)| (buffer.read(range), highlight.name()))
            .collect();
        assert_eq!(
            names,
            [("// c".into(), "comment"), ("fn".into(), "keyword")]
        );
    });
}
