use zil_base::{Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::{CallableKind, CoreModule, Type};

const ORIGINAL: &str = include_str!("impl205_fixtures/00_original_state_and_replacement.zil");
const LAST_USE: &str = include_str!("impl205_fixtures/03_mixed_unused_copy_alias_last_use.zil");
const MIXED_ALIAS: &str = include_str!("impl205_fixtures/01_mixed_unused_copy_alias.zil");
const SINGLE_ALIAS: &str = include_str!("impl205_fixtures/02_single_state_unused_copy_alias.zil");

fn run(text: &str) -> Result<CoreModule, Vec<Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "state205.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let result = zil_expand::expand_with_map(&source, &syntax);
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let out = std::path::PathBuf::from(root).join(source.fingerprint().to_string());
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("source.zil"), text).unwrap();
        std::fs::write(out.join("expand.txt"), format!("{result:#?}")).unwrap();
    }
    result.map(|result| {
        result
            .source_map
            .validate(std::slice::from_ref(&source), &Limits::default())
            .unwrap();
        assert!(result.core.unsupported.is_empty());
        result.core
    })
}

fn header(text: &str, owner: u32, replacement: u32, fields: &[u32], kind: CallableKind) {
    let core = run(text).unwrap();
    let function = core.functions.iter().find(|f| f.name == "replace").unwrap();
    assert!(function.type_parameters.is_empty());
    assert!(function.callable_return.is_none());
    let [state] = function.state_parameters.as_slice() else {
        panic!("{function:#?}");
    };
    let [binding] = state.bindings.as_slice() else {
        panic!("{state:#?}");
    };
    let [direct] = function.callable_parameters.as_slice() else {
        panic!("{function:#?}");
    };
    assert_eq!(state.index, owner);
    assert_eq!(direct.index, replacement);
    assert_eq!(binding.generic_index, 0);
    assert_eq!(direct.generic_index, 0);
    assert_eq!(binding.generic_name, "F");
    assert_eq!(direct.generic_name, binding.generic_name);
    assert_eq!(binding.fields, fields);
    assert_eq!(binding.kind, kind);
    assert_eq!(direct.kind, kind);
    assert_eq!(binding.parameters, direct.parameters);
    assert_eq!(binding.result, Type::I32);
    assert_eq!(direct.result, binding.result);
    assert_eq!(function.parameters[owner as usize].ty, function.return_type);
    assert_ne!(owner, replacement);
}

#[test]
fn original_138_bytes_publish_one_owner_and_one_real_replacement() {
    assert_eq!(ORIGINAL.len(), 138);
    header(ORIGINAL, 0, 1, &[1, 2], CallableKind::Fn);
}

#[test]
fn declaration_positions_and_field_count_are_not_the_single_type_slot() {
    for (bound, kind) in [
        ("Fn", CallableKind::Fn),
        ("FnMut", CallableKind::FnMut),
        ("FnOnce", CallableKind::FnOnce),
    ] {
        for (fields, selectors) in [("a:F", vec![1]), ("a:F;b:F", vec![1, 2])] {
            for (parameters, owner, replacement) in [
                ("state:State<i64,F>,tag:i32,replacement:F", 0, 2),
                ("replacement:F,tag:i32,state:State<i64,F>", 2, 0),
            ] {
                let text = format!(
                    "package State<T,F>{{manages bytes:ByteBuffer;{fields};tag:T}} \
                     fn replace<F:{bound}()->i32>({parameters})->State<i64,F>{{return state}} fn main(){{}}"
                );
                header(&text, owner, replacement, &selectors, kind);
            }
        }
    }
}

#[test]
fn unused_copy_parameter_keeps_the_early_complete_header_obligation() {
    // No concrete ordinary T means this source definition is checked and then
    // omitted. It cannot rely on the eventual Core selector validator.
    let core = run(LAST_USE).unwrap();
    assert!(!core.functions.iter().any(|f| f.name == "replace"));
    for text in [
        LAST_USE.replace("a:F;b:F;tag:T", "a:F;b:F;c:F;tag:T"),
        LAST_USE.replace("a:F;b:F;tag:T", "a:F;b:(F,);tag:T"),
        LAST_USE.replace("replacement:F)", "replacement:F,extra:ByteBuffer)"),
        LAST_USE.replace("->State<T,F>{", "->State<i64,F>{"),
        LAST_USE.replace("F:Fn()->i32", "F:Fn()->&[u8]"),
    ] {
        let errors = run(&text).unwrap_err();
        assert!(errors.iter().all(|d| d.phase == Phase::Expand));
        assert!(
            errors.iter().any(|d| matches!(
                d.category,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            )),
            "{errors:?}"
        );
    }
}

#[test]
fn adjacent_shapes_do_not_use_the_mixed_header_exception() {
    for text in [
        ORIGINAL.replace("replacement:F", "replacement:State<F>"),
        ORIGINAL.replace("replacement:F", "replacement:&F"),
        ORIGINAL.replace("state:State<F>", "state:&mut State<F>"),
        ORIGINAL.replace("replacement:F)", "replacement:F,another:F)"),
    ] {
        let errors = run(&text).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.category == ResultCategory::Unsupported),
            "{errors:?}"
        );
    }
}

fn alias_reject(text: &str, message: &str) {
    let errors = run(text).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Expand
            && d.category == ResultCategory::SafeReject
            && d.code == "E-GENERIC-ARGUMENT"
            && d.message.contains(message)),
        "{errors:?}"
    );
}

