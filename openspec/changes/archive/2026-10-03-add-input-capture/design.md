# Design

## Context

- **`win32-input`:** a crate with a doc comment and nothing else. It may use `unsafe`, and only `deskhop-agent` may link it (checked by `cargo archcheck`).
- **`model`:** already has the types this crate needs: `InputEvent`, `InputAction`, `Origin`, `Key` (a HID keyboard-page usage), `Screen`, `Monitor`, `MonitorId` and `Millis`.
- **`engine`:** owns `CaptureMode` and expects:
  - local events in delivery order, each with a timestamp and an origin;
  - injected motion to come back as `Origin::Injected` events, which it uses to cross out of a controlled machine;
  - warps that produce no motion.
- **Hooks:** Windows removes a low-level hook without telling the process when the hook procedure is slow (`LowLevelHooksTimeout`).
- **Workspace pin:** `windows` 0.61.

**Probe result (2026-10-02, this machine, Windows 11).** A scratch program installed `WH_MOUSE_LL`, registered Raw Input for mice with `RIDEV_INPUTSINK`, and injected relative moves tagged in `dwExtraInfo`, which the hook swallowed. The probe found:
- Raw Input still delivered the swallowed moves, with `hDevice` 0 and the tag in `RAWMOUSE::ulExtraInformation`.
- The cursor did not move.
- Physical mouse moves arrived with a nonzero `hDevice` and no tag.
- Hook calls and `WM_INPUT` messages interleave with no one-to-one pairing; one hook call sometimes matched two raw reports, or the other way round.
- Some raw reports carry zero movement.

Not yet checked: how a precision touch pad reports in Raw Input (task 1.2).

## Goals / Non-Goals

**Goals:**
- Implement every requirement in `specs/input-capture/spec.md` behind a small, safe API that `bin/agent` can drive without touching Win32.
- Keep each hook call down to: read a flag, translate the event, push to a channel, return.
- Keep the Win32-free logic in pure functions that unit tests cover. That logic is key translation, EDID parsing, the monitor-identity rule, the withhold decision, and origin classification.

**Non-Goals:**
- Pen, touch, and absolute-position pointers (tablets, remote desktop, VM integration). Their motion is not reported; see D4.
- Elevated foreground windows, the secure desktop, and `SendSAS`.
- Coalescing motion from mice that report at very high rates.
- The pipe to the service, the shared clock, and resetting capture when the pipe drops. These belong to `add-ipc-and-service`.

## Decisions

### D1. API shape

```rust
pub fn start() -> Result<(Capture, Receiver<Captured>), StartError>;
pub enum Captured { Input { at: Millis, input: InputEvent, origin: Origin }, Screen(Screen, Vec<MonitorDetail>) }
impl Capture {
    pub fn set_mode(&self, mode: CaptureMode);
    pub fn inject(&self, action: InputAction) -> Result<(), InjectError>;
    pub fn warp(&self, to: Point) -> Result<(), WarpError>;
}   // Drop: mode to PassAll, unhook, stop both threads, join.
pub fn now() -> Millis;
```

- **Channel:** `std::sync::mpsc`, unbounded, so a stalled consumer can never block the hook. Memory grows only while nothing reads, and the agent always reads.
- **One capture per process:** hook procedures carry no user data, so their state is per process. A second `start()` while one is running returns `StartError::AlreadyRunning`.
- **`MonitorDetail`:** carries every candidate identity for each monitor (EDID id if any, device path, GDI name) for the trace mode. The engine only sees `Screen`.

*Alternative:* a callback in place of a channel. A callback runs consumer code inside the hook, which is exactly the slowness that gets a hook removed.

### D2. Two threads, so the hook thread never does slow work

- **Input thread:** owns both low-level hooks and a hidden top-level window registered for Raw Input (mouse, `RIDEV_INPUTSINK`). It runs a message loop that does nothing else.
  - Each hook call and each `WM_INPUT` costs a few system calls: `GetRawInputData`, `GetCursorPos` and a channel push.
- **Screen thread:** owns a second hidden top-level window and does all display enumeration (D7). Enumeration reads the registry and calls `QueryDisplayConfig`, which can take tens of milliseconds. On the input thread it would delay every hook call by that long.
- **Calls from the caller's thread:** `inject` and `warp` call `SendInput` and `SetCursorPos` directly. Both are thread-safe, and nothing needs to be ordered with the hook thread.
- **Window type:** both windows are hidden top-level windows, not message-only windows, because message-only windows do not receive broadcast messages such as `WM_DISPLAYCHANGE`.

*Alternative:* one thread for everything. This is simpler, but it puts enumeration in the path of the hooks.

### D3. Withholding in the hook

The mode lives in a process-wide `AtomicU8`. `set_mode` stores it with `Release`, and the hook loads it with `Acquire`. One pure function decides:

| Mode | Physical key | Physical button / wheel / move | Injected |
|---|---|---|---|
| `PassAll` | pass | pass | pass |
| `WithholdAll` | withhold | withhold | pass |
| `WithholdMouse` | pass | withhold | pass |

