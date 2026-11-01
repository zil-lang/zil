//! Producer resource checks run independently of Verify and native lowering.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind as I, Type, TypedModule};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "product206-analyze-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn typed(label: &str, source: &str) -> TypedModule {
    let dir = directory(label);
    fs::write(dir.join("source.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "product206.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    fs::write(dir.join("core.txt"), format!("{core:#?}")).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    module
}
fn analyze(label: &str, module: &TypedModule, reject: Option<&str>) {
    let dir = directory(label);
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    let result = zil_analyze::analyze(module, &Limits::default());
    fs::write(dir.join("analyze.txt"), format!("{result:#?}")).unwrap();
    if let Some(message) = reject {
        let errors = result.expect_err(label);
        assert!(
            errors.iter().any(|d| d.phase == Phase::Analyze
                && d.category == ResultCategory::SafeReject
                && d.code == "E-ANALYZE-IR"
                && d.message.contains(message)),
            "{label}: {errors:#?}"
        );
    } else {
        result.unwrap_or_else(|errors| panic!("{label}: {errors:#?}"));
    }
}

#[test]
fn frozen_created_and_incoming_leaves_remain_distinct_through_each_wrapper() {
    for (label, source, reject) in [
        (
            "mixed-created",
            include_str!("impl206_fixtures/01_mixed_child_created.zil"),
            false,
        ),
        (
            "mixed-incoming",
            include_str!("impl206_fixtures/01_mixed_child_incoming.zil"),
            true,
        ),
        (
            "whole-created",
            include_str!("impl206_fixtures/02_whole_transfer_twice_created.zil"),
            false,
        ),
        (
            "whole-incoming",
            include_str!("impl206_fixtures/02_whole_transfer_twice_incoming.zil"),
            true,
        ),
        (
            "wrappers-created",
            include_str!("impl206_fixtures/03_child_only_wrappers_created.zil"),
            false,
        ),
        (
            "wrappers-incoming",
            include_str!("impl206_fixtures/03_child_only_wrappers_incoming.zil"),
            true,
        ),
        (
            "ordinary-calls",
            include_str!("impl206_fixtures/04_unknown_ordinary_calls_do_not_create.zil"),
            true,
        ),
    ] {
        let module = typed(label, source);
        analyze(
            label,
            &module,
            reject.then_some("create result forwards an incoming responsibility"),
        );
    }
}

#[test]
fn typed_operand_and_leaf_index_mutations_cannot_forge_created_origins() {
    let source = include_str!("impl206_fixtures/03_child_only_wrappers_created.zil");
    let base = typed("raw-base", source);
    analyze("raw-good", &base, None);
    for swap_operands in [false, true] {
        let mut module = base.clone();
        let function = module
            .functions
            .iter_mut()
            .find(|f| f.name == "origin")
            .unwrap();
        let mut changed = 0;
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if swap_operands {
                if let I::MakePackage { fields }=&mut instruction.kind
                    && instruction.result.is_some_and(|v| matches!(&function.values[v.0 as usize],Type::Package {fields:decl,..} if decl.len()==4)) {
                    fields.swap(1,3); changed+=1;
                }
            } else if let I::TakePackageField { local, index } = &mut instruction.kind
                && matches!(&function.locals[local.0 as usize].ty,Type::Package {fields,..} if fields.len()==4)
                && *index == 3
            {
                *index = 1;
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        analyze(
            if swap_operands {
                "raw-operands"
            } else {
                "raw-leaf-index"
            },
            &module,
            Some("create result forwards an incoming responsibility"),
        );
    }
}

#[test]
fn child_refill_preserves_actual_leaf_origins_and_partial_shapes_stay_local() {
    let model = "resource R{n:i32}package P{manages old:R;manages fresh:R}package Outer{child:P}";
    let good = format!(
        "{model}fn origin(give old:R)->R contract{{result(create)}}{{var outer=Outer{{child:P{{old:give old,fresh:R{{n:29}}}}}};let child=outer.child;outer.child=child;let moved=outer;let next=moved.child;return give next.fresh}}fn main(){{}}"
    );
    analyze("refill-good", &typed("refill-good", &good), None);
    analyze(
        "refill-incoming",
        &typed(
            "refill-incoming",
            &good.replace("give next.fresh", "give next.old"),
        ),
        Some("create result forwards an incoming responsibility"),
    );
    for (label, body) in [
        (
            "partial-child",
            "let child=outer.child;let old=give child.old;outer.child=child;",
        ),
        ("partial-parent", "let child=outer.child;let moved=outer;"),
    ] {
        let source = format!(
            "{model}fn main(){{var outer=Outer{{child:P{{old:R{{n:1}},fresh:R{{n:2}}}}}};{body}}}"
        );
        analyze(
            label,
            &typed(label, &source),
            Some("whole aggregate move requires every owned element or field live"),
        );
    }
}

fn wrap_type(ty: &mut Type, old: &Type, outer: &Type) {
    if ty == old {
        *ty = outer.clone();
        return;
    }
    match ty {
        Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Address { inner, .. }
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::OwnedStore { element: inner }
        | Type::Opaque { hidden: inner, .. } => wrap_type(inner, old, outer),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, ty) in fields {
                wrap_type(ty, old, outer)
            }
        }
        Type::Tuple(types) => {
            for ty in types {
                wrap_type(ty, old, outer)
            }
        }
        Type::Enum { variants, .. } => {
            for (_, types) in variants {
                for ty in types {
                    wrap_type(ty, old, outer)
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            wrap_type(error, old, outer);
            wrap_type(result, old, outer);
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for ty in parameters {
                wrap_type(ty, old, outer)
            }
            wrap_type(result, old, outer);
        }
        _ => {}
    }
}

#[test]
fn ordinary_product_admission_does_not_widen_other_raw_parent_contexts() {
    for (label, body) in [
        ("address", "fn boundary(p:&P){}fn main(){}"),
        ("array", "fn boundary(p:[P;1])->[P;1]{return p}fn main(){}"),
        ("sum", "fn boundary(p:bool!P)->bool!P{return p}fn main(){}"),
        ("store", "fn boundary(give s:Store<P>){}fn main(){}"),
        (
            "capture",
            "fn main(){let p=P{r:R{n:1}};let f=move()=>{return p.r.n};f()}",
        ),
    ] {
        let source = format!(
            "resource R{{n:i32}}package P{{manages r:R}}package Outer{{child:P}}fn identity(p:P)->P{{return p}}{body}"
        );
        let mut module = typed(label, &source);
        analyze(&format!("old-{label}"), &module, None);
        let old = module
            .functions
            .iter()
            .find(|f| f.name == "identity")
            .unwrap()
            .return_type
            .clone();
        let outer = Type::Package {
            name: "@module0:Outer".into(),
            fields: vec![("child".into(), old.clone())],
        };
        for function in &mut module.functions {
            wrap_type(&mut function.return_type, &old, &outer);
            for local in &mut function.locals {
                wrap_type(&mut local.ty, &old, &outer);
            }
            for value in &mut function.values {
                wrap_type(value, &old, &outer);
            }
        }
        for resource in &mut module.resources {
            wrap_type(&mut resource.payload, &old, &outer);
        }
        for closure in &mut module.closures {
            for capture in &mut closure.captures {
                wrap_type(capture, &old, &outer);
            }
            for parameter in &mut closure.parameters {
                wrap_type(parameter, &old, &outer);
            }
            wrap_type(&mut closure.result, &old, &outer);
        }
        let dir = directory(&format!("closed-{label}"));
        fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
        let result = zil_analyze::analyze(&module, &Limits::default());
        fs::write(dir.join("analyze.txt"), format!("{result:#?}")).unwrap();
        if label == "address" || label == "sum" {
            // 224 admits shared Outer; §8.1.5 now also admits the unchanged
            // internally consistent bool!Outer parameter/result mutation.
            assert!(result.is_ok(), "{label}: {result:#?}");
            continue;
        }
        let errors = result.expect_err(label);
        if label == "capture" {
            // 214-216 admits complete Product captures, but this unchanged
            // mutation still constructs Outer.child from a Managed R and
            // projects it as R. It must now fail the actual field type check.
            assert!(
                errors.iter().any(|d| d.phase == Phase::Analyze
                    && d.category == ResultCategory::SafeReject
                    && d.code == "E-ANALYZE-IR"
                    && d.message == "resource closure: package field type mismatch"),
                "{label}: {errors:#?}"
            );
        } else if label == "store" {
            // 223 admits complete Product Store values. This unchanged raw
            // mutation still retains the old canonical Store element name.
            assert!(
                errors.iter().any(|d| d.phase == Phase::Analyze
                    && d.category == ResultCategory::SafeReject
                    && d.code == "E-ANALYZE-IR"
                    && d.message == "Product Store lacks its exact canonical element definition"),
                "{label}: {errors:#?}"
            );
        } else {
            assert!(errors.iter().any(|d| d.phase == Phase::Analyze
                && d.category == ResultCategory::Unsupported), "{label}: {errors:#?}");
        }
    }
}

#[test]
fn same_typed_children_keep_their_own_actual_leaf_origins() {
    let source = "resource R{n:i32}package P{manages leaf:R}package Outer{left:P;right:P}fn origin(give old:R)->R contract{result(create)}{let left=P{leaf:R{n:29}};let right=P{leaf:give old};let outer=Outer{left:left,right:right};let child=outer.left;return give child.leaf}fn main(){}";
    let base = typed("same-types", source);
    analyze("same-types-good", &base, None);
    analyze(
        "same-types-source-incoming",
        &typed(
            "same-types-source-incoming",
            &source.replace("outer.left", "outer.right"),
        ),
        Some("create result forwards an incoming responsibility"),
    );
    let mut raw = base.clone();
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "origin")
        .unwrap();
    let mut changed = 0;
    for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::TakePackageField { local, index } = &mut instruction.kind
            && matches!(&function.locals[local.0 as usize].ty, Type::Package { fields, .. }
                if fields.len() == 2 && fields.iter().all(|(_, ty)| matches!(ty, Type::Package { .. })))
        {
            assert_eq!(*index, 0);
            *index = 1;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    analyze(
        "same-types-raw-retarget",
        &raw,
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn cfg_joins_preserve_incoming_on_each_reachable_product_path() {
    let model = "resource R{n:i32}package P{manages leaf:R}package Outer{child:P}";
    let both_created = format!(
        "{model}fn origin(give old:R,choose:bool)->R contract{{result(create)}}{{var outer=Outer{{child:P{{leaf:R{{n:1}}}}}};if choose{{outer.child=P{{leaf:R{{n:2}}}}}}else{{outer.child=P{{leaf:R{{n:3}}}}}};let child=outer.child;return give child.leaf}}fn main(){{}}"
    );
    analyze("cfg-created", &typed("cfg-created", &both_created), None);
    let mixed = both_created.replace("leaf:R{n:3}", "leaf:give old");
    analyze(
        "cfg-mixed",
        &typed("cfg-mixed", &mixed),
        Some("create result forwards an incoming responsibility"),
    );
    let loop_source = format!(
        "{model}fn origin(give old:R,choose:bool)->R contract{{result(create)}}{{var outer=Outer{{child:P{{leaf:give old}}}};while choose{{outer.child=P{{leaf:R{{n:2}}}};break}};let child=outer.child;return give child.leaf}}fn main(){{}}"
    );
    analyze(
        "zero-iteration-incoming",
        &typed("zero-iteration-incoming", &loop_source),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn unused_resource_payload_does_not_admit_an_ordinary_product() {
    let types = typed(
        "resource-product-shape",
        "resource R{n:i32}package P{manages leaf:R}package Outer{child:P}fn identity(p:Outer)->Outer{return p}fn main(){}",
    );
    let product = types
        .functions
        .iter()
        .find(|f| f.name == "identity")
        .unwrap()
        .return_type
        .clone();
    let mut module = typed(
        "resource-unused-control",
        "resource Unused{n:i32}fn main(){}",
    );
    analyze("resource-unused-control", &module, None);
    let Type::Record { fields, .. } = &mut module.resources[0].payload else {
        panic!("Copy head")
    };
    fields.push(("hidden".into(), product));
    let dir = directory("resource-unused-product");
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    let result = zil_analyze::analyze(&module, &Limits::default());
    fs::write(dir.join("analyze.txt"), format!("{result:#?}")).unwrap();
    let errors = result.expect_err("unused resource head cannot own Product");
    assert!(
        errors
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::Unsupported),
        "{errors:#?}"
    );
}
