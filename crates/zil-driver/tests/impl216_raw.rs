//! Source-derived coordinated raw/Wire tests for the new Product capture
//! boundary. The frozen 206 originals are retained unchanged in their suite.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

const FLAT: &str = "resource R{n:i32}\npackage Leaf{manages a:R;n:i32}\nfn local_capture(p:Leaf)->i32{let f=move()=>p.n;return f()}\nfn main(){}\n";

fn control() -> (TypedModule, CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "capture216.zil", FLAT);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    (module, candidate)
}

fn product() -> (TypedModule, CandidatePlan) {
    let (mut module, plan) = control();
    let leaf = module.closures[0].captures[0].clone();
    let product = Type::Package {
        name: "Independent216Root".into(),
        fields: vec![("child".into(), leaf.clone()), ("n".into(), Type::I32)],
    };
    for f in &mut module.functions {
        for ty in f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values) {
            if *ty == leaf {
                *ty = product.clone();
            }
        }
    }
    module.closures[0].captures[0] = product;
    (module, plan)
}

fn both(module: &TypedModule, candidate: &CandidatePlan, expected: Option<&str>) {
    let mut candidate = candidate.clone();
    candidate.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(module, &candidate, &limits).unwrap();
    let evidence = std::env::var_os("ZIL_TEST_REPORT_DIR").map(|root| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        let path = std::path::PathBuf::from(root).join(format!("raw216-{name}"));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("flat-source-control.zil"), FLAT).unwrap();
        std::fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
        std::fs::write(path.join("candidate.txt"), format!("{candidate:#?}")).unwrap();
        std::fs::write(path.join("input.wire"), &wire).unwrap();
        path
    });
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (module.clone(), candidate.clone())
    );
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), candidate, &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        if let Some(path) = &evidence {
            std::fs::write(path.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
        }
        if let Some(message) = expected {
            let errors = result.expect_err("invalid Product capture must fail independently");
            assert!(
                errors.iter().any(|e| e.message.contains(message)),
                "{errors:#?}"
            );
            assert!(errors.iter().all(|e| e.code != "E-VERIFY-PLAN-IDENTITY"
                && e.category != ResultCategory::ResourceLimit));
        } else {
            let verified = result.unwrap();
            let lir = zil_lower::lower(&verified).unwrap();
            let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
            assert!(llvm.contains("%zil_env_0 = type"), "{llvm}");
        }
    }
}

fn view(module: &TypedModule) -> (usize, LocalId) {
    let entry = module.closures[0].function.0 as usize;
    let local = module.functions[entry]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::BindPackageCapture { local, .. } => Some(local),
            _ => None,
        })
        .unwrap();
    (entry, local)
}

#[test]
fn complete_product_capture_has_independent_raw_wire_and_machine_layout_admission() {
    let (module, plan) = product();
    both(&module, &plan, None);
}

#[test]
fn missing_capture_bind_is_still_rejected() {
    let (mut module, plan) = product();
    let (entry, _) = view(&module);
    for block in &mut module.functions[entry].blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::BindPackageCapture { .. }));
    }
    both(
        &module,
        &plan,
        Some("load may read a local before initialization"),
    );
}

#[test]
fn wrong_capture_index_is_still_rejected() {
    let (mut module, plan) = product();
    let (entry, _) = view(&module);
    for instruction in module.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::BindPackageCapture { index, .. } = &mut instruction.kind {
            *index = 1;
        }
    }
    both(
        &module,
        &plan,
        Some("Package capture index is outside the environment"),
    );
}

#[test]
fn shared_capture_cannot_discharge_a_child() {
    let (mut module, plan) = product();
    let (entry, local) = view(&module);
    let f = &mut module.functions[entry];
    let block = &mut f.blocks[f.entry.0 as usize];
    block.instructions.insert(
        2,
        Instruction {
            result: None,
            span: f.span,
            kind: InstructionKind::DropPackageField { local, index: 0 },
        },
    );
    both(
        &module,
        &plan,
        Some("a shared Fn entry cannot write or consume an owned Package capture"),
    );
}

#[test]
fn repeatable_capture_must_return_with_its_child_present() {
    let (mut module, plan) = product();
    module.closures[0].kind = CallableKind::FnMut;
    for f in &mut module.functions {
        for local in &mut f.locals {
            local.mutable = true;
        }
        for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::CallClosureShared { callee, arguments } = &instruction.kind {
                instruction.kind = InstructionKind::CallClosureMut {
                    callee: *callee,
                    arguments: arguments.clone(),
                };
            }
        }
    }
    let (entry, local) = view(&module);
    let f = &mut module.functions[entry];
    let block = &mut f.blocks[f.entry.0 as usize];
    block.instructions.insert(
        2,
        Instruction {
            result: None,
            span: f.span,
            kind: InstructionKind::DropPackageField { local, index: 0 },
        },
    );
    both(
        &module,
        &plan,
        Some("repeatable closure entry does not preserve every owned field"),
    );
}

#[test]
fn product_capture_llvm_has_one_flag_per_child_and_rejects_forged_lir() {
    let (module, mut candidate) = product();
    candidate.module_fingerprint = module.fingerprint();
    let verified = zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let mut lir = zil_lower::lower(&verified).unwrap();
    let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
    assert!(
        llvm.contains("%zil_env_0 = type { { { { i32 }, i32 }, i32 }, i1, i1 }"),
        "{llvm}"
    );
    assert!(llvm.contains("%package.live."));
    lir.closures[0].captures[0] = Type::I32;
    let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "E-CODEGEN-UNBOUND-LIR"));
}

#[test]
fn product_capture_has_a_fresh_exact_verify_budget_boundary() {
    let (module, mut candidate) = product();
    candidate.module_fingerprint = module.fingerprint();
    let run = |max_analysis_steps| {
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits {
                max_analysis_steps,
                ..Limits::default()
            },
        )
    };
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    let expected = run(high).unwrap().fingerprint();
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match run(n) {
            Ok(verified) => {
                assert_eq!(verified.fingerprint(), expected);
                high = n;
            }
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                low = n;
            }
        }
    }
    assert_eq!(run(high).unwrap().fingerprint(), expected);
    assert!(
        run(high - 1)
            .unwrap_err()
            .iter()
            .all(|e| e.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(high).unwrap().fingerprint(), expected);
    println!("Product216 Verify N={high}; N-1=ResourceLimit; fresh N=Pass");
}
