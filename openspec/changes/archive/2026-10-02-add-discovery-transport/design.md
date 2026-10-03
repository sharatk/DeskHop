# Design

## Context

- **`pairing`:** provides `Machine` (identity, name, membership view), `PairingMode`, `Joiner` and `RecordInbox`. These are sans-I/O: they take decoded messages and both clocks, and return encoded frames and close reasons.
- **`proto`:** provides framing, `Session` (`Hello`) and the message types.
- **`transport`:** an empty stub. The workspace already pins `quinn` 0.11, `mdns-sd` 0.13 and `tokio` 1.
- **ADR 0004 Amendment 2:** fixes the security shape: self-signed certificates holding the identity key, pinned by a custom verifier, with no CA.
- **ADR 0002:** the service owns the transport, and the service process does not exist yet.

This change therefore delivers a `transport` library that can be tested on its own. `add-ipc-and-service` runs it inside the service.

## Goals / Non-Goals

**Goals:**
- Run every rule in the `peer-transport` and `discovery` specs between real QUIC endpoints on loopback, in `cargo test`.
- Keep all decisions about trust inside `pairing`'s state machines and the TLS verifiers. Mostly, the transport only moves bytes and keeps time.
- Make no Win32 calls and use no `unsafe`.

**Non-Goals:**
- Engine messages and motion datagrams. QUIC datagrams are turned on in the transport config, but there is no API for them until `add-input-forwarding`.
- Storing anything on disk, power events (goodbye on sleep), and starting inside the service. Those are `add-ipc-and-service`.
- The firewall rule (`add-installer`), and IPv6-only networks.

## Decisions

### D1. One node task owns all state; one task per connection moves bytes

`Node::start(config)` returns a `NodeHandle` (commands) and a receiver of `Event`s.

- **The node task** owns `Machine`, `PairingMode`, the connection table, the dial backoff timers and the stranger count. It runs the state machines synchronously.
- **Connection tasks** read the control stream, decode frames with `proto::frame` and `Session`, and pass each pairing or membership message to the node. They write back whatever frames the node returns.
- **Commands:** `open_pairing`, `close_pairing`, `join(code)`, `remove(peer)`, `goodbye`, `shutdown`.
- **Events:** `PairingOpened(code)`, `PairingClosed(ModeClosed)`, `Paired{peer, name}`, `JoinFailed(JoinFailure)`, `PeerUp(peer)`, `PeerDown(peer, Down::{Lost, Goodbye, Removed})`, `TrustStore(bytes)`, `Addresses(map)`.

*Alternative:* shared state behind a mutex, used by every connection task. That gives more lock ordering to reason about, and the `pairing` state machines already assume a single caller.

### D2. Certificates

- **Generating:** `rcgen` builds a self-signed certificate from `Identity::to_pkcs8_der()` with the `ring` backend, Ed25519, subject alt name `deskhop`, and `rcgen`'s default validity (validity is ignored). It is regenerated at every start; nothing is cached.
- **Reading the key:** the peer's key comes from the certificate's SubjectPublicKeyInfo, accepted only if the algorithm is Ed25519 (OID 1.3.101.112) and the key is 32 bytes.
  - The parser is `rustls-webpki` 0.103, which `rustls` already depends on: `EndEntityCert` parses the certificate, and `subject_public_key_info()` returns the DER SPKI. (Chosen during apply over `x509-cert`; nothing new is compiled.)
  - The SPKI must equal the fixed 44-byte Ed25519 encoding from RFC 8410: `30 2a 30 05 06 03 2b 65 70 03 21 00`, then the 32-byte key. Any other algorithm, parameters or length is rejected.
- **Handshake signatures:** verified with `rustls::crypto::verify_tls13_signature` and the `ring` provider's algorithms, restricted to `ED25519`.
- **TLS 1.3 only.**

*Alternative:* raw public keys (RFC 7250) in place of certificates. That would mean no X.509 parsing at all, but ADR 0004 Amendment 2 says certificates, and `rustls`' raw-public-key support is newer. It is worth revisiting in an ADR later.

### D3. Verifiers

- **Server side:** a client-certificate verifier that requires a certificate, accepts any Ed25519 key, and checks the TLS 1.3 signature. The node decides membership after the handshake, from `Connection::peer_identity()`.
- **Client side:** a server verifier built for each dial with `Expect::Peer(id)` (member dial) or `Expect::Any` (pairing dial). It checks the key, then the TLS 1.3 signature.
- **Names:** server names are ignored. The dialer always sends SNI `deskhop`.

