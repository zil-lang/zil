//! Private exact-input work products. These recipes are not validation evidence:
//! current declaration and complete native-unit preflight remain in the caller.
use super::{CallImportPolicy, distribute_item, rewrite_direct_calls};
use crate::EntryPoint;
use crate::object_cache::{BufferedHasher, installed_compiler_identity};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::hash::Hash;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::Fingerprint;
use zil_ir::{InstructionKind, LirView};
use zil_llvm_api::model::{Item, Linkage, Module, Op, Val};

/// Observations from complete actual raw items, never digest-selected rows.
/// Symbol names are rebound to the current table even when an item is equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CurrentGraph {
    source: Module,
    rows: Vec<std::sync::Arc<ObservedReads>>,
    bound: std::sync::Arc<ClassGraph>,
}
#[derive(Debug, PartialEq, Eq)]
struct ObservedReads {
    references: BTreeSet<String>,
    calls: std::sync::Arc<BTreeSet<String>>,
    work: usize,
}
struct BodyObservation<'a> {
    blocks: &'a zil_llvm_api::model::SharedVec<zil_llvm_api::model::Block>,
    reads: std::sync::Arc<ObservedReads>,
}

#[cfg(test)]
std::thread_local! {
    static GRAPH_WORK: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

impl CurrentGraph {
    pub(super) fn read(module: &Module, previous: Option<&Self>) -> Option<Self> {
        Self::read_bounded(module, previous, MAX_REFS)
    }
    fn read_bounded(module: &Module, previous: Option<&Self>, budget: usize) -> Option<Self> {
        if module.items.len() > MAX_ITEMS {
            return None;
        }
        let mut remaining = budget.checked_sub(module.items.len())?;
        let mut rows = Vec::with_capacity(module.items.len());
        // Current emit templates can share actual immutable block arrays across
        // different symbols. Reuse only their raw observations, charge the same
        // work, and bind every reference against the current symbol table below.
        let mut bodies = BTreeMap::<usize, BodyObservation<'_>>::new();
        for (index, item) in module.items.iter().enumerate() {
            let body = match item {
                Item::Function(function) => function.blocks.as_ref(),
                _ => None,
            };
            if let Some(blocks) = body
                && let Some(observed) = bodies.get(&(blocks.as_ptr() as usize))
                && (observed.blocks.ptr_eq(blocks) || observed.blocks == blocks)
            {
                remaining = remaining.checked_sub(observed.reads.work)?;
                rows.push(observed.reads.clone());
                #[cfg(test)]
                GRAPH_WORK.with(|work| {
                    let (fresh, reused) = work.get();
                    work.set((fresh, reused + 1));
                });
                continue;
            }
            if let Some(old) = previous.and_then(|old| {
                let prior = match item {
                    Item::Function(f) => *old.bound.names.get(&f.name)?,
                    Item::Global(g) => *old.bound.names.get(&g.name)?,
                    Item::NamedType(..) => index,
                };
                if old.source.items.get(prior) != Some(item) {
                    return None;
                }
                old.rows.get(prior)
            }) {
                remaining = remaining.checked_sub(old.work)?;
                rows.push(old.clone());
                if let Some(blocks) = body {
                    bodies.insert(
                        blocks.as_ptr() as usize,
                        BodyObservation {
                            blocks,
                            reads: old.clone(),
                        },
                    );
                }
                #[cfg(test)]
                GRAPH_WORK.with(|work| {
                    let (fresh, reused) = work.get();
                    work.set((fresh, reused + 1));
                });
                continue;
            }
            #[cfg(test)]
            GRAPH_WORK.with(|work| {
                let (fresh, reused) = work.get();
                work.set((fresh + 1, reused));
            });
            let before = remaining;
            let mut references = BTreeSet::new();
            let mut calls = BTreeSet::new();
            match item {
                Item::Function(function) => {
                    for block in function.blocks.iter().flatten() {
                        remaining = remaining.checked_sub(1)?;
                        for instruction in &block.instructions {
                            remaining = remaining.checked_sub(1)?;
                            if let Op::Call {
                                callee: Val::Global(name),
                                ..
                            } = &instruction.op
                            {
                                calls.insert(name.clone());
                            }
                            for value in instruction.op.values() {
                                value_references(value, &mut references, &mut remaining)?;
                            }
                        }
                    }
                }
                Item::Global(global) => {
                    if let Some(value) = &global.initializer {
                        value_references(value, &mut references, &mut remaining)?;
                    }
                }
                Item::NamedType(..) => {}
            }
            let observed = std::sync::Arc::new(ObservedReads {
                references,
                calls: std::sync::Arc::new(calls),
                work: before - remaining,
            });
            if let Some(blocks) = body {
                bodies.insert(
                    blocks.as_ptr() as usize,
                    BodyObservation {
                        blocks,
                        reads: observed.clone(),
                    },
                );
            }
            rows.push(observed);
        }
        let bound = std::sync::Arc::new(ClassGraph::read(module, &rows)?);
        Some(Self {
            source: module.clone(),
            rows,
            bound,
        })
    }
    pub(super) fn references(&self, index: usize) -> &BTreeSet<String> {
        &self.rows[index].references
    }
    pub(super) fn direct_calls(&self, index: usize) -> &BTreeSet<String> {
        &self.rows[index].calls
    }
    pub(super) fn matches(&self, module: &Module) -> bool {
        self.source.triple == module.triple && self.source.items == module.items
    }
}

/// Full values of the planner's grouping inputs. Class symbol spelling may
/// change with a body edit; its current index, role, ABI and graph must not.
/// This is an in-process comparison, independent of any disk key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Grouping {
    target: zil_base::TargetProfile,
    schema: u32,
    module: zil_base::ModuleId,
    triple: String,
    entry: EntryPoint,
    count: usize,
    source: Vec<SourceGrouping>,
    items: Vec<(Item, usize, bool)>,
    reads: Vec<ClassReads>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceGrouping {
    id: zil_ir::FunctionId,
    name: String,
    foreign: bool,
    file: zil_base::FileId,
    blocks: Vec<usize>,
    calls: Vec<(usize, usize, zil_ir::FunctionId)>,
}
impl Grouping {
    pub(super) fn class_inventory_changed(
        &self,
        source: &Module,
        classes: &BTreeSet<String>,
    ) -> bool {
        if self.items.len() != source.items.len() {
            return false;
        }
        let mut count = 0;
        for ((_, _, class), item) in self.items.iter().zip(source.items.iter()) {
            if *class {
                count += 1;
                if !matches!(item, Item::Function(f) if classes.contains(&f.name)) {
                    return true;
                }
            }
        }
        count != classes.len()
    }

    pub(super) fn same_source_environment(&self, current: &Self) -> bool {
        self.target == current.target
            && self.schema == current.schema
            && self.module == current.module
            && self.triple == current.triple
            && self.entry == current.entry
            && self.count == current.count
            && self.source == current.source
            && self
                .items
                .iter()
                .filter(|(_, _, class)| !*class)
                .map(|(item, _, _)| item)
                .eq(current
                    .items
                    .iter()
                    .filter(|(_, _, class)| !*class)
                    .map(|(item, _, _)| item))
    }
    pub(super) fn read(
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        observed: &CurrentGraph,
    ) -> Option<Self> {
        if !(2..=MAX_UNITS).contains(&count) {
            return None;
        }
        if !observed.matches(module) {
            return None;
        }
        let graph = &observed.bound;
        let mut remaining = MAX_REFS.checked_sub(lir.functions.len())?;
        let mut source = Vec::with_capacity(lir.functions.len());
        for function in lir.functions {
            remaining = remaining.checked_sub(function.blocks.len())?;
            let mut blocks = Vec::with_capacity(function.blocks.len());
            let mut calls = Vec::new();
            for (block, body) in function.blocks.iter().enumerate() {
                remaining = remaining.checked_sub(body.instructions.len())?;
                blocks.push(body.instructions.len());
                for (index, instruction) in body.instructions.iter().enumerate() {
                    let target = match &instruction.kind {
                        InstructionKind::Call { function, .. } => Some(*function),
                        InstructionKind::TaskCreate { worker, .. }
                        | InstructionKind::TaskStart { worker, .. }
                        | InstructionKind::WithScopedThread { worker, .. } => Some(*worker),
                        _ => None,
                    };
                    if let Some(target) = target {
                        calls.push((block, index, target));
                    }
                }
            }
            source.push(SourceGrouping {
                id: function.id,
                name: function.name.clone(),
                foreign: function.foreign.is_some(),
                file: function.span.file,
                blocks,
                calls,
            });
        }
        let items = module
            .items
            .iter()
            .map(|item| match item {
                Item::Function(function) if function.blocks.is_some() => {
                    let mut header = function.clone();
                    header.blocks = None;
                    let class = classes.contains(&function.name);
                    if class {
                        header.name.clear();
                    }
                    (Item::Function(header), function.cost(), class)
                }
                _ => (item.clone(), 0, false),
            })
            .collect();
        Some(Self {
            target: lir.target.clone(),
            schema: lir.schema_version,
            module: lir.module_id,
            triple: module.triple.clone(),
            entry,
            count,
            source,
            items,
            reads: graph.reads.clone(),
        })
    }
}

const MAGIC: &[u8; 8] = b"ZILCGU01";
const DIRECTORY: &str = "partition-layout-v1";
pub(crate) const MAX_ITEMS: usize = 262_144;
const MAX_UNITS: usize = 256;
const MAX_REFS: usize = 1_048_576;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const HEADER: usize = 8 + 32 + 32;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Unit {
    pub owner: usize,
    pub prelude: Vec<usize>,
    pub declarations: Vec<usize>,
    pub imports: Vec<usize>,
    pub definitions: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub units: Vec<Unit>,
}

impl Layout {
    /// Adapt a completed raw work product to the current class inventory. The
    /// source environment was compared in full by Materialization; owners use
    /// the very same current grouping routine as the original planner.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn adapt_classes(
        &self,
        old_source: &Module,
        old_graph: &CurrentGraph,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        observed: &CurrentGraph,
    ) -> Option<Self> {
        if !old_graph.matches(old_source)
            || !observed.matches(module)
            || !(2..=MAX_UNITS).contains(&count)
        {
            return None;
        }
        Self::derive_classes(
            module,
            lir,
            entry,
            count,
            classes,
            observed,
            Some((self, old_source, old_graph)),
        )
    }

    /// No prior machine or serialized role is needed for direct-restart edits.
    /// Derive the original policy from this request's actual raw graph once.
    pub(super) fn plan_classes_current(
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        observed: &CurrentGraph,
    ) -> Option<Self> {
        if !observed.matches(module) || !(2..=MAX_UNITS).contains(&count) {
            return None;
        }
        Self::derive_classes(module, lir, entry, count, classes, observed, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn derive_classes(
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        observed: &CurrentGraph,
        previous: Option<(&Self, &Module, &CurrentGraph)>,
    ) -> Option<Self> {
        let graph = &observed.bound;
        let defs = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if !definition(item) {
                    return None;
                }
                let name = match item {
                    Item::Function(function) => &function.name,
                    Item::Global(global) => &global.name,
                    Item::NamedType(..) => return None,
                };
                Some(super::Definition {
                    name: name.clone(),
                    index,
                    local: local(item),
                    refs: std::borrow::Cow::Borrowed(observed.references(index)),
                    calls: std::borrow::Cow::Borrowed(observed.direct_calls(index)),
                })
            })
            .collect::<Vec<_>>();
        let names = defs
            .iter()
            .enumerate()
            .map(|(id, definition)| (definition.name.as_str(), id))
            .collect::<BTreeMap<_, _>>();
        let assigned =
            super::definition_owners(module, lir, entry, count, true, true, &defs, &names).ok()?;
        let mut owners = vec![usize::MAX; module.items.len()];
        let mut members = BTreeMap::<usize, Vec<usize>>::new();
        for (definition, owner) in defs.iter().zip(assigned) {
            owners[definition.index] = owner;
            members.entry(owner).or_default().push(definition.index);
        }
        if members.len() < 2 {
            return None;
        }
        let mut old_owners =
            vec![usize::MAX; previous.map_or(0, |(_, source, _)| source.items.len())];
        if let Some((layout, _, _)) = previous {
            for unit in &layout.units {
                for &index in &unit.definitions {
                    *old_owners.get_mut(index)? = unit.owner;
                }
            }
        }
        let named = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if let Item::NamedType(name, _) = item {
                    Some((name.as_str(), index))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();
        let remap = previous.map_or_else(Vec::new, |(_, source, _)| {
            source
                .items
                .iter()
                .map(|item| match item {
                    Item::NamedType(name, _) => named.get(name.as_str()).copied(),
                    Item::Function(function) => graph.names.get(&function.name).copied(),
                    Item::Global(global) => graph.names.get(&global.name).copied(),
                })
                .collect::<Vec<_>>()
        });
        let map_indices = |indices: &[usize]| -> Option<Vec<usize>> {
            let mut mapped = indices
                .iter()
                .map(|&index| remap.get(index).copied().flatten())
                .collect::<Option<Vec<_>>>()?;
            mapped.sort_unstable();
            if mapped.windows(2).any(|pair| pair[0] == pair[1]) {
                return None;
            }
            Some(mapped)
        };
        let mut importable = vec![false; module.items.len()];
        for function in lir
            .functions
            .iter()
            .filter(|function| function.foreign.is_none())
        {
            let name = super::source_name(&graph.names, function.id);
            importable[*graph.names.get(&name)?] = true;
        }
        for name in classes {
            importable[*graph.names.get(name)?] = true;
        }
        let mut remaining = graph.remaining;
        let mut units = Vec::with_capacity(members.len());
        let mut seen = vec![0usize; module.items.len()];
        let mut imported = vec![0usize; module.items.len()];
        let mut referenced = vec![0usize; module.items.len()];
        let mut pending = Vec::new();
        for (owner, definitions) in members {
            let mut exhausted = false;
            let retained = previous.and_then(|(layout, _, old_graph)| {
                let old = layout.units.iter().find(|unit| unit.owner == owner)?;
                let unit = Unit {
                    owner,
                    definitions: map_indices(&old.definitions)?,
                    imports: map_indices(&old.imports)?,
                    prelude: map_indices(&old.prelude)?,
                    declarations: map_indices(&old.declarations)?,
                };
                if unit.definitions != definitions {
                    return None;
                }
                // A body-only change preserves roles; a changed edge, removed
                // symbol or moved callee owner rebuilds this unit's closure.
                for &index in old.definitions.iter().chain(&old.imports) {
                    remaining = match remaining.checked_sub(1) {
                        Some(left) => left,
                        None => {
                            exhausted = true;
                            return None;
                        }
                    };
                    let current = remap[index]?;
                    let before = &old_graph.rows[index];
                    let after = &observed.rows[current];
                    if before.references != after.references || before.calls != after.calls {
                        return None;
                    }
                    for &target in &old_graph.bound.reads[index].references {
                        remaining = match remaining.checked_sub(1) {
                            Some(left) => left,
                            None => {
                                exhausted = true;
                                return None;
                            }
                        };
                        if old_owners[target] != owners[remap[target]?] {
                            return None;
                        }
                    }
                }
                Some(unit)
            });
            if exhausted {
                return None;
            }
            if let Some(unit) = retained {
                units.push(unit);
                continue;
            }
            let stamp = units.len() + 1;
            for &index in &definitions {
                enqueue_calls(
                    &graph.reads[index].calls,
                    &owners,
                    owner,
                    stamp,
                    &mut seen,
                    &mut pending,
                    &mut remaining,
                )?;
            }
            let mut imports = Vec::new();
            while let Some(index) = pending.pop() {
                if !importable[index]
                    || !matches!(&module.items[index], Item::Function(function)
                        if function.blocks.is_some() && function.linkage == Linkage::External)
                    || graph.reads[index]
                        .references
                        .iter()
                        .any(|&target| local(&module.items[target]))
                {
                    return None;
                }
                imported[index] = stamp;
                imports.push(index);
                enqueue_calls(
                    &graph.reads[index].calls,
                    &owners,
                    owner,
                    stamp,
                    &mut seen,
                    &mut pending,
                    &mut remaining,
                )?;
            }
            imports.sort_unstable();
            let mut prelude = named.values().copied().collect::<Vec<_>>();
            let mut declarations = Vec::new();
            for &index in definitions.iter().chain(&imports) {
                for &target in &graph.reads[index].references {
                    remaining = remaining.checked_sub(1)?;
                    if referenced[target] == stamp {
                        continue;
                    }
                    referenced[target] = stamp;
                    if owners[target] == usize::MAX {
                        prelude.push(target);
                    } else if owners[target] != owner {
                        if local(&module.items[target]) {
                            return None;
                        }
                        if imported[target] != stamp {
                            declarations.push(target);
                        }
                    }
                }
            }
            prelude.sort_unstable();
            prelude.dedup();
            declarations.sort_unstable();
            units.push(Unit {
                owner,
                definitions,
                imports,
                prelude,
                declarations,
            });
        }
        let layout = Self { units };
        if previous.is_some() {
            // Retained roles are proposals. Recheck the complete current
            // closure/local-state/copy-budget contract before replay.
            layout.valid_classes_observed(module, lir, entry, count, classes, observed)?;
        } else if !layout.valid_in_domain(module, lir, count, Some(classes)) {
            // Fresh roles already came from the complete current closure walk
            // above. Still check every role, clone collision and the original
            // SOURCE-only total copy budget; class cost never enlarges it.
            return None;
        }
        Some(layout)
    }
    /// Both the original planner and a checked receipt use this exact emitter.
    #[cfg(test)]
    pub(super) fn replay(&self, module: Module) -> Vec<Module> {
        self.replay_reusing(module, None)
    }
    #[cfg(test)]
    pub(super) fn replay_reusing(
        &self,
        module: Module,
        previous: Option<&super::Materialization>,
    ) -> Vec<Module> {
        self.replay_reusing_counted(module, previous).0
    }
    pub(super) fn replay_reusing_counted(
        &self,
        module: Module,
        previous: Option<&super::Materialization>,
    ) -> (Vec<Module>, usize) {
        self.replay_observed(module, previous, None)
    }
    pub(super) fn replay_observed(
        &self,
        module: Module,
        previous: Option<&super::Materialization>,
        observed: Option<&CurrentGraph>,
    ) -> (Vec<Module>, usize) {
        use super::materialization::{DEFINITION, IMPORT_BODY};
        // Raw current values, never a cached fingerprint, permit skipping a
        // mutation walk. A stale observation falls back to the original walk.
        let observed = observed.filter(|graph| graph.matches(&module));
        let reuse = previous.and_then(|previous| previous.bind(&module, self));
        let mut complete = self
            .units
            .iter()
            .enumerate()
            .map(|(index, unit)| reuse.as_ref().and_then(|reuse| reuse.unit(index, unit)))
            .collect::<Vec<_>>();
        if complete.iter().all(Option::is_some) {
            let units = complete
                .into_iter()
                .map(|unit| {
                    let mut unit = unit.expect("equal complete unit");
                    unit.fingerprint.clone_from(&module.fingerprint);
                    unit
                })
                .collect();
            return (
                units,
                reuse.as_ref().map_or(0, |reuse| reuse.reused_items()),
            );
        }
        let mut remaining = vec![0usize; module.items.len()];
        let mut prototypes = BTreeMap::new();
        for (index, unit) in self.units.iter().enumerate() {
            if complete[index].is_some() {
                continue;
            }
            for &index in unit.definitions.iter().chain(&unit.imports) {
                remaining[index] += 1;
            }
            for &index in unit.declarations.iter().chain(&unit.imports) {
                prototypes
                    .entry(index)
                    .or_insert_with(|| match &module.items[index] {
                        Item::Function(f) => Item::Function(f.declaration()),
                        Item::Global(g) => Item::Global(g.declaration()),
                        Item::NamedType(..) => unreachable!("a definition has a declaration"),
                    });
            }
        }
        let mut original = if complete.iter().any(Option::is_some) {
            let mut needed = remaining
                .iter()
                .map(|&count| count != 0)
                .collect::<Vec<_>>();
            for (index, unit) in self.units.iter().enumerate() {
                if complete[index].is_none() {
                    for &index in &unit.prelude {
                        needed[index] = true;
                    }
                }
            }
            // Equal whole units retain their real owners. Do not clone every
            // unrelated current header just to drop it after emitting a sparse
            // set of changed units.
            module
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| needed[index].then(|| item.clone()))
                .collect::<Vec<_>>()
        } else {
            module.items.into_iter().map(Some).collect::<Vec<_>>()
        };
        let mut units = Vec::with_capacity(self.units.len());
        for (unit_index, unit) in self.units.iter().enumerate() {
            if let Some(mut current) = complete[unit_index].take() {
                current.fingerprint.clone_from(&module.fingerprint);
                units.push(current);
                continue;
            }
            let clone_names = unit
                .imports
                .iter()
                .map(|&index| {
                    let Item::Function(function) =
                        original[index].as_ref().expect("remaining import")
                    else {
                        unreachable!("an imported definition is a function")
                    };
                    (
                        function.name.clone(),
                        format!("zil.import.{}.{}", unit.owner, function.name),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let needs_rewrite = |index| {
                observed.is_none_or(|graph| {
                    graph
                        .direct_calls(index)
                        .iter()
                        .any(|name| clone_names.contains_key(name))
                })
            };
            let mut items = Vec::with_capacity(
                unit.prelude.len()
                    + unit.declarations.len()
                    + 2 * unit.imports.len()
                    + unit.definitions.len(),
            );
            for &index in &unit.prelude {
                items.push(original[index].as_ref().expect("retained prelude").clone());
            }
            for &index in &unit.declarations {
                items.push(prototypes[&index].clone());
            }
            for &index in &unit.imports {
                // Canonical address operands keep this prototype. Only direct
                // calls are rewritten to the private implementation.
                items.push(prototypes[&index].clone());
                let reused = reuse
                    .as_ref()
                    .and_then(|reuse| reuse.item(unit_index, unit.owner, index, IMPORT_BODY));
                let original = distribute_item(&mut original[index], &mut remaining[index]);
                if let Some(item) = reused {
                    items.push(item);
                    continue;
                }
                let Item::Function(mut function) = original else {
                    unreachable!("an imported definition is a function")
                };
                function.linkage = Linkage::Internal;
                function.name = clone_names[&function.name].clone();
                if needs_rewrite(index) {
                    rewrite_direct_calls(&mut function, &clone_names);
                }
                items.push(Item::Function(function));
            }
            for &index in &unit.definitions {
                let reused = reuse
                    .as_ref()
                    .and_then(|reuse| reuse.item(unit_index, unit.owner, index, DEFINITION));
                let mut item = distribute_item(&mut original[index], &mut remaining[index]);
                if let Some(previous) = reused {
                    items.push(previous);
                    continue;
                }
                if let Item::Function(function) = &mut item
                    && needs_rewrite(index)
                {
                    rewrite_direct_calls(function, &clone_names);
                }
                items.push(item);
            }
            units.push(Module {
                triple: module.triple.clone(),
                fingerprint: module.fingerprint.clone(),
                items: items.into(),
            });
        }
        (
            units,
            reuse.as_ref().map_or(0, |reuse| reuse.reused_items()),
        )
    }

    fn encode(&self) -> Option<Vec<u8>> {
        if !(2..=MAX_UNITS).contains(&self.units.len()) {
            return None;
        }
        let refs = self.units.iter().try_fold(0usize, |sum, unit| {
            sum.checked_add(unit.prelude.len())?
                .checked_add(unit.declarations.len())?
                .checked_add(unit.imports.len())?
                .checked_add(unit.definitions.len())
        })?;
        if refs > MAX_REFS {
            return None;
        }
        let length = 4usize
            .checked_add(self.units.len().checked_mul(20)?)?
            .checked_add(refs.checked_mul(4)?)?;
        if length > MAX_BYTES {
            return None;
        }
        let mut bytes = Vec::with_capacity(length);
        put(&mut bytes, self.units.len())?;
        for unit in &self.units {
            put(&mut bytes, unit.owner)?;
            for indices in [
                &unit.prelude,
                &unit.declarations,
                &unit.imports,
                &unit.definitions,
            ] {
                put(&mut bytes, indices.len())?;
                for &index in indices {
                    put(&mut bytes, index)?;
                }
            }
        }
        Some(bytes)
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let mut reader = Reader(bytes);
        let count = reader.word()?;
        if !(2..=MAX_UNITS).contains(&count) {
            return None;
        }
        let mut refs = 0usize;
        let mut units = Vec::with_capacity(count);
        for _ in 0..count {
            units.push(Unit {
                owner: reader.word()?,
                prelude: reader.indices(&mut refs)?,
                declarations: reader.indices(&mut refs)?,
                imports: reader.indices(&mut refs)?,
                definitions: reader.indices(&mut refs)?,
            });
        }
        reader.0.is_empty().then_some(Self { units })
    }

    /// Structural checks protect replay's indices, allocations and linkage. The
    /// checksum/current-input key protects the original graph decision itself,
    /// with the same trusted-directory boundary as the existing object cache.
    fn valid(&self, module: &Module, lir: &LirView<'_>, count: usize) -> bool {
        self.valid_in_domain(module, lir, count, None)
    }

    fn valid_in_domain(
        &self,
        module: &Module,
        lir: &LirView<'_>,
        count: usize,
        classes: Option<&BTreeSet<String>>,
    ) -> bool {
        if module.items.len() > MAX_ITEMS || self.units.len() > count {
            return false;
        }
        let mut assigned = vec![false; module.items.len()];
        let named = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| matches!(item, Item::NamedType(..)).then_some(index))
            .collect::<Vec<_>>();
        let has_imports = self.units.iter().any(|unit| !unit.imports.is_empty());
        let definition_names = if classes.is_some() {
            module
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| definition(item))
                .filter_map(|(index, item)| match item {
                    Item::Function(f) => Some((f.name.as_str(), index)),
                    Item::Global(g) => Some((g.name.as_str(), index)),
                    Item::NamedType(..) => None,
                })
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        };
        let source_names = if has_imports || classes.is_some() {
            lir.functions
                .iter()
                .filter(|f| f.foreign.is_none())
                .map(|f| {
                    if classes.is_some() {
                        super::source_name(&definition_names, f.id)
                    } else {
                        format!("zil.fn.{}", f.id.0)
                    }
                })
                .collect::<BTreeSet<_>>()
        } else {
            BTreeSet::new()
        };
        let mut source_cost = 0usize;
        let mut symbols = BTreeSet::new();
        for item in module
            .items
            .iter()
            .filter(|_| has_imports || classes.is_some())
        {
            match item {
                Item::Function(function) => {
                    if !symbols.insert(function.name.as_str()) {
                        return false;
                    }
                    if function.blocks.is_some() && source_names.contains(&function.name) {
                        let Some(cost) = source_cost.checked_add(function.cost()) else {
                            return false;
                        };
                        source_cost = cost;
                    }
                }
                Item::Global(global) => {
                    if !symbols.insert(global.name.as_str()) {
                        return false;
                    }
                }
                Item::NamedType(..) => {}
            }
        }
        if let Some(classes) = classes
            && (source_names.iter().any(|name| {
                definition_names.get(name.as_str()).is_none_or(|&index| {
                    !matches!(&module.items[index], Item::Function(f) if f.blocks.is_some())
                })
            }) || classes.iter().any(|name| {
                definition_names.get(name.as_str()).is_none_or(|&index| {
                    !matches!(&module.items[index], Item::Function(f)
                        if f.blocks.is_some() && f.linkage == Linkage::External)
                })
            }))
        {
            return false;
        }
        let mut copied_cost = 0usize;
        let mut previous = None;
        for unit in &self.units {
            if unit.owner >= count
                || previous.is_some_and(|owner| owner >= unit.owner)
                || unit.definitions.is_empty()
            {
                return false;
            }
            previous = Some(unit.owner);
            for indices in [
                &unit.prelude,
                &unit.declarations,
                &unit.imports,
                &unit.definitions,
            ] {
                if indices.last().is_some_and(|&i| i >= module.items.len())
                    || indices.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return false;
                }
            }
            let mut named_here = Vec::new();
            for &index in &unit.prelude {
                if definition(&module.items[index]) {
                    return false;
                }
                if matches!(module.items[index], Item::NamedType(..)) {
                    named_here.push(index);
                }
            }
            if named_here != named {
                return false;
            }
            for &index in &unit.definitions {
                if !definition(&module.items[index]) || assigned[index] {
                    return false;
                }
                assigned[index] = true;
            }
            for &index in &unit.declarations {
                if !definition(&module.items[index])
                    || local(&module.items[index])
                    || unit.definitions.binary_search(&index).is_ok()
                    || unit.imports.binary_search(&index).is_ok()
                {
                    return false;
                }
            }
            for &index in &unit.imports {
                let Item::Function(function) = &module.items[index] else {
                    return false;
                };
                if function.blocks.is_none()
                    || function.linkage != Linkage::External
                    || !(source_names.contains(&function.name)
                        || classes.is_some_and(|classes| classes.contains(&function.name)))
                    || unit.definitions.binary_search(&index).is_ok()
                    || symbols
                        .contains(format!("zil.import.{}.{}", unit.owner, function.name).as_str())
                {
                    return false;
                }
                let Some(cost) = copied_cost.checked_add(function.cost()) else {
                    return false;
                };
                copied_cost = cost;
                if copied_cost > source_cost {
                    return false;
                }
            }
        }
        module
            .items
            .iter()
            .enumerate()
            .all(|(i, item)| assigned[i] == definition(item))
    }

    /// Recheck the actual current graph, not a class-name prefix or checksum.
    /// One current body walk supplies both closure checks and materialization's
    /// observed calls. Bounded indexed walks avoid repeating the source planner.
    #[cfg(test)]
    fn valid_classes(
        &self,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
    ) -> Option<CurrentGraph> {
        let graph = CurrentGraph::read(module, None)?;
        self.valid_classes_observed(module, lir, entry, count, classes, &graph)?;
        Some(graph)
    }
    pub(super) fn valid_classes_observed(
        &self,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        observed: &CurrentGraph,
    ) -> Option<()> {
        if !self.valid_in_domain(module, lir, count, Some(classes)) {
            return None;
        }
        if !observed.matches(module) {
            return None;
        }
        let graph = &observed.bound;
        let mut remaining = graph.remaining;
        let mut owners = vec![usize::MAX; module.items.len()];
        for (owner, unit) in self.units.iter().enumerate() {
            for &index in &unit.definitions {
                owners[index] = owner;
            }
        }
        let mut importable = vec![false; module.items.len()];
        for function in lir.functions.iter().filter(|f| f.foreign.is_none()) {
            let name = super::source_name(&graph.names, function.id);
            let &index = graph.names.get(name.as_str())?;
            importable[index] = true;
            let selected = match entry {
                EntryPoint::Main => function.name == "main",
                EntryPoint::ReturnValue(wanted) | EntryPoint::Test(wanted) => function.id == wanted,
                EntryPoint::None => false,
            };
            if selected
                && let Some(&wrapper) = graph.names.get("main")
                && owners[wrapper] != usize::MAX
                && owners[index] != owners[wrapper]
            {
                return None;
            }
        }
        for name in classes {
            importable[*graph.names.get(name.as_str())?] = true;
        }
        if let (Some(&main), Some(&pe)) = (graph.names.get("main"), graph.names.get("zil.pe.entry"))
            && owners[main] != usize::MAX
            && owners[pe] != usize::MAX
            && owners[main] != owners[pe]
        {
            return None;
        }
        let named = module
            .items
            .iter()
            .filter(|item| matches!(item, Item::NamedType(..)))
            .count();
        let mut seen = vec![0usize; module.items.len()];
        let mut imported = vec![0usize; module.items.len()];
        let mut referenced = vec![0usize; module.items.len()];
        let mut pending = Vec::new();
        for (owner, unit) in self.units.iter().enumerate() {
            let stamp = owner + 1;
            for &index in &unit.imports {
                imported[index] = stamp;
            }
            for &index in &unit.definitions {
                enqueue_calls(
                    &graph.reads[index].calls,
                    &owners,
                    owner,
                    stamp,
                    &mut seen,
                    &mut pending,
                    &mut remaining,
                )?;
            }
            let mut reached = 0usize;
            while let Some(index) = pending.pop() {
                if imported[index] != stamp || !importable[index] {
                    return None;
                }
                // Copies may not duplicate/refer across a local-state component,
                // even if an address rather than a direct call reaches it.
                if graph.reads[index]
                    .references
                    .iter()
                    .any(|&target| local(&module.items[target]))
                {
                    return None;
                }
                reached += 1;
                enqueue_calls(
                    &graph.reads[index].calls,
                    &owners,
                    owner,
                    stamp,
                    &mut seen,
                    &mut pending,
                    &mut remaining,
                )?;
            }
            if reached != unit.imports.len() {
                return None;
            }
            let mut prelude_count = named;
            let mut declaration_count = 0usize;
            for &index in unit.definitions.iter().chain(&unit.imports) {
                for &target in &graph.reads[index].references {
                    remaining = remaining.checked_sub(1)?;
                    if referenced[target] == stamp {
                        continue;
                    }
                    referenced[target] = stamp;
                    if owners[target] == usize::MAX {
                        if unit.prelude.binary_search(&target).is_err() {
                            return None;
                        }
                        prelude_count += 1;
                    } else if owners[target] != owner {
                        if local(&module.items[target]) {
                            return None;
                        }
                        if imported[target] != stamp {
                            if unit.declarations.binary_search(&target).is_err() {
                                return None;
                            }
                            declaration_count += 1;
                        }
                    }
                }
            }
            if prelude_count != unit.prelude.len() || declaration_count != unit.declarations.len() {
                return None;
            }
        }
        Some(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ClassReads {
    references: Vec<usize>,
    calls: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassGraph {
    names: BTreeMap<String, usize>,
    reads: Vec<ClassReads>,
    remaining: usize,
}
impl ClassGraph {
    fn read(module: &Module, rows: &[std::sync::Arc<ObservedReads>]) -> Option<Self> {
        let mut names = BTreeMap::new();
        for (index, item) in module.items.iter().enumerate() {
            let name = match item {
                Item::Function(f) => f.name.as_str(),
                Item::Global(g) => g.name.as_str(),
                Item::NamedType(..) => continue,
            };
            if names.insert(name.to_owned(), index).is_some() {
                return None;
            }
        }
        let mut remaining = MAX_REFS.checked_sub(module.items.len())?;
        let mut reads = Vec::with_capacity(module.items.len());
        for row in rows {
            remaining = remaining.checked_sub(row.work)?;
            let mut current = ClassReads::default();
            for name in &row.references {
                current.references.push(*names.get(name.as_str())?);
            }
            for name in row.calls.iter() {
                current.calls.push(*names.get(name.as_str())?);
            }
            current.references.sort_unstable();
            current.references.dedup();
            current.calls.sort_unstable();
            current.calls.dedup();
            reads.push(current);
        }
        Some(Self {
            names,
            reads,
            remaining,
        })
    }
}
fn value_references(
    value: &Val,
    references: &mut BTreeSet<String>,
    remaining: &mut usize,
) -> Option<()> {
    let mut pending = Vec::new();
    let mut first = Some(value);
    while let Some(value) = first.take().or_else(|| pending.pop()) {
        *remaining = remaining.checked_sub(1)?;
        match value {
            Val::Global(name) => {
                references.insert(name.clone());
            }
            Val::Array(args) => {
                if pending.len().checked_add(args.len())? > *remaining {
                    return None;
                }
                pending.extend(args.iter().map(|arg| &arg.value));
            }
            Val::Gep { ptr, indices, .. } => {
                if pending.len().checked_add(indices.len())?.checked_add(1)? > *remaining {
                    return None;
                }
                pending.push(ptr);
                pending.extend(indices.iter().map(|arg| &arg.value));
            }
            _ => {}
        }
    }
    Some(())
}
fn enqueue_calls(
    calls: &[usize],
    owners: &[usize],
    owner: usize,
    stamp: usize,
    seen: &mut [usize],
    pending: &mut Vec<usize>,
    remaining: &mut usize,
) -> Option<()> {
    for &target in calls {
        *remaining = remaining.checked_sub(1)?;
        if owners[target] != usize::MAX && owners[target] != owner && seen[target] != stamp {
            seen[target] = stamp;
            pending.push(target);
        }
    }
    Some(())
}

fn definition(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.blocks.is_some(),
        Item::Global(g) => g.initializer.is_some(),
        Item::NamedType(..) => false,
    }
}
fn local(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.linkage.is_local(),
        Item::Global(g) => g.linkage.is_local(),
        Item::NamedType(..) => false,
    }
}
fn put(bytes: &mut Vec<u8>, value: usize) -> Option<()> {
    bytes.extend_from_slice(&u32::try_from(value).ok()?.to_le_bytes());
    Some(())
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn word(&mut self) -> Option<usize> {
        let (word, tail) = self.0.split_at_checked(4)?;
        self.0 = tail;
        Some(u32::from_le_bytes(word.try_into().ok()?) as usize)
    }
    fn indices(&mut self, total: &mut usize) -> Option<Vec<usize>> {
        let count = self.word()?;
        *total = total.checked_add(count)?;
        if *total > MAX_REFS || count > self.0.len() / 4 {
            return None;
        }
        (0..count).map(|_| self.word()).collect()
    }
}

pub(super) struct Cache {
    root: PathBuf,
    key: Fingerprint,
}
impl Cache {
    pub(super) fn new(
        root: &Path,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        split: bool,
        imports: CallImportPolicy,
    ) -> Option<Self> {
        if ["LD_PRELOAD", "LD_AUDIT"]
            .into_iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return None;
        }
        let key = action_key(
            module,
            lir,
            entry,
            count,
            split,
            imports,
            installed_compiler_identity().ok()?,
        )?;
        Some(Self {
            root: root.into(),
            key,
        })
    }
    pub(super) fn load(&self, module: &Module, lir: &LirView<'_>, count: usize) -> Option<Layout> {
        let layout = self.read()?;
        layout.valid(module, lir, count).then_some(layout)
    }
    #[cfg(test)]
    pub(super) fn load_classes(
        &self,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
    ) -> Option<(Layout, CurrentGraph)> {
        let observed = CurrentGraph::read(module, None)?;
        let layout = self.load_classes_observed(module, lir, entry, count, classes, &observed)?;
        Some((layout, observed))
    }
    pub(super) fn load_classes_observed(
        &self,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        observed: &CurrentGraph,
    ) -> Option<Layout> {
        let layout = self.read()?;
        layout.valid_classes_observed(module, lir, entry, count, classes, observed)?;
        Some(layout)
    }
    pub(super) fn key(&self) -> Fingerprint {
        self.key
    }
    fn read(&self) -> Option<Layout> {
        private_existing(&self.root).ok()?;
        let directory = self.root.join(DIRECTORY);
        private_existing(&directory).ok()?;
        let path = entry_path(&directory, self.key);
        let meta = fs::symlink_metadata(&path).ok()?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() < HEADER as u64
            || meta.len() > (HEADER + MAX_BYTES) as u64
        {
            return None;
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        File::open(path)
            .ok()?
            .take((HEADER + MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() != meta.len() as usize
            || &bytes[..8] != MAGIC
            || bytes[8..40] != self.key.0
            || Fingerprint::of(&bytes[HEADER..]).0 != bytes[40..HEADER]
        {
            return None;
        }
        Layout::decode(&bytes[HEADER..])
    }
    // A stable-class recipe is never interchangeable with the ordinary source
    // import policy, even if their complete machine inputs happen to agree.
    pub(super) fn with_classes(mut self, classes: &BTreeSet<String>) -> Self {
        let mut state = BufferedHasher::new();
        b"ZIL-CGU-STABLE-CLASSES-v1".hash(&mut state);
        self.key.hash(&mut state);
        classes.hash(&mut state);
        self.key = state.digest();
        self
    }
    pub(super) fn pending(self, layout: &Layout) -> Option<Pending> {
        let path = entry_path(&self.root.join(DIRECTORY), self.key);
        Some(Pending {
            root: self.root,
            path,
            key: self.key,
            bytes: layout.encode()?,
        })
    }
}

fn action_key(
    module: &Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split: bool,
    imports: CallImportPolicy,
    compiler: Fingerprint,
) -> Option<Fingerprint> {
    if !(2..=MAX_UNITS).contains(&count) || module.items.len() > MAX_ITEMS {
        return None;
    }
    let mut remaining = MAX_REFS.checked_sub(lir.functions.len())?;
    let mut state = BufferedHasher::new();
    MAGIC.hash(&mut state);
    compiler.hash(&mut state);
    count.hash(&mut state);
    split.hash(&mut state);
    imports.hash(&mut state);
    match entry {
        EntryPoint::None => 0u8.hash(&mut state),
        EntryPoint::Main => 1u8.hash(&mut state),
        EntryPoint::ReturnValue(id) => {
            2u8.hash(&mut state);
            id.hash(&mut state);
        }
        EntryPoint::Test(id) => {
            3u8.hash(&mut state);
            id.hash(&mut state);
        }
    }
    lir.target.hash(&mut state);
    lir.schema_version.hash(&mut state);
    lir.module_id.hash(&mut state);
    lir.functions.len().hash(&mut state);
    // Exact projection of every LIR field read by the partition planner. The
    // complete machine model below binds bodies/types/linkage independently.
    for function in lir.functions {
        function.id.hash(&mut state);
        function.name.hash(&mut state);
        function.foreign.is_some().hash(&mut state);
        function.span.file.hash(&mut state);
        function.blocks.len().hash(&mut state);
        remaining = remaining.checked_sub(function.blocks.len())?;
        for block in &function.blocks {
            block.instructions.len().hash(&mut state);
            remaining = remaining.checked_sub(block.instructions.len())?;
            for instruction in &block.instructions {
                let callee = match &instruction.kind {
                    InstructionKind::Call { function, .. } => Some(function),
                    InstructionKind::TaskCreate { worker, .. }
                    | InstructionKind::TaskStart { worker, .. }
                    | InstructionKind::WithScopedThread { worker, .. } => Some(worker),
                    _ => None,
                };
                callee.hash(&mut state);
            }
        }
    }
    // Exhaustive destructuring requires future machine fields to be considered.
    // The diagnostic fingerprint is freshly preserved by replay, never authority.
    let Module {
        triple,
        fingerprint: _,
        items,
    } = module;
    triple.hash(&mut state);
    items.hash(&mut state);
    Some(state.digest())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    root: PathBuf,
    path: PathBuf,
    key: Fingerprint,
    bytes: Vec<u8>,
}
impl Pending {
    pub(crate) fn key(&self) -> Fingerprint {
        self.key
    }
    /// Pin the real root only after PreparedCache has created/validated it.
    /// Driver publication guards register this exact optional destination.
    pub(crate) fn pin(mut self) -> Option<Self> {
        private_existing(&self.root).ok()?;
        self.root = fs::canonicalize(&self.root).ok()?;
        let directory = self.root.join(DIRECTORY);
        match fs::symlink_metadata(&directory) {
            Ok(_) => private_existing(&directory).ok()?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        self.path = entry_path(&directory, self.key);
        Some(self)
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn commit(self) {
        // Optional metadata cannot revoke an already published compilation.
        let _ = self.store();
    }
    fn store(&self) -> io::Result<()> {
        private_existing(&self.root)?;
        let directory = self.root.join(DIRECTORY);
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        private_existing(&directory)?;
        let temp = directory.join(format!(
            ".pending-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temp)?;
            file.write_all(MAGIC)?;
            file.write_all(&self.key.0)?;
            file.write_all(&Fingerprint::of(&self.bytes).0)?;
            file.write_all(&self.bytes)?;
            file.flush()?;
            drop(file);
            fs::rename(&temp, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}
fn entry_path(directory: &Path, key: Fingerprint) -> PathBuf {
    directory.join(format!("{key}.layout"))
}
fn private_existing(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "layout cache is not a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "layout cache is not private",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
