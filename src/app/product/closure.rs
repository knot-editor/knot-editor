//! Protected closure planning, approval, and commit for tabs, a pane's tabs,
//! windows, and quit.

use super::*;
use crate::app::documents::DocumentDirty;
use knot_model::workbench::closure as model;

impl ProductCommandTarget {
    fn view_key(&self) -> ProtectedViewKey {
        ProtectedViewKey {
            session: self.shell.entity_id().as_u64(),
            workbench: self.workbench.entity_id().as_u64(),
            presentation: self.presentation,
            pane: self.pane,
            tab: self.tab,
            surface: self.surface,
        }
    }
}

fn closure_views(views: &[ProtectedView]) -> Vec<model::ClosureView> {
    views
        .iter()
        .map(|view| model::ClosureView {
            key: view.key,
            pinned: view.pinned,
            dock: view.dock,
        })
        .collect()
}

/// Every open document's state for the closure decisions.
fn document_statuses(cx: &App) -> Vec<model::DocumentStatus> {
    ProductShell::documents(cx)
        .read(cx)
        .documents()
        .map(|document| model::DocumentStatus {
            id: document.id(),
            revision: document.model().read(cx).revision(),
            dirty: document.is_dirty(cx),
        })
        .collect()
}

impl ProductShell {
    pub(super) fn protected_closure_is_active(cx: &App) -> bool {
        *cx.global::<ApplicationProtectedClosure>().0.borrow()
    }

    pub(super) fn set_protected_closure_active(active: bool, cx: &App) {
        *cx.global::<ApplicationProtectedClosure>().0.borrow_mut() = active;
    }

    pub(super) fn collect_product_views(cx: &mut App) -> Vec<ProtectedView> {
        let mut views = Vec::new();
        for shell in cx.global::<ApplicationSessions>().shells() {
            let session = shell.entity_id();
            let workbench = shell.read(cx).workbench.clone();
            let snapshots = workbench.read(cx).tab_snapshots();
            views.extend(snapshots.into_iter().map(|snapshot| {
                let (has_editor, pinned, presentation) = {
                    let workbench = workbench.read(cx);
                    let tab = workbench
                        .tab(snapshot.pane_id, snapshot.tab_id)
                        .expect("snapshots name live tabs");
                    (
                        workbench.editor(tab).is_some(),
                        tab.is_pinned(),
                        tab.presentation(),
                    )
                };
                ProtectedView {
                    key: ProtectedViewKey {
                        session: session.as_u64(),
                        workbench: workbench.entity_id().as_u64(),
                        presentation,
                        pane: snapshot.pane_id,
                        tab: snapshot.tab_id,
                        surface: snapshot.surface,
                    },
                    pinned,
                    dock: workbench.read(cx).dock_location(snapshot.pane_id).is_some(),
                    target: has_editor.then(|| ProductCommandTarget {
                        shell: shell.downgrade(),
                        workbench: workbench.downgrade(),
                        pane: snapshot.pane_id,
                        tab: snapshot.tab_id,
                        surface: snapshot.surface,
                        presentation,
                        focused: true,
                        view: None,
                    }),
                }
            }));
        }
        views
    }

    pub(super) fn build_protected_close_plan(
        request: ProtectedCloseRequest,
        cx: &mut App,
    ) -> Result<ProtectedClosePlan, ProtectedCloseFailure> {
        request
            .origin
            .validate_tab(cx)
            .map_err(|_| ProtectedCloseFailure::InvalidTarget)?;
        let views = Self::collect_product_views(cx);
        let origin = request.origin.view_key();
        let decided = model::plan(
            request.kind,
            &origin,
            request.discard,
            &closure_views(&views),
            &document_statuses(cx),
        )
        .map_err(|_| ProtectedCloseFailure::InvalidTarget)?;
        let prompts = decided
            .prompts
            .into_iter()
            .map(|(document, key)| {
                let view = views.iter().find(|view| view.key == key);
                (
                    document,
                    view.and_then(|view| view.target.clone())
                        .expect("document tab has editor focus"),
                )
            })
            .collect();
        Ok(ProtectedClosePlan {
            request,
            scope: decided.scope,
            prompts,
            discarded: decided.discarded,
            hot_exit: false,
        })
    }

