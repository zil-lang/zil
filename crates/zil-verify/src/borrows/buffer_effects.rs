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
        Root::ParameterPort(..) => true,
        Root::ParameterCapture(parameter, capture) => {
            let ty = &function.locals[function.parameters[parameter as usize].0 as usize].ty;
            helpers::parameter_type(ty).is_some_and(|(id, _)| {
                borrowed(&module.closures[id.0 as usize].captures[capture as usize])
            })
        }
        Root::Window(origin, ..) | Root::StoreElementWindow(origin, ..) => external(
            module,
            function,
            match origin {
                HoldRoot::Static => Root::Static,
                HoldRoot::Local(l) => Root::Local(l),
                HoldRoot::Parameter(p) => Root::Parameter(p),
                HoldRoot::Capture(c) => Root::Capture(c),
            },
        ),
        Root::Static | Root::Local(_) | Root::Field(..) | Root::Projection(..) => false,
    }
}

pub(super) fn access(kind: &InstructionKind, descriptor_origins: bool) -> Option<(ValueId, bool)> {
    match kind {
        InstructionKind::AsyncRegistrationValid { registration } => Some((*registration, false)),
        InstructionKind::AsyncRegistrationRequestClose { registration }
        | InstructionKind::AsyncRegistrationPollClose { registration } => {
            Some((*registration, true))
        }
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
            // Only actual intervals within one known address establish
            // separation here. Different symbolic environment captures still
            // require caller substitution, even when they share a coarse root.
            let disjoint_windows = matches!((*a, *b),
                (Root::Window(origin, start, end), Root::Window(other, lo, hi))
                    if origin == other && (end <= lo || hi <= start));
            let own_task_shell = matches!((*a,*b),
                (Root::Parameter(p),Root::ParameterPort(q,_)) | (Root::ParameterPort(q,_),Root::Parameter(p)) if p==q);
            if a != b
                && !disjoint_windows
                && !own_task_shell
                && external(module, function, *a)
                && external(module, function, *b)
            {
                budget.spend(apart.len().saturating_add(1))?;
                apart.insert(((*a).min(*b), (*a).max(*b)));
            }
        }
    }
    Ok(())
}

/// Instantiate a callee's required separation. Equal possible roots cannot
/// satisfy it; unequal input roots become the caller's own explicit obligation.
pub(super) fn instantiate(
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

pub(super) struct ApartContext<'a> {
    pub(super) module: &'a TypedModule,
    pub(super) graphs: &'a [FunctionGraph],
    pub(super) facts: &'a [Facts],
    pub(super) capture_facts: &'a [captures::CaptureFacts],
    pub(super) effects: &'a [captures::Effects],
    pub(super) mask: Option<&'a [bool]>,
}

pub(super) fn close_apart(
    context: ApartContext<'_>,
    mut apart: Vec<Apart>,
    budget: &mut Budget,
) -> Check<()> {
    let ApartContext {
        module,
        graphs,
        facts,
        capture_facts,
        effects,
        mask,
    } = context;
    loop {
        let mut changed = false;
        for (index, function) in module.functions.iter().enumerate() {
            if mask.is_some_and(|mask| mask[index]) {
                continue;
            }
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
