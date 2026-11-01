//! Narrow facts from the current, exactly bound verified source CFG.
use std::collections::BTreeSet;
use zil_ir::{Constant, InstructionKind, Type, TypedFunction, ValueId};

// Optimization-only bounds. A miss emits all original arithmetic checks.
const MAX_CANDIDATES: usize = 16_384;
const MAX_SCAN: usize = 1_000_000;

pub(super) fn record(candidates: &mut Option<BTreeSet<ValueId>>, value: ValueId) {
    let Some(set) = candidates else { return };
    if set.len() >= MAX_CANDIDATES && !set.contains(&value) {
        *candidates = None;
    } else {
        set.insert(value);
    }
}

pub(super) fn retain_safe(function: &TypedFunction, candidates: &mut Option<BTreeSet<ValueId>>) {
    let Some(wanted) = candidates.as_ref().filter(|set| !set.is_empty()) else {
        return;
    };
    let mut safe = BTreeSet::new();
    let mut remaining = MAX_SCAN;
    for block in &function.blocks {
        let Some(left) = remaining.checked_sub(1) else {
            *candidates = None;
            return;
        };
        remaining = left;
        for instruction in &block.instructions {
            let Some(left) = remaining.checked_sub(1) else {
                *candidates = None;
                return;
            };
            remaining = left;
            let Some(id) = instruction.result.filter(|id| wanted.contains(id)) else {
                continue;
            };
            let valid = match (&instruction.kind, function.values.get(id.0 as usize)) {
                (InstructionKind::Constant(Constant::I32(n)), Some(Type::I32)) => {
                    *n != 0 && *n != -1
                }
                (InstructionKind::Constant(Constant::I64(n)), Some(Type::I64)) => {
                    *n != 0 && *n != -1
                }
                (InstructionKind::Constant(Constant::U8(n)), Some(Type::U8)) => *n != 0,
                (InstructionKind::Constant(Constant::U16(n)), Some(Type::U16)) => *n != 0,
                _ => false,
            };
            if valid {
                safe.insert(id);
            }
        }
    }
    *candidates = Some(safe);
}
