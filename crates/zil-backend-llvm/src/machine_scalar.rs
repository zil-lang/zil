//! Evaluate bounded scalar leaves at known direct call sites.
//! Planning reads the unchanged model; nonempty plans require full original preflight.
//! It neither removes definitions nor changes argument evaluation or control flow.
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use zil_ir::{LirView, Type};
use zil_llvm_api::model::{Arg, Function, Item, Module, Op, Ty, Val};

const MAX_LEAF_BLOCKS: usize = 32;
const MAX_LEAF_INSTRUCTIONS: usize = 128;
const MAX_LEAF_PARAMETERS: usize = 16;
const MAX_EVALUATION_STEPS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scalar {
    bits: u32,
    raw: u64,
}
impl Scalar {
    fn new(bits: u32, raw: u64) -> Self {
        Self {
            bits,
            raw: raw & mask(bits),
        }
    }
    fn signed(self) -> i128 {
        if self.raw & (1u64 << (self.bits - 1)) != 0 {
            i128::from(self.raw) - (1i128 << self.bits)
        } else {
            i128::from(self.raw)
        }
    }
    fn value(self) -> Val {
        if self.bits == 1 {
            Val::Bool(self.raw != 0)
        } else {
            // Positive signless representation also handles the unsigned range.
            Val::Int(i128::from(self.raw))
        }
    }
}
fn mask(bits: u32) -> u64 {
    u64::MAX >> (64 - bits)
}
fn width(ty: &Ty) -> Option<u32> {
    match ty {
        Ty::Int(bits @ (1 | 8 | 16 | 32 | 64)) => Some(*bits),
        _ => None,
    }
}
fn source_scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64
    )
}

#[derive(Clone, Copy, Debug)]
enum Known {
    Scalar(Scalar),
    Overflow(Scalar, bool),
}
type Values<'a> = HashMap<&'a str, Known>;

fn scalar(value: &Val, ty: &Ty, values: &Values<'_>) -> Option<Scalar> {
    let bits = width(ty)?;
    let raw = match value {
        Val::Int(n) => *n as u64,
        Val::Bool(b) if bits == 1 => u64::from(*b),
        Val::Zero => 0,
        Val::Local(name) => match values.get(name.as_str())? {
            Known::Scalar(value) if value.bits == bits => return Some(*value),
            _ => return None,
        },
        // In particular, freeze(undef/poison) is not a known integer.
        _ => return None,
    };
    Some(Scalar::new(bits, raw))
}

#[derive(Clone, Copy)]
enum Arithmetic {
    Add,
    Subtract,
    Multiply,
}
#[derive(Clone, Copy)]
struct Intrinsic {
    bits: u32,
    signed: bool,
    operation: Arithmetic,
}
type Intrinsics<'a> = BTreeMap<&'a str, Intrinsic>;

fn overflow_declarations(module: &Module) -> Intrinsics<'_> {
    let mut result = BTreeMap::new();
    for item in &module.items {
        let Item::Function(function) = item else {
            continue;
        };
        if function.blocks.is_some()
            || !function.attrs.is_empty()
            || function.signature.variadic
            || !function.signature.result_attrs.is_empty()
            || function.signature.params.len() != 2
            || function
                .signature
                .params
                .iter()
                .any(|p| !p.attrs.is_empty())
        {
            continue;
        }
        for bits in [8, 16, 32, 64] {
            if function.signature.result != Ty::Struct(vec![Ty::Int(bits), Ty::Int(1)])
                || function
                    .signature
                    .params
                    .iter()
                    .any(|p| p.ty != Ty::Int(bits))
            {
                continue;
            }
            for (name, signed, operation) in [
                ("sadd", true, Arithmetic::Add),
                ("ssub", true, Arithmetic::Subtract),
                ("smul", true, Arithmetic::Multiply),
                ("uadd", false, Arithmetic::Add),
                ("usub", false, Arithmetic::Subtract),
                ("umul", false, Arithmetic::Multiply),
            ] {
                if function.name == format!("llvm.{name}.with.overflow.i{bits}") {
                    result.insert(
                        function.name.as_str(),
                        Intrinsic {
                            bits,
                            signed,
                            operation,
                        },
                    );
                }
            }
        }
    }
    result
}

