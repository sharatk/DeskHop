//! Pairing codes: `<locator>-<secret><check>`, all digits.
//!
//! The locator is the host part of the inviter's IPv4 address, so the joiner
//! can find it with or without multicast. The secret is the SPAKE2 password's
//! entropy. The check digit (Damm) catches typos before an attempt is spent.

use std::fmt;
use std::net::Ipv4Addr;

use rand_core::{CryptoRng, RngCore};

/// Digits in the secret.
pub const SECRET_DIGITS: usize = 6;

const SECRET_RANGE: u32 = 1_000_000;

/// Largest multiple of [`SECRET_RANGE`] that fits in a `u32`; draws at or
/// above it are rejected so the secret is uniform.
const SECRET_LIMIT: u32 = (u32::MAX / SECRET_RANGE) * SECRET_RANGE;

/// Damm's totally anti-symmetric quasigroup of order 10.
const DAMM: [[u8; 10]; 10] = [
    [0, 3, 1, 7, 5, 9, 8, 6, 4, 2],
    [7, 0, 9, 2, 1, 5, 4, 8, 6, 3],
    [4, 2, 0, 6, 8, 7, 1, 3, 5, 9],
    [1, 7, 5, 0, 9, 8, 3, 4, 2, 6],
    [6, 1, 2, 3, 0, 4, 5, 9, 7, 8],
    [3, 6, 7, 4, 2, 0, 9, 5, 8, 1],
    [5, 8, 6, 9, 7, 2, 0, 1, 3, 4],
    [8, 9, 4, 5, 3, 6, 2, 0, 1, 7],
    [9, 4, 3, 8, 6, 1, 7, 2, 0, 5],
    [2, 5, 8, 1, 4, 3, 6, 7, 9, 0],
];

/// The Damm check digit of `digits` (each 0–9).
fn damm(digits: impl IntoIterator<Item = u8>) -> u8 {
    digits
        .into_iter()
        .fold(0, |interim, d| DAMM[usize::from(interim)][usize::from(d)])
}

/// Why typed text is not a pairing code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeError {
    /// No `-` separating the locator from the digits.
    MissingLocator,
    /// A locator byte that is empty, not a number, or above 255.
    LocatorByte,
    /// More than 4 locator bytes.
    LocatorLength,
    /// Not exactly 7 digits after the locator.
    DigitCount,
    /// A character other than digits, `.`, `-`, and whitespace.
    InvalidCharacter,
    /// The check digit does not match: a typo.
    CheckDigit,
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingLocator => "code has no locator",
            Self::LocatorByte => "code has an invalid locator",
            Self::LocatorLength => "code locator is too long",
            Self::DigitCount => "code needs 7 digits after the locator",
            Self::InvalidCharacter => "code has an invalid character",
            Self::CheckDigit => "code has a typo",
        })
    }
}

impl std::error::Error for CodeError {}

/// A pairing code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// The trailing bytes of the inviter's IPv4 address, 1 to 4 of them.
    locator: Vec<u8>,
    /// 0 to 999,999.
    secret: u32,
}

impl Code {
    /// A new code for an inviter at `addr` on a network with `prefix_len`
    /// bits of prefix.
    pub fn generate<R: CryptoRng + RngCore>(addr: Ipv4Addr, prefix_len: u8, rng: &mut R) -> Self {
        let whole_prefix_bytes = usize::from(prefix_len.min(32) / 8).min(3);
        let locator = addr.octets()[whole_prefix_bytes..].to_vec();
        Self {
            locator,
            secret: random_secret(rng),
        }
    }

    /// The same locator with a new secret.
    pub fn with_new_secret<R: CryptoRng + RngCore>(&self, rng: &mut R) -> Self {
        Self {
            locator: self.locator.clone(),
            secret: random_secret(rng),
        }
    }

