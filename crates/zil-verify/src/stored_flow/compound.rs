//! Reconstruct inline Tuple permissions from actual child layouts. Local
//! class ordinals remain child-local; physical paths distinguish their loans.
use super::*;

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
        InstructionKind::UnpackTuple { local, .. } | InstructionKind::DropTuple { local } => {
            &f.locals[local.0 as usize].ty
        }
        _ => return Ok(false),
    };
    if !matches!(ty, Type::Tuple(_)) || slots(ty, b)?.is_none() {
        return Ok(false);
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
        if matches!(child,Type::Address{..}){
            let actual=&values.get(value.0 as usize).ok_or_else(||err("compound pointer field SSA is absent"))?.address;
            add_leaf(out,&[PlaceProjection::Field(index as u32)],actual,b)?;
        }
        for (tail, permission) in &values[value.0 as usize].leaves {
            b.spend(tail.len().saturating_add(1))?;
            let mut path = vec![PlaceProjection::Field(index as u32)];
            path.extend(tail);
            add_leaf(out, &path, permission, b)?;
        }
    }
    Ok(())
}

/// Project the actual physical field once for both SSA extraction and owned
/// unpacking. Only a real Address child receives an outer pointer permission;
/// a qualified/opaque child retains its complete field directory instead.
pub(super) fn field_payload(source:&Payload,index:u32,ty:&Type,b:&mut Budget)->Check<Payload>{
    let mut child=Payload::default();
    for(path,permission)in &source.leaves{
        b.spend(path.len().saturating_add(1))?;
        if let Some((PlaceProjection::Field(field),tail))=path.split_first(){
            if *field!=index{continue;}
            if tail.is_empty()&&matches!(ty,Type::Address{..}){join_permission(&mut child.address,permission,b)?;}
            else{add_leaf(&mut child,tail,permission,b)?;}
        }
    }
    Ok(child)
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
        let f=&mut module.functions[0];f.parameters.clear();f.values=vec![address.clone(),Type::I32,tuple.clone(),address.clone()];f.locals=(0..3).map(|index|LocalDecl{id:LocalId(index),name:format!("p{index}"),ty:if index==0{tuple.clone()}else if index==1{address.clone()}else{Type::I32},mutable:true,span:template.span}).collect();
        let plan=CandidatePlan::empty_for(&module);
        for wire in [false,true]{
            let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();let decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;
            let f=if wire{&decoded.functions[0]}else{&module.functions[0]};let mut b=Budget::new(&limits);
            assert_eq!(slots(&tuple,&mut b).unwrap(),Some(vec![(vec![PlaceProjection::Field(0)],StoredBorrowMode::SharedAddress)]));
            let permission=Permission{sources:vec![Source{base:HoldRoot::Parameter(0),static_dependency:StaticProvenance::NotStatic,leaf:vec![],inline:vec![],retained_port:None,heap_element:None,heap_parents:vec![]}],loans:vec![0,2]};
            let mut values=vec![Payload::default();4];values[0].address=permission.clone();
            let i=Instruction{result:Some(ValueId(2)),kind:InstructionKind::MakeAggregate{fields:vec![ValueId(0),ValueId(1)]},span:f.span};let mut made=Payload::default();construct(f,&i,&[ValueId(0),ValueId(1)],&values,&mut made,&mut b).unwrap();
            let selected=field_payload(&made,0,&address,&mut b).unwrap();assert_eq!(selected.address,permission);assert!(selected.leaves.is_empty());
            assert!(field_payload(&made,1,&Type::I32,&mut b).unwrap().address.sources.is_empty());
            let mut state=Frame{locals:vec![made,Payload::default(),Payload::default()],..Frame::default()};unpack(f,LocalId(0),&[LocalId(1),LocalId(2)],&mut state,&mut b).unwrap();assert_eq!(state.locals[1].address,permission);assert!(state.locals[0].leaves.is_empty());
            assert!(construct(f,&i,&[ValueId(1),ValueId(0)],&values,&mut Payload::default(),&mut b).is_err(),"constructor cannot swap differently typed fields");
            let retagged=Instruction{result:Some(ValueId(1)),kind:InstructionKind::ExtractField{aggregate:ValueId(2),index:0},span:f.span};
            assert!(projection(f,&retagged,&mut b).is_err(),"pointer child cannot be retagged as a scalar projection");
            assert!(state.locals[1].address.sources.iter().all(|source|source.base==HoldRoot::Parameter(0)&&source.static_dependency==StaticProvenance::NotStatic),"a physical leaf never invents a Static literal from its shape");
        }
    }
}
