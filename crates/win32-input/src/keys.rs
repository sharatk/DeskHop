//! Scan codes (set 1, as low-level hooks report them) to USB HID
//! keyboard-page usages and back, after Microsoft's "USB HID to PS/2 scan
//! code translation table".

use model::Key;

const VK_PAUSE: u32 = 0x13;
const VK_NUMLOCK: u32 = 0x90;
const VK_PACKET: u32 = 0xe7;

/// What a low-level keyboard hook event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyKind {
    /// A key on the HID keyboard page.
    Key(Key),
    /// `E0 2A` or `E0 36`: a shift some keyboards send around navigation
    /// keys. Not a key; never reported.
    FakeShift,
    /// A key with no keyboard-page usage (media, volume, vendor keys), or a
    /// Unicode packet. Always reaches the OS; never reported.
    Other,
}

/// How to inject a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stroke {
    /// By scan code, so the local layout decides the character.
    Scan { code: u16, extended: bool },
    /// By virtual key, for keys whose scan code sequence one input cannot
    /// carry (Pause, Num Lock).
    Virtual { vk: u16, scan: u16, extended: bool },
}

/// `(usage, scan code, E0 prefix)`, one entry per usage.
const TABLE: &[(u16, u8, bool)] = &[
    // Letters a..z.
    (0x04, 0x1e, false),
    (0x05, 0x30, false),
    (0x06, 0x2e, false),
    (0x07, 0x20, false),
    (0x08, 0x12, false),
    (0x09, 0x21, false),
    (0x0a, 0x22, false),
    (0x0b, 0x23, false),
    (0x0c, 0x17, false),
    (0x0d, 0x24, false),
    (0x0e, 0x25, false),
    (0x0f, 0x26, false),
    (0x10, 0x32, false),
    (0x11, 0x31, false),
    (0x12, 0x18, false),
    (0x13, 0x19, false),
    (0x14, 0x10, false),
    (0x15, 0x13, false),
    (0x16, 0x1f, false),
    (0x17, 0x14, false),
    (0x18, 0x16, false),
    (0x19, 0x2f, false),
    (0x1a, 0x11, false),
    (0x1b, 0x2d, false),
    (0x1c, 0x15, false),
    (0x1d, 0x2c, false),
    // Digits 1..9, 0.
    (0x1e, 0x02, false),
    (0x1f, 0x03, false),
    (0x20, 0x04, false),
    (0x21, 0x05, false),
    (0x22, 0x06, false),
    (0x23, 0x07, false),
    (0x24, 0x08, false),
    (0x25, 0x09, false),
    (0x26, 0x0a, false),
    (0x27, 0x0b, false),
    // Enter, Escape, Backspace, Tab, Space.
    (0x28, 0x1c, false),
    (0x29, 0x01, false),
    (0x2a, 0x0e, false),
    (0x2b, 0x0f, false),
    (0x2c, 0x39, false),
    // - = [ ] \ ; ' ` , . /  (0x32, non-US #, shares 0x2B with \ and is
    // never produced here).
    (0x2d, 0x0c, false),
    (0x2e, 0x0d, false),
    (0x2f, 0x1a, false),
    (0x30, 0x1b, false),
    (0x31, 0x2b, false),
    (0x33, 0x27, false),
    (0x34, 0x28, false),
    (0x35, 0x29, false),
    (0x36, 0x33, false),
    (0x37, 0x34, false),
    (0x38, 0x35, false),
    // Caps Lock, F1..F12.
    (0x39, 0x3a, false),
    (0x3a, 0x3b, false),
    (0x3b, 0x3c, false),
    (0x3c, 0x3d, false),
    (0x3d, 0x3e, false),
    (0x3e, 0x3f, false),
    (0x3f, 0x40, false),
    (0x40, 0x41, false),
    (0x41, 0x42, false),
    (0x42, 0x43, false),
    (0x43, 0x44, false),
    (0x44, 0x57, false),
    (0x45, 0x58, false),
    // Print Screen, Scroll Lock; Pause (0x48) is special.
    (0x46, 0x37, true),
    (0x47, 0x46, false),
    // Insert, Home, Page Up, Delete, End, Page Down, arrows.
    (0x49, 0x52, true),
    (0x4a, 0x47, true),
    (0x4b, 0x49, true),
    (0x4c, 0x53, true),
    (0x4d, 0x4f, true),
    (0x4e, 0x51, true),
    (0x4f, 0x4d, true),
    (0x50, 0x4b, true),
    (0x51, 0x50, true),
    (0x52, 0x48, true),
    // Keypad; Num Lock (0x53) is special.
    (0x54, 0x35, true),
    (0x55, 0x37, false),
    (0x56, 0x4a, false),
    (0x57, 0x4e, false),
    (0x58, 0x1c, true),
    (0x59, 0x4f, false),
    (0x5a, 0x50, false),
    (0x5b, 0x51, false),
    (0x5c, 0x4b, false),
    (0x5d, 0x4c, false),
    (0x5e, 0x4d, false),
    (0x5f, 0x47, false),
    (0x60, 0x48, false),
    (0x61, 0x49, false),
    (0x62, 0x52, false),
    (0x63, 0x53, false),
    // Non-US \, Application, Power, keypad =.
    (0x64, 0x56, false),
    (0x65, 0x5d, true),
    (0x66, 0x5e, true),
    (0x67, 0x59, false),
    // F13..F24.
    (0x68, 0x64, false),
    (0x69, 0x65, false),
    (0x6a, 0x66, false),
    (0x6b, 0x67, false),
    (0x6c, 0x68, false),
    (0x6d, 0x69, false),
    (0x6e, 0x6a, false),
    (0x6f, 0x6b, false),
    (0x70, 0x6c, false),
    (0x71, 0x6d, false),
    (0x72, 0x6e, false),
    (0x73, 0x76, false),
    // Keypad comma, International 1..5.
    (0x85, 0x7e, false),
    (0x87, 0x73, false),
    (0x88, 0x70, false),
    (0x89, 0x7d, false),
    (0x8a, 0x79, false),
    (0x8b, 0x7b, false),
    // Left Ctrl, Shift, Alt, Win; right Ctrl, Shift, Alt, Win.
    (0xe0, 0x1d, false),
    (0xe1, 0x2a, false),
    (0xe2, 0x38, false),
    (0xe3, 0x5b, true),
    (0xe4, 0x1d, true),
    (0xe5, 0x36, false),
    (0xe6, 0x38, true),
    (0xe7, 0x5c, true),
];

