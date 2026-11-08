//! The document collection: identity, persistence relationship, and dirty
//! state of the open documents.
//!
//! A document pairs a buffer model handle, `M`, with the identity the
//! application keeps across persistence and presentation changes. The
//! collection never reads or changes the model: callers pass revisions in and
//! read content equality through [`Document::is_dirty_with`], so the owner of
//! the handle decides how to reach the model.

use std::{collections::HashMap, sync::Arc};

use crate::{filesystem::ResourceVersion, resource::ResourceUri, workbench::DocumentId};

/// A document's persistence relationship.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentState {
    /// A user document with no destination yet.
    ///
    /// It is clean at creation and dirty whenever its current content state no
    /// longer matches the state recorded at `clean_revision`.
    Untitled { clean_revision: u64 },
    /// A user document associated with a destination that does not exist yet.
    ///
    /// It remains dirty until the first successful persistence transitions it
    /// to [`DocumentState::Persisted`].
    Destination { uri: ResourceUri },
    /// A user document whose contents have been persisted to a resource.
    ///
    /// Edits are dirty while the model's current content state differs from the
    /// state recorded at `persisted_revision`.
    Persisted {
        uri: ResourceUri,
        persisted_revision: u64,
        version: ResourceVersion,
    },
}

impl DocumentState {
    pub fn resource_uri(&self) -> Option<&ResourceUri> {
        match self {
            Self::Destination { uri } | Self::Persisted { uri, .. } => Some(uri),
            Self::Untitled { .. } => None,
        }
    }

    pub fn persisted_revision(&self) -> Option<u64> {
        match self {
            Self::Persisted {
                persisted_revision, ..
            } => Some(*persisted_revision),
            _ => None,
        }
    }

