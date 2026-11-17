//! Exact direct-call classes for the explicitly closed executable strategy.
//!
//! The caller must preflight the COMPLETE original module before calling this
//! pass, supply every entry/export name, and retain the ordinary later checks.
//! This is not an interposition-safe optimization for open modules. Names only
//! restrict the candidate domain; complete Function equality authorizes a class.
//! Definitions, linkage, attributes, local names and all address uses survive.
mod recovery;
use std::collections::{BTreeSet, HashMap};
use zil_llvm_api::model::{Arg, Block, Function, Item, Linkage, Module, Op, SharedVec, Ty, Val};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    pub(crate) eligible_functions: usize,
    pub(crate) equivalence_classes: usize,
    pub(crate) rewritten_calls: usize,
}

/// One actual current class-analysis epoch, retained only with its existing
/// bounded TransformProduct. Inputs and observed callee classes are complete
/// values; selectors never authorize reuse on their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cache {
    triple: String,
    context: Vec<(Function, bool)>,
    functions: Vec<std::sync::Arc<FunctionReads>>,
    /// Reuse from either the previous epoch or another current function.
    pub(crate) reused_inputs: usize,
    pub(crate) rebuilt_inputs: usize,
    pub(crate) reused_keys: usize,
    pub(crate) rebuilt_keys: usize,
    /// Subsets of reuse performed within this request, including a cold restart.
    pub(crate) shared_inputs: usize,
    pub(crate) shared_keys: usize,
    pub(crate) body_owners: usize,
    pub(crate) address_scans: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct FunctionReads {
    input: Function,
    addresses: std::sync::Arc<BTreeSet<String>>,
    dependencies: std::sync::Arc<Option<BTreeSet<usize>>>,
    cost: usize,
    normalized: Option<std::sync::Arc<Normalized>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Normalized {
    callees: Vec<(usize, String)>,
    function: Function,
    selector: zil_base::Fingerprint,
}

// Only the definition's own symbol is omitted, exactly as in normalized_key.
// Its candidate role and address exclusions are checked separately below.
fn same_class_header(left: &Function, right: &Function) -> bool {
    left.linkage == right.linkage && left.signature == right.signature && left.attrs == right.attrs
}

fn body_owner(function: &Function) -> Option<*const Vec<Block>> {
    // Use the Arc-owned Vec, not its slice pointer (empty slices can alias).
    // This only selects a bucket; the borrowed current module keeps all owners
    // alive and every hit also checks SharedVec::ptr_eq. Never retain this map.
    function
        .blocks
        .as_ref()
        .map(|blocks| std::ptr::from_ref(&**blocks))
}

fn same_body_owner(left: &Function, right: &Function) -> bool {
    matches!((&left.blocks, &right.blocks), (Some(left), Some(right)) if left.ptr_eq(right))
}

pub(super) fn function_addresses(
    function: &Function,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<BTreeSet<String>> {
    let mut addresses = BTreeSet::new();
    for block in function.blocks.iter().flatten() {
        for inst in &block.instructions {
            match &inst.op {
                Op::Call {
                    callee: Val::Global(_),
                    args,
                    ..
                } => {
                    for arg in args {
                        address_uses(&arg.value, &mut addresses, cancelled)?;
                    }
                }
                _ => {
                    for value in inst.op.values() {
                        address_uses(value, &mut addresses, cancelled)?;
                    }
                }
            }
        }
    }
    Some(addresses)
}

fn integer(ty: &Ty) -> bool {
    matches!(ty, Ty::Int(1 | 8 | 16 | 32 | 64))
}

fn scalar(value: &Val) -> bool {
    matches!(
        value,
        Val::Local(_) | Val::Int(_) | Val::Bool(_) | Val::Zero
    )
}

fn integer_arg(arg: &Arg) -> bool {
    integer(&arg.ty) && scalar(&arg.value)
}

fn overflow_pair(ty: &Ty) -> bool {
    matches!(ty, Ty::Struct(fields) if fields.len() == 2
        && matches!(fields[0], Ty::Int(8 | 16 | 32 | 64)) && fields[1] == Ty::Int(1))
}

fn generated(name: &str) -> bool {
    name.strip_prefix("zil.fn.")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

fn intrinsic(function: &Function) -> bool {
    let sig = &function.signature;
    if function.blocks.is_some()
        || function.linkage != Linkage::External
        || !function.attrs.is_empty()
        || sig.variadic
        || !sig.result_attrs.is_empty()
        || sig.params.iter().any(|p| !p.attrs.is_empty())
    {
        return false;
    }
    if function.name == "llvm.trap" {
        return sig.result == Ty::Void && sig.params.is_empty();
    }
    if !overflow_pair(&sig.result) || sig.params.len() != 2 {
        return false;
    }
    let Ty::Struct(fields) = &sig.result else {
        return false;
    };
    let Ty::Int(bits) = fields[0] else {
        return false;
    };
    sig.params.iter().all(|p| p.ty == Ty::Int(bits))
        && ["sadd", "ssub", "smul", "uadd", "usub", "umul"]
            .iter()
            .any(|op| function.name == format!("llvm.{op}.with.overflow.i{bits}"))
}

// Walk addresses in unsupported operations too. In particular, a function
// passed as an argument to a direct call is an address use, not a call edge.
// Iteration avoids depending on the nesting depth of constant aggregates.
fn address_uses(
    value: &Val,
    addresses: &mut BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<()> {
    let mut pending = Vec::new();
    let mut first = Some(value);
    while let Some(value) = first.take().or_else(|| pending.pop()) {
        if cancelled() {
            return None;
        }
        match value {
            Val::Global(name) => {
                addresses.insert(name.clone());
            }
            Val::Array(args) => pending.extend(args.iter().map(|arg| &arg.value)),
            Val::Gep { ptr, indices, .. } => {
                pending.push(ptr);
                pending.extend(indices.iter().map(|arg| &arg.value));
            }
            _ => {}
        }
    }
    Some(())
}

fn call_abi(result: &Ty, attrs: &[zil_llvm_api::model::Attr], args: &[Arg], f: &Function) -> bool {
    !f.signature.variadic
        && *result == f.signature.result
        && attrs == f.signature.result_attrs
        && args.len() == f.signature.params.len()
        && args
            .iter()
            .zip(&f.signature.params)
            .all(|(a, p)| a.ty == p.ty && a.attrs == p.attrs && integer_arg(a))
}

fn dependencies(
    function: &Function,
    functions: &[&Function],
    names: &HashMap<&str, usize>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<BTreeSet<usize>> {
    if !integer(&function.signature.result)
        || function.signature.variadic
        || function.signature.params.iter().any(|p| !integer(&p.ty))
        || function.linkage == Linkage::AvailableExternally
    {
        return None;
    }
    let mut dependencies = BTreeSet::new();
    for block in function.blocks.as_ref()? {
        for inst in &block.instructions {
            if cancelled() {
                return None;
            }
            let allowed = match &inst.op {
                Op::Binary {
                    opcode,
                    ty,
                    left,
                    right,
                } => {
                    integer(ty)
                        && scalar(left)
                        && scalar(right)
                        && matches!(
                            opcode.as_str(),
                            "add"
                                | "sub"
                                | "mul"
                                | "udiv"
                                | "sdiv"
                                | "urem"
                                | "srem"
                                | "shl"
                                | "lshr"
                                | "ashr"
                                | "and"
                                | "or"
                                | "xor"
                        )
                }
                Op::Compare {
                    predicate,
                    ty,
                    left,
                    right,
                } => {
                    integer(ty)
                        && scalar(left)
                        && scalar(right)
                        && matches!(
                            predicate.as_str(),
                            "eq" | "ne"
                                | "ugt"
                                | "uge"
                                | "ult"
                                | "ule"
                                | "sgt"
                                | "sge"
                                | "slt"
                                | "sle"
                        )
                }
                Op::Cast { opcode, arg, to } => {
                    matches!(opcode.as_str(), "trunc" | "zext" | "sext")
                        && integer_arg(arg)
                        && integer(to)
                }
                Op::Extract { arg, index } => {
                    overflow_pair(&arg.ty) && matches!(arg.value, Val::Local(_)) && *index < 2
                }
                Op::Call {
                    result,
                    result_attrs,
                    callee: Val::Global(name),
                    args,
                    vararg,
                } => {
                    let &callee = names.get(name.as_str())?;
                    let target = functions[callee];
                    if vararg.is_some() || !call_abi(result, result_attrs, args, target) {
                        return None;
                    }
                    if !intrinsic(target) {
                        if !integer(result) || target.blocks.is_none() {
                            return None;
                        }
                        dependencies.insert(callee);
                    }
                    true
                }
                Op::Phi { ty, incoming } => integer(ty) && incoming.iter().all(|(v, _)| scalar(v)),
                Op::Branch(_) | Op::Unreachable => true,
                Op::CondBranch { condition, .. } => scalar(condition),
                Op::Return(Some(arg)) => integer_arg(arg),
                Op::Select {
                    ty,
                    condition,
                    yes,
                    no,
                } => integer(ty) && scalar(condition) && scalar(yes) && scalar(no),
                // No memory, pointer/aggregate manipulation, freeze, foreign or
                // indirect calls, unknown intrinsic, undef or poison admission.
                _ => false,
            };
            if !allowed {
                return None;
            }
        }
    }
    Some(dependencies)
}

#[cfg(test)]
fn normalized_key(
    function: &Function,
    functions: &[&Function],
    names: &HashMap<&str, usize>,
    representatives: &[Option<usize>],
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Function> {
    normalized_key_inner(function, functions, names, representatives, None, cancelled)
}

fn normalized_key_inner(
    function: &Function,
    functions: &[&Function],
    names: &HashMap<&str, usize>,
    representatives: &[Option<usize>],
    stable_names: Option<&[Option<String>]>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Function> {
    let mut key = function.clone();
    key.name.clear();
    let mut stopped = false;
    let needs_rewrite = key.blocks.as_ref()?.iter().any(|block| {
        block.instructions.iter().any(|inst| {
            if cancelled() {
                stopped = true;
                return true;
            }
            if let Op::Call {
                callee: Val::Global(name),
                ..
            } = &inst.op
            {
                let Some(&target) = names.get(name.as_str()) else {
                    stopped = true;
                    return true;
                };
                return if let Some(stable) = stable_names {
                    stable[target].as_ref().is_some_and(|stable| stable != name)
                } else {
                    representatives[target].is_some_and(|rep| rep != target)
                };
            }
            false
        })
    });
    if stopped {
        return None;
    }
    // Clearing the header name does not need mutable block access. Keep leaf,
    // intrinsic-only and already-canonical bodies on their original SharedVec.
    if needs_rewrite {
        for block in key.blocks.as_mut()?.iter_mut() {
            for inst in &mut block.instructions {
                if cancelled() {
                    return None;
                }
                if let Op::Call {
                    callee: Val::Global(name),
                    ..
                } = &mut inst.op
                {
                    let target = *names.get(name.as_str())?;
                    if let Some(stable) = stable_names {
                        if let Some(stable) = &stable[target] {
                            *name = stable.clone();
                        }
                    } else if let Some(rep) = representatives[target]
                        && rep != target
                    {
                        *name = functions[rep].name.clone();
                    }
                }
            }
        }
    }
    Some(key)
}

/// Return None on cancellation/ambiguous names, without modifying the module.
/// Pure DAGs are processed bottom-up. Cycles and all their callers remain
/// unclassified; no recursive traversal or fixed-point speculation is used.
pub(crate) fn canonicalize(
    module: &mut Module,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Stats> {
    canonicalize_inner(module, protected, cancelled, false, None).map(|(stats, _, _)| stats)
}

/// Materialize a complete equal class under a content-derived private symbol.
/// The hash selects only its spelling: full normalized Function equality forms
/// classes, and a spelling collision rejects the entire staged optimization.
/// Original definitions and every address operand survive unchanged.
/// These actual new definitions, not a name-prefix heuristic, authorize imports.
#[cfg(test)]
pub(crate) fn canonicalize_stable(
    module: &mut Module,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<(Stats, BTreeSet<String>)> {
    canonicalize_stable_cached(module, protected, cancelled, None)
        .map(|(stats, symbols, _)| (stats, symbols))
}

pub(crate) fn canonicalize_stable_cached(
    module: &mut Module,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
    previous: Option<&Cache>,
) -> Option<(Stats, BTreeSet<String>, Cache)> {
    canonicalize_inner(module, protected, cancelled, true, previous)
}

fn canonicalize_inner(
    module: &mut Module,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
    stable: bool,
    previous: Option<&Cache>,
) -> Option<(Stats, BTreeSet<String>, Cache)> {
    if cancelled() {
        return None;
    }
    let mut functions = Vec::new();
    let mut names = HashMap::new();
    let mut addresses = protected.clone();
    let mut all_symbols = BTreeSet::new();
    if stable && module.items.len() > crate::machine_cgu::layout::MAX_ITEMS {
        return None;
    }
    for item in &module.items {
        if cancelled() {
            return None;
        }
        match item {
            Item::Global(global) => {
                all_symbols.insert(global.name.as_str());
                if let Some(value) = &global.initializer {
                    address_uses(value, &mut addresses, cancelled)?;
                }
            }
            Item::Function(function) => {
                all_symbols.insert(function.name.as_str());
                if names
                    .insert(function.name.as_str(), functions.len())
                    .is_some()
                {
                    return None;
                }
                functions.push(function);
            }
            Item::NamedType(..) => {}
        }
    }
    let count = functions.len();
    let context = functions
        .iter()
        .map(|function| {
            let defined = function.blocks.is_some();
            let mut header = (*function).clone();
            header.blocks = None;
            (header, defined)
        })
        .collect::<Vec<_>>();
    let compatible = stable
        && previous.is_some_and(|previous| {
            previous.triple == module.triple && previous.context == context
        });
    let mut cache = Cache {
        triple: module.triple.clone(),
        context,
        ..Cache::default()
    };
    // Request-local only: complete current callee headers/name resolution are
    // fixed for this entire computation. A transform that splits a SharedVec
    // creates another owner and misses, even when the contents happen to equal.
    let mut body_inputs: HashMap<*const Vec<Block>, Vec<usize>> = HashMap::new();
    // Both inventories retain strong immutable block owners for this entire
    // request. Cross-process codecs rebuild different Arcs; compare each owner
    // pair in full once, while every row still binds its COMPLETE own header.
    let mut equal_bodies: HashMap<(*const Vec<Block>, *const Vec<Block>), bool> = HashMap::new();
    let mut input_owners = Vec::with_capacity(count);
    for (index, function) in functions.iter().enumerate() {
        if cancelled() {
            return None;
        }
        let old = previous
            .and_then(|old| old.functions.get(index))
            .filter(|old| {
                if old.input.name != function.name || !same_class_header(&old.input, function) {
                    return false;
                }
                match (&old.input.blocks, &function.blocks) {
                    (None, None) => true,
                    (Some(a), Some(b)) if a.ptr_eq(b) => true,
                    (Some(a), Some(b)) => *equal_bodies
                        .entry((std::ptr::from_ref(&**a), std::ptr::from_ref(&**b)))
                        .or_insert_with(|| a == b),
                    _ => false,
                }
            });
        let owner = body_owner(function);
        let candidates = owner.and_then(|owner| body_inputs.get(&owner));
        let body = candidates.and_then(|candidates| {
            candidates
                .iter()
                .copied()
                .find(|&other| same_body_owner(function, &cache.functions[other].input))
        });
        let shared = candidates.and_then(|candidates| {
            candidates.iter().copied().find(|&other| {
                let input = &cache.functions[other].input;
                same_body_owner(function, input) && same_class_header(function, input)
            })
        });
        input_owners.push(shared.unwrap_or(index));
        let reads = if compatible { old.cloned() } else { None };
        let reads = if let Some(reads) = reads {
            cache.reused_inputs += 1;
            reads
        } else if let Some(other) = shared {
            cache.reused_inputs += 1;
            cache.shared_inputs += 1;
            let shared = &cache.functions[other];
            std::sync::Arc::new(FunctionReads {
                input: (*function).clone(),
                addresses: shared.addresses.clone(),
                dependencies: shared.dependencies.clone(),
                cost: shared.cost,
                normalized: shared.normalized.clone(),
            })
        } else {
            cache.rebuilt_inputs += 1;
            std::sync::Arc::new(FunctionReads {
                input: (*function).clone(),
                addresses: if let Some(other) = body {
                    cache.functions[other].addresses.clone()
                } else if let Some(old) = old {
                    old.addresses.clone()
                } else {
                    cache.address_scans += usize::from(function.blocks.is_some());
                    std::sync::Arc::new(function_addresses(function, cancelled)?)
                },
                dependencies: std::sync::Arc::new(dependencies(
                    function, &functions, &names, cancelled,
                )),
                cost: body.map_or_else(|| function.cost(), |other| cache.functions[other].cost),
                normalized: None,
            })
        };
        // The address contribution is independent of the header or candidate
        // role. Union it once per actual body, including unsupported bodies.
        if body.is_none() {
            addresses.extend(reads.addresses.iter().cloned());
        }
        cache.functions.push(reads);
        if shared.is_none()
            && let Some(owner) = owner
        {
            if body.is_none() {
                cache.body_owners += 1;
            }
            body_inputs.entry(owner).or_default().push(index);
        }
    }
    let mut outstanding = vec![None; count];
    let mut parents = vec![Vec::new(); count];
    let mut ready = BTreeSet::new();
    for (index, function) in functions.iter().enumerate() {
        if cancelled() {
            return None;
        }
        if !generated(&function.name) || addresses.contains(&function.name) {
            continue;
        }
        let Some(deps) = cache.functions[index].dependencies.as_ref().as_ref() else {
            continue;
        };
        outstanding[index] = Some(deps.len());
        if deps.is_empty() {
            ready.insert(index);
        }
        for &callee in deps {
            parents[callee].push(index);
        }
    }
    let mut representatives: Vec<Option<usize>> = vec![None; count];
    let mut stable_names: Vec<Option<String>> = vec![None; count];
    let mut implementations = std::collections::BTreeMap::new();
    let source_cost = cache
        .functions
        .iter()
        .try_fold(0usize, |sum, f| sum.checked_add(f.cost))?;
    let mut implementation_cost = 0usize;
    // Cached selectors only locate a bucket. Complete normalized Function Eq
    // still authorizes every class, including deliberate hash collisions.
    let mut classes: HashMap<zil_base::Fingerprint, Vec<(Function, usize)>> = HashMap::new();
    let mut current_keys: HashMap<usize, std::sync::Arc<Normalized>> = HashMap::new();
    let mut class_count = 0usize;
    let mut stats = Stats::default();
    while !ready.is_empty() {
        let mut next = BTreeSet::new();
        for index in ready {
            if cancelled() {
                return None;
            }
            let dependencies = cache.functions[index].dependencies.as_ref().as_ref()?;
            // Compare the actual current callee names while borrowed. The
            // successful normalized product owns them once, not per alias.
            let matches = |old: &Normalized| {
                old.callees.len() == dependencies.len()
                    && old
                        .callees
                        .iter()
                        .zip(dependencies)
                        .all(|((id, name), callee)| {
                            if id != callee {
                                return false;
                            }
                            let current = if stable {
                                stable_names[*callee].as_deref()
                            } else {
                                representatives[*callee].map(|rep| functions[rep].name.as_str())
                            };
                            current == Some(name.as_str())
                        })
            };
            let cached = stable
                .then(|| cache.functions[index].normalized.as_ref())
                .flatten()
                .filter(|old| matches(old));
            let normalized = if let Some(cached) = cached {
                cache.reused_keys += 1;
                cached.clone()
            } else if let Some(shared) = current_keys
                .get(&input_owners[index])
                .filter(|shared| matches(shared))
            {
                cache.reused_keys += 1;
                cache.shared_keys += 1;
                shared.clone()
            } else {
                cache.rebuilt_keys += 1;
                let callees = dependencies
                    .iter()
                    .map(|&callee| {
                        let name = if stable {
                            stable_names[callee].as_ref()?.clone()
                        } else {
                            functions[representatives[callee]?].name.clone()
                        };
                        Some((callee, name))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let function = normalized_key_inner(
                    functions[index],
                    &functions,
                    &names,
                    &representatives,
                    stable.then_some(stable_names.as_slice()),
                    cancelled,
                )?;
                use std::hash::Hash;
                let mut hash = crate::object_cache::BufferedHasher::new();
                b"ZIL-CLOSED-CALL-CLASS-v1".hash(&mut hash);
                function.hash(&mut hash);
                std::sync::Arc::new(Normalized {
                    callees,
                    function,
                    selector: hash.digest(),
                })
            };
            current_keys
                .entry(input_owners[index])
                .or_insert_with(|| normalized.clone());
            if stable {
                std::sync::Arc::make_mut(&mut cache.functions[index]).normalized =
                    Some(normalized.clone());
            }
            let bucket = classes.entry(normalized.selector).or_default();
            let rep = if let Some((_, rep)) =
                bucket.iter().find(|(key, _)| *key == normalized.function)
            {
                *rep
            } else {
                if stable {
                    if normalized.function.linkage != Linkage::External {
                        return None;
                    }
                    let name = format!("zil.class.{}", normalized.selector.to_hex());
                    if all_symbols.contains(name.as_str()) || implementations.contains_key(&name) {
                        return None;
                    }
                    let mut implementation = normalized.function.clone();
                    implementation.name = name.clone();
                    implementation_cost =
                        implementation_cost.checked_add(cache.functions[index].cost)?;
                    if implementation_cost > source_cost
                        || module
                            .items
                            .len()
                            .checked_add(implementations.len())?
                            .checked_add(1)?
                            > crate::machine_cgu::layout::MAX_ITEMS
                    {
                        return None;
                    }
                    implementations.insert(name.clone(), implementation);
                    stable_names[index] = Some(name);
                }
                bucket.push((normalized.function.clone(), index));
                class_count += 1;
                index
            };
            representatives[index] = Some(rep);
            if stable {
                stable_names[index] = stable_names[rep].clone();
            }
            stats.eligible_functions += 1;
            for &parent in &parents[index] {
                let remaining = outstanding[parent].as_mut()?;
                *remaining -= 1;
                if *remaining == 0 {
                    next.insert(parent);
                }
            }
        }
        ready = next;
    }
    stats.equivalence_classes = class_count;
    let replacements: HashMap<String, String> = representatives
        .iter()
        .enumerate()
        .filter_map(|(index, rep)| {
            let rep = (*rep)?;
            if stable {
                Some((
                    functions[index].name.clone(),
                    stable_names[index].as_ref()?.clone(),
                ))
            } else {
                (rep != index).then(|| (functions[index].name.clone(), functions[rep].name.clone()))
            }
        })
        .collect();
    if replacements.is_empty() {
        return (!cancelled()).then_some((stats, BTreeSet::new(), cache));
    }
    // Stage all changes in a COW clone: cancellation at any point cannot leave
    // an incompletely transformed original module or publish partial classes.
    let mut changed = module.clone();
    // The replacement map is fixed. Rewrite an observed original body once,
    // including non-candidates, and preserve sharing for downstream consumers.
    let mut rewritten_bodies: HashMap<*const Vec<Block>, (usize, SharedVec<Block>, usize)> =
        HashMap::new();
    let mut function_index = 0;
    for item in &mut changed.items {
        if cancelled() {
            return None;
        }
        let Item::Function(function) = item else {
            continue;
        };
        let index = function_index;
        function_index += 1;
        let Some(owner) = body_owner(function) else {
            continue;
        };
        if let Some((_, blocks, calls)) = rewritten_bodies
            .get(&owner)
            .filter(|(other, _, _)| same_body_owner(function, functions[*other]))
        {
            function.blocks = Some(blocks.clone());
            stats.rewritten_calls += *calls;
            continue;
        }
        let mut calls = 0;
        for block in function.blocks.iter().flatten() {
            for inst in &block.instructions {
                if cancelled() {
                    return None;
                }
                if matches!(&inst.op, Op::Call { callee: Val::Global(name), .. } if replacements.contains_key(name))
                {
                    calls += 1;
                }
            }
        }
        if calls != 0 {
            // Only a function CLASSIFIED in this current pass may consume the
            // normalized data. A previous normalized row for a now-protected
            // function or unclassified dependency never supplies this output.
            // Current normalized_key changed precisely these target atoms;
            // its complete input/header/callee binding was checked above.
            let normalized = stable
                .then_some(representatives[index])
                .flatten()
                .and_then(|_| cache.functions[index].normalized.as_ref());
            if let Some(normalized) = normalized {
                function.blocks = normalized.function.blocks.clone();
            } else {
                for block in function.blocks.as_mut()?.iter_mut() {
                    for inst in &mut block.instructions {
                        if cancelled() {
                            return None;
                        }
                        if let Op::Call {
                            callee: Val::Global(name),
                            ..
                        } = &mut inst.op
                            && let Some(rep) = replacements.get(name)
                        {
                            *name = rep.clone();
                        }
                    }
                }
            }
        }
        stats.rewritten_calls += calls;
        rewritten_bodies.insert(owner, (index, function.blocks.as_ref()?.clone(), calls));
    }
    if cancelled() {
        return None;
    }
    // Preserve the existing call-free closed DCE route. Unreferenced class
    // bodies would add only work, with no direct-call atom to stabilize.
    if stable && stats.rewritten_calls == 0 {
        return Some((stats, BTreeSet::new(), cache));
    }
    let symbols = implementations.keys().cloned().collect();
    if stable {
        changed
            .items
            .extend(implementations.into_values().map(Item::Function));
    }
    if stats.rewritten_calls != 0 || stable {
        *module = changed;
    }
    Some((stats, symbols, cache))
}

#[cfg(test)]
#[path = "machine_call_classes_tests.rs"]
mod shared_body_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zil_llvm_api::model::{Attr, Block, Global, Inst, Param, Signature};

    fn body(name: &str, instructions: Vec<Inst>) -> Function {
        Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions,
                }]
                .into(),
            ),
        }
    }

    fn ret(value: Val) -> Inst {
        Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), value))))
    }

    fn leaf(name: &str, value: i128) -> Function {
        body(name, vec![ret(Val::Int(value))])
    }

    fn call(name: &str, target: &str) -> Function {
        body(
            name,
            vec![
                Inst::new(
                    Some("v".into()),
                    Op::Call {
                        result: Ty::Int(32),
                        result_attrs: vec![],
                        callee: Val::Global(target.into()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                ret(Val::Local("v".into())),
            ],
        )
    }

    fn module(functions: Vec<Function>) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "unchanged-original-identity".into(),
            items: functions.into_iter().map(Item::Function).collect(),
        }
    }

    fn run(module: &mut Module) -> Stats {
        canonicalize(module, &BTreeSet::new(), &|| false).unwrap()
    }

    fn target(module: &Module, name: &str) -> String {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(f) if f.name == name => f.blocks.as_ref().unwrap()[0]
                    .instructions
                    .iter()
                    .find_map(|inst| match &inst.op {
                        Op::Call {
                            callee: Val::Global(name),
                            ..
                        } => Some(name.clone()),
                        _ => None,
                    }),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn nested_classes_keep_all_definitions_and_choose_source_order_not_topological_accident() {
        // The earlier parent depends on the later leaf. A simple priority-ready
        // queue could incorrectly make the later parent the representative.
        let original = module(vec![
            call("zil.fn.8", "zil.fn.2"),
            leaf("zil.fn.1", 7),
            call("zil.fn.9", "zil.fn.1"),
            leaf("zil.fn.2", 7),
            call("zil.fn.10", "zil.fn.8"),
            call("zil.fn.11", "zil.fn.9"),
            call("main", "zil.fn.11"),
        ]);
        let mut actual = original.clone();
        assert_eq!(
            run(&mut actual),
            Stats {
                eligible_functions: 6,
                equivalence_classes: 3,
                rewritten_calls: 3,
            }
        );
        assert_eq!(target(&actual, "zil.fn.8"), "zil.fn.1");
        assert_eq!(target(&actual, "zil.fn.11"), "zil.fn.8");
        assert_eq!(target(&actual, "main"), "zil.fn.10");
        // Compare complete modules after changing ONLY the three authorized
        // direct target atoms in the expected copy, not a lossy names inventory.
        let mut expected = original;
        for (item, name) in [(0, "zil.fn.1"), (5, "zil.fn.8"), (6, "zil.fn.10")] {
            let Item::Function(f) = &mut expected.items[item] else {
                panic!()
            };
            let Op::Call { callee, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[0].op
            else {
                panic!()
            };
            *callee = Val::Global(name.into());
        }
        assert_eq!(actual, expected);
        assert_eq!(run(&mut actual).rewritten_calls, 0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn normalized_keys_share_bodies_until_a_direct_target_actually_changes() {
        let first = leaf("zil.fn.0", 7);
        let duplicate = leaf("zil.fn.1", 7);
        let canonical_call = call("zil.fn.2", "zil.fn.0");
        let changed_call = call("zil.fn.3", "zil.fn.1");
        let trap = Function {
            name: "llvm.trap".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        };
        let intrinsic_call = body(
            "zil.fn.4",
            vec![
                Inst::new(
                    None,
                    Op::Call {
                        result: Ty::Void,
                        result_attrs: vec![],
                        callee: Val::Global(trap.name.clone()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                Inst::new(None, Op::Unreachable),
            ],
        );
        let functions = vec![
            &first,
            &duplicate,
            &canonical_call,
            &changed_call,
            &trap,
            &intrinsic_call,
        ];
        let names = functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.as_str(), i))
            .collect();
        let representatives = [Some(0), Some(0), None, None, None, None];
        for function in [&first, &duplicate, &canonical_call, &intrinsic_call] {
            let key =
                normalized_key(function, &functions, &names, &representatives, &|| false).unwrap();
            assert!(key.name.is_empty());
            assert!(
                key.blocks
                    .as_ref()
                    .unwrap()
                    .ptr_eq(function.blocks.as_ref().unwrap())
            );
            let mut expected = function.clone();
            expected.name.clear();
            assert_eq!(key, expected);
        }
        let key = normalized_key(&changed_call, &functions, &names, &representatives, &|| {
            false
        })
        .unwrap();
        assert!(
            !key.blocks
                .as_ref()
                .unwrap()
                .ptr_eq(changed_call.blocks.as_ref().unwrap())
        );
        let mut expected = canonical_call.clone();
        expected.name.clear();
        assert_eq!(key, expected);
        assert_eq!(changed_call, call("zil.fn.3", "zil.fn.1"));
    }

    #[test]
    fn changed_constant_abi_linkage_attributes_and_local_names_do_not_collapse() {
        let base = leaf("zil.fn.0", 7);
        let mut abi = leaf("zil.fn.2", 7);
        abi.signature.result_attrs.push(Attr::ZeroExt);
        let mut linkage = leaf("zil.fn.3", 7);
        linkage.linkage = Linkage::Internal;
        let mut attrs = leaf("zil.fn.4", 7);
        attrs.attrs.push(Attr::NoInline);
        let mut local = leaf("zil.fn.5", 7);
        local.blocks.as_mut().unwrap()[0].name = "different".into();
        let mut m = module(vec![base, leaf("zil.fn.1", 8), abi, linkage, attrs, local]);
        let before = m.clone();
        let stats = run(&mut m);
        assert_eq!(stats.eligible_functions, 6);
        assert_eq!(stats.equivalence_classes, 6);
        assert_eq!(m, before);
    }

    #[test]
    fn protected_entry_and_nested_global_or_argument_addresses_are_not_class_members() {
        let mut sink = call("sink", "foreign");
        let Op::Call { args, .. } = &mut sink.blocks.as_mut().unwrap()[0].instructions[0].op else {
            panic!()
        };
        args.push(Arg::new(Ty::Ptr, Val::Global("zil.fn.3".into())));
        let mut foreign = leaf("foreign", 0);
        foreign.blocks = None;
        foreign.signature.params.push(Param::unnamed(Ty::Ptr));
        let mut m = module(vec![
            leaf("zil.fn.0", 7),
            leaf("zil.fn.1", 7),
            leaf("zil.fn.2", 7),
            leaf("zil.fn.3", 7),
            call("main", "zil.fn.1"),
            sink,
            foreign,
        ]);
        m.items.push(Item::Global(Global {
            name: "addresses".into(),
            linkage: Linkage::External,
            ty: Ty::array(1, Ty::array(1, Ty::Ptr)),
            initializer: Some(Val::Array(vec![Arg::new(
                Ty::array(1, Ty::Ptr),
                Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("zil.fn.2".into()))]),
            )])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        }));
        let before = m.clone();
        let protected = BTreeSet::from(["zil.fn.1".to_string()]);
        let stats = canonicalize(&mut m, &protected, &|| false).unwrap();
        assert_eq!(stats.eligible_functions, 1);
        assert_eq!(m, before);
    }

    #[test]
    fn recursion_and_callers_of_recursion_foreign_or_unknown_intrinsics_are_refused() {
        let mut foreign = leaf("foreign", 0);
        foreign.blocks = None;
        let mut unknown = leaf("llvm.readcyclecounter", 0);
        unknown.blocks = None;
        let mut m = module(vec![
            call("zil.fn.0", "zil.fn.1"),
            call("zil.fn.1", "zil.fn.0"),
            call("zil.fn.2", "zil.fn.0"),
            call("zil.fn.3", "zil.fn.3"),
            call("zil.fn.4", "foreign"),
            call("zil.fn.5", "foreign"),
            call("zil.fn.6", "llvm.readcyclecounter"),
            call("zil.fn.7", "zil.fn.4"),
            foreign,
            unknown,
            call("main", "zil.fn.5"),
        ]);
        let before = m.clone();
        assert_eq!(run(&mut m), Stats::default());
        assert_eq!(m, before);
    }

    #[test]
    fn memory_indirect_pointer_undef_and_unknown_opcodes_are_refused() {
        let operations = vec![
            Op::Load {
                ty: Ty::Int(32),
                ptr: Val::Global("data".into()),
                align: 4,
            },
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: Val::Local("callback".into()),
                args: vec![],
                vararg: None,
            },
            Op::Cast {
                opcode: "ptrtoint".into(),
                arg: Arg::new(Ty::Ptr, Val::Global("data".into())),
                to: Ty::Int(32),
            },
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: Val::Undef,
                right: Val::Int(1),
            },
            Op::Binary {
                opcode: "unknown".into(),
                ty: Ty::Int(32),
                left: Val::Int(0),
                right: Val::Int(1),
            },
            Op::Freeze(Arg::new(Ty::Int(32), Val::Poison)),
        ];
        for operation in operations {
            let a = body(
                "zil.fn.0",
                vec![
                    Inst::new(Some("v".into()), operation),
                    ret(Val::Local("v".into())),
                ],
            );
            let mut b = a.clone();
            b.name = "zil.fn.1".into();
            let mut m = module(vec![a, b, call("main", "zil.fn.1")]);
            let before = m.clone();
            assert_eq!(run(&mut m).eligible_functions, 0);
            assert_eq!(m, before);
        }
        let mut a = leaf("zil.fn.0", 0);
        a.signature.params.push(Param::new(Ty::Ptr, "p"));
        let mut b = a.clone();
        b.name = "zil.fn.1".into();
        let mut m = module(vec![a, b]);
        assert_eq!(run(&mut m).eligible_functions, 0);
    }

    #[test]
    fn checked_overflow_and_trap_are_compared_exactly_without_arithmetic_rewriting() {
        let pair = Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]);
        let overflow = Function {
            name: "llvm.sadd.with.overflow.i32".into(),
            linkage: Linkage::External,
            signature: Signature::new(
                pair.clone(),
                vec![Param::unnamed(Ty::Int(32)), Param::unnamed(Ty::Int(32))],
            ),
            attrs: vec![],
            blocks: None,
        };
        let trap = Function {
            name: "llvm.trap".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        };
        let mut checked = body(
            "zil.fn.0",
            vec![
                Inst::new(
                    Some("pair".into()),
                    Op::Call {
                        result: pair.clone(),
                        result_attrs: vec![],
                        callee: Val::Global(overflow.name.clone()),
                        args: vec![
                            Arg::new(Ty::Int(32), Val::Int(i128::from(i32::MAX))),
                            Arg::new(Ty::Int(32), Val::Int(1)),
                        ],
                        vararg: None,
                    },
                ),
                Inst::new(
                    Some("value".into()),
                    Op::Extract {
                        arg: Arg::new(pair.clone(), Val::Local("pair".into())),
                        index: 0,
                    },
                ),
                Inst::new(
                    Some("flag".into()),
                    Op::Extract {
                        arg: Arg::new(pair, Val::Local("pair".into())),
                        index: 1,
                    },
                ),
                Inst::new(
                    None,
                    Op::CondBranch {
                        condition: Val::Local("flag".into()),
                        yes: "trap".into(),
                        no: "exit".into(),
                    },
                ),
            ],
        );
        checked.blocks.as_mut().unwrap().push(Block {
            name: "trap".into(),
            instructions: vec![
                Inst::new(
                    None,
                    Op::Call {
                        result: Ty::Void,
                        result_attrs: vec![],
                        callee: Val::Global(trap.name.clone()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                Inst::new(None, Op::Unreachable),
            ],
        });
        checked.blocks.as_mut().unwrap().push(Block {
            name: "exit".into(),
            instructions: vec![ret(Val::Local("value".into()))],
        });
        let mut duplicate = checked.clone();
        duplicate.name = "zil.fn.1".into();
        let mut m = module(vec![
            overflow,
            trap,
            checked,
            duplicate,
            call("main", "zil.fn.1"),
        ]);
        let before = m.clone();
        let stats = run(&mut m);
        assert_eq!(stats.eligible_functions, 2);
        assert_eq!(stats.equivalence_classes, 1);
        assert_eq!(stats.rewritten_calls, 1);
        assert_eq!(target(&m, "main"), "zil.fn.0");
        assert_eq!(&m.items[..4], &before.items[..4]);
    }

    #[test]
    fn cancellation_even_during_final_staging_is_transactional() {
        let original = module(vec![
            leaf("zil.fn.0", 7),
            leaf("zil.fn.1", 7),
            call("main", "zil.fn.1"),
        ]);
        let polls = AtomicUsize::new(0);
        let mut measured = original.clone();
        canonicalize(&mut measured, &BTreeSet::new(), &|| {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        })
        .unwrap();
        let total = polls.load(Ordering::Relaxed);
        for stop in [0, total / 2, total - 2, total - 1] {
            let polls = AtomicUsize::new(0);
            let mut m = original.clone();
            assert!(
                canonicalize(&mut m, &BTreeSet::new(), &|| {
                    polls.fetch_add(1, Ordering::Relaxed) >= stop
                })
                .is_none()
            );
            assert_eq!(m, original);
        }
    }

    #[test]
    fn long_call_dag_does_not_use_rust_recursion() {
        let mut functions = vec![leaf("zil.fn.0", 7)];
        for i in 1..2048 {
            functions.push(call(&format!("zil.fn.{i}"), &format!("zil.fn.{}", i - 1)));
        }
        let mut m = module(functions);
        let before = m.clone();
        let stats = run(&mut m);
        assert_eq!(stats.eligible_functions, 2048);
        assert_eq!(stats.equivalence_classes, 2048);
        assert_eq!(m, before);
    }
    #[test]
    fn stable_classes_survive_editing_the_previous_source_order_representative() {
        let original = module(vec![
            leaf("zil.fn.1", 7),
            leaf("zil.fn.2", 7),
            call("main", "zil.fn.2"),
        ]);
        let mut first = original.clone();
        let (_, symbols) = canonicalize_stable(&mut first, &BTreeSet::new(), &|| false).unwrap();
        let stable = target(&first, "main");
        assert!(symbols.contains(&stable));
        let mut edited = original.clone();
        edited.items[0] = Item::Function(leaf("zil.fn.1", 9));
        canonicalize_stable(&mut edited, &BTreeSet::new(), &|| false).unwrap();
        assert_eq!(target(&edited, "main"), stable);
        for name in ["zil.fn.1", "zil.fn.2"] {
            assert!(
                edited
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Function(f) if f.name==name))
            );
        }
        // A true dependency change must still change the actual called class.
        let mut changed = original;
        changed.items[1] = Item::Function(leaf("zil.fn.2", 9));
        canonicalize_stable(&mut changed, &BTreeSet::new(), &|| false).unwrap();
        assert_ne!(target(&changed, "main"), stable);
    }

    #[test]
    fn stable_classes_reject_global_symbol_collision_without_mutating_input() {
        let original = module(vec![leaf("zil.fn.1", 7), call("main", "zil.fn.1")]);
        let mut probe = original.clone();
        canonicalize_stable(&mut probe, &BTreeSet::new(), &|| false).unwrap();
        let mut collision = original;
        collision.items.push(Item::Global(Global {
            name: target(&probe, "main"),
            linkage: Linkage::External,
            constant: false,
            ty: Ty::Int(32),
            initializer: Some(Val::Int(0)),
            unnamed_addr: false,
            alignment: 4,
        }));
        let before = collision.clone();
        assert!(canonicalize_stable(&mut collision, &BTreeSet::new(), &|| false).is_none());
        assert_eq!(collision, before);
    }

    #[test]
    fn call_free_program_keeps_existing_closed_dce_path() {
        let mut actual = module(vec![leaf("zil.fn.1", 7), leaf("main", 9)]);
        let before = actual.clone();
        let (_, symbols) = canonicalize_stable(&mut actual, &BTreeSet::new(), &|| false).unwrap();
        assert!(symbols.is_empty());
        assert_eq!(actual, before);
    }
    #[test]
    fn memo_reuses_only_equal_inputs_and_current_callee_class_dependencies() {
        let original = module(vec![
            leaf("zil.fn.0", 7),
            leaf("zil.fn.1", 7),
            call("zil.fn.2", "zil.fn.0"),
            call("zil.fn.3", "zil.fn.1"),
            call("main", "zil.fn.3"),
        ]);
        let mut first = original.clone();
        let (_, _, cache) =
            canonicalize_stable_cached(&mut first, &BTreeSet::new(), &|| false, None).unwrap();
        let mut source = original;
        source.items[0] = Item::Function(leaf("zil.fn.0", 9));
        let mut actual = source.clone();
        let (stats, symbols, next) =
            canonicalize_stable_cached(&mut actual, &BTreeSet::new(), &|| false, Some(&cache))
                .unwrap();
        let mut expected = source.clone();
        let fresh = canonicalize_stable(&mut expected, &BTreeSet::new(), &|| false).unwrap();
        assert_eq!((stats, symbols), fresh);
        assert_eq!(actual, expected);
        assert_eq!((next.reused_inputs, next.rebuilt_inputs), (4, 1));
        assert_eq!((next.reused_keys, next.rebuilt_keys), (2, 2));
        // A role change must reclassify actual membership and all its callers,
        // even while every immutable function body is still equal.
        let protected = BTreeSet::from(["zil.fn.1".to_owned()]);
        let mut actual = source.clone();
        canonicalize_stable_cached(&mut actual, &protected, &|| false, Some(&next)).unwrap();
        let mut expected = source;
        canonicalize_stable(&mut expected, &protected, &|| false).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(target(&actual, "zil.fn.3"), "zil.fn.1");
    }

    #[test]
    fn cached_selector_collision_does_not_authorize_different_class_bodies() {
        let original = module(vec![
            leaf("zil.fn.0", 7),
            leaf("zil.fn.1", 9),
            call("main", "zil.fn.1"),
        ]);
        let mut first = original.clone();
        let (_, _, mut cache) =
            canonicalize_stable_cached(&mut first, &BTreeSet::new(), &|| false, None).unwrap();
        let selector = cache.functions[0].normalized.as_ref().unwrap().selector;
        std::sync::Arc::make_mut(
            std::sync::Arc::make_mut(&mut cache.functions[1])
                .normalized
                .as_mut()
                .unwrap(),
        )
        .selector = selector;
        let mut actual = original.clone();
        assert!(
            canonicalize_stable_cached(&mut actual, &BTreeSet::new(), &|| false, Some(&cache))
                .is_none()
        );
        assert_eq!(actual, original);
    }
}
