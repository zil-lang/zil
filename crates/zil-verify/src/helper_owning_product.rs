//! Prove that the incoming callback is transferred as data into an actual
//! returned product or passed to a fully typed by-value aggregate parameter.
use super::*;

pub(super) fn transfers_parameter(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: LocalId,
    b: &mut Budget,
) -> Check<bool> {
    let locals = f.locals.len();
    let count = locals.saturating_add(f.values.len());
    b.spend(count.saturating_mul(2).saturating_add(f.blocks.len()))?;
    let mut edges = vec![Vec::new(); count];
    let mut live = vec![false; f.blocks.len()];
    let mut pending = vec![f.entry];
    while let Some(id) = pending.pop() {
        b.spend(1)?;
        let row = live
            .get_mut(id.0 as usize)
            .ok_or_else(|| bad(f, "owning constructor has an absent CFG target"))?;
        if std::mem::replace(row, true) {
            continue;
        }
        let block = &f.blocks[id.0 as usize];
        match block.terminator {
            Terminator::Jump(next) => pending.push(next),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => pending.extend([then_block, else_block]),
            _ => {}
        }
        for op in &block.instructions {
            b.spend(1)?;
            let mut edge = |from: usize, to: usize| -> Check<()> {
                b.spend(1)?;
                if from >= count || to >= count {
                    return Err(bad(f, "owning constructor operand is absent"));
                }
                edges[from].push(to);
                Ok(())
            };
            match &op.kind {
                InstructionKind::Store { local, value } => {
                    edge(locals + value.0 as usize, local.0 as usize)?
                }
                InstructionKind::TakeLocal { local } => {
                    if let Some(output) = op.result {
                        edge(local.0 as usize, locals + output.0 as usize)?;
                    }
                }
                kind if aggregate(kind) => {
                    if let Some(output) = op.result {
                        for field in kind.value_operands() {
                            edge(locals + field.0 as usize, locals + output.0 as usize)?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut derived = vec![false; count];
    let mut work = vec![parameter.0 as usize];
    while let Some(node) = work.pop() {
        b.spend(1)?;
        if std::mem::replace(&mut derived[node], true) {
            continue;
        }
        b.spend(edges[node].len())?;
        work.extend_from_slice(&edges[node]);
    }
    let mut transferred = false;
    for (index, block) in f.blocks.iter().enumerate() {
        if !live[index] {
            continue;
        }
        match block.terminator {
            Terminator::Return(Some(value)) if marked_value(f, &derived, value)? => {
                if !product_type(&f.values[value.0 as usize]) {
                    return Ok(false);
                }
                transferred = true;
            }
            Terminator::Branch { condition, .. } if marked_value(f, &derived, condition)? => {
                return Ok(false);
            }
            _ => {}
        }
        for op in &block.instructions {
            b.spend(1)?;
            let kind = &op.kind;
            let mut marked_inputs = 0usize;
            for value in kind.value_operands() {
                b.spend(1)?;
                marked_inputs += usize::from(marked_value(f, &derived, value)?);
            }
            if marked_inputs > 0 {
                if let InstructionKind::Call {
                    function,
                    arguments,
                } = kind
                {
                    let Some(target) = module.functions.get(function.0 as usize) else {
                        return Err(bad(f, "owning aggregate Call has no callee"));
                    };
                    if target.parameters.len() != arguments.len() {
                        return Ok(false);
                    }
                    for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                        b.spend(1)?;
                        if !marked_value(f, &derived, *argument)? {
                            continue;
                        }
                        let ty = &f.values[argument.0 as usize];
                        resources::charge_payload(ty, b)?;
                        if !product_type(ty)
                            || target
                                .locals
                                .get(parameter.0 as usize)
                                .is_none_or(|l| &l.ty != ty)
                        {
                            return Ok(false);
                        }
                    }
                    transferred = true;
                } else if marked_inputs != 1
                    || !(aggregate(kind) || matches!(kind, InstructionKind::Store { .. }))
                {
                    return Ok(false);
                }
            }
            for local in kind.local_operands() {
                b.spend(1)?;
                if local.0 as usize >= locals {
                    return Err(bad(f, "owning aggregate local is absent"));
                }
                if !derived[local.0 as usize] {
                    continue;
                }
                match kind {
                    // Every possible writer of a carrier must supply the
                    // parameter. An unmarked branch cannot donate another F.
                    InstructionKind::Store { .. } if marked_inputs == 1 && local != parameter => {}
                    InstructionKind::TakeLocal { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::DropClosure { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::DropEnum { .. }
                    | InstructionKind::DropTuple { .. }
                    | InstructionKind::DropOwnedArray { .. }
                    | InstructionKind::DropErrorUnion { .. } => {}
                    _ => return Ok(false),
                }
            }
        }
    }
    Ok(transferred)
}

fn aggregate(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::MakePackage { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::MakeOwnedTuple { .. }
            | InstructionKind::MakeOwnedArray { .. }
    )
}

fn product_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Package { .. }
            | Type::Tuple(_)
            | Type::Enum { .. }
            | Type::ErrorUnion { .. }
            | Type::Array { .. }
    )
}
fn marked_value(f: &TypedFunction, derived: &[bool], value: ValueId) -> Check<bool> {
    derived
        .get(f.locals.len().saturating_add(value.0 as usize))
        .copied()
        .ok_or_else(|| bad(f, "owning aggregate value is absent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constructor_does_not_cover_an_independent_invocation_raw_wire() {
        let (mut module, plan) = crate::tests::baseline();
        let f = &mut module.functions[0];
        let callback = Type::Closure(ClosureId(0));
        let product = Type::Tuple(vec![callback.clone()]);
        f.locals[0].ty = callback.clone();
        f.parameters = vec![LocalId(0)];
        f.values = vec![callback, product.clone()];
        f.return_type = product;
        let span = f.span;
        let take = Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::TakeLocal { local: LocalId(0) },
            span,
        };
        let pack = Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::MakeOwnedTuple {
                fields: vec![ValueId(0)],
            },
            span,
        };
        f.blocks[0].instructions = vec![take, pack];
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        let limits = Limits::default();
        for mixed in [false, true] {
            if mixed {
                module.functions[0].blocks[0].instructions.insert(
                    0,
                    Instruction {
                        result: None,
                        kind: InstructionKind::CallClosureShared {
                            callee: LocalId(0),
                            arguments: vec![],
                        },
                        span,
                    },
                );
            }
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            for actual in [&module, &wire] {
                let mut budget = Budget::new(&limits);
                assert_eq!(
                    transfers_parameter(actual, &actual.functions[0], LocalId(0), &mut budget)
                        .unwrap(),
                    !mixed
                );
            }
        }
    }
}
