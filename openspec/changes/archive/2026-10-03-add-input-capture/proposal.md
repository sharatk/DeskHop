# Proposal

## Why

The engine decides focus from `model` input events and returns capture modes, injections and cursor warps, and the transport can carry them between machines. Nothing on Windows produces those events or carries out those decisions yet: `win32-input` is an empty crate and `deskhop-agent.exe` exits at once. This change builds the agent's half of the input path, so that `add-input-forwarding` and `add-ipc-and-service` can connect it to the engine and the network. It is the "`win32-input` in a bare agent" step of the design brief's build order.

## What Changes

- **Capture.** `win32-input` reports every local key, mouse button, wheel and motion event as a `model::InputEvent`. Each event carries a monotonic timestamp and its `Origin`.
  - Keys and buttons come from low-level hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`). Keys are reported as USB HID keyboard-page usages, translated from scan codes. Keys with no keyboard-page usage, such as media keys, always reach the local OS and are not reported.
  - Motion comes from Raw Input, so it is the raw device movement before pointer speed, plus the cursor position in virtual-screen pixels.
  - Origin is physical for input from a device on this machine, and injected for anything produced by software, DeskHop's own injection included.
- **Withholding.** The hook keeps physical input from the local OS according to a capture mode: pass all, withhold all, or withhold mouse only. Injected input is never withheld, and withheld input is still reported. The hook decides from a flag and only enqueues; it never waits on the consumer.
- **Injection and warp.** `model::InputAction`s are injected with `SendInput`:
  - Keys are injected by scan code, so the receiving machine's keyboard layout applies.
  - Motion is injected as relative movement, so the receiving machine's pointer speed applies.
  - Buttons and wheel are injected as they are.

  The cursor can be warped to a virtual-screen point.
- **Screen.** The monitors are reported as a `model::Screen` at start and again whenever the set, their positions, or their DPI change. Rectangles are in per-monitor-DPI-aware virtual-screen pixels, and each monitor has its effective DPI.
- **Monitor identity (ADR 0005 spike).**
  - Each monitor's `MonitorId` comes from its EDID manufacturer, product and serial when the serial is present and unique among connected monitors.
  - Otherwise it falls back to the Windows monitor device path.
  - The trace mode prints both, so the rule can be checked on real docks and laptops.
- **`CaptureMode` moves from `engine` to `model`**, so the agent can name it without depending on `engine`. `engine` re-exports it, so its API is unchanged. The wheel's units (1/120 of a notch, as in Windows) are documented on `model::InputEvent::Wheel`.
- **Bare agent.** `deskhop-agent.exe --trace` is a developer diagnostic that starts capture and prints events and screen changes to the console.
  - `--withhold mouse|all <seconds>` withholds for at most 10 seconds and then returns to pass-all.
  - `--session` and `--winlogon` stay stubs until `add-ipc-and-service` gives them a pipe to talk to.

## Capabilities

### New Capabilities
- `input-capture`: how this machine's input is observed, classified, withheld and injected on Windows, and how its monitors and cursor are reported. This covers events and origins, key translation, capture modes, injection, cursor warp, screen reports and monitor identity.

### Modified Capabilities
None. `input-focus` already requires physical and injected input to be told apart, and forwarded motion to be raw. This change implements those requirements on Windows without changing them.

## Impact

- **Crates:**
  - **`win32-input`:** gains all the capture, injection and screen code.
    - It depends on `model`, plus `windows` (the workspace pin, 0.61) with the Win32 features it needs.
    - It is one of the two crates allowed `unsafe`, and every `unsafe` block carries a `// SAFETY:` comment.
  - **`model`:** gains `CaptureMode`, moved from `engine`, and doc comments on wheel units. No new dependencies.
  - **`engine`:** re-exports `model::CaptureMode`; its behavior is unchanged.
  - **`bin/agent`:** gains the `--trace` diagnostic.
- **Dependency rules hold:**
  - Only `deskhop-agent` links `win32-input`.
  - `engine` still reaches no `win32-*`, `transport` or `ipc`, and still builds on Linux.
  - `model` gains no dependency.
  - Every crate except the two `win32-*` crates keeps `#![forbid(unsafe_code)]`.
  - No `cfg` branches for other operating systems.
- **No grid, no server/client toggle, no IP address field.** Nothing here is user-facing apart from a developer flag.
- **No wire or pipe changes:** `proto` and `ipc` are untouched, and so is `docs/protocol.md`.
- **CI:** the ARM64 `cargo check` covers `win32-input` and `deskhop-agent` too. Tests that inject real input are `#[ignore]` and run by hand on a desktop.
- **Later changes:**
  - `add-ipc-and-service`: runs the engine in the service, carries events and decisions over the pipe, and returns capture to pass-all when the pipe drops. It also settles the clock that the service shares with the agent, and implements `RequestSecureAttention` with `SendSAS` in the service.
  - `add-input-forwarding`: carries the engine's messages and motion between peers.
  - Elevated foreground windows and the secure desktop (the winlogon agent, and possibly `uiAccess`) build on this crate later.
