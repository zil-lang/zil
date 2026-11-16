//! Reuse actual partition items after their CURRENT layout roles are checked.
//! Raw observations and the completed layout remain bounded by this product's
//! lifetime. They are not a verification receipt or persisted machine authority.
use super::{CallImportPolicy, direct_calls, layout};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};
use zil_llvm_api::model::{Item, Module};

pub(super) const PRELUDE: u8 = 0;
pub(super) const DECLARATION: u8 = 1;
pub(super) const IMPORT_DECLARATION: u8 = 2;
pub(super) const IMPORT_BODY: u8 = 3;
pub(super) const DEFINITION: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Materialization {
    source: Module,
    pub(crate) reused_items: usize,
    pub(crate) constructed_items: usize,
    units: Vec<Module>,
    inputs: OnceLock<Vec<UnitInputs>>,
    calls: OnceLock<Vec<Arc<BTreeSet<String>>>>,
    layout: Arc<layout::Layout>,
    graph: Option<Arc<layout::CurrentGraph>>,
    grouping: OnceLock<Option<Arc<layout::Grouping>>>,
    grouping_input: Option<Arc<GroupingInput>>,
    // BoundedClasses means the current 512-cost class-only transitive policy.
    // Both completed roles and their deferred grouping retain this selection;
    // no machine body or policy authority is restored across processes.
    imports: CallImportPolicy,
    // Only the production path that rendered these exact layout roles sets
    // this association. Raw test/candidate recipes cannot lend role slots.
    completed_roles: bool,
}
// Only the raw LIR fields read by Grouping. Persistent functions retain their
// actual owners without walking/copying every body on a one-request CLI run.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GroupingInput {
    lir: zil_ir::LirModule,
    entry: crate::EntryPoint,
    count: usize,
    classes: BTreeSet<String>,
    imports: CallImportPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitInputs {
    owner: usize,
    slots: BTreeMap<(usize, u8), usize>,
    names: BTreeMap<String, String>,
}
pub(super) struct Reuse<'a> {
    previous: &'a Materialization,
    reused_items: std::cell::Cell<usize>,
    same: Vec<bool>,
    source_indices: Vec<Option<usize>>,
    unit_indices: Vec<Option<usize>>,
    changed_names: Vec<BTreeSet<String>>,
}
impl Materialization {
    pub(super) fn new(
        source: Module,
        layout: &layout::Layout,
        units: &[Module],
        reused_items: usize,
    ) -> Self {
        Self {
            reused_items,
            constructed_items: units
                .iter()
                .map(|unit| unit.items.len())
                .sum::<usize>()
                .saturating_sub(reused_items),
            source,
            units: units.to_vec(),
            inputs: OnceLock::new(),
            calls: OnceLock::new(),
            layout: Arc::new(layout.clone()),
            graph: None,
            grouping: OnceLock::new(),
            grouping_input: None,
            imports: CallImportPolicy::Complete,
            completed_roles: false,
        }
    }
    pub(super) fn import_policy(&self) -> CallImportPolicy {
        self.imports
    }
    pub(super) fn with_import_policy(mut self, imports: CallImportPolicy) -> Self {
        self.imports = imports;
        self
    }
    fn inputs(&self) -> &[UnitInputs] {
        self.inputs.get_or_init(|| {
            self.layout
                .units
                .iter()
                .map(|unit| {
                    let mut slots = BTreeMap::new();
                    let mut next = 0;
                    for (indices, role) in
                        [(&unit.prelude, PRELUDE), (&unit.declarations, DECLARATION)]
                    {
                        for &index in indices {
                            slots.insert((index, role), next);
                            next += 1;
                        }
                    }
                    for &index in &unit.imports {
                        slots.insert((index, IMPORT_DECLARATION), next);
                        slots.insert((index, IMPORT_BODY), next + 1);
                        next += 2;
                    }
                    for &index in &unit.definitions {
                        slots.insert((index, DEFINITION), next);
                        next += 1;
                    }
                    UnitInputs {
                        owner: unit.owner,
                        slots,
                        names: import_names(&self.source, unit),
                    }
                })
                .collect()
        })
    }
    fn calls(&self, index: usize) -> Option<&BTreeSet<String>> {
        if let Some(graph) = self.graph() {
            return Some(graph.direct_calls(index));
        }
        self.calls
            .get_or_init(|| {
                self.source
                    .items
                    .iter()
                    .map(|item| {
                        Arc::new(match item {
                            Item::Function(function) => direct_calls(function),
                            _ => BTreeSet::new(),
                        })
                    })
                    .collect()
            })
            .get(index)
            .map(Arc::as_ref)
    }
    pub(super) fn defer_grouping(
        mut self,
        lir: &zil_ir::LirView<'_>,
        entry: crate::EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
    ) -> Self {
        if self.grouping.get().is_none() && self.graph.is_some() {
            self.grouping_input = Some(Arc::new(GroupingInput {
                lir: zil_ir::LirModule {
                    schema_version: lir.schema_version,
                    module_id: lir.module_id,
                    verified_fingerprint: zil_base::Fingerprint::ZERO,
                    target: lir.target.clone(),
                    functions: lir.functions.clone(),
                    closures: vec![],
                    resources: vec![],
                    layouts: vec![],
                },
                entry,
                count,
                classes: classes.clone(),
                imports: self.imports,
            }));
        }
        self
    }
    fn grouping(&self) -> Option<&layout::Grouping> {
        self.grouping
            .get_or_init(|| {
                let input = self.grouping_input.as_ref()?;
                layout::Grouping::read_with_policy(
                    &self.source,
                    &input.lir.as_view(),
                    input.entry,
                    input.count,
                    &input.classes,
                    self.graph()?,
                    input.imports,
                )
                .map(Arc::new)
            })
            .as_deref()
    }
    #[cfg(test)]
    pub(super) fn has_reuse_inventory(&self) -> bool {
        self.inputs.get().is_some() || self.calls.get().is_some() || self.grouping.get().is_some()
    }
    pub(super) fn with_graph(
        source: Module,
        layout: &layout::Layout,
        units: &[Module],
        reused_items: usize,
        graph: layout::CurrentGraph,
        grouping: Option<layout::Grouping>,
    ) -> Self {
        let mut product = Self::new(source, layout, units, reused_items);
        product.graph = Some(Arc::new(graph));
        if let Some(grouping) = grouping {
            let _ = product.grouping.set(Some(Arc::new(grouping)));
        }
        product
    }
    pub(super) fn graph(&self) -> Option<&layout::CurrentGraph> {
        self.graph.as_deref()
    }
    pub(super) fn completed_with_graph(
        source: Module,
        layout: &layout::Layout,
        units: &[Module],
        reused_items: usize,
        graph: layout::CurrentGraph,
        grouping: Option<layout::Grouping>,
    ) -> Self {
        let mut product = Self::with_graph(source, layout, units, reused_items, graph, grouping);
        product.completed_roles = true;
        product
    }
    pub(super) fn has_completed_roles(&self) -> bool {
        self.completed_roles
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn adapt_layout(
        &self,
        current: &Module,
        lir: &zil_ir::LirView<'_>,
        entry: crate::EntryPoint,
        count: usize,
        classes: &BTreeSet<String>,
        graph: &layout::CurrentGraph,
        grouping: &layout::Grouping,
    ) -> Option<Option<layout::Layout>> {
        if !self.completed_roles || !self.grouping()?.same_source_environment(grouping) {
            return None;
        }
        // This adapter handles a changed class domain. An unrelated current
        // call-graph change with the same classes still requires the original
        // planner and cannot lend this recipe's role-to-slot associations.
        if !self
            .grouping()?
            .class_inventory_changed(&self.source, classes)
        {
            return Some(None);
        }
        Some(self.layout.adapt_classes(
            &self.source,
            self.graph()?,
            current,
            lir,
            entry,
            count,
            classes,
            graph,
            self.imports,
        ))
    }
    pub(super) fn layout_for(&self, grouping: &layout::Grouping) -> Option<&layout::Layout> {
        (self.grouping() == Some(grouping)).then_some(self.layout.as_ref())
    }
    pub(super) fn bind<'a>(
        &'a self,
        current: &Module,
        layout: &layout::Layout,
    ) -> Option<Reuse<'a>> {
        if self.source.triple != current.triple {
            return None;
        }
        let mut old_names = BTreeMap::new();
        for (index, item) in self.source.items.iter().enumerate() {
            if old_names.insert(item_key(item), index).is_some() {
                return None;
            }
        }
        let source_indices = current
            .items
            .iter()
            .map(|item| old_names.get(&item_key(item)).copied())
            .collect::<Vec<_>>();
        let unit_indices = layout
            .units
            .iter()
            .map(|unit| self.inputs().iter().position(|old| old.owner == unit.owner))
            .collect::<Vec<_>>();
        // Full values authorize raw item reuse; position and hashes alone do not.
        let same = current
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                source_indices[index].is_some_and(|old| self.source.items.get(old) == Some(item))
            })
            .collect();
        let changed_names = layout
            .units
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                let names = import_names(current, unit);
                let Some(old) = unit_indices[index].and_then(|index| self.inputs().get(index))
                else {
                    return names.into_keys().collect();
                };
                names
                    .keys()
                    .chain(old.names.keys())
                    .filter(|name| names.get(*name) != old.names.get(*name))
                    .cloned()
                    .collect()
            })
            .collect();
        Some(Reuse {
            previous: self,
            reused_items: std::cell::Cell::new(0),
            same,
            source_indices,
            unit_indices,
            changed_names,
        })
    }
}
impl Reuse<'_> {
    pub(super) fn reused_items(&self) -> usize {
        self.reused_items.get()
    }
    pub(super) fn unit(&self, index: usize, unit: &layout::Unit) -> Option<Module> {
        let prior = self.unit_indices.get(index).copied().flatten()?;
        let old = self.previous.layout.units.get(prior)?;
        if old.owner != unit.owner
            || [
                (&unit.prelude, &old.prelude),
                (&unit.declarations, &old.declarations),
                (&unit.imports, &old.imports),
                (&unit.definitions, &old.definitions),
            ]
            .iter()
            .any(|(current, old)| {
                current.len() != old.len()
                    || current.iter().zip(old.iter()).any(|(&current, &old)| {
                        self.source_indices.get(current) != Some(&Some(old))
                    })
            })
            || !unit
                .prelude
                .iter()
                .chain(&unit.declarations)
                .chain(&unit.imports)
                .chain(&unit.definitions)
                .all(|&source| self.same.get(source) == Some(&true))
        {
            return None;
        }
        // Every source value and every role/name binding is equal. Retain the
        // actual raw unit owner, including its declarations and import bodies.
        let module = self.previous.units.get(prior)?.clone();
        self.reused_items
            .set(self.reused_items.get() + module.items.len());
        Some(module)
    }
    pub(super) fn item(&self, unit: usize, owner: usize, index: usize, role: u8) -> Option<Item> {
        if !self.same.get(index).copied()? {
            return None;
        }
        let prior_unit = self.unit_indices.get(unit).copied().flatten()?;
        let index = self.source_indices.get(index).copied().flatten()?;
        let input = self.previous.inputs().get(prior_unit)?;
        if input.owner != owner {
            return None;
        }
        let changed = &self.changed_names[unit];
        if matches!(role, IMPORT_BODY | DEFINITION)
            && self
                .previous
                .calls(index)?
                .iter()
                .any(|name| changed.contains(name))
        {
            return None;
        }
        // An imported body's own private symbol also depends on its current role.
        if role == IMPORT_BODY
            && let Item::Function(function) = self.previous.source.items.get(index)?
            && changed.contains(&function.name)
        {
            return None;
        }
        let item = self
            .previous
            .units
            .get(prior_unit)?
            .items
            .get(*input.slots.get(&(index, role))?)?
            .clone();
        self.reused_items.set(self.reused_items.get() + 1);
        Some(item)
    }
}
fn item_key(item: &Item) -> (u8, &str) {
    match item {
        Item::NamedType(name, _) => (0, name),
        Item::Function(function) => (1, &function.name),
        Item::Global(global) => (1, &global.name),
    }
}
fn import_names(module: &Module, unit: &layout::Unit) -> BTreeMap<String, String> {
    unit.imports
        .iter()
        .filter_map(|&index| {
            let Item::Function(function) = module.items.get(index)? else {
                return None;
            };
            Some((
                function.name.clone(),
                format!("zil.import.{}.{}", unit.owner, function.name),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Function, Inst, Linkage, Op, Signature, Ty, Val};

    fn function(name: &str, callee: Option<&str>) -> Item {
        let mut instructions = Vec::new();
        let value = if let Some(callee) = callee {
            instructions.push(Inst::new(
                Some("value".into()),
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global(callee.into()),
                    args: vec![],
                    vararg: None,
                },
            ));
            Val::Local("value".into())
        } else {
            Val::Int(7)
        };
        instructions.push(Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(32), value))),
        ));
        Item::Function(Function {
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
        })
    }
    fn fixture() -> (Module, layout::Layout) {
        let module = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "old".into(),
            items: vec![
                function("zil.fn.0", Some("zil.fn.1")),
                function("zil.fn.1", None),
                function("main", Some("zil.fn.0")),
            ]
            .into(),
        };
        let layout = layout::Layout {
            units: vec![
                layout::Unit {
                    owner: 0,
                    prelude: vec![],
                    declarations: vec![],
                    imports: vec![1],
                    definitions: vec![0, 2],
                },
                layout::Unit {
                    owner: 1,
                    prelude: vec![],
                    declarations: vec![],
                    imports: vec![],
                    definitions: vec![1],
                },
            ],
        };
        (module, layout)
    }
    fn named<'a>(module: &'a Module, name: &str) -> &'a Function {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name == name => Some(function),
                _ => None,
            })
            .unwrap()
    }
    #[test]
    fn changed_import_body_matches_fresh_and_keeps_equal_callers_shared() {
        let (source, layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        let mut current = source;
        current.fingerprint = "current".into();
        let Item::Function(function) = &mut current.items[1] else {
            unreachable!()
        };
        function.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(9))));
        let actual = layout.replay_reusing(current.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(current));
        assert!(
            named(&actual[0], "zil.fn.0")
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(named(&previous[0], "zil.fn.0").blocks.as_ref().unwrap())
        );
        assert!(
            !named(&actual[0], "zil.import.0.zil.fn.1")
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(
                    named(&previous[0], "zil.import.0.zil.fn.1")
                        .blocks
                        .as_ref()
                        .unwrap()
                )
        );
    }
    #[test]
    fn changed_import_binding_rewrites_an_unchanged_caller() {
        let (source, mut layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        layout.units[0].imports.clear();
        layout.units[0].declarations.push(1);
        let actual = layout.replay_reusing(source.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(source));
        assert!(
            !named(&actual[0], "zil.fn.0")
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(named(&previous[0], "zil.fn.0").blocks.as_ref().unwrap())
        );
        assert_eq!(
            direct_calls(named(&actual[0], "zil.fn.0")),
            BTreeSet::from(["zil.fn.1".into()])
        );
    }
    #[test]
    fn changed_owner_rebuilds_private_import_names_even_for_leaf_bodies() {
        let (source, mut layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        layout.units[0].owner = 7;
        let actual = layout.replay_reusing(source.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(source));
        assert_eq!(
            named(&actual[0], "zil.import.7.zil.fn.1").name,
            "zil.import.7.zil.fn.1"
        );
        assert_eq!(
            direct_calls(named(&actual[0], "zil.fn.0")),
            BTreeSet::from(["zil.import.7.zil.fn.1".into()])
        );
    }
    #[test]
    fn equal_complete_units_keep_actual_item_owners_and_current_diagnostic_identity() {
        let (source, layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        let mut current = source;
        current.fingerprint = "new-current-verification".into();
        let (actual, reused) = layout.replay_reusing_counted(current.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(current.clone()));
        assert_eq!(
            reused,
            previous.iter().map(|unit| unit.items.len()).sum::<usize>()
        );
        for (actual, previous) in actual.iter().zip(&previous) {
            assert!(actual.items.ptr_eq(&previous.items));
            assert_eq!(actual.fingerprint, current.fingerprint);
        }
    }
}
