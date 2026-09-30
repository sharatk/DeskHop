# Design

## Context

`proto` today is a stub: a crate doc, `#![forbid(unsafe_code)]`, and `PROTOCOL_VERSION = 1`. It has no dependencies and no I/O, and `engine` depends on it. The service will run its decoder on bytes from the LAN as SYSTEM (ADR 0002), which makes it the first parser on the most exposed trust boundary in `docs/threat-model.md`. By the time bytes reach `proto`, `transport` will already have authenticated the QUIC connection against the trust store (`add-pairing`, `add-discovery-transport`), but a paired peer can still be buggy or compromised, so `proto` does not rely on that.

Requirements are in `specs/wire-protocol/spec.md`; this document covers how to meet them.

## Goals / Non-Goals

**Goals:**
- A sans-I/O `proto`: pure functions and small state machines over byte slices that `transport` drives from its own async code.
- Decoding that no input can make panic, loop, or allocate unboundedly, enforced by the compiler and by tests rather than by review alone.
- A wire format simple enough to document completely in `docs/protocol.md`.

**Non-Goals:**
- Choosing which QUIC stream is the control stream, opening streams, or calling quinn's close. That is `add-discovery-transport`.
- Any message other than `Hello`. Input, motion, layout, pairing, and clipboard messages come with their own changes.
- Encoding helpers for payloads that don't exist yet. Add them with the first message that needs them.

## Decisions

### 1. Hand-written little-endian encoding, zero dependencies
Each layout is written out by hand with `u16::from_le_bytes` and friends over checked slices.
- *Alternative: `serde` + `postcard`.* Less code per message and a stable, documented format, but it adds two dependencies to the audit-surface crate, and derive-generated decoders are harder to review line by line. Rejected; revisit if the message count makes hand-writing error-prone.
- *Alternative: protobuf or flatbuffers.* Code generation and a schema toolchain for a protocol of a handful of fixed-layout messages. Rejected.

### 2. Fixed 6-byte header: `len: u32le | type: u16le`
- *Alternative: varint length.* Saves bytes that don't matter on a LAN, and makes "how many bytes is the header" depend on the input. Rejected.
- *Alternative: `u16` length.* Would cap frames at 64 KiB forever. A `u32` field with a 64 KiB limit enforced separately lets a future version raise the limit without changing the header, which can never change.
- The limit of 65,536 bytes keeps per-stream memory small. Clipboard and file transfer will chunk.

### 3. Sans-I/O, zero-copy decoder
The decoder takes the bytes the caller has buffered and returns one of: a frame (type, borrowed payload slice, bytes consumed), "need more input", or a protocol error. It never owns a buffer and never allocates, so the memory bound in the spec holds by construction: the caller never needs more than header + limit buffered. A per-connection handshake state machine consumes decoded control-stream frames and enforces "`Hello` first, exactly once", and a pure negotiation function takes the two ranges and returns the agreed version or a mismatch saying which side is older.
- *Alternative: an async `Framed` codec in `proto`.* Would pull `tokio` into `proto` and break "no I/O". Rejected; `transport` wraps the sans-I/O decoder.

### 4. Symmetric `Hello`, no acknowledgement
Both peers send `Hello` immediately and compute the same result from the two ranges.
- *Alternative: initiator proposes, responder chooses.* Introduces connection roles, which ADR 0004 removes. It also costs an extra round trip. Rejected.

### 5. Range negotiation, highest common version
Each release advertises `[MIN_PROTOCOL_VERSION, PROTOCOL_VERSION]`. PCs updated days apart keep working as long as the ranges overlap, and the mismatch result says which PC to update.
- *Alternative: exact match.* Every protocol bump would break every pair until both PCs update. Rejected.

### 6. Strict payloads, except `Hello`
Every message's payload must match its layout exactly; leftover bytes are a protocol error, which catches bugs early. `Hello` alone ignores trailing bytes, because it is decoded before any version is agreed: a future version can append fields to `Hello` without breaking older peers.

### 7. One type registry with a channel per type
Stream and datagram messages share one 16-bit namespace, and each type records which channel it may use. Diagnostics and packet captures can name any message from its type alone.
- *Alternative: separate namespaces for streams and datagrams.* The same number would mean two things. Rejected.

### 8. Close reasons are QUIC application error codes
QUIC's CONNECTION_CLOSE already carries an application error code, so `proto` defines the codes (`0` normal, `1` protocol error, `2` version mismatch) and `transport` passes them to quinn.
- *Alternative: a `Close` message before closing.* Duplicates what QUIC provides and can be lost when the connection closes. Rejected.

### 9. Protocol version stays at 1
The project rule is that a change to `proto` bumps the version. Version 1 has never shipped and defined no messages, so no peer anywhere speaks a different version 1; this change defines what version 1 is. Bumping to 2 would leave a version that never existed in the history. The rule applies from the first release onward, and `docs/protocol.md` records version 1's contents.

### 10. Enforcing "never panics"
- **Lints.** `proto` denies `clippy::indexing_slicing`, `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, and `clippy::arithmetic_side_effects` outside tests. Slicing goes through `get(..)`, lengths through `usize::try_from` and checked arithmetic. `cargo clippy -- -D warnings` in CI makes these hard errors.
- **Deterministic test.** A test in `proto` feeds a few hundred thousand byte strings (random bytes from a tiny in-test xorshift generator, plus mutations of valid frames) through every decoder and checks that each call returns a result and consumed ≤ input length. It runs in `cargo test` on stable Rust, on Windows and Linux, with no dev-dependency.
- **Fuzzing.** `tools/fuzz` holds cargo-fuzz targets for stream decoding (including the handshake state machine) and datagram decoding. It is excluded from the Cargo workspace because cargo-fuzz needs nightly Rust; its `libfuzzer-sys` dependency never reaches `proto`. A scheduled GitHub Actions job runs each target for 5 minutes on nightly Linux and uploads any crash inputs as artifacts. `proto` is platform-independent, so fuzzing on Linux covers the Windows build.

### 11. Documentation
`docs/protocol.md` gets the header and datagram layouts, the type registry table (type, name, channel, introduced in), the `Hello` layout and negotiation rule, close reasons, and version 1 in the history. `docs/threat-model.md` gets the LAN → service decoding defenses: frame limit checked from the header, no allocation from claimed lengths, strict payloads, lints, and fuzzing.

## Risks / Trade-offs

- [The frame header and the start of `Hello` can never change] → The header has a `u32` length with room to grow, and `Hello` accepts trailing fields.
- [Strict rejection of unknown types means a buggy peer kills the connection] → Intended: the negotiated version defines the full message set, so an unknown type is always a bug, and failing loudly surfaces it. The close reason says "protocol error" in diagnostics.
- [64 KiB may be too small for some future message] → Capabilities with large data chunk it; the limit can be raised in a later version without a header change.
- [The 1,200-byte datagram cap may exceed what a path supports] → `transport` checks quinn's `max_datagram_size` at send time. Motion payloads are tens of bytes.
- [Keeping version 1 deviates from the project's bump rule] → Decision 9 records why. A reviewer who disagrees can bump to 2 with no other change.
- [Fuzzing runs only nightly, only on Linux] → The deterministic never-panic test runs on every push on both platforms.

## Migration Plan

None. Nothing has shipped. Rollback is reverting the change.
