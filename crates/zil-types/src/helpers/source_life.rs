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
        || bound.kind != CallableKind::Fn
        || bound.closed_environment
        || bound.generic_index != 0
        || context.bound_ordinal != 0
        || context.parameters.iter().enumerate().any(|(index, kind)| {
            if index == context.generic_parameter as usize {
                *kind != LifeParameterKind::Type
            } else {
                *kind != LifeParameterKind::Lifetime
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
    pay(
        left,
        work.saturating_mul(copies.saturating_add(1)),
        function.span,
    )
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64
    )
}
fn tag(ty: &Type) -> Option<&StorageLifeShape> {
    let Type::Address {
        mutable: false,
        inner,
    } = ty
    else {
        return None;
    };
    let Type::BorrowedStorage { storage, life } = inner.as_ref() else {
        return None;
    };
    (matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
        && life.binder_count == 1
        && life.retained.is_empty())
    .then_some(life)
}

/// Only this context-bearing lane accepts a source bound with Fixed rows.
/// Actual receiver and argument class equations remain independent kernel work.
pub(crate) fn relation(bound: &CoreCallableParameter, span: Span) -> R<()> {
    let life = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper bound relation is absent"))?;
    if bound.source_life.is_none() {
        return borrowed_fp::relation(&bound.parameters, &bound.result, life, span);
    }
    if bound.parameters.len() != 1
        || tag(&bound.parameters[0]).is_none()
        || !scalar(&bound.result)
        || life.fixed_binder_count != 1
        || life.invocation_binder_count != 1
        || life.bindings.len() != 1
        || life.classes.len() != 1
        || life.bindings[0].position
            != (StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![],
            })
        || life.bindings[0].binder != StorageBinder::Invocation(0)
        || life.classes[0].position
            != (SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            })
        || life.classes[0].binder != StorageBinder::Fixed(0)
    {
        return Err(unsupported(
            span,
            "source helper first profile needs one shared qualified Fn input and scalar result",
        ));
    }
    borrowed_fp_qualified::relation(&bound.parameters, &bound.result, life, span)
}

