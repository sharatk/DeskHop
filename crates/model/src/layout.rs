//! Peers, the learned edge→peer layout, and messages between peers' engines.

use std::collections::BTreeMap;

use crate::{EdgeFraction, InputAction, MonitorId, Side};

/// A paired peer, identified by its long-lived identity key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(pub [u8; 32]);

/// Which peer lies beyond each outer monitor side, for the current monitor
/// set. Filled by layout learning (ADR 0005).
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
}

/// What one machine's engine tells another's. Encoded on the wire by
/// `add-input-forwarding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerMessage {
    /// You now have focus: place the cursor on your `side` at `fraction`.
    FocusEnter { side: Side, fraction: EdgeFraction },
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
}
