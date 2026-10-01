# DeskHop protocols

Two versioned protocols. A change to either bumps its version constant and updates this file in the same PR.

| Protocol | Between | Version constant | Current |
|---|---|---|---|
| Peer wire protocol | DeskHop peers on the LAN, over QUIC | `proto::PROTOCOL_VERSION` | 1 |
| IPC protocol | Service, agents, and UI, over named pipes | `ipc::IPC_VERSION` | 1 |

This document is written so that a third party could implement a compatible peer (ADR 0003, ADR 0004).

## Peer wire protocol

Shape: ADR 0004. Symmetric peers, mDNS/DNS-SD discovery with pair-by-code fallback, PAKE pairing over QUIC whose TLS is pinned to each peer's identity key, and QUIC with datagrams for motion and streams for everything else. Normative requirements: `openspec/specs/wire-protocol/spec.md`. All integers are little-endian.

### Stream frames

Every message on a QUIC stream is one frame:

```
+----------------+--------------+-------------------+
| len: u32le     | type: u16le  | payload: len bytes|
+----------------+--------------+-------------------+
  <------- 6-byte header ------->
```

- `len` counts the payload only. The header never changes in any version: it is read before a version is agreed.
- `len` must not exceed 65,536. A receiver rejects a larger value from the header alone, before the payload arrives. Messages with more data than that are split into several frames by the capability that defines them.
- Example: type `0x0001` with payload `01 00 01 00` is `04 00 00 00 01 00 01 00 01 00`.

### Datagrams

Every message sent as a QUIC datagram is one datagram, with no length field:

```
+--------------+---------------------------+
| type: u16le  | payload: rest of datagram |
+--------------+---------------------------+
```

A datagram is 2 to 1,200 bytes, type included. Anything shorter or longer is a protocol error.

### Message types

Stream and datagram messages share one 16-bit namespace. A type not defined in the negotiated version, or sent on a channel it may not use, is a protocol error. Every payload must match its layout exactly; leftover bytes are a protocol error, except for `Hello`.

