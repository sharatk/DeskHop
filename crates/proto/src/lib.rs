//! Wire messages, framing, and versioning. No I/O.
//!
//! The only crate that defines wire types. A change here bumps
//! [`PROTOCOL_VERSION`] and updates `docs/protocol.md` in the same PR.

#![forbid(unsafe_code)]

/// Peer-to-peer protocol version, carried in every handshake.
pub const PROTOCOL_VERSION: u16 = 1;
