//! Actual owned ByteBuffer payload views beside stored address loans.
use super::*;
use zil_base::TargetProfile;
use zil_driver::{CompileReport, compile_source};
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

#[test]
fn byte_head_indexes_keep_function_local_ssa_identity() {
    let left = LOCAL.replace("fn main()->i32", "fn left()->i32");
    let body = LOCAL.split_once("fn main()->i32").unwrap().1;
    let right = body.replace("66u8", "77u8").replace("-66+", "-77+");
    let text = format!("{left}\nfn right()->i32{right}\nfn main()->i32{{return left()+right()}}");
    let report = check_source(&text, "same SSA numbers in distinct owning functions");
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let module = typed(&text);
    let mut views = module
        .functions
        .iter()
        .flat_map(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter(|i| matches!(i.kind, zil_ir::InstructionKind::ByteBufferBorrow { .. }))
                .map(move |i| (f.id, i.result.unwrap()))
        })
        .collect::<Vec<_>>();
    views.sort();
    assert_eq!(views.len(), 2);
    assert_ne!(views[0].0, views[1].0);
    assert_eq!(views[0].1, views[1].1);
    control(&module);
}

#[test]
fn byte_view_loops_use_current_loan_payloads_and_end_each_read_before_growth() {
    let text = LOCAL.replace(
        "    let view=bytes_view(reply.data)\n    let observed=view[0]\n    bytes_reserve(reply.data,2)",
        "    var observed=0u8\n    var iteration=0\n    while iteration<2 {\n        let view=bytes_view(reply.data)\n        let selected=&view[0]\n        observed=*selected\n        bytes_reserve(reply.data,2)\n        iteration+=1\n    }",
    );
    assert_ne!(text, LOCAL);
    let report = check_source(&text, "latest byte-view loan payload over a real backedge");
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    control(&typed(&text));
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
        assert!(report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
            && d.message == "stored target proof: simultaneously live access overlaps an actual stored target/parent loan"), "{label}: {}", report.to_json());
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
    assert!(report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
        && d.message == "stored target proof: simultaneously live access overlaps an actual stored target/parent loan"), "{}", report.to_json());
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
fn byte_view_sources_keep_old_descriptor_epochs_and_partial_branches() {
    for (label, effect) in [
        ("field replacement", "reply.data=bytes_new()"),
        ("whole replacement", "reply=Reply{data:bytes_new(),tag:30}"),
    ] {
        let text = LOCAL.replace(
            "    let view=bytes_view(reply.data)",
            &format!("    let saved=reply.data\n    {effect}\n    let view=bytes_view(saved)"),
        );
        let report = check_source(&text, label);
        // The actual saved description cannot survive replacement of its
        // owning field; reject that retirement before constructing a view.
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, zil_base::Phase::Analyze);
        assert_eq!(report.verification_runs, 0);
        assert!(report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
            && d.message == "resource closure: head transition invalidates a description required later"), "{label}: {}", report.to_json());
    }
    let partial = LOCAL
        .replace("fn main()->i32", "fn check(flag:bool)->i32")
        .replace(
            "    let view=bytes_view(reply.data)",
            "    if flag {let moved=give reply.data}\n    let view=bytes_view(reply.data)",
        );
    let partial = format!("{partial}\nfn main()->i32{{return check(true)}}");
    let report = check_source(&partial, "one predecessor has no owned field");
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
            && d.message
                == "resource closure: package description requires that field live on every path"),
        "{}",
        report.to_json()
    );
    for projection in ["&view[0]", "&view[0..1]"] {
        let text = replace_effect("").replace(
            "    let observed=view[0]",
            &format!("    let selected={projection}\n    bytes_reserve(reply.data,2)\n    let observed=view[0]"),
        );
        // The original slice has no later use; only its selected child keeps
        // the manager pinned across relocation.
        let text = if projection == "&view[0]" {
            text.replace("let observed=view[0]", "let observed=*selected")
        } else {
            text.replace("let observed=view[0]", "let observed=selected[0]")
        };
        let report = check_source(&text, projection);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
            && d.message == "stored target proof: simultaneously live access overlaps an actual stored target/parent loan"), "{projection}: {}", report.to_json());
    }
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

