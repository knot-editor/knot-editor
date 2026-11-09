//! Native Toggle Tool…: choose a tool from the window's transient picker and
//! toggle it.

use super::*;
use knot_protocol::PickerItem;

pub(super) const TOGGLE_TOOL_TITLE: &str = "Toggle Tool";

impl ProductShell {
    /// Offer every tool of this window, the explorer included before it is
    /// registered, and toggle the choice.
    pub(super) fn start_toggle_tool(&mut self, cx: &mut Context<Self>) {
        let workbench = self.workbench.read(cx);
        let mut tools = workbench
            .tools()
            .iter()
            .map(|tool| (tool.stable_id.clone(), tool.title.clone()))
            .collect::<Vec<_>>();
        if workbench
            .tool_by_stable_id(crate::app::tool_layout::EXPLORER_TOOL_ID)
            .is_none()
        {
            tools.insert(
                0,
                (
                    crate::app::tool_layout::EXPLORER_TOOL_ID.into(),
                    "Explorer".into(),
                ),
            );
        }
        let items = tools
            .into_iter()
            .map(|(id, title)| PickerItem {
                detail: None,
                hint: None,
                id,
                label: title,
                description: None,
            })
            .collect();
        let Ok(receiver) = self.begin_owned_picker(None, TOGGLE_TOOL_TITLE.into(), items, None, cx)
        else {
            cx.notify();
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            let Ok(TransientOutcome::Picker(choice)) = receiver.await else {
                return;
            };
            let _ = this.update(cx, |shell, cx| shell.toggle_tool(&choice, cx));
        });
        self.tasks.push(task);
    }
}
