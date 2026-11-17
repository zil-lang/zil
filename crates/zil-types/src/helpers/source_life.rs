//! The original helper bound is a raw proposal, not a class-equality proof.
use super::*;

fn bad(span: Span, message: &str) -> Diagnostic {
    error("E-CALLABLE-CONTEXT", span, message)
}
fn pay(left: &mut usize, count: usize, span: Span) -> R<()> {
    *left = left.checked_sub(count).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "source helper context work exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

/// Flat context preflight before a role/signature/Core snapshot. None performs
/// no work. Const kinds are outside this profile and rejected before cloning.
pub(crate) fn reserve(
    bound: &CoreCallableParameter,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
    span: Span,
) -> R<()> {
    let Some(context) = &bound.source_life else {
        return Ok(());
    };
    let factor = copies.saturating_add(1);
    let width = context
        .parameters
        .len()
        .saturating_add(context.named_fixed.len())
        .saturating_add(context.inputs.len())
        .saturating_add(1);
    pay(
        left,
        width.saturating_mul(width).saturating_mul(factor),
        span,
    )?;
    for count in [
        context.parameters.len(),
        context.named_fixed.len(),
        context.inputs.len(),
        context.identity.module_path.len(),
        context.identity.declaration_path.len(),
        context.identity.generation_path.len(),
    ] {
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source helper context vector exceeded",
                Some(span),
            ));
        }
        pay(left, count.saturating_mul(factor), span)?;
    }
    if context.declaration.compiler
        || !context.identity.generation_path.is_empty()
        || !matches!(
            bound.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        )
        || context.parameters.iter().enumerate().any(|(index, kind)| {
            if index == context.generic_parameter as usize {
                *kind != LifeParameterKind::Type
            } else {
                !matches!(kind, LifeParameterKind::Lifetime | LifeParameterKind::Type)
            }
        })
        || context.parameters.get(context.generic_parameter as usize)
            != Some(&LifeParameterKind::Type)
    {
        return Err(unsupported(
            span,
            "source helper context requires one original shared Fn bound and life declarations",
        ));
    }
    for text in std::iter::once(&context.identity.package)
        .chain(&context.identity.module_path)
        .chain(&context.identity.declaration_path)
    {
        pay(
            left,
            text.len().saturating_add(1).saturating_mul(factor),
            span,
        )?;
    }
    let life = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper context omits its bound P/R proposal"))?;
    for count in [life.bindings.len(), life.classes.len()] {
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source helper bound vector exceeded",
                Some(span),
            ));
        }
    }
    let occurrences = life.bindings.len().saturating_add(life.classes.len());
    if life.fixed_binder_count as usize > occurrences
        || life.invocation_binder_count as usize > occurrences
    {
        return Err(bad(
            span,
            "source helper bound domain exceeds actual P/R occurrences",
        ));
    }
    pay(
        left,
        borrowed_fp::metadata_work(life).saturating_mul(factor),
        span,
    )?;
    for path in life
        .bindings
        .iter()
        .map(|r| r.position.path.len())
        .chain(life.classes.iter().map(|r| r.position.path.len()))
        .chain(context.inputs.iter().map(|r| match &r.position {
            CoreClosureHeaderPosition::Borrow(p) => p.path.len(),
            CoreClosureHeaderPosition::Class(p) => p.path.len(),
        }))
    {
        if path > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source helper context path depth exceeded",
                Some(span),
            ));
        }
        pay(left, path.saturating_add(1).saturating_mul(factor), span)?;
    }
    if life
        .bindings
        .iter()
        .any(|r| matches!(r.position.base, StorageBase::Capture(_)))
        || life
            .classes
            .iter()
            .any(|r| matches!(r.position.base, StorageBase::Capture(_)))
    {
        return Err(bad(
            span,
            "source helper bound is a P/R proposal, not a capture contract",
        ));
    }
    Ok(())
}

pub(crate) fn reserve_function(
    function: &CoreFunction,
    bound: &CoreCallableParameter,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    reserve(bound, limits, left, copies, function.span)?;
    if bound.source_life.is_none() {
        return Ok(());
    }
    let mut work = function
        .parameters
        .len()
        .saturating_add(bound.generic_name.len());
    if let Some(Type::Address { inner, .. }) =
        function.parameters.get(bound.index as usize).map(|p| &p.ty)
        && let Type::Named(name) = inner.as_ref()
    {
        work = work.saturating_add(name.len());
    }
    if let Some(Type::Named(name)) = function.parameters.get(bound.index as usize).map(|p| &p.ty) {
        work = work.saturating_add(name.len());
    }
    if function.callable_parameters.len() > 1 {
        let roles = function.callable_parameters.len();
        if let Some(context) = &bound.source_life {
            let identity = context
                .identity
                .package
                .len()
                .saturating_add(
                    context
                        .identity
                        .module_path
                        .iter()
                        .map(String::len)
                        .sum::<usize>(),
                )
                .saturating_add(
                    context
                        .identity
                        .declaration_path
                        .iter()
                        .map(String::len)
                        .sum::<usize>(),
                )
                .saturating_add(context.identity.generation_path.len());
            work = work.saturating_add(
                context
                    .parameters
                    .len()
                    .saturating_mul(roles)
                    .saturating_mul(
                        identity
                            .saturating_add(context.parameters.len())
                            .saturating_add(4),
                    ),
            );
        }
        work = work.saturating_add(function.parameters.len().saturating_mul(roles));
    }
    for parameter in &function.parameters {
        if let Type::Managed(payload) = &parameter.ty
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            work = work.saturating_add(fields.len().saturating_add(1));
        }
    }
    for state in &function.state_parameters {
        for binding in &state.bindings {
            work = work.saturating_add(binding.fields.len().saturating_mul(4));
        }
    }
    pay(
        left,
        work.saturating_mul(copies.saturating_add(1)),
        function.span,
    )
}

