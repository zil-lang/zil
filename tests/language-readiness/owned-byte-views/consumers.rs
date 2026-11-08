//! Actual owned ByteBuffer payload views beside stored address loans.
use super::*;
use zil_base::TargetProfile;
use zil_ir::{CandidatePlan, TypedModule};

const LOCAL: &str = include_str!("local-field.zil");

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "owned-byte-view.zil", text)
}

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &Limits::default(),
        &TargetProfile::host_llvm(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn control(module: &TypedModule) -> CandidatePlan {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    plan
}

#[test]
fn actual_owned_byte_view_reads_the_payload_and_ends_before_relocation() {
    let report = compile_source(&source(LOCAL), &CompileOptions::default());
    println!("owned byte view: {}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    control(&typed(LOCAL));
}

#[test]
fn actual_owned_local_bytebuffer_uses_its_own_head_with_stored_loans() {
    let source = LOCAL
        .replace("    var reply=Reply{data:give data,tag:20}\n", "")
        .replace("reply.data", "data");
    control(&typed(&source));
}

fn check_source(text: &str, label: &str) -> CompileReport {
    let report = compile_source(&source(text), &CompileOptions::default());
    println!("{label}: {}", report.to_json());
    report
}

fn replace_effect(effect: &str) -> String {
    LOCAL.replace("    bytes_reserve(reply.data,2)\n", effect)
}

#[test]
fn byte_views_reject_active_relocation_move_and_field_replacement() {
    for (label, effect) in [
        ("reserve", "bytes_reserve(reply.data,2)"),
        ("push", "bytes_push(reply.data,67u8)"),
        ("truncate", "bytes_truncate(reply.data,0)"),
        ("field give", "let previous=give reply.data"),
        ("field replacement", "reply.data=bytes_new()"),
        ("whole move", "let moved=reply"),
    ] {
        let text = replace_effect("").replace(
            "    let observed=view[0]",
            &format!("    {effect}\n    let observed=view[0]"),
        );
        let report = check_source(&text, label);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
    }
    let indexed = replace_effect("").replace(
        "    let observed=view[0]",
        "    let selected=&view[0]\n    bytes_reserve(reply.data,2)\n    let observed=*selected",
    );
    let report = check_source(&indexed, "indexed child after slice SSA ends");
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn byte_views_keep_other_owning_heads_and_fields_independent() {
    let other = LOCAL
        .replace(
            "    let view=bytes_view(reply.data)",
            "    var other=Reply{data:bytes_new(),tag:30}\n    let view=bytes_view(reply.data)",
        )
        .replace(
            "    let observed=view[0]",
            "    bytes_reserve(other.data,2)\n    let observed=view[0]",
        );
    let other_field = LOCAL
        .replace(
            "manages data:ByteBuffer; tag:i64",
            "manages data:ByteBuffer; manages extra:ByteBuffer; tag:i64",
        )
        .replace(
            "data:give data,tag:20",
            "data:give data,extra:bytes_new(),tag:20",
        )
        .replace(
            "    let observed=view[0]",
            "    bytes_reserve(reply.extra,2)\n    let observed=view[0]",
        );
    for (label, text) in [("other owner", other), ("other actual field", other_field)] {
        let report = check_source(&text, label);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        control(&typed(&text));
    }
}

#[test]
fn new_byte_description_after_replacement_observes_the_new_instance() {
    let text = replace_effect("    reply.data=bytes_new()\n    if !bytes_reserve(reply.data,1) {return 3}\n    if !bytes_push(reply.data,77u8) {return 4}\n    let fresh=bytes_view(reply.data)\n").replace("return i32(observed)-66+*held.at-7", "return i32(observed)+i32(fresh[0])-143+*held.at-7");
    let report = check_source(&text, "new byte instance after field replacement");
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    control(&typed(&text));
}

#[test]
fn descriptor_only_helper_calls_need_a_checked_effect_while_view_is_live() {
    let prefix = "fn resize(buffer:ByteBuffer){bytes_reserve(buffer,2)}\n";
    let valid = format!("{prefix}{}", replace_effect("    resize(reply.data)\n"));
    let active = format!(
        "{prefix}{}",
        replace_effect("").replace(
            "    let observed=view[0]",
            "    resize(reply.data)\n    let observed=view[0]"
        )
    );
    let alias = format!(
        "{prefix}{}",
        replace_effect("")
            .replace(
                "    let view=bytes_view(reply.data)",
                "    let alias=reply.data\n    let view=bytes_view(reply.data)"
            )
            .replace(
                "    let observed=view[0]",
                "    resize(alias)\n    let observed=view[0]"
            )
    );
    let report = check_source(&valid, "helper after last view read");
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    control(&typed(&valid));
    for (label, text) in [
        ("helper with direct description", active),
        ("helper with aliased description", alias),
    ] {
        let report = check_source(&text, label);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("synchronous read consumer")),
            "{}",
            report.to_json()
        );
    }
}
