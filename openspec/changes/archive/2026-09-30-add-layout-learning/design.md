# Design

## Context

`engine` holds one `model::Layout` (monitor side → peer) and treats an unmapped side as a wall (`input-focus`). Nothing fills the layout yet, and nothing keeps it per monitor set. `model::Screen` can tell whether one point is on an outer edge, but not whether a monitor's side is outer anywhere along its length. Peer messages are neutral `model` types with no wire encoding yet.

Requirements are in `specs/layout/spec.md` and the modified `specs/input-focus/spec.md`. Decisions from exploration and ADR 0005 that shape this: slots per monitor side; the MVP fills a whole machine side; one book of layouts per monitor set; learning needs a deliberate push; the first crossing goes to the only unplaced peer, with Undo; and "Forget layout" is the MVP escape hatch.

## Goals / Non-Goals

**Goals:**
- Learning, reverse learning, Undo, Forget, and monitor-set switching as pure engine behavior, replay-tested like `input-focus`.
- A layout book that the service can store and hand back unchanged.

**Non-Goals:**
- The "which machine?" prompt for 3+ machines, the arrangement view, or splitting one side between two peers (all post-MVP).
- Showing the toast, the tray command, or storing the book (`add-input-capture`, `add-ipc-and-service`).
- Choosing what goes into `MonitorId` (`add-input-capture`, per ADR 0005).

## Decisions

### 1. A layout book keyed by monitor set
`model::MonitorSetKey` is the sorted, de-duplicated list of `MonitorId`s in a `Screen`. `model::LayoutBook` maps keys to `Layout`s. The engine holds the book plus the key of the current set; everything else in the engine keeps reading "the current layout".
- *Alternative: one layout, rebuilt on every monitor change.* Loses the docked layout on every undock. Rejected; ADR 0005 asks for one per set.

### 2. "At least partly outer" by interval coverage
For monitor `m` and a direction, collect the monitors that touch `m` on that side (their near edge equals `m`'s far edge) and overlap its span. The side is at least partly outer if those overlaps don't cover `m`'s whole span. Filling a machine side means mapping that direction for every such monitor. This agrees with the existing point test (`Screen::outer_edge`) wherever the cursor can actually be.

### 3. Learning inside the crossing check
`Engine::crossing` already finds the outer side, corner zone and held buttons. For an unmapped side, it now asks for the single unplaced connected peer: a peer on no side of the current layout.
- **One candidate:** the push accumulator runs whatever the sensitivity setting, because learning always needs a push. When the threshold is reached, the engine fills the side, emits `LayoutsChanged` and `OfferUndo`, and then crosses exactly as for a mapped side.
- **No candidate, or more than one:** the side stays a wall, as today.

### 4. `FocusEnter` carries `from`
The reverse edge must point at the machine whose edge the cursor left. After a hand-off through a middle machine, that isn't the machine sending the message. `PeerMessage::FocusEnter` becomes `{ side, fraction, from }`:
- On its own crossing, the sender puts itself in `from`.
- On a hand-off (`EdgeExit`), the controller puts in the machine that reported the exit.

The receiver learns `side → from` when `from` is unplaced there and every at-least-partly-outer monitor side in that direction is unmapped. Reverse learning isn't a guess, so it offers no Undo and doesn't need `from` to be the only unplaced peer.

### 5. Undo is one pending record
The engine remembers only the most recent learning: `{ peer, side, learned_at }`. `Event::Undo { at }` acts only within `Tuning::undo_ms` (10,000) of `learned_at`. Undo:
- removes the side from the current layout;
- sends `ForgetSide { side: side.opposite() }` to the peer, which removes the slots on that side that map to the sender, in its current set;
- if this machine is forwarding to that peer, sends `FocusWithdrawn` and returns to `Local` at the exit point.

`FocusWithdrawn` makes the controlled machine release what it injected and return to `Local`. It's the first way for a controller to leave on its own.
- *Simplification:* "focus went through the learned crossing" is checked as "currently forwarding to that peer". Within a 10 s window the difference is negligible.

### 6. Forget layout clears the whole book
`Event::ForgetLayout` empties the book (all monitor sets), clears any pending Undo, and emits `LayoutsChanged`. Peers keep their own layouts: each machine owns its edges, and forgetting on both machines is two clicks.

### 7. First-seen monitor sets inherit by side
On `ScreenChanged` to a key the book doesn't have, the new layout is built from the previous current layout:
- For each direction, take the peer mapped on the most monitor sides in that direction (on a tie, the lowest `PeerId`).
- Map that peer onto every at-least-partly-outer monitor side of the new set in that direction.

A known key just switches. The book stores a set only once its layout isn't empty, so plugging in a projector for a minute doesn't leave an entry behind.

### 8. Reporting and loading
The engine emits `Decision::LayoutsChanged(LayoutBook)` with the full book whenever it changes. The book is small (a handful of sets × a handful of sides), and a full snapshot needs no merge logic in the service. `Event::LayoutsLoaded(LayoutBook)` replaces the book at start-up and selects the current set's layout. `Event::LayoutChanged(Layout)` stays: it sets the current set's layout, for tests now and the arrangement view later.

## Risks / Trade-offs

- [An accidental push still teaches a wrong edge] → The push threshold applies to learning even with `instant`, Undo is offered for 10 s, and Forget layout always works.
- [A paired peer can clear a side on this machine with `ForgetSide`] → Paired peers already drive this machine's input; this adds no meaningful power. `add-input-forwarding` records it in the threat model when the message goes on the wire.
- [Inheritance guesses wrong for an unusual new monitor arrangement] → It only fills sides the old set had, and learning, Undo and Forget still apply.
- [`MonitorId` changes when a monitor moves between dock ports] → Then the set counts as first-seen, inherits by side, and keeps working. The per-set history is lost, not the layout. The ADR 0005 spike decides the ID.

## Migration Plan

None. Nothing is stored yet. The `FocusEnter` field change touches only engine code and tests, because the message isn't on the wire.
