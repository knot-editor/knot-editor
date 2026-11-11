//! Native commands that edit the window's ordered workspace roots.
//!
//! Every edit is a replacement of the whole root set computed from the live
//! state on the foreground, so retained roots keep their identity.

use super::*;
use crate::app::resource::ResourceUri;

impl ProductShell {
    /// Choose a directory in the session's transient slot and append it.
    pub(super) fn start_add_folder(&mut self, cx: &mut Context<Self>) {
        let Ok(receiver) = self.begin_folder_picker(None, cx) else {
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(TransientOutcome::Folder(path)) => path,
                Ok(TransientOutcome::Failed(message)) => {
                    Self::notice(
                        &this,
                        PromptLevel::Warning,
                        "Cannot add folder",
                        Some(message),
                        cx,
                    );
                    return;
                }
                _ => return,
            };
            let (sender, resolved) = tokio::sync::oneshot::channel();
            cx.update(|cx| {
                operations::resolve_directory(
                    path,
                    move |result| {
                        let _ = sender.send(result);
                    },
                    cx,
                )
            });
            let Ok(result) = resolved.await else {
                return;
            };
            let _ = this.update(cx, |shell, cx| match result {
                Ok(uri) => shell.add_workspace_root(uri, cx),
                Err(error) => {
                    use knot_protocol::declarations::HostError;
                    shell.show_notice(
                        PromptLevel::Warning,
                        "Cannot add folder",
                        Some(error.code().to_string()),
                        cx,
                    );
                }
            });
        });
        self.tasks.push(task);
    }

    pub(super) fn add_workspace_root(&mut self, uri: ResourceUri, cx: &mut Context<Self>) {
        let mut roots = self.root_uris();
        if roots.contains(&uri) {
            cx.notify();
            return;
        }
        roots.push(uri);
        self.replace_workspace_roots(roots, cx);
    }

    pub(crate) fn root_uris(&self) -> Vec<ResourceUri> {
        self.workspace
            .snapshot()
            .roots()
            .iter()
            .map(|root| root.uri.clone())
            .collect()
    }
}
