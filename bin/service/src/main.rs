//! `deskhop-service.exe`: Windows service, SYSTEM, session 0.
//!
//! Owns transport, pairing keys, and managed configuration. Spawns the session
//! and winlogon agents. Every spawn into a session or the secure desktop goes
//! through one function in this crate. Never touches input.

#![forbid(unsafe_code)]

fn main() {}