#[test]
fn original_omitted_and_retained_definitions_reject_the_same_old_address() {
    alias_reject(MIXED_ALIAS, "State alias outlives");
    alias_reject(SINGLE_ALIAS, "State alias outlives");
    let non_t = SINGLE_ALIAS
        .replace("State<T,F>", "State<F>")
        .replace(";tag:T", "")
        .replace("<T:Copy,F:", "<F:");
    alias_reject(&non_t, "State alias outlives");
    run(LAST_USE).unwrap();
    run(&SINGLE_ALIAS.replace("    view()\n", "").replace(
        "    let replacement=next.a",
        "    view()\n    let replacement=next.a",
    ))
    .unwrap();
}

#[test]
fn last_use_sibling_and_fresh_epoch_keep_distinct_field_positions() {
    run(&MIXED_ALIAS.replace("next.a=replacement", "next.b=replacement")).unwrap();
    run(&MIXED_ALIAS.replace("    view()", "    let fresh=&next.a\n    fresh()")).unwrap();
    run(&SINGLE_ALIAS.replace("    view()", "    let fresh=&next.a\n    fresh()")).unwrap();
    alias_reject(
        &MIXED_ALIAS.replace("let view=&next.a", "let first=&next.a\n    let view=first"),
        "State alias outlives",
    );
    alias_reject(
        &MIXED_ALIAS
            .replace("next.a=replacement", "let other=next")
            .replace("return next", "return other"),
        "State alias outlives",
    );
}

#[test]
fn branch_join_and_loop_condition_do_not_resurrect_an_old_epoch() {
    alias_reject(
        &MIXED_ALIAS
            .replace("replacement:F)", "replacement:F,flag:bool)")
            .replace("next.a=replacement", "if flag { next.a=replacement }"),
        "State alias outlives",
    );
    run(&LAST_USE
        .replace("replacement:F)", "replacement:F,flag:bool)")
        .replace("    view()", "    if flag { view() } else { view() }"))
    .unwrap();
    let prefix = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{var next=state;let view=&next.a;";
    alias_reject(
        &format!(
            "{prefix}while view()==0 {{ let moved=next.a;next.a=moved }} return next }}fn main(){{}}"
        ),
        "loop condition or backedge",
    );
    alias_reject(
        &format!(
            "{prefix}while true {{ view();let moved=next.a;next.a=moved;continue }} return next }}fn main(){{}}"
        ),
        "loop condition or backedge",
    );
    run(&format!("{prefix}while view()==0 {{ let moved=next.a;next.a=moved;break }} return next }}fn main(){{}}")).unwrap();
    run(&format!(
        "{prefix}while view()==0 {{ let moved=next.b;next.b=moved }} return next }}fn main(){{}}"
    ))
    .unwrap();
}

#[test]
fn callee_and_earlier_argument_windows_survive_nested_preparation() {
    let prefix = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn consume<F:Fn(i32)->i32>(callback:F)->i32{return 0}\nfn both<F:Fn(i32)->i32>(callback:&F,value:i32)->i32{return callback(value)}\n";
    for call in [
        "view(consume(next.a))",
        "next.a(consume(next.a))",
        "both(&next.a,consume(next.a))",
    ] {
        let text = format!(
            "{prefix}fn replace<T:Copy,F:Fn(i32)->i32>(state:State<T,F>,replacement:F)->State<T,F>{{var next=state;let view=&next.a;{call};next.a=replacement;return next}}fn main(){{}}"
        );
        alias_reject(&text, "prepared callee or argument window");
        let sibling = text
            .replace("consume(next.a)", "consume(next.b)")
            .replace("next.a=replacement", "next.b=replacement");
        run(&sibling).unwrap();
    }
}

#[test]
fn conditional_addresses_and_reborrows_preserve_alternatives_and_parent_precision() {
    let text = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn replace<T:Copy,F:FnMut()->i32>(state:State<T,F>,replacement:F,flag:bool)->State<T,F>{var next=state;let view=if flag { &mut next.a } else { &mut next.b };view();return next}fn main(){}";
    run(text).unwrap();
    let reads = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{var next=state;let first=&mut next.tag;let second=&mut *first;let one=*second;let two=*first;return next}fn main(){}";
    run(reads).unwrap();
    alias_reject(
        &reads.replace(
            "let one=*second;let two=*first",
            "let one=*first;let two=*second",
        ),
        "State alias outlives",
    );
    let refresh = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{var next=state;var view=&next.a;while view()==0 {let moved=next.a;next.a=moved;view=&next.a}return next}fn main(){}";
    run(refresh).unwrap();
    alias_reject(
        &refresh.replace(
            "let moved=next.a;next.a=moved;view=&next.a",
            "view=&next.a;let moved=next.a;next.a=moved",
        ),
        "loop condition or backedge",
    );
}

fn boundary(text: &str) -> usize {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "state205-budget.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let check = |n| {
        zil_expand::expand_with_map_limits(
            &source,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    check(high).unwrap();
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match check(n) {
            Ok(_) => high = n,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = n;
            }
        }
    }
    check(high).unwrap();
    assert!(
        check(high - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    check(high).unwrap();
    high
}

#[test]
fn real_alias_work_and_new_header_have_exact_fresh_budget_boundaries() {
    let legacy =
        include_str!("../../zil-driver/tests/impl197_cases/01-existing-state-f-sibling/main.zil");
    for (name, source) in [
        ("header", ORIGINAL),
        ("unused_alias", LAST_USE),
        ("legacy_alias", legacy),
    ] {
        let n = boundary(source);
        eprintln!("state205_{name}_expand_n={n}");
        if name == "legacy_alias" {
            assert!(n > 1731);
        }
    }
}
