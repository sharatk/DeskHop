# Design

## Context

- **`proto`:** has framing, a message registry holding only `Hello`, version negotiation, and a sans-I/O `Session`. It has no dependencies and denies panics and unchecked arithmetic outside tests.
- **`model`:** already defines `PeerId([u8; 32])` and `Millis`.
- **`pairing` and `transport`:** empty stubs.
- **ADR 0004 Amendment 2:** fixes the cryptographic shape: Ed25519 identities pinned in QUIC's TLS, SPAKE2 with key confirmation bound to the connection, and desk membership through signed records.

This change builds the parts that need no network. `add-discovery-transport` later supplies the TLS exporter value, the certificate keys and the connection.

## Goals / Non-Goals

**Goals:**
- Every rule in the `pairing` and `desk-membership` specs can be tested in memory: two state machines exchanging `proto` messages, with a clock and a random-number source the test controls.
- `proto` decodes the new messages without panicking on any input, like the rest of `proto`.
- Key material is wiped from memory when dropped.

**Non-Goals:**
- QUIC, the self-signed certificate, the verifier that pins keys, mDNS, and sending records to members as soon as they change. These belong to `add-discovery-transport`.
- File paths, atomic writes, and the permission only SYSTEM can read. `pairing` only encodes and decodes the file contents.
- Choosing a machine's name or network adapter. The caller passes in the name, the IPv4 address and the prefix length.

## Decisions

### D1. Sans-I/O state machines driven by the caller's clock

`pairing` exposes three types:
- `PairingMode`: the inviter's single slot. It holds the code, the window deadline, the failed-attempt count and the attempt in progress.
- `InviterAttempt`: one connection on the inviter's side.
- `Joiner`: the typing side.

Each takes decoded `proto` messages and `now: Millis`, and returns actions: send a message, close with a reason, or pairing complete with the peer, its name and its records. Each also reports its next deadline, so the caller only has to call it again at that time, the way the engine decides time-based rules from event timestamps.

*Alternative:* async functions over a stream trait. That would tie `pairing` to `tokio` and make the 30-second and 10-minute rules depend on wall-clock time in tests.

### D2. Wire types live in `proto`; cryptography lives in `pairing`

- **`proto::pairing`** defines `PairPake`, `PairConfirm`, `MemberRecord` (it borrows the name from the frame) and `RecordsDone`. It covers their checked layouts and the bytes a record's signature covers (`deskhop member record v1` followed by the record without its signature). It still has no dependencies, and UTF-8 is checked with `core::str::from_utf8`.
- **`pairing`** signs, verifies and derives keys.
- The new close reasons extend `proto::CloseReason`.

*Alternative:* define the records in `pairing`. That would break AGENTS.md's rule that `proto` is the only crate defining wire types, and leave the record format outside the fuzzed decoders.

### D3. One cryptographic stack, set by `spake2`

`spake2` 0.4 builds on `curve25519-dalek` 4, `sha2` 0.10, `hkdf` 0.12 and `rand_core` 0.6. `pairing` uses the matching versions:
- `ed25519-dalek` 2, with features `rand_core` and `zeroize`
- `hkdf` 0.12
- `sha2` 0.10
- `rand_core` 0.6 with `getrandom`, for `OsRng`
- `subtle` 2, for comparing tags in constant time
- `zeroize` 1

That way each primitive is compiled once.

*Alternative:* `ed25519-dalek` 3. It would pull in a second copy of `curve25519-dalek` (version 5) and of `sha2` (0.11): twice the cryptographic code to audit, and two copies of the same curve. Revisit when `spake2` moves to the newer stack.

### D4. SPAKE2 inputs

- Symmetric mode, with the identity string `deskhop pair v1`.
- The password is the UTF-8 bytes of the code's canonical text, `<locator>-<secret><check>` with no spaces, so both sides compute the same password from the typed and the displayed code.
- In symmetric mode the message is 33 bytes: one side byte and one compressed Edwards point. That fixes `PairPake` at 33 bytes, and `docs/protocol.md` describes the algorithm for anyone writing another implementation.

### D5. Key confirmation

```
PRK      = HKDF-Extract(salt = "deskhop pair v1", IKM = SPAKE2 key)
binding  = exporter (32 bytes) || inviter cert key (32) || joiner cert key (32)
tag_J    = HKDF-Expand(PRK, "joiner"  || binding, 32)
tag_I    = HKDF-Expand(PRK, "inviter" || binding, 32)
```

- `transport` will produce the exporter value with the label `EXPORTER-deskhop-pair-v1`, an empty context and a length of 32. `pairing` takes it as an input.
- The two role labels mean a tag can't be reflected back.
- The inviter sends its tag only after the joiner's verifies, so a wrong guess learns nothing.
- Tags are compared with `subtle::ConstantTimeEq`.

*Alternative:* an HMAC over the transcript. That adds nothing here: the SPAKE2 key already covers both messages, and HKDF's info field carries the binding.

### D6. Attempt accounting

