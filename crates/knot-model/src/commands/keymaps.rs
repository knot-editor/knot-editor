//! Keybinding slots, and how typed keystrokes match them.

use std::collections::{HashMap, HashSet};

use crate::keys::Keystroke;

use knot_protocol::{
    Command, CommandArgumentValue, CommandName, ExtensionId, ExtensionLifecycleId,
    KeybindingMutation,
};

use crate::commands::key_router::TypedMatch;

/// The preset's bindings are the defaults beneath every extension binding.
/// Extension bindings, including personal config's, form one stack in which
/// the latest write wins, so removing a lifecycle reveals the previous write.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingOwner {
    Preset,
    Extension(ExtensionId, ExtensionLifecycleId),
}

impl BindingOwner {
    fn is_default(self) -> bool {
        self == Self::Preset
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SlotKey {
    owner: BindingOwner,
    key: String,
    view: Option<String>,
    /// A key context the focused view must have; only with a view.
    context: Option<String>,
}

impl SlotKey {
    /// How many facts of a view's state the slot requires.
    fn facts(&self) -> usize {
        usize::from(self.view.is_some()) + usize::from(self.context.is_some())
    }

    fn applies(&self, key: &str, view: Option<&str>, contexts: &[&str]) -> bool {
        self.key == key
            && (self.view.is_none() || self.view.as_deref() == view)
            && self
                .context
                .as_deref()
                .is_none_or(|context| contexts.contains(&context))
    }
}

/// A text binding's scope; the owner and facts follow key slots' rule.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextSlotKey {
    owner: BindingOwner,
    view: Option<String>,
    context: Option<String>,
}

#[derive(Clone, Debug)]
struct TextSlot {
    command: Option<CommandName>,
    order: u64,
}

#[derive(Clone, Debug)]
struct Slot {
    command: Option<Command>,
    order: u64,
}

/// One slot of the keymap, as Show Keybindings lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedBinding {
    /// Empty for a text binding.
    pub key: String,
    /// Whether the slot routes typed text rather than binding a key.
    pub text: bool,
    pub view: Option<String>,
    pub context: Option<String>,
    pub owner: BindingOwner,
    pub command: Option<Command>,
    /// The owner of the slot that wins where this one applies.
    pub shadowed_by: Option<BindingOwner>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BindingResolution {
    Command(Command),
    Unbound,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct EffectiveKey {
    global: Option<BindingResolution>,
    views: HashMap<String, BindingResolution>,
    /// The latest context-scoped write for each view and context, with its
    /// order, so a view holding several contexts picks the latest of them.
    contexts: HashMap<(String, String), (u64, BindingResolution)>,
}

impl EffectiveKey {
    fn resolve(&self, view: Option<&str>, contexts: &[&str]) -> Option<&BindingResolution> {
        view.and_then(|view| {
            contexts
                .iter()
                .filter_map(|context| self.contexts.get(&(view.to_owned(), (*context).to_owned())))
                .max_by_key(|(order, _)| *order)
                .map(|(_, resolution)| resolution)
        })
        .or_else(|| view.and_then(|view| self.views.get(view)))
        .or(self.global.as_ref())
    }

    fn is_empty(&self) -> bool {
        self.global.is_none() && self.views.is_empty() && self.contexts.is_empty()
    }
}

#[derive(Debug, Eq, PartialEq)]
/// Which part of a binding is invalid.
pub enum BindingError {
    Key,
    View,
    Context,
}

#[derive(Clone, Default)]
pub struct BindingRegistry {
    slots: HashMap<SlotKey, Slot>,
    text_slots: HashMap<TextSlotKey, TextSlot>,
    active: HashMap<String, EffectiveKey>,
    /// The keystrokes of each active key sequence.
    strokes: HashMap<String, Vec<Keystroke>>,
    kinds: HashSet<String>,
    next_order: u64,
}

impl BindingRegistry {
    pub fn new() -> Self {
        let mut this = Self::default();
        this.kinds
            .extend(["editor", "terminal", "bar"].into_iter().map(str::to_owned));
        this
    }

    pub fn declare_extension_kind(&mut self, kind: &str) -> Result<(), BindingError> {
        if kind.is_empty()
            || matches!(kind, "editor" | "terminal" | "bar")
            || kind.chars().any(char::is_whitespace)
        {
            return Err(BindingError::View);
        }
        self.kinds.insert(kind.to_owned());
        Ok(())
    }

    /// Remove a contributed view kind together with its scoped bindings.
    pub fn remove_extension_kind(&mut self, kind: &str) -> bool {
        if matches!(kind, "editor" | "terminal" | "bar") || !self.kinds.remove(kind) {
            return false;
        }
        let keys = self
            .slots
            .keys()
            .filter(|key| key.view.as_deref() == Some(kind))
            .map(|key| key.key.clone())
            .collect::<HashSet<_>>();
        self.slots
            .retain(|key, _| key.view.as_deref() != Some(kind));
        let mut changed = false;
        for key in keys {
            let before = self.active.get(&key).cloned();
            self.refresh_key(&key);
            changed |= before != self.active.get(&key).cloned();
        }
        changed
    }

    pub fn set(
        &mut self,
        owner: BindingOwner,
        key: &str,
        view: Option<&str>,
        command: Option<Command>,
    ) -> Result<bool, BindingError> {
        self.set_in_context(owner, key, view, None, command)
    }

    pub fn set_in_context(
        &mut self,
        owner: BindingOwner,
        key: &str,
        view: Option<&str>,
        context: Option<&str>,
        command: Option<Command>,
    ) -> Result<bool, BindingError> {
        let key = canonical_key(key)?;
        let view = self.validate_view(view)?;
        let context = validate_context(view.as_deref(), context)?;
        let before = self.active.get(&key).cloned();
        self.next_order += 1;
        self.slots.insert(
            SlotKey {
                owner,
                key: key.clone(),
                view,
                context,
            },
            Slot {
                command,
                order: self.next_order,
            },
        );
        self.refresh_key(&key);
        Ok(before != self.active.get(&key).cloned())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn remove(
        &mut self,
        owner: BindingOwner,
        key: &str,
        view: Option<&str>,
    ) -> Result<bool, BindingError> {
        self.remove_in_context(owner, key, view, None)
    }

    pub fn remove_in_context(
        &mut self,
        owner: BindingOwner,
        key: &str,
        view: Option<&str>,
        context: Option<&str>,
    ) -> Result<bool, BindingError> {
        let key = canonical_key(key)?;
        // Removal is idempotent: the scoped kind may already be gone when an
        // unload or view disposal commits before the keybinding batch.
        let view = view.map(ToOwned::to_owned);
        let context = validate_context(view.as_deref(), context)?;
        let before = self.active.get(&key).cloned();
        self.slots.remove(&SlotKey {
            owner,
            key: key.clone(),
            view,
            context,
        });
        self.refresh_key(&key);
        Ok(before != self.active.get(&key).cloned())
    }

    /// Route committed text typed in matching views to `command`, or mask a
    /// lower text binding with `None`.
    pub fn set_text(
        &mut self,
        owner: BindingOwner,
        view: Option<&str>,
        context: Option<&str>,
        command: Option<CommandName>,
    ) -> Result<(), BindingError> {
        let view = self.validate_view(view)?;
        let context = validate_context(view.as_deref(), context)?;
        self.next_order += 1;
        self.text_slots.insert(
            TextSlotKey {
                owner,
                view,
                context,
            },
            TextSlot {
                command,
                order: self.next_order,
            },
        );
        Ok(())
    }

    pub fn remove_text(
        &mut self,
        owner: BindingOwner,
        view: Option<&str>,
        context: Option<&str>,
    ) -> Result<(), BindingError> {
        let context = validate_context(view, context)?;
        self.text_slots.remove(&TextSlotKey {
            owner,
            view: view.map(ToOwned::to_owned),
            context,
        });
        Ok(())
    }

    /// The command that receives text typed in `view` holding `contexts`,
    /// or `None` when the text is inserted.
    pub fn resolve_text(&self, view: &str, contexts: &[&str]) -> Option<CommandName> {
        self.winning_text_slot(Some(view), contexts)
            .and_then(|slot_key| self.text_slots[slot_key].command.clone())
    }

    fn winning_text_slot(&self, view: Option<&str>, contexts: &[&str]) -> Option<&TextSlotKey> {
        self.text_slots
            .iter()
            .filter(|(slot_key, _)| {
                (slot_key.view.is_none() || slot_key.view.as_deref() == view)
                    && slot_key
                        .context
                        .as_deref()
                        .is_none_or(|context| contexts.contains(&context))
            })
            .max_by_key(|(slot_key, slot)| {
                (
                    usize::from(slot_key.view.is_some()) + usize::from(slot_key.context.is_some()),
                    slot.order,
                )
            })
            .map(|(slot_key, _)| slot_key)
    }

    pub fn remove_owner(&mut self, owner: BindingOwner) -> bool {
        self.text_slots.retain(|key, _| key.owner != owner);
        let keys = self
            .slots
            .keys()
            .filter(|key| key.owner == owner)
            .map(|key| key.key.clone())
            .collect::<HashSet<_>>();
        self.slots.retain(|key, _| key.owner != owner);
        let mut changed = false;
        for key in keys {
            let before = self.active.get(&key).cloned();
            self.refresh_key(&key);
            changed |= before != self.active.get(&key).cloned();
        }
        changed
    }

    /// Replace the preset slot's bindings. A binding whose view kind is not
    /// declared is skipped, so a preset may scope keys to a contributed view
    /// that is absent.
    pub fn replace_preset(&mut self, bindings: &[PresetBinding]) -> bool {
        let before = self.active.clone();
        self.remove_owner(BindingOwner::Preset);
        for binding in bindings {
            let _ = self.set(
                BindingOwner::Preset,
                &binding.key,
                binding.view.as_deref(),
                binding.command.clone(),
            );
        }
        before != self.active
    }

    pub fn apply_batch(
        &mut self,
        owner: BindingOwner,
        mutations: &[KeybindingMutation],
    ) -> Result<bool, String> {
        let mut candidate = self.clone();
        for (index, mutation) in mutations.iter().enumerate() {
            let result = match mutation {
                KeybindingMutation::Set {
                    key,
                    view,
                    context,
                    command,
                } => {
                    if command
                        .as_ref()
                        .is_some_and(|command| command.name.as_ref().is_empty())
                    {
                        return Err(format!(
                            "keybinding operation {}: command name is empty",
                            index + 1
                        ));
                    }
                    candidate.set_in_context(
                        owner,
                        key,
                        view.as_deref(),
                        context.as_deref(),
                        command.clone(),
                    )
                }
                KeybindingMutation::Remove { key, view, context } => {
                    candidate.remove_in_context(owner, key, view.as_deref(), context.as_deref())
                }
                KeybindingMutation::SetText {
                    view,
                    context,
                    command,
                } => {
                    if command
                        .as_ref()
                        .is_some_and(|command| command.as_ref().is_empty())
                    {
                        return Err(format!(
                            "keybinding operation {}: command name is empty",
                            index + 1
                        ));
                    }
                    candidate
                        .set_text(owner, view.as_deref(), context.as_deref(), command.clone())
                        .map(|()| false)
                }
                KeybindingMutation::RemoveText { view, context } => candidate
                    .remove_text(owner, view.as_deref(), context.as_deref())
                    .map(|()| false),
                KeybindingMutation::Invalid(message) => {
                    return Err(format!("keybinding operation {}: {message}", index + 1));
                }
            };
            if let Err(error) = result {
                let message = match error {
                    BindingError::Key => "invalid key sequence",
                    BindingError::View => "unknown view kind",
                    BindingError::Context => "a context must be a non-empty name and needs a view",
                };
                return Err(format!("keybinding operation {}: {message}", index + 1));
            }
        }
        let changed = self.active != candidate.active;
        *self = candidate;
        Ok(changed)
    }

    /// Reads a canonical key sequence from the merged map.
    #[cfg(any(test, feature = "test-support"))]
    pub fn resolve(&self, key: &str, view: Option<&str>) -> Option<BindingResolution> {
        self.resolve_in(key, view, &[])
    }

    /// Reads a canonical key sequence for a focused view holding `contexts`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn resolve_in(
        &self,
        key: &str,
        view: Option<&str>,
        contexts: &[&str],
    ) -> Option<BindingResolution> {
        self.active.get(key)?.resolve(view, contexts).cloned()
    }

    /// The key sequence to show for each argument-less command in a view:
    /// the binding with the fewest keystrokes, then the shortest text, with
    /// remaining ties going to the latest write.
    pub fn shown_bindings(&self, view: Option<&str>) -> HashMap<CommandName, String> {
        let mut shown: HashMap<CommandName, (usize, usize, std::cmp::Reverse<u64>, &str)> =
            HashMap::new();
        for (key, effective) in &self.active {
            let Some(BindingResolution::Command(command)) = effective.resolve(view, &[]) else {
                continue;
            };
            if command.arguments != CommandArgumentValue::Null {
                continue;
            }
            let order = self
                .slots
                .iter()
                .filter(|(slot_key, slot)| {
                    slot_key.key == *key && slot.command.as_ref() == Some(command)
                })
                .map(|(_, slot)| slot.order)
                .max()
                .unwrap_or_default();
            let rank = (
                key.split_whitespace().count(),
                key.len(),
                std::cmp::Reverse(order),
                key.as_str(),
            );
            shown
                .entry(command.name.clone())
                .and_modify(|best| *best = (*best).min(rank))
                .or_insert(rank);
        }
        shown
            .into_iter()
            .map(|(name, (_, _, _, key))| (name, key.to_owned()))
            .collect()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn keys(&self) -> Vec<String> {
        let mut keys = self.active.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    }

    /// How `typed` matches the bindings that resolve to a command in
    /// `view` holding `contexts`. A keystroke triggers a binding as
    /// [`Keystroke::triggers`] decides. Among exact matches a direct one wins.
    pub fn match_typed(
        &self,
        typed: &[Keystroke],
        view: Option<&str>,
        contexts: &[&str],
    ) -> TypedMatch {
        let mut result = TypedMatch::default();
        let mut direct = false;
        for (key, effective) in &self.active {
            let strokes = &self.strokes[key];
            if strokes.len() < typed.len()
                || !typed
                    .iter()
                    .zip(strokes)
                    .all(|(typed, bound)| typed.triggers(bound))
            {
                continue;
            }
            let Some(BindingResolution::Command(command)) = effective.resolve(view, contexts)
            else {
                continue;
            };
            if strokes.len() > typed.len() {
                result.longer = true;
                continue;
            }
            let is_direct = typed
                .iter()
                .zip(strokes)
                .all(|(typed, bound)| typed.key == bound.key && typed.modifiers == bound.modifiers);
            if result.exact.is_none() || (is_direct && !direct) {
                result.exact = Some(command.clone());
                direct = is_direct;
            }
        }
        result
    }

    fn validate_view(&self, view: Option<&str>) -> Result<Option<String>, BindingError> {
        match view {
            Some(kind) if self.kinds.contains(kind) => Ok(Some(kind.to_owned())),
            Some(_) => Err(BindingError::View),
            None => Ok(None),
        }
    }

    /// Every slot with its owner and, when another slot wins where it
    /// applies, the owner of that slot. A global slot applies outside every
    /// view and a scoped slot inside its view. Sorted by key, then view.
    pub fn listing(&self) -> Vec<ListedBinding> {
        let mut listing = self
            .slots
            .iter()
            .map(|(slot_key, slot)| {
                let contexts = slot_key.context.as_deref().into_iter().collect::<Vec<_>>();
                let winner = self.winning_slot(&slot_key.key, slot_key.view.as_deref(), &contexts);
                ListedBinding {
                    key: slot_key.key.clone(),
                    text: false,
                    view: slot_key.view.clone(),
                    context: slot_key.context.clone(),
                    owner: slot_key.owner,
                    command: slot.command.clone(),
                    shadowed_by: winner
                        .filter(|winner| *winner != slot_key)
                        .map(|winner| winner.owner),
                }
            })
            .chain(self.text_slots.iter().map(|(slot_key, slot)| {
                let contexts = slot_key.context.as_deref().into_iter().collect::<Vec<_>>();
                let winner = self.winning_text_slot(slot_key.view.as_deref(), &contexts);
                ListedBinding {
                    key: String::new(),
                    text: true,
                    view: slot_key.view.clone(),
                    context: slot_key.context.clone(),
                    owner: slot_key.owner,
                    command: slot.command.clone().map(|name| Command {
                        name,
                        arguments: CommandArgumentValue::Null,
                    }),
                    shadowed_by: winner
                        .filter(|winner| *winner != slot_key)
                        .map(|winner| winner.owner),
                }
            }))
            .collect::<Vec<_>>();
        listing.sort_by(|a, b| {
            (&a.key, &a.view, &a.context, a.shadowed_by.is_some()).cmp(&(
                &b.key,
                &b.view,
                &b.context,
                b.shadowed_by.is_some(),
            ))
        });
        listing
    }

    /// The slot that decides `key` in `view` holding `contexts`: extension
    /// slots beat preset ones, then the slot requiring the most facts (view,
    /// context) wins, then the latest write.
    fn winning_slot(&self, key: &str, view: Option<&str>, contexts: &[&str]) -> Option<&SlotKey> {
        self.slots
            .iter()
            .filter(|(slot_key, _)| slot_key.applies(key, view, contexts))
            .max_by_key(|(slot_key, slot)| {
                (!slot_key.owner.is_default(), slot_key.facts(), slot.order)
            })
            .map(|(slot_key, _)| slot_key)
    }

    fn refresh_key(&mut self, key: &str) {
        let resolution = |slot_key: &SlotKey| match &self.slots[slot_key].command {
            Some(command) => BindingResolution::Command(command.clone()),
            None => BindingResolution::Unbound,
        };
        let winner = |view: Option<&str>| self.winning_slot(key, view, &[]).map(resolution);
        let global = winner(None);
        let views = self
            .kinds
            .iter()
            .filter_map(|kind| {
                let resolution = winner(Some(kind))?;
                (Some(&resolution) != global.as_ref()).then(|| (kind.clone(), resolution))
            })
            .collect();
        // Every context slot is an extension's and requires two facts, so the
        // latest one for a view and context wins there.
        let contexts = self
            .slots
            .iter()
            .filter(|(slot_key, _)| slot_key.key == key)
            .filter_map(|(slot_key, slot)| {
                let context = slot_key.context.clone()?;
                Some(((slot_key.view.clone()?, context), slot.order, slot_key))
            })
            .fold(
                HashMap::<(String, String), (u64, BindingResolution)>::new(),
                |mut contexts, (scope, order, slot_key)| {
                    if contexts
                        .get(&scope)
                        .is_none_or(|(latest, _)| *latest < order)
                    {
                        contexts.insert(scope, (order, resolution(slot_key)));
                    }
                    contexts
                },
            );
        let effective = EffectiveKey {
            global,
            views,
            contexts,
        };
        if effective.is_empty() {
            self.active.remove(key);
            self.strokes.remove(key);
        } else {
            self.active.insert(key.to_owned(), effective);
            self.strokes.insert(
                key.to_owned(),
                key.split_whitespace()
                    .filter_map(|stroke| Keystroke::parse(stroke).ok())
                    .collect(),
            );
        }
    }
}

/// Render a canonical key sequence for display: `⌘K ⌘S` on macOS and
/// `Ctrl+K Ctrl+S` elsewhere.
pub fn display_key(key: &str) -> String {
    display_key_for(key, cfg!(target_os = "macos"))
}

fn display_key_for(key: &str, mac: bool) -> String {
    key.split_whitespace()
        .map(|stroke| {
            let Ok(stroke) = Keystroke::parse(stroke) else {
                return stroke.to_owned();
            };
            let modifiers = stroke.modifiers;
            let name = display_key_name(&stroke.key, mac);
            if mac {
                let mut text = String::new();
                for (held, symbol) in [
                    (modifiers.control, "⌃"),
                    (modifiers.alt, "⌥"),
                    (modifiers.shift, "⇧"),
                    (modifiers.platform, "⌘"),
                    (modifiers.function, "fn "),
                ] {
                    if held {
                        text.push_str(symbol);
                    }
                }
                text + &name
            } else {
                let platform = if cfg!(windows) { "Win" } else { "Super" };
                [
                    (modifiers.control, "Ctrl"),
                    (modifiers.alt, "Alt"),
                    (modifiers.shift, "Shift"),
                    (modifiers.platform, platform),
                    (modifiers.function, "Fn"),
                ]
                .into_iter()
                .filter_map(|(held, label)| held.then_some(label))
                .chain([name.as_str()])
                .collect::<Vec<_>>()
                .join("+")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_key_name(key: &str, mac: bool) -> String {
    let named = match (key, mac) {
        ("left", true) => "←",
        ("right", true) => "→",
        ("up", true) => "↑",
        ("down", true) => "↓",
        ("enter", true) => "↩",
        ("escape", true) => "⎋",
        ("tab", true) => "⇥",
        ("backspace", true) => "⌫",
        ("delete", true) => "⌦",
        ("left", false) => "Left",
        ("right", false) => "Right",
        ("up", false) => "Up",
        ("down", false) => "Down",
        ("enter", false) => "Enter",
        ("escape", false) => "Esc",
        ("tab", false) => "Tab",
        ("backspace", false) => "Backspace",
        ("delete", false) => "Delete",
        ("space", _) => "Space",
        ("pageup", _) => "PgUp",
        ("pagedown", _) => "PgDn",
        ("home", _) => "Home",
        ("end", _) => "End",
        _ => {
            let mut chars = key.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            };
        }
    };
    named.to_owned()
}

pub fn view_context(kind: &str) -> String {
    match kind {
        "editor" => "editor".to_owned(),
        "terminal" => "terminal".to_owned(),
        "workspace-tree" => "workspace_tree".to_owned(),
        _ => {
            let mut context = "knot_view_".to_owned();
            for byte in kind.as_bytes() {
                context.push_str(&format!("{byte:02x}"));
            }
            context
        }
    }
}

/// A context names a key context of the slot's view, so it needs one.
fn validate_context(
    view: Option<&str>,
    context: Option<&str>,
) -> Result<Option<String>, BindingError> {
    match context {
        Some(context) if view.is_none() || context.is_empty() => Err(BindingError::Context),
        context => Ok(context.map(ToOwned::to_owned)),
    }
}

pub fn canonical_key(source: &str) -> Result<String, BindingError> {
    if source.trim().is_empty() {
        return Err(BindingError::Key);
    }
    let canonical = source
        .split_whitespace()
        .map(|stroke| {
            let stroke = if stroke.contains('-') {
                stroke.to_ascii_lowercase()
            } else {
                stroke.to_owned()
            };
            Keystroke::parse(&stroke)
                .map(|stroke| stroke.unparse())
                .map_err(|_| BindingError::Key)
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    Ok(canonical)
}

/// The key each argument-less command shows in a menu: its shown global
/// single keystroke. macOS performs a menu's key equivalent when the window
/// leaves the key unhandled, so a key that some view unbinds gets no menu key,
/// or pressing it there would run the menu command.
pub fn menu_shortcuts(registry: &BindingRegistry) -> Vec<(CommandName, String)> {
    registry
        .shown_bindings(None)
        .into_iter()
        .filter(|(_, key)| {
            !key.contains(' ')
                && !registry.active[key]
                    .views
                    .values()
                    .chain(
                        registry.active[key]
                            .contexts
                            .values()
                            .map(|(_, resolution)| resolution),
                    )
                    .any(|resolution| *resolution == BindingResolution::Unbound)
        })
        .collect()
}

mod presets;
#[cfg(test)]
mod tests;

pub use presets::*;
