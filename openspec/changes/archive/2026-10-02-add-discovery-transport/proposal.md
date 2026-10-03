# Proposal

## Why

`add-pairing` built pairing and desk membership as state machines with no networking. Nothing connects two machines yet: no QUIC, no certificate that carries the identity key, no pinning, no discovery. This change puts them on the network, so two PCs on a LAN can pair by code, find each other again after a restart, notice when the other goes away, and keep their membership views in step. It is the last piece before input can cross between machines (`add-input-capture`, `add-input-forwarding`).

## What Changes

- **QUIC endpoint on a fixed port.** Every machine listens on UDP 47391 (unassigned by IANA, below Windows' ephemeral range), so a pairing code's locator plus that port is enough to connect when multicast is blocked. The installer opens this port later (`add-installer`).
- **Identity in TLS.**
  - Each machine presents a self-signed certificate holding its Ed25519 identity key, regenerated at every start. Only the key matters.
  - Both sides present certificates. A dialer pins the key it expects. A listener accepts any key at the TLS level and decides after the handshake.
- **Two kinds of connection, told apart by ALPN.**
  - `deskhop/1` is a member connection: `Hello`, then both sides exchange membership records.
    - A key the listener doesn't know yet may introduce itself with records signed by members. This covers a machine that joined through another member.
    - If the key is still not a member after its records, the listener closes with a new close reason, `5` (not a member).
  - `deskhop-pair/1` is a pairing connection: `Hello`, then the pairing exchange from `add-pairing`.
- **Connection rules.**
  - One connection per pair of machines. When both dial at once, both keep the connection dialed by the lower peer identity.
  - A peer counts as up once its record exchange completes.
- **Liveness.**
  - A keepalive every 250 ms, and a peer with nothing heard for 1 s is lost.
  - A goodbye (close reason `0`) marks a peer down at once. The service will send it before sleep or shutdown.
  - Lost members are redialled with backoff, from 0.5 s up to 30 s.
- **Membership in motion.**
  - New or changed records are pushed to connected members at once.
  - A removed member's connection is closed with reason `5`.
  - The trust store and each member's last-known addresses are reported for the service to store, and fed back at start-up. That lets machines on networks that block multicast find each other again after a restart.
- **Bounded exposure to strangers.**
  - A connection from an unknown key has 10 s to pair or introduce itself.
  - At most 8 such connections are open at once; more are refused.
- **Discovery (mDNS).**
  - Each machine advertises `_deskhop._udp.local.` with its peer identity in TXT, plus `pair=1` while pairing mode is open, and browses for others.
  - Members are dialled at the addresses they advertise, and at their last-known addresses.
- **Pairing on the network.**
  - The inviter's code uses the address of the interface that carries its default route, with that interface's prefix length.
  - The joiner dials every candidate at once: the address rebuilt from each of its own IPv4 interfaces, and any `pair=1` advertisement whose IPv4 address ends in the locator. The first candidate that answers the pairing exchange wins.
  - The joiner learns the inviter's identity from its certificate.
- **Wire protocol.** Adds close reason `5`. `PROTOCOL_VERSION` stays 1, because version 1 has never shipped. `docs/protocol.md` gains the transport section: port, ALPNs, certificates, the control stream, mDNS records, and the connection rules.

## Capabilities

### New Capabilities
- `peer-transport`: QUIC on a fixed port, identity certificates and pinning, member and pairing connections, introduction by records, one connection per pair, liveness and goodbye, reconnection, pushing membership changes, limits on unauthenticated connections, and the stored addresses and trust store.
- `discovery`: mDNS advertisement and browsing, choosing the inviter's address for its code, and the joiner's candidate addresses.

### Modified Capabilities
- `wire-protocol`: "Close reasons" gains code `5` (not a member).

## Impact

- **Crates:**
  - **`transport`:** gains all the code.
    - It depends on `proto`, `model` and `pairing`.
    - New external crates: `quinn` 0.11 (no default features; `runtime-tokio`, `rustls-ring`), `rustls` 0.23 (`ring`), `rcgen` 0.14 (`ring`), `mdns-sd`, `if-addrs` 0.15 and `tokio`.
    - The workspace pin for `mdns-sd` moves from 0.13 to 0.21.
  - **`proto`:** close reason `5`.
  - **`pairing`:** small additions only: `JoinFailure::Unreachable` (no candidate answered), and `Applied::accepted` (the records that won, so they can be pushed to members).
- **Dependency rules hold:**
  - `engine` still reaches none of `transport`, `ipc` or `win32-*`.
  - `transport` forbids `unsafe`. Interface enumeration comes from `if-addrs`, whose `unsafe` stays inside that crate.
  - No `cfg` branches for other operating systems.
- **No UI, no grid, no IP address field.** Users still type only the pairing code.
- **Build:** `ring` needs clang to build for ARM64 Windows. Bootstrap adds Visual Studio's Clang component, and CI gains an ARM64 `cargo check`.
- **Docs:** `docs/protocol.md` (the transport section, close reason `5`) and `docs/threat-model.md` (strangers' connections, spoofed mDNS, what an advertisement reveals).
- **Later changes:**
  - `add-ipc-and-service`: runs the node in the service, stores the trust store and addresses, sends goodbye on sleep and shutdown, and maps peer up/down to the engine's `PeerConnected`/`PeerLost`.
  - `add-input-forwarding`: carries engine messages on these connections and motion on QUIC datagrams.
  - `add-installer`: firewall rules for UDP 47391 and mDNS (UDP 5353) for the service.
