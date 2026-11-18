//! Private, per-drop backwards obligations. A Ready edge is useful only for
//! its actual canonical close result and an unchanged receiver occupant.
//! No result of this pass is a type-level NoExternal or Transfer certificate.
use super::*;

type Point = (usize, usize);

struct Code<'a> {
    f: &'a TypedFunction,
    definitions: Vec<Option<Point>>,
    predecessors: Vec<Vec<usize>>,
    reachable: Vec<bool>,
}

impl<'a> Code<'a> {
    fn new(f: &'a TypedFunction, b: &mut Budget) -> Check<Self> {
        b.spend(
            f.values
                .len()
                .saturating_add(f.blocks.len().saturating_mul(3)),
        )?;
        let mut definitions = vec![None; f.values.len()];
        let mut predecessors = vec![Vec::new(); f.blocks.len()];
        for (block, row) in f.blocks.iter().enumerate() {
            for (position, instruction) in row.instructions.iter().enumerate() {
                b.spend(1)?;
                if let Some(value) = instruction.result {
                    let slot = definitions.get_mut(value.0 as usize).ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            Some(instruction.span),
                            "close proof result is outside the actual SSA arena",
                        )
                    })?;
                    if slot.replace((block, position)).is_some() {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(instruction.span),
                            "close proof SSA result repeats",
                        ));
                    }
                }
            }
            let targets = match row.terminator {
                Terminator::Jump(target) => [Some(target), None],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => [Some(then_block), Some(else_block)],
                _ => [None, None],
            };
            for target in targets.into_iter().flatten() {
                b.spend(1)?;
                predecessors
                    .get_mut(target.0 as usize)
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            Some(f.span),
                            "close proof CFG target is absent",
                        )
                    })?
                    .push(block);
            }
        }
        Ok(Self {
            f,
            definitions,
            predecessors,
            reachable: live_blocks(f, b)?,
        })
    }

    fn definition(&self, value: ValueId) -> Option<(Point, &'a Instruction)> {
        let point = *self.definitions.get(value.0 as usize)?.as_ref()?;
        Some((
            point,
            self.f.blocks.get(point.0)?.instructions.get(point.1)?,
        ))
    }
}

fn task_owner(ty: &Type) -> bool {
    matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32))
}

fn synchronous_other(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    resources::charge_payload(ty, b)?;
    if stored_callables::scalar(ty) || matches!(ty, Type::Address { .. } | Type::Desc(_)) {
        return Ok(true);
    }
    let Type::Managed(payload) = ty else {
        return Ok(false);
    };
    if !stored_owned::payload(payload, b)? {
        return Ok(false);
    }
    b.spend(module.resources.len())?;
    let mut actual = None;
    for resource in &module.resources {
        resources::charge_payload(&resource.payload, b)?;
        if resource.payload == **payload {
            if actual.replace(resource).is_some() {
                return Ok(false);
            }
        }
    }
    let Some(resource) = actual else {
        return Ok(false);
    };
    let Some(drop) = resource.teardown else {
        return Ok(true);
    };
    let Some(function) = module.functions.get(drop.0 as usize) else {
        return Ok(false);
    };
    b.spend(function.parameters.len().saturating_add(1))?;
    Ok(function.foreign.is_none()
        && function.environment.is_none()
        && function.call_kind == CallKind::Direct
        && function.return_type == Type::Unit
        && matches!(function.parameters.as_slice(), [parameter]
            if function.locals.get(parameter.0 as usize).is_some_and(|local| local.ty == Type::Desc(payload.clone()))))
}

