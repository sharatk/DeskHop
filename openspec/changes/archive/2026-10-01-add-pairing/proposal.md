# Proposal

## Why

Nothing yet decides which machines may connect: the transport, input forwarding and the service all need to know who is trusted. ADR 0004 Amendment 2 settles the shape: identity keys pinned in QUIC's TLS, a typed digit code authenticated by SPAKE2, and a desk that a new machine joins through any one member. This change builds that logic, without networking, so it can be tested in memory before `add-discovery-transport` puts it on QUIC.

## What Changes

- **Identity.** Each machine has one Ed25519 identity key, generated once. Its public key is the machine's `PeerId`. The key is exported in the form `transport` needs to build its TLS certificate.
- **Pairing code.** All digits: `<locator>-<secret><check>`.
  - The locator is the host part of the displaying machine's IPv4 address: 1 byte on a /24, 2 on a /16, 3 on a /8. The typing machine rebuilds the full address from its own prefix.
  - The secret is 6 random digits. One check digit (the Damm algorithm) catches a mistyped digit or two swapped neighbours before an attempt is spent.
  - Spaces and extra hyphens in the secret are ignored when typed.
- **Pairing exchange.** A sans-I/O state machine for each side: the *inviter* shows the code, and the *joiner* types it.
  - The joiner sends its SPAKE2 message first, then both sides send theirs.
  - Each side sends a key-confirmation tag. It is derived with HKDF from the SPAKE2 key, the TLS exporter output and both certificate keys, so the code is never sent and a relay in the middle fails.
  - The inviter checks the joiner's tag before sending its own. The tags carry each machine's name.
- **Pairing limits.**
  - The inviter accepts pairing only while pairing mode is open, for at most 10 minutes.
  - It handles one attempt at a time. A connection that sent its SPAKE2 message and doesn't complete counts as a failed attempt.
  - After 3 failed attempts pairing mode closes, and the user opens it again for a new code. That allows 3 guesses in a million each time it is opened.
  - The joiner learns why pairing failed: wrong code, or the machine wasn't ready to pair.
- **Desk membership.**
  - After pairing, each side signs a membership record for the other and sends every record it holds. Two machines that are already in desks merge them; there are no roles.
  - A machine accepts a record signed by any current member. Of a machine's records, the one with the highest epoch wins; on a tie, removal wins. Epochs are the signer's Unix time (or one above what it holds, if later). That keeps a removed machine out against stale records, and lets a deliberate re-pair bring it back.
  - "Remove this PC" works from any member. A machine that receives its own removal forgets the desk.
  - Members exchange records on every connection, ending with a marker, so a member that was offline catches up.
- **Storage formats.** The service writes the identity key file and the trust store file in a versioned byte format. The file paths and the permission only SYSTEM can read are for the service and the installer.
- **Wire protocol.**
  - Four new stream message types: `PairPake`, `PairConfirm`, `MemberRecord` and `RecordsDone`.
  - Two new close reasons: `3` for a wrong pairing code and `4` for a machine not ready to pair.
  - `PROTOCOL_VERSION` stays 1, because version 1 has never shipped. `docs/protocol.md` gains the layouts.
- **ADR 0004 Amendment 2** is marked Accepted. It was merged in PR #7.

## Capabilities

### New Capabilities
- `pairing`: identity keys, the pairing code format, the pairing exchange and key confirmation, attempt and time limits, and the identity key file format.
- `desk-membership`: signed membership and removal records, how records are accepted and conflicts resolved, merging desks, record exchange between members, and the trust store file format.

### Modified Capabilities
- `wire-protocol`:
  - "Message type registry": version 1 now defines the pairing and membership message types alongside `Hello`.
  - "Close reasons": gains codes `3` and `4`.

## Impact

- **Crates:**
  - **`proto`:** the new message types, their layouts and the close reasons. No new dependencies; it stays the only crate that defines wire types.
  - **`pairing`:**
    - New code: identity, codes, the pairing state machines, membership records, the trust store and the file formats.
    - It depends on `proto` and `model` (for `PeerId` and `Millis`).
    - External crates, all on the stack that `spake2` 0.4 uses, so there is one copy of each: `spake2` 0.4, `ed25519-dalek` 2, `hkdf` 0.12, `sha2` 0.10, `rand_core` 0.6 (OS random numbers), `subtle` and `zeroize`.
  - **No new dependencies in `model` or `engine`.**
- **Dependency rules hold:**
  - `engine` still doesn't reach `pairing`, `transport` or `ipc`.
  - `pairing` doesn't touch `win32-*` and forbids `unsafe`.
  - `rustls`, certificate generation and the verifier that pins keys belong to `add-discovery-transport`.
- **No UI in this change:**
  - No layout grid, no server/client toggle and no IP address field. The locator is part of the code, and the user never sees or types an address.
  - Opening pairing mode and typing the code come in `add-onboarding`.
- **Docs:**
  - `docs/protocol.md`: the message layouts, the close reasons and the version history.
  - `docs/threat-model.md`: replace "bounded" with the actual limits.
  - ADR 0004: status set to Accepted.
- **Later changes:**
  - `add-discovery-transport`: the self-signed certificate, the verifier that pins keys, the TLS exporter, sending records to connected members as soon as they change, and running the state machines on QUIC.
  - `add-ipc-and-service`: file paths, atomic writes and the "Remove this PC" command.
  - `add-installer`: the permission on the files that only SYSTEM can read.
  - `add-onboarding`: the pairing screens.
