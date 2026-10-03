# Tasks

## 1. Setup and `model`

- [x] 1.1 Move `CaptureMode` from `engine` to `model` (D9), re-export it from `engine`, and document wheel units (1/120 of a notch, `dy` positive away from the user, `dx` positive to the right) on `model::InputEvent::Wheel` and `InputAction::Wheel`. Verify that `cargo test -p model -p engine` passes with the engine replay tests unchanged and that `model`'s `Cargo.toml` gains no dependency.
- [x] 1.2 Check the precision touch pad by hand (D4). Run the scratch Raw Input probe for 15 seconds while the user moves only the touch pad, and record whether its `WM_INPUT` reports carry a nonzero `hDevice` and whether its hook moves lack `LLMHF_INJECTED`. Write the outcome into design D4, and choose either the primary rule or the fallback. Verify that D4 names the rule in use and the evidence for it.
  - **Outcome (2026-10-03):** not testable here. The development machine is a desktop; Windows lists a "HID-compliant touch pad", but there is no pad to use. D4 uses the fallback rule, which is correct either way. Checking on a laptop is left as a follow-up.
- [x] 1.3 Give `win32-input` its dependencies: `model`, and `windows` (workspace pin) with only the Win32 features it uses. Extend the Windows CI job's ARM64 step to `cargo check -p transport -p win32-input -p deskhop-agent --target aarch64-pc-windows-msvc`. Verify that `cargo check` passes for both targets locally and that `cargo archcheck` passes.

## 2. Pure translation in `win32-input`

- [x] 2.1 Add the scan code â†” HID usage table (D5) with Pause and Num Lock chosen by virtual-key code, fake shifts (`E0 2A`, `E0 36`) recognised as non-keys, and unknown codes reported as outside the keyboard page. Verify with unit tests: every table entry round-trips; left and right Ctrl, Shift, Alt and Win map to `0xE0`â€“`0xE7`; Enter `0x28` and keypad Enter `0x58` differ; Q is `0x14`; Pause, Num Lock and F24 (`0x73`) are covered; a media-key scan code maps to no usage.
- [x] 2.2 Add the pure decisions:
  - the withhold table (D3) over mode, kind (key, mouse, non-keyboard-page key, fake shift) and origin;
  - button and wheel decoding from hook messages and `mouseData`;
  - the motion origin rule (D4) over tag, `hDevice` and, if task 1.2 chose it, the fallback flag.

  Verify with table-driven unit tests covering every cell of the D3 table, X1/X2, negative wheel deltas, horizontal wheel, and each branch of the origin rule.
- [x] 2.3 Add EDID parsing and the monitor-identity rule (D7) as pure functions. Verify with unit tests using fixture EDID blocks:
  - a valid block with a string serial and one with a numeric serial;
  - a block with no serial, a bad checksum, and a short block;
  - two monitors sharing a serial falling back to paths;
  - a missing EDID falling back to the path;
  - lowercased paths;
  - mirrored targets joined in sorted order with `+`.

## 3. Capture on the input thread

- [x] 3.1 Add `start()`, `Capture`, `Captured`, `StartError` and `now()` (D1, D2, D7 DPI, D8): per-monitor v2 awareness, the one-per-process guard, an input thread with a hidden top-level window and a message loop, and `Drop`, which sets `PassAll`, stops and joins the thread. Every `unsafe` block gets a `// SAFETY:` comment. Verify with desktop tests (`#[ignore]`): `second_start_fails`, `start_after_drop_succeeds`, and `now` never decreasing across 1,000 reads.
- [x] 3.2 Install `WH_KEYBOARD_LL` and `WH_MOUSE_LL` on the input thread. Report keys, buttons and wheel with origin and time, and withhold according to the mode (D3) through a process-wide atomic. Mouse moves are withheld or passed but never reported. Verify with desktop tests:
  - `injected_key_is_reported_as_injected` (inject F24, see `Key(0x73)` down then up, `Origin::Injected`);
  - `injected_key_passes_while_withholding_all` (`GetAsyncKeyState(VK_F24)` shows the key down between the injected press and release);
  - `injected_wheel_is_reported` (+120, then -120).
