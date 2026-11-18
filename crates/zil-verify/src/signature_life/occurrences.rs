use super::*;
use zil_base::DiagnosticRole;

pub(super) fn failure(
    span: Option<Span>,
    base: StorageBase,
    message: impl Into<String>,
) -> Diagnostic {
    let message = format!("signature class relation fails. {}", message.into());
    let role = if base == StorageBase::Result {
        DiagnosticRole::Result
    } else {
        DiagnosticRole::Input
    };
    let mut error = reject("E-VERIFY-SIGNATURE-CLASS", span, message.clone());
    error.relate(role, span, None, message);
    error
}

pub(super) fn binding<'a>(
    c: &'a StorageLifeContract,
    row: usize,
    expected: &StoragePosition,
    span: Option<Span>,
) -> Check<&'a StorageLifeBinding> {
    if let Some(actual) = physical_binding(c, row, expected) {
        return Ok(actual);
    }
    match c.bindings.get(row) {
        Some(actual) => Err(failure(
            span,
            expected.base,
            format!(
                "signature borrow row {row} must bind {expected:?}; found {:?} with {:?}",
                actual.position, actual.binder,
            ),
        )),
        None => Err(failure(
            span,
            expected.base,
            format!("signature omits physical borrow row {row} for {expected:?}",),
        )),
    }
}

pub(super) fn class<'a>(
    c: &'a StorageLifeContract,
    row: usize,
    expected: &SignatureLifeClass,
    span: Option<Span>,
) -> Check<&'a SignatureLifeClassBinding> {
    match c.classes.get(row) {
        Some(actual) if actual.position == *expected => Ok(actual),
        Some(actual) => Err(failure(
            span,
            expected.base,
            format!(
                "signature class row {row} must bind {expected:?}; found {:?} with {:?}",
                actual.position, actual.binder,
            ),
        )),
        None => Err(failure(
            span,
            expected.base,
            format!("signature omits intrinsic class row {row} for {expected:?}",),
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Authority {
    Shared,
    Exclusive,
    Description,
}

impl Authority {
    fn for_address(mutable: bool) -> Self {
        if mutable {
            Self::Exclusive
        } else {
            Self::Shared
        }
    }

    fn permits(self, requested: Self) -> bool {
        self == requested || matches!(self, Self::Exclusive | Self::Description)
    }
}

/// The caller has already authenticated every row against the actual physical
/// signature. A result can reuse only a matching input binding that supplies
/// its access mode. The body independently proves the returned SSA source.
pub(super) fn returned_root<P: InputType>(
    c: &StorageLifeContract,
    parameters: &[P],
    captures: Option<&[Type]>,
    kind: Option<CallableKind>,
    result: &Type,
    allow_byte_input: bool,
    span: Option<Span>,
) -> Check<()> {
    let Type::Address { mutable, inner } = result else {
        return Ok(());
    };
    let requested = Authority::for_address(*mutable);
    let output = StoragePosition {
        base: StorageBase::Result,
        path: vec![],
    };
    if requested == Authority::Shared && c.static_bindings.contains(&output) {
        return Ok(());
    }
    let output_binder = outer(c, output.base).ok_or_else(|| {
        failure(
            span,
            output.base,
            "signature result address has no physical root binding",
        )
    })?;
    let actual_input = |base| match base {
        StorageBase::Parameter(index) => parameters.get(index as usize).map(InputType::ty),
        StorageBase::Capture(index) => captures.and_then(|types| types.get(index as usize)),
        _ => None,
    };
    let authority = |position: &StoragePosition| match position.path.as_slice() {
        [] => match (position.base, actual_input(position.base)) {
            (_, Some(Type::Address { mutable, .. })) => Some(Authority::for_address(*mutable)),
            (StorageBase::Parameter(_), Some(Type::Desc(payload)))
                if allow_byte_input
                    && payload.as_ref() == &Type::ByteBuffer
                    && matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8) =>
            {
                Some(Authority::Description)
            }
            (StorageBase::Receiver, _) if captures.is_some() => match kind {
                Some(CallableKind::Fn) => Some(Authority::Shared),
                Some(CallableKind::FnMut) => Some(Authority::Exclusive),
                _ => None,
            },
            _ => None,
        },
        [ContractProjection::Place(PlaceProjection::Field(_))]
            if matches!(position.base, StorageBase::Parameter(_)) =>
        {
            let ty = actual_input(position.base)?;
            let value = match ty {
                Type::Address { inner, .. } | Type::Desc(inner) => inner.as_ref(),
                _ => ty,
            };
            super::super::stored_flow::shared_array_value(value).then_some(Authority::Shared)
        }
        _ => None,
    };
    let mut candidate = None;
    for row in c
        .bindings
        .iter()
        .filter(|row| row.position.base != StorageBase::Result && row.binder == output_binder)
    {
        let mode = authority(&row.position);
        if mode.is_some_and(|mode| mode.permits(requested)) {
            return Ok(());
        }
        candidate.get_or_insert_with(|| format!("input {:?} has authority {mode:?}", row.position));
    }
    for row in c.classes.iter().filter(|row| row.binder == output_binder) {
        let mode = match (row.position.base, actual_input(row.position.base)) {
            (StorageBase::Parameter(_), Some(Type::Address { mutable: true, .. }))
                if row.position.path.contains(&LifeTypeProjection::Pointee) =>
            {
                Some(Authority::Exclusive)
            }
            (
                StorageBase::Capture(_),
                Some(Type::Address { mutable: true, .. } | Type::BorrowedStorage { .. }),
            ) => Some(Authority::Exclusive),
            (StorageBase::Parameter(_) | StorageBase::Capture(_), Some(_)) => {
                Some(Authority::Shared)
            }
            _ => None,
        };
        if mode.is_some_and(|mode| mode.permits(requested)) {
            return Ok(());
        }
        if row.position.base != StorageBase::Result {
            candidate.get_or_insert_with(|| {
                format!("input class {:?} has authority {mode:?}", row.position)
            });
        }
    }
    let detail = candidate.as_deref().unwrap_or("no input binds that class");
    Err(failure(
        span,
        StorageBase::Result,
        format!(
            "signature result root {output_binder:?} needs {requested:?} input authority; {detail}",
        ),
    ))
}
