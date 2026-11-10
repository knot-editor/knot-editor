//! The application's documents: the global wrapping the plain
//! [`knot_model::documents::DocumentCollection`] over buffer entities, and the
//! effects document changes have on the rest of the application.

use std::sync::Arc;

use gpui::{App, Entity, Global};

use super::model::Buffer;
use super::resource::ResourceUri;

pub(crate) use knot_model::documents::{DocumentState, PersistenceCapture};
pub(crate) use knot_model::workbench::DocumentId;

use super::filesystem::ResourceVersion;

pub(crate) type Document = knot_model::documents::Document<Entity<Buffer>>;

/// Dirty state, which reads the buffer entity.
pub(crate) trait DocumentDirty {
    fn is_dirty(&self, cx: &App) -> bool;
}

impl DocumentDirty for Document {
    fn is_dirty(&self, cx: &App) -> bool {
        self.is_dirty_with(|model, revision| model.read(cx).content_matches_revision(revision))
    }
}

/// The application-owned lifetime and persistence registry for documents. The
/// plain collection decides; this wrapper stamps the model, detects its
/// language and indentation, and tells recovery and observation.
pub(crate) struct DocumentCollection(knot_model::documents::DocumentCollection<Entity<Buffer>>);

impl std::ops::Deref for DocumentCollection {
    type Target = knot_model::documents::DocumentCollection<Entity<Buffer>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for DocumentCollection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DocumentCollection {
    pub(crate) fn new() -> Self {
        Self(knot_model::documents::DocumentCollection::new())
    }

    pub(crate) fn create_untitled(
        &mut self,
        title: impl Into<Arc<str>>,
        model: Entity<Buffer>,
        cx: &mut App,
    ) -> DocumentId {
        let clean_revision = model.read(cx).revision();
        let id = self.0.insert_untitled(title, model.clone(), clean_revision);
        self.attach(id, &model, cx);
        id
    }

    pub(crate) fn create_destination(
        &mut self,
        title: impl Into<Arc<str>>,
        model: Entity<Buffer>,
        uri: ResourceUri,
        cx: &mut App,
    ) -> DocumentId {
        let inserted = self.0.insert_destination(title, model.clone(), uri);
        self.attach_new(inserted, &model, cx)
    }

    pub(crate) fn create_persisted(
        &mut self,
        title: impl Into<Arc<str>>,
        model: Entity<Buffer>,
        uri: ResourceUri,
        persisted_revision: u64,
        version: ResourceVersion,
        cx: &mut App,
    ) -> DocumentId {
        let inserted =
            self.0
                .insert_persisted(title, model.clone(), uri, persisted_revision, version);
        self.attach_new(inserted, &model, cx)
    }

    /// Insert a recovered user document, keeping its recorded identity when one
    /// is given. Returns `None` when the identity or resource is already live,
    /// so recovered content never replaces an open document.
    pub(crate) fn create_recovered(
        &mut self,
        title: impl Into<Arc<str>>,
        model: Entity<Buffer>,
        state: DocumentState,
        identity: Option<uuid::Uuid>,
        cx: &mut App,
    ) -> Option<DocumentId> {
        let id = self
            .0
            .insert_recovered(title, model.clone(), state, identity)?;
        self.attach(id, &model, cx);
        Some(id)
    }

    pub(crate) fn remove(&mut self, id: DocumentId, cx: &mut App) -> bool {
        let Some(document) = self.0.take(id) else {
            return false;
        };
        document.model().update(cx, |model, cx| {
            model.close();
            cx.notify();
        });
        super::settings::release_document(document.identity(), cx);
        super::recovery::forget_document(document.identity(), cx);
        super::application_observation::changed(cx);
        true
    }

    /// Commit exactly the bytes and revision captured before asynchronous I/O;
    /// see [`knot_model::documents::DocumentCollection::complete_persistence`].
    pub(crate) fn finish_persistence(
        &mut self,
        id: DocumentId,
        model: &Entity<Buffer>,
        capture: &PersistenceCapture,
        resource: (impl Into<Arc<str>>, ResourceUri),
        persisted_state: (ResourceVersion, u64),
        cx: &mut App,
    ) -> bool {
        let Some(moved) =
            self.0
                .complete_persistence(id, model, capture, resource, persisted_state)
        else {
            return false;
        };
        let document = self.0.get(id).expect("a completed document is live");
        if moved {
            super::languages::detect_document(document.model(), document.resource_uri(), cx);
        }
        // A successful persistence changes the saved baseline while edits racing
        // the I/O remain dirty; re-publish so the automatic record matches the
        // committed state or is retired once the document is clean.
        super::recovery::publish(document, cx);
        super::application_observation::changed(cx);
        true
    }

    /// Load `text` into the document's model as one undoable edit of only the
    /// changed lines, then commit it as the persisted state like
    /// [`Self::finish_persistence`]. Fails when the model refuses the edits or
    /// the capture is stale.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish_reload(
        &mut self,
        id: DocumentId,
        model: &Entity<Buffer>,
        capture: &PersistenceCapture,
        text: &str,
        resource: (impl Into<Arc<str>>, ResourceUri),
        version: ResourceVersion,
        cx: &mut App,
    ) -> bool {
        let edits = knot_model::reload::line_edits(&model.read(cx).text(), text);
        let applied = model.update(cx, |model, cx| {
            let revision = model.revision();
            let applied = model.apply_edits(&edits, revision);
            if matches!(applied, Ok(true)) {
                cx.notify();
            }
            applied.is_ok()
        });
        let revision = model.read(cx).revision();
        applied && self.finish_persistence(id, model, capture, resource, (version, revision), cx)
    }

