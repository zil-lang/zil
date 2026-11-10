//! Anonymous callable bodies have no source-level export name. Internalize only
//! after partition, and only when every actual reference stays in the owner unit.
//! A passed function address still denotes the one original definition; no body
//! is cloned, no inline boundary or ownership/aliasing fact is added.
use std::collections::BTreeSet;
use zil_ir::ClosureDefinition;
use zil_llvm_api::model::Module;

pub(crate) fn internalize(units: &mut [Module], closures: &[ClosureDefinition]) -> usize {
    if closures.is_empty() {
        return 0;
    }
    let candidates = closures
        .iter()
        .flat_map(|closure| {
            [
                format!("zil.fn.{}", closure.function.0),
                format!("zil.fn.{}.lc1", closure.function.0),
            ]
        })
        .collect::<BTreeSet<_>>();
    // The existing reference-complete ownership check covers calls, addresses,
    // globals and imported bodies. Cross-unit users retain the external symbol;
    // duplicate definitions leave the model unchanged for mandatory preflight.
    crate::machine_cgu::internalize_closed_executable_names(units, &candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_ir::{CallableKind, ClosureId, FunctionId, Type};
    use zil_llvm_api::model::{self as m, Arg, Block, Function, Global, Inst, Item, Linkage, Op, Signature, Ty, Val};

    fn closure(id: u32) -> ClosureDefinition {
        ClosureDefinition {
            storage_life: None,
            id: ClosureId(id), function: FunctionId(id), captures: vec![Type::I64],
            parameters: vec![], result: Type::I32, kind: CallableKind::Fn,
            lending_captures: vec![],
        }
    }
    fn function(name: &str) -> Item {
        Item::Function(Function {
            name: name.into(), linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]), attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions: vec![Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))))],
            }]),
        })
    }
    fn module(items: Vec<Item>) -> Module {
        Module { triple: "x86_64-unknown-linux-gnu".into(), fingerprint: String::new(), items }
    }
    fn linkage(unit: &Module, name: &str) -> Linkage {
        unit.items.iter().find_map(|i| match i {
            Item::Function(f) if f.name==name => Some(f.linkage), _=>None,
        }).unwrap()
    }
    #[test]
    fn anonymous_local_linkage_preserves_bodies_addresses_and_named_exports() {
        let table=Item::Global(Global {
            name:"exported_callbacks".into(), linkage:Linkage::External,
            ty:Ty::array(2,Ty::Ptr), constant:true,
            initializer:Some(Val::Array(vec![Arg::new(Ty::Ptr,m::global("zil.fn.1".into())),Arg::new(Ty::Ptr,m::global("zil.fn.1".into()))])),
            align:8,
        });
        let original=module(vec![function("zil.fn.0"),function("zil.fn.1"),function("zil.fn.2.lc1"),table,function("zil.fn.999")]);
        let mut units=vec![original.clone()];
        assert_eq!(internalize(&mut units,&[closure(1),closure(2)]),2);
        assert_eq!(linkage(&units[0],"zil.fn.0"),Linkage::External);
        assert_eq!(linkage(&units[0],"zil.fn.999"),Linkage::External);
        for index in [1,2] {
            let Item::Function(actual)=&mut units[0].items[index] else {unreachable!()};
            assert_eq!(actual.linkage,Linkage::Internal);
            actual.linkage=Linkage::External;
        }
        assert_eq!(units[0],original,"only closure linkage may change, including repeated address identity");
        assert_eq!(internalize(&mut units,&[]),0);
    }
    #[test]
    fn cross_unit_calls_addresses_and_optimizer_imports_keep_canonical_symbols() {
        let body=function("zil.fn.1");
        let Item::Function(f)=&body else {unreachable!()};
        let mut caller=function("caller");
        let Item::Function(c)=&mut caller else {unreachable!()};
        c.blocks.as_mut().unwrap()[0].instructions.insert(0,Inst::new(Some("call".into()),Op::Call {
            result:Ty::Int(32),result_attrs:vec![],callee:m::global("zil.fn.1".into()),args:vec![],vararg:None,
        }));
        let table=Item::Global(Global {
            name:"foreign_callback_slot".into(),linkage:Linkage::External,ty:Ty::Ptr,constant:false,
            initializer:Some(m::global("zil.fn.1".into())),align:8,
        });
        for user in [caller,table] {
            let mut units=vec![module(vec![body.clone()]),module(vec![Item::Function(f.declaration()),user])];
            let original=units.clone();assert_eq!(internalize(&mut units,&[closure(1)]),0);assert_eq!(units,original);
        }
        let mut imported=f.clone();imported.linkage=Linkage::AvailableExternally;
        let mut units=vec![module(vec![body.clone()]),module(vec![Item::Function(imported)])];
        assert_eq!(internalize(&mut units,&[closure(1)]),1);
        assert_eq!(linkage(&units[1],"zil.fn.1"),Linkage::AvailableExternally);
    }
    #[test]
    fn duplicate_definitions_remain_visible_to_the_existing_preflight() {
        let mut units=vec![module(vec![function("zil.fn.1")]),module(vec![function("zil.fn.1")])];
        let original=units.clone();assert_eq!(internalize(&mut units,&[closure(1)]),0);assert_eq!(units,original);
        let mut units=vec![module(vec![function("zil.fn.1")])];
        assert_eq!(internalize(&mut units,&[closure(1)]),1);
        assert_eq!(internalize(&mut units,&[closure(1)]),0);
    }
}
