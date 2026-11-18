//! Narrow owner carriers alongside capture-grounded Fixed. These checks only
//! classify and type actual operations. The independent resource ledger still
//! proves every owner/description epoch, transfer, Once use and cleanup.
use super::*;

pub(super) fn payload(ty: &Type, b: &mut Budget) -> Check<bool> {
    let Type::Record { fields, .. } = ty else {
        return Ok(false);
    };
    for (_, field) in fields {
        if !stored_callables::scalar(field) && !stored_flow::retained::resource_field(field, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn owner(ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(matches!(ty, Type::Managed(inner) if payload(inner, b)?))
}
pub(super) fn description(ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(matches!(ty, Type::Desc(inner) if payload(inner, b)?))
}
fn bad(span: Span, message: &str) -> Diagnostic {
    reject("E-VERIFY-STORED-OWNER", Some(span), message)
}
fn shape(ty: &Type, span: Span, b: &mut Budget) -> Check<()> {
    stored_flow::type_work(ty, &b.limits.clone(), b)?;
    if owner(ty, b)? || description(ty, b)? {
        Ok(())
    } else {
        Err(unsupported(
            "Fixed/owner integration requires a class-free finite Copy Record carrier",
            Some(span),
        ))
    }
}
fn actual(f: &TypedFunction, value: ValueId, span: Span) -> Check<&Type> {
    f.values
        .get(value.0 as usize)
        .ok_or_else(|| bad(span, "owner operand has no actual type"))
}
fn local(f: &TypedFunction, id: LocalId, span: Span) -> Check<&Type> {
    f.locals
        .get(id.0 as usize)
        .map(|l| &l.ty)
        .ok_or_else(|| bad(span, "owner local is absent"))
}
fn result<'a>(f: &'a TypedFunction, i: &Instruction) -> Check<&'a Type> {
    actual(
        f,
        i.result
            .ok_or_else(|| bad(i.span, "owner operation has no result"))?,
        i.span,
    )
}
fn same_result(f: &TypedFunction, i: &Instruction, expected: &Type, b: &mut Budget) -> Check<()> {
    let ty = result(f, i)?;
    stored_flow::type_work(expected, &b.limits.clone(), b)?;
    stored_flow::type_work(ty, &b.limits.clone(), b)?;
    if ty == expected {
        Ok(())
    } else {
        Err(bad(
            i.span,
            "owner operation result differs from its actual source",
        ))
    }
}
fn capture<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    index: u32,
    span: Span,
) -> Check<&'a Type> {
    let d = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .ok_or_else(|| bad(span, "owner operation has no actual environment"))?;
    if d.storage_life.is_none() {
        return Err(unsupported(
            "owning Fixed capture operation requires its actual qualified environment",
            Some(span),
        ));
    }
    d.captures
        .get(index as usize)
        .ok_or_else(|| bad(span, "owner capture index absent"))
}
/// One actual-type classifier for profile, staging and transfer. Routing flags
/// select work; they never erase a supported owner operation in a mixed task
/// module. This predicate supplies no initialization, source or cleanup debt.
pub(super) fn operation(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<bool>{
    let ty=match &i.kind{
        InstructionKind::MakeManaged{..}=>result(f,i)?,
        InstructionKind::TakeLocal{local:id}|InstructionKind::DescribeLocal{local:id}|InstructionKind::DropManaged{local:id}=>local(f,*id,i.span)?,
        InstructionKind::TakeCapture{index}|InstructionKind::DescribeCapture{index}=>{
            let Some(definition)=f.environment.and_then(|id|module.closures.get(id.0 as usize))else{return Ok(false);};
            let Some(ty)=definition.captures.get(*index as usize)else{return Err(bad(i.span,"owner capture index absent"));};
            if definition.storage_life.is_none(){return Ok(false);}ty
        },
        InstructionKind::ReadDesc{desc,..}|InstructionKind::WriteDesc{desc,..}=>actual(f,*desc,i.span)?,
        _=>return Ok(false),
    };
    stored_flow::type_work(ty,&b.limits.clone(),b)?;
    let supported=match i.kind{
        InstructionKind::ReadDesc{..}|InstructionKind::WriteDesc{..}=>description(ty, b)?,
        InstructionKind::TakeCapture{..}=>owner(ty, b)?||matches!(ty,Type::Closure(_)|Type::Address{mutable:true,..}),
        _=>owner(ty, b)?,
    };
    if supported{types(module,f,i,b)?;}Ok(supported)
}
pub(super) fn types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    match &i.kind {
        InstructionKind::MakeManaged { fields } => {
            let out = result(f, i)?;
            shape(out, i.span, b)?;
            let Type::Managed(inner) = out else {
                return Err(bad(i.span, "MakeManaged must produce an actual owner"));
            };
            let Type::Record {
                fields: expected, ..
            } = inner.as_ref()
            else {
                unreachable!()
            };
            b.spend(fields.len().saturating_add(expected.len()))?;
            if fields.len() != expected.len() {
                return Err(bad(i.span, "MakeManaged omits an actual payload field"));
            }
            for (value, (_, ty)) in fields.iter().zip(expected) {
                if actual(f, *value, i.span)? != ty {
                    return Err(bad(i.span, "MakeManaged operand type differs from its actual field"));
                }
            }
        }
        InstructionKind::TakeLocal { local: id } => {
            let ty = local(f, *id, i.span)?;
            shape(ty, i.span, b)?;
            if !owner(ty, b)? {
                return Err(bad(i.span, "TakeLocal requires an actual finite owner"));
            }
            same_result(f, i, ty, b)?;
        }
        InstructionKind::TakeCapture { index } => {
            if !f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .is_some_and(|d| d.kind == CallableKind::FnOnce)
            {
                return Err(bad(
                    i.span,
                    "moving a captured owner requires the actual consuming FnOnce entry",
                ));
            }
            let ty = capture(module, f, *index, i.span)?;
            if matches!(ty,Type::Closure(_)){
                // This opcode consumes an owned environment, not a Managed
                // scalar Record. The real closure kernel checks the exact
                // index/kind/typed result; its responsibility ledger still
                // proves the capture is live and transferred exactly once.
                let actual=super::closures::check_resource_instruction(module,f,i,b)?;
                if actual.as_ref()!=Some(ty){return Err(bad(i.span,"captured closure transfer changes its exact actual environment type"));}
                let(count,_pending_sources)=super::dynamic_witness::intrinsic_domain(module,ty,b)?;
                if count!=0&&!b.returned_environment_sources{return Err(unsupported("captured closure transfer needs its complete returned-environment source consumer",Some(i.span)));}
                return same_result(f,i,ty,b);
            }
            if matches!(ty, Type::Address { mutable: true, .. }) {
                stored_flow::type_work(ty, &b.limits.clone(), b)?;
                // Transfer the exact borrowed capability, not owned pointee
                // cleanup. The entry and vacancy ledgers prove a unique Take.
                return same_result(f, i, ty, b);
            }
            shape(ty, i.span, b)?;
            if !owner(ty, b)? {
                return Err(bad(i.span, "TakeCapture requires an actual owned capture"));
            }
            same_result(f, i, ty, b)?;
        }
        InstructionKind::DescribeLocal { .. } | InstructionKind::DescribeCapture { .. } => {
            // The two operands are distinct domains; resolve each actual source.
            let ty = match &i.kind {
                InstructionKind::DescribeLocal { local: id } => local(f, *id, i.span)?,
                InstructionKind::DescribeCapture { index } => capture(module, f, *index, i.span)?,
                _ => unreachable!(),
            };
            shape(ty, i.span, b)?;
            let Type::Managed(inner) = ty else {
                return Err(bad(i.span, "description source is not an actual owner"));
            };
            let out = result(f, i)?;
            shape(out, i.span, b)?;
            let Type::Desc(actual) = out else {
                return Err(bad(i.span, "description must produce Desc"));
            };
            stored_flow::type_work(ty, &b.limits.clone(), b)?;
            stored_flow::type_work(out, &b.limits.clone(), b)?;
            if inner != actual {
                return Err(bad(
                    i.span,
                    "description payload differs from actual source",
                ));
            }
        }
        InstructionKind::ReadDesc { desc, index } => {
            let ty = actual(f, *desc, i.span)?;
            shape(ty, i.span, b)?;
            let Type::Desc(inner) = ty else {
                return Err(bad(i.span, "field read requires actual Desc"));
            };
            let Type::Record { fields, .. } = inner.as_ref() else {
                unreachable!()
            };
            let (_, field) = fields
                .get(*index as usize)
                .ok_or_else(|| bad(i.span, "description field index absent"))?;
            same_result(f, i, field, b)?;
        }
        InstructionKind::WriteDesc{desc,index,value}=>{
            let ty=actual(f,*desc,i.span)?;shape(ty,i.span,b)?;
            let Type::Desc(inner)=ty else{return Err(bad(i.span,"field write requires an actual Desc"));};
            let Type::Record{fields,..}=inner.as_ref()else{unreachable!()};
            let(_,field)=fields.get(*index as usize).ok_or_else(||bad(i.span,"description write field index absent"))?;
            let operand=actual(f,*value,i.span)?;
            stored_flow::type_work(field,&b.limits.clone(),b)?;stored_flow::type_work(operand,&b.limits.clone(),b)?;
            if operand!=field||i.result.is_some(){return Err(bad(i.span,"description write changes its exact field type or adds a result"));}
        }
        InstructionKind::DropManaged { local: id } => {
            let ty = local(f, *id, i.span)?;
            shape(ty, i.span, b)?;
            if !owner(ty, b)? || i.result.is_some() {
                return Err(bad(
                    i.span,
                    "DropManaged requires actual owner and no result",
                ));
            }
        }
        _ => return Err(bad(i.span, "unknown finite owner operation")),
    }
    Ok(())
}
/// A module cannot carry arbitrary resource profiles just because one closure
/// has an allowed owner. Each declaration must be the exact Copy payload of
/// an actual owning capture in a qualified concrete environment. Its mode
/// separately controls whether the body may merely describe or consume it.
pub(super) fn module(module: &TypedModule, b: &mut Budget) -> Check<()> {
    if module.resources.is_empty() {
        return Ok(());
    }
    b.spend(module.closures.len())?;
    let mut payloads = Vec::new();
    for d in &module.closures {
        if d.storage_life.is_some() {
            b.spend(d.captures.len())?;
            for ty in &d.captures {
                stored_flow::type_work(ty, &b.limits.clone(), b)?;
                if let Type::Managed(inner) = ty
                    && payload(inner, b)?
                {
                    payloads.push(inner.as_ref());
                }
            }
        }
    }
    b.spend(module.resources.len())?;
    for resource in &module.resources {
        stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
        // Canonical builtin heap definitions keep their own complete resource
        // proof. They are not required to be Record Fixed captures.
        if ordinary_store_value(module,&resource.payload,b)?||resource.payload==Type::ByteBuffer{continue;}
        let finite_payload = payload(&resource.payload, b)?;
        if finite_payload {
            let owner=Type::Managed(Box::new(resource.payload.clone()));
            // The complete actual definition has already passed resource
            // admission. A Known0 ordinary resource declaration need not be
            // used by a Fixed capture; construction and every cleanup remain
            // obligations of the actual owner/debt kernel.
            if crate::dynamic_witness::intrinsic_domain(module,&owner,b)?.0==0{
                crate::dynamic_witness::structural_actual(module,&owner,DynStructuralBound::Value,b)?;
                continue;
            }
        }
        if !finite_payload {
            return Err(unsupported(
                "unrelated resource shape is outside owning Fixed integration",
                None,
            ));
        }
        b.spend(payloads.len())?;
        let mut found = false;
        for actual in &payloads {
            stored_flow::type_work(actual, &b.limits.clone(), b)?;
            stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
            if *actual == &resource.payload {
                found = true;
            }
        }
        if !found {
            return Err(unsupported(
                "resource declaration is not an actual finite owning Fixed capture",
                None,
            ));
        }
    }
    Ok(())
}

