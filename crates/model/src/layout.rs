//! Peers, the learned edge→peer layout, and messages between peers' engines.

use std::collections::BTreeMap;

use crate::{EdgeFraction, InputAction, MonitorId, MonitorSetKey, Side};

/// A paired peer, identified by its long-lived identity key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(pub [u8; 32]);

/// Which peer lies beyond each outer monitor side, for one monitor set.
/// Filled by layout learning (ADR 0005).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    slots: BTreeMap<(MonitorId, Side), PeerId>,
}

impl Layout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, monitor: MonitorId, side: Side, peer: PeerId) {
        self.slots.insert((monitor, side), peer);
    }

    pub fn clear(&mut self, monitor: &MonitorId, side: Side) {
        self.slots.remove(&(monitor.clone(), side));
    }

    pub fn peer(&self, monitor: &MonitorId, side: Side) -> Option<PeerId> {
        self.slots.get(&(monitor.clone(), side)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MonitorId, Side, PeerId)> {
        self.slots.iter().map(|((m, s), p)| (m, *s, *p))
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// True if `peer` is beyond any side.
    pub fn places(&self, peer: PeerId) -> bool {
        self.slots.values().any(|p| *p == peer)
    }

    /// Removes every slot on `side` that maps to `peer`; true if any did.
    pub fn forget(&mut self, side: Side, peer: PeerId) -> bool {
        let before = self.slots.len();
        self.slots.retain(|(_, s), p| !(*s == side && *p == peer));
        self.slots.len() != before
    }

    /// The peer mapped on the most slots on `side`; ties go to the lowest id.
    pub fn main_peer(&self, side: Side) -> Option<PeerId> {
        let mut counts: BTreeMap<PeerId, usize> = BTreeMap::new();
        for (_, s, p) in self.iter() {
            if s == side {
                *counts.entry(p).or_default() += 1;
            }
        }
        let best = counts.values().copied().max()?;
        counts.into_iter().find(|(_, n)| *n == best).map(|(p, _)| p)
    }
}

/// A machine's layouts, one per monitor set it has seen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutBook {
    sets: BTreeMap<MonitorSetKey, Layout>,
}

impl LayoutBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &MonitorSetKey) -> Option<&Layout> {
        self.sets.get(key)
    }

    /// Stores `layout` for `key`; an empty layout removes the entry.
    pub fn set(&mut self, key: MonitorSetKey, layout: Layout) {
        if layout.is_empty() {
            self.sets.remove(&key);
        } else {
            self.sets.insert(key, layout);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MonitorSetKey, &Layout)> {
        self.sets.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }
}

/// What one machine's engine tells another's. Encoded on the wire by
/// `add-input-forwarding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerMessage {
    /// You now have focus: place the cursor on your `side` at `fraction`.
    /// `from` is the machine whose edge the cursor left.
    FocusEnter {
        side: Side,
        fraction: EdgeFraction,
        from: PeerId,
    },
    /// Another peer already controls me; keep your focus.
    FocusRefused,
    /// The cursor left my `side` at `fraction` toward `target`.
    EdgeExit {
        target: PeerId,
        side: Side,
        fraction: EdgeFraction,
    },
    /// My own user took focus back from you.
    TakenBack,
    /// I no longer send you input; release what I held and take focus back.
    FocusWithdrawn,
    /// Remove the slots on your `side` that map to me.
    ForgetSide { side: Side },
    /// Input from the machine that has focus here.
    Input(InputAction),
    /// Show your secure attention (Ctrl-Alt-Del) screen.
    SecureAttention,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_per_monitor_side() {
        let (a, b) = (MonitorId("a".into()), MonitorId("b".into()));
        let peer = PeerId([7; 32]);
        let mut layout = Layout::new();
        layout.set(a.clone(), Side::Right, peer);
        assert_eq!(layout.peer(&a, Side::Right), Some(peer));
        assert_eq!(layout.peer(&a, Side::Left), None);
        assert_eq!(layout.peer(&b, Side::Right), None);
        layout.clear(&a, Side::Right);
        assert_eq!(layout.peer(&a, Side::Right), None);
    }

    #[test]
    fn forget_and_main_peer() {
        let (a, b) = (MonitorId("a".into()), MonitorId("b".into()));
        let (p, q) = (PeerId([1; 32]), PeerId([2; 32]));
        let mut layout = Layout::new();
        layout.set(a.clone(), Side::Right, q);
        layout.set(b.clone(), Side::Right, p);
        // One each: the lower id wins the tie.
        assert_eq!(layout.main_peer(Side::Right), Some(p));
        assert!(layout.places(q));
        assert!(layout.forget(Side::Right, q));
        assert!(!layout.places(q));
        assert!(!layout.forget(Side::Right, q));
        assert_eq!(layout.main_peer(Side::Left), None);
    }

    #[test]
    fn book_round_trips_and_drops_empty_layouts() {
        let key = MonitorSetKey(vec![MonitorId("a".into())]);
        let mut layout = Layout::new();
        layout.set(MonitorId("a".into()), Side::Left, PeerId([3; 32]));
        let mut book = LayoutBook::new();
        book.set(key.clone(), layout.clone());
        assert_eq!(book.get(&key), Some(&layout));
        book.set(key.clone(), Layout::new());
        assert_eq!(book.get(&key), None);
        assert!(book.is_empty());
    }
}
