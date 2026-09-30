//! Replay tests for `openspec/specs/layout` and the `input-focus` walls
//! requirement it modifies. Each test is named after the spec scenario.

mod replay;

use engine::{Decision, Event, FocusView};
use model::{InputAction, Key, Layout, LayoutBook, MonitorId, PeerMessage, Point, Screen, Side};
use replay::{A, B, C, Desk, hd, key, monitor, motion};

const KEY_X: Key = Key(0x1b);
const RIGHT: (i32, i32) = (5, 0);
const LEFT: (i32, i32) = (-5, 0);
const UP: (i32, i32) = (0, -5);

fn id(s: &str) -> MonitorId {
    MonitorId(s.into())
}

fn layout(slots: &[(&str, Side, model::PeerId)]) -> Layout {
    let mut l = Layout::new();
    for (m, side, p) in slots {
        l.set(id(m), *side, *p);
    }
    l
}

/// A and B, connected, with nothing learned.
fn fresh() -> Desk {
    Desk::new(&[(A, hd("A1")), (B, hd("B1"))])
}

fn offered_undo(out: &replay::Outcome, machine: model::PeerId) -> bool {
    out.of(machine)
        .iter()
        .any(|d| matches!(d, Decision::OfferUndo { .. }))
}

/// Learns A's right side toward B with a 20-unit push ending at t = 30.
fn learn_a_right(desk: &mut Desk) -> replay::Outcome {
    let mut outs = desk.at(0).push(A, RIGHT, (1919, 540), 4);
    assert_eq!(desk.focus(A), FocusView::Forwarding(B));
    outs.pop().unwrap()
}

// ---- Learning an edge by pushing through it ----

#[test]
fn first_push_reaches_the_only_unplaced_peer() {
    let mut desk = fresh();
    let outs = desk.at(0).push(A, RIGHT, (1919, 540), 5);
    assert_eq!(desk.layout_of(A).peer(&id("A1"), Side::Right), Some(B));
    assert_eq!(desk.focus(A), FocusView::Forwarding(B));
    // The fourth push (20 units) learned and crossed.
    assert!(offered_undo(&outs[3], A));
    assert!(outs[..3].iter().all(|o| o.is_quiet()));
}

#[test]
fn a_brush_against_the_edge_learns_nothing() {
    let mut desk = fresh();
    desk.at(0).push(A, (4, 0), (1919, 540), 2);
    assert!(desk.layout_of(A).is_empty());
    assert_eq!(desk.focus(A), FocusView::Local);
}

#[test]
fn instant_sensitivity_still_needs_a_push_to_learn() {
    let mut desk = fresh();
    let out = desk.physical(A, motion(5, 0, 1919, 540));
    assert!(out.is_quiet(), "{out:?}");
    assert!(desk.layout_of(A).is_empty());
    assert_eq!(desk.focus(A), FocusView::Local);
}

// ---- Learning fills the whole machine side ----

#[test]
fn two_monitors_stacked() {
    let stacked = Screen::new(vec![
        monitor("top", 0, 0, 1920, 1080, 96),
        monitor("bottom", 0, 1080, 1920, 1080, 96),
    ]);
    let mut desk = Desk::new(&[(A, stacked), (B, hd("B1"))]);
    desk.at(0).push(A, RIGHT, (1919, 500), 4);
    let l = desk.layout_of(A);
    assert_eq!(l.peer(&id("top"), Side::Right), Some(B));
    assert_eq!(l.peer(&id("bottom"), Side::Right), Some(B));
}

#[test]
fn taller_monitor_beside_a_shorter_one() {
    let screen = Screen::new(vec![
        monitor("tall", 0, 0, 1920, 1440, 96),
        monitor("short", 1920, 360, 1920, 1080, 96),
    ]);
    let mut desk = Desk::new(&[(A, screen), (B, hd("B1"))]);
    desk.at(0).push(A, RIGHT, (3839, 800), 4);
    let l = desk.layout_of(A);
    assert_eq!(l.peer(&id("short"), Side::Right), Some(B));
    assert_eq!(l.peer(&id("tall"), Side::Right), Some(B));
}

