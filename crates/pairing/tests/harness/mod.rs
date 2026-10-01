//! In-memory pairing: frames pass between a joiner and an inviter through the
//! real `proto` encoding, with TLS values the test chooses.

#![allow(dead_code)]

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};

use model::{Millis, PeerId};
use pairing::{
    Binding, Code, ConnId, Identity, JoinFailure, Joiner, JoinerOutput, Machine, MachineName,
    ModeOutput, Now, PairingMode, RecordInbox, UnixTime,
};
use proto::CloseReason;
use proto::frame::{self, Decoded};
use proto::pairing::PairingMessage;
use rand_core::{CryptoRng, RngCore};

/// A deterministic generator. Tests only.
pub struct Rng(pub u64);

impl RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        rand_core::impls::fill_bytes_via_next(self, dest);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for Rng {}

pub fn rng(seed: u64) -> Rng {
    Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
}

pub fn machine(seed: u64, name: &str) -> Machine {
    Machine::new(
        Identity::generate(&mut rng(seed)),
        MachineName::new(name).unwrap(),
    )
}

/// A wall clock that moves forward one second every time it is read, so
/// each signed record is later than the one before.
pub fn unix() -> UnixTime {
    static CLOCK: AtomicU64 = AtomicU64::new(1_800_000_000);
    UnixTime(CLOCK.fetch_add(1, Ordering::Relaxed))
}

/// Both clocks, the monotonic one at `ms`.
pub fn at(ms: u64) -> Now {
    Now {
        mono: Millis(ms),
        unix: unix(),
    }
}

pub const INVITER_ADDR: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 137);
pub const CONN: ConnId = ConnId(1);

/// The bindings both sides see on a direct connection.
pub fn binding(joiner: &Machine, inviter: &Machine) -> Binding {
    Binding {
        exporter: [0x5a; 32],
        inviter: inviter.peer_id(),
        joiner: joiner.peer_id(),
    }
}

/// An inviter with pairing mode open at time 0.
pub fn open_mode(seed: u64) -> PairingMode {
    let mut mode = PairingMode::new();
    let out = mode.open(INVITER_ADDR, 24, Millis(0), &mut rng(seed));
    assert!(out.is_empty());
    mode
}

/// What the user typed: the shown code's text, parsed again.
pub fn typed(mode: &PairingMode) -> Code {
    Code::parse(&mode.code().unwrap().to_string()).unwrap()
}

/// Decodes one frame into its type and payload.
pub fn decode(bytes: &[u8]) -> (u16, Vec<u8>) {
    match frame::decode(bytes).unwrap() {
        Decoded::Frame { frame, consumed } => {
            assert_eq!(consumed, bytes.len());
            (frame.ty, frame.payload.to_vec())
        }
        Decoded::NeedMore => panic!("incomplete frame"),
    }
}

/// Everything that happened on one pairing connection.
#[derive(Debug, Default)]
pub struct Outcome {
    pub joiner: Vec<JoinerOutput>,
    pub inviter: Vec<ModeOutput>,
    /// Who closed the connection, and how.
    pub closed_by_joiner: Option<CloseReason>,
    pub closed_by_inviter: Option<CloseReason>,
}

impl Outcome {
    pub fn joiner_failure(&self) -> Option<JoinFailure> {
        self.joiner.iter().find_map(|o| match o {
            JoinerOutput::Failed(f) => Some(*f),
            _ => None,
        })
    }

    pub fn joiner_paired(&self) -> bool {
        self.joiner
            .iter()
            .any(|o| matches!(o, JoinerOutput::Paired { .. }))
    }

    pub fn inviter_paired(&self) -> bool {
        self.inviter
            .iter()
            .any(|o| matches!(o, ModeOutput::Paired { .. }))
    }
}

