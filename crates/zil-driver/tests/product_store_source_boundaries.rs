//! Independent 223 review: no producer or author fixture grants acceptance.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, Type, TypedModule};

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!("review223-{label}"));
    fs::create_dir_all(&out).unwrap();
    out
}
fn typed(source: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review223.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn verify_expected(
    label: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    accept: bool,
    code: Option<&str>,
) {
    plan.module_fingerprint = module.fingerprint();
    let out = directory(label);
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    fs::write(out.join("input.wire"), &wire).unwrap();
    let target = module.target.clone();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &Limits::default()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &target, &Limits::default()),
        ),
    ] {
        fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
        if accept {
            result.unwrap();
            continue;
        }
        let errors = result.expect_err(label);
        if let Some(code) = code {
            assert_eq!(errors.len(), 1, "{label}: {errors:?}");
            assert_eq!(errors[0].code, code, "{label}: {errors:?}");
        }
        assert!(
            errors
                .iter()
                .all(|d| d.phase == Phase::Verify && d.category != ResultCategory::ResourceLimit),
            "{label}: {errors:#?}"
        );
        assert!(
            errors.iter().all(|d| !d.message.contains("fingerprint")),
            "{label}: {errors:#?}"
        );
    }
}

#[test]
fn original_unused_mutable_product_copy_helper_is_promoted() {
    let source = include_str!("product_store_wire_replay_fixtures/mutable_control.zil");
    let mut module = typed(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let product = module
        .functions
        .iter()
        .find(|f| f.name == "identity")
        .unwrap()
        .return_type
        .clone();
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "write")
        .unwrap();
    // A fully coherent signature mutation. The Copy field remains index 1 and
    // i32; incoming parameter Hold and WriteAddress remain exactly unchanged.
    fn replace(ty: &mut Type, product: &Type) {
        if let Type::Address { inner, .. } = ty
            && matches!(inner.as_ref(),Type::Package{name,..} if name.ends_with(":Leaf"))
        {
            **inner = product.clone();
        }
    }
    for local in &mut f.locals {
        replace(&mut local.ty, &product);
    }
    for value in &mut f.values {
        replace(value, &product);
    }
    // Original coherent223 input retained. Incoming &mut is actual authority;
    // only the outer Copy tag is written, with Parameter(0) provenance intact.
    verify_expected(
        "mutable-product-parameter-promoted315",
        module.clone(),
        plan.clone(),
        true,
        None,
    );
    for owned in [false, true] {
        let mut changed = module.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "write")
            .unwrap();
        if owned {
            let Type::Package { fields, .. } = &product else {
                unreachable!()
            };
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let zil_ir::InstructionKind::AddressField { index, .. } = &mut i.kind {
                    *index = 0;
                    f.values[i.result.unwrap().0 as usize] = Type::Address {
                        mutable: true,
                        inner: Box::new(fields[0].1.clone()),
                    };
                }
            }
        } else {
            for ty in f
                .locals
                .iter_mut()
                .map(|l| &mut l.ty)
                .chain(f.values.iter_mut())
            {
                if let Type::Address { mutable, .. } = ty {
                    *mutable = false;
                }
            }
        }
        verify_expected(
            if owned {
                "mutable-owned-child315"
            } else {
                "shared-copy-write315"
            },
            changed,
            plan.clone(),
            false,
            Some(if owned {
                "E-UNSUPPORTED"
            } else {
                "E-VERIFY-ADDRESS"
            }),
        );
    }
    let source = source.replace("write(p:&mut Leaf)","write(p:&mut Root)").replace("fn main(){}", "fn main(){var root=Root{child:Leaf{owner:R{n:1},tag:2},tag:3};let child=root.child;write(&mut root)}");
    let r = zil_driver::compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "incomplete223.zil", source),
        &zil_driver::CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..Default::default()
        },
    );
    fs::write(
        directory("incomplete-call315").join("result.json"),
        r.to_json(),
    )
    .unwrap();
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Analyze, "{}", r.to_json());
    assert!(
        r.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"),
        "{}",
        r.to_json()
    );
}

#[test]
fn ordinary_product_sums_are_admitted_but_direct_nested_union_stays_closed() {
    let source = include_str!("product_store_wire_replay_fixtures/unit_protocols.zil");
    let original = typed(source);
    let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    zil_verify::verify_module(
        original.clone(),
        plan.clone(),
        &original.target,
        &Limits::default(),
    )
    .unwrap();
    for variant in 0..4 {
        let mut module = original.clone();
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "carry")
            .unwrap();
        fn change(ty: &mut Type, variant: usize) {
            if let Type::ErrorUnion { error, result } = ty {
                match variant {
                    0 => **result = Type::I32,
                    1 => **result = error.as_ref().clone(),
                    2 => {
                        **error = Type::ErrorUnion {
                            error: error.clone(),
                            result: Box::new(Type::Unit),
                        }
                    }
                    3 => {
                        *result = error.clone();
                        **error = Type::Bool;
                    }
                    _ => unreachable!(),
                }
            }
        }
        change(&mut f.return_type, variant);
        for local in &mut f.locals {
            change(&mut local.ty, variant);
        }
        for value in &mut f.values {
            change(value, variant);
        }
        // §8.1.5 permits Product alternatives, not direct non-Copy
        // ErrorUnion nesting in an ErrorUnion error slot (variant 2).
        verify_expected(
            &format!("non-unit-sum-{variant}"),
            module,
            plan.clone(),
            variant != 2,
            None,
        );
    }
}
