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
        CaseId(sha256(&encoded))
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

fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut hash = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            schedule[index] = u32::from_be_bytes(word.try_into().expect("four-byte word"));
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = hash;
        for index in 0..64 {
            let choice = (e & f) ^ (!e & g);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let first = h
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(ROUND[index])
                .wrapping_add(schedule[index]);
            let second = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }

        for (state, value) in hash.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(value);
        }
    }

    let mut digest = [0; 32];
    for (bytes, word) in digest.chunks_exact_mut(4).zip(hash) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    digest
}
