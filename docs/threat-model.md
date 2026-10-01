# DeskHop threat model

*Status: skeleton. Filled in alongside the `pairing`, `transport`, and `ipc` specs.*

DeskHop injects keystrokes received over the network, and its service runs as SYSTEM. This document records what it defends against and where.

## Assets

- Identity keys and the trust store (service).
- Keystrokes and clipboard contents in transit.
- The ability to inject input into a machine, including the secure desktop.

## Trust boundaries

| Boundary | Less trusted side | Notes |
|---|---|---|
| LAN → service | Any host on the network | Unauthenticated packets reach `proto` decoding as SYSTEM. |
| Named pipe → service | Agents, UI | The service validates everything it receives (ADR 0002). |
| Service → winlogon agent | — | Agent runs as SYSTEM on the secure desktop. |

## Threats

### Hostile bytes on the wire (LAN → service)

A host on the LAN, or a paired peer that is buggy or compromised, sends malformed frames or datagrams to make the SYSTEM service crash, hang, or exhaust memory. Connection authentication (`pairing`, `transport`) narrows who can send frames at all; `proto` does not rely on it.

Defenses, specified in `openspec/specs/wire-protocol/spec.md`:

- **Bounded memory.** A frame's payload length is checked against the 65,536-byte limit from the 6-byte header alone, before any payload is buffered. Decoders borrow from the caller's buffer and never allocate from a length the peer claims. Datagrams are capped at 1,200 bytes.
- **Strict parsing.** Unknown message types, types on the wrong channel, and payloads that do not match their layout close the connection with a protocol error, not a best-effort guess. Only `Hello` tolerates trailing bytes, so that later versions can extend it.
- **No panics by construction.** `proto` denies indexing, `unwrap`, `expect`, `panic`, and unchecked arithmetic outside tests; CI builds with `-D warnings`.
- **Tested against arbitrary input.** A deterministic never-panic test runs in every `cargo test`, and cargo-fuzz targets for stream and datagram decoding run nightly in CI.

Residual risk: a connected peer can keep the connection busy with valid traffic. Rate limiting belongs to `transport`.

### Guessing the pairing code (LAN → service)

A host on the LAN connects while a machine is in pairing mode and tries codes. SPAKE2 gives it one online guess per attempt and nothing to brute-force offline; a relay between two pairing machines fails key confirmation, which is bound to the TLS exporter and both certificate keys (ADR 0004, Amendment 2). Pairing mode opens only when the user asks, closes after a bounded time, and replaces the code after a bounded number of failed attempts. Exact limits: `pairing` spec.

Residual risk: a few online guesses per code, by a host on the same LAN, while the user has pairing open.

### Desk membership

Any member of a desk can add a machine: the others accept a membership record signed by any member (ADR 0004, Amendment 2). A compromised member can therefore add a machine of the attacker's choosing. It can already inject input into every member, so this does not widen what a compromised member can do. Removal records are kept, so a removed machine cannot return through a stale membership record.

### Identity key at rest

The identity key and trust store are files that only SYSTEM can read. A local administrator can read or replace them. A local administrator can already inject input and read the clipboard, so this is outside the threat model; DPAPI was rejected for the same reason.
