# Proposal

## Why

Every later change (edge transitions, motion, pairing, clipboard) adds messages to the peer wire protocol, and none can be specified until there is an envelope to put them in: how bytes are split into frames, how a message says what it is, and how two peers running different DeskHop versions agree on what to speak. `proto` is also the code that parses untrusted network bytes inside a SYSTEM service, so the envelope has to be small, dependency-free, and robust from the first commit.

## What Changes

- Define the stream frame: a fixed 6-byte header (payload length, message type) followed by the payload, with a hard size limit checked before any payload is buffered.
- Define the datagram envelope: a message type followed by the payload, one message per QUIC datagram.
- Define the message-type registry, the channel (stream or datagram) each type is allowed on, and strict rejection of unknown types and malformed payloads.
- Define the version handshake: both peers send `Hello` (lowest and highest version they speak) as the first frame on the control stream and use the highest version in common. No overlap closes the connection with a version-mismatch reason that says which side is older.
- Define connection close reasons as QUIC application error codes.
- Make decoding total: arbitrary input never panics and never allocates based on a length the peer claims. Enforce this with lints in `proto`, a deterministic never-panic test, and a cargo-fuzz target run nightly in CI.
- Encoding is hand-written little-endian. `proto` keeps zero dependencies.
- Document the wire format in `docs/protocol.md` so a third party can implement it (ADR 0003, ADR 0004).

No input, layout, pairing, or clipboard payloads are defined here; each later change adds its own messages.

## Capabilities

### New Capabilities
- `wire-protocol`: framing on streams and datagrams, the message-type registry, the version handshake and negotiation, close reasons, and decoding robustness.

### Modified Capabilities
_None; there are no existing specs._

## Impact

- **Crates:** `proto` only. It stays I/O-free and gains no dependencies. `engine` already depends on `proto` and is unaffected. Nothing in `win32-*`, `transport`, or `ipc` changes. The dependency rules in AGENTS.md hold.
- **New tooling:** `tools/fuzz` (cargo-fuzz targets for frame and datagram decoding), excluded from the Cargo workspace because it needs nightly Rust. Its `libfuzzer-sys` dependency does not reach `proto`.
- **CI:** a scheduled nightly Linux job runs the fuzz targets for a fixed time.
- **Docs:** `docs/protocol.md` gains the wire format; `docs/threat-model.md` gains the decoding defenses for the LAN → service boundary.
- **Protocol version:** stays at 1. Version 1 has never shipped and had no messages; this change defines what version 1 contains.
- **Consumers:** `add-discovery-transport` will choose which QUIC stream is the control stream and map close reasons to QUIC close calls. This change specifies the behavior, not the I/O.
