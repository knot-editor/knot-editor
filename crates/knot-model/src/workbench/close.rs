use super::{PaneId, TabId, TabSurfaceId, TerminalSessionId, Workbench};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentCloseDisposition {
    Retained,
    CloseRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloseTransition {
    pub closed_tab: TabId,
    pub removed_pane: Option<PaneId>,
    pub focused_pane: Option<PaneId>,
    pub document: Option<DocumentCloseDisposition>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PendingClose {
    pane_id: PaneId,
    tab_id: TabId,
    document_id: super::DocumentId,
}

impl PendingClose {
    #[cfg(test)]
    pub fn document_id(&self) -> super::DocumentId {
        self.document_id
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum CloseRequestOutcome {
    Closed(CloseTransition),
    Pending(PendingClose),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseConfirmation {
    Close,
    Cancel,
}

impl Workbench {
    /// Request closure without presenting UI or performing persistence.
    ///
    /// `open_view_count` is application-wide because another native window may
    /// show the same document. A dirty document needs confirmation only when
    /// this tab is its final view.
    pub fn request_close_tab(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        document_id: super::DocumentId,
        document_is_dirty: bool,
        open_view_count: usize,
    ) -> Option<CloseRequestOutcome> {
        assert!(open_view_count > 0, "the closing tab is an open view");
        if !self.contains_tab(pane_id, tab_id, document_id) {
            return None;
        }
        assert!(
            open_view_count >= self.view_count(document_id),
            "application view count must include every view in this workbench"
        );
        if open_view_count == 1 && document_is_dirty {
            return Some(CloseRequestOutcome::Pending(PendingClose {
                pane_id,
                tab_id,
                document_id,
            }));
        }
        Some(CloseRequestOutcome::Closed(self.close_tab_now(
            pane_id,
            tab_id,
            Some(open_view_count),
        )))
    }

    pub fn request_close_terminal_tab(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        session_id: TerminalSessionId,
    ) -> Option<CloseTransition> {
        self.contains_surface(pane_id, tab_id, TabSurfaceId::Terminal(session_id))
            .then(|| self.close_tab_now(pane_id, tab_id, None))
    }

    /// Complete or cancel a previously pending final-view close.
    ///
    /// The caller resolves Save/Don't Save into `Close` only after persistence
    /// or discard has succeeded. Stale requests are rejected rather than
    /// closing a replacement tab.
    pub fn resolve_pending_close(
        &mut self,
        pending: PendingClose,
        confirmation: CloseConfirmation,
        open_view_count: usize,
    ) -> Option<CloseTransition> {
        if confirmation == CloseConfirmation::Cancel {
            return None;
        }
        assert!(open_view_count > 0, "the closing tab is an open view");
        if !self.contains_tab(pending.pane_id, pending.tab_id, pending.document_id) {
            return None;
        }
        assert!(
            open_view_count >= self.view_count(pending.document_id),
            "application view count must include every view in this workbench"
        );
        Some(self.close_tab_now(pending.pane_id, pending.tab_id, Some(open_view_count)))
    }

    fn close_tab_now(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        open_view_count: Option<usize>,
    ) -> CloseTransition {
        let removed = self
            .remove_tab(pane_id, tab_id)
            .expect("validated tab must remain present through synchronous close");
        debug_assert_eq!(removed.tab.id, tab_id);
        CloseTransition {
            closed_tab: tab_id,
            removed_pane: removed.removed_pane,
            focused_pane: self.focused_pane,
            document: open_view_count.map(|open_view_count| {
                if open_view_count == 1 {
                    DocumentCloseDisposition::CloseRequested
                } else {
                    DocumentCloseDisposition::Retained
                }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{DocumentId, SplitDirection, SplitPlacement};

    #[test]
    fn terminal_close_never_requests_document_disposition() {
        let document = DocumentId::new(1);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let document_tab = workbench.focused_pane().unwrap().active_tab_id();
            let session = TerminalSessionId(7);
            let terminal_tab = workbench.open_terminal_tab(pane, session).unwrap();
            assert!(
                workbench
                    .request_close_tab(pane, terminal_tab, document, true, 1)
                    .is_none()
            );
            assert!(
                workbench
                    .request_close_terminal_tab(pane, terminal_tab, TerminalSessionId(8))
                    .is_none()
            );
            assert_eq!(workbench.view_count(document), 1);
            let transition = workbench
                .request_close_terminal_tab(pane, terminal_tab, session)
                .unwrap();
            assert_eq!(transition.document, None);
            assert_eq!(
                workbench.focused_pane().unwrap().active_tab_id(),
                document_tab
            );
            assert_eq!(workbench.view_count(document), 1);
            assert!(matches!(
                workbench.request_close_tab(pane, document_tab, document, true, 1),
                Some(CloseRequestOutcome::Pending(_))
            ));
        }
    }

    #[test]
    fn closing_a_tab_selects_an_ordered_neighbor() {
        let first_document = DocumentId::new(2);
        let second_document = DocumentId::new(3);
        let workbench = &mut Workbench::new_for_document(first_document);
        let pane = workbench.focused_pane_id().unwrap();
        let first_tab = workbench.focused_pane().unwrap().active_tab_id();
        let second_tab = workbench
            .open_tab_for_document(pane, second_document)
            .unwrap();

        let transition = match workbench
            .request_close_tab(pane, second_tab, second_document, false, 1)
            .unwrap()
        {
            CloseRequestOutcome::Closed(transition) => transition,
            CloseRequestOutcome::Pending(_) => panic!("clean document should close"),
        };
        assert_eq!(
            transition.document,
            Some(DocumentCloseDisposition::CloseRequested)
        );
        assert_eq!(transition.removed_pane, None);
        assert_eq!(workbench.focused_pane().unwrap().active_tab_id(), first_tab);
    }

    #[test]
    fn closing_a_panes_last_tab_collapses_its_split() {
        let document = DocumentId::new(4);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let remaining_pane = workbench.focused_pane_id().unwrap();
            let closing_pane = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let closing_tab = workbench.focused_pane().unwrap().active_tab_id();
            let transition = match workbench
                .request_close_tab(closing_pane, closing_tab, document, false, 2)
                .unwrap()
            {
                CloseRequestOutcome::Closed(transition) => transition,
                CloseRequestOutcome::Pending(_) => panic!("another view remains"),
            };

            assert_eq!(transition.removed_pane, Some(closing_pane));
            assert_eq!(transition.focused_pane, Some(remaining_pane));
            assert_eq!(
                transition.document,
                Some(DocumentCloseDisposition::Retained)
            );
            assert_eq!(
                workbench.center(),
                Some(&super::super::WorkbenchLayout::Pane(remaining_pane))
            );
            assert_eq!(workbench.panes().len(), 1);
        }
    }

    #[test]
    fn dirty_final_view_waits_for_confirmation_and_last_tab_can_empty_workbench() {
        let document = DocumentId::new(5);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let tab = workbench.focused_pane().unwrap().active_tab_id();
            let pending = match workbench
                .request_close_tab(pane, tab, document, true, 1)
                .unwrap()
            {
                CloseRequestOutcome::Pending(pending) => pending,
                CloseRequestOutcome::Closed(_) => panic!("dirty final view must wait"),
            };

            assert_eq!(pending.document_id(), document);
            assert_eq!(
                workbench.resolve_pending_close(pending, CloseConfirmation::Cancel, 1),
                None
            );

            let pending = match workbench
                .request_close_tab(pane, tab, document, true, 1)
                .unwrap()
            {
                CloseRequestOutcome::Pending(pending) => pending,
                CloseRequestOutcome::Closed(_) => unreachable!(),
            };
            let transition = workbench
                .resolve_pending_close(pending, CloseConfirmation::Close, 1)
                .unwrap();
            assert_eq!(transition.focused_pane, None);
            assert_eq!(transition.removed_pane, Some(pane));
            assert_eq!(
                transition.document,
                Some(DocumentCloseDisposition::CloseRequested)
            );
            assert!(workbench.center().is_none());
            assert!(workbench.panes().is_empty());
        }
    }

    #[test]
    fn dirty_document_closes_without_confirmation_when_another_view_remains() {
        let document = DocumentId::new(6);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let first_pane = workbench.focused_pane_id().unwrap();
            let first_tab = workbench.focused_pane().unwrap().active_tab_id();
            workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::After)
                .unwrap();

            let outcome = workbench
                .request_close_tab(first_pane, first_tab, document, true, 2)
                .unwrap();
            assert!(matches!(
                outcome,
                CloseRequestOutcome::Closed(CloseTransition {
                    document: Some(DocumentCloseDisposition::Retained),
                    ..
                })
            ));
            assert_eq!(workbench.view_count(document), 1);
        }
    }

    #[test]
    fn stale_pending_close_cannot_close_a_different_tab() {
        let document = DocumentId::new(7);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let tab = workbench.focused_pane().unwrap().active_tab_id();
            let pending = match workbench
                .request_close_tab(pane, tab, document, true, 1)
                .unwrap()
            {
                CloseRequestOutcome::Pending(pending) => pending,
                CloseRequestOutcome::Closed(_) => unreachable!(),
            };
            assert!(workbench.remove_tab(pane, tab).is_some());
            assert_eq!(
                workbench.resolve_pending_close(pending, CloseConfirmation::Close, 1),
                None
            );
        }
    }

    #[test]
    fn pending_close_rechecks_application_view_count() {
        let document = DocumentId::new(8);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let tab = workbench.focused_pane().unwrap().active_tab_id();
            let pending = match workbench
                .request_close_tab(pane, tab, document, true, 1)
                .unwrap()
            {
                CloseRequestOutcome::Pending(pending) => pending,
                CloseRequestOutcome::Closed(_) => unreachable!(),
            };
            workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();

            let transition = workbench
                .resolve_pending_close(pending, CloseConfirmation::Close, 2)
                .unwrap();
            assert_eq!(
                transition.document,
                Some(DocumentCloseDisposition::Retained)
            );
            assert_eq!(workbench.view_count(document), 1);
        }
    }
}
