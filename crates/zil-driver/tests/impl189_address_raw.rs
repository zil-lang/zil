//! Signature claims and current-call permissions are independent of the producer.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{CandidatePlan, Instruction, InstructionKind as I, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SHARED: &str = "fn main()->i32{let n=41;let f=(p:&i32)=>*p;return f(&n)-41}";

fn typed(source: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw189.zil", source);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn baseline(source: &str) -> (TypedModule, CandidatePlan) {
    let module = typed(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    (module, plan)
}
fn record(name: &str, module: &TypedModule, plan: &CandidatePlan, results: &[Vec<Diagnostic>]) {
    if let Some(base) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = PathBuf::from(base).join(format!(
            "raw189-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("module.txt"), format!("{module:#?}")).unwrap();
        fs::write(dir.join("candidate.txt"), format!("{plan:#?}")).unwrap();
        fs::write(dir.join("results.txt"), format!("{results:#?}")).unwrap();
    }
}
fn reject(
    name: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    category: ResultCategory,
    code: &str,
) {
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    assert_eq!(decoded, (module.clone(), plan.clone()));
    let results = [
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err(),
    ];
    record(name, &module, &plan, &results);
    for diagnostics in results {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == category && d.code == code),
            "{name}: {diagnostics:?}"
        );
    }
}

#[test]
fn raw_and_wire_do_not_trust_the_claimed_parameter_qualification() {
    let (mut module, plan) = baseline(SHARED);
    let Type::Address { mutable, .. } = &mut module.closures[0].parameters[0] else {
        unreachable!()
    };
    *mutable = true;
    reject(
        "entry-disagrees",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE-ENTRY",
    );

    let (mut module, plan) = baseline(SHARED);
    let closure = &mut module.closures[0];
    let Type::Address { mutable, .. } = &mut closure.parameters[0] else {
        unreachable!()
    };
    *mutable = true;
    let entry = closure.function.0 as usize;
    for local in &mut module.functions[entry].locals {
        if let Type::Address { mutable, .. } = &mut local.ty {
            *mutable = true;
        }
    }
    for ty in &mut module.functions[entry].values {
        if let Type::Address { mutable, .. } = ty {
            *mutable = true;
        }
    }
    let mut changed = false;
    for function in &mut module.functions {
        if function.environment.is_some() {
            continue;
        }
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let I::ReborrowAddress { mutable, .. } = &mut instruction.kind {
                    *mutable = true;
                    let Type::Address { mutable, .. } =
                        &mut function.values[instruction.result.unwrap().0 as usize]
                    else {
                        unreachable!()
                    };
                    *mutable = true;
                    changed = true;
                }
            }
        }
    }
    assert!(changed);
    reject(
        "shared-parent-as-exclusive",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-ADDRESS",
    );
}

#[test]
fn raw_and_wire_keep_explicit_parameter_holds_and_closed_shapes() {
    let (module, mut plan) = baseline(SHARED);
    let mut changed = false;
    for (function, facts) in module.functions.iter().zip(&mut plan.functions) {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let I::CallClosure { arguments, .. } = &instruction.kind {
                facts.holds[arguments[0].0 as usize].roots.clear();
                changed = true;
            }
        }
    }
    assert!(changed);
    reject(
        "erased-argument-hold",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-HOLD",
    );
    for (name, inner) in [
        (
            "nested-address",
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
        ),
        (
            "slice-address",
            Type::Slice {
                element: Box::new(Type::I32),
            },
        ),
    ] {
        let (mut module, plan) = baseline(SHARED);
        module.closures[0].parameters[0] = Type::Address {
            mutable: false,
            inner: Box::new(inner),
        };
        reject(
            name,
            module,
            plan,
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn raw_and_wire_reconstruct_capture_aliases_against_actual_arguments() {
    let source = "resource R{n:i32}fn main()->i32{let origin=R{n:40};let other=R{n:2};let edit=origin;let h=&other;let q=&h.n;let f=(p:&i32)=>{edit.n+=1;return *p};return f(q)-2}";
    let (mut module, plan) = baseline(source);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let origin = main.locals.iter().find(|l| l.name == "origin").unwrap().id;
    let other = main.locals.iter().find(|l| l.name == "other").unwrap().id;
    let mut changed = false;
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::DescribeLocal { local } = &mut instruction.kind
            && *local == origin
        {
            *local = other;
            changed = true;
        }
    }
    assert!(changed);
    // Keep the original producer rows; the mutation changes executable storage
    // identity only. A corrected fingerprint is not permission evidence.
    reject(
        "capture-now-aliases-explicit-argument",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn raw_and_wire_rebuild_hidden_writes_through_a_borrowed_helper() {
    for helper in [false, true] {
        let declaration = if helper {
            "fn apply<F:Fn(&i32)->i32>(f:&F,p:&i32)->i32{return f(p)}"
        } else {
            ""
        };
        let call = if helper { "apply(&f,q)" } else { "f(q)" };
        let source = format!(
            "resource R{{n:i32}}{declaration}fn main()->i32{{let r=R{{n:21}};let edit=r;let h=&r;let q=&h.n;let f=(p:&i32)=>edit.n+*p;return {call}-42}}"
        );
        let (mut module, plan) = baseline(&source);
        let entry = module.closures[0].function.0 as usize;
        let mut inserted = false;
        for block in &mut module.functions[entry].blocks {
            let mut instructions = Vec::new();
            for instruction in std::mem::take(&mut block.instructions) {
                let write = if let I::ReadDesc { desc, index } = instruction.kind {
                    Some(Instruction {
                        result: None,
                        kind: I::WriteDesc {
                            desc,
                            index,
                            value: instruction.result.unwrap(),
                        },
                        span: instruction.span,
                    })
                } else {
                    None
                };
                instructions.push(instruction);
                if let Some(write) = write {
                    instructions.push(write);
                    inserted = true;
                }
            }
            block.instructions = instructions;
        }
        assert!(inserted);
        reject(
            if helper {
                "hidden-helper-write"
            } else {
                "hidden-direct-write"
            },
            module,
            plan,
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn actual_address_call_work_has_a_measured_bounded_failure() {
    let source = "fn apply<F:Fn(&mut i32,&i32)->i32>(f:&F,p:&mut i32,q:&i32)->i32{return f(p,q)}fn main()->i32{var a=20;let b=22;let f=(p:&mut i32,q:&i32)=>{*p+=*q;return *p};return apply(&f,&mut a,&b)-42}";
    let (module, plan) = baseline(source);
    let works = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        };
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
    };
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    assert!(works(high).is_ok());
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if works(middle).is_ok() {
            high = middle;
        } else {
            low = middle;
        }
    }
    let failed = works(low).unwrap_err();
    assert!(
        failed
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{failed:?}"
    );
    assert!(works(high).is_ok());
    record(
        &format!("verify-budget-{low}-reject-{high}-accept"),
        &module,
        &plan,
        &[failed],
    );
}
