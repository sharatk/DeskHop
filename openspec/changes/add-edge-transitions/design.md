# Design

## Context

`model` and `engine` are stubs today: a crate doc and `#![forbid(unsafe_code)]` each, no types. `engine` already declares dependencies on `model`, `proto`, and `policy`. The `wire-protocol` spec gives an envelope but no input messages; per the proposal, this change does not add any. Hooks, injection, sockets, and the service do not exist yet.

Requirements are in `specs/input-focus/spec.md`. Constraints that shape everything below: `engine` must build and test on Linux; hook procedures may only enqueue (AGENTS.md); `proto`, `model`, and `engine` are the audit surface and gain no dependencies here.

## Goals / Non-Goals

**Goals:**
- A deterministic, sans-I/O engine: the same event sequence always gives the same decisions, so every scenario in the spec is a replay test.
- `model` types that `win32-input`, `transport`, and `add-layout-learning` can adopt without reshaping.
- Every tunable number (corner size, push threshold, burst gap, takeover window) in one place.

**Non-Goals:**
- Learning or editing the layout. The engine reads a `Layout` it is given.
- Wire encoding of peer messages, or where the engine process runs. Those are `add-input-forwarding` and `add-ipc-and-service`.
- Clipboard, pairing, sleep notifications, the Undo toast.

## Decisions

### 1. Sans-I/O engine driven by timestamped events
`Engine::handle(event) -> Vec<Decision>`, with every local input event carrying a monotonic timestamp in milliseconds (`model::Millis`). The engine never reads a clock, spawns a thread, or sleeps. Every time-based rule (burst gaps, the 2 s takeover window, the push pause) is decided when the next local event arrives, by comparing timestamps, so no rule needs to fire without new input and the engine needs no timers or tick events.
- *Alternative: the engine owns timers (tokio).* Breaks determinism and Linux-testable purity, and adds a dependency. Rejected.

### 2. One engine per machine, symmetric roles
Every machine runs the same engine. Its focus state is exactly one of:

```
  Local                      this machine's input stays here
  Forwarding { to, exit }    this machine's input goes to peer `to`; `exit` = where the cursor left
  Controlled { by, contest } peer `by`'s input is injected here; `contest` tracks local bursts
```

Being controlled and forwarding at the same time is impossible: a controlled machine's own physical input takes over (becoming `Local`) before it can cross anywhere, and a forwarded cursor that leaves a controlled machine makes the controlling machine forward elsewhere.

### 3. Each machine owns its edges
A controlled machine applies the crossing rules to its own screen, layout, and settings, and sends `EdgeExit { target, side, fraction }` to its controller, which moves its forwarding to `target` (itself, or a third peer). The controller never needs the controlled machine's geometry or layout.
- *Alternative: the controller tracks a virtual cursor on the peer's screen.* Needs the peer's geometry and pointer ballistics. With pointer speed applied at the target (spec: raw movement), only the target knows where its cursor is. Rejected.
- When the controller cannot reach the target (a third peer it is not connected to), it sends `FocusEnter` back to the controlled machine at the point the cursor left, so the edge behaves as a wall. Held input on the controlled machine was already released when it reported the exit; that loss is accepted for this rare case.

### 4. Peer messages are neutral `model` types
`model::PeerMessage`: `FocusEnter { side, fraction }`, `FocusRefused`, `EdgeExit { target, side, fraction }`, `TakenBack`, `Input(InputAction)`, `SecureAttention`. `model::InputAction` (key, button, wheel, raw motion) is shared by forwarding and by `Decision::Inject`. The engine emits `Decision::Send(peer, msg)` and consumes `Event::FromPeer(peer, msg)`. `add-input-forwarding` gives each a `proto` message type; `Input(Motion)` becomes the datagram.

### 5. Capture mode is a published flag, not a per-event call
Swallowing an event has to be decided inside the hook procedure, which may only enqueue. The engine therefore publishes a `CaptureMode` (`PassAll`; `WithholdAll` while forwarding; `WithholdMouse` while controlled) via `Decision::SetCapture`, and the agent's hook reads it as an atomic flag. Per-event verdicts arrive too late to be used by a hook.
- Consequence: between the engine deciding to cross and the agent's flag flipping, a few milliseconds of input can still reach the local OS. The cursor is pinned at the edge during that time, so only keystrokes typed in that window are affected. `add-input-capture` measures this.

### 6. Neutral input codes
Keys are USB HID usage IDs (page 0x07), buttons are `Left`, `Right`, `Middle`, `X1`, `X2`, and the wheel carries vertical and horizontal deltas. Every event carries `Origin::Physical` or `Origin::Injected`, as the agent reports it (Windows marks injected low-level events). `win32-input` maps Windows virtual keys and scan codes to HID usages. Ctrl+Alt+End is left or right Control, left or right Alt, and End (usage 0x4D).

