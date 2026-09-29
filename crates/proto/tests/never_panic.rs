//! Decoding is total: arbitrary input never panics, and every decoded frame
//! lies inside its input. Deterministic, so it runs in `cargo test` on stable;
//! `tools/fuzz` covers the same drivers with coverage guidance.

use proto::datagram::MAX_DATAGRAM_LEN;
use proto::frame::{self, Decoded, HEADER_LEN};
use proto::hello::Hello;
use proto::registry;
use proto::session::Session;

const ITERATIONS: usize = 300_000;

/// xorshift64: tiny, deterministic, good enough to shake a parser.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }

    fn bytes_below(&mut self, max_len: usize) -> Vec<u8> {
        let len = self.below(max_len);
        self.bytes(len)
    }
}

/// Feeds `input` as one stream: decodes frames until it needs more input or
/// fails, handing each to the session as a control or other-stream frame.
fn drive_stream(session: &mut Session, input: &[u8], rng: &mut Rng) {
    let mut rest = input;
    loop {
        match frame::decode(rest) {
            Ok(Decoded::NeedMore) | Err(_) => return,
            Ok(Decoded::Frame { frame, consumed }) => {
                assert!(consumed >= HEADER_LEN && consumed <= rest.len());
                assert_eq!(consumed - HEADER_LEN, frame.payload.len());
                let ok = if rng.below(2) == 0 {
                    session.on_control_frame(frame).is_ok()
                } else {
                    session.on_stream_frame(frame).is_ok()
                };
                if !ok {
                    return;
                }
                rest = &rest[consumed..];
            }
        }
    }
}

fn agreed_session() -> Session {
    let mut session = Session::new(Hello::this_release());
    let payload = Hello::this_release().payload();
    session
        .on_control_frame(frame::Frame {
            ty: registry::HELLO,
            payload: &payload,
        })
        .unwrap();
    session
}

/// A few valid frames back to back: a `Hello`, then assorted types.
fn valid_frames(rng: &mut Rng) -> Vec<u8> {
    let mut out = Vec::new();
    Hello::this_release().encode_frame(&mut out).unwrap();
    for _ in 0..rng.below(4) {
        let ty = rng.below(4) as u16;
        let payload = rng.bytes_below(12);
        frame::encode(ty, &payload, &mut out).unwrap();
    }
    out
}

fn mutate(input: &mut Vec<u8>, rng: &mut Rng) {
    for _ in 0..=rng.below(4) {
        if input.is_empty() {
            input.push(rng.next() as u8);
            continue;
        }
        let at = rng.below(input.len());
        match rng.below(4) {
            0 => input[at] ^= 1 << rng.below(8),
            1 => input[at] = rng.next() as u8,
            2 => input.truncate(at),
            _ => input.insert(at, rng.next() as u8),
        }
    }
}

#[test]
fn random_stream_bytes_never_panic() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..ITERATIONS {
        let input = rng.bytes_below(64);
        drive_stream(&mut Session::new(Hello::this_release()), &input, &mut rng);
        drive_stream(&mut agreed_session(), &input, &mut rng);
    }
}

#[test]
fn mutated_frames_never_panic() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for _ in 0..ITERATIONS {
        let mut input = valid_frames(&mut rng);
        mutate(&mut input, &mut rng);
        drive_stream(&mut Session::new(Hello::this_release()), &input, &mut rng);
        drive_stream(&mut agreed_session(), &input, &mut rng);
    }
}

#[test]
fn random_datagrams_never_panic() {
    let mut rng = Rng(0xd1b5_4a32_d192_ed03);
    let (fresh, agreed) = (Session::new(Hello::this_release()), agreed_session());
    for _ in 0..ITERATIONS {
        let len = if rng.below(100) == 0 {
            rng.below(MAX_DATAGRAM_LEN + 100)
        } else {
            rng.below(16)
        };
        let input = rng.bytes(len);
        let _ = fresh.on_datagram(&input);
        let _ = agreed.on_datagram(&input);
    }
}
