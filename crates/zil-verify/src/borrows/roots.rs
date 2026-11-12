//! Private verified storage projections. Public candidates still use HoldRoot.
//! A field root never transports a loan across replacement or whole movement.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Root {
    Static,
    Local(LocalId),
    Field(LocalId, u32),
    Projection(LocalId, u32, u32),
    Window(HoldRoot, u32, u32),
    // The owner header's inline interval and the heap element's payload
    // interval are separate domains. Conflict checks use the header interval.
    StoreElementWindow(HoldRoot, u32, u32, u32, u32),
    Capture(u32),
    Parameter(u32),
    ParameterPort(u32, u32),
    ParameterCapture(u32, u32),
}
impl Root {
    pub(super) fn coarse(self) -> HoldRoot {
        match self {
            Self::Static => HoldRoot::Static,
            Self::Local(local) | Self::Field(local, _) | Self::Projection(local, ..) => {
                HoldRoot::Local(local)
            }
            Self::Window(root, ..) => root,
            Self::StoreElementWindow(root, ..) => root,
            Self::Capture(index) => HoldRoot::Capture(index),
            Self::Parameter(index)
            | Self::ParameterCapture(index, _)
            | Self::ParameterPort(index, _) => HoldRoot::Parameter(index),
        }
    }
    pub(super) fn intersects(self, other: Self) -> bool {
        let header = |root: Self| match root {
            Self::StoreElementWindow(origin, start, end, ..) => Self::Window(origin, start, end),
            root => root,
        };
        let (left, right) = (header(self), header(other));
        // An incoming environment position is distinct from its externally
        // captured targets. Different capture slots are symbolic: the actual
        // caller must instantiate them before proving any separation.
        if matches!(left, Self::ParameterCapture(..)) || matches!(right, Self::ParameterCapture(..))
        {
            return left == right;
        }
        if matches!(left, Self::ParameterPort(..)) || matches!(right, Self::ParameterPort(..)) {
            return left == right;
        }
        if left.coarse() != right.coarse() {
            return false;
        }
        match (left, right) {
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
            Self::StoreElementWindow(HoldRoot::Local(local), ..) => Some(local),
            _ => None,
        }
    }
}
impl Ord for Root {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let projection = |root: &Self| match root {
            Self::Field(_, index) => (1, *index, 0, 0, 0),
            Self::Projection(_, start, end) => (2, *start, *end, 0, 0),
            Self::Window(_, start, end) => (4, *start, *end, 0, 0),
            Self::StoreElementWindow(_, start, end, payload_start, payload_end) => {
                (5, *start, *end, *payload_start, *payload_end)
            }
            Self::ParameterCapture(_, capture) => (3, *capture, 0, 0, 0),
            Self::ParameterPort(_, port) => (6, *port, 0, 0, 0),
            _ => (0, 0, 0, 0, 0),
        };
        (self.coarse(), projection(self)).cmp(&(other.coarse(), projection(other)))
    }
}
impl PartialOrd for Root {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn store_element_roots(
    function: &TypedFunction,
    result: ValueId,
    source: &[Root],
    budget: &mut Budget,
) -> Check<Vec<Root>> {
    fn payload_width(ty: &Type, budget: &mut Budget) -> Check<u32> {
        budget.spend(1)?;
        let mut count = 1u32;
        let mut add = |child: &Type| -> Check<()> {
            count = count
                .checked_add(payload_width(child, budget)?)
                .ok_or_else(|| limit("Store element payload interval overflow"))?;
            Ok(())
        };
        match ty {
            Type::Tuple(fields) => {
                for child in fields {
                    add(child)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, child) in fields {
                    add(child)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for child in fields {
                        add(child)?;
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
    // This copy is additional work beyond the ordinary roots_join. Charge it
    // before allocating, even when all payload widths are already cached.
    budget.spend(source.len())?;
    let mut output = Vec::with_capacity(source.len());
    let mut width = None;
    for root in source {
        output.push(match *root {
            Root::Window(origin, header_start, header_end) => {
                let payload_end = if let Some(width) = width {
                    width
                } else {
                    let Type::Address { inner, .. } = value_type(function, result, None)? else {
                        return Err(reject(
                            "E-VERIFY-ADDRESS",
                            None,
                            "Store element result is not an address",
                        ));
                    };
                    let count = payload_width(inner, budget)?;
                    width = Some(count);
                    count
                };
                Root::StoreElementWindow(origin, header_start, header_end, 0, payload_end)
            }
            root => root,
        });
    }
    Ok(output)
}

/// Reconstruct the two physical domains of an Option-carried Store element
/// from the independent stored solver's private actual source. The header is
/// inline in the manager; the element projection is inside heap storage.
pub(super) fn store_option_root(
    f: &TypedFunction,
    result_type: &Type,
    source: &crate::stored_flow::Source,
    budget: &mut Budget,
) -> Check<Root> {
    if source.heap_element.is_none() {
        return match (source.base, source.leaf.as_slice()) {
            (HoldRoot::Parameter(position), [zil_ir::PlaceProjection::Variant(selector)])
                if selector.variant == 1 && selector.index == 0 =>
            {
                if !source
                    .inline
                    .iter()
                    .all(|step| matches!(step, zil_ir::PlaceProjection::Field(_)))
                {
                    return Err(reject(
                        "E-VERIFY-STORE-WINDOW",
                        None,
                        "Option input projection is not a checked field path",
                    ));
                }
                budget.spend(source.inline.len().saturating_add(1))?;
                Ok(Root::Parameter(position))
            }
            _ => Err(reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Option payload has no checked Store element domain",
            )),
        };
    }
    if source.retained_port.is_some() || !source.leaf.is_empty() {
        return Err(reject(
            "E-VERIFY-STORE-WINDOW",
            None,
            "Store window carries an unrelated retained selector",
        ));
    }
    let local = match source.base {
        HoldRoot::Local(local) => local,
        HoldRoot::Parameter(position) => *f.parameters.get(position as usize).ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Store window manager input is absent",
            )
        })?,
        HoldRoot::Capture(_) | HoldRoot::Static => {
            return Err(reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Store window lacks an ordinary manager root",
            ));
        }
    };
    let mut ty = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Store window manager local is absent",
            )
        })?
        .ty;
    // The actual manager may arrive through either address mode. This only
    // exposes its header shape; the receiver loan and permissions are checked
    // independently before a returned Store window can escape.
    if let Type::Address { inner, .. } = ty {
        ty = inner;
    }
    let (header_start, header_end) = mixed_interval(ty, &source.inline, false, budget)?;
    for step in &source.inline {
        ty = match (step, ty) {
            (
                zil_ir::PlaceProjection::Field(index),
                Type::Package { fields, .. } | Type::Record { fields, .. },
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            _ => None,
        }
        .ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Store header path crosses an invalid actual field",
            )
        })?;
    }
    let Type::Managed(head) = ty else {
        return Err(reject(
            "E-VERIFY-STORE-WINDOW",
            None,
            "Option window header is not an owned Store",
        ));
    };
    let Type::OwnedStore { element } = head.as_ref() else {
        return Err(reject(
            "E-VERIFY-STORE-WINDOW",
            None,
            "Option window header is not an owned Store",
        ));
    };
    let heap = source.heap_element.as_ref().unwrap();
    let (payload_start, payload_end) = mixed_interval(element, heap, false, budget)?;
    let mut pointee = element.as_ref();
    for step in heap {
        pointee = match (step, pointee) {
            (
                zil_ir::PlaceProjection::Field(index),
                Type::Package { fields, .. } | Type::Record { fields, .. },
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            _ => None,
        }
        .ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Store element path crosses an invalid actual field",
            )
        })?;
    }
    let expected = match result_type {
        Type::Address {
            mutable: false,
            inner,
        } => inner.as_ref(),
        Type::BorrowedStorage { storage, .. } => {
            let Type::Enum { variants, .. } = storage.as_ref() else {
                return Err(reject(
                    "E-VERIFY-STORE-WINDOW",
                    None,
                    "Option carrier lacks its actual Enum",
                ));
            };
            let [
                Type::Address {
                    mutable: false,
                    inner,
                },
            ] = variants
                .get(1)
                .map(|(_, payload)| payload.as_slice())
                .unwrap_or(&[])
            else {
                return Err(reject(
                    "E-VERIFY-STORE-WINDOW",
                    None,
                    "Option carrier lacks a shared Some payload",
                ));
            };
            inner.as_ref()
        }
        _ => {
            return Err(reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Store window result lacks a shared address",
            ));
        }
    };
    resources::charge_payload(expected, budget)?;
    if pointee != expected {
        return Err(reject(
            "E-VERIFY-STORE-WINDOW",
            None,
            "Store element projection differs from the complete result pointee",
        ));
    }
    Ok(Root::StoreElementWindow(
        source.base,
        header_start,
        header_end,
        payload_start,
        payload_end,
    ))
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
            || matches!(ty, Type::Package { .. })
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