fn arithmetic(intrinsic: Intrinsic, left: Scalar, right: Scalar) -> Known {
    let bits = intrinsic.bits;
    let raw = match intrinsic.operation {
        Arithmetic::Add => left.raw.wrapping_add(right.raw),
        Arithmetic::Subtract => left.raw.wrapping_sub(right.raw),
        Arithmetic::Multiply => left.raw.wrapping_mul(right.raw),
    };
    let overflow = if intrinsic.signed {
        let (a, b) = (left.signed(), right.signed());
        let exact = match intrinsic.operation {
            Arithmetic::Add => a + b,
            Arithmetic::Subtract => a - b,
            Arithmetic::Multiply => a * b,
        };
        exact < -(1i128 << (bits - 1)) || exact > (1i128 << (bits - 1)) - 1
    } else {
        let (a, b) = (u128::from(left.raw), u128::from(right.raw));
        match intrinsic.operation {
            Arithmetic::Add => a + b > u128::from(mask(bits)),
            Arithmetic::Subtract => a < b,
            Arithmetic::Multiply => a * b > u128::from(mask(bits)),
        }
    };
    Known::Overflow(Scalar::new(bits, raw), overflow)
}

fn known_operation(op: &Op, values: &Values<'_>, intrinsics: &Intrinsics<'_>) -> Option<Known> {
    let value = match op {
        Op::Binary {
            opcode,
            ty,
            left,
            right,
        } => {
            let (a, b) = (scalar(left, ty, values)?, scalar(right, ty, values)?);
            let raw = match opcode.as_str() {
                "add" => a.raw.wrapping_add(b.raw),
                "sub" => a.raw.wrapping_sub(b.raw),
                "mul" => a.raw.wrapping_mul(b.raw),
                "and" => a.raw & b.raw,
                "or" => a.raw | b.raw,
                "xor" => a.raw ^ b.raw,
                _ => return None,
            };
            Scalar::new(a.bits, raw)
        }
        Op::Compare {
            predicate,
            ty,
            left,
            right,
        } => {
            let (a, b) = (scalar(left, ty, values)?, scalar(right, ty, values)?);
            let result = match predicate.as_str() {
                "eq" => a.raw == b.raw,
                "ne" => a.raw != b.raw,
                "ult" => a.raw < b.raw,
                "ule" => a.raw <= b.raw,
                "ugt" => a.raw > b.raw,
                "uge" => a.raw >= b.raw,
                "slt" => a.signed() < b.signed(),
                "sle" => a.signed() <= b.signed(),
                "sgt" => a.signed() > b.signed(),
                "sge" => a.signed() >= b.signed(),
                _ => return None,
            };
            Scalar::new(1, u64::from(result))
        }
        Op::Cast { opcode, arg, to } => {
            let input = scalar(&arg.value, &arg.ty, values)?;
            let bits = width(to)?;
            let raw = match opcode.as_str() {
                "trunc" if bits < input.bits => input.raw,
                "zext" if bits > input.bits => input.raw,
                "sext" if bits > input.bits => input.signed() as u64,
                _ => return None,
            };
            Scalar::new(bits, raw)
        }
        Op::Freeze(arg) => scalar(&arg.value, &arg.ty, values)?,
        Op::Extract { arg, index } => {
            let Val::Local(name) = &arg.value else {
                return None;
            };
            let Known::Overflow(value, overflow) = values.get(name.as_str())? else {
                return None;
            };
            match index {
                0 => *value,
                1 => Scalar::new(1, u64::from(*overflow)),
                _ => return None,
            }
        }
        Op::Call {
            result,
            result_attrs,
            callee: Val::Global(name),
            args,
            vararg,
        } => {
            let intrinsic = *intrinsics.get(name.as_str())?;
            if !result_attrs.is_empty()
                || vararg.is_some()
                || args.len() != 2
                || args
                    .iter()
                    .any(|a| a.ty != Ty::Int(intrinsic.bits) || !a.attrs.is_empty())
            {
                return None;
            }
            // Unknown inputs are common while scanning callers. Reject them
            // before allocating the intrinsic return-pair type for comparison.
            let left = scalar(&args[0].value, &args[0].ty, values)?;
            let right = scalar(&args[1].value, &args[1].ty, values)?;
            if *result != Ty::Struct(vec![Ty::Int(intrinsic.bits), Ty::Int(1)]) {
                return None;
            }
            return Some(arithmetic(intrinsic, left, right));
        }
        _ => return None,
    };
    Some(Known::Scalar(value))
}

