//! Research-only constant summaries over the original scalar planner's virtual view.
//! No mutation or validation authority is created here.
use super::*;
use zil_llvm_api::model::{Block, Inst};

const MAX_SUMMARY_WORK: usize = 1_000_000;
type Coordinate = (usize, usize, usize);

#[derive(Default)]
pub(super) struct Plan {
    pub bodies: BTreeMap<usize, Scalar>,
    pub calls: Vec<Replacement>,
}

struct Work(usize);
impl Work {
    fn take(&mut self, amount: usize) -> Option<()> {
        match self.0.checked_sub(amount) {
            Some(remaining) => {
                self.0 = remaining;
                Some(())
            }
            None => {
                self.0 = 0;
                None
            }
        }
    }
}

struct Candidate<'a> {
    item: usize,
    function: &'a Function,
    labels: BTreeMap<&'a str, usize>,
    dependencies: BTreeSet<usize>,
}

fn atom(value: &Val) -> bool {
    matches!(
        value,
        Val::Local(_) | Val::Int(_) | Val::Bool(_) | Val::Zero
    )
}
fn scalar_arg(arg: &Arg) -> bool {
    arg.attrs.is_empty() && width(&arg.ty).is_some() && atom(&arg.value)
}
fn overflow_call(op: &Op, intrinsics: &Intrinsics<'_>) -> bool {
    let Op::Call {
        result,
        result_attrs,
        callee: Val::Global(name),
        args,
        vararg: None,
    } = op
    else {
        return false;
    };
    let Some(intrinsic) = intrinsics.get(name.as_str()) else {
        return false;
    };
    matches!(result, Ty::Struct(fields) if fields.as_slice()==[Ty::Int(intrinsic.bits),Ty::Int(1)])
        && result_attrs.is_empty()
        && args.len() == 2
        && args
            .iter()
            .all(|a| a.attrs.is_empty() && width(&a.ty) == Some(intrinsic.bits) && atom(&a.value))
}
fn plain_zero_call(op: &Op, function: &Function) -> bool {
    matches!(op, Op::Call { result, result_attrs, args, vararg: None, .. }
        if result_attrs.is_empty() && args.is_empty()
        && width(result)==width(&function.signature.result) && width(result).is_some())
}

// Keep the unchanged borrowed planning inputs explicit; this helper owns no model state.
#[allow(clippy::too_many_arguments)]
fn classify<'a>(
    item: usize,
    function: &'a Function,
    virtuals: &HashMap<Coordinate, Scalar>,
    names: &BTreeMap<&str, usize>,
    functions: &[(usize, &'a Function)],
    intrinsics: &Intrinsics<'_>,
    trap: bool,
    work: &mut Work,
) -> Option<Candidate<'a>> {
    let blocks = function.blocks.as_ref()?;
    if blocks.is_empty() {
        return None;
    }
    work.take(blocks.len().checked_mul(4)?)?;
    let labels = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    if labels.len() != blocks.len() {
        return None;
    }
    let mut reachable = vec![false; blocks.len()];
    let mut edges = vec![Vec::new(); blocks.len()];
    let mut pending = vec![0];
    let mut dependencies = BTreeSet::new();
    while let Some(block) = pending.pop() {
        work.take(1)?;
        if reachable[block] {
            continue;
        }
        reachable[block] = true;
        for (instruction, inst) in blocks[block].instructions.iter().enumerate() {
            work.take(1)?;
            if virtuals.contains_key(&(item, block, instruction)) {
                inst.result.as_ref()?;
                continue;
            }
            let terminating = matches!(
                inst.op,
                Op::Return(_) | Op::Unreachable | Op::Branch(_) | Op::CondBranch { .. }
            );
            if terminating && instruction + 1 != blocks[block].instructions.len() {
                return None;
            }
            match &inst.op {
                Op::Binary {
                    opcode,
                    ty,
                    left,
                    right,
                } if width(ty).is_some()
                    && atom(left)
                    && atom(right)
                    && matches!(
                        opcode.as_str(),
                        "add" | "sub" | "mul" | "and" | "or" | "xor"
                    ) => {}
                Op::Compare {
                    predicate,
                    ty,
                    left,
                    right,
                } if width(ty).is_some()
                    && atom(left)
                    && atom(right)
                    && matches!(
                        predicate.as_str(),
                        "eq" | "ne" | "ult" | "ule" | "ugt" | "uge" | "slt" | "sle" | "sgt" | "sge"
                    ) => {}
                Op::Cast { opcode, arg, to }
                    if scalar_arg(arg)
                        && width(to).is_some()
                        && matches!(opcode.as_str(), "trunc" | "zext" | "sext") => {}
                Op::Freeze(arg) if scalar_arg(arg) => {}
                Op::Extract { arg, index }
                    if *index <= 1
                        && arg.attrs.is_empty()
                        && matches!(&arg.value, Val::Local(_))
                        && matches!(&arg.ty,Ty::Struct(fields) if fields.len()==2 && width(&fields[0]).is_some() && fields[1]==Ty::Int(1)) =>
                    {}
                op if overflow_call(op, intrinsics) => {}
                Op::Call {
                    result: Ty::Void,
                    result_attrs,
                    callee: Val::Global(name),
                    args,
                    vararg: None,
                } if trap && name == "llvm.trap" && result_attrs.is_empty() && args.is_empty() => {}
                op @ Op::Call {
                    callee: Val::Global(name),
                    ..
                } => {
                    let &id = names.get(name.as_str())?;
                    if !plain_zero_call(op, functions[id].1) {
                        return None;
                    }
                    work.take(1)?;
                    dependencies.insert(id);
                }
                Op::Branch(label) => {
                    work.take(1)?;
                    edges[block].push(*labels.get(label.as_str())?);
                }
                Op::CondBranch { condition, yes, no } if atom(condition) => {
                    work.take(2)?;
                    edges[block].push(*labels.get(yes.as_str())?);
                    edges[block].push(*labels.get(no.as_str())?);
                }
                Op::Return(Some(arg))
                    if scalar_arg(arg) && width(&arg.ty) == width(&function.signature.result) => {}
                Op::Unreachable => {}
                _ => return None,
            }
        }
        work.take(edges[block].len())?;
        pending.extend(edges[block].iter().copied());
    }
    let mut incoming = vec![0usize; blocks.len()];
    for (id, successors) in edges.iter().enumerate() {
        work.take(1)?;
        if reachable[id] {
            for &to in successors {
                work.take(1)?;
                incoming[to] += 1;
            }
        }
    }
    let mut ready = (0..blocks.len())
        .filter(|&i| reachable[i] && incoming[i] == 0)
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        work.take(1)?;
        visited += 1;
        for &to in &edges[id] {
            work.take(1)?;
            incoming[to] -= 1;
            if incoming[to] == 0 {
                ready.push_back(to);
            }
        }
    }
    if visited != reachable.iter().filter(|&&r| r).count() {
        return None;
    }
    Some(Candidate {
        item,
        function,
        labels,
        dependencies,
    })
}

