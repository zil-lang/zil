//! Private provenance lattice for this layer. Distinct roles join to mixed;
//! classification flags and borrowed participation survive that join. No
//! concrete type identity supplies a role or separates actual instances.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Origin {
    // 0 = no supplied role; usize::MAX = multiple distinct supplied roles.
    role: usize,
    flags: u8,
}
impl Origin {
    pub(super) const EMPTY: Self = Self { role: 0, flags: 0 };
    pub(super) const FACTORY: Self = Self { role: 0, flags: 4 };
    pub(super) const UNKNOWN: Self = Self { role: 0, flags: 8 };
    pub(super) const BODY: Self = Self { role: 0, flags: 16 };
    pub(super) const CONCRETE: Self = Self { role: 0, flags: 32 };
    pub(super) fn supplied(index: usize, borrowed: bool) -> Self {
        Self {
            role: index + 1,
            flags: if borrowed { 64 } else { 0 },
        }
    }
    pub(super) fn index(self) -> Option<usize> {
        (self.role != 0 && self.role != usize::MAX && self.flags & !64 == 0).then(|| self.role - 1)
    }
    pub(super) fn concrete(self) -> bool {
        self == Self::FACTORY || self == Self::BODY || self == Self::CONCRETE
    }
    pub(super) fn join(self, other: Self) -> Self {
        let role = if self.role == 0 {
            other.role
        } else if other.role == 0 || self.role == other.role {
            self.role
        } else {
            usize::MAX
        };
        Self {
            role,
            flags: self.flags | other.flags,
        }
    }
    /// Physical table storage replaces the old u8, paid before allocation.
    pub(super) fn table_work(count: usize) -> usize {
        count.saturating_mul(std::mem::size_of::<Self>())
    }
}
impl std::ops::BitOr for Origin {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        self.join(other)
    }
}
impl std::ops::BitOrAssign for Origin {
    fn bitor_assign(&mut self, other: Self) {
        *self = self.join(other);
    }
}