struct Leaf<'a> {
    function: &'a Function,
    labels: BTreeMap<&'a str, usize>,
}

fn scalar_signature(function: &Function) -> bool {
    function.signature.params.len() <= MAX_LEAF_PARAMETERS
        && !function.signature.variadic
        && function.attrs.is_empty()
        && function.signature.result_attrs.is_empty()
        && width(&function.signature.result).is_some()
        && function
            .signature
            .params
            .iter()
            .all(|p| width(&p.ty).is_some() && p.attrs.is_empty())
}

fn leaf<'a>(function: &'a Function, intrinsics: &Intrinsics<'_>) -> Option<Leaf<'a>> {
    let blocks = function.blocks.as_ref()?;
    if blocks.is_empty()
        || blocks.len() > MAX_LEAF_BLOCKS
        || blocks
            .iter()
            .try_fold(MAX_LEAF_INSTRUCTIONS, |remaining, block| {
                remaining.checked_sub(block.instructions.len())
            })
            .is_none()
        || !scalar_signature(function)
    {
        return None;
    }
    let labels = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = vec![false; blocks.len()];
    let mut edges = vec![vec![]; blocks.len()];
    let mut stack = vec![0];
    while let Some(id) = stack.pop() {
        if reachable[id] {
            continue;
        }
        reachable[id] = true;
        for instruction in &blocks[id].instructions {
            match &instruction.op {
                Op::Binary { opcode, ty, .. }
                    if width(ty).is_some()
                        && matches!(
                            opcode.as_str(),
                            "add" | "sub" | "mul" | "and" | "or" | "xor"
                        ) => {}
                Op::Compare { ty, .. } if width(ty).is_some() => {}
                Op::Cast { opcode, arg, to }
                    if width(&arg.ty).is_some()
                        && width(to).is_some()
                        && matches!(opcode.as_str(), "trunc" | "zext" | "sext") => {}
                Op::Freeze(arg) if width(&arg.ty).is_some() => {}
                Op::Extract { .. } => {}
                Op::Call {
                    callee: Val::Global(name),
                    ..
                } if intrinsics.contains_key(name.as_str()) => {}
                Op::Call {
                    result: Ty::Void,
                    callee: Val::Global(name),
                    args,
                    vararg: None,
                    result_attrs,
                } if name == "llvm.trap" && args.is_empty() && result_attrs.is_empty() => {}
                Op::Branch(label) => edges[id].push(*labels.get(label.as_str())?),
                Op::CondBranch { yes, no, .. } => {
                    edges[id].push(*labels.get(yes.as_str())?);
                    edges[id].push(*labels.get(no.as_str())?);
                }
                Op::Return(Some(arg)) if width(&arg.ty).is_some() => {}
                Op::Unreachable => {}
                _ => return None,
            }
        }
        stack.extend(edges[id].iter().copied());
    }
    // The complete entry-reachable CFG must be acyclic, not just the chosen path.
    let mut incoming = vec![0usize; blocks.len()];
    for (id, successors) in edges.iter().enumerate() {
        if reachable[id] {
            for &successor in successors {
                incoming[successor] += 1;
            }
        }
    }
    let mut ready = (0..blocks.len())
        .filter(|&i| reachable[i] && incoming[i] == 0)
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        for &successor in &edges[id] {
            incoming[successor] -= 1;
            if incoming[successor] == 0 {
                ready.push_back(successor);
            }
        }
    }
    if visited != reachable.iter().filter(|&&r| r).count() {
        return None;
    }
    Some(Leaf { function, labels })
}

