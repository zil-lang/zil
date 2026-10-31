//! Contract strict monotone checked-add chains in the explicit incremental O2 strategy.
//! Each nonempty plan requires complete unchanged-model preflight before mutation.
//! Canonical function definitions, trap behavior and the scalar evaluator remain intact.
use std::collections::{HashMap, VecDeque};
use zil_llvm_api::model::{Arg, Block, Function, Inst, Item, Module, Op, Ty, Val};

const MAX_USE_SCAN_DEPTH: usize = 128;
const MAX_USE_SCAN_VALUES: usize = 1_000_000;

#[cfg(test)]
std::thread_local! {
    static WORK: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}
#[cfg(test)]
pub(crate) fn reset_work() {
    WORK.with(|work| work.set((0, 0)));
}
#[cfg(test)]
pub(crate) fn work() -> (usize, usize) {
    WORK.with(std::cell::Cell::get)
}

// This runs before preflight, so it must not recursively descend untrusted Val.
// Resource limits only suppress an optimization plan, never create a diagnostic.
fn collect_uses<'a>(
    root: &'a Val,
    uses: &mut HashMap<&'a str, usize>,
    pending: &mut Vec<(&'a Val, usize)>,
    remaining: &mut usize,
) -> bool {
    pending.clear();
    pending.push((root, 0));
    while let Some((value, depth)) = pending.pop() {
        #[cfg(test)]
        WORK.with(|work| {
            let (nodes, probes) = work.get();
            work.set((nodes + 1, probes));
        });
        if depth > MAX_USE_SCAN_DEPTH || *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        match value {
            Val::Local(name) => *uses.entry(name).or_default() += 1,
            Val::Array(args) => {
                if args.len() > remaining.saturating_sub(pending.len()) {
                    return false;
                }
                pending.extend(args.iter().map(|arg| (&arg.value, depth + 1)));
            }
            Val::Gep { ptr, indices, .. } => {
                if indices.len().saturating_add(1) > remaining.saturating_sub(pending.len()) {
                    return false;
                }
                pending.push((ptr.as_ref(), depth + 1));
                pending.extend(indices.iter().map(|arg| (&arg.value, depth + 1)));
            }
            _ => {}
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kind {
    bits: u32,
    signed: bool,
}
impl Kind {
    fn ty(self) -> Ty {
        Ty::Int(self.bits)
    }
    fn pair(self) -> Ty {
        Ty::Struct(vec![self.ty(), Ty::Int(1)])
    }
    fn interpret(self, raw: i128) -> i128 {
        let modulus = 1i128 << self.bits;
        let raw = raw & (modulus - 1);
        if self.signed && raw >= modulus / 2 {
            raw - modulus
        } else {
            raw
        }
    }
    fn fits(self, n: i128) -> bool {
        if self.signed {
            let bound = 1i128 << (self.bits - 1);
            (-bound..bound).contains(&n)
        } else {
            (0..(1i128 << self.bits)).contains(&n)
        }
    }
}

#[derive(Clone)]
struct Check {
    kind: Kind,
    intrinsic: String,
    source: String,
    constant: i128,
    pair: String,
    value: String,
    flag: String,
    materializer: Option<String>,
    trap: String,
    next: String,
}
struct Chain {
    blocks: Vec<usize>,
    replacement: Vec<Inst>,
}
struct FunctionPlan {
    item: usize,
    chains: Vec<Chain>,
}

fn literal(value: &Val) -> Option<i128> {
    match value {
        Val::Int(n) => Some(*n),
        Val::Zero => Some(0),
        _ => None,
    }
}
fn empty_signature(function: &Function) -> bool {
    function.blocks.is_none()
        && function.attrs.is_empty()
        && function.signature.result_attrs.is_empty()
        && !function.signature.variadic
        && function.signature.params.iter().all(|p| p.attrs.is_empty())
}
fn declarations(module: &Module) -> (HashMap<&str, Kind>, bool) {
    let mut intrinsics = HashMap::new();
    let mut trap = false;
    for item in &module.items {
        let Item::Function(function) = item else {
            continue;
        };
        if !empty_signature(function) {
            continue;
        }
        if function.name == "llvm.trap"
            && function.signature.result == Ty::Void
            && function.signature.params.is_empty()
        {
            trap = true;
        }
        let (signed, suffix) =
            if let Some(suffix) = function.name.strip_prefix("llvm.sadd.with.overflow.i") {
                (true, suffix)
            } else if let Some(suffix) = function.name.strip_prefix("llvm.uadd.with.overflow.i") {
                (false, suffix)
            } else {
                continue;
            };
        let bits = match suffix {
            "8" => 8,
            "16" => 16,
            "32" => 32,
            "64" => 64,
            _ => continue,
        };
        let kind = Kind { bits, signed };
        if function.signature.result == kind.pair()
            && function.signature.params.len() == 2
            && function.signature.params.iter().all(|p| p.ty == kind.ty())
        {
            intrinsics.insert(function.name.as_str(), kind);
        }
    }
    (intrinsics, trap)
}

fn checked_block(block: &Block, intrinsics: &HashMap<&str, Kind>) -> Option<Check> {
    let instructions = &block.instructions;
    let offset = match instructions.len() {
        4 => 0,
        5 => 1,
        _ => return None,
    };
    let call = &instructions[offset];
    let Op::Call {
        result,
        result_attrs,
        callee: Val::Global(name),
        args,
        vararg: None,
    } = &call.op
    else {
        return None;
    };
    let kind = *intrinsics.get(name.as_str())?;
    if result != &kind.pair()
        || !result_attrs.is_empty()
        || args.len() != 2
        || args
            .iter()
            .any(|a| a.ty != kind.ty() || !a.attrs.is_empty())
    {
        return None;
    }
    let materializer = if offset == 1 {
        let prefix = &instructions[0];
        let Op::Binary {
            opcode,
            ty,
            left,
            right,
        } = &prefix.op
        else {
            return None;
        };
        if opcode != "add" || ty != &kind.ty() {
            return None;
        }
        // Concrete, flag-free integer add only. Truncate before interpreting sign.
        let raw = kind.interpret(literal(left)?) + kind.interpret(literal(right)?);
        Some((prefix.result.as_ref()?, kind.interpret(raw)))
    } else {
        None
    };
    let constant = |value: &Val| {
        literal(value).map(|n| kind.interpret(n)).or_else(|| {
            let Val::Local(name) = value else { return None };
            materializer.filter(|(m, _)| *m == name).map(|(_, n)| n)
        })
    };
    let (source, number, constant_value) = match (&args[0].value, &args[1].value) {
        (Val::Local(source), value) if constant(&args[0].value).is_none() => {
            (source, constant(value)?, value)
        }
        (value, Val::Local(source)) if constant(&args[1].value).is_none() => {
            (source, constant(value)?, value)
        }
        _ => return None,
    };
    if let Some((materializer, _)) = materializer
        && constant_value != &Val::Local(materializer.clone())
    {
        return None;
    }
    let pair = call.result.as_ref()?;
    let extract = |instruction: &Inst, wanted| {
        let Op::Extract { arg, index } = &instruction.op else {
            return None;
        };
        if *index != wanted
            || arg.ty != kind.pair()
            || !arg.attrs.is_empty()
            || arg.value != Val::Local(pair.clone())
        {
            return None;
        }
        instruction.result.clone()
    };
    let value = extract(&instructions[offset + 1], 0)?;
    let flag = extract(&instructions[offset + 2], 1)?;
    let Op::CondBranch {
        condition: Val::Local(condition),
        yes,
        no,
    } = &instructions[offset + 3].op
    else {
        return None;
    };
    if condition != &flag || yes == no || instructions[offset + 3].result.is_some() {
        return None;
    }
    Some(Check {
        kind,
        intrinsic: name.clone(),
        source: source.clone(),
        constant: number,
        pair: pair.clone(),
        value,
        flag,
        materializer: materializer.map(|(m, _)| m.clone()),
        trap: yes.clone(),
        next: no.clone(),
    })
}

fn exact_trap(block: &Block) -> bool {
    let [call, unreachable] = block.instructions.as_slice() else {
        return false;
    };
    matches!(&call.op, Op::Call { result: Ty::Void, result_attrs, callee: Val::Global(name), args, vararg: None }
        if name == "llvm.trap" && result_attrs.is_empty() && args.is_empty())
        && call.result.is_none()
        && unreachable.result.is_none()
        && matches!(unreachable.op, Op::Unreachable)
}

fn function_chains(function: &Function, intrinsics: &HashMap<&str, Kind>) -> Vec<Chain> {
    let Some(blocks) = &function.blocks else {
        return vec![];
    };
    // Allocation-free necessary-condition gate. checked_block can only succeed
    // at these lengths/offsets with one of these direct intrinsic callees.
    // Fewer than two such blocks proves that the unchanged planner returns no
    // chains, without building owned Check names or a per-block candidate Vec.
    let mut possible = 0;
    for block in blocks {
        let offset = match block.instructions.len() {
            4 => 0,
            5 => 1,
            _ => continue,
        };
        if let Op::Call {
            callee: Val::Global(name),
            ..
        } = &block.instructions[offset].op
            && let Some(kind) = intrinsics.get(name.as_str())
        {
            if offset == 1 {
                // checked_block requires this shallow materializer shape.
                // Do not interpret constants or allocate a Check in the gate.
                let prefix = &block.instructions[0];
                let Op::Binary {
                    opcode,
                    ty,
                    left,
                    right,
                } = &prefix.op
                else {
                    continue;
                };
                if opcode != "add"
                    || ty != &kind.ty()
                    || prefix.result.is_none()
                    || literal(left).is_none()
                    || literal(right).is_none()
                {
                    continue;
                }
            }
            possible += 1;
            if possible == 2 {
                break;
            }
        }
    }
    if possible < 2 {
        return vec![];
    }
    let checks = blocks
        .iter()
        .map(|b| checked_block(b, intrinsics))
        .collect::<Vec<_>>();
    if checks.iter().filter(|c| c.is_some()).take(2).count() < 2 {
        return vec![];
    }
    let names = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect::<HashMap<_, _>>();
    if names.len() != blocks.len() {
        return vec![];
    }
    // Every nonempty old plan contains at least two adjacent checked blocks.
    // This borrowed existence check is necessary only: eligibility, predecessor,
    // use-count, cycle, sign, sum and Phi checks remain in the unchanged planner.
    let has_adjacent_checks = checks.iter().flatten().any(|previous| {
        names
            .get(previous.next.as_str())
            .and_then(|&index| checks[index].as_ref())
            .is_some_and(|next| {
                next.kind == previous.kind
                    && next.intrinsic == previous.intrinsic
                    && next.trap == previous.trap
                    && next.source == previous.value
            })
    });
    if !has_adjacent_checks {
        return vec![];
    }
    let mut predecessors = vec![Vec::new(); blocks.len()];
    let mut successors = vec![Vec::new(); blocks.len()];
    let mut uses = HashMap::<&str, usize>::new();
    let mut pending = Vec::new();
    let mut remaining = MAX_USE_SCAN_VALUES;
    for (index, block) in blocks.iter().enumerate() {
        for instruction in &block.instructions {
            for value in instruction.op.values() {
                if !collect_uses(value, &mut uses, &mut pending, &mut remaining) {
                    return vec![];
                }
            }
        }
        let targets: Vec<&str> = match block.instructions.last().map(|i| &i.op) {
            Some(Op::Branch(next)) => vec![next],
            Some(Op::CondBranch { yes, no, .. }) => vec![yes, no],
            Some(Op::Return(_) | Op::Unreachable) => vec![],
            _ => return vec![],
        };
        for target in targets {
            let Some(&target) = names.get(target) else {
                return vec![];
            };
            successors[index].push(target);
            predecessors[target].push(index);
        }
    }
    // Conservatively reject any function CFG cycle, including unreachable cycles.
    let mut indegrees = predecessors.iter().map(Vec::len).collect::<Vec<_>>();
    let mut queue = indegrees
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(block) = queue.pop_front() {
        visited += 1;
        for &next in &successors[block] {
            indegrees[next] -= 1;
            if indegrees[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if visited != blocks.len() {
        return vec![];
    }
    let single = |name: &str, count: usize| uses.get(name).copied().unwrap_or(0) == count;
    let eligible = |check: &Check| {
        single(&check.pair, 2)
            && single(&check.flag, 1)
            && check
                .materializer
                .as_ref()
                .is_none_or(|name| single(name, 1))
            && names
                .get(check.trap.as_str())
                .is_some_and(|&i| exact_trap(&blocks[i]))
    };
    let mut claimed = vec![false; blocks.len()];
    let mut plans = Vec::new();
    for start in 0..blocks.len() {
        if claimed[start] {
            continue;
        }
        let Some(first) = checks[start].as_ref().filter(|c| eligible(c)) else {
            continue;
        };
        let mut sequence = vec![start];
        let mut current = start;
        let mut sum = first.constant;
        let mut sign = first.constant.signum();
        loop {
            #[cfg(test)]
            WORK.with(|work| {
                let (nodes, probes) = work.get();
                work.set((nodes, probes + 1));
            });
            let previous = checks[current].as_ref().expect("chain member");
            let Some(&next_id) = names.get(previous.next.as_str()) else {
                break;
            };
            if next_id == 0 || claimed[next_id] || predecessors[next_id].as_slice() != [current] {
                break;
            }
            let Some(next) = checks[next_id].as_ref().filter(|c| eligible(c)) else {
                break;
            };
            if next.kind != first.kind
                || next.intrinsic != first.intrinsic
                || next.trap != first.trap
                || next.source != previous.value
                || !single(&previous.value, 1)
            {
                break;
            }
            let next_sign = next.constant.signum();
            if first.kind.signed && sign != 0 && next_sign != 0 && sign != next_sign {
                break;
            }
            let Some(total) = sum
                .checked_add(next.constant)
                .filter(|n| first.kind.fits(*n))
            else {
                break;
            };
            sum = total;
            if sign == 0 {
                sign = next_sign;
            }
            sequence.push(next_id);
            current = next_id;
        }
        if sequence.len() < 2 {
            continue;
        }
        let endpoint = names[checks[current].as_ref().expect("last member").next.as_str()];
        if blocks[endpoint]
            .instructions
            .iter()
            .any(|i| matches!(i.op, Op::Phi { .. }))
        {
            // Keep the final checked block and its edge into PHI unchanged.
            // Contracting the prefix consumes a long run immediately; repeated
            // rejection of every suffix would otherwise make this path quadratic.
            let final_id = sequence.pop().expect("nonempty chain");
            sum -= checks[final_id].as_ref().expect("final check").constant;
            if sequence.len() < 2 {
                continue;
            }
            current = *sequence.last().expect("prefix remains");
        }
        let last = checks[current]
            .as_ref()
            .expect("last retained-prefix member");
        let pair = first.kind.pair();
        let replacement = vec![
            Inst::new(
                Some(last.pair.clone()),
                Op::Call {
                    result: pair.clone(),
                    result_attrs: vec![],
                    callee: Val::Global(first.intrinsic.clone()),
                    args: vec![
                        Arg::new(first.kind.ty(), Val::Local(first.source.clone())),
                        Arg::new(first.kind.ty(), Val::Int(sum)),
                    ],
                    vararg: None,
                },
            ),
            Inst::new(
                Some(last.value.clone()),
                Op::Extract {
                    arg: Arg::new(pair.clone(), Val::Local(last.pair.clone())),
                    index: 0,
                },
            ),
            Inst::new(
                Some(last.flag.clone()),
                Op::Extract {
                    arg: Arg::new(pair, Val::Local(last.pair.clone())),
                    index: 1,
                },
            ),
            Inst::new(
                None,
                Op::CondBranch {
                    condition: Val::Local(last.flag.clone()),
                    yes: last.trap.clone(),
                    no: last.next.clone(),
                },
            ),
        ];
        for &id in &sequence {
            claimed[id] = true;
        }
        plans.push(Chain {
            blocks: sequence,
            replacement,
        });
    }
    plans
}

pub(crate) fn fold_checked_add_chains(module: &mut Module) -> usize {
    let (intrinsics, has_trap) = declarations(module);
    if intrinsics.is_empty() || !has_trap {
        return 0;
    }
    let plans = module
        .items
        .iter()
        .enumerate()
        .filter_map(|(item, value)| {
            let Item::Function(function) = value else {
                return None;
            };
            let chains = function_chains(function, &intrinsics);
            (!chains.is_empty()).then_some(FunctionPlan { item, chains })
        })
        .collect::<Vec<_>>();
    if plans.is_empty() || zil_llvm_api::validate_module(module).is_err() {
        return 0;
    }
    let mut removed_checks = 0;
    for plan in plans {
        let Item::Function(function) = &mut module.items[plan.item] else {
            unreachable!()
        };
        let blocks = function.blocks.as_mut().expect("planned function");
        let mut removed = vec![false; blocks.len()];
        for chain in plan.chains {
            blocks[chain.blocks[0]].instructions = chain.replacement;
            for &id in &chain.blocks[1..] {
                removed[id] = true;
                removed_checks += 1;
            }
        }
        let mut id = 0;
        blocks.retain(|_| {
            let keep = !removed[id];
            id += 1;
            keep
        });
    }
    removed_checks
}

#[cfg(test)]
#[path = "machine_checked_add_chain_tests.rs"]
mod tests;
