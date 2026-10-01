//! Desk membership across several machines (spec: desk-membership).

mod harness;

use harness::*;
use pairing::{MAX_SUBJECTS, Machine, MachineName, RecordInbox, Violation};
use proto::pairing::{Change, MemberRecord, Name, PairingMessage};

/// Two machines already paired.
fn desk_of_two() -> (Machine, Machine) {
    let (mut a, mut b) = (machine(1, "A"), machine(2, "B"));
    pair_now(&mut b, &mut a, 100);
    (a, b)
}

#[test]
fn third_machine_joins_through_one_member() {
    let (mut a, mut b) = desk_of_two();
    let mut c = machine(3, "C");
    pair_now(&mut c, &mut b, 101);
    assert_eq!(members(&c, &[&a, &b]), [true, true]);
    assert_eq!(members(&b, &[&c]), [true]);
    assert_eq!(members(&a, &[&c]), [false]);
    exchange(&mut a, &mut b);
    assert_eq!(members(&a, &[&c]), [true]);
}

#[test]
fn two_desks_merge() {
    let (mut a, mut c) = (machine(1, "A"), machine(3, "C"));
    pair_now(&mut c, &mut a, 100);
    let (mut b, mut d) = (machine(2, "B"), machine(4, "D"));
    pair_now(&mut d, &mut b, 101);
    pair_now(&mut b, &mut a, 102);
    exchange(&mut a, &mut c);
    exchange(&mut b, &mut d);
    for (m, others) in [
        (&a, [&b, &c, &d]),
        (&b, [&a, &c, &d]),
        (&c, [&a, &b, &d]),
        (&d, [&a, &b, &c]),
    ] {
        assert_eq!(members(m, &others), [true, true, true], "{:?}", m.name());
    }
}

#[test]
fn remove_from_any_member() {
    let (mut a, mut b) = desk_of_two();
    let mut c = machine(3, "C");
    pair_now(&mut c, &mut b, 101);
    exchange(&mut a, &mut b);
    b.remove(c.peer_id(), unix());
    assert_eq!(members(&b, &[&c]), [false]);
    exchange(&mut a, &mut b);
    assert_eq!(members(&a, &[&c]), [false]);
    // A stale addition from C's own store cannot bring it back.
    exchange(&mut a, &mut c);
    assert_eq!(members(&a, &[&c]), [false]);
}

#[test]
fn removed_machine_forgets_the_desk() {
    let (mut a, mut b) = desk_of_two();
    let mut c = machine(3, "C");
    pair_now(&mut c, &mut b, 101);
    let identity_before = c.peer_id();
    let removal = b.remove(c.peer_id(), unix());
    let applied = c.apply([removal]);
    assert!(applied.forgotten);
    assert_eq!(members(&c, &[&a, &b]), [false, false]);
    assert_eq!(c.desk().records().count(), 0);
    assert_eq!(c.peer_id(), identity_before);
    exchange(&mut a, &mut b);
}

#[test]
fn re_pairing_brings_a_removed_machine_back() {
    let (mut a, mut b) = desk_of_two();
    let mut c = machine(3, "C");
    pair_now(&mut c, &mut b, 101);
    let removal = b.remove(c.peer_id(), unix());
    c.apply([removal]);
    pair_now(&mut c, &mut a, 102);
    exchange(&mut a, &mut b);
    assert_eq!(members(&b, &[&c]), [true]);
    assert_eq!(members(&c, &[&a, &b]), [true, true]);
}

#[test]
fn offline_member_catches_up() {
    let (mut a, mut b) = desk_of_two();
    let mut c = machine(3, "C");
    // A is offline while C joins through B.
    pair_now(&mut c, &mut b, 101);
    let mut inbox = RecordInbox::new();
    let frames = b.record_frames();
    let (last, records) = frames.split_last().unwrap();
    for bytes in records {
        let (ty, payload) = decode(bytes);
        let message = PairingMessage::decode(ty, &payload).unwrap();
        assert_eq!(inbox.on_message(&message, &mut a), Ok(None));
    }
    assert_eq!(members(&a, &[&c]), [false]);
    let (ty, payload) = decode(last);
    let applied = inbox
        .on_message(&PairingMessage::decode(ty, &payload).unwrap(), &mut a)
        .unwrap()
        .unwrap();
    assert_eq!(applied.added, [c.peer_id()]);
}

#[test]
fn record_after_records_done_applies_at_once() {
    let (mut a, mut b) = desk_of_two();
    let c = machine(3, "C");
    let mut inbox = RecordInbox::new();
    inbox
        .on_message(&PairingMessage::RecordsDone, &mut a)
        .unwrap();
    let record = b
        .add(c.peer_id(), MachineName::new("C").unwrap(), unix())
        .unwrap();
    let applied = inbox
        .on_message(&PairingMessage::Record(record.wire()), &mut a)
        .unwrap()
        .unwrap();
    assert_eq!(applied.added, [c.peer_id()]);
    assert_eq!(
        inbox.on_message(&PairingMessage::RecordsDone, &mut a),
        Err(Violation::OutOfOrder)
    );
}

#[test]
fn record_flood() {
    let (mut a, _) = desk_of_two();
    let record = MemberRecord {
        subject: [1; 32],
        epoch: 1,
        signer: [2; 32],
        change: Change::Add(Name::new("x").unwrap()),
        signature: [0; 64],
    };
    let mut inbox = RecordInbox::new();
    for _ in 0..MAX_SUBJECTS {
        assert_eq!(
            inbox.on_message(&PairingMessage::Record(record), &mut a),
            Ok(None)
        );
    }
    assert_eq!(
        inbox.on_message(&PairingMessage::Record(record), &mut a),
        Err(Violation::TooManyRecords)
    );
}

#[test]
fn pairing_messages_in_a_record_exchange_are_out_of_order() {
    let (mut a, _) = desk_of_two();
    let mut inbox = RecordInbox::new();
    assert_eq!(
        inbox.on_message(
            &PairingMessage::Pake(proto::pairing::PairPake([0; 33])),
            &mut a
        ),
        Err(Violation::OutOfOrder)
    );
}

#[test]
fn trust_store_survives_a_restart_after_pairing() {
    let (a, _) = desk_of_two();
    let file = a.trust_store();
    let identity = pairing::Identity::from_file(&*a.identity().to_file()).unwrap();
    let mut restarted = Machine::new(identity, a.name().clone());
    restarted.load_trust_store(&file).unwrap();
    assert_eq!(restarted.desk(), a.desk());
}