    fn attach_new(
        &mut self,
        inserted: knot_model::documents::Inserted,
        model: &Entity<Buffer>,
        cx: &mut App,
    ) -> DocumentId {
        if let knot_model::documents::Inserted::New(id) = inserted {
            self.attach(id, model, cx);
        }
        inserted.id()
    }

    /// Stamp a new document's model with its identity and tell the rest of the
    /// application about it.
    fn attach(&self, id: DocumentId, model: &Entity<Buffer>, cx: &mut App) {
        let document = self.0.get(id).expect("an inserted document is live");
        let identity = document.identity();
        super::languages::detect_document(model, document.resource_uri(), cx);
        model.update(cx, |model, _| model.set_document(identity));
        if document.resource_uri().is_some() {
            detect_indentation(model, cx);
        }
        super::recovery::observe_document(model, identity, cx);
        super::application_observation::changed(cx);
    }
}

/// Record the indentation style a stamped buffer's text uses as its
/// document's `editor.insertSpaces`, when the text has a clear style.
pub(crate) fn detect_indentation(model: &Entity<Buffer>, cx: &mut App) {
    let model = model.read(cx);
    if let (Some(document), Some(style)) = (model.document(), model.detect_indent_style()) {
        super::settings::set_document_insert_spaces(
            document,
            style == super::indentation::IndentStyle::Spaces,
            cx,
        );
    }
}

#[derive(Clone)]
pub(crate) struct ApplicationDocuments(pub(crate) Entity<DocumentCollection>);