### 7. Geometry in virtual-screen pixels
`model::Monitor { id: MonitorId, rect: Rect, dpi: u32 }`; `model::Screen` is the current monitor set. Coordinates are per-monitor-DPI-aware virtual-screen pixels (AGENTS.md). `MonitorId` is opaque; `add-input-capture` decides what fills it (ADR 0005). The layout is `model::Layout`, a map from `(MonitorId, Side)` to `PeerId`.
- **Outer side:** the cursor at point `p` on side `s` of monitor `m` is at an outer edge when no monitor contains the pixel one step beyond `p` in direction `s`. This handles monitors that only partly overlap.
- **Corner zone:** `round(2 mm × dpi / 25.4)` pixels from each end of the monitor's side. That's 8 px at 96 DPI and 15 px at 192 DPI.
- **Entry fraction:** measured along the bounding extent of all monitors on the relevant axis. It is a `u16` fixed-point value (0 = start, 65535 = end), so the result is identical on every machine and fits the wire later without floats. Landing: map the fraction onto the target's opposite side; if that point is on no monitor, clamp to the nearest monitor pixel on that side.

### 8. Outward push detection
The agent reports each motion as the raw device delta plus the cursor position after Windows applied it. At an outer edge, Windows clamps the cursor, so the position does not change but the raw delta still points outward; that is the push. The `push` sensitivity accumulates the outward raw component: at least 20 units, reset by a pause over 250 ms or by movement away from the edge. These values are `engine::Tuning` constants.

### 9. Held-input bookkeeping
The engine keeps two sets of keys and buttons:
- **Pressed-local:** held on this machine's OS while focus was local.
- **Forwarded-down:** presses forwarded to a peer, plus presses injected here on a controller's behalf.

On any focus change the machine losing focus emits `Decision::Inject` releases for its set, and the set is cleared. A forwarded release with no matching press is dropped.

### 10. Takeover
While `Controlled`, the engine records physical local motion as bursts:
- A new burst starts after 150 ms without local motion.
- The first burst is withheld (capture mode `WithholdMouse` makes the hook drop it).
- A second burst starting within 2 s, or a burst lasting 2 s, triggers takeover: `SetCapture(PassAll)`, release everything injected for the controller, and send `TakenBack`.
- A physical key press takes over immediately. Keys are never withheld while controlled, so the key reaches the OS.

On `TakenBack` or `PeerLost`, the former controller returns to `Local` and emits `WarpCursor(exit)`.

### 11. Refusing a second controller
`FocusEnter` arriving at a machine that is already `Controlled` by another peer gets `FocusRefused`. The sender returns to `Local` and warps to its exit point. The sender switches to `Forwarding` optimistically when it sends `FocusEnter`, so a refusal costs one round trip, only in this rare case.

### 12. Configuration
`engine::Config { sensitivity: EdgeSensitivity }`, where the default is `Instant`. It's set at construction and replaced by `Event::ConfigChanged`. `EdgeSensitivity` lives in `engine`, not `policy`: the setting and its storage arrive with `add-ipc-and-service`, and managed policy can map onto it then.

### 13. Replay tests
`engine/tests/replay` provides a `Desk`: several machines' engines side by side. A test feeds an event to one machine; every `Send` it decides is delivered to the addressed engine until the desk is quiet, and the test checks each machine's decisions. Each spec scenario is one test named after it. No mocks: an engine's whole world is its event stream.

## Risks / Trade-offs

- [The capture-mode flag lags the decision by a pipe round trip] → The window is a few ms and the cursor is pinned at the edge meanwhile. `add-input-capture` measures it, and could move the flag flip into the agent when the crossing is detected there.
- [Optimistic forwarding on `FocusEnter` sends a few events to a peer that then refuses] → Only happens when a second controller collides. The refusing machine drops input from a peer that isn't its controller.
- [20 raw units is small on high-DPI mice] → It's a `Tuning` constant, tuned with real hardware in `add-input-capture`. The default sensitivity is `instant`, so most users never meet it.
- [HID usages need a complete mapping from Windows codes] → Owned by `win32-input`. The engine only compares for equality, plus the fixed Ctrl, Alt and End set.
- [Unmapped edges are always walls until `add-layout-learning`] → Expected; replay tests supply layouts directly. The MVP needs both changes before two PCs can be used together.

## Open Questions

- Where the engine runs (service or agent) and how often its decisions cross a pipe. This doesn't change the engine's interface; it's settled in `add-ipc-and-service`.
