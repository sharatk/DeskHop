//! EDID parsing and the monitor-identity rule (design D7), as pure functions.

use model::MonitorId;

const HEADER: [u8; 8] = [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00];
const BASE_LEN: usize = 128;
const DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];
const DESCRIPTOR_LEN: usize = 18;
const SERIAL_TAG: u8 = 0xff;

/// What identifies a monitor in its EDID base block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edid {
    /// Three-letter PNP manufacturer id, such as `DEL`.
    pub manufacturer: String,
    pub product: u16,
    /// The serial-number descriptor's text, else the nonzero numeric serial
    /// in hex; `None` when the monitor reports neither.
    pub serial: Option<String>,
}

impl Edid {
    /// The identity this EDID gives when its serial is present and unique.
    fn key(&self) -> Option<String> {
        let serial = self.serial.as_ref()?;
        Some(format!(
            "edid:{}{:04X}:{serial}",
            self.manufacturer, self.product
        ))
    }
}

/// Parses an EDID base block; `None` unless it is at least 128 bytes with
/// the fixed header, a valid checksum and a well-formed manufacturer id.
pub(crate) fn parse(bytes: &[u8]) -> Option<Edid> {
    let base = bytes.get(..BASE_LEN)?;
    if base[..8] != HEADER || base.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0 {
        return None;
    }
    let packed = u16::from_be_bytes([base[8], base[9]]);
    let manufacturer = [10u16, 5, 0]
        .iter()
        .map(|shift| match (packed >> shift) & 0x1f {
            n @ 1..=26 => Some(char::from(b'A' + n as u8 - 1)),
            _ => None,
        })
        .collect::<Option<String>>()?;
    let product = u16::from_le_bytes([base[10], base[11]]);
    let numeric = u32::from_le_bytes([base[12], base[13], base[14], base[15]]);

    let text = DESCRIPTORS.iter().find_map(|&at| {
        let d = &base[at..at + DESCRIPTOR_LEN];
        if d[..3] != [0, 0, 0] || d[3] != SERIAL_TAG {
            return None;
        }
        let raw = &d[5..];
        let end = raw.iter().position(|&b| b == 0x0a).unwrap_or(raw.len());
        let s: String = raw[..end]
            .iter()
            .filter(|b| b.is_ascii_graphic() || **b == b' ')
            .map(|&b| char::from(b))
            .collect();
        let s = s.trim().to_owned();
        (!s.is_empty()).then_some(s)
    });
    let serial = text.or_else(|| (numeric != 0).then(|| format!("{numeric:08X}")));
    Some(Edid {
        manufacturer,
        product,
        serial,
    })
}

/// One display target: a monitor attached to a connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// The monitor device interface path (`monitorDevicePath`).
    pub path: String,
    pub edid: Option<Edid>,
}

/// Each target's identity: its EDID key when that key is present and no
/// other connected target shares it, else its lowercased device path.
pub(crate) fn target_ids(targets: &[Target]) -> Vec<String> {
    let keys: Vec<Option<String>> = targets
        .iter()
        .map(|t| t.edid.as_ref().and_then(Edid::key))
        .collect();
    targets
        .iter()
        .zip(&keys)
        .map(|(t, key)| match key {
            Some(k) if keys.iter().filter(|o| o.as_ref() == Some(k)).count() == 1 => k.clone(),
            _ => format!("path:{}", t.path.to_lowercase()),
        })
        .collect()
}

