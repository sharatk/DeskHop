# Tasks

## 1. `proto`: pairing and membership messages

- [x] 1.1 Register `PairPake` (`0x0002`), `PairConfirm` (`0x0003`), `MemberRecord` (`0x0004`) and `RecordsDone` (`0x0005`) as stream types since version 1, and replace `version_1_defines_only_hello`. Verify with registry unit tests that version 1 defines exactly the five types, and `pairing_message_in_a_datagram_is_rejected`.
- [x] 1.2 Add `proto::pairing`: typed `PairPake`, `PairConfirm`, `MemberRecord` (borrowing its name) and `RecordsDone`, with strict decode, encode, and the bytes a record signature covers. Verify with unit tests: round trips; removal is 138 bytes; unknown kind; addition with no name; removal with a name; name of 64 bytes; invalid UTF-8; `RecordsDone` with a payload; `PairPake` of 32 and 34 bytes.
- [x] 1.3 Add `CloseReason::WrongPairingCode` (`3`) and `CloseReason::NotReadyToPair` (`4`). Verify that the close-code round-trip and spec-value tests cover all five codes, and that `from_code(5)` is `None`.
- [x] 1.4 Extend the never-panic tests and the `decode_stream` fuzz target to decode every message's payload by type. Verify that `cargo test -p proto` passes, `cargo +nightly fuzz build` succeeds in `tools/fuzz`, and `crates/proto/Cargo.toml` still has no dependencies.
- [x] 1.5 Update `docs/protocol.md`:
  - the four payload layouts with byte diagrams;
  - the SPAKE2 algorithm (group, symmetric-mode constant S, identity string, transcript hash, password) and the key-confirmation derivation, so another implementation can reproduce them;
  - close reasons `3` and `4`;
  - the version-history row for version 1.

## 2. `pairing`: crate setup and identity

- [x] 2.1 Add `spake2` 0.4, `ed25519-dalek` 2 (`rand_core`, `zeroize`), `hkdf` 0.12, `sha2` 0.10, `rand_core` 0.6 (`getrandom`), `subtle` 2 and `zeroize` 1 to the workspace dependencies, and to `pairing` with `proto` and `model`. Verify that `cargo archcheck` passes and that `cargo tree -d -p pairing` shows a single version each of `curve25519-dalek`, `sha2` and `rand_core`.
- [x] 2.2 Add `Identity`: generate from a supplied random source, give its `PeerId`, sign, encode and decode the 37-byte `DHID` file, and keep the seed in a buffer that is wiped on drop. Verify with unit tests `first_start_creates_an_identity`, `identity_survives_a_restart` and `corrupt_identity_file` (wrong length, magic, version).

## 3. `pairing`: codes

- [x] 3.1 Add the Damm check digit, and code generation from (IPv4 address, prefix length, random source) with the locator's byte count set by the prefix. Verify with unit tests: Damm's published example (`572` â†’ `4`); `code_on_a_24_network`; `code_on_a_16_network`; prefixes /8, /23 and /28; and that the secret is uniform over 000000â€“999999, using a random source that returns rejected values first.
- [x] 3.2 Add code parsing. Verify with unit tests: `typed_with_spaces_and_extra_hyphens`; `one_mistyped_digit` (every single-digit change and every adjacent swap of one code is rejected); `locator_out_of_range`; a missing locator; 5 locator bytes; 6 or 8 digits; and a letter.
- [x] 3.3 Add rebuilding the inviter's address from the joiner's own IPv4 address. Verify with unit tests `same_24_subnet` and `two_byte_locator`.

## 4. `pairing`: the pairing exchange

- [x] 4.1 Add the SPAKE2 step (symmetric mode, identity `deskhop pair v1`, password the canonical code text) and the key-confirmation tags from design D5, compared in constant time. Verify with a known-answer test pinned to a fixed random seed, and unit tests showing that the joiner's and the inviter's tags differ and that changing any binding input changes both.
- [x] 4.2 Add `Joiner` and `InviterAttempt` as sans-I/O state machines (decoded messages and `now` in; send, close, or completion with the peer's identity, name and records out; next deadline reported). Build an in-memory test harness that passes messages between them with chosen exporter values and certificate keys. Verify with harness tests `matching_codes`, `wrong_code`, `message_out_of_order`, `relay_in_the_middle`, `reflected_tag` and `empty_name`.
- [x] 4.3 Add `PairingMode`: opened by the user with a new code, a 10-minute window, one attempt at a time, the 30-second attempt limit, failed-attempt counting, closing after 3 failures, and closing after a success. Verify with harness tests `pairing_mode_closed`, `window_expires`, `third_wrong_guess_closes_pairing_mode`, `two_wrong_guesses_leave_pairing_mode_open`, `abandoned_attempt`, `concurrent_attempt`, `one_join_per_opening`, and a dropped connection counting as a failed attempt.

## 5. `pairing`: desk membership

- [x] 5.1 Add signing and verification of membership records over `proto`'s signed bytes, with `verify_strict`. Verify with unit tests `valid_signature`, `tampered_record` (every byte position) and `self_signed_addition`.
- [x] 5.2 Add `Desk`, the membership view: the winning record per subject (highest epoch; removal wins a tie), this machine always a member, at most 256 subjects, and `Machine::add` and `Machine::remove` choosing the epoch from the wall clock. Verify with unit tests `later_removal_wins`, `stale_addition_loses`, `removal_wins_a_tie`, `re_pairing_a_removed_machine` and the 257th subject being refused.
- [x] 5.3 Add `Desk::apply`: accept a record only when its signature is valid and its signer is a member, retrying until nothing more is accepted, and keeping records whose signer is later removed. Verify with unit tests `record_from_a_stranger`, `records_out_of_dependency_order` and a record kept after its signer's removal.
- [x] 5.4 Connect completed pairing to `Desk`: each side signs an addition of the other, adds it to its own view, then sends every winning record followed by `RecordsDone`. Verify with multi-machine harness tests `third_machine_joins_through_one_member` and `two_desks_merge`.
- [x] 5.5 Add removal, including removing this machine, and forgetting the desk when a removal of this machine wins. Verify with harness tests `remove_from_any_member` and `removed_machine_forgets_the_desk` (the identity is unchanged).
- [x] 5.6 Add the receiving side of the record exchange: buffer up to 256 records before `RecordsDone`, apply them on `RecordsDone`, and apply a later single `MemberRecord` immediately. Verify with harness tests `offline_member_catches_up` and `record_flood` (257 records is a protocol error).
- [x] 5.7 Add trust store file encoding and decoding (`DHTS`, version, count, length-prefixed records; every signature checked). Verify with unit tests `round_trip` and `corrupt_trust_store` (truncated; bad signature; wrong magic or version).

## 6. Docs and checks

- [x] 6.1 Update `docs/threat-model.md`: replace "bounded" with the limits (10 minutes, 30 seconds, 3 attempts, 256 records), and add record divergence after a signer is removed as residual risk.
- [x] 6.2 Mark ADR 0004 Amendment 2 Accepted (status header and amendment status).
- [x] 6.3 Add `-p pairing` to the Linux CI job and to AGENTS.md's Linux test command, since `pairing` has no Windows code. Verify that the CI workflow file and AGENTS.md agree.
- [x] 6.4 Run `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` and `cargo archcheck`. Then break one rule at a time (the tie rule, the signer check, the attempt limit, the tag binding) and confirm that only the matching tests fail.
