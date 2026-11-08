//! Source signature admission, environment modes and separately closed
//! profiles. Native execution and independent raw/Wire attacks are separate.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CallableKind, TypedModule};

const DATA: &str = "resource R{n:i32}package Event{manages r:R;n:i32}";
fn typed(source: &str) -> TypedModule {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owning_callback_signature_admission.zil",
        source,
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn output(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review246-{label}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn consuming_new_data_does_not_consume_fn_or_fnmut_environment() {
    for (label, body, kind) in [
        (
            "fn",
            "let f=(event:Event)->Event=>{return event}",
            CallableKind::Fn,
        ),
        (
            "fnmut",
            "var n=0;var f=move(event:Event)->Event=>{n+=1;return event}",
            CallableKind::FnMut,
        ),
        (
            "once",
            "let r=R{n:1};let f=move(event:Event)->Event=>{let held=give r;return event}",
            CallableKind::FnOnce,
        ),
    ] {
        let source = format!("{DATA}fn main(){{{body}}}");
        let out = output(label);
        fs::write(out.join("source.zil"), &source).unwrap();
        let module = typed(&source);
        fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
        assert_eq!(module.closures.len(), 1);
        assert_eq!(module.closures[0].kind, kind, "{label}");
        assert_eq!(module.closures[0].parameters.len(), 1);
        assert_eq!(module.closures[0].parameters[0], module.closures[0].result);
    }
}

#[test]
fn unused_public_single_once_definitions_check_the_complete_owned_signature() {
    let source = format!(
        "{DATA}fn dispatch<F:FnOnce(Event)->Event>(f:F,event:Event)->Event{{return f(event)}}fn main(){{}}"
    );
    typed(&source);
    typed(
        "resource F{n:i32}package Event{manages leaf:F}fn dispatch<F:FnOnce(Event)->Event>(f:F,event:Event)->Event{return f(event)}fn main(){let f=(event:Event)->Event=>{return event};let result=dispatch(f,Event{leaf:F{n:7}})}",
    );
}

#[test]
fn owning_data_does_not_open_unrelated_helper_or_capture_profiles() {
    let cases = [
        (
            "borrowed-public",
            "fn dispatch<F:Fn(Event)->Event>(f:&F,event:Event)->Event{return f(event)}fn main(){}",
        ),
        (
            "two-public-roles",
            "fn dispatch<F:FnOnce(Event)->Event,G:FnOnce(Event)->Event>(f:F,g:G,event:Event)->Event{return g(f(event))}fn main(){}",
        ),
        (
            "enum-capture",
            "enum Choice{Empty,Data(Event)}fn main(){let value=Choice.Empty;let f=move(event:Event)->Event=>{let held=value;return event}}",
        ),
        (
            "store-capture-owning-result",
            "fn main(){let values:Store<Event>=store_new();let f=move(event:Event)->Event=>{store_len(values);return event}}",
        ),
        (
            "shared-product-owning-result",
            "fn main(){let parent=Event{r:R{n:1},n:2};let view=&parent;let f=move(event:Event)->Event=>{let n=view.n;return event}}",
        ),
    ];
    for (label, tail) in cases {
        let source = format!("{DATA}{tail}");
        let out = output(label);
        fs::write(out.join("source.zil"), &source).unwrap();
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "owning_callback_signature_admission.zil",
                source,
            ),
            &CompileOptions {
                stop_after: Stage::Types,
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        let (phase, reason) = match label {
            "borrowed-public" => (Phase::Types, "finite nonescaping callable roles"),
            // Final frontend preserves the legacy multi-F Expand route and
            // checks the new owning profile in Types, including unused Core.
            "two-public-roles" => (Phase::Types, "finite nonescaping callable roles"),
            "enum-capture" => (Phase::Types, "capture requires a finite Copy value"),
            "store-capture-owning-result" => (
                Phase::Types,
                "Store capture environments require Copy results",
            ),
            "shared-product-owning-result" => (Phase::Types, "shared Product capture"),
            _ => unreachable!(),
        };
        assert!(
            report.diagnostics.iter().any(|d| d.phase == phase
                && d.code == "E-UNSUPPORTED"
                && d.message.contains(reason)),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn borrowed_input_owned_result_calls_keep_environment_modes_and_input_usable() {
    for (setup, closure, invoke, kind) in [
        (
            "",
            "let f=(item:&Event)->Event=>{return Event{r:R{n:3},n:item.n+1}}",
            "let reply=f(&item)",
            CallableKind::Fn,
        ),
        (
            "var count=0;",
            "var f=move(item:&Event)->Event=>{count+=1;return Event{r:R{n:3},n:item.n+count}}",
            "let first=f(&item);let reply=f(&item)",
            CallableKind::FnMut,
        ),
        (
            "let held=R{n:9};",
            "let f=move(item:&Event)->Event=>{let token=give held;return Event{r:give token,n:item.n+1}}",
            "let reply=f(&item)",
            CallableKind::FnOnce,
        ),
    ] {
        let source = format!(
            "{DATA}fn main()->i32{{let item=Event{{r:R{{n:1}},n:7}};{setup}{closure};{invoke};return reply.n-item.n-1}}"
        );
        let module = typed(&source);
        assert_eq!(module.closures[0].kind, kind);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    }
}

#[test]
fn borrowed_input_owned_result_public_helpers_check_unused_definitions() {
    for header in [
        "fn dispatch<F:Fn(&Event)->Event>(f:&F,item:&Event)->Event{return f(item)}",
        "fn dispatch<F:FnMut(&Event)->Event>(f:&mut F,item:&Event)->Event{return f(item)}",
        "fn dispatch<F:FnOnce(&Event)->Event>(f:F,item:&Event)->Event{return f(item)}",
    ] {
        typed(&format!("{DATA}{header}fn main(){{}}"));
    }
    let source = format!(
        "{DATA}fn dispatch<F:FnMut(&Event)->Event>(f:&mut F,item:&Event)->Event{{return f(item)}}fn main()->i32{{let item=Event{{r:R{{n:1}},n:7}};var count=0;var f=move(item:&Event)->Event=>{{count+=1;return Event{{r:R{{n:3}},n:item.n+count}}}};let a=dispatch(&mut f,&item);let b=dispatch(&mut f,&item);return a.n+b.n-item.n*2-3}}"
    );
    let module = typed(&source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
}

#[test]
fn borrowed_input_owned_result_crosses_actual_model_and_helper_modules() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/callbacks/borrowed-input-owned-result/main.zil");
    let report = zil_driver::compile_path(
        &path,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn owned_and_borrowed_inputs_each_retain_their_own_responsibility() {
    let source = format!(
        "{DATA}fn main()->i32{{let n=7;let f=(event:Event,p:&i32)->Event=>{{return event}};let reply=f(Event{{r:R{{n:3}},n:2}},&n);return reply.n+n-9}}"
    );
    let module = typed(&source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
}

#[test]
fn borrowed_input_owned_callbacks_keep_actual_input_and_result_authority_separate() {
    for (label, tail) in [
        (
            "move-input-leaf",
            "fn main(){let item=Event{r:R{n:1},n:2};let f=(item:&Event)->Event=>{return Event{r:give item.r,n:item.n}};let reply=f(&item)}",
        ),
        (
            "mutable-through-shared",
            "fn main(){let item=Event{r:R{n:1},n:2};let f=(item:&Event)->Event=>{item.n=3;return Event{r:R{n:4},n:item.n}};let reply=f(&item)}",
        ),
        (
            "private-create",
            "fn extract()->R contract{result(create)}{let item=Event{r:R{n:1},n:2};let f=(item:&Event)->Event=>{return Event{r:R{n:3},n:item.n}};let reply=f(&item);return give reply.r}fn main(){}",
        ),
        (
            "unused-once-twice",
            "fn bad<F:FnOnce(&Event)->Event>(f:F,item:&Event)->Event{let first=f(item);return f(item)}fn main(){}",
        ),
        (
            "unused-shared-mut",
            "fn bad<F:FnMut(&Event)->Event>(f:&F,item:&Event)->Event{return f(item)}fn main(){}",
        ),
    ] {
        let source = format!("{DATA}{tail}");
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "borrowed_input_callback_reject.zil",
                source,
            ),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        let expected = if label == "move-input-leaf" {
            // Giving a field through a borrowed Package has no owner place;
            // preserve the existing precise unsupported operation boundary.
            assert!(report.diagnostics.iter().any(|d| {
                d.phase == Phase::Types
                    && d.message
                        .contains("give requires a managed local or direct Managed package field")
            }));
            ResultCategory::Unsupported
        } else {
            ResultCategory::SafeReject
        };
        assert_eq!(report.status, expected, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0, "{label}");
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn mixed_callback_cannot_move_an_owner_while_passing_its_live_internal_alias() {
    let source = format!(
        "{DATA}fn main(){{let event=Event{{r:R{{n:3}},n:2}};let view=&event;let f=(owned:Event,borrowed:&Event)->Event=>{{let n=borrowed.n;return owned}};let reply=f(event,view)}}"
    );
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "mixed_input_alias.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.phase == Phase::Analyze),
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn borrowed_owned_callback_new_work_has_exact_and_fresh_budget_boundaries() {
    let source = "package Reply{manages bytes:ByteBuffer;n:i32}fn dispatch<F:FnMut(&i32)->Reply>(f:&mut F,n:&i32)->Reply{return f(n)}fn main()->i32{var count=0;var f=move(n:&i32)->Reply=>{count+=1;return Reply{bytes:bytes_new(),n:*n+count}};let n=7;let reply=dispatch(&mut f,&n);return reply.n-8}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "borrowed_owned_budget.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let run =
            |n| {
                let limits = Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                };
                match phase {
                    Phase::Types => zil_types::check(&core, &target, &limits)
                        .map(|actual| assert_eq!(actual, module)),
                    Phase::Analyze => zil_analyze::analyze(&module, &limits)
                        .map(|actual| assert_eq!(actual, plan)),
                    Phase::Verify => {
                        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
                            .map(|actual| {
                                assert_eq!(actual.module(), &module);
                                assert_eq!(actual.plan(), &plan);
                            })
                    }
                    _ => unreachable!(),
                }
            };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        run(high).unwrap();
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(()) => high = mid,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{phase:?}: {errors:?}"
                    );
                    low = mid;
                }
            }
        }
        run(high).unwrap();
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        run(high).unwrap();
        println!(
            "borrowed input owned result {phase:?}: N={high}, N-1 ResourceLimit, fresh N equal"
        );
    }
}

#[test]
fn borrowed_slice_inputs_and_copy_slice_results_keep_their_separate_profiles() {
    for source in [
        format!("{DATA}fn dispatch<F:Fn(&[i32])->Event>(f:&F,s:&[i32])->Event{{return f(s)}}fn main()->i32{{let values=[7,9];let f=(s:&[i32])->Event=>{{return Event{{r:R{{n:3}},n:s[0]+s[1]}}}};let reply=dispatch(&f,&values[..]);return reply.n-16}}"),
        "fn main()->i32{let values=[7,9];let f=(s:&[i32])->i32=>{return s[0]+s[1]};return f(&values[..])-16}".into(),
    ] {
        let module=typed(&source);
        let plan=zil_analyze::analyze(&module,&Limits::default()).unwrap();
        zil_verify::verify_module(module.clone(),plan,&module.target,&Limits::default()).unwrap();
    }
}