fn inventory(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<Option<Vec<u32>>> {
    let Some(entry) = module.functions.get(d.function.0 as usize) else {
        return Ok(None);
    };
    if entry.environment != Some(d.id)
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
        || entry.return_type != d.result
    {
        return Ok(None);
    }
    b.spend(d.captures.len())?;
    let mut tasks = Vec::new();
    for (index, ty) in d.captures.iter().enumerate() {
        if task_owner(ty) {
            tasks.push(index as u32);
        } else if !synchronous_other(module, ty, b)? {
            return Ok(None);
        }
    }
    Ok(Some(tasks))
}

// The first closed-capture consumer deliberately requires stable task fields.
// Replacement, escaping mutable heads, and nested invocations need their own
// occupant summaries; none is inferred from matching closure/class numbers.
fn stable_entry(module:&TypedModule,code: &Code<'_>, d: &ClosureDefinition, b: &mut Budget) -> Check<bool> {
    for (block, row) in code.f.blocks.iter().enumerate() {
        if !code.reachable[block] {
            continue;
        }
        for instruction in &row.instructions {
            b.spend(1)?;
            if let Some(value)=environment_owner_write(&instruction.kind){
                let Some(ty)=code.f.values.get(value.0 as usize)else{return Ok(false);};
                if contains_kind(module,ty,b,false)?||contains_kind(module,ty,b,true)?{return Ok(false);}
            }
            match &instruction.kind {
                InstructionKind::TakeCapture { index }
                | InstructionKind::WriteCapture { index, .. }
                    if d.captures.get(*index as usize).is_some_and(task_owner) =>
                {
                    return Ok(false);
                }
                InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::WithForeignCallback { .. }
                | InstructionKind::WithScopedThread { .. }
                | InstructionKind::WithSyncCell { .. } => return Ok(false),
                InstructionKind::Call { .. }
                | InstructionKind::CallIndirect { .. }
                | InstructionKind::ForeignCall { .. }
                | InstructionKind::WriteAddress { .. } => return Ok(false),
                _ => {}
            }
        }
    }
    Ok(true)
}

fn capture_head(
    code: &Code<'_>,
    value: ValueId,
    b: &mut Budget,
    seen: &mut BTreeSet<ValueId>,
) -> Check<Option<u32>> {
    b.spend(seen.len().saturating_add(1))?;
    if !seen.insert(value) {
        return Ok(None);
    }
    let result = match code.definition(value).map(|(_, i)| &i.kind) {
        Some(InstructionKind::BorrowCapture {
            index,
            mutable: true,
        }) => Some(*index),
        Some(InstructionKind::ReborrowAddress { address, .. }) => {
            capture_head(code, *address, b, seen)?
        }
        Some(InstructionKind::Load(local)) => {
            let mut answer = None;
            let mut found = false;
            for (block, row) in code.f.blocks.iter().enumerate() {
                if !code.reachable[block] {
                    continue;
                }
                for instruction in &row.instructions {
                    b.spend(1)?;
                    if let InstructionKind::Store {
                        local: target,
                        value,
                    } = instruction.kind
                        && target == *local
                    {
                        let source = capture_head(code, value, b, seen)?;
                        if source.is_none() || found && source != answer {
                            seen.remove(&value);
                            return Ok(None);
                        }
                        answer = source;
                        found = true;
                    }
                }
            }
            answer
        }
        _ => None,
    };
    seen.remove(&value);
    Ok(result)
}

fn close_query(
    code: &Code<'_>,
    value: ValueId,
    b: &mut Budget,
    seen: &mut BTreeSet<ValueId>,
) -> Check<Option<u32>> {
    b.spend(seen.len().saturating_add(1))?;
    if !seen.insert(value) {
        return Ok(None);
    }
    let result = match code.definition(value).map(|(_, i)| &i.kind) {
        Some(InstructionKind::TaskPollClose { task }) => {
            if code.f.values.get(value.0 as usize)
                != Some(&super::super::task_poll_type(&Type::Unit))
            {
                None
            } else {
                capture_head(code, *task, b, &mut BTreeSet::new())?
            }
        }
        Some(InstructionKind::Load(local)) => {
            let mut answer = None;
            let mut found = false;
            for (block, row) in code.f.blocks.iter().enumerate() {
                if !code.reachable[block] {
                    continue;
                }
                for instruction in &row.instructions {
                    b.spend(1)?;
                    if let InstructionKind::Store {
                        local: target,
                        value: source,
                    } = instruction.kind
                        && target == *local
                    {
                        let source = close_query(code, source, b, seen)?;
                        if source.is_none() || found && source != answer {
                            seen.remove(&value);
                            return Ok(None);
                        }
                        answer = source;
                        found = true;
                    }
                }
            }
            answer
        }
        _ => None,
    };
    seen.remove(&value);
    Ok(result)
}

fn ready_test(
    code: &Code<'_>,
    condition: ValueId,
    b: &mut Budget,
) -> Check<Option<(ValueId, bool)>> {
    b.spend(5)?;
    let Some((_, instruction)) = code.definition(condition) else {
        return Ok(None);
    };
    let InstructionKind::Binary {
        op: BinaryOp::Equal,
        left,
        right,
    } = instruction.kind
    else {
        return Ok(None);
    };
    for (tag, number) in [(left, right), (right, left)] {
        if let Some((_, constant)) = code.definition(number)
            && let InstructionKind::Constant(Constant::I32(number @ (0 | 1))) = constant.kind
            && let Some((_, tag)) = code.definition(tag)
            && let InstructionKind::VariantTag { value } = tag.kind
        {
            return Ok(Some((value, number == 1)));
        }
    }
    Ok(None)
}

// Reachability in the graph with proved Ready edges cut. A reachable entry is
// a counterexample. Cycles supply no anchor: their incoming entry path is still
// visited, while a loop entered after a real close retains that induction base.
fn capture_closed(code: &Code<'_>, at: Point, capture: u32, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![at.0];
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if !seen.insert(block) {
            continue;
        }
        if block == code.f.entry.0 as usize {
            return Ok(false);
        }
        for &predecessor in &code.predecessors[block] {
            b.spend(1)?;
            if !code.reachable[predecessor] {
                continue;
            }
            let mut anchor = false;
            if let Terminator::Branch {
                condition,
                then_block,
                else_block,
            } = code.f.blocks[predecessor].terminator
                && then_block != else_block
                && let Some((value, yes)) = ready_test(code, condition, b)?
                && (if yes { then_block } else { else_block }).0 as usize == block
            {
                anchor = close_query(code, value, b, &mut BTreeSet::new())? == Some(capture);
            }
            if !anchor {
                pending.push(predecessor);
            }
        }
    }
    Ok(true)
}

