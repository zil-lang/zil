//! Optional, per-produced-function canonicalization before LLVM sees the model.
//! It never grants verification authority, changes a compile quota or adds poison
//! overflow flags. All speculative work is transactional and has its own fuel.
mod cleanup;
mod ranges;
mod ssa;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use zil_ir::TypedFunction;
use zil_llvm_api::model as m;

const DEFAULT_FUEL: usize = 1_000_000;
const MAX_BLOCKS: usize = 2048;
const MAX_INSTRUCTIONS: usize = 65_536;
const MAX_SLOTS: usize = 512;

struct Fuel(usize);
impl Fuel {
    fn take(&mut self, amount: usize) -> Option<()> {
        self.0 = self.0.checked_sub(amount)?;
        Some(())
    }
}
#[derive(Default, Debug, PartialEq, Eq)]
struct Work {
    slots: usize,
    checks: usize,
    cleanup: usize,
}

pub(super) fn optimize(source: &TypedFunction, machine: &mut m::Function) {
    let _ = run(source, machine, DEFAULT_FUEL);
}

/// Only the private current producer completion marker reaches this entry.
/// Memory SSA was constructed from the same source plan; current machine range
/// simplification and the complete compact cleanup remain unchanged. Native
/// independent model preflight and LLVM before/after verification still follow.
pub(super) fn optimize_produced_ssa(source: &TypedFunction, machine: &mut m::Function) {
    if machine.name != super::symbol(source.id) || source.environment.is_some() {
        return;
    }
    let Some(blocks) = machine.blocks.as_ref() else {
        return;
    };
    if blocks.len() > MAX_BLOCKS
        || source.locals.len() > MAX_SLOTS
        || blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.instructions.len()))
            .is_none_or(|n| n > MAX_INSTRUCTIONS)
    {
        return;
    }
    let mut fuel = Fuel(DEFAULT_FUEL);
    let Some(cfg) = ssa::Cfg::new(machine, &mut fuel) else {
        return;
    };
    let mut candidate = machine.clone();
    let Some(_) = ranges::simplify(&mut candidate, &cfg, &mut fuel) else {
        return;
    };
    let Some(_) = cleanup::simplify(&mut candidate, &mut fuel) else {
        return;
    };
    *machine = candidate;
}

