//! Completion sessions: requests to the registered providers, the aggregated
//! items and failures they answer with, and the list selection.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use knot_protocol::{ExtensionId, ExtensionLifecycleId, ProviderRegistrationId};

/// One provider's request of an open completion. The provider receives it as
/// the JSON argument of `provideCompletions`, with the document it completes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionRequest {
    pub registration: ProviderRegistrationId,
    pub revision: u64,
    pub cursor_byte_offset: usize,
    pub prefix: String,
    pub generation: u64,
}

impl CompletionRequest {
    /// The JSON argument `provideCompletions` receives for `document`.
    pub fn argument(&self, document: &str) -> String {
        serde_json::json!({
            "document": document,
            "revision": self.revision,
            "cursorByteOffset": self.cursor_byte_offset,
            "prefix": self.prefix,
        })
        .to_string()
    }

    /// The response a provider call settles this request with: the items of
    /// its JSON result, or why it failed.
    pub fn response(&self, result: Result<String, String>) -> CompletionResponse {
        CompletionResponse {
            registration: self.registration,
            revision: self.revision,
            generation: self.generation,
            result: result.and_then(|text| {
                serde_json::from_str(&text).map_err(|error| {
                    format!("completions must be a list of {{ label, insertText }}: {error}")
                })
            }),
        }
    }
}

/// One candidate a provider answers with.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResultItem {
    pub label: String,
    pub insert_text: String,
}