#[derive(Default)]
struct ExitProof {
    always: BTreeSet<u32>,
    ready: BTreeSet<u32>,
}

fn exits(module: &TypedModule, id: ClosureId, b: &mut Budget) -> Check<Option<ExitProof>> {
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(None);
    };
    let Some(tasks) = inventory(module, d, b)? else {
        return Ok(None);
    };
    let code = Code::new(&module.functions[d.function.0 as usize], b)?;
    if !stable_entry(module,&code, d, b)? {
        return Ok(None);
    }
    let mut proof = ExitProof::default();
    let mut returns = Vec::new();
    for (block, row) in code.f.blocks.iter().enumerate() {
        b.spend(1)?;
        if code.reachable[block]
            && let Terminator::Return(value) = row.terminator
        {
            returns.push(((block, row.instructions.len()), value));
        }
    }
    if returns.is_empty() {
        return Ok(None);
    }
    for capture in tasks {
        let mut all_closed = true;
        let mut ready_closed = d.result == super::super::task_poll_type(&Type::Unit);
        for &(at, value) in &returns {
            if capture_closed(&code, at, capture, b)? {
                continue;
            }
            all_closed = false;
            let conditional = if let Some(value) = value {
                close_query(&code, value, b, &mut BTreeSet::new())? == Some(capture)
                    || matches!(
                        code.definition(value).map(|(_, i)| &i.kind),
                        Some(InstructionKind::MakeVariant { variant: 0, .. })
                    )
            } else {
                false
            };
            ready_closed &= conditional;
        }
        if all_closed {
            proof.always.insert(capture);
        }
        if ready_closed {
            proof.ready.insert(capture);
        }
    }
    Ok(Some(proof))
}

mod receiver;

pub(super) fn allow_at(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    at: Point,
    b: &mut Budget,
) -> Check<bool> {
    receiver::allow_at(module, f, local, at, b)
}

