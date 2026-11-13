//! Bounded ownership composition, not stored-address permission. Actual debt,
//! capture consumption, weak-description liveness and Drop remain resource proof.
use super::*;

fn closed(text: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Analyze, text, None)]
}

pub(super) fn data(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    match ty {
        Type::Managed(payload) | Type::Desc(payload) => {
            super::super::stored_callables::resource_payload(payload, b)
        }
        _ => Ok(false),
    }
}

// Nominal identity checks an actual definition; it grants no target or loan.
fn registered(module: &TypedModule, payload: &Type, b: &mut Budget) -> CompileResult<()> {
    let Type::Record { name, .. } = payload else {
        return Err(failure("resource head has no nominal record payload"));
    };
    fee(b, module.resources.len())?;
    for definition in &module.resources {
        fee(b, name.len().saturating_add(definition.name.len()))?;
        if *name == definition.name {
            b.type_work(payload, false, "actual resource head payload comparison")?;
            b.type_work(
                &definition.payload,
                false,
                "actual resource definition comparison",
            )?;
            return if *payload == definition.payload {
                Ok(())
            } else {
                Err(failure(
                    "resource head differs from its actual definition payload",
                ))
            };
        }
    }
    Err(failure("resource head has no actual resource definition"))
}

// Complete ordinary owner values may accompany stored inputs without being a
// Fixed closure environment. This classification grants no stored target:
// actual producers, initialization, transfers and cleanup use the CFG ledgers.
pub(in crate) fn ordinary_value(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    fee(b, 1)?;
    match ty {
        Type::Address { inner, .. }
            if matches!(inner.as_ref(), Type::Managed(_) | Type::Package { .. }) =>
        {
            ordinary_value(module, inner, b)
        }
        Type::Managed(payload) | Type::Desc(payload) => match payload.as_ref() {
            Type::Record { .. } if data(ty, b)? => {
                registered(module, payload, b)?;
                Ok(true)
            }
            Type::ByteBuffer => {
                fee(b, module.resources.len())?;
                let mut count = 0usize;
                for resource in &module.resources {
                    fee(b, resource.name.len().saturating_add(19))?;
                    if resource.payload == Type::ByteBuffer
                        || resource.name == "@builtin:ByteBuffer"
                    {
                        if resource.payload != Type::ByteBuffer
                            || resource.name != "@builtin:ByteBuffer"
                            || resource.teardown.is_some()
                        {
                            return Err(failure("ordinary byte owner has a noncanonical resource"));
                        }
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(failure(
                        "ordinary byte owner lacks a unique actual resource",
                    ));
                }
                Ok(true)
            }
            Type::OwnedStore { .. } => {
                b.type_work(payload, true, "ordinary Store owner qualification snapshot")?;
                let owner = Type::Managed(payload.clone());
                super::super::ownership_store::ordinary_owned(module, &owner, b)?;
                Ok(true)
            }
            _ => Ok(false),
        },
        Type::Package { .. } => {
            super::super::resources::product::admit_owned(module, ty, b)?;
            Ok(true)
        }
        Type::ErrorUnion { .. } => {
            super::super::resources::sums::admit(module, ty, b)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn value(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<()> {
    fee(b, 1)?;
    let mut pending = vec![(ty, true, 0usize)];
    while let Some((ty, top, depth)) = pending.pop() {
        fee(b, 1)?;
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "owned Fixed value depth exceeded",
                None,
            )]);
        }
        match ty {
            Type::Managed(payload) | Type::Desc(payload) if top && data(ty, b)? => {
                registered(module, payload, b)?;
            }
            Type::Closure(_) if top => {}
            Type::Address { inner, .. } if top && matches!(inner.as_ref(), Type::Closure(_)) => {}
            Type::Managed(_)
            | Type::Desc(_)
            | Type::Package { .. }
            | Type::Closure(_)
            | Type::ByteBuffer
            | Type::OwnedStore { .. }
            | Type::Task { .. }
            | Type::TaskI32
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. } => {
                return Err(closed(
                    "owned Fixed permits only direct class-free resource heads, not nested owning/address heads",
                ));
            }
            Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. }
            | Type::BorrowedStorage { storage: inner, .. } => {
                fee(b, 1)?;
                pending.push((inner, false, depth + 1));
            }
            Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| (t, false, depth + 1)));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|t| (t, false, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields.iter().map(|t| (t, false, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([
                    (error.as_ref(), false, depth + 1),
                    (result.as_ref(), false, depth + 1),
                ]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                fee(b, parameters.len().saturating_add(1))?;
                pending.extend(parameters.iter().map(|t| (t, false, depth + 1)));
                pending.push((result, false, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

// Raw teardown metadata is independently authenticated before it can explain
// a borrowed receiver or participate in implicit environment destruction.
pub(crate) fn teardown(
    module: &TypedModule,
    resource: &zil_ir::ResourceDefinition,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(id) = resource.teardown else {
        return Ok(());
    };
    fee(b, 1)?;
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| failure("Fixed owner teardown entry is absent"))?;
    if f.id != id
        || f.foreign.is_some()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.storage_life.is_some()
        || f.parameters.len() != 1
        || f.return_type != Type::Unit
        || f.result_mode != PassingMode::Infer
    {
        return Err(failure(
            "Fixed owner teardown requires an ordinary actual Desc-to-Unit entry",
        ));
    }
    let Some(Type::Desc(actual)) = f
        .parameters
        .first()
        .and_then(|p| f.locals.get(p.0 as usize))
        .map(|l| &l.ty)
    else {
        return Err(failure(
            "Fixed owner teardown receiver is not an actual Desc",
        ));
    };
    b.type_work(actual, false, "Fixed teardown actual payload")?;
    b.type_work(
        &resource.payload,
        false,
        "Fixed teardown registered payload",
    )?;
    if **actual != resource.payload {
        return Err(failure(
            "Fixed owner teardown has a different nominal payload",
        ));
    }
    // Teardown is an implicit responsibility edge, never a public direct call,
    // function value or closure entry that could discharge a different owner.
    fee(b, module.closures.len())?;
    if module.closures.iter().any(|d| d.function == id) {
        return Err(failure("Fixed teardown is also a closure entry"));
    }
    for f in &module.functions {
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            if block.instructions.iter().any(|i| matches!(i.kind,
                InstructionKind::Call{function,..} | InstructionKind::FunctionRef { function } if function == id))
            { return Err(failure("Fixed owner teardown cannot be called as an ordinary function")); }
        }
    }
    Ok(())
}

pub(super) fn profile(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    if module.resources.is_empty() {
        return Ok(false);
    }
    fee(b, module.closures.len())?;
    let mut candidates = vec![false; module.closures.len()];
    for (index, d) in module.closures.iter().enumerate() {
        fee(b, d.captures.len().saturating_add(1))?;
        if d.storage_life.is_some() {
            for capture in &d.captures {
                candidates[index] |= super::super::stored_callables::owned_capture(capture, b)?;
            }
        }
    }
    let mut actual = false;
    fee(b, candidates.len())?;
    if candidates.contains(&true) {
        for i in module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
        {
            fee(b, 1)?;
            if let InstructionKind::MakeClosure { closure, .. } = i.kind {
                actual |= candidates.get(closure.0 as usize).copied().unwrap_or(false);
            }
        }
    }
    if !actual {
        // This is the direct Fixed-environment family, not a module-wide
        // requirement on every ordinary resource. Other actual head families
        // qualify their own producers; the resource ledger still checks debt.
        return Ok(false);
    }
    fee(b, module.resources.len())?;
    for (index, r) in module.resources.iter().enumerate() {
        b.type_work(&r.payload, true, "bounded resource definition payload")?;
        if !super::super::stored_callables::resource_payload(&r.payload, b)? {
            return Err(closed(
                "owned Fixed requires class-free scalar-field resource payloads",
            ));
        }
        let Type::Record { name, .. } = &r.payload else {
            unreachable!()
        };
        fee(b, name.len().saturating_add(r.name.len()))?;
        if *name != r.name {
            return Err(failure(
                "resource definition name differs from its actual payload",
            ));
        }
        fee(b, index)?;
        for previous in &module.resources[..index] {
            fee(b, previous.name.len().saturating_add(r.name.len()))?;
            if previous.name == r.name {
                return Err(failure("resource definition name is not unique"));
            }
        }
        teardown(module, r, b)?;
        if let Some(id) = r.teardown {
            fee(b, index)?;
            if module.resources[..index]
                .iter()
                .any(|previous| previous.teardown == Some(id))
            {
                return Err(failure(
                    "Fixed owner teardown is registered for two resources",
                ));
            }
        }
    }
    for d in &module.closures {
        fee(
            b,
            d.captures
                .len()
                .saturating_add(d.parameters.len())
                .saturating_add(1),
        )?;
        for ty in d
            .captures
            .iter()
            .chain(&d.parameters)
            .chain(std::iter::once(&d.result))
        {
            value(module, ty, b)?;
        }
    }
    for f in &module.functions {
        fee(b, f.parameters.len().saturating_add(1))?;
        if matches!(f.return_type, Type::Managed(_) | Type::Desc(_)) {
            return Err(closed(
                "owned Fixed resource result transfer is outside this profile",
            ));
        }
        for p in &f.parameters {
            let ty = &f.locals[p.0 as usize].ty;
            if matches!(ty, Type::Desc(_)) {
                fee(b, module.resources.len())?;
                if !module
                    .resources
                    .iter()
                    .any(|resource| resource.teardown == Some(f.id))
                {
                    return Err(closed(
                        "owned Fixed description parameters require the actual resource teardown entry",
                    ));
                }
            }
            // A direct Managed parameter is already the actual owning carrier.
            // The mandatory independent resource ledger proves its transfer.
        }
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            value(module, ty, b)?;
        }
    }
    Ok(true)
}

pub(super) fn describe(out: &mut Value, base: HoldRoot, b: &mut Budget) -> CompileResult<()> {
    // Weak resource identity is not a stored pointer or a parent borrow loan.
    fee(b, 1)?;
    out.held.targets.insert(Target {
        retained_port: None,
        base,
        stored: Vec::new(),
        place: Vec::new(),
        heap_parents: Vec::new(),
        heap_element: None,
    });
    Ok(())
}

pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::MakeManaged { .. }
            | InstructionKind::TakeCapture { .. }
            | InstructionKind::DescribeLocal { .. }
            | InstructionKind::DescribeCapture { .. }
            | InstructionKind::ReadDesc { .. }
            | InstructionKind::DropManaged { .. }
    )
}
fn result(
    f: &TypedFunction,
    i: &Instruction,
    expected: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    b.type_work(expected, false, "owned Fixed exact producer result")?;
    let id = i
        .result
        .ok_or_else(|| failure("resource producer has no result"))?;
    if value_type(f, id)? != expected {
        return Err(failure("resource producer result differs from actual type"));
    }
    Ok(())
}
fn payload<'a>(ty: &'a Type, managed: bool, b: &mut Budget) -> CompileResult<&'a Type> {
    let p = match (managed, ty) {
        (true, Type::Managed(p)) | (false, Type::Desc(p)) => p.as_ref(),
        _ => {
            return Err(failure(
                "resource operation requires its exact head/description kind",
            ));
        }
    };
    if !super::super::stored_callables::resource_payload(p, b)? {
        return Err(closed(
            "resource operation needs a class-free scalar-field payload",
        ));
    }
    Ok(p)
}
fn capture<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    index: u32,
) -> CompileResult<(&'a ClosureDefinition, &'a Type)> {
    let d = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .ok_or_else(|| failure("resource capture has no actual environment"))?;
    let t = d
        .captures
        .get(index as usize)
        .ok_or_else(|| failure("resource capture is absent"))?;
    Ok((d, t))
}

