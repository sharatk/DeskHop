# Tasks

## 1. `model` additions

- [x] 1.1 Add `MonitorSetKey` (sorted, de-duplicated `MonitorId`s) with `Screen::set_key`, and `LayoutBook` (key → `Layout`); verify unit tests that monitor order does not change the key and that the book round-trips a layout
- [x] 1.2 Add `Screen::outer_sides(side)`, the monitors whose side on that direction is at least partly outer, using interval coverage of touching neighbours; verify unit tests for one monitor, two side by side, two stacked, and a taller monitor beside a shorter one
- [x] 1.3 Change `PeerMessage::FocusEnter` to carry `from`, and add `FocusWithdrawn` and `ForgetSide { side }`; verify `cargo test -p model` and that `crates/model/Cargo.toml` has no dependencies

## 2. Engine: messages that learning relies on

- [x] 2.1 Send `from` on every `FocusEnter` (itself on its own crossing, the reporting machine on a hand-off), and handle `FocusWithdrawn` on a controlled machine (release injected input, return to `Local`); verify all existing `input_focus` replay tests pass and a new replay test `focus_withdrawn_releases_the_controlled_machine`

## 3. Engine: learning by pushing

- [x] 3.1 Hold a `LayoutBook` and the current set key; learn an unmapped side toward the single unplaced connected peer after the push threshold (whatever the sensitivity), filling every at-least-partly-outer monitor side in that direction, emitting `LayoutsChanged` and `OfferUndo`, then crossing; verify replay tests `first_push_reaches_the_only_unplaced_peer`, `a_brush_against_the_edge_learns_nothing`, and `instant_sensitivity_still_needs_a_push_to_learn`
- [x] 3.2 Verify the whole-side fill with replay tests `two_monitors_stacked` and `taller_monitor_beside_a_shorter_one`
- [x] 3.3 Keep unmapped sides as walls with no candidate or several; verify replay tests `every_peer_already_placed`, `two_unplaced_peers`, and `unmapped_side_with_nothing_to_learn`

## 4. Engine: reverse edges

- [x] 4.1 On `FocusEnter`, learn `side → from` when `from` is unplaced and that direction is unmapped, reporting `LayoutsChanged` without `OfferUndo`; verify replay tests `both_sides_from_one_push`, `reverse_edge_through_a_middle_machine`, and `existing_side_is_kept`

## 5. Engine: Undo and Forget layout

- [x] 5.1 Keep the pending Undo record; handle `Event::Undo { at }` within `Tuning::undo_ms` (remove the side, send `ForgetSide`, and when forwarding to that peer send `FocusWithdrawn` and return to the exit point); handle `ForgetSide` from a peer; verify replay tests `undo_right_after_learning` and `undo_too_late`
- [x] 5.2 Handle `Event::ForgetLayout` (empty the book, clear pending Undo, report); verify replay test `relearn_after_forgetting`

## 6. Engine: monitor sets and reporting

- [x] 6.1 On `ScreenChanged`, switch to a known set's layout, or build a first-seen set's layout by inheriting each direction's peer onto its at-least-partly-outer monitor sides; store only non-empty layouts; verify replay tests `docking_a_laptop` and `undocking_returns_the_old_layout`
- [x] 6.2 Emit `LayoutsChanged` with the full book on every change and accept `Event::LayoutsLoaded` at start-up; verify replay test `learning_is_reported` and a test that a loaded book makes its sides cross without learning

## 7. Documentation

- [x] 7.1 Update the `engine` crate docs with learning, Undo, Forget layout, monitor sets, the new events and decisions, and `undo_ms` in the tuning table; verify `cargo doc -p engine -p model --no-deps` with `RUSTDOCFLAGS="-D warnings"` succeeds

## 8. Integration checks

- [x] 8.1 Verify every scenario in `specs/layout/spec.md` and `specs/input-focus/spec.md` has a replay test of the same name
- [x] 8.2 Verify `cargo test -p proto -p model -p engine -p policy`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `cargo archcheck` pass, and that the Linux CI job passes
- [x] 8.3 Verify `openspec validate add-layout-learning --strict` passes
