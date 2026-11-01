//! Partition the owned machine IR by semantic symbol references, never by parsing
//! LLVM assembly. Local-linkage definitions and all users remain co-located.
use crate::EntryPoint;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{FileId, Fingerprint};
#[cfg(test)]
use zil_ir::LirModule;
use zil_ir::LirView;
use zil_llvm_api::model::{Function, Item, Module, Op, Val};

pub(crate) struct Plan {
    pub units: Vec<Module>,
    pub fallback: Option<&'static str>,
}
struct Definition {
    name: String,
    index: usize,
    local: bool,
    refs: BTreeSet<String>,
    calls: BTreeSet<String>,
}

// Source callers can change when a callee's result propagates even after their
// machine calls disappear. Isolate only a small, closed set of current linkage
// components; ordinary dynamic call graphs retain the existing partition.
fn small_caller_components(
    module: &Module,
    lir: &LirView<'_>,
    defs: &[Definition],
    names: &BTreeMap<&str, usize>,
    roots: &[usize],
    entry: Option<usize>,
) -> Option<BTreeSet<usize>> {
    let mut remaining = 1_000_000usize.checked_sub(lir.functions.len())?;
    let source_ids = lir
        .functions
        .iter()
        .filter(|function| function.foreign.is_none())
        .map(|function| function.id)
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    for function in lir.functions.iter().filter(|f| f.foreign.is_none()) {
        remaining = remaining.checked_sub(function.blocks.len())?;
        let mut calls_source = false;
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            remaining = remaining.checked_sub(1)?;
            if matches!(&instruction.kind, zil_ir::InstructionKind::Call { function, .. }
                if source_ids.contains(function))
            {
                calls_source = true;
                break;
            }
        }
        if calls_source {
            let name = format!("zil.fn.{}", function.id.0);
            selected.insert(roots[*names.get(name.as_str())?]);
            if selected.len() > 256 {
                return None;
            }
        }
    }
    if selected.is_empty() {
        return None;
    }
    if let Some(entry) = entry {
        selected.insert(roots[entry]);
    }
    let mut total_cost = 0usize;
    let mut selected_cost = 0usize;
    let mut selected_count = 0usize;
    for (id, definition) in defs.iter().enumerate() {
        remaining = remaining.checked_sub(1)?;
        let cost = match &module.items[definition.index] {
            Item::Function(function) => {
                remaining = remaining.checked_sub(function.blocks.as_ref().map_or(0, Vec::len))?;
                function.cost()
            }
            _ => 1,
        };
        total_cost = total_cost.checked_add(cost)?;
        if !selected.contains(&roots[id]) {
            continue;
        }
        selected_count += 1;
        selected_cost = selected_cost.checked_add(cost)?;
        if selected_count > 256 || selected_cost > 4096 {
            return None;
        }
        for callee in &definition.calls {
            remaining = remaining.checked_sub(1)?;
            if let Some(&callee) = names.get(callee.as_str())
                && !selected.contains(&roots[callee])
            {
                // Do not pull a dynamic caller's ordinary leaf implementations
                // into this group. The original complete importer stays intact.
                return None;
            }
        }
    }
    (selected_cost.checked_mul(16)? <= total_cost).then_some(selected)
}

