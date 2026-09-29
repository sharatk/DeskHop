//! Clipboard listener, text/DIB/HDROP formats, file transfer glue.
//!
//! One of two crates where `unsafe` is allowed. Every `unsafe` block carries a
//! `// SAFETY:` comment. Linked only by `bin/agent`.
