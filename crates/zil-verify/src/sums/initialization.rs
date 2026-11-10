//! Exact four-state initialization: bytes for tiny inventories, packed otherwise.
use super::*;
const LANES: usize = usize::BITS as usize / 2;
const SMALL: usize = std::mem::size_of::<usize>();
#[derive(Clone, Debug, PartialEq, Eq)]
enum Storage {
    Small(Vec<u8>),
    Packed { len: usize, words: Vec<usize> },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Initialized {
    storage: Storage,
}
fn tail_mask(len: usize) -> usize {
    let lanes = len % LANES;
    if lanes == 0 {
        usize::MAX
    } else {
        (1usize << (lanes * 2)) - 1
    }
}
impl Initialized {
    pub(super) fn new(len: usize, value: u8, budget: &mut Budget) -> Check<Self> {
        budget.spend(1)?;
        if value > 3 {
            return Err(bad(None, "Sum initialization state exceeds two bits"));
        }
        // Up to one machine word of bytes, packing does not reduce payload
        // storage and adds shifts/masks to every access. Keep the actual bytes.
        if len <= SMALL {
            budget.spend(len.saturating_add(2))?;
            return Ok(Self {
                storage: Storage::Small(vec![value; len]),
            });
        }
        let count = len.div_ceil(LANES);
        budget.spend(count.saturating_add(2))?;
        let pattern = (usize::MAX / 3) * usize::from(value);
        let mut words = vec![pattern; count];
        if let Some(last) = words.last_mut() {
            budget.spend(3)?; // actual tail lookup, mask and write
            *last &= tail_mask(len);
        }
        Ok(Self {
            storage: Storage::Packed { len, words },
        })
    }
    fn len(&self) -> usize {
        match &self.storage {
            Storage::Small(bytes) => bytes.len(),
            Storage::Packed { len, .. } => *len,
        }
    }
    pub(super) fn copy_work(&self) -> usize {
        match &self.storage {
            Storage::Small(bytes) => bytes.len().saturating_add(2),
            Storage::Packed { words, .. } => words.len().saturating_add(2),
        }
    }
    fn validate(&self, budget: &mut Budget) -> Check<()> {
        if let Storage::Packed { len, words } = &self.storage {
            budget.spend(2)?;
            if words.len() != len.div_ceil(LANES) {
                return Err(bad(None, "Sum initialization word inventory differs"));
            }
            if let Some(last) = words.last() {
                budget.spend(3)?;
                if *last & !tail_mask(*len) != 0 {
                    return Err(bad(None, "Sum initialization tail has hidden states"));
                }
            }
        }
        // Small's Vec is itself the complete logical inventory. Only new,
        // from_flat and set can write its private bytes, all checking 0..3.
        Ok(())
    }
    pub(super) fn get(&self, index: usize, budget: &mut Budget) -> Check<u8> {
        match &self.storage {
            Storage::Small(bytes) => {
                budget.spend(2)?; // actual checked byte index and read, no masks
                bytes
                    .get(index)
                    .copied()
                    .ok_or_else(|| bad(None, "Sum initialization local identity absent"))
            }
            Storage::Packed { len, words } => {
                self.validate(budget)?;
                budget.spend(4)?;
                if index >= *len {
                    return Err(bad(None, "Sum initialization local identity absent"));
                }
                Ok(((words[index / LANES] >> ((index % LANES) * 2)) & 3) as u8)
            }
        }
    }
    pub(super) fn set(&mut self, index: usize, value: u8, budget: &mut Budget) -> Check<()> {
        if let Storage::Small(bytes) = &mut self.storage {
            budget.spend(3)?; // checked byte index, value bound and direct write
            let slot = bytes
                .get_mut(index)
                .ok_or_else(|| bad(None, "Sum initialization local identity absent"))?;
            if value > 3 {
                return Err(bad(None, "Sum initialization state exceeds two bits"));
            }
            *slot = value;
            return Ok(());
        }
        self.validate(budget)?;
        budget.spend(6)?;
        let Storage::Packed { len, words } = &mut self.storage else {
            unreachable!()
        };
        if index >= *len {
            return Err(bad(None, "Sum initialization local identity absent"));
        }
        if value > 3 {
            return Err(bad(None, "Sum initialization state exceeds two bits"));
        }
        let shift = (index % LANES) * 2;
        let word = &mut words[index / LANES];
        *word = (*word & !(3usize << shift)) | (usize::from(value) << shift);
        Ok(())
    }
    pub(super) fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        self.validate(budget)?;
        other.validate(budget)?;
        let common = self.len().min(other.len());
        let mut changed = false;
        if let (Storage::Packed { words: left, .. }, Storage::Packed { words: right, .. }) =
            (&mut self.storage, &other.storage)
        {
            let count = common.div_ceil(LANES);
            for index in 0..count {
                budget.spend(5)?;
                let mask = if index + 1 == count {
                    tail_mask(common)
                } else {
                    usize::MAX
                };
                let old = left[index];
                left[index] |= right[index] & mask;
                changed |= old != left[index];
            }
        } else {
            // The overlap has at most SMALL bytes when either side is small.
            // Execute the old scalar zip, including self's untouched suffix.
            for index in 0..common {
                let old = self.get(index, budget)?;
                let incoming = other.get(index, budget)?;
                budget.spend(1)?; // actual OR/change test
                let value = old | incoming;
                changed |= old != value;
                self.set(index, value, budget)?;
            }
        }
        Ok(changed)
    }
    #[cfg(test)]
    pub(super) fn from_flat(values: &[u8], budget: &mut Budget) -> Check<Self> {
        let mut result = Self::new(values.len(), 0, budget)?;
        budget.spend(values.len())?;
        for (index, value) in values.iter().copied().enumerate() {
            result.set(index, value, budget)?;
        }
        Ok(result)
    }
    #[cfg(test)]
    pub(super) fn flat(&self) -> Vec<u8> {
        match &self.storage {
            Storage::Small(bytes) => bytes.clone(),
            Storage::Packed { len, words } => (0..*len)
                .map(|i| ((words[i / LANES] >> ((i % LANES) * 2)) & 3) as u8)
                .collect(),
        }
    }
}
#[cfg(test)]
mod tests;
