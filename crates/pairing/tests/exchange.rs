//! The pairing exchange and pairing mode, run in memory (spec: pairing).

mod harness;

use harness::*;
use model::Millis;
use pairing::{
    ATTEMPT_MS, Code, ConnId, JoinFailure, Joiner, JoinerOutput, ModeClosed, ModeOutput, WINDOW_MS,
};
use proto::CloseReason;
use proto::pairing::{PairConfirm, PairingMessage};

/// A valid code that is not the one `mode` shows.
fn other_code(mode: &pairing::PairingMode) -> Code {
    let shown = typed(mode);
    let other = shown.with_new_secret(&mut rng(4242));
    assert_ne!(other, shown);
    other
}

#[test]
fn matching_codes() {
    let (mut b, mut a) = (machine(2, "DESK-B"), machine(1, "DESK-A"));
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind = binding(&b, &a);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        CONN,
        Millis(0),
    );

    assert!(outcome.joiner.contains(&JoinerOutput::Paired {
        peer: id(&a),
        name: pairing::MachineName::new("DESK-A").unwrap(),
    }));
    assert!(outcome.inviter.contains(&ModeOutput::Paired {
        conn: CONN,
        peer: id(&b),
        name: pairing::MachineName::new("DESK-B").unwrap(),
    }));
    assert_eq!(outcome.closed_by_joiner, Some(CloseReason::Normal));
    assert!(a.desk().is_member(&id(&b)));
    assert!(b.desk().is_member(&id(&a)));
    assert_eq!(outcome.joiner_failure(), None);
}

#[test]
fn wrong_code() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let code = other_code(&mode);
    let bind = binding(&b, &a);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        CONN,
        Millis(0),
    );

    assert_eq!(
        outcome.closed_by_inviter,
        Some(CloseReason::WrongPairingCode)
    );
    assert_eq!(outcome.joiner_failure(), Some(JoinFailure::WrongCode));
    assert!(!a.desk().is_member(&id(&b)));
    assert!(!b.desk().is_member(&id(&a)));
}

#[test]
fn inviter_never_sends_its_tag_after_a_wrong_one() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let code = other_code(&mode);
    let bind = binding(&b, &a);
    let (mut j, out) = Joiner::start(&code, bind, &mut rng(1));
    let JoinerOutput::Send(pake) = &out[0] else {
        panic!()
    };
    let (ty, payload) = decode(pake);
    let reply = mode.on_message(
        CONN,
        bind,
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut a,
        at(0),
        &mut rng(2),
    );
    let ModeOutput::Send(_, theirs) = &reply[0] else {
        panic!()
    };
    let (ty, payload) = decode(theirs);
    let out = j.on_message(
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut b,
        unix(),
    );
    let JoinerOutput::Send(confirm) = &out[0] else {
        panic!()
    };
    let (ty, payload) = decode(confirm);
    let reply = mode.on_message(
        CONN,
        bind,
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut a,
        at(1),
        &mut rng(3),
    );
    assert_eq!(
        reply,
        [ModeOutput::Close(CONN, CloseReason::WrongPairingCode)]
    );
}

#[test]
fn message_out_of_order() {
    let mut a = machine(1, "A");
    let b = machine(2, "B");
    let mut mode = open_mode(10);
    let confirm = PairingMessage::Confirm(PairConfirm {
        tag: [0; 32],
        name: proto::pairing::Name::new("B").unwrap(),
    });
    let out = mode.on_message(CONN, binding(&b, &a), &confirm, &mut a, at(0), &mut rng(1));
    assert_eq!(out, [ModeOutput::Close(CONN, CloseReason::ProtocolError)]);
}