fn evaluate_summary(
    candidate: &Candidate<'_>,
    virtuals: &HashMap<Coordinate, Scalar>,
    names: &BTreeMap<&str, usize>,
    summaries: &BTreeMap<usize, Scalar>,
    functions: &[(usize, &Function)],
    intrinsics: &Intrinsics<'_>,
    work: &mut Work,
) -> Option<Scalar> {
    let blocks = candidate.function.blocks.as_ref()?;
    let mut values = HashMap::new();
    let mut block = 0;
    loop {
        work.take(1)?;
        let mut next = None;
        for (id, inst) in blocks[block].instructions.iter().enumerate() {
            work.take(1)?;
            if let Some(&value) = virtuals.get(&(candidate.item, block, id)) {
                values.insert(inst.result.as_deref()?, Known::Scalar(value));
                continue;
            }
            let known = match &inst.op {
                Op::Branch(label) => {
                    next = Some(*candidate.labels.get(label.as_str())?);
                    continue;
                }
                Op::CondBranch { condition, yes, no } => {
                    let condition = scalar(condition, &Ty::Int(1), &values)?;
                    next = Some(*candidate.labels.get(if condition.raw != 0 {
                        yes.as_str()
                    } else {
                        no.as_str()
                    })?);
                    continue;
                }
                Op::Return(Some(arg)) => {
                    let value = scalar(&arg.value, &arg.ty, &values)?;
                    return (Some(value.bits) == width(&candidate.function.signature.result))
                        .then_some(value);
                }
                op @ Op::Call {
                    callee: Val::Global(name),
                    ..
                } if names.contains_key(name.as_str()) => {
                    let id = names[name.as_str()];
                    if !plain_zero_call(op, functions[id].1) {
                        return None;
                    }
                    Known::Scalar(*summaries.get(&id)?)
                }
                op => known_operation(op, &values, intrinsics)?,
            };
            if matches!(known, Known::Overflow(_, true)) {
                return None;
            }
            values.insert(inst.result.as_deref()?, known);
        }
        block = next?;
    }
}

fn already_constant(function: &Function, value: Scalar) -> bool {
    let Some(blocks) = &function.blocks else {
        return false;
    };
    if blocks.len() != 1 || blocks[0].instructions.len() != 1 {
        return false;
    }
    let Op::Return(Some(arg)) = &blocks[0].instructions[0].op else {
        return false;
    };
    arg.attrs.is_empty() && scalar(&arg.value, &arg.ty, &HashMap::new()) == Some(value)
}

