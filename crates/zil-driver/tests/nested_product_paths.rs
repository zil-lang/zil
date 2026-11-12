//! Selective Product paths preserve actual parent storage, loans, and leaf debt.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind as I, PlaceProjection as P, TypedModule};

const SOURCE: &str = "resource R{n:i32}package Child{manages a:R;manages b:R;n:i32}package Parent{child:Child;manages side:R}fn main()->i32{var p=Parent{child:Child{a:R{n:1},b:R{n:2},n:3},side:R{n:4}};let Parent{child:Child{a:a,b:b,n:ref mut n},side:side}=p;*n=9;return a.n+b.n+side.n+*n}";
fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "nested_paths.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

#[test]
fn actual_nested_product_paths_reach_both_independent_kernels_and_wire() {
    let module = typed(SOURCE);
    let function = &module.functions[0];
    assert!(function.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
        matches!(&i.kind, I::TakeTupleField { path, .. } if path == &[P::Field(0),P::Field(0)])
    }));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn forged_deep_paths_cannot_duplicate_or_retype_actual_leaf_debt() {
    let base = typed(SOURCE);
    let plan = zil_analyze::analyze(&base, &Limits::default()).unwrap();
    for mutation in 0..3 {
        let mut module = base.clone();
        let function = &mut module.functions[0];
        let mut changed = 0;
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::TakeTupleField { path, .. } = &mut instruction.kind
                && path == &[P::Field(0), P::Field(1)]
            {
                *path = match mutation {
                    0 => vec![P::Field(0), P::Field(0)], // same initialized debt twice
                    1 => vec![P::Field(0), P::Field(2)], // scalar cannot be an owning R
                    _ => vec![P::Field(0), P::Field(99)], // no actual field
                };
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
        let mut candidate = plan.clone();
        candidate.module_fingerprint = module.fingerprint();
        assert!(
            zil_verify::verify_module(
                module.clone(),
                candidate.clone(),
                &module.target,
                &Limits::default()
            )
            .is_err()
        );
        let wire =
            zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).is_err()
        );
    }
}

#[test]
fn a_deep_borrow_cannot_survive_an_intersecting_move_or_whole_parent_take() {
    let head = "resource R{n:i32}package Child{manages a:R;manages b:R;n:i32}package Parent{child:Child;manages side:R}";
    for body in [
        "let view=&p.child;let Child{a:moved,b:_,n:_}=p.child;return view.n+moved.n",
        "let Parent{child:Child{a:a,b:ref b,n:ref mut n},side:_}=p;let whole=p;return b.n+*n+whole.child.n+a.n",
    ] {
        let source = format!(
            "{head}fn main()->i32{{var p=Parent{{child:Child{{a:R{{n:1}},b:R{{n:2}},n:3}},side:R{{n:4}}}};{body}}}"
        );
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "negative.zil", source);
        let report = zil_driver::compile_source(&unit, &zil_driver::CompileOptions::default());
        assert_eq!(
            report.status,
            zil_base::ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}