impl Global for ApplicationDocuments {}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::app::model::BufferModel;

    fn version(value: u8) -> ResourceVersion {
        ResourceVersion::new([value])
    }

    #[gpui::test]
    fn application_global_owns_the_document_collection(cx: &mut TestAppContext) {
        let documents = cx.new(|_| DocumentCollection::new());
        let weak = documents.downgrade();
        cx.set_global(ApplicationDocuments(documents.clone()));

        drop(documents);

        assert_eq!(
            cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone()),
            weak.upgrade().unwrap()
        );
    }

    #[gpui::test]
    fn collection_strongly_owns_models_until_document_removal(cx: &mut TestAppContext) {
        let model = cx.new(|_| Buffer::new(BufferModel::from_text("owned")));
        let weak = model.downgrade();
        let mut documents = DocumentCollection::new();
        let id = cx.update(|cx| documents.create_untitled("Untitled", model, cx));

        assert!(!documents.get(id).unwrap().identity().is_nil());
        assert!(weak.upgrade().is_some());
        assert!(cx.update(|cx| documents.remove(id, cx)));
        assert!(weak.upgrade().is_none());
        assert!(!cx.update(|cx| documents.remove(id, cx)));
    }

    #[gpui::test]
    fn states_have_explicit_dirty_semantics_and_transitions(cx: &mut TestAppContext) {
        let destination_uri = ResourceUri::parse("mem://workspace/new.txt").unwrap();
        let persisted_uri = ResourceUri::parse("mem://workspace/saved.txt").unwrap();
        let untitled_uri = ResourceUri::parse("mem://workspace/untitled.txt").unwrap();
        let untitled = cx.new(|_| Buffer::new(BufferModel::from_text("")));
        let destination = cx.new(|_| Buffer::new(BufferModel::from_text("")));
        let persisted = cx.new(|_| Buffer::new(BufferModel::from_text("saved")));
        let mut documents = DocumentCollection::new();

        let untitled_id =
            cx.update(|cx| documents.create_untitled("Untitled", untitled.clone(), cx));
        let destination_id = cx.update(|cx| {
            documents.create_destination(
                "new.txt",
                destination.clone(),
                destination_uri.clone(),
                cx,
            )
        });
        let persisted_id = cx.update(|cx| {
            documents.create_persisted(
                "saved.txt",
                persisted.clone(),
                persisted_uri.clone(),
                0,
                version(1),
                cx,
            )
        });

        assert!(matches!(
            documents.get(untitled_id).unwrap().state(),
            DocumentState::Untitled { .. }
        ));
        assert!(cx.read(|cx| documents.get(destination_id).unwrap().is_dirty(cx)));
        assert!(!cx.read(|cx| documents.get(persisted_id).unwrap().is_dirty(cx)));

        untitled.update(cx, |model, _| model.replace(0..0, "edit").unwrap());
        persisted.update(cx, |model, _| model.replace(0..0, "edit ").unwrap());
        assert!(cx.read(|cx| documents.get(untitled_id).unwrap().is_dirty(cx)));
        assert!(cx.read(|cx| documents.get(persisted_id).unwrap().is_dirty(cx)));

        untitled.update(cx, |model, _| model.undo().unwrap());
        persisted.update(cx, |model, _| model.undo().unwrap());
        assert!(!cx.read(|cx| documents.get(untitled_id).unwrap().is_dirty(cx)));
        assert!(!cx.read(|cx| documents.get(persisted_id).unwrap().is_dirty(cx)));
        untitled.update(cx, |model, _| model.redo().unwrap());
        persisted.update(cx, |model, _| model.redo().unwrap());
        assert!(cx.read(|cx| documents.get(untitled_id).unwrap().is_dirty(cx)));
        assert!(cx.read(|cx| documents.get(persisted_id).unwrap().is_dirty(cx)));

        let capture = documents
            .begin_persistence(destination_id, &destination)
            .unwrap();
        assert!(cx.update(|cx| documents.finish_persistence(
            destination_id,
            &destination,
            &capture,
            ("new.txt", destination_uri.clone()),
            (version(2), 0),
            cx
        )));
        assert!(matches!(
            documents.get(destination_id).unwrap().state(),
            DocumentState::Persisted { .. }
        ));
        assert!(!cx.read(|cx| documents.get(destination_id).unwrap().is_dirty(cx)));

        let untitled_revision = untitled.read_with(cx, |model, _| model.revision());
        let capture = documents.begin_persistence(untitled_id, &untitled).unwrap();
        assert!(cx.update(|cx| documents.finish_persistence(
            untitled_id,
            &untitled,
            &capture,
            ("untitled.txt", untitled_uri.clone()),
            (version(3), untitled_revision),
            cx
        )));
        let document = documents.get(untitled_id).unwrap();
        assert_eq!(document.title(), "untitled.txt");
        assert_eq!(document.resource_uri(), Some(&untitled_uri));
        assert!(!cx.read(|cx| document.is_dirty(cx)));
    }

    #[gpui::test]
    fn normalized_resource_identity_is_deduplicated_across_states(cx: &mut TestAppContext) {
        let uri = ResourceUri::parse("MEM://workspace/src/../notes.txt").unwrap();
        let first_model = cx.new(|_| Buffer::new(BufferModel::from_text("first")));
        let duplicate_model = cx.new(|_| Buffer::new(BufferModel::from_text("duplicate")));
        let duplicate_weak = duplicate_model.downgrade();
        let untitled_model = cx.new(|_| Buffer::new(BufferModel::from_text("untitled")));
        let mut documents = DocumentCollection::new();

        let first = cx.update(|cx| {
            documents.create_destination("notes.txt", first_model.clone(), uri.clone(), cx)
        });
        let duplicate = cx.update(|cx| {
            documents.create_persisted("notes.txt", duplicate_model, uri.clone(), 0, version(1), cx)
        });

        assert_eq!(first, duplicate);
        assert_eq!(documents.documents().count(), 1);
        assert_eq!(documents.document_for_resource(&uri), Some(first));
        assert_eq!(documents.get(first).unwrap().model(), &first_model);
        let first_identity = documents.get(first).unwrap().identity();
        assert_eq!(documents.get(duplicate).unwrap().identity(), first_identity);
        assert!(duplicate_weak.upgrade().is_none());

        let untitled =
            cx.update(|cx| documents.create_untitled("Untitled", untitled_model.clone(), cx));
        let capture = documents
            .begin_persistence(untitled, &untitled_model)
            .unwrap();
        assert!(!cx.update(|cx| documents.finish_persistence(
            untitled,
            &untitled_model,
            &capture,
            ("notes.txt", uri.clone()),
            (version(2), 0),
            cx
        )));
        assert!(matches!(
            documents.get(untitled).unwrap().state(),
            DocumentState::Untitled { .. }
        ));

        assert!(cx.update(|cx| documents.remove(first, cx)));
        assert_eq!(documents.document_for_resource(&uri), None);
        let reopened = cx.update(|cx| {
            documents.create_persisted(
                "notes.txt",
                cx.new(|_| Buffer::new(BufferModel::from_text("reopened"))),
                uri,
                0,
                version(3),
                cx,
            )
        });
        assert_ne!(reopened, first);
        assert_ne!(documents.get(reopened).unwrap().identity(), first_identity);
    }

    #[gpui::test]
    fn edit_racing_save_keeps_persisted_document_dirty(cx: &mut TestAppContext) {
        let uri = ResourceUri::parse("mem://workspace/notes.txt").unwrap();
        let model = cx.new(|_| Buffer::new(BufferModel::from_text("notes")));
        let mut documents = DocumentCollection::new();
        let id = cx.update(|cx| {
            documents.create_persisted("notes.txt", model.clone(), uri.clone(), 0, version(1), cx)
        });

        model.update(cx, |model, _| model.replace(0..0, "first ").unwrap());
        let captured_revision = model.read_with(cx, |model, _| model.revision());
        model.update(cx, |model, _| model.replace(0..0, "second ").unwrap());

        let capture = documents.begin_persistence(id, &model).unwrap();
        assert!(cx.update(|cx| documents.finish_persistence(
            id,
            &model,
            &capture,
            ("notes.txt", uri.clone()),
            (version(2), captured_revision),
            cx
        )));
        let document = documents.get(id).unwrap();
        assert_eq!(
            document.state().persisted_revision(),
            Some(captured_revision)
        );
        assert!(cx.read(|cx| document.is_dirty(cx)));
        model.update(cx, |model, _| model.undo().unwrap());
        assert!(!cx.read(|cx| document.is_dirty(cx)));
    }
}
