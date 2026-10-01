//! The pairing exchange and record exchange as sans-I/O state machines
//! (design D1, D6).
//!
//! Callers decode frames with `proto` and pass each pairing message in, with
//! the time on a monotonic clock. Outputs are encoded frames to send and
//! close reasons. Nothing here waits: each machine reports its next deadline,
//! and the caller calls back at that time.

use model::{Millis, PeerId};
use proto::CloseReason;
use proto::pairing::{PairConfirm, PairPake, PairingMessage};
use rand_core::{CryptoRng, RngCore};

use crate::code::Code;
use crate::confirm::{Binding, Pake, Tags, tags_match};
use crate::desk::{Applied, MAX_SUBJECTS, Machine, UnixTime};
use crate::record::{MachineName, SignedRecord};

/// How long pairing mode stays open.
pub const WINDOW_MS: u64 = 10 * 60 * 1000;

/// How long one attempt may take, from the joiner's `PairPake` to the end of
/// the record exchange.
pub const ATTEMPT_MS: u64 = 30 * 1000;

/// Failed attempts that close pairing mode. With a 6-digit secret, that is 3
/// guesses in a million each time the user opens it.
pub const MAX_FAILURES: u8 = 3;

/// The time on both of the caller's clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    /// Monotonic, for the pairing window and attempt limit.
    pub mono: Millis,
    /// Wall clock, for the epoch of the record pairing signs.
    pub unix: UnixTime,
}

/// A peer broke the pairing or record-exchange order. The connection closes
/// with [`CloseReason::ProtocolError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// A message the current step does not expect.
    OutOfOrder,
    /// More than [`MAX_SUBJECTS`] records before `RecordsDone`.
    TooManyRecords,
    /// A `PairPake` that is not a valid SPAKE2 message.
    BadPake,
}

fn frame(message: &PairingMessage<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    // Pairing messages are at most 201 bytes, far under the frame limit.
    let _ = message.encode_frame(&mut out);
    out
}

/// The receiving side of one record exchange: records are held until the
/// peer's `RecordsDone`, then applied together. A record after that is a
/// single update, applied at once.
#[derive(Debug, Default)]
pub struct RecordInbox {
    pending: Vec<SignedRecord>,
    done: bool,
}

impl RecordInbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles a `MemberRecord` or `RecordsDone`. Returns what changed when
    /// records were applied.
    pub fn on_message(
        &mut self,
        message: &PairingMessage<'_>,
        machine: &mut Machine,
    ) -> Result<Option<Applied>, Violation> {
        match message {
            PairingMessage::Record(wire) => {
                let record = SignedRecord::from_wire(wire);
                if self.done {
                    return Ok(Some(machine.apply([record])));
                }
                if self.pending.len() >= MAX_SUBJECTS {
                    return Err(Violation::TooManyRecords);
                }
                self.pending.push(record);
                Ok(None)
            }
            PairingMessage::RecordsDone if !self.done => {
                self.done = true;
                Ok(Some(machine.apply(std::mem::take(&mut self.pending))))
            }
            _ => Err(Violation::OutOfOrder),
        }
    }

    /// Whether the peer's `RecordsDone` has arrived.
    pub fn is_done(&self) -> bool {
        self.done
    }
}

/// What the joiner should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinerOutput {
    /// Send this frame on the pairing connection.
    Send(Vec<u8>),
    /// Close the connection with this reason.
    Close(CloseReason),
    /// Pairing completed: the inviter is now a member.
    Paired { peer: PeerId, name: MachineName },
    /// Pairing failed; tell the user why.
    Failed(JoinFailure),
}

/// Why pairing failed, as the joiner reports it to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinFailure {
    /// The code was wrong, or something relayed the connection.
    WrongCode,
    /// The other machine was not in pairing mode, or was busy.
    NotReady,
    /// The other machine broke the protocol.
    ProtocolError,
    /// This machine's desk is full.
    DeskFull,
    /// The connection closed before pairing completed.
    ConnectionLost,
}

enum JoinerState {
    AwaitPake(Pake),
    AwaitConfirm(Tags),
    Records {
        peer: PeerId,
        name: MachineName,
        inbox: RecordInbox,
    },
    Finished,
}

/// The machine where the code was typed.
pub struct Joiner {
    state: JoinerState,
    binding: Binding,
}