// This walk describes signature occurrences only. It grants neither Copy,
// capture permission nor a lifetime image. Qualified children keep their full
// intrinsic boundary, including phantom classes; physical paths stay separate.
#[derive(Default)]
struct Occurrences {
    borrows: Vec<(StoragePosition, bool)>,
    classes: Vec<SignatureLifeClass>,
}
impl Occurrences {
    fn ty(
        &mut self,
        ty: &Type,
        base: StorageBase,
        physical: &mut Vec<ContractProjection>,
        intrinsic: &mut Vec<LifeTypeProjection>,
        row_limit: usize,
        span: Span,
    ) -> R<()> {
        if self.borrows.len().saturating_add(self.classes.len()) > row_limit {
            return Err(bad(
                span,
                "source bound contains an incomplete actual occurrence domain",
            ));
        }
        match ty {
            Type::Address { mutable, inner } => {
                self.borrows.push((
                    StoragePosition {
                        base,
                        path: physical.clone(),
                    },
                    *mutable,
                ));
                intrinsic.push(LifeTypeProjection::Pointee);
                match inner.as_ref() {
                    Type::BorrowedStorage { life, .. } => {
                        self.boundary(life.binder_count, base, intrinsic, row_limit, span)?
                    }
                    Type::Address { .. } => {
                        return Err(unsupported(
                            span,
                            "nested address callable requires its complete original pointee domain",
                        ));
                    }
                    _ => self.children(inner, base, physical, intrinsic, row_limit, span)?,
                }
                intrinsic.pop();
            }
            Type::BorrowedStorage { life, .. } => {
                self.boundary(life.binder_count, base, intrinsic, row_limit, span)?
            }
            Type::Managed(inner) | Type::Desc(inner) => {
                // Resource carriers add no physical Address. Task's intrinsic
                // boundary remains at its actual payload coordinate.
                self.ty(inner, base, physical, intrinsic, row_limit, span)?;
            }
            Type::Task { retained_life, .. } => {
                for (port, row) in retained_life.ports.iter().enumerate() {
                    physical.push(ContractProjection::RetainedPort(port as u32));
                    self.borrows.push((
                        StoragePosition {
                            base,
                            path: physical.clone(),
                        },
                        row.mode == StoredBorrowMode::ExclusiveAddress,
                    ));
                    physical.pop();
                }
                intrinsic.push(LifeTypeProjection::Pointee);
                self.boundary(retained_life.binder_count, base, intrinsic, row_limit, span)?;
                intrinsic.pop();
            }
            Type::MutexGuard { retained_life, .. } => {
                self.boundary(retained_life.binder_count, base, intrinsic, row_limit, span)?
            }
            Type::BorrowedFunctionPointer { life, .. } => {
                self.boundary(life.fixed_binder_count, base, intrinsic, row_limit, span)?
            }
            Type::Closure(_)
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Named(_)
            | Type::Unsupported(_)
            | Type::Opaque { .. } => {
                return Err(unsupported(
                    span,
                    "callable signature requires its actual complete type domain",
                ));
            }
            _ => self.children(ty, base, physical, intrinsic, row_limit, span)?,
        }
        Ok(())
    }
    fn boundary(
        &mut self,
        count: u32,
        base: StorageBase,
        path: &[LifeTypeProjection],
        row_limit: usize,
        span: Span,
    ) -> R<()> {
        if count as usize > row_limit.saturating_sub(self.classes.len()) {
            return Err(bad(span, "source bound omits a qualified intrinsic class"));
        }
        for class in 0..count {
            self.classes.push(SignatureLifeClass {
                base,
                path: path.to_vec(),
                class,
            });
        }
        Ok(())
    }
    fn child(
        &mut self,
        ty: &Type,
        selector: PlaceProjection,
        base: StorageBase,
        physical: &mut Vec<ContractProjection>,
        intrinsic: &mut Vec<LifeTypeProjection>,
        row_limit: usize,
        span: Span,
    ) -> R<()> {
        physical.push(ContractProjection::Place(selector));
        intrinsic.push(LifeTypeProjection::Place(selector));
        let result = self.ty(ty, base, physical, intrinsic, row_limit, span);
        intrinsic.pop();
        physical.pop();
        result
    }
    fn children(
        &mut self,
        ty: &Type,
        base: StorageBase,
        physical: &mut Vec<ContractProjection>,
        intrinsic: &mut Vec<LifeTypeProjection>,
        row_limit: usize,
        span: Span,
    ) -> R<()> {
        match ty {
            Type::Tuple(fields) => {
                for (index, ty) in fields.iter().enumerate() {
                    self.child(
                        ty,
                        PlaceProjection::Field(index as u32),
                        base,
                        physical,
                        intrinsic,
                        row_limit,
                        span,
                    )?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, ty)) in fields.iter().enumerate() {
                    self.child(
                        ty,
                        PlaceProjection::Field(index as u32),
                        base,
                        physical,
                        intrinsic,
                        row_limit,
                        span,
                    )?;
                }
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, ty) in fields.iter().enumerate() {
                        self.child(
                            ty,
                            PlaceProjection::Variant(zil_ir::VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            base,
                            physical,
                            intrinsic,
                            row_limit,
                            span,
                        )?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.child(
                    error,
                    PlaceProjection::ErrorArm(true),
                    base,
                    physical,
                    intrinsic,
                    row_limit,
                    span,
                )?;
                self.child(
                    result,
                    PlaceProjection::ErrorArm(false),
                    base,
                    physical,
                    intrinsic,
                    row_limit,
                    span,
                )?;
            }
            Type::Array { element, length } if matches!(element.as_ref(), Type::Address { .. }) => {
                if *length as usize > row_limit {
                    return Err(bad(span, "array signature omits actual element borrows"));
                }
                for index in 0..*length {
                    self.child(
                        element,
                        PlaceProjection::Field(index),
                        base,
                        physical,
                        intrinsic,
                        row_limit,
                        span,
                    )?;
                }
            }
            Type::Array { element, .. } => {
                intrinsic.push(LifeTypeProjection::Element);
                self.ty(element, base, physical, intrinsic, row_limit, span)?;
                intrinsic.pop();
            }
            Type::Managed(inner) | Type::Desc(inner) => {
                self.ty(inner, base, physical, intrinsic, row_limit, span)?
            }
            Type::BorrowedStorage { .. }
            | Type::Address { .. }
            | Type::Task { .. }
            | Type::MutexGuard { .. }
            | Type::BorrowedFunctionPointer { .. } => {
                self.ty(ty, base, physical, intrinsic, row_limit, span)?
            }
            Type::Closure(_)
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Named(_)
            | Type::Unsupported(_)
            | Type::Opaque { .. }
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. }
            | Type::OwnedStore { .. } => {
                return Err(unsupported(
                    span,
                    "callable occurrence requires its actual complete owner/type domain",
                ));
            }
            Type::Slice { element } | Type::Mutex { payload: element } => {
                let before = (self.borrows.len(), self.classes.len());
                self.ty(element, base, physical, intrinsic, row_limit, span)?;
                if before != (self.borrows.len(), self.classes.len()) {
                    return Err(unsupported(
                        span,
                        "opaque payload needs its complete intrinsic owner domain",
                    ));
                }
            }
            // These are closed scalar/native/code leaves. Admitting their
            // structural zero domain supplies no resource or Copy permission.
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::ByteBuffer
            | Type::ForeignHandle
            | Type::SyncCell(_)
            | Type::TaskI32
            | Type::CWideString
            | Type::ForeignCallback { .. }
            | Type::FunctionPointer { .. } => {}
        }
        Ok(())
    }
}
fn environment_lending(kind: CallableKind, result: &Type, life: &StorageLifeContract) -> bool {
    kind != CallableKind::FnOnce
        && matches!(result, Type::Address { mutable: false, .. })
        && !life
            .bindings
            .iter()
            .any(|row| row.position.base == StorageBase::Result)
        && !life
            .static_bindings
            .iter()
            .any(|row| row.base == StorageBase::Result)
}
fn environment_result(bound: &CoreCallableParameter) -> bool {
    bound
        .life
        .as_ref()
        .is_some_and(|life| environment_lending(bound.kind, &bound.result, life))
}

