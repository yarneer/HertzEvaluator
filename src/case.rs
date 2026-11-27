use sha2::{Digest, Sha256};
use std::fmt;

const CASE_ID_PREFIX: &[u8] = b"hertz-evaluator.case.v1\0";

/// An immutable reviewable case payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    wording: String,
    required_facts: Vec<String>,
}

impl Case {
    /// Creates a case from its wording and ordered required-fact checks.
    pub fn new(
        wording: impl Into<String>,
        required_facts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            wording: wording.into(),
            required_facts: required_facts.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the case wording exactly as registered.
    pub fn wording(&self) -> &str {
        &self.wording
    }

    /// Returns the ordered required-fact checks exactly as registered.
    pub fn required_facts(&self) -> &[String] {
        &self.required_facts
    }

    pub(crate) fn id(&self) -> CaseId {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(CASE_ID_PREFIX);
        encode_string(&mut encoded, &self.wording);
        encoded.extend_from_slice(&(self.required_facts.len() as u64).to_be_bytes());
        for fact in &self.required_facts {
            encode_string(&mut encoded, fact);
        }
        CaseId(Sha256::digest(encoded).into())
    }
}

fn encode_string(encoded: &mut Vec<u8>, value: &str) {
    encoded.extend_from_slice(&(value.len() as u64).to_be_bytes());
    encoded.extend_from_slice(value.as_bytes());
}

/// A version-1, SHA-256 content address for a [`Case`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CaseId([u8; 32]);

impl CaseId {
    /// Creates an id from a SHA-256 digest.
    ///
    /// This is useful when loading an id received from an external system.
    /// It does not imply that a case with this id is registered.
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the underlying SHA-256 digest.
    pub const fn digest(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for CaseId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("case-v1-sha256:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
