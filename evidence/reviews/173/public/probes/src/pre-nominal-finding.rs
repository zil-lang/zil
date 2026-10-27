#![cfg(test)]
use zil_base::{FileId,ModuleId,Limits,Span,TargetProfile,Fingerprint,SCHEMA_VERSION,ResultCategory,SourceUnit};
use zil_ir::*;
fn module_candidate() -> (TypedModule, CandidatePlan) {
    let span = Span::new(FileId(2), 4, 8);
    let module = TypedModule {
        resources: Vec::new(),
        closures: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(9),
        source_fingerprint: Fingerprint::of(b"candidate"),
        target: TargetProfile::host_llvm(),
        functions: vec![TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            result_mode: PassingMode::Infer,
            foreign: None,
            environment: None,
            id: FunctionId(0),
            name: "identity".into(),
            parameters: vec![LocalId(0)],
            return_type: Type::I32,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "x".into(),
                ty: Type::I32,
                mutable: false,
                span,
            }],
            values: vec![Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span,
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span,
        }],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: vec![ValueHold {
            value: ValueId(0),
            roots: vec![],
        }],
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![LocalId(0)],
        }],
    });
    (module, plan)
}

#[test]
fn independently_bound_embedded_new_metadata_precedes_identity_and_rejection() {
    let (baseline,plan)=module_candidate();let target=TargetProfile::host_llvm();let defaults=Limits::default();
    zil_verify::verify_module(baseline.clone(),plan.clone(),&target,&defaults).unwrap();
    for wide in [false,true] {
        let element=if wide {
            Type::Record{name:"wide-new-only".into(),fields:(0..1000).map(|i|(format!("field{i}"),Type::I32)).collect()}
        } else {
            let mut t=Type::I32;for _ in 0..8 {t=Type::OwnedStore{element:Box::new(t)}}t
        };
        let mut forged=baseline.clone();
        forged.functions[0].blocks[0].instructions[0].kind=InstructionKind::OwnedStoreNew{element};
        let bytes=wire::encode_module_candidate(&forged,&plan,&defaults).unwrap();
        let limits=if wide {Limits{max_analysis_steps:800,..defaults.clone()}} else {Limits{max_call_depth:4,..defaults.clone()}};
        let raw=zil_verify::verify_module(forged,plan.clone(),&target,&limits).unwrap_err();
        let decoded=zil_verify::verify_module_bytes(&bytes,&target,&limits).unwrap_err();
        for (kind,errors) in [("raw",raw),("wire",decoded)] {
            assert!(errors.iter().any(|d|d.category==ResultCategory::ResourceLimit),"wide={wide} {kind}: {errors:?}");
            println!("wide={wide} {kind}: {errors:?}");
        }
    }
    let mut small=baseline;
    small.functions[0].blocks[0].instructions[0].kind=InstructionKind::OwnedStoreNew{element:Type::I32};
    let stale=zil_verify::verify_module(small.clone(),plan.clone(),&target,&defaults).unwrap_err();
    assert!(stale.iter().any(|d|d.code=="E-VERIFY-PLAN-IDENTITY"),"{stale:?}");
    let mut plan=plan;plan.module_fingerprint=small.fingerprint();
    let refused=zil_verify::verify_module(small,plan,&target,&defaults).unwrap_err();
    assert!(refused.iter().any(|d|d.category==ResultCategory::Unsupported&&d.message.contains("independent checker")),"{refused:?}");
    println!("small-stale: {stale:?}; small-bound: {refused:?}");
}

fn expanded(source:&str)->zil_base::CompileResult<CoreModule> {
    let unit=SourceUnit::new(FileId(0),ModuleId(0),"review173.zil",source);
    let syntax=zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit,&syntax)
}

#[test]
fn source_reservation_keeps_member_names_legal() {
    for source in [
        "fn store_push(){} fn main(){}",
        "struct Store{x:i32} fn main(){}",
    ] {
        let errors=expanded(source).unwrap_err();
        assert!(errors.iter().any(|d|d.code=="E-OWNED-STORE-NAME"),"{source}: {errors:?}");
        println!("reserved source: {errors:?}");
    }
    // Local bindings are deliberately checked in Types::names (172), while
    // 173 registers declaration/type names in Expand.
    expanded("fn main(){let store_pop=1}").unwrap();
    expanded("struct Plain{x:i32} impl Plain{fn store_push(self:Self)->i32=>.x} fn main(){}").unwrap();
    let core=expanded("fn consume(s:Store<i32>){} fn main(){}").unwrap();
    assert!(matches!(&core.functions[0].parameters[0].ty,Type::Desc(inner) if matches!(inner.as_ref(),Type::OwnedStore{element} if **element==Type::I32)));
    for source in ["fn consume(s:Store){} fn main(){}", "fn consume(s:Store<i32,i64>){} fn main(){}"] {
        let errors=expanded(source).unwrap_err();
        println!("arity: {errors:?}");
        assert!(errors.iter().all(|d|d.category==ResultCategory::SafeReject),"{errors:?}");
    }
}