fn bounded_plan(
    module: &Module,
    sources: &BTreeSet<String>,
    replacements: &[Replacement],
    work: &mut Work,
) -> Option<Plan> {
    let mut functions = Vec::new();
    for (item, value) in module.items.iter().enumerate() {
        work.take(1)?;
        let Item::Function(function) = value else {
            continue;
        };
        if function.signature.params.is_empty()
            && function.blocks.is_some()
            && sources.contains(&function.name)
            && scalar_signature(function)
        {
            work.take(1)?;
            functions.push((item, function));
        }
    }
    if functions.is_empty() {
        return Some(Plan::default());
    }
    work.take(functions.len().checked_mul(4)?)?;
    let names = functions
        .iter()
        .enumerate()
        .map(|(i, (_, f))| (f.name.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    if names.len() != functions.len() {
        return Some(Plan::default());
    }
    work.take(replacements.len())?;
    let virtuals = replacements
        .iter()
        .map(|&(i, b, n, v)| ((i, b, n), v))
        .collect::<HashMap<_, _>>();
    work.take(module.items.len())?;
    let intrinsics = overflow_declarations(module);
    let trap = module.items.iter().any(|item| {
        matches!(item,Item::Function(f)
        if f.name=="llvm.trap" && f.blocks.is_none() && f.attrs.is_empty()
        && !f.signature.variadic && f.signature.params.is_empty()
        && f.signature.result_attrs.is_empty() && f.signature.result==Ty::Void)
    });
    let mut candidates = Vec::with_capacity(functions.len());
    for &(item, f) in &functions {
        if work.0 == 0 {
            return None;
        }
        let candidate = classify(
            item,
            f,
            &virtuals,
            &names,
            &functions,
            &intrinsics,
            trap,
            work,
        );
        if work.0 == 0 {
            return None;
        }
        candidates.push(candidate);
    }
    let mut incoming = vec![0usize; functions.len()];
    let mut users = vec![Vec::new(); functions.len()];
    let mut ready = BTreeSet::new();
    for (id, candidate) in candidates.iter().enumerate() {
        work.take(1)?;
        let Some(candidate) = candidate else { continue };
        incoming[id] = candidate.dependencies.len();
        for &dep in &candidate.dependencies {
            work.take(1)?;
            users[dep].push(id);
        }
        if incoming[id] == 0 {
            ready.insert(id);
        }
    }
    let mut summaries = BTreeMap::new();
    while let Some(id) = ready.pop_first() {
        work.take(1)?;
        let candidate = candidates[id].as_ref()?;
        if let Some(value) = evaluate_summary(
            candidate,
            &virtuals,
            &names,
            &summaries,
            &functions,
            &intrinsics,
            work,
        ) {
            work.take(1)?;
            summaries.insert(id, value);
            for &user in &users[id] {
                work.take(1)?;
                incoming[user] -= 1;
                if incoming[user] == 0 {
                    ready.insert(user);
                }
            }
        }
        if work.0 == 0 {
            return None;
        }
    }
    let mut plan = Plan::default();
    for (&id, &value) in &summaries {
        work.take(1)?;
        if !already_constant(functions[id].1, value) {
            plan.bodies.insert(functions[id].0, value);
        }
    }
    if summaries.is_empty() {
        return Some(plan);
    }
    for (item, value) in module.items.iter().enumerate() {
        work.take(1)?;
        let Item::Function(function) = value else {
            continue;
        };
        // Bodies are replaced last. No old call coordinate inside them is needed.
        if plan.bodies.contains_key(&item) {
            continue;
        }
        for (block, b) in function.blocks.iter().flatten().enumerate() {
            work.take(1)?;
            for (instruction, inst) in b.instructions.iter().enumerate() {
                work.take(1)?;
                if inst.result.is_none() || virtuals.contains_key(&(item, block, instruction)) {
                    continue;
                }
                let Op::Call {
                    callee: Val::Global(name),
                    ..
                } = &inst.op
                else {
                    continue;
                };
                let Some(&id) = names.get(name.as_str()) else {
                    continue;
                };
                let Some(&value) = summaries.get(&id) else {
                    continue;
                };
                if plain_zero_call(&inst.op, functions[id].1) {
                    work.take(1)?;
                    plan.calls.push((item, block, instruction, value));
                }
            }
        }
    }
    Some(plan)
}

pub(super) fn plan(
    module: &Module,
    sources: &BTreeSet<String>,
    replacements: &[Replacement],
) -> Plan {
    // Additional computation is opt-in only when the unchanged leaf planner
    // already found a real edit. Preserve the old no-fold path without scans.
    if replacements.is_empty() {
        return Plan::default();
    }
    bounded_plan(module, sources, replacements, &mut Work(MAX_SUMMARY_WORK)).unwrap_or_default()
}

impl Plan {
    pub(super) fn count(&self) -> usize {
        self.bodies.len() + self.calls.len()
    }
    pub(super) fn apply(self, module: &mut Module) {
        for (item, block, instruction, value) in self.calls {
            let Item::Function(function) = &mut module.items[item] else {
                unreachable!()
            };
            function.blocks.as_mut().expect("preflighted original")[block].instructions
                [instruction]
                .op = Op::Freeze(Arg::new(Ty::Int(value.bits), value.value()));
        }
        for (item, value) in self.bodies {
            let Item::Function(function) = &mut module.items[item] else {
                unreachable!()
            };
            let name = function.blocks.as_ref().expect("preflighted original")[0]
                .name
                .clone();
            function.blocks = Some(vec![Block {
                name,
                instructions: vec![Inst::new(
                    None,
                    Op::Return(Some(Arg::new(Ty::Int(value.bits), value.value()))),
                )],
            }]);
        }
    }
}

#[cfg(test)]
#[path = "machine_scalar_summary_tests.rs"]
mod tests;