/// Complete actual header and original-kind checks, also for unused templates.
/// reserve prepays the bounded all-pairs selector/inverse comparisons.
pub(crate) fn validate(function: &CoreFunction, bound: &CoreCallableParameter) -> R<()> {
    let Some(context) = &bound.source_life else {
        return Ok(());
    };
    let span = function.span;
    if function.callable_parameters.len() != 1
        || !function.type_parameters.is_empty()
        || !function.state_parameters.is_empty()
        || function.receiver.is_some()
        || function.foreign.is_some()
        || function.callable_return.is_some()
        || !scalar(&function.return_type)
        || !matches!(function.parameters.get(bound.index as usize).map(|p|&p.ty),
            Some(Type::Address {mutable:false,inner}) if matches!(inner.as_ref(),Type::Named(name) if name == &bound.generic_name))
    {
        return Err(unsupported(
            span,
            "source helper requires its actual free shared Fn parameter and scalar result",
        ));
    }
    relation(bound, span)?;
    let header = function
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper actual data header is absent"))?;
    if context.inputs.len() != header.bindings.len().saturating_add(header.classes.len())
        || context.named_fixed.len() != 1
        || context.named_fixed[0].fixed != 0
    {
        return Err(bad(
            span,
            "source helper context does not cover its actual bound/header",
        ));
    }
    let valid_source =
        |source: u32| context.parameters.get(source as usize) == Some(&LifeParameterKind::Lifetime);
    let bound_source = context.named_fixed[0].parameter;
    if !valid_source(bound_source) {
        return Err(bad(
            span,
            "source helper bound Fixed has no original full life ordinal",
        ));
    }
    for (index, input) in context.inputs.iter().enumerate() {
        let base = StorageBase::Parameter(input.parameter);
        let parameter = function
            .parameters
            .get(input.parameter as usize)
            .ok_or_else(|| bad(span, "source helper anchor input is absent"))?;
        if let Some(source) = input.source_parameter
            && !valid_source(source)
        {
            return Err(bad(
                span,
                "source helper input anchor has no original life ordinal",
            ));
        }
        match &input.position {
            CoreClosureHeaderPosition::Borrow(position) => {
                if position.base != base
                    || !position.path.is_empty()
                    || !matches!(parameter.ty, Type::Address { mutable: false, .. })
                    || !header
                        .bindings
                        .iter()
                        .any(|r| r.position == *position && r.binder == input.header_binder)
                {
                    return Err(bad(
                        span,
                        "source helper anchor differs from its actual outer header borrow",
                    ));
                }
            }
            CoreClosureHeaderPosition::Class(position) => {
                if position.base != base
                    || position.path.as_slice() != [LifeTypeProjection::Pointee]
                    || position.class != 0
                    || tag(&parameter.ty).is_none()
                    || input.source_parameter.is_none()
                    || !matches!(input.header_binder, StorageBinder::Fixed(_))
                    || !header
                        .classes
                        .iter()
                        .any(|r| r.position == *position && r.binder == input.header_binder)
                {
                    return Err(bad(
                        span,
                        "source helper anchor differs from its actual qualified data class",
                    ));
                }
            }
        }
        for prior in &context.inputs[..index] {
            if prior.position == input.position {
                return Err(bad(span, "source helper header selector is duplicated"));
            }
            if let (Some(a), Some(b)) = (prior.source_parameter, input.source_parameter)
                && ((a == b) != (prior.header_binder == input.header_binder))
            {
                return Err(bad(
                    span,
                    "source helper declared-life/header inverse is inconsistent",
                ));
            }
        }
    }
    if !context.inputs.iter().any(|input| {
        input.source_parameter == Some(bound_source)
            && matches!(input.position, CoreClosureHeaderPosition::Class(_))
    }) {
        return Err(bad(
            span,
            "source helper bound life lacks a real qualified data input",
        ));
    }
    for (index, parameter) in function.parameters.iter().enumerate() {
        if index != bound.index as usize && !(scalar(&parameter.ty) || tag(&parameter.ty).is_some())
        {
            return Err(unsupported(
                span,
                "source helper first data profile is scalar or shared qualified input",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HeaderKey {
    Source(StorageBinder),
    Environment(u32),
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
    relation(bound, span)?;
    let context = bound
        .source_life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper context is absent"))?;
    let proposed = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper bound P/R is absent"))?;
    let actual = definition.storage_life.as_ref().ok_or_else(|| {
        bad(
            span,
            "source helper actual environment has no capture contract",
        )
    })?;
    if definition.kind != CallableKind::Fn
        || definition.parameters != bound.parameters
        || definition.result != bound.result
    {
        return Err(bad(
            span,
            "source helper actual environment differs from its declared Fn signature",
        ));
    }
    borrowed_fp_qualified::capture_relation(
        &definition.parameters,
        &definition.captures,
        &definition.result,
        actual,
        span,
    )?;
    pay(
        left,
        borrowed_fp::metadata_work(actual)
            .saturating_mul(3)
            .saturating_add(borrowed_fp::metadata_work(proposed)),
        span,
    )?;
    // Align every declared P/R occurrence first. The sole Fixed is a semantic
    // qualified input, never the captured outer Address's independent Fixed.
    let actual_parameter_borrows = actual
        .bindings
        .iter()
        .filter(|r| !matches!(r.position.base, StorageBase::Capture(_)))
        .collect::<Vec<_>>();
    let actual_parameter_classes = actual
        .classes
        .iter()
        .filter(|r| !matches!(r.position.base, StorageBase::Capture(_)))
        .collect::<Vec<_>>();
    if actual_parameter_borrows.len() != proposed.bindings.len()
        || actual_parameter_classes.len() != proposed.classes.len()
        || actual_parameter_borrows[0].position != proposed.bindings[0].position
        || !matches!(
            actual_parameter_borrows[0].binder,
            StorageBinder::Invocation(_)
        )
        || actual_parameter_classes[0].position != proposed.classes[0].position
    {
        return Err(bad(
            span,
            "source helper actual P/R does not implement its declared source bound",
        ));
    }
    let StorageBinder::Fixed(environment_fixed) = actual_parameter_classes[0].binder else {
        return Err(bad(
            span,
            "source helper bound needs a persistent actual environment class",
        ));
    };
    let source = context.named_fixed[0].parameter;
    pay(left, context.inputs.len(), span)?;
    let source_binder = context
        .inputs
        .iter()
        .find(|input| {
            input.source_parameter == Some(source)
                && matches!(input.position, CoreClosureHeaderPosition::Class(_))
        })
        .ok_or_else(|| bad(span, "source helper bound has no actual data header class"))?
        .header_binder;
    let old = original
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper header is absent"))?;
    pay(
        left,
        borrowed_fp::metadata_work(old)
            .saturating_mul(3)
            .saturating_add(original.parameters.len())
            .saturating_add(actual.fixed_binder_count as usize),
        span,
    )?;
    let mut writer = Writer {
        fixed: BTreeMap::new(),
        invocation: BTreeMap::new(),
        bindings: Vec::new(),
        classes: Vec::new(),
    };
    let mut borrows = old.bindings.iter();
    let mut classes = old.classes.iter();
    for (index, parameter) in original.parameters.iter().enumerate() {
        let base = StorageBase::Parameter(index as u32);
        if matches!(parameter.ty, Type::Address { .. }) {
            let row = borrows
                .next()
                .ok_or_else(|| bad(span, "source helper header omits an actual outer borrow"))?;
            if row.position.base != base || !row.position.path.is_empty() {
                return Err(bad(
                    span,
                    "source helper outer borrow order is noncanonical",
                ));
            }
            let binder = writer.binder(HeaderKey::Source(row.binder), left, span)?;
            writer.bindings.push(StorageLifeBinding {
                position: row.position.clone(),
                binder,
            });
        }
        if index == bound.index as usize {
            for class in 0..actual.fixed_binder_count {
                let key = if class == environment_fixed {
                    HeaderKey::Source(source_binder)
                } else {
                    HeaderKey::Environment(class)
                };
                let binder = writer.binder(key, left, span)?;
                writer.classes.push(SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base,
                        path: vec![LifeTypeProjection::Pointee],
                        class,
                    },
                    binder,
                });
            }
        } else if tag(&parameter.ty).is_some() {
            let row = classes
                .next()
                .ok_or_else(|| bad(span, "source helper header omits an actual data class"))?;
            if row.position.base != base
                || row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                || row.position.class != 0
            {
                return Err(bad(
                    span,
                    "source helper data class order differs from its actual parameter",
                ));
            }
            let binder = writer.binder(HeaderKey::Source(row.binder), left, span)?;
            writer.classes.push(SignatureLifeClassBinding {
                position: row.position.clone(),
                binder,
            });
        }
    }
    if borrows.next().is_some() || classes.next().is_some() {
        return Err(bad(span, "source helper header has extra source positions"));
    }
    // Complete role is copied only after every source bound occurrence and its
    // data-header inverse has been checked. It remains untrusted Typed input.
    Ok((writer.finish(), actual.clone()))
}
