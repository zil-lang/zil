//! Independent canonical semantic-class checking. Rows do not supply roots.
use super::*;
mod occurrences;

// Only the actual type graph can establish a value-only Array. Its nested
// qualified instances are checked independently in module preflight.
fn pure_array_value(ty: &Type) -> bool {
    if !matches!(ty, Type::Array { .. }) {
        return false;
    }
    pure_value_tree(ty)
}

// A row-free ordinary owner contributes no lifetime class. Its ownership,
// nominal identity, effects and destructor remain separate resource proofs.
// Preflight and signature_work pay the complete finite type graph first.
fn ordinary_owned_value(ty: &Type) -> bool {
    match ty {
        Type::ByteBuffer => true,
        Type::Managed(inner)
            if matches!(
                inner.as_ref(),
                Type::Record { .. } | Type::ByteBuffer | Type::OwnedStore { .. }
            ) =>
        {
            ordinary_owned_value(inner)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().all(|(_, child)| ordinary_owned_value(child))
        }
        Type::Tuple(children) => children.iter().all(ordinary_owned_value),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, children)| children.iter().all(ordinary_owned_value)),
        Type::ErrorUnion { error, result } => {
            ordinary_owned_value(error) && ordinary_owned_value(result)
        }
        Type::Array { element, .. } | Type::OwnedStore { element } => ordinary_owned_value(element),
        Type::Unit | Type::Bool | Type::ForeignHandle | Type::CWideString => true,
        number => number.is_integer() || number.is_float(),
    }
}

pub(super) fn closed_owned_parameter(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. } => pending.push(inner),
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                pending.extend(fields.iter().map(|(_, child)| child))
            }
            Type::Tuple(fields) => pending.extend(fields),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::ByteBuffer => {}
            other if super::stored_callables::scalar(other) => {}
            _ => return false,
        }
    }
    true
}

fn pure_nominal_value(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) && pure_value_tree(ty)
}

// Each qualified child of an ordinary Tuple has its own local class domain.
// The enclosing signature must name every domain at its physical Field path.
// Admission of the actual nominal instances and all stored leaves is a
// separate module/body proof; this only selects the shared value directory.
fn scoped_compound(ty: &Type) -> bool {
    scoped_compound_inner(ty, false)
}
fn scoped_compound_inner(ty: &Type, qualified: bool) -> bool {
    match ty {
        Type::BorrowedStorage { storage, life } => {
            life.slots.iter().all(|slot| {
                matches!(
                    slot.mode,
                    StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
                )
            }) && scoped_compound_inner(storage, true)
        }
        Type::Address { .. } => qualified,
        Type::Managed(payload) => payload.is_copy_value(),
        Type::Tuple(fields) => fields
            .iter()
            .all(|field| scoped_compound_inner(field, qualified)),
        Type::Record { fields, .. } => fields
            .iter()
            .all(|(_, field)| scoped_compound_inner(field, qualified)),
        Type::Array { element, .. } => scoped_compound_inner(element, qualified),
        Type::Enum { variants, .. } => variants.iter().all(|(_, fields)| {
            fields
                .iter()
                .all(|field| scoped_compound_inner(field, qualified))
        }),
        Type::ErrorUnion { error, result } => {
            scoped_compound_inner(error, qualified) && scoped_compound_inner(result, qualified)
        }
        other => other.is_copy_value(),
    }
}

fn pure_value_tree(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            Type::BorrowedStorage { storage, life } if life.slots.is_empty() => {
                pending.push(storage)
            }
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|(_, t)| t)),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64 => {}
            _ => return false,
        }
    }
    true
}

fn array_classes<'a>(
    ty: &'a Type,
    path: &mut Vec<LifeTypeProjection>,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32, &'a StorageLifeShape)>,
) {
    match ty {
        Type::BorrowedStorage { life, .. } => {
            for class in 0..life.binder_count {
                out.push((path.clone(), class, life));
            }
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)));
                array_classes(child, path, out);
                path.pop();
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            array_classes(element, path, out);
            path.pop();
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Record { fields, .. } => {
            for (index, (_, field)) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                for (index, field) in fields.iter().enumerate() {
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    array_classes(field, path, out);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

pub(super) fn charge(c: &StorageLifeContract, b: &mut Budget) -> Check<()> {
    let n = c
        .bindings
        .len()
        .saturating_add(c.classes.len())
        .saturating_add(c.static_bindings.len());
    b.spend(
        n.saturating_mul(n)
            .saturating_add(n.saturating_mul(2))
            .saturating_add(2)
            .saturating_add(c.fixed_binder_count as usize)
            .saturating_add(c.invocation_binder_count as usize),
    )?;
    for r in &c.bindings {
        b.spend(r.position.path.len())?;
    }
    for position in &c.static_bindings {
        b.spend(position.path.len())?;
    }
    for r in &c.classes {
        b.spend(r.position.path.len())?;
    }
    Ok(())
}

pub(super) fn physical_binding<'a>(c:&'a StorageLifeContract,row:usize,position:&StoragePosition)->Option<&'a StorageLifeBinding>{
    c.bindings.get(row).filter(|row| row.position == *position)
}

pub(super) fn outer(c: &StorageLifeContract, base: StorageBase) -> Option<StorageBinder> {
    c.bindings
        .iter()
        .find(|r| r.position.base == base && r.position.path.is_empty())
        .map(|r| r.binder)
}
pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
) -> Check<()> {
    pending_static(c, span)?;
    relation_inner(
        parameters,
        result,
        c,
        span,
        false,
        None,
        None,
        false,
        None,
        &[],
        &[],
    )
}
pub(super) fn function_relation(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
) -> Check<()> {
    function_relation_inner(module, parameters, result, c, span, allow_callback, false, &[])
}
pub(super) fn function_relation_byte_inputs(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
) -> Check<()> {
    function_relation_inner(module, parameters, result, c, span, allow_callback, true, &[])
}
// Internal input views retain the exact immutable Type. Ordinary exported
// wrappers keep their existing owned-slice API; header preflight borrows its
// actual local declarations without rebuilding the whole parameter trees.
trait InputType{fn ty(&self)->&Type;}
impl InputType for Type{fn ty(&self)->&Type{self}}
impl InputType for &Type{fn ty(&self)->&Type{self}}