// ---- No guess without a single candidate ----

#[test]
fn every_peer_already_placed() {
    let mut desk = fresh();
    desk.layout(A, &[("A1", Side::Right, B)]);
    desk.at(0).push(A, LEFT, (0, 540), 5);
    assert_eq!(desk.layout_of(A).peer(&id("A1"), Side::Left), None);
    assert_eq!(desk.focus(A), FocusView::Local);
}

#[test]
fn two_unplaced_peers() {
    let mut desk = Desk::new(&[(A, hd("A1")), (B, hd("B1")), (C, hd("C1"))]);
    let outs = desk.at(0).push(A, RIGHT, (1919, 540), 5);
    assert!(outs.iter().all(|o| o.is_quiet()));
    assert!(desk.layout_of(A).is_empty());
    assert_eq!(desk.focus(A), FocusView::Local);
}

#[test]
fn unmapped_side_with_nothing_to_learn() {
    let mut desk = fresh();
    desk.layout(A, &[("A1", Side::Right, B)]);
    let outs = desk.at(0).push(A, UP, (900, 0), 5);
    assert!(outs.iter().all(|o| o.is_quiet()));
    assert_eq!(desk.focus(A), FocusView::Local);
}

// ---- Learning the reverse edge ----

#[test]
fn both_sides_from_one_push() {
    let mut desk = fresh();
    let out = learn_a_right(&mut desk);
    assert_eq!(desk.layout_of(B).peer(&id("B1"), Side::Left), Some(A));
    // A reverse edge is not a guess: no Undo on B.
    assert!(!offered_undo(&out, B));
}

#[test]
fn reverse_edge_through_a_middle_machine() {
    let mut desk = Desk::new(&[(A, hd("A1")), (B, hd("B1")), (C, hd("C1"))]);
    desk.layout(A, &[("A1", Side::Right, B)]);
    desk.layout(B, &[("B1", Side::Left, A), ("B1", Side::Right, C)]);
    desk.physical(A, motion(5, 0, 1919, 540));
    desk.injected(B, motion(5, 0, 1919, 540));
    assert_eq!(desk.focus(C), FocusView::Controlled(A));
    assert_eq!(desk.layout_of(C).peer(&id("C1"), Side::Left), Some(B));
}

#[test]
fn existing_side_is_kept() {
    let mut desk = Desk::new(&[(A, hd("A1")), (B, hd("B1")), (C, hd("C1"))]);
    desk.layout(A, &[("A1", Side::Right, B)]);
    desk.layout(B, &[("B1", Side::Left, C)]);
    desk.physical(A, motion(5, 0, 1919, 540));
    assert_eq!(desk.focus(B), FocusView::Controlled(A));
    let l = desk.layout_of(B);
    assert_eq!(l.peer(&id("B1"), Side::Left), Some(C));
    assert!(!l.places(A));
}

// ---- Undo a learned edge ----

#[test]
fn undo_right_after_learning() {
    let mut desk = fresh();
    learn_a_right(&mut desk);
    let out = desk.event(
        A,
        Event::Undo {
            at: model::Millis(3030),
        },
    );
    assert_eq!(desk.layout_of(A).peer(&id("A1"), Side::Right), None);
    assert!(!desk.layout_of(B).places(A));
    assert_eq!(desk.focus(A), FocusView::Local);
    assert_eq!(desk.focus(B), FocusView::Local);
    assert!(
        out.of(A)
            .contains(&Decision::WarpCursor(Point::new(1919, 540)))
    );
}