/// New modules do not acquire arbitrary nested owners or owning-head addresses
/// from the old Copy-value fallback. Concrete environment capture types are
/// checked separately; only top-level finite owner/Desc carriers are added.
pub(super) fn value_type(ty: &Type, b: &mut Budget) -> Check<()> {
    if matches!(ty, Type::Managed(_) | Type::Desc(_)) {
        shape(ty, Span::default(), b)?;
        return Ok(());
    }
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Type::Managed(_)
            | Type::Desc(_)
            | Type::Package { .. }
            | Type::OwnedStore { .. }
            | Type::Task { .. }
            | Type::TaskI32
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. }
            | Type::ByteBuffer => {
                return Err(unsupported(
                    "nested owner or owner-head address is outside finite Fixed integration",
                    None,
                ));
            }
            Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::BorrowedStorage { storage: inner, .. } => {
                b.spend(1)?;
                pending.push(inner);
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}

// A separate ordinary Store lane. The scalar Record capture premise above is
// intentionally unchanged: these facts describe only a complete target-free
// Store element domain, never an owner, allocation interval or live window.
pub(super) fn ordinary_store_value<'a>(module:&'a TypedModule,ty:&'a Type,b:&mut Budget)->Check<bool>{
    stored_flow::type_work(ty,&b.limits.clone(),b)?;
    let head=match ty{Type::Address{inner,..}=>match inner.as_ref(){head @ (Type::Managed(_)|Type::OwnedStore{..})=>head,_=>return Ok(false)},ty=>ty};
    let payload=match head{Type::Managed(inner)|Type::Desc(inner)=>inner.as_ref(),ty=>ty};
    let Type::OwnedStore{element}=payload else{return Ok(false);};
    target_free_value(module,element,b)
}

