//! Wire messages, framing, and versioning. No I/O.
//!
//! The only crate that defines wire types. A change here bumps
//! [`PROTOCOL_VERSION`] and updates `docs/protocol.md` in the same PR.
//!
//! Everything here is sans-I/O: decoders take the bytes the caller has
//! buffered and never allocate, so `transport` owns all buffering and the
//! memory a peer can make us hold is bounded by [`frame::MAX_FRAME_PAYLOAD`].
//! Decoding must never panic on any input; the lints below enforce that.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::arithmetic_side_effects
    )
)]

pub mod codec;
pub mod datagram;
pub mod error;
pub mod frame;
pub mod hello;
pub mod registry;
pub mod session;

pub use error::{EncodeError, ProtocolError};

/// Highest peer-to-peer protocol version this release speaks.
pub const PROTOCOL_VERSION: u16 = 1;

/// Lowest peer-to-peer protocol version this release speaks.
pub const MIN_PROTOCOL_VERSION: u16 = 1;

/// Why a connection was closed, sent as the QUIC application error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Normal,
    ProtocolError,
    VersionMismatch,
}

impl CloseReason {
    /// The QUIC application error code for this reason.
    pub const fn code(self) -> u64 {
        match self {
            Self::Normal => 0,
            Self::ProtocolError => 1,
            Self::VersionMismatch => 2,
        }
    }

    /// The reason for a QUIC application error code, if it is one we define.
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Normal),
            1 => Some(Self::ProtocolError),
            2 => Some(Self::VersionMismatch),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_codes_round_trip() {
        for reason in [
            CloseReason::Normal,
            CloseReason::ProtocolError,
            CloseReason::VersionMismatch,
        ] {
            assert_eq!(CloseReason::from_code(reason.code()), Some(reason));
        }
        assert_eq!(CloseReason::from_code(3), None);
    }

    #[test]
    fn close_codes_match_spec() {
        assert_eq!(CloseReason::Normal.code(), 0);
        assert_eq!(CloseReason::ProtocolError.code(), 1);
        assert_eq!(CloseReason::VersionMismatch.code(), 2);
    }
}