impl Joiner {
    /// Starts pairing with `code` on a connection whose TLS values are
    /// `binding`. Sends `PairPake`.
    pub fn start<R: CryptoRng + RngCore>(
        code: &Code,
        binding: Binding,
        rng: &mut R,
    ) -> (Self, Vec<JoinerOutput>) {
        let (pake, message) = Pake::start(code, rng);
        let joiner = Self {
            state: JoinerState::AwaitPake(pake),
            binding,
        };
        let out = vec![JoinerOutput::Send(frame(&PairingMessage::Pake(PairPake(
            message,
        ))))];
        (joiner, out)
    }

    /// Handles one pairing message from the inviter. `now` dates the record
    /// that adds the inviter.
    pub fn on_message(
        &mut self,
        message: &PairingMessage<'_>,
        machine: &mut Machine,
        now: UnixTime,
    ) -> Vec<JoinerOutput> {
        let state = std::mem::replace(&mut self.state, JoinerState::Finished);
        match (state, message) {
            (JoinerState::AwaitPake(pake), PairingMessage::Pake(theirs)) => {
                let Some(tags) = pake.finish(&theirs.0, &self.binding) else {
                    return fail_joiner(CloseReason::ProtocolError, JoinFailure::ProtocolError);
                };
                let confirm = PairConfirm {
                    tag: tags.joiner,
                    name: machine.name().wire(),
                };
                self.state = JoinerState::AwaitConfirm(tags);
                vec![JoinerOutput::Send(frame(&PairingMessage::Confirm(confirm)))]
            }
            (JoinerState::AwaitConfirm(tags), PairingMessage::Confirm(confirm)) => {
                if !tags_match(&confirm.tag, &tags.inviter) {
                    return fail_joiner(CloseReason::WrongPairingCode, JoinFailure::WrongCode);
                }
                let peer = self.binding.inviter;
                let name = MachineName::from(confirm.name);
                if machine.add(peer, name.clone(), now).is_err() {
                    return fail_joiner(CloseReason::Normal, JoinFailure::DeskFull);
                }
                self.state = JoinerState::Records {
                    peer,
                    name,
                    inbox: RecordInbox::new(),
                };
                machine
                    .record_frames()
                    .into_iter()
                    .map(JoinerOutput::Send)
                    .collect()
            }
            (
                JoinerState::Records {
                    peer,
                    name,
                    mut inbox,
                },
                m,
            ) => match inbox.on_message(m, machine) {
                Err(_) => fail_joiner(CloseReason::ProtocolError, JoinFailure::ProtocolError),
                Ok(_) if inbox.is_done() => vec![
                    JoinerOutput::Close(CloseReason::Normal),
                    JoinerOutput::Paired { peer, name },
                ],
                Ok(_) => {
                    self.state = JoinerState::Records { peer, name, inbox };
                    Vec::new()
                }
            },
            (JoinerState::Finished, _) => Vec::new(),
            _ => fail_joiner(CloseReason::ProtocolError, JoinFailure::ProtocolError),
        }
    }

    /// The connection closed with `reason` (`None` if it dropped without
    /// one). Reports a failure unless pairing already ended.
    pub fn on_closed(&mut self, reason: Option<CloseReason>) -> Option<JoinFailure> {
        if matches!(self.state, JoinerState::Finished) {
            return None;
        }
        self.state = JoinerState::Finished;
        Some(match reason {
            Some(CloseReason::WrongPairingCode) => JoinFailure::WrongCode,
            Some(CloseReason::NotReadyToPair) => JoinFailure::NotReady,
            Some(CloseReason::ProtocolError) => JoinFailure::ProtocolError,
            _ => JoinFailure::ConnectionLost,
        })
    }
}

fn fail_joiner(reason: CloseReason, failure: JoinFailure) -> Vec<JoinerOutput> {
    vec![JoinerOutput::Close(reason), JoinerOutput::Failed(failure)]
}

/// A connection, as the caller numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnId(pub u64);

/// What the inviter should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeOutput {
    /// Send this frame on the connection.
    Send(ConnId, Vec<u8>),
    /// Close the connection with this reason.
    Close(ConnId, CloseReason),
    /// Pairing completed on this connection: the joiner is now a member.
    /// Pairing mode has closed.
    Paired {
        conn: ConnId,
        peer: PeerId,
        name: MachineName,
    },
    /// Pairing mode closed, for this reason. The code is no longer valid.
    Closed(ModeClosed),
}

/// Why pairing mode closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeClosed {
    /// A pairing completed; each opening admits one machine.
    Paired,
    /// The 10-minute window ran out.
    Expired,
    /// The user closed it.
    ByUser,
    /// [`MAX_FAILURES`] attempts failed. The user opens pairing mode again
    /// for a new code.
    TooManyFailures,
}

