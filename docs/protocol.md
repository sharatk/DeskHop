# DeskHop protocols

Two versioned protocols. A change to either bumps its version constant and updates this file in the same PR.

| Protocol | Between | Version constant | Current |
|---|---|---|---|
| Peer wire protocol | DeskHop peers on the LAN, over QUIC | `proto::PROTOCOL_VERSION` | 1 |
| IPC protocol | Service, agents, and UI, over named pipes | `ipc::IPC_VERSION` | 1 |

This document is written so that a third party could implement a compatible peer (ADR 0003, ADR 0004).

## Peer wire protocol

Shape: ADR 0004. Symmetric peers, mDNS/DNS-SD discovery with pair-by-code fallback, a PAKE-authenticated Noise pairing, and QUIC with datagrams for motion and streams for everything else. Normative requirements: `openspec/specs/wire-protocol/spec.md`. All integers are little-endian.

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

### Close reasons

Sent as the QUIC application error code when closing a connection.

| Code | Reason |
|---|---|
| `0` | Normal close |
| `1` | Protocol error |
| `2` | Version mismatch |

## IPC protocol

Shape: ADR 0002. Service, agents, and UI may run different versions during an upgrade.

No messages are defined yet.

## Version history

| Protocol | Version | Change |
|---|---|---|
| Peer | 1 | Stream frames, datagrams, message-type registry, `Hello` version negotiation, close reasons. |
| IPC | 1 | Initial version; no messages. |
