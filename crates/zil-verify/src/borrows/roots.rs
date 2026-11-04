//! Private verified storage projections. Public candidates still use HoldRoot.
//! A field root never transports a loan across replacement or whole movement.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Root {
    Local(LocalId),
    Field(LocalId, u32),
    Projection(LocalId, u32, u32),
    Window(HoldRoot, u32, u32),
    Capture(u32),
    Parameter(u32),
    ParameterCapture(u32, u32),
}
impl Root {
    pub(super) fn coarse(self) -> HoldRoot {
        match self {
            Self::Local(local) | Self::Field(local, _) | Self::Projection(local, ..) => {
                HoldRoot::Local(local)
            }
            Self::Window(root, ..) => root,
            Self::Capture(index) => HoldRoot::Capture(index),
            Self::Parameter(index) | Self::ParameterCapture(index, _) => HoldRoot::Parameter(index),
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
        if self.coarse() != other.coarse() {
            return false;
        }
        match (self, other) {
            (Self::Projection(_, a, b), Self::Projection(_, c, d))
            | (Self::Window(_, a, b), Self::Window(_, c, d))
            | (Self::Projection(_, a, b), Self::Window(_, c, d))
            | (Self::Window(_, a, b), Self::Projection(_, c, d)) => a < d && c < b,
            (Self::Field(..), Self::Field(..)) => self == other,
            _ => true,
        }
    }
    pub(super) fn local(self) -> Option<LocalId> {
        match self {
            Self::Local(local) | Self::Field(local, _) | Self::Projection(local, ..) => Some(local),
            Self::Window(HoldRoot::Local(local), ..) => Some(local),
            _ => None,
        }
    }
}
impl Ord for Root {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let projection = |root: &Self| match root {
            Self::Field(_, index) => (1, *index, 0),
            Self::Projection(_, start, end) => (2, *start, *end),
            Self::Window(_, start, end) => (4, *start, *end),
            Self::ParameterCapture(_, capture) => (3, *capture, 0),
            _ => (0, 0, 0),
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
    // or a distinct payload type. User resources must have an independently
    // checked nominal finite Copy record payload; opaque carriers stay coarse.
    resources::charge_payload(ty, budget)?;
    Ok(
        if ty.is_copy_value()
            || matches!(ty, Type::Closure(_))
            || matches!(ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value())
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
    budget.spend(left.len().saturating_mul(right.len()).saturating_add(1))?;
    Ok(left.iter().any(|a| right.iter().any(|b| a.intersects(*b))))
}

pub(super) fn contains_overlap(roots: &[Root], root: Root, budget: &mut Budget) -> Check<bool> {
    budget.spend(roots.len().saturating_add(1))?;
    Ok(roots.iter().any(|r| r.intersects(root)))
}

/// Virtual inline-tree intervals preserve complete real projection ancestry.
/// Alternative tags get disjoint intervals only after the Sum checker proves
/// each active prefix; replacing the parent still overlaps every interval.
pub(super) fn variant_root(
    f: &TypedFunction,
    local: LocalId,
    path: &[VariantProjection],
    budget: &mut Budget,
) -> Check<Root> {
    fn size(t: &Type, b: &mut Budget) -> Check<u32> {
        b.spend(1)?;
        let mut count = 1u32;
        let mut add = |t: &Type| -> Check<()> {
            count = count
                .checked_add(size(t, b)?)
                .ok_or_else(|| limit("projection interval overflow"))?;
            Ok(())
        };
        match t {
            Type::Tuple(fs) => {
                for t in fs {
                    add(t)?
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    add(t)?
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        add(t)?
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                add(error)?;
                add(result)?;
            }
            _ => {}
        }
        Ok(count)
    }
    let mut ty = local_type(f, local, None)?;
    let mut start = 0u32;
    for step in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(reject(
                "E-VERIFY-SUM",
                None,
                "payload path crosses non-Enum",
            ));
        };
        start = start
            .checked_add(1)
            .ok_or_else(|| limit("projection interval overflow"))?;
        for (_, fs) in variants.iter().take(step.variant as usize) {
            for t in fs {
                start = start
                    .checked_add(size(t, budget)?)
                    .ok_or_else(|| limit("projection interval overflow"))?;
            }
        }
        let fields = &variants
            .get(step.variant as usize)
            .ok_or_else(|| reject("E-VERIFY-SUM", None, "payload tag outside Enum"))?
            .1;
        for t in fields.iter().take(step.index as usize) {
            start = start
                .checked_add(size(t, budget)?)
                .ok_or_else(|| limit("projection interval overflow"))?;
        }
        ty = fields
            .get(step.index as usize)
            .ok_or_else(|| reject("E-VERIFY-SUM", None, "payload field outside variant"))?;
    }
    let end = start
        .checked_add(size(ty, budget)?)
        .ok_or_else(|| limit("projection interval overflow"))?;
    Ok(if path.is_empty() {
        Root::Local(local)
    } else {
        Root::Projection(local, start, end)
    })
}

pub(super) fn address_roots(
    f: &TypedFunction,
    address: ValueId,
    path: &[VariantProjection],
    source: &[Root],
    budget: &mut Budget,
) -> Check<Vec<Root>> {
    let Type::Address { inner, .. } = value_type(f, address, None)? else {
        return Err(reject(
            "E-VERIFY-SUM-ADDRESS",
            None,
            "payload base is not an address",
        ));
    };
    fn width(t: &Type, b: &mut Budget) -> Check<u32> {
        b.spend(1)?;
        let mut n = 1u32;
        let mut add = |t: &Type| -> Check<()> {
            n = n
                .checked_add(width(t, b)?)
                .ok_or_else(|| limit("address interval overflow"))?;
            Ok(())
        };
        match t {
            Type::Tuple(fs) => {
                for t in fs {
                    add(t)?
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    add(t)?
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        add(t)?
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                add(error)?;
                add(result)?;
            }
            _ => {}
        }
        Ok(n)
    }
    let mut ty = inner.as_ref();
    let mut start = 0u32;
    for part in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(reject(
                "E-VERIFY-SUM-ADDRESS",
                None,
                "payload path crosses non-Enum",
            ));
        };
        start = start
            .checked_add(1)
            .ok_or_else(|| limit("address interval overflow"))?;
        for (_, fs) in variants.iter().take(part.variant as usize) {
            for t in fs {
                start = start
                    .checked_add(width(t, budget)?)
                    .ok_or_else(|| limit("address interval overflow"))?;
            }
        }
        let fs = &variants
            .get(part.variant as usize)
            .ok_or_else(|| reject("E-VERIFY-SUM-ADDRESS", None, "payload tag out of range"))?
            .1;
        for t in fs.iter().take(part.index as usize) {
            start = start
                .checked_add(width(t, budget)?)
                .ok_or_else(|| limit("address interval overflow"))?;
        }
        ty = fs
            .get(part.index as usize)
            .ok_or_else(|| reject("E-VERIFY-SUM-ADDRESS", None, "payload field out of range"))?;
    }
    let end = start
        .checked_add(width(ty, budget)?)
        .ok_or_else(|| limit("address interval overflow"))?;
    budget.spend(source.len())?;
    source
        .iter()
        .map(|root| {
            Ok(match *root {
                Root::Local(l) => Root::Projection(l, start, end),
                Root::Parameter(p) => Root::Window(HoldRoot::Parameter(p), start, end),
                Root::Projection(l, offset, limit_end) => {
                    let a = offset
                        .checked_add(start)
                        .ok_or_else(|| limit("address interval overflow"))?;
                    let z = offset
                        .checked_add(end)
                        .ok_or_else(|| limit("address interval overflow"))?;
                    if z > limit_end {
                        return Err(reject(
                            "E-VERIFY-SUM-ADDRESS",
                            None,
                            "child payload escapes parent interval",
                        ));
                    }
                    Root::Projection(l, a, z)
                }
                Root::Window(origin, offset, limit_end) => {
                    let a = offset
                        .checked_add(start)
                        .ok_or_else(|| limit("address interval overflow"))?;
                    let z = offset
                        .checked_add(end)
                        .ok_or_else(|| limit("address interval overflow"))?;
                    if z > limit_end {
                        return Err(reject(
                            "E-VERIFY-SUM-ADDRESS",
                            None,
                            "child payload escapes parent interval",
                        ));
                    }
                    Root::Window(origin, a, z)
                }
                root => root,
            })
        })
        .collect()
}

pub(super) fn inline_field_root(
    f: &TypedFunction,
    local: LocalId,
    path: &[u32],
    budget: &mut Budget,
) -> Check<Root> {
    fn width(t: &Type, b: &mut Budget) -> Check<u32> {
        b.spend(1)?;
        let mut n = 1u32;
        let mut add = |t: &Type| -> Check<()> {
            n = n
                .checked_add(width(t, b)?)
                .ok_or_else(|| limit("field interval overflow"))?;
            Ok(())
        };
        match t {
            Type::Tuple(fs) => {
                for t in fs {
                    add(t)?
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    add(t)?
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        add(t)?
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                add(error)?;
                add(result)?;
            }
            _ => {}
        }
        Ok(n)
    }
    let mut ty = local_type(f, local, None)?;
    let mut start = 0u32;
    for field in path {
        start = start
            .checked_add(1)
            .ok_or_else(|| limit("field interval overflow"))?;
        ty = match ty {
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                for t in fields.iter().take(*field as usize) {
                    start = start
                        .checked_add(width(t, budget)?)
                        .ok_or_else(|| limit("field interval overflow"))?;
                }
                fields.get(*field as usize)
            }
            Type::Record { fields, .. } => {
                budget.spend(fields.len())?;
                for (_, t) in fields.iter().take(*field as usize) {
                    start = start
                        .checked_add(width(t, budget)?)
                        .ok_or_else(|| limit("field interval overflow"))?;
                }
                fields.get(*field as usize).map(|(_, t)| t)
            }
            _ => {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    None,
                    "field path crosses non-Copy aggregate",
                ));
            }
        }
        .ok_or_else(|| reject("E-VERIFY-ADDRESS", None, "field path outside aggregate"))?;
    }

    let end = start
        .checked_add(width(ty, budget)?)
        .ok_or_else(|| limit("field interval overflow"))?;
    Ok(Root::Projection(local, start, end))
}