struct KnownZeroRoot<'a> {
    base: StorageBase,
    whole_type: &'a Type,
    statics: Vec<StoragePosition>,
}

/// Scheduling of an exact zero free-domain resource occurrence uses Verify's
/// own actual definition inventory. No target or ownership follows from it.
#[allow(clippy::too_many_arguments)]
pub(super) fn function_relation_resource_scopes(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    allow_byte_input: bool,
    b: &mut Budget,
) -> Check<()> {
    function_resource_inputs(module,parameters,result,c,span,allow_callback,allow_byte_input,b)
}
pub(super) fn function_relation_resource_views(module:&TypedModule,parameters:&[&Type],result:&Type,c:&StorageLifeContract,span:Option<Span>,allow_callback:bool,allow_byte_input:bool,b:&mut Budget)->Check<()>{
    function_resource_inputs(module,parameters,result,c,span,allow_callback,allow_byte_input,b)
}
fn function_resource_inputs<P:InputType>(module:&TypedModule,parameters:&[P],result:&Type,c:&StorageLifeContract,span:Option<Span>,allow_callback:bool,allow_byte_input:bool,b:&mut Budget)->Check<()>{
    let mut roots = Vec::new();
    for (ordinal, ty) in parameters.iter().map(InputType::ty).chain(std::iter::once(result)).enumerate() {
        let base = if ordinal == parameters.len() {
            StorageBase::Result
        } else {
            StorageBase::Parameter(
                u32::try_from(ordinal)
                    .map_err(|_| limit("resource occurrence ordinal overflow"))?,
            )
        };
        if !zero_resource_occurrence(module,ty,span,b)?{continue;}
        crate::stored_flow::type_bounds(ty, &b.limits.clone(), b)?;
        roots.push(KnownZeroRoot {
            base,
            whole_type: ty,
            statics: Vec::new(),
        });
    }
    if !roots.is_empty() && !c.static_bindings.is_empty() {
        return Err(reject(
            "E-VERIFY-SIGNATURE-CLASS",
            span,
            "resource scope needs its complete static occurrence consumer",
        ));
    }
    function_relation_inner(
        module,
        parameters,
        result,
        c,
        span,
        allow_callback,
        allow_byte_input,
        &roots,
    )
}
fn zero_resource_occurrence(module:&TypedModule,ty:&Type,span:Option<Span>,b:&mut Budget)->Check<bool>{
    let head=if let Type::Address{inner,..}=ty{inner.as_ref()}else{ty};
    let payload=match head{Type::Managed(inner)|Type::Desc(inner)=>inner.as_ref(),other=>other};
    match payload{
        Type::Registration{..}|Type::AsyncRegistration{..}=>{},
        Type::Mutex{..}=>{
            // The manager's complete child domain is independent of its
            // enclosing borrowed description head. Zero needs an exact
            // sealed resource definition, not an absent header.
            b.spend(module.resources.len())?;
            let mut rows=module.resources.iter().filter(|row|row.payload==*payload);
            let resource=rows.next().ok_or_else(||reject("E-VERIFY-SIGNATURE-CLASS",span,"mutex scope has no exact sealed resource definition"))?;
            if rows.next().is_some(){return Err(reject("E-VERIFY-SIGNATURE-CLASS",span,"mutex scope has duplicate sealed resource definitions"));}
            crate::generic_mutex::definition(module,resource,b)?;
            if crate::dynamic_witness::intrinsic_domain(module,payload,b)?.0!=0{return Ok(false);}
        },
        Type::Task{retained_life,..}if retained_life.binder_count==0=>{
            crate::tasks::require_closed_context(payload,span)?;
            if crate::semantic::task_domains::task_metadata_domain(module,payload,b)?!=0{return Ok(false);}
        },
        _=>return Ok(false),
    }
    Ok(crate::tasks::input_scope::module_domain(module,payload,b)?==crate::tasks::input_scope::Domain::Known(0))
}
/// A closure's captures have the same complete resource domains as ordinary
/// inputs. Known0 schedules no class row, and creates no ownership or SyncDrop
/// permission; actual creator, body/loan, phase and teardown proofs still run.
#[allow(clippy::too_many_arguments)]
pub(super) fn closure_relation_resource_scopes(module:&TypedModule,parameters:&[Type],captures:&[Type],kind:CallableKind,result:&Type,c:&StorageLifeContract,span:Option<Span>,b:&mut Budget)->Check<()>{closure_resource_inputs(module,parameters,captures,kind,result,c,span,b)}
pub(super) fn closure_relation_resource_views(module:&TypedModule,parameters:&[&Type],captures:&[Type],kind:CallableKind,result:&Type,c:&StorageLifeContract,span:Option<Span>,b:&mut Budget)->Check<()>{closure_resource_inputs(module,parameters,captures,kind,result,c,span,b)}
fn closure_resource_inputs<P:InputType>(module:&TypedModule,parameters:&[P],captures:&[Type],kind:CallableKind,result:&Type,c:&StorageLifeContract,span:Option<Span>,b:&mut Budget)->Check<()>{
    let mut roots=Vec::new();
    for(ordinal,ty)in parameters.iter().map(InputType::ty).chain(captures).chain(std::iter::once(result)).enumerate(){
        let base=if ordinal<parameters.len(){StorageBase::Parameter(ordinal as u32)}else if ordinal<parameters.len()+captures.len(){StorageBase::Capture((ordinal-parameters.len())as u32)}else{StorageBase::Result};
        if matches!(base,StorageBase::Capture(_))&&matches!(ty,Type::Tuple(_)|Type::Record{..}|Type::Package{..}|Type::Array{..}){
            if let Some(statics)=crate::stored_callables::constant_value_positions(module,ty,base,c,b)?{
                crate::stored_flow::type_bounds(ty,&b.limits.clone(),b)?;
                roots.push(KnownZeroRoot{base,whole_type:ty,statics});continue;
            }
        }
        if zero_resource_occurrence(module,ty,span,b)?{crate::stored_flow::type_bounds(ty,&b.limits.clone(),b)?;roots.push(KnownZeroRoot{base,whole_type:ty,statics:vec![]});}
    }
    relation_inputs(parameters,result,c,span,true,Some(captures),Some(module),false,Some(kind),&[],&roots)
}
fn function_relation_inner<P:InputType>(
    module: &TypedModule,
    parameters: &[P],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    allow_byte_input: bool,
    zero_resources: &[KnownZeroRoot<'_>],
) -> Check<()> {
    pending_static(c, span)?;
    // A returned Task's physical ports belong to its full retained frame,
    // not to the enclosing helper's count/order of root Address parameters.
    // Constructors and ordinary helpers use the same complete occurrence walk;
    // actual source selection is independently proved by the SSA/store kernel.
    relation_inputs(
        parameters,
        result,
        c,
        span,
        allow_callback,
        None,
        Some(module),
        allow_byte_input,
        None,
        &[],
        zero_resources,
    )
}
pub(super) fn role_relation(
    module: &TypedModule,
    definition: &ClosureDefinition,
    proposed: &StorageLifeContract,
    span: Option<Span>,
    b: &mut Budget,
) -> Check<()> {
    let actual = definition.storage_life.as_ref().ok_or_else(|| {
        unsupported(
            "callable role requires a complete actual definition scope",
            span,
        )
    })?;
    charge(actual, b)?;
    charge(proposed, b)?;
    for relation in [actual, proposed] {
        closure_relation_resource_scopes(
            module,
            &definition.parameters,
            &definition.captures,
            definition.kind,
            super::stored_callables::life_result(definition),
            relation,
            span,
            b,
        )?;
    }
    let invalid = || {
        reject(
            "E-VERIFY-SIGNATURE-CLASS",
            span,
            "callable role changes an actual persistent class or constant occurrence",
        )
    };
    if actual.fixed_binder_count != proposed.fixed_binder_count
        || actual.static_bindings != proposed.static_bindings
        || actual.bindings.len() != proposed.bindings.len()
        || actual.classes.len() != proposed.classes.len()
    {
        return Err(invalid());
    }
    for (left, right) in actual.bindings.iter().zip(&proposed.bindings) {
        b.spend(
            left.position
                .path
                .len()
                .saturating_add(right.position.path.len())
                .saturating_add(1),
        )?;
        if left.position != right.position
            || ((matches!(left.binder, StorageBinder::Fixed(_))
                || matches!(right.binder, StorageBinder::Fixed(_)))
                && left.binder != right.binder)
        {
            return Err(invalid());
        }
    }
    for (left, right) in actual.classes.iter().zip(&proposed.classes) {
        b.spend(
            left.position
                .path
                .len()
                .saturating_add(right.position.path.len())
                .saturating_add(1),
        )?;
        if left.position != right.position
            || ((matches!(left.binder, StorageBinder::Fixed(_))
                || matches!(right.binder, StorageBinder::Fixed(_)))
                && left.binder != right.binder)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Concrete dynamic obligations and completed original result-role exports
/// supply exact output positions. Generic structural callers have no such
/// context and continue using the strict entry points above.
#[allow(clippy::too_many_arguments)]
pub(super) fn function_relation_pending(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    allow_byte_input: bool,
    pending: &[SignatureLifeClass],
) -> Check<()> {
    relation_inner(
        parameters,
        result,
        c,
        span,
        allow_callback,
        None,
        Some(module),
        allow_byte_input,
        None,
        pending,
        &[],
    )
}
fn pending_static(c: &StorageLifeContract, span: Option<Span>) -> Check<()> {
    if c.static_bindings.is_empty() {
        Ok(())
    } else {
        Err(unsupported(
            "constant borrow occurrences require the complete actual constructor/write proof",
            span,
        ))
    }
}
#[allow(clippy::too_many_arguments)]
fn relation_inner(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    captures: Option<&[Type]>,
    module: Option<&TypedModule>,
    allow_byte_input: bool,
    closure_kind: Option<CallableKind>,
    pending: &[SignatureLifeClass],
    zero_resources: &[KnownZeroRoot<'_>],
) -> Check<()> {
    relation_inputs(parameters,result,c,span,allow_callback,captures,module,allow_byte_input,closure_kind,pending,zero_resources)
}
pub(super) fn function_relation_pending_views(module:&TypedModule,parameters:&[&Type],result:&Type,c:&StorageLifeContract,span:Option<Span>,allow_callback:bool,allow_byte_input:bool,pending:&[SignatureLifeClass])->Check<()>{
    relation_inputs(parameters,result,c,span,allow_callback,None,Some(module),allow_byte_input,None,pending,&[])
}
/// Tie each physical slot of an inline qualified value to the projected
/// intrinsic class at its actual boundary.
fn qualified_borrows(
    c: &StorageLifeContract,
    shape: &StorageLifeShape,
    base: StorageBase,
    prefix: &[LifeTypeProjection],
    first_class: usize,
    borrow: &mut usize,
    constant: &mut usize,
    span: Option<Span>,
) -> Check<()> {
    let invalid = || occurrences::failure(span, base, "invalid qualified physical slot mapping");
    for slot in &shape.slots {
        let mut path = Vec::new();
        for step in prefix {
            let LifeTypeProjection::Place(selector) = step else { return Err(invalid()) };
            path.push(ContractProjection::Place(*selector));
        }
        path.extend(slot.path.iter().copied().map(ContractProjection::Place));
        let position = StoragePosition { base, path };
        if slot.binder >= shape.binder_count {
            return Err(invalid());
        }
        let image = c.classes.get(first_class + slot.binder as usize).ok_or_else(invalid)?;
        if c.static_bindings.get(*constant) == Some(&position) {
            if slot.mode != StoredBorrowMode::SharedAddress || physical_binding(c, *borrow, &position).is_some() {
                return Err(invalid());
            }
            *constant += 1;
        } else {
            let row = occurrences::binding(c, *borrow, &position, span)?;
            if row.binder != image.binder {
                return Err(occurrences::failure(span, base, format!(
                    "qualified slot {position:?} has physical binding {:?}; intrinsic class {} maps to {:?}",
                    row.binder, slot.binder, image.binder,
                )));
            }
            *borrow += 1;
        }
    }
    Ok(())
}
/// Complete occurrence rows below a raw wrapper. A qualified nominal is a
/// domain boundary: its full N is listed once, and its own child map remains
/// independently checked. Wrapper descendants retain every path and class.
/// The last ordinal is the exact constructor embedding into the first N;
/// it never identifies distinct classes because their outside images agree.
pub(super) fn raw_reference_classes(
    module:Option<&TypedModule>,ty:&Type,
    mut row:impl FnMut(usize,u32,u32)->Check<()>,
)->Check<u32>{
    let invalid=||reject("E-VERIFY-SIGNATURE-CLASS",None,"raw reference lacks its complete constructor class domain");
    let count=crate::stored_flow::reference_domain(module,ty).ok_or_else(invalid)?;
    let(mut current,mut depth,mut offset,mut remaining)=(ty,1usize,0u32,count);
    loop{
        for class in 0..remaining{row(depth,class,offset.checked_add(class).ok_or_else(invalid)?)?;}
        match current{
            Type::Address{inner,..}|Type::Desc(inner)=>{offset=offset.checked_add(1).ok_or_else(invalid)?;remaining=remaining.checked_sub(1).ok_or_else(invalid)?;current=inner;},
            Type::Managed(inner)=>current=inner,
            _=>break,
        }
        depth=depth.checked_add(1).ok_or_else(invalid)?;
    }
    Ok(count)
}
fn relation_inputs<P:InputType>(
    parameters: &[P],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    captures: Option<&[Type]>,
    module: Option<&TypedModule>,
    allow_byte_input: bool,
    closure_kind: Option<CallableKind>,
    pending: &[SignatureLifeClass],
    zero_resources: &[KnownZeroRoot<'_>],
) -> Check<()> {
    let invalid = || {
        reject(
            "E-VERIFY-SIGNATURE-CLASS",
            span,
            "incomplete, noncanonical or ungrounded signature class relation",
        )
    };
    let rows = c.bindings.len().checked_add(c.classes.len()).ok_or_else(invalid)?;
    let mut borrow = 0usize;
    let mut constant = 0usize;
    let mut class = 0usize;
    let mut next = [0u32; 2];
    let mut inputs = [0u32; 2];
    let mut grounded = vec![false; (c.fixed_binder_count as usize).min(rows)];
    let capture_types = captures.unwrap_or(&[]);
    for (ordinal, ty) in parameters
        .iter().map(InputType::ty)
        .chain(capture_types)
        .chain(std::iter::once(result))
        .enumerate()
    {
        let output = ordinal == parameters.len() + capture_types.len();
        let capture = !output && ordinal >= parameters.len();
        if output {
            // The receiver is an actual closure invocation input, not a type
            // containing copies of its persistent capture-class directory.
            if let Some(row) = c.bindings.get(borrow)
                && row.position.base == StorageBase::Receiver
            {
                if captures.is_none()
                    || !matches!(closure_kind, Some(CallableKind::Fn | CallableKind::FnMut))
                    || !row.position.path.is_empty()
                {
                    return Err(occurrences::failure(span, StorageBase::Receiver, format!(
                        "signature receiver row {:?} is not the root of an actual Fn or FnMut receiver",
                        row.position,
                    )));
                }
                let StorageBinder::Invocation(index) = row.binder else {
                    return Err(occurrences::failure(span, StorageBase::Receiver, format!(
                        "signature receiver needs an Invocation binder; found {:?}", row.binder,
                    )));
                };
                if index >= c.invocation_binder_count || index > next[1] {
                    return Err(occurrences::failure(span, StorageBase::Receiver, format!(
                        "signature receiver binds Invocation({index}); domain has {} binders and next canonical introduction is {}",
                        c.invocation_binder_count, next[1],
                    )));
                }
                if index == next[1] {
                    next[1] += 1;
                }
                borrow += 1;
            }
            inputs = next;
        }
        let base = if output {
            StorageBase::Result
        } else if capture {
            StorageBase::Capture(u32::try_from(ordinal - parameters.len()).map_err(|_| invalid())?)
        } else {
            StorageBase::Parameter(u32::try_from(ordinal).map_err(|_| invalid())?)
        };
        let mut visit = |position: &dyn std::fmt::Debug, binder: StorageBinder, retained: bool, pending_here: bool| -> Check<()> {
            let failure = |reason: &str| occurrences::failure(span, base, format!(
                "signature occurrence {position:?} binds {binder:?}; {reason}",
            ));
            if capture && matches!(binder, StorageBinder::Invocation(_)) {
                return Err(failure("a persistent capture needs a Fixed binder"));
            }
            let (domain, id, count) = match binder {
                StorageBinder::Fixed(i) => (0, i, c.fixed_binder_count),
                StorageBinder::Invocation(i) => (1, i, c.invocation_binder_count),
            };
            if id >= count {
                return Err(failure(&format!("binder ordinal {id} is outside the declared domain of {count}")));
            }
            if output && id >= inputs[domain] && !pending_here {
                return Err(failure(&format!("result has no validated input for ordinal {id}; {} input binders were established in this domain", inputs[domain])));
            }
            if id > next[domain] {
                return Err(failure(&format!("noncanonical binder ordinal {id}; next introduction is {}", next[domain])));
            }
            if id == next[domain] {
                next[domain] += 1;
            }
            if retained && domain == 0 {
                if let Some(seen) = grounded.get_mut(id as usize) { *seen = true; }
            }
            Ok(())
        };
        let (actual, pointee) = match ty {
            Type::Address { inner, .. } | Type::Desc(inner) => {
                let writable = matches!(ty, Type::Address { mutable: true, .. } | Type::Desc(_));
                let position = StoragePosition { base, path: vec![] };
                if c.static_bindings.get(constant) == Some(&position) {
                    // A constant occurrence has no class or invocation binder.
                    // This checks only its shape; actual Static provenance is
                    // discharged separately by the typed body/constructor proof.
                    if writable {
                        return Err(occurrences::failure(span, base, format!(
                            "signature marks writable occurrence {position:?} as constant",
                        )));
                    }
                    if let Some(row) = physical_binding(c, borrow, &position) {
                        return Err(occurrences::failure(span, base, format!(
                            "signature declares occurrence {position:?} both constant and bound to {:?}", row.binder,
                        )));
                    }
                    constant += 1;
                } else {
                    let row = occurrences::binding(c, borrow, &position, span)?;
                    borrow += 1;
                    visit(&row.position, row.binder, capture, false)?;
                }

                (inner.as_ref(), true)
            }
            _ => (ty, false),
        };
        if let Some(root)=zero_resources.iter().find(|root|root.base==base&&root.whole_type==ty){
            for position in &root.statics{
                if c.static_bindings.get(constant)!=Some(position){return Err(invalid());}
                constant+=1;
            }
            continue;
        }
        if super::stored_flow::shared_array_value(actual) {
            if capture {
                return Err(invalid());
            }
            let Type::Array { length, .. } = actual else {
                unreachable!()
            };
            for index in 0..*length {
                let position = StoragePosition {
                    base,
                    path: vec![ContractProjection::Place(PlaceProjection::Field(index))],
                };
                if c.static_bindings.get(constant) == Some(&position) {
                    if c.bindings
                        .get(borrow)
                        .is_some_and(|r| r.position == position)
                    {
                        return Err(invalid());
                    }
                    constant += 1;
                } else {
                    let row = occurrences::binding(c, borrow, &position, span)?;
                    borrow += 1;
                    visit(&row.position, row.binder, false, false)?;
                }
            }
            continue;
        }
        let task = match actual {
            Type::Task { .. } => Some((actual, None, false)),
            Type::Managed(inner) | Type::Desc(inner)
                if matches!(inner.as_ref(), Type::Task { .. }) =>
            {
                Some((inner.as_ref(), None, true))
            }
            Type::ErrorUnion {
                result: success, ..
            } => match success.as_ref() {
                Type::Managed(inner) | Type::Desc(inner)
                    if matches!(inner.as_ref(), Type::Task { .. }) =>
                {
                    Some((inner.as_ref(), Some(PlaceProjection::ErrorArm(false)), true))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((task, arm, wrapped)) = task {
            super::tasks::require_closed_context(task, span)?;
            let Type::Task { retained_life, .. } = task else {
                unreachable!()
            };
            if retained_life.ports.is_empty() {
                return Err(invalid());
            }
            let mut class_path = Vec::new();
            let mut port_path = Vec::new();
            if let Some(arm) = arm {
                class_path.push(LifeTypeProjection::Place(arm));
                port_path.push(ContractProjection::Place(arm));
            }
            if pointee {
                class_path.push(LifeTypeProjection::Pointee);
            }
            if wrapped {
                class_path.push(LifeTypeProjection::Pointee);
            }
            for (port, shape) in retained_life.ports.iter().enumerate() {
                let mut path = port_path.clone();
                path.push(ContractProjection::RetainedPort(port as u32));
                let position = StoragePosition { base, path };
                let row = occurrences::binding(c, borrow, &position, span)?;
                let expected = SignatureLifeClass {base, path: class_path.clone(), class: shape.binder};
                let input_class = occurrences::class(c, class + shape.binder as usize, &expected, span)?;
                if input_class.binder != row.binder {
                    return Err(occurrences::failure(span, base, format!(
                        "signature retained port {:?} binds {:?}; actual projected class {:?} binds {:?}",
                        position, row.binder, expected, input_class.binder,
                    )));
                }
                if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                    return Err(invalid());
                }
                borrow += 1;
                visit(&row.position, row.binder, !output, false)?;
            }
            for local in 0..retained_life.binder_count {
                let expected = SignatureLifeClass {base, path: class_path.clone(), class: local};
                let row = occurrences::class(c, class, &expected, span)?;
                if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                    return Err(invalid());
                }
                class += 1;
                visit(&row.position, row.binder, !output, false)?;
            }
            continue;
        }
        // The allocation outcome retains the success owner's complete class
        // identity even on an error return. Its port source proof remains in
        // the independent SSA/store kernel; this is metadata equality only.
        if let Type::ErrorUnion {
            error,
            result: success,
        } = actual
            && let Type::OwnedDyn { interface } = success.as_ref()
            && let Some(owner) =
                module.and_then(|m| m.dyn_interfaces.iter().find(|d| d.id == *interface))
            && owner.context.is_some()
        {
            if error.as_ref() != &crate::dynamic_dispatch::create_error() {
                return Err(invalid());
            }
            for local in 0..owner.retained_life.binder_count {
                let mut path = if pointee {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    vec![]
                };
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)));
                let expected = SignatureLifeClass { base, path, class: local };
                let row = occurrences::class(c, class, &expected, span)?;
                class += 1;
                visit(
                    &row.position, row.binder,
                    if captures.is_some() { capture } else { !output },
                    output && pending.contains(&row.position),
                )?;
            }
            continue;
        }
        match actual {
            Type::Address{..}if pointee=>{
                let first=class;
                raw_reference_classes(module,actual,|depth,local,parent|{
                    let row=c.classes.get(class).ok_or_else(invalid)?;
                    if row.position.base!=base||row.position.path.len()!=depth||!row.position.path.iter().all(|step|*step==LifeTypeProjection::Pointee)||row.position.class!=local{return Err(invalid());}
                    let parent=c.classes.get(first.checked_add(parent as usize).ok_or_else(invalid)?).ok_or_else(invalid)?;
                    if parent.position.path!=[LifeTypeProjection::Pointee]||parent.binder!=row.binder{return Err(invalid());}
                    class+=1;visit(&row.position, row.binder,if captures.is_some(){capture}else{!output},output&&pending.contains(&row.position))
                })?;
            },
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                let owner = module
                    .and_then(|m| m.dyn_interfaces.iter().find(|d| d.id == *interface))
                    .ok_or_else(invalid)?;
                for local in 0..owner.retained_life.binder_count {
                    let expected = SignatureLifeClass { base, path: if pointee { vec![LifeTypeProjection::Pointee] } else { vec![] }, class: local };
                    let row = occurrences::class(c, class, &expected, span)?;
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(invalid());
                    }
                    class += 1;
                    visit(
                        &row.position, row.binder,
                        if captures.is_some() { capture } else { !output },
                        output && pending.contains(&row.position),
                    )?;
                }
            }
            Type::BorrowedStorage { storage, life } => {
                if (!matches!(
                    storage.as_ref(),
                    Type::Tuple(_) | Type::Record { .. } | Type::Package { .. }
                ) || (!life.retained.is_empty()
                    && !matches!(storage.as_ref(), Type::Package { .. } | Type::Tuple(_))))
                    && !pure_nominal_value(actual)
                    && !super::stored_flow::enum_address::signature(actual)
                    && !super::stored_flow::option_address::signature(actual)
                {
                    return Err(unsupported(
                        "signature owning/nested carrier consumer is not implemented",
                        span,
                    ));
                }
                if life.binder_count == 0 {
                    return Err(occurrences::failure(span, base, format!(
                        "signature nominal occurrence {base:?} has an empty intrinsic class domain",
                    )));
                }
                let first_class=class;
                for local in 0..life.binder_count {
                    let expected = SignatureLifeClass { base, path: if pointee { vec![LifeTypeProjection::Pointee] } else { vec![] }, class: local };
                    let row = occurrences::class(c, class, &expected, span)?;
                    class += 1;
                    visit(
                        &row.position, row.binder,
                        if captures.is_some() { capture } else { !output },
                        output && pending.contains(&row.position),
                    )?;
                }
                // A by-value carrier transfers each real physical borrowed
                // slot in addition to its complete intrinsic class domain.
                // Borrowing the carrier instead transfers only the outer
                // address; its referent slots are not new input loans.
                if !pointee {
                    qualified_borrows(c, life, base, &[], first_class, &mut borrow, &mut constant, span)?;
                }
            }
            Type::Array { .. } | Type::ErrorUnion { .. } | Type::Tuple(_)
                if pure_array_value(actual)
                    || super::stored_flow::enum_address::signature(actual)
                    || matches!(actual, Type::Tuple(_)) && scoped_compound(actual) =>
            {
                let mut path = if pointee {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    Vec::new()
                };
                let mut classes = Vec::new();
                array_classes(actual, &mut path, &mut classes);
                for (path, local, life) in classes {
                    if local == 0 && path.iter().all(|step| matches!(step, LifeTypeProjection::Place(_))) {
                        qualified_borrows(c, life, base, &path, class, &mut borrow, &mut constant, span)?;
                    }
                    let expected = SignatureLifeClass { base, path, class: local };
                    let row = occurrences::class(c, class, &expected, span)?;
                    class += 1;
                    visit(
                        &row.position, row.binder,
                        if captures.is_some() { capture } else { !output },
                        output && pending.contains(&row.position),
                    )?;
                }
            }
            Type::Closure(id) if allow_callback => {
                let count = module
                    .and_then(|m| m.closures.get(id.0 as usize))
                    .and_then(|d| d.storage_life.as_ref())
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-SIGNATURE-CLASS",
                            span,
                            "callable class directory is unknown, not a complete empty domain",
                        )
                    })?
                    .fixed_binder_count;
                for local in 0..count {
                    let expected = SignatureLifeClass { base, path: if pointee { vec![LifeTypeProjection::Pointee] } else { vec![] }, class: local };
                    let row = occurrences::class(c, class, &expected, span)?;
                    class += 1;
                    // An output row cannot ground itself. Only an exact
                    // independently proved export position may introduce a
                    // local result role with all source dependencies intact.
                    visit(&row.position, row.binder, !output, output && pending.contains(&row.position))?;
                }
            }
            t if pointee => {
                if !(t.is_address_pointee()
                    || matches!(t, Type::Slice { element } if element.is_copy_value())
                    // Input Package shape permits either receiver mode. The
                    // actual Store source, parent loan and access authority
                    // still require the independent body checks.
                    || !output
                        && matches!(base, StorageBase::Parameter(_))
                        && matches!(ty, Type::Address { .. })
                        && matches!(t, Type::Package { .. })
                    || allow_callback && matches!(t, Type::Closure(_))
                    // Captures already established their exact physical Fixed
                    // row above. Creator and invocation independently ground
                    // its actual generation; header validation grants no access.
                    || !output && (capture || allow_byte_input)
                        && matches!(ty,Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer))
                {
                    return Err(occurrences::failure(span, base, format!(
                        "signature {base:?} has an ineligible physical pointee for {ty:?}",
                    )));
                }
            }
            t if (capture || allow_callback && !output) && super::stored_owned::owner(t) => {}
            t => {
                if !(t.is_copy_value()
                    || ordinary_owned_value(t)
                    || pure_array_value(t)
                    || allow_callback && !output && matches!(t, Type::Closure(_)))
                {
                    return Err(invalid());
                }
            }
        }
    }
    if let Some(extra) = c.bindings.get(borrow) {
        return Err(occurrences::failure(span, extra.position.base, format!(
            "signature has extra physical borrow row {borrow}: {extra:?}",
        )));
    }
    if let Some(extra) = c.static_bindings.get(constant) {
        return Err(occurrences::failure(span, extra.base, format!(
            "signature has extra constant borrow row {constant}: {extra:?}",
        )));
    }
    if let Some(extra) = c.classes.get(class) {
        return Err(occurrences::failure(span, extra.position.base, format!(
            "signature has extra intrinsic class row {class}: {extra:?}",
        )));
    }
    if next != [c.fixed_binder_count, c.invocation_binder_count] {
        return Err(occurrences::failure(span, StorageBase::Result, format!(
            "signature declares {} Fixed and {} Invocation binders; validated positions introduce {} Fixed and {} Invocation binders",
            c.fixed_binder_count, c.invocation_binder_count, next[0], next[1],
        )));
    }
    let input_classes = c.classes.iter().filter(|r| matches!(r.position.base, StorageBase::Parameter(_))).count();
    if captures.is_none() && (c.fixed_binder_count as usize > c.classes.len()
        || c.fixed_binder_count as usize > input_classes.saturating_add(pending.len()))
    {
        return Err(occurrences::failure(span, StorageBase::Result, format!(
            "signature declares {} Fixed binders but supplies {input_classes} input class rows and {} independently established result class positions",
            c.fixed_binder_count, pending.len(),
        )));
    }
    if let Some(index) = grounded.iter().enumerate().find_map(|(index, seen)| {
        (!seen && !c.classes.iter().any(|row| row.binder == StorageBinder::Fixed(index as u32)
            && pending.contains(&row.position))).then_some(index)
    }) {
        return Err(occurrences::failure(span, StorageBase::Result, format!(
            "signature Fixed({index}) has no retained input occurrence or independently established result class",
        )));
    }
    occurrences::returned_root(c, parameters, captures, closure_kind, result, allow_byte_input, span)
}

