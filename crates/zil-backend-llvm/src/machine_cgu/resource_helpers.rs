//! Direct-call-only private code is not shared mutable state. Keep actual local
//! identities co-located, but copy unobservable helper implementations as needed.
//! No inline policy, ownership fact or verification result is changed.
use super::{Plan, Sets, direct_calls, distribute_item, rewrite_direct_calls};
use crate::EntryPoint;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_base::Fingerprint;
use zil_ir::LirView;
use zil_llvm_api::model::{Item, Linkage, Module, Op, Val};

// A policy budget, not a promise about LLVM's eventual inlining decision.
// Large source bodies keep a canonical cross-unit call; copying all of them
// would invalidate the caller on every implementation edit and can grow code.
// Compiler-owned call-only helper closures remain completely available.
const MAX_IMPORTED_SOURCE_COST: usize = 32;

struct Definition {
    item: usize,
    name: String,
    local: bool,
    function: bool,
    refs: BTreeSet<String>,
    calls: BTreeSet<String>,
    cost: usize,
}
struct Unit {
    owner: usize,
    definitions: Vec<usize>,
    helpers: Vec<usize>,
    imports: Vec<usize>,
    declarations: Vec<usize>,
    prelude: Vec<usize>,
}
pub(super) struct Recipe {
    units: Vec<Unit>,
}
fn spend(fuel: &mut usize, count: usize) -> Option<()> {
    *fuel = fuel.checked_sub(count)?;
    Some(())
}