### D4. Connection setup

- **ALPN:** `deskhop/1` for member connections and `deskhop-pair/1` for pairing connections. The server lists both; a client offers exactly one.
- **Control stream:** the dialer opens the first bidirectional stream, and both sides write `Hello` first through `proto::session::Session`. Frames are read into a buffer that is never larger than the frame header plus the frame limit, as `proto` requires.
- **Member connections:**
  - The dialer sends its records and `RecordsDone`.
  - The listener feeds them to a `RecordInbox`. If the dialer is now a member, the listener replies with its records and `RecordsDone`; otherwise it closes with code `5`.
  - The dialer's inbox then receives the listener's records.
  - The peer is up once both inboxes have finished.
- **Pairing connections:**
  - The dialer runs a `Joiner`.
  - The listener sends `PairPake` and all later pairing messages to the node's `PairingMode`, tagged with the connection's `ConnId` and a `Binding` built from the exporter and both certificate keys.
- **Exporter:** `quinn::Connection::export_keying_material(out, b"EXPORTER-deskhop-pair-v1", b"")`, 32 bytes.

### D5. Liveness

The quinn `TransportConfig` sets `keep_alive_interval` to 250 ms and `max_idle_timeout` to 1 s on both sides.

- `ConnectionError::TimedOut` means `Down::Lost`.
- An application close with code `0` from the peer means `Down::Goodbye`, unless the connection was the duplicate being closed under D6.
- `goodbye()` closes every member connection with `0`, reports those peers down as goodbye, and makes the node quiet: no dials, and incoming member connections closed with `0`, until `resume()`. The service calls `goodbye()` before sleep and `resume()` after wake.

### D6. One connection per pair

The connection table maps each peer to its one connection that is up, with a note of which side dialed it.

When a second connection to the same peer finishes its record exchange, it replaces the existing one if it was dialed by the lower identity (compared as bytes), and is closed with `0` otherwise. The replaced connection is closed with `0` and flagged so that its close is not reported as down. Both sides apply the same rule to the same pair of connections, so they agree.

### D7. Strangers

Any connection whose key is not a member is counted when its handshake completes. That includes every pairing connection, and member connections until their introduction succeeds.

- At most 8 are counted at once; beyond that, the connection is closed at once with `4` (pairing) or `5` (member).
- Each has a 10-second deadline that the node tracks with the other timers.
- `PairingMode`'s own 30-second attempt limit still applies inside that window. The 10 seconds covers only the time until the first `PairPake` or `RecordsDone`.

### D8. Reconnecting and addresses

- **Addresses:** for each member, the node keeps a set of candidate addresses. It is filled from start-up config, from mDNS sightings, and from where connections in either direction were last seen.
- **Dialing:** a member that is not up is dialled at all its candidates at once. The first to finish wins; the rest are dropped.
- **Backoff:** per member, 0.5 s doubling to 30 s. It resets on success and on a new sighting.
- **Reporting:** `Event::Addresses` is sent when a member's last-reached address changes, deduplicated so a steady connection reports nothing.

### D9. Membership changes

`pairing::Desk::apply` gains `Applied::accepted`: the records that won and were stored.

- The node compares the view before and after anything that can change it (a record exchange, pairing on either side, a removal). It sends every record that is new or changed to each member connection that has already sent its own records, as a single `MemberRecord`; the desk-membership spec allows records after `RecordsDone`. Comparing views covers the changes `PairingMode` and `Joiner` make inside `pairing`, which `Applied` does not see.
- When `Applied::removed` names an up member, that connection gets the removal, then a close with `5`.
- When a removal of this machine wins (`forgotten`), every connection is closed with `0` and every peer reported down as removed.
- `Event::TrustStore` carries `Machine::trust_store()` after any change.

### D10. Discovery

- **Advertising:** `mdns-sd` 0.21 `ServiceDaemon` advertises `_deskhop._udp.local.` with an instance name of 32 hex digits, the port, and TXT `id` (plus `pair=1`). Addresses are filled in automatically.
- **Browsing:** resolved services become `Sighting { id, addrs, port, pairing }` and are sent to the node.
- **Pure parts:** TXT building and parsing, and locator matching, are pure functions with unit tests. A live mDNS test is `#[ignore]`d, because CI runners do not reliably pass multicast.
- **Tests:** loopback tests turn discovery off in `Config` and inject sightings through a test-only handle method.