#[test]
fn undo_too_late() {
    let mut desk = fresh();
    learn_a_right(&mut desk);
    let out = desk.event(
        A,
        Event::Undo {
            at: model::Millis(11_030),
        },
    );
    assert!(out.is_quiet(), "{out:?}");
    assert_eq!(desk.layout_of(A).peer(&id("A1"), Side::Right), Some(B));
    assert_eq!(desk.focus(A), FocusView::Forwarding(B));
}

// ---- Forget layout ----

#[test]
fn relearn_after_forgetting() {
    let mut desk = fresh();
    desk.layout(A, &[("A1", Side::Right, B)]);
    let out = desk.event(A, Event::ForgetLayout);
    assert_eq!(out.of(A), [Decision::LayoutsChanged(LayoutBook::new())]);
    desk.at(0).push(A, LEFT, (0, 540), 5);
    let l = desk.layout_of(A);
    assert_eq!(l.peer(&id("A1"), Side::Left), Some(B));
    assert_eq!(l.peer(&id("A1"), Side::Right), None);
}

// ---- Layouts per monitor set ----

fn docked() -> Screen {
    Screen::new(vec![
        monitor("L", 0, 0, 1920, 1080, 96),
        monitor("E", 1920, 0, 2560, 1440, 96),
    ])
}

#[test]
fn docking_a_laptop() {
    let mut desk = Desk::new(&[(A, hd("L")), (B, hd("B1"))]);
    desk.layout(A, &[("L", Side::Right, B)]);
    let out = desk.event(A, Event::ScreenChanged(docked()));
    let l = desk.layout_of(A);
    assert_eq!(l.peer(&id("E"), Side::Right), Some(B));
    // The laptop's right side now faces the external monitor.
    assert_eq!(l.peer(&id("L"), Side::Right), None);
    assert!(matches!(out.of(A), [Decision::LayoutsChanged(_)]));
}

#[test]
fn undocking_returns_the_old_layout() {
    let mut desk = Desk::new(&[(A, hd("L")), (B, hd("B1"))]);
    desk.layout(A, &[("L", Side::Right, B)]);
    desk.event(A, Event::ScreenChanged(docked()));
    desk.layout(A, &[("E", Side::Top, B)]);
    desk.event(A, Event::ScreenChanged(hd("L")));
    assert_eq!(desk.layout_of(A), layout(&[("L", Side::Right, B)]));
}

// ---- Layout changes are reported ----

#[test]
fn learning_is_reported() {
    let mut desk = fresh();
    let out = learn_a_right(&mut desk);
    let book = out
        .of(A)
        .iter()
        .find_map(|d| match d {
            Decision::LayoutsChanged(book) => Some(book.clone()),
            _ => None,
        })
        .expect("layouts reported");
    let key = hd("A1").set_key();
    assert_eq!(book.get(&key), Some(&layout(&[("A1", Side::Right, B)])));
}

#[test]
fn loaded_book_crosses_without_learning() {
    let mut desk = fresh();
    let mut book = LayoutBook::new();
    book.set(hd("A1").set_key(), layout(&[("A1", Side::Right, B)]));
    desk.event(A, Event::LayoutsLoaded(book));
    let out = desk.physical(A, motion(5, 0, 1919, 540));
    assert_eq!(desk.focus(A), FocusView::Forwarding(B));
    assert!(!offered_undo(&out, A));
}

// ---- Messages learning relies on ----

#[test]
fn focus_withdrawn_releases_the_controlled_machine() {
    let mut desk = fresh();
    desk.layout(A, &[("A1", Side::Right, B)]);
    desk.physical(A, motion(5, 0, 1919, 540));
    desk.physical(A, key(KEY_X, true));
    let out = desk.event(
        B,
        Event::FromPeer {
            peer: A,
            msg: PeerMessage::FocusWithdrawn,
        },
    );
    assert_eq!(
        out.of(B)[0],
        Decision::Inject(InputAction::Key {
            key: KEY_X,
            down: false
        })
    );
    assert_eq!(desk.focus(B), FocusView::Local);
}