fn evaluate(
    leaf: &Leaf<'_>,
    arguments: &[Scalar],
    intrinsics: &Intrinsics<'_>,
    fuel: &mut usize,
) -> Option<Scalar> {
    if arguments.len() != leaf.function.signature.params.len() {
        return None;
    }
    let mut values = HashMap::new();
    for (parameter, &argument) in leaf.function.signature.params.iter().zip(arguments) {
        if width(&parameter.ty) != Some(argument.bits) {
            return None;
        }
        values.insert(parameter.name.as_deref()?, Known::Scalar(argument));
    }
    let blocks = leaf.function.blocks.as_ref()?;
    let mut block = 0;
    let mut steps = 0;
    loop {
        let mut next = None;
        for instruction in &blocks[block].instructions {
            if steps >= MAX_LEAF_INSTRUCTIONS || *fuel == 0 {
                return None;
            }
            steps += 1;
            *fuel -= 1;
            match &instruction.op {
                Op::Branch(label) => next = Some(*leaf.labels.get(label.as_str())?),
                Op::CondBranch { condition, yes, no } => {
                    let condition = scalar(condition, &Ty::Int(1), &values)?;
                    let label = if condition.raw != 0 { yes } else { no };
                    next = Some(*leaf.labels.get(label.as_str())?);
                }
                Op::Return(Some(arg)) => return scalar(&arg.value, &arg.ty, &values),
                // A reached trap or other unknown/effectful operation fails evaluation.
                op => {
                    let known = known_operation(op, &values, intrinsics)?;
                    if matches!(known, Known::Overflow(_, true)) {
                        return None;
                    }
                    values.insert(instruction.result.as_deref()?, known);
                }
            }
        }
        block = next?;
    }
}

pub(crate) fn fold_known_leaf_calls(module: &mut Module, lir: &LirView<'_>) -> usize {
    let sources = lir
        .functions
        .iter()
        .filter(|function| {
            function.foreign.is_none()
                && function.environment.is_none()
                && source_scalar(&function.return_type)
                && function.parameters.iter().all(|id| {
                    function
                        .locals
                        .get(id.0 as usize)
                        .is_some_and(|local| source_scalar(&local.ty))
                })
        })
        .map(|function| format!("zil.fn.{}", function.id.0))
        .collect::<BTreeSet<_>>();
    fold_with_sources(module, &sources)
}

type Replacement = (usize, usize, usize, Scalar);

// This planner may inspect an invalid model. It never publishes a transformed
// module: all coordinates refer to this immutable input, and any nonempty plan
// must pass the complete original preflight before any coordinate is applied.
fn plan_with_sources(module: &Module, sources: &BTreeSet<String>) -> Vec<Replacement> {
    let mut fuel = MAX_EVALUATION_STEPS;
    plan_with_sources_and_fuel(module, sources, &mut fuel)
}

