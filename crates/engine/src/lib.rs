//! Pure state machine: focus owner, edge transitions, routing, layout learning.
//!
//! Depends only on `model`, `proto`, and `policy`. Never on `win32-*`,
//! `transport`, or `ipc`. Must build and test on Linux. Tests are replay tests:
//! a sequence of `model` events in, routing decisions out.
//!
//! # How it is driven
//!
//! Every machine runs one [`Engine`]. The host feeds it [`Event`]s: local input
//! from the agent (with a monotonic timestamp and its [`Origin`]), messages
//! from peers' engines, peers connecting and disconnecting, and changes to the
//! screen, layout, or [`Config`]. [`Engine::handle`] returns the [`Decision`]s
//! that follow. The engine never reads a clock, starts a thread, or does I/O:
//! the same events always produce the same decisions.
//!
//! # Focus states
//!
//! | State | Meaning | [`CaptureMode`] |
//! |---|---|---|
//! | `Local` | This machine's input stays here. | `PassAll` |
//! | `Forwarding(peer)` | This machine's input goes to `peer`; the cursor waits at its exit point. | `WithholdAll` |
//! | `Controlled(peer)` | `peer`'s input is injected here; local mouse input is withheld while the takeover rules run. | `WithholdMouse` |
//!
//! The agent's hook reads the capture mode as a flag; it never waits for the
//! engine (hook procedures may only enqueue).
//!
//! # Tuning
//!
//! Every tunable number is in [`Tuning::DEFAULT`]:
//!
//! | Value | Default | Rule |
//! |---|---|---|
//! | `corner_tenths_mm` | 20 (2 mm) | Corner zones at both ends of each monitor side never cross. |
//! | `push_units` | 20 | Outward raw movement needed to cross with [`EdgeSensitivity::Push`]. |
//! | `push_pause_ms` | 250 | A pause longer than this resets the push count. |
//! | `burst_gap_ms` | 150 | Local movement after a longer stillness starts a new burst. |
//! | `takeover_ms` | 2000 | A second burst within this long after the first, or one burst this long, takes control back. |

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use model::geometry::corner_zone_px;
use model::{
    Button, EdgeFraction, InputAction, InputEvent, Key, Layout, Millis, MonitorId, Origin, PeerId,
    PeerMessage, Point, Screen, Side,
};

/// Every tunable number the engine uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    pub corner_tenths_mm: u32,
    pub push_units: i32,
    pub push_pause_ms: u64,
    pub burst_gap_ms: u64,
    pub takeover_ms: u64,
}

impl Tuning {
    pub const DEFAULT: Tuning = Tuning {
        corner_tenths_mm: 20,
        push_units: 20,
        push_pause_ms: 250,
        burst_gap_ms: 150,
        takeover_ms: 2000,
    };
}

/// How readily the cursor crosses a mapped edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeSensitivity {
    /// Cross on the first outward push.
    #[default]
    Instant,
    /// Cross only after [`Tuning::push_units`] of outward movement.
    Push,
}

/// User settings the engine reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Config {
    pub sensitivity: EdgeSensitivity,
}

/// Which local input the agent's hook must keep from this machine's OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    /// Local focus: withhold nothing.
    PassAll,
    /// Forwarding: withhold all physical input; it goes to the peer.
    WithholdAll,
    /// Controlled: withhold physical mouse input; keys pass through.
    WithholdMouse,
}

/// Where focus is, as seen from outside the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusView {
    Local,
    Forwarding(PeerId),
    Controlled(PeerId),
}

/// Something that happened, fed to [`Engine::handle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Local input as the agent saw it.
    Local {
        at: Millis,
        input: InputEvent,
        origin: Origin,
    },
    /// A message from a peer's engine.
    FromPeer {
        peer: PeerId,
        msg: PeerMessage,
    },
    PeerConnected(PeerId),
    PeerLost(PeerId),
    ScreenChanged(Screen),
    LayoutChanged(Layout),
    ConfigChanged(Config),
}