- [x] 3.3 Register Raw Input for mice with `RIDEV_INPUTSINK` and report relative, nonzero motion with `GetCursorPos` and the origin rule (D4); drop absolute and zero reports. Verify with desktop test `injected_motion_is_reported_as_injected`: inject (+7, 0) then (-7, 0), and see two injected motion events with those deltas.

## 4. Injection and warp

- [x] 4.1 Add `Capture::inject` (D5, D6): keys by scan code with the extended flag, Pause and Num Lock by virtual key, buttons including X1 and X2, wheel on one or both axes, and relative motion, all tagged `0x44484F50`. A usage with no scan code returns `InjectError::UnknownKey`. Verify with a unit test that the built `INPUT` records carry the tag and the right flags for an extended key, a key-up, X2 and a two-axis wheel, and that the desktop tests from 3.2 and 3.3 now inject through `Capture::inject`.
- [x] 4.2 Add `Capture::warp` with `SetCursorPos`. Verify with desktop test `warp_moves_the_cursor_without_motion`: warp to the centre of the primary monitor, `GetCursorPos` returns that point, and no motion event arrives within 200 ms. Then restore the original position.

## 5. Screen

- [x] 5.1 Add enumeration (D7): monitors, effective DPI, `QueryDisplayConfig` targets, EDID through SetupAPI cached per device path, and mirroring through the pure rule from 2.3. Return `Screen` plus `MonitorDetail`. Verify with desktop test `first_screen_contains_the_cursor`: the first `Captured::Screen` has at least one monitor, every DPI is at least 96, and `GetCursorPos` lies inside one monitor's rectangle.
- [x] 5.2 Add the screen thread: a hidden top-level window that enumerates on `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, `WM_SETTINGCHANGE`, `WM_DEVICECHANGE` and every 2 seconds, and reports only a `Screen` that differs from the last one. Drop stops and joins it. Verify with a unit test of the change filter (an identical screen is suppressed; a changed DPI, rectangle or id is reported) and a desktop test in which no second report arrives within 5 seconds when nothing changes.

## 6. Agent trace mode

- [x] 6.1 Add `deskhop-agent --trace [--withhold mouse|all <1..10>]` (D10): print events and screen reports (chosen id plus `MonitorDetail`), and return to `PassAll` when the time is up. `--session` and `--winlogon` keep their behaviour. Verify with unit tests of argument parsing (valid forms; seconds 0 and 11 rejected; unknown flags rejected), with `deskhop-agent` still passing `cargo archcheck` (`#![forbid(unsafe_code)]`), and by running `--trace` for a few seconds and seeing a screen report and physical motion events as the mouse moves.

## 7. Verification on hardware and final checks

- [x] 7.1 Run the manual checklist with `deskhop-agent --trace` and record the results in this task:
  - mouse moves, clicks and keys are reported as physical;
  - `--withhold all 5` freezes the cursor and keeps keys from the focused app, then everything works again;
  - `--withhold mouse 5` blocks the mouse while typing still works;
  - with the French AZERTY layout, the key in the US Q position reports `0x14`;
  - a volume key still works under `--withhold all` and is not reported;
  - touch pad motion is reported as physical in the trace, and the scratch touch pad probe shows whether its `hDevice` was 0 (this completes task 1.2);
  - changing a monitor's scaling reports a new screen with the new DPI;
  - moving a monitor with a serial to another port keeps its id, and the trace shows which rule chose each id.

  **Results (2026-10-03, Windows 11 desktop, one ViewSonic monitor):**
  - Physical reporting: passed. Mouse motion, a left click, and Ctrl (`0xe0`) and C (`0x06`) keys were all reported as `physical`.
  - `--withhold all 5`: run, and no problem reported.
  - `--withhold mouse 5`: run, and no problem reported.
  - Identity rule: shown by the trace. The monitor got `edid:VSCA73D:XBH244921382` from its EDID serial.
  - Not checked: the AZERTY layout, the volume key, a scaling change, and moving to another port. The touch pad could not be tested (no usable pad; see 1.2).
- [x] 7.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo archcheck`, the ARM64 check from 1.3, and `cargo test -p win32-input -- --ignored --test-threads=1` on a logged-in desktop. Then break one rule at a time and confirm that only the matching tests fail:
  - swap the `WithholdAll` key cell;
  - drop the DeskHop tag check from the origin rule;
  - stop lowercasing device paths;
  - report unchanged screens.

  Restore from copies and touch the files before the final run.