/// The result of a one-shot call is delivered only after remaining cleanup.
/// Every exit must therefore be Closed, independently of its returned tag.
pub(super) fn after_once(
    module: &TypedModule,
    id: ClosureId,
    b: &mut Budget,
) -> Check<bool> {
    if !b.task_origin_roots { return Ok(false); }
    let Some(d) = module.closures.get(id.0 as usize) else { return Ok(false); };
    let Some(tasks) = inventory(module, d, b)? else { return Ok(false); };
    let Some(proof) = exits(module, id, b)? else { return Ok(false); };
    b.spend(tasks.len().saturating_mul(proof.always.len().saturating_add(1)))?;
    Ok(tasks.into_iter().all(|capture| proof.always.contains(&capture)))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn model()->TypedModule{
        let(mut module,_)=crate::tests::baseline();let span=module.functions[0].span;let op=|result:Option<u32>,kind|Instruction{result:result.map(ValueId),kind,span};
        let task=Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:super::super::super::empty_context()}));let address=Type::Address{mutable:true,inner:Box::new(task)};let poll=super::super::super::task_poll_type(&Type::Unit);
        let mut entry=module.functions[0].clone();entry.id=FunctionId(1);entry.name="close_each_actual_capture".into();entry.environment=Some(ClosureId(0));entry.parameters.clear();entry.locals.clear();entry.values=vec![address.clone(),poll.clone(),Type::I32,Type::I32,Type::Bool,address,poll,Type::I32,Type::Bool,Type::I32];
        entry.blocks=vec![BasicBlock{id:BlockId(0),instructions:vec![op(Some(0),InstructionKind::BorrowCapture{index:0,mutable:true}),op(Some(1),InstructionKind::TaskPollClose{task:ValueId(0)}),op(Some(2),InstructionKind::VariantTag{value:ValueId(1)}),op(Some(3),InstructionKind::Constant(Constant::I32(1))),op(Some(4),InstructionKind::Binary{op:BinaryOp::Equal,left:ValueId(2),right:ValueId(3)})],terminator:Terminator::Branch{condition:ValueId(4),then_block:BlockId(1),else_block:BlockId(0)}},BasicBlock{id:BlockId(1),instructions:vec![op(Some(5),InstructionKind::BorrowCapture{index:1,mutable:true}),op(Some(6),InstructionKind::TaskPollClose{task:ValueId(5)}),op(Some(7),InstructionKind::VariantTag{value:ValueId(6)}),op(Some(8),InstructionKind::Binary{op:BinaryOp::Equal,left:ValueId(7),right:ValueId(3)})],terminator:Terminator::Branch{condition:ValueId(8),then_block:BlockId(2),else_block:BlockId(1)}},BasicBlock{id:BlockId(2),instructions:vec![op(Some(9),InstructionKind::Constant(Constant::I32(0)))],terminator:Terminator::Return(Some(ValueId(9)))}];entry.entry=BlockId(0);
        let caller=&mut module.functions[0];caller.environment=None;caller.parameters=vec![LocalId(0)];caller.locals=vec![LocalDecl{id:LocalId(0),name:"callback".into(),ty:Type::Closure(ClosureId(0)),mutable:true,span}];caller.values=vec![Type::I32];caller.blocks=vec![BasicBlock{id:BlockId(0),instructions:vec![op(Some(0),InstructionKind::CallClosureMut{callee:LocalId(0),arguments:vec![]})],terminator:Terminator::Return(Some(ValueId(0)))}];caller.entry=BlockId(0);
        let Type::Address{inner:task,..}=&entry.values[0]else{unreachable!()};module.closures=vec![ClosureDefinition{id:ClosureId(0),function:FunctionId(1),captures:vec![*task.clone(),*task.clone()],parameters:vec![],result:Type::I32,kind:CallableKind::FnMut,storage_life:None,lending_captures:vec![]}];module.functions.push(entry);module
    }
    fn wire(module:&TypedModule)->TypedModule{let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(module,&CandidatePlan::empty_for(module),&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0}
    fn closed(module:&TypedModule)->(bool,bool){let mut b=Budget::new(&Limits::default());b.task_origin_roots=true;let at=module.functions[0].blocks[0].instructions.iter().position(|instruction|matches!(instruction.kind,InstructionKind::CallClosureMut{..})).unwrap()+1;(after_once(module,ClosureId(0),&mut b).unwrap(),allow_at(module,&module.functions[0],LocalId(0),(0,at),&mut b).unwrap())}
    #[test]
    fn both_same_type_captures_need_their_own_ready_edges_and_unchanged_occupants_raw_wire(){
        let module=model();for module in [&module,&wire(&module)]{assert_eq!(closed(module),(true,true));}
        for attack in 0..3{
            let mut changed=module.clone();let entry=&mut changed.functions[1];let span=entry.span;
            match attack{
                0=>entry.blocks[1].instructions[1].kind=InstructionKind::TaskPollClose{task:ValueId(0)},
                1=>{entry.values.push(Type::Unit);entry.blocks[1].instructions.insert(1,Instruction{result:Some(ValueId(10)),kind:InstructionKind::Constant(Constant::Unit),span});entry.blocks[1].instructions[2].kind=InstructionKind::MakeVariant{variant:1,fields:vec![ValueId(10)]};},
                _=>{let task=changed.closures[0].captures[0].clone();changed.closures[0].parameters=vec![task.clone()];entry.parameters=vec![LocalId(0)];entry.locals=vec![LocalDecl{id:LocalId(0),name:"replacement".into(),ty:task.clone(),mutable:true,span}];entry.values.push(task.clone());entry.blocks[2].instructions.extend([Instruction{result:Some(ValueId(10)),kind:InstructionKind::TakeLocal{local:LocalId(0)},span},Instruction{result:None,kind:InstructionKind::ReservePlaceAddress{reservation:ReservationId(0),address:ValueId(0),path:vec![]},span},Instruction{result:None,kind:InstructionKind::CommitPlaceReplace{reservation:ReservationId(0),value:ValueId(10)},span}]);let caller=&mut changed.functions[0];caller.parameters.push(LocalId(1));caller.locals.push(LocalDecl{id:LocalId(1),name:"replacement_input".into(),ty:task.clone(),mutable:true,span});caller.values.push(task);caller.blocks[0].instructions[0].kind=InstructionKind::CallClosureMut{callee:LocalId(0),arguments:vec![ValueId(1)]};caller.blocks[0].instructions.insert(0,Instruction{result:Some(ValueId(1)),kind:InstructionKind::TakeLocal{local:LocalId(1)},span});},
            }
            for changed in [&changed,&wire(&changed)]{assert_eq!(closed(changed),(false,false),"same-type/Ready/replacement control {attack}");}
        }
    }
}