/// What the host must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Publish a new capture mode to the agent's hook.
    SetCapture(CaptureMode),
    /// Send a message to a peer's engine.
    Send { peer: PeerId, msg: PeerMessage },
    /// Inject input into this machine's OS.
    Inject(InputAction),
    /// Move this machine's cursor.
    WarpCursor(Point),
    /// Ask this machine to show its secure attention (Ctrl-Alt-Del) screen.
    RequestSecureAttention,
    /// Focus moved; for the UI and diagnostics.
    FocusChanged(FocusView),
}

/// Keys and buttons held down somewhere as a result of one input stream.
#[derive(Debug, Clone, Default)]
struct Held {
    keys: BTreeSet<Key>,
    buttons: BTreeSet<Button>,
}

impl Held {
    /// Records `a`; returns false for a release with no matching press.
    fn note(&mut self, a: InputAction) -> bool {
        match a {
            InputAction::Key { key, down: true } => {
                self.keys.insert(key);
                true
            }
            InputAction::Key { key, down: false } => self.keys.remove(&key),
            InputAction::Button { button, down: true } => {
                self.buttons.insert(button);
                true
            }
            InputAction::Button {
                button,
                down: false,
            } => self.buttons.remove(&button),
            InputAction::Wheel { .. } | InputAction::Motion { .. } => true,
        }
    }

    /// Releases for everything held, emptying the set.
    fn releases(&mut self) -> impl Iterator<Item = InputAction> + use<> {
        let keys = std::mem::take(&mut self.keys)
            .into_iter()
            .map(|key| InputAction::Key { key, down: false });
        let buttons =
            std::mem::take(&mut self.buttons)
                .into_iter()
                .map(|button| InputAction::Button {
                    button,
                    down: false,
                });
        keys.chain(buttons)
    }

    fn clear(&mut self) {
        self.keys.clear();
        self.buttons.clear();
    }
}

/// Local mouse activity on a controlled machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Contest {
    /// Last local mouse activity.
    last: Option<Millis>,
    /// Start of the current burst.
    burst_start: Millis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Local,
    Forwarding { to: PeerId, exit: Point },
    Controlled { by: PeerId, contest: Contest },
}

/// Outward movement accumulated against one edge, for [`EdgeSensitivity::Push`].
#[derive(Debug, Clone)]
struct Push {
    monitor: MonitorId,
    side: Side,
    total: i32,
    last: Millis,
}

/// A crossing the rules allow: toward `peer` through `side` at `fraction`.
struct Crossing {
    peer: PeerId,
    side: Side,
    fraction: EdgeFraction,
}

/// Focus and routing for one machine.
#[derive(Debug, Clone)]
pub struct Engine {
    local: PeerId,
    config: Config,
    tuning: Tuning,
    screen: Screen,
    layout: Layout,
    connected: BTreeSet<PeerId>,
    focus: Focus,
    /// Physical keys currently down on this machine's keyboard.
    physical_keys: BTreeSet<Key>,
    /// Held on this machine's OS by local physical input while focus was local.
    pressed_local: Held,
    /// Forwarded as pressed to the peer this machine forwards to.
    forwarded: Held,
    /// Injected as pressed here on the controlling peer's behalf.
    injected: Held,
    push: Option<Push>,
    out: Vec<Decision>,
}

impl Engine {
    /// An engine for the machine whose identity is `local`.
    pub fn new(local: PeerId, config: Config) -> Self {
        Self {
            local,
            config,
            tuning: Tuning::DEFAULT,
            screen: Screen::default(),
            layout: Layout::default(),
            connected: BTreeSet::new(),
            focus: Focus::Local,
            physical_keys: BTreeSet::new(),
            pressed_local: Held::default(),
            forwarded: Held::default(),
            injected: Held::default(),
            push: None,
            out: Vec::new(),
        }
    }

