//! The right-click menus of a tab header (`tab` items) and of a surface's
//! content (`content` items), whose commands run against the clicked tab
//! rather than the focused one, and the command that opens the focused
//! surface's menu from the keyboard.

use super::*;
use crate::app::context_menu::ContextMenu;
use crate::app::menus::{ApplicationMenus, MenuEntry, MenuLocation, MenuScope};
use crate::app::workbench::CommandView;
use gpui_component::menu::PopupMenuItem;

impl KnotWindow {
    /// A command target for the tab `tab` of `pane`, which is not necessarily
    /// the focused one: the tab's own view takes its commands. Only the
    /// focused pane's active extension view is named, as for a focused
    /// capture.
    pub(crate) fn capture_tab_target(
        &mut self,
        pane: PaneId,
        tab: TabId,
        cx: &mut Context<Self>,
    ) -> Option<ProductCommandTarget> {
        let workbench = self.workbench.read(cx);
        let clicked = workbench.tab(pane, tab)?;
        let is_active_focus = matches!(self.focus, Focus::Workbench | Focus::Popup)
            && workbench
                .focused_pane()
                .is_some_and(|focused| focused.id() == pane && focused.active_tab_id() == tab);
        let view = match workbench.command_view(clicked) {
            Some(CommandView::Extension(extension)) if is_active_focus => {
                Some(extension.instance_id(cx))
            }
            _ => None,
        };
        let target = TargetTab {
            pane,
            tab,
            surface: clicked.surface_id(),
            presentation: clicked.presentation(),
        };
        Some(ProductCommandTarget {
            window: cx.entity().downgrade(),
            workbench: self.workbench.downgrade(),
            tab: Some(target),
            focused: true,
            view,
            leaf: is_active_focus
                .then(|| workbench.focused_leaf(clicked, cx))
                .flatten(),
        })
    }

    /// The view kind and key contexts of a captured tab's surface, which the
    /// menu items' scopes match.
    fn target_scope(target: &ProductCommandTarget, cx: &App) -> (Option<String>, Vec<String>) {
        match target.command_view(cx) {
            Some(CommandView::Editor(view)) => {
                let contexts = view
                    .read(cx)
                    .editor()
                    .read(cx)
                    .key_contexts()
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                (Some("editor".to_owned()), contexts)
            }
            Some(CommandView::Terminal(_)) => (Some("terminal".to_owned()), Vec::new()),
            Some(CommandView::Extension(view)) => (Some(view.kind(cx)), Vec::new()),
            None => (None, Vec::new()),
        }
    }

    /// Each command's title, for items that have none of their own.
    pub(crate) fn command_titles(cx: &App) -> std::collections::HashMap<String, String> {
        Self::command_dispatcher(cx)
            .read(cx)
            .catalog()
            .borrow()
            .definitions()
            .map(|definition| (definition.name.to_string(), definition.title.clone()))
            .collect()
    }

    /// The groups of a menu at `location` for the surface `target` captured.
    pub(crate) fn menu_groups(
        location: MenuLocation,
        target: &ProductCommandTarget,
        cx: &App,
    ) -> Vec<Vec<MenuEntry>> {
        let (view, contexts) = Self::target_scope(target, cx);
        let contexts = contexts.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(menus) = cx.try_global::<ApplicationMenus>() else {
            return Vec::new();
        };
        let titles = Self::command_titles(cx);
        menus.0.groups(
            location,
            MenuScope {
                view: view.as_deref(),
                contexts: &contexts,
            },
            |command| {
                titles
                    .get(command)
                    .cloned()
                    .unwrap_or_else(|| command.to_owned())
            },
        )
    }

    /// Open the menu of tab `tab` at `position`. It takes focus, which returns
    /// to the active surface when it closes; the clicked tab stays as it was.
    pub(super) fn open_tab_menu(
        &mut self,
        pane: PaneId,
        tab: TabId,
        position: Point<Pixels>,
        native_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_surface_menu(MenuLocation::Tab, pane, tab, position, native_window, cx);
    }