enum AttemptState {
    AwaitConfirm(Tags),
    Records {
        peer: PeerId,
        name: MachineName,
        inbox: RecordInbox,
    },
}

struct Attempt {
    conn: ConnId,
    started: Millis,
    binding: Binding,
    state: AttemptState,
}

struct Open {
    code: Code,
    until: Millis,
    failures: u8,
    attempt: Option<Attempt>,
}

/// The inviter: pairing mode, its code, and its one attempt at a time.
#[derive(Default)]
pub struct PairingMode {
    open: Option<Open>,
}

impl PairingMode {
    /// Pairing mode, closed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens pairing mode with a new code for an inviter at `addr` on a
    /// network with `prefix_len` bits of prefix. Reopening replaces the code
    /// and ends any attempt in progress.
    pub fn open<R: CryptoRng + RngCore>(
        &mut self,
        addr: std::net::Ipv4Addr,
        prefix_len: u8,
        now: Millis,
        rng: &mut R,
    ) -> Vec<ModeOutput> {
        let out = self.close_attempt();
        self.open = Some(Open {
            code: Code::generate(addr, prefix_len, rng),
            until: Millis(now.0.saturating_add(WINDOW_MS)),
            failures: 0,
            attempt: None,
        });
        out
    }

    /// Closes pairing mode at the user's request.
    pub fn close(&mut self) -> Vec<ModeOutput> {
        self.close_for(ModeClosed::ByUser)
    }

    /// Closes pairing mode, ending any attempt without counting it.
    fn close_for(&mut self, why: ModeClosed) -> Vec<ModeOutput> {
        let mut out = self.close_attempt();
        if self.open.take().is_some() {
            out.push(ModeOutput::Closed(why));
        }
        out
    }

    /// The code to show, while pairing mode is open.
    pub fn code(&self) -> Option<&Code> {
        self.open.as_ref().map(|o| &o.code)
    }

    /// When the caller must call [`PairingMode::on_tick`] next.
    pub fn deadline(&self) -> Option<Millis> {
        let open = self.open.as_ref()?;
        let attempt_end = open
            .attempt
            .as_ref()
            .map(|a| Millis(a.started.0.saturating_add(ATTEMPT_MS)));
        Some(attempt_end.map_or(open.until, |end| end.min(open.until)))
    }

    /// Ends the window or the attempt if their time has passed.
    pub fn on_tick(&mut self, now: Millis) -> Vec<ModeOutput> {
        let Some(open) = &mut self.open else {
            return Vec::new();
        };
        if now >= open.until {
            return self.close_for(ModeClosed::Expired);
        }
        let expired = open
            .attempt
            .as_ref()
            .is_some_and(|a| now.since(a.started) >= ATTEMPT_MS);
        if expired {
            return self.fail_attempt(CloseReason::ProtocolError);
        }
        Vec::new()
    }

    /// Handles a pairing message on connection `conn`, whose TLS values are
    /// `binding`.
    pub fn on_message<R: CryptoRng + RngCore>(
        &mut self,
        conn: ConnId,
        binding: Binding,
        message: &PairingMessage<'_>,
        machine: &mut Machine,
        now: Now,
        rng: &mut R,
    ) -> Vec<ModeOutput> {
        let mut out = self.on_tick(now.mono);
        if self.attempt_conn() == Some(conn) {
            out.extend(self.continue_attempt(message, machine, now.unix));
            return out;
        }
        let PairingMessage::Pake(theirs) = message else {
            out.push(ModeOutput::Close(conn, CloseReason::ProtocolError));
            return out;
        };
        match self.open.as_mut() {
            Some(open) if open.attempt.is_none() => {
                out.extend(Self::begin(open, conn, binding, theirs, now.mono, rng));
                if self.attempt_conn().is_none() {
                    out.extend(self.count_failure());
                }
            }
            _ => out.push(ModeOutput::Close(conn, CloseReason::NotReadyToPair)),
        }
        out
    }

    /// The connection closed or dropped. An attempt in progress on it counts
    /// as failed.
    pub fn on_closed(&mut self, conn: ConnId) -> Vec<ModeOutput> {
        if self.attempt_conn() != Some(conn) {
            return Vec::new();
        }
        if let Some(open) = &mut self.open {
            open.attempt = None;
        }
        self.count_failure()
    }

    fn attempt_conn(&self) -> Option<ConnId> {
        self.open.as_ref()?.attempt.as_ref().map(|a| a.conn)
    }