    pub fn persisted_version(&self) -> Option<&ResourceVersion> {
        match self {
            Self::Persisted { version, .. } => Some(version),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistenceCapture {
    identity: uuid::Uuid,
    generation: u64,
    state: DocumentState,
}

/// A document: identity, persistence relationship, and its model handle.
pub struct Document<M> {
    id: DocumentId,
    identity: uuid::Uuid,
    title: Arc<str>,
    model: M,
    state: DocumentState,
    persistence_generation: u64,
}

impl<M> Document<M> {
    pub fn id(&self) -> DocumentId {
        self.id
    }

    /// Opaque identity retained across persistence and presentation changes.
    pub fn identity(&self) -> uuid::Uuid {
        self.identity
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn model(&self) -> &M {
        &self.model
    }

    pub fn state(&self) -> &DocumentState {
        &self.state
    }

    pub fn resource_uri(&self) -> Option<&ResourceUri> {
        self.state.resource_uri()
    }

    /// Whether the document differs from its clean baseline. `content_matches`
    /// says whether the model's content equals its content at a revision.
    pub fn is_dirty_with(&self, content_matches: impl FnOnce(&M, u64) -> bool) -> bool {
        match self.state {
            DocumentState::Untitled { clean_revision } => {
                !content_matches(&self.model, clean_revision)
            }
            DocumentState::Destination { .. } => true,
            DocumentState::Persisted {
                persisted_revision, ..
            } => !content_matches(&self.model, persisted_revision),
        }
    }
}

/// Whether an insertion made a document or found the resource's existing one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Inserted {
    New(DocumentId),
    Existing(DocumentId),
}

impl Inserted {
    pub fn id(self) -> DocumentId {
        match self {
            Self::New(id) | Self::Existing(id) => id,
        }
    }
}

/// The application-owned lifetime and persistence registry for documents.
pub struct DocumentCollection<M> {
    next_id: u64,
    documents: Vec<Document<M>>,
    resources: HashMap<ResourceUri, DocumentId>,
}

impl<M> Default for DocumentCollection<M> {
    fn default() -> Self {
        Self {
            next_id: 1,
            documents: Vec::new(),
            resources: HashMap::new(),
        }
    }
}

impl<M: PartialEq> DocumentCollection<M> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a user document with no destination, clean at `clean_revision`.
    pub fn insert_untitled(
        &mut self,
        title: impl Into<Arc<str>>,
        model: M,
        clean_revision: u64,
    ) -> DocumentId {
        self.insert(
            title,
            model,
            DocumentState::Untitled { clean_revision },
            uuid::Uuid::new_v4(),
        )
    }

    pub fn insert_destination(
        &mut self,
        title: impl Into<Arc<str>>,
        model: M,
        uri: ResourceUri,
    ) -> Inserted {
        self.insert_resource(title, model, DocumentState::Destination { uri })
    }

    pub fn insert_persisted(
        &mut self,
        title: impl Into<Arc<str>>,
        model: M,
        uri: ResourceUri,
        persisted_revision: u64,
        version: ResourceVersion,
    ) -> Inserted {
        self.insert_resource(
            title,
            model,
            DocumentState::Persisted {
                uri,
                persisted_revision,
                version,
            },
        )
    }

    /// Insert a recovered user document, keeping its recorded identity when one
    /// is given. Returns `None` when the identity or resource is already live,
    /// so recovered content never replaces an open document.
    pub fn insert_recovered(
        &mut self,
        title: impl Into<Arc<str>>,
        model: M,
        state: DocumentState,
        identity: Option<uuid::Uuid>,
    ) -> Option<DocumentId> {
        if identity.is_some_and(|identity| self.document_by_identity(identity).is_some())
            || state
                .resource_uri()
                .is_some_and(|uri| self.resources.contains_key(uri))
        {
            return None;
        }
        let uri = state.resource_uri().cloned();
        let id = self.insert(
            title,
            model,
            state,
            identity.unwrap_or_else(uuid::Uuid::new_v4),
        );
        if let Some(uri) = uri {
            self.resources.insert(uri, id);
        }
        Some(id)
    }

    pub fn documents(&self) -> impl Iterator<Item = &Document<M>> {
        self.documents.iter()
    }

    pub fn get(&self, id: DocumentId) -> Option<&Document<M>> {
        self.documents.iter().find(|document| document.id == id)
    }

    pub fn document_by_identity(&self, identity: uuid::Uuid) -> Option<&Document<M>> {
        self.documents
            .iter()
            .find(|document| document.identity == identity)
    }

    pub fn document_for_resource(&self, uri: &ResourceUri) -> Option<DocumentId> {
        self.resources.get(uri).copied()
    }

    /// Remove a document and release its resource.
    pub fn take(&mut self, id: DocumentId) -> Option<Document<M>> {
        let index = self
            .documents
            .iter()
            .position(|document| document.id == id)?;
        let document = self.documents.remove(index);
        if let Some(uri) = document.resource_uri() {
            self.resources.remove(uri);
        }
        Some(document)
    }

    pub fn begin_persistence(&mut self, id: DocumentId, model: &M) -> Option<PersistenceCapture> {
        let document = self
            .documents
            .iter_mut()
            .find(|document| document.id == id)?;
        if &document.model != model {
            return None;
        }
        document.persistence_generation = document
            .persistence_generation
            .checked_add(1)
            .expect("document persistence generation overflowed");
        Some(PersistenceCapture {
            identity: document.identity(),
            generation: document.persistence_generation,
            state: document.state.clone(),
        })
    }

    /// Commit exactly the bytes and revision captured before asynchronous I/O.
    ///
    /// The document may have acquired newer edits, which remain dirty. Identity
    /// changes and later persistence attempts invalidate this completion.
    /// Returns whether the document moved to a new resource, or `None` when
    /// the completion is stale.
    pub fn complete_persistence(
        &mut self,
        id: DocumentId,
        model: &M,
        capture: &PersistenceCapture,
        resource: (impl Into<Arc<str>>, ResourceUri),
        persisted_state: (ResourceVersion, u64),
    ) -> Option<bool> {
        let (title, uri) = resource;
        let (version, revision) = persisted_state;
        if self
            .resources
            .get(&uri)
            .is_some_and(|existing| *existing != id)
        {
            return None;
        }
        let document = self
            .documents
            .iter_mut()
            .find(|document| document.id == id)?;
        if document.identity() != capture.identity
            || &document.model != model
            || document.persistence_generation != capture.generation
            || document.state != capture.state
        {
            return None;
        }
        // Only Save As, or a first save, moves the document to a new resource;
        // a plain Save keeps its language even after its shebang changed.
        let moved = document.resource_uri() != Some(&uri);
        if let Some(old_uri) = document.resource_uri() {
            self.resources.remove(old_uri);
        }
        document.title = title.into();
        document.state = DocumentState::Persisted {
            uri: uri.clone(),
            persisted_revision: revision,
            version,
        };
        self.resources.insert(uri, id);
        Some(moved)
    }

    fn insert_resource(
        &mut self,
        title: impl Into<Arc<str>>,
        model: M,
        state: DocumentState,
    ) -> Inserted {
        let uri = state
            .resource_uri()
            .expect("resource document state has a URI")
            .clone();
        if let Some(existing) = self.document_for_resource(&uri) {
            return Inserted::Existing(existing);
        }
        let id = self.insert(title, model, state, uuid::Uuid::new_v4());
        self.resources.insert(uri, id);
        Inserted::New(id)
    }

    fn insert(
        &mut self,
        title: impl Into<Arc<str>>,
        model: M,
        state: DocumentState,
        identity: uuid::Uuid,
    ) -> DocumentId {
        let id = DocumentId::new(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("document identity space exhausted");
        self.documents.push(Document {
            id,
            identity,
            title: title.into(),
            model,
            state,
            persistence_generation: 0,
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(value: u8) -> ResourceVersion {
        ResourceVersion::new([value])
    }

    fn uri(text: &str) -> ResourceUri {
        ResourceUri::parse(text).unwrap()
    }

    #[test]
    fn resources_are_deduplicated_and_untitled_documents_never_take_one() {
        let uri = uri("MEM://workspace/src/../notes.txt");
        let mut documents = DocumentCollection::new();
        let first = documents.insert_destination("notes.txt", 1u32, uri.clone());
        let duplicate = documents.insert_persisted("notes.txt", 2, uri.clone(), 0, version(1));

        assert!(matches!(first, Inserted::New(_)));
        assert_eq!(duplicate, Inserted::Existing(first.id()));
        assert_eq!(documents.documents().count(), 1);
        assert_eq!(documents.get(first.id()).unwrap().model(), &1);

        let untitled = documents.insert_untitled("Untitled", 3, 0);
        let capture = documents.begin_persistence(untitled, &3).unwrap();
        assert_eq!(
            documents.complete_persistence(
                untitled,
                &3,
                &capture,
                ("notes.txt", uri.clone()),
                (version(2), 0)
            ),
            None
        );

        let identity = documents.get(first.id()).unwrap().identity();
        assert!(documents.take(first.id()).is_some());
        assert_eq!(documents.document_for_resource(&uri), None);
        let reopened = documents.insert_persisted("notes.txt", 4, uri, 0, version(3));
        assert_ne!(reopened.id(), first.id());
        assert_ne!(documents.get(reopened.id()).unwrap().identity(), identity);
    }

    #[test]
    fn dirty_state_follows_the_baseline_of_each_persistence_state() {
        let mut documents = DocumentCollection::new();
        let untitled = documents.insert_untitled("Untitled", 1u32, 5);
        let destination = documents
            .insert_destination("a.txt", 2, uri("mem://w/a.txt"))
            .id();
        let persisted = documents
            .insert_persisted("b.txt", 3, uri("mem://w/b.txt"), 7, version(1))
            .id();
        // The model's content matches revision 5 and 7 only.
        let matches = |_: &u32, revision: u64| revision == 5 || revision == 7;
        assert!(!documents.get(untitled).unwrap().is_dirty_with(matches));
        assert!(documents.get(destination).unwrap().is_dirty_with(matches));
        assert!(!documents.get(persisted).unwrap().is_dirty_with(matches));
        assert!(
            documents
                .get(persisted)
                .unwrap()
                .is_dirty_with(|_, _| false)
        );
    }

    #[test]
    fn recovered_documents_keep_identity_and_never_displace_live_ones() {
        let uri = uri("mem://workspace/notes.txt");
        let identity = uuid::Uuid::new_v4();
        let mut documents = DocumentCollection::new();
        let id = documents
            .insert_recovered(
                "notes.txt",
                1u32,
                DocumentState::Persisted {
                    uri: uri.clone(),
                    persisted_revision: 0,
                    version: version(1),
                },
                Some(identity),
            )
            .unwrap();
        assert_eq!(documents.get(id).unwrap().identity(), identity);
        assert_eq!(documents.document_for_resource(&uri), Some(id));

        for (state, identity) in [
            (
                DocumentState::Untitled { clean_revision: 0 },
                Some(identity),
            ),
            (DocumentState::Destination { uri: uri.clone() }, None),
        ] {
            assert!(
                documents
                    .insert_recovered("other", 2, state, identity)
                    .is_none()
            );
        }
        assert_eq!(documents.documents().count(), 1);
    }

    #[test]
    fn stale_persistence_completion_cannot_change_document_state() {
        let uri = uri("mem://workspace/notes.txt");
        let mut documents = DocumentCollection::new();
        let id = documents
            .insert_persisted("notes.txt", 1u32, uri.clone(), 0, version(1))
            .id();

        assert!(documents.begin_persistence(id, &2).is_none());
        let stale = documents.begin_persistence(id, &1).unwrap();
        let current = documents.begin_persistence(id, &1).unwrap();
        let finish = |documents: &mut DocumentCollection<u32>, id, capture| {
            documents.complete_persistence(
                id,
                &1,
                capture,
                ("notes.txt", uri.clone()),
                (version(2), 1),
            )
        };
        assert_eq!(finish(&mut documents, id, &stale), None);
        assert_eq!(
            finish(&mut documents, DocumentId::new(u64::MAX), &current),
            None
        );
        assert_eq!(
            documents.get(id).unwrap().state().persisted_revision(),
            Some(0)
        );
        assert_eq!(finish(&mut documents, id, &current), Some(false));
        assert_eq!(
            documents.get(id).unwrap().state().persisted_revision(),
            Some(1)
        );
    }

    #[test]
    fn save_as_retargets_resource_identity_only_on_a_current_completion() {
        let old_uri = uri("mem://workspace/old.txt");
        let new_uri = uri("mem://workspace/new.txt");
        let mut documents = DocumentCollection::new();
        let id = documents
            .insert_persisted("old.txt", 1u32, old_uri.clone(), 0, version(1))
            .id();
        let identity = documents.get(id).unwrap().identity();
        let capture = documents.begin_persistence(id, &1).unwrap();

        assert_eq!(
            documents.complete_persistence(
                id,
                &1,
                &capture,
                ("new.txt", new_uri.clone()),
                (version(2), 0)
            ),
            Some(true)
        );
        assert_eq!(documents.document_for_resource(&old_uri), None);
        assert_eq!(documents.document_for_resource(&new_uri), Some(id));
        assert_eq!(documents.get(id).unwrap().title(), "new.txt");
        assert_eq!(documents.get(id).unwrap().identity(), identity);
    }
}