    /// Where focus is now.
    pub fn focus(&self) -> FocusView {
        match self.focus {
            Focus::Local => FocusView::Local,
            Focus::Forwarding { to, .. } => FocusView::Forwarding(to),
            Focus::Controlled { by, .. } => FocusView::Controlled(by),
        }
    }

    /// Handles one event and returns the decisions it leads to, in order.
    pub fn handle(&mut self, event: Event) -> Vec<Decision> {
        match event {
            Event::Local { at, input, origin } => self.on_local(at, input, origin),
            Event::FromPeer { peer, msg } => self.on_peer(peer, msg),
            Event::PeerConnected(peer) => {
                self.connected.insert(peer);
            }
            Event::PeerLost(peer) => self.on_peer_lost(peer),
            Event::ScreenChanged(screen) => {
                self.screen = screen;
                self.push = None;
            }
            Event::LayoutChanged(layout) => {
                self.layout = layout;
                self.push = None;
            }
            Event::ConfigChanged(config) => {
                self.config = config;
                self.push = None;
            }
        }
        std::mem::take(&mut self.out)
    }

    // ---- local input ----

    fn on_local(&mut self, at: Millis, input: InputEvent, origin: Origin) {
        if origin == Origin::Physical
            && let InputEvent::Key { key, down } = input
        {
            if down {
                self.physical_keys.insert(key);
            } else {
                self.physical_keys.remove(&key);
            }
        }
        match (self.focus, origin) {
            (Focus::Local, Origin::Physical) => self.local_input(at, input),
            (Focus::Forwarding { to, .. }, Origin::Physical) => self.forward(to, input),
            (Focus::Controlled { .. }, Origin::Physical) => self.contested_input(at, input),
            (Focus::Controlled { by, .. }, Origin::Injected) => {
                if let InputEvent::Motion { dx, dy, cursor } = input {
                    self.controlled_motion(at, by, dx, dy, cursor);
                }
            }
            // Injected input never crosses, forwards, or takes over.
            (Focus::Local | Focus::Forwarding { .. }, Origin::Injected) => {}
        }
    }

    /// Physical input while focus is local.
    fn local_input(&mut self, at: Millis, input: InputEvent) {
        match input {
            InputEvent::Key { key, down } => {
                self.pressed_local.note(InputAction::Key { key, down });
            }
            InputEvent::Button { button, down } => {
                self.pressed_local
                    .note(InputAction::Button { button, down });
            }
            InputEvent::Wheel { .. } => {}
            InputEvent::Motion { dx, dy, cursor } => {
                let button_held = !self.pressed_local.buttons.is_empty();
                if let Some(c) = self.crossing(at, cursor, dx, dy, button_held) {
                    let releases: Vec<_> = self.pressed_local.releases().collect();
                    self.out.extend(releases.into_iter().map(Decision::Inject));
                    self.forwarded.clear();
                    self.send(
                        c.peer,
                        PeerMessage::FocusEnter {
                            side: c.side.opposite(),
                            fraction: c.fraction,
                        },
                    );
                    self.set_focus(
                        Focus::Forwarding {
                            to: c.peer,
                            exit: cursor,
                        },
                        None,
                    );
                }
            }
        }
    }

    /// Physical input while forwarding to `to`.
    fn forward(&mut self, to: PeerId, input: InputEvent) {
        let action = match input {
            InputEvent::Key {
                key: Key::END,
                down: true,
            } if self.secure_attention_chord() => {
                self.send(to, PeerMessage::SecureAttention);
                return;
            }
            InputEvent::Key { key, down } => InputAction::Key { key, down },
            InputEvent::Button { button, down } => InputAction::Button { button, down },
            InputEvent::Wheel { dx, dy } => InputAction::Wheel { dx, dy },
            InputEvent::Motion { dx, dy, .. } => InputAction::Motion { dx, dy },
        };
        if self.forwarded.note(action) {
            self.send(to, PeerMessage::Input(action));
        }
    }