    /// Open the content menu of the active tab of `pane` at `position`, when
    /// it shows an editor or a terminal.
    pub(super) fn open_content_menu(
        &mut self,
        pane: PaneId,
        position: Point<Pixels>,
        native_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = self
            .workbench
            .read(cx)
            .pane(pane)
            .map(|pane| pane.active_tab_id());
        if let Some(tab) = tab {
            self.open_surface_menu(
                MenuLocation::Content,
                pane,
                tab,
                position,
                native_window,
                cx,
            );
        }
    }

    fn open_surface_menu(
        &mut self,
        location: MenuLocation,
        pane: PaneId,
        tab: TabId,
        position: Point<Pixels>,
        native_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.capture_tab_target(pane, tab, cx) else {
            return;
        };
        let groups = Self::menu_groups(location, &target, cx);
        if groups.is_empty() {
            return;
        }
        let window = cx.entity().downgrade();
        let menu = ContextMenu::open(
            native_window,
            cx,
            position,
            None,
            move |menu| {
                let mut menu = menu;
                for (index, group) in groups.into_iter().enumerate() {
                    if index > 0 {
                        menu = menu.separator();
                    }
                    for entry in group {
                        let window = window.clone();
                        let target = target.clone();
                        menu =
                            menu.item(PopupMenuItem::new(entry.title).on_click(move |_, _, cx| {
                                let command = knot_protocol::Command {
                                    name: entry.command.clone().into(),
                                    arguments: entry.arguments.clone(),
                                };
                                let target = target.clone();
                                let _ = window.update(cx, |this, cx| {
                                    this.dispatch_command(command, target, cx)
                                });
                            }));
                    }
                }
                menu
            },
            Self::close_surface_menu,
        );
        self.surface_menu = Some(menu);
        self.set_focus(Focus::Popup, cx);
        cx.notify();
    }

    fn close_surface_menu(&mut self, cx: &mut Context<Self>) {
        if self.surface_menu.take().is_some() {
            self.close_popup_focus(cx);
            cx.notify();
        }
    }
}

/// `workbench.show-context-menu`: open the menu of the focused surface: a
/// tree's selected row, or an editor's content menu at the caret.
#[crate::app::commands::declarations::command("workbench.show-context-menu", title = "Show Context Menu", description = "Show the context menu of the focused tree row or editor.", id = SHOW_CONTEXT_MENU_COMMAND, participant = Workbench)]
pub(crate) async fn show_context_menu(
    context: crate::app::commands::declarations::CommandContext,
    (): (),
) -> Result<(), knot_protocol::CommandOutcome> {
    use crate::app::workbench::ExtensionView;
    use knot_protocol::CommandOutcome;
    context
        .foreground(|target, cx| {
            let tab = target
                .tab
                .filter(|_| target.focused)
                .ok_or(CommandOutcome::Unavailable)?;
            let window = target
                .native_window(cx)
                .ok_or(CommandOutcome::Unavailable)?;
            let product = target.window.clone();
            let opened = match target.command_view(cx) {
                Some(CommandView::Extension(ExtensionView::Tree(tree))) => window
                    .update(cx, |_, window, cx| {
                        tree.update(cx, |tree, cx| tree.open_selected_context_menu(window, cx))
                    })
                    .map_err(|_| CommandOutcome::InvalidTarget)?,
                Some(CommandView::Editor(view)) => window
                    .update(cx, |_, window, cx| {
                        let position = view.read(cx).caret_menu_position(window, cx);
                        product
                            .update(cx, |product, cx| {
                                product.open_content_menu(tab.pane, position, window, cx);
                                product.surface_menu.is_some()
                            })
                            .unwrap_or(false)
                    })
                    .map_err(|_| CommandOutcome::InvalidTarget)?,
                _ => false,
            };
            opened.then_some(()).ok_or(CommandOutcome::Unavailable)
        })
        .await
}