#[test]
fn relay_in_the_middle() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let relay = machine(3, "R");
    let mut mode = open_mode(10);
    let code = typed(&mode);
    // The joiner's connection ends at the relay, which opens its own to the
    // inviter: each side sees the relay's certificate and its own exporter.
    let bind_joiner = pairing::Binding {
        exporter: [1; 32],
        inviter: id(&relay),
        joiner: id(&b),
    };
    let bind_inviter = pairing::Binding {
        exporter: [2; 32],
        inviter: id(&a),
        joiner: id(&relay),
    };
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind_joiner,
        bind_inviter,
        CONN,
        Millis(0),
    );
    assert_eq!(
        outcome.closed_by_inviter,
        Some(CloseReason::WrongPairingCode)
    );
    assert!(!a.desk().is_member(&id(&relay)));
    assert!(!b.desk().is_member(&id(&relay)));
}

#[test]
fn reflected_tag() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind = binding(&b, &a);
    let (mut j, out) = Joiner::start(&code, bind, &mut rng(1));
    let JoinerOutput::Send(pake) = &out[0] else {
        panic!()
    };
    let (ty, payload) = decode(pake);
    let reply = mode.on_message(
        CONN,
        bind,
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut a,
        at(0),
        &mut rng(2),
    );
    let ModeOutput::Send(_, theirs) = &reply[0] else {
        panic!()
    };
    let (ty, payload) = decode(theirs);
    let out = j.on_message(
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut b,
        unix(),
    );
    let JoinerOutput::Send(confirm) = &out[0] else {
        panic!()
    };
    // Send the joiner's own confirmation straight back to it.
    let (ty, payload) = decode(confirm);
    let out = j.on_message(
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut b,
        unix(),
    );
    assert_eq!(
        out,
        [
            JoinerOutput::Close(CloseReason::WrongPairingCode),
            JoinerOutput::Failed(JoinFailure::WrongCode),
        ]
    );
}

#[test]
fn empty_name() {
    let mut payload = vec![0; 32];
    payload.push(0);
    assert!(PairingMessage::decode(proto::registry::PAIR_CONFIRM, &payload).is_err());
}

#[test]
fn pairing_mode_closed() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = pairing::PairingMode::new();
    let code = Code::parse(&open_mode(10).code().unwrap().to_string()).unwrap();
    let bind = binding(&b, &a);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        CONN,
        Millis(0),
    );
    assert_eq!(outcome.closed_by_inviter, Some(CloseReason::NotReadyToPair));
    assert_eq!(outcome.joiner_failure(), Some(JoinFailure::NotReady));
}

#[test]
fn window_expires() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let code = typed(&mode);
    assert_eq!(mode.deadline(), Some(Millis(WINDOW_MS)));
    assert_eq!(mode.on_tick(Millis(WINDOW_MS - 1)), []);
    assert_eq!(
        mode.on_tick(Millis(WINDOW_MS)),
        [ModeOutput::Closed(ModeClosed::Expired)]
    );
    assert!(mode.code().is_none());
    let bind = binding(&b, &a);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        CONN,
        Millis(WINDOW_MS),
    );
    assert_eq!(outcome.joiner_failure(), Some(JoinFailure::NotReady));
}

#[test]
fn window_expiry_is_noticed_without_a_tick() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind = binding(&b, &a);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        CONN,
        Millis(WINDOW_MS + 5),
    );
    assert!(
        outcome
            .inviter
            .contains(&ModeOutput::Closed(ModeClosed::Expired))
    );
    assert_eq!(outcome.joiner_failure(), Some(JoinFailure::NotReady));
}

#[test]
fn third_wrong_guess_closes_pairing_mode() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let shown = typed(&mode);
    let wrong = other_code(&mode);
    let bind = binding(&b, &a);
    for attempt in 1..=3 {
        let outcome = pair(
            &mut b,
            &mut a,
            &mut mode,
            &wrong,
            bind,
            bind,
            ConnId(attempt),
            Millis(0),
        );
        assert_eq!(outcome.joiner_failure(), Some(JoinFailure::WrongCode));
        let closed = outcome
            .inviter
            .contains(&ModeOutput::Closed(ModeClosed::TooManyFailures));
        assert_eq!(closed, attempt == 3);
    }
    assert!(mode.code().is_none());
    // The shown code no longer works, even typed correctly.
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &shown,
        bind,
        bind,
        ConnId(4),
        Millis(0),
    );
    assert_eq!(outcome.joiner_failure(), Some(JoinFailure::NotReady));
    // Opening again gives a new code that pairs.
    mode.open(INVITER_ADDR, 24, Millis(1), &mut rng(11));
    let fresh = typed(&mode);
    assert_ne!(fresh, shown);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &fresh,
        bind,
        bind,
        ConnId(5),
        Millis(2),
    );
    assert!(outcome.joiner_paired());
}

