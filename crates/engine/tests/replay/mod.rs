//! Replay harness: several machines' engines on one desk. Events go into one
//! engine; every `Send` it decides is delivered to the addressed engine, and
//! so on until the desk is quiet. The outcome is each machine's decisions.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use engine::{Config, Decision, Engine, Event, FocusView};
use model::{
    Button, InputEvent, Key, Layout, Millis, Monitor, MonitorId, Origin, PeerId, Point, Rect,
    Screen, Side,
};

pub const A: PeerId = PeerId([0xa; 32]);
pub const B: PeerId = PeerId([0xb; 32]);
pub const C: PeerId = PeerId([0xc; 32]);

pub fn monitor(id: &str, x: i32, y: i32, w: i32, h: i32, dpi: u32) -> Monitor {
    Monitor {
        id: MonitorId(id.into()),
        rect: Rect::new(x, y, w, h),
        dpi,
    }
}

/// One 1920×1080 monitor at 96 DPI.
pub fn hd(id: &str) -> Screen {
    Screen::new(vec![monitor(id, 0, 0, 1920, 1080, 96)])
}

pub fn motion(dx: i32, dy: i32, x: i32, y: i32) -> InputEvent {
    InputEvent::Motion {
        dx,
        dy,
        cursor: Point::new(x, y),
    }
}

pub fn key(key: Key, down: bool) -> InputEvent {
    InputEvent::Key { key, down }
}

pub fn button(button: Button, down: bool) -> InputEvent {
    InputEvent::Button { button, down }
}

/// Every decision each machine made in response to one event.
#[derive(Debug, Default)]
pub struct Outcome(BTreeMap<PeerId, Vec<Decision>>);

impl Outcome {
    pub fn of(&self, machine: PeerId) -> &[Decision] {
        self.0.get(&machine).map_or(&[], Vec::as_slice)
    }

    pub fn is_quiet(&self) -> bool {
        self.0.values().all(Vec::is_empty)
    }
}

pub struct Desk {
    engines: BTreeMap<PeerId, Engine>,
    down: BTreeSet<(PeerId, PeerId)>,
    now: Millis,
}

impl Desk {
    /// Machines with the given screens, all connected to each other.
    pub fn new(machines: &[(PeerId, Screen)]) -> Self {
        let mut engines = BTreeMap::new();
        for (id, screen) in machines {
            let mut e = Engine::new(*id, Config::default());
            e.handle(Event::ScreenChanged(screen.clone()));
            for (other, _) in machines {
                if other != id {
                    e.handle(Event::PeerConnected(*other));
                }
            }
            engines.insert(*id, e);
        }
        Self {
            engines,
            down: BTreeSet::new(),
            now: Millis(0),
        }
    }

    /// Maps monitor sides on `machine` to peers.
    pub fn layout(&mut self, machine: PeerId, slots: &[(&str, Side, PeerId)]) -> &mut Self {
        let mut layout = Layout::new();
        for (id, side, peer) in slots {
            layout.set(MonitorId((*id).into()), *side, *peer);
        }
        self.event(machine, Event::LayoutChanged(layout));
        self
    }

    pub fn config(&mut self, machine: PeerId, config: Config) -> &mut Self {
        self.event(machine, Event::ConfigChanged(config));
        self
    }

    /// Sets the time for the events that follow.
    pub fn at(&mut self, ms: u64) -> &mut Self {
        self.now = Millis(ms);
        self
    }

    pub fn physical(&mut self, machine: PeerId, input: InputEvent) -> Outcome {
        let at = self.now;
        self.event(
            machine,
            Event::Local {
                at,
                input,
                origin: Origin::Physical,
            },
        )
    }

    pub fn injected(&mut self, machine: PeerId, input: InputEvent) -> Outcome {
        let at = self.now;
        self.event(
            machine,
            Event::Local {
                at,
                input,
                origin: Origin::Injected,
            },
        )
    }

    /// `machine` loses its connection to `peer`. Nothing is delivered between
    /// them afterwards; `peer` is not told.
    pub fn lose(&mut self, machine: PeerId, peer: PeerId) -> Outcome {
        self.down.insert((machine, peer));
        self.down.insert((peer, machine));
        self.event(machine, Event::PeerLost(peer))
    }

    pub fn focus(&self, machine: PeerId) -> FocusView {
        self.engines[&machine].focus()
    }

    /// Feeds `event` to `machine` and delivers every resulting message.
    pub fn event(&mut self, machine: PeerId, event: Event) -> Outcome {
        let mut outcome = Outcome::default();
        let mut queue = VecDeque::from([(machine, event)]);
        while let Some((to, event)) = queue.pop_front() {
            let decisions = self
                .engines
                .get_mut(&to)
                .expect("machine on the desk")
                .handle(event);
            for d in &decisions {
                if let Decision::Send { peer, msg } = *d
                    && !self.down.contains(&(to, peer))
                    && self.engines.contains_key(&peer)
                {
                    queue.push_back((peer, Event::FromPeer { peer: to, msg }));
                }
            }
            outcome.0.entry(to).or_default().extend(decisions);
        }
        outcome
    }
}