/// Complete ordinary data qualification, shared by owned products, sum tags
/// and Store elements. It supplies no Copy flag, owner, loan or cleanup proof.
pub(super) fn target_free_value<'a>(module:&'a TypedModule,ty:&'a Type,b:&mut Budget)->Check<bool>{
    target_free_tree(module, ty, false, b)
}

pub(super) fn target_free_product_or_sum(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    // This lane serves Tuple and Enum transport. Store and the other
    // consumers of the ordinary value test retain their own admissible leaves.
    let closed_erasure = matches!(ty, Type::Tuple(_) | Type::Enum { .. });
    target_free_tree(module, ty, closed_erasure, b)
}

fn target_free_tree(
    module: &TypedModule,
    ty: &Type,
    closed_erasure: bool,
    b: &mut Budget,
) -> Check<bool> {
    stored_flow::type_work(ty,&b.limits.clone(),b)?;
    let mut pending=vec![(ty, 0usize, closed_erasure)];
    while let Some((ty, depth, closed_erasure))=pending.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("ordinary Store target domain depth exceeded"));}
        match ty{
            Type::BorrowedStorage{storage,life}=>{
                if life.binder_count!=0||!life.slots.is_empty()||!life.retained.is_empty()||!life.nominal_arguments.is_empty(){return Ok(false);}
                // An empty annotation is not its declaration/actual proof.
                super::stored_flow::qualification(module,ty,&b.limits.clone(),b)?;
                pending.push((storage, depth + 1, false));
            },
            Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1, false)));},
            Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|ty| (ty, depth + 1, closed_erasure)));},
            Type::Enum{variants,..}=>{b.spend(variants.len())?;for(_,fields)in variants{b.spend(fields.len())?;pending.extend(fields.iter().map(|ty| (ty, depth + 1, closed_erasure)));}},
            Type::Managed(inner)|Type::Array{element:inner,..}|Type::OwnedStore{element:inner}=>pending.push((inner, depth + 1, false)),
            Type::ErrorUnion{error,result}=>{pending.push((error, depth + 1, false));pending.push((result, depth + 1, false));},
            Type::ByteBuffer=>{},
            Type::OwnedDyn { interface } if closed_erasure => {
                if super::dynamic_ports::interface(module, *interface, b)?.context.is_some() {
                    return Ok(false);
                }
                super::dynamic_dispatch::closed_owned(module, *interface, b)?;
            },
            Type::Closure(id)=>{
                // A real complete closure directory determines its persistent
                // κ domain. Invocation parameter borrows are not stored data;
                // captured pointers, including Static ones, still disqualify
                // this target-free lane when traversed below.
                let(count,_pending_sources)=super::dynamic_witness::intrinsic_domain(module,ty,b)?;
                if count!=0{return Ok(false);}
                let definition=super::closures::definition(module,*id,None)?;
                b.spend(definition.captures.len())?;
                pending.extend(definition.captures.iter().map(|capture| (capture, depth + 1, false)));
            },
            Type::FunctionPointer{..}=>if !super::class_flow::plain_pointer_type(ty,b)?{return Ok(false);},
            Type::ForeignCallback{parameters,result}=>{resources::charge_payload(ty,b)?;super::native_values::callback_signature(parameters,result,None)?;},
            ty if stored_callables::scalar(ty)=>{},
            _=>return Ok(false),
        }
    }
    Ok(true)
}

