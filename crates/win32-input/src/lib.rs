//! Low-level hooks, Raw Input, `SendInput`, cursor clip/warp, DPI.
//!
//! One of two crates where `unsafe` is allowed. Every `unsafe` block carries a
//! `// SAFETY:` comment. Hook procedures do nothing but enqueue: Windows
//! silently unhooks a slow low-level hook. Linked only by `bin/agent`.
