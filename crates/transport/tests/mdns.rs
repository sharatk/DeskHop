//! Two nodes finding each other by real mDNS on this machine's interfaces.
//!
//! Ignored by default: CI runners do not reliably pass multicast, and on a
//! developer machine Windows Firewall may ask about the test binary. Run by
//! hand:
//!
//! ```text
//! cargo test -p transport --test mdns -- --ignored
//! ```

mod harness;

use std::net::Ipv4Addr;
use std::time::Duration;

use harness::*;
use transport::Event;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs multicast on a real network interface"]
async fn members_find_each_other_by_mdns() {
    let (mut a, mut b) = (machine("A"), machine("B"));
    befriend(&mut a, &mut b);
    let live = |m| {
        let mut c = config(m);
        c.bind = Ipv4Addr::UNSPECIFIED.into();
        c.discovery = true;
        c.network = None;
        c
    };
    let (mut a, mut b) = (start(live(a)), start(live(b)));
    let (ida, idb) = (a.id, b.id);
    a.saw(Duration::from_secs(15), |e| *e == Event::PeerUp(idb))
        .await;
    b.saw(Duration::from_secs(15), |e| *e == Event::PeerUp(ida))
        .await;
}
