use super::*;
use crate::workbench::{DocumentId, TabSurfaceId, TerminalSessionId};

fn fixture() -> (Workbench, DocumentId) {
    let document = DocumentId::new(1);
    (Workbench::new_for_document(document), document)
}

fn destination(anchor: TabMoveCapture, placement: TabMovePlacement) -> TabMoveDestination {
    TabMoveDestination::At { anchor, placement }
}

fn active_presentation(workbench: &Workbench, pane: PaneId) -> PresentationId {
    workbench.pane(pane).unwrap().active_tab().presentation()
}

#[test]
fn every_same_pane_insertion_preserves_presentation_and_order() {
    for source_index in 0..4 {
        for slot in 0..=4 {
            let (mut workbench, document) = fixture();
            let pane = workbench.focused_pane_id().unwrap();
            for _ in 1..4 {
                workbench.open_tab_for_document(pane, document).unwrap();
            }
            let old = workbench.tab_snapshots();
            let source = workbench
                .capture_tab_move(pane, old[source_index].tab_id)
                .unwrap();
            let placement = if slot == 4 {
                TabMovePlacement::End
            } else {
                TabMovePlacement::Before
            };
            let anchor = workbench
                .capture_tab_move(pane, old[slot.min(3)].tab_id)
                .unwrap();
            let moved = workbench
                .relocate_tab(source, destination(anchor, placement))
                .unwrap();
            let mut expected = old.clone();
            let moving = expected.remove(source_index);
            expected.insert(slot - usize::from(source_index < slot), moving);
            assert_eq!(workbench.tab_snapshots(), expected);
            assert_eq!(moved.destination, source.tab);
            assert_eq!(workbench.focused_pane_id(), Some(pane));
            assert_eq!(
                workbench.pane(pane).unwrap().active_tab_id(),
                source.tab.tab_id
            );
            assert_eq!(active_presentation(&workbench, pane), source.presentation);
            assert_eq!(workbench.view_count(document), 4);
            workbench.debug_assert_invariants();
        }
    }
}

#[test]
fn all_edges_move_the_tab_and_order_the_new_pane() {
    for edge in [
        SplitEdge::Left,
        SplitEdge::Right,
        SplitEdge::Top,
        SplitEdge::Bottom,
    ] {
        let (mut workbench, document) = fixture();
        let pane = workbench.focused_pane_id().unwrap();
        let source = workbench
            .capture_tab_move(pane, workbench.pane(pane).unwrap().active_tab_id())
            .unwrap();
        let neighbor = workbench.open_tab_for_document(pane, document).unwrap();
        let anchor = workbench.capture_tab_move(pane, neighbor).unwrap();
        let moved = workbench
            .relocate_tab(source, destination(anchor, TabMovePlacement::Split(edge)))
            .unwrap();
        let new_pane = moved.destination.pane_id;
        let (direction, placement) = edge.split();
        let (first, second) = if placement == SplitPlacement::Before {
            (new_pane, pane)
        } else {
            (pane, new_pane)
        };
        assert_eq!(
            workbench.center(),
            Some(&WorkbenchLayout::Split {
                direction,
                ratio: 0.5,
                first: Box::new(WorkbenchLayout::Pane(first)),
                second: Box::new(WorkbenchLayout::Pane(second)),
            })
        );
        assert_eq!(workbench.pane(pane).unwrap().active_tab_id(), neighbor);
        assert_eq!(workbench.focused_pane_id(), Some(new_pane));
        assert_eq!(
            active_presentation(&workbench, new_pane),
            source.presentation
        );
        assert_eq!(workbench.view_count(document), 2);
        assert!(!workbench.contains_tab_move(source));
    }
}

#[test]
fn single_tab_edge_move_collapses_the_original_pane() {
    let (mut workbench, document) = fixture();
    let old = workbench.tab_snapshots()[0];
    let source = workbench.capture_tab_move(old.pane_id, old.tab_id).unwrap();
    let moved = workbench
        .relocate_tab(
            source,
            destination(source, TabMovePlacement::Split(SplitEdge::Left)),
        )
        .unwrap();
    assert_eq!(moved.removed_pane, Some(old.pane_id));
    assert_eq!(
        workbench.center(),
        Some(&WorkbenchLayout::Pane(moved.destination.pane_id))
    );
    assert_eq!(workbench.panes.len(), 1);
    assert_eq!(workbench.view_count(document), 1);
}

