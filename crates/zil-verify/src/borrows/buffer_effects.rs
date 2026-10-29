//! Byte-storage effects and necessary parameter separation, reconstructed only
//! from checked CFGs. Finite prefixes are included even without a return edge.
use super::*;

pub(super) type Apart = BTreeSet<(Root, Root)>;

fn external(module: &TypedModule, function: &TypedFunction, root: Root) -> bool {
    let borrowed = |ty: &Type| matches!(ty, Type::Desc(_) | Type::Address { .. });
    match root {
        Root::Parameter(parameter) => {
            let ty = &function.locals[function.parameters[parameter as usize].0 as usize].ty;
            // Current environment positions already participate in ordinary
            // argument loans. They are distinct from their external targets.
            borrowed(ty) && helpers::parameter_type(ty).is_none()
        }
        Root::Capture(capture) => function
            .environment
            .is_some_and(|id| borrowed(&module.closures[id.0 as usize].captures[capture as usize])),
        Root::ParameterCapture(parameter, capture) => {
            let ty = &function.locals[function.parameters[parameter as usize].0 as usize].ty;
            helpers::parameter_type(ty).is_some_and(|(id, _)| {
                borrowed(&module.closures[id.0 as usize].captures[capture as usize])
            })
        }
        Root::Local(_) | Root::Field(..) => false,
    }
}

pub(super) fn access(kind: &InstructionKind, descriptor_origins: bool) -> Option<(ValueId, bool)> {
    match kind {
        InstructionKind::ReadDesc { desc, .. } if descriptor_origins => Some((*desc, false)),
        InstructionKind::WriteDesc { desc, .. } if descriptor_origins => Some((*desc, true)),
        _ => byte_buffers::access(kind).or_else(|| dynamic_store::access(kind)),
    }
}

/// Different input names carry no separation evidence. Keep the obligation
/// symbolic until a caller can substitute actual owned storage roots.
pub(super) fn unknown_pairs(
    module: &TypedModule,
    function: &TypedFunction,
    left: &[Root],
    right: &[Root],
    apart: &mut Apart,
    budget: &mut Budget,
) -> Check<()> {
    for a in left {
        for b in right {
            budget.spend(1)?;
            if a != b && external(module, function, *a) && external(module, function, *b) {
                budget.spend(apart.len().saturating_add(1))?;
                apart.insert(((*a).min(*b), (*a).max(*b)));
            }
        }
    }
    Ok(())
}

/// Instantiate a callee's required separation. Equal possible roots cannot
/// satisfy it; unequal input roots become the caller's own explicit obligation.
fn instantiate(
    module: &TypedModule,
    function: &TypedFunction,
    left: &[Root],
    right: &[Root],
    apart: &mut Apart,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    if left.is_empty() || right.is_empty() {
        return Err(reject(
            "E-BORROW-ORIGIN",
            Some(span),
            "byte-storage separation has an ungrounded actual argument",
        ));
    }
    for a in left {
        for b in right {
            budget.spend(1)?;
            if a.intersects(*b) {
                return Err(reject(
                    "E-BORROW-CONFLICT",
                    Some(span),
                    "call arguments alias where its independently reconstructed window contract requires separation",
                ));
            }
        }
    }
    unknown_pairs(module, function, left, right, apart, budget)
}

pub(super) fn close_apart(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    facts: &[Facts],
    capture_facts: &[captures::CaptureFacts],
    effects: &[captures::Effects],
    mut apart: Vec<Apart>,
    budget: &mut Budget,
) -> Check<()> {
    loop {
        let mut changed = false;
        for (index, function) in module.functions.iter().enumerate() {
            budget.spend(1)?;
            let context = captures::Context {
                module,
                function,
                ordinary: &facts[index],
                facts: &capture_facts[index],
                effects,
            };
            for (b, (block, reachable)) in function
                .blocks
                .iter()
                .zip(&graphs[index].reachable)
                .enumerate()
            {
                budget.spend(1)?;
                if !reachable {
                    continue;
                }
                let mut locals = captures::input_state(&capture_facts[index], b, budget)?;
                for instruction in &block.instructions {
                    budget.spend(1)?;
                    if let Some(call) = context.invocation(instruction, &locals, budget)? {
                        budget.spend(apart[call.function.0 as usize].len())?;
                        let imported = apart[call.function.0 as usize].clone();
                        let before = apart[index].len();
                        for (a, b) in imported {
                            let left = call.substitute(a, budget)?;
                            let right = call.substitute(b, budget)?;
                            instantiate(
                                module,
                                function,
                                &left,
                                &right,
                                &mut apart[index],
                                instruction.span,
                                budget,
                            )?;
                        }
                        changed |= apart[index].len() != before;
                    }
                    captures::local_transfer(
                        &mut locals,
                        instruction,
                        &capture_facts[index].values,
                        budget,
                    )?;
                }
            }
        }
        if !changed {
            return Ok(());
        }
    }
}
