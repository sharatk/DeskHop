//! Signed membership records, owned.

use model::PeerId;
use proto::pairing::{self as wire, MAX_NAME_LEN, MemberRecord, Name};

use crate::identity::{Identity, verify};

/// A machine name: 1 to 63 bytes of UTF-8. The service cuts longer names at a
/// character boundary before they get here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MachineName(String);

impl MachineName {
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        (1..=MAX_NAME_LEN)
            .contains(&name.len())
            .then_some(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn wire(&self) -> Name<'_> {
        // The constructor holds the same bounds as `Name`.
        Name::new(&self.0).unwrap_or_else(|| unreachable!("machine name out of range"))
    }
}

impl From<Name<'_>> for MachineName {
    fn from(name: Name<'_>) -> Self {
        Self(name.as_str().to_owned())
    }
}

/// What a record does to its subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Add(MachineName),
    Remove,
}

/// One signed membership record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRecord {
    pub subject: PeerId,
    pub epoch: u64,
    pub signer: PeerId,
    pub change: Change,
    pub signature: [u8; 64],
}

impl SignedRecord {
    /// A record signed by `identity`.
    pub(crate) fn sign(identity: &Identity, subject: PeerId, epoch: u64, change: Change) -> Self {
        let mut record = Self {
            subject,
            epoch,
            signer: identity.peer_id(),
            change,
            signature: [0; 64],
        };
        record.signature = identity.sign(&record.wire().signed_bytes());
        record
    }

    pub fn from_wire(record: &MemberRecord<'_>) -> Self {
        Self {
            subject: PeerId(record.subject),
            epoch: record.epoch,
            signer: PeerId(record.signer),
            change: match record.change {
                wire::Change::Add(name) => Change::Add(name.into()),
                wire::Change::Remove => Change::Remove,
            },
            signature: record.signature,
        }
    }

    pub fn wire(&self) -> MemberRecord<'_> {
        MemberRecord {
            subject: self.subject.0,
            epoch: self.epoch,
            signer: self.signer.0,
            change: match &self.change {
                Change::Add(name) => wire::Change::Add(name.wire()),
                Change::Remove => wire::Change::Remove,
            },
            signature: self.signature,
        }
    }

    pub fn is_removal(&self) -> bool {
        self.change == Change::Remove
    }

    /// The signature verifies, and an addition is not signed by its subject.
    pub fn is_valid(&self) -> bool {
        let self_added = !self.is_removal() && self.signer == self.subject;
        !self_added && verify(&self.signer, &self.wire().signed_bytes(), &self.signature)
    }

    /// Orders records for the same subject: higher epoch wins; on equal
    /// epochs, removal wins.
    pub(crate) fn rank(&self) -> (u64, bool) {
        (self.epoch, self.is_removal())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng;

    fn name(s: &str) -> MachineName {
        MachineName::new(s).unwrap()
    }

    #[test]
    fn valid_signature() {
        let a = Identity::generate(&mut test_rng(1));
        let c = Identity::generate(&mut test_rng(2)).peer_id();
        let record = SignedRecord::sign(&a, c, 1, Change::Add(name("C")));
        assert!(record.is_valid());
        let decoded =
            SignedRecord::from_wire(&MemberRecord::decode(&record.wire().payload()).unwrap());
        assert_eq!(decoded, record);
        assert!(decoded.is_valid());
    }

    #[test]
    fn tampered_record() {
        let a = Identity::generate(&mut test_rng(1));
        let c = Identity::generate(&mut test_rng(2)).peer_id();
        for change in [Change::Add(name("C")), Change::Remove] {
            let payload = SignedRecord::sign(&a, c, 1, change).wire().payload();
            for i in 0..payload.len() {
                for bit in [0x01, 0x80] {
                    let mut bytes = payload.clone();
                    bytes[i] ^= bit;
                    // Changes that break the layout never become records at all.
                    if let Ok(wire) = MemberRecord::decode(&bytes) {
                        let record = SignedRecord::from_wire(&wire);
                        assert!(!record.is_valid(), "byte {i} bit {bit:#x} accepted");
                    }
                }
            }
        }
    }

    #[test]
    fn self_signed_addition() {
        let c = Identity::generate(&mut test_rng(3));
        let record = SignedRecord::sign(&c, c.peer_id(), 1, Change::Add(name("C")));
        assert!(!record.is_valid());
        let removal = SignedRecord::sign(&c, c.peer_id(), 1, Change::Remove);
        assert!(removal.is_valid());
    }

    #[test]
    fn name_bounds() {
        assert_eq!(MachineName::new(""), None);
        assert!(MachineName::new("x".repeat(63)).is_some());
        assert_eq!(MachineName::new("x".repeat(64)), None);
    }
}
