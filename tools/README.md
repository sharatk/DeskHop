# Tools

| Tool | Status | Purpose |
|---|---|---|
| `archcheck/` | Ready | Enforces the dependency and `unsafe` rules in AGENTS.md. `cargo archcheck`. |
| `bootstrap.cmd` | Ready | Runs `bootstrap.ps1` from an elevated terminal. Installs the Windows toolchain: Rust, MSVC Build Tools, Windows SDK, Node, WiX. |
| fuzzers | Planned | `proto` decoding fuzzers. Strategy is an open question in ADR 0004. |
| packet capture | Planned | Decode captured DeskHop traffic. |
| event replay | Planned | Feed recorded `model` event sequences through `engine`. |
