# Spec Delta

## MODIFIED Requirements

### Requirement: Close reasons
A peer closing a connection SHALL give one of these QUIC application error codes: `0` normal close, `1` protocol error, `2` version mismatch, `3` wrong pairing code, `4` not ready to pair, `5` not a member. A peer SHALL close with protocol error whenever this specification calls something a protocol error. New close reasons SHALL be added in `docs/protocol.md` with the version that introduced them.

#### Scenario: Protocol error closes the connection
- **WHEN** a receiver reports a protocol error on any stream or datagram
- **THEN** the peer closes the connection with code `1`

#### Scenario: Version mismatch closes the connection
- **WHEN** version negotiation finds no overlap
- **THEN** the peer closes the connection with code `2`

#### Scenario: Pairing close codes
- **WHEN** a receiver reads close code `3` or `4`
- **THEN** it reports a wrong pairing code or a machine not ready to pair, respectively

#### Scenario: Not a member
- **WHEN** a receiver reads close code `5`
- **THEN** it reports that the other machine does not count it as a member of its desk
