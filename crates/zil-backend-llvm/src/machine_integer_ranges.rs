//! Current-model integer ranges, independent of source verification. Universal
//! return bounds never assume argument bounds or that a callee returns normally.
//! Only an already-false overflow flag changes; calls/results/effects stay put.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use zil_llvm_api::model::{Attr, Function, Item, Linkage, Module, Op, Ty, Val};

const MAX_WORK: usize = 2_000_000;
const MAX_ROUNDS: usize = 4;
mod compact;
mod plan;
mod purity;
pub(crate) use purity::mark as mark_purity;
const MIN: i128 = i32::MIN as i128;
const MAX: i128 = i32::MAX as i128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    lo: i128,
    hi: i128,
}
impl Range {
    const TOP: Self = Self { lo: MIN, hi: MAX };
    fn hull(self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
    fn intersect(self, other: Self) -> Option<Self> {
        let r = Self {
            lo: self.lo.max(other.lo),
            hi: self.hi.min(other.hi),
        };
        (r.lo <= r.hi).then_some(r)
    }
    fn fits(self) -> bool {
        MIN <= self.lo && self.hi <= MAX
    }
}
fn constant(n: i128) -> Range {
    let n = i128::from(n as i32);
    Range { lo: n, hi: n }
}
type Values<'a> = HashMap<&'a str, Range>;
fn value(v: &Val, values: &Values<'_>) -> Range {
    match v {
        Val::Int(n) if MIN <= *n && *n <= u32::MAX as i128 => constant(*n),
        Val::Zero => constant(0),
        Val::Local(n) => values.get(n.as_str()).copied().unwrap_or(Range::TOP),
        _ => Range::TOP,
    }
}
#[derive(Clone, Copy)]
enum Arithmetic {
    Add,
    Sub,
    Mul,
}
fn math(op: Arithmetic, a: Range, b: Range) -> Range {
    match op {
        Arithmetic::Add => Range {
            lo: a.lo + b.lo,
            hi: a.hi + b.hi,
        },
        Arithmetic::Sub => Range {
            lo: a.lo - b.hi,
            hi: a.hi - b.lo,
        },
        Arithmetic::Mul => {
            let xs = [a.lo * b.lo, a.lo * b.hi, a.hi * b.lo, a.hi * b.hi];
            Range {
                lo: *xs.iter().min().unwrap(),
                hi: *xs.iter().max().unwrap(),
            }
        }
    }
}
fn arithmetic(name: &str) -> Option<Arithmetic> {
    match name {
        "add" => Some(Arithmetic::Add),
        "sub" => Some(Arithmetic::Sub),
        "mul" => Some(Arithmetic::Mul),
        _ => None,
    }
}
fn intrinsic(f: &Function) -> Option<Arithmetic> {
    if f.blocks.is_some()
        || f.linkage != Linkage::External
        || !f.attrs.is_empty()
        || f.signature.variadic
        || !f.signature.result_attrs.is_empty()
        || f.signature.result != Ty::Struct(vec![Ty::Int(32), Ty::Int(1)])
        || f.signature.params.len() != 2
        || f.signature
            .params
            .iter()
            .any(|p| p.ty != Ty::Int(32) || !p.attrs.is_empty())
    {
        return None;
    }
    match f.name.as_str() {
        "llvm.sadd.with.overflow.i32" => Some(Arithmetic::Add),
        "llvm.ssub.with.overflow.i32" => Some(Arithmetic::Sub),
        "llvm.smul.with.overflow.i32" => Some(Arithmetic::Mul),
        _ => None,
    }
}
struct World<'a> {
    intrinsics: BTreeMap<&'a str, Arithmetic>,
    returns: BTreeMap<&'a str, Range>,
    arguments: BTreeMap<&'a str, Vec<Range>>,
}
fn overflow(op: &Op, world: &World<'_>, values: &Values<'_>) -> Option<Range> {
    let Op::Call {
        result,
        result_attrs,
        callee: Val::Global(name),
        args,
        vararg,
    } = op
    else {
        return None;
    };
    if !result_attrs.is_empty()
        || vararg.is_some()
        || args.len() != 2
        || *result != Ty::Struct(vec![Ty::Int(32), Ty::Int(1)])
        || args
            .iter()
            .any(|a| a.ty != Ty::Int(32) || !a.attrs.is_empty())
    {
        return None;
    }
    let op = *world.intrinsics.get(name.as_str())?;
    Some(math(
        op,
        value(&args[0].value, values),
        value(&args[1].value, values),
    ))
}
fn operation(op: &Op, pairs: &HashMap<&str, &Op>, world: &World<'_>, values: &Values<'_>) -> Range {
    match op {
        Op::Binary {
            opcode,
            ty: Ty::Int(32),
            left,
            right,
        } => {
            let a = value(left, values);
            let b = value(right, values);
            if let Some(op) = arithmetic(opcode) {
                let r = math(op, a, b);
                if r.fits() {
                    r
                } else if a.lo == a.hi && b.lo == b.hi {
                    constant(r.lo)
                } else {
                    Range::TOP
                }
            } else if opcode == "srem" && b.lo == b.hi && b.lo > 0 {
                Range {
                    lo: if a.lo >= 0 {
                        0
                    } else {
                        (-(b.lo - 1)).max(a.lo)
                    },
                    hi: if a.hi <= 0 { 0 } else { (b.lo - 1).min(a.hi) },
                }
            } else {
                Range::TOP
            }
        }
        Op::Phi {
            ty: Ty::Int(32),
            incoming,
        } => incoming
            .iter()
            .map(|(v, _)| value(v, values))
            .reduce(Range::hull)
            .unwrap_or(Range::TOP),
        Op::Select {
            ty: Ty::Int(32),
            yes,
            no,
            ..
        } => value(yes, values).hull(value(no, values)),
        Op::Extract { arg, index: 0 } if arg.ty == Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]) => {
            let Val::Local(name) = &arg.value else {
                return Range::TOP;
            };
            pairs
                .get(name.as_str())
                .and_then(|op| overflow(op, world, values))
                .filter(|r| r.fits())
                .unwrap_or(Range::TOP)
        }
        Op::Call {
            result: Ty::Int(32),
            callee: Val::Global(name),
            ..
        } => world
            .returns
            .get(name.as_str())
            .copied()
            .unwrap_or(Range::TOP),
        // In particular, freeze(poison/undef) can choose an arbitrary value.
        _ => Range::TOP,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Facts {
    returned: Range,
    flags: Vec<(usize, usize)>,
    calls: Vec<(String, Vec<Range>)>,
}
#[cfg(test)]
std::thread_local! {
    static FACT_EXECUTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
// Keep the original complete walker as an independent test oracle.
#[cfg(test)]
fn facts(f: &Function, world: &World<'_>, work: &mut usize, inputs: &[Range]) -> Option<Facts> {
    #[cfg(test)]
    FACT_EXECUTIONS.with(|n| n.set(n.get() + 1));
    let blocks = f.blocks.as_ref()?;
    let count = blocks.iter().try_fold(f.signature.params.len(), |n, b| {
        n.checked_add(b.instructions.len())
    })?;
    *work = work.checked_sub(count)?;
    let pairs: HashMap<_, _> = blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| i.result.as_deref().map(|n| (n, &i.op)))
        .collect();
    let mut values = Values::new();
    for (parameter, range) in f.signature.params.iter().zip(inputs) {
        if parameter.ty == Ty::Int(32) {
            values.insert(parameter.name.as_deref()?, *range);
        }
    }
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for inst in blocks.iter().flat_map(|b| &b.instructions) {
            *work = work.checked_sub(1)?;
            if let Some(name) = inst.result.as_deref() {
                let old = values.get(name).copied().unwrap_or(Range::TOP);
                let new = old.intersect(operation(&inst.op, &pairs, world, &values))?;
                if new != old {
                    values.insert(name, new);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut returned = None;
    let mut flags = Vec::new();
    let mut calls = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        for (ii, i) in b.instructions.iter().enumerate() {
            *work = work.checked_sub(1)?;
            if let Op::Return(Some(arg)) = &i.op {
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
            } = &i.op
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
            if let Op::Extract { arg, index: 1 } = &i.op
                && arg.ty == Ty::Struct(vec![Ty::Int(32), Ty::Int(1)])
                && let Val::Local(name) = &arg.value
                && let Some(op) = pairs.get(name.as_str())
                && overflow(op, world, &values).is_some_and(Range::fits)
            {
                flags.push((bi, ii));
            }
        }
    }
    Some(Facts {
        returned: returned.unwrap_or(Range::TOP),
        flags,
        calls,
    })
}
fn eligible(f: &Function, protected: &BTreeSet<String>) -> bool {
    f.blocks.is_some()
        && f.signature.result == Ty::Int(32)
        && f.linkage != Linkage::AvailableExternally
        && !f.attrs.contains(&Attr::NoInline)
        && !protected.contains(&f.name)
}
/// Two independent passes: summaries first treat EVERY call result as top.
/// Thus even recursive calls cannot bootstrap an optimistic return bound.
/// Plans use only these universal facts from this same complete current model.
/// Recomputing the transform after any current callee change binds all readers.
pub(crate) fn fold(
    module: &mut Module,
    protected: &BTreeSet<String>,
    preflight: &mut crate::machine_preflight::CurrentPreflight,
) -> usize {
    fold_with_work(module, protected, preflight, MAX_WORK, true)
}

fn fold_with_work(
    module: &mut Module,
    protected: &BTreeSet<String>,
    preflight: &mut crate::machine_preflight::CurrentPreflight,
    mut work: usize,
    reuse_rounds: bool,
) -> usize {
    if preflight.check(module).is_err() {
        return 0;
    }
    let mut body_plans = plan::Plans::default();
    #[cfg(test)]
    {
        body_plans.reference = !reuse_rounds;
    }
    let mut excluded = protected.clone();
    let mut address_owners = HashMap::new();
    for item in &module.items {
        match item {
            Item::Global(g) => {
                if let Some(v) = &g.initializer {
                    v.symbols(&mut excluded);
                }
            }
            Item::Function(f) => {
                if let Some(blocks) = &f.blocks {
                    let key = blocks.as_ptr() as usize;
                    if let std::collections::hash_map::Entry::Vacant(e) = address_owners.entry(key)
                    {
                        let Some(cost) = blocks
                            .iter()
                            .try_fold(0usize, |n, b| n.checked_add(b.instructions.len() + 1))
                        else {
                            return 0;
                        };
                        let Some(left) = work.checked_sub(cost) else {
                            return 0;
                        };
                        work = left;
                        let Some(addresses) =
                            crate::machine_call_classes::function_addresses(f, &|| false)
                        else {
                            return 0;
                        };
                        excluded.extend(addresses);
                        e.insert(());
                    }
                }
            }
            _ => {}
        }
    }
    let mut world = World {
        intrinsics: module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(f) => intrinsic(f).map(|op| (f.name.as_str(), op)),
                _ => None,
            })
            .collect(),
        returns: BTreeMap::new(),
        arguments: BTreeMap::new(),
    };
    // Body-owner sharing is current-request only; exact signatures bind each use.
    type Cache<'a> = HashMap<usize, Vec<(&'a Function, Facts)>>;
    let mut cache = Cache::new();
    let mut summaries = Vec::new();
    for item in &module.items {
        if let Item::Function(f) = item
            && eligible(f, &excluded)
        {
            let blocks = f.blocks.as_ref().unwrap();
            let key = blocks.as_ptr() as usize;
            let cached = cache
                .get(&key)
                .and_then(|xs| {
                    xs.iter().find(|(old, _)| {
                        old.signature == f.signature && old.blocks.as_ref().unwrap().ptr_eq(blocks)
                    })
                })
                .map(|(_, r)| r.clone());
            let r = if let Some(fs) = cached {
                fs.returned
            } else {
                let Some(fs) = body_plans.facts(f, &world, &mut work, &[]) else {
                    return 0;
                };
                let r = fs.returned;
                cache.entry(key).or_default().push((f, fs));
                r
            };
            summaries.push((f.name.as_str(), r));
        }
    }
    world.returns.extend(summaries);
    // Only non-exported, non-address-taken generated definitions receive
    // argument assumptions. All current body callers contribute, including
    // protected entries and void/non-scalar callers. Missing incoming calls
    // remain Top. Every iteration descends from a sound whole-program bound;
    // recursive unknown callers therefore cannot bootstrap narrow arguments.
    for item in &module.items {
        if let Item::Function(f) = item
            && eligible(f, &excluded)
            && f.name.starts_with("zil.fn.")
            && !f.signature.variadic
            && f.signature.params.iter().all(|p| p.ty == Ty::Int(32))
        {
            world
                .arguments
                .insert(&f.name, vec![Range::TOP; f.signature.params.len()]);
        }
    }
    // Returns, intrinsic declarations and the argument domain/arity are fixed
    // from here through final planning. Only each function's input bounds move;
    // facts() reads no other argument values. Keep results across rounds keyed
    // by the exact body owner, signature and those bounds. A new round still
    // pays the original computation fee once per input, including the final
    // planning round; sharing never changes MAX_WORK's refusal boundary.
    struct InputFacts<'a> {
        function: &'a Function,
        inputs: Vec<Range>,
        facts: Arc<Facts>,
        cost: usize,
        round: usize,
    }
    type InputCache<'a> = HashMap<usize, Vec<InputFacts<'a>>>;
    fn cached_facts<'a>(
        f: &'a Function,
        world: &World<'_>,
        work: &mut usize,
        cache: &mut InputCache<'a>,
        round: usize,
        body_plans: &mut plan::Plans<'a>,
    ) -> Option<Arc<Facts>> {
        let blocks = f.blocks.as_ref()?;
        let inputs = world
            .arguments
            .get(f.name.as_str())
            .map_or(&[][..], Vec::as_slice);
        let key = std::ptr::from_ref(&**blocks) as usize;
        if let Some(entry) = cache.get_mut(&key).and_then(|xs| {
            xs.iter_mut().find(|entry| {
                entry.function.signature == f.signature
                    && entry.inputs == inputs
                    && entry.function.blocks.as_ref().unwrap().ptr_eq(blocks)
            })
        }) {
            if entry.round != round {
                *work = work.checked_sub(entry.cost)?;
                entry.round = round;
            }
            return Some(entry.facts.clone());
        }
        let before = *work;
        let fs = Arc::new(body_plans.facts(f, world, work, inputs)?);
        cache.entry(key).or_default().push(InputFacts {
            function: f,
            inputs: inputs.to_vec(),
            facts: fs.clone(),
            cost: before - *work,
            round,
        });
        Some(fs)
    }
    // At most MAX_ROUNDS + 1 versions of the already bounded original
    // per-round cache. It owns facts, never another World or an older request.
    let mut cache = InputCache::new();
    for round in 0..MAX_ROUNDS {
        if !reuse_rounds {
            cache.clear();
        }
        let mut incoming: BTreeMap<String, Vec<Range>> = BTreeMap::new();
        let mut contributed = BTreeSet::new();
        for item in &module.items {
            if let Item::Function(f) = item
                && f.blocks.is_some()
            {
                let Some(fs) =
                    cached_facts(f, &world, &mut work, &mut cache, round, &mut body_plans)
                else {
                    return 0;
                };
                // Repeated identical call contributions have the same hull.
                // Every function above still pays its original round fee.
                if !contributed.insert(Arc::as_ptr(&fs)) {
                    continue;
                }
                for (name, ranges) in &fs.calls {
                    match incoming.entry(name.clone()) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(ranges.clone());
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            for (a, b) in entry.get_mut().iter_mut().zip(ranges) {
                                *a = a.hull(*b);
                            }
                        }
                    }
                }
            }
        }
        let mut changed = false;
        for (name, ranges) in incoming {
            let Some(old) = world.arguments.get_mut(name.as_str()) else {
                return 0;
            };
            if old.len() != ranges.len() {
                return 0;
            }
            for (a, b) in old.iter_mut().zip(ranges) {
                let Some(new) = a.intersect(b) else {
                    return 0;
                };
                changed |= new != *a;
                *a = new;
            }
        }
        if !changed {
            break;
        }
    }
    let mut plans = Vec::new();
    if !reuse_rounds {
        cache.clear();
    }
    for (index, item) in module.items.iter().enumerate() {
        // Protected symbols retain their ABI and unknown entry arguments;
        // universally safe arithmetic inside their bodies can still simplify.
        if let Item::Function(f) = item
            && f.blocks.is_some()
            && f.linkage != Linkage::AvailableExternally
            && !f.attrs.contains(&Attr::NoInline)
        {
            let Some(fs) = cached_facts(
                f,
                &world,
                &mut work,
                &mut cache,
                MAX_ROUNDS,
                &mut body_plans,
            ) else {
                return 0;
            };
            if !fs.flags.is_empty() {
                let Some(cost) = f
                    .blocks
                    .as_ref()
                    .unwrap()
                    .iter()
                    .try_fold(0usize, |n, b| n.checked_add(b.instructions.len() + 1))
                else {
                    return 0;
                };
                let Some(left) = work.checked_sub(cost) else {
                    return 0;
                };
                work = left;
                plans.push((index, fs.flags.clone()));
            }
        }
    }
    if plans.is_empty() || preflight.check(module).is_err() {
        return 0;
    }
    let intrinsics = world
        .intrinsics
        .iter()
        .map(|(name, op)| ((*name).to_owned(), *op))
        .collect();
    let mut changed = 0;
    // Rewriting one actual body never destroys sharing for its current peers.
    type Rewritten = HashMap<
        usize,
        Vec<(
            Function,
            Vec<(usize, usize)>,
            zil_llvm_api::model::SharedVec<zil_llvm_api::model::Block>,
        )>,
    >;
    let mut rewritten = Rewritten::new();
    for (index, flags) in plans {
        let Item::Function(f) = &mut module.items[index] else {
            unreachable!()
        };
        let key = f.blocks.as_ref().unwrap().as_ptr() as usize;
        let prior = rewritten
            .get(&key)
            .and_then(|xs| {
                xs.iter().find(|(old, old_flags, _)| {
                    old.signature == f.signature
                        && old.attrs == f.attrs
                        && old.linkage == f.linkage
                        && old_flags == &flags
                        && old
                            .blocks
                            .as_ref()
                            .unwrap()
                            .ptr_eq(f.blocks.as_ref().unwrap())
                })
            })
            .map(|(_, _, blocks)| blocks.clone());
        if let Some(blocks) = prior {
            f.blocks = Some(blocks);
            changed += flags.len();
        } else {
            let original = f.clone();
            changed += compact::apply(f, &flags, &intrinsics);
            rewritten.entry(key).or_default().push((
                original,
                flags,
                f.blocks.as_ref().unwrap().clone(),
            ));
        }
    }
    changed
}

#[cfg(test)]
mod tests;