#[test]
fn byte_views_raw_and_wire_reconstruct_heads_and_active_payload_loans() {
    use zil_ir::InstructionKind;
    let module = typed(LOCAL);
    let plan = control(&module);
    for attack in [
        "relocation-before-read",
        "moved-manager",
        "wrong-field",
        "duplicate-producer",
        "unsealed-resource",
    ] {
        let mut raw = module.clone();
        if attack == "unsealed-resource" {
            let resource = raw
                .resources
                .iter_mut()
                .find(|r| r.name == "@builtin:ByteBuffer")
                .unwrap();
            resource.name.push_str("Forged");
        } else {
            let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
            if attack == "relocation-before-read" {
                let block = f
                    .blocks
                    .iter_mut()
                    .find(|block| {
                        block
                            .instructions
                            .iter()
                            .any(|i| matches!(i.kind, InstructionKind::ByteBufferBorrow { .. }))
                    })
                    .unwrap();
                let read = block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::ReadAddress { .. }))
                    .unwrap();
                let reserve = block
                    .instructions
                    .iter()
                    .rposition(|i| matches!(i.kind, InstructionKind::ByteBufferReserve { .. }))
                    .unwrap();
                assert!(read < reserve);
                let observation = block.instructions[read].result.unwrap();
                let store = block
                    .instructions
                    .iter()
                    .position(|i| {
                        matches!(i.kind,
                    InstructionKind::Store { value, .. } if value == observation)
                    })
                    .unwrap();
                assert!(read < store && store < reserve);
                // Preserve SSA dominance, including the observation's local
                // Store. Only the payload read/use cross the actual write.
                let observation = block.instructions.drain(read..=store).collect::<Vec<_>>();
                let insertion = reserve - (store - read);
                block.instructions.splice(insertion..insertion, observation);
            } else {
                let data = f.locals.iter().find(|l| l.name == "data").unwrap().id;
                let describe = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::DescribePackageField { .. }))
                    .unwrap();
                match attack {
                    "moved-manager" => {
                        describe.kind = InstructionKind::DescribeLocal { local: data }
                    }
                    "wrong-field" => {
                        let InstructionKind::DescribePackageField { index, .. } =
                            &mut describe.kind
                        else {
                            unreachable!()
                        };
                        *index = 1;
                    }
                    "duplicate-producer" => {
                        let duplicate = describe.clone();
                        f.blocks
                            .iter_mut()
                            .find(|b| b.instructions.iter().any(|i| i == &duplicate))
                            .unwrap()
                            .instructions
                            .push(duplicate);
                    }
                    _ => unreachable!(),
                }
            }
        }
        let mut claim = plan.clone();
        assert_ne!(claim.module_fingerprint, raw.fingerprint());
        claim.module_fingerprint = raw.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
        for (kernel, diagnostics) in [
            (
                "Analyze",
                zil_analyze::analyze(&raw, &Limits::default()).unwrap_err(),
            ),
            (
                "Verify",
                zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default())
                    .unwrap_err(),
            ),
            (
                "Wire",
                zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default())
                    .unwrap_err(),
            ),
        ] {
            // Only relocation exercises an active loan conflict. The other
            // mutations authenticate initialization, source structure or the
            // sealed resource; their earlier failures are recorded separately.
            let (category, code, message) = match (attack, kernel == "Analyze") {
                ("relocation-before-read", true) => (
                    ResultCategory::SafeReject,
                    "E-ANALYZE-IR",
                    "stored target proof: simultaneously live access overlaps an actual stored target/parent loan",
                ),
                ("relocation-before-read", false) => (
                    ResultCategory::SafeReject,
                    "E-VERIFY-STORED",
                    "stored target proof: actual target intersects a live incompatible loan",
                ),
                ("moved-manager", true) => (
                    ResultCategory::SafeReject,
                    "E-UNINITIALIZED",
                    "local 2 is not initialized on every incoming path",
                ),
                ("moved-manager", false) => (
                    ResultCategory::SafeReject,
                    "E-VERIFY-INITIALIZATION",
                    "load may read a local before initialization",
                ),
                ("wrong-field", true) => (
                    ResultCategory::SafeReject,
                    "E-ANALYZE-IR",
                    "stored target proof: stored/address producer has incomplete actual target facts",
                ),
                ("wrong-field", false) => (
                    ResultCategory::SafeReject,
                    "E-VERIFY-PACKAGE",
                    "package description requires a manages field",
                ),
                ("duplicate-producer", true) => (
                    ResultCategory::SafeReject,
                    "E-UNINITIALIZED",
                    "local 6 is not initialized on every incoming path",
                ),
                ("duplicate-producer", false) => (
                    ResultCategory::SafeReject,
                    "E-VERIFY-STORED",
                    "stored target proof: byte source has more than one actual SSA definition",
                ),
                ("unsealed-resource", true) => (
                    ResultCategory::SafeReject,
                    "E-ANALYZE-IR",
                    "stored target proof: byte view requires the canonical sealed ByteBuffer resource",
                ),
                ("unsealed-resource", false) => (
                    ResultCategory::SafeReject,
                    "E-VERIFY-BYTE-BUFFER",
                    "byte storage definition must be unique, sealed and have no user teardown",
                ),
                _ => unreachable!(),
            };
            // Preserve the exact observed diagnostic even if a future gate
            // changes which assertion below fails first.
            eprintln!("{attack}/{kernel}: {diagnostics:?}");
            assert_eq!(diagnostics.len(), 1, "{attack}/{kernel}: {diagnostics:?}");
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.category, category, "{attack}/{kernel}");
            assert_eq!(
                diagnostic.phase,
                if kernel == "Analyze" {
                    zil_base::Phase::Analyze
                } else {
                    zil_base::Phase::Verify
                },
                "{attack}/{kernel}"
            );
            assert_eq!(diagnostic.code, code, "{attack}/{kernel}");
            assert_eq!(diagnostic.message, message, "{attack}/{kernel}");
            eprintln!("{attack}/{kernel}: {diagnostics:?}");
        }
    }
}

