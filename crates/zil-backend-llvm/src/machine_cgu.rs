//! Partition the owned machine IR by semantic symbol references, never by parsing
//! LLVM assembly. Local-linkage definitions and all users remain co-located.
use crate::EntryPoint;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{FileId, Fingerprint};
use zil_ir::LirModule;
use zil_llvm_api::model::{Item, Module};

pub(crate) struct Plan {
    pub units: Vec<Module>,
    pub fallback: Option<&'static str>,
}
struct Definition {
    name: String,
    index: usize,
    local: bool,
    refs: BTreeSet<String>,
}

pub(crate) fn partition(module: Module, lir: &LirModule, entry: EntryPoint, count: usize) -> Plan {
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
    let mut symbols = BTreeSet::new();
    for (index, item) in module.items.iter().enumerate() {
        // Removing declarations must not hide a collision with a definition in
        // a different unit. Leave the original module to the ordinary validator.
        let symbol = match item {
            Item::Function(f) => Some(&f.name),
            Item::Global(g) => Some(&g.name),
            Item::NamedType(..) => None,
        };
        if let Some(symbol) = symbol
            && !symbols.insert(symbol)
        {
            return fallback(module, "duplicate machine definition or declaration");
        }
        match item {
            Item::Function(f) if f.blocks.is_some() => defs.push(Definition {
                name: f.name.clone(),
                index,
                local: f.linkage.is_local(),
                refs: f.references(),
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
    let mut files = BTreeMap::<FileId, usize>::new();
    let mut source_ids = Vec::new();
    for function in &lir.functions {
        if function.foreign.is_some() {
            continue;
        }
        let name = format!("zil.fn.{}", function.id.0);
        let Some(&id) = names.get(name.as_str()) else {
            return fallback(module, "source grouping missing machine definition");
        };
        if let Some(&first) = files.get(&function.span.file) {
            sets.join(first, id);
        } else {
            files.insert(function.span.file, id);
        }
        source_ids.push((function.span.file.0, id));
        let selected = match entry {
            EntryPoint::Main => function.name == "main",
            EntryPoint::ReturnValue(wanted) => function.id == wanted,
            EntryPoint::None => false,
        };
        if selected && let Some(&wrapper) = names.get("main") {
            sets.join(id, wrapper);
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
    let roots = (0..defs.len()).map(|id| sets.root(id)).collect::<Vec<_>>();
    let mut source_keys = BTreeMap::<usize, u32>::new();
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
                |file| *file as usize % count,
            )
        });
    }
    let owners = roots.iter().map(|r| buckets[r]).collect::<Vec<_>>();
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
    // Only signatures needed across unit boundaries get copied. Definitions are moved.
    let mut prototypes = BTreeMap::new();
    for (id, d) in defs.iter().enumerate() {
        for symbol in &d.refs {
            if let Some(&dependency) = names.get(symbol.as_str())
                && owners[dependency] != owners[id]
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
    let mut original = module.items.into_iter().map(Some).collect::<Vec<_>>();
    let prelude = prelude
        .into_iter()
        .map(|index| original[index].take().expect("unique prelude item"))
        .collect::<Vec<_>>();
    let mut units = Vec::with_capacity(members.len());
    for (owner, definitions) in members {
        let referenced = definitions
            .iter()
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
        for &id in &definitions {
            needed.extend(
                defs[id]
                    .refs
                    .iter()
                    .filter_map(|name| names.get(name.as_str()).copied()),
            );
        }
        for id in needed {
            if owners[id] != owner {
                debug_assert!(!defs[id].local);
                items.push(prototypes[&id].clone());
            }
        }
        items.extend(definitions.iter().map(|&id| {
            original[defs[id].index]
                .take()
                .expect("definition has one owner")
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
}