/// Optional and transaction-like: every rejection leaves the original model
/// available to the established planner, including its full safety preflight.
pub(super) fn prepare(
    module: &Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
) -> Option<Recipe> {
    if !(2..=64).contains(&count) || lir.closures.is_empty() || module.items.len() > 65536 {
        return None;
    }
    let files = lir
        .functions
        .iter()
        .filter(|f| f.foreign.is_none())
        .map(|f| f.span.file.0)
        .collect::<BTreeSet<_>>();
    // A giant single function cannot become incremental merely by duplicating
    // its helpers. Do not pay copying/planning costs for that unchanged case.
    if files.len() < 2 {
        return None;
    }
    let mut fuel = 1usize << 20;
    spend(
        &mut fuel,
        module.items.len().checked_add(lir.functions.len())?,
    )?;
    let mut defs = Vec::new();
    let mut prelude = Vec::new();
    let mut symbols = BTreeSet::new();
    let mut addressed = BTreeSet::new();
    for (index, item) in module.items.iter().enumerate() {
        let name = match item {
            Item::Function(f) => Some(&f.name),
            Item::Global(g) => Some(&g.name),
            _ => None,
        };
        if name.is_some_and(|name| !symbols.insert(name.clone())) {
            return None;
        }
        match item {
            Item::Function(f) if f.blocks.is_some() => {
                let cost = f.cost();
                spend(&mut fuel, cost)?;
                for i in f.blocks.iter().flatten().flat_map(|b| &b.instructions) {
                    match &i.op {
                        Op::Call {
                            callee: Val::Global(_),
                            args,
                            ..
                        } => {
                            for arg in args {
                                arg.value.symbols(&mut addressed);
                            }
                        }
                        _ => {
                            for value in i.op.values() {
                                value.symbols(&mut addressed);
                            }
                        }
                    }
                }
                defs.push(Definition {
                    item: index,
                    name: f.name.clone(),
                    local: f.linkage.is_local(),
                    function: true,
                    refs: f.references(),
                    calls: direct_calls(f),
                    cost,
                });
            }
            Item::Global(g) if g.initializer.is_some() => {
                let mut refs = BTreeSet::new();
                g.initializer.as_ref()?.symbols(&mut refs);
                addressed.extend(refs.iter().cloned());
                defs.push(Definition {
                    item: index,
                    name: g.name.clone(),
                    local: g.linkage.is_local(),
                    function: false,
                    refs,
                    calls: BTreeSet::new(),
                    cost: 1,
                });
            }
            _ => prelude.push(index),
        }
    }
    let names = defs
        .iter()
        .enumerate()
        .map(|(id, d)| (d.name.clone(), id))
        .collect::<BTreeMap<_, _>>();
    let mut source = BTreeSet::new();
    let mut source_ids = Vec::new();
    let mut selected_entry = None;
    for f in lir.functions.iter().filter(|f| f.foreign.is_none()) {
        let original = format!("zil.fn.{}", f.id.0);
        let large = format!("{original}.lc1");
        let id = *names.get(&large).or_else(|| names.get(&original))?;
        if !source.insert(id) {
            return None;
        }
        source_ids.push((f.span.file.0, id));
        if match entry {
            EntryPoint::Main => f.name == "main",
            EntryPoint::ReturnValue(wanted) | EntryPoint::Test(wanted) => f.id == wanted,
            EntryPoint::None => false,
        } {
            selected_entry = Some(id);
        }
    }
    let mut helpers = defs
        .iter()
        .enumerate()
        .map(|(id, d)| {
            d.function && d.local && !source.contains(&id) && !addressed.contains(&d.name)
        })
        .collect::<Vec<_>>();
    // Reverse dependencies propagate a non-copyable identity only to its real
    // helper users. Cycles of call-only functions are legal; cycles containing
    // an observed address or local global are removed together.
    let mut users = vec![Vec::new(); defs.len()];
    for (id, d) in defs.iter().enumerate() {
        for name in &d.refs {
            spend(&mut fuel, 1)?;
            if let Some(&target) = names.get(name) {
                users[target].push(id);
            }
        }
    }
    let mut rejected = VecDeque::new();
    for (id, d) in defs.iter().enumerate() {
        if d.local && !helpers[id] {
            rejected.push_back(id);
        }
    }
    while let Some(target) = rejected.pop_front() {
        for &caller in &users[target] {
            spend(&mut fuel, 1)?;
            if helpers[caller] {
                helpers[caller] = false;
                rejected.push_back(caller);
            }
        }
    }
    if !helpers.iter().any(|v| *v) {
        return None;
    }
    let mut sets = Sets((0..defs.len()).collect());
    for (id, d) in defs.iter().enumerate() {
        for name in &d.refs {
            if let Some(&target) = names.get(name)
                && defs[target].local
                && !helpers[target]
            {
                sets.join(id, target);
            }
        }
    }
    if let Some(entry) = selected_entry
        && let Some(&wrapper) = names.get("main")
    {
        sets.join(entry, wrapper);
    }
    if let (Some(&main), Some(&pe)) = (names.get("main"), names.get("zil.pe.entry")) {
        sets.join(main, pe);
    }
    // Preserve the optimizer neighborhood of a branch-only return dispatch.
    // Same-argument alternatives may collapse after body merging; separating
    // those large implementations would turn that into real dispatch overhead.
    // This is a conservative placement affinity, never a semantic equivalence
    // assertion and never an input-name or benchmark-specific classification.
    for &id in &source {
        let Item::Function(function) = &module.items[defs[id].item] else {
            continue;
        };
        let mut alternatives: Vec<(
            &[zil_llvm_api::model::Arg],
            &zil_llvm_api::model::Ty,
            Vec<usize>,
        )> = Vec::new();
        for block in function.blocks.iter().flatten() {
            let [call, returned] = block.instructions.as_slice() else {
                continue;
            };
            let Op::Call {
                callee: Val::Global(name),
                args,
                result,
                ..
            } = &call.op
            else {
                continue;
            };
            let Op::Return(Some(value)) = &returned.op else {
                continue;
            };
            let Some(local) = &call.result else {
                continue;
            };
            if value.ty != *result || value.value != Val::Local(local.clone()) {
                continue;
            }
            let Some(&target) = names.get(name) else {
                continue;
            };
            if !source.contains(&target) || defs[target].cost <= MAX_IMPORTED_SOURCE_COST {
                continue;
            }
            spend(
                &mut fuel,
                alternatives.len().checked_add(args.len())?.checked_add(1)?,
            )?;
            if let Some((_, _, targets)) = alternatives
                .iter_mut()
                .find(|(prior, ty, _)| *prior == args.as_slice() && *ty == result)
            {
                if !targets.contains(&target) {
                    targets.push(target);
                }
            } else {
                alternatives.push((args, result, vec![target]));
            }
        }
        for (_, _, targets) in alternatives {
            if targets.len() > 1 {
                for target in targets {
                    sets.join(id, target);
                }
            }
        }
    }
    let mut first = BTreeMap::new();
    for &(file, id) in &source_ids {
        if let Some(&other) = first.get(&file) {
            sets.join(other, id);
        } else {
            first.insert(file, id);
        }
    }
    let roots = (0..defs.len()).map(|id| sets.root(id)).collect::<Vec<_>>();
    let mut source_keys = BTreeMap::<usize, u32>::new();
    for (file, id) in source_ids {
        source_keys
            .entry(roots[id])
            .and_modify(|key| *key = (*key).min(file))
            .or_insert(file);
    }
    let owners = defs
        .iter()
        .enumerate()
        .map(|(id, _)| {
            source_keys.get(&roots[id]).map_or_else(
                || {
                    let hash = Fingerprint::of(defs[roots[id]].name.as_bytes()).0;
                    u64::from_le_bytes(hash[..8].try_into().expect("eight bytes")) as usize % count
                },
                |file| *file as usize % count,
            )
        })
        .collect::<Vec<_>>();
    let mut members = BTreeMap::<usize, Vec<usize>>::new();
    for (id, &owner) in owners.iter().enumerate() {
        if !helpers[id] {
            members.entry(owner).or_default().push(id);
        }
    }
    if members.len() < 2 {
        return None;
    }
    let original_cost = defs
        .iter()
        .try_fold(0usize, |sum, d| sum.checked_add(d.cost))?;
    let mut extra = 0usize;
    let mut helper_uses = vec![0usize; defs.len()];
    let mut units = Vec::new();
    for (owner, definitions) in members {
        let mut pending = VecDeque::from(definitions.clone());
        let mut seen = BTreeSet::new();
        let mut imports = BTreeSet::new();
        let mut copied_helpers = BTreeSet::new();
        let mut needed = BTreeSet::new();
        let mut referenced = BTreeSet::new();
        while let Some(id) = pending.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            spend(&mut fuel, 1)?;
            for name in &defs[id].refs {
                spend(&mut fuel, 1)?;
                referenced.insert(name.clone());
                let Some(&target) = names.get(name) else {
                    continue;
                };
                if helpers[target] {
                    if copied_helpers.insert(target) {
                        pending.push_back(target);
                    }
                } else if defs[target].local {
                    if owners[target] != owner {
                        return None;
                    }
                } else if owners[target] != owner {
                    needed.insert(target);
                    if defs[id].calls.contains(name)
                        && defs[target].function
                        && defs[target].cost <= MAX_IMPORTED_SOURCE_COST
                    {
                        if !source.contains(&target) {
                            return None;
                        }
                        if imports.insert(target) {
                            let Item::Function(f) = &module.items[defs[target].item] else {
                                return None;
                            };
                            if f.linkage != Linkage::External {
                                return None;
                            }
                            extra = extra.checked_add(defs[target].cost)?;
                            if extra > original_cost {
                                return None;
                            }
                            pending.push_back(target);
                        }
                    }
                }
            }
        }
        for &id in &copied_helpers {
            if helper_uses[id] != 0 {
                extra = extra.checked_add(defs[id].cost)?;
            }
            helper_uses[id] += 1;
            if extra > original_cost {
                return None;
            }
        }
        for &id in &imports {
            if symbols.contains(&format!("zil.resource.import.{owner}.{}", defs[id].name)) {
                return None;
            }
        }
        let retained = prelude
            .iter()
            .copied()
            .filter(|&index| match &module.items[index] {
                Item::NamedType(..) => true,
                Item::Function(f) => referenced.contains(&f.name),
                Item::Global(g) => referenced.contains(&g.name),
            })
            .collect();
        units.push(Unit {
            owner,
            definitions: definitions.into_iter().map(|id| defs[id].item).collect(),
            helpers: copied_helpers.into_iter().map(|id| defs[id].item).collect(),
            imports: imports.into_iter().map(|id| defs[id].item).collect(),
            declarations: needed.into_iter().map(|id| defs[id].item).collect(),
            prelude: retained,
        });
    }
    // Dead/private helpers and unused declarations may be removed by this plan.
    // Validate the complete unchanged input before that can hide malformed IR.
    if zil_llvm_api::validate_module(module).is_err() {
        return None;
    }
    Some(Recipe { units })
}

