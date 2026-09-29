# ADR 0004 — Symmetric peers, mDNS + pairing code, QUIC over Noise

**Status:** Accepted (shape); details to be settled in `openspec/` changes. Amendment 1 (pairing) Proposed 2026-09-28. Layout refined by ADR 0005.
**Date:** 2026-09-27

## Context

Synergy's configuration pain comes from its protocol shape: a designated server, clients pointed at an IP, TLS certificates managed by hand, and a screen grid the user fills in. The zero-config promise requires the protocol to remove each of those, not the UI to hide them.

Input has two traffic classes. Mouse motion is high-rate, loss-tolerant, and stale after a few milliseconds. Keys, focus changes, layout updates, and clipboard are low-rate and must arrive in order.

## Decision

- **Symmetric peers.** No server/client roles. Whichever machine has physical input is the source; the role moves with the cursor.
- **Discovery** by mDNS/DNS-SD, with a **pair-by-code fallback** that carries an IP hint for networks that block multicast.
- **Pairing** with a short code shown on both screens, establishing long-lived identity keys via a Noise handshake (`snow`). The trust store is per machine; no CA, no certificates. *Superseded by Amendment 1.*
- **Transport** is QUIC (`quinn`): unreliable datagrams for coalesced motion, ordered streams for everything else. One connection per peer pair.
- **Layout** is a graph of edge→peer transitions, learned by use: the first time the cursor exits an edge, the UI asks which machine is there. Machines with several monitors expose their virtual-screen edges. *Refined by ADR 0005.*
- **`proto`** carries a protocol version in every handshake and is the only crate that defines wire types. It has no I/O.
- **LAN only** in v1. No relay, no NAT traversal.

## Alternatives considered

- **TLS over TCP + raw UDP.** Works on a LAN; rejected because QUIC gives both traffic classes on one authenticated connection with a single handshake.
- **Designated server** (Synergy, Mouse Without Borders). Simpler to reason about; rejected because it is the root of the configuration burden.
- **Clipboard over a separate channel.** Unnecessary; a QUIC stream per transfer suffices.

## Open questions for spec changes

- Noise handshake pattern and pairing-code length/entropy.
- Motion coalescing interval and datagram payload layout.
- Behavior when two machines both have physical input at once.
- How a machine with multiple monitors presents edges during layout learning.
- Fuzzing strategy for `proto` decoding in the service.

## Consequences

- A third party could write a compatible client from `docs/protocol.md`; that is intended.
- Corporate networks that block multicast degrade to pair-by-code, never to typing IP addresses into a grid.
- The engine is unaware of transport; it consumes `model` events and emits routing decisions, which keeps it testable on Linux.

## Amendment 1 — Pairing by typed code with a PAKE

**Status:** Proposed
**Date:** 2026-09-28

### Context

The original decision describes a code "shown on both screens", which the user compares. The pair-by-code fallback describes a code the user types on the other machine. These need different cryptography. For v1 one flow is enough, and a typed code covers both the normal and the fallback case.

A short typed code cannot be used directly as a Noise pre-shared key: anyone who records one pairing can brute-force the code offline.

### Decision

- **Pairing uses a typed code.** One machine displays a short code; the user types it on the other. The same flow is used whether or not mDNS found the peer; on networks that block multicast the code also carries an IP hint.
- **The code authenticates through a PAKE** (a password-authenticated key exchange, such as CPace or SPAKE2), so an attacker gets one online guess per pairing attempt and nothing to brute-force offline.
- **Identity keys are unchanged.** Pairing still establishes long-lived identity keys via Noise (`snow`) and stores them in the per-machine trust store. The PAKE authenticates that exchange; how the two compose is settled in the `pairing` spec.
- The PAKE implementation is a dependency of `pairing` only. `proto`, `model`, and `engine` are unaffected.

### Alternatives considered

- **Compared code** (derived from the handshake hash, shown on both screens, like Bluetooth numeric comparison). Needs no PAKE and resists offline attack. Deferred: v1 uses one flow, and the fallback needs a typed code anyway.
- **Typed code as a Noise pre-shared key.** Simplest; rejected because a recorded handshake allows offline brute force of a short code.

### Consequences

- `pairing` gains one cryptographic dependency, chosen in the `pairing` spec with a stated reason.
- The open question "Noise handshake pattern and pairing-code length/entropy" becomes: which PAKE, how it composes with the Noise handshake, and code length.
- `docs/threat-model.md` records online guessing as the residual pairing risk, with a limit on attempts.
