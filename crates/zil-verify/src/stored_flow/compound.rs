//! Reconstruct inline Tuple permissions from actual child layouts. Local
//! class ordinals remain child-local; physical paths distinguish their loans.
use super::*;

/// Only an inline shared pointer product uses this copying lane. slots()
/// alone also describes affine/owning products and is not Copy admission.
pub(super) fn shared_value(module:&TypedModule,ty:&Type,span:Option<Span>,b:&mut Budget)->Check<bool>{
    if !matches!(ty,Type::Tuple(_)){return Ok(false);}
    let mut pending=vec![(ty,0usize)];let mut pointer=false;
    while let Some((current,depth))=pending.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("shared Tuple type qualification depth exceeded"));}
        match current{
            Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|field|(field,depth+1)));},
            Type::Address{mutable:false,..}=>{crate::check_function_signature_type(module,current,span,b)?;pointer=true;},
            other if other.is_copy_value()=>crate::check_type(other,span)?,
            _=>return Ok(false),
        }
    }
    if pointer{b.nested_callable_sources=true;}
    Ok(pointer)
}

pub(super) fn borrowed_child(ty: &Type, b: &mut Budget) -> Check<bool> {
    if let Some(actual) = anonymous_tuple::layout(ty, b)? {
        return Ok(!actual.owned);
    }
    let Type::BorrowedStorage { storage, life } = ty else {
        return Ok(false);
    };
    if !matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
        || !life.retained.is_empty()
    {
        return Ok(false);
    }
    physical(ty, &b.limits.clone(), b)?;
    b.spend(life.slots.len())?;
    Ok(life.slots.iter().all(|s| {
        matches!(
            s.mode,
            StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
        )
    }))
}

pub(super) fn operation(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    let ty = match &i.kind {
        InstructionKind::MakeOwnedTuple { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("compound constructor lacks result"))?,
        )?,
        InstructionKind::UpdateField{aggregate,..}=>vt(f,*aggregate)?,
        InstructionKind::UnpackTuple { local, .. } | InstructionKind::DropTuple { local } => {
            &f.locals[local.0 as usize].ty
        }
        _ => return Ok(false),
    };
    if !matches!(ty, Type::Tuple(_)) || slots(ty, b)?.is_none() {
        return Ok(false);
    }
    if let InstructionKind::UpdateField{index,value,..}=&i.kind{
        let Type::Tuple(children)=ty else{unreachable!()};
        let child=children.get(*index as usize).ok_or_else(||err("compound field update ordinal is absent"))?;
        let output=vt(f,i.result.ok_or_else(||err("compound field update lacks result"))?)?;
        type_work(ty,&b.limits.clone(),b)?;type_work(output,&b.limits.clone(),b)?;
        type_work(child,&b.limits.clone(),b)?;type_work(vt(f,*value)?,&b.limits.clone(),b)?;
        if output!=ty||vt(f,*value)?!=child{return Err(err("compound field update changes complete parent or child type"));}
    }
    if let InstructionKind::UnpackTuple { local, fields } = &i.kind {
        let Type::Tuple(children) = ty else {
            unreachable!()
        };
        b.spend(fields.len().saturating_mul(fields.len().saturating_add(2)))?;
        if fields.len() != children.len() {
            return Err(err("compound unpack omits actual children"));
        }
        let mut seen = BTreeSet::new();
        for (target, child) in fields.iter().zip(children) {
            type_work(child, &b.limits.clone(), b)?;
            let actual = &f
                .locals
                .get(target.0 as usize)
                .ok_or_else(|| err("compound destination is absent"))?
                .ty;
            type_work(actual, &b.limits.clone(), b)?;
            if target == local || !seen.insert(*target) || actual != child {
                return Err(err("compound unpack duplicates, aliases or retags a child"));
            }
        }
    }
    Ok(true)
}

pub(super) fn unpack(
    f:&TypedFunction,
    local: LocalId,
    fields: &[LocalId],
    state: &mut Frame,
    b: &mut Budget,
) -> Check<()> {
    let payload = copy_payload(&state.locals[local.0 as usize], b)?;
    for (index, target) in fields.iter().enumerate() {
        let ty=&f.locals.get(target.0 as usize).ok_or_else(||err("compound unpack target is absent"))?.ty;
        let child=field_payload(&payload,index as u32,ty,b)?;
        clear_local_holes(state, *target, b)?;
        state.locals[target.0 as usize] = child;
    }
    state.locals[local.0 as usize] = Payload::default();
    clear_local_holes(state, local, b)
}