impl Recipe {
    pub(super) fn replay(self, module: Module) -> Plan {
        let mut remaining = vec![0usize; module.items.len()];
        let mut prototypes = BTreeMap::new();
        for unit in &self.units {
            for &id in unit
                .definitions
                .iter()
                .chain(&unit.helpers)
                .chain(&unit.imports)
            {
                remaining[id] += 1;
            }
            for &id in &unit.declarations {
                prototypes
                    .entry(id)
                    .or_insert_with(|| match &module.items[id] {
                        Item::Function(f) => Item::Function(f.declaration()),
                        Item::Global(g) => Item::Global(g.declaration()),
                        _ => unreachable!("definition"),
                    });
            }
        }
        let mut original = module.items.into_iter().map(Some).collect::<Vec<_>>();
        let mut output = Vec::new();
        for unit in self.units {
            let clone_names = unit
                .imports
                .iter()
                .map(|&id| {
                    let Item::Function(f) = original[id].as_ref().expect("remaining import") else {
                        unreachable!()
                    };
                    (
                        f.name.clone(),
                        format!("zil.resource.import.{}.{}", unit.owner, f.name),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let mut items = Vec::new();
            for &id in &unit.prelude {
                items.push(original[id].as_ref().expect("prelude").clone());
            }
            for id in &unit.declarations {
                items.push(prototypes[id].clone());
            }
            for (ids, imported) in [
                (&unit.imports, true),
                (&unit.helpers, false),
                (&unit.definitions, false),
            ] {
                for &id in ids {
                    let mut item = distribute_item(&mut original[id], &mut remaining[id]);
                    if let Item::Function(f) = &mut item {
                        if imported {
                            f.name = clone_names[&f.name].clone();
                            f.linkage = Linkage::Internal;
                        }
                        rewrite_direct_calls(f, &clone_names);
                    }
                    items.push(item);
                }
            }
            output.push(Module {
                triple: module.triple.clone(),
                fingerprint: module.fingerprint.clone(),
                items: items.into(),
            });
        }
        Plan {
            units: output,
            fallback: None,
        }
    }
}

#[cfg(test)]
mod tests;
