//! Neutral input events, screen geometry, and the edge→peer layout graph.
//!
//! No Win32 types and no I/O: `win32-input` maps Windows input into these
//! types, and `engine` makes every decision from them.

#![forbid(unsafe_code)]

pub mod geometry;
pub mod input;
pub mod layout;
pub mod time;

pub use geometry::{EdgeFraction, Monitor, MonitorId, Point, Rect, Screen, Side};
pub use input::{Button, InputAction, InputEvent, Key, Origin};
pub use layout::{Layout, PeerId, PeerMessage};
pub use time::Millis;
