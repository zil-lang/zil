//! Analyze admission/ownership checks on source-derived raw IR, bypassing new Types admission.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

fn product() -> TypedModule {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "capture215.zil",
        "resource R{n:i32}\npackage Leaf{manages a:R;n:i32}\nfn local_capture(p:Leaf)->i32{let f=move()=>p.n;return 0}\nfn main(){}\n",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let mut module =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let leaf = module.closures[0].captures[0].clone();
    let root = Type::Package {
        name: "Independent215Root".into(),
        fields: vec![("child".into(), leaf.clone()), ("n".into(), Type::I32)],
    };
    for f in &mut module.functions {
        for ty in f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values) {
            if *ty == leaf {
                *ty = root.clone();
            }
        }
    }
    module.closures[0].captures[0] = root;
    module
}

fn both(module: &TypedModule, expected_error: Option<&str>) {
    let limits = Limits::default();
    let bytes =
        wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits).unwrap();
    let (decoded, _) = wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(&decoded, module);
    for input in [module, &decoded] {
        let result = zil_analyze::analyze(input, &limits);
        if let Some(message) = expected_error {
            let errors =
                result.expect_err("invalid Product capture must not produce an Analyze candidate");
            assert!(errors.iter().all(|e| e.phase == Phase::Analyze));
            assert!(
                errors.iter().any(|e| e.message.contains(message)),
                "{errors:#?}"
            );
            assert!(
                errors
                    .iter()
                    .all(|e| e.category != ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
        } else {
            result.unwrap();
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
fn complete_product_capture_is_checked_by_analyze_and_wire() {
    both(&product(), None);
}

#[test]
fn capture_with_borrowed_child_stays_closed() {
    let mut module = product();
    if let Type::Package { fields, .. } = &mut module.closures[0].captures[0] {
        fields[0].1 = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
    }
    both(&module, Some("package requires finite Copy"));
}

#[test]
fn capture_child_must_be_present_at_repeatable_return() {
    let mut module = product();
    module.closures[0].kind = CallableKind::FnMut;
    for f in &mut module.functions {
        for local in &mut f.locals {
            local.mutable = true;
        }
    }
    let (entry, local) = view(&module);
    let f = &mut module.functions[entry];
    f.blocks[f.entry.0 as usize].instructions.insert(
        2,
        Instruction {
            result: None,
            span: f.span,
            kind: InstructionKind::DropPackageField { local, index: 0 },
        },
    );
    both(
        &module,
        Some("repeatable entry fails to restore owned fields"),
    );
}

#[test]
fn capture_child_cannot_be_transferred_twice() {
    let mut module = product();
    module.closures[0].kind = CallableKind::FnOnce;
    let (entry, local) = view(&module);
    let f = &mut module.functions[entry];
    let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty else {
        unreachable!()
    };
    let child = fields[0].1.clone();
    for position in 2..4 {
        let value = ValueId(f.values.len() as u32);
        f.values.push(child.clone());
        f.blocks[f.entry.0 as usize].instructions.insert(
            position,
            Instruction {
                result: Some(value),
                span: f.span,
                kind: InstructionKind::TakePackageField { local, index: 0 },
            },
        );
    }
    both(
        &module,
        Some("package field take requires that field live on every path"),
    );
}

#[test]
fn conditional_cleanup_remains_idempotent_for_a_consumed_child() {
    let mut module = product();
    module.closures[0].kind = CallableKind::FnOnce;
    let (entry, local) = view(&module);
    let f = &mut module.functions[entry];
    for _ in 0..2 {
        f.blocks[f.entry.0 as usize].instructions.insert(
            2,
            Instruction {
                result: None,
                span: f.span,
                kind: InstructionKind::DropPackageField { local, index: 0 },
            },
        );
    }
    both(&module, None);
}

#[test]
fn fresh_exact_budget_and_one_less_are_distinct() {
    let module = product();
    let mut high = Limits::default().max_analysis_steps;
    let expected = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut low = 0;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        };
        match zil_analyze::analyze(&module, &limits) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|e| e.category == ResultCategory::ResourceLimit)
                );
                low = mid;
            }
        }
    }
    for _ in 0..2 {
        assert_eq!(
            zil_analyze::analyze(
                &module,
                &Limits {
                    max_analysis_steps: high,
                    ..Limits::default()
                },
            )
            .unwrap(),
            expected
        );
    }
    let errors = zil_analyze::analyze(
        &module,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == ResultCategory::ResourceLimit)
    );
    eprintln!("Product capture Analyze exact budget: {high}");
}