    /// Parses typed text. Whitespace anywhere and hyphens after the first are
    /// ignored.
    pub fn parse(text: &str) -> Result<Self, CodeError> {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if !compact.chars().all(|c| matches!(c, '0'..='9' | '.' | '-')) {
            return Err(CodeError::InvalidCharacter);
        }
        let (locator_text, rest) = compact.split_once('-').ok_or(CodeError::MissingLocator)?;
        let locator = locator_text
            .split('.')
            .map(|part| {
                if part.is_empty() || part.len() > 3 {
                    return Err(CodeError::LocatorByte);
                }
                part.parse::<u8>().map_err(|_| CodeError::LocatorByte)
            })
            .collect::<Result<Vec<u8>, _>>()?;
        if locator.len() > 4 {
            return Err(CodeError::LocatorLength);
        }
        let digits: Vec<u8> = rest
            .chars()
            .filter(|&c| c != '-')
            .map(|c| {
                c.to_digit(10)
                    .map(|d| d as u8)
                    .ok_or(CodeError::InvalidCharacter)
            })
            .collect::<Result<_, _>>()?;
        let [secret_digits @ .., check] = digits.as_slice() else {
            return Err(CodeError::DigitCount);
        };
        if secret_digits.len() != SECRET_DIGITS {
            return Err(CodeError::DigitCount);
        }
        let secret = secret_digits.iter().fold(0, |n, &d| n * 10 + u32::from(d));
        let code = Self { locator, secret };
        if code.check_digit() != *check {
            return Err(CodeError::CheckDigit);
        }
        Ok(code)
    }

    /// The locator as shown: bytes in decimal, separated by `.`.
    pub fn locator_text(&self) -> String {
        self.locator
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }

    /// The secret's 6 digits, zero-padded.
    fn secret_text(&self) -> String {
        format!("{:06}", self.secret)
    }

    /// The Damm check digit over the locator's digits, then the secret's.
    pub fn check_digit(&self) -> u8 {
        let text = format!("{}{}", self.locator_text(), self.secret_text());
        damm(text.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0'))
    }

    /// The inviter's address, from the joiner's own: the joiner's leading
    /// bytes, then the locator.
    pub fn inviter_addr(&self, own: Ipv4Addr) -> Ipv4Addr {
        let mut octets = own.octets();
        let start = 4 - self.locator.len();
        octets[start..].copy_from_slice(&self.locator);
        Ipv4Addr::from(octets)
    }

    /// The SPAKE2 password: the canonical text's bytes.
    pub(crate) fn password(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}

/// Canonical text: `<locator>-<secret><check>`, for example `137-4829153`.
impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}{}",
            self.locator_text(),
            self.secret_text(),
            self.check_digit()
        )
    }
}

