//! Author public source-to-Types checks. Independent kernels are tested separately.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn core(text: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "front240.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}
fn checked(text: &str) -> TypedModule {
    zil_types::check(&core(text), &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn rejected(text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "reject240.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    if let Ok(c) = zil_expand::expand(&source, &ast) {
        assert!(
            zil_types::check(&c, &TargetProfile::host_llvm(), &Limits::default()).is_err(),
            "accepted: {text}"
        );
    }
}
#[test]
fn original_q_sources_reach_types_without_source_changes() {
    for text in [
        include_str!("impl240_fixtures/original03.zil"),
        include_str!("impl240_fixtures/original05.zil"),
    ] {
        checked(text);
    }
}
#[test]
fn right_nested_pattern_uses_local_observation_and_real_reverse_dfs_publication() {
    let t = checked(include_str!("impl240_fixtures/nested.zil"));
    let f = t.functions.iter().find(|f| f.name == "main").unwrap();
    let names = f
        .locals
        .iter()
        .map(|l| l.name.as_str())
        .filter(|n| ["a", "c", "d"].contains(n))
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["d", "c", "a"]);
    assert!(
        f.blocks.iter().flat_map(|b| &b.instructions).any(
            |i| matches!(&i.kind,InstructionKind::VariantTagLocal{path,..} if !path.is_empty())
        )
    );
    assert!(
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
            .count()
            >= 2
    );
    assert!(f.locals.iter().any(|l|matches!(&l.ty,Type::Package{fields,..} if fields.iter().any(|(_,t)|matches!(t,Type::Enum{..})))));
}
#[test]
fn product_sum_fields_and_error_payloads_are_complete_values() {
    checked(include_str!("impl240_fixtures/product.zil"));
}
#[test]
fn resource_field_requires_explicit_ownership_and_cannot_upgrade_ref() {
    for field in ["R", "Ref<R>", "manages Ref<R>", "manages &R"] {
        rejected(&format!(
            "resource R{{n:i32}}enum E{{V({field})}}fn main(){{}}"
        ));
    }
    checked(
        "resource R{n:i32}enum E{V(manages R,manages ByteBuffer)}fn main(){let r=R{n:1};let b=bytes_new();let e=E.V(give r,give b)}",
    );
}
#[test]
fn copy_generic_enum_is_preserved_but_new_owning_generic_enum_is_closed() {
    checked(
        "enum E<T>{V(T),Nil}fn main()->i32{let e=E.V(3);match e{E.V(n)=>{return n}\nE.Nil()=>{return 0}}}",
    );
    rejected(
        "resource R{n:i32}package P{manages r:R}enum E<T>{V(T)}fn main(){let e=E.V(P{r:R{n:1}})}",
    );
    rejected("resource R{n:i32}enum E<T>{V(manages R,T)}fn main(){}");
}
#[test]
fn ordinary_sum_does_not_acquire_store_element_or_resource_header_permission() {
    rejected(
        "resource R{n:i32}enum E{V(manages R)}package P{state:E}fn main(){let store:Store<P>=store_new()}",
    );
    rejected("resource R{n:i32}enum E{V(manages R)}resource Bad{state:E}fn main(){}");
}

#[test]
fn scrutinee_copy_scratch_cleans_before_hidden_owner_registration() {
    let t = checked(include_str!("impl240_fixtures/copy_scratch.zil"));
    let main = t.functions.iter().find(|f| f.name == "main").unwrap();
    assert!(
        main.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
    );
}
#[test]
fn unused_new_definitions_keep_exact_field_and_variant_names() {
    for text in [
        "resource R{n:i32}enum E{V(manages R),V()}fn main(){}",
        "resource R{n:i32}enum E{V(manages R)}package P{state:E;state:E}fn main(){}",
        "resource R{n:i32}enum E{V(manages R)}enum Bad{V(Ref<R>)}fn main(){}",
    ] {
        rejected(text);
    }
}
#[test]
fn same_module_sum_crosses_an_actual_project_import() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import lib;fn main()->i32{var value=lib.make();value=lib.relay(value);match value{lib.E.Nil()=>{}\nlib.E.Pair(a,b)=>{}}return 0}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "lib.zil",
            "resource R{n:i32}pub enum E{Nil,Pair(manages R,manages R)}pub fn make()->E{return E.Pair(R{n:65},R{n:66})}pub fn relay(value:E)->E{return value}",
        ),
    ];
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &[
            vec![CoreImport {
                alias: "lib".into(),
                module: ModuleId(1),
                span: zil_base::Span::default(),
            }],
            vec![],
        ],
        &Limits::default(),
    )
    .unwrap()
    .into_iter()
    .map(|(m, _)| m)
    .collect::<Vec<_>>();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: modules[0].core.source_fingerprint,
        modules,
    };
    zil_types::check_program(&program, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
}
#[test]
fn old_product_type_budget_and_new_sum_boundary_retry_are_exact() {
    let old = "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}fn main(){}";
    let target = TargetProfile::host_llvm();
    let check = |c: &CoreModule, n| {
        zil_types::check(
            c,
            &target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let c = core(old);
    assert!(check(&c, 418).is_ok());
    assert!(
        check(&c, 417)
            .unwrap_err()
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    let c = core(include_str!("impl240_fixtures/nested.zil"));
    let expected = check(&c, Limits::default().max_analysis_steps).unwrap();
    let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
    while lo + 1 < hi {
        let n = lo + (hi - lo) / 2;
        if check(&c, n).is_ok() { hi = n } else { lo = n }
    }
    assert!(
        check(&c, hi - 1)
            .unwrap_err()
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(check(&c, hi).unwrap(), expected);
    assert_eq!(check(&c, hi).unwrap(), expected);
    println!("Types nested owning Sum exact N={hi}; old Product N=418");
}
