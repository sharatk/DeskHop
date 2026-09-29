//! Typed named-pipe protocol between service, agents, and UI.
//!
//! Service, agents, and UI can be mid-upgrade at different versions. A change
//! here bumps [`IPC_VERSION`] and updates `docs/protocol.md` in the same PR.

#![forbid(unsafe_code)]

/// Named-pipe protocol version, exchanged when a pipe client connects.
pub const IPC_VERSION: u16 = 1;
