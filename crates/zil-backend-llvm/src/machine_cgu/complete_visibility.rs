//! Research-only proof derived from the unchanged complete-import planner.
//! No cached authority, new budget, or source-language permission is created.
use super::{CallImportPolicy, Plan, partition_view_cached, source_name};
use crate::EntryPoint;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::json_string;
use zil_ir::LirView;
use zil_llvm_api::model::{Function, Item, Linkage, Module, Op, Val};

#[derive(Clone)]
struct Fact {
    cost: usize,
    calls: BTreeSet<String>,
}
struct Unit {
    canonical: BTreeSet<String>,
    allowed: BTreeSet<String>,
}
pub(crate) struct Proof {
    facts: BTreeMap<String, Fact>,
    sources: BTreeSet<String>,
    foreign: BTreeSet<String>,
    source_cost: usize,
    original: Vec<Unit>,
}

/// Current-request facts produced only after the unchanged planner, owner,
/// closure and copy-budget checks below. The JSON is diagnostic, never input.
pub(crate) struct CheckedProof {
    pub(crate) receipt: String,
    pub(crate) units: Vec<CheckedUnit>,
}
pub(crate) struct CheckedUnit {
    pub(crate) canonical: BTreeSet<String>,
    pub(crate) allowed: BTreeSet<String>,
}

fn no_address(value: &Val) -> bool {
    match value {
        Val::Global(_) => false,
        Val::Array(args) => args.iter().all(|a| no_address(&a.value)),
        Val::Gep { ptr, indices, .. } => {
            no_address(ptr) && indices.iter().all(|a| no_address(&a.value))
        }
        _ => true,
    }
}
fn calls(function: &Function) -> Result<BTreeSet<String>, &'static str> {
    let mut calls = BTreeSet::new();
    for instruction in function
        .blocks
        .iter()
        .flatten()
        .flat_map(|b| &b.instructions)
    {
        match &instruction.op {
            Op::Call {
                callee: Val::Global(name),
                args,
                ..
            } => {
                if !args.iter().all(|a| no_address(&a.value)) {
                    return Err("function/global address argument outside pure research scope");
                }
                calls.insert(name.clone());
            }
            Op::Call { .. } => return Err("indirect call outside pure research scope"),
            op if !op.values().all(no_address) => {
                return Err("function/global address operand outside pure research scope");
            }
            _ => {}
        }
    }
    Ok(calls)
}
fn known_foreign(name: &str) -> bool {
    matches!(name, "getchar" | "putchar" | "llvm.trap")
        || [8, 16, 32, 64].into_iter().any(|bits| {
            ["sadd", "ssub", "smul", "uadd", "usub", "umul"]
                .into_iter()
                .any(|op| name == format!("llvm.{op}.with.overflow.i{bits}"))
        })
}

impl Proof {
    /// The caller has already performed complete current-machine preflight.
    pub(crate) fn prepare(
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
    ) -> Result<Self, &'static str> {
        if !lir.closures.is_empty() {
            return Err("closures outside pure research scope");
        }
        let names = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if let Item::Function(f) = item {
                    Some((f.name.as_str(), i))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();
        let sources = lir
            .functions
            .iter()
            .filter(|f| f.foreign.is_none())
            .map(|f| source_name(&names, f.id))
            .collect::<BTreeSet<_>>();
        let original = partition_view_cached(
            module.clone(),
            lir,
            entry,
            count,
            true,
            CallImportPolicy::for_incremental_o2(true, crate::FunctionMerging::AfterOptimization),
            None,
        )
        .0;
        Self::from_original(module, sources, original)
    }