fn run(source: &TypedFunction, machine: &mut m::Function, fuel: usize) -> Option<Work> {
    if machine.name != super::symbol(source.id) || source.environment.is_some() {
        return None;
    }
    let mut fuel = Fuel(fuel);
    let blocks = machine.blocks.as_ref()?;
    if blocks.len() > MAX_BLOCKS
        || blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.instructions.len()))?
            > MAX_INSTRUCTIONS
        || source.locals.len() > MAX_SLOTS
    {
        return None;
    }
    let slots = ssa::source_slots(source, &mut fuel)?;
    let mut cleanup_eligible = true;
    // Bound nested operand walks before any recursive visitor or model clone.
    for block in blocks {
        for inst in &block.instructions {
            if matches!(inst.op, m::Op::Call { .. }) {
                let trap = matches!(&inst.op,m::Op::Call {result:m::Ty::Void,result_attrs,callee:m::Val::Global(name),args,vararg:None}
                    if name=="llvm.trap" && result_attrs.is_empty() && args.is_empty());
                cleanup_eligible &= trap || ranges::canonical_checked_call(&inst.op);
            }
            for root in inst.op.values() {
                let mut pending = vec![(root, 0usize)];
                while let Some((v, depth)) = pending.pop() {
                    fuel.take(1)?;
                    if depth > 128 {
                        return None;
                    }
                    match v {
                        m::Val::Array(args) => {
                            fuel.take(args.len())?;
                            pending.extend(args.iter().map(|a| (&a.value, depth + 1)));
                        }
                        m::Val::Gep { ptr, indices, .. } => {
                            fuel.take(indices.len())?;
                            pending.push((ptr, depth + 1));
                            pending.extend(indices.iter().map(|a| (&a.value, depth + 1)));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    let cfg = ssa::Cfg::new(machine, &mut fuel)?;
    // The Vec model is cloned before mutation. Rejection drops the candidate
    // without touching the original produced function.
    let mut candidate = machine.clone();
    let slots = ssa::promote(&mut candidate, &cfg, slots, &mut fuel)?;
    let checks = ranges::simplify(&mut candidate, &cfg, &mut fuel)?;
    // Retain the established SSA/range form across ordinary call boundaries.
    // The extra CFG reshaping targets complete arithmetic leaf bodies only.
    let cleanup = if cleanup_eligible {
        cleanup::simplify(&mut candidate, &mut fuel)?
    } else {
        0
    };
    let work = Work {
        slots,
        checks,
        cleanup,
    };
    if work.slots != 0 || work.checks != 0 || work.cleanup != 0 {
        *machine = candidate;
    }
    Some(work)
}

fn canonical_slot(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("local.")?;
    if digits.is_empty()
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    digits.parse().ok()
}
fn visit_local(value: &m::Val, f: &mut impl FnMut(&str)) {
    match value {
        m::Val::Local(name) => f(name),
        m::Val::Array(args) => {
            for a in args {
                visit_local(&a.value, f);
            }
        }
        m::Val::Gep { ptr, indices, .. } => {
            visit_local(ptr, f);
            for a in indices {
                visit_local(&a.value, f);
            }
        }
        _ => {}
    }
}
pub(super) fn map_operands(op: &mut m::Op, f: &mut impl FnMut(&mut m::Val)) {
    use m::Op as O;
    match op {
        O::Alloca { .. } | O::Branch(_) | O::Unreachable | O::Return(None) => {}
        O::Load { ptr, .. } | O::AtomicLoad { ptr, .. } => f(ptr),
        O::Store { arg, ptr, .. }
        | O::AtomicStore { arg, ptr, .. }
        | O::AtomicRmw { arg, ptr, .. } => {
            f(&mut arg.value);
            f(ptr);
        }
        O::AtomicCompareExchange {
            expected,
            desired,
            ptr,
            ..
        } => {
            f(&mut expected.value);
            f(desired);
            f(ptr);
        }
        O::Binary { left, right, .. } | O::Compare { left, right, .. } => {
            f(left);
            f(right);
        }
        O::Select {
            condition, yes, no, ..
        } => {
            f(condition);
            f(yes);
            f(no);
        }
        O::Cast { arg, .. } | O::Extract { arg, .. } | O::Freeze(arg) | O::Return(Some(arg)) => {
            f(&mut arg.value)
        }
        O::Insert { arg, field, .. } => {
            f(&mut arg.value);
            f(&mut field.value);
        }
        O::Gep { ptr, indices, .. } => {
            f(ptr);
            for a in indices {
                f(&mut a.value);
            }
        }
        O::Call { callee, args, .. } => {
            f(callee);
            for a in args {
                f(&mut a.value);
            }
        }
        O::Phi { incoming, .. } => {
            for (v, _) in incoming {
                f(v);
            }
        }
        O::CondBranch { condition, .. } => f(condition),
    }
}
fn resolve_value(
    value: &m::Val,
    aliases: &BTreeMap<String, m::Val>,
    cache: &mut BTreeMap<String, m::Val>,
    active: &mut BTreeSet<String>,
    depth: usize,
    fuel: &mut Fuel,
) -> Option<m::Val> {
    fuel.take(1)?;
    if depth > 256 {
        return None;
    }
    if let m::Val::Local(name) = value {
        if let Some(value) = cache.get(name) {
            return Some(value.clone());
        }
        if let Some(next) = aliases.get(name) {
            if !active.insert(name.clone()) {
                return None;
            }
            let result = resolve_value(next, aliases, cache, active, depth + 1, fuel)?;
            active.remove(name);
            cache.insert(name.clone(), result.clone());
            return Some(result);
        }
    }
    let mut result = value.clone();
    match &mut result {
        m::Val::Array(args) => {
            for a in args {
                a.value = resolve_value(&a.value, aliases, cache, active, depth + 1, fuel)?;
            }
        }
        m::Val::Gep { ptr, indices, .. } => {
            **ptr = resolve_value(ptr, aliases, cache, active, depth + 1, fuel)?;
            for a in indices {
                a.value = resolve_value(&a.value, aliases, cache, active, depth + 1, fuel)?;
            }
        }
        _ => {}
    }
    Some(result)
}
fn rewrite_aliases(
    machine: &mut m::Function,
    aliases: &BTreeMap<String, m::Val>,
    fuel: &mut Fuel,
) -> Option<()> {
    if aliases.is_empty() {
        return Some(());
    }
    let mut cache = BTreeMap::new();
    let mut active = BTreeSet::new();
    for value in aliases.values() {
        let _ = resolve_value(value, aliases, &mut cache, &mut active, 0, fuel)?;
    }
    for block in machine.blocks.as_mut()?.iter_mut() {
        for instruction in &mut block.instructions {
            let mut ok = true;
            map_operands(&mut instruction.op, &mut |v| {
                if let Some(next) = resolve_value(v, aliases, &mut cache, &mut active, 0, fuel) {
                    *v = next;
                } else {
                    ok = false;
                }
            });
            if !ok {
                return None;
            }
        }
    }
    Some(())
}