/// Authenticate every P/R occurrence and its full partition. Fixed rows name
/// original declared classes here; grounding the actual environment is a later
/// whole-type operation, never an extra invented data argument.
pub(crate) fn relation(bound: &CoreCallableParameter, span: Span) -> R<()> {
    let life = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper bound relation is absent"))?;
    signature_relation(
        &bound.parameters,
        &bound.result,
        life,
        bound.source_life.as_ref(),
        bound.kind,
        span,
    )
}
pub(crate) fn state_relation(bound: &CoreStateBinding, span: Span) -> R<()> {
    match &bound.life {
        Some(life) => signature_relation(
            &bound.parameters,
            &bound.result,
            life,
            bound.source_life.as_ref(),
            bound.kind,
            span,
        ),
        None if bound.source_life.is_some() => Err(bad(
            span,
            "stored source bound omits its complete P/R relation",
        )),
        None => Ok(()),
    }
}
fn signature_relation(
    parameters: &[Type],
    result: &Type,
    life: &StorageLifeContract,
    context: Option<&CoreCallableLifeContext>,
    kind: CallableKind,
    span: Span,
) -> R<()> {
    if context.is_none() {
        return borrowed_fp::relation(parameters, result, life, span);
    }
    let mut expected = Occurrences::default();
    let rows = life
        .bindings
        .len()
        .saturating_add(life.classes.len())
        .saturating_add(life.static_bindings.len());
    for (index, parameter) in parameters.iter().enumerate() {
        expected.ty(
            parameter,
            StorageBase::Parameter(index as u32),
            &mut Vec::new(),
            &mut Vec::new(),
            rows,
            span,
        )?;
    }
    if !environment_lending(kind, result, life) {
        expected.ty(
            result,
            StorageBase::Result,
            &mut Vec::new(),
            &mut Vec::new(),
            rows,
            span,
        )?;
    }
    let mut seen_fixed = std::collections::BTreeSet::new();
    let mut seen_invocation = std::collections::BTreeSet::new();
    let mut binder = |value: StorageBinder| -> R<()> {
        let (index, seen, count) = match value {
            StorageBinder::Fixed(index) => (index, &mut seen_fixed, life.fixed_binder_count),
            StorageBinder::Invocation(index) => {
                (index, &mut seen_invocation, life.invocation_binder_count)
            }
        };
        if index >= count {
            return Err(bad(span, "source bound binder leaves its complete domain"));
        }
        seen.insert(index);
        Ok(())
    };
    let mut borrows = life.bindings.iter();
    let mut statics = life.static_bindings.iter();
    let mut constant = statics.next();
    for (position, mutable) in &expected.borrows {
        if constant == Some(position) {
            if *mutable {
                return Err(bad(
                    span,
                    "Static cannot supply an exclusive signature occurrence",
                ));
            }
            constant = statics.next();
        } else {
            let row = borrows
                .next()
                .ok_or_else(|| bad(span, "source bound omits an actual P/R borrow"))?;
            if row.position != *position {
                return Err(bad(
                    span,
                    "source bound changes an actual P/R borrow selector",
                ));
            }
            binder(row.binder)?;
        }
    }
    // The two row tables are separate; first occurrence can interleave a
    // physical borrow and an intrinsic class. Check density jointly.
    if life.classes.len() != expected.classes.len() {
        return Err(bad(
            span,
            "source bound omits its complete intrinsic class domain",
        ));
    }
    for (row, position) in life.classes.iter().zip(expected.classes) {
        if row.position != position {
            return Err(bad(
                span,
                "source bound changes a complete intrinsic selector",
            ));
        }
        binder(row.binder)?;
    }
    if borrows.next().is_some()
        || constant.is_some()
        || statics.next().is_some()
        || seen_fixed.len() != life.fixed_binder_count as usize
        || seen_invocation.len() != life.invocation_binder_count as usize
    {
        return Err(bad(
            span,
            "source bound contains extra or unused relation rows",
        ));
    }
    Ok(())
}

