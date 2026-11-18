//! Track the incoming callback through real owning constructions and field
//! publications. A previously checked callee may take the naked callback
//! only at its exact independently verified data parameter.
use super::*;

pub(super) fn transfers_parameter(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: LocalId,
    data_parameters: &[Option<u32>],
    b: &mut Budget,
) -> Check<bool> {
    let locals = f.locals.len();
    b.spend(f.blocks.len().saturating_add(locals).saturating_add(f.values.len()))?;
    let mut initial = vec![false; locals];
    initial[parameter.0 as usize] = true;
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(initial);
    let mut values = vec![false; f.values.len()];
    let mut transferred = false;
    loop {
        let mut changed = false;
        for (index, block) in f.blocks.iter().enumerate() {
            b.spend(1)?;
            let Some(entry) = &incoming[index] else { continue; };
            b.spend(entry.len())?;
            let mut exact = entry.clone();
            for op in &block.instructions {
                b.spend(1)?;
                let kind = &op.kind;
                let mut marked = 0;
                for value in kind.value_operands() {
                    b.spend(1)?;
                    marked += usize::from(contains_value(f, &values, value)?);
                }
                let mut output = false;
                match kind {
                    InstructionKind::Store { local, .. } => {
                        exact[local.0 as usize] = marked != 0;
                    }
                    InstructionKind::TakeLocal { local } => {
                        output = std::mem::take(&mut exact[local.0 as usize]);
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropEnum { local }
                    | InstructionKind::DropTuple { local }
                    | InstructionKind::DropOwnedArray { local }
                    | InstructionKind::DropErrorUnion { local } => {
                        exact[local.0 as usize] = false;
                    }
                    InstructionKind::StorePackageField { local, index, value } if marked != 0 => {
                        let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty else {
                            return Ok(false);
                        };
                        let Some((_, field)) = fields.get(*index as usize) else {
                            return Ok(false);
                        };
                        if exact[local.0 as usize] || field != &f.values[value.0 as usize] {
                            return Ok(false);
                        }
                        exact[local.0 as usize] = true;
                    }
                    kind if aggregate(kind) && marked != 0 => {
                        if marked != 1 { return Ok(false); }
                        output = true;
                    }
                    InstructionKind::Call { function, arguments } if marked != 0 => {
                        let target = module.functions.get(function.0 as usize)
                            .ok_or_else(|| bad(f, "owning data call has no actual callee"))?;
                        if target.parameters.len() != arguments.len() { return Ok(false); }
                        for (ordinal, (argument, parameter)) in arguments.iter().zip(&target.parameters).enumerate() {
                            b.spend(1)?;
                            if !contains_value(f, &values, *argument)? { continue; }
                            let ty = &f.values[argument.0 as usize];
                            resources::charge_payload(ty, b)?;
                            let direct = matches!(ty, Type::Closure(_))
                                && data_parameters.get(function.0 as usize).copied().flatten()
                                    .is_some_and(|selected| selected as usize == ordinal);
                            if !(product_type(ty) || direct)
                                || target.locals.get(parameter.0 as usize).is_none_or(|row| &row.ty != ty)
                            {
                                return Ok(false);
                            }
                        }
                        transferred = true;
                        output = product_type(&target.return_type)
                            || matches!(target.return_type, Type::Closure(_));
                    }
                    _ => {
                        if marked != 0 { return Ok(false); }
                        for local in kind.local_operands() {
                            b.spend(1)?;
                            if exact[local.0 as usize] { return Ok(false); }
                        }
                    }
                }
                if output {
                    let result = op.result.ok_or_else(|| bad(f, "owning data transfer has no actual output"))?;
                    let slot = values.get_mut(result.0 as usize)
                        .ok_or_else(|| bad(f, "owning data output is absent"))?;
                    changed |= !std::mem::replace(slot, true);
                }
            }
            let successors = match block.terminator {
                Terminator::Return(Some(value)) if contains_value(f, &values, value)? => {
                    if !product_type(&f.values[value.0 as usize]) { return Ok(false); }
                    transferred = true;
                    [None, None]
                }
                Terminator::Jump(next) => [Some(next), None],
                Terminator::Branch { condition, then_block, else_block } => {
                    if contains_value(f, &values, condition)? { return Ok(false); }
                    [Some(then_block), Some(else_block)]
                }
                _ => [None, None],
            };
            for successor in successors.into_iter().flatten() {
                let slot = incoming.get_mut(successor.0 as usize)
                    .ok_or_else(|| bad(f, "owning data transfer has an absent CFG successor"))?;
                b.spend(exact.len().saturating_add(1))?;
                if let Some(previous) = slot {
                    for (target, source) in previous.iter_mut().zip(&exact) {
                        changed |= *source && !*target;
                        *target |= *source;
                    }
                } else {
                    *slot = Some(exact.clone());
                    changed = true;
                }
            }
        }
        if !changed { return Ok(transferred); }
    }
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

fn contains_value(f: &TypedFunction, values: &[bool], value: ValueId) -> Check<bool> {
    values.get(value.0 as usize).copied()
        .ok_or_else(|| bad(f, "owning data value is absent"))
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
                    transfers_parameter(actual, &actual.functions[0], LocalId(0), &[], &mut budget)
                        .unwrap(),
                    !mixed
                );
            }
        }
    }
}