    fn from_original(
        module: &Module,
        sources: BTreeSet<String>,
        original: Plan,
    ) -> Result<Self, &'static str> {
        let mut facts = BTreeMap::new();
        let mut foreign = BTreeSet::new();
        for item in &module.items {
            match item {
                Item::Function(f) if f.blocks.is_some() => {
                    if f.linkage != Linkage::External
                        || (!sources.contains(&f.name) && f.name != "main")
                    {
                        return Err("non-source or local definition outside pure research scope");
                    }
                    if facts
                        .insert(
                            f.name.clone(),
                            Fact {
                                cost: f.cost(),
                                calls: calls(f)?,
                            },
                        )
                        .is_some()
                    {
                        return Err("duplicate canonical definition");
                    }
                }
                Item::Function(f) if known_foreign(&f.name) => {
                    foreign.insert(f.name.clone());
                }
                Item::Function(_) => return Err("unknown declaration outside pure research scope"),
                Item::Global(_) => return Err("globals outside pure research scope"),
                Item::NamedType(..) => {}
            }
        }
        if !sources.iter().all(|name| facts.contains_key(name)) {
            return Err("missing current source definition");
        }
        for fact in facts.values() {
            if fact
                .calls
                .iter()
                .any(|name| !facts.contains_key(name) && !foreign.contains(name))
            {
                return Err("unknown direct-call target");
            }
        }
        let source_cost = sources
            .iter()
            .try_fold(0usize, |sum, name| sum.checked_add(facts[name].cost))
            .ok_or("source cost overflow")?;
        if original.fallback.is_some() {
            return Err("original complete planner fallback");
        }
        let mut original_units = Vec::new();
        for unit in &original.units {
            let mut canonical = BTreeSet::new();
            let mut allowed = BTreeSet::new();
            for item in &unit.items {
                let Item::Function(f) = item else { continue };
                if f.blocks.is_none() {
                    continue;
                }
                if facts.contains_key(&f.name) {
                    if !canonical.insert(f.name.clone()) {
                        return Err("duplicate planned owner");
                    }
                } else {
                    let rest = f
                        .name
                        .strip_prefix("zil.import.")
                        .ok_or("unknown planned body")?;
                    let (owner, name) =
                        rest.split_once('.').ok_or("invalid planned import name")?;
                    if owner.parse::<usize>().is_err()
                        || !sources.contains(name)
                        || f.linkage != Linkage::Internal
                        || f.cost() != facts[name].cost
                        || !allowed.insert(name.to_owned())
                    {
                        return Err("invalid or duplicate planned import");
                    }
                }
            }
            original_units.push(Unit { canonical, allowed });
        }
        Ok(Self {
            facts,
            sources,
            foreign,
            source_cost,
            original: original_units,
        })
    }

    pub(crate) fn finish(self, canonical_units: &[Module]) -> Result<String, &'static str> {
        self.finish_checked(canonical_units)
            .map(|proof| proof.receipt)
    }

    pub(crate) fn finish_checked(
        self,
        canonical_units: &[Module],
    ) -> Result<CheckedProof, &'static str> {
        let mut canonical = Vec::new();
        let mut owners = BTreeMap::new();
        for (index, unit) in canonical_units.iter().enumerate() {
            let mut names = BTreeSet::new();
            for item in &unit.items {
                if let Item::Function(f) = item
                    && f.blocks.is_some()
                {
                    if !self.facts.contains_key(&f.name)
                        || !names.insert(f.name.clone())
                        || owners.insert(f.name.clone(), index).is_some()
                    {
                        return Err("invalid canonical owner");
                    }
                }
            }
            if names.is_empty() {
                return Err("empty canonical owner unit");
            }
            canonical.push(names);
        }
        if owners.len() != self.facts.len() || canonical.len() != self.original.len() {
            return Err("canonical definitions or unit count changed");
        }
        let mut matched = BTreeSet::new();
        let mut copied_cost = 0usize;
        let mut receipts = Vec::new();
        let mut checked_units = Vec::new();
        for (index, names) in canonical.iter().enumerate() {
            let candidates = self
                .original
                .iter()
                .enumerate()
                .filter(|(_, unit)| &unit.canonical == names)
                .collect::<Vec<_>>();
            if candidates.len() != 1 || !matched.insert(candidates[0].0) {
                return Err("canonical owner sets differ from original planner");
            }
            let allowed = &candidates[0].1.allowed;
            let mut pending = names.iter().cloned().collect::<Vec<_>>();
            let mut visited = BTreeSet::new();
            let mut required = BTreeSet::new();
            while let Some(name) = pending.pop() {
                if !visited.insert(name.clone()) {
                    continue;
                }
                for target in &self.facts[&name].calls {
                    if self.foreign.contains(target) {
                        continue;
                    }
                    let owner = *owners.get(target).ok_or("unknown closure target")?;
                    if owner != index {
                        if !self.sources.contains(target) {
                            return Err("non-source cross-owner call");
                        }
                        required.insert(target.clone());
                    }
                    pending.push(target.clone());
                }
            }
            // This rejects the established planner's silent copy-budget fallback,
            // even if it returns eight ordinary units with fallback == None.
            if &required != allowed {
                return Err("missing or extra original planned import");
            }
            let mut unit_cost = 0usize;
            let mut imports = Vec::new();
            for name in allowed {
                let cost = self.facts[name].cost;
                unit_cost = unit_cost
                    .checked_add(cost)
                    .ok_or("unit copy cost overflow")?;
                copied_cost = copied_cost
                    .checked_add(cost)
                    .ok_or("total copy cost overflow")?;
                if copied_cost > self.source_cost {
                    return Err("original source copy budget exceeded");
                }
                imports.push(format!(
                    "{{\"name\":{},\"cost\":{cost}}}",
                    json_string(name)
                ));
            }
            receipts.push(format!("{{\"unit\":{index},\"canonical\":{},\"allowed\":[{}],\"required_original\":{},\"copy_cost\":{unit_cost}}}",
                strings(names), imports.join(","), strings(&required)));
            checked_units.push(CheckedUnit {
                canonical: names.clone(),
                allowed: allowed.clone(),
            });
        }
        let receipt = format!(
            "{{\"format\":\"zil-thinlto-complete-visibility/1\",\"original_source_cost\":{},\"total_copy_cost\":{copied_cost},\"foreign\":{},\"units\":[{}],\"current_complete_planner\":true,\"canonical_owners_equal\":true,\"original_required_exactly_allowed\":true}}",
            self.source_cost,
            strings(&self.foreign),
            receipts.join(",")
        );
        Ok(CheckedProof {
            receipt,
            units: checked_units,
        })
    }
}

