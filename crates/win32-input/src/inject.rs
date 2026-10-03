//! Injecting input and warping the cursor (design D5, D6).

use std::fmt;

use model::{Button, InputAction, Key, Point};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

use crate::decide::TAG;
use crate::keys::{self, Stroke};

const XBUTTON1: u32 = 1;
const XBUTTON2: u32 = 2;

/// Why [`crate::Capture::inject`] injected nothing, or not everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectError {
    /// The usage has no scan code; nothing was injected.
    UnknownKey(Key),
    /// Windows accepted fewer inputs than given: another desktop is active,
    /// or the foreground window has a higher integrity level.
    Blocked,
}

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey(k) => write!(f, "no scan code for HID usage {:#04x}", k.0),
            Self::Blocked => f.write_str("Windows blocked the injected input"),
        }
    }
}

impl std::error::Error for InjectError {}

/// Why [`crate::Capture::warp`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarpError(pub windows::core::Error);

impl fmt::Display for WarpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "moving the cursor failed: {}", self.0)
    }
}

impl std::error::Error for WarpError {}

pub(crate) fn send(action: InputAction) -> Result<(), InjectError> {
    let inputs = inputs(action)?;
    if inputs.is_empty() {
        return Ok(());
    }
    // SAFETY: `inputs` is a slice of fully initialised INPUT records, and
    // the size passed is that of one record.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(InjectError::Blocked)
    }
}

pub(crate) fn warp(to: Point) -> Result<(), WarpError> {
    // SAFETY: plain values; the process is per-monitor DPI aware, so these
    // are physical virtual-screen pixels.
    unsafe { SetCursorPos(to.x, to.y) }.map_err(WarpError)
}

/// The INPUT records for `action`, each tagged with [`TAG`].
pub(crate) fn inputs(action: InputAction) -> Result<Vec<INPUT>, InjectError> {
    Ok(match action {
        InputAction::Key { key, down } => {
            let stroke = keys::stroke(key).ok_or(InjectError::UnknownKey(key))?;
            let up = if down {
                KEYBD_EVENT_FLAGS(0)
            } else {
                KEYEVENTF_KEYUP
            };
            let ext = |e: bool| {
                if e {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                }
            };
            let (vk, scan, flags) = match stroke {
                Stroke::Scan { code, extended } => (0, code, KEYEVENTF_SCANCODE | ext(extended)),
                Stroke::Virtual { vk, scan, extended } => (vk, scan, ext(extended)),
            };
            vec![key_input(vk, scan, flags | up)]
        }
        InputAction::Button { button, down } => {
            let (flags, data) = match (button, down) {
                (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
                (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
                (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                (Button::X1, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
                (Button::X1, false) => (MOUSEEVENTF_XUP, XBUTTON1),
                (Button::X2, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
                (Button::X2, false) => (MOUSEEVENTF_XUP, XBUTTON2),
            };
            vec![mouse_input(0, 0, data, flags)]
        }
        InputAction::Wheel { dx, dy } => {
            let mut v = Vec::new();
            if dy != 0 {
                v.push(mouse_input(0, 0, dy as u32, MOUSEEVENTF_WHEEL));
            }
            if dx != 0 {
                v.push(mouse_input(0, 0, dx as u32, MOUSEEVENTF_HWHEEL));
            }
            v
        }
        InputAction::Motion { dx, dy } => vec![mouse_input(dx, dy, 0, MOUSEEVENTF_MOVE)],
    })
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: TAG as usize,
            },
        },
    }
}

fn mouse_input(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: TAG as usize,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(input: &INPUT) -> KEYBDINPUT {
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        // SAFETY: the type says the keyboard member was written.
        unsafe { input.Anonymous.ki }
    }

    fn mouse(input: &INPUT) -> MOUSEINPUT {
        assert_eq!(input.r#type, INPUT_MOUSE);
        // SAFETY: the type says the mouse member was written.
        unsafe { input.Anonymous.mi }
    }

    #[test]
    fn extended_key_up_by_scan_code() {
        // Right Ctrl: E0 1D.
        let v = inputs(InputAction::Key {
            key: Key::RIGHT_CTRL,
            down: false,
        })
        .unwrap();
        let k = key(&v[0]);
        assert_eq!(v.len(), 1);
        assert_eq!(k.wVk, VIRTUAL_KEY(0));
        assert_eq!(k.wScan, 0x1d);
        assert_eq!(
            k.dwFlags,
            KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP
        );
        assert_eq!(k.dwExtraInfo, TAG as usize);
    }

    #[test]
    fn plain_key_down() {
        let v = inputs(InputAction::Key {
            key: Key(0x14),
            down: true,
        })
        .unwrap();
        let k = key(&v[0]);
        assert_eq!((k.wScan, k.dwFlags), (0x10, KEYEVENTF_SCANCODE));
    }

    #[test]
    fn pause_by_virtual_key() {
        let v = inputs(InputAction::Key {
            key: Key(0x48),
            down: true,
        })
        .unwrap();
        let k = key(&v[0]);
        assert_eq!(k.wVk, VIRTUAL_KEY(0x13));
        assert_eq!(k.dwFlags, KEYBD_EVENT_FLAGS(0));
    }

    #[test]
    fn unknown_key() {
        let r = inputs(InputAction::Key {
            key: Key(0x32),
            down: true,
        });
        assert!(matches!(r, Err(InjectError::UnknownKey(Key(0x32)))));
    }

    #[test]
    fn x2_button() {
        let v = inputs(InputAction::Button {
            button: Button::X2,
            down: true,
        })
        .unwrap();
        let m = mouse(&v[0]);
        assert_eq!((m.dwFlags, m.mouseData), (MOUSEEVENTF_XDOWN, XBUTTON2));
        assert_eq!(m.dwExtraInfo, TAG as usize);
    }

    #[test]
    fn two_axis_wheel() {
        let v = inputs(InputAction::Wheel { dx: 120, dy: -240 }).unwrap();
        assert_eq!(v.len(), 2);
        let (a, b) = (mouse(&v[0]), mouse(&v[1]));
        assert_eq!((a.dwFlags, a.mouseData as i32), (MOUSEEVENTF_WHEEL, -240));
        assert_eq!((b.dwFlags, b.mouseData as i32), (MOUSEEVENTF_HWHEEL, 120));
        assert!(
            inputs(InputAction::Wheel { dx: 0, dy: 0 })
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn relative_motion() {
        let v = inputs(InputAction::Motion { dx: 7, dy: -3 }).unwrap();
        let m = mouse(&v[0]);
        assert_eq!((m.dx, m.dy, m.dwFlags), (7, -3, MOUSEEVENTF_MOVE));
        assert_eq!(m.dwExtraInfo, TAG as usize);
    }
}