/// Runs one pairing connection to the end: the joiner types `code`, frames
/// pass both ways until neither side sends, and a close on either side is
/// reported to the other.
#[allow(clippy::too_many_arguments)]
pub fn pair(
    joiner: &mut Machine,
    inviter: &mut Machine,
    mode: &mut PairingMode,
    code: &Code,
    bind_joiner: Binding,
    bind_inviter: Binding,
    conn: ConnId,
    now: Millis,
) -> Outcome {
    let mut outcome = Outcome::default();
    let (mut j, first) = Joiner::start(code, bind_joiner, &mut rng(77));
    let mut to_inviter = Vec::new();
    let mut to_joiner = Vec::new();
    route_joiner(first, &mut to_inviter, &mut outcome);
    let mut inviter_rng = rng(78);
    loop {
        if outcome.closed_by_joiner.is_some() || outcome.closed_by_inviter.is_some() {
            break;
        }
        if let Some(bytes) = (!to_inviter.is_empty()).then(|| to_inviter.remove(0)) {
            let (ty, payload) = decode(&bytes);
            let message = PairingMessage::decode(ty, &payload).unwrap();
            let out = mode.on_message(
                conn,
                bind_inviter,
                &message,
                inviter,
                at(now.0),
                &mut inviter_rng,
            );
            route_inviter(out, conn, &mut to_joiner, &mut outcome);
        } else if let Some(bytes) = (!to_joiner.is_empty()).then(|| to_joiner.remove(0)) {
            let (ty, payload) = decode(&bytes);
            let message = PairingMessage::decode(ty, &payload).unwrap();
            let out = j.on_message(&message, joiner, unix());
            route_joiner(out, &mut to_inviter, &mut outcome);
        } else {
            break;
        }
    }
    if let Some(reason) = outcome.closed_by_inviter
        && let Some(f) = j.on_closed(Some(reason))
    {
        outcome.joiner.push(JoinerOutput::Failed(f));
    }
    if outcome.closed_by_joiner.is_some() {
        outcome.inviter.extend(mode.on_closed(conn));
    }
    outcome
}

fn route_joiner(out: Vec<JoinerOutput>, to_inviter: &mut Vec<Vec<u8>>, outcome: &mut Outcome) {
    for o in out {
        match o {
            JoinerOutput::Send(bytes) => to_inviter.push(bytes),
            JoinerOutput::Close(reason) => {
                outcome.closed_by_joiner = Some(reason);
                outcome.joiner.push(JoinerOutput::Close(reason));
            }
            other => outcome.joiner.push(other),
        }
    }
}

fn route_inviter(
    out: Vec<ModeOutput>,
    conn: ConnId,
    to_joiner: &mut Vec<Vec<u8>>,
    outcome: &mut Outcome,
) {
    for o in out {
        match o {
            ModeOutput::Send(c, bytes) if c == conn => to_joiner.push(bytes),
            ModeOutput::Close(c, reason) if c == conn => {
                outcome.closed_by_inviter = Some(reason);
                outcome.inviter.push(ModeOutput::Close(c, reason));
            }
            other => outcome.inviter.push(other),
        }
    }
}

/// Pairs `joiner` with `inviter` directly, opening pairing mode for it.
pub fn pair_now(joiner: &mut Machine, inviter: &mut Machine, seed: u64) -> Outcome {
    let mut mode = open_mode(seed);
    let code = typed(&mode);
    let b = binding(joiner, inviter);
    let outcome = pair(joiner, inviter, &mut mode, &code, b, b, CONN, Millis(0));
    assert!(
        outcome.joiner_paired() && outcome.inviter_paired(),
        "{outcome:?}"
    );
    outcome
}

/// A record exchange between two members on a new connection.
pub fn exchange(a: &mut Machine, b: &mut Machine) {
    let from_a = a.record_frames();
    let from_b = b.record_frames();
    deliver(&from_a, b);
    deliver(&from_b, a);
}

fn deliver(frames: &[Vec<u8>], to: &mut Machine) {
    let mut inbox = RecordInbox::new();
    for bytes in frames {
        let (ty, payload) = decode(bytes);
        let message = PairingMessage::decode(ty, &payload).unwrap();
        inbox.on_message(&message, to).unwrap();
    }
    assert!(inbox.is_done());
}

/// Whether `machine` counts each of `peers` as a member.
pub fn members(machine: &Machine, peers: &[&Machine]) -> Vec<bool> {
    peers
        .iter()
        .map(|p| machine.desk().is_member(&p.peer_id()))
        .collect()
}

pub fn id(m: &Machine) -> PeerId {
    m.peer_id()
}
