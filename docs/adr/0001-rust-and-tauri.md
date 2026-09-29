# ADR 0001 — Rust for the client, Tauri 2 for the shell

**Status:** Accepted
**Date:** 2026-09-27

## Context

The client has three parts with different constraints: a network-facing daemon running as SYSTEM, agents that sit inside the Win32 input pipeline (low-level hooks, `SendInput`, cursor warping, secure-desktop injection), and a settings UI that is optional and rarely open.

The team's background is Python, JavaScript, Node, React. There was interest in using the project to learn Node, and an initial lean toward Electron.

The deciding constraint is that the process parsing packets off the LAN runs as SYSTEM. A memory-safety bug there is a remote SYSTEM exploit on every installed desk. The project is also open source, so the code that runs as SYSTEM must be easy to audit.

## Decision

One language for the client: **Rust** for the service, both agents, the engine, and the Tauri bridge. **Tauri 2** with React + TypeScript for the shell.

## Alternatives considered

| | Verdict |
|---|---|
| Electron + Rust or C# service | Sound. Rejected for footprint (~200 MB installed, ~100 MB idle) in an always-on tray app, and for a second language. |
| Node service + Rust agents + Electron | The honest "learn Node" option: Node cannot own hooks or injection, so Rust remains regardless. Rejected to keep one language. |
| C++23 | Closest to every Win32 sample. Rejected: puts the SYSTEM-daemon safety story entirely on discipline, and raises the bar for open-source contributors. |
| Go + Wails | Excellent for the network layer, poor for hooks (needs `LockOSThread` and a C shim). Rejected. |
| WinUI 3 / WPF + C# | Native, but abandons the React skill set. Rejected. |

Windows-only removes Electron's portability advantage and Tauri's Linux-webview weakness: every Windows 10/11 machine already has WebView2.

## Consequences

- One toolchain, one dependency tree, ~10 MB installed, ~30 MB idle.
- `windows-rs` is maintained by Microsoft and covers the full API surface; worked examples are thinner than C++/C#, so `docs/` must capture Win32 patterns as they are learned.
- Rust learning curve is the accepted cost. The engine is a pure state machine and is the friendliest place to start.
