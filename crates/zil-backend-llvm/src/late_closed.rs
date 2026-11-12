//! Current whole-CGU closure for the explicit late closed-executable contract.
//! Nonforeign spelling identifies generated definitions only; the caller's
//! explicit contract, not that spelling, removes external ZIL-export roots.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use zil_ir::LirView;
use zil_llvm_api::{
    LateClosedPlan,
    model::{Item, Linkage, Module, Op},
};

pub(crate) fn plans(units: &[Module], lir: &LirView<'_>) -> Vec<Arc<LateClosedPlan>> {
    let permitted = lir
        .functions
        .iter()
        .filter(|f| f.foreign.is_none() && f.environment.is_none())
        .flat_map(|f| {
            [
                format!("zil.fn.{}", f.id.0),
                format!("zil.fn.{}.lc1", f.id.0),
            ]
        })
        .collect::<BTreeSet<_>>();
    plans_for_names(units, &permitted)
}
fn plans_for_names(units: &[Module], permitted: &BTreeSet<String>) -> Vec<Arc<LateClosedPlan>> {
    let mut owners = BTreeMap::new();
    let mut invalid = false;
    for (unit, module) in units.iter().enumerate() {
        for item in &module.items {
            if let Item::Function(f) = item
                && f.blocks.is_some()
                && f.linkage == Linkage::External
                && permitted.contains(&f.name)
                && owners.insert(f.name.clone(), unit).is_some()
            {
                invalid = true;
            }
        }
    }
    let mut excluded = BTreeSet::new();
    for (unit, module) in units.iter().enumerate() {
        for item in &module.items {
            match item {
                Item::Function(f) => {
                    // Includes available_externally implementation imports.
                    for name in f.references() {
                        if owners.get(&name).is_some_and(|&owner| owner != unit) {
                            excluded.insert(name);
                        }
                    }
                    for i in f.blocks.iter().flatten().flat_map(|b| &b.instructions) {
                        for value in i.op.values() {
                            if matches!(&i.op, Op::Call { callee, .. }
                                if std::ptr::eq(value, callee) && matches!(callee, zil_llvm_api::model::Val::Global(_)))
                            {
                                continue;
                            }
                            value.symbols(&mut excluded);
                        }
                    }
                }
                Item::Global(g) => {
                    if let Some(v) = &g.initializer {
                        v.symbols(&mut excluded);
                    }
                }
                Item::NamedType(..) => {}
            }
        }
    }
    units
        .iter()
        .enumerate()
        .map(|(unit, module)| {
            let names = owners
                .iter()
                .filter(|(n, owner)| !invalid && **owner == unit && !excluded.contains(*n))
                .map(|(name, _)| name.clone());
            Arc::new(LateClosedPlan::new(module, names))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Function, Global, Inst, Signature, Ty, Val};
    fn f(name: &str, value: Val) -> Item {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), value))),
                    )],
                }]
                .into(),
            ),
        })
    }
    fn caller(target: &str) -> Item {
        let Item::Function(mut f) = f("zil.fn.2", Val::Local("v".into())) else {
            unreachable!()
        };
        f.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
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
        );
        Item::Function(f)
    }
    fn module(items: Vec<Item>) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "diagnostic".into(),
            items: items.into(),
        }
    }
    fn selected(units: &[Module]) -> Vec<Vec<String>> {
        plans_for_names(
            units,
            &["zil.fn.1".into(), "zil.fn.2".into()].into_iter().collect(),
        )
        .iter()
        .map(|p| p.candidates().to_vec())
        .collect()
    }
    #[test]
    fn exact_current_cross_unit_and_address_closure_preserves_external_roots() {
        let leaf = f("zil.fn.1", Val::Int(7));
        assert_eq!(
            selected(&[module(vec![leaf.clone(), caller("zil.fn.1")])]),
            vec![vec!["zil.fn.1", "zil.fn.2"]]
        );
        assert_eq!(
            selected(&[module(vec![leaf.clone()]), module(vec![caller("zil.fn.1")])]),
            vec![Vec::<String>::new(), vec!["zil.fn.2".into()]]
        );
        let mut imported = caller("zil.fn.1");
        if let Item::Function(f) = &mut imported {
            f.linkage = Linkage::AvailableExternally;
        }
        assert!(
            selected(&[module(vec![leaf.clone()]), module(vec![imported])])
                .iter()
                .all(Vec::is_empty)
        );
        let address = Item::Global(Global {
            name: "table".into(),
            linkage: Linkage::External,
            ty: Ty::Ptr,
            initializer: Some(Val::Global("zil.fn.1".into())),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        });
        assert!(selected(&[module(vec![leaf.clone(), address])])[0].is_empty());
        assert!(
            selected(&[module(vec![leaf.clone()]), module(vec![leaf])])
                .iter()
                .all(Vec::is_empty)
        );
        assert!(selected(&[module(vec![f("foreign", Val::Int(0))])])[0].is_empty());
    }
}
