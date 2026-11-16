use std::collections::BTreeMap;
use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit, TargetProfile};
use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};

fn expand(text: &str) -> Result<zil_ir::CoreModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "task-callable-source.zil", text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let aliases = BTreeMap::new();
    zil_expand::expand_with_map_semantics(
        &source,
        &parsed,
        &Limits::default(),
        &TargetProfile::default(),
        SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .map(|expanded| expanded.core)
}

#[test]
fn task_callable_sources_keep_original_context_and_poll_results() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/returned-task-result-fnonce.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil"
        ),
    ] {
        let core = expand(text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(
            core.semantic.is_some(),
            "full original program must remain present"
        );
        assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
        assert!(!core.functions.is_empty());
    }
}

#[test]
fn scalar_observer_and_mutable_callable_check_the_original_body() {
    for text in [
        "extern \"C\" fn observed(code:i32);fn observe<F:FnOnce()->i32>(callback:F)->i32{let answer=callback();unsafe{observed(answer);}return answer;}fn main()->i32{let callback=move()=>42;return observe(callback);}",
        "fn run<F:FnMut()->i32>(original:F)->i32{var callback=original;let first=callback();return first+callback();}fn main()->i32{var n=0;let callback=move()=>{n+=1;return n;};return run(callback);}",
    ] {
        let core = expand(text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(core.semantic.is_some());
        assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
    }
    // These unused declarations must still be checked before specialization.
    for text in [
        "extern \"C\" fn observed(code:i32);fn observe<F:FnOnce()->i32>(callback:F)->i32{let answer=callback();observed(answer);return answer;}fn main(){}",
        "extern \"C\" fn observed(code:i32);fn observe<F:FnOnce()->i32>(callback:F)->i32{let answer=callback();unsafe{observed(true);}return answer;}fn main(){}",
        "fn run<F:FnMut()->i32>(original:F)->i32{let callback=original;return callback();}fn main(){}",
        "fn run<F:FnMut()->i32>(original:F)->i32{var first=original;var second=original;return first()+second();}fn main(){}",
    ] {
        let errors = expand(text).unwrap_err();
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand),
            "{text}: {errors:?}"
        );
    }
}
