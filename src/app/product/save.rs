//! Save, Save As, and save-conflict prompts for the window.

use super::*;

impl KnotWindow {
    pub(super) fn finish_save_command(
        &mut self,
        completion: CommandCompletion,
        outcome: knot_protocol::CommandOutcome,
        cx: &mut Context<Self>,
    ) {
        completion.complete(outcome.clone());
        Self::command_dispatcher(cx).update(cx, |dispatcher, cx| {
            dispatcher.record_outcome(outcome, cx);
        });
    }

    pub(super) fn target_is_live(&self, target: &ProductCommandTarget, cx: &App) -> bool {
        target.workbench.upgrade() == Some(self.workbench.clone()) && target.validate(cx).is_ok()
    }

    pub(crate) fn start_save_command(
        &mut self,
        save_as: bool,
        target: ProductCommandTarget,
        completion: CommandCompletion,
        cx: &mut Context<Self>,
    ) {
        if !self.target_is_live(&target, cx) {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        }
        let documents = Self::documents(cx);
        let Some(document_id) = target_document(&target) else {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::Unavailable, cx);
            return;
        };
        let Some(document) = documents.read(cx).get(document_id) else {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        };

        if save_as || matches!(document.state(), DocumentState::Untitled { .. }) {
            self.start_save_dialog(target, completion, cx);
        } else {
            let uri = document.resource_uri().unwrap().clone();
            self.start_save_to_uri(target, uri, false, completion, cx);
        }
    }

    pub(super) fn start_save_dialog(
        &mut self,
        target: ProductCommandTarget,
        completion: CommandCompletion,
        cx: &mut Context<Self>,
    ) {
        let suggested_name = Self::documents(cx)
            .read(cx)
            .get(target_document(&target).expect("save target is a document"))
            .map(|document| document.title().to_owned())
            .unwrap_or_else(|| "Untitled".into());
        let selection = if let Some(dialog) = cx.try_global::<ApplicationSaveDialog>() {
            dialog.0.select(suggested_name)
        } else {
            let directory =
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
            Box::pin(async move {
                match selection.await {
                    Ok(Ok(Some(path))) => match OpenRequest::from_path(&path, Path::new("/")) {
                        Ok(request) => SaveDialogOutcome::Selected(request.uri().clone()),
                        Err(message) => SaveDialogOutcome::Failed(message),
                    },
                    Ok(Ok(None)) => SaveDialogOutcome::Cancelled,
                    Ok(Err(error)) => SaveDialogOutcome::Failed(error.to_string()),
                    Err(error) => SaveDialogOutcome::Failed(error.to_string()),
                }
            }) as SaveDialogFuture
        };
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            let outcome = selection.await;
            let fallback = completion.clone();
            if this
                .update(cx, |this, cx| match outcome {
                    SaveDialogOutcome::Selected(uri) => {
                        this.start_save_to_uri(target, uri, true, completion, cx);
                    }
                    SaveDialogOutcome::Cancelled => {
                        this.finish_save_command(
                            completion,
                            knot_protocol::CommandOutcome::Cancelled,
                            cx,
                        );
                        cx.notify();
                    }
                    SaveDialogOutcome::Failed(message) => {
                        this.show_notice(
                            PromptLevel::Warning,
                            "Save failed",
                            Some(message.clone()),
                            cx,
                        );
                        this.finish_save_command(
                            completion,
                            knot_protocol::CommandOutcome::HandlerFailure { message },
                            cx,
                        );
                        cx.notify();
                    }
                })
                .is_err()
            {
                fallback.complete(knot_protocol::CommandOutcome::InvalidTarget);
            }
        });
        self.tasks.push(task);
    }

    pub(super) fn start_save_to_uri(
        &mut self,
        target: ProductCommandTarget,
        uri: crate::app::resource::ResourceUri,
        explicit_overwrite: bool,
        completion: CommandCompletion,
        cx: &mut Context<Self>,
    ) {
        if !self.target_is_live(&target, cx) {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        }
        let documents = Self::documents(cx);
        let (model, title, state) = {
            let documents = documents.read(cx);
            let Some(document) =
                documents.get(target_document(&target).expect("save target is a document"))
            else {
                self.finish_save_command(
                    completion,
                    knot_protocol::CommandOutcome::InvalidTarget,
                    cx,
                );
                return;
            };
            (
                document.model().clone(),
                resource_title(&uri),
                document.state().clone(),
            )
        };
        if documents
            .read(cx)
            .document_for_resource(&uri)
            .is_some_and(|document| Some(document) != target_document(&target))
        {
            let message = format!("another open document already owns {uri}");
            self.show_notice(
                PromptLevel::Warning,
                "Save failed",
                Some(message.clone()),
                cx,
            );
            self.finish_save_command(
                completion,
                knot_protocol::CommandOutcome::HandlerFailure { message },
                cx,
            );
            cx.notify();
            return;
        }
        // The cleanup is an ordinary edit of the target's editor, so what is
        // captured is what the editor shows and Undo takes it back.
        if let Some(crate::app::workbench::CommandView::Editor(view)) = target.command_view(cx) {
            let editor = view.read(cx).editor().clone();
            if editor.read(cx).model() == &model {
                editor.update(cx, |editor, cx| editor.clean_up_for_save(cx));
            }
        }
        let (text, revision) = model.read_with(cx, |model, _| (model.text(), model.revision()));
        let Some(capture) = documents.update(cx, |documents, _| {
            documents.begin_persistence(
                target_document(&target).expect("save target is a document"),
                &model,
            )
        }) else {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        };
        cx.notify();
        let providers = Self::filesystems(cx);
        let task = cx.spawn(async move |this, cx| {
            let result: Result<
                (crate::app::resource::ResourceUri, ResourceVersion),
                ResourceError,
            > = async {
                let uri = providers.normalize(uri).await?;
                let provider = providers.provider(&uri)?;
                let version = if explicit_overwrite {
                    match provider.stat(uri.clone()).await? {
                        ResourceStat::Missing => {
                            provider.create(uri.clone(), text.into_bytes()).await?
                        }
                        ResourceStat::File => {
                            let existing = provider.read(uri.clone()).await?;
                            provider
                                .replace(uri.clone(), existing.version, text.into_bytes())
                                .await?
                        }
                        ResourceStat::Directory => {
                            return Err(ResourceError::WrongKind {
                                uri,
                                expected: ResourceKind::File,
                                actual: ResourceKind::Directory,
                            });
                        }
                    }
                } else {
                    match state {
                        DocumentState::Destination { .. } => {
                            provider.create(uri.clone(), text.into_bytes()).await?
                        }
                        DocumentState::Persisted { version, .. } => {
                            provider
                                .replace(uri.clone(), version, text.into_bytes())
                                .await?
                        }
                        DocumentState::Untitled { .. } => {
                            return Err(ResourceError::InvalidUri {
                                uri: uri.to_string(),
                                reason: "document has no direct save destination".into(),
                            });
                        }
                    }
                };
                Ok((uri, version))
            }
            .await;
            let fallback = completion.clone();
            if this
                .update(cx, |this, cx| match result {
                    Ok((uri, version)) if this.target_is_live(&target, cx) => {
                        let committed = Self::documents(cx).update(cx, |documents, cx| {
                            documents.finish_persistence(
                                target_document(&target).expect("save target is a document"),
                                &model,
                                &capture,
                                (title, uri.clone()),
                                (version, revision),
                                cx,
                            )
                        });
                        let outcome = if committed {
                            knot_protocol::CommandOutcome::Completed
                        } else {
                            knot_protocol::CommandOutcome::InvalidTarget
                        };
                        this.finish_save_command(completion, outcome, cx);
                        cx.notify();
                    }
                    Ok(_) => {
                        this.finish_save_command(
                            completion,
                            knot_protocol::CommandOutcome::InvalidTarget,
                            cx,
                        );
                    }
                    Err(ResourceError::Conflict { uri }) => {
                        this.prompt_for_save_conflict(target, uri, completion, cx);
                    }
                    Err(ResourceError::AlreadyExists { uri }) => {
                        this.prompt_for_save_conflict(target, uri, completion, cx);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        this.show_notice(
                            PromptLevel::Warning,
                            "Save failed",
                            Some(message.clone()),
                            cx,
                        );
                        this.finish_save_command(
                            completion,
                            knot_protocol::CommandOutcome::HandlerFailure { message },
                            cx,
                        );
                        cx.notify();
                    }
                })
                .is_err()
            {
                fallback.complete(knot_protocol::CommandOutcome::InvalidTarget);
            }
        });
        self.tasks.push(task);
    }

    pub(super) fn prompt_for_save_conflict(
        &mut self,
        target: ProductCommandTarget,
        uri: crate::app::resource::ResourceUri,
        completion: CommandCompletion,
        cx: &mut Context<Self>,
    ) {
        if !self.target_is_live(&target, cx) {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        }
        let prompt = self.prompt(
            PromptLevel::Warning,
            "The file changed outside Knot",
            Some(
                "Reload the external version, choose another destination, or cancel the save."
                    .into(),
            ),
            vec![
                PromptButton::new("Reload"),
                PromptButton::new("Save As…"),
                PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            let answer = prompt.await.unwrap_or(2);
            let fallback = completion.clone();
            if this
                .update(cx, |this, cx| match answer {
                    0 => this.start_conflict_reload(target, uri, completion, cx),
                    1 => this.start_save_dialog(target, completion, cx),
                    _ => {
                        this.finish_save_command(
                            completion,
                            knot_protocol::CommandOutcome::Cancelled,
                            cx,
                        );
                        cx.notify();
                    }
                })
                .is_err()
            {
                fallback.complete(knot_protocol::CommandOutcome::InvalidTarget);
            }
        });
        self.tasks.push(task);
    }

    pub(super) fn start_conflict_reload(
        &mut self,
        target: ProductCommandTarget,
        uri: crate::app::resource::ResourceUri,
        completion: CommandCompletion,
        cx: &mut Context<Self>,
    ) {
        if !self.target_is_live(&target, cx) {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        }
        let documents = Self::documents(cx);
        let Some((model, title)) = documents
            .read(cx)
            .get(target_document(&target).expect("save target is a document"))
            .map(|document| (document.model().clone(), document.title().to_owned()))
        else {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        };
        let revision = model.read(cx).revision();
        let Some(capture) = documents.update(cx, |documents, _| {
            documents.begin_persistence(
                target_document(&target).expect("save target is a document"),
                &model,
            )
        }) else {
            self.finish_save_command(completion, knot_protocol::CommandOutcome::InvalidTarget, cx);
            return;
        };
        cx.notify();
        let providers = Self::filesystems(cx);
        let task = cx.spawn(async move |this, cx| {
            let result = load_resource(providers, uri.clone()).await;
            let fallback = completion.clone();
            if this
                .update(cx, |this, cx| {
                    let outcome = match result {
                        Ok(OpenResource::File { uri, text, version })
                            if this.target_is_live(&target, cx)
                                && model.read(cx).revision() == revision =>
                        {
                            if Self::documents(cx).update(cx, |documents, cx| {
                                documents.finish_reload(
                                    target_document(&target).expect("save target is a document"),
                                    &model,
                                    &capture,
                                    &text,
                                    (title, uri.clone()),
                                    version,
                                    cx,
                                )
                            }) {
                                knot_protocol::CommandOutcome::Completed
                            } else {
                                knot_protocol::CommandOutcome::InvalidTarget
                            }
                        }
                        Ok(_) => knot_protocol::CommandOutcome::InvalidTarget,
                        Err(error) => knot_protocol::CommandOutcome::HandlerFailure {
                            message: error.to_string(),
                        },
                    };
                    this.finish_save_command(completion, outcome, cx);
                    cx.notify();
                })
                .is_err()
            {
                fallback.complete(knot_protocol::CommandOutcome::InvalidTarget);
            }
        });
        self.tasks.push(task);
    }
}
