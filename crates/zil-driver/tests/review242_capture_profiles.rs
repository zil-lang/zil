//! Root/242 first found the ordinary-vs-capture qualification hole. These are
//! independent coherent mutations of an old legal complete Package capture;
//! no source-level rejection is used as proof of the raw/Wire boundary.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;
fn replace(ty: &mut Type, before: &Type, after: &Type) {
    if ty == before {
        *ty = after.clone();
        return;
    }
    match ty {
        Type::Managed(t)
        | Type::Desc(t)
        | Type::OwnedStore { element: t }
        | Type::Array { element: t, .. }
        | Type::Slice { element: t }
        | Type::Address { inner: t, .. }
        | Type::Opaque { hidden: t, .. } => replace(t, before, after),
        Type::Package { fields, .. } | Type::Record { fields, .. } => {
            for (_, t) in fields {
                replace(t, before, after);
            }
        }
        Type::Tuple(fields) => {
            for t in fields {
                replace(t, before, after);
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for t in fields {
                    replace(t, before, after);
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            replace(error, before, after);
            replace(result, before, after);
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for t in parameters {
                replace(t, before, after);
            }
            replace(result, before, after);
        }
        _ => {}
    }
}
fn module() -> TypedModule {
    let text = "resource R{n:i32}package P{manages r:R}fn main(){let p=P{r:R{n:1}};let f=move()=>{let q=p;return 0}}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "capture-profile242.zil", text);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let mut module =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    // Keep the actually supplied owned capture but make its raw body ignore it.
    // This removes old Package-view and TakeCapture operation gates, so the
    // capture-definition qualification itself must reject a new Sum field.
    for f in module
        .functions
        .iter_mut()
        .filter(|f| f.environment.is_some())
    {
        assert!(f.parameters.is_empty());
        f.locals.truncate(1);
        assert_eq!(f.locals[0].id, LocalId(0));
        f.values = vec![Type::I32];
        let instruction = |result, kind| Instruction {
            result,
            kind,
            span: f.span,
        };
        f.blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                instruction(
                    None,
                    InstructionKind::BindPackageCapture {
                        local: LocalId(0),
                        index: 0,
                    },
                ),
                instruction(
                    Some(ValueId(0)),
                    InstructionKind::Constant(Constant::I32(0)),
                ),
                instruction(
                    None,
                    InstructionKind::EndPackageCapture {
                        local: LocalId(0),
                        index: 0,
                    },
                ),
                instruction(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                ),
            ],
            terminator: Terminator::Return(Some(ValueId(0))),
        }];
        f.entry = BlockId(0);
    }
    module
}
fn directory(label: &str) -> PathBuf {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review242-capture-{label}-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    out
}
#[test]
fn new_ordinary_sums_do_not_qualify_direct_or_nested_owned_captures() {
    let original = module();
    let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    zil_verify::verify_module(
        original.clone(),
        plan.clone(),
        &original.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&original, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &original.target, &Limits::default()).unwrap();
    let before = original.closures[0].captures[0].clone();
    let Type::Package { fields, .. } = &before else {
        panic!("old legal complete Package capture")
    };
    assert_eq!(fields.len(), 1);
    let sum = Type::Enum {
        name: "@review242:Owned".into(),
        variants: vec![
            ("Payload".into(), vec![fields[0].1.clone()]),
            ("Empty".into(), vec![]),
        ],
    };
    let mut failures = Vec::new();
    for (label, after, kind) in [
        ("direct-enum", sum.clone(), 0),
        (
            "product-enum",
            Type::Package {
                name: "@review242:Wrapped".into(),
                fields: vec![("state".into(), sum.clone())],
            },
            1,
        ),
        (
            "error-enum",
            Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(sum.clone()),
            },
            2,
        ),
    ] {
        let mut mutated = original.clone();
        let mut candidate = plan.clone();
        for f in &mut mutated.functions {
            let old_values: Vec<_> = f
                .values
                .iter()
                .enumerate()
                .filter_map(|(i, t)| (t == &before).then_some(ValueId(i as u32)))
                .collect();
            let old_locals: Vec<_> = f
                .locals
                .iter()
                .filter_map(|l| (l.ty == before).then_some(l.id))
                .collect();
            replace(&mut f.return_type, &before, &after);
            for l in &mut f.locals {
                replace(&mut l.ty, &before, &after);
            }
            for t in &mut f.values {
                replace(t, &before, &after);
            }
            let no_view = kind != 1 && f.environment.is_some();
            if no_view {
                f.locals.clear();
            }
            for block in &mut f.blocks {
                let mut instructions = Vec::new();
                for mut instruction in std::mem::take(&mut block.instructions) {
                    if no_view
                        && matches!(
                            instruction.kind,
                            InstructionKind::StorageLive(_)
                                | InstructionKind::StorageDead(_)
                                | InstructionKind::BindPackageCapture { .. }
                                | InstructionKind::EndPackageCapture { .. }
                        )
                    {
                        continue;
                    }
                    if instruction
                        .result
                        .is_some_and(|id| old_values.contains(&id))
                        && let InstructionKind::MakePackage { fields } = &mut instruction.kind
                    {
                        assert_eq!(fields.len(), 1);
                        let fields = std::mem::take(fields);
                        if kind == 0 {
                            instruction.kind = InstructionKind::MakeVariant { variant: 0, fields };
                        } else {
                            let value = ValueId(f.values.len() as u32);
                            f.values.push(sum.clone());
                            instructions.push(Instruction {
                                result: Some(value),
                                kind: InstructionKind::MakeVariant { variant: 0, fields },
                                span: instruction.span,
                            });
                            candidate
                                .functions
                                .iter_mut()
                                .find(|p| p.function == f.id)
                                .unwrap()
                                .holds
                                .push(ValueHold {
                                    value,
                                    roots: vec![],
                                });
                            instruction.kind = if kind == 1 {
                                InstructionKind::MakePackage {
                                    fields: vec![value],
                                }
                            } else {
                                InstructionKind::MakeErrorUnion {
                                    is_error: false,
                                    value,
                                }
                            };
                        }
                    }
                    if let InstructionKind::DropPackage { local } = instruction.kind
                        && old_locals.contains(&local)
                    {
                        instruction.kind = match kind {
                            0 => InstructionKind::DropEnum { local },
                            2 => InstructionKind::DropErrorUnion { local },
                            _ => InstructionKind::DropPackage { local },
                        };
                    }
                    instructions.push(instruction);
                }
                block.instructions = instructions;
            }
        }
        for closure in &mut mutated.closures {
            for t in closure.captures.iter_mut().chain(&mut closure.parameters) {
                replace(t, &before, &after);
            }
            replace(&mut closure.result, &before, &after);
        }
        candidate.module_fingerprint = mutated.fingerprint();
        let out = directory(label);
        fs::write(out.join("module.txt"), format!("{mutated:#?}")).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&mutated, &candidate, &Limits::default())
            .unwrap();
        fs::write(out.join("candidate.wire"), &wire).unwrap();
        let raw = zil_verify::verify_module(
            mutated.clone(),
            candidate,
            &mutated.target,
            &Limits::default(),
        );
        let replay = zil_verify::verify_module_bytes(&wire, &mutated.target, &Limits::default());
        let analyze = zil_analyze::analyze(&mutated, &Limits::default());
        for (route, result) in [
            ("raw", raw.map(|_| ())),
            ("wire", replay.map(|_| ())),
            ("analyze", analyze.map(|_| ())),
        ] {
            fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
            match result {
                Ok(()) => failures.push(format!("{label}/{route} accepted closed capture profile")),
                Err(errors)
                    if errors
                        .iter()
                        .any(|d| d.category == ResultCategory::Unsupported) => {}
                Err(errors) => failures.push(format!(
                    "{label}/{route} failed only unrelated metadata/type: {errors:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn unused_error_union_slots_preserve_the_old_managed_success_profile() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "error-slots242.zil",
        "resource R{n:i32}fn main(){}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let original =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    let managed = Type::Managed(Box::new(original.resources[0].payload.clone()));
    let direct = Type::ErrorUnion {
        error: Box::new(Type::Unit),
        result: Box::new(managed.clone()),
    };
    let enumeration = Type::Enum {
        name: "@review242:Owns".into(),
        variants: vec![("R".into(), vec![managed.clone()])],
    };
    let cases = [
        ("direct-managed-control", direct.clone(), true),
        (
            "enum-error-unit-control",
            Type::ErrorUnion {
                error: Box::new(enumeration.clone()),
                result: Box::new(Type::Unit),
            },
            true,
        ),
        (
            "enum-holds-union-control",
            Type::Enum {
                name: "@review242:Holds".into(),
                variants: vec![("Union".into(), vec![direct.clone()])],
            },
            true,
        ),
        (
            "owned-union-error-managed",
            Type::ErrorUnion {
                error: Box::new(direct.clone()),
                result: Box::new(managed.clone()),
            },
            false,
        ),
        (
            "owned-enum-error-managed",
            Type::ErrorUnion {
                error: Box::new(enumeration),
                result: Box::new(managed.clone()),
            },
            false,
        ),
        (
            "owned-union-error-unit",
            Type::ErrorUnion {
                error: Box::new(direct.clone()),
                result: Box::new(Type::Unit),
            },
            false,
        ),
        (
            "unit-error-owned-union-result",
            Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(direct.clone()),
            },
            false,
        ),
        (
            "illegal-slot-inside-enum",
            Type::Enum {
                name: "@review242:NestedBad".into(),
                variants: vec![(
                    "Bad".into(),
                    vec![Type::ErrorUnion {
                        error: Box::new(direct),
                        result: Box::new(managed),
                    }],
                )],
            },
            false,
        ),
    ];
    let mut failures = Vec::new();
    for (label, ty, accept) in cases {
        let mut mutated = original.clone();
        let mut candidate = plan.clone();
        let f = &mut mutated.functions[0];
        f.locals.push(LocalDecl {
            id: LocalId(f.locals.len() as u32),
            name: "unused_shape".into(),
            ty,
            mutable: false,
            span: f.span,
        });
        candidate.module_fingerprint = mutated.fingerprint();
        let out = directory(label);
        fs::write(out.join("module.txt"), format!("{mutated:#?}")).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&mutated, &candidate, &Limits::default())
            .unwrap();
        fs::write(out.join("candidate.wire"), &wire).unwrap();
        let raw = zil_verify::verify_module(
            mutated.clone(),
            candidate,
            &mutated.target,
            &Limits::default(),
        );
        let replay = zil_verify::verify_module_bytes(&wire, &mutated.target, &Limits::default());
        let analyzed = zil_analyze::analyze(&mutated, &Limits::default());
        for (route, result) in [
            ("raw", raw.map(|_| ())),
            ("wire", replay.map(|_| ())),
            ("analyze", analyzed.map(|_| ())),
        ] {
            fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
            match (accept, result) {
                (true, Ok(())) => {}
                (false, Err(errors))
                    if errors
                        .iter()
                        .any(|d| d.category == ResultCategory::Unsupported) => {}
                (false, Err(errors))
                    if route == "analyze"
                        && matches!(label, "owned-enum-error-managed" | "illegal-slot-inside-enum")
                        && !errors.is_empty()
                        && errors.iter().all(|d|
                            d.phase == zil_base::Phase::Analyze
                                && d.category == ResultCategory::SafeReject
                                && d.code == "E-ANALYZE-IR"
                                && d.message == "resource closure: Managed success requires a Copy error payload") => {}
                (_, result) => failures.push(format!(
                    "{label}/{route} expected admitted={accept}, actual={result:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