fn plan_with_sources_and_fuel(
    module: &Module,
    sources: &BTreeSet<String>,
    fuel: &mut usize,
) -> Vec<Replacement> {
    if sources.is_empty() {
        return vec![];
    }
    let intrinsics = overflow_declarations(module);
    let functions = module
        .items
        .iter()
        .filter_map(|item| {
            let Item::Function(function) = item else {
                return None;
            };
            sources
                .contains(&function.name)
                .then_some((function.name.as_str(), function))
        })
        .collect::<BTreeMap<_, _>>();
    if functions.is_empty() {
        return vec![];
    }
    // Cache eligibility, never results at particular arguments. Every function
    // body stays original until the entire plan has passed original preflight.
    let mut leaves = BTreeMap::<&str, Option<Leaf<'_>>>::new();
    let mut replacements = vec![];
    'callers: for (item_id, item) in module.items.iter().enumerate() {
        let Item::Function(function) = item else {
            continue;
        };
        if *fuel == 0 {
            break 'callers;
        }
        // A function without a named direct call to an original source callee
        // cannot contribute a replacement, consume evaluation fuel or populate
        // the leaf cache. Its caller-local known values have no other consumers.
        // Keep wrappers and unreachable blocks in this conservative search.
        if !function.blocks.iter().flatten().any(|block| {
            block.instructions.iter().any(|instruction| {
                instruction.result.is_some()
                    && matches!(&instruction.op, Op::Call { callee: Val::Global(name), .. }
                        if functions.contains_key(name.as_str()))
            })
        }) {
            continue;
        }
        let mut values = HashMap::new();
        for (block_id, block) in function.blocks.iter().flatten().enumerate() {
            for (instruction_id, instruction) in block.instructions.iter().enumerate() {
                if *fuel == 0 {
                    break 'callers;
                }
                let Some(result_name) = instruction.result.as_deref() else {
                    continue;
                };
                let replacement = if let Op::Call {
                    result,
                    result_attrs,
                    callee: Val::Global(name),
                    args,
                    vararg: None,
                } = &instruction.op
                {
                    functions
                        .get(name.as_str())
                        .filter(|callee| {
                            scalar_signature(callee)
                                && width(result).is_some()
                                && *result == callee.signature.result
                                && result_attrs.is_empty()
                                && args.len() <= MAX_LEAF_PARAMETERS
                                && args.len() == callee.signature.params.len()
                                && args.iter().zip(&callee.signature.params).all(|(a, p)| {
                                    width(&a.ty).is_some() && a.ty == p.ty && a.attrs == p.attrs
                                })
                        })
                        .and_then(|callee| {
                            let arguments = args
                                .iter()
                                .map(|a| scalar(&a.value, &a.ty, &values))
                                .collect::<Option<Vec<_>>>()?;
                            let leaf = leaves
                                .entry(name.as_str())
                                .or_insert_with(|| leaf(callee, &intrinsics))
                                .as_ref()?;
                            evaluate(leaf, &arguments, &intrinsics, fuel)
                        })
                } else {
                    None
                };
                let known = if let Some(value) = replacement {
                    replacements.push((item_id, block_id, instruction_id, value));
                    Some(Known::Scalar(value))
                } else {
                    // No caller CFG rewriting, speculative branches, phi, load or call effects.
                    known_operation(&instruction.op, &values, &intrinsics)
                };
                if let Some(known) = known {
                    values.insert(result_name, known);
                }
            }
        }
    }
    replacements
}

fn fold_with_sources(module: &mut Module, sources: &BTreeSet<String>) -> usize {
    let replacements = plan_with_sources(module, sources);
    if replacements.is_empty() {
        return 0;
    }
    let summaries = summary::plan(module, sources, &replacements);
    if zil_llvm_api::validate_module(module).is_err() {
        // No mutation occurred, including on malformed original models. Leave
        // the existing diagnostic/backend path and per-unit preflight intact.
        return 0;
    }
    let count = replacements.len() + summaries.count();
    apply_leaf_replacements(module, replacements);
    summaries.apply(module);
    count
}

fn apply_leaf_replacements(module: &mut Module, replacements: Vec<Replacement>) {
    for (item, block, instruction, value) in replacements {
        let Item::Function(function) = &mut module.items[item] else {
            unreachable!()
        };
        function.blocks.as_mut().expect("original definition")[block].instructions[instruction]
            .op = Op::Freeze(Arg::new(Ty::Int(value.bits), value.value()));
    }
}

#[cfg(test)]
fn fold_leaf_only_with_sources(module: &mut Module, sources: &BTreeSet<String>) -> usize {
    let replacements = plan_with_sources(module, sources);
    if replacements.is_empty() || zil_llvm_api::validate_module(module).is_err() {
        return 0;
    }
    let count = replacements.len();
    apply_leaf_replacements(module, replacements);
    count
}

#[path = "machine_scalar_summary.rs"]
mod summary;

#[cfg(test)]
#[path = "machine_scalar_tests.rs"]
mod tests;