    /// Starts an attempt from the joiner's `PairPake`. On a message that is
    /// not valid SPAKE2, no attempt is stored and the caller counts the
    /// failure.
    fn begin<R: CryptoRng + RngCore>(
        open: &mut Open,
        conn: ConnId,
        binding: Binding,
        theirs: &PairPake,
        now: Millis,
        rng: &mut R,
    ) -> Vec<ModeOutput> {
        let (pake, ours) = Pake::start(&open.code, rng);
        let Some(tags) = pake.finish(&theirs.0, &binding) else {
            return vec![ModeOutput::Close(conn, CloseReason::ProtocolError)];
        };
        open.attempt = Some(Attempt {
            conn,
            started: now,
            binding,
            state: AttemptState::AwaitConfirm(tags),
        });
        vec![ModeOutput::Send(
            conn,
            frame(&PairingMessage::Pake(PairPake(ours))),
        )]
    }

    fn continue_attempt(
        &mut self,
        message: &PairingMessage<'_>,
        machine: &mut Machine,
        now: UnixTime,
    ) -> Vec<ModeOutput> {
        let Some(Attempt {
            conn,
            started,
            binding,
            state,
        }) = self.open.as_mut().and_then(|o| o.attempt.take())
        else {
            return Vec::new();
        };
        let state = match (state, message) {
            (AttemptState::AwaitConfirm(tags), PairingMessage::Confirm(confirm)) => {
                if !tags_match(&confirm.tag, &tags.joiner) {
                    return self.failed(conn, CloseReason::WrongPairingCode);
                }
                let peer = binding.joiner;
                let name = MachineName::from(confirm.name);
                if machine.add(peer, name.clone(), now).is_err() {
                    return self.failed(conn, CloseReason::Normal);
                }
                let reply = PairConfirm {
                    tag: tags.inviter,
                    name: machine.name().wire(),
                };
                let mut out = vec![ModeOutput::Send(
                    conn,
                    frame(&PairingMessage::Confirm(reply)),
                )];
                out.extend(
                    machine
                        .record_frames()
                        .into_iter()
                        .map(|f| ModeOutput::Send(conn, f)),
                );
                self.resume(Attempt {
                    conn,
                    started,
                    binding,
                    state: AttemptState::Records {
                        peer,
                        name,
                        inbox: RecordInbox::new(),
                    },
                });
                return out;
            }
            (
                AttemptState::Records {
                    peer,
                    name,
                    mut inbox,
                },
                m,
            ) => match inbox.on_message(m, machine) {
                Err(_) => return self.failed(conn, CloseReason::ProtocolError),
                Ok(_) if inbox.is_done() => {
                    self.open = None;
                    return vec![
                        ModeOutput::Paired { conn, peer, name },
                        ModeOutput::Closed(ModeClosed::Paired),
                    ];
                }
                Ok(_) => AttemptState::Records { peer, name, inbox },
            },
            _ => return self.failed(conn, CloseReason::ProtocolError),
        };
        self.resume(Attempt {
            conn,
            started,
            binding,
            state,
        });
        Vec::new()
    }

    fn resume(&mut self, attempt: Attempt) {
        if let Some(open) = &mut self.open {
            open.attempt = Some(attempt);
        }
    }

    /// Ends the attempt in progress as failed, closing its connection.
    fn fail_attempt(&mut self, reason: CloseReason) -> Vec<ModeOutput> {
        match self.open.as_mut().and_then(|o| o.attempt.take()) {
            Some(attempt) => self.failed(attempt.conn, reason),
            None => Vec::new(),
        }
    }

    /// Closes `conn` and counts a failed attempt.
    fn failed(&mut self, conn: ConnId, reason: CloseReason) -> Vec<ModeOutput> {
        let mut out = vec![ModeOutput::Close(conn, reason)];
        out.extend(self.count_failure());
        out
    }

    /// Counts a failed attempt; the last one allowed closes pairing mode.
    fn count_failure(&mut self) -> Vec<ModeOutput> {
        let Some(open) = &mut self.open else {
            return Vec::new();
        };
        open.failures = open.failures.saturating_add(1);
        if open.failures < MAX_FAILURES {
            return Vec::new();
        }
        self.close_for(ModeClosed::TooManyFailures)
    }

    /// Ends any attempt without counting it.
    fn close_attempt(&mut self) -> Vec<ModeOutput> {
        self.open
            .as_mut()
            .and_then(|o| o.attempt.take())
            .map(|a| ModeOutput::Close(a.conn, CloseReason::NotReadyToPair))
            .into_iter()
            .collect()
    }
}