- `PairingMode` holds at most one `InviterAttempt`. A `PairPake` that arrives while one is in progress, or while the mode is closed, gets close reason `4` and the attempt count doesn't change.
- An attempt is failed when any of these happens: a wrong tag, a protocol error, the connection dropping (the caller reports it), or 30 s passing from `PairPake` to the end of the record exchange. A timed-out attempt closes its connection with close reason `1`, protocol error, since the joiner stalled the exchange.
- After the third failed attempt, `PairingMode` closes with `ModeClosed::TooManyFailures`, and its code stops working. Drawing a new secret instead was tried first and rejected: wrong guesses fail in milliseconds, so the attacker could keep guessing against fresh codes for the whole 10 minutes (about a 6% chance at 100 attempts a second). Closing allows 3 guesses in a million per opening. Every close reports a `ModeClosed` reason (`Paired`, `Expired`, `ByUser`, `TooManyFailures`) so the UI can say why.
- A completed attempt closes the mode.

### D7. Code format and the check digit

- **Byte count:** the locator has `4 - min(prefix / 8, 3)` bytes.
- **Check digit:** the Damm algorithm, using the standard 10×10 table from Damm's 2004 thesis, over the locator's decimal digits followed by the secret's. Damm catches every single-digit error and every swap of two adjacent digits, and needs no extra character, unlike Luhn and Verhoeff (Verhoeff also needs a longer table).
- **Parsing:** removes whitespace, splits at the first hyphen, drops later hyphens, then checks the parts.
- **Generating the secret:** reject sampling from `OsRng` gives a uniform result over 000000 to 999999.

### D8. Membership view

- `Desk` holds `BTreeMap<PeerId, SignedRecord>`, keeping the winning record for each subject, plus the machine's own identity.
- `apply(records)` works through the records repeatedly until nothing more can be accepted: verify each signature, check the signer is a member, keep the record if it wins. It then returns what changed: added, removed, or, when a removal of this machine wins, "forgotten".
- `Machine::add` and `Machine::remove` sign with the epoch max(held + 1, Unix time in seconds), at least 1. The caller passes the wall-clock time, as it passes the monotonic clock. A counter alone was tried first and failed: a member that never received a removal re-paired the machine at a lower epoch, and the next exchange removed it again. With the wall clock, the later action wins on every member whichever member signed it; clocks that disagree by more than the gap between a removal and a re-pair can still pick the wrong one (see Risks).
- The 256-subject cap applies both in `apply` and in the exchange counter.
- Signatures are checked with `verify_strict`, which rejects weak keys and signatures that aren't in canonical form.

### D9. File formats

- **Identity file:** `DHID`, `1`, then the 32-byte seed. That's 37 bytes.
- **Trust store file:** `DHTS`, `1`, a count as u16, then each record as a u16 length followed by its `MemberRecord` encoding.
- Decoding checks every record's signature, but not who signed it, since the file comes from the machine's own disk.
- Both formats are encode and decode functions over bytes, so the service decides how they're written.
- The seed sits in a `Zeroizing` buffer while being encoded or decoded.

### D10. `PeerId` comes from `model`

`pairing` depends on `model` for `PeerId` and `Millis`, so the engine, the transport and the trust store share one type. `model` gains nothing and stays free of dependencies.

### D11. Protocol version stays 1

Version 1 has never shipped, so its registry grows instead of moving to version 2. `add-input-forwarding` will add the engine messages the same way. `docs/protocol.md` gets the four layouts, close reasons `3` and `4`, and an updated version-history row.

## Risks / Trade-offs

- **`spake2` has not been audited, and other implementations would have to match its symmetric mode.**
  - It runs only while pairing mode is open, and only in `pairing`.
  - The crate's own test vectors run in our CI, through a known-answer test pinned to a fixed random seed.
  - `docs/protocol.md` describes the algorithm (group, symmetric-mode constant S, transcript hash) so that other implementers can reproduce it.
- **Records can diverge across members.** A record signed by a member that is removed before the record reaches everyone is accepted by some members and dropped by others.
  - This is rare: it needs an addition and a removal in flight at once.
  - The fix is to remove and pair again. The threat model records it.
- **Epochs depend on clocks roughly agreeing.** If a removal and a re-pair of the same machine happen closer together than the clocks of the two signers disagree, the earlier action can win. Windows keeps clocks in sync by default, and the user can repeat the action. A machine never signs below an epoch it holds, so its own actions stay ordered even if its clock steps back.
- **Pinned to older RustCrypto versions.** These are the versions `spake2` needs. Updating `spake2` means updating `ed25519-dalek`, `hkdf` and `sha2` in one PR; D3 records the rule.
- **A machine name can be at most 63 bytes.** Longer names are cut at a character boundary by the caller (`add-ipc-and-service`). Windows DNS host names fit within 63 bytes.
- **The locator assumes both machines are on the same IPv4 subnet.** Across subnets, the address built from the code is wrong and the connection fails. Discovery over mDNS can still match the locator against the addresses it has seen; that is `add-discovery-transport`'s job.
- **Clock:** `Millis` must be monotonic. `transport` takes it from `Instant`, not from wall-clock time.

## Migration Plan

Nothing has shipped, so there is nothing to migrate.
- The `proto` registry grows within version 1.
- The `pairing` stub gains its first code.

To roll back, revert the PR.

