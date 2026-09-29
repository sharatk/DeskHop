//! Stream decoding plus the session state machine. The first input byte picks
//! whether the session has already agreed a version and, bit by bit, whether
//! each frame goes to the control stream or another stream.

#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;
use proto::frame::{self, Decoded, Frame, HEADER_LEN};
use proto::hello::Hello;
use proto::registry;
use proto::session::Session;

fuzz_target!(|data: &[u8]| {
    let Some((&routing, mut rest)) = data.split_first() else {
        return;
    };
    let mut session = Session::new(Hello::this_release());
    if routing & 1 == 1 {
        let payload = Hello::this_release().payload();
        let hello = Frame { ty: registry::HELLO, payload: &payload };
        assert!(session.on_control_frame(hello).is_ok());
    }
    let mut bit = 1;
    loop {
        match frame::decode(rest) {
            Ok(Decoded::NeedMore) | Err(_) => return,
            Ok(Decoded::Frame { frame, consumed }) => {
                assert!(consumed >= HEADER_LEN && consumed <= rest.len());
                bit = (bit + 1) % 8;
                let ok = if routing >> bit & 1 == 0 {
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
});