const PAUSE: Key = Key(0x48);
const NUM_LOCK: Key = Key(0x53);
const PRINT_SCREEN: Key = Key(0x46);

/// Classifies a low-level keyboard hook event from its scan code, its
/// `LLKHF_EXTENDED` flag and its virtual-key code.
pub(crate) fn classify(scan: u32, extended: bool, vk: u32) -> KeyKind {
    match vk {
        VK_PAUSE => return KeyKind::Key(PAUSE),
        VK_NUMLOCK => return KeyKind::Key(NUM_LOCK),
        VK_PACKET => return KeyKind::Other,
        _ => {}
    }
    match (scan, extended) {
        (0x2a | 0x36, true) => return KeyKind::FakeShift,
        // Alt+Print Screen (SysRq) and Ctrl+Pause (Break).
        (0x54, false) => return KeyKind::Key(PRINT_SCREEN),
        (0x46, true) => return KeyKind::Key(PAUSE),
        _ => {}
    }
    TABLE
        .iter()
        .find(|&&(_, s, e)| u32::from(s) == scan && e == extended)
        .map_or(KeyKind::Other, |&(u, _, _)| KeyKind::Key(Key(u)))
}

/// How to inject `key`, or `None` for a usage with no scan code.
pub(crate) fn stroke(key: Key) -> Option<Stroke> {
    match key {
        PAUSE => Some(Stroke::Virtual {
            vk: VK_PAUSE as u16,
            scan: 0x45,
            extended: false,
        }),
        NUM_LOCK => Some(Stroke::Virtual {
            vk: VK_NUMLOCK as u16,
            scan: 0x45,
            extended: true,
        }),
        _ => TABLE
            .iter()
            .find(|&&(u, _, _)| u == key.0)
            .map(|&(_, s, e)| Stroke::Scan {
                code: u16::from(s),
                extended: e,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scan: u32, extended: bool) -> KeyKind {
        classify(scan, extended, 0)
    }

    #[test]
    fn every_entry_round_trips() {
        for &(u, s, e) in TABLE {
            assert_eq!(key(u32::from(s), e), KeyKind::Key(Key(u)), "usage {u:#x}");
            assert_eq!(
                stroke(Key(u)),
                Some(Stroke::Scan {
                    code: u16::from(s),
                    extended: e
                }),
                "usage {u:#x}"
            );
        }
    }

    #[test]
    fn one_entry_per_usage_and_per_scan_code() {
        for (i, a) in TABLE.iter().enumerate() {
            for b in &TABLE[i + 1..] {
                assert_ne!(a.0, b.0, "usage {:#x} twice", a.0);
                assert!((a.1, a.2) != (b.1, b.2), "scan code {:#x} twice", a.1);
            }
        }
    }

    #[test]
    fn modifiers() {
        let found: Vec<KeyKind> = [
            (0x1d, false),
            (0x2a, false),
            (0x38, false),
            (0x5b, true),
            (0x1d, true),
            (0x36, false),
            (0x38, true),
            (0x5c, true),
        ]
        .iter()
        .map(|&(s, e)| key(s, e))
        .collect();
        let expected: Vec<KeyKind> = (0xe0..=0xe7).map(|u| KeyKind::Key(Key(u))).collect();
        assert_eq!(found, expected);
        assert_eq!(key(0x1d, false), KeyKind::Key(Key::LEFT_CTRL));
        assert_eq!(key(0x1d, true), KeyKind::Key(Key::RIGHT_CTRL));
    }

    #[test]
    fn enter_and_keypad_enter_differ() {
        assert_eq!(key(0x1c, false), KeyKind::Key(Key(0x28)));
        assert_eq!(key(0x1c, true), KeyKind::Key(Key(0x58)));
    }

    #[test]
    fn q_and_f24() {
        assert_eq!(key(0x10, false), KeyKind::Key(Key(0x14)));
        assert_eq!(key(0x76, false), KeyKind::Key(Key(0x73)));
    }

    #[test]
    fn pause_and_num_lock_by_virtual_key() {
        assert_eq!(classify(0x45, false, VK_PAUSE), KeyKind::Key(PAUSE));
        assert_eq!(classify(0x45, true, VK_NUMLOCK), KeyKind::Key(NUM_LOCK));
        assert_eq!(key(0x46, true), KeyKind::Key(PAUSE));
        assert!(matches!(
            stroke(PAUSE),
            Some(Stroke::Virtual { vk: 0x13, .. })
        ));
        assert!(matches!(
            stroke(NUM_LOCK),
            Some(Stroke::Virtual { vk: 0x90, .. })
        ));
    }

    #[test]
    fn print_screen_with_alt() {
        assert_eq!(key(0x37, true), KeyKind::Key(PRINT_SCREEN));
        assert_eq!(key(0x54, false), KeyKind::Key(PRINT_SCREEN));
    }

    #[test]
    fn fake_shifts_are_not_keys() {
        assert_eq!(key(0x2a, true), KeyKind::FakeShift);
        assert_eq!(key(0x36, true), KeyKind::FakeShift);
    }

    #[test]
    fn media_keys_and_packets_are_other() {
        // Volume Up (E0 30), Play/Pause (E0 22).
        assert_eq!(key(0x30, true), KeyKind::Other);
        assert_eq!(key(0x22, true), KeyKind::Other);
        assert_eq!(classify(0x41, false, VK_PACKET), KeyKind::Other);
    }

    #[test]
    fn usage_without_scan_code() {
        assert_eq!(stroke(Key(0x32)), None);
        assert_eq!(stroke(Key(0xffff)), None);
    }
}
