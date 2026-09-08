//! ed25519 signatures for the registry index: detached, over canonical
//! bytes, spelled `ed25519:<hex>`.
//!
//! [`Signing`] is the half a root or a publisher holds; [`Public`] is the
//! half an index spells and a consumer pins.

use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// The algorithm name every key and signature spelling carries.
pub const ALGORITHM: &str = "ed25519";

/// Hexadecimal widths, derived from the scheme's byte lengths.
pub const KEY_HEX_LEN: usize = ed25519_dalek::PUBLIC_KEY_LENGTH * 2;
pub const SIGNATURE_HEX_LEN: usize = ed25519_dalek::SIGNATURE_LENGTH * 2;

/// A key that signs: the private half a root or a publisher holds.
#[derive(Clone)]
pub struct Signing(SigningKey);

impl Signing {
    #[must_use]
    pub fn from_seed(seed: [u8; ed25519_dalek::SECRET_KEY_LENGTH]) -> Self {
        Self(SigningKey::from_bytes(&seed))
    }

    #[must_use]
    pub fn public(&self) -> Public {
        Public(self.0.verifying_key())
    }

    #[must_use]
    pub fn sign(&self, contents: &[u8]) -> Detached {
        Detached(self.0.sign(contents))
    }
}

impl std::fmt::Debug for Signing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Signing")
            .field("public", &self.public().spelling())
            .finish_non_exhaustive()
    }
}

/// A key that verifies: the public half of a [`Signing`] key, as an index
/// file spells it and a consumer's configuration pins it.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Public(VerifyingKey);

impl Ord for Public {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl PartialOrd for Public {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Public {
    /// # Errors
    /// [`SpellingError`] when the spelling is not `ed25519:` plus the
    /// right length of hex naming a key.
    pub fn parse(spelling: &str) -> Result<Self, SpellingError> {
        let hex = key_material(spelling, KEY_HEX_LEN)?;
        let mut bytes = [0; ed25519_dalek::PUBLIC_KEY_LENGTH];
        if !pith_ids::decode_hex_into(&mut bytes, hex) {
            return Err(SpellingError::Shape {
                spelling: spelling.into(),
            });
        }
        VerifyingKey::from_bytes(&bytes)
            .map(Self)
            .map_err(|_| SpellingError::Shape {
                spelling: spelling.into(),
            })
    }

    #[must_use]
    pub fn spelling(&self) -> Box<str> {
        let mut spelling = format!("{ALGORITHM}:");
        for byte in self.0.as_bytes() {
            spelling.push_str(&format!("{byte:02x}"));
        }
        spelling.into()
    }

    #[must_use]
    pub fn verify(&self, contents: &[u8], signature: &Detached) -> bool {
        self.0.verify(contents, &signature.0).is_ok()
    }

    /// The pinned key. An unknown algorithm is a refusal, not a guess.
    ///
    /// # Errors
    /// [`SpellingError`] when the pinned key is not an ed25519 spelling.
    pub fn of_pinned(pinned: &pith_hir::RootKey) -> Result<Self, SpellingError> {
        Self::parse(&pinned.to_string())
    }
}

impl std::fmt::Debug for Public {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Public({})", self.spelling())
    }
}

/// One detached signature over canonical bytes.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Detached(Ed25519Signature);

impl Detached {
    #[must_use]
    pub fn spelling(&self) -> Box<str> {
        format!("{ALGORITHM}:{}", pith_ids::encode_hex(&self.0.to_bytes())).into()
    }

    /// # Errors
    /// [`SpellingError`] when the spelling is not `ed25519:` plus the
    /// right length of hex.
    pub fn parse(spelling: &str) -> Result<Self, SpellingError> {
        let hex = key_material(spelling, SIGNATURE_HEX_LEN)?;
        let mut bytes = [0; ed25519_dalek::SIGNATURE_LENGTH];
        if !pith_ids::decode_hex_into(&mut bytes, hex) {
            return Err(SpellingError::Shape {
                spelling: spelling.into(),
            });
        }
        Ok(Self(Ed25519Signature::from_bytes(&bytes)))
    }
}

impl std::fmt::Debug for Detached {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Detached({})", self.spelling())
    }
}

/// A spelling that names neither a key nor a signature: it is not
/// `algorithm:` followed by the right length of hexadecimal for the thing
/// being read.
#[derive(Debug, PartialEq, Eq)]
pub enum SpellingError {
    Shape { spelling: Box<str> },
}

impl std::fmt::Display for SpellingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape { spelling } => write!(
                formatter,
                "`{spelling}` is not `{ALGORITHM}:` followed by hexadecimal of the right length"
            ),
        }
    }
}

impl std::error::Error for SpellingError {}

fn key_material(spelling: &str, hex_len: usize) -> Result<&str, SpellingError> {
    spelling
        .strip_prefix(ALGORITHM)
        .and_then(|rest| rest.strip_prefix(':'))
        .filter(|material| material.len() == hex_len)
        .ok_or(SpellingError::Shape {
            spelling: spelling.into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; ed25519_dalek::SECRET_KEY_LENGTH] = [7; ed25519_dalek::SECRET_KEY_LENGTH];

    #[test]
    fn a_signature_verifies_under_its_own_public_key_only() {
        let signer = Signing::from_seed(SEED);
        let other = Signing::from_seed([9; ed25519_dalek::SECRET_KEY_LENGTH]);
        let signature = signer.sign(b"the signed contents");
        assert!(signer.public().verify(b"the signed contents", &signature));
        assert!(!other.public().verify(b"the signed contents", &signature));
        assert!(!signer.public().verify(b"other contents", &signature));
    }

    #[test]
    fn spellings_round_trip_and_reject_the_wrong_shape() {
        let signer = Signing::from_seed(SEED);
        let spelling = signer.public().spelling();
        assert_eq!(Public::parse(&spelling), Ok(signer.public()));
        let signature = signer.sign(b"contents");
        assert_eq!(
            Detached::parse(&signature.spelling()).map(|parsed| parsed.spelling()),
            Ok(signature.spelling())
        );
        let truncated = format!("{}{}", &spelling[..spelling.len() - 4], "zz");
        assert_eq!(
            Public::parse(&truncated),
            Err(SpellingError::Shape {
                spelling: truncated.into()
            })
        );
        let non_hex = format!("{ALGORITHM}:{}", "0g".repeat(KEY_HEX_LEN / 2));
        assert_eq!(
            Public::parse(&non_hex),
            Err(SpellingError::Shape {
                spelling: non_hex.into()
            })
        );
    }

    #[test]
    fn a_pinned_key_of_another_algorithm_is_refused_rather_than_interpreted() {
        let pinned = pith_hir::RootKey::parse("ml-dsa:material").expect("the spelling parses");
        assert_eq!(
            Public::of_pinned(&pinned),
            Err(SpellingError::Shape {
                spelling: "ml-dsa:material".into()
            })
        );
    }
}
