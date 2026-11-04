//! Actual stored targets are distinct from the bytes containing their pointers.
//! This pass does not use a candidate, source label, or nominal name as a root.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Target {
    pub base: HoldRoot,
    /// Empty is an ordinary input pointee. A nonempty path is an input's
    /// *stored target*, never a byte offset into that input's carrier.
    pub stored: Vec<PlaceProjection>,
}
type Targets = BTreeSet<Target>;
type Loans = BTreeSet<usize>;
type Path = Vec<PlaceProjection>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Permission {
    targets: Targets,
    loans: Loans,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Value {
    pointer: Permission,
    slots: BTreeMap<Path, Permission>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    locals: BTreeMap<LocalId, Value>,
    /// External addressed carriers have real cells too. Parameter position is
    /// not their target identity; each leaf starts with a symbolic target.
    external: BTreeMap<u32, BTreeMap<Path, Permission>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Loan {
    targets: Targets,
    parents: Loans,
    mutable: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Facts {
    pub values: Vec<Vec<Target>>,
    pub returned: BTreeMap<Path, Vec<Target>>,
}

fn failure(text: &str) -> Vec<Diagnostic> {
    bad(format!("stored target proof: {text}"))
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "stored target CFG proof")
}
fn ordered(b: &mut Budget, n: usize, width: usize) -> CompileResult<()> {
    b.product(
        n.checked_ilog2().map_or(1, |n| n as usize + 2),
        width.saturating_add(1),
        1,
        "stored path comparison/search",
    )
}
fn target_work(t: &Targets, b: &mut Budget) -> CompileResult<()> {
    fee(b, t.len())?;
    for target in t {
        fee(b, target.stored.len())?;
    }
    Ok(())
}
fn union(to: &mut Permission, from: &Permission, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for target in &from.targets {
        ordered(b, to.targets.len(), target.stored.len())?;
        fee(b, target.stored.len().saturating_add(1))?;
        changed |= to.targets.insert(target.clone());
    }
    for loan in &from.loans {
        ordered(b, to.loans.len(), 0)?;
        changed |= to.loans.insert(*loan);
    }
    Ok(changed)
}
fn merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = union(&mut to.pointer, &from.pointer, b)?;
    for (path, value) in &from.slots {
        ordered(b, to.slots.len(), path.len())?;
        fee(b, path.len().saturating_add(1))?;
        changed |= union(to.slots.entry(path.clone()).or_default(), value, b)?;
    }
    Ok(changed)
}
fn snapshot(value: &Value, b: &mut Budget) -> CompileResult<Value> {
    let mut result = Value::default();
    merge(&mut result, value, b)?;
    Ok(result)
}
fn state_copy(s: &State, b: &mut Budget) -> CompileResult<State> {
    fee(b, s.locals.len().saturating_add(s.external.len()))?;
    let mut out = State::default();
    for (local, value) in &s.locals {
        out.locals.insert(*local, snapshot(value, b)?);
    }
    for (index, slots) in &s.external {
        let value = Value { pointer: Permission::default(), slots: slots.clone() };
        // Charge the deep snapshot before retaining any external state.
        let mut charged = Value::default();
        merge(&mut charged, &value, b)?;
        out.external.insert(*index, charged.slots);
    }
    Ok(out)
}
fn state_merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<()> {
    for (local, value) in &from.locals {
        ordered(b, to.locals.len(), 0)?;
        merge(to.locals.entry(*local).or_default(), value, b)?;
    }
    for (index, slots) in &from.external {
        ordered(b, to.external.len(), 0)?;
        let target = to.external.entry(*index).or_default();
        for (path, permission) in slots {
            ordered(b, target.len(), path.len())?;
            fee(b, path.len().saturating_add(1))?;
            union(target.entry(path.clone()).or_default(), permission, b)?;
        }
    }
    Ok(())
}

fn carrier(ty: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::BorrowedStorage { storage, life } = ty { Some((storage, life)) } else { None }
}
fn pointed_carrier(ty: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::Address { inner, .. } = ty { carrier(inner) } else { None }
}
fn value_type(f: &TypedFunction, id: ValueId) -> CompileResult<&Type> {
    f.values.get(id.0 as usize).ok_or_else(|| failure("value outside actual table"))
}
fn slot<'a>(ty: &'a Type, path: &[PlaceProjection], b: &mut Budget) -> CompileResult<&'a Type> {
    let mut ty = ty;
    for part in path {
        fee(b, 1)?;
        ty = match (ty, part) {
            (Type::Record { fields, .. } | Type::Package { fields, .. }, PlaceProjection::Field(i)) => fields.get(*i as usize).map(|(_,t)|t),
            (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
            (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants.get(p.variant as usize).and_then(|(_, fs)|fs.get(p.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => Some(if *e { error } else { result }),
            _ => None,
        }.ok_or_else(|| failure("physical path crosses an absent or non-inline ancestor"))?;
    }
    Ok(ty)
}

/// This first executable slice admits finite physical shared leaves. Other
/// common retained carriers remain closed instead of acquiring guessed roots.
pub(super) fn physical(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Some((storage, life)) = carrier(ty) else { return Ok(false) };
    fee(b, life.slots.len().saturating_add(1))?;
    if !matches!(storage, Type::Record { .. } | Type::Tuple(_))
        || !life.retained.is_empty() || !life.nominal_arguments.is_empty()
    {
        return Err(vec![Diagnostic::unsupported(Phase::Analyze,"nested retained/nominal stored carriers require their independent consumer",None)]);
    }
    let mut actual = Vec::new();
    fn walk(ty: &Type, path: &mut Path, out: &mut Vec<Path>, b: &mut Budget) -> CompileResult<()> {
        fee(b, 1)?;
        match ty {
            Type::Address { mutable: false, inner } => {
                b.type_work(inner, true, "stored shared actual pointee")?;
                if !inner.is_address_pointee() { return Err(failure("stored shared leaf has an ineligible actual pointee")) }
                fee(b, path.len().saturating_add(1))?;
                out.push(path.clone());
            }
            Type::Record { fields, .. } => for (i,(_,t)) in fields.iter().enumerate() {
                fee(b,1)?; path.push(PlaceProjection::Field(i as u32)); walk(t,path,out,b)?; path.pop();
            },
            Type::Tuple(fields) => for (i,t) in fields.iter().enumerate() {
                fee(b,1)?; path.push(PlaceProjection::Field(i as u32)); walk(t,path,out,b)?; path.pop();
            },
            t => { b.type_work(t,true,"stored ordinary physical field")?; if !t.is_copy_value() { return Err(vec![Diagnostic::unsupported(Phase::Analyze,"nonshared physical stored leaf is not yet implemented",None)]) } }
        }
        Ok(())
    }
    walk(storage,&mut Vec::new(),&mut actual,b)?;
    if actual.len()!=life.slots.len() || actual.is_empty() { return Err(failure("physical stored slot coverage is incomplete")) }
    let mut seen = BTreeSet::new();
    let mut next = 0u32;
    for (path,decl) in actual.iter().zip(&life.slots) {
        fee(b,path.len().saturating_add(decl.path.len()))?;
        if path!=&decl.path || decl.mode!=StoredBorrowMode::SharedAddress || decl.binder>=life.binder_count { return Err(failure("stored slot order, mode or binder differs from actual fields")) }
        ordered(b,seen.len(),0)?;
        if seen.insert(decl.binder) { if decl.binder!=next { return Err(failure("stored binders are not dense by first occurrence")) } next=next.checked_add(1).ok_or_else(||failure("binder overflow"))?; }
    }
    if next!=life.binder_count { return Err(failure("stored binder domain has unused classes")) }
    Ok(true)
}

fn seed(f: &TypedFunction, loans: &mut Vec<Loan>, b: &mut Budget) -> CompileResult<State> {
    let mut state=State::default();
    for (index,local) in f.parameters.iter().enumerate() {
        fee(b,1)?;
        let ty=&f.locals[local.0 as usize].ty;
        let mut value=Value::default();
        if let Type::Address { mutable, .. }=ty {
            let target=Target{base:HoldRoot::Parameter(index as u32),stored:Vec::new()};
            let id=loans.len(); fee(b,2)?;
            loans.push(Loan{targets:BTreeSet::from([target.clone()]),parents:Loans::new(),mutable:*mutable});
            value.pointer=Permission{targets:BTreeSet::from([target]),loans:Loans::from([id])};
        }
        let qualified=carrier(ty).or_else(||pointed_carrier(ty));
        if let Some((_,life))=qualified {
            let mut slots=BTreeMap::new();
            for decl in &life.slots {
                fee(b,decl.path.len().saturating_mul(3).saturating_add(3))?;
                let target=Target{base:HoldRoot::Parameter(index as u32),stored:decl.path.clone()};
                let id=loans.len();
                loans.push(Loan{targets:BTreeSet::from([target.clone()]),parents:Loans::new(),mutable:false});
                slots.insert(decl.path.clone(),Permission{targets:BTreeSet::from([target]),loans:Loans::from([id])});
            }
            if pointed_carrier(ty).is_some() { state.external.insert(index as u32,slots); } else { value.slots=slots; }
        }
        state.locals.insert(*local,value);
    }
    Ok(state)
}

fn select(state: &State, address: &Permission, path: &[PlaceProjection], b: &mut Budget) -> CompileResult<Permission> {
    if address.targets.is_empty() { return Err(failure("stored selection has no actual carrier location")) }
    let mut out=Permission::default();
    for root in &address.targets {
        fee(b,root.stored.len().saturating_add(1))?;
        if !root.stored.is_empty() { return Err(failure("stored target is not an inline carrier address")) }
        let slots=match root.base {
            HoldRoot::Local(local)=> { ordered(b,state.locals.len(),0)?; &state.locals.get(&local).ok_or_else(||failure("carrier local is unavailable"))?.slots },
            HoldRoot::Parameter(index)=> { ordered(b,state.external.len(),0)?; state.external.get(&index).ok_or_else(||failure("external carrier has no actual slot facts"))? },
            HoldRoot::Capture(_)=>return Err(vec![Diagnostic::unsupported(Phase::Analyze,"captured stored carrier facts are not yet implemented",None)]),
        };
        ordered(b,slots.len(),path.len())?;
        let leaf=slots.get(path).ok_or_else(||failure("stored leaf is unavailable or uninitialized"))?;
        union(&mut out,leaf,b)?;
    }
    if out.targets.is_empty() { return Err(failure("stored leaf has no actual target")) }
    Ok(out)
}

fn roots(value: &Value, b: &mut Budget) -> CompileResult<Targets> {
    let mut permission=Permission::default();
    union(&mut permission,&value.pointer,b)?;
    for leaf in value.slots.values() { union(&mut permission,leaf,b)?; }
    Ok(permission.targets)
}

fn substitute(source: &Target, arguments: &[ValueId], values: &[Value], state: &State, b: &mut Budget) -> CompileResult<Permission> {
    let HoldRoot::Parameter(index)=source.base else { return Err(failure("callee result retains activation-local or captured storage")) };
    let actual=*arguments.get(index as usize).ok_or_else(||failure("callee source parameter has no actual argument"))?;
    let value=values.get(actual.0 as usize).ok_or_else(||failure("call actual is outside values"))?;
    if source.stored.is_empty() { let mut out=Permission::default(); union(&mut out,&value.pointer,b)?; Ok(out) }
    else if !value.slots.is_empty() {
        ordered(b,value.slots.len(),source.stored.len())?;
        let mut out=Permission::default(); union(&mut out,value.slots.get(&source.stored).ok_or_else(||failure("actual carrier lacks the callee's returned leaf"))?,b)?; Ok(out)
    } else { select(state,&value.pointer,&source.stored,b) }
}

fn transfer(module: &TypedModule, f: &TypedFunction, i: &Instruction, state: &mut State, values: &[Value], summaries: &[Facts], b: &mut Budget) -> CompileResult<Value> {
    fee(b,1)?;
    let mut out=Value::default();
    let get=|id:ValueId|values.get(id.0 as usize).ok_or_else(||failure("operand is outside actual values"));
    match &i.kind {
        InstructionKind::Load(local)=> { ordered(b,state.locals.len(),0)?; out=snapshot(state.locals.get(local).ok_or_else(||failure("read of unavailable carrier/local"))?,b)?; },
        InstructionKind::Store{local,value}=> { ordered(b,state.locals.len(),0)?; state.locals.insert(*local,snapshot(get(*value)?,b)?); },
        InstructionKind::StorageLive(local)|InstructionKind::StorageDead(local)=> { ordered(b,state.locals.len(),0)?; state.locals.remove(local); },
        InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local}=> {
            fee(b,1)?; out.pointer.targets.insert(Target{base:HoldRoot::Local(*local),stored:Vec::new()});
        }
        InstructionKind::ReborrowAddress{address,..}|InstructionKind::AddressField{address,..}|InstructionKind::AddressIndex{address,..}|InstructionKind::AddressSlice{address,..}=> { union(&mut out.pointer,&get(*address)?.pointer,b)?; },
        InstructionKind::MakeAggregate{fields}=> {
            let ty=value_type(f,i.result.ok_or_else(||failure("aggregate lacks result"))?)?;
            let Some((storage,life))=carrier(ty) else { return Ok(out) };
            for decl in &life.slots {
                let Some((PlaceProjection::Field(index),tail))=decl.path.split_first() else { return Err(failure("aggregate slot has no physical field")) };
                let source=get(*fields.get(*index as usize).ok_or_else(||failure("aggregate omits borrowed field"))?)?;
                let mut permission=Permission::default();
                if tail.is_empty() { union(&mut permission,&source.pointer,b)?; } else {
                    ordered(b,source.slots.len(),tail.len())?;
                    union(&mut permission,source.slots.get(tail).ok_or_else(||failure("nested aggregate omits stored field"))?,b)?;
                }
                if permission.targets.is_empty() { return Err(failure("constructed stored field has no actual target")) }
                if !matches!(slot(storage,&decl.path,b)?,Type::Address{mutable:false,..}) { return Err(failure("constructed stored field is not shared")) }
                ordered(b,out.slots.len(),decl.path.len())?; fee(b,decl.path.len())?;
                out.slots.insert(decl.path.clone(),permission);
            }
        }
        InstructionKind::LoadStoredBorrow{storage,path,mode:StoredBorrowLoad::CopyShared}=> {
            out.pointer=select(state,&get(*storage)?.pointer,path,b)?;
        }
        InstructionKind::LoadStoredBorrow{..}|InstructionKind::StoreStoredBorrow{..}=> return Err(vec![Diagnostic::unsupported(Phase::Analyze,"stored reborrow/take/replacement CFG consumer is not yet implemented",Some(i.span))]),
        InstructionKind::Call{function,arguments}=> {
            let callee=module.functions.get(function.0 as usize).ok_or_else(||failure("call target absent"))?;
            let summary=summaries.get(function.0 as usize).ok_or_else(||failure("callee stored summary absent"))?;
            for (path,sources) in &summary.returned {
                let mut permission=Permission::default();
                for source in sources { union(&mut permission,&substitute(source,arguments,values,state,b)?,b)?; }
                if path.is_empty() { out.pointer=permission; } else { fee(b,path.len())?; out.slots.insert(path.clone(),permission); }
            }
            if callee.foreign.is_some() || callee.environment.is_some() || callee.call_kind!=CallKind::Direct { return Err(failure("stored call requires an independently checked ordinary direct body")) }
        }
        InstructionKind::Constant(_)|InstructionKind::Unary{..}|InstructionKind::Binary{..}|InstructionKind::ReadAddress{..}|InstructionKind::WriteAddress{..}|InstructionKind::SliceLength{..}|InstructionKind::ScopeEnter{..}|InstructionKind::ScopeExit{..}=> {},
        _=>return Err(vec![Diagnostic::unsupported(Phase::Analyze,"instruction has no common stored-target transfer yet",Some(i.span))]),
    }
    Ok(out)
}

/// Path-aware return solving is separate from the legacy parameter-only
/// reference summaries. No declared life bound fabricates a nonempty result.
pub(super) fn derive(module: &TypedModule, graphs: &[(Vec<Vec<usize>>,Vec<bool>)], b: &mut Budget) -> CompileResult<Vec<Facts>> {
    fee(b,module.functions.len())?;
    let mut summaries=vec![Facts::default();module.functions.len()];
    loop {
        let mut changed=false;
        for (index,f) in module.functions.iter().enumerate() {
            let (preds,reachable)=&graphs[index];
            fee(b,f.values.len().saturating_add(f.blocks.len()))?;
            let mut loans=Vec::new();
            let entry=seed(f,&mut loans,b)?;
            let mut values=vec![Value::default();f.values.len()];
            let mut outputs=vec![State::default();f.blocks.len()];
            loop {
                let mut local_changed=false;
                for (bi,block) in f.blocks.iter().enumerate() {
                    fee(b,1)?; if !reachable[bi] { continue }
                    let mut state=if bi==f.entry.0 as usize { state_copy(&entry,b)? } else { State::default() };
                    for pred in &preds[bi] { fee(b,1)?; if reachable[*pred] { state_merge(&mut state,&outputs[*pred],b)?; } }
                    for instruction in &block.instructions {
                        let out=transfer(module,f,instruction,&mut state,&values,&summaries,b)?;
                        if let Some(id)=instruction.result { local_changed|=merge(&mut values[id.0 as usize],&out,b)?; }
                    }
                    fee(b,state.locals.len().saturating_add(outputs[bi].locals.len()))?;
                    if state!=outputs[bi] { outputs[bi]=state; local_changed=true; }
                }
                if !local_changed { break }
            }
            let mut actual=Facts{values:Vec::with_capacity(values.len()),returned:BTreeMap::new()};
            for value in &values { let root=roots(value,b)?; fee(b,root.len())?; actual.values.push(root.into_iter().collect()); }
            for (block,live) in f.blocks.iter().zip(reachable) {
                fee(b,1)?;
                if *live && let Terminator::Return(Some(id))=block.terminator {
                    let value=&values[id.0 as usize];
                    let mut add=|path:&Path,permission:&Permission|->CompileResult<()> {
                        ordered(b,actual.returned.len(),path.len())?; fee(b,path.len())?;
                        let row=actual.returned.entry(path.clone()).or_default();
                        for target in &permission.targets {
                            fee(b,target.stored.len().saturating_mul(row.len().saturating_add(1)).saturating_add(1))?;
                            if !matches!(target.base,HoldRoot::Parameter(_)) { return Err(failure("stored return would retain an activation-local target")) }
                            if !row.contains(target) { row.push(target.clone()); }
                        }
                        Ok(())
                    };
                    if matches!(f.return_type,Type::Address{..}) { add(&Vec::new(),&value.pointer)?; }
                    for (path,permission) in &value.slots { add(path,permission)?; }
                }
            }
            if actual!=summaries[index] { summaries[index]=actual; changed=true; }
        }
        if !changed { break }
    }
    Ok(summaries)
}
