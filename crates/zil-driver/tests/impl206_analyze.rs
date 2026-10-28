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