    #[cfg(test)]
    pub(super) fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((pane, tab, document)) = self.workbench.read(cx).focused_pane().and_then(|pane| {
            Some((
                pane.id(),
                pane.active_tab_id(),
                pane.active_tab().document_id()?,
            ))
        }) else {
            return;
        };
        self.close_tab(pane, tab, document, window, cx);
    }

    pub(crate) fn begin_protected_close(
        kind: ProtectedCloseKind,
        origin: ProductCommandTarget,
        completion: Option<CommandCompletion>,
        cx: &mut App,
    ) {
        Self::begin_protected_operation(
            ProtectedCloseRequest {
                kind,
                origin,
                caller: None,
                discard: false,
            },
            Box::new(move |result, cx| {
                use knot_protocol::{CommandOutcome, PresentationCloseOutcome};
                let outcome = match result {
                    Ok(PresentationCloseOutcome::Closed) => CommandOutcome::Completed,
                    Ok(PresentationCloseOutcome::Cancelled) => CommandOutcome::Cancelled,
                    Err(error) => error.command_outcome(),
                };
                Self::finish_protected_close_completion(completion, outcome, cx);
            }),
            cx,
        );
    }

    /// Close a tab chosen by pointer, such as a middle-clicked tab, whether or
    /// not it is active. Documents and terminals go through protected closure;
    /// a tool tab hides like its hide button.
    pub(super) fn close_tab_from_pointer(
        &mut self,
        pane: PaneId,
        tab: TabId,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self
            .workbench
            .read(cx)
            .pane(pane)
            .and_then(|pane| pane.tabs().iter().find(|candidate| candidate.id() == tab))
        else {
            return;
        };
        if let TabSurfaceId::Tool(id) = tab.surface_id() {
            self.hide_tool_from_pointer(id, cx);
            return;
        }
        if self.workbench.read(cx).command_view(tab).is_none() {
            return;
        }
        let target = ProductCommandTarget {
            shell: cx.entity().downgrade(),
            workbench: self.workbench.downgrade(),
            pane,
            tab: tab.id(),
            surface: tab.surface_id(),
            presentation: tab.presentation(),
            focused: true,
            view: None,
        };
        // Protected closure updates this shell, so it starts outside the
        // current update.
        cx.defer(move |cx| {
            super::super::commands::dispatch::break_captured_history_group(&target, cx);
            Self::begin_protected_close(ProtectedCloseKind::Tab, target, None, cx);
        });
    }

    pub(super) fn hide_tool_from_pointer(
        &mut self,
        id: crate::app::workbench::ToolFrameId,
        cx: &mut Context<Self>,
    ) {
        self.workbench.update(cx, |workbench, _| {
            workbench.hide_tool(id);
        });
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    pub(super) fn begin_protected_operation(
        request: ProtectedCloseRequest,
        completion: ProtectedCloseCompletion,
        cx: &mut App,
    ) {
        if Self::protected_closure_is_active(cx) {
            completion(Err(ProtectedCloseFailure::InProgress), cx);
            return;
        }
        let kind = request.kind;
        let mut plan = match Self::build_protected_close_plan(request, cx) {
            Ok(plan) => plan,
            Err(error) => {
                completion(Err(error), cx);
                return;
            }
        };
        let Some(shell) = plan.request.origin.shell.upgrade() else {
            completion(Err(ProtectedCloseFailure::InvalidTarget), cx);
            return;
        };
        Self::set_protected_closure_active(true, cx);
        if kind != ProtectedCloseKind::Quit || plan.prompts.is_empty() {
            shell.update(cx, |shell, cx| {
                shell.start_protected_close_plan(plan, completion, cx)
            });
            return;
        }
        // Quit is a hot exit when recovery commits every dirty document; a
        // failing store falls back to the ordinary prompts.
        let protected = crate::app::recovery::protect_for_exit(cx);
        cx.spawn(async move |cx| {
            if protected.await {
                plan.prompts.clear();
                plan.hot_exit = true;
            }
            cx.update(|cx| {
                shell.update(cx, |shell, cx| {
                    shell.start_protected_close_plan(plan, completion, cx)
                })
            });
        })
        .detach();
    }

    pub(super) fn start_protected_close_plan(
        &mut self,
        plan: ProtectedClosePlan,
        completion: ProtectedCloseCompletion,
        cx: &mut Context<Self>,
    ) {
        use knot_protocol::{CommandOutcome, PresentationCloseOutcome};

        cx.spawn(async move |this, cx| {
            let mut approvals = plan.discarded.clone();
            let mut outcome = Ok(PresentationCloseOutcome::Closed);
            for (document, target) in &plan.prompts {
                let prompt = cx.update(|cx| {
                    if plan
                        .request
                        .caller
                        .as_ref()
                        .is_some_and(|caller| !caller.is_live(cx))
                    {
                        return Err(ProtectedCloseFailure::Cancelled);
                    }
                    let documents = Self::documents(cx);
                    let Some(document_state) = documents.read(cx).get(*document) else {
                        return Err(ProtectedCloseFailure::InvalidTarget);
                    };
                    if !document_state.is_dirty(cx) {
                        return Ok(None);
                    }
                    let title = document_state.title().to_owned();
                    let prompted_revision = document_state.model().read(cx).revision();
                    target
                        .session(cx)
                        .ok_or(ProtectedCloseFailure::InvalidTarget)?;
                    let prompt = super::prompt(
                        target.window(cx),
                        PromptLevel::Warning,
                        format!("Do you want to save the changes to “{title}”?"),
                        Some("Your changes will be lost if you don't save them.".into()),
                        vec![
                            PromptButton::new("Save"),
                            PromptButton::new("Don't Save"),
                            PromptButton::cancel("Cancel"),
                        ],
                        cx,
                    );
                    Ok(Some((prompt, prompted_revision)))
                });
                let (prompt, prompted_revision) = match prompt {
                    Ok(Some(prompt)) => prompt,
                    Ok(None) => continue,
                    Err(error) => {
                        outcome = Err(error);
                        break;
                    }
                };
                match prompt.await.unwrap_or(2) {
                    0 => {
                        let save = cx.update(|cx| {
                            if plan
                                .request
                                .caller
                                .as_ref()
                                .is_some_and(|caller| !caller.is_live(cx))
                            {
                                return Err(ProtectedCloseFailure::Cancelled);
                            }
                            let Some(shell) = target.session(cx) else {
                                return Err(ProtectedCloseFailure::InvalidTarget);
                            };
                            let (save_completion, receiver) = CommandCompletion::new();
                            shell.update(cx, |shell, cx| {
                                shell.start_save_command(
                                    false,
                                    target.clone(),
                                    save_completion,
                                    cx,
                                );
                            });
                            Ok(receiver)
                        });
                        let save_outcome = match save {
                            Ok(receiver) => receiver.await.unwrap_or(CommandOutcome::Cancelled),
                            Err(error) => {
                                outcome = Err(error);
                                break;
                            }
                        };
                        if save_outcome != CommandOutcome::Completed {
                            outcome = match save_outcome {
                                CommandOutcome::Cancelled => {
                                    Ok(PresentationCloseOutcome::Cancelled)
                                }
                                CommandOutcome::InvalidTarget => {
                                    Err(ProtectedCloseFailure::InvalidTarget)
                                }
                                CommandOutcome::HandlerFailure { message } => {
                                    Err(ProtectedCloseFailure::PersistenceFailed { message })
                                }
                                _ => Err(ProtectedCloseFailure::PersistenceFailed {
                                    message: "save is unavailable".into(),
                                }),
                            };
                            break;
                        }
                        let approval = cx.update(|cx| {
                            let documents = Self::documents(cx);
                            let Some(document_state) = documents.read(cx).get(*document) else {
                                return Err(ProtectedCloseFailure::InvalidTarget);
                            };
                            if document_state.is_dirty(cx) {
                                return Err(ProtectedCloseFailure::PersistenceFailed {
                                    message: "document changed while it was being saved".into(),
                                });
                            }
                            Ok(CloseApproval {
                                document: *document,
                                revision: document_state.model().read(cx).revision(),
                                kind: CloseApprovalKind::Saved,
                            })
                        });
                        match approval {
                            Ok(approval) => approvals.push(approval),
                            Err(error) => {
                                outcome = Err(error);
                                break;
                            }
                        }
                    }
                    1 => {
                        let approval = cx.update(|cx| {
                            let documents = Self::documents(cx);
                            let Some(document_state) = documents.read(cx).get(*document) else {
                                return Err(ProtectedCloseFailure::InvalidTarget);
                            };
                            if document_state.model().read(cx).revision() != prompted_revision {
                                return Err(ProtectedCloseFailure::InvalidTarget);
                            }
                            Ok(CloseApproval {
                                document: *document,
                                revision: prompted_revision,
                                kind: CloseApprovalKind::Discarded,
                            })
                        });
                        match approval {
                            Ok(approval) => approvals.push(approval),
                            Err(error) => {
                                outcome = Err(error);
                                break;
                            }
                        }
                    }
                    _ => {
                        outcome = Ok(PresentationCloseOutcome::Cancelled);
                        break;
                    }
                }
            }
            let current_views = if matches!(outcome, Ok(PresentationCloseOutcome::Closed)) {
                Some(cx.update(Self::collect_product_views))
            } else {
                None
            };
            let mut completion = Some(completion);
            if this
                .update(cx, |this, cx| {
                    if plan
                        .request
                        .caller
                        .as_ref()
                        .is_some_and(|caller| !caller.is_live(cx))
                    {
                        outcome = Err(ProtectedCloseFailure::Cancelled);
                    } else if matches!(outcome, Ok(PresentationCloseOutcome::Closed)) {
                        outcome = match current_views {
                            Some(ref views) => this
                                .commit_protected_close(&plan, &approvals, views, cx)
                                .map(|()| PresentationCloseOutcome::Closed),
                            None => Err(ProtectedCloseFailure::InvalidTarget),
                        };
                    }
                    Self::set_protected_closure_active(false, cx);
                    completion.take().unwrap()(outcome, cx);
                })
                .is_err()
            {
                cx.update(|cx| {
                    Self::set_protected_closure_active(false, cx);
                    completion.take().unwrap()(Err(ProtectedCloseFailure::InvalidTarget), cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn finish_protected_close_completion(
        completion: Option<CommandCompletion>,
        outcome: knot_protocol::CommandOutcome,
        cx: &mut App,
    ) {
        let Some(completion) = completion else {
            return;
        };
        completion.complete(outcome.clone());
        Self::command_dispatcher(cx).update(cx, |dispatcher, cx| {
            dispatcher.record_outcome(outcome, cx);
        });
    }

    pub(super) fn commit_protected_close(
        &mut self,
        plan: &ProtectedClosePlan,
        approvals: &[CloseApproval],
        views: &[ProtectedView],
        cx: &mut Context<Self>,
    ) -> Result<(), ProtectedCloseFailure> {
        let model::Commit {
            scope: current_scope,
            discarded,
            final_documents,
        } = model::revalidate(
            plan.request.kind,
            &plan.request.origin.view_key(),
            &plan.scope,
            plan.hot_exit,
            approvals,
            &closure_views(views),
            &document_statuses(cx),
        )
        .map_err(|_| ProtectedCloseFailure::InvalidTarget)?;
        let documents = Self::documents(cx);
        // A discarded document retires its automatic protection even when the
        // quit transition keeps the document entity alive until process exit.
        let discarded: Vec<uuid::Uuid> = discarded
            .into_iter()
            .filter_map(|id| {
                documents
                    .read(cx)
                    .get(id)
                    .map(|document| document.identity())
            })
            .collect();
        for identity in discarded {
            crate::app::recovery::discard(identity, cx);
        }

        match plan.request.kind {
            ProtectedCloseKind::Tab => self.close_tab_after_approval(
                plan.request.origin.pane,
                plan.request.origin.tab,
                plan.request.origin.surface,
                cx,
            ),
            ProtectedCloseKind::OtherTabs
            | ProtectedCloseKind::Pane
            | ProtectedCloseKind::OtherPanes => {
                for key in &current_scope {
                    self.close_tab_after_approval(key.pane, key.tab, key.surface, cx)?;
                }
                if current_scope
                    .iter()
                    .any(|key| matches!(key.surface, TabSurfaceId::Tool(_)))
                {
                    self.publish_tool_layout(cx);
                }
                Ok(())
            }
            ProtectedCloseKind::Window => match self.close_window(cx) {
                Ok(()) => {
                    documents.update(cx, |documents, cx| {
                        for document in final_documents {
                            documents.remove(document, cx);
                        }
                    });
                    Self::close_terminal_ids(
                        current_scope.iter().filter_map(|key| match key.surface {
                            TabSurfaceId::Terminal(id) => Some(id),
                            TabSurfaceId::Document(_) | TabSurfaceId::Tool(_) => None,
                        }),
                        cx,
                    );
                    Ok(())
                }
                Err(_) => Err(ProtectedCloseFailure::InvalidTarget),
            },
            ProtectedCloseKind::Quit => {
                Self::close_terminal_ids(
                    current_scope.iter().filter_map(|key| match key.surface {
                        TabSurfaceId::Terminal(id) => Some(id),
                        TabSurfaceId::Document(_) | TabSurfaceId::Tool(_) => None,
                    }),
                    cx,
                );
                crate::app::extensions::host::quit_after_storage(cx);
                Ok(())
            }
        }
    }

    #[cfg(test)]
    pub(super) fn close_tab(
        &mut self,
        pane: PaneId,
        tab: TabId,
        document: DocumentId,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> knot_protocol::CommandOutcome {
        use knot_protocol::CommandOutcome;

        if !self.workbench.read(cx).contains_tab(pane, tab, document) {
            return CommandOutcome::InvalidTarget;
        }
        let open_view_count = cx.global::<ApplicationSessions>().view_count(document, cx);
        let documents = Self::documents(cx);
        let document_is_dirty = documents.read(cx).get(document).unwrap().is_dirty(cx);
        let outcome = self.workbench.update(cx, |workbench, _cx| {
            workbench.request_close_tab(pane, tab, document, document_is_dirty, open_view_count)
        });
        let Some(outcome) = outcome else {
            return CommandOutcome::InvalidTarget;
        };
        let transition = match outcome {
            CloseRequestOutcome::Pending(_) => {
                cx.notify();
                return CommandOutcome::Unavailable;
            }
            CloseRequestOutcome::Closed(transition) => transition,
        };

        if transition.document == Some(DocumentCloseDisposition::CloseRequested) {
            documents.update(cx, |documents, cx| {
                documents.remove(document, cx);
            });
        }
        if transition.workbench_is_empty() {
            let replacement = create_untitled_document(&documents, cx);
            let model = documents.read(cx).get(replacement).unwrap().model().clone();
            let workbench = cx.new(|cx| Workbench::new_for_document(replacement, model, cx));
            cx.global::<ApplicationSessions>()
                .register(cx.weak_entity(), &workbench);
            self.workbench = workbench;
        }
        self.focus_active_editor(cx);
        cx.notify();
        CommandOutcome::Completed
    }

    pub(super) fn close_tab_after_approval(
        &mut self,
        pane: PaneId,
        tab: TabId,
        surface: TabSurfaceId,
        cx: &mut Context<Self>,
    ) -> Result<(), ProtectedCloseFailure> {
        use crate::app::workbench::CloseConfirmation;

        if !self.workbench.read(cx).contains_surface(pane, tab, surface) {
            return Err(ProtectedCloseFailure::InvalidTarget);
        }
        self.remember_closed_tab(pane, tab, cx);
        if let TabSurfaceId::Tool(id) = surface {
            self.workbench.update(cx, |workbench, _| {
                workbench.hide_tool(id);
            });
            self.focus_active_surface(cx);
            cx.notify();
            return Ok(());
        }
        let TabSurfaceId::Document(document) = surface else {
            let TabSurfaceId::Terminal(session_id) = surface else {
                unreachable!()
            };
            let outcome = self.workbench.update(cx, |workbench, _| {
                workbench.request_close_terminal_tab(pane, tab, session_id)
            });
            let Some(transition) = outcome else {
                return Err(ProtectedCloseFailure::InvalidTarget);
            };
            self.close_terminal(session_id, cx);
            if transition.workbench_is_empty() {
                self.replace_empty_workbench(cx.weak_entity(), cx);
            }
            self.focus_active_surface(cx);
            cx.notify();
            return Ok(());
        };
        let open_view_count = cx.global::<ApplicationSessions>().view_count(document, cx);
        let documents = Self::documents(cx);
        let document_is_dirty = documents
            .read(cx)
            .get(document)
            .is_some_and(|document| document.is_dirty(cx));
        let outcome = self.workbench.update(cx, |workbench, _| {
            workbench.request_close_tab(pane, tab, document, document_is_dirty, open_view_count)
        });
        let transition = match outcome {
            Some(CloseRequestOutcome::Closed(transition)) => transition,
            Some(CloseRequestOutcome::Pending(pending)) => self
                .workbench
                .update(cx, |workbench, _| {
                    workbench.resolve_pending_close(
                        pending,
                        CloseConfirmation::Close,
                        open_view_count,
                    )
                })
                .unwrap(),
            None => return Err(ProtectedCloseFailure::InvalidTarget),
        };
        if transition.document == Some(DocumentCloseDisposition::CloseRequested) {
            documents.update(cx, |documents, cx| {
                documents.remove(document, cx);
            });
        }
        if transition.workbench_is_empty() {
            self.replace_empty_workbench(cx.weak_entity(), cx);
        }
        self.focus_active_surface(cx);
        cx.notify();
        Ok(())
    }
}

pub(super) fn install_protected_window_close(
    shell: &Entity<ProductShell>,
    window: &Window,
    cx: &App,
) {
    let shell = shell.downgrade();
    window.on_window_should_close(cx, move |_, cx| {
        let Some(shell) = shell.upgrade() else {
            return true;
        };
        if shell.read(cx).workbench.read(cx).panes().is_empty() {
            return true;
        }
        let target = shell.update(cx, |shell, cx| shell.capture_command_target(cx));
        if let Some(target) = target {
            cx.defer(move |cx| {
                ProductShell::begin_protected_close(ProtectedCloseKind::Window, target, None, cx);
            });
        }
        false
    });
}
