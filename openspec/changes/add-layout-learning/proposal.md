# Proposal

## Why

After `add-edge-transitions`, the engine crosses only edges that already map to a peer, and nothing fills the map: every edge is a wall. The brief's promise, "push the cursor off an edge and it crosses", needs the layout to be learned by use (ADR 0005), and it needs a way back when learning guesses wrong, because the arrangement view that edits layouts comes after the MVP.

## What Changes

- An unmapped outer side learns its peer on the first deliberate push (the push threshold always applies to learning) when exactly one connected peer is unplaced on this machine. The whole machine side is filled at once: every monitor whose side is at least partly outer on that side.
- With no unplaced peer, or more than one, unmapped sides stay walls. The "which machine?" prompt for 3+ machines comes after the MVP.
- The machine the cursor enters learns the reverse side toward the machine it came from, if that side is unmapped and that machine is unplaced there. No second question.
- After learning, the engine offers Undo for 10 s. Undo removes the learned side on both machines, and if focus went to that peer through the learned edge, focus comes back to where it left.
- "Forget layout" clears this machine's learned layout for every monitor set. The next push relearns it.
- Layouts are kept per monitor set. A set seen for the first time starts from the sides learned for the previous set; returning to a known set restores its own layout.
- The engine reports every layout change so it can be stored, and accepts stored layouts at start-up.
- **Engine messages:** `FocusEnter` gains `from` (the machine whose edge the cursor left), and two messages are added: `FocusWithdrawn` (the controller leaves on its own) and `ForgetSide` (drop the side that maps to me). They're encoded on the wire later, in `add-input-forwarding`, with the rest.

## Capabilities

### New Capabilities
- `layout`: learning edges by use, the reverse edge, Undo, Forget layout, and layouts per monitor set.

### Modified Capabilities
- `input-focus`: "Edges that act as walls" changes. An unmapped side is a wall only when layout learning cannot place a peer there.

## Impact

- **Crates:** `model` (monitor-set keys, a per-set layout book, outer-side coverage, message changes; still no dependencies) and `engine` (learning, reverse edges, Undo, Forget, set switching; replay tests). The dependency rules in AGENTS.md hold.
- **Existing tests:** `input-focus` replay tests keep passing unchanged, except that constructing `FocusEnter` now includes `from`.
- **Later changes:**
  - `add-input-capture`: the session agent shows the Undo notice as a Windows toast, and supplies `MonitorId`s (logging both the EDID and the device path, per ADR 0005).
  - `add-ipc-and-service`: stores the layout book, feeds it back at start-up, and exposes "Forget layout" in the tray.
  - `add-input-forwarding`: encodes the new messages.
