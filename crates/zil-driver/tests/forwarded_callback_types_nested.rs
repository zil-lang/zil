use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
fn typed(text: &str) -> zil_ir::TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "nested252.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
#[test]
fn original_nested_owned_callback_creates_moves_refills_and_calls() {
    let module = typed(include_str!(
        "forwarded_callback_types_fixtures/nested_original.zil"
    ));
    if let Some(root) = std::env::var_os("ZIL252_NESTED_OUT") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("original.typed.txt"), format!("{module:#?}")).unwrap();
    }
    assert!(module.functions.iter().filter(|f|f.environment.is_some()).any(|f|f.locals.iter().any(|l|matches!(&l.ty,zil_ir::Type::Array{element,length:1} if matches!(element.as_ref(),zil_ir::Type::Closure(_))))));
}

#[test]
fn nested_owned_callback_creation_and_move_without_refill_or_call() {
    let source = include_str!("forwarded_callback_types_fixtures/nested_original.zil");
    typed(&source.replace("f=f;", ""));
    typed(
        &source
            .replace("f=f;", "let g=f;")
            .replace("let p=f(", "let p=g("),
    );
    let cancel = source.replace(
        "        let p=f(Event{bytes:bytes_new(),n:3});\n        return p.n-10",
        "        return 0",
    );
    typed(&cancel);
}

fn nested_result(text: &str) -> Result<zil_ir::TypedModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "nested-negative252.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default())
}

#[test]
fn nested_owned_environment_is_not_copy_or_a_thin_pointer() {
    let source = include_str!("forwarded_callback_types_fixtures/nested_original.zil");
    for (label, text) in [
        (
            "duplicate move",
            source
                .replace("f=f;", "let g=f;let h=f;")
                .replace("let p=f(", "let p=g("),
        ),
        (
            "public pointer",
            source.replace("f=f;", "let pointer:fn(Event)->Reply=f;"),
        ),
        ("array", source.replace("f=f;", "let callbacks=[f];")),
    ] {
        let errors = match nested_result(&text) {
            Err(errors) => errors,
            Ok(module) => zil_analyze::analyze(&module, &Limits::default()).unwrap_err(),
        };
        println!("nested252 {label}: {errors:?}");
    }
}

#[test]
fn nested_owned_callback_can_pass_through_the_existing_keep_profile() {
    let source = include_str!("forwarded_callback_types_fixtures/nested_original.zil");
    let source = format!(
        "fn keep<F:FnOnce(Event)->Reply>(f:F)->F{{return f}}\n{}",
        source.replace("f=f;", "f=keep(f);")
    );
    typed(&source);
}

#[test]
fn nested_owned_move_does_not_revive_an_old_borrow_or_retain_external_capture() {
    let original = include_str!("forwarded_callback_types_fixtures/nested_original.zil");
    let stale = original
        .replace("f=f;", "let view=&f;let moved=f;")
        .replace("let p=f(", "let p=view(");
    let module = typed(&stale);
    let errors = match zil_analyze::analyze(&module, &Limits::default()) {
        Err(errors) => errors,
        Ok(plan) => zil_verify::verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default(),
        )
        .unwrap_err(),
    };
    assert!(
        errors
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::SafeReject)
    );
    println!("nested252 stale borrow: {errors:?}");
    let external = original
        .replace("let r=R{n:7};", "let n=7;let view=&n;")
        .replace("e.n+r.n", "e.n+*view")
        .replace("f=f;", "f=keep(f);");
    let external = format!("fn keep<F:FnOnce(Event)->Reply>(f:F)->F{{return f}}\n{external}");
    let errors = nested_result(&external).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("self-contained")
            || e.message.contains("external")
            || e.message.contains("borrow")),
        "{errors:?}"
    );
    println!("nested252 external capture: {errors:?}");
}

const OLD_COPY_NESTED: &str = "fn main()->i32{let outer=move()->i32=>{let a=1;let f=move(n:i32)=>n+a;return f(2)};return outer()-3}";
#[test]
fn nested_types_request_budget_and_copy_control_have_exact_fresh_boundaries() {
    for (label, text) in [
        (
            "own",
            include_str!("forwarded_callback_types_fixtures/nested_original.zil"),
        ),
        ("copy", OLD_COPY_NESTED),
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "nested-budget252.zil", text);
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &ast).unwrap();
        let run = |n| {
            zil_types::check(
                &core,
                &TargetProfile::host_llvm(),
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
        };
        let expected = run(Limits::default().max_analysis_steps).unwrap();
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            if run(mid).is_ok() {
                high = mid
            } else {
                low = mid
            }
        }
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        assert_eq!(run(high).unwrap(), expected);
        println!("nested252 {label} Types N={high};N-1 ResourceLimit;fresh N identity");
    }
}