#[test]
fn two_wrong_guesses_leave_pairing_mode_open() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let shown = typed(&mode);
    let wrong = other_code(&mode);
    let bind = binding(&b, &a);
    for attempt in 1..=2 {
        pair(
            &mut b,
            &mut a,
            &mut mode,
            &wrong,
            bind,
            bind,
            ConnId(attempt),
            Millis(0),
        );
    }
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &shown,
        bind,
        bind,
        ConnId(3),
        Millis(0),
    );
    assert!(outcome.joiner_paired());
}

#[test]
fn abandoned_attempt() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind = binding(&b, &a);
    let (_silent, out) = Joiner::start(&code, bind, &mut rng(1));
    let JoinerOutput::Send(pake) = &out[0] else {
        panic!()
    };
    let (ty, payload) = decode(pake);
    mode.on_message(
        ConnId(1),
        bind,
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut a,
        at(1_000),
        &mut rng(2),
    );
    assert_eq!(mode.deadline(), Some(Millis(1_000 + ATTEMPT_MS)));
    assert_eq!(mode.on_tick(Millis(1_000 + ATTEMPT_MS - 1)), []);
    assert_eq!(
        mode.on_tick(Millis(1_000 + ATTEMPT_MS)),
        [ModeOutput::Close(ConnId(1), CloseReason::ProtocolError)]
    );
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        ConnId(2),
        Millis(40_000),
    );
    assert!(outcome.joiner_paired());
}

#[test]
fn concurrent_attempt() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut c = machine(3, "C");
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind_b = binding(&b, &a);
    // B's attempt starts and pauses after the inviter's PairPake.
    let (mut jb, out) = Joiner::start(&code, bind_b, &mut rng(1));
    let JoinerOutput::Send(pake) = &out[0] else {
        panic!()
    };
    let (ty, payload) = decode(pake);
    let reply = mode.on_message(
        ConnId(1),
        bind_b,
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut a,
        at(0),
        &mut rng(2),
    );
    // C tries meanwhile.
    let bind_c = binding(&c, &a);
    let outcome = pair(
        &mut c,
        &mut a,
        &mut mode,
        &code,
        bind_c,
        bind_c,
        ConnId(2),
        Millis(1),
    );
    assert_eq!(outcome.closed_by_inviter, Some(CloseReason::NotReadyToPair));
    assert!(mode.code().is_some(), "a refused attempt does not count");
    // B's attempt continues to the end.
    let ModeOutput::Send(_, theirs) = &reply[0] else {
        panic!()
    };
    let mut to_joiner = vec![theirs.clone()];
    let mut paired = (false, false);
    while let Some(bytes) = (!to_joiner.is_empty()).then(|| to_joiner.remove(0)) {
        let (ty, payload) = decode(&bytes);
        for o in jb.on_message(
            &PairingMessage::decode(ty, &payload).unwrap(),
            &mut b,
            unix(),
        ) {
            match o {
                JoinerOutput::Send(f) => {
                    let (ty, payload) = decode(&f);
                    for m in mode.on_message(
                        ConnId(1),
                        bind_b,
                        &PairingMessage::decode(ty, &payload).unwrap(),
                        &mut a,
                        at(2),
                        &mut rng(3),
                    ) {
                        match m {
                            ModeOutput::Send(_, f) => to_joiner.push(f),
                            ModeOutput::Paired { .. } => paired.1 = true,
                            _ => {}
                        }
                    }
                }
                JoinerOutput::Paired { .. } => paired.0 = true,
                _ => {}
            }
        }
    }
    assert_eq!(paired, (true, true));
}