#[test]
fn byte_views_raw_and_wire_do_not_reuse_a_retired_descriptor_after_refill() {
    use zil_ir::InstructionKind;
    for replacement in [
        "reply.data=bytes_new()",
        "reply=Reply{data:bytes_new(),tag:30}",
    ] {
        let text = LOCAL.replace(
            "    let view=bytes_view(reply.data)",
            &format!(
                "    let saved=reply.data\n    {replacement}\n    let view=bytes_view(reply.data)"
            ),
        );
        let mut raw = typed(&text);
        let mut claim = control(&raw);
        let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let retired = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::DescribePackageField { .. }))
            .unwrap()
            .result
            .unwrap();
        let view = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::ByteBufferBorrow { .. }))
            .unwrap();
        let InstructionKind::ByteBufferBorrow { buffer, .. } = &mut view.kind else {
            unreachable!()
        };
        assert_ne!(*buffer, retired);
        *buffer = retired;
        claim.module_fingerprint = raw.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
        for (kernel, errors) in [
            (
                "Analyze",
                zil_analyze::analyze(&raw, &Limits::default()).unwrap_err(),
            ),
            (
                "Verify",
                zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default())
                    .unwrap_err(),
            ),
            (
                "Wire",
                zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default())
                    .unwrap_err(),
            ),
        ] {
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::SafeReject),
                "{kernel}: {errors:?}"
            );
            assert!(
                errors.iter().all(|d| !d.code.contains("DOMINANCE")),
                "{kernel}: {errors:?}"
            );
            assert!(errors.iter().any(|d| if kernel == "Analyze" {
                d.code == "E-ANALYZE-IR" && d.message == "resource closure: head transition invalidates a description required later"
            } else {
                d.code == "E-VERIFY-RESOURCE" && d.message == "moving, clearing or reconstructing a resource head would invalidate a live description"
            }), "retired descriptor/{kernel}: {errors:?}");
            eprintln!("retired descriptor/{kernel}: {errors:?}");
        }
    }
}
