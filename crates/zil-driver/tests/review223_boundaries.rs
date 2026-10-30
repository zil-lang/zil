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
fn rejected(label: &str, module: TypedModule, mut plan: CandidatePlan) {
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
        let errors = result.expect_err(label);
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
fn unused_mutable_nested_product_parameter_cannot_gain_a_write_capability() {
    let source = include_str!("review223_fixtures/mutable_control.zil");
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
    rejected("mutable-product-parameter", module, plan);
}

#[test]
fn exact_unit_protocols_cannot_admit_other_or_nested_owning_sums() {
    let source = include_str!("review223_fixtures/unit_protocols.zil");
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
                        *error = Box::new(Type::ErrorUnion {
                            error: error.clone(),
                            result: Box::new(Type::Unit),
                        })
                    }
                    3 => {
                        *result = error.clone();
                        *error = Box::new(Type::Bool);
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
        rejected(&format!("non-unit-sum-{variant}"), module, plan.clone());
    }
}