pub(super) fn entails(required: &StorageLifeContract, supplied: &StorageLifeContract) -> bool {
    if required.static_bindings != supplied.static_bindings
        || required.bindings.len() != supplied.bindings.len()
        || required.classes.len() != supplied.classes.len()
    {
        return false;
    }
    for (a, b) in required.bindings.iter().zip(&supplied.bindings) {
        if a.position != b.position {
            return false;
        }
    }
    for (a, b) in required.classes.iter().zip(&supplied.classes) {
        if a.position != b.position {
            return false;
        }
    }
    let count = required.bindings.len() + required.classes.len();
    let pair = |i: usize| -> (StorageBinder, StorageBinder) {
        if i < required.bindings.len() {
            (required.bindings[i].binder, supplied.bindings[i].binder)
        } else {
            let j = i - required.bindings.len();
            (required.classes[j].binder, supplied.classes[j].binder)
        }
    };
    for i in 0..count {
        let (a, x) = pair(i);
        for j in 0..i {
            let (b, y) = pair(j);
            if a == b && x != y {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn impossible_fixed_domain_never_allocates_a_claimed_grounding_vector() {
        let relation_data = StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: u32::MAX,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
        };
        assert!(relation(&[], &Type::I32, &relation_data, None).is_err());
    }
    #[test]
    fn constant_occurrence_directory_is_complete_and_disjoint() {
        let slice = Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        };
        let capture = StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![],
        };
        let result = StoragePosition {
            base: StorageBase::Result,
            path: vec![],
        };
        let c = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
            static_bindings: vec![capture.clone(), result.clone()],
        };
        let check = |c: &StorageLifeContract| {
            relation_inner(
                &[],
                &slice,
                c,
                None,
                true,
                Some(std::slice::from_ref(&slice)),
                None,
                false,
                Some(CallableKind::FnOnce),
                &[],
                &[],
            )
        };
        assert!(check(&c).is_ok());
        for positions in [
            vec![],
            vec![capture.clone()],
            vec![result.clone(), capture.clone()],
            vec![capture.clone(), capture.clone(), result.clone()],
        ] {
            let mut bad = c.clone();
            bad.static_bindings = positions;
            assert!(check(&bad).is_err());
        }
        let mut bad = c.clone();
        bad.fixed_binder_count = 1;
        bad.bindings.push(StorageLifeBinding {
            position: capture,
            binder: StorageBinder::Fixed(0),
        });
        assert!(check(&bad).is_err());
        assert!(!entails(&c, &bad));
        assert!(relation(&[], &slice, &c, None).is_err());
    }
}