fn role_paths(
    function: &CoreFunction,
    bound: &CoreCallableParameter,
) -> R<Vec<Vec<LifeTypeProjection>>> {
    let span = function.span;
    let parameter = function
        .parameters
        .get(bound.index as usize)
        .ok_or_else(|| bad(span, "source role actual parameter is absent"))?;
    let mode = parameter_mode(&parameter.ty, &bound.generic_name);
    if let Some(mode) = mode {
        if mode.is_some_and(|mutable| {
            bound.kind == CallableKind::FnOnce || bound.kind == CallableKind::FnMut && !mutable
        }) {
            return Err(bad(span, "source role changes the actual receiver mode"));
        }
        if !function
            .callable_parameters
            .iter()
            .any(|actual| actual == bound)
        {
            return Err(bad(span, "source role changes its actual bound record"));
        }
        return Ok(vec![if mode.is_some() {
            vec![LifeTypeProjection::Pointee]
        } else {
            vec![]
        }]);
    }
    let binding = function
        .state_parameters
        .iter()
        .filter(|state| state.index == bound.index)
        .flat_map(|state| &state.bindings)
        .find(|binding| {
            binding.generic_index == bound.generic_index
                && binding.generic_name == bound.generic_name
        })
        .ok_or_else(|| bad(span, "source role has no actual State binding"))?;
    if binding.kind != bound.kind
        || binding.parameters != bound.parameters
        || binding.result != bound.result
        || binding.life != bound.life
        || binding.source_life != bound.source_life
    {
        return Err(bad(
            span,
            "source State role changes its original complete signature",
        ));
    }
    let (ty, borrowed) = match &parameter.ty {
        Type::Address { inner, .. } => (inner.as_ref(), true),
        ty => (ty, false),
    };
    let Type::Package { fields, .. } = ty else {
        return Err(bad(span, "source State role has no actual Package fields"));
    };
    let mut paths = Vec::new();
    for (ordinal, field) in binding.fields.iter().enumerate() {
        if (ordinal > 0 && binding.fields[ordinal - 1] >= *field)
            || !fields
                .get(*field as usize)
                .is_some_and(|(_, ty)| matches!(ty,Type::Named(name)if name==&binding.generic_name))
        {
            return Err(bad(
                span,
                "source State role changes an actual symbolic field selector",
            ));
        }
        let mut path = if borrowed {
            vec![LifeTypeProjection::Pointee]
        } else {
            vec![]
        };
        path.push(LifeTypeProjection::Place(PlaceProjection::Field(*field)));
        paths.push(path);
    }
    if paths.is_empty() {
        return Err(bad(
            span,
            "source State role omits its complete field occurrences",
        ));
    }
    Ok(paths)
}
fn field_namespace(path: &[LifeTypeProjection]) -> u32 {
    match path.last() {
        Some(LifeTypeProjection::Place(PlaceProjection::Field(field))) => *field,
        _ => u32::MAX,
    }
}

