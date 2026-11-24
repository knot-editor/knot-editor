//! Buffers that packages create outside document identity.
//!
//! Each belongs to the lifecycle that created it and closes with it or on
//! disposal. A buffer created with a `persist` key keeps its text in the
//! package's durable storage: writes are debounced after edits, forced at
//! shutdown, lifecycle teardown, and disposal, and a buffer created with the
//! same key starts from the stored text.

use std::time::Duration;

use gpui::{App, AppContext, Context, Entity, Task};
use knot_protocol::{BufferHandle, ExtensionKey, RequestContext};
use serde_json::Value;

use super::ProductExtensionHost;
use crate::app::extensions::buffers::BufferError;
use crate::app::model::{Buffer, BufferModel};

/// How long after the last edit a persisted buffer's text is written.
const PERSIST_DEBOUNCE: Duration = Duration::from_millis(500);

/// The storage record holding a persisted buffer's text, in the package's
/// records beside the keys its own code uses.
pub(crate) fn persist_record(key: &str) -> String {
    format!("knot.buffer:{key}")
}

/// What `createBuffer` asks for.
#[derive(Clone, Debug)]
pub(crate) struct BufferSpec {
    pub text: String,
    pub language: Option<String>,
    pub read_only: bool,
    pub persist: Option<String>,
}

pub(super) struct ExtensionBufferState {
    owner: ExtensionKey,
    /// The strong reference that keeps the buffer alive until its owner ends it.
    pub(super) model: Entity<Buffer>,
    persist: Option<Persisted>,
}

struct Persisted {
    record: String,
    /// The revision the storage record reflects or a pending write will.
    stored_revision: u64,
    write: Option<Task<()>>,
}

/// A buffer cannot be created.
pub(crate) enum CreateError {
    /// Another live buffer of the package uses the persist key.
    KeyInUse,
}

impl ProductExtensionHost {
    pub(crate) fn create_extension_buffer(
        &mut self,
        request: &RequestContext,
        spec: BufferSpec,
        stored: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<BufferHandle, CreateError> {
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        let record = spec.persist.as_deref().map(persist_record);
        if let Some(record) = &record
            && self.extension_buffers.values().any(|state| {
                state.owner == owner
                    && state
                        .persist
                        .as_ref()
                        .is_some_and(|persisted| &persisted.record == record)
            })
        {
            return Err(CreateError::KeyInUse);
        }
        let text = stored.unwrap_or(spec.text);
        let model = cx.new(|_| {
            Buffer::new(if spec.read_only {
                BufferModel::from_read_only_text(text)
            } else {
                BufferModel::from_text(text)
            })
        });
        if let Some(language) = spec.language {
            crate::app::languages::set_language(&model, language, cx);
        }
        let handle = self.track_buffer(&model, cx);
        let revision = model.read(cx).revision();
        self.extension_buffers.insert(
            handle,
            ExtensionBufferState {
                owner,
                model: model.clone(),
                persist: record.map(|record| Persisted {
                    record,
                    stored_revision: revision,
                    write: None,
                }),
            },
        );
        Ok(handle)
    }

    fn owned_extension_buffer(
        &mut self,
        request: &RequestContext,
        handle: BufferHandle,
    ) -> Result<&mut ExtensionBufferState, BufferError> {
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        self.extension_buffers
            .get_mut(&handle)
            .filter(|state| state.owner == owner)
            .ok_or(BufferError::BufferClosed)
    }

    /// End a buffer on its owner's disposal. Its persisted text is kept.
    pub(crate) fn dispose_extension_buffer(
        &mut self,
        request: &RequestContext,
        handle: BufferHandle,
        cx: &mut Context<Self>,
    ) -> Result<(), BufferError> {
        self.owned_extension_buffer(request, handle)?;
        self.release_extension_buffer(handle, cx);
        Ok(())
    }

    /// Remove a buffer's persisted text; later edits persist again.
    pub(crate) fn clear_persisted_buffer(
        &mut self,
        request: &RequestContext,
        handle: BufferHandle,
        cx: &mut Context<Self>,
    ) -> Result<(), BufferError> {
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        let state = self.owned_extension_buffer(request, handle)?;
        let model = state.model.clone();
        let Some(persisted) = state.persist.as_mut() else {
            return Ok(());
        };
        persisted.write = None;
        persisted.stored_revision = model.read(cx).revision();
        let record = persisted.record.clone();
        self.write_record(owner, record, None);
        Ok(())
    }

    /// Schedule a persisted buffer's write after a change to its text.
    pub(super) fn persist_buffer_changed(&mut self, handle: BufferHandle, cx: &mut Context<Self>) {
        let Some(state) = self.extension_buffers.get_mut(&handle) else {
            return;
        };
        let model = state.model.clone();
        let Some(persisted) = state.persist.as_mut() else {
            return;
        };
        if persisted.stored_revision == model.read(cx).revision() {
            return;
        }
        persisted.write = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PERSIST_DEBOUNCE).await;
            let _ = this.update(cx, |host, cx| host.write_persisted(handle, cx));
        }));
    }

    /// Write a persisted buffer's text if the record is behind it.
    fn write_persisted(&mut self, handle: BufferHandle, cx: &mut App) {
        let Some(state) = self.extension_buffers.get_mut(&handle) else {
            return;
        };
        let owner = state.owner;
        let model = state.model.clone();
        let Some(persisted) = state.persist.as_mut() else {
            return;
        };
        if let Some(write) = persisted.write.take() {
            write.detach();
        }
        let model = model.read(cx);
        if persisted.stored_revision == model.revision() {
            return;
        }
        persisted.stored_revision = model.revision();
        let record = persisted.record.clone();
        let text = model.text();
        self.write_record(owner, record, Some(text));
    }

    /// Write or, for no text, delete a record of the package's storage. Empty
    /// text is no text.
    fn write_record(&mut self, owner: ExtensionKey, record: String, text: Option<String>) {
        if self.storage_stopping {
            return;
        }
        let (Ok(storage), Some(storage_owner)) =
            (self.storage.as_ref(), self.storage_owners.get(&owner))
        else {
            return;
        };
        let records = storage.for_owner(storage_owner.clone());
        let task = match text.filter(|text| !text.is_empty()) {
            Some(text) => records.write(record, Value::String(text)),
            None => records.delete(record),
        };
        // Storage shutdown waits for work already admitted.
        task.detach();
    }

    /// Force every pending write out, before storage stops accepting work.
    pub(crate) fn flush_persisted_buffers(&mut self, cx: &mut App) {
        let handles = self.extension_buffers.keys().copied().collect::<Vec<_>>();
        for handle in handles {
            self.write_persisted(handle, cx);
        }
    }

    /// Close a buffer after writing its pending text.
    fn release_extension_buffer(&mut self, handle: BufferHandle, cx: &mut App) {
        self.write_persisted(handle, cx);
        self.extension_buffers.remove(&handle);
        self.buffer_observers.remove(&handle);
        let _ = self.buffers.close_buffer(handle, cx);
    }

    /// Close every buffer one lifecycle created.
    pub(super) fn release_extension_buffers_of(&mut self, key: ExtensionKey, cx: &mut App) {
        let owned = self
            .extension_buffers
            .iter()
            .filter(|(_, state)| state.owner == key)
            .map(|(handle, _)| *handle)
            .collect::<Vec<_>>();
        for handle in owned {
            self.release_extension_buffer(handle, cx);
        }
    }
}
