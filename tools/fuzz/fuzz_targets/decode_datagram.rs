//! Datagram decoding, before and after a version is agreed.

#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;
use proto::frame::Frame;
use proto::hello::Hello;
use proto::registry;
use proto::session::Session;

fuzz_target!(|data: &[u8]| {
    let fresh = Session::new(Hello::this_release());
    let _ = fresh.on_datagram(data);

    let mut agreed = Session::new(Hello::this_release());
    let payload = Hello::this_release().payload();
    let hello = Frame { ty: registry::HELLO, payload: &payload };
    assert!(agreed.on_control_frame(hello).is_ok());
    let _ = agreed.on_datagram(data);
});
