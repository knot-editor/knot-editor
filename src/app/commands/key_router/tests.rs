use super::*;
use crate::app::commands::keymaps::{BindingOwner, BindingRegistry};
use crate::host::protocol::{CommandArgumentValue, ExtensionId, ExtensionLifecycleId};

fn command(name: &str) -> Command {
    Command {
        name: name.to_owned().into(),
        arguments: CommandArgumentValue::Null,
    }
}

fn key(source: &str) -> Keystroke {
    Keystroke::parse(source).unwrap().with_simulated_ime()
}

/// A registry binding `cmd-k right`, `cmd-k`, `g g`, and, in the editor's
/// `modal` context, `z b`.
fn registry() -> BindingRegistry {
    let mut registry = BindingRegistry::new();
    let owner = BindingOwner::Extension(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    registry
        .set(owner, "cmd-k right", None, Some(command("split")))
        .unwrap();
    registry
        .set(owner, "cmd-j", None, Some(command("join")))
        .unwrap();
    registry
        .set(owner, "g g", Some("editor"), Some(command("top")))
        .unwrap();
    registry
        .set(owner, "g", Some("editor"), Some(command("go")))
        .unwrap();
    registry
        .set_in_context(
            owner,
            "z b",
            Some("editor"),
            Some("modal"),
            Some(command("bottom")),
        )
        .unwrap();
    registry
}

struct Typing<'a> {
    router: KeyRouter,
    registry: &'a BindingRegistry,
    contexts: Vec<&'a str>,
}

impl<'a> Typing<'a> {
    fn new(registry: &'a BindingRegistry, contexts: &[&'a str]) -> Self {
        Self {
            router: KeyRouter::default(),
            registry,
            contexts: contexts.to_vec(),
        }
    }

    fn key(&mut self, source: &str) -> Vec<KeyAction> {
        let (registry, contexts) = (self.registry, self.contexts.clone());
        self.router.key(key(source), |typed| {
            registry.match_typed(typed, Some("editor"), &contexts)
        })
    }

    fn flush(&mut self) -> Vec<KeyAction> {
        let (registry, contexts) = (self.registry, self.contexts.clone());
        self.router
            .flush(|typed| registry.match_typed(typed, Some("editor"), &contexts))
    }
}

fn run(name: &str) -> KeyAction {
    KeyAction::Run(command(name))
}

fn native(source: &str) -> KeyAction {
    KeyAction::Native(key(source))
}

#[test]
fn a_sequence_waits_for_its_last_key_and_runs_its_command() {
    let registry = registry();
    let mut typing = Typing::new(&registry, &[]);
    assert_eq!(typing.key("cmd-k"), []);
    assert!(typing.router.is_pending());
    assert_eq!(typing.key("right"), [run("split")]);
    assert!(!typing.router.is_pending());
    assert_eq!(typing.key("cmd-j"), [run("join")]);
    assert_eq!(typing.key("x"), [native("x")]);
}

#[test]
fn a_sequence_that_stops_matching_delivers_its_keys_in_order() {
    let registry = registry();
    let mut typing = Typing::new(&registry, &[]);
    typing.key("cmd-k");
    assert_eq!(typing.key("a"), [native("cmd-k"), native("a")]);
    // The key after the failed prefix may begin a binding of its own.
    typing.key("cmd-k");
    assert_eq!(typing.key("cmd-j"), [native("cmd-k"), run("join")]);
    typing.key("cmd-k");
    assert_eq!(typing.key("cmd-k"), [native("cmd-k")]);
    assert!(typing.router.is_pending());
}

#[test]
fn a_bound_prefix_waits_and_runs_when_the_sequence_ends_or_times_out() {
    let registry = registry();
    let mut typing = Typing::new(&registry, &[]);
    assert_eq!(typing.key("g"), []);
    assert_eq!(typing.key("g"), [run("top")]);
    typing.key("g");
    assert_eq!(typing.key("x"), [run("go"), native("x")]);
    typing.key("g");
    assert_eq!(typing.flush(), [run("go")]);
    assert!(!typing.router.is_pending());
}

#[test]
fn a_timed_out_unbound_prefix_is_delivered() {
    let registry = registry();
    let mut typing = Typing::new(&registry, &[]);
    typing.key("cmd-k");
    assert_eq!(typing.flush(), [native("cmd-k")]);
    assert_eq!(typing.flush(), []);
}

#[test]
fn a_sequence_bound_in_a_context_is_pending_only_while_the_context_is_on() {
    let registry = registry();
    let mut off = Typing::new(&registry, &[]);
    assert_eq!(off.key("z"), [native("z")]);
    assert_eq!(off.key("b"), [native("b")]);
    let mut on = Typing::new(&registry, &["modal"]);
    assert_eq!(on.key("z"), []);
    assert_eq!(on.key("b"), [run("bottom")]);
}