    fn secure_attention_chord(&self) -> bool {
        self.physical_keys.iter().any(|k| k.is_ctrl())
            && self.physical_keys.iter().any(|k| k.is_alt())
    }

    /// Physical input while a peer controls this machine.
    fn contested_input(&mut self, at: Millis, input: InputEvent) {
        match input {
            InputEvent::Key { key, down } => {
                self.pressed_local.note(InputAction::Key { key, down });
                if down {
                    self.take_back();
                }
            }
            InputEvent::Button { .. } | InputEvent::Wheel { .. } | InputEvent::Motion { .. } => {
                self.local_mouse_activity(at);
            }
        }
    }

    /// Groups local mouse activity into bursts and takes focus back on a
    /// second burst soon after the first, or on one long burst.
    fn local_mouse_activity(&mut self, at: Millis) {
        let t = self.tuning;
        let Focus::Controlled { contest, .. } = &mut self.focus else {
            return;
        };
        let take = match contest.last {
            Some(last) if at.since(last) <= t.burst_gap_ms => {
                at.since(contest.burst_start) >= t.takeover_ms
            }
            Some(last) if at.since(last) <= t.takeover_ms => true,
            Some(_) | None => {
                contest.burst_start = at;
                false
            }
        };
        contest.last = Some(at);
        if take {
            self.take_back();
        }
    }

    /// This machine's own user takes focus back from its controller.
    fn take_back(&mut self) {
        let Focus::Controlled { by, .. } = self.focus else {
            return;
        };
        let releases: Vec<_> = self.injected.releases().collect();
        self.out.extend(releases.into_iter().map(Decision::Inject));
        self.send(by, PeerMessage::TakenBack);
        self.set_focus(Focus::Local, None);
    }

    /// Injected motion while controlled by `by`: this machine's own edges.
    fn controlled_motion(&mut self, at: Millis, by: PeerId, dx: i32, dy: i32, cursor: Point) {
        let button_held = !self.injected.buttons.is_empty();
        if let Some(c) = self.crossing(at, cursor, dx, dy, button_held) {
            let releases: Vec<_> = self.injected.releases().collect();
            self.out.extend(releases.into_iter().map(Decision::Inject));
            self.send(
                by,
                PeerMessage::EdgeExit {
                    target: c.peer,
                    side: c.side,
                    fraction: c.fraction,
                },
            );
            self.set_focus(Focus::Local, None);
        }
    }

    /// Whether a movement to `cursor` by `(dx, dy)` crosses an edge now.
    fn crossing(
        &mut self,
        at: Millis,
        cursor: Point,
        dx: i32,
        dy: i32,
        button_held: bool,
    ) -> Option<Crossing> {
        let candidate = [Side::Left, Side::Right, Side::Top, Side::Bottom]
            .into_iter()
            .find_map(|side| {
                let amount = side.outward(dx, dy);
                if amount <= 0 || button_held {
                    return None;
                }
                let m = self.screen.outer_edge(cursor, side)?;
                let zone = corner_zone_px(m.dpi, self.tuning.corner_tenths_mm);
                if m.in_corner(cursor, side, zone) {
                    return None;
                }
                let peer = self.layout.peer(&m.id, side)?;
                self.connected
                    .contains(&peer)
                    .then(|| (m.id.clone(), side, amount, peer))
            });
        let Some((monitor, side, amount, peer)) = candidate else {
            self.push = None;
            return None;
        };

        if self.config.sensitivity == EdgeSensitivity::Push {
            let total = match &self.push {
                Some(p)
                    if p.monitor == monitor
                        && p.side == side
                        && at.since(p.last) <= self.tuning.push_pause_ms =>
                {
                    p.total.saturating_add(amount)
                }
                _ => amount,
            };
            if total < self.tuning.push_units {
                self.push = Some(Push {
                    monitor,
                    side,
                    total,
                    last: at,
                });
                return None;
            }
        }
        self.push = None;
        Some(Crossing {
            peer,
            side,
            fraction: self.screen.fraction_at(side, cursor),
        })
    }

