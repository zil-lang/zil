//! Auxiliary memory facts use the same immutable call-before source binding.
//! Physical pointer slots and opaque port ledgers keep their separate kernels.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    base: HoldRoot,
    through: bool,
    target_leaf: Path,
    ty: Type,
    facts: Vec<(Path, ResultFacts)>,
    tags: enum_address::Variants,
    physical: Vec<(Path, SymbolicPermission, bool)>,
    description_paths: Vec<Path>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::stored_flow) struct After(Vec<Row>);
#[derive(Default)]
pub(in crate::stored_flow) struct PreparedAfter {
    rows: Vec<(Source, Type, Payload, bool, bool, Vec<Path>)>,
    pending: bool,
}
// Pointee memory of an address captured inside an owning callback input
// is distinct from both the callback and the immutable captured pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::stored_flow) struct ProjectedPointee {
    pub(in crate::stored_flow) base: HoldRoot,
    pub(in crate::stored_flow) leaf: Path,
    ty: Type,
    pub(in crate::stored_flow) payload: Payload,
    written: bool,
}

pub(in crate::stored_flow) fn projected_inputs(
    module:&TypedModule,f:&TypedFunction,b:&mut Budget,
)->Check<Vec<ProjectedPointee>> {
    let mut rows=Vec::new();
    if f.callable_contract.is_none(){return Ok(rows);}
    b.spend(f.parameters.len())?;
    for (index,local) in f.parameters.iter().enumerate(){
        let Some(Type::Closure(id))=f.locals.get(local.0 as usize).map(|local|&local.ty) else{continue;};
        let definition=crate::closures::definition(module,*id,Some(f.span))?;
        // Only actual invocation or transfer to another concrete callback
        // argument can affect this owned input's captured pointee memory.
        let mut invokes=false;
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for op in &block.instructions {
                invokes|=matches!(&op.kind,InstructionKind::CallClosureOnce{callee,..}
                    if f.values.get(callee.0 as usize)==Some(&Type::Closure(*id)));
                if let InstructionKind::Call{arguments,..}=&op.kind {
                    b.spend(arguments.len())?;
                    invokes|=arguments.iter().any(|value|f.values.get(value.0 as usize)==Some(&Type::Closure(*id)));
                }
            }
        }
        if !invokes{continue;}
        b.spend(definition.captures.len())?;
        for (capture,ty) in definition.captures.iter().enumerate(){
            let Type::Address{mutable:true,inner}=ty else{continue;};
            if !transport_type(module,inner,b)?{continue;}
            type_work(inner,&b.limits.clone(),b)?;
            b.spend(2)?;
            rows.push(ProjectedPointee{
                base:HoldRoot::Parameter(index as u32),
                leaf:vec![PlaceProjection::Field(capture as u32)],
                ty:inner.as_ref().clone(),payload:Payload::default(),written:false,
            });
        }
    }
    Ok(rows)
}

pub(in crate::stored_flow) fn projected_work(rows:&[ProjectedPointee],b:&mut Budget)->Check<()> {
    b.spend(rows.len())?;
    for row in rows{
        b.spend(row.leaf.len())?;
        type_work(&row.ty,&b.limits.clone(),b)?;
        payload_work(&row.payload,b)?;
    }
    Ok(())
}

pub(in crate::stored_flow) fn join_projected(to:&mut [ProjectedPointee],from:&[ProjectedPointee],b:&mut Budget)->Check<()> {
    b.spend(to.len().saturating_add(from.len()))?;
    if to.len()!=from.len(){return Err(err("projected pointee directories differ across actual CFG flow"));}
    for (left,right) in to.iter_mut().zip(from){
        b.spend(left.leaf.len().saturating_add(right.leaf.len()))?;
        if left.base!=right.base||left.leaf!=right.leaf{return Err(err("projected pointee changes its exact captured input"));}
        join_payload(&mut left.payload,&right.payload,b)?;
        left.written&=right.written;
    }
    Ok(())
}

pub(in crate::stored_flow) fn projected_memory<'a>(rows:&'a[ProjectedPointee],target:&Source,b:&mut Budget)->Check<Option<&'a Payload>> {
    if rows.is_empty()||target.leaf.is_empty()||target.retained_port.is_some()
        ||target.heap_element.is_some()||!target.heap_parents.is_empty(){return Ok(None);}
    b.spend(rows.len().saturating_mul(target.leaf.len().saturating_add(1)))?;
    Ok(rows.iter().find(|row|row.written&&row.base==target.base&&row.leaf==target.leaf).map(|row|&row.payload))
}

fn projected_target<'a>(rows:&'a mut[ProjectedPointee],target:&Source,ty:&Type,b:&mut Budget)->Check<&'a mut ProjectedPointee>{
    b.spend(rows.len().saturating_mul(target.leaf.len().saturating_add(1)))?;
    let row=rows.iter_mut().find(|row|row.base==target.base&&row.leaf==target.leaf)
        .ok_or_else(||err("auxiliary after-state has no exact mutable captured input pointee"))?;
    type_work(&row.ty,&b.limits.clone(),b)?;
    type_work(ty,&b.limits.clone(),b)?;
    if row.ty!=*ty{return Err(err("auxiliary after-state changes its exact mutable captured pointee type"));}
    Ok(row)
}