/// Legacy direct-field roots and mixed paths name the same physical tree.
/// Resolve only mixed comparisons so legacy field-only fees remain unchanged.
pub(super) fn overlaps_for(
    f: &TypedFunction,
    left: &[Root],
    right: &[Root],
    budget: &mut Budget,
) -> Check<bool> {
    budget.spend(left.len().saturating_mul(right.len()).saturating_add(1))?;
    for a in left {
        for b in right {
            let (mut a, mut b) = (*a, *b);
            match (a, b) {
                (Root::Field(l, i), Root::Projection(r, ..))
                | (Root::Field(l, i), Root::Window(HoldRoot::Local(r), ..)) if l == r => {
                    a = mixed_root(f, l, &[zil_ir::PlaceProjection::Field(i)], false, budget)?;
                }
                (Root::Projection(l, ..), Root::Field(r, i))
                | (Root::Window(HoldRoot::Local(l), ..), Root::Field(r, i)) if l == r => {
                    b = mixed_root(f, r, &[zil_ir::PlaceProjection::Field(i)], false, budget)?;
                }
                _ => {}
            }
            if a.intersects(b) {
                return Ok(true);
            }
        }
    }
    Ok(false)
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
                Root::StoreElementWindow(
                    origin,
                    header_start,
                    header_end,
                    payload_start,
                    payload_end,
                ) => {
                    let a = payload_start
                        .checked_add(start)
                        .ok_or_else(|| limit("Store element field interval overflow"))?;
                    let z = payload_start
                        .checked_add(end)
                        .ok_or_else(|| limit("Store element field interval overflow"))?;
                    if z > payload_end {
                        return Err(reject(
                            "E-VERIFY-ADDRESS",
                            None,
                            "Store element field escapes payload interval",
                        ));
                    }
                    Root::StoreElementWindow(origin, header_start, header_end, a, z)
                }
                root => root,
            })
        })
        .collect()
}

