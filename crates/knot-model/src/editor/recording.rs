//! Recordings of the input an editor receives: the commands its keys run and
//! the text typed into it, as plain data. The window and the editor's view
//! append entries as input arrives; the extension host owns recording
//! identities. Presentation state, never part of the buffer.

use knot_protocol::CommandArgumentValue;

use super::*;

/// One recorded step: a command and its arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedCommand {
    pub command: String,
    pub arguments: CommandArgumentValue,
}

impl RecordedCommand {
    /// Typed `text`, replacing the `replace` characters before the caret.
    pub fn typed(text: &str, replace: usize) -> Self {
        let mut arguments = std::collections::BTreeMap::from([(
            "text".to_owned(),
            CommandArgumentValue::String(text.to_owned()),
        )]);
        if replace > 0 {
            arguments.insert(
                "replace".to_owned(),
                CommandArgumentValue::Number(replace as f64),
            );
        }
        Self {
            command: commands::TYPE_COMMAND.to_owned(),
            arguments: CommandArgumentValue::Object(arguments),
        }
    }
}

/// The active recordings of one editor, by identity.
#[derive(Default)]
pub(super) struct Recordings {
    active: Vec<(u64, Vec<RecordedCommand>)>,
}

impl EditorState {
    /// Start collecting input under `id`.
    pub fn start_recording(&mut self, id: u64) {
        self.recordings.active.push((id, Vec::new()));
    }

    /// End recording `id` and take what it collected; `None` when it is not
    /// active.
    pub fn finish_recording(&mut self, id: u64) -> Option<Vec<RecordedCommand>> {
        let index = self
            .recordings
            .active
            .iter()
            .position(|(active, _)| *active == id)?;
        Some(self.recordings.active.remove(index).1)
    }

    /// Append `entry` to every active recording.
    pub fn record_input(&mut self, entry: RecordedCommand) {
        if let [(_, only)] = self.recordings.active.as_mut_slice() {
            only.push(entry);
            return;
        }
        for (_, entries) in &mut self.recordings.active {
            entries.push(entry.clone());
        }
    }

    pub fn is_recording(&self) -> bool {
        !self.recordings.active.is_empty()
    }
}