/// A GEP through an inline Copy aggregate keeps the parent's authority while
/// narrowing the real storage interval used by the later read/reborrow.
pub(super) fn inline_address_field(
    f: &TypedFunction,
    address: ValueId,
    index: u32,
    source: &[Root],
    budget: &mut Budget,
) -> Check<Vec<Root>> {
    let Type::Address { inner, .. } = value_type(f, address, None)? else {
        return Err(reject(
            "E-VERIFY-ADDRESS",
            None,
            "inline field base is not an address",
        ));
    };
    resources::charge_payload(inner, budget)?;
    if !matches!(
        inner.as_ref(),
        Type::Tuple(_) | Type::Record { .. } | Type::Package { .. }
    ) {
        budget.spend(source.len())?;
        return Ok(source.to_vec());
    }
    fn width(t: &Type, b: &mut Budget) -> Check<u32> {
        b.spend(1)?;
        let mut total = 1u32;
        let mut add = |t: &Type| -> Check<()> {
            total = total
                .checked_add(width(t, b)?)
                .ok_or_else(|| limit("inline field interval overflow"))?;
            Ok(())
        };
        match t {
            Type::Tuple(fs) => {
                for t in fs {
                    add(t)?
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    add(t)?
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        add(t)?
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                add(error)?;
                add(result)?;
            }
            _ => {}
        }
        Ok(total)
    }
    let mut start = 1u32;
    let target = match inner.as_ref() {
        Type::Tuple(fs) => {
            budget.spend(fs.len())?;
            for t in fs.iter().take(index as usize) {
                start = start
                    .checked_add(width(t, budget)?)
                    .ok_or_else(|| limit("inline field interval overflow"))?;
            }
            fs.get(index as usize)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            budget.spend(fields.len())?;
            for (_, t) in fields.iter().take(index as usize) {
                start = start
                    .checked_add(width(t, budget)?)
                    .ok_or_else(|| limit("inline field interval overflow"))?;
            }
            fields.get(index as usize).map(|(_, t)| t)
        }
        _ => unreachable!(),
    }
    .ok_or_else(|| reject("E-VERIFY-ADDRESS", None, "inline field is out of range"))?;
    let end = start
        .checked_add(width(target, budget)?)
        .ok_or_else(|| limit("inline field interval overflow"))?;
    budget.spend(source.len())?;
    source
        .iter()
        .map(|root| {
            Ok(match *root {
                Root::Local(l) => Root::Projection(l, start, end),
                Root::Parameter(p) => Root::Window(HoldRoot::Parameter(p), start, end),
                Root::Projection(l, base, bound) => {
                    let a = base
                        .checked_add(start)
                        .ok_or_else(|| limit("inline field interval overflow"))?;
                    let z = base
                        .checked_add(end)
                        .ok_or_else(|| limit("inline field interval overflow"))?;
                    if z > bound {
                        return Err(reject(
                            "E-VERIFY-ADDRESS",
                            None,
                            "inline field escapes parent interval",
                        ));
                    }
                    Root::Projection(l, a, z)
                }
                Root::Window(origin, base, bound) => {
                    let a = base
                        .checked_add(start)
                        .ok_or_else(|| limit("inline field interval overflow"))?;
                    let z = base
                        .checked_add(end)
                        .ok_or_else(|| limit("inline field interval overflow"))?;
                    if z > bound {
                        return Err(reject(
                            "E-VERIFY-ADDRESS",
                            None,
                            "inline field escapes parent interval",
                        ));
                    }
                    Root::Window(origin, a, z)
                }
                root => root,
            })
        })
        .collect()
}

pub(super) fn mixed_root(
    f: &TypedFunction,
    local: LocalId,
    path: &[zil_ir::PlaceProjection],
    header: bool,
    b: &mut Budget,
) -> Check<Root> {
    use zil_ir::PlaceProjection;
    fn width(t: &Type, b: &mut Budget) -> Check<u32> {
        b.spend(1)?;
        let mut n = 1u32;
        let mut add = |t: &Type| -> Check<()> {
            n = n
                .checked_add(width(t, b)?)
                .ok_or_else(|| limit("mixed interval overflow"))?;
            Ok(())
        };
        match t {
            Type::Tuple(fs) => {
                for t in fs {
                    add(t)?;
                }
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                for (_, t) in fields {
                    add(t)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        add(t)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                add(error)?;
                add(result)?;
            }
            _ => {}
        }
        Ok(n)
    }
    let mut ty = local_type(f, local, None)?;
    let mut start = 0u32;
    for step in path {
        b.spend(1)?;
        start = start
            .checked_add(1)
            .ok_or_else(|| limit("mixed interval overflow"))?;
        ty = match (step, ty) {
            (PlaceProjection::Field(i), Type::Tuple(fs)) => {
                for t in fs.iter().take(*i as usize) {
                    start = start
                        .checked_add(width(t, b)?)
                        .ok_or_else(|| limit("mixed interval overflow"))?;
                }
                fs.get(*i as usize)
            }
            (PlaceProjection::Field(i), Type::Package { fields, .. }) => {
                for (_, t) in fields.iter().take(*i as usize) {
                    start = start
                        .checked_add(width(t, b)?)
                        .ok_or_else(|| limit("mixed interval overflow"))?;
                }
                fields.get(*i as usize).map(|(_, t)| t)
            }
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => {
                for (_, fs) in variants.iter().take(p.variant as usize) {
                    for t in fs {
                        start = start
                            .checked_add(width(t, b)?)
                            .ok_or_else(|| limit("mixed interval overflow"))?;
                    }
                }
                let fs = &variants
                    .get(p.variant as usize)
                    .ok_or_else(|| {
                        reject("E-VERIFY-PLACE", None, "mixed actual place route invalid")
                    })?
                    .1;
                for t in fs.iter().take(p.index as usize) {
                    start = start
                        .checked_add(width(t, b)?)
                        .ok_or_else(|| limit("mixed interval overflow"))?;
                }
                fs.get(p.index as usize)
            }
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                if !*e {
                    start = start
                        .checked_add(width(error, b)?)
                        .ok_or_else(|| limit("mixed interval overflow"))?;
                }
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| reject("E-VERIFY-PLACE", None, "mixed actual place route invalid"))?;
    }
    let length = if header {
        if !matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) {
            return Err(reject(
                "E-VERIFY-PLACE",
                None,
                "mixed actual place route invalid",
            ));
        }
        1
    } else {
        width(ty, b)?
    };
    let end = start
        .checked_add(length)
        .ok_or_else(|| limit("mixed interval overflow"))?;
    Ok(Root::Projection(local, start, end))
}