pub(super) fn profile(module: &TypedModule, b: &mut Budget) -> Check<bool> {
    b.spend(module.functions.len())?;
    for f in &module.functions {
        b.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(1),
        )?;
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            if slots(ty, b)?.is_some() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(super) fn slots(ty: &Type, b: &mut Budget) -> Check<Option<Vec<(Path, StoredBorrowMode)>>> {
    if let Some(actual) = anonymous_tuple::layout(ty, b)? {
        b.spend(actual.life.slots.len().saturating_mul(2))?;
        return Ok(Some(
            actual
                .life
                .slots
                .iter()
                .map(|s| (s.path.clone(), s.mode))
                .collect(),
        ));
    }
    if !matches!(ty, Type::Tuple(_)) {
        return Ok(None);
    }
    let mut pending = vec![(ty, Vec::new(), 0usize)];
    let mut out = Vec::new();
    while let Some((current, prefix, depth)) = pending.pop() {
        b.spend(prefix.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth {
            return Err(limit("compound permission type depth exceeded"));
        }
        match current {
            Type::Tuple(children) => {
                b.spend(
                    children
                        .len()
                        .saturating_mul(prefix.len().saturating_add(2)),
                )?;
                for (index, child) in children.iter().enumerate().rev() {
                    let mut path = prefix.clone();
                    path.push(PlaceProjection::Field(index as u32));
                    pending.push((child, path, depth + 1));
                }
            }
            Type::BorrowedStorage { life, .. } => {
                // Declaration correspondence is independently checked during
                // module qualification; no producer projection table is read.
                physical(current, &b.limits.clone(), b)?;
                if !life.retained.is_empty()
                    || life.slots.iter().any(|s| {
                        !matches!(
                            s.mode,
                            StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
                        )
                    })
                {
                    return Ok(None);
                }
                for slot in &life.slots {
                    b.spend(
                        prefix
                            .len()
                            .saturating_add(slot.path.len())
                            .saturating_add(1),
                    )?;
                    let mut path = prefix.clone();
                    path.extend(&slot.path);
                    out.push((path, slot.mode));
                }
            }
            Type::Address{mutable, ..}=>{
                // A physical pointer field keeps its actual permission. This
                // enumerates no variable class and proves no Static source.
                out.push((prefix,if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress}));
            },
            Type::Managed(payload) if payload.is_copy_value() => {}
            other if other.is_copy_value() => {}
            _ => return Ok(None),
        }
    }
    Ok((!out.is_empty()).then_some(out))
}

pub(super) fn projection(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    let InstructionKind::ExtractField { aggregate, index } = i.kind else {
        return Ok(false);
    };
    let parent = vt(f, aggregate)?;
    if !matches!(parent, Type::Tuple(_)) || slots(parent, b)?.is_none() {
        return Ok(false);
    }
    let Type::Tuple(children) = parent else {
        unreachable!()
    };
    let child = children
        .get(index as usize)
        .ok_or_else(|| err("compound field ordinal is absent"))?;
    type_work(child, &b.limits.clone(), b)?;
    let actual = vt(
        f,
        i.result
            .ok_or_else(|| err("compound projection lacks result"))?,
    )?;
    type_work(actual, &b.limits.clone(), b)?;
    if actual != child {
        return Err(err(
            "compound projection changes its complete actual child type",
        ));
    }
    Ok(true)
}

pub(super) fn construct(
    f: &TypedFunction,
    i: &Instruction,
    fields: &[ValueId],
    values: &[Payload],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    let result = vt(
        f,
        i.result
            .ok_or_else(|| err("compound construction lacks result"))?,
    )?;
    if !matches!(result, Type::Tuple(_)) || slots(result, b)?.is_none() {
        return Ok(());
    }
    let Type::Tuple(children) = result else {
        unreachable!()
    };
    b.spend(fields.len().saturating_add(children.len()))?;
    if fields.len() != children.len() {
        return Err(err("compound construction arity differs"));
    }
    for (index, (value, child)) in fields.iter().zip(children).enumerate() {
        type_work(child, &b.limits.clone(), b)?;
        type_work(vt(f, *value)?, &b.limits.clone(), b)?;
        if vt(f, *value)? != child {
            return Err(err("compound construction changes actual child type"));
        }
        value_transport::attach(out,&[PlaceProjection::Field(index as u32)],child,values.get(value.0 as usize).ok_or_else(||err("compound child SSA is absent"))?,b)?;
    }
    Ok(())
}

/// Functional field replacement retains every untouched physical permission,
/// removes the complete overwritten subtree, then installs only the actual
/// replacement SSA's sources and loan parents. Static is never inferred here.
pub(super) fn update(f:&TypedFunction,i:&Instruction,values:&[Payload],b:&mut Budget)->Check<Payload>{
    if !operation(f,i,b)?{return Err(err("field update lacks its complete compound operation"));}
    let InstructionKind::UpdateField{aggregate,index,value}=i.kind else{return Err(err("compound update selects another actual opcode"));};
    let source=values.get(aggregate.0 as usize).ok_or_else(||err("compound update source SSA is absent"))?;
    let replacement=values.get(value.0 as usize).ok_or_else(||err("compound update replacement SSA is absent"))?;
    let mut out=copy_payload(source,b)?;
    let Type::Tuple(children)=vt(f,aggregate)? else{unreachable!()};
    value_transport::replace(&mut out,&[PlaceProjection::Field(index)],&children[index as usize],replacement,b)?;
    Ok(out)
}

/// Project the actual physical field once for both SSA extraction and owned
/// unpacking. Only a real Address child receives an outer pointer permission;
/// a qualified/opaque child retains its complete field directory instead.
pub(super) fn field_payload(source:&Payload,index:u32,ty:&Type,b:&mut Budget)->Check<Payload>{
    value_transport::project(source,&[PlaceProjection::Field(index)],ty,b)
}

pub(super) fn seed(
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    // Top-level qualified carriers already have their independently seeded
    // slot loans. This path only seeds child domains of an ordinary Tuple.
    if !matches!(ty, Type::Tuple(_)) {
        return Ok(());
    }
    let Some(slots) = slots(ty, b)? else {
        return Ok(());
    };
    for (path, mode) in slots {
        b.spend(path.len().saturating_mul(3).saturating_add(4))?;
        let source = Source {
            static_dependency: StaticProvenance::NotStatic,
            base,
            retained_port: None,
            leaf: path.clone(),
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        };
        let id = loans.len();
        let exclusive = mode == StoredBorrowMode::ExclusiveAddress;
        loans.push(Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: Vec::new(),
            exclusive,
        });
        b.stored_exclusive_loans |= exclusive;
        add_leaf(
            out,
            &path,
            &Permission {
                sources: vec![source],
                loans: vec![id],
            },
            b,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod physical_tests{
    use super::*;
    #[test]
    fn raw_tuple_pointer_fields_preserve_actual_source_and_loan_parents_through_construct_project_and_unpack_raw_wire(){
        let limits=Limits::default();let(mut module,_)=crate::tests::baseline();let template=module.functions[0].clone();
        let address=Type::Address{mutable:false,inner:Box::new(Type::I32)};let tuple=Type::Tuple(vec![address.clone(),Type::I32]);
        let f=&mut module.functions[0];f.parameters.clear();f.values=vec![address.clone(),Type::I32,tuple.clone(),address.clone(),tuple.clone(),tuple.clone(),address.clone()];f.locals=(0..3).map(|index|LocalDecl{id:LocalId(index),name:format!("p{index}"),ty:if index==0{tuple.clone()}else if index==1{address.clone()}else{Type::I32},mutable:true,span:template.span}).collect();
        let plan=CandidatePlan::empty_for(&module);
        for wire in [false,true]{
            let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();let decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;
            let f=if wire{&decoded.functions[0]}else{&module.functions[0]};let mut b=Budget::new(&limits);
            assert_eq!(slots(&tuple,&mut b).unwrap(),Some(vec![(vec![PlaceProjection::Field(0)],StoredBorrowMode::SharedAddress)]));
            let permission=Permission{sources:vec![Source{base:HoldRoot::Parameter(0),static_dependency:StaticProvenance::NotStatic,leaf:vec![],inline:vec![],retained_port:None,heap_element:None,heap_parents:vec![]}],loans:vec![0,2]};
            let mut values=vec![Payload::default();7];values[0].address=permission.clone();
            let i=Instruction{result:Some(ValueId(2)),kind:InstructionKind::MakeAggregate{fields:vec![ValueId(0),ValueId(1)]},span:f.span};let mut made=Payload::default();construct(f,&i,&[ValueId(0),ValueId(1)],&values,&mut made,&mut b).unwrap();
            let selected=field_payload(&made,0,&address,&mut b).unwrap();assert_eq!(selected.address,permission);assert!(selected.leaves.is_empty());
            // A scalar sibling update preserves the original physical pointer.
            // Replacing the pointer removes the old literal and all old loans.
            values[2]=copy_payload(&made,&mut b).unwrap();
            let scalar=Instruction{result:Some(ValueId(4)),kind:InstructionKind::UpdateField{aggregate:ValueId(2),index:1,value:ValueId(1)},span:f.span};
            let unchanged=update(f,&scalar,&values,&mut b).unwrap();assert_eq!(field_payload(&unchanged,0,&address,&mut b).unwrap().address,permission);
            let mut literal=permission.clone();literal.sources[0].base=HoldRoot::Static;literal.sources[0].static_dependency=StaticProvenance::Literal;literal.loans=vec![];
            let mut captured=Payload::default();add_leaf(&mut captured,&[PlaceProjection::Field(0)],&literal,&mut b).unwrap();values[2]=captured;
            let literal_unchanged=update(f,&scalar,&values,&mut b).unwrap();assert_eq!(field_payload(&literal_unchanged,0,&address,&mut b).unwrap().address,literal);
            let mut replacement=permission.clone();replacement.sources[0].base=HoldRoot::Parameter(1);replacement.loans=vec![4];values[6].address=replacement.clone();
            let replace=Instruction{result:Some(ValueId(5)),kind:InstructionKind::UpdateField{aggregate:ValueId(2),index:0,value:ValueId(6)},span:f.span};
            let replaced=update(f,&replace,&values,&mut b).unwrap();assert_eq!(field_payload(&replaced,0,&address,&mut b).unwrap().address,replacement,"old Static literal and loan ancestry cannot survive replacement");
            for(index,value)in [(0,ValueId(1)),(2,ValueId(6))]{let hostile=Instruction{result:Some(ValueId(5)),kind:InstructionKind::UpdateField{aggregate:ValueId(2),index,value},span:f.span};assert!(operation(f,&hostile,&mut b).is_err());}

            assert!(field_payload(&made,1,&Type::I32,&mut b).unwrap().address.sources.is_empty());
            let mut state=Frame{locals:vec![made,Payload::default(),Payload::default()],..Frame::default()};unpack(f,LocalId(0),&[LocalId(1),LocalId(2)],&mut state,&mut b).unwrap();assert_eq!(state.locals[1].address,permission);assert!(state.locals[0].leaves.is_empty());
            assert!(construct(f,&i,&[ValueId(1),ValueId(0)],&values,&mut Payload::default(),&mut b).is_err(),"constructor cannot swap differently typed fields");
            let retagged=Instruction{result:Some(ValueId(1)),kind:InstructionKind::ExtractField{aggregate:ValueId(2),index:0},span:f.span};
            assert!(projection(f,&retagged,&mut b).is_err(),"pointer child cannot be retagged as a scalar projection");
            assert!(state.locals[1].address.sources.iter().all(|source|source.base==HoldRoot::Parameter(0)&&source.static_dependency==StaticProvenance::NotStatic),"a physical leaf never invents a Static literal from its shape");
        }
    }
}


#[cfg(test)]
mod shared_boundary_tests {
    use super::*;
    use super::super::call_results::{return_parents,call_result};
    fn source(base:HoldRoot,leaf:Path)->Source{Source{base,leaf,inline:vec![],retained_port:None,heap_parents:vec![],heap_element:None,static_dependency:StaticProvenance::NotStatic}}
    fn permission(source:Source,id:usize)->Permission{Permission{sources:vec![source],loans:vec![id]}}
    fn inst(f:&TypedFunction,result:Option<u32>,kind:InstructionKind)->Instruction{Instruction{result:result.map(ValueId),kind,span:f.span}}
    fn roundtrip(module:&TypedModule)->TypedModule{let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(module,&CandidatePlan::empty_for(module),&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0}
    fn helper_header()->StorageLifeContract{
        StorageLifeContract{fixed_binder_count:0,invocation_binder_count:4,bindings:vec![
            StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(0),path:vec![]},binder:StorageBinder::Invocation(0)},
            StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(1),path:vec![]},binder:StorageBinder::Invocation(1)},
            StorageLifeBinding{position:StoragePosition{base:StorageBase::Result,path:vec![ContractProjection::Place(PlaceProjection::Field(0))]},binder:StorageBinder::Invocation(0)},
            StorageLifeBinding{position:StoragePosition{base:StorageBase::Result,path:vec![ContractProjection::Place(PlaceProjection::Field(1))]},binder:StorageBinder::Invocation(1)}],static_bindings:vec![],classes:vec![
            SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Parameter(0),path:vec![LifeTypeProjection::Pointee],class:0},binder:StorageBinder::Invocation(2)},
            SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Parameter(1),path:vec![LifeTypeProjection::Pointee],class:0},binder:StorageBinder::Invocation(3)}]}
    }
    fn module()->TypedModule{
        let(mut module,_)=crate::tests::baseline();let template=module.functions[0].clone();module.functions.clear();module.closures.clear();
        let pointer=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        for id in 0..2{
            let header=StorageLifeContract{fixed_binder_count:1,invocation_binder_count:1,bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(0),path:vec![]},binder:StorageBinder::Fixed(0)},StorageLifeBinding{position:StoragePosition{base:StorageBase::Receiver,path:vec![]},binder:StorageBinder::Invocation(0)},StorageLifeBinding{position:StoragePosition{base:StorageBase::Result,path:vec![]},binder:StorageBinder::Invocation(0)}],static_bindings:vec![],classes:vec![]};
            let mut entry=template.clone();entry.id=FunctionId(id);entry.name=format!("get{id}");entry.environment=Some(ClosureId(id));entry.parameters.clear();entry.locals.clear();entry.return_type=pointer.clone();entry.values=vec![pointer.clone()];entry.storage_life=Some(header.clone());entry.blocks.truncate(1);entry.blocks[0].instructions=vec![inst(&entry,Some(0),InstructionKind::ReadCapture{index:0})];entry.blocks[0].terminator=Terminator::Return(Some(ValueId(0)));
            module.closures.push(ClosureDefinition{id:ClosureId(id),function:entry.id,captures:vec![pointer.clone()],parameters:vec![],result:pointer.clone(),kind:CallableKind::Fn,storage_life:Some(header),lending_captures:vec![]});module.functions.push(entry);
        }
        let mut helper=template.clone();helper.id=FunctionId(2);helper.name="tuple_helper".into();helper.environment=None;helper.storage_life=Some(helper_header());helper.parameters=vec![LocalId(0),LocalId(1)];helper.return_type=Type::Tuple(vec![pointer.clone(),pointer.clone()]);helper.locals=(0..2).map(|id|LocalDecl{id:LocalId(id),name:format!("code{id}"),ty:Type::Address{mutable:false,inner:Box::new(Type::Closure(ClosureId(id)))},mutable:false,span:template.span}).collect();helper.values=vec![helper.locals[0].ty.clone(),helper.locals[1].ty.clone(),pointer.clone(),pointer.clone(),helper.return_type.clone()];helper.blocks.truncate(1);helper.blocks[0].instructions=vec![inst(&helper,Some(2),InstructionKind::CallClosureBorrowed{callee:ValueId(0),kind:CallableKind::Fn,arguments:vec![]}),inst(&helper,Some(3),InstructionKind::CallClosureBorrowed{callee:ValueId(1),kind:CallableKind::Fn,arguments:vec![]}),inst(&helper,Some(4),InstructionKind::MakeAggregate{fields:vec![ValueId(2),ValueId(3)]})];helper.blocks[0].terminator=Terminator::Return(Some(ValueId(4)));module.functions.push(helper);
        module
    }
    // Runtime-component control: entry permissions and the two leaf getter
    // summaries are input facts, not public certificates or whole-module WF.
    // The helper's two *new* head loans are created by real invocation steps.
    #[test]
    fn shared_tuple_results_preserve_each_real_callee_head_through_direct_and_all_callable_modes_raw_wire(){
        let module=module();let limits=Limits::default();
        for module in [&module,&roundtrip(&module)]{
            let f=&module.functions[2];let mut b=Budget::new(&limits);b.function_count=module.functions.len();b.closure_receiver_borrows=true;b.nested_callable_sources=true;
            let mut loans=vec![Loan::default();f.values.len()];let mut entry=Frame::default();let mut inputs=vec![Payload::default();2];
            for id in 0..2{
                let head=source(HoldRoot::Parameter(id),vec![]);let hid=loans.len();loans.push(Loan{sources:vec![head.clone()],..Loan::default()});inputs[id as usize].address=permission(head,hid);
                let data=source(HoldRoot::Parameter(id),vec![PlaceProjection::Field(0)]);let did=loans.len();loans.push(Loan{sources:vec![data.clone()],..Loan::default()});add_leaf(&mut inputs[id as usize],&[PlaceProjection::Field(0)],&permission(data,did),&mut b).unwrap();
            }
            entry.locals=inputs.clone();entry.external=inputs.clone();let mut state=entry.clone();let mut values=vec![Payload::default();f.values.len()];values[..2].clone_from_slice(&inputs);
            let sites=write_sites(module,f,&mut loans,&mut b).unwrap();let mut dyn_sites=dyn_leases::Sites::default();let mut summaries=vec![Summary::default();3];
            for id in 0..2{summaries[id].complete=true;summaries[id].result=vec![(vec![],vec![source(HoldRoot::Capture(0),vec![])])];}
            for operation in &f.blocks[0].instructions{crate::check_instruction(module,f,operation,&mut b).unwrap();let result=step(module,f,operation,&mut state,&values,&summaries,&sites,&mut dyn_sites,&mut loans,false,&mut b).unwrap();values[operation.result.unwrap().0 as usize]=result;}
            let solution=Solution{values:values.clone(),outputs:vec![state.clone()],entry:entry.clone(),loans,sites,dyn_sites};let mut summary=Summary{complete:true,..Summary::default()};summary.result=values[4].leaves.iter().map(|(path,p)|(path.clone(),p.sources.clone())).collect();return_parents(module,f,&values[4],&solution,&mut summary,&mut b).unwrap();
            for field in 0..2{
                let path=vec![PlaceProjection::Field(field)];let parents=&summary.physical_result_parents.iter().find(|(key,_)|key==&path).unwrap().1;
                assert!(parents.contains(&source(HoldRoot::Parameter(field),vec![])),"actual helper-created head must survive separately from data");
                assert!(!parents.contains(&source(HoldRoot::Parameter(1-field),vec![])),"same-type sibling cannot supply this field's head");
                assert_eq!(summary.result[field as usize].1,vec![source(HoldRoot::Parameter(field),vec![PlaceProjection::Field(0)])]);
            }
            let mut local_escape=solution;let lease=local_escape.sites[0].3;local_escape.loans[lease].sources=vec![source(HoldRoot::Local(LocalId(0)),vec![])];
            assert!(return_parents(module,f,&values[4],&local_escape,&mut Summary{complete:true,..Summary::default()},&mut b).is_err());
            let mut caller=f.clone();caller.id=FunctionId(3);caller.name="tuple_caller".into();caller.environment=None;caller.parameters.clear();caller.locals=(0..4).map(|id|LocalDecl{id:LocalId(id),name:format!("local{id}"),ty:if id<2{Type::Closure(ClosureId(id))}else{Type::I32},mutable:true,span:caller.span}).collect();caller.values=vec![f.values[0].clone(),f.values[1].clone(),f.return_type.clone()];
            let mut caller_loans=vec![Loan::default();12];let mut before=Frame{locals:vec![Payload::default();4],..Frame::default()};let mut actuals=vec![Payload::default();3];
            for id in 0..2{
                let hid=5+id as usize*2;let did=hid+1;let head=source(HoldRoot::Local(LocalId(id)),vec![]);caller_loans[hid].sources=vec![head.clone()];actuals[id as usize].address=permission(head,hid);
                let data=source(HoldRoot::Local(LocalId(id+2)),vec![]);caller_loans[did].sources=vec![data.clone()];let p=permission(data,did);add_leaf(&mut actuals[id as usize],&[PlaceProjection::Field(0)],&p,&mut b).unwrap();add_leaf(&mut before.locals[id as usize],&[PlaceProjection::Field(0)],&p,&mut b).unwrap();
            }
            let direct=inst(&caller,Some(2),InstructionKind::Call{function:FunctionId(2),arguments:vec![ValueId(0),ValueId(1)]});let mut all=summaries.clone();all[2]=summary.clone();let mut current=before.clone();let result=step(module,&caller,&direct,&mut current,&actuals,&all,&Vec::new(),&mut dyn_leases::Sites::default(),&mut caller_loans,false,&mut b).unwrap();
            for field in 0..2{let p=leaf(&result,&[PlaceProjection::Field(field)],&mut b).unwrap().unwrap();assert_eq!(p.sources,vec![source(HoldRoot::Local(LocalId(field+2)),vec![])]);assert!(p.loans.contains(&(5+field as usize*2)));assert!(!p.loans.contains(&(5+(1-field)as usize*2)));}
            for mode in 0..6{
                let mut module=module.clone();let kind=if mode==1||mode==4{CallableKind::FnMut}else if mode==3{CallableKind::FnOnce}else{CallableKind::Fn};module.functions[2].environment=Some(ClosureId(2));module.closures.push(ClosureDefinition{id:ClosureId(2),function:FunctionId(2),captures:vec![],parameters:vec![f.values[0].clone(),f.values[1].clone()],result:f.return_type.clone(),kind,storage_life:Some(helper_header()),lending_captures:vec![]});
                let mut unknown=module.clone();unknown.closures[2].storage_life=None;unknown.functions[2].storage_life=None;
                assert!(crate::dynamic_witness::intrinsic_domain(&unknown,&Type::Closure(ClosureId(2)),&mut b).is_err(),"missing opaque closure relation remains Unknown, not a complete zero-class input");
                let d=&module.closures[2];
                crate::signature_life::closure_relation_resource_scopes(&module,&d.parameters,&d.captures,d.kind,&d.result,d.storage_life.as_ref().unwrap(),None,&mut b).unwrap();
                let mut caller=caller.clone();caller.locals.push(LocalDecl{id:LocalId(4),name:"receiver".into(),ty:Type::Closure(ClosureId(2)),mutable:true,span:caller.span});
                let mut actuals=actuals.clone();actuals[2]=Payload::default();caller.values[2]=if mode==3||mode==5{Type::Closure(ClosureId(2))}else{Type::Address{mutable:mode==1,inner:Box::new(Type::Closure(ClosureId(2)))}};caller.values.push(f.return_type.clone());
                let operation=inst(&caller,Some(3),if mode==3{InstructionKind::CallClosureOnce{callee:ValueId(2),arguments:vec![ValueId(0),ValueId(1)]}}else if mode==5{InstructionKind::CallClosure{callee:ValueId(2),arguments:vec![ValueId(0),ValueId(1)]}}else if mode==2{InstructionKind::CallClosureShared{callee:LocalId(4),arguments:vec![ValueId(0),ValueId(1)]}}else if mode==4{InstructionKind::CallClosureMut{callee:LocalId(4),arguments:vec![ValueId(0),ValueId(1)]}}else{InstructionKind::CallClosureBorrowed{callee:ValueId(2),kind,arguments:vec![ValueId(0),ValueId(1)]}});
                actuals[2].address=permission(source(HoldRoot::Local(LocalId(4)),vec![]),9);let mut before=before.clone();before.locals.push(Payload::default());let mut out=Payload::default();assert!(call_result(&module,&caller,&operation,&mut before,&actuals,&all,&Vec::new(),&mut caller_loans,&mut out,&mut b).unwrap());
                assert_eq!(out.leaves,result.leaves,"mode {mode} must transport the same separately grounded data and parents");
                let mut missing=all.clone();missing[2].physical_result_parents.pop();assert!(call_result(&module,&caller,&operation,&mut before,&actuals,&missing,&Vec::new(),&mut caller_loans,&mut Payload::default(),&mut b).is_err());
            }
        }
    }
    #[test]
    fn shared_tuple_type_eligibility_keeps_copy_ct_owner_and_exclusive_consumers_separate(){
        let(module,_)=crate::tests::baseline();let limits=Limits::default();let pointer=Type::Address{mutable:false,inner:Box::new(Type::Slice{element:Box::new(Type::U8)})};let tuple=Type::Tuple(vec![pointer.clone(),Type::I32]);let mut b=Budget::new(&limits);
        assert!(shared_value(&module,&tuple,None,&mut b).unwrap());assert!(b.nested_callable_sources);assert!(!tuple.is_copy_value());assert!(crate::check_type(&tuple,None).is_err());
        assert!(!shared_value(&module,&Type::Tuple(vec![Type::Address{mutable:true,inner:Box::new(Type::I32)}]),None,&mut b).unwrap());assert!(!shared_value(&module,&Type::Tuple(vec![pointer,Type::Managed(Box::new(Type::I32))]),None,&mut b).unwrap());
    }
}