#[test]
fn moving_between_panes_collapses_a_nested_branch() {
    let (mut workbench, document) = fixture();
    let first = workbench.focused_pane_id().unwrap();
    let second = workbench
        .split_pane(first, SplitDirection::Horizontal, SplitPlacement::After)
        .unwrap();
    let third = workbench
        .split_pane(second, SplitDirection::Vertical, SplitPlacement::After)
        .unwrap();
    let source = workbench
        .capture_tab_move(second, workbench.pane(second).unwrap().active_tab_id())
        .unwrap();
    let anchor = workbench
        .capture_tab_move(first, workbench.pane(first).unwrap().active_tab_id())
        .unwrap();
    let moved = workbench
        .relocate_tab(source, destination(anchor, TabMovePlacement::Before))
        .unwrap();
    assert_eq!(moved.removed_pane, Some(second));
    assert_eq!(
        workbench.center(),
        Some(&WorkbenchLayout::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(WorkbenchLayout::Pane(first)),
            second: Box::new(WorkbenchLayout::Pane(third)),
        })
    );
    assert_eq!(workbench.pane(first).unwrap().tabs[0].id, source.tab.tab_id);
    assert_eq!(workbench.view_count(document), 3);
}

#[test]
fn cross_workbench_transfer_allocates_destination_ids() {
    let (mut source, document) = fixture();
    let mut destination = Workbench::new_for_document(document);
    let captured = source.tab_snapshots()[0];
    let capture = source
        .capture_tab_move(captured.pane_id, captured.tab_id)
        .unwrap();
    let anchor = destination.tab_snapshots()[0];
    let anchor = destination
        .capture_tab_move(anchor.pane_id, anchor.tab_id)
        .unwrap();
    let anchor_presentation = anchor.presentation;
    let moved = source
        .transfer_tab_to(
            &mut destination,
            capture,
            self::destination(anchor, TabMovePlacement::End),
        )
        .unwrap();
    assert_eq!(moved.destination.tab_id, TabId(2));
    assert!(source.center().is_none());
    assert!(!source.contains_tab_move(capture));
    let arrived = destination.focused_pane().unwrap().active_tab();
    assert_eq!(arrived.id(), moved.destination.tab_id);
    assert_ne!(arrived.presentation(), anchor_presentation);
    assert_eq!(source.view_count(document), 0);
    assert_eq!(destination.view_count(document), 2);
}

#[test]
fn failed_or_stale_transfers_leave_both_workbenches_intact() {
    let (mut source, document) = fixture();
    let mut destination = Workbench::new_for_document(document);
    let source_snapshot = source.tab_snapshots()[0];
    let capture = source
        .capture_tab_move(source_snapshot.pane_id, source_snapshot.tab_id)
        .unwrap();
    let anchor_snapshot = destination.tab_snapshots()[0];
    let anchor = destination
        .capture_tab_move(anchor_snapshot.pane_id, anchor_snapshot.tab_id)
        .unwrap();
    let before = (source.tab_snapshots(), destination.tab_snapshots());
    assert!(
        source
            .transfer_tab_to(&mut destination, capture, TabMoveDestination::Empty)
            .is_none()
    );
    // Showing the same document anew makes the captured anchor stale.
    destination.replace_tab_document(anchor_snapshot.pane_id, anchor_snapshot.tab_id, document);
    assert!(
        source
            .transfer_tab_to(
                &mut destination,
                capture,
                self::destination(anchor, TabMovePlacement::End),
            )
            .is_none()
    );
    let anchor = destination
        .capture_tab_move(anchor_snapshot.pane_id, anchor_snapshot.tab_id)
        .unwrap();
    source.replace_tab_document(source_snapshot.pane_id, source_snapshot.tab_id, document);
    assert!(
        source
            .transfer_tab_to(
                &mut destination,
                capture,
                self::destination(anchor, TabMovePlacement::End),
            )
            .is_none()
    );
    assert_eq!(
        (source.tab_snapshots(), destination.tab_snapshots()),
        before
    );
    assert_eq!(source.next_tab_id, 2);
    assert_eq!(destination.next_tab_id, 2);
}

#[test]
fn terminal_transfer_keeps_session_and_rejects_duplicate_destination() {
    let (mut source, document) = fixture();
    let pane = source.focused_pane_id().unwrap();
    let session = TerminalSessionId(41);
    let tab = source.open_terminal_tab(pane, session).unwrap();
    let capture = source.capture_tab_move(pane, tab).unwrap();
    let mut destination = Workbench::empty();
    let moved = source
        .transfer_tab_to(&mut destination, capture, TabMoveDestination::Empty)
        .unwrap();
    assert_eq!(moved.destination.surface, TabSurfaceId::Terminal(session));
    assert_eq!(source.view_count(document), 1);
    assert_eq!(destination.view_count(document), 0);
    let tab = source.open_terminal_tab(pane, session).unwrap();
    let capture = source.capture_tab_move(pane, tab).unwrap();
    let anchor = destination.tab_snapshots()[0];
    let anchor = destination
        .capture_tab_move(anchor.pane_id, anchor.tab_id)
        .unwrap();
    let before = source.tab_snapshots();
    assert!(
        source
            .transfer_tab_to(
                &mut destination,
                capture,
                self::destination(anchor, TabMovePlacement::End),
            )
            .is_none()
    );
    assert_eq!(source.tab_snapshots(), before);
}
