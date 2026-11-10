#!/usr/bin/env python3
from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/machine_cgu/resource_helpers.rs');raw=p.read_bytes()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='83267452dc7a10603bde81fbb28bb193c1ac7a4c'
s=raw.decode()
old='use zil_llvm_api::model::{Item, Linkage, Module, Op, Val};\n'
new=old+'''
// A policy budget, not a promise about LLVM's eventual inlining decision.
// Large source bodies keep a canonical cross-unit call; copying all of them
// would invalidate the caller on every implementation edit and can grow code.
// Compiler-owned call-only helper closures remain completely available.
const MAX_IMPORTED_SOURCE_COST: usize = 32;
'''
assert s.count(old)==1;s=s.replace(old,new,1)
old='''                    if defs[id].calls.contains(name) && defs[target].function {
'''
new='''                    if defs[id].calls.contains(name)
                        && defs[target].function
                        && defs[target].cost <= MAX_IMPORTED_SOURCE_COST
                    {
'''
assert s.count(old)==1;s=s.replace(old,new,1);p.write_text(s)
p=Path('crates/zil-backend-llvm/src/machine_cgu.rs');s=p.read_text()
old='''    // modules into one object. The optional plan retains complete direct-call
    // visibility, identity-bearing local components and the original copy bound.
'''
new='''    // modules into one object. Keep full helper visibility, bounded small source
    // imports, identity-bearing local components and the original copy bound.
'''
assert s.count(old)==1;s=s.replace(old,new,1);p.write_text(s)
p=Path('crates/zil-backend-llvm/src/machine_cgu/resource_helpers/tests.rs');s=p.read_text()
assert 'large_source_bodies_keep_canonical_calls_and_independent_object_inputs' not in s
s+='''

#[test]
fn large_source_bodies_keep_canonical_calls_and_independent_object_inputs() {
    let lir = source(2);
    let mut input = module(vec![caller(0, "zil.fn.1"), caller(1, "helper"), helper("helper", 7)]);
    let Item::Function(function) = &mut input.items[1] else { unreachable!() };
    let mut index = 0;
    while function.cost() <= MAX_IMPORTED_SOURCE_COST {
        function.blocks.as_mut().unwrap()[0].instructions.insert(0, inst(
            Some(&format!("extra.{index}")),
            Op::Binary { opcode: "xor".into(), ty: Ty::Int(64), left: local("x"), right: Val::Int(index) },
        ));
        index += 1;
    }
    let before = planned(input.clone(), &lir, 8);
    assert_eq!(before.units.len(), 2);
    assert_eq!(symbol_count(&before.units[0], "zil.resource.import.0.zil.fn.1"), 0);
    assert!(before.units[0].items.iter().any(|item| matches!(item,
        Item::Function(f) if f.name == "zil.fn.1" && f.blocks.is_none())));
    assert_eq!(symbol_count(&before.units[1], "zil.fn.1"), 1);
    for unit in &before.units { zil_llvm_api::validate_module(unit).unwrap(); }

    // This changes the actual returned result, not merely unreachable padding.
    // The caller's prototype and instruction stream must remain identical.
    let Item::Function(function) = &mut input.items[1] else { unreachable!() };
    *function.blocks.as_mut().unwrap()[0].instructions.last_mut().unwrap() =
        inst(None, Op::Return(Some(Arg::new(Ty::Int(64), Val::Int(99)))));
    let after = planned(input, &lir, 8);
    assert_eq!(before.units[0], after.units[0]);
    assert_ne!(before.units[1], after.units[1]);
}
'''
p.write_text(s)
print('Source imports bounded to32 machine instruction/block units; original helpers, identities and total copy guard unchanged')