/// Type-authenticate each real Store operation before selecting its independent
/// provenance transfer. All resource state/capacity/loan/cleanup checks remain
/// mandatory; this selector supplies none of those permissions.
pub(super) fn ordinary_store_instruction(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<bool>{
    let candidate=match &i.kind{
        InstructionKind::OwnedStoreNew{..}=>result(f,i)?,
        InstructionKind::OwnedStoreReserve{store,..}|InstructionKind::OwnedStorePush{store,..}|InstructionKind::OwnedStorePop{store,..}|InstructionKind::OwnedStoreLen{store}|InstructionKind::OwnedStoreCapacity{store}|InstructionKind::OwnedStoreBorrow{store,..}|InstructionKind::OwnedStoreBorrowMut{store,..}=>actual(f,*store,i.span)?,
        InstructionKind::DescribeLocal{local:id}|InstructionKind::TakeLocal{local:id}|InstructionKind::DropManaged{local:id}=>local(f,*id,i.span)?,
        InstructionKind::DescribeCapture{index}|InstructionKind::TakeCapture{index}=>{
            let closure=f.environment.and_then(|id|module.closures.get(id.0 as usize)).ok_or_else(||bad(i.span,"ordinary Store capture has no actual environment"))?;
            if Some(closure.id)!=f.environment||closure.function!=f.id{return Err(bad(i.span,"ordinary Store capture selects another actual entry"));}
            closure.captures.get(*index as usize).ok_or_else(||bad(i.span,"ordinary Store capture ordinal is absent"))?
        },
        InstructionKind::DescribeAddress{..}|InstructionKind::DescribePackageField{..}=>result(f,i)?,
        _=>return Ok(false),
    };
    if !ordinary_store_value(module,candidate,b)?{return Ok(false);}
    crate::check_instruction(module,f,i,b)?;
    Ok(true)
}

#[cfg(test)]
mod closure_store_tests{
    use super::*;
    #[test]
    fn store_closure_lane_requires_known_zero_and_target_free_captures_raw_wire(){
        let limits=Limits::default();
        for variant in 0..4{
            let(mut module,plan)=crate::tests::baseline();
            let count=u32::from(variant==1);
            let borrowed=matches!(variant,1|2);
            let position=StoragePosition{base:StorageBase::Capture(0),path:vec![]};
            let relation=StorageLifeContract{fixed_binder_count:count,invocation_binder_count:0,bindings:if count==1{vec![StorageLifeBinding{position:position.clone(),binder:StorageBinder::Fixed(0)}]}else{vec![]},classes:vec![],static_bindings:if variant==2{vec![position]}else{vec![]}};
            let header=(variant!=3).then_some(relation);
            module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=header.clone();
            module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![if borrowed{Type::Address{mutable:false,inner:Box::new(Type::I32)}}else{Type::I32}],parameters:vec![],result:Type::I32,kind:CallableKind::Fn,storage_life:header,lending_captures:vec![]});
            let ty=Type::Managed(Box::new(Type::OwnedStore{element:Box::new(Type::Package{name:"store_subject".into(),fields:vec![("callback".into(),Type::Closure(ClosureId(0)))]})}));
            for wire in [false,true]{
                let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
                let(candidate,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
                let result=ordinary_store_value(&candidate,&ty,&mut Budget::new(&limits));
                if variant==0{assert!(result.unwrap());}
                else if variant==3{assert!(result.is_err(),"Unknown relation cannot become Known0");}
                else{assert!(!result.unwrap(),"variable and Static borrowed captures retain their actual target obligations");}
            }
        }
    }
}

