//! Commands recently confirmed in the command palette, most recent first.

use serde::{Deserialize, Serialize};

/// The most commands kept.
pub const COMMAND_HISTORY_LIMIT: usize = 50;

/// The recently confirmed command names, as persisted.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RecentCommands {
    commands: Vec<String>,
}

impl RecentCommands {
    /// Command names, most recent first.
    pub fn recent(&self) -> &[String] {
        &self.commands
    }

    /// Move a confirmed command to the front, dropping names that are no
    /// longer registered and keeping at most [`COMMAND_HISTORY_LIMIT`].
    pub fn record(&self, name: &str, registered: impl Fn(&str) -> bool) -> Self {
        let commands = std::iter::once(name.to_owned())
            .chain(
                self.commands
                    .iter()
                    .filter(|recent| *recent != name && registered(recent))
                    .cloned(),
            )
            .take(COMMAND_HISTORY_LIMIT)
            .collect();
        Self { commands }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_most_recent_first() {
        let history = RecentCommands::default()
            .record("file.save", |_| true)
            .record("file.open", |_| true)
            .record("file.save", |_| true);
        assert_eq!(history.recent(), ["file.save", "file.open"]);
    }

    #[test]
    fn prunes_unregistered_names_and_keeps_the_limit() {
        let mut history = RecentCommands::default();
        for index in 0..COMMAND_HISTORY_LIMIT + 5 {
            history = history.record(&format!("command.{index}"), |_| true);
        }
        assert_eq!(history.recent().len(), COMMAND_HISTORY_LIMIT);
        assert_eq!(
            history.recent()[0],
            format!("command.{}", COMMAND_HISTORY_LIMIT + 4)
        );

        let history = history.record("command.new", |name| name != "command.54");
        assert_eq!(history.recent()[0], "command.new");
        assert!(!history.recent().contains(&"command.54".to_owned()));
        assert_eq!(history.recent().len(), COMMAND_HISTORY_LIMIT);
    }
}
