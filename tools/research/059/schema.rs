//! Coordinate naming/encoding only. Candidate and oracle build their lists independently.
use crate::budget::{MAX_BITS, R, Work};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryStatus {
    Complete,
    UnsupportedMultiplicity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    pub fields: BTreeMap<String, (usize, usize)>,
    pub bits: usize,
}
impl Schema {
    pub fn from_fields(fields: BTreeMap<String, usize>, work: &mut Work) -> R<Self> {
        work.charge(fields.len())?;
        let mut at = 0usize;
        let mut result = BTreeMap::new();
        for (key, width) in fields {
            work.charge(key.len())?;
            let base = at;
            at = at
                .checked_add(width)
                .ok_or("ResourceLimit: schema overflow")?;
            if at > MAX_BITS {
                return Err("ResourceLimit: schema bits".into());
            }
            result.insert(key, (base, width));
        }
        Ok(Self {
            fields: result,
            bits: at,
        })
    }
    pub fn field(&self, key: &str) -> R<(usize, usize)> {
        self.fields
            .get(key)
            .copied()
            .ok_or_else(|| format!("Unsupported: absent coordinate {key}"))
    }
    pub fn get(&self, row: &[u64], key: &str) -> R<u32> {
        let (at, w) = self.field(key)?;
        let mut v = 0;
        for i in 0..w {
            v |= (((row[(at + i) / 64] >> ((at + i) % 64)) & 1) as u32) << i;
        }
        Ok(v)
    }
    pub fn set(&self, row: &mut [u64], key: &str, value: u32) -> R<()> {
        let (at, w) = self.field(key)?;
        if value >> w != 0 {
            return Err("InternalError: coordinate range".into());
        }
        for i in 0..w {
            let mask = 1u64 << ((at + i) % 64);
            if value >> i & 1 == 0 {
                row[(at + i) / 64] &= !mask;
            } else {
                row[(at + i) / 64] |= mask;
            }
        }
        Ok(())
    }
}
pub type Rows = std::collections::BTreeSet<Vec<u64>>;
pub type Relations = BTreeMap<String, Rows>;