    // ---- peers ----

    fn on_peer(&mut self, peer: PeerId, msg: PeerMessage) {
        match (self.focus, msg) {
            (Focus::Local, PeerMessage::FocusEnter { side, fraction }) => {
                self.injected.clear();
                let warp = self.screen.entry_point(side, fraction);
                self.set_focus(
                    Focus::Controlled {
                        by: peer,
                        contest: Contest::default(),
                    },
                    warp,
                );
            }
            (_, PeerMessage::FocusEnter { .. }) => self.send(peer, PeerMessage::FocusRefused),

            (
                Focus::Forwarding { to, exit },
                PeerMessage::FocusRefused | PeerMessage::TakenBack,
            ) if to == peer => {
                self.forwarded.clear();
                self.set_focus(Focus::Local, Some(exit));
            }
            (
                Focus::Forwarding { to, exit },
                PeerMessage::EdgeExit {
                    target,
                    side,
                    fraction,
                },
            ) if to == peer => self.follow_exit(peer, exit, target, side, fraction),

            (Focus::Controlled { by, .. }, PeerMessage::Input(action)) if by == peer => {
                if self.injected.note(action) {
                    self.out.push(Decision::Inject(action));
                }
            }
            (Focus::Controlled { by, .. }, PeerMessage::SecureAttention) if by == peer => {
                self.out.push(Decision::RequestSecureAttention);
            }
            // Input from a peer that does not control us, or stale messages.
            _ => {}
        }
    }

    /// The machine this one forwards to reported the cursor leaving it.
    fn follow_exit(
        &mut self,
        from: PeerId,
        exit: Point,
        target: PeerId,
        side: Side,
        fraction: EdgeFraction,
    ) {
        // `from` released everything it held for us when the cursor left.
        self.forwarded.clear();
        if target == self.local {
            let warp = self.screen.entry_point(side.opposite(), fraction);
            self.set_focus(Focus::Local, warp);
        } else if self.connected.contains(&target) {
            self.send(
                target,
                PeerMessage::FocusEnter {
                    side: side.opposite(),
                    fraction,
                },
            );
            self.set_focus(Focus::Forwarding { to: target, exit }, None);
        } else {
            // Unreachable from here: put the cursor back where it left `from`.
            self.send(from, PeerMessage::FocusEnter { side, fraction });
        }
    }

    fn on_peer_lost(&mut self, peer: PeerId) {
        self.connected.remove(&peer);
        self.push = None;
        match self.focus {
            Focus::Forwarding { to, exit } if to == peer => {
                self.forwarded.clear();
                self.set_focus(Focus::Local, Some(exit));
            }
            Focus::Controlled { by, .. } if by == peer => {
                let releases: Vec<_> = self.injected.releases().collect();
                self.out.extend(releases.into_iter().map(Decision::Inject));
                self.set_focus(Focus::Local, None);
            }
            _ => {}
        }
    }

    // ---- output ----

    fn send(&mut self, peer: PeerId, msg: PeerMessage) {
        self.out.push(Decision::Send { peer, msg });
    }

    fn set_focus(&mut self, focus: Focus, warp: Option<Point>) {
        let before = (self.capture(), self.focus());
        self.focus = focus;
        self.push = None;
        if let Some(p) = warp {
            self.out.push(Decision::WarpCursor(p));
        }
        if self.capture() != before.0 {
            self.out.push(Decision::SetCapture(self.capture()));
        }
        if self.focus() != before.1 {
            self.out.push(Decision::FocusChanged(self.focus()));
        }
    }

    fn capture(&self) -> CaptureMode {
        match self.focus {
            Focus::Local => CaptureMode::PassAll,
            Focus::Forwarding { .. } => CaptureMode::WithholdAll,
            Focus::Controlled { .. } => CaptureMode::WithholdMouse,
        }
    }
}