/// The identity of a monitor shown on `ids`' targets: one target's id, or
/// for mirrored targets their ids sorted and joined with `+`.
pub(crate) fn monitor_id(mut ids: Vec<String>) -> MonitorId {
    ids.sort();
    ids.dedup();
    MonitorId(ids.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid base block for `DEL` product 0x4123 with `numeric` serial and
    /// an optional serial-number descriptor.
    fn block(numeric: u32, text: Option<&str>) -> Vec<u8> {
        let mut b = vec![0u8; BASE_LEN];
        b[..8].copy_from_slice(&HEADER);
        // D=4, E=5, L=12.
        let packed: u16 = (4 << 10) | (5 << 5) | 12;
        b[8..10].copy_from_slice(&packed.to_be_bytes());
        b[10..12].copy_from_slice(&0x4123u16.to_le_bytes());
        b[12..16].copy_from_slice(&numeric.to_le_bytes());
        // A monitor-name descriptor first, so the serial is found by tag.
        b[54..59].copy_from_slice(&[0, 0, 0, 0xfc, 0]);
        b[59..64].copy_from_slice(b"U2720");
        if let Some(t) = text {
            b[72..77].copy_from_slice(&[0, 0, 0, SERIAL_TAG, 0]);
            let mut field = [0x20u8; 13];
            field[..t.len()].copy_from_slice(t.as_bytes());
            if t.len() < 13 {
                field[t.len()] = 0x0a;
            }
            b[77..90].copy_from_slice(&field);
        }
        let sum = b.iter().fold(0u8, |a, x| a.wrapping_add(*x));
        b[127] = 0u8.wrapping_sub(sum);
        b
    }

    fn edid(serial: Option<&str>) -> Edid {
        Edid {
            manufacturer: "DEL".into(),
            product: 0x4123,
            serial: serial.map(Into::into),
        }
    }

    #[test]
    fn string_serial() {
        assert_eq!(
            parse(&block(0x01020304, Some("ABC123"))),
            Some(edid(Some("ABC123")))
        );
    }

    #[test]
    fn numeric_serial() {
        assert_eq!(
            parse(&block(0x01020304, None)),
            Some(edid(Some("01020304")))
        );
    }

    #[test]
    fn no_serial() {
        assert_eq!(parse(&block(0, None)), Some(edid(None)));
        assert_eq!(parse(&block(0, Some("   "))), Some(edid(None)));
    }

    #[test]
    fn extension_blocks_are_ignored() {
        let mut b = block(7, None);
        b.extend_from_slice(&[0x02; 128]);
        assert_eq!(parse(&b), Some(edid(Some("00000007"))));
    }

    #[test]
    fn bad_blocks() {
        let mut bad_sum = block(7, None);
        bad_sum[20] ^= 1;
        assert_eq!(parse(&bad_sum), None);
        assert_eq!(parse(&block(7, None)[..127]), None);
        let mut bad_header = block(7, None);
        bad_header[0] = 1;
        bad_header[127] = bad_header[127].wrapping_sub(1);
        assert_eq!(parse(&bad_header), None);
        assert_eq!(parse(&[]), None);
    }

    fn target(path: &str, serial: Option<&str>) -> Target {
        Target {
            path: path.into(),
            edid: Some(edid(serial)),
        }
    }

    #[test]
    fn unique_serial_gives_edid_identity() {
        let ids = target_ids(&[target(r"\\?\DISPLAY#DEL4123#5&1&UID1", Some("ABC"))]);
        assert_eq!(ids, ["edid:DEL4123:ABC"]);
    }

    #[test]
    fn shared_serials_fall_back_to_paths() {
        let ids = target_ids(&[
            target(r"\\?\DISPLAY#DEL4123#A", Some("SAME")),
            target(r"\\?\DISPLAY#DEL4123#B", Some("SAME")),
            target(r"\\?\DISPLAY#DEL4123#C", Some("OTHER")),
        ]);
        assert_eq!(
            ids,
            [
                r"path:\\?\display#del4123#a",
                r"path:\\?\display#del4123#b",
                "edid:DEL4123:OTHER",
            ]
        );
    }

    #[test]
    fn missing_serial_or_edid_falls_back_to_the_path() {
        let ids = target_ids(&[
            target(r"\\?\DISPLAY#DEL4123#A", None),
            Target {
                path: r"\\?\DISPLAY#BOE0A1B#Internal".into(),
                edid: None,
            },
        ]);
        assert_eq!(
            ids,
            [
                r"path:\\?\display#del4123#a",
                r"path:\\?\display#boe0a1b#internal"
            ]
        );
    }

    #[test]
    fn mirrored_targets_join_sorted() {
        let id = monitor_id(vec!["path:b".into(), "edid:DEL4123:A".into()]);
        assert_eq!(id, MonitorId("edid:DEL4123:A+path:b".into()));
        assert_eq!(
            monitor_id(vec!["edid:X".into()]),
            MonitorId("edid:X".into())
        );
    }
}