#[test]
fn one_join_per_opening() {
    let (mut b, mut a) = (machine(2, "B"), machine(1, "A"));
    let mut c = machine(3, "C");
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind = binding(&b, &a);
    let outcome = pair(
        &mut b,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        ConnId(1),
        Millis(0),
    );
    assert!(
        outcome
            .inviter
            .contains(&ModeOutput::Closed(ModeClosed::Paired))
    );
    assert!(mode.code().is_none());
    let bind = binding(&c, &a);
    let outcome = pair(
        &mut c,
        &mut a,
        &mut mode,
        &code,
        bind,
        bind,
        ConnId(2),
        Millis(1),
    );
    assert_eq!(outcome.joiner_failure(), Some(JoinFailure::NotReady));
}

#[test]
fn dropped_connection_counts_as_a_failed_attempt() {
    let a_seed = 1;
    let mut a = machine(a_seed, "A");
    let b = machine(2, "B");
    let mut mode = open_mode(10);
    let code = typed(&mode);
    let bind = binding(&b, &a);
    for conn in 1..=3 {
        let (_, out) = Joiner::start(&code, bind, &mut rng(conn));
        let JoinerOutput::Send(pake) = &out[0] else {
            panic!()
        };
        let (ty, payload) = decode(pake);
        mode.on_message(
            ConnId(conn),
            bind,
            &PairingMessage::decode(ty, &payload).unwrap(),
            &mut a,
            at(0),
            &mut rng(2),
        );
        let out = mode.on_closed(ConnId(conn));
        assert_eq!(
            out.contains(&ModeOutput::Closed(ModeClosed::TooManyFailures)),
            conn == 3
        );
    }
}

#[test]
fn reopening_replaces_the_code_and_ends_the_attempt() {
    let mut a = machine(1, "A");
    let b = machine(2, "B");
    let mut mode = open_mode(10);
    let first = typed(&mode);
    let bind = binding(&b, &a);
    let (_, out) = Joiner::start(&first, bind, &mut rng(1));
    let JoinerOutput::Send(pake) = &out[0] else {
        panic!()
    };
    let (ty, payload) = decode(pake);
    mode.on_message(
        CONN,
        bind,
        &PairingMessage::decode(ty, &payload).unwrap(),
        &mut a,
        at(0),
        &mut rng(2),
    );
    let out = mode.open(INVITER_ADDR, 24, Millis(5), &mut rng(11));
    assert_eq!(out, [ModeOutput::Close(CONN, CloseReason::NotReadyToPair)]);
    assert_ne!(typed(&mode), first);
    assert_eq!(mode.close(), [ModeOutput::Closed(ModeClosed::ByUser)]);
}

#[test]
fn inviter_close_codes_reach_the_joiner() {
    let code = typed(&open_mode(10));
    let (a, b) = (machine(1, "A"), machine(2, "B"));
    for (reason, failure) in [
        (CloseReason::WrongPairingCode, JoinFailure::WrongCode),
        (CloseReason::NotReadyToPair, JoinFailure::NotReady),
        (CloseReason::ProtocolError, JoinFailure::ProtocolError),
        (CloseReason::NotAMember, JoinFailure::ProtocolError),
        (CloseReason::Normal, JoinFailure::ConnectionLost),
    ] {
        let (mut j, _) = Joiner::start(&code, binding(&b, &a), &mut rng(1));
        assert_eq!(j.on_closed(Some(reason)), Some(failure));
        assert_eq!(j.on_closed(Some(reason)), None);
    }
    let (mut j, _) = Joiner::start(&code, binding(&b, &a), &mut rng(1));
    assert_eq!(j.on_closed(None), Some(JoinFailure::ConnectionLost));
}