| Type | Name | Channel | Since | Payload |
|---|---|---|---|---|
| `0x0000` | reserved | — | — | Never valid. |
| `0x0001` | `Hello` | stream (control) | 1 | See below. |
| `0x0002` | `PairPake` | stream | 1 | See [Pairing](#pairing). |
| `0x0003` | `PairConfirm` | stream | 1 | See [Pairing](#pairing). |
| `0x0004` | `MemberRecord` | stream | 1 | See [Desk membership](#desk-membership). |
| `0x0005` | `RecordsDone` | stream | 1 | Empty. See [Desk membership](#desk-membership). |

### Hello and version negotiation

```
Hello payload:
+-------------+-------------+-------------------------------------+
| min: u16le  | max: u16le  | fields from later versions (ignored) |
+-------------+-------------+-------------------------------------+
```

- As soon as the connection is up, each peer sends `Hello` as the first frame on the control stream, without waiting for the other's. `min` and `max` are the lowest and highest versions the sender speaks; `min` ≥ 1 and `min` ≤ `max`.
- Both peers then use `min(local.max, remote.max)` if it is at least `max(local.min, remote.min)`. Both sides compute the same result; there is no acknowledgement.
- If the ranges do not overlap, each peer closes with code `2`. The side whose `max` is below the other's `min` runs the older release.
- Protocol errors: a first control-stream frame that is not `Hello`, a second `Hello`, `Hello` on any other stream, `min` = 0, or `min` > `max`.
- Before the version is agreed, datagrams are dropped, and frames on other streams are held unread until agreement (QUIC does not order data across streams).
- `Hello` ignores bytes after `max`, so later versions can append fields. The first four bytes never change.

### Identity

Each machine has one Ed25519 key. Its 32-byte public key is the machine's peer identity, and its TLS certificate carries that key. Normative requirements: `openspec/specs/pairing/spec.md`.

### Pairing

The *inviter* shows a code; on the *joiner*, the user types it. The code authenticates a SPAKE2 exchange, and key-confirmation tags bind the result to the connection.

**Code.** `<locator>-<secret><check>`, all digits, for example `137-4829153`.

- *Locator:* the trailing `4 - min(⌊prefix / 8⌋, 3)` bytes of the inviter's IPv4 address, in decimal, separated by `.` (1 byte on a /24, 2 on a /16, 3 on a /8). The joiner rebuilds the address from its own leading bytes.
- *Secret:* 6 digits, uniform over `000000`–`999999`.
- *Check:* one Damm check digit over the locator's decimal digits followed by the secret's digits.
- *Canonical text:* the form shown above, with no spaces. When parsing typed text, whitespace anywhere and hyphens after the first are ignored.

**SPAKE2.** Symmetric mode over the Ed25519 group, compatible with python-spake2's `SPAKE2_Symmetric` and the `spake2` Rust crate:

- *Password* `pw`: the UTF-8 bytes of the code's canonical text. *Identity* `id`: the ASCII bytes `deskhop pair v1`.
- *Password scalar:* HKDF-SHA256 with an empty salt, `pw` as input key material, and info `SPAKE2 pw`, expanded to 48 bytes. Read those bytes as a big-endian integer and reduce it modulo the group order.
- *Constant S:* the point whose compressed encoding is `6f00dae87c1be1a73b5922ef431cd8f57879569c222d22b1cd71e8546ab8e6f1`.
- *Message:* `0x53` (`S`), then the compressed point `x·B + pw_scalar·S`, for a random scalar `x`. 33 bytes.
- *Shared point:* `K = x·(Y − pw_scalar·S)`, where `Y` is the other side's point.
- *Shared key:* `SHA-256(SHA-256(pw) ‖ SHA-256(id) ‖ first ‖ second ‖ K)`, where `first` and `second` are the two 32-byte points in ascending byte order.

**Key confirmation.**

```
PRK     = HKDF-Extract(salt = "deskhop pair v1", IKM = shared key)        (SHA-256)
binding = exporter ‖ inviter certificate key ‖ joiner certificate key     (32 + 32 + 32 bytes)
tag_J   = HKDF-Expand(PRK, "joiner"  ‖ binding, 32)
tag_I   = HKDF-Expand(PRK, "inviter" ‖ binding, 32)
```

`exporter` is the TLS exporter value with label `EXPORTER-deskhop-pair-v1`, an empty context, and a length of 32. Tags are compared in constant time.

**Order**, after `Hello`:

1. The joiner sends `PairPake`. The inviter replies with `PairPake`, or closes with code `4` if pairing mode is closed or another attempt is running.
2. The joiner sends `PairConfirm` with `tag_J` and its name.
3. If `tag_J` is wrong, the inviter closes with code `3` and sends no tag. Otherwise it replies with `PairConfirm` carrying `tag_I` and its name.
4. If `tag_I` is wrong, the joiner closes with code `3`.
5. Each side signs a `MemberRecord` adding the other, then sends all its records and `RecordsDone`, as described under [Desk membership](#desk-membership). After receiving the inviter's `RecordsDone`, the joiner closes with code `0`.

Any other order is a protocol error.

**Limits on the inviter.**

- Pairing mode lasts 10 minutes, ends after one successful pairing, and handles one attempt at a time.
- An attempt starts at the joiner's `PairPake` and fails if any of these happens: a wrong tag, a protocol error, the connection dropping, or 30 seconds passing before the record exchange ends. A timed-out attempt is closed with code `1`.
- After 3 failed attempts, pairing mode closes and its code stops working. The user opens it again for a new code.

```
PairPake payload (33 bytes):
+-----------+---------------------+
| side: u8  | point: 32 bytes     |
| 0x53 'S'  | compressed Edwards  |
+-----------+---------------------+

PairConfirm payload (33 + n bytes):
+-----------------+-----------+---------------------+
| tag: 32 bytes   | n: u8     | name: n bytes UTF-8 |
|                 | 1..=63    |                     |
+-----------------+-----------+---------------------+
```

### Desk membership

Members sign records that add or remove a machine. A record is accepted when its Ed25519 signature verifies and its signer is a member in the receiver's view. Normative requirements: `openspec/specs/desk-membership/spec.md`.

```
MemberRecord payload (138 + n bytes):
+---------+------------------+------------+------------------+---------+------------+-------------------+
| kind:u8 | subject: 32 B    | epoch: u64 | signer: 32 B     | n: u8   | name: n B  | signature: 64 B   |
| 1 add   | peer identity    |            | peer identity    | add:    | UTF-8      | Ed25519 by signer |
| 2 remove|                  |            |                  | 1..=63  |            |                   |
|         |                  |            |                  | remove: |            |                   |
|         |                  |            |                  | 0       |            |                   |
+---------+------------------+------------+------------------+---------+------------+-------------------+
```

- *Signature:* covers the ASCII bytes `deskhop member record v1`, followed by the payload without its last 64 bytes. An addition whose signer is its own subject is invalid.
- *Which record wins:* for each subject, the record with the highest epoch wins; on equal epochs, removal wins. A machine holds records for at most 256 subjects.
- *Epoch:* the signer's Unix time in seconds, or one above the highest epoch it holds for that subject if that is greater, and at least 1.
- *Exchange:* on every connection between members, and at the end of pairing, each side sends every winning record it holds, then `RecordsDone` (an empty payload). A receiver applies the records when `RecordsDone` arrives, retrying records whose signer becomes a member partway through.
  - A `MemberRecord` after `RecordsDone` is a single update and is applied at once.
  - More than 256 records before `RecordsDone`, or a second `RecordsDone`, is a protocol error.
- *Removal of the receiver:* a machine that accepts a winning removal of itself drops every record. Its identity key is kept.

### Close reasons

Sent as the QUIC application error code when closing a connection.

| Code | Reason |
|---|---|
| `0` | Normal close |
| `1` | Protocol error |
| `2` | Version mismatch |
| `3` | Wrong pairing code: a key-confirmation tag did not match |
| `4` | Not ready to pair: pairing mode is closed, or another attempt is running |

## IPC protocol

Shape: ADR 0002. Service, agents, and UI may run different versions during an upgrade.

No messages are defined yet.

## Version history

| Protocol | Version | Change |
|---|---|---|
| Peer | 1 | Stream frames, datagrams, message-type registry, `Hello` version negotiation, close reasons, pairing (`PairPake`, `PairConfirm`), desk membership (`MemberRecord`, `RecordsDone`). |
| IPC | 1 | Initial version; no messages. |