// Canonical ByteBuffer has no stored child domain. This routes its existing
// ordinary primitive operations without weakening the scalar capture premise.
pub(super) fn ordinary_byte_value(ty:&Type,b:&mut Budget)->Check<bool>{
    stored_flow::type_work(ty,&b.limits.clone(),b)?;
    Ok(matches!(ty,Type::ByteBuffer)||matches!(ty,Type::Managed(inner)|Type::Desc(inner)if inner.as_ref()==&Type::ByteBuffer))
}
pub(super) fn ordinary_byte_instruction(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<bool>{
    let candidate=match &i.kind{
        InstructionKind::ByteBufferNew=>result(f,i)?,
        InstructionKind::ByteBufferReserve{buffer,..}|InstructionKind::ByteBufferPush{buffer,..}|InstructionKind::ByteBufferTruncate{buffer,..}|InstructionKind::ByteBufferLen{buffer}|InstructionKind::ByteBufferCapacity{buffer}=>actual(f,*buffer,i.span)?,
        InstructionKind::DescribeLocal{local:id}|InstructionKind::TakeLocal{local:id}|InstructionKind::DropManaged{local:id}=>local(f,*id,i.span)?,
        InstructionKind::DescribeCapture{index}|InstructionKind::TakeCapture{index}=>{
            let closure=f.environment.and_then(|id|module.closures.get(id.0 as usize)).ok_or_else(||bad(i.span,"ordinary byte capture has no actual environment"))?;
            if Some(closure.id)!=f.environment||closure.function!=f.id{return Err(bad(i.span,"ordinary byte capture selects another entry"));}
            closure.captures.get(*index as usize).ok_or_else(||bad(i.span,"ordinary byte capture ordinal is absent"))?
        },
        InstructionKind::DescribeAddress{..}|InstructionKind::DescribePackageField{..}=>result(f,i)?,
        _=>return Ok(false),
    };
    if !ordinary_byte_value(candidate,b)?{return Ok(false);}
    crate::check_instruction(module,f,i,b)?;Ok(true)
}

#[cfg(test)]
mod scalar_write_tests{
    use super::*;
    #[test]
    fn typed_capture_owner_operations_survive_mixed_routing_without_losing_once_or_full_type_checks(){
        let limits=Limits::default();
        for attack in 0..4{
            let(mut module,plan)=crate::tests::baseline();
            let owner=Type::Managed(Box::new(Type::Record{name:"OwnedScalar".into(),fields:vec![("value".into(),Type::I32)]}));
            let header=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:0,bindings:vec![],classes:vec![],static_bindings:vec![]};
            module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=Some(header.clone());module.functions[0].values=vec![if attack==2{Type::I32}else{owner.clone()}];
            module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![owner],parameters:vec![],result:Type::I32,kind:if attack==1{CallableKind::FnMut}else{CallableKind::FnOnce},storage_life:Some(header),lending_captures:vec![]});
            let instruction=Instruction{result:Some(ValueId(0)),kind:InstructionKind::TakeCapture{index:if attack==3{1}else{0}},span:module.functions[0].span};
            for wire in [false,true]{
                let decoded;let module=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};
                let mut b=Budget::new(&limits);b.task_failure_carriers=true;b.stored_package_instance=true;
                assert!(b.masked_stored_tasks());
                let result=operation(module,&module.functions[0],&instruction,&mut b);
                if attack==0{assert!(result.unwrap());}else{assert!(result.is_err(),"owner capture attack {attack}, wire {wire}");}
            }
        }
    }
    #[test]
    fn description_write_checks_the_complete_copy_field_in_raw_and_wire() {
        let limits = Limits::default();
        for aggregate in [false, true] {
            let field_shape = |leaf| {
                if aggregate {
                    Type::Tuple(vec![Type::Array { element: Box::new(leaf), length: 2 }, Type::Bool])
                } else {
                    leaf
                }
            };
            for attack in 0..5 {
                let (mut module, plan) = crate::tests::baseline();
                let f = &mut module.functions[0];
                let field = field_shape(if attack == 4 {
                    Type::Managed(Box::new(Type::I32))
                } else {
                    Type::I32
                });
                f.values = vec![
                    Type::Desc(Box::new(Type::Record {
                        name: "Owner".into(),
                        fields: vec![("value".into(), field)],
                    })),
                    field_shape(Type::I32),
                    field_shape(Type::U32),
                ];
                let operation = Instruction {
                    result: (attack == 3).then_some(ValueId(1)),
                    kind: InstructionKind::WriteDesc {
                        desc: ValueId(0),
                        index: if attack == 1 { 99 } else { 0 },
                        value: ValueId(if attack == 2 { 2 } else { 1 }),
                    },
                    span: f.span,
                };
                let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
                let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
                for current in [&module, &decoded] {
                    let f = &current.functions[0];
                    assert_eq!(
                        description(&f.values[0], &mut Budget::new(&limits)).unwrap(),
                        attack != 4,
                    );
                    let classified =
                        super::operation(current, f, &operation, &mut Budget::new(&limits));
                    match attack {
                        0 => assert!(classified.unwrap()),
                        4 => assert!(!classified.unwrap()),
                        _ => assert!(classified.is_err()),
                    }
                    let result = types(current, f, &operation, &mut Budget::new(&limits));
                    assert_eq!(result.is_ok(), attack == 0, "aggregate {aggregate}, attack {attack}");
                }
            }
        }
    }
}
