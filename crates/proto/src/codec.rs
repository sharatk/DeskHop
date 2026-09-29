//! Helpers for decoding message payloads.

use crate::ProtocolError;

/// The payload of a fixed-layout message as an array, or a protocol error if
/// it is shorter or longer than `N` bytes.
pub fn fixed_payload<const N: usize>(ty: u16, payload: &[u8]) -> Result<[u8; N], ProtocolError> {
    <[u8; N]>::try_from(payload).map_err(|_| ProtocolError::PayloadLength {
        ty,
        expected: N,
        actual: payload.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for a future 8-byte message type.
    const TEST_TYPE: u16 = 0x7fff;

    #[test]
    fn exact_length_decodes() {
        let payload = [1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(fixed_payload::<8>(TEST_TYPE, &payload), Ok(payload));
    }

    #[test]
    fn trailing_byte_is_rejected() {
        assert_eq!(
            fixed_payload::<8>(TEST_TYPE, &[0; 9]),
            Err(ProtocolError::PayloadLength {
                ty: TEST_TYPE,
                expected: 8,
                actual: 9
            })
        );
    }

    #[test]
    fn short_payload_is_rejected() {
        assert_eq!(
            fixed_payload::<8>(TEST_TYPE, &[0; 7]),
            Err(ProtocolError::PayloadLength {
                ty: TEST_TYPE,
                expected: 8,
                actual: 7
            })
        );
    }
}
