//! Lower bounds for the existing 64-bit Store POD layout. A bounded symbolic
//! leaf promises a positive size. Its exact stride is checked on instantiation.
use crate::Type;

#[derive(Clone, Copy)]
pub struct PodExtent {
    bytes: u64,
    alignment: u64,
}

impl PodExtent {
    pub const EMPTY: Self = Self {
        bytes: 0,
        alignment: 1,
    };
    pub const POSITIVE: Self = Self {
        bytes: 1,
        alignment: 1,
    };

    pub fn scalar(ty: &Type) -> Option<Self> {
        let bytes = match ty {
            Type::Unit => return Some(Self::EMPTY),
            Type::Bool | Type::U8 | Type::I8 => 1,
            Type::U16 | Type::I16 => 2,
            Type::I32 | Type::U32 | Type::F32 => 4,
            Type::I64 | Type::U64 | Type::Usize | Type::Isize | Type::F64 => 8,
            _ => return None,
        };
        Some(Self {
            bytes,
            alignment: bytes,
        })
    }

    fn aligned(bytes: u64, alignment: u64) -> Option<u64> {
        bytes
            .checked_next_multiple_of(alignment)
            .filter(|n| *n <= i64::MAX as u64)
    }

    pub fn append(self, field: Self) -> Option<Self> {
        let bytes = Self::aligned(self.bytes, field.alignment)?.checked_add(field.bytes)?;
        (bytes <= i64::MAX as u64).then_some(Self {
            bytes,
            alignment: self.alignment.max(field.alignment),
        })
    }

    pub fn finish(self) -> Option<Self> {
        Some(Self {
            bytes: Self::aligned(self.bytes, self.alignment)?,
            ..self
        })
    }

    pub fn repeat(self, count: u64) -> Option<Self> {
        let count = u32::try_from(count).ok()?;
        let bytes = self.bytes.checked_mul(u64::from(count))?;
        (bytes <= i64::MAX as u64).then_some(Self { bytes, ..self })
    }

    pub fn positive(self) -> bool {
        self.bytes != 0
    }
}
