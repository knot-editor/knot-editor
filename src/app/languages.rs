//! Language registrations (the plain registry is `knot_model::languages`), document language detection, and the syntax state
//! each buffer's language attaches.
//!
//! Extensions and personal config register languages: a language ID, the
//! compiled-in grammar it parses with, the file names, extensions, and shebang
//! interpreters it claims, its queries, and the editing data native editing
//! commands read (see [`editing`]). An association claims patterns for
//! an ID without defining the language. The latest registration of an ID
//! defines that language, and the latest registration or association claiming
//! a pattern decides detection, so removing either reveals the previous one.
//!
//! A buffer records a language ID, explicit or detected, independently of
//! registration: an unregistered ID paints and reports as `plaintext` until
//! something registers it. Every registration change re-detects open documents whose
//! language is not explicit and reattaches syntax wherever the resolved
//! language changed.

use std::{collections::HashMap, sync::Arc};

use gpui::{App, Entity, EntityId, Global, WeakEntity};

use knot_protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};

pub(crate) use knot_model::editing;
pub(crate) use knot_model::languages::*;

use super::{
    documents::ApplicationDocuments,
    model::{Buffer, BufferModel},
    resource::ResourceUri,
    syntax::SyntaxState,
};

/// The application's language registrations, and the buffers whose syntax
/// follows them.
#[derive(Default)]
pub(crate) struct ApplicationLanguages {
    registry: LanguageRegistry,
    /// Every live buffer whose language has been resolved, so a registration
    /// change can reattach its syntax.
    models: HashMap<EntityId, WeakEntity<Buffer>>,
}

impl Global for ApplicationLanguages {}

pub(crate) fn register(owner: LanguageOwner, language: CompiledLanguage, cx: &mut App) -> u64 {
    let id = cx
        .default_global::<ApplicationLanguages>()
        .registry
        .register(owner, language);
    refresh(cx);
    id
}

pub(crate) fn associate(
    owner: LanguageOwner,
    association: LanguageAssociation,
    cx: &mut App,
) -> u64 {
    let id = cx
        .default_global::<ApplicationLanguages>()
        .registry
        .associate(owner, association);
    refresh(cx);
    id
}

pub(crate) fn unregister(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    id: u64,
    cx: &mut App,
) -> bool {
    let removed = cx
        .default_global::<ApplicationLanguages>()
        .registry
        .unregister(extension, lifecycle, id);
    if removed {
        refresh(cx);
    }
    removed
}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut App,
) {
    if cx.has_global::<ApplicationLanguages>()
        && cx
            .global_mut::<ApplicationLanguages>()
            .registry
            .remove_lifecycle(extension, lifecycle)
    {
        refresh(cx);
    }
}

/// The language a buffer reports: its ID while registered, else plain text.
pub(crate) fn effective_language(model: &BufferModel, cx: &App) -> String {
    model
        .language()
        .filter(|id| {
            cx.try_global::<ApplicationLanguages>()
                .is_some_and(|languages| languages.registry.resolve(id).is_some())
        })
        .unwrap_or(PLAIN_TEXT)
        .to_owned()
}

/// The editing configuration of a registered language ID.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn editing_configuration(
    id: &str,
    cx: &App,
) -> Option<Arc<editing::EditingConfiguration>> {
    cx.try_global::<ApplicationLanguages>()
        .and_then(|languages| languages.registry.editing(id))
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn registry(cx: &App) -> &LanguageRegistry {
    &cx.global::<ApplicationLanguages>().registry
}

pub(crate) fn list(cx: &App) -> Vec<LanguageInfo> {
    cx.try_global::<ApplicationLanguages>()
        .map(|languages| languages.registry.list())
        .unwrap_or_default()
}

pub(crate) fn get(id: &str, cx: &App) -> Option<LanguageDefinition> {
    cx.try_global::<ApplicationLanguages>()
        .and_then(|languages| languages.registry.get(id).cloned())
}

/// Detect a document's language from its resource name and first line,
/// unless its language is explicit, and attach the matching syntax.
pub(crate) fn detect_document(model: &Entity<Buffer>, uri: Option<&ResourceUri>, cx: &mut App) {
    redetect(model, uri, cx);
    attach(model, false, cx);
}

/// Make `language` a document's explicit language, or return the document to
/// detection with `None`.
pub(crate) fn set_document_language(
    model: &Entity<Buffer>,
    uri: Option<&ResourceUri>,
    language: Option<String>,
    cx: &mut App,
) {
    match language {
        Some(language) => model.update(cx, |model, _| model.set_language(Some(language), true)),
        None => model.update(cx, |model, _| model.set_language(None, false)),
    }
    detect_document(model, uri, cx);
}

/// Give a buffer that is not a document, such as a text frame's, a fixed
/// language.
pub(crate) fn set_language(model: &Entity<Buffer>, language: String, cx: &mut App) {
    model.update(cx, |model, _| model.set_language(Some(language), true));
    attach(model, false, cx);
}

fn redetect(model: &Entity<Buffer>, uri: Option<&ResourceUri>, cx: &mut App) {
    if model.read(cx).language_is_explicit() {
        return;
    }
    let file_name = uri.and_then(|uri| uri.as_url().path_segments()?.next_back());
    let first_line = shebang_line(model.read(cx));
    let detected = cx
        .default_global::<ApplicationLanguages>()
        .registry
        .detect(file_name, &first_line);
    model.update(cx, |model, _| model.set_language(detected, false));
}

/// Give `model` a fresh syntax state when the language its ID resolves to
/// differs from the one it parses. After a registration change, a language
/// that injects reparses too, since its injections may resolve differently.
fn attach(model: &Entity<Buffer>, registration_changed: bool, cx: &mut App) {
    let languages = cx.default_global::<ApplicationLanguages>();
    if !languages.models.contains_key(&model.entity_id()) {
        languages
            .models
            .retain(|_, model| model.upgrade().is_some());
        languages
            .models
            .insert(model.entity_id(), model.downgrade());
    }
    let resolved = model
        .read(cx)
        .language()
        .and_then(|id| cx.global::<ApplicationLanguages>().registry.resolve(id));
    let current = model.read(cx).syntax_language();
    let unchanged = match (&resolved, current) {
        (Some(resolved), Some(current)) => {
            Arc::ptr_eq(resolved, current) && !(registration_changed && resolved.can_inject())
        }
        (None, None) => true,
        _ => false,
    };
    if unchanged {
        return;
    }
    let resolver = cx.global::<ApplicationLanguages>().registry.resolver();
    model.update(cx, |model, cx| {
        model.set_syntax(
            resolved.map(|language| SyntaxState::new(language, resolver)),
            cx,
        )
    });
}

/// Follow a registration change: re-detect every open document whose
/// language is not explicit, then reattach every tracked buffer's syntax.
///
/// This reads `ApplicationDocuments`, so a registration change must never
/// happen while the document collection is being updated; the collection
/// itself only calls `detect_document`, which does not refresh.
fn refresh(cx: &mut App) {
    let documents = cx
        .try_global::<ApplicationDocuments>()
        .map(|documents| {
            documents
                .0
                .read(cx)
                .documents()
                .map(|document| (document.model().clone(), document.resource_uri().cloned()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (model, uri) in &documents {
        redetect(model, uri.as_ref(), cx);
    }
    let models: Vec<_> = cx
        .default_global::<ApplicationLanguages>()
        .models
        .values()
        .filter_map(WeakEntity::upgrade)
        .collect();
    for model in models {
        if model.read(cx).is_open() {
            attach(&model, true, cx);
        }
    }
    super::application_observation::changed(cx);
}

#[cfg(test)]
mod tests;
