# Spec Delta

## MODIFIED Requirements

### Requirement: Message type registry
Each message type SHALL be a 16-bit number assigned in `docs/protocol.md`, together with the channel it may use (stream or datagram) and the protocol version that introduced it. Type `0x0000` SHALL be reserved and never valid. A receiver SHALL treat as a protocol error any message whose type is not defined in the negotiated version, and any message that arrives on a channel its type does not allow. Protocol version 1 SHALL define exactly these message types, all on streams: `0x0001` `Hello`, `0x0002` `PairPake`, `0x0003` `PairConfirm`, `0x0004` `MemberRecord`, and `0x0005` `RecordsDone`.

#### Scenario: Reserved type
- **WHEN** a receiver decodes a frame with message type `0x0000`
- **THEN** it reports a protocol error

#### Scenario: Unknown type after negotiation
- **WHEN** peers have negotiated version 1 and a receiver decodes a frame with message type `0x0042`
- **THEN** it reports a protocol error

#### Scenario: Stream-only type in a datagram
- **WHEN** a receiver gets a datagram with message type `0x0001` (`Hello`)
- **THEN** it reports a protocol error

#### Scenario: Pairing message in a datagram
- **WHEN** a receiver gets a datagram with message type `0x0004` (`MemberRecord`)
- **THEN** it reports a protocol error

### Requirement: Close reasons
A peer closing a connection SHALL give one of these QUIC application error codes: `0` normal close, `1` protocol error, `2` version mismatch, `3` wrong pairing code, `4` not ready to pair. A peer SHALL close with protocol error whenever this specification calls something a protocol error. New close reasons SHALL be added in `docs/protocol.md` with the version that introduced them.

#### Scenario: Protocol error closes the connection
- **WHEN** a receiver reports a protocol error on any stream or datagram
- **THEN** the peer closes the connection with code `1`

#### Scenario: Version mismatch closes the connection
- **WHEN** version negotiation finds no overlap
- **THEN** the peer closes the connection with code `2`

#### Scenario: Pairing close codes
- **WHEN** a receiver reads close code `3` or `4`
- **THEN** it reports a wrong pairing code or a machine not ready to pair, respectively

## ADDED Requirements

### Requirement: Pairing and membership message layouts
The pairing and membership messages SHALL have these payloads, all integers little-endian:
- `PairPake`: exactly 33 bytes, a SPAKE2 symmetric-mode message.
- `PairConfirm`: a 32-byte key-confirmation tag, a name length `n` (u8, 1 to 63), and `n` bytes of UTF-8 machine name.
- `MemberRecord`: a kind (u8: `1` addition, `2` removal), the subject's 32-byte peer identity, the epoch (u64), the signer's 32-byte peer identity, a name length `n` (u8; 1 to 63 for an addition, 0 for a removal), `n` bytes of UTF-8 machine name, and a 64-byte Ed25519 signature.
- `RecordsDone`: empty.

Any other kind, a name length outside its range, invalid UTF-8, or a payload whose length does not match its fields SHALL be a protocol error.

#### Scenario: Removal record layout
- **WHEN** a removal record is encoded
- **THEN** its payload is 1 + 32 + 8 + 32 + 1 + 64 = 138 bytes, with name length `0`

#### Scenario: Unknown record kind
- **WHEN** a `MemberRecord` payload has kind `3`
- **THEN** the receiver reports a protocol error

#### Scenario: Addition with no name
- **WHEN** a `MemberRecord` payload has kind `1` and name length `0`
- **THEN** the receiver reports a protocol error

#### Scenario: RecordsDone with a payload
- **WHEN** a `RecordsDone` frame carries 1 byte of payload
- **THEN** the receiver reports a protocol error
