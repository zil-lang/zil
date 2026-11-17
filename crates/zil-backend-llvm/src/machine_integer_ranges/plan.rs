//! Index the immutable current body once, before evaluating its changing inputs.
//! Plans contain no World facts: current returns, arguments and intrinsic
//! declarations are still read on every evaluation. They never cross requests.
use super::*;
use zil_llvm_api::model::{Block, SharedVec};

#[derive(Default)]
pub(super) struct Plans<'a> {
    bodies: HashMap<usize, Body<'a>>,
    #[cfg(test)]
    pub(super) reference: bool,
}

struct Body<'a> {
    blocks: &'a SharedVec<Block>,
    instructions: usize,
    pairs: HashMap<&'a str, &'a Op>,
    transfers: Vec<(&'a str, &'a Op)>,
    observations: Vec<(usize, usize, &'a Op)>,
}

#[cfg(test)]
std::thread_local! {
    pub(super) static BODY_PLANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl<'a> Plans<'a> {
    pub(super) fn facts(
        &mut self,
        f: &'a Function,
        world: &World<'_>,
        work: &mut usize,
        inputs: &[Range],
    ) -> Option<Facts> {
        #[cfg(test)]
        if self.reference {
            return super::facts(f, world, work, inputs);
        }
        let blocks = f.blocks.as_ref()?;
        // The borrowed module keeps the Arc allocation alive through planning;
        // use its Vec owner, not an empty slice's potentially shared pointer.
        let key = std::ptr::from_ref(&**blocks) as usize;
        let body = match self.bodies.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(Body::new(blocks)?),
        };
        if !body.blocks.ptr_eq(blocks) {
            return None;
        }
        body.facts(f, world, work, inputs)
    }
}

impl<'a> Body<'a> {
    fn new(blocks: &'a SharedVec<Block>) -> Option<Self> {
        #[cfg(test)]
        BODY_PLANS.with(|n| n.set(n.get() + 1));
        let mut body = Self {
            blocks,
            instructions: 0,
            pairs: HashMap::new(),
            transfers: Vec::new(),
            observations: Vec::new(),
        };
        for (bi, block) in blocks.iter().enumerate() {
            body.instructions = body.instructions.checked_add(block.instructions.len())?;
            for (ii, inst) in block.instructions.iter().enumerate() {
                if let Some(name) = inst.result.as_deref() {
                    body.pairs.insert(name, &inst.op);
                    // Every other operation in operation() is unconditionally
                    // Top. Top cannot change an intersection, including freeze.
                    // Keep original instruction order and all Phi branches.
                    if matches!(
                        inst.op,
                        Op::Binary {
                            ty: Ty::Int(32),
                            ..
                        } | Op::Phi {
                            ty: Ty::Int(32),
                            ..
                        } | Op::Select {
                            ty: Ty::Int(32),
                            ..
                        } | Op::Extract { index: 0, .. }
                            | Op::Call {
                                result: Ty::Int(32),
                                callee: Val::Global(_),
                                ..
                            }
                    ) {
                        body.transfers.push((name, &inst.op));
                    }
                }
                if matches!(
                    inst.op,
                    Op::Return(Some(_))
                        | Op::Call {
                            callee: Val::Global(_),
                            ..
                        }
                        | Op::Extract { index: 1, .. }
                ) {
                    body.observations.push((bi, ii, &inst.op));
                }
            }
        }
        Some(body)
    }

    fn facts(
        &self,
        f: &Function,
        world: &World<'_>,
        work: &mut usize,
        inputs: &[Range],
    ) -> Option<Facts> {
        #[cfg(test)]
        FACT_EXECUTIONS.with(|n| n.set(n.get() + 1));
        // Original map-construction fee and full instruction count per round,
        // even for omitted Top operations. Failed evaluations are never cached
        // or applied, so charging a complete scan up front is transactional.
        *work = work.checked_sub(self.instructions.checked_add(f.signature.params.len())?)?;
        let mut values = Values::new();
        for (parameter, range) in f.signature.params.iter().zip(inputs) {
            if parameter.ty == Ty::Int(32) {
                values.insert(parameter.name.as_deref()?, *range);
            }
        }
        for _ in 0..MAX_ROUNDS {
            *work = work.checked_sub(self.instructions)?;
            let mut changed = false;
            for &(name, op) in &self.transfers {
                let old = values.get(name).copied().unwrap_or(Range::TOP);
                let new = old.intersect(operation(op, &self.pairs, world, &values))?;
                if new != old {
                    values.insert(name, new);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        *work = work.checked_sub(self.instructions)?;
        let mut returned = None;
        let mut flags = Vec::new();
        let mut calls = Vec::new();
        for &(bi, ii, op) in &self.observations {
            if let Op::Return(Some(arg)) = op {
                let r = if arg.ty == Ty::Int(32) {
                    value(&arg.value, &values)
                } else {
                    Range::TOP
                };
                returned = Some(returned.map_or(r, |old: Range| old.hull(r)));
            }
            if let Op::Call {
                callee: Val::Global(name),
                args,
                vararg,
                ..
            } = op
                && let Some(parameters) = world.arguments.get(name.as_str())
            {
                let ranges = if vararg.is_none()
                    && args.len() == parameters.len()
                    && args.iter().all(|a| a.ty == Ty::Int(32))
                {
                    args.iter().map(|a| value(&a.value, &values)).collect()
                } else {
                    vec![Range::TOP; parameters.len()]
                };
                calls.push((name.clone(), ranges));
            }
            if let Op::Extract { arg, index: 1 } = op
                && arg.ty == Ty::Struct(vec![Ty::Int(32), Ty::Int(1)])
                && let Val::Local(name) = &arg.value
                && let Some(op) = self.pairs.get(name.as_str())
                && overflow(op, world, &values).is_some_and(Range::fits)
            {
                flags.push((bi, ii));
            }
        }
        Some(Facts {
            returned: returned.unwrap_or(Range::TOP),
            flags,
            calls,
        })
    }
}
