//! Rebuild current untrusted fixtures from their complete actual Source. This
//! writer does not decode or relabel any historical wire format. The generated
//! plan remains an untrusted candidate for the independent raw/Wire tests.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};

fn original_source(label: &str) -> String {
    match label {
        "semantic/ordinary-identity" => "crates/zil-verify/src/semantic/fixtures/ordinary-identity.zil".into(),
        "dyn-context/static-callback-getter-cycle" => "tests/fixtures/dyn-context/static-callback-getter-cycle-control.zil".into(),
        "returned-owning-holders/semantic-only-phantom-capture" => format!("tests/fixtures/{label}.zil"),
        "loan_control" => "tests/borrowed-callable-arguments/loan_control.zil".into(),
        label if label.starts_with("dyn-context/") => format!("crates/zil-verify/tests/fixtures/{label}.zil"),
        _ => format!("tests/{label}.zil"),
    }
}

// Installation remains the caller's explicit copy of these exact generated
// bytes. A mapping names an existing fixture reader; no historical bytes are
// loaded and no schema/version field is edited by this producer.
fn fixture_outputs(label: &str) -> Vec<(&'static str, String)> {
    match label {
        "semantic/ordinary-identity" => vec![
            ("candidate", "crates/zil-verify/src/semantic/fixtures/ordinary-identity-complete-call-sites.wire".into()),
        ],
        "dyn-context/context-read" => vec![
            ("typed", format!("crates/zil-verify/tests/fixtures/{label}.typed.wire")),
            ("candidate", format!("crates/zil-verify/tests/fixtures/{label}.candidate.wire")),
        ],
        "dyn-context/context-factory" | "dyn-context/context-unused" => vec![
            ("candidate", format!("crates/zil-verify/tests/fixtures/{label}.candidate.wire")),
        ],
        "dyn-context/static-callback-getter-cycle" => vec![
            ("typed", format!("crates/zil-verify/tests/fixtures/{label}.typed.wire")),
        ],
        "returned-owning-holders/semantic-only-phantom-capture" => vec![
            ("candidate", "crates/zil-verify/src/semantic/fixtures/phantom-closure-complete-creator.wire".into()),
        ],
        "loan_control" => vec![
            ("candidate", "tests/borrowed-callable-arguments/loan_control.candidate.wire".into()),
            ("revival.candidate", "tests/borrowed-callable-arguments/loan_revival.candidate.wire".into()),
        ],
        label if label.starts_with("returned-environment-sources/") => vec![
            ("candidate", format!("tests/{label}.wire")),
        ],
        _ => Vec::new(),
    }
}