- **Withholding:** the hook returns 1 without calling `CallNextHookEx`.
- **Reporting:** withheld events are still reported. Mouse moves seen by the hook are withheld or passed but never reported, because motion comes from Raw Input (D4).
- **Keys outside the keyboard page:** always passed and never reported.
- **Fake shifts:** scan codes `E0 2A` and `E0 36`, which some keyboards send around navigation keys, are not keys. They are never reported, and under `WithholdAll` they are withheld so nothing leaks.
- **Released keys:** the hook keeps no record of held keys. When focus leaves, the engine already injects releases for keys pressed locally. A release whose press was withheld reaches the OS as an unmatched key-up, which Windows ignores.

### D4. Motion and origin

- **Source of motion:** Raw Input. `WM_INPUT` with relative movement (`MOUSE_MOVE_ABSOLUTE` clear) and a nonzero `lLastX` or `lLastY` becomes `InputEvent::Motion { dx, dy, cursor }`.
  - `cursor` comes from `GetCursorPos` while handling the message. It may trail the move by one event, which is enough for edge tests.
  - Raw reports with absolute movement are not reported.
  - Zero-movement reports are dropped. They carry button changes, which the hook already reports.
- **Origin of keys, buttons and wheel:** the hook flags `LLKHF_INJECTED` and `LLMHF_INJECTED`.
- **Origin of motion:**
  1. `ulExtraInformation` equal to DeskHop's tag means injected. All of DeskHop's `SendInput` calls put the 32-bit tag `0x4448_4F50` ("DHOP") in `dwExtraInfo`.
  2. Otherwise, a nonzero `hDevice` means physical.
  3. Otherwise, it is injected (another program's `SendInput`).
- **Rule in use for step 3: the fallback.** The touch pad check (task 1.2) could not be run: the development machine is a desktop with no usable touch pad. So step 3 uses the fallback, which is correct whichever way a touch pad reports:
  - Step 3 uses the most recent non-DeskHop move the hook saw: physical if that move lacked `LLMHF_INJECTED`, injected otherwise.
  - The hook records that flag in a process-wide atomic.
  - This is an approximation. It is wrong only when a touch pad and another program inject motion within the same few milliseconds.
  - If 7.1 shows touch pads report a nonzero `hDevice`, the fallback still applies only to `hDevice` 0 reports, and those then come only from other programs.
- *Alternative:* motion as differences between successive hook positions. Those are measured after pointer acceleration, which breaks the `input-focus` requirement that forwarded motion is raw.

### D5. Keys

- **Lookup:** a static table maps scan code set 1 (with the `E0` prefix taken from `LLKHF_EXTENDED`) to HID keyboard-page usages and back. It follows Microsoft's "USB HID to PS/2 scan code translation table".
- **Pause and Num Lock:** both arrive as scan code `0x45` and are told apart by virtual-key code (`VK_PAUSE`, `VK_NUMLOCK`). This is the only use of virtual-key codes.
- **Unknown codes:** a scan code with no usage is a key outside the keyboard page (D3).
- **Injection:** `KEYEVENTF_SCANCODE`, plus `KEYEVENTF_EXTENDEDKEY` for `E0` codes, so the local layout decides the character.
  - Pause and Num Lock are injected by virtual key, because their scan code sequences cannot be expressed in one `KEYBDINPUT`.
  - A usage with no scan code returns `InjectError::UnknownKey`, and nothing is injected.
- **Key repeat:** the OS's repeated `WM_KEYDOWN`/`WM_SYSKEYDOWN` are reported as further presses. Injected keys do not auto-repeat on the receiving machine, so forwarding the repeats is how holding a key works on a peer.

### D6. Buttons, wheel, injection and warp

- **Buttons:** left, right and middle from their messages; X1 and X2 from the high word of `mouseData`.
- **Wheel:** the signed high word of `mouseData`. `WM_MOUSEWHEEL` gives `dy`, positive away from the user. `WM_MOUSEHWHEEL` gives `dx`, positive to the right. Units are 1/120 of a notch, documented on `model::InputEvent::Wheel`.
- **Motion injection:** relative `MOUSEEVENTF_MOVE`, which Windows scales by the local pointer speed and acceleration.
- **Wheel injection:** a wheel action with both axes becomes two inputs.
- **Warp:** `SetCursorPos`. Windows does not run low-level hooks or produce Raw Input for it, so no motion is reported.

### D7. Screen and monitor identity

- **Monitors:** `EnumDisplayMonitors`, then `GetMonitorInfoW` (`rcMonitor`, GDI device name), then `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`.
- **Identity source:** `QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS)` links each GDI source name to its targets. `DISPLAYCONFIG_TARGET_DEVICE_NAME` gives each target's `monitorDevicePath`.
- **EDID:** read from the monitor's `Device Parameters\EDID` registry value through SetupAPI (`GUID_DEVINTERFACE_MONITOR`, matched by interface path). It is cached per device path.
  - The EDID is used only if its 128-byte base block has the header and a valid checksum.
  - Serial: descriptor tag `0xFF` text if present, else the nonzero 32-bit field.
- **Identity rule** (from the spec, applied by a pure function):
  - `edid:<MFG><PRODUCT>:<serial>` when the serial is present and unique among connected targets;
  - else `path:<monitorDevicePath, lowercased>`.
- **Mirroring:** monitors mirroring one source become one `Monitor`, whose id joins the sorted target ids with `+`.
- **When to report:** the screen thread enumerates at start, on `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, `WM_SETTINGCHANGE` and `WM_DEVICECHANGE`, and on a 2-second timer. Not every scaling change produces a broadcast, and the timer costs little because EDIDs are cached. It reports only when the `Screen` differs from the last one sent.
- **DPI awareness:** `start()` sets the process to per-monitor v2 (`SetProcessDpiAwarenessContext`). It fails with `StartError::DpiAwareness` if the process has some other awareness. Then `rcMonitor`, hook points, `GetCursorPos` and `SetCursorPos` all use physical pixels on every thread.

*Alternative:* `connectorInstance` plus the EDID ids from `DISPLAYCONFIG_TARGET_DEVICE_NAME`, with no registry read. That has no serial number, so a monitor moved to another port would change identity.

### D8. Clock

`now()` is `QueryPerformanceCounter` scaled to milliseconds. It is monotonic and consistent across processes on Windows 10 and later. Events are stamped when the hook or `WM_INPUT` handler runs. The hook struct's own `time` field is not used, because it is 32-bit tick time with about 15 ms resolution.

How the service lines its own times (such as Undo) up with the agent's is part of `add-ipc-and-service`.

### D9. `CaptureMode` moves to `model`

`model::CaptureMode` keeps the same three variants and docs, and `engine` adds `pub use model::CaptureMode`. Engine tests and API are unchanged. `model` gains no dependency.

### D10. `deskhop-agent --trace`

`--trace` starts capture and prints one line per event, plus every screen report with each monitor's chosen id and its `MonitorDetail`. `--withhold mouse|all <1..10>` sets that mode, and a timer on the agent's main thread puts it back to `PassAll`. If the agent dies early, Windows removes its hooks. The agent remains `#![forbid(unsafe_code)]`.

### D11. Tests

- **Unit tests,** run in `cargo test` on Windows, for the pure functions:
  - the key table both ways and its special cases;
  - EDID parsing of fixture blocks (good, bad checksum, string serial, numeric serial, none);
  - the identity rule (unique, duplicated serials, missing serials, mirroring);
  - the withhold table;
  - wheel decoding;
  - the motion origin rule.
- **Desktop tests,** `#[ignore]`, run by hand with `cargo test -p win32-input -- --ignored --test-threads=1` on a logged-in desktop. They inject real input, so they never run by default:
  - inject F24 and see an injected `0x73`;
  - under `WithholdAll`, injected F24 still reaches the OS (`GetAsyncKeyState`);
  - injected motion comes back injected with the same `dx`/`dy`;
  - a warp moves the cursor and reports nothing;
  - a second `start` fails;
  - the first screen report contains the cursor.
- **Manual checks** with `--trace` cover what software cannot produce: physical origin, withholding of real input, layout independence, the touch pad, monitor identity across ports, and scaling changes.

## Risks / Trade-offs

- **Elevated foreground windows.** `SendInput` from a medium-integrity agent does not reach elevated windows, and hooks may not see input aimed at them, because of UIPI. → Documented limitation; ADR 0002 already plans for the winlogon agent and possibly `uiAccess`.
- **Secure desktop (UAC prompt, lock screen).** Input goes to another desktop, which the session agent cannot see. → The winlogon agent, later.
- **Hook removed for slowness.** → The input thread does nothing else (D2). If Windows removes the hook anyway, input passes through: this fails safe, but capture is lost silently. `add-ipc-and-service` can detect hook loss by noticing no events while Raw Input still arrives, and restart capture.
- **Keys Windows reserves.** Ctrl+Alt+Del and Win+L cannot be withheld. → Accepted: Ctrl+Alt+End is the forwarded secure attention chord, and Win+L locks the local machine.
- **Touch pads (D4).** Classification depends on task 1.2. → The fallback is designed; the spec scenarios do not change either way.
- **Very high-rate mice.** An 8 kHz mouse produces 8,000 `WM_INPUT` messages a second. → Fine for this change; batched reads with `GetRawInputBuffer` are a contained optimisation if profiling needs it.
- **Monitors without a unique serial.** These fall back to the device path, and a port change loses their layout. → This is ADR 0005's accepted fallback. The trace mode shows which rule applied.
- **Threat model.** This change crosses no trust boundary: events stay inside the agent process. The pipe boundary and the winlogon agent update `docs/threat-model.md` in their own changes. → `docs/threat-model.md` is unchanged here.
