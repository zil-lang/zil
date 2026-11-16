//! Consume proved overflow flags without changing ordinary calls or their order.
//! The caller precharges this bounded collection of linear walks and validates
//! the complete original model. Coordinates are consumed only before any edit.
use super::Arithmetic;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use zil_llvm_api::model::{Block, Function, Op, Ty, Val};

fn pair_type(ty: &Ty) -> bool {
    matches!(ty, Ty::Struct(fields) if fields.as_slice() == [Ty::Int(32), Ty::Int(1)])
}

fn uses(blocks: &[Block]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let mut pending = Vec::new();
    for inst in blocks.iter().flat_map(|b| &b.instructions) {
        pending.extend(inst.op.values());
        while let Some(value) = pending.pop() {
            match value {
                Val::Local(name) => *counts.entry(name.clone()).or_default() += 1,
                Val::Array(args) => pending.extend(args.iter().map(|a| &a.value)),
                Val::Gep { ptr, indices, .. } => {
                    pending.push(ptr.as_ref());
                    pending.extend(indices.iter().map(|a| &a.value));
                }
                _ => {}
            }
        }
    }
    counts
}

fn checked_binary(op: &Op, intrinsics: &BTreeMap<String, Arithmetic>) -> Option<Op> {
    let Op::Call {
        result,
        result_attrs,
        callee: Val::Global(name),
        args,
        vararg: None,
    } = op
    else {
        return None;
    };
    if !pair_type(result)
        || !result_attrs.is_empty()
        || args.len() != 2
        || args
            .iter()
            .any(|a| a.ty != Ty::Int(32) || !a.attrs.is_empty())
    {
        return None;
    }
    let opcode = match intrinsics.get(name)? {
        Arithmetic::Add => "add",
        Arithmetic::Sub => "sub",
        Arithmetic::Mul => "mul",
    };
    Some(Op::Binary {
        opcode: opcode.into(),
        ty: Ty::Int(32),
        left: args[0].value.clone(),
        right: args[1].value.clone(),
    })
}

fn literal(value: &Val) -> Option<i32> {
    match value {
        Val::Int(n) => Some(*n as i32),
        Val::Zero => Some(0),
        _ => None,
    }
}

fn materializer(op: &Op) -> Option<i32> {
    let Op::Binary {
        opcode,
        ty: Ty::Int(32),
        left,
        right,
    } = op
    else {
        return None;
    };
    let (left, right) = (literal(left)?, literal(right)?);
    let value = match opcode.as_str() {
        "add" => left.wrapping_add(right),
        "sub" => left.wrapping_sub(right),
        "mul" => left.wrapping_mul(right),
        _ => return None,
    };
    (value > 0).then_some(value)
}

fn phi_labels(blocks: &[Block]) -> HashSet<String> {
    blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| {
            if let Op::Phi { incoming, .. } = &i.op {
                Some(incoming)
            } else {
                None
            }
        })
        .flatten()
        .map(|(_, label)| label.clone())
        .collect()
}

fn successors(op: &Op) -> impl Iterator<Item = &str> {
    let (first, second) = match op {
        Op::Branch(label) => (Some(label.as_str()), None),
        Op::CondBranch { yes, no, .. } => (Some(yes.as_str()), (yes != no).then_some(no.as_str())),
        _ => (None, None),
    };
    first.into_iter().chain(second)
}

fn merge_chains(blocks: Vec<Block>) -> Vec<Block> {
    let labels: HashMap<_, _> = blocks
        .iter()
        .enumerate()
        .map(|(index, b)| (b.name.clone(), index))
        .collect();
    let phi_labels = phi_labels(&blocks);
    let mut predecessors = vec![0usize; blocks.len()];
    for block in &blocks {
        if let Some(last) = block.instructions.last() {
            for label in successors(&last.op) {
                if let Some(&index) = labels.get(label) {
                    predecessors[index] += 1;
                }
            }
        }
    }
    let mut remaining: Vec<_> = blocks.into_iter().map(Some).collect();
    let mut result = Vec::new();
    for index in 0..remaining.len() {
        let Some(mut block) = remaining[index].take() else {
            continue;
        };
        while let Some(Op::Branch(label)) = block.instructions.last().map(|i| &i.op) {
            let Some(&next) = labels.get(label.as_str()) else {
                break;
            };
            // Taking each block once is also the cycle guard. Original incoming
            // counts remain valid for this chain: its tail's sole outgoing edge
            // is simply transferred to the head, with no new predecessor added.
            if next == 0 || next == index || predecessors[next] != 1 || phi_labels.contains(label) {
                break;
            }
            let Some(successor) = remaining[next].as_ref() else {
                break;
            };
            if successor
                .instructions
                .iter()
                .any(|i| matches!(i.op, Op::Phi { .. }))
            {
                break;
            }
            let successor = remaining[next].take().expect("checked present successor");
            block.instructions.pop();
            block.instructions.extend(successor.instructions);
        }
        result.push(block);
    }
    result
}