#[test]
fn complete_original_sources_publish_current_untrusted_fixture_records() {
    let output = std::env::var_os("ZIL_CURRENT_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "zil-current-source-fixtures-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
        });
    fs::create_dir_all(&output).unwrap();
    let cases = [
        (
            "semantic/ordinary-identity",
            include_str!("../../zil-verify/src/semantic/fixtures/ordinary-identity.zil"),
        ),
        (
            "dyn-context/context-read",
            include_str!("../../zil-verify/tests/fixtures/dyn-context/context-read.zil"),
        ),
        (
            "dyn-context/context-factory",
            include_str!("../../zil-verify/tests/fixtures/dyn-context/context-factory.zil"),
        ),
        (
            "dyn-context/context-unused",
            include_str!("../../zil-verify/tests/fixtures/dyn-context/context-unused.zil"),
        ),
        (
            "dyn-context/short-external-view",
            include_str!("../../zil-verify/tests/fixtures/dyn-context/short-external-view.zil"),
        ),
        (
            "dyn-context/short-inline-view",
            include_str!("../../zil-verify/tests/fixtures/dyn-context/short-inline-view.zil"),
        ),
        (
            "dyn-context/static-callback-getter-cycle",
            include_str!(
                "../../../tests/fixtures/dyn-context/static-callback-getter-cycle-control.zil"
            ),
        ),
        (
            "returned-environment-sources/two-addresses",
            include_str!("../../../tests/returned-environment-sources/two-addresses.zil"),
        ),
        (
            "returned-environment-sources/owned-projected-multiple",
            include_str!(
                "../../../tests/returned-environment-sources/owned-projected-multiple.zil"
            ),
        ),
        (
            "returned-environment-sources/holder-pointer",
            include_str!("../../../tests/returned-environment-sources/holder-pointer.zil"),
        ),
        (
            "returned-environment-sources/holder-pointer-replaced",
            include_str!("../../../tests/returned-environment-sources/holder-pointer-replaced.zil"),
        ),
        (
            "returned-environment-sources/nested-borrowed",
            include_str!("../../../tests/returned-environment-sources/nested-borrowed.zil"),
        ),
        (
            "returned-environment-sources/wrapped-borrowed",
            include_str!("../../../tests/returned-environment-sources/wrapped-borrowed.zil"),
        ),
        (
            "returned-environment-sources/loop-sources",
            include_str!("../../../tests/returned-environment-sources/loop-sources.zil"),
        ),
        (
            "returned-owning-holders/semantic-only-phantom-capture",
            include_str!("../../../tests/fixtures/returned-owning-holders/semantic-only-phantom-capture.zil"),
        ),
        (
            "loan_control",
            include_str!("../../../tests/borrowed-callable-arguments/loan_control.zil"),
        ),
    ];
    // Exact labels only, so a focused fixture refresh cannot silently select
    // zero inputs or hide a typo. Without a filter every complete case runs.
    let selected = std::env::var("ZIL_CURRENT_FIXTURE_CASES").ok().map(|value| {
        value.split(',').map(str::trim).map(str::to_owned).collect::<BTreeSet<_>>()
    });
    if let Some(selected) = &selected {
        assert!(!selected.is_empty() && selected.iter().all(|label|
            cases.iter().any(|(known, _)| label.as_str() == *known)), "unknown or empty current Source fixture selection: {selected:?}");
    }
    let expected = cases.iter().filter(|(label, _)|
        selected.as_ref().is_none_or(|selected| selected.contains(*label))).count();
    assert!(expected > 0, "current Source fixture selection must be nonempty");
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut failures = Vec::new();
    let mut records = Vec::new();
    for (label, text) in cases {
        if selected.as_ref().is_some_and(|selected| !selected.contains(label)) { continue; }
        let path = output.join(label);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path.with_extension("zil"), text).unwrap();
        let result = (|| -> Result<(), String> {
            let source = SourceUnit::new(FileId(0), ModuleId(0), format!("{label}.zil"), text);
            let syntax = zil_syntax::parse(&source)
                .into_result()
                .map_err(|e| format!("Parse: {e:#?}"))?;
            let aliases = BTreeMap::new();
            let expanded = zil_expand::expand_program_with_semantics(
                std::slice::from_ref(&source),
                &[&syntax],
                &[Vec::new()],
                &limits,
                &target,
                zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                    root: source.module_id,
                    aliases: &aliases,
                    declarations: zil_expand::source_context::DeclarationPlacement::Original,
                },
            )
            .map_err(|e| format!("Expand: {e:#?}"))?;
            if expanded.len() != 1 {
                return Err("Source changed the one actual module inventory".into());
            }
            let core = &expanded[0].0.core;
            fs::write(path.with_extension("core.txt"), format!("{core:#?}")).unwrap();
            let module =
                zil_types::check(core, &target, &limits).map_err(|e| format!("Types: {e:#?}"))?;
            if module.schema_version != zil_base::SCHEMA_VERSION
                || module
                    .semantic
                    .as_ref()
                    .is_none_or(|program| program.definitions.is_empty())
            {
                return Err(
                    "Source fixture omitted its complete original semantic inventory".into(),
                );
            }
            let original = core.semantic.as_ref().ok_or("Source omitted its original definitions")?;
            if module.semantic.as_ref().unwrap().definitions != original.definitions {
                return Err("Types changed or omitted complete original Source definitions".into());
            }
            fs::write(path.with_extension("typed.txt"), format!("{module:#?}")).unwrap();
            let empty = zil_ir::CandidatePlan::empty_for(&module);
            let typed = zil_ir::wire::encode_module_candidate(&module, &empty, &limits)
                .map_err(|e| format!("typed wire: {e:#?}"))?;
            let (decoded, decoded_plan) = zil_ir::wire::decode_module_candidate(&typed, &limits)
                .map_err(|e| format!("typed decode: {e:#?}"))?;
            if decoded != module || decoded_plan != empty {
                return Err(
                    "complete typed fixture round trip changed actual Source metadata".into(),
                );
            }
            fs::write(path.with_extension("typed.wire"), &typed).unwrap();
            let plan = zil_analyze::analyze(&module, &limits)
                .map_err(|e| format!("Analyze candidate: {e:#?}"))?;
            let candidate = zil_ir::wire::encode_module_candidate(&module, &plan, &limits)
                .map_err(|e| format!("candidate wire: {e:#?}"))?;
            let (decoded, decoded_plan) =
                zil_ir::wire::decode_module_candidate(&candidate, &limits)
                    .map_err(|e| format!("candidate decode: {e:#?}"))?;
            if decoded != module || decoded_plan != plan {
                return Err("complete candidate fixture round trip changed untrusted input".into());
            }
            fs::write(path.with_extension("candidate.wire"), &candidate).unwrap();
            if label == "loan_control" {
                // Preserve the exact actual operand mutation independently
                // exercised by forwarded_callback_verification_wire. Keep
                // the complete original Source program and every plan claim.
                let mut changed = module.clone();
                let mut changed_plan = plan.clone();
                let main = changed
                    .functions
                    .iter_mut()
                    .find(|function| function.name == "main")
                    .ok_or("loan control has no actual main")?;
                let owner = main
                    .locals
                    .iter()
                    .find(|local| local.name == "e")
                    .ok_or("loan control has no actual owner")?
                    .id;
                let sibling = main
                    .locals
                    .iter()
                    .find(|local| local.name == "other")
                    .ok_or("loan control has no actual sibling")?
                    .id;
                if owner == sibling
                    || main.locals[owner.0 as usize].ty != main.locals[sibling.0 as usize].ty
                {
                    return Err("loan control actual owner/sibling types differ".into());
                }
                let mut mutations = 0;
                for instruction in main
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.instructions)
                {
                    if let zil_ir::InstructionKind::TakeLocal { local } = &mut instruction.kind
                        && *local == sibling
                    {
                        *local = owner;
                        mutations += 1;
                    }
                }
                if mutations != 1 {
                    return Err(format!(
                        "loan control requires one actual mutation, got {mutations}"
                    ));
                }
                changed_plan.module_fingerprint = changed.fingerprint();
                let revival =
                    zil_ir::wire::encode_module_candidate(&changed, &changed_plan, &limits)
                        .map_err(|e| format!("loan revival wire: {e:#?}"))?;
                let (decoded, decoded_plan) =
                    zil_ir::wire::decode_module_candidate(&revival, &limits)
                        .map_err(|e| format!("loan revival decode: {e:#?}"))?;
                if decoded != changed || decoded_plan != changed_plan {
                    return Err("loan revival round trip changed its exact actual mutation".into());
                }
                fs::write(path.with_extension("revival.candidate.wire"), &revival).unwrap();
                fs::write(
                    path.with_extension("revival.typed.txt"),
                    format!("{changed:#?}"),
                )
                .unwrap();
            }
            let original = original_source(label);
            let mappings = fixture_outputs(label).into_iter().map(|(kind, destination)|
                format!("{{\"artifact\":\"{label}.{kind}.wire\",\"destination\":\"{destination}\"}}"))
                .collect::<Vec<_>>().join(",");
            records.push(format!("{{\"source\":\"{label}.zil\",\"original_source\":\"{original}\",\"source_sha256\":\"{}\",\"typed_sha256\":\"{}\",\"candidate_sha256\":\"{}\",\"module_fingerprint\":\"{}\",\"semantic_definitions\":{},\"functions\":{},\"closures\":{},\"fixture_mappings\":[{mappings}]}}",Fingerprint::of(text.as_bytes()).to_hex(),Fingerprint::of(&typed).to_hex(),Fingerprint::of(&candidate).to_hex(),module.fingerprint().to_hex(),module.semantic.as_ref().unwrap().definitions.len(),module.functions.len(),module.closures.len()));
            Ok(())
        })();
        if let Err(error) = result {
            fs::write(path.with_extension("errors.txt"), &error).unwrap();
            failures.push(format!("{label}: {error}"));
        }
    }
    fs::write(output.join("manifest.json"),format!("{{\"status\":\"{}\",\"schema\":{},\"wire\":{},\"authority\":\"UntrustedSourceAndProducerCandidates; independent raw/Wire verification required\",\"selected_cases\":{expected},\"records\":[{}]}}\n",if failures.is_empty(){"Generated"}else{"Incomplete"},zil_base::SCHEMA_VERSION,zil_ir::wire::WIRE_VERSION,records.join(","))).unwrap();
    assert!(
        failures.is_empty(),
        "{}; actual fixture output {}",
        failures.join("\n"),
        output.display()
    );
    assert_eq!(records.len(), expected);
    eprintln!("current complete Source fixtures: {}", output.display());
}
