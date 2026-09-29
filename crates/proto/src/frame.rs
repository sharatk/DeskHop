//! Stream frames: `len: u32le | type: u16le | payload`.
//!
//! The header layout never changes in any protocol version, because it is
//! read before a version is agreed.

use crate::{EncodeError, ProtocolError};

/// Bytes in a frame header.
pub const HEADER_LEN: usize = 6;

/// Largest payload a frame may carry.
pub const MAX_FRAME_PAYLOAD: usize = 65_536;

/// One decoded frame, borrowing its payload from the caller's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub ty: u16,
    pub payload: &'a [u8],
}

/// The result of one decoding step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded<'a> {
    /// A complete frame, and how many bytes of the input it used.
    Frame { frame: Frame<'a>, consumed: usize },
    /// Not enough input for a complete frame. Nothing was consumed.
    NeedMore,
}

/// Decodes the first frame in `buf`.
///
/// An oversized length is rejected from the header alone, so the caller never
/// needs to buffer more than [`HEADER_LEN`] + [`MAX_FRAME_PAYLOAD`] bytes.
pub fn decode(buf: &[u8]) -> Result<Decoded<'_>, ProtocolError> {
    let Some(&[l0, l1, l2, l3, t0, t1]) = buf.first_chunk::<HEADER_LEN>() else {
        return Ok(Decoded::NeedMore);
    };
    let claimed = u32::from_le_bytes([l0, l1, l2, l3]);
    let too_large = ProtocolError::FrameTooLarge { len: claimed };
    let len = usize::try_from(claimed).map_err(|_| too_large)?;
    if len > MAX_FRAME_PAYLOAD {
        return Err(too_large);
    }
    let end = HEADER_LEN.checked_add(len).ok_or(too_large)?;
    let Some(payload) = buf.get(HEADER_LEN..end) else {
        return Ok(Decoded::NeedMore);
    };
    Ok(Decoded::Frame {
        frame: Frame {
            ty: u16::from_le_bytes([t0, t1]),
            payload,
        },
        consumed: end,
    })
}

/// Appends one frame to `out`.
pub fn encode(ty: u16, payload: &[u8], out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let too_large = EncodeError::PayloadTooLarge { len: payload.len() };
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(too_large);
    }
    let len = u32::try_from(payload.len()).map_err(|_| too_large)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(ty: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        encode(ty, payload, &mut out).unwrap();
        out
    }

    #[test]
    fn frame_with_payload_matches_spec_bytes() {
        assert_eq!(
            encoded(0x0001, &[0x01, 0x00, 0x01, 0x00]),
            [0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00]
        );
    }

    #[test]
    fn empty_payload_is_header_only() {
        assert_eq!(encoded(0x0001, &[]), [0, 0, 0, 0, 0x01, 0x00]);
    }

    #[test]
    fn round_trip() {
        let bytes = encoded(0x1234, b"payload");
        assert_eq!(
            decode(&bytes),
            Ok(Decoded::Frame {
                frame: Frame {
                    ty: 0x1234,
                    payload: b"payload"
                },
                consumed: bytes.len()
            })
        );
    }

    #[test]
    fn split_header_needs_more() {
        let bytes = encoded(0x0001, &[1, 2, 3]);
        assert_eq!(decode(&bytes[..3]), Ok(Decoded::NeedMore));
    }

    #[test]
    fn split_payload_needs_more() {
        let bytes = encoded(0x0001, &[1, 2, 3]);
        assert_eq!(decode(&bytes[..bytes.len() - 1]), Ok(Decoded::NeedMore));
    }

    #[test]
    fn two_frames_in_one_read() {
        let mut bytes = encoded(0x0001, b"first");
        bytes.extend(encoded(0x0002, b"second"));

        let Ok(Decoded::Frame { frame, consumed }) = decode(&bytes) else {
            panic!("first frame")
        };
        assert_eq!((frame.ty, frame.payload), (0x0001, &b"first"[..]));

        let Ok(Decoded::Frame {
            frame,
            consumed: rest,
        }) = decode(&bytes[consumed..])
        else {
            panic!("second frame")
        };
        assert_eq!((frame.ty, frame.payload), (0x0002, &b"second"[..]));
        assert_eq!(consumed + rest, bytes.len());
    }

    #[test]
    fn frame_at_limit_decodes() {
        let payload = vec![0xab; MAX_FRAME_PAYLOAD];
        let bytes = encoded(0x0001, &payload);
        let Ok(Decoded::Frame { frame, .. }) = decode(&bytes) else {
            panic!("frame at limit")
        };
        assert_eq!(frame.payload.len(), MAX_FRAME_PAYLOAD);
    }

    #[test]
    fn oversized_length_fails_from_header_alone() {
        let header = [0x01, 0x00, 0x01, 0x00, 0x01, 0x00]; // 65,537
        assert_eq!(
            decode(&header),
            Err(ProtocolError::FrameTooLarge { len: 65_537 })
        );
    }

    #[test]
    fn huge_length_fails_from_header_alone() {
        let header = [0xff, 0xff, 0xff, 0xff, 0x01, 0x00];
        assert_eq!(
            decode(&header),
            Err(ProtocolError::FrameTooLarge { len: u32::MAX })
        );
    }

    #[test]
    fn encode_rejects_oversized_payload() {
        let mut out = Vec::new();
        assert_eq!(
            encode(0x0001, &vec![0; MAX_FRAME_PAYLOAD + 1], &mut out),
            Err(EncodeError::PayloadTooLarge {
                len: MAX_FRAME_PAYLOAD + 1
            })
        );
        assert!(out.is_empty());
    }
}