/// This authenticates the actual raw declaration coordinates. Permission and
/// class equalities are still checked from original semantics independently.
pub(crate) fn validate(function: &CoreFunction, bound: &CoreCallableParameter) -> R<()> {
    let Some(context) = &bound.source_life else {
        return Ok(());
    };
    let span = function.span;
    if function.receiver.is_some() || function.foreign.is_some() {
        return Err(bad(
            span,
            "source bound must retain its actual free function owner",
        ));
    }
    role_paths(function, bound)?;
    relation(bound, span)?;
    let life = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source bound omits its full P/R relation"))?;
    if context.named_fixed.len() != life.fixed_binder_count as usize {
        return Err(bad(
            span,
            "source bound omits its full declared Fixed inverse",
        ));
    }
    for (index, fixed) in context.named_fixed.iter().enumerate() {
        if fixed.fixed as usize != index
            || context.parameters.get(fixed.parameter as usize)
                != Some(&LifeParameterKind::Lifetime)
            || context.named_fixed[..index]
                .iter()
                .any(|old| old.parameter == fixed.parameter)
        {
            return Err(bad(
                span,
                "source bound changes its original full lifetime ordinal",
            ));
        }
    }
    let header = function
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(span, "source bound omits its actual header proposal"))?;
    if context.inputs.len()
        != header
            .bindings
            .iter()
            .filter(|row| matches!(row.position.base, StorageBase::Parameter(_)))
            .count()
            .saturating_add(
                header
                    .classes
                    .iter()
                    .filter(|row| matches!(row.position.base, StorageBase::Parameter(_)))
                    .count(),
            )
    {
        return Err(bad(
            span,
            "source bound does not cover every actual header input",
        ));
    }
    for (index, input) in context.inputs.iter().enumerate() {
        if function.parameters.get(input.parameter as usize).is_none()
            || input.source_parameter.is_some_and(|source| {
                context.parameters.get(source as usize) != Some(&LifeParameterKind::Lifetime)
            })
        {
            return Err(bad(
                span,
                "source bound has an absent input or lifetime anchor",
            ));
        }
        let base = StorageBase::Parameter(input.parameter);
        let present =
            match &input.position {
                CoreClosureHeaderPosition::Borrow(position) => {
                    position.base == base
                        && header.bindings.iter().any(|row| {
                            row.position == *position && row.binder == input.header_binder
                        })
                }
                CoreClosureHeaderPosition::Class(position) => {
                    position.base == base
                        && header.classes.iter().any(|row| {
                            row.position == *position && row.binder == input.header_binder
                        })
                }
            };
        if !present {
            return Err(bad(
                span,
                "source anchor differs from its exact actual header occurrence",
            ));
        }
        for prior in &context.inputs[..index] {
            if prior.position == input.position {
                return Err(bad(span, "source header anchor is duplicated"));
            }
            if let (Some(a), Some(b)) = (prior.source_parameter, input.source_parameter)
                && ((a == b) != (prior.header_binder == input.header_binder))
            {
                return Err(bad(
                    span,
                    "source lifetime/header inverse changes its partition",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HeaderKey {
    Source(StorageBinder),
    Declared(u32),
    Environment(u32, u32, u32),
}
struct Writer {
    fixed: BTreeMap<HeaderKey, u32>,
    invocation: BTreeMap<u32, u32>,
    bindings: Vec<StorageLifeBinding>,
    classes: Vec<SignatureLifeClassBinding>,
}
impl Writer {
    fn binder(&mut self, key: HeaderKey, left: &mut usize, span: Span) -> R<StorageBinder> {
        pay(
            left,
            self.fixed
                .len()
                .saturating_add(self.invocation.len())
                .saturating_add(1),
            span,
        )?;
        match key {
            HeaderKey::Source(StorageBinder::Invocation(id)) => {
                let next = self.invocation.len() as u32;
                Ok(StorageBinder::Invocation(
                    *self.invocation.entry(id).or_insert(next),
                ))
            }
            key => {
                let next = self.fixed.len() as u32;
                Ok(StorageBinder::Fixed(*self.fixed.entry(key).or_insert(next)))
            }
        }
    }
    fn finish(self) -> StorageLifeContract {
        StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: self.fixed.len() as u32,
            invocation_binder_count: self.invocation.len() as u32,
            bindings: self.bindings,
            classes: self.classes,
        }
    }
}

/// Complete role and specialized helper header formation. The actual pending
/// environment is materialized before this runs; no argument is re-evaluated.
pub(crate) fn form(
    original: &CoreFunction,
    bound: &CoreCallableParameter,
    definition: &ClosureDefinition,
    left: &mut usize,
    span: Span,
) -> R<(StorageLifeContract, StorageLifeContract)> {
    validate(original, bound)?;
    let context = bound
        .source_life
        .as_ref()
        .ok_or_else(|| bad(span, "source context is absent"))?;
    let proposed = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source P/R proposal is absent"))?;
    let actual = definition
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(span, "actual environment header is unresolved"))?;
    if !compatible(definition.kind, bound.kind)
        || definition.parameters != bound.parameters
        || definition.result != bound.result
    {
        return Err(bad(
            span,
            "actual environment changes its declared callable signature",
        ));
    }
    let old = original
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(span, "actual source header is absent"))?;
    let width = borrowed_fp::metadata_work(actual)
        .saturating_add(borrowed_fp::metadata_work(proposed))
        .saturating_add(borrowed_fp::metadata_work(old));
    pay(left, width.saturating_mul(width.saturating_add(1)), span)?;
    let lending = environment_result(bound);
    let pr = |base: StorageBase| {
        matches!(base, StorageBase::Parameter(_)) || base == StorageBase::Result && !lending
    };
    let mut images = BTreeMap::<StorageBinder, StorageBinder>::new();
    let mut inverse = BTreeMap::<StorageBinder, StorageBinder>::new();
    let mut align = |expected: StorageBinder, found: StorageBinder| -> R<()> {
        let in_domain = match found {
            StorageBinder::Fixed(index) => index < actual.fixed_binder_count,
            StorageBinder::Invocation(index) => index < actual.invocation_binder_count,
        };
        if !in_domain
            || !matches!(
                (expected, found),
                (StorageBinder::Fixed(_), StorageBinder::Fixed(_))
                    | (StorageBinder::Invocation(_), StorageBinder::Invocation(_))
            )
            || images
                .insert(expected, found)
                .is_some_and(|old| old != found)
            || inverse
                .insert(found, expected)
                .is_some_and(|old| old != expected)
        {
            return Err(bad(
                span,
                "actual P/R changes a declared Fixed/Invocation partition",
            ));
        }
        Ok(())
    };
    let mut borrows = actual.bindings.iter().filter(|row| pr(row.position.base));
    for row in &proposed.bindings {
        let found = borrows
            .next()
            .ok_or_else(|| bad(span, "actual environment omits a declared P/R borrow"))?;
        if row.position != found.position {
            return Err(bad(span, "actual P/R borrow selector differs"));
        }
        align(row.binder, found.binder)?;
    }
    let mut classes = actual.classes.iter().filter(|row| pr(row.position.base));
    for row in &proposed.classes {
        let found = classes.next().ok_or_else(|| {
            bad(
                span,
                "actual environment omits a declared intrinsic P/R class",
            )
        })?;
        if row.position != found.position {
            return Err(bad(span, "actual P/R class selector differs"));
        }
        align(row.binder, found.binder)?;
    }
    if borrows.next().is_some()
        || classes.next().is_some()
        || actual
            .static_bindings
            .iter()
            .filter(|row| pr(row.base))
            .ne(proposed.static_bindings.iter())
    {
        return Err(bad(
            span,
            "actual environment has extra or changed P/R occurrences",
        ));
    }
    // Fixed captures are already formed by the actual closure producer. Keep
    // their entire domain, including classes unused by this particular bound.
    // The finished semantic directory independently checks their real sources.
    let mut declared = BTreeMap::new();
    for fixed in &context.named_fixed {
        let Some(StorageBinder::Fixed(actual)) =
            images.get(&StorageBinder::Fixed(fixed.fixed)).copied()
        else {
            return Err(bad(
                span,
                "declared Fixed has no exact actual environment image",
            ));
        };
        declared.insert(actual, fixed.parameter);
    }
    let mut source_keys = BTreeMap::new();
    for input in &context.inputs {
        if let Some(source) = input.source_parameter
            && context
                .named_fixed
                .iter()
                .any(|fixed| fixed.parameter == source)
        {
            source_keys.insert(input.header_binder, HeaderKey::Declared(source));
        }
    }
    let source_key = |binder| {
        source_keys
            .get(&binder)
            .copied()
            .unwrap_or(HeaderKey::Source(binder))
    };
    let mut writer = Writer {
        fixed: BTreeMap::new(),
        invocation: BTreeMap::new(),
        bindings: Vec::new(),
        classes: Vec::new(),
    };
    // Preserve exact source P/R rows first. No result/capture relation is erased
    // merely because it is outside the callback's invocation signature.
    for index in 0..original.parameters.len() {
        let base = StorageBase::Parameter(index as u32);
        for row in old.bindings.iter().filter(|row| row.position.base == base) {
            let binder = writer.binder(source_key(row.binder), left, span)?;
            writer.bindings.push(StorageLifeBinding {
                position: row.position.clone(),
                binder,
            });
        }
        for row in old.classes.iter().filter(|row| row.position.base == base) {
            let binder = writer.binder(source_key(row.binder), left, span)?;
            writer.classes.push(SignatureLifeClassBinding {
                position: row.position.clone(),
                binder,
            });
        }
        if index == bound.index as usize {
            for path in role_paths(original, bound)? {
                for class in 0..actual.fixed_binder_count {
                    let key = declared.get(&class).map_or(
                        HeaderKey::Environment(bound.index, field_namespace(&path), class),
                        |source| HeaderKey::Declared(*source),
                    );
                    let binder = writer.binder(key, left, span)?;
                    writer.classes.push(SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base,
                            path: path.clone(),
                            class,
                        },
                        binder,
                    });
                }
            }
        }
    }
    for row in old
        .bindings
        .iter()
        .filter(|row| row.position.base == StorageBase::Result)
    {
        let binder = writer.binder(source_key(row.binder), left, span)?;
        writer.bindings.push(StorageLifeBinding {
            position: row.position.clone(),
            binder,
        });
    }
    for row in old
        .classes
        .iter()
        .filter(|row| row.position.base == StorageBase::Result)
    {
        let binder = writer.binder(source_key(row.binder), left, span)?;
        writer.classes.push(SignatureLifeClassBinding {
            position: row.position.clone(),
            binder,
        });
    }
    let mut header = writer.finish();
    header.bindings.sort_by(|a, b| a.position.cmp(&b.position));
    header.classes.sort_by(|a, b| a.position.cmp(&b.position));
    header.static_bindings = old.static_bindings.clone();
    // Result fields whose symbolic F has just become this actual Closure are
    // completed from its original occurrence by creator_headers at finish.
    Ok((header, actual.clone()))
}

