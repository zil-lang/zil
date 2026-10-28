//! Private verified storage projections. Public candidates still use HoldRoot.
//! A field root never transports a loan across replacement or whole movement.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Root {
    Local(LocalId),
    Field(LocalId, u32),
    Capture(u32),
    Parameter(u32),
    ParameterCapture(u32, u32),
}
impl Root {
    pub(super) fn coarse(self) -> HoldRoot {
        match self {
            Self::Local(local) | Self::Field(local, _) => HoldRoot::Local(local),
            Self::Capture(index) => HoldRoot::Capture(index),
            Self::Parameter(index) | Self::ParameterCapture(index, _) => HoldRoot::Parameter(index),
        }
    }
    fn field(self) -> Option<u32> {
        match self {
            Self::Field(_, index) => Some(index),
            _ => None,
        }
    }
    pub(super) fn intersects(self, other: Self) -> bool {
        // An incoming environment position is distinct from its externally
        // captured targets. Different capture slots are symbolic: the actual
        // caller must instantiate them before proving any separation.
        if matches!(self, Self::ParameterCapture(..)) || matches!(other, Self::ParameterCapture(..))
        {
            return self == other;
        }
        self.coarse() == other.coarse()
            && (self.field().is_none() || other.field().is_none() || self == other)
    }
    pub(super) fn local(self) -> Option<LocalId> {
        match self {
            Self::Local(local) | Self::Field(local, _) => Some(local),
            _ => None,
        }
    }
}
impl Ord for Root {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let projection = |root: &Self| match root {
            Self::Field(_, index) => (1, *index),
            Self::ParameterCapture(_, capture) => (2, *capture),
            _ => (0, 0),
        };
        (self.coarse(), projection(self)).cmp(&(other.coarse(), projection(other)))
    }
}
impl PartialOrd for Root {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn field_root(
    function: &TypedFunction,
    local: LocalId,
    index: u32,
    budget: &mut Budget,
) -> Check<Root> {
    let Some(Type::Package { fields, .. }) = function.locals.get(local.0 as usize).map(|l| &l.ty)
    else {
        return Ok(Root::Local(local));
    };
    let Some((_, ty)) = fields.get(index as usize) else {
        return Ok(Root::Local(local));
    };
    // Package validation separately proves the real inline field layout and
    // self-contained callback environment. Never infer separation from a name
    // or a distinct payload type. Unknown user cleanup keeps its coarse root.
    resources::charge_payload(ty, budget)?;
    Ok(
        if ty.is_copy_value()
            || matches!(ty, Type::Closure(_))
            || byte_buffers::is_owned(ty)
            || matches!(ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. }))
        {
            Root::Field(local, index)
        } else {
            Root::Local(local)
        },
    )
}

pub(super) fn overlaps(left: &[Root], right: &[Root], budget: &mut Budget) -> Check<bool> {
    budget.spend(left.len().saturating_add(right.len()))?;
    let (mut a, mut b) = (0, 0);
    while a < left.len() && b < right.len() {
        if left[a].intersects(right[b]) {
            return Ok(true);
        }
        if left[a] < right[b] {
            a += 1;
        } else {
            b += 1;
        }
    }
    Ok(false)
}

pub(super) fn contains_overlap(roots: &[Root], root: Root, budget: &mut Budget) -> Check<bool> {
    lookup(budget, roots.len())?;
    match roots.binary_search(&root) {
        Ok(_) => Ok(true),
        Err(_) if matches!(root, Root::ParameterCapture(..)) => Ok(false),
        Err(next) if root.field().is_none() => {
            Ok(roots.get(next).is_some_and(|r| r.coarse() == root.coarse()))
        }
        Err(_) => {
            lookup(budget, roots.len())?;
            let Root::Field(local, _) = root else {
                unreachable!()
            };
            Ok(roots.binary_search(&Root::Local(local)).is_ok())
        }
    }
}