#[cfg(test)]
pub(crate) fn partition(module: Module, lir: &LirModule, entry: EntryPoint, count: usize) -> Plan {
    partition_view(module, &lir.as_view(), entry, count, false, false)
}
pub(crate) fn partition_view(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
) -> Plan {
    let fallback = |module, reason| Plan {
        units: vec![module],
        fallback: Some(reason),
    };
    if count < 2 {
        return Plan {
            units: vec![module],
            fallback: None,
        };
    }
    let mut defs = Vec::new();
    let mut prelude = Vec::new();
    let mut symbols = BTreeSet::<&str>::new();
    for (index, item) in module.items.iter().enumerate() {
        // Removing declarations must not hide a collision with a definition in
        // a different unit. Leave the original module to the ordinary validator.
        let symbol = match item {
            Item::Function(f) => Some(&f.name),
            Item::Global(g) => Some(&g.name),
            Item::NamedType(..) => None,
        };
        if let Some(symbol) = symbol
            && !symbols.insert(symbol.as_str())
        {
            return fallback(module, "duplicate machine definition or declaration");
        }
        match item {
            Item::Function(f) if f.blocks.is_some() => defs.push(Definition {
                name: f.name.clone(),
                index,
                local: f.linkage.is_local(),
                refs: f.references(),
                calls: if import_complete_call_closures {
                    direct_calls(f)
                } else {
                    BTreeSet::new()
                },
            }),
            Item::Global(g) if g.initializer.is_some() => {
                let mut refs = BTreeSet::new();
                if let Some(value) = &g.initializer {
                    value.symbols(&mut refs);
                }
                defs.push(Definition {
                    name: g.name.clone(),
                    index,
                    local: g.linkage.is_local(),
                    refs,
                    calls: BTreeSet::new(),
                });
            }
            _ => prelude.push(index),
        }
    }
    let mut names = BTreeMap::new();
    for (id, d) in defs.iter().enumerate() {
        if names.insert(d.name.as_str(), id).is_some() {
            return fallback(module, "duplicate machine definition");
        }
    }
    let mut sets = Sets((0..defs.len()).collect());
    // File boundaries are not linkage boundaries. Fixed-size O0 chunks make
    // body-only edits local without moving unaffected chunks as the body grows.
    // The explicit O2 strategy also copies complete call implementations below.
    const FUNCTIONS_PER_CHUNK: usize = 256;
    let mut ordinals = BTreeMap::<FileId, usize>::new();
    let mut source_ids = Vec::new();
    let mut entry_id = None;
    for function in lir.functions {
        if function.foreign.is_some() {
            continue;
        }
        let name = format!("zil.fn.{}", function.id.0);
        let Some(&id) = names.get(name.as_str()) else {
            return fallback(module, "source grouping missing machine definition");
        };
        let ordinal = ordinals.entry(function.span.file).or_default();
        let chunk = if split_large_files {
            *ordinal / FUNCTIONS_PER_CHUNK
        } else {
            0
        };
        *ordinal += 1;
        source_ids.push(((function.span.file.0, chunk), id));
        let selected = match entry {
            EntryPoint::Main => function.name == "main",
            EntryPoint::ReturnValue(wanted) => function.id == wanted,
            EntryPoint::None => false,
        };
        if selected {
            entry_id = Some(id);
            if let Some(&wrapper) = names.get("main") {
                sets.join(id, wrapper);
            }
        }
    }
    if let (Some(&main), Some(&pe)) = (names.get("main"), names.get("zil.pe.entry")) {
        sets.join(main, pe);
    }
    for (id, d) in defs.iter().enumerate() {
        for name in &d.refs {
            if let Some(&target) = names.get(name.as_str())
                && defs[target].local
            {
                sets.join(id, target);
            }
        }
    }
    // Determine mandatory local-state components before optional source chunk
    // grouping. A component belongs wholly to the caller group or wholly outside.
    let local_roots = (0..defs.len()).map(|id| sets.root(id)).collect::<Vec<_>>();
    let caller_components = if split_large_files && import_complete_call_closures {
        entry_id.and_then(|entry| {
            small_caller_components(&module, lir, &defs, &names, &local_roots, Some(entry))
        })
    } else {
        None
    };
    let mut files = BTreeMap::new();
    for &(group, id) in &source_ids {
        if let Some(&first) = files.get(&group) {
            sets.join(first, id);
        } else {
            files.insert(group, id);
        }
    }
    let roots = (0..defs.len()).map(|id| sets.root(id)).collect::<Vec<_>>();
    let mut source_keys = BTreeMap::<usize, (u32, usize)>::new();
    for (file, id) in source_ids {
        source_keys
            .entry(roots[id])
            .and_modify(|key| *key = (*key).min(file))
            .or_insert(file);
    }
    let mut buckets = BTreeMap::new();
    for (id, d) in defs.iter().enumerate() {
        buckets.entry(roots[id]).or_insert_with(|| {
            source_keys.get(&roots[id]).map_or_else(
                || {
                    let hash = Fingerprint::of(d.name.as_bytes()).0;
                    u64::from_le_bytes(hash[..8].try_into().expect("eight bytes")) as usize % count
                },
                |(file, chunk)| (*file as usize % count + chunk % count) % count,
            )
        });
    }
    let mut owners = roots.iter().map(|r| buckets[r]).collect::<Vec<_>>();
    if let (Some(callers), Some(entry)) = (caller_components, entry_id) {
        let entry_owner = owners[entry];
        // Keep every ordinary definition's original owner, including chunks
        // joined indirectly by local state. Only the small caller components
        // move, together, to the existing entry bucket; no bucket is reserved.
        for (id, owner) in owners.iter_mut().enumerate() {
            if callers.contains(&local_roots[id]) {
                *owner = entry_owner;
            }
        }
    }
    let mut members = BTreeMap::<usize, Vec<usize>>::new();
    for (id, &owner) in owners.iter().enumerate() {
        members.entry(owner).or_default().push(id);
    }
    if members.len() <= 1 {
        return fallback(module, "one linkage-connected source group");
    }
    // Some foreign/global declarations will no longer reach any native unit.
    // Validate this declaration-only view once before filtering, so unused
    // malformed types, ABI attributes or local declarations still fail. It has
    // no function bodies and does not repeat whole-program SSA preflight.
    // Named types remain in every unit in this first dependency-closure slice.
    if prelude
        .iter()
        .any(|&index| !matches!(module.items[index], Item::NamedType(..)))
    {
        let declarations = Module {
            triple: module.triple.clone(),
            fingerprint: module.fingerprint.clone(),
            items: prelude
                .iter()
                .map(|&index| module.items[index].clone())
                .collect(),
        };
        if zil_llvm_api::validate_module(&declarations).is_err() {
            return fallback(module, "invalid machine declaration prelude");
        }
    }
    // Import a complete direct-call closure, not a definition-ID prefix. Mere
    // function addresses retain canonical declarations without copying a body.
    // Every copy becomes a real private implementation below: its exact body
    // may participate in non-inlining IPO and must be emitted if not inlined.
    // Extra copies may consume at most one source graph's instruction/block
    // cost; exceeding that whole-plan guard falls back without truncation.
    let mut imports = BTreeMap::<usize, BTreeSet<usize>>::new();
    if import_complete_call_closures {
        let source_functions = lir
            .functions
            .iter()
            .filter(|f| f.foreign.is_none())
            .map(|f| format!("zil.fn.{}", f.id.0))
            .collect::<BTreeSet<_>>();
        let original_cost = source_functions
            .iter()
            .filter_map(|name| names.get(name.as_str()))
            .filter_map(|&id| match &module.items[defs[id].index] {
                Item::Function(function) => Some(function.cost()),
                _ => None,
            })
            .sum::<usize>();
        let mut copied_cost = 0usize;
        for (&owner, definitions) in &members {
            let mut pending = definitions
                .iter()
                .flat_map(|&id| defs[id].calls.iter())
                .filter_map(|name| names.get(name.as_str()).copied())
                .collect::<BTreeSet<_>>();
            let mut seen = BTreeSet::new();
            let mut imported = BTreeSet::new();
            while let Some(id) = pending.pop_first() {
                if owners[id] == owner || !seen.insert(id) {
                    continue;
                }
                if !source_functions.contains(&defs[id].name) {
                    return fallback(
                        module,
                        "complete O2 call closure includes a compiler-owned definition",
                    );
                }
                let Item::Function(function) = &module.items[defs[id].index] else {
                    continue;
                };
                if function.linkage != zil_llvm_api::model::Linkage::External
                    || defs[id]
                        .refs
                        .iter()
                        .any(|name| names.get(name.as_str()).is_some_and(|&id| defs[id].local))
                {
                    // Copying a local global would duplicate observable state;
                    // referencing it from another unit would lose its identity.
                    // Preserve the original complete module until a grouping
                    // policy can represent this closure without either change.
                    return fallback(
                        module,
                        "complete O2 call closure requires a local definition",
                    );
                }
                let Some(total) = copied_cost.checked_add(function.cost()) else {
                    return fallback(module, "complete O2 call closure exceeds total copy cost");
                };
                if total > original_cost {
                    return fallback(module, "complete O2 call closure exceeds total copy cost");
                }
                copied_cost = total;
                imported.insert(id);
                pending.extend(
                    defs[id]
                        .calls
                        .iter()
                        .filter_map(|name| names.get(name.as_str()).copied()),
                );
            }
            imports.insert(owner, imported);
        }
    }
    let mut prototypes = BTreeMap::new();
    let mut uses_left = vec![1usize; defs.len()];
    for (&owner, definitions) in &members {
        let imported = imports.entry(owner).or_default();
        for &id in definitions.iter().chain(imported.iter()) {
            for symbol in &defs[id].refs {
                if let Some(&dependency) = names.get(symbol.as_str())
                    && owners[dependency] != owner
                    && !imported.contains(&dependency)
                {
                    prototypes.entry(dependency).or_insert_with(|| {
                        match &module.items[defs[dependency].index] {
                            Item::Function(f) => Item::Function(f.declaration()),
                            Item::Global(g) => Item::Global(g.declaration()),
                            Item::NamedType(..) => unreachable!(),
                        }
                    });
                }
            }
        }
        for &id in imported.iter() {
            uses_left[id] += 1;
            let clone_name = format!("zil.import.{owner}.{}", defs[id].name);
            if symbols.contains(clone_name.as_str()) {
                return fallback(module, "O2 implementation clone symbol collision");
            }
            prototypes.entry(id).or_insert_with(|| {
                let Item::Function(function) = &module.items[defs[id].index] else {
                    unreachable!()
                };
                Item::Function(function.declaration())
            });
        }
    }
    let mut original = module.items.into_iter().map(Some).collect::<Vec<_>>();
    let prelude = prelude
        .into_iter()
        .map(|index| original[index].take().expect("unique prelude item"))
        .collect::<Vec<_>>();
    let mut units = Vec::with_capacity(members.len());
    for (owner, definitions) in members {
        let imported = &imports[&owner];
        let clone_names = imported
            .iter()
            .map(|&id| {
                (
                    defs[id].name.clone(),
                    format!("zil.import.{owner}.{}", defs[id].name),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let referenced = definitions
            .iter()
            .chain(imported.iter())
            .flat_map(|&id| defs[id].refs.iter())
            .collect::<BTreeSet<_>>();
        let mut items = prelude
            .iter()
            .filter(|item| match item {
                Item::NamedType(..) => true,
                Item::Function(f) => referenced.contains(&f.name),
                Item::Global(g) => referenced.contains(&g.name),
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut needed = BTreeSet::new();
        for &id in definitions.iter().chain(imported.iter()) {
            needed.extend(
                defs[id]
                    .refs
                    .iter()
                    .filter_map(|name| names.get(name.as_str()).copied()),
            );
        }
        for id in needed {
            if owners[id] != owner && !imported.contains(&id) {
                debug_assert!(!defs[id].local);
                items.push(prototypes[&id].clone());
            }
        }
        for &id in imported {
            // Address operands and callback arguments must still resolve to
            // the canonical owner even when the direct callee is a clone.
            items.push(prototypes[&id].clone());
            let Item::Function(mut function) =
                distribute_item(&mut original[defs[id].index], &mut uses_left[id])
            else {
                unreachable!()
            };
            function.linkage = zil_llvm_api::model::Linkage::Internal;
            function.name = clone_names[&defs[id].name].clone();
            rewrite_direct_calls(&mut function, &clone_names);
            items.push(Item::Function(function));
        }
        items.extend(definitions.iter().map(|&id| {
            let mut item = distribute_item(&mut original[defs[id].index], &mut uses_left[id]);
            if let Item::Function(function) = &mut item {
                rewrite_direct_calls(function, &clone_names);
            }
            item
        }));
        units.push(Module {
            triple: module.triple.clone(),
            fingerprint: module.fingerprint.clone(),
            items,
        });
    }
    Plan {
        units,
        fallback: None,
    }
}

fn distribute_item(original: &mut Option<Item>, remaining: &mut usize) -> Item {
    assert!(
        *remaining > 0,
        "definition distribution has a remaining consumer"
    );
    *remaining -= 1;
    if *remaining == 0 {
        original.take().expect("definition has one canonical owner")
    } else {
        original
            .as_ref()
            .expect("definition has future consumers")
            .clone()
    }
}

fn direct_calls(function: &Function) -> BTreeSet<String> {
    function
        .blocks
        .iter()
        .flatten()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.op {
            Op::Call {
                callee: Val::Global(name),
                ..
            } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn rewrite_direct_calls(function: &mut Function, names: &BTreeMap<String, String>) {
    if names.is_empty() {
        return;
    }
    for instruction in function
        .blocks
        .iter_mut()
        .flatten()
        .flat_map(|block| &mut block.instructions)
    {
        if let Op::Call {
            callee: Val::Global(name),
            ..
        } = &mut instruction.op
            && let Some(clone) = names.get(name)
        {
            *name = clone.clone();
        }
    }
}

/// Called only by the explicit closed Linux executable build path, after the
/// ordinary partition. Changing linkage before partition would merge local
/// call-graph edges and discard available codegen parallelism.
pub(crate) fn internalize_closed_executable(units: &mut [Module], lir: &LirView<'_>) -> usize {
    let candidates = lir
        .functions
        .iter()
        .filter(|function| function.foreign.is_none())
        .map(|function| format!("zil.fn.{}", function.id.0))
        .collect::<BTreeSet<_>>();
    internalize_closed_executable_names(units, &candidates)
}

pub(crate) fn internalize_closed_executable_names(
    units: &mut [Module],
    candidates: &BTreeSet<String>,
) -> usize {
    let mut owners = BTreeMap::new();
    for (unit, module) in units.iter().enumerate() {
        for item in &module.items {
            if let Item::Function(function) = item
                && function.blocks.is_some()
                && function.linkage != zil_llvm_api::model::Linkage::AvailableExternally
                && candidates.contains(&function.name)
                && owners.insert(function.name.as_str(), unit).is_some()
            {
                // Leave malformed duplicate definitions unchanged for the
                // existing mandatory machine preflight to reject.
                return 0;
            }
        }
    }
    let mut cross_unit = BTreeSet::new();
    if units.len() > 1 {
        for (unit, module) in units.iter().enumerate() {
            for item in &module.items {
                let references = match item {
                    Item::Function(function) => function.references(),
                    Item::Global(global) => {
                        let mut references = BTreeSet::new();
                        if let Some(value) = &global.initializer {
                            value.symbols(&mut references);
                        }
                        references
                    }
                    Item::NamedType(..) => continue,
                };
                for reference in references {
                    if owners
                        .get(reference.as_str())
                        .is_some_and(|&owner| owner != unit)
                    {
                        cross_unit.insert(reference);
                    }
                }
            }
        }
    }
    let mut changed = 0;
    for module in units {
        for item in &mut module.items {
            if let Item::Function(function) = item
                && function.blocks.is_some()
                && function.linkage == zil_llvm_api::model::Linkage::External
                && candidates.contains(&function.name)
                && !cross_unit.contains(&function.name)
            {
                function.linkage = zil_llvm_api::model::Linkage::Internal;
                changed += 1;
            }
        }
    }
    changed
}

struct Sets(Vec<usize>);
impl Sets {
    fn root(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        self.0[a.max(b)] = a.min(b);
    }
}

#[cfg(all(
    test,
    feature = "llvm-api",
    target_os = "linux",
    target_arch = "x86_64"
))]
#[path = "machine_cgu_clone_controls.rs"]
mod clone_controls;

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::*;
    fn lir() -> LirModule {
        LirModule {
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: zil_base::ModuleId(0),
            verified_fingerprint: Fingerprint::ZERO,
            target: zil_base::TargetProfile::host_llvm(),
            functions: vec![],
            resources: vec![],
            closures: vec![],
            layouts: vec![],
        }
    }
    fn f(name: &str, result: Ty, value: Val) -> Item {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(result.clone(), vec![]),
            attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions: vec![Inst::new(None, Op::Return(Some(Arg::new(result, value))))],
            }]),
        })
    }
    fn m(items: Vec<Item>) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "test".into(),
            items,
        }
    }
    fn call_function(name: &str, result: Ty, target: &str) -> Item {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(result.clone(), vec![]),
            attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions: vec![
                    Inst::new(
                        Some("value".into()),
                        Op::Call {
                            result: result.clone(),
                            result_attrs: vec![],
                            callee: Val::Global(target.into()),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    Inst::new(
                        None,
                        Op::Return(Some(Arg::new(result, Val::Local("value".into())))),
                    ),
                ],
            }]),
        })
    }
    fn definitions(m: &Module) -> BTreeSet<&str> {
        m.items
            .iter()
            .filter_map(|i| match i {
                Item::Function(f) if f.blocks.is_some() => Some(f.name.as_str()),
                Item::Global(g) if g.initializer.is_some() => Some(g.name.as_str()),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn large_file_chunks_still_union_shared_local_addresses() {
        // LIR identities are grouping inputs only in this machine-model test.
        let mut source = lir();
        for id in 0..600 {
            source.functions.push(zil_ir::TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                id: zil_ir::FunctionId(id),
                name: format!("f{id}"),
                foreign: None,
                environment: None,
                parameters: vec![],
                return_type: zil_ir::Type::I32,
                result_mode: zil_ir::PassingMode::Infer,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: zil_ir::BlockId(0),
                span: zil_base::Span::default(),
            });
        }
        let mut items = (0..600)
            .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(0)))
            .collect::<Vec<_>>();
        for id in [0, 256] {
            items[id] = f(
                &format!("zil.fn.{id}"),
                Ty::Ptr,
                Val::Global("shared".into()),
            );
        }
        items.push(Item::Global(Global {
            name: "shared".into(),
            linkage: Linkage::Private,
            ty: Ty::Int(32),
            initializer: Some(Val::Int(7)),
            constant: false,
            unnamed_addr: false,
            alignment: 4,
        }));
        let module = m(items);
        let split = partition_view(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            8,
            true,
            false,
        );
        assert_eq!(split.units.len(), 2);
        let shared = split
            .units
            .iter()
            .find(|m| definitions(m).contains("shared"))
            .unwrap();
        assert!(definitions(shared).contains("zil.fn.0"));
        assert!(definitions(shared).contains("zil.fn.256"));
        assert!(!definitions(shared).contains("zil.fn.512"));
        for unit in &split.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
        let original = partition_view(module, &source.as_view(), EntryPoint::None, 8, false, false);
        assert_eq!(original.units.len(), 1);
    }
    #[test]
    fn small_source_callers_keep_local_components_and_dynamic_fallback() {
        // Typed calls provide grouping metadata here; only the current machine
        // models are validated. Real source acceptance belongs to driver tests.
        let mut source = candidate_lir(&(0..600).collect::<Vec<_>>());
        source.functions[0].name = "main".into();
        for (caller, callee) in [(0, 256), (256, 512)] {
            source.functions[caller].blocks = vec![zil_ir::BasicBlock {
                id: zil_ir::BlockId(0),
                instructions: vec![zil_ir::Instruction {
                    result: Some(zil_ir::ValueId(0)),
                    kind: zil_ir::InstructionKind::Call {
                        function: zil_ir::FunctionId(callee),
                        arguments: vec![],
                    },
                    span: zil_base::Span::default(),
                }],
                terminator: zil_ir::Terminator::Return(Some(zil_ir::ValueId(0))),
            }];
        }
        let mut items = (0..600)
            .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(7)))
            .collect::<Vec<_>>();
        items[256] = call_function("zil.fn.256", Ty::Int(32), "zil.fn.512");
        let Item::Function(local) = &mut items[512] else {
            unreachable!()
        };
        local.linkage = Linkage::Internal;
        items.push(call_function("main", Ty::Int(32), "zil.fn.0"));
        let plan = partition_view(
            m(items.clone()),
            &source.as_view(),
            EntryPoint::Main,
            8,
            true,
            true,
        );
        assert!(plan.fallback.is_none());
        assert_eq!(plan.units.len(), 2);
        let entry = plan
            .units
            .iter()
            .find(|unit| definitions(unit).contains("main"))
            .unwrap();
        for name in ["zil.fn.0", "zil.fn.256", "zil.fn.512"] {
            assert!(definitions(entry).contains(name));
        }
        assert!(!definitions(entry).contains("zil.fn.257"));
        assert_eq!(function_linkage(entry, "zil.fn.256"), Linkage::External);
        assert_eq!(function_linkage(entry, "zil.fn.512"), Linkage::Internal);
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }

        // A remaining call to an ordinary leaf declines the new policy. The
        // exact original partition/import model, including its keys, survives.
        items[256] = call_function("zil.fn.256", Ty::Int(32), "zil.fn.1");
        let dynamic = partition_view(
            m(items.clone()),
            &source.as_view(),
            EntryPoint::Main,
            8,
            true,
            true,
        );
        for function in &mut source.functions {
            function.blocks.clear();
        }
        let original = partition_view(m(items), &source.as_view(), EntryPoint::Main, 8, true, true);
        assert_eq!(dynamic.fallback, original.fallback);
        assert_eq!(dynamic.units, original.units);
        for unit in &dynamic.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
    }
    #[test]
    fn o2_imports_follow_transitive_bodies_and_keep_unique_emitted_owners() {
        let source = candidate_lir(&(0..900).collect::<Vec<_>>());
        let mut items = (0..900)
            .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(7)))
            .collect::<Vec<_>>();
        items[0] = call_function("zil.fn.0", Ty::Int(32), "zil.fn.256");
        items[256] = call_function("zil.fn.256", Ty::Int(32), "zil.fn.512");
        let plan = partition_view(
            m(items.clone()),
            &source.as_view(),
            EntryPoint::None,
            8,
            true,
            true,
        );
        assert_eq!(plan.units.len(), 4);
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
        let first = &plan.units[0];
        assert_eq!(function_linkage(first, "zil.fn.0"), Linkage::External);
        assert_eq!(
            function_linkage(first, "zil.import.0.zil.fn.256"),
            Linkage::Internal
        );
        assert_eq!(
            function_linkage(first, "zil.import.0.zil.fn.512"),
            Linkage::Internal
        );
        for id in 0..900 {
            assert_eq!(plan.units.iter().flat_map(|u| &u.items).filter(|item| matches!(item, Item::Function(f) if f.name == format!("zil.fn.{id}") && f.blocks.is_some() && f.linkage != Linkage::AvailableExternally)).count(), 1);
        }
        // Imported bodies affect exactly the units which consume them; unrelated
        // owner units retain the same machine cache input.
        items[512] = f("zil.fn.512", Ty::Int(32), Val::Int(9));
        let edited = partition_view(m(items), &source.as_view(), EntryPoint::None, 8, true, true);
        assert_ne!(plan.units[0].items, edited.units[0].items);
        assert_ne!(plan.units[1].items, edited.units[1].items);
        assert_ne!(plan.units[2].items, edited.units[2].items);
        assert_eq!(plan.units[3].items, edited.units[3].items);
        let mut closed = plan.units;
        let changed = internalize_closed_executable(&mut closed, &source.as_view());
        assert!(changed > 0);
        assert_eq!(
            function_linkage(&closed[0], "zil.import.0.zil.fn.256"),
            Linkage::Internal
        );
        assert_eq!(
            function_linkage(&closed[1], "zil.fn.256"),
            Linkage::Internal
        );
        for unit in &closed {
            zil_llvm_api::validate_module(unit).unwrap();
        }
    }

    #[test]
    fn o2_imports_preserve_local_identity_and_import_external_global_declarations() {
        let source = candidate_lir(&(0..600).collect::<Vec<_>>());
        let mut items = (0..600)
            .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(7)))
            .collect::<Vec<_>>();
        items[0] = call_function("zil.fn.0", Ty::Ptr, "zil.fn.256");
        items[256] = f("zil.fn.256", Ty::Ptr, Val::Global("hidden".into()));
        items.push(Item::Global(Global {
            name: "hidden".into(),
            linkage: Linkage::Private,
            ty: Ty::Int(32),
            initializer: Some(Val::Int(7)),
            constant: false,
            unnamed_addr: false,
            alignment: 4,
        }));
        let plan = partition_view(
            m(items.clone()),
            &source.as_view(),
            EntryPoint::None,
            8,
            true,
            true,
        );
        assert_eq!(plan.units.len(), 1);
        assert_eq!(
            plan.fallback,
            Some("complete O2 call closure requires a local definition")
        );
        assert_eq!(plan.units[0].items, items);
        let Item::Global(global) = items.last_mut().unwrap() else {
            panic!()
        };
        global.linkage = Linkage::External;
        let plan = partition_view(m(items), &source.as_view(), EntryPoint::None, 8, true, true);
        assert_eq!(
            function_linkage(&plan.units[0], "zil.import.0.zil.fn.256"),
            Linkage::Internal
        );
        assert!(
            plan.units[0]
                .items
                .iter()
                .any(|item| matches!(item, Item::Global(g) if g.name == "hidden"))
        );
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
    }

    #[test]
    fn address_only_references_never_copy_implementations() {
        let source = candidate_lir(&(0..600).collect::<Vec<_>>());
        let mut items = (0..600)
            .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(7)))
            .collect::<Vec<_>>();
        items[0] = f("zil.fn.0", Ty::Ptr, Val::Global("zil.fn.256".into()));
        let plan = partition_view(m(items), &source.as_view(), EntryPoint::None, 8, true, true);
        assert_eq!(plan.units.len(), 3);
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
            assert!(unit.items.iter().all(
                |item| !matches!(item, Item::Function(f) if f.name.starts_with("zil.import."))
            ));
        }
        assert_eq!(
            function_linkage(&plan.units[0], "zil.fn.256"),
            Linkage::External
        );
        assert!(!definitions(&plan.units[0]).contains("zil.fn.256"));
    }

    #[test]
    fn large_transitive_closure_is_complete_and_keeps_canonical_arguments() {
        let source = candidate_lir(&(0..512).collect::<Vec<_>>());
        let mut items = (0..512)
            .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(7)))
            .collect::<Vec<_>>();
        items[0] = call_function("zil.fn.0", Ty::Int(32), "zil.fn.256");
        for (id, item) in items.iter_mut().enumerate().take(511).skip(256) {
            *item = call_function(
                &format!("zil.fn.{id}"),
                Ty::Int(32),
                &format!("zil.fn.{}", id + 1),
            );
            let Item::Function(function) = item else {
                unreachable!()
            };
            let body = &mut function.blocks.as_mut().unwrap()[0].instructions;
            let mut prefix = (0..300)
                .map(|number| {
                    Inst::new(
                        Some(format!("unused.{number}")),
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: Val::Int(1),
                            right: Val::Int(2),
                        },
                    )
                })
                .collect::<Vec<_>>();
            prefix.append(body);
            *body = prefix;
        }
        let plan = partition_view(m(items), &source.as_view(), EntryPoint::None, 8, true, true);
        assert_eq!(plan.units.len(), 2);
        assert!(plan.fallback.is_none());
        let first = &plan.units[0];
        assert_eq!(first.items.iter().filter(|item| matches!(item, Item::Function(f) if f.name.starts_with("zil.import.0.") && f.blocks.is_some())).count(), 256);
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
        // Only the direct callee operand is rewritten, even when the same
        // function symbol also appears as an argument or a returned address.
        let mut function = Function {
            name: "caller".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Ptr, vec![]),
            attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions: vec![
                    Inst::new(
                        Some("result".into()),
                        Op::Call {
                            result: Ty::Ptr,
                            result_attrs: vec![],
                            callee: Val::Global("f".into()),
                            args: vec![Arg::new(Ty::Ptr, Val::Global("f".into()))],
                            vararg: None,
                        },
                    ),
                    Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Ptr, Val::Global("f".into())))),
                    ),
                ],
            }]),
        };
        rewrite_direct_calls(
            &mut function,
            &BTreeMap::from([("f".into(), "private.f".into())]),
        );
        let instructions = &function.blocks.as_ref().unwrap()[0].instructions;
        let Op::Call { callee, args, .. } = &instructions[0].op else {
            unreachable!()
        };
        assert_eq!(callee, &Val::Global("private.f".into()));
        assert_eq!(args[0].value, Val::Global("f".into()));
        assert_eq!(
            instructions[1].op,
            Op::Return(Some(Arg::new(Ty::Ptr, Val::Global("f".into()))))
        );
    }

    #[test]
    fn multiply_shared_closure_falls_back_before_copying_any_body() {
        let source = candidate_lir(&(0..600).collect::<Vec<_>>());
        let module = m((0..600)
            .map(|id| {
                call_function(
                    &format!("zil.fn.{id}"),
                    Ty::Int(32),
                    &format!("zil.fn.{}", (id + 1) % 600),
                )
            })
            .collect());
        let plan = partition_view(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            8,
            true,
            true,
        );
        assert_eq!(
            plan.fallback,
            Some("complete O2 call closure exceeds total copy cost")
        );
        assert_eq!(plan.units, vec![module]);
        zil_llvm_api::validate_module(&plan.units[0]).unwrap();
    }

    #[test]
    fn canonical_owner_can_be_first_middle_or_last_consumer() {
        for target in [0, 256, 512] {
            let source = candidate_lir(&(0..768).collect::<Vec<_>>());
            let mut items = (0..768)
                .map(|id| f(&format!("zil.fn.{id}"), Ty::Int(32), Val::Int(7)))
                .collect::<Vec<_>>();
            let target_name = format!("zil.fn.{target}");
            items[target] = f(&target_name, Ty::Ptr, Val::Global(target_name.clone()));
            let canonical = items[target].clone();
            for caller in [0, 256, 512] {
                if caller != target {
                    items[caller] =
                        call_function(&format!("zil.fn.{caller}"), Ty::Ptr, &target_name);
                }
            }
            let plan = partition_view(m(items), &source.as_view(), EntryPoint::None, 8, true, true);
            assert_eq!(plan.units.len(), 3);
            assert!(plan.fallback.is_none());
            for (owner, unit) in plan.units.iter().enumerate() {
                zil_llvm_api::validate_module(unit).unwrap();
                if owner == target / 256 {
                    assert!(unit.items.contains(&canonical));
                } else {
                    let name = format!("zil.import.{owner}.{target_name}");
                    let Item::Function(clone) = unit
                        .items
                        .iter()
                        .find(|item| matches!(item, Item::Function(f) if f.name == name))
                        .unwrap()
                    else {
                        unreachable!()
                    };
                    assert_eq!(clone.linkage, Linkage::Internal);
                    assert!(clone.references().contains(&target_name));
                    assert!(
                        matches!(&clone.blocks.as_ref().unwrap()[0].instructions[0].op, Op::Return(Some(arg)) if arg.value == Val::Global(target_name.clone()))
                    );
                }
            }
        }
    }

    #[test]
    fn local_identity_and_global_initializers_are_colocated_once() {
        let mut hidden = f("hidden", Ty::Int(32), Val::Int(3));
        if let Item::Function(f) = &mut hidden {
            f.linkage = Linkage::Internal;
        }
        let module = m(vec![
            hidden,
            Item::Global(Global {
                name: "table".into(),
                linkage: Linkage::Private,
                ty: Ty::array(1, Ty::Ptr),
                initializer: Some(Val::Array(vec![Arg::new(
                    Ty::Ptr,
                    Val::Global("hidden".into()),
                )])),
                constant: true,
                unnamed_addr: false,
                alignment: 8,
            }),
            f("left", Ty::Ptr, Val::Global("table".into())),
            f("right", Ty::Ptr, Val::Global("hidden".into())),
            f("independent", Ty::Int(32), Val::Int(7)),
        ]);
        let plan = partition(module.clone(), &lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        let shared = plan
            .units
            .iter()
            .find(|m| definitions(m).contains("hidden"))
            .unwrap();
        for n in ["hidden", "table", "left", "right"] {
            assert!(definitions(shared).contains(n));
        }
        for n in definitions(&module) {
            assert_eq!(
                plan.units
                    .iter()
                    .filter(|m| definitions(m).contains(n))
                    .count(),
                1
            );
        }
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
    }
    #[test]
    fn external_uses_import_only_immediate_declarations() {
        let module = m(vec![
            f("a", Ty::Ptr, Val::Global("b".into())),
            f("b", Ty::Ptr, Val::Global("c".into())),
            f("c", Ty::Int(32), Val::Int(7)),
            f("unused", Ty::Int(32), Val::Int(9)),
        ]);
        let plan = partition(module.clone(), &lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        for unit in &plan.units {
            let definitions = definitions(unit);
            let refs = unit
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Function(f) if f.blocks.is_some() => Some(f.references()),
                    _ => None,
                })
                .flatten()
                .collect::<BTreeSet<_>>();
            for item in &unit.items {
                if let Item::Function(f) = item
                    && f.blocks.is_none()
                {
                    assert!(refs.contains(&f.name));
                    assert!(!definitions.contains(f.name.as_str()));
                }
            }
            zil_llvm_api::validate_module(unit).unwrap();
        }
        assert_eq!(
            plan.units
                .iter()
                .filter(|m| definitions(m).contains("unused"))
                .count(),
            1
        );
    }
    #[test]
    fn foreign_addresses_and_global_initializers_keep_only_consumed_declarations() {
        let declaration = |name: &str| {
            let Item::Function(function) = f(name, Ty::Int(32), Val::Int(0)) else {
                unreachable!()
            };
            Item::Function(function.declaration())
        };
        let mut foreign = declaration("foreign");
        if let Item::Function(f) = &mut foreign {
            f.signature.result = Ty::Int(8);
            f.signature.result_attrs.push(Attr::ZeroExt);
            f.attrs.push(Attr::NonLazyBind);
        }
        let global = Item::Global(Global {
            name: "external_state".into(),
            linkage: Linkage::External,
            ty: Ty::Named("State".into()),
            initializer: None,
            constant: false,
            unnamed_addr: false,
            alignment: 8,
        });
        let module = m(vec![
            Item::NamedType("State".into(), vec![Ty::Int(64)]),
            foreign.clone(),
            declaration("unused"),
            global.clone(),
            Item::Global(Global {
                name: "table".into(),
                linkage: Linkage::Private,
                ty: Ty::array(1, Ty::Ptr),
                initializer: Some(Val::Array(vec![Arg::new(
                    Ty::Ptr,
                    Val::Global("foreign".into()),
                )])),
                constant: true,
                unnamed_addr: false,
                alignment: 8,
            }),
            f("left", Ty::Ptr, Val::Global("table".into())),
            f("right", Ty::Ptr, Val::Global("external_state".into())),
            f("independent", Ty::Int(32), Val::Int(7)),
        ]);
        let plan = partition(module, &lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
            let defined = definitions(unit);
            assert_eq!(unit.items.contains(&foreign), defined.contains("table"));
            assert_eq!(unit.items.contains(&global), defined.contains("right"));
            assert!(
                !unit
                    .items
                    .iter()
                    .any(|i| matches!(i, Item::Function(f) if f.name == "unused"))
            );
        }
    }
    #[test]
    fn filtering_cannot_hide_invalid_unused_declarations_or_cross_unit_collisions() {
        let Item::Function(function) = f("unused", Ty::Int(32), Val::Int(0)) else {
            unreachable!()
        };
        let valid = function.declaration();
        let mut local = valid.clone();
        local.linkage = Linkage::Internal;
        let mut void_param = valid.clone();
        void_param.signature.params.push(Param::unnamed(Ty::Void));
        let mut bad_attr = valid.clone();
        bad_attr.signature.result = Ty::Ptr;
        bad_attr.signature.result_attrs.push(Attr::ZeroExt);
        let mut unknown_type = valid.clone();
        unknown_type.signature.result = Ty::Named("missing".into());
        let mut collision = valid;
        collision.name = "left".into();
        let invalid_global = Item::Global(Global {
            name: "unused_global".into(),
            linkage: Linkage::External,
            ty: Ty::Int(32),
            initializer: None,
            constant: false,
            unnamed_addr: false,
            alignment: 3,
        });
        for invalid in [local, void_param, bad_attr, unknown_type, collision]
            .into_iter()
            .map(Item::Function)
            .chain([invalid_global])
        {
            let module = m(vec![
                invalid,
                f("left", Ty::Int(32), Val::Int(1)),
                f("right", Ty::Int(32), Val::Int(2)),
            ]);
            assert!(zil_llvm_api::validate_module(&module).is_err());
            let plan = partition(module.clone(), &lir(), EntryPoint::None, 64);
            assert!(plan.fallback.is_some());
            assert_eq!(plan.units, vec![module]);
            assert!(zil_llvm_api::validate_module(&plan.units[0]).is_err());
        }
    }
    #[test]
    fn local_component_falls_back_without_copying_and_duplicates_remain_invalid() {
        let mut h = f("hidden", Ty::Int(32), Val::Int(7));
        if let Item::Function(f) = &mut h {
            f.linkage = Linkage::Private;
        }
        let module = m(vec![h, f("address", Ty::Ptr, Val::Global("hidden".into()))]);
        assert_eq!(
            partition(module.clone(), &lir(), EntryPoint::None, 64)
                .units
                .len(),
            1
        );
        let bad = m(vec![
            f("same", Ty::Int(32), Val::Int(1)),
            f("same", Ty::Int(32), Val::Int(2)),
        ]);
        assert!(
            partition(bad.clone(), &lir(), EntryPoint::None, 64)
                .fallback
                .unwrap()
                .contains("duplicate")
        );
        assert!(zil_llvm_api::validate_module(&bad).is_err());
    }
    // Raw machine/LIR identity controls, not source-language acceptance tests.
    fn candidate_lir(ids: &[u32]) -> LirModule {
        let mut module = lir();
        module.functions = ids
            .iter()
            .map(|&id| zil_ir::TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: zil_ir::FunctionId(id),
                name: format!("source_name_{id}"),
                environment: None,
                parameters: vec![],
                return_type: zil_ir::Type::I32,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: zil_ir::BlockId(0),
                span: zil_base::Span::default(),
            })
            .collect();
        module
    }

    fn function_linkage(module: &Module, name: &str) -> Linkage {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name == name => Some(function.linkage),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn closed_policy_uses_exact_nonforeign_identity_and_keeps_other_items() {
        let mut lir = candidate_lir(&[0, 1]);
        lir.functions[1].foreign = Some(zil_ir::ForeignDeclaration {
            abi: "C".into(),
            symbol: "foreign".into(),
            noescape_parameters: vec![],
            static_callbacks: false,
        });
        let original = m(vec![
            f("zil.fn.0", Ty::Int(32), Val::Int(7)),
            f("zil.fn.1", Ty::Int(32), Val::Int(8)),
            f("zil.fn.999", Ty::Int(32), Val::Int(9)),
            f("main", Ty::Int(32), Val::Int(0)),
            f("runtime.helper", Ty::Int(32), Val::Int(0)),
            Item::Function(Function {
                name: "foreign".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), vec![]),
                attrs: vec![],
                blocks: None,
            }),
        ]);
        let mut units = vec![original.clone()];
        assert_eq!(internalize_closed_executable(&mut units, &lir.as_view()), 1);
        assert_eq!(function_linkage(&units[0], "zil.fn.0"), Linkage::Internal);
        assert_eq!(units[0].items[1..], original.items[1..]);
        zil_llvm_api::validate_module(&units[0]).unwrap();
        assert_eq!(internalize_closed_executable(&mut units, &lir.as_view()), 0);
    }

    #[test]
    fn closed_policy_keeps_cross_unit_function_addresses_and_global_initializers() {
        let one = f("zil.fn.1", Ty::Int(32), Val::Int(7));
        let three = f("zil.fn.3", Ty::Int(32), Val::Int(9));
        let declare = |item: &Item| match item {
            Item::Function(function) => Item::Function(function.declaration()),
            _ => unreachable!(),
        };
        let table = Item::Global(Global {
            name: "callback_table".into(),
            linkage: Linkage::External,
            ty: Ty::array(1, Ty::Ptr),
            initializer: Some(Val::Array(vec![Arg::new(
                Ty::Ptr,
                Val::Global("zil.fn.3".into()),
            )])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        });
        let mut units = vec![
            m(vec![
                declare(&one),
                declare(&three),
                f("zil.fn.0", Ty::Ptr, Val::Global("zil.fn.1".into())),
                f("zil.fn.2", Ty::Int(32), Val::Int(11)),
                table.clone(),
            ]),
            m(vec![one, three]),
        ];
        assert_eq!(
            internalize_closed_executable(&mut units, &candidate_lir(&[0, 1, 2, 3]).as_view()),
            2
        );
        assert_eq!(units.len(), 2);
        for name in ["zil.fn.0", "zil.fn.2"] {
            assert_eq!(function_linkage(&units[0], name), Linkage::Internal);
        }
        for name in ["zil.fn.1", "zil.fn.3"] {
            assert_eq!(function_linkage(&units[1], name), Linkage::External);
            assert_eq!(function_linkage(&units[0], name), Linkage::External);
        }
        assert_eq!(units[0].items[4], table);
        for module in &units {
            zil_llvm_api::validate_module(module).unwrap();
        }
    }

    #[test]
    fn closed_policy_preserves_local_address_use_and_duplicate_rejection() {
        let table = Item::Global(Global {
            name: "callback".into(),
            linkage: Linkage::External,
            ty: Ty::Ptr,
            initializer: Some(Val::Global("zil.fn.0".into())),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        });
        let mut units = vec![m(vec![
            f("zil.fn.0", Ty::Int(32), Val::Int(7)),
            table.clone(),
        ])];
        assert_eq!(
            internalize_closed_executable(&mut units, &candidate_lir(&[0]).as_view()),
            1
        );
        assert_eq!(units[0].items[1], table);
        zil_llvm_api::validate_module(&units[0]).unwrap();
        let bad = m(vec![
            f("zil.fn.0", Ty::Int(32), Val::Int(1)),
            f("zil.fn.0", Ty::Int(32), Val::Int(2)),
        ]);
        let mut units = vec![bad.clone()];
        assert_eq!(
            internalize_closed_executable(&mut units, &candidate_lir(&[0]).as_view()),
            0
        );
        assert_eq!(units[0], bad);
        assert_eq!(
            zil_llvm_api::validate_module(&units[0]),
            zil_llvm_api::validate_module(&bad)
        );
        assert!(zil_llvm_api::validate_module(&bad).is_err());
    }
}