#[cfg(test)]
mod input_view_tests{
    use super::*;
    #[test]
    fn complete_by_value_carrier_keeps_physical_slots_and_intrinsic_classes_separate(){
        let(module,_)=crate::tests::baseline();let address=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        let carrier=Type::BorrowedStorage{storage:Box::new(Type::Record{name:"Borrowed".into(),fields:vec![("left".into(),address.clone()),("right".into(),address.clone())]}),life:StorageLifeShape{binder_count:2,slots:(0..2).map(|index|StorageLifeSlot{path:vec![PlaceProjection::Field(index)],binder:index,mode:StoredBorrowMode::SharedAddress}).collect(),retained:vec![],nominal_arguments:vec![]}};
        let mut bindings=(0..2).map(|index|StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(index),path:vec![]},binder:StorageBinder::Invocation(index)}).collect::<Vec<_>>();
        bindings.extend((0..2).map(|index|StorageLifeBinding{position:StoragePosition{base:StorageBase::Result,path:vec![ContractProjection::Place(PlaceProjection::Field(index))]},binder:StorageBinder::Invocation(index)}));
        let contract=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:2,bindings,static_bindings:vec![],classes:(0..2).map(|index|SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Result,path:vec![],class:index},binder:StorageBinder::Invocation(index)}).collect()};
        let parameters=[address.clone(),address];
        function_relation(&module,&parameters,&carrier,&contract,None,false).unwrap();
        for attack in 0..5{let mut c=contract.clone();match attack{
            0=>{c.bindings.pop();},
            1=>c.bindings[2].binder=StorageBinder::Invocation(1),
            2=>c.bindings[2].position.path=vec![ContractProjection::Place(PlaceProjection::Field(99))],
            3=>c.bindings.push(c.bindings[2].clone()),
            _=>{c.classes.pop();},
        }assert!(function_relation(&module,&parameters,&carrier,&c,None,false).is_err(),"malformed physical/class relation {attack}");}
    }
    #[test]
    fn local_callable_result_role_needs_its_exact_independent_export_position(){
        let(mut module,_)=crate::tests::baseline();
        let address=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![address.clone()],parameters:vec![],result:Type::I32,kind:CallableKind::Fn,lending_captures:vec![],storage_life:Some(StorageLifeContract{fixed_binder_count:1,invocation_binder_count:0,bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(0),path:vec![]},binder:StorageBinder::Fixed(0)}],static_bindings:vec![],classes:vec![]})});
        let parameters=[address.clone(),address];let views=parameters.iter().collect::<Vec<_>>();
        let position=SignatureLifeClass{base:StorageBase::Result,path:vec![],class:0};
        let c=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:3,bindings:(0..2).map(|index|StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(index),path:vec![]},binder:StorageBinder::Invocation(index)}).collect(),static_bindings:vec![],classes:vec![SignatureLifeClassBinding{position:position.clone(),binder:StorageBinder::Invocation(2)}]};
        let result=Type::Closure(ClosureId(0));
        assert!(function_relation_resource_views(&module,&views,&result,&c,None,true,false,&mut Budget::new(&Limits::default())).is_err(),"raw output/header does not ground itself");
        function_relation_pending_views(&module,&views,&result,&c,None,true,false,&[position.clone()]).unwrap();
        let mut wrong=position.clone();wrong.path.push(LifeTypeProjection::Pointee);
        assert!(function_relation_pending_views(&module,&views,&result,&c,None,true,false,&[wrong]).is_err());
        let mut extra=c.clone();extra.invocation_binder_count+=1;
        assert!(function_relation_pending_views(&module,&views,&result,&extra,None,true,false,&[position]).is_err(),"a completed output does not erase any other domain coordinate");
    }
    #[test]
    fn borrowed_parameter_views_preserve_complete_header_and_hostile_domain_checks(){
        let(module,_)=crate::tests::baseline();
        let parameters=vec![Type::Address{mutable:false,inner:Box::new(Type::I32)}];
        let views=parameters.iter().collect::<Vec<_>>();
        let relation=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:1,bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(0),path:vec![]},binder:StorageBinder::Invocation(0)}],static_bindings:vec![],classes:vec![]};
        for attack in 0..5{
            let mut c=relation.clone();match attack{
                0=>{},
                1=>c.invocation_binder_count=2,
                2=>c.bindings[0].binder=StorageBinder::Fixed(0),
                3=>c.classes.push(SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Parameter(0),path:vec![LifeTypeProjection::Pointee],class:0},binder:StorageBinder::Invocation(0)}),
                _=>c.static_bindings.push(StoragePosition{base:StorageBase::Parameter(0),path:vec![]}),
            }
            let expected=function_relation_resource_scopes(&module,&parameters,&Type::I32,&c,None,false,false,&mut Budget::new(&Limits::default()));
            let actual=function_relation_resource_views(&module,&views,&Type::I32,&c,None,false,false,&mut Budget::new(&Limits::default()));
            assert_eq!(actual,expected,"complete input relation at attack {attack}");
            if attack==0{actual.unwrap();}else{assert_ne!(actual.unwrap_err().category,zil_base::ResultCategory::ResourceLimit);}
        }
    }
}