/// Compose complete role-local headers using each actual P/R-to-environment
/// relation and its original full declared-lifetime inverse. Neither equal
/// ClosureId nor an equal local binder number establishes a cross-role link.
pub(crate) fn joint_header(
    original: &CoreFunction,
    partials: &[(u32, u32, StorageLifeContract, StorageLifeContract)],
    left: &mut usize,
    span: Span,
) -> R<StorageLifeContract> {
    let source = original
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(span, "source roles omit their actual original header"))?;
    let width = partials.iter().fold(
        borrowed_fp::metadata_work(source),
        |sum, (_, _, header, role)| {
            sum.saturating_add(borrowed_fp::metadata_work(header))
                .saturating_add(borrowed_fp::metadata_work(role))
        },
    );
    pay(
        left,
        width
            .saturating_mul(width.saturating_add(1))
            .saturating_mul(2)
            .saturating_add(original.parameters.len().saturating_mul(partials.len())),
        span,
    )?;
    let mut source_keys = BTreeMap::new();
    let mut role_keys = BTreeMap::new();
    for (parameter, generic, _, actual) in partials {
        let direct = original
            .callable_parameters
            .iter()
            .find(|bound| bound.index == *parameter && bound.generic_index == *generic);
        let owned;
        let bound = if let Some(bound) = direct {
            bound
        } else {
            let state = original
                .state_parameters
                .iter()
                .find(|state| state.index == *parameter)
                .ok_or_else(|| bad(span, "composed stored role has no actual State parameter"))?;
            let binding = state
                .bindings
                .iter()
                .find(|binding| binding.generic_index == *generic)
                .ok_or_else(|| bad(span, "composed stored role has no actual original bound"))?;
            // The caller prepaid the complete source bound before this copy.
            owned = CoreCallableParameter {
                index: *parameter,
                generic_index: *generic,
                generic_name: binding.generic_name.clone(),
                kind: binding.kind,
                closed_environment: false,
                transfer: false,
                thread_callable: false,
                parameters: binding.parameters.clone(),
                result: binding.result.clone(),
                life: binding.life.clone(),
                source_life: binding.source_life.clone(),
            };
            &owned
        };
        validate(original, bound)?;
        let context = bound
            .source_life
            .as_ref()
            .ok_or_else(|| bad(span, "composed role omits its original class inverse"))?;
        let proposed = bound
            .life
            .as_ref()
            .ok_or_else(|| bad(span, "composed role omits its full P/R"))?;
        for input in &context.inputs {
            if let Some(source) = input.source_parameter
                && context
                    .named_fixed
                    .iter()
                    .any(|fixed| fixed.parameter == source)
            {
                if source_keys
                    .insert(input.header_binder, HeaderKey::Declared(source))
                    .is_some_and(|old| old != HeaderKey::Declared(source))
                {
                    return Err(bad(
                        span,
                        "composed roles change an original declared-life anchor",
                    ));
                }
            }
        }
        // Every selection includes the full actual position. Lookup by the
        // ordinal of a type-shaped callback could merge distinct environments.
        for fixed in &context.named_fixed {
            let mut image = None;
            for row in proposed
                .bindings
                .iter()
                .filter(|row| row.binder == StorageBinder::Fixed(fixed.fixed))
            {
                let found = actual
                    .bindings
                    .iter()
                    .find(|actual| actual.position == row.position)
                    .ok_or_else(|| bad(span, "composed role loses an actual P/R borrow"))?;
                if image
                    .replace(found.binder)
                    .is_some_and(|old| old != found.binder)
                {
                    return Err(bad(span, "composed role changes its declared Fixed image"));
                }
            }
            for row in proposed
                .classes
                .iter()
                .filter(|row| row.binder == StorageBinder::Fixed(fixed.fixed))
            {
                let found = actual
                    .classes
                    .iter()
                    .find(|actual| actual.position == row.position)
                    .ok_or_else(|| bad(span, "composed role loses an actual P/R class"))?;
                if image
                    .replace(found.binder)
                    .is_some_and(|old| old != found.binder)
                {
                    return Err(bad(span, "composed role changes its declared Fixed image"));
                }
            }
            let Some(StorageBinder::Fixed(class)) = image else {
                return Err(bad(
                    span,
                    "composed declared class has no actual persistent image",
                ));
            };
            if role_keys
                .insert(
                    (*parameter, *generic, class),
                    HeaderKey::Declared(fixed.parameter),
                )
                .is_some_and(|old| old != HeaderKey::Declared(fixed.parameter))
            {
                return Err(bad(span, "composed role merges different declared classes"));
            }
        }
    }
    let source_key = |binder| {
        source_keys
            .get(&binder)
            .copied()
            .unwrap_or(HeaderKey::Source(binder))
    };
    let mut writer = Writer {
        fixed: BTreeMap::new(),
        invocation: BTreeMap::new(),
        bindings: Vec::new(),
        classes: Vec::new(),
    };
    for index in 0..original.parameters.len() {
        let base = StorageBase::Parameter(index as u32);
        for row in source
            .bindings
            .iter()
            .filter(|row| row.position.base == base)
        {
            let binder = writer.binder(source_key(row.binder), left, span)?;
            writer.bindings.push(StorageLifeBinding {
                position: row.position.clone(),
                binder,
            });
        }
        for row in source
            .classes
            .iter()
            .filter(|row| row.position.base == base)
        {
            let binder = writer.binder(source_key(row.binder), left, span)?;
            writer.classes.push(SignatureLifeClassBinding {
                position: row.position.clone(),
                binder,
            });
        }
        for (parameter, generic, header, actual) in partials
            .iter()
            .filter(|(parameter, _, _, _)| *parameter as usize == index)
        {
            for row in header.classes.iter().filter(|row| {
                row.position.base == base && row.position.class < actual.fixed_binder_count
            }) {
                // A source data class already retained above is not a newly
                // materialized callback environment occurrence.
                if source
                    .classes
                    .iter()
                    .any(|old| old.position == row.position)
                {
                    continue;
                }
                let key = role_keys
                    .get(&(*parameter, *generic, row.position.class))
                    .copied()
                    .unwrap_or(HeaderKey::Environment(
                        *parameter,
                        field_namespace(&row.position.path),
                        row.position.class,
                    ));
                let binder = writer.binder(key, left, span)?;
                if writer
                    .classes
                    .iter()
                    .any(|old| old.position == row.position && old.binder != binder)
                {
                    return Err(bad(
                        span,
                        "composed State roles conflict at one actual environment field",
                    ));
                }
                if !writer
                    .classes
                    .iter()
                    .any(|old| old.position == row.position)
                {
                    writer.classes.push(SignatureLifeClassBinding {
                        position: row.position.clone(),
                        binder,
                    });
                }
            }
        }
    }
    for row in source
        .bindings
        .iter()
        .filter(|row| row.position.base == StorageBase::Result)
    {
        let binder = writer.binder(source_key(row.binder), left, span)?;
        writer.bindings.push(StorageLifeBinding {
            position: row.position.clone(),
            binder,
        });
    }
    for row in source
        .classes
        .iter()
        .filter(|row| row.position.base == StorageBase::Result)
    {
        let binder = writer.binder(source_key(row.binder), left, span)?;
        writer.classes.push(SignatureLifeClassBinding {
            position: row.position.clone(),
            binder,
        });
    }
    let mut header = writer.finish();
    header.bindings.sort_by(|a, b| a.position.cmp(&b.position));
    header.classes.sort_by(|a, b| a.position.cmp(&b.position));
    header.static_bindings = source.static_bindings.clone();
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};

    fn address() -> Type {
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }
    }
    fn contract(fixed: u32, parameters: &[u32], captures: &[u32]) -> StorageLifeContract {
        StorageLifeContract {
            fixed_binder_count: fixed,
            invocation_binder_count: 0,
            bindings: parameters
                .iter()
                .enumerate()
                .map(|(index, binder)| StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(index as u32),
                        path: vec![],
                    },
                    binder: StorageBinder::Fixed(*binder),
                })
                .chain(
                    captures
                        .iter()
                        .enumerate()
                        .map(|(index, binder)| StorageLifeBinding {
                            position: StoragePosition {
                                base: StorageBase::Capture(index as u32),
                                path: vec![],
                            },
                            binder: StorageBinder::Fixed(*binder),
                        }),
                )
                .collect(),
            classes: vec![],
            static_bindings: vec![],
        }
    }
    fn bound(index: u32, name: &str) -> CoreCallableParameter {
        CoreCallableParameter {
            life: Some(contract(2, &[0, 1], &[])),
            source_life: Some(CoreCallableLifeContext {
                declaration: LifeDeclarationId {
                    module: ModuleId(0),
                    declaration: NodeId(0),
                    compiler: false,
                },
                identity: LifeDeclarationIdentity {
                    package: "test".into(),
                    module_path: vec![],
                    declaration_path: vec!["helper".into()],
                    generation_path: vec![],
                },
                parameters: vec![
                    LifeParameterKind::Lifetime,
                    LifeParameterKind::Lifetime,
                    LifeParameterKind::Type,
                    LifeParameterKind::Type,
                ],
                generic_parameter: 2 + index,
                bound_ordinal: 0,
                named_fixed: vec![
                    CoreClosureNamedFixed {
                        fixed: 0,
                        parameter: 0,
                    },
                    CoreClosureNamedFixed {
                        fixed: 1,
                        parameter: 1,
                    },
                ],
                inputs: vec![],
            }),
            index,
            generic_index: index,
            generic_name: name.into(),
            kind: CallableKind::FnMut,
            closed_environment: false,
            transfer: false,
            thread_callable: false,
            parameters: vec![address(), address()],
            result: Type::I32,
        }
    }
    fn function(bounds: Vec<CoreCallableParameter>) -> CoreFunction {
        CoreFunction {
            id: NodeId(0),
            name: "helper".into(),
            span: Span::default(),
            source_scope: None,
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: Some(contract(0, &[], &[])),
            call_kind: None,
            resume_bound: TaskResumeBound::Local,
            life: None,
            parameters: bounds
                .iter()
                .map(|bound| CoreParameter {
                    name: bound.generic_name.to_lowercase(),
                    ty: Type::Named(bound.generic_name.clone()),
                    mode: PassingMode::Infer,
                    span: Span::default(),
                })
                .collect(),
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            receiver: None,
            foreign: None,
            type_parameters: vec![],
            state_parameters: vec![],
            protocol_contracts: vec![],
            callable_return: None,
            callable_parameters: bounds,
            body: CoreBlock {
                statements: vec![],
                span: Span::default(),
            },
        }
    }
    fn actual(id: u32, fixed: u32, parameters: &[u32], captures: &[u32]) -> ClosureDefinition {
        ClosureDefinition {
            id: ClosureId(id),
            function: FunctionId(id),
            kind: CallableKind::FnMut,
            parameters: parameters.iter().map(|_| address()).collect(),
            captures: captures.iter().map(|_| address()).collect(),
            result: Type::I32,
            lending_captures: vec![],
            storage_life: Some(contract(fixed, parameters, captures)),
        }
    }

    #[test]
    fn declared_bound_aligns_full_actual_partition_and_preserves_extra_captures() {
        let original = function(vec![bound(0, "F")]);
        let actual = actual(0, 3, &[2, 0], &[0, 1, 2]);
        let mut left = 1_000_000;
        let (header, role) = form(
            &original,
            &original.callable_parameters[0],
            &actual,
            &mut left,
            Span::default(),
        )
        .unwrap();
        assert_eq!(header.fixed_binder_count, 3);
        assert_eq!(header.classes.len(), 3);
        assert_eq!(role, actual.storage_life.clone().unwrap());
        let wrong = self::actual(0, 3, &[2, 2], &[0, 1, 2]);
        assert!(
            form(
                &original,
                &original.callable_parameters[0],
                &wrong,
                &mut left,
                Span::default()
            )
            .is_err()
        );
    }

    #[test]
    fn multiple_roles_share_declared_images_and_keep_unrelated_environment_classes_distinct() {
        let original = function(vec![bound(0, "F"), bound(1, "G")]);
        let definitions = [
            actual(0, 3, &[2, 0], &[0, 1, 2]),
            actual(1, 2, &[0, 1], &[0, 1]),
        ];
        let mut left = 1_000_000;
        let mut partials = Vec::new();
        for (bound, definition) in original.callable_parameters.iter().zip(definitions) {
            let (header, role) =
                form(&original, bound, &definition, &mut left, Span::default()).unwrap();
            partials.push((bound.index, bound.generic_index, header, role));
        }
        let header = joint_header(&original, &partials, &mut left, Span::default()).unwrap();
        assert_eq!(header.fixed_binder_count, 3);
        let image = |parameter, class| {
            header
                .classes
                .iter()
                .find(|row| {
                    row.position.base == StorageBase::Parameter(parameter)
                        && row.position.class == class
                })
                .unwrap()
                .binder
        };
        assert_eq!(image(0, 2), image(1, 0));
        assert_eq!(image(0, 0), image(1, 1));
        assert_ne!(image(0, 1), image(1, 0));
        assert_ne!(image(0, 1), image(1, 1));
    }

    #[test]
    fn source_bound_cannot_drop_or_duplicate_an_intrinsic_or_physical_occurrence() {
        let mut bound = bound(0, "F");
        relation(&bound, Span::default()).unwrap();
        bound.life.as_mut().unwrap().bindings.pop();
        assert!(relation(&bound, Span::default()).is_err());
        bound = self::bound(0, "F");
        bound.life.as_mut().unwrap().bindings[1].position.base = StorageBase::Parameter(0);
        assert!(relation(&bound, Span::default()).is_err());
        bound = self::bound(0, "F");
        bound.source_life.as_mut().unwrap().named_fixed[1].parameter = 0;
        let original = function(vec![bound.clone()]);
        assert!(validate(&original, &bound).is_err());
    }
    #[test]
    fn stored_roles_keep_actual_field_coordinates_and_complete_environment_images() {
        let bounds = vec![bound(0, "F"), bound(1, "G")];
        let mut original = function(bounds.clone());
        original.parameters = vec![CoreParameter {
            name: "state".into(),
            ty: Type::Package {
                name: "Pair".into(),
                fields: vec![
                    ("f".into(), Type::Named("F".into())),
                    ("g".into(), Type::Named("G".into())),
                ],
            },
            mode: PassingMode::Infer,
            span: Span::default(),
        }];
        original.state_parameters = vec![CoreStateParameter {
            index: 0,
            bindings: bounds
                .iter()
                .enumerate()
                .map(|(index, bound)| CoreStateBinding {
                    generic_index: bound.generic_index,
                    generic_name: bound.generic_name.clone(),
                    kind: bound.kind,
                    parameters: bound.parameters.clone(),
                    result: bound.result.clone(),
                    fields: vec![index as u32],
                    life: bound.life.clone(),
                    source_life: bound.source_life.clone(),
                })
                .collect(),
        }];
        original.callable_parameters.clear();
        let mut left = 1_000_000;
        let bounds = state_origins::bounds(&original, &Limits::default(), &mut left)
            .unwrap()
            .into_owned();
        let definitions = [
            actual(0, 3, &[2, 0], &[0, 1, 2]),
            actual(1, 2, &[0, 1], &[0, 1]),
        ];
        let mut partials = Vec::new();
        for (bound, definition) in bounds.iter().zip(definitions) {
            let (header, role) =
                form(&original, bound, &definition, &mut left, Span::default()).unwrap();
            partials.push((bound.index, bound.generic_index, header, role));
        }
        let header = joint_header(&original, &partials, &mut left, Span::default()).unwrap();
        assert_eq!(header.fixed_binder_count, 3);
        let image = |field, class| {
            header
                .classes
                .iter()
                .find(|row| {
                    row.position.path == [LifeTypeProjection::Place(PlaceProjection::Field(field))]
                        && row.position.class == class
                })
                .unwrap()
                .binder
        };
        assert_eq!(image(0, 2), image(1, 0));
        assert_eq!(image(0, 0), image(1, 1));
        assert_ne!(image(0, 1), image(1, 0));
    }
}