/// A provider's answer to one request: its items or its failure message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionResponse {
    pub registration: ProviderRegistrationId,
    pub revision: u64,
    pub generation: u64,
    pub result: Result<Vec<CompletionResultItem>, String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompletionItemId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionItem {
    pub id: CompletionItemId,
    pub label: String,
    pub insert_text: String,
    pub provider_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionFailure {
    pub provider_label: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionSnapshot {
    pub items: Arc<[CompletionItem]>,
    pub pending_provider_count: usize,
    pub failures: Arc<[CompletionFailure]>,
}

fn initial_selection(snapshot: &CompletionSnapshot) -> Option<CompletionItemId> {
    snapshot.items.first().map(|item| item.id)
}

fn move_selection(
    snapshot: &CompletionSnapshot,
    selected: &mut Option<CompletionItemId>,
    delta: isize,
) {
    if snapshot.items.is_empty() {
        *selected = None;
        return;
    }
    let index = selected
        .and_then(|selected| snapshot.items.iter().position(|item| item.id == selected))
        .unwrap_or(0);
    let last = snapshot.items.len().saturating_sub(1) as isize;
    let next = (index as isize + delta).clamp(0, last) as usize;
    *selected = Some(snapshot.items[next].id);
}

fn preserve_selection(snapshot: &CompletionSnapshot, selected: &mut Option<CompletionItemId>) {
    if selected.is_none_or(|selected| !snapshot.items.iter().any(|item| item.id == selected)) {
        *selected = initial_selection(snapshot);
    }
}

/// The native completion list presenting one aggregated snapshot.
pub struct CompletionSurface {
    snapshot: CompletionSnapshot,
    selected: Option<CompletionItemId>,
}

impl CompletionSurface {
    pub fn attach(snapshot: CompletionSnapshot) -> Self {
        let selected = initial_selection(&snapshot);
        Self { snapshot, selected }
    }

    pub fn update(&mut self, snapshot: CompletionSnapshot) {
        self.snapshot = snapshot;
        preserve_selection(&self.snapshot, &mut self.selected);
    }

    pub fn move_selection(&mut self, delta: isize) {
        move_selection(&self.snapshot, &mut self.selected, delta);
    }

    pub fn selected_item(&self) -> Option<CompletionItemId> {
        self.selected
    }

    pub fn snapshot(&self) -> &CompletionSnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug)]
struct ProviderItem {
    id: CompletionItemId,
    item: CompletionResultItem,
    local_order: usize,
}

#[derive(Clone, Debug)]
enum ProviderState {
    Pending,
    Successful(Vec<ProviderItem>),
    Failed(String),
}

#[derive(Clone, Debug)]
struct SessionProvider {
    registration: CompletionProviderRegistration,
    state: ProviderState,
}

pub struct CompletionController {
    revision: u64,
    cursor_byte_offset: usize,
    prefix_range: Range<usize>,
    prefix: String,
    generation: u64,
    next_item_id: u64,
    providers: HashMap<ProviderRegistrationId, SessionProvider>,
    snapshot: CompletionSnapshot,
}

impl CompletionController {
    pub fn new(
        revision: u64,
        cursor_byte_offset: usize,
        prefix_range: Range<usize>,
        prefix: String,
        generation: u64,
        registrations: Vec<CompletionProviderRegistration>,
    ) -> Self {
        let pending_provider_count = registrations.len();
        let providers = registrations
            .into_iter()
            .map(|registration| {
                (
                    registration.id,
                    SessionProvider {
                        registration,
                        state: ProviderState::Pending,
                    },
                )
            })
            .collect();
        Self {
            revision,
            cursor_byte_offset,
            prefix_range,
            prefix,
            generation,
            next_item_id: 1,
            providers,
            snapshot: CompletionSnapshot {
                items: Arc::from([]),
                pending_provider_count,
                failures: Arc::from([]),
            },
        }
    }

    pub fn requests(&self) -> Vec<(CompletionProviderRegistration, CompletionRequest)> {
        let mut providers = self
            .providers
            .values()
            .map(|provider| provider.registration.clone())
            .collect::<Vec<_>>();
        providers.sort_by_key(|provider| provider.id.value());
        providers
            .into_iter()
            .map(|provider| {
                let request = CompletionRequest {
                    registration: provider.id,
                    revision: self.revision,
                    cursor_byte_offset: self.cursor_byte_offset,
                    prefix: self.prefix.clone(),
                    generation: self.generation,
                };
                (provider, request)
            })
            .collect()
    }

    pub fn snapshot(&self) -> &CompletionSnapshot {
        &self.snapshot
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn prefix_range(&self) -> Range<usize> {
        self.prefix_range.clone()
    }

    pub fn item(&self, id: CompletionItemId) -> Option<&CompletionItem> {
        self.snapshot.items.iter().find(|item| item.id == id)
    }

    pub fn apply_response(&mut self, response: CompletionResponse) -> bool {
        if response.revision != self.revision || response.generation != self.generation {
            return false;
        }
        let Some(provider) = self.providers.get_mut(&response.registration) else {
            return false;
        };
        if !matches!(provider.state, ProviderState::Pending) {
            return false;
        }
        provider.state = match response.result {
            Ok(items) => ProviderState::Successful(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(local_order, item)| {
                        let id = CompletionItemId(self.next_item_id);
                        self.next_item_id = self
                            .next_item_id
                            .checked_add(1)
                            .expect("completion item identity space exhausted");
                        ProviderItem {
                            id,
                            item,
                            local_order,
                        }
                    })
                    .collect(),
            ),
            Err(message) => ProviderState::Failed(message),
        };
        self.rebuild_snapshot();
        true
    }

    pub fn provider_unavailable(&mut self, registration: ProviderRegistrationId) -> bool {
        if self.providers.remove(&registration).is_none() {
            return false;
        }
        self.rebuild_snapshot();
        true
    }

    fn rebuild_snapshot(&mut self) {
        let mut ranked = Vec::new();
        let mut failures = Vec::new();
        let mut pending_provider_count = 0;
        for provider in self.providers.values() {
            match &provider.state {
                ProviderState::Pending => pending_provider_count += 1,
                ProviderState::Failed(message) => failures.push((
                    provider.registration.id.value(),
                    CompletionFailure {
                        provider_label: provider.registration.label.clone(),
                        message: message.clone(),
                    },
                )),
                ProviderState::Successful(items) => {
                    for item in items {
                        let match_rank = if item.item.label.starts_with(&self.prefix) {
                            0
                        } else if item
                            .item
                            .label
                            .get(..self.prefix.len())
                            .is_some_and(|start| start.eq_ignore_ascii_case(&self.prefix))
                        {
                            1
                        } else {
                            continue;
                        };
                        ranked.push((
                            match_rank,
                            provider.registration.id.value(),
                            item.local_order,
                            CompletionItem {
                                id: item.id,
                                label: item.item.label.clone(),
                                insert_text: item.item.insert_text.clone(),
                                provider_label: provider.registration.label.clone(),
                            },
                        ));
                    }
                }
            }
        }
        ranked.sort_by_key(|(match_rank, provider_order, local_order, _)| {
            (*match_rank, *provider_order, *local_order)
        });
        failures.sort_by_key(|(provider_order, _)| *provider_order);
        let mut insertion_texts = HashSet::new();
        self.snapshot = CompletionSnapshot {
            items: ranked
                .into_iter()
                .filter_map(|(_, _, _, item)| {
                    insertion_texts
                        .insert(item.insert_text.clone())
                        .then_some(item)
                })
                .collect::<Vec<_>>()
                .into(),
            pending_provider_count,
            failures: failures
                .into_iter()
                .map(|(_, failure)| failure)
                .collect::<Vec<_>>()
                .into(),
        };
    }
}

/// A `completion` contract provider a completion asks, labelled for its rows
/// and failures; registration order ranks its items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionProviderRegistration {
    pub id: ProviderRegistrationId,
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        CompletionController, CompletionItem, CompletionItemId, CompletionProviderRegistration,
        CompletionSnapshot, CompletionSurface,
    };
    use super::{CompletionRequest, CompletionResponse, CompletionResultItem};
    use knot_protocol::{ExtensionId, ExtensionLifecycleId, ProviderRegistrationId};

    fn provider(id: u64, label: &str) -> CompletionProviderRegistration {
        CompletionProviderRegistration {
            id: ProviderRegistrationId::new(id),
            extension: ExtensionId::new(id),
            lifecycle: ExtensionLifecycleId::new(1),
            label: label.into(),
        }
    }

    #[test]
    fn a_request_crosses_as_json_and_its_result_parses_into_items() {
        let request = CompletionRequest {
            registration: ProviderRegistrationId::new(4),
            revision: 2,
            cursor_byte_offset: 7,
            prefix: "fo".into(),
            generation: 3,
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request.argument("doc")).unwrap(),
            serde_json::json!({
                "document": "doc", "revision": 2, "cursorByteOffset": 7, "prefix": "fo",
            })
        );
        assert_eq!(
            request.response(Ok(
                r#"[{"label":"foo","insertText":"foo()","kind":1}]"#.into()
            )),
            CompletionResponse {
                registration: request.registration,
                revision: 2,
                generation: 3,
                result: Ok(vec![CompletionResultItem {
                    label: "foo".into(),
                    insert_text: "foo()".into(),
                }]),
            }
        );
        for invalid in [r#"[{"label":"foo"}]"#, r#""foo""#, "null"] {
            let message = request.response(Ok(invalid.into())).result.unwrap_err();
            assert!(message.contains("label, insertText"), "{message}");
        }
        assert_eq!(
            request.response(Err("broke".into())).result,
            Err("broke".into())
        );
    }

    #[test]
    fn merge_is_deterministic_across_arrival_order_and_deduplicates_insertion_text() {
        fn merged(reverse: bool) -> Vec<(String, String)> {
            let (first, second) = (provider(1, "first"), provider(2, "second"));
            let mut controller = CompletionController::new(
                4,
                3,
                0..3,
                "foo".into(),
                7,
                vec![first.clone(), second.clone()],
            );
            let responses = [
                CompletionResponse {
                    registration: first.id,
                    revision: 4,
                    generation: 7,
                    result: Ok(vec![
                        CompletionResultItem {
                            label: "fooOne".into(),
                            insert_text: "shared".into(),
                        },
                        CompletionResultItem {
                            label: "FooTwo".into(),
                            insert_text: "two".into(),
                        },
                    ]),
                },
                CompletionResponse {
                    registration: second.id,
                    revision: 4,
                    generation: 7,
                    result: Ok(vec![
                        CompletionResultItem {
                            label: "fooThree".into(),
                            insert_text: "three".into(),
                        },
                        CompletionResultItem {
                            label: "fooDuplicate".into(),
                            insert_text: "shared".into(),
                        },
                        CompletionResultItem {
                            label: "bar".into(),
                            insert_text: "excluded".into(),
                        },
                    ]),
                },
            ];
            for response in if reverse {
                responses.into_iter().rev().collect::<Vec<_>>()
            } else {
                responses.into_iter().collect()
            } {
                assert!(controller.apply_response(response));
            }
            controller
                .snapshot()
                .items
                .iter()
                .map(|item| (item.label.clone(), item.provider_label.clone()))
                .collect()
        }

        let expected = vec![
            ("fooOne".into(), "first".into()),
            ("fooThree".into(), "second".into()),
            ("FooTwo".into(), "first".into()),
        ];
        assert_eq!(merged(false), expected);
        assert_eq!(merged(true), expected);
    }

    #[test]
    fn stale_duplicate_and_failed_responses_are_isolated() {
        let (first, second) = (provider(1, "first"), provider(2, "second"));
        let mut controller = CompletionController::new(
            2,
            0,
            0..0,
            String::new(),
            3,
            vec![first.clone(), second.clone()],
        );

        assert!(!controller.apply_response(CompletionResponse {
            registration: first.id,
            revision: 1,
            generation: 3,
            result: Ok(Vec::new()),
        }));
        assert!(controller.apply_response(CompletionResponse {
            registration: first.id,
            revision: 2,
            generation: 3,
            result: Ok(vec![CompletionResultItem {
                label: "visible".into(),
                insert_text: "visible".into(),
            }]),
        }));
        assert!(!controller.apply_response(CompletionResponse {
            registration: first.id,
            revision: 2,
            generation: 3,
            result: Ok(Vec::new()),
        }));
        assert!(controller.apply_response(CompletionResponse {
            registration: second.id,
            revision: 2,
            generation: 3,
            result: Err("recoverable".into()),
        }));

        assert_eq!(controller.snapshot().items[0].label, "visible");
        assert_eq!(controller.snapshot().pending_provider_count, 0);
        assert_eq!(controller.snapshot().failures[0].provider_label, "second");
    }

    #[test]
    fn surface_updates_preserve_selection_while_replacement_resets_it() {
        fn snapshot(items: &[(u64, &str)]) -> CompletionSnapshot {
            CompletionSnapshot {
                items: items
                    .iter()
                    .map(|(id, label)| CompletionItem {
                        id: CompletionItemId(*id),
                        label: (*label).into(),
                        insert_text: (*label).into(),
                        provider_label: "fixture".into(),
                    })
                    .collect::<Vec<_>>()
                    .into(),
                pending_provider_count: 0,
                failures: Arc::from([]),
            }
        }

        let mut list = CompletionSurface::attach(snapshot(&[(1, "one"), (2, "two")]));
        list.move_selection(1);
        assert_eq!(list.selected_item(), Some(CompletionItemId(2)));
        let reordered = snapshot(&[(3, "three"), (2, "two"), (1, "one")]);
        list.update(reordered);
        assert_eq!(list.selected_item(), Some(CompletionItemId(2)));
    }
}
