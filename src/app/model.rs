//! The gpui entity over a document's [`BufferModel`], and the registries
//! that name buffers to extensions.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use gpui::{AppContext, Context, Entity, Subscription, WeakEntity};
use knot_protocol::{BufferHandle, ExtensionId, ExtensionLifecycleId};

pub use knot_model::buffer::*;

use super::syntax::SyntaxState;

/// The entity observers watch for changes to a [`BufferModel`]. A caller that
/// commits through it notifies; views, recovery capture, and background
/// syntax parses all start from that notification.
pub struct Buffer {
    model: BufferModel,
    /// Starts a background parse after any notify that leaves the tree
    /// behind the text, so commits need no gpui context.
    syntax_driver: Option<Subscription>,
}

impl Buffer {
    pub fn new(model: BufferModel) -> Self {
        Self {
            model,
            syntax_driver: None,
        }
    }

    /// Parse this buffer with a fresh `syntax`, or stop parsing with `None`.
    pub(crate) fn set_syntax(&mut self, syntax: Option<SyntaxState>, cx: &mut Context<Self>) {
        self.model.set_syntax(syntax);
        if self.model.syntax_language().is_some() && self.syntax_driver.is_none() {
            self.syntax_driver = Some(cx.observe_self(Self::continue_syntax_parse));
        }
        cx.notify();
    }

    fn continue_syntax_parse(&mut self, cx: &mut Context<Self>) {
        let Some(parse) = self.model.start_syntax_parse() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let Some(parsed) = cx.background_spawn(async move { parse.run() }).await else {
                return Ok(());
            };
            this.update(cx, |buffer, cx| {
                if buffer.model.finish_syntax_parse(parsed) {
                    cx.notify();
                }
            })
        })
        .detach();
    }

    pub(crate) fn close(&mut self) {
        self.model.close();
        self.syntax_driver = None;
    }
}

impl Deref for Buffer {
    type Target = BufferModel;

    fn deref(&self) -> &BufferModel {
        &self.model
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut BufferModel {
        &mut self.model
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BufferClosed;

/// Maps transport-safe buffer handles to foreground-owned gpui buffer entities.
pub struct BufferRegistry {
    next_handle: u64,
    buffers: HashMap<BufferHandle, WeakEntity<Buffer>>,
}

impl BufferRegistry {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            buffers: HashMap::new(),
        }
    }

    pub fn open(&mut self, model: &Entity<Buffer>) -> BufferHandle {
        if let Some(handle) = self.handle_for(model) {
            return handle;
        }
        self.register(model.downgrade())
    }

    fn register(&mut self, model: WeakEntity<Buffer>) -> BufferHandle {
        let handle = BufferHandle::new(self.next_handle);
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .expect("buffer handle space exhausted");
        self.buffers.insert(handle, model);
        handle
    }

    pub(crate) fn handle_for(&self, model: &Entity<Buffer>) -> Option<BufferHandle> {
        self.buffers.iter().find_map(|(handle, candidate)| {
            (candidate.upgrade().as_ref() == Some(model)).then_some(*handle)
        })
    }

    pub fn resolve(
        &self,
        handle: BufferHandle,
        cx: &gpui::App,
    ) -> Result<Entity<Buffer>, BufferClosed> {
        self.buffers
            .get(&handle)
            .and_then(WeakEntity::upgrade)
            .filter(|model| model.read(cx).is_open())
            .ok_or(BufferClosed)
    }

    pub fn close<C: AppContext>(
        &mut self,
        handle: BufferHandle,
        cx: &mut C,
    ) -> Result<(), BufferClosed> {
        let model = self
            .buffers
            .get(&handle)
            .and_then(WeakEntity::upgrade)
            .ok_or(BufferClosed)?;
        model.update(cx, |model, cx| {
            if model.is_open() {
                model.close();
                cx.notify();
            }
        });
        self.invalidate(handle);
        Ok(())
    }

    pub(crate) fn remove_contribution_lifecycle<C: AppContext>(
        &self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        cx: &mut C,
    ) {
        for model in self.buffers.values().filter_map(WeakEntity::upgrade) {
            model.update(cx, |model, cx| {
                if model.remove_lifecycle_state(extension, lifecycle) {
                    cx.notify();
                }
            });
        }
    }

    fn invalidate(&mut self, handle: BufferHandle) {
        self.buffers.remove(&handle);
    }
}

impl Default for BufferRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
