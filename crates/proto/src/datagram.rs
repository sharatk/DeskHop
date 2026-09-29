//! Datagrams: `type: u16le | payload`, one message per QUIC datagram.

use crate::{EncodeError, ProtocolError};

/// Bytes in a datagram's type field.
pub const TYPE_LEN: usize = 2;

/// Largest datagram, type included.
pub const MAX_DATAGRAM_LEN: usize = 1_200;

/// One decoded datagram, borrowing its payload from the caller's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Datagram<'a> {
    pub ty: u16,
    pub payload: &'a [u8],
}

/// Decodes one whole datagram.
pub fn decode(bytes: &[u8]) -> Result<Datagram<'_>, ProtocolError> {
    if bytes.len() > MAX_DATAGRAM_LEN {
        return Err(ProtocolError::DatagramTooLarge { len: bytes.len() });
    }
    let Some((ty, payload)) = bytes.split_first_chunk::<TYPE_LEN>() else {
        return Err(ProtocolError::DatagramTooShort { len: bytes.len() });
    };
    Ok(Datagram {
        ty: u16::from_le_bytes(*ty),
        payload,
    })
}

/// Appends one datagram to `out`, which should be empty.
pub fn encode(ty: u16, payload: &[u8], out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let too_large = EncodeError::PayloadTooLarge { len: payload.len() };
    let total = payload.len().checked_add(TYPE_LEN).ok_or(too_large)?;
    if total > MAX_DATAGRAM_LEN {
        return Err(too_large);
    }
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_formed_datagram() {
        let mut bytes = vec![0x02, 0x00];
        bytes.extend([7; 18]);
        assert_eq!(
            decode(&bytes),
            Ok(Datagram {
                ty: 0x0002,
                payload: &[7; 18]
            })
        );
    }

    #[test]
    fn one_byte_is_too_short() {
        assert_eq!(
            decode(&[0x01]),
            Err(ProtocolError::DatagramTooShort { len: 1 })
        );
    }

    #[test]
    fn type_only_is_valid() {
        assert_eq!(
            decode(&[0x02, 0x00]),
            Ok(Datagram {
                ty: 0x0002,
                payload: &[]
            })
        );
    }

    #[test]
    fn at_limit_decodes_and_over_limit_fails() {
        assert!(decode(&[0; MAX_DATAGRAM_LEN]).is_ok());
        assert_eq!(
            decode(&[0; MAX_DATAGRAM_LEN + 1]),
            Err(ProtocolError::DatagramTooLarge { len: 1_201 })
        );
    }

    #[test]
    fn round_trip() {
        let mut out = Vec::new();
        encode(0x0102, b"motion", &mut out).unwrap();
        assert_eq!(
            decode(&out),
            Ok(Datagram {
                ty: 0x0102,
                payload: b"motion"
            })
        );
    }

    #[test]
    fn encode_rejects_oversized_payload() {
        let mut out = Vec::new();
        assert!(encode(0x0102, &[0; MAX_DATAGRAM_LEN - 1], &mut out).is_err());
        assert!(out.is_empty());
    }
}
