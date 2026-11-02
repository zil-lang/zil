//! Source -> ordinary TypedIR -> independently checked raw/Wire candidates.
//! The test candidate supplies only definite initialization and coarse Hold rows;
//! Analyze is deliberately never called, including for positive controls.
use std::collections::BTreeSet;
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;

fn candidate_holds(f: &TypedFunction) -> Vec<ValueHold> {
    let tracked = |ty: &Type| {
        matches!(ty, Type::Address { .. })
            || matches!(ty,Type::Desc(inner) if matches!(inner.as_ref(),Type::ByteBuffer|Type::OwnedStore{..}))
    };
    let mut values = vec![BTreeSet::new(); f.values.len()];
    let mut locals = vec![BTreeSet::new(); f.locals.len()];
    for (index, local) in f.parameters.iter().enumerate() {
        if tracked(&f.locals[local.0 as usize].ty) {
            locals[local.0 as usize].insert(HoldRoot::Parameter(index as u32));
        }
    }
    loop {
        let mut changed = false;
        for block in &f.blocks {
            for instruction in &block.instructions {
                if let InstructionKind::Store { local, value } = instruction.kind {
                    let old = locals[local.0 as usize].len();
                    locals[local.0 as usize].extend(values[value.0 as usize].iter().copied());
                    changed |= old != locals[local.0 as usize].len();
                }
                let Some(output) = instruction.result else {
                    continue;
                };
                if !tracked(&f.values[output.0 as usize]) {
                    continue;
                }
                let row = match instruction.kind {
                    InstructionKind::DescribeLocal { local }
                    | InstructionKind::DescribePackageField { local, .. }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::BorrowPackageField { local, .. }
                    | InstructionKind::BorrowOwnedArrayElement { local, .. }
                    | InstructionKind::ProjectOwnedArrayField { local, .. } => {
                        BTreeSet::from([HoldRoot::Local(local)])
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        locals[local.0 as usize].clone()
                    }
                    InstructionKind::DescribeCapture { index }
                    | InstructionKind::ReadCapture { index }
                    | InstructionKind::BorrowCapture { index, .. } => {
                        BTreeSet::from([HoldRoot::Capture(index)])
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => {
                        values[address.0 as usize].clone()
                    }
                    InstructionKind::ByteBufferBorrow { buffer, .. }
                    | InstructionKind::OwnedStoreBorrow { store: buffer, .. }
                    | InstructionKind::OwnedStoreBorrowMut { store: buffer, .. } => {
                        values[buffer.0 as usize].clone()
                    }
                    _ => BTreeSet::new(),
                };
                let old = values[output.0 as usize].len();
                values[output.0 as usize].extend(row);
                changed |= old != values[output.0 as usize].len();
            }
        }
        if !changed {
            break;
        }
    }
    values
        .into_iter()
        .enumerate()
        .map(|(i, roots)| ValueHold {
            value: ValueId(i as u32),
            roots: roots.into_iter().collect(),
        })
        .collect()
}

fn transfer(instructions: &[Instruction], state: &mut [bool]) {
    for instruction in instructions {
        match instruction.kind {
            InstructionKind::Store { local, .. }
            | InstructionKind::BindPackageCapture { local, .. } => state[local.0 as usize] = true,
            InstructionKind::UnpackVariant {
                local, ref fields, ..
            } => {
                state[local.0 as usize] = false;
                for target in fields {
                    state[target.0 as usize] = true;
                }
            }
            InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local)
            | InstructionKind::TakeLocal { local }
            | InstructionKind::DropManaged { local }
            | InstructionKind::DropPackage { local }
            | InstructionKind::DropEnum { local }
            | InstructionKind::DropErrorUnion { local }
            | InstructionKind::DropClosure { local }
            | InstructionKind::DropOwnedArray { local }
            | InstructionKind::EndPackageCapture { local, .. } => state[local.0 as usize] = false,
            _ => {}
        }
    }
}
pub(crate) fn candidate(module: &TypedModule) -> CandidatePlan {
    let mut plan = CandidatePlan::empty_for(module);
    for f in &module.functions {
        if f.foreign.is_some() {
            plan.functions.push(FunctionPlan {
                function: f.id,
                holds: vec![],
                blocks: vec![],
            });
            continue;
        }
        let n = f.blocks.len();
        let m = f.locals.len();
        let mut predecessors = vec![vec![]; n];
        let mut successors = vec![vec![]; n];
        for (index, block) in f.blocks.iter().enumerate() {
            match block.terminator {
                Terminator::Jump(to) => successors[index].push(to.0 as usize),
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    successors[index].push(then_block.0 as usize);
                    if then_block != else_block {
                        successors[index].push(else_block.0 as usize);
                    }
                }
                _ => {}
            }
            for &next in &successors[index] {
                predecessors[next].push(index);
            }
        }
        let mut reachable = vec![false; n];
        let mut pending = vec![f.entry.0 as usize];
        while let Some(block) = pending.pop() {
            if !std::mem::replace(&mut reachable[block], true) {
                pending.extend(&successors[block]);
            }
        }
        let mut parameters = vec![false; m];
        for p in &f.parameters {
            parameters[p.0 as usize] = true;
        }
        let mut ins = vec![vec![true; m]; n];
        let mut outs = ins.clone();
        for b in 0..n {
            if b == f.entry.0 as usize {
                ins[b] = parameters.clone();
            } else if !reachable[b] {
                ins[b].fill(false);
            }
            outs[b] = ins[b].clone();
            transfer(&f.blocks[b].instructions, &mut outs[b]);
        }
        loop {
            let mut changed = false;
            for b in 0..n {
                let mut row = if b == f.entry.0 as usize {
                    parameters.clone()
                } else {
                    vec![reachable[b]; m]
                };
                if b != f.entry.0 as usize && reachable[b] {
                    for &p in &predecessors[b] {
                        if reachable[p] {
                            for (bit, previous) in row.iter_mut().zip(&outs[p]) {
                                *bit &= *previous;
                            }
                        }
                    }
                }
                let mut output = row.clone();
                transfer(&f.blocks[b].instructions, &mut output);
                if row != ins[b] || output != outs[b] {
                    ins[b] = row;
                    outs[b] = output;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let ids = |row: &[bool]| {
            row.iter()
                .enumerate()
                .filter_map(|(i, bit)| bit.then_some(LocalId(i as u32)))
                .collect()
        };
        plan.functions.push(FunctionPlan {
            function: f.id,
            holds: candidate_holds(f),
            blocks: (0..n)
                .map(|b| BlockPlan {
                    block: BlockId(b as u32),
                    initialized_in: ids(&ins[b]),
                    initialized_out: ids(&outs[b]),
                })
                .collect(),
        });
    }
    plan
}

fn check(name: &str, text: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = candidate(&module);
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let path = std::env::temp_dir().join(format!("242-source-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&path).unwrap();
    let flat = module
        .functions
        .iter()
        .map(|f| {
            format!(
                "fn {} locals {:?}\n{}",
                f.name,
                f.locals.iter().map(|l| (l.id, &l.name)).collect::<Vec<_>>(),
                f.blocks
                    .iter()
                    .map(|b| format!(
                        "{:?}:\n{}\n{:?}\n",
                        b.id,
                        b.instructions
                            .iter()
                            .map(|i| format!("{:?} = {:?}", i.result, i.kind))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        b.terminator
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path.join("flat.txt"), flat).unwrap();
    std::fs::write(path.join("source.zil"), text).unwrap();
    std::fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
    std::fs::write(path.join("candidate.wire"), &bytes).unwrap();
    for result in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        let verified = result.unwrap_or_else(|e| panic!("{name} {path:?}: {e:?}"));
        assert!(verified.uses_owned_sum_extension());
        assert_eq!(verified.limits(), &limits);
    }
    eprintln!("242 source raw/Wire {name}: {}", path.display());
}

#[test]
fn original_q03_and_q05_reach_independent_verify_without_analyze() {
    check(
        "original03",
        include_str!("impl240_fixtures/original03.zil"),
    );
    check(
        "original05",
        include_str!("impl240_fixtures/original05.zil"),
    );
}
#[test]
fn right_nested_pattern_and_product_sum_error_payload_reach_independent_verify() {
    check("nested", include_str!("impl240_fixtures/nested.zil"));
    check("product", include_str!("impl240_fixtures/product.zil"));
}
#[test]
fn copy_payload_scratch_cleanup_precedes_pattern_publication() {
    check(
        "copy_scratch",
        include_str!("impl240_fixtures/copy_scratch.zil"),
    );
}

#[test]
fn ordinary_identity_and_factory_value_returns_do_not_grant_leaf_create() {
    const BASE: &str = "resource R{n:i32}enum E{Pair(manages R,manages R)}fn relay(e:E)->E{return e}fn factory()->E{return E.Pair(R{n:1},R{n:2})}";
    check(
        "direct_create",
        &format!(
            "{BASE}fn good()->R contract{{result(create)}}{{let e=E.Pair(R{{n:1}},R{{n:2}});match e{{E.Pair(a,b)=>{{return give b}}}}}}fn main(){{}}"
        ),
    );
    for (name, expression) in [
        ("identity", "relay(E.Pair(R{n:1},R{n:2}))"),
        ("factory", "factory()"),
    ] {
        let text = format!(
            "{BASE}fn bad()->R contract{{result(create)}}{{let e={expression};match e{{E.Pair(a,b)=>{{return give b}}}}}}fn main(){{}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), name, &text);
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &ast).unwrap();
        let module = zil_types::check(&core, &target, &limits).unwrap();
        let plan = candidate(&module);
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let path =
            std::env::temp_dir().join(format!("242-source-{}-create-{name}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("source.zil"), text).unwrap();
        std::fs::write(path.join("candidate.wire"), &bytes).unwrap();
        for result in [
            zil_verify::verify_module(module, plan, &target, &limits),
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ] {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|e| e.code == "E-VERIFY-RESOURCE"
                    && e.message.contains("not independently proven fresh")),
                "{name}: {errors:?}"
            );
        }
        eprintln!(
            "242 source ordinary {name} create rejected: {}",
            path.display()
        );
    }
}

#[test]
fn ordinary_helper_sum_data_parameters_reach_independent_raw_and_wire_verify() {
    const BASE: &str = r#"
resource R{n:i32}
enum E{Empty,Pair(manages R,manages R)}
package Wrap{state:E;ready:bool}
fn sink(give owner:R){}
fn make()->E{return E.Pair(R{n:65},R{n:66})}
"#;
    for (name, helper, argument) in [
        (
            "helper_enum_forwarded_owner",
            "fn use_value<F:Fn()->i32>(callback:F,value:E)->i32{let n=callback();match value{E.Empty()=>{return n}\nE.Pair(a,b)=>{let saved=give a;sink(give saved);sink(give b);return n}}}",
            "make()",
        ),
        (
            "helper_product_sum",
            "fn use_value<F:Fn()->i32>(callback:F,value:Wrap)->i32{return callback()}",
            "Wrap{state:make(),ready:true}",
        ),
        (
            "helper_error_union_sum",
            "fn fallible()->void!Wrap{return Wrap{state:make(),ready:true}}fn use_value<F:Fn()->i32>(callback:F,value:void!Wrap)->i32{return callback()}",
            "fallible()",
        ),
    ] {
        check(
            name,
            &format!("{BASE}{helper}fn main()->i32{{return use_value(()=>7,{argument})-7}}"),
        );
    }
}