fn strings(names: &BTreeSet<String>) -> String {
    format!(
        "[{}]",
        names
            .iter()
            .map(|name| json_string(name))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Inst, Signature, Ty};

    fn function(name: &str, targets: &[&str]) -> Item {
        let mut instructions = targets
            .iter()
            .map(|target| {
                Inst::new(
                    None,
                    Op::Call {
                        result: Ty::Void,
                        result_attrs: vec![],
                        callee: Val::Global((*target).into()),
                        args: vec![],
                        vararg: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        instructions.push(Inst::new(None, Op::Return(None)));
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions,
            }]),
        })
    }
    fn module(items: Vec<Item>) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "test".into(),
            items,
        }
    }
    fn imported(item: &Item, owner: usize) -> Item {
        let Item::Function(mut f) = item.clone() else {
            unreachable!()
        };
        f.name = format!("zil.import.{owner}.{}", f.name);
        f.linkage = Linkage::Internal;
        Item::Function(f)
    }
    fn example() -> (Module, BTreeSet<String>, Vec<Module>, Plan) {
        let a = function("zil.fn.0", &["zil.fn.1.lc1"]);
        let b = function("zil.fn.1.lc1", &[]);
        let original = module(vec![a.clone(), b.clone()]);
        let sources = ["zil.fn.0".into(), "zil.fn.1.lc1".into()]
            .into_iter()
            .collect();
        let canonical = vec![module(vec![a.clone()]), module(vec![b.clone()])];
        let full = Plan {
            units: vec![module(vec![a, imported(&b, 0)]), module(vec![b])],
            fallback: None,
        };
        (original, sources, canonical, full)
    }
    #[test]
    fn original_budgeted_visibility_preserves_exact_lc1_name() {
        let (original, sources, canonical, full) = example();
        let receipt = Proof::from_original(&original, sources, full)
            .unwrap()
            .finish(&canonical)
            .unwrap();
        assert!(receipt.contains("\"original_required_exactly_allowed\":true"));
        assert!(receipt.contains("\"name\":\"zil.fn.1.lc1\""));
    }
    #[test]
    fn silent_copy_fallback_with_same_units_and_no_fallback_is_rejected() {
        let (original, sources, canonical, _) = example();
        let fallback = Plan {
            units: canonical.clone(),
            fallback: None,
        };
        assert_eq!(
            Proof::from_original(&original, sources, fallback)
                .unwrap()
                .finish(&canonical)
                .err(),
            Some("missing or extra original planned import")
        );
    }
    #[test]
    fn changed_canonical_owners_and_extra_import_are_rejected() {
        let (original, sources, canonical, mut full) = example();
        let moved = full.units[1].items.remove(0);
        full.units[0].items.push(moved);
        assert_eq!(
            Proof::from_original(&original, sources, full)
                .unwrap()
                .finish(&canonical)
                .err(),
            Some("canonical owner sets differ from original planner")
        );
        let (mut original, mut sources, mut canonical, mut full) = example();
        let extra = function("zil.fn.2", &[]);
        original.items.push(extra.clone());
        sources.insert("zil.fn.2".into());
        canonical[1].items.push(extra.clone());
        full.units[1].items.push(extra.clone());
        full.units[0].items.push(imported(&extra, 0));
        assert_eq!(
            Proof::from_original(&original, sources, full)
                .unwrap()
                .finish(&canonical)
                .err(),
            Some("missing or extra original planned import")
        );
    }
    #[test]
    fn unknown_target_and_nested_address_are_rejected() {
        let (mut original, sources, _, full) = example();
        let Item::Function(f) = &mut original.items[0] else {
            unreachable!()
        };
        let Op::Call { callee, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[0].op else {
            unreachable!()
        };
        *callee = Val::Global("unknown".into());
        assert_eq!(
            Proof::from_original(&original, sources, full).err(),
            Some("unknown direct-call target")
        );
        let (mut original, sources, _, full) = example();
        let Item::Function(f) = &mut original.items[0] else {
            unreachable!()
        };
        let Op::Call { args, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[0].op else {
            unreachable!()
        };
        args.push(Arg::new(
            Ty::Ptr,
            Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("zil.fn.1.lc1".into()))]),
        ));
        assert_eq!(
            Proof::from_original(&original, sources, full).err(),
            Some("function/global address argument outside pure research scope")
        );
    }
    #[test]
    fn recursive_closure_terminates_and_copy_cost_is_per_unit() {
        let names = ["zil.fn.0", "zil.fn.1", "zil.fn.2"];
        let defs = names
            .iter()
            .enumerate()
            .map(|(i, name)| function(name, &[names[(i + 1) % 3]]))
            .collect::<Vec<_>>();
        let original = module(defs.clone());
        let sources = names.iter().map(|name| (*name).into()).collect();
        let canonical = defs
            .iter()
            .map(|item| module(vec![item.clone()]))
            .collect::<Vec<_>>();
        let full = Plan {
            units: defs
                .iter()
                .enumerate()
                .map(|(owner, item)| {
                    let mut items = vec![item.clone()];
                    items.extend(
                        defs.iter()
                            .enumerate()
                            .filter(|(i, _)| *i != owner)
                            .map(|(_, item)| imported(item, owner)),
                    );
                    module(items)
                })
                .collect(),
            fallback: None,
        };
        assert_eq!(
            Proof::from_original(&original, sources, full)
                .unwrap()
                .finish(&canonical)
                .err(),
            Some("original source copy budget exceeded")
        );
    }
}
