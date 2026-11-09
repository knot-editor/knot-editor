//! Native Move Tool to Dock…: choose where the focused tool goes from the
//! window's transient picker.

use super::*;
use knot_protocol::{CommandOutcome, PickerItem};

pub(super) const MOVE_TOOL_TITLE: &str = "Move Tool to Dock";

const DESTINATIONS: [(&str, DockLocation); 3] = [
    ("Left", DockLocation::Left),
    ("Right", DockLocation::Right),
    ("Bottom", DockLocation::Bottom),
];

impl ProductShell {
    /// Offer the docks for the captured tool and place it at the choice, publishing the tool-only default as any explicit
    /// move does.
    pub(crate) fn start_move_tool(
        &mut self,
        target: &ProductCommandTarget,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let Some(TabSurfaceId::Tool(id)) = target.surface() else {
            return Err(CommandOutcome::Unavailable);
        };
        let items = DESTINATIONS
            .iter()
            .enumerate()
            .map(|(index, (label, _))| PickerItem {
                detail: None,
                hint: None,
                id: index.to_string(),
                label: (*label).into(),
                description: None,
            })
            .collect();
        let Ok(receiver) = self.begin_owned_picker(None, MOVE_TOOL_TITLE.into(), items, None, cx)
        else {
            cx.notify();
            return Ok(());
        };
        let task = cx.spawn(async move |this, cx| {
            let Ok(TransientOutcome::Picker(choice)) = receiver.await else {
                return;
            };
            let Some((_, dock)) = choice
                .parse::<usize>()
                .ok()
                .and_then(|index| DESTINATIONS.get(index))
            else {
                return;
            };
            let _ = this.update(cx, |shell, cx| shell.place_tool(id, *dock, cx));
        });
        self.tasks.push(task);
        Ok(())
    }
}