#[cfg(test)]
mod raw_reference_directory_tests {
    use super::*;
    fn shared(inner:Type)->Type{Type::Address{mutable:false,inner:Box::new(inner)}}
    #[test]
    fn nested_header_keeps_whole_and_descendant_occurrences_and_exact_embedding(){
        let(module,_)=crate::tests::baseline();
        let parameters=[shared(shared(shared(Type::I32)))];
        let contract=StorageLifeContract{fixed_binder_count:2,invocation_binder_count:1,bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(0),path:vec![]},binder:StorageBinder::Invocation(0)}],static_bindings:vec![],classes:vec![
            SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Parameter(0),path:vec![LifeTypeProjection::Pointee],class:0},binder:StorageBinder::Fixed(0)},
            SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Parameter(0),path:vec![LifeTypeProjection::Pointee],class:1},binder:StorageBinder::Fixed(1)},
            SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Parameter(0),path:vec![LifeTypeProjection::Pointee,LifeTypeProjection::Pointee],class:0},binder:StorageBinder::Fixed(1)},
        ]};
        function_relation(&module,&parameters,&Type::I32,&contract,None,false).unwrap();
        for attack in 0..4{
            let mut altered=contract.clone();
            match attack{0=>{altered.classes.pop();},1=>altered.classes[2].binder=StorageBinder::Fixed(0),2=>altered.classes[2].position.path.push(LifeTypeProjection::Pointee),_=>altered.classes.push(altered.classes[2].clone())}
            assert!(function_relation(&module,&parameters,&Type::I32,&altered,None,false).is_err(),"attack {attack} must not alter the complete raw child forest");
        }
    }
    #[test]
    fn returned_task_header_keeps_occurrences_without_identifying_helper_inputs_with_worker_ports(){
        let(module,_)=crate::tests::baseline();
        let address=Type::Address{mutable:true,inner:Box::new(Type::I32)};
        let shape=RetainedLifeShape{binder_count:1,ports:vec![RetainedBorrowPort{leaf:address.clone(),mode:StoredBorrowMode::ExclusiveAddress,binder:0}],retained:vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class:0,binder:0}],nominal_arguments:vec![]};
        let result=Type::ErrorUnion{error:Box::new(Type::I32),result:Box::new(Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:shape})))};
        let output=StorageLifeBinding{position:StoragePosition{base:StorageBase::Result,path:vec![ContractProjection::Place(PlaceProjection::ErrorArm(false)),ContractProjection::RetainedPort(0)]},binder:StorageBinder::Invocation(0)};
        let class=SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Result,path:vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)),LifeTypeProjection::Pointee],class:0},binder:StorageBinder::Invocation(0)};
        for input_count in [1,2]{
            let parameters=vec![address.clone();input_count];
            let mut bindings=(0..input_count).map(|input|StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(input as u32),path:vec![]},binder:StorageBinder::Invocation(0)}).collect::<Vec<_>>();bindings.push(output.clone());
            let c=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:1,bindings,static_bindings:vec![],classes:vec![class.clone()]};
            function_relation(&module,&parameters,&result,&c,None,false).unwrap();
            for attack in 0..4{let mut altered=c.clone();match attack{0=>{altered.classes.clear();},1=>{altered.bindings.pop();},2=>altered.bindings.last_mut().unwrap().position.path=vec![ContractProjection::Place(PlaceProjection::ErrorArm(false)),ContractProjection::RetainedPort(1)],_=>altered.classes[0].position.class=1}assert!(function_relation(&module,&parameters,&result,&altered,None,false).is_err(),"complete physical port/class occurrence attack {attack}");}
        }
        // This is only header identity. Two same-domain inputs do not select
        // a data source; equal-task-source Raw/Wire controls still check the
        // actual returned second input through the independent source ledger.
    }

}