fn discard_empty_unreachable(blocks: Vec<Block>) -> Vec<Block> {
    let labels: HashMap<_, _> = blocks
        .iter()
        .enumerate()
        .map(|(index, b)| (b.name.as_str(), index))
        .collect();
    let phi_labels = phi_labels(&blocks);
    let mut keep = vec![false; blocks.len()];
    // Keep every definition and PHI predecessor even if unreachable, then keep
    // its entire successor closure too: no retained branch may dangle.
    let mut pending: Vec<_> = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, b)| {
            (index == 0
                || phi_labels.contains(&b.name)
                || b.instructions.iter().any(|i| i.result.is_some()))
            .then_some(index)
        })
        .collect();
    while let Some(index) = pending.pop() {
        if keep[index] {
            continue;
        }
        keep[index] = true;
        if let Some(last) = blocks[index].instructions.last() {
            for label in successors(&last.op) {
                if let Some(&next) = labels.get(label) {
                    pending.push(next);
                }
            }
        }
    }
    blocks
        .into_iter()
        .zip(keep)
        .filter_map(|(b, keep)| keep.then_some(b))
        .collect()
}

/// Returns the number of proved flags consumed, not the number of cleanup edits.
pub(super) fn apply(
    f: &mut Function,
    flags: &[(usize, usize)],
    intrinsics: &BTreeMap<String, Arithmetic>,
) -> usize {
    let Some(original) = &f.blocks else {
        return 0;
    };
    let mut proved = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for &(block, instruction) in flags {
        let Some(inst) = original
            .get(block)
            .and_then(|b| b.instructions.get(instruction))
        else {
            continue;
        };
        if let Op::Extract { arg, index: 1 } = &inst.op
            && pair_type(&arg.ty)
            && arg.attrs.is_empty()
            && let Val::Local(pair) = &arg.value
            && let Some(name) = &inst.result
        {
            proved.insert(name.clone());
            pairs.insert(pair.clone());
        }
    }
    if proved.is_empty() {
        return 0;
    }
    let mut blocks: Vec<Block> = original.iter().cloned().collect();
    for inst in blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if inst
            .result
            .as_ref()
            .is_some_and(|name| proved.contains(name))
        {
            inst.op = Op::Binary {
                opcode: "or".into(),
                ty: Ty::Int(1),
                left: Val::Bool(false),
                right: Val::Bool(false),
            };
        }
    }
    let counts = uses(&blocks);
    let replacements: HashMap<_, _> = blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|inst| {
            let name = inst.result.as_ref()?;
            if !pairs.contains(name) || counts.get(name) != Some(&1) {
                return None;
            }
            checked_binary(&inst.op, intrinsics).map(|op| (name.clone(), op))
        })
        .collect();
    let mut lowered = HashSet::new();
    for inst in blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let Op::Extract { arg, index: 0 } = &inst.op
            && pair_type(&arg.ty)
            && arg.attrs.is_empty()
            && let Val::Local(name) = &arg.value
            && let Some(op) = replacements.get(name)
        {
            lowered.insert(name.clone());
            inst.op = op.clone();
        }
    }
    // Exact sole-use matching above proves these particular intrinsic calls dead.
    for block in &mut blocks {
        block
            .instructions
            .retain(|i| !i.result.as_ref().is_some_and(|n| lowered.contains(n)));
    }
    let phi_destinations: HashSet<_> = blocks
        .iter()
        .filter(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.op, Op::Phi { .. }))
        })
        .map(|b| b.name.clone())
        .collect();
    for inst in blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let Op::CondBranch { condition: Val::Local(flag), yes, no } = &inst.op
            && proved.contains(flag)
            // Leave PHI-edge surgery to LLVM; ordinary trap edges have no PHI.
            && !phi_destinations.contains(yes)
        {
            inst.op = Op::Branch(no.clone());
        }
    }
    let constants: HashMap<_, _> = blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| Some((i.result.as_ref()?.clone(), materializer(&i.op)?)))
        .collect();
    let mut inlined = HashSet::new();
    for inst in blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let Op::Binary {
            opcode,
            ty: Ty::Int(32),
            right,
            ..
        } = &mut inst.op
            && opcode == "srem"
            && let Val::Local(name) = right
            && let Some(&value) = constants.get(name)
        {
            inlined.insert(name.clone());
            *right = Val::Int(i128::from(value));
        }
    }
    let counts = uses(&blocks);
    for block in &mut blocks {
        block.instructions.retain(|i| {
            !i.result.as_ref().is_some_and(|name| {
                (proved.contains(name) || inlined.contains(name)) && !counts.contains_key(name)
            })
        });
    }
    f.blocks = Some(discard_empty_unreachable(merge_chains(blocks)).into());
    proved.len()
}

#[cfg(test)]
mod tests;
