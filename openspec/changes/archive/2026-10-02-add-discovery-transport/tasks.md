# Tasks

## 1. `proto` and `pairing` additions

- [x] 1.1 Add `CloseReason::NotAMember` (`5`). Verify that the close-code tests cover all six codes and that `from_code(6)` is `None`.
- [x] 1.2 In `pairing`, add `JoinFailure::Unreachable`, have `Joiner::on_closed` map close reason `5` to `ProtocolError`, and add `Applied::accepted`, the records that won and were stored, including on the single-record path. Verify with unit tests: accepted records are listed in acceptance order; stale and dropped records are not listed; and all existing `pairing` tests still pass.

## 2. `transport` setup and build

- [x] 2.1 Add the dependencies from design D14, moving the workspace `mdns-sd` pin to 0.21 and adding `rustls`, `rcgen` and `if-addrs`. Verify that `cargo archcheck` passes, that `cargo tree -d -p transport` shows no second copy of `ring`, `rustls` or `curve25519-dalek`, and that `transport` still forbids `unsafe`.
- [x] 2.2 Add the `Microsoft.VisualStudio.Component.VC.Llvm.Clang` component to `tools/bootstrap.ps1`, and a `cargo check -p transport --target aarch64-pc-windows-msvc` step to the Windows CI job. Verify that the check passes locally after bootstrap.

## 3. TLS identity

- [x] 3.1 Generate the self-signed Ed25519 certificate from `Identity` (D2), and read a peer's identity from a certificate's Ed25519 public key. Choose the parser as D2 describes and record the choice in `design.md`. Verify with unit tests `identity_from_the_certificate`, a non-Ed25519 certificate being rejected, and a malformed DER certificate being rejected without a panic.
- [x] 3.2 Add the verifiers (D3): a client-certificate verifier that accepts any Ed25519 key, and a server verifier that expects one peer or any peer. Both are TLS 1.3 only and check the handshake signature. Verify with loopback tests `certificate_without_the_private_key` (a client presenting B's certificate while signing with another key), `wrong_machine_at_a_members_address` and `unknown_alpn`.

## 4. The node

- [x] 4.1 Add `Node::start(Config)`, with the port, bind address, discovery switch, stored trust store and stored addresses; `NodeHandle` commands; `Event`s; and the control stream with `Hello` through `proto::session::Session`. Add a test harness: loopback nodes, a helper that waits for an event with a deadline, and a UDP relay that can be cut. Verify with tests `port_already_in_use` and a `Hello`-only connection agreeing version 1.
- [x] 4.2 Add member connections (D4): the dialer sends its records first, the listener applies them, then closes with `5` or replies, and the peer is reported up once both exchanges finish. Verify with loopback tests `members_connect`, `introduced_by_records` and `stranger_on_a_member_connection` (no records sent before the close).
- [x] 4.3 Add pairing connections: the listener sends messages to `PairingMode` with a `Binding` from the TLS exporter and both certificate keys; joiner candidates are dialed at once and the first `PairPake` wins (D12); after pairing, both sides dial a member connection. Verify with loopback tests `pairing_then_connecting`, `pairing_mode_closed`, `nobody_answers`, `inviters_mode_closed` and `multicast_blocked`. For the last, the joiner's injected interface list is `127.0.0.5/8` and the inviter listens on `127.0.0.2`, with discovery off.
- [x] 4.4 Add the one-connection rule (D6). Verify with loopback test `both_dial_at_once`, repeated 20 times: one `PeerUp` per side, no `PeerDown`, and exactly one connection left.
- [x] 4.5 Add liveness (D5) and `goodbye`. Verify with loopback tests `peer_goes_silent` (cut the relay: lost between 1 and 2 s, checked with a 3-second ceiling) and `goodbye_before_sleep` (down as goodbye within 200 ms).
- [x] 4.6 Add reconnecting with backoff (D8). Verify with a unit test `backoff` (0.5, 1, 2, 4, 8, 16, 30, 30 seconds) and loopback test `peer_comes_back` (restore the relay and inject a sighting; up again within 1 s).
- [x] 4.7 Push membership changes (D9): accepted and newly signed records go to every up member, a removed member is closed with `5`, and a machine whose own removal wins forgets the desk and closes everything. Verify with loopback tests `removal_reaches_the_removed_machine` and `new_member_spreads` (with three nodes).
- [x] 4.8 Add the limits on strangers (D7). Verify with loopback tests `idle_stranger` and `ninth_stranger`.
- [x] 4.9 Add the `TrustStore` and `Addresses` events, and feeding both back at start. Verify with loopback test `restart_on_a_network_without_multicast`: pair, record both events, restart both nodes from them with discovery off, and both report up.

## 5. Discovery

- [x] 5.1 Add building and parsing of the instance name and TXT record, and turning sightings into dials. Verify with unit tests `advertised_identity`, `pairing_mode_open` and `malformed_identity`, and loopback tests `member_found` and `spoofed_advertisement` using injected sightings.
- [x] 5.2 Add choosing the inviter's address (D11) and building the joiner's candidates, as pure functions over an interface list and a default-route address, plus the OS-backed versions. Verify with unit tests `laptop_with_wifi_and_a_vpn`, `no_ipv4_network`, `same_subnet` and `different_subnet_found_by_mdns`, including candidates deduplicated and the advertised port used.
- [x] 5.3 Wire up the `mdns-sd` daemon: advertise, update when pairing mode opens or closes, and browse into sightings. Add a live two-node test marked `#[ignore]` with a note on running it by hand. Verify the unit tests pass and the ignored test passes when run by hand on a developer machine.

## 6. Docs and checks

- [x] 6.1 Update `docs/protocol.md`:
  - a transport section: port 47391/UDP, the ALPNs, certificate contents and how the key is read, the dialer opening the control stream, the order on member connections, the one-connection rule, keepalive and idle timeout, and the mDNS service type, instance name and TXT keys;
  - close reason `5`;
  - the version-history row.
- [x] 6.2 Update `docs/threat-model.md` with the transport's trust boundary:
  - strangers' handshakes and the limits on them;
  - spoofed mDNS answered by pinning;
  - what an advertisement reveals (a public key, and that pairing mode is open);
  - why records are withheld until the dialer is a member;
  - the firewall rule that `add-installer` needs.
- [x] 6.3 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo archcheck`. Then break one rule at a time and confirm that only the matching tests fail: the pinning check, the membership check before close `5`, the lower-identity rule, and the stranger limit. Touch the restored files before the final test run.
