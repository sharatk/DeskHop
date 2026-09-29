//! Message types, the channel each may use, and the version that introduced
//! it. `docs/protocol.md` lists the same table for implementers.

use crate::ProtocolError;

/// Never a valid message type.
pub const RESERVED: u16 = 0x0000;

/// Version handshake. The first frame on the control stream.
pub const HELLO: u16 = 0x0001;

/// Where a message type may be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stream,
    Datagram,
}

/// One registered message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeInfo {
    pub ty: u16,
    pub name: &'static str,
    pub channel: Channel,
    /// Protocol version that introduced this type.
    pub since: u16,
}

/// Every message type, in type order.
pub const REGISTRY: &[TypeInfo] = &[TypeInfo {
    ty: HELLO,
    name: "Hello",
    channel: Channel::Stream,
    since: 1,
}];

/// The message type `ty` as defined in protocol `version`.
pub fn lookup(ty: u16, version: u16) -> Option<&'static TypeInfo> {
    REGISTRY
        .iter()
        .find(|info| info.ty == ty && info.since <= version)
}

/// Checks that `ty` is defined in `version` and allowed on `channel`.
pub fn check(ty: u16, channel: Channel, version: u16) -> Result<&'static TypeInfo, ProtocolError> {
    if ty == RESERVED {
        return Err(ProtocolError::ReservedType);
    }
    let info = lookup(ty, version).ok_or(ProtocolError::UnknownType { ty })?;
    if info.channel != channel {
        return Err(ProtocolError::WrongChannel { ty });
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_1_defines_only_hello() {
        assert_eq!(REGISTRY.len(), 1);
        let hello = check(HELLO, Channel::Stream, 1).unwrap();
        assert_eq!((hello.name, hello.since), ("Hello", 1));
    }

    #[test]
    fn reserved_type_is_rejected() {
        assert_eq!(
            check(RESERVED, Channel::Stream, 1),
            Err(ProtocolError::ReservedType)
        );
    }

    #[test]
    fn unknown_type_is_rejected() {
        assert_eq!(
            check(0x0042, Channel::Stream, 1),
            Err(ProtocolError::UnknownType { ty: 0x0042 })
        );
    }

    #[test]
    fn stream_type_in_datagram_is_rejected() {
        assert_eq!(
            check(HELLO, Channel::Datagram, 1),
            Err(ProtocolError::WrongChannel { ty: HELLO })
        );
    }

    #[test]
    fn registry_is_sorted_and_unique() {
        assert!(REGISTRY.windows(2).all(|w| w[0].ty < w[1].ty));
        assert!(REGISTRY.iter().all(|info| info.ty != RESERVED));
    }
}
