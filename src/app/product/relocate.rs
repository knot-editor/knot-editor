//! Foreground coordination of exact tab and destination identities.

use super::*;
use crate::app::workbench::{TabMoveCapture, TabMoveDestination};
use knot_protocol::CommandOutcome;

#[derive(Clone)]
pub(super) struct TabRelocationSource {
    pub(super) workbench: WeakEntity<Workbench>,
    pub(super) tab: TabMoveCapture,
}

#[derive(Clone)]
pub(super) struct TabRelocationDestination {
    pub(super) shell: WeakEntity<ProductShell>,
    pub(super) workbench: WeakEntity<Workbench>,
    pub(super) tab: TabMoveDestination,
}

impl ProductShell {
    pub(super) fn capture_tab_relocation(
        &self,
        pane: PaneId,
        tab: TabId,
        cx: &App,
    ) -> Option<TabRelocationSource> {
        Some(TabRelocationSource {
            workbench: self.workbench.downgrade(),
            tab: self.workbench.read(cx).capture_tab_move(pane, tab)?,
        })
    }

    fn relocation_is_live(&self, source: &TabRelocationSource, cx: &App) -> bool {
        if source.workbench.upgrade() != Some(self.workbench.clone())
            || !self.workbench.read(cx).contains_tab_move(source.tab)
        {
            return false;
        }
        match source.tab.tab.surface {
            TabSurfaceId::Tool(_) => true,
            TabSurfaceId::Document(document) => {
                Self::documents(cx).read(cx).get(document).is_some()
            }
            TabSurfaceId::Terminal(id) => {
                let session = cx
                    .global::<ApplicationTerminalSessions>()
                    .0
                    .borrow()
                    .sessions
                    .get(&id)
                    .cloned();
                let workbench = self.workbench.read(cx);
                let view = workbench
                    .tab(source.tab.tab.pane_id, source.tab.tab.tab_id)
                    .and_then(|tab| workbench.terminal_view(tab));
                session
                    .zip(view)
                    .is_some_and(|(session, view)| view.read(cx).is_attached_to(&session))
            }
        }
    }

    fn focus_relocated_surface(&mut self, cx: &mut Context<Self>) {
        let editor = self.workbench.read(cx).focused_editor().cloned();
        if let Some(editor) = editor {
            editor.read(cx).editor().clone().update(cx, |editor, cx| {
                editor.preserve_scroll_for_relocation();
                cx.notify();
            });
        }
        self.focus_active_surface(cx);
    }

    pub(super) fn relocate_tab_to(
        &mut self,
        source: &TabRelocationSource,
        destination: &TabRelocationDestination,
        cx: &mut Context<Self>,
    ) -> CommandOutcome {
        if !self.relocation_is_live(source, cx) {
            return CommandOutcome::InvalidTarget;
        }
        let this = cx.weak_entity();
        if destination.shell == this {
            if destination.workbench.upgrade() != Some(self.workbench.clone()) {
                return CommandOutcome::InvalidTarget;
            }
            let moved = self.workbench.update(cx, |workbench, cx| {
                let transition = workbench.relocate_tab(source.tab, destination.tab);
                if transition.is_some() {
                    cx.notify();
                }
                transition
            });
            if moved.is_none() {
                return CommandOutcome::InvalidTarget;
            }
            if matches!(source.tab.tab.surface, TabSurfaceId::Tool(_)) {
                self.publish_tool_layout(cx);
            }
            self.focus_relocated_surface(cx);
            cx.notify();
            return CommandOutcome::Completed;
        }
        if matches!(source.tab.tab.surface, TabSurfaceId::Tool(_)) {
            return CommandOutcome::Unavailable;
        }
        if destination.workbench.upgrade() == Some(self.workbench.clone()) {
            return CommandOutcome::InvalidTarget;
        }
        let Some(destination_shell) = destination.shell.upgrade().filter(|shell| {
            cx.global::<ApplicationSessions>()
                .is_open(shell.entity_id())
        }) else {
            return CommandOutcome::InvalidTarget;
        };
        let result = {
            destination_shell.update(cx, |shell, cx| {
                if destination.workbench.upgrade() != Some(shell.workbench.clone())
                    || !self.relocation_is_live(source, cx)
                {
                    return CommandOutcome::InvalidTarget;
                }
                let source_workbench = self.workbench.clone();
                let destination_workbench = shell.workbench.clone();
                let target = destination.tab;
                let session = match source.tab.tab.surface {
                    TabSurfaceId::Terminal(id) => cx
                        .global::<ApplicationTerminalSessions>()
                        .0
                        .borrow()
                        .sessions
                        .get(&id)
                        .cloned(),
                    TabSurfaceId::Document(_) | TabSurfaceId::Tool(_) => None,
                };
                let moved = source_workbench.update(cx, |workbench, cx| {
                    let moved = destination_workbench.update(cx, |destination, cx| {
                        let moved =
                            workbench.transfer_tab_to(destination, source.tab, target, |view| {
                                view.update(cx, |view, cx| view.detach(cx));
                                cx.new(|cx| {
                                    TerminalView::new(
                                        session.expect("validated terminal registry entry"),
                                        cx,
                                    )
                                })
                            });
                        if moved.is_some() {
                            cx.notify();
                        }
                        moved
                    });
                    if moved.is_some() {
                        cx.notify();
                    }
                    moved
                });
                if moved.is_none() {
                    return CommandOutcome::InvalidTarget;
                }
                shell.focus_relocated_surface(cx);
                if !matches!(destination.tab, TabMoveDestination::Empty) {
                    shell.activate_window(cx);
                }
                cx.notify();
                CommandOutcome::Completed
            })
        };
        if result != CommandOutcome::Completed {
            return CommandOutcome::InvalidTarget;
        }
        self.focus_active_surface(cx);
        cx.notify();
        CommandOutcome::Completed
    }

    pub(super) fn relocate_tab_to_new_window_with(
        &mut self,
        source: &TabRelocationSource,
        cx: &mut Context<Self>,
        create_destination: impl FnOnce(&mut App) -> anyhow::Result<WindowHandle<ProductShell>>,
    ) -> CommandOutcome {
        if !self.relocation_is_live(source, cx) {
            return CommandOutcome::InvalidTarget;
        }
        if matches!(source.tab.tab.surface, TabSurfaceId::Tool(_)) {
            return CommandOutcome::Unavailable;
        }
        let existing = cx.windows();
        let Ok(destination) = create_destination(cx) else {
            self.show_notice(
                PromptLevel::Warning,
                "Could not create destination window",
                None,
                cx,
            );
            return CommandOutcome::HandlerFailure {
                message: "could not create destination window".into(),
            };
        };
        let destination_window = destination.into();
        if existing.contains(&destination_window) {
            return CommandOutcome::InvalidTarget;
        }
        let shell = destination.entity(cx).ok();
        let outcome = if let Some(shell) = shell {
            let workbench = shell.read(cx).workbench.downgrade();
            self.relocate_tab_to(
                source,
                &TabRelocationDestination {
                    shell: shell.downgrade(),
                    workbench,
                    tab: TabMoveDestination::Empty,
                },
                cx,
            )
        } else {
            CommandOutcome::InvalidTarget
        };
        if outcome != CommandOutcome::Completed {
            let _ = cx.update_window(destination_window, |_, window, _| window.remove_window());
        } else {
            #[cfg(target_os = "macos")]
            let _ = cx.update_window(destination_window, |_, window, cx| {
                window.draw(cx).clear(cx);
                window.activate_window();
            });
        }
        outcome
    }
}

#[cfg(test)]
mod tests;
