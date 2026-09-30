# Proposal

## Why

The product is the moment the cursor slides off one screen onto another, and the moments around it: a colleague touching the other PC, a peer going to sleep, a Ctrl key held as focus moves. Every later change (layout learning, input capture, transport) plugs into the decisions made here, so the rules need to exist first as a pure state machine that can be replayed and tested on Linux, before any hook or socket exists (design brief build order: `model` + `engine` with replay tests).

## What Changes

- Define neutral input events in `model`: key (HID usage), mouse button, wheel, and raw motion, each tagged as physical or injected, with a monotonic timestamp.
- Define screen geometry in `model`: monitors in virtual-screen pixels with their DPI, monitor sides, and the layout slots (monitor side → peer) that `add-layout-learning` will fill.
- Define the engine: focus ownership, crossing rules, takeover, releasing held keys, peer loss, and the Ctrl+Alt+End secure-attention request. Inputs are `model` events; outputs are decisions (what the local OS should swallow or have injected, where to warp the cursor, what to tell a peer, when to wake the engine again).
- Crossing a mapped edge is instant by default. An "always require a push" setting adds a push threshold. Corner dead zones (about 2 mm) are always on. A held mouse button, a disconnected peer, or an unmapped edge makes the edge a wall.
- The cursor enters the next machine at the same relative position along the facing side.
- A controlled machine reports its own edge exits, so each machine owns its own edges and a cursor can move back, or on to a third machine.
- On the controlled machine, local physical input takes control back: a key press immediately, mouse movement on a second burst within 2 s or after 2 s of continuous movement, with the first burst swallowed.
- Every focus change, including losing a peer, releases held keys and buttons on the machine losing focus. Losing a peer returns the cursor to where it left.
- Forwarded motion is raw device movement. The receiving machine applies its own pointer speed.
- Replay-test harness for `engine`: a sequence of events in, a sequence of decisions out.

Not in this change: learning or changing the layout (`add-layout-learning`), wire encoding of peer messages and motion (`add-input-forwarding`, the renamed `add-motion-forwarding`), hooks and injection (`add-input-capture`), calling `SendSAS` (`add-ipc-and-service`).

## Capabilities

### New Capabilities
- `input-focus`: which machine receives input, when and how the cursor crosses an edge, how control is taken back, and what is released when focus moves.

### Modified Capabilities
_None._ `wire-protocol` is unchanged; peer messages are defined here as neutral types and encoded on the wire by a later change.

## Impact

- **Crates:** `model` (new types, still no dependencies) and `engine` (state machine and replay tests, depending only on `model`). The dependency rules in AGENTS.md hold. `engine` keeps its declared dependencies on `proto` and `policy` but does not use them yet. No `win32-*`, `transport`, or `ipc` code changes.
- **Linux CI:** `cargo test -p model -p engine` covers everything in this change.
- **Later changes:** `add-input-capture` must honor the capture mode the engine publishes from inside the hook procedure (a flag read, not an engine call), because hooks may only enqueue. `add-input-forwarding` encodes `model::PeerMessage` in `proto`. `add-ipc-and-service` stores the edge-sensitivity setting and performs the secure-attention request.
- **Plan:** the MVP change list renames `add-motion-forwarding` to `add-input-forwarding`, which now also carries the focus, key, and button messages.