fn random_secret<R: CryptoRng + RngCore>(rng: &mut R) -> u32 {
    loop {
        let draw = rng.next_u32();
        if draw < SECRET_LIMIT {
            return draw % SECRET_RANGE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng;

    /// Returns the queued `u32`s in order.
    struct Queue(Vec<u32>);

    impl RngCore for Queue {
        fn next_u32(&mut self) -> u32 {
            self.0.remove(0)
        }
        fn next_u64(&mut self) -> u64 {
            u64::from(self.next_u32())
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for b in dest {
                *b = self.next_u32() as u8;
            }
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    impl CryptoRng for Queue {}

    fn code(locator: &[u8], secret: u32) -> Code {
        Code {
            locator: locator.to_vec(),
            secret,
        }
    }

    #[test]
    fn damm_published_example() {
        assert_eq!(damm([5, 7, 2]), 4);
        assert_eq!(damm([5, 7, 2, 4]), 0);
    }

    #[test]
    fn code_on_a_24_network() {
        let c = Code::generate(
            Ipv4Addr::new(192, 168, 1, 137),
            24,
            &mut Queue(vec![482_915]),
        );
        assert_eq!(c, code(&[137], 482_915));
        let check = damm([1, 3, 7, 4, 8, 2, 9, 1, 5]);
        assert_eq!(c.to_string(), format!("137-482915{check}"));
    }

    #[test]
    fn code_on_a_16_network() {
        let c = Code::generate(Ipv4Addr::new(10, 20, 1, 137), 16, &mut test_rng(1));
        assert_eq!(c.locator_text(), "1.137");
    }

    #[test]
    fn other_prefixes() {
        let addr = Ipv4Addr::new(10, 20, 30, 40);
        let locator = |prefix| Code::generate(addr, prefix, &mut test_rng(1)).locator_text();
        assert_eq!(locator(8), "20.30.40");
        assert_eq!(locator(23), "30.40");
        assert_eq!(locator(28), "40");
        assert_eq!(locator(4), "10.20.30.40");
    }

    #[test]
    fn rejected_draws_keep_the_secret_uniform() {
        let c = Code::generate(
            Ipv4Addr::new(192, 168, 1, 2),
            24,
            &mut Queue(vec![u32::MAX, SECRET_LIMIT, SECRET_LIMIT - 1]),
        );
        assert_eq!(c.secret, (SECRET_LIMIT - 1) % SECRET_RANGE);
        assert_eq!(c.secret, 999_999);
        assert_eq!(code(&[1], 7).to_string()[2..8], *"000007");
    }

    #[test]
    fn canonical_text_parses_back() {
        for c in [
            code(&[137], 482_915),
            code(&[1, 137], 0),
            code(&[10, 20, 30, 40], 999_999),
        ] {
            assert_eq!(Code::parse(&c.to_string()), Ok(c));
        }
    }

    #[test]
    fn typed_with_spaces_and_extra_hyphens() {
        let c = code(&[137], 482_915);
        let check = c.check_digit();
        assert_eq!(
            Code::parse(&format!("137 - 482 915-{check}")),
            Ok(c.clone())
        );
        assert_eq!(Code::parse(&format!(" 137-48-29-15{check} ")), Ok(c));
    }

    #[test]
    fn one_mistyped_digit() {
        let text = code(&[1, 137], 482_915).to_string();
        let positions: Vec<usize> = text
            .char_indices()
            .filter(|(_, ch)| ch.is_ascii_digit())
            .map(|(i, _)| i)
            .collect();
        for &i in &positions {
            for d in b'0'..=b'9' {
                let mut typo = text.clone().into_bytes();
                if typo[i] == d {
                    continue;
                }
                typo[i] = d;
                let typo = String::from_utf8(typo).unwrap();
                assert!(Code::parse(&typo).is_err(), "{typo} accepted");
            }
        }
        for pair in positions.windows(2) {
            let (i, j) = (pair[0], pair[1]);
            if j != i + 1 {
                continue;
            }
            let mut typo = text.clone().into_bytes();
            if typo[i] == typo[j] {
                continue;
            }
            typo.swap(i, j);
            let typo = String::from_utf8(typo).unwrap();
            assert!(Code::parse(&typo).is_err(), "{typo} accepted");
        }
    }

    #[test]
    fn locator_out_of_range() {
        assert_eq!(Code::parse("300-4829150"), Err(CodeError::LocatorByte));
    }

    #[test]
    fn malformed_codes() {
        let digits = format!("482915{}", code(&[137], 482_915).check_digit());
        assert_eq!(Code::parse(&digits), Err(CodeError::MissingLocator));
        assert_eq!(
            Code::parse(&format!("-{digits}")),
            Err(CodeError::LocatorByte)
        );
        assert_eq!(
            Code::parse(&format!("1..2-{digits}")),
            Err(CodeError::LocatorByte)
        );
        assert_eq!(
            Code::parse(&format!("1.2.3.4.5-{digits}")),
            Err(CodeError::LocatorLength)
        );
        assert_eq!(Code::parse("137-482915"), Err(CodeError::DigitCount));
        assert_eq!(Code::parse("137-48291500"), Err(CodeError::DigitCount));
        assert_eq!(Code::parse("137-"), Err(CodeError::DigitCount));
        assert_eq!(Code::parse("137-48291a0"), Err(CodeError::InvalidCharacter));
    }

    #[test]
    fn same_24_subnet() {
        let c = code(&[137], 482_915);
        assert_eq!(
            c.inviter_addr(Ipv4Addr::new(192, 168, 1, 52)),
            Ipv4Addr::new(192, 168, 1, 137)
        );
    }

    #[test]
    fn two_byte_locator() {
        let c = code(&[1, 137], 482_915);
        assert_eq!(
            c.inviter_addr(Ipv4Addr::new(10, 20, 7, 9)),
            Ipv4Addr::new(10, 20, 1, 137)
        );
    }
}