### D11. Interfaces and the inviter's address

- **Default route:** bind a UDP socket to `0.0.0.0:0` and `connect` it to `192.0.2.1:9` (TEST-NET-1, which goes out through the default route). Connecting a UDP socket only selects a route and sends nothing. `local_addr()` is then the default-route source address.
- **Prefix length:** taken from `if-addrs` for the interface with that address.
- **Fallbacks:** as the discovery spec orders them.
- **Joiner:** its own interfaces come from the same `if-addrs` list.

### D12. Joining over several candidates

`join(code)` builds the candidate list (discovery spec) and dials each with its own `Joiner` and `Expect::Any`.

- The first connection to receive the inviter's `PairPake` becomes the winner; every other candidate is closed with `0`.
- If all candidates fail, the failure is chosen by the spec's order: wrong code, then not ready, then unreachable.
- Candidates are dialed with a 5-second handshake deadline.
- `pairing::JoinFailure` gains `Unreachable`.

### D13. Clocks

- `Millis` is the time since `Node::start`, from `tokio::time::Instant`.
- `UnixTime` comes from `SystemTime::now()`, and is 0 if the clock is before 1970.
- Both are read once per event.

### D14. Dependencies

In `transport`:
- `quinn` 0.11 (`default-features = false`; `runtime-tokio`, `rustls-ring`, `log`)
- `rustls` 0.23 (`default-features = false`; `ring`, `std`, `tls12` off)
- `rcgen` 0.14 (`default-features = false`; `crypto`, `ring`)
- `mdns-sd` 0.21
- `if-addrs` 0.15
- `tokio` 1 (`rt-multi-thread`, `macros`, `sync`, `time`, `net`)
- `proto`, `model`, `pairing`

`ring` is used rather than `aws-lc-rs`, which needs CMake and NASM on Windows. `ring` needs clang only when building for ARM64, so bootstrap adds the `Microsoft.VisualStudio.Component.VC.Llvm.Clang` component and CI runs `cargo check -p transport --target aarch64-pc-windows-msvc`.

### D15. Wire protocol

There are no new message types. Close reason `5` (not a member) joins `proto::CloseReason`. `PROTOCOL_VERSION` stays 1, because version 1 has not shipped. `docs/protocol.md` gains a transport section covering the port, the ALPNs, certificate contents, the control stream, the order on member connections, mDNS records, and the one-connection rule.

## Risks / Trade-offs

- **A 1 s idle timeout on flaky Wi-Fi can report a peer lost after a short dropout.** Losing a peer releases held keys and forces a reconnect.
  - The value is a named constant, and reconnecting starts after 0.5 s.
  - Real-world tuning belongs to the spike already planned for sleep and locking.
- **Windows' own mDNS responder also binds UDP 5353.** `mdns-sd` binds with address reuse and coexists with it in practice, but service-to-service multicast can be blocked by the firewall profile.
  - The installer rule must allow 5353 inbound for the service as well as 47391, and this change records that for `add-installer`.
  - Remembered addresses cover the gap.
- **Loopback tests run in real time.** quinn timers can't be paused, so the liveness tests take about 1 to 3 seconds each. Their bounds allow for scheduler jitter: lost between 1 and 2 seconds, and checked with a 3-second ceiling.
- **The library choice for reading certificates is open until apply (D2).** Either option keeps the spec's behavior, and the property tests (wrong key, stolen certificate) run against whichever is used.
- **Running tests without the installer's firewall rule.** On a developer machine, binding `0.0.0.0` can raise a Windows Firewall prompt. Tests bind `127.0.0.1` with a port per test.
- **A stranger can make the listener do a TLS handshake.** That handshake costs it an Ed25519 signature and verification. The limit on strangers bounds the state they can hold, but not the handshake rate; rate limiting is left to the firewall and to a later change if it is ever needed.

## Migration Plan

Nothing has shipped, so there is nothing to migrate. To roll back, revert the PR. The workspace's `mdns-sd` pin moves from 0.13 to 0.21; nothing used it yet.
