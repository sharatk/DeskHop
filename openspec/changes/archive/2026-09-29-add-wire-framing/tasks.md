# Tasks

## 1. `proto` crate setup

- [x] 1.1 Add the panic-freedom lints to `crates/proto/src/lib.rs` for non-test code (`indexing_slicing`, `unwrap_used`, `expect_used`, `panic`, `arithmetic_side_effects`), keeping `#![forbid(unsafe_code)]`; verify `cargo clippy -p proto -- -D warnings` passes
- [x] 1.2 Add `MIN_PROTOCOL_VERSION = 1` beside `PROTOCOL_VERSION = 1`, the frame limit (65,536), the datagram limit (1,200), and the close-reason codes (`0` normal, `1` protocol error, `2` version mismatch); verify `cargo build -p proto` and that `proto/Cargo.toml` still has no dependencies

## 2. Stream frames

- [x] 2.1 Implement frame encoding (6-byte header, then payload) and verify a unit test produces `04 00 00 00 01 00 01 00 01 00` for type `0x0001` with payload `01 00 01 00`, and a 6-byte frame for an empty payload
- [x] 2.2 Implement sans-I/O, zero-copy frame decoding that returns a frame, "need more input", or a protocol error; verify unit tests for the split-header, two-frames-in-one-read, at-limit (65,536), oversized (65,537), and 4,294,967,295-length scenarios, the last two failing from the header alone
- [x] 2.3 Document the frame header and size limit in `docs/protocol.md` and verify the documented byte example matches the test in 2.1

## 3. Datagrams and the type registry

- [x] 3.1 Implement datagram decoding (2-byte type, then payload, 2 to 1,200 bytes total) and verify unit tests for the well-formed 20-byte, 1-byte, and 1,201-byte cases
- [x] 3.2 Implement the type registry for version 1 (`0x0000` reserved, `0x0001` `Hello` on streams) with per-type channel checks; verify unit tests reject type `0x0000`, unknown type `0x0042`, and `Hello` in a datagram
- [x] 3.3 Add the type registry table (type, name, channel, introduced in) and the datagram layout to `docs/protocol.md`; verify it lists exactly `0x0000` (reserved) and `0x0001` `Hello`

## 4. Hello, handshake, and negotiation

- [x] 4.1 Implement `Hello` encoding and decoding (lowest and highest version as `u16le`, trailing bytes ignored); verify this release's `Hello` payload is `01 00 01 00`, a 10-byte payload with valid first 4 bytes decodes, and lowest 0 or lowest > highest is rejected
- [x] 4.2 Implement the per-connection handshake state machine for the control stream (`Hello` must be first and appear once; `Hello` on another stream is an error; other-stream frames before agreement are held; datagrams before agreement are dropped); verify unit tests for each error case, the held stream frame, and the datagram-before-`Hello` drop
- [x] 4.3 Implement pure version negotiation returning the agreed version or a mismatch that says which side is older; verify unit tests for ranges 1–3 vs 2–5 (→ 3), 4–5 vs 1–2 (other older), 1–2 vs 4–5 (this side older), and that swapping the two arguments gives the same agreed version
- [x] 4.4 Enforce strict payload lengths for every message type other than `Hello`; verify with a unit test using a test-only fixed-layout type that a 9-byte payload for an 8-byte layout is rejected
- [x] 4.5 Document `Hello`, negotiation, close reasons, and version 1 in the history table of `docs/protocol.md`; verify the version history row for peer protocol 1 describes framing and `Hello`

## 5. Decoding robustness

- [x] 5.1 Add a deterministic never-panic test in `proto` (in-test xorshift generator, random byte strings and mutations of valid frames and datagrams, fed through frame decoding, datagram decoding, and the handshake state machine); verify it runs in under 10 seconds in `cargo test -p proto` and asserts consumed ≤ input length for every frame
- [x] 5.2 Add the LAN → service decoding defenses to `docs/threat-model.md` (header-checked frame limit, no allocation from claimed lengths, strict payloads, lints, fuzzing); verify the entry references `specs/wire-protocol`

## 6. Fuzzing

- [x] 6.1 Create `tools/fuzz` as a cargo-fuzz project with `decode_stream` (frame decoding plus handshake) and `decode_datagram` targets, each with `#![forbid(unsafe_code)]`, and add `exclude = ["tools/fuzz"]` to the workspace; verify `cargo build --workspace` still succeeds on stable and `cargo archcheck` still passes
- [x] 6.2 Add `.github/workflows/fuzz.yml`: nightly schedule plus manual trigger, nightly Rust on Linux, each target for 300 seconds, crash inputs uploaded as artifacts; verify with a manual run that both targets complete without crashes
- [x] 6.3 Update the fuzzers row in `tools/README.md` from Planned to Ready, with the command to run a target locally; verify the documented command matches the workflow

## 7. Integration checks

- [x] 7.1 Verify `cargo test -p proto -p model -p engine -p policy` passes on Linux CI and `cargo test --workspace` passes on Windows
- [x] 7.2 Verify `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `cargo archcheck` pass
- [x] 7.3 Verify `openspec validate add-wire-framing --strict` passes
