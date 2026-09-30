# Tasks

## 1. `model` types

- [x] 1.1 Add time and input types to `model`: `Millis`, `Origin` (physical or injected), `Key` (HID usage), `Button`, `Wheel`, raw `Motion` with the resulting cursor position, and `InputEvent`; verify `cargo build -p model` and that `crates/model/Cargo.toml` still has no dependencies
- [x] 1.2 Add geometry types and helpers: `Rect`, `MonitorId`, `Monitor` (with DPI), `Screen`, `Side`, the outer-side test (no monitor one pixel beyond), and corner-zone length `round(2 mm Ã— dpi / 25.4)`; verify unit tests for a side-by-side pair, a partly overlapping pair, and corner lengths of 8 px at 96 DPI and 15 px at 192 DPI
- [x] 1.3 Add the `u16` fixed-point edge fraction with conversion to and from a point along a machine's screen extent, clamped to the nearest monitor pixel on a side; verify unit tests for 0%, 25%, 100%, and a landing point that falls in a gap between monitors
- [x] 1.4 Add `PeerId`, `Layout` (`(MonitorId, Side)` â†’ `PeerId`), and `PeerMessage` (`FocusEnter`, `FocusRefused`, `EdgeExit`, `TakenBack`, `Key`, `Button`, `Wheel`, `Motion`, `SecureAttention`); verify `cargo test -p model` and `cargo clippy -p model --all-targets -- -D warnings` pass

## 2. Engine skeleton and replay harness

- [x] 2.1 Add `engine::Event`, `Decision`, `Config`, `EdgeSensitivity` (default `Instant`), `Tuning` constants, `CaptureMode`, and `Engine::new`/`handle` with the `Local` state only; verify `cargo build -p engine` on stable
- [x] 2.2 Add the replay builder in `crates/engine/tests/` (timestamped events in, decisions out, with readable failure output); verify with the scenario test `input_stays_local`

## 3. Focus and forwarding

- [x] 3.1 Implement `Forwarding` state: withhold and forward physical key, button, wheel, and raw motion to the focused peer, cursor parked at the exit point, `SetCapture(WithholdAll)`; verify replay test `input_goes_to_the_focused_peer`
- [x] 3.2 Ignore injected-origin local input for forwarding and takeover; forward motion as raw deltas; verify replay tests `injected_motion_does_not_take_over` and `different_pointer_speeds` (forwarded deltas equal the raw input)

## 4. Crossing from this machine

- [x] 4.1 Implement crossing on an outer, mapped side toward a connected peer with no button held, sending `FocusEnter` with the entry fraction; verify replay tests `instant_crossing` and `inner_side_is_not_an_edge`
- [x] 4.2 Implement walls for unmapped sides, disconnected peers, and held buttons; verify replay tests `dragging_at_the_edge` and `peer_not_connected`
- [x] 4.3 Implement corner zones from monitor DPI; verify replay tests `closing_a_maximized_window` and `corner_size_follows_dpi`
- [x] 4.4 Implement `push` sensitivity (20 units, 250 ms pause reset, reset on inward movement) and `Event::ConfigChanged`; verify replay tests `push_sensitivity_needs_a_deliberate_push`, `push_sensitivity_crosses_on_a_sustained_push`, and `pause_resets_the_push`
- [x] 4.5 Implement entry placement on `FocusEnter` (opposite side, same fraction, clamped to a monitor), becoming `Controlled`; verify replay test `same_relative_height`

## 5. Crossing out of a controlled machine

- [x] 5.1 Apply crossing rules to injected motion on a controlled machine and send `EdgeExit`; on the controller, move focus back to itself at the entry position or on to a connected third peer, treating an unreachable third peer as a wall; verify replay tests `back_to_the_controlling_machine` and `on_to_a_third_machine`
- [x] 5.2 Refuse `FocusEnter` from a second peer while controlled, and return a refused sender to `Local` at its exit point; verify replay test `second_controller_refused`

## 6. Releasing held input

- [x] 6.1 Track pressed-local and forwarded-down keys and buttons; on every focus change, inject releases on the machine losing focus; drop unmatched forwarded releases; verify replay tests `crossing_with_shift_held`, `release_after_the_crossing`, and `leaving_a_controlled_machine_with_a_key_held`

## 7. Takeover and peer loss

- [x] 7.1 Implement takeover bursts on a controlled machine (150 ms gap, first burst withheld via `WithholdMouse`, second burst within 2 s or a 2 s burst takes over, windows decided from event timestamps) and immediate takeover on a physical key; verify replay tests `accidental_nudge`, `second_movement_takes_over`, `sustained_movement_takes_over`, and `key_press_takes_over`, plus a test that a burst 2.5 s after the first counts as a first burst again
- [x] 7.2 Handle `TakenBack` and `PeerLost` on the controller (back to `Local`, warp to the exit point, release held input) and `PeerLost` on the controlled machine (release injected keys); verify replay tests `controlled_machine_takes_over`, `peer_lost_while_focused`, and `controller_lost_while_controlled`

## 8. Secure attention request

- [x] 8.1 While forwarding, withhold Ctrl+Alt+End and send `SecureAttention`; on receipt, emit `Decision::RequestSecureAttention`; with local focus, pass the keys through; verify replay tests `ctrl_alt_end_while_controlling_a_peer` and `ctrl_alt_end_with_local_focus`

## 9. Documentation

- [x] 9.1 Document the engine's event and decision types, its focus states, and every `Tuning` value in the `engine` crate docs; verify `cargo doc -p engine --no-deps` builds without warnings

## 10. Integration checks

- [x] 10.1 Verify every scenario in `specs/input-focus/spec.md` has a replay test of the same name (checked with a grep of scenario titles against test names)
- [x] 10.2 Verify `cargo test -p proto -p model -p engine -p policy`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `cargo archcheck` pass, and that the Linux CI job passes
- [x] 10.3 Verify `openspec validate add-edge-transitions --strict` passes