fn mixed_interval(
    mut ty: &Type,
    path: &[zil_ir::PlaceProjection],
    header: bool,
    b: &mut Budget,
) -> Check<(u32, u32)> {
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
            (
                PlaceProjection::Field(i),
                Type::Package { fields, .. } | Type::Record { fields, .. },
            ) => {
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
    Ok((start, end))
}

pub(super) fn mixed_root(
    f: &TypedFunction,
    local: LocalId,
    path: &[zil_ir::PlaceProjection],
    header: bool,
    b: &mut Budget,
) -> Check<Root> {
    let (start, end) = mixed_interval(local_type(f, local, None)?, path, header, b)?;
    Ok(Root::Projection(local, start, end))
}

pub(super) fn mixed_address_roots(
    f: &TypedFunction,
    address: ValueId,
    path: &[zil_ir::PlaceProjection],
    header: bool,
    source: &Roots,
    budget: &mut Budget,
) -> Check<Roots> {
    let Type::Address { inner, .. } = value_type(f, address, None)? else {
        return Err(reject(
            "E-VERIFY-PLACE",
            None,
            "mixed base is not actual address",
        ));
    };
    let (start, end) = mixed_interval(inner, path, header, budget)?;
    budget.spend(source.len())?;
    source
        .iter()
        .map(|root| {
            Ok(match *root {
                Root::Local(l) => Root::Projection(l, start, end),
                Root::Parameter(p) => Root::Window(HoldRoot::Parameter(p), start, end),
                Root::Capture(c) => Root::Window(HoldRoot::Capture(c), start, end),
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
