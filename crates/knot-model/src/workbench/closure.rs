//! Protected closure decisions: which views a close covers, which dirty
//! documents it must ask about, and whether a planned close still holds when
//! it commits.
//!
//! The application collects the open views and the documents' state, calls
//! these functions, and runs the prompts, saves, and closes they imply.

use std::collections::{HashMap, HashSet};

use super::{Area, DocumentId, PaneId, PresentationId, TabId, TabSurfaceId};

/// What a close covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseKind {
    Tab,
    /// Every unpinned tab of the origin's pane except the origin.
    OtherTabs,
    /// Every tab of the origin's pane.
    Pane,
    /// Every tab of the session's other panes in the origin's area.
    OtherPanes,
    Window,
    Quit,
}

/// The identity of one open tab across the application's sessions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewKey {
    pub session: u64,
    pub workbench: u64,
    pub presentation: PresentationId,
    pub pane: PaneId,
    pub tab: TabId,
    pub surface: TabSurfaceId,
}

/// Where a close starts: its session and workbench, and the tab it was
/// captured on, which every kind but `Window` and `Quit` needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloseOrigin {
    pub session: u64,
    pub workbench: u64,
    pub view: Option<ViewKey>,
}

impl From<ViewKey> for CloseOrigin {
    fn from(view: ViewKey) -> Self {
        Self {
            session: view.session,
            workbench: view.workbench,
            view: Some(view),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClosureView {
    pub key: ViewKey,
    pub pinned: bool,
    /// The area of the view's pane.
    pub area: Area,
}

/// A document's state as a close sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentStatus {
    pub id: DocumentId,
    pub revision: u64,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalKind {
    Saved,
    Discarded,
}

/// The answer to a document's prompt, at the revision it applies to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Approval {
    pub document: DocumentId,
    pub revision: u64,
    pub kind: ApprovalKind,
}

/// The close names something that is gone or has changed since it was planned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTarget;

#[derive(Debug)]
pub struct ClosePlan {
    pub scope: Vec<ViewKey>,
    /// The dirty documents losing their final view, each with the first
    /// view of the scope that shows it.
    pub prompts: Vec<(DocumentId, ViewKey)>,
    /// Discards approved without asking, at the revision the plan saw.
    pub discarded: Vec<Approval>,
}

/// The views a close of `kind` from `origin` covers.
pub fn scope(kind: CloseKind, origin: &CloseOrigin, views: &[ClosureView]) -> Vec<ViewKey> {
    let view = origin.view;
    let origin_area = views
        .iter()
        .find(|candidate| Some(candidate.key) == view)
        .map(|candidate| candidate.area);
    let same_workbench = |candidate: &ClosureView| {
        candidate.key.session == origin.session && candidate.key.workbench == origin.workbench
    };
    views
        .iter()
        .filter(|candidate| match (kind, view) {
            (CloseKind::Tab, Some(view)) => candidate.key == view,
            (CloseKind::OtherTabs, Some(view)) => {
                same_workbench(candidate)
                    && candidate.key.pane == view.pane
                    && candidate.key.tab != view.tab
                    && !candidate.pinned
            }
            (CloseKind::Pane, Some(view)) => {
                same_workbench(candidate) && candidate.key.pane == view.pane
            }
            (CloseKind::OtherPanes, Some(view)) => {
                same_workbench(candidate)
                    && candidate.key.pane != view.pane
                    && Some(candidate.area) == origin_area
            }
            (CloseKind::Window, _) => candidate.key.session == origin.session,
            (CloseKind::Quit, _) => true,
            (
                CloseKind::Tab | CloseKind::OtherTabs | CloseKind::Pane | CloseKind::OtherPanes,
                None,
            ) => false,
        })
        .map(|candidate| candidate.key)
        .collect()
}

fn document_counts<'a>(keys: impl IntoIterator<Item = &'a ViewKey>) -> HashMap<DocumentId, usize> {
    let mut counts = HashMap::new();
    for key in keys {
        if let TabSurfaceId::Document(document) = key.surface {
            *counts.entry(document).or_insert(0usize) += 1;
        }
    }
    counts
}

/// Plan a close. `discard` approves the discard of every dirty document that
/// loses its final view instead of prompting.
pub fn plan(
    kind: CloseKind,
    origin: &CloseOrigin,
    discard: bool,
    views: &[ClosureView],
    documents: &[DocumentStatus],
) -> Result<ClosePlan, InvalidTarget> {
    let needs_view = !matches!(kind, CloseKind::Window | CloseKind::Quit);
    match origin.view {
        Some(origin) if !views.iter().any(|view| view.key == origin) => {
            return Err(InvalidTarget);
        }
        None if needs_view => return Err(InvalidTarget),
        _ => {}
    }
    let scope = scope(kind, origin, views);
    let total = document_counts(views.iter().map(|view| &view.key));
    let scoped = document_counts(&scope);
    let status = |id: DocumentId| documents.iter().find(|document| document.id == id);
    let mut prompted = HashSet::new();
    let mut prompts = Vec::new();
    for key in &scope {
        let TabSurfaceId::Document(document) = key.surface else {
            continue;
        };
        let loses_final_view =
            kind == CloseKind::Quit || scoped.get(&document) == total.get(&document);
        if loses_final_view
            && prompted.insert(document)
            && status(document).is_some_and(|d| d.dirty)
        {
            prompts.push((document, *key));
        }
    }
    if kind == CloseKind::Quit
        && documents
            .iter()
            .any(|document| document.dirty && !prompted.contains(&document.id))
    {
        return Err(InvalidTarget);
    }
    let discarded = if discard {
        prompts
            .drain(..)
            .map(|(document, _)| Approval {
                document,
                revision: status(document)
                    .expect("prompted document is live")
                    .revision,
                kind: ApprovalKind::Discarded,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(ClosePlan {
        scope,
        prompts,
        discarded,
    })
}

/// What a committing close does, once it still holds.
#[derive(Debug)]
pub struct Commit {
    /// The views to close.
    pub scope: Vec<ViewKey>,
    /// The documents whose discard was approved.
    pub discarded: Vec<DocumentId>,
    /// The documents losing their final view.
    pub final_documents: Vec<DocumentId>,
}

/// Check a planned close against the current views and documents. It holds
/// when the scope is unchanged, every approval still matches its document, and
/// no dirty document loses its final view unapproved (a hot exit keeps them in
/// recovery instead).
pub fn revalidate(
    kind: CloseKind,
    origin: &CloseOrigin,
    planned: &[ViewKey],
    hot_exit: bool,
    approvals: &[Approval],
    views: &[ClosureView],
    documents: &[DocumentStatus],
) -> Result<Commit, InvalidTarget> {
    let current = scope(kind, origin, views);
    if current.len() != planned.len() || current.iter().any(|key| !planned.contains(key)) {
        return Err(InvalidTarget);
    }
    for approval in approvals {
        let Some(document) = documents.iter().find(|d| d.id == approval.document) else {
            return Err(InvalidTarget);
        };
        if document.revision != approval.revision
            || (approval.kind == ApprovalKind::Saved && document.dirty)
        {
            return Err(InvalidTarget);
        }
    }
    let discarded = |id: DocumentId| {
        approvals
            .iter()
            .any(|approval| approval.document == id && approval.kind == ApprovalKind::Discarded)
    };
    let total = document_counts(views.iter().map(|view| &view.key));
    let scoped = document_counts(&current);
    let loses_final_view =
        |id: DocumentId| kind == CloseKind::Quit || scoped.get(&id) == total.get(&id);
    for document in documents {
        if loses_final_view(document.id) && document.dirty && !hot_exit && !discarded(document.id) {
            return Err(InvalidTarget);
        }
    }
    Ok(Commit {
        scope: current,
        discarded: documents
            .iter()
            .map(|document| document.id)
            .filter(|id| discarded(*id))
            .collect(),
        final_documents: scoped
            .iter()
            .filter(|(document, count)| total.get(document) == Some(count))
            .map(|(document, _)| *document)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: u64 = 1;

    fn key(pane: u64, tab: u64, document: u64) -> ViewKey {
        ViewKey {
            session: SESSION,
            workbench: 10,
            presentation: PresentationId(tab),
            pane: PaneId(pane),
            tab: TabId(tab),
            surface: TabSurfaceId::Document(DocumentId::new(document)),
        }
    }

    fn view(key: ViewKey) -> ClosureView {
        ClosureView {
            key,
            pinned: false,
            area: Area::Center,
        }
    }

    fn status(document: u64, dirty: bool) -> DocumentStatus {
        DocumentStatus {
            id: DocumentId::new(document),
            revision: 3,
            dirty,
        }
    }

    #[test]
    fn a_tab_close_prompts_only_when_it_loses_the_documents_final_view() {
        let (a, b) = (key(1, 1, 1), key(1, 2, 1));
        let views = [view(a), view(b)];
        let documents = [status(1, true)];
        let plan = plan(CloseKind::Tab, &a.into(), false, &views, &documents).unwrap();
        assert!(plan.prompts.is_empty());

        let plan = super::plan(CloseKind::Tab, &a.into(), false, &views[..1], &documents).unwrap();
        assert_eq!(plan.prompts, vec![(DocumentId::new(1), a)]);
    }

    #[test]
    fn discarding_replaces_prompts_with_approvals_at_the_planned_revision() {
        let a = key(1, 1, 1);
        let plan = plan(
            CloseKind::Tab,
            &a.into(),
            true,
            &[view(a)],
            &[status(1, true)],
        )
        .unwrap();
        assert!(plan.prompts.is_empty());
        assert_eq!(
            plan.discarded,
            vec![Approval {
                document: DocumentId::new(1),
                revision: 3,
                kind: ApprovalKind::Discarded,
            }]
        );
    }

    #[test]
    fn window_and_quit_closes_need_no_origin_tab() {
        let views = [view(key(1, 1, 1)), view(key(2, 2, 2))];
        let origin = CloseOrigin {
            session: SESSION,
            workbench: 10,
            view: None,
        };
        assert_eq!(scope(CloseKind::Window, &origin, &views).len(), 2);
        assert!(plan(CloseKind::Window, &origin, false, &views, &[]).is_ok());
        assert_eq!(
            plan(CloseKind::Tab, &origin, false, &views, &[]).unwrap_err(),
            InvalidTarget
        );
    }

    #[test]
    fn an_origin_outside_the_views_is_invalid() {
        let a = key(1, 1, 1);
        assert_eq!(
            plan(CloseKind::Tab, &a.into(), false, &[], &[]).unwrap_err(),
            InvalidTarget
        );
    }

    #[test]
    fn bulk_scopes_skip_pins_docks_and_other_windows() {
        let origin = key(1, 1, 1);
        let pinned = ClosureView {
            pinned: true,
            ..view(key(1, 2, 2))
        };
        let sibling = view(key(1, 3, 3));
        let other_pane = view(key(2, 4, 4));
        let dock = ClosureView {
            area: Area::Dock(super::super::DockLocation::Left),
            ..view(key(3, 5, 5))
        };
        let other_window = view(ViewKey {
            session: 2,
            ..key(1, 6, 6)
        });
        let views = [
            view(origin),
            pinned,
            sibling,
            other_pane,
            dock,
            other_window,
        ];
        let keys = |kind| scope(kind, &origin.into(), &views);
        assert_eq!(keys(CloseKind::OtherTabs), vec![sibling.key]);
        assert_eq!(keys(CloseKind::Pane), vec![origin, pinned.key, sibling.key]);
        assert_eq!(keys(CloseKind::OtherPanes), vec![other_pane.key]);
        assert_eq!(keys(CloseKind::Window).len(), 5);
        assert_eq!(keys(CloseKind::Quit).len(), 6);
    }

    #[test]
    fn quit_prompts_for_dirty_documents_and_rejects_unviewed_ones() {
        let a = key(1, 1, 1);
        let views = [view(a)];
        let plan = plan(
            CloseKind::Quit,
            &a.into(),
            false,
            &views,
            &[status(1, true)],
        )
        .unwrap();
        assert_eq!(plan.prompts.len(), 1);
        assert_eq!(
            super::plan(
                CloseKind::Quit,
                &a.into(),
                false,
                &views,
                &[status(1, false), status(2, true)]
            )
            .unwrap_err(),
            InvalidTarget
        );
    }

    #[test]
    fn a_commit_needs_the_planned_scope_and_matching_approvals() {
        let (a, b) = (key(1, 1, 1), key(1, 2, 2));
        let views = [view(a), view(b)];
        let approval = Approval {
            document: DocumentId::new(1),
            revision: 3,
            kind: ApprovalKind::Discarded,
        };
        let documents = [status(1, true), status(2, false)];
        let commit = revalidate(
            CloseKind::Window,
            &a.into(),
            &[a, b],
            false,
            &[approval],
            &views,
            &documents,
        )
        .unwrap();
        assert_eq!(commit.discarded, vec![DocumentId::new(1)]);
        assert_eq!(commit.final_documents.len(), 2);

        let check = |planned: &[ViewKey], approvals: &[Approval], documents: &[DocumentStatus]| {
            revalidate(
                CloseKind::Window,
                &a.into(),
                planned,
                false,
                approvals,
                &views,
                documents,
            )
            .is_err()
        };
        assert!(check(&[a], &[approval], &documents), "scope changed");
        assert!(check(&[a, b], &[], &documents), "dirty without approval");
        let edited = [
            DocumentStatus {
                revision: 4,
                ..documents[0]
            },
            documents[1],
        ];
        assert!(
            check(&[a, b], &[approval], &edited),
            "edited after approval"
        );
        let saved = Approval {
            kind: ApprovalKind::Saved,
            ..approval
        };
        assert!(check(&[a, b], &[saved], &documents), "dirty after save");
    }

    #[test]
    fn a_hot_exit_commits_dirty_documents_without_approval() {
        let a = key(1, 1, 1);
        assert!(
            revalidate(
                CloseKind::Quit,
                &a.into(),
                &[a],
                true,
                &[],
                &[view(a)],
                &[status(1, true)]
            )
            .is_ok()
        );
    }

    #[test]
    fn documents_with_surviving_views_are_not_final() {
        let (a, b) = (key(1, 1, 1), key(2, 2, 1));
        let views = [view(a), view(b)];
        let commit = revalidate(
            CloseKind::Tab,
            &a.into(),
            &[a],
            false,
            &[],
            &views,
            &[status(1, true)],
        )
        .unwrap();
        assert!(commit.final_documents.is_empty());
    }
}