pub(in crate::stored_flow) fn after_work(after: &After, b: &mut Budget) -> Check<()> {
    b.spend(after.0.len())?;
    for row in &after.0 {
        b.spend(row.target_leaf.len())?;
        type_work(&row.ty, &b.limits.clone(), b)?;
        row.tags.work(b)?;
        for path in &row.description_paths {
            b.spend(path.len().saturating_add(1))?;
        }
        for (path, permission, _) in &row.physical {
            b.spend(path.len())?;
            source_work(&permission.sources, b)?;
            source_work(&permission.parents, b)?;
            b.spend(1)?;
        }
        for (path, facts) in &row.facts {
            b.spend(path.len())?;
            result_work(facts, b)?;
        }
    }
    Ok(())
}
fn auxiliary_type(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![(ty, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("auxiliary after-state type depth exceeded"));
        }
        if option_address::signature(ty)
            || matches!(
                ty,
                Type::Desc(_)
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
                    | Type::Managed(_)
                    | Type::Task { .. }
            )
            || matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Dyn{..}|Type::OwnedDyn{..}))
        {
            return Ok(true);
        }
        match ty {
            Type::BorrowedStorage { storage, .. } => pending.push((storage, depth + 1)),
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Enum { .. } | Type::ErrorUnion { .. } => return Ok(true),
            Type::Array { element, .. } => pending.push((element, depth + 1)),
            Type::Closure(id) => {
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .filter(|d| d.id == *id)
                    .ok_or_else(|| err("auxiliary after-state closure identity is absent"))?;
                b.spend(definition.captures.len())?;
                pending.extend(definition.captures.iter().map(|ty| (ty, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(false)
}
/// Physical pointer and descriptor fields. A root Address is a separate
/// reference cell and never obtains a duplicate after-state row here.
fn physical_paths(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Vec<(Path, bool)>> {
    let mut pending = vec![(ty, Path::new(), 0usize)];
    let mut out = vec![];
    while let Some((ty, path, depth)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("physical after-state directory depth exceeded"));
        }
        match ty {
            Type::Address { mutable, .. } => {
                if !path.is_empty() {
                    b.spend(1)?;
                    out.push((path, *mutable));
                }
            }
            Type::Desc(_) => {
                if !path.is_empty() {
                    b.spend(1)?;
                    out.push((path, true));
                }
            }
            Type::BorrowedStorage { life, .. } => {
                for slot in &life.slots {
                    b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                    let mut key = path.clone();
                    key.extend(&slot.path);
                    if !key.is_empty() {
                        out.push((key, slot.mode == StoredBorrowMode::ExclusiveAddress));
                    }
                }
            }
            Type::Closure(id) => {
                for (key, ty) in returned_environment::leaves(module, *id, b)? {
                    if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
                        b.spend(path.len().saturating_add(key.len()).saturating_add(1))?;
                        let mut full = path.clone();
                        full.extend(key);
                        out.push((
                            full,
                            matches!(ty, Type::Address { mutable: true, .. } | Type::Desc(_)),
                        ));
                    }
                }
            }
            _ => {
                let mut fields = vec![];
                match ty {
                    Type::Tuple(children) => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Record {
                        fields: children, ..
                    }
                    | Type::Package {
                        fields: children, ..
                    } => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, (_, t))| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Enum { variants, .. } => {
                        for (v, (_, children)) in variants.iter().enumerate() {
                            b.spend(children.len())?;
                            fields.extend(children.iter().enumerate().map(|(i, t)| {
                                (
                                    PlaceProjection::Variant(VariantProjection {
                                        variant: v as u32,
                                        index: i as u32,
                                    }),
                                    t,
                                )
                            }));
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        b.spend(2)?;
                        fields.extend([
                            (PlaceProjection::ErrorArm(true), error.as_ref()),
                            (PlaceProjection::ErrorArm(false), result.as_ref()),
                        ]);
                    }
                    Type::Array { element, length } => {
                        b.spend(*length as usize)?;
                        fields.extend(
                            (0..*length).map(|i| (PlaceProjection::Field(i), element.as_ref())),
                        );
                    }
                    _ => {}
                }
                b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (step, child) in fields {
                    let mut key = path.clone();
                    key.push(step);
                    pending.push((child, key, depth + 1));
                }
            }
        }
    }
    Ok(out)
}
fn transport_type(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(auxiliary_type(module, ty, b)? || !physical_paths(module, ty, b)?.is_empty())
}

// A complete Store-only product has inline scalars and separately managed
// Store heads. The actual prefix, mutation, owner and loans of those heads
// are checked by the Store kernels. It has no inline auxiliary channel.
fn store_product_without_auxiliary(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    let mut owns_store = false;
    while let Some(field) = pending.pop() {
        b.spend(1)?;
        match field {
            Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }) => {
                owns_store = true;
            }
            field if field.is_b0_scalar() => {}
            _ => return Ok(false),
        }
    }
    if owns_store {
        dynamic_store::check_store_element(module, ty, None, b)?;
    }
    Ok(owns_store)
}
fn physical_at<'a>(
    module: &TypedModule,
    ty: &Type,
    payload: &'a Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<&'a Permission>> {
    if let Some(permission) = leaf(payload, path, b)? {
        if !permission.sources.is_empty() {
            return Ok(Some(permission));
        }
    }
    if matches!(
        call_results::destination(module, ty, path, b)?,
        Type::Desc(_)
    ) {
        return Ok(facts(payload, path, b)?.map(|facts| &facts.description));
    }
    Ok(None)
}
pub(in crate::stored_flow) fn after_sites(
    module: &TypedModule,
    callee: &TypedFunction,
    b: &mut Budget,
) -> Check<Vec<(u32, Path, bool)>> {
    let mut rows = vec![];
    b.spend(callee.parameters.len())?;
    for (index, local) in callee.parameters.iter().enumerate() {
        if let Type::Address {
            mutable: true,
            inner,
        } = &callee.locals[local.0 as usize].ty
        {
            for (path, exclusive) in physical_paths(module, inner, b)? {
                b.spend(1)?;
                rows.push((index as u32, path, exclusive));
            }
        }
    }
    for row in projected_inputs(module,callee,b)? {
        let HoldRoot::Parameter(index)=row.base else{unreachable!();};
        for (path,exclusive) in physical_paths(module,&row.ty,b)? {
            b.spend(row.leaf.len().saturating_add(path.len()).saturating_add(1))?;
            let mut key=row.leaf.clone();
            key.extend(path);
            rows.push((index,key,exclusive));
        }
    }
    if let Some(definition) = callee
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
    {
        b.spend(definition.captures.len())?;
        for (index, ty) in definition.captures.iter().enumerate() {
            let ty = match ty {
                Type::Address {
                    mutable: true,
                    inner,
                } => inner.as_ref(),
                _ if definition.kind == CallableKind::FnMut => ty,
                _ => continue,
            };
            for (path, exclusive) in physical_paths(module, ty, b)? {
                b.spend(path.len().saturating_add(2))?;
                let mut key = vec![PlaceProjection::Field(index as u32)];
                key.extend(path);
                rows.push((u32::MAX, key, exclusive));
            }
        }
    }
    Ok(rows)
}
fn active_row(
    module: &TypedModule,
    row: &Row,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<bool> {
    row.tags.work(b)?;
    let mut payload = Payload {
        enum_variants: row.tags.clone(),
        ..Default::default()
    };
    for (key, facts) in &row.facts {
        facts.option.work(b)?;
        if key.is_empty() {
            payload.option_variants = facts.option.clone();
        } else {
            put(
                &mut payload,
                key,
                &FieldFacts {
                    option: facts.option.clone(),
                    ..Default::default()
                },
                b,
            )?;
        }
    }
    call_results::path_active(module, &row.ty, &payload, path, b)
}
fn channels_known(expected: &ResultFacts, actual: Option<&ResultFacts>) -> bool {
    let Some(actual) = actual else {
        return !(!expected.address.sources.is_empty()
            || !expected.description.sources.is_empty()
            || !expected.views.is_empty()
            || expected.option.mask() != 0
            || expected.creation.is_some());
    };
    (expected.address.sources.is_empty() || !actual.address.sources.is_empty())
        && (expected.description.sources.is_empty() || !actual.description.sources.is_empty())
        && (expected.views.is_empty() || !actual.views.is_empty())
        && (expected.option.mask() == 0 || actual.option.mask() != 0)
        && (expected.creation.is_none() || actual.creation.is_some())
}
pub(in crate::stored_flow) fn summarize_after(
    module: &TypedModule,
    f: &TypedFunction,
    entry: &Frame,
    state: &Frame,
    solution: &Solution,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    let mut inputs = vec![];
    for (index, local) in f.parameters.iter().enumerate() {
        b.spend(1)?;
        if let Type::Address {
            mutable: true,
            inner,
        } = &f.locals[local.0 as usize].ty
        {
            inputs.push((
                HoldRoot::Parameter(index as u32),
                true,
                &[][..],
                inner.as_ref(),
                memory_holder(state, HoldRoot::Parameter(index as u32), b)?,
            ));
        }
    }
    if let Some(definition) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
    {
        for (index, ty) in definition.captures.iter().enumerate() {
            b.spend(1)?;
            match ty {
                Type::Address {
                    mutable: true,
                    inner,
                } => inputs.push((
                    HoldRoot::Capture(index as u32),
                    true,
                    &[][..],
                    inner.as_ref(),
                    memory_holder(state, HoldRoot::Capture(index as u32), b)?,
                )),
                _ if definition.kind == CallableKind::FnMut => inputs.push((
                    HoldRoot::Capture(index as u32),
                    false,
                    &[][..],
                    ty,
                    state.captures.get(index),
                )),
                _ => {}
            }
        }
    }
    b.spend(state.projected_pointees.len())?;
    for row in &state.projected_pointees{
        if !row.written {
            summary.complete=false;
            continue;
        }
        inputs.push((row.base,true,row.leaf.as_slice(),&row.ty,Some(&row.payload)));
    }
    for (base, through, target_leaf, ty, payload) in inputs {
        if !transport_type(module, ty, b)? {
            continue;
        }
        let payload = payload.ok_or_else(|| err("auxiliary after-state input holder is absent"))?;
        let (facts, known) =
            super::results::snapshot_rows(module, f, ty, payload, solution, true, b)?;
        summary.complete &= known;
        let initial = if !target_leaf.is_empty(){None}else if through {
            memory_holder(entry, base, b)?
        } else {
            match base {
                HoldRoot::Capture(index) => entry.captures.get(index as usize),
                _ => None,
            }
        };
        let mut physical = vec![];
        for (path, _) in physical_paths(module, ty, b)? {
            if !call_results::path_active(module, ty, payload, &path, b)? {
                continue;
            }
            let Some(permission) = physical_at(module, ty, payload, &path, b)? else {
                summary.complete = false;
                continue;
            };
            if permission.sources.is_empty() {
                summary.complete = false;
                continue;
            }
            let stable = match initial {
                Some(initial) => physical_at(module, ty, initial, &path, b)?
                    .is_some_and(|previous| previous == permission),
                None => false,
            };
            let symbolic = super::results::symbolic(permission, f, solution, b)?;
            // Reuse the full final data/source/parent boundary for each
            // physical field, without turning its parents into targets.
            b.spend(path.len().saturating_add(1))?;
            source_work(&symbolic.sources, b)?;
            source_work(&symbolic.parents, b)?;
            let check = ResultFacts {
                address: symbolic.clone(),
                ..Default::default()
            };
            super::results::validate_after_target(
                module,
                f,
                base,
                through,
                target_leaf,
                ty,
                &[(path.clone(), check)],
                b,
            )?;
            b.spend(1)?;
            physical.push((path, symbolic, stable));
        }
        if option_address::signature(ty) {
            // The sealed Option constructor/read lane records its actual tag
            // in the dedicated Option ledger. Ordinary Option modules do not
            // run the Task tag-tree producer; an empty Enum ledger therefore
            // cannot stand in for, or veto, the real Option evidence.
            payload.option_variants.work(b)?;
            summary.complete &= payload.option_variants.mask() != 0;
        } else if matches!(ty, Type::ErrorUnion { .. } | Type::Enum { .. })
            || matches!(ty,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Enum{..}))
        {
            summary.complete &= payload.enum_variants.grounded(ty, b)?;
        }
        super::results::validate_after_target(module, f, base, through, target_leaf, ty, &facts, b)?;
        let mut description_paths = vec![];
        for (path, _) in &facts {
            if matches!(
                call_results::destination(module, ty, path, b)?,
                Type::Desc(_)
            ) {
                b.spend(path.len().saturating_add(1))?;
                description_paths.push(path.clone());
            }
        }

        b.spend(summary.value_after.0.len())?;
        if let Some(old) = summary
            .value_after
            .0
            .iter_mut()
            .find(|row| row.base == base && row.target_leaf == target_leaf)
        {
            type_work(&old.ty, &b.limits.clone(), b)?;
            type_work(ty, &b.limits.clone(), b)?;
            if old.through != through || old.ty != *ty {
                return Err(err("auxiliary after-state changes its complete input type"));
            }
            for (path, _, _) in &old.physical {
                b.spend(physical.len().saturating_mul(path.len().saturating_add(1)))?;
                if call_results::path_active(module, ty, payload, path, b)?
                    && !physical.iter().any(|(key, _, _)| key == path)
                {
                    summary.complete = false;
                }
            }
            for (path, permission, stable) in physical {
                b.spend(
                    old.physical
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                if let Some((_, previous, unchanged)) =
                    old.physical.iter_mut().find(|(key, _, _)| *key == path)
                {
                    super::results::join_symbolic(previous, &permission, b)?;
                    *unchanged &= stable;
                } else {
                    if active_row(module, old, &path, b)? {
                        summary.complete = false;
                    }
                    b.spend(path.len().saturating_add(1))?;
                    source_work(&permission.sources, b)?;
                    source_work(&permission.parents, b)?;
                    old.physical.push((path, permission, stable));
                }
            }
            // Every exit must supply each channel; a union is not evidence
            // that a missing Return field was initialized.
            for (path, row) in &old.facts {
                b.spend(facts.len().saturating_mul(path.len().saturating_add(1)))?;
                if call_results::path_active(module, ty, payload, path, b)?
                    && !channels_known(
                        row,
                        facts
                            .iter()
                            .find(|(key, _)| key == path)
                            .map(|(_, row)| row),
                    )
                {
                    summary.complete = false;
                }
            }
            for (path, row) in facts {
                b.spend(old.facts.len().saturating_mul(path.len().saturating_add(1)))?;
                if let Some((_, prior)) = old.facts.iter_mut().find(|(key, _)| *key == path) {
                    if !channels_known(&row, Some(prior)) {
                        summary.complete = false;
                    }
                    super::results::join_result(prior, &row, b)?;
                } else {
                    if active_row(module, old, &path, b)?
                        && (row.option.mask() != 0
                            || !row.address.sources.is_empty()
                            || !row.description.sources.is_empty()
                            || !row.views.is_empty())
                    {
                        summary.complete = false;
                    }
                    b.spend(path.len().saturating_add(1))?;
                    result_work(&row, b)?;
                    old.facts.push((path, row));
                }
            }
            for path in description_paths {
                b.spend(
                    old.description_paths
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                if !old.description_paths.contains(&path) {
                    b.spend(1)?;
                    old.description_paths.push(path);
                }
            }
            old.tags.join(&payload.enum_variants, b)?;
        } else {
            type_work(ty, &b.limits.clone(), b)?;
            payload.enum_variants.work(b)?;
            b.spend(1)?;
            summary.value_after.0.push(Row {
                base,
                through,
                target_leaf:target_leaf.to_vec(),
                ty: ty.clone(),
                facts,
                tags: payload.enum_variants.clone(),
                physical,
                description_paths,
            });
        }
    }
    Ok(())
}
/// One directory for actual parameter data after a call. View heads remain
/// ancestry, while both real descriptor channels name the descriptor target.
pub(in crate::stored_flow) fn after_sources(
    _module: &TypedModule,
    after: &After,
    b: &mut Budget,
) -> Check<Vec<(HoldRoot, Path, Vec<Source>)>> {
    let mut out: Vec<(HoldRoot, Path, Vec<Source>)> = vec![];
    for row in &after.0 {
        let base = row.base;
        let mut append = |path: &Path, sources: &[Source], b: &mut Budget| -> Check<()> {
            b.spend(row.target_leaf.len().saturating_add(path.len()))?;
            let mut full=row.target_leaf.clone();
            full.extend(path);
            let path=&full;
            b.spend(out.len().saturating_mul(path.len().saturating_add(1)))?;
            if let Some((_, _, known)) = out
                .iter_mut()
                .find(|(parameter, key, _)| *parameter == base && key == path)
            {
                add_sources(known, sources, b)?;
            } else {
                b.spend(path.len().saturating_add(1))?;
                source_work(sources, b)?;
                out.push((base, path.clone(), sources.to_vec()));
            }
            Ok(())
        };
        for (path, permission, _) in &row.physical {
            append(path, &permission.sources, b)?;
        }
        for (path, facts) in &row.facts {
            b.spend(
                row.description_paths
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            if row.description_paths.contains(path) {
                append(path, &facts.address.sources, b)?;
                append(path, &facts.description.sources, b)?;
            }
        }
    }
    Ok(out)
}
pub(in crate::stored_flow) fn prepare_after(
    before: &call_results::Before<'_>,
    _callee: &TypedFunction,
    summary: &Summary,
    i: &Instruction,
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<PreparedAfter> {
    let mut out = PreparedAfter::default();
    if !summary.complete {
        b.spend(_callee.parameters.len())?;
        for local in &_callee.parameters {
            if let Type::Address {
                mutable: true,
                inner,
            } = &_callee.locals[local.0 as usize].ty
            {
                if transport_type(before.module(), inner, b)? {
                    out.pending = true;
                    break;
                }
            }
        }
        if let Some(definition) = _callee
            .environment
            .and_then(|id| before.module().closures.get(id.0 as usize))
        {
            b.spend(definition.captures.len())?;
            for ty in &definition.captures {
                let target = match ty {
                    Type::Address {
                        mutable: true,
                        inner,
                    } => Some(inner.as_ref()),
                    _ if definition.kind == CallableKind::FnMut => Some(ty),
                    _ => None,
                };
                if let Some(ty) = target {
                    if transport_type(before.module(), ty, b)? {
                        out.pending = true;
                        break;
                    }
                }
            }
        }
        if !out.pending && !projected_inputs(before.module(),_callee,b)?.is_empty(){out.pending=true;}
        return Ok(out);
    }
    for row in &summary.value_after.0 {
        // A consumed environment has retired as an occupant. Its actual
        // mutable pointee effects still publish, but its own slots do not.
        if !row.through && matches!(row.base, HoldRoot::Capture(_)) && before.consumed_environment()
        {
            continue;
        }
        let mut value = Payload::default();
        let (tags, known) = before.enum_result(&row.tags, b)?;
        if !known {
            out.pending = true;
            continue;
        }
        value.enum_variants = tags;
        let selected = Summary {
            complete: true,
            field_result_facts: {
                for (path, facts) in &row.facts {
                    b.spend(path.len())?;
                    result_work(facts, b)?;
                }
                row.facts.clone()
            },
            ..Default::default()
        };
        if !before.field_results(&selected, &row.ty, i, sites, loans, &mut value, b)? {
            out.pending = true;
            continue;
        }
        let mut physical_known = true;
        for (path, permission, stable) in &row.physical {
            if !call_results::path_active(before.module(), &row.ty, &value, path, b)? {
                continue;
            }
            let description = matches!(
                call_results::destination(before.module(), &row.ty, path, b)?,
                Type::Desc(_)
            );
            let (mut actual, known) =
                before.physical_permission(permission, description, i, sites, loans, b)?;
            physical_known &= known && !actual.sources.is_empty();
            if !known || actual.sources.is_empty() {
                continue;
            }
            if !*stable {
                let (parameter, site) = match row.base {
                    HoldRoot::Parameter(index) => {
                        b.spend(row.target_leaf.len().saturating_add(path.len()))?;
                        let mut selected=row.target_leaf.clone();
                        selected.extend(path);
                        (index,selected)
                    },
                    HoldRoot::Capture(index) => {
                        b.spend(path.len().saturating_add(1))?;
                        let mut key = vec![PlaceProjection::Field(index)];
                        key.extend(path);
                        (u32::MAX, key)
                    }
                    _ => return Err(err("physical after-state has no persistent input")),
                };
                actual = retain_written_slot(&actual, i, parameter, &site, sites, loans, b)?;
            }
            add_leaf(&mut value, path, &actual, b)?;
        }
        if !physical_known {
            out.pending = true;
            continue;
        }
        let targets = before.after_targets(row.base, row.through, &row.target_leaf, b)?;
        if targets.sources.is_empty() {
            out.pending = true;
            continue;
        }
        let unique = targets.sources.len() == 1;
        let complete_physical = physical_paths(before.module(), &row.ty, b)?
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>();
        for target in &targets.sources {
            if target.heap_element.is_some()
                && target.retained_port.is_none()
                && target.inline.is_empty()
                && target.leaf.is_empty()
                && target.base != HoldRoot::Static
                && unique && row.through
                && row.facts.is_empty()
                && row.physical.is_empty()
                && row.tags == enum_address::Variants::default()
                && matches!(row.ty, Type::Package { .. })
                && store_product_without_auxiliary(before.module(), &row.ty, b)?
            {
                // The whole, uniquely selected heap element has no inline
                // auxiliary memory to publish. Its actual Store updates and
                // access proofs still execute in their independent kernels.
                continue;
            }
            if target.retained_port.is_some()
                || target.heap_element.is_some()
                || !target.heap_parents.is_empty()
                || (!target.leaf.is_empty() && (!target.inline.is_empty()||!matches!(target.base,HoldRoot::Parameter(_))))
                || target.base == HoldRoot::Static
            {
                return Err(err(
                    "auxiliary after-state lacks an exact mutable inline target",
                ));
            }
            source_work(std::slice::from_ref(target), b)?;
            type_work(&row.ty, &b.limits.clone(), b)?;
            payload_work(&value, b)?;
            b.spend(1)?;
            out.rows.push((
                target.clone(),
                row.ty.clone(),
                value.clone(),
                unique,
                !matches!(row.ty, Type::Address { .. }),
                {
                    b.spend(complete_physical.iter().fold(0usize, |n, path| {
                        n.saturating_add(path.len().saturating_add(1))
                    }))?;
                    complete_physical.clone()
                },
            ));
        }
    }
    Ok(out)
}
pub(in crate::stored_flow) fn publish_after(
    state: &mut Frame,
    prepared: PreparedAfter,
    b: &mut Budget,
) -> Check<()> {
    state.pending_external_update |= prepared.pending;
    for (target, _ty, value, strong, root_address, physical_domain) in prepared.rows {
        let holder = if target.leaf.is_empty(){
            memory_holder_mut(state,target.base,b)?.ok_or_else(||err("auxiliary after-state target holder is absent"))?
        }else{
            let row=projected_target(&mut state.projected_pointees,&target,&_ty,b)?;
            row.written|=strong;
            &mut row.payload
        };
        if strong {
            // A known new inactive arm owns no old leaf. Remove only the
            // complete declared physical slots carried by this snapshot;
            // independent opaque ledgers and private pointee shadows stay.
            b.spend(
                holder
                    .leaves
                    .len()
                    .saturating_mul(physical_domain.iter().fold(0usize, |n, path| {
                        n.saturating_add(
                            path.len()
                                .saturating_add(target.inline.len())
                                .saturating_add(1),
                        )
                    })),
            )?;
            holder.leaves.retain(
                |(path, _)| {
                    path.strip_prefix(target.inline.as_slice())
                        .is_none_or(|relative| {
                            !physical_domain
                                .iter()
                                .any(|slot| slot.as_slice() == relative)
                        })
                },
                b,
            )?;
        }
        for (path, permission) in &value.leaves {
            b.spend(target.inline.len().saturating_add(path.len()))?;
            let mut full = target.inline.clone();
            full.extend(path);
            if strong && let Some(index) = holder.leaves.position(&full, b)? {
                permission_work(permission, b)?;
                *holder.leaves.permission_mut(index) = permission.clone();
            } else {
                add_leaf(holder, &full, permission, b)?;
            }
        }
        if strong {
            b.spend(
                holder
                    .field_facts
                    .iter()
                    .map(|(path, _)| {
                        path.len()
                            .saturating_add(target.inline.len())
                            .saturating_add(1)
                    })
                    .sum(),
            )?;
            holder
                .field_facts
                .retain(|(path, _)| !path.starts_with(&target.inline));
            if target.inline.is_empty() {
                holder.store_description = Permission::default();
                holder.dyn_views.clear();
                holder.option_variants = option_address::Variants::default();
                holder.creation_complete = None;
            }
        }
        if target.inline.is_empty() {
            if root_address {
                if strong {
                    holder.address = Permission::default();
                }
                join_permission(&mut holder.address, &value.address, b)?;
            }
            join_permission(&mut holder.store_description, &value.store_description, b)?;
            for view in &value.dyn_views {
                dyn_ports::views_work(&holder.dyn_views, b)?;
                permission_work(&view.head, b)?;
                if !holder.dyn_views.contains(view) {
                    b.spend(1)?;
                    holder.dyn_views.push(view.clone());
                }
            }
            holder.option_variants.join(&value.option_variants, b)?;
            if let Some(creation) = value.creation_complete {
                holder.creation_complete =
                    Some(holder.creation_complete.unwrap_or(false) || creation);
            }
            if strong {
                value.enum_variants.work(b)?;
                holder.enum_variants = value.enum_variants.clone();
            } else {
                holder.enum_variants.join(&value.enum_variants, b)?;
            }
            for (path, facts) in &value.field_facts {
                put(holder, path, facts, b)?;
            }
        } else {
            permission_work(&value.address, b)?;
            permission_work(&value.store_description, b)?;
            dyn_ports::views_work(&value.dyn_views, b)?;
            value.option_variants.work(b)?;
            let root = FieldFacts {
                address: value.address.clone(),
                description: value.store_description.clone(),
                views: value.dyn_views.clone(),
                option: value.option_variants.clone(),
                creation: value.creation_complete,
            };
            root.work(b)?;
            put(holder, &target.inline, &root, b)?;
            for (path, facts) in &value.field_facts {
                b.spend(target.inline.len().saturating_add(path.len()))?;
                let mut full = target.inline.clone();
                full.extend(path);
                put(holder, &full, facts, b)?;
            }
            if strong {
                holder
                    .enum_variants
                    .replace_child(&target.inline, &value.enum_variants, b)?;
            } else {
                let mut tags = holder.enum_variants.project(&target.inline, b)?;
                tags.join(&value.enum_variants, b)?;
                holder
                    .enum_variants
                    .replace_child(&target.inline, &tags, b)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(base: HoldRoot, path: Path) -> Source {
        Source {
            base,
            inline: path,
            leaf: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    fn permission(index: u32) -> Permission {
        Permission {
            sources: vec![source(HoldRoot::Parameter(index), vec![])],
            loans: vec![],
        }
    }
    #[test]
    fn auxiliary_after_strong_subtree_and_weak_alternative_preserve_physical_and_sibling_facts() {
        let limits = Limits::default();
        for strong in [false, true] {
            let mut b = Budget::new(&limits);
            let mut holder = Payload::default();
            for field in 0..2 {
                put(
                    &mut holder,
                    &[PlaceProjection::Field(field)],
                    &FieldFacts {
                        description: permission(field),
                        ..Default::default()
                    },
                    &mut b,
                )
                .unwrap();
            }
            add_leaf(
                &mut holder,
                &[PlaceProjection::Field(0)],
                &permission(7),
                &mut b,
            )
            .unwrap();
            let value = Payload {
                store_description: permission(2),
                creation_complete: Some(true),
                ..Default::default()
            };
            let mut frame = Frame {
                locals: vec![holder],
                ..Default::default()
            };
            let update = PreparedAfter {
                rows: vec![(
                    source(HoldRoot::Local(LocalId(0)), vec![PlaceProjection::Field(0)]),
                    Type::I32,
                    value,
                    strong,
                    true,
                    vec![],
                )],
                pending: false,
            };
            publish_after(&mut frame, update, &mut b).unwrap();
            let holder = &frame.locals[0];
            let updated = facts(holder, &[PlaceProjection::Field(0)], &mut b)
                .unwrap()
                .unwrap();
            assert_eq!(
                updated.description.sources.len(),
                if strong { 1 } else { 2 }
            );
            assert!(
                updated
                    .description
                    .sources
                    .contains(&source(HoldRoot::Parameter(2), vec![]))
            );
            assert_eq!(
                facts(holder, &[PlaceProjection::Field(1)], &mut b)
                    .unwrap()
                    .unwrap()
                    .description
                    .sources,
                permission(1).sources
            );
            assert_eq!(
                leaf(holder, &[PlaceProjection::Field(0)], &mut b)
                    .unwrap()
                    .unwrap()
                    .sources,
                permission(7).sources,
                "physical slots have a separate actual after kernel"
            );
        }
    }
    #[test]
    fn pointee_root_after_replaces_beta_facts_and_preserves_alpha_value() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let mut holder = Payload {
            address: permission(0),
            store_description: permission(1),
            creation_complete: Some(true),
            ..Default::default()
        };
        put(
            &mut holder,
            &[PlaceProjection::Field(0)],
            &FieldFacts {
                description: permission(2),
                ..Default::default()
            },
            &mut b,
        )
        .unwrap();
        let mut value = Payload::default();
        put(
            &mut value,
            &[PlaceProjection::Field(0)],
            &FieldFacts {
                description: permission(3),
                ..Default::default()
            },
            &mut b,
        )
        .unwrap();
        let alpha = Payload {
            address: permission(7),
            store_description: permission(8),
            creation_complete: Some(true),
            ..Default::default()
        };
        let mut frame = Frame {
            external: vec![alpha],
            pointees: vec![(HoldRoot::Parameter(0), holder)],
            ..Default::default()
        };
        publish_after(
            &mut frame,
            PreparedAfter {
                rows: vec![(
                    source(HoldRoot::Parameter(0), vec![]),
                    Type::Tuple(vec![Type::I32]),
                    value,
                    true,
                    false,
                    vec![],
                )],
                pending: false,
            },
            &mut b,
        )
        .unwrap();
        assert_eq!(frame.external[0].address.sources, permission(7).sources);
        assert_eq!(
            frame.external[0].store_description.sources,
            permission(8).sources
        );
        assert_eq!(frame.external[0].creation_complete, Some(true));
        let beta = memory_holder(&frame, HoldRoot::Parameter(0), &mut b)
            .unwrap()
            .unwrap();
        assert!(beta.store_description.sources.is_empty());
        assert_eq!(beta.creation_complete, None);
        assert_eq!(
            facts(beta, &[PlaceProjection::Field(0)], &mut b)
                .unwrap()
                .unwrap()
                .description
                .sources,
            permission(3).sources
        );
    }
    #[test]
    fn raw_aggregate_physical_after_restores_current_beta_fields_and_preserves_stable_loan_identity()
     {
        let (mut module, _) = crate::tests::baseline();
        let mut callee = module.functions[0].clone();
        let pointer = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let tuple = Type::Tuple(vec![pointer.clone(), pointer.clone()]);
        let carrier = Type::Address {
            mutable: true,
            inner: Box::new(tuple.clone()),
        };
        callee.environment = None;
        callee.foreign = None;
        callee.call_kind = CallKind::Direct;
        callee.parameters = vec![LocalId(0), LocalId(1)];
        callee.locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "slot".into(),
                ty: carrier.clone(),
                mutable: false,
                span: callee.span,
            },
            LocalDecl {
                id: LocalId(1),
                name: "replacement".into(),
                ty: pointer.clone(),
                mutable: false,
                span: callee.span,
            },
        ];
        module.functions[0] = callee;
        let callee = &module.functions[0];
        let mut caller = callee.clone();
        caller.values = vec![carrier, pointer];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        assert!(transport_type(&module, &tuple, &mut b).unwrap());
        assert_eq!(after_sites(&module, callee, &mut b).unwrap().len(), 2);
        let field0 = vec![PlaceProjection::Field(0)];
        let field1 = vec![PlaceProjection::Field(1)];
        let old = |index, loan| Permission {
            sources: vec![source(HoldRoot::Local(LocalId(index)), vec![])],
            loans: vec![loan],
        };
        let mut beta = Payload::default();
        add_leaf(&mut beta, &field0, &old(3, 4), &mut b).unwrap();
        add_leaf(&mut beta, &field1, &old(2, 3), &mut b).unwrap();
        let mut alpha = Payload {
            address: Permission {
                sources: vec![source(HoldRoot::Parameter(0), vec![])],
                loans: vec![0],
            },
            ..Default::default()
        };
        add_leaf(&mut alpha, &field1, &old(3, 4), &mut b).unwrap();
        let replacement = Payload {
            address: old(1, 1),
            ..Default::default()
        };
        let values = vec![alpha.clone(), replacement];
        let mut frame = Frame {
            external: vec![alpha.clone(), Payload::default()],
            pointees: vec![(HoldRoot::Parameter(0), beta)],
            ..Default::default()
        };
        let instruction = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: callee.id,
                arguments: vec![ValueId(0), ValueId(1)],
            },
            span: callee.span,
        };
        let fresh = SymbolicPermission {
            sources: vec![source(HoldRoot::Parameter(1), vec![])],
            parents: vec![source(HoldRoot::Parameter(0), vec![])],
            receiver: false,
        };
        let mut stable_source = source(HoldRoot::Parameter(0), vec![]);
        stable_source.leaf = field1.clone();
        let stable = SymbolicPermission {
            sources: vec![stable_source],
            ..Default::default()
        };
        let summary = Summary {
            complete: true,
            value_after: After(vec![Row {
                base: HoldRoot::Parameter(0),
                through: true,
                target_leaf:vec![],
                ty: tuple,
                facts: vec![],
                tags: enum_address::Variants::default(),
                physical: vec![
                    (field0.clone(), fresh, false),
                    (field1.clone(), stable, true),
                ],
                description_paths: vec![],
            }]),
            ..Default::default()
        };
        let mut loans = vec![Loan::default(); 5];
        let sites = vec![(
            &instruction as *const Instruction as usize,
            0,
            field0.clone(),
            2,
            false,
        )];
        let before =
            call_results::Before::function(&module, &caller, &instruction, &values, &frame, &mut b)
                .unwrap();
        let prepared = prepare_after(
            &before,
            callee,
            &summary,
            &instruction,
            &sites,
            &mut loans,
            &mut b,
        )
        .unwrap();
        assert!(!prepared.pending);
        publish_after(&mut frame, prepared, &mut b).unwrap();
        let beta = memory_holder(&frame, HoldRoot::Parameter(0), &mut b)
            .unwrap()
            .unwrap();
        let changed = leaf(beta, &field0, &mut b).unwrap().unwrap();
        assert_eq!(changed.sources, old(1, 1).sources);
        assert_eq!(changed.loans, vec![2]);
        assert!(
            loans[2].parents.contains(&0) && loans[2].parents.contains(&1),
            "real carrier/head and data parents survive separately"
        );
        assert_eq!(leaf(beta, &field1, &mut b).unwrap().unwrap(), &old(2, 3));
        assert_eq!(
            frame.external[0].leaves, alpha.leaves,
            "beta publication does not rewrite the pointer SSA snapshot"
        );
    }

    #[test]
    fn complete_inactive_arm_after_retires_only_strong_physical_slots() {
        let (module, _) = crate::tests::baseline();
        let mut function = module.functions[0].clone();
        let borrowed = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let sum = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(borrowed),
        };
        function.values = vec![Type::Unit, sum.clone()];
        let produce = Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::MakeErrorUnion {
                value: ValueId(0),
                is_error: true,
            },
            span: function.span,
        };
        for strong in [false, true] {
            let limits = Limits::default();
            let mut b = Budget::new(&limits);
            b.raw_reference_cells = true;
            let tags = enum_address::tags::transfer(
                &function,
                &produce,
                &Frame::default(),
                &[Payload::default()],
                &mut b,
            )
            .unwrap()
            .unwrap();
            let arm = vec![PlaceProjection::ErrorArm(false)];
            let private = vec![PlaceProjection::ErrorArm(false), PlaceProjection::Field(9)];
            let mut old = Payload::default();
            let mut old_permission = permission(1);
            old_permission.loans = vec![7];
            add_leaf(&mut old, &arm, &old_permission, &mut b).unwrap();
            add_leaf(&mut old, &private, &permission(2), &mut b).unwrap();
            old.enum_variants = enum_address::Variants::runtime(&sum, 0, &mut b).unwrap();
            let domain = physical_paths(&module, &sum, &mut b)
                .unwrap()
                .into_iter()
                .map(|(path, _)| path)
                .collect();
            let mut frame = Frame {
                locals: vec![old],
                ..Default::default()
            };
            publish_after(
                &mut frame,
                PreparedAfter {
                    rows: vec![(
                        source(HoldRoot::Local(LocalId(0)), vec![]),
                        sum.clone(),
                        Payload {
                            enum_variants: tags,
                            ..Default::default()
                        },
                        strong,
                        false,
                        domain,
                    )],
                    pending: false,
                },
                &mut b,
            )
            .unwrap();
            let updated = &frame.locals[0];
            assert_eq!(leaf(updated, &arm, &mut b).unwrap().is_none(), strong);
            if !strong {
                assert_eq!(leaf(updated, &arm, &mut b).unwrap().unwrap().loans, vec![7]);
            }
            assert!(
                leaf(updated, &private, &mut b).unwrap().is_some(),
                "untransported pointee shadow is not an ordinary physical slot"
            );
            assert_eq!(updated.enum_variants.path_active(&arm), !strong);
        }
    }

    #[test]
    fn public_after_directory_keeps_parameter_capture_and_descriptor_data_roles_separate_from_view_heads()
     {
        let (module, _) = crate::tests::baseline();
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let key = vec![PlaceProjection::Field(0)];
        let symbolic = |index| SymbolicPermission {
            sources: vec![source(HoldRoot::Parameter(index), vec![])],
            ..Default::default()
        };
        let facts = ResultFacts {
            address: symbolic(2),
            description: symbolic(3),
            views: vec![(None, symbolic(9))],
            ..Default::default()
        };
        let row = Row {
            base: HoldRoot::Parameter(0),
            through: true,
            target_leaf:vec![],
            ty: Type::Tuple(vec![Type::Desc(Box::new(Type::I32))]),
            facts: vec![(key.clone(), facts)],
            tags: enum_address::Variants::default(),
            physical: vec![(key.clone(), symbolic(1), false)],
            description_paths: vec![key.clone()],
        };
        let capture = Row {
            base: HoldRoot::Capture(0),
            through: true,
            target_leaf:vec![],
            ty: Type::Tuple(vec![Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }]),
            facts: vec![],
            tags: enum_address::Variants::default(),
            physical: vec![(key.clone(), symbolic(4), true)],
            description_paths: vec![],
        };
        let rows = after_sources(&module, &After(vec![row, capture]), &mut b).unwrap();
        let parameter = rows
            .iter()
            .find(|(base, path, _)| *base == HoldRoot::Parameter(0) && path == &key)
            .unwrap();
        assert_eq!(parameter.2.len(), 3);
        assert!(
            !parameter
                .2
                .contains(&source(HoldRoot::Parameter(9), vec![])),
            "view ancestry never becomes a descriptor data target"
        );
        assert!(
            rows.iter()
                .any(|(base, path, sources)| *base == HoldRoot::Capture(0)
                    && path == &key
                    && sources == &symbolic(4).sources)
        );
    }
}

#[cfg(test)]
mod option_root_tests {
    use super::*;

    fn option() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Enum {
                name: "Option".into(),
                variants: vec![
                    ("None".into(), vec![]),
                    (
                        "Some".into(),
                        vec![Type::Address {
                            mutable: false,
                            inner: Box::new(Type::I32),
                        }],
                    ),
                ],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![StorageLifeSlot {
                    path: key(),
                    binder: 0,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![StorageRetainedBinding {
                    path: vec![LifeTypeProjection::TypeArgument(0)],
                    class: 0,
                    binder: 0,
                }],
                nominal_arguments: vec![],
            },
        }
    }
    fn key() -> Path {
        vec![PlaceProjection::Variant(VariantProjection {
            variant: 1,
            index: 0,
        })]
    }
    fn source(base: HoldRoot, leaf: Path) -> Source {
        Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base,
            leaf,
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
        }
    }
    fn payload(mask: u8, permission: Option<Permission>, b: &mut Budget) -> Payload {
        let mut out = Payload {
            option_variants: if mask == 0 {
                option_address::Variants::default()
            } else {
                option_address::Variants::construction(u32::from(mask == 2), b).unwrap()
            },
            ..Default::default()
        };
        if let Some(permission) = permission {
            add_leaf(&mut out, &key(), &permission, b).unwrap();
        }
        out
    }

    // These are actual memory/after components. Whole raw/Source admission
    // additionally authenticates the sealed nominal instance and constructors.
    #[test]
    fn option_root_writes_and_after_use_real_option_tags_for_direct_mut_and_once_entries() {
        for kind in [None, Some(CallableKind::FnMut), Some(CallableKind::FnOnce)] {
            let (mut module, _) = crate::tests::baseline();
            let ty = option();
            let carrier = Type::Address {
                mutable: true,
                inner: Box::new(ty.clone()),
            };
            let borrowed = Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            };
            let base = if kind.is_some() {
                HoldRoot::Capture(0)
            } else {
                HoldRoot::Parameter(0)
            };
            let input = if kind.is_some() {
                HoldRoot::Capture(1)
            } else {
                HoldRoot::Parameter(1)
            };
            let storage = if kind.is_some() {
                StorageBase::Capture(0)
            } else {
                StorageBase::Parameter(0)
            };
            let input_storage = if kind.is_some() {
                StorageBase::Capture(1)
            } else {
                StorageBase::Parameter(1)
            };
            let relation = StorageLifeContract {
                fixed_binder_count: 1,
                invocation_binder_count: 1,
                bindings: vec![
                    StorageLifeBinding {
                        position: StoragePosition {
                            base: storage,
                            path: vec![],
                        },
                        binder: StorageBinder::Invocation(0),
                    },
                    StorageLifeBinding {
                        position: StoragePosition {
                            base: input_storage,
                            path: vec![],
                        },
                        binder: StorageBinder::Fixed(0),
                    },
                ],
                classes: vec![SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: storage,
                        path: vec![LifeTypeProjection::Pointee],
                        class: 0,
                    },
                    binder: StorageBinder::Fixed(0),
                }],
                static_bindings: vec![],
            };
            let f = &mut module.functions[0];
            f.return_type = Type::Unit;
            f.storage_life = Some(relation.clone());
            if let Some(kind) = kind {
                f.parameters.clear();
                f.environment = Some(ClosureId(0));
                module.closures.push(ClosureDefinition {
                    id: ClosureId(0),
                    function: FunctionId(0),
                    captures: vec![carrier.clone(), borrowed.clone()],
                    parameters: vec![],
                    result: Type::Unit,
                    kind,
                    storage_life: Some(relation),
                    lending_captures: vec![],
                });
            } else {
                f.parameters = vec![LocalId(0), LocalId(1)];
                f.locals = vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "slot".into(),
                        ty: carrier,
                        mutable: false,
                        span: f.span,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "value".into(),
                        ty: borrowed,
                        mutable: false,
                        span: f.span,
                    },
                ];
            }
            let f = &module.functions[0];
            let limits = Limits::default();
            let mut b = Budget::new(&limits);
            assert!(!b.task_sources());
            let current = Permission {
                sources: vec![source(input, vec![])],
                loans: vec![0],
            };
            let old = payload(2, Some(current.clone()), &mut b);
            let entry = Frame {
                pointees: vec![(base, old.clone())],
                ..Default::default()
            };
            let solution = Solution {
                values: vec![],
                outputs: vec![],
                entry: entry.clone(),
                loans: vec![Loan {
                    sources: current.sources.clone(),
                    ..Default::default()
                }],
                sites: vec![],
                dyn_sites: dyn_leases::Sites::default(),
            };
            for mask in [1, 2] {
                let replacement = payload(mask, (mask == 2).then(|| current.clone()), &mut b);
                let mut state = entry.clone();
                write(
                    &mut state,
                    &Permission {
                        sources: vec![source(base, vec![])],
                        loans: vec![],
                    },
                    &ty,
                    &replacement,
                    &module,
                    &mut b,
                )
                .unwrap();
                assert!(!state.pending_external_update);
                let beta = memory_holder(&state, base, &mut b).unwrap().unwrap();
                assert_eq!(beta.option_variants.mask(), mask);
                assert_eq!(leaf(beta, &key(), &mut b).unwrap().is_some(), mask == 2);
                assert!(
                    !beta.enum_variants.present(),
                    "Option evidence is not a fabricated Enum tag"
                );
                let mut summary = Summary {
                    complete: true,
                    ..Default::default()
                };
                summarize_after(&module, f, &entry, &state, &solution, &mut summary, &mut b)
                    .unwrap();
                assert!(
                    summary.complete,
                    "entry {kind:?}, actual Option mask {mask}"
                );
                assert_eq!(summary.value_after.0.len(), 1);
                let row = &summary.value_after.0[0];
                assert_eq!(row.base, base);
                assert!(
                    row.through,
                    "consuming the closure must not discard a mutable pointee effect"
                );
                assert_eq!(row.physical.len(), usize::from(mask == 2));
                if mask == 2 {
                    assert_eq!(row.physical[0].1.sources, current.sources);
                }
            }
            for invalid in [payload(0, None, &mut b), payload(2, None, &mut b)] {
                let mut state = entry.clone();
                write(
                    &mut state,
                    &Permission {
                        sources: vec![source(base, vec![])],
                        loans: vec![],
                    },
                    &ty,
                    &invalid,
                    &module,
                    &mut b,
                )
                .unwrap();
                assert!(
                    state.pending_external_update,
                    "unknown tag or missing active Some target cannot complete a write"
                );
                assert_eq!(
                    memory_holder(&state, base, &mut b).unwrap().unwrap(),
                    &old,
                    "a pending write cannot erase the real old occupant"
                );
            }
        }
    }

    #[test]
    fn option_input_dependencies_read_beta_and_weak_writes_preserve_possible_old_occupants() {
        let (mut module, _) = crate::tests::baseline();
        let ty = option();
        let carrier = Type::Address {
            mutable: true,
            inner: Box::new(ty.clone()),
        };
        module.functions[0].parameters = vec![LocalId(0)];
        module.functions[0].locals[0].ty = carrier.clone();
        let mut caller = module.functions[0].clone();
        caller.values = vec![carrier];
        let instruction = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![ValueId(0)],
            },
            span: caller.span,
        };
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let input = option_address::Variants::parameter(0, &mut b).unwrap();
        let summary = Summary {
            complete: true,
            field_result_facts: vec![(
                vec![],
                ResultFacts {
                    option: input,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        for mask in [0, 1, 2] {
            let mut alpha = payload(if mask == 1 { 2 } else { 1 }, None, &mut b);
            alpha.address = Permission {
                sources: vec![source(HoldRoot::Parameter(0), vec![])],
                loans: vec![],
            };
            let values = [alpha];
            let frame = Frame {
                external: vec![Payload::default()],
                pointees: vec![(HoldRoot::Parameter(0), payload(mask, None, &mut b))],
                ..Default::default()
            };
            let before = call_results::Before::function(
                &module,
                &caller,
                &instruction,
                &values,
                &frame,
                &mut b,
            )
            .unwrap();
            let mut result = Payload::default();
            let known = before
                .field_results(
                    &summary,
                    &ty,
                    &instruction,
                    &vec![],
                    &mut [],
                    &mut result,
                    &mut b,
                )
                .unwrap();
            assert_eq!(known, mask != 0);
            assert_eq!(
                result.option_variants.mask(),
                mask,
                "the unrelated alpha tag cannot stand for current beta memory"
            );
        }
        let permission = |index| Permission {
            sources: vec![source(HoldRoot::Parameter(index), vec![])],
            loans: vec![index as usize],
        };
        let first = payload(2, Some(permission(2)), &mut b);
        let second = payload(2, Some(permission(3)), &mut b);
        let mut frame = Frame {
            locals: vec![first, second],
            ..Default::default()
        };
        let head = Permission {
            sources: vec![
                source(HoldRoot::Local(LocalId(0)), vec![]),
                source(HoldRoot::Local(LocalId(1)), vec![]),
            ],
            loans: vec![],
        };
        let none = payload(1, None, &mut b);
        write(&mut frame, &head, &ty, &none, &module, &mut b).unwrap();
        for (index, occupant) in frame.locals.iter().enumerate() {
            assert_eq!(
                occupant.option_variants.mask(),
                3,
                "a may-target update cannot claim every cell was selected"
            );
            assert_eq!(
                leaf(occupant, &key(), &mut b).unwrap().unwrap(),
                &permission(index as u32 + 2),
                "old data and all existing parents remain possible"
            );
        }
        assert!(option_address::Variants::construction(2, &mut b).is_err());
    }
}