// Describing an actual addressed scalar resource preserves the head loan but
// grants no pointer stored inside its payload.
pub(super) fn scalar_description(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::DescribeAddress { address } = i.kind else {
        return Ok(false);
    };
    let Type::Address { inner, .. } = value_type(f, address)? else {
        return Ok(false);
    };
    let Type::Managed(payload) = inner.as_ref() else {
        return Ok(false);
    };
    if !super::super::stored_callables::resource_payload(payload, b)? {
        return Ok(false);
    }
    b.type_work(
        payload,
        false,
        "actual addressed scalar resource description",
    )?;
    result(f, i, &Type::Desc(payload.clone()), b)?;
    Ok(true)
}

pub(super) fn owning_head_description(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::DescribeAddress { address } = i.kind else {
        return Ok(false);
    };
    let Some(result) = i.result else {
        return Err(failure("head description has no result"));
    };
    let actual = value_type(f, result)?;
    let Type::Desc(payload) = actual else {
        return Ok(false);
    };
    if !matches!(payload.as_ref(), Type::ByteBuffer | Type::OwnedStore { .. }) {
        return Ok(false);
    }
    b.type_work(payload, false, "actual addressed owning head description")?;
    if !matches!(value_type(f,address)?, Type::Address { inner, .. }
        if matches!(inner.as_ref(),Type::Managed(owner) if owner == payload))
    {
        return Err(failure(
            "head description changes its actual addressed owner type",
        ));
    }
    ordinary_value(module, actual, b)
}

pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    match &i.kind {
        InstructionKind::MakeManaged { fields } => {
            let ty = value_type(
                f,
                i.result
                    .ok_or_else(|| failure("resource construction lacks result"))?,
            )?;
            let Type::Record {
                fields: declared, ..
            } = payload(ty, true, b)?
            else {
                unreachable!()
            };
            fee(b, fields.len())?;
            if fields.len() != declared.len() {
                return Err(failure(
                    "resource constructor fields differ from actual payload",
                ));
            }
            for (v, (_, expected)) in fields.iter().zip(declared) {
                if value_type(f, *v)? != expected {
                    return Err(failure(
                        "resource constructor field has the wrong actual type",
                    ));
                }
            }
        }
        InstructionKind::TakeCapture { index } => {
            let (d, ty) = capture(module, f, *index)?;
            if d.kind != CallableKind::FnOnce {
                return Err(failure(
                    "owned capture transfer requires a true FnOnce definition",
                ));
            }
            payload(ty, true, b)?;
            result(f, i, ty, b)?;
        }
        InstructionKind::TakeLocal { local }
            if matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
        {
            let ty = &f.locals[local.0 as usize].ty;
            payload(ty, true, b)?;
            result(f, i, ty, b)?;
        }
        InstructionKind::DescribeLocal { local } => {
            let ty = f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("described resource local is absent"))?;
            let p = payload(&ty.ty, true, b)?;
            let out = value_type(
                f,
                i.result
                    .ok_or_else(|| failure("resource description lacks result"))?,
            )?;
            b.type_work(p, false, "resource description payload comparison")?;
            if !matches!(out, Type::Desc(inner) if inner.as_ref()==p) {
                return Err(failure(
                    "resource description differs from actual local payload",
                ));
            }
        }
        InstructionKind::DescribeCapture { index } => {
            let (_, ty) = capture(module, f, *index)?;
            let p = payload(ty, true, b)?;
            let out = value_type(
                f,
                i.result
                    .ok_or_else(|| failure("resource description lacks result"))?,
            )?;
            b.type_work(p, false, "resource capture description payload comparison")?;
            if !matches!(out, Type::Desc(inner) if inner.as_ref()==p) {
                return Err(failure(
                    "resource description differs from actual capture payload",
                ));
            }
        }
        InstructionKind::ReadDesc { desc, index } => {
            let Type::Record { fields, .. } = payload(value_type(f, *desc)?, false, b)? else {
                return Err(failure(
                    "resource field description lacks actual Record payload",
                ));
            };
            let ty = &fields
                .get(*index as usize)
                .ok_or_else(|| failure("resource field projection is absent"))?
                .1;
            result(f, i, ty, b)?;
        }
        InstructionKind::DropManaged { local } => {
            let ty = f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("dropped resource local is absent"))?;
            payload(&ty.ty, true, b)?;
            if i.result.is_some() {
                return Err(failure("resource drop cannot produce a value"));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
