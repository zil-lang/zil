#!/usr/bin/env python3
from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/machine_cgu/resource_helpers.rs');raw=p.read_bytes()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='a1bb52c3c585a65d248b32cbe4819e2aa44a7ee3'
s=raw.decode()
old='''    let mut first = BTreeMap::new();
'''
new='''    // Preserve the optimizer neighborhood of a branch-only return dispatch.
    // Same-argument alternatives may collapse after body merging; separating
    // those large implementations would turn that into real dispatch overhead.
    // This is a conservative placement affinity, never a semantic equivalence
    // assertion and never an input-name or benchmark-specific classification.
    for &id in &source {
        let Item::Function(function) = &module.items[defs[id].item] else { continue; };
        let mut alternatives: Vec<(&[zil_llvm_api::model::Arg], &zil_llvm_api::model::Ty, Vec<usize>)> = Vec::new();
        for block in function.blocks.iter().flatten() {
            let [call, returned] = block.instructions.as_slice() else { continue; };
            let Op::Call { callee: Val::Global(name), args, result, .. } = &call.op else { continue; };
            let Op::Return(Some(value)) = &returned.op else { continue; };
            let Some(local) = &call.result else { continue; };
            if value.ty != *result || value.value != Val::Local(local.clone()) { continue; }
            let Some(&target) = names.get(name) else { continue; };
            if !source.contains(&target) || defs[target].cost <= MAX_IMPORTED_SOURCE_COST { continue; }
            spend(&mut fuel, alternatives.len().checked_add(args.len())?.checked_add(1)?)?;
            if let Some((_, _, targets)) = alternatives.iter_mut().find(|(prior, ty, _)| *prior == args.as_slice() && *ty == result) {
                if !targets.contains(&target) { targets.push(target); }
            } else {
                alternatives.push((args, result, vec![target]));
            }
        }
        for (_, _, targets) in alternatives {
            if targets.len() > 1 {
                for target in targets { sets.join(id, target); }
            }
        }
    }
    let mut first = BTreeMap::new();
'''
assert s.count(old)==1;s=s.replace(old,new,1);p.write_text(s)
p=Path('crates/zil-backend-llvm/src/machine_cgu/resource_helpers/tests.rs');s=p.read_text()
s+='''

#[test]
fn same_argument_return_dispatch_preserves_optimizer_locality_not_function_identity() {
    let mut dispatch = function("zil.fn.0", Linkage::External, vec![]);
    let Item::Function(function) = &mut dispatch else { unreachable!() };
    function.blocks = Some(vec![
        Block { name: "entry".into(), instructions: vec![
            inst(Some("test"), Op::Compare { predicate: "eq".into(), ty: Ty::Int(64), left: local("x"), right: Val::Int(0) }),
            inst(None, Op::CondBranch { condition: local("test"), yes: "left".into(), no: "right".into() }),
        ] },
        Block { name: "left".into(), instructions: vec![inst(Some("a"), call("zil.fn.1")), inst(None, Op::Return(Some(Arg::new(Ty::Int(64), local("a")))))] },
        Block { name: "right".into(), instructions: vec![inst(Some("b"), call("zil.fn.2")), inst(None, Op::Return(Some(Arg::new(Ty::Int(64), local("b")))))] },
    ]);
    let mut input = module(vec![dispatch, caller(1, "helper"), caller(2, "helper"), caller(3, "helper"), helper("helper", 7)]);
    for id in [1,2] {
        let Item::Function(function) = &mut input.items[id] else { unreachable!() };
        let mut index = 0;
        while function.cost() <= MAX_IMPORTED_SOURCE_COST {
            function.blocks.as_mut().unwrap()[0].instructions.insert(0, inst(Some(&format!("extra.{index}")),
                Op::Binary { opcode: "xor".into(), ty: Ty::Int(64), left: local("x"), right: Val::Int(index) }));
            index += 1;
        }
    }
    let lir = source(4);
    let plan = planned(input.clone(), &lir, 8);
    let owner = plan.units.iter().find(|u| symbol_count(u,"zil.fn.0")==1).unwrap();
    assert_eq!(symbol_count(owner,"zil.fn.1"),1);
    assert_eq!(symbol_count(owner,"zil.fn.2"),1);
    assert_eq!(symbol_count(owner,"zil.fn.3"),0);
    assert_eq!(plan.units.len(),2);
    for unit in &plan.units { zil_llvm_api::validate_module(unit).unwrap(); }
    // Different arguments do not receive the same affinity: preserve ordinary
    // independent units rather than assuming these calls can be collapsed.
    let Item::Function(function) = &mut input.items[0] else { unreachable!() };
    let Op::Call {args, ..} = &mut function.blocks.as_mut().unwrap()[2].instructions[0].op else { unreachable!() };
    args[0].value = Val::Int(4);
    let plan = planned(input, &lir, 8);
    let owner = plan.units.iter().find(|u| symbol_count(u,"zil.fn.0")==1).unwrap();
    assert_eq!(symbol_count(owner,"zil.fn.1"),0);
    assert_eq!(symbol_count(owner,"zil.fn.2"),0);
    assert_eq!(plan.units.len(),4);
    for unit in &plan.units { zil_llvm_api::validate_module(unit).unwrap(); }
}
'''
p.write_text(s)
p=Path('docs/guide/resource-helper-partition.md');s=p.read_text()
needle='\n\n导入的源实现使用私有副本'
text='''\n\n对于多个基本块中“用相同实参调用不同大实现，然后直接返回其结果”的尾分派，调用者与这些实现保持同组，让LLVM继续观察相同实现合并、分支合并等机会。这是保守的代码布局亲和规则，不宣称这些函数语义相等，也不合并它们的身份；不同实参或经过后续运算的普通串联调用不因该规则合组。该类分派因此可能维持原有重编范围，不能对其承诺与独立模块一样的增量收益。\n\n导入的源实现使用私有副本'''
assert s.count(needle)==1;s=s.replace(needle,text,1);p.write_text(s)
print('Added general same-argument tail-dispatch locality; semantic and benchmark inputs unchanged')
