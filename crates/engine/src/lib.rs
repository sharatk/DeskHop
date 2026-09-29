//! Pure state machine: focus owner, edge transitions, routing, layout learning.
//!
//! Depends only on `model`, `proto`, and `policy`. Never on `win32-*`,
//! `transport`, or `ipc`. Must build and test on Linux. Tests are replay tests:
//! a sequence of `model` events in, routing decisions out.

#![forbid(unsafe_code)]
