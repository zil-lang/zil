//! Test-only untrusted snapshots. They never change the production checker or
//! turn a missing class/W/partition proof into a Types Success result.
use crate::*;
use std::cell::RefCell;
use std::path::PathBuf;
use zil_base::{FileId, Fingerprint, ModuleId, SourceUnit};

thread_local! {
    static DESTINATION: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Test-only phase evidence for failures before complete semantic materialization.
/// These text dumps are not published Typed candidates or proof receipts.
pub(super) fn record_stage(module: &TypedModule, stage: &str) -> R<()> {
    let destination = DESTINATION.with(|slot| slot.borrow().clone());
    if let Some(destination) = destination {
        std::fs::write(
            destination.join(format!("untrusted-{stage}-typed.txt")),
            format!("{module:#?}"),
        )
        .map_err(|error| crate::error("E-SEMANTIC-PROBE", Span::default(), error.to_string()))?;
    }
    Ok(())
}

pub(super) fn record(
    module: &mut TypedModule,
    semantic: &SemanticModule,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let destination = DESTINATION.with(|slot| slot.borrow().clone());
    let Some(destination) = destination else {
        return Ok(());
    };
    crate::dyn_metadata::semantic_snapshot(semantic, limits, left, 1)?;
    module.semantic = Some(semantic.clone());
    // This intentionally is not an analyzed CandidatePlan. Its empty claims
    // cannot be used as an Analyze or Verify success receipt. Independent
    // consumers can decode the actual Typed operands and reconstruct theirs.
    let candidate = CandidatePlan {
        schema_version: SCHEMA_VERSION,
        module_fingerprint: Fingerprint::ZERO,
        target_fingerprint: Fingerprint::ZERO,
        functions: Vec::new(),
        origins: Vec::new(),
        responsibilities: Vec::new(),
        unsupported: Vec::new(),
    };
    let bytes =
        zil_ir::wire::encode_module_candidate(module, &candidate, limits).map_err(|error| {
            crate::error(
                "E-SEMANTIC-PROBE",
                Span::default(),
                format!("untrusted snapshot encoding failed: {error:?}"),
            )
        })?;
    std::fs::write(destination.join("untrusted-typed.wire"), bytes)
        .map_err(|error| crate::error("E-SEMANTIC-PROBE", Span::default(), error.to_string()))?;
    std::fs::write(
        destination.join("untrusted-typed.txt"),
        format!("{module:#?}"),
    )
    .map_err(|error| crate::error("E-SEMANTIC-PROBE", Span::default(), error.to_string()))?;
    Ok(())
}

#[test]
fn original_source_semantics_reach_typed_materialization_and_the_complete_proof_gate() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/runs/semantic-types-source-probe");
    let mut failures = Vec::new();
    for (label, text) in [
        (
            "ordinary-generic",
            include_str!("../../../../tests/fixtures/semantic-scopes/ordinary-generic.zil"),
        ),
        (
            "unused-generic-identity",
            include_str!("../../../../tests/fixtures/semantic-scopes/unused-generic-identity.zil"),
        ),
        (
            "local-closure",
            include_str!("../../../../tests/fixtures/semantic-scopes/local-closure.zil"),
        ),
        (
            "phantom-capture",
            include_str!(
                "../../../../tests/fixtures/returned-owning-holders/semantic-only-phantom-capture.zil"
            ),
        ),
    ] {
        let directory = root.join(label);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("source.zil"), text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), label, text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let aliases = BTreeMap::new();
        let context = zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        };
        let expanded = zil_expand::expand_program_with_semantics(
            &[source],
            &[&syntax],
            &[Vec::new()],
            &limits,
            &target,
            context,
        );
        let core = match expanded {
            Ok(mut modules) => modules.remove(0).0.core,
            Err(errors) => {
                std::fs::write(
                    directory.join("diagnostics.txt"),
                    format!("Source failed: {errors:#?}"),
                )
                .unwrap();
                failures.push(format!("{label}: Source {errors:?}"));
                continue;
            }
        };
        std::fs::write(directory.join("core.txt"), format!("{core:#?}")).unwrap();
        if core.semantic.is_none() {
            failures.push(format!("{label}: actual Source emission omitted Some"));
            continue;
        }
        DESTINATION.with(|slot| *slot.borrow_mut() = Some(directory.clone()));
        let typed = crate::check(&core, &target, &limits);
        DESTINATION.with(|slot| *slot.borrow_mut() = None);
        std::fs::write(directory.join("diagnostics.txt"), format!("{typed:#?}")).unwrap();
        match typed {
            Ok(module) => assert!(
                module.semantic.is_some(),
                "{label}: final publication erased the original program"
            ),
            Err(errors) => failures.push(format!("{label}: Types {errors:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "formal Types Success still requires every original proof:\n{}",
        failures.join("\n")
    );
}

/// Concrete i32/i32 instantiations and dead branches cannot discharge an
/// invalid symbolic definition. Unsupported is not a successful rejection.
#[test]
fn original_generic_definitions_reject_independent_binders_and_missing_copy() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/runs/semantic-types-original-counterexamples");
    let mut failures = Vec::new();
    for (label, text) in [
        (
            "unused-generic-copy",
            include_str!("../../../../tests/fixtures/semantic-scopes/unused-generic-copy.zil"),
        ),
        (
            "equal-actuals-generic-return",
            include_str!(
                "../../../../tests/fixtures/semantic-scopes/equal-actuals-generic-return.zil"
            ),
        ),
        (
            "equal-actuals-generic-call",
            include_str!(
                "../../../../tests/fixtures/semantic-scopes/equal-actuals-generic-call.zil"
            ),
        ),
        (
            "unselected-generic-branch",
            include_str!(
                "../../../../tests/fixtures/semantic-scopes/unselected-generic-branch.zil"
            ),
        ),
    ] {
        let directory = root.join(label);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("source.zil"), text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), label, text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let aliases = BTreeMap::new();
        let context = zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        };
        let core = match zil_expand::expand_program_with_semantics(
            &[source],
            &[&syntax],
            &[Vec::new()],
            &limits,
            &target,
            context,
        ) {
            Ok(mut modules) => modules.remove(0).0.core,
            Err(errors) => {
                std::fs::write(
                    directory.join("diagnostics.txt"),
                    format!("Source failed: {errors:#?}"),
                )
                .unwrap();
                // Source already checks symbolic definitions. Its precise
                // semantic rejection is a valid language-pipeline outcome,
                // but is not evidence that Types/A/V checked this body.
                let rejected = !errors.is_empty()
                    && errors.iter().all(|error| {
                        error.phase == Phase::Expand
                            && error.category == zil_base::ResultCategory::SafeReject
                            && error.code == "E-GENERIC-ARGUMENT"
                    });
                if !rejected {
                    failures.push(format!(
                        "{label}: Source failed before original proof: {errors:?}"
                    ));
                }
                continue;
            }
        };
        std::fs::write(directory.join("core.txt"), format!("{core:#?}")).unwrap();
        let complete_original = core.semantic.as_ref().is_some_and(|semantic| {
            semantic.definitions.iter().any(|definition| {
                matches!(
                    definition.body,
                    SemanticDefinitionBody::FunctionWithBody { .. }
                )
            }) && !semantic.definitions.iter().any(|definition| {
                matches!(definition.body, SemanticDefinitionBody::Function { .. })
            })
        });
        if !complete_original {
            failures.push(format!(
                "{label}: Source omitted the complete original body"
            ));
            continue;
        }
        let checked = crate::check(&core, &target, &limits);
        std::fs::write(directory.join("diagnostics.txt"), format!("{checked:#?}")).unwrap();
        let rejected = checked.as_ref().err().is_some_and(|errors| {
            !errors.is_empty()
                && errors.iter().all(|error| {
                    error.phase == Phase::Types
                        && error.category == zil_base::ResultCategory::SafeReject
                        && error.code == "E-SEMANTIC-ORIGINAL"
                })
        });
        if !rejected {
            failures.push(format!(
                "{label}: expected original symbolic rejection: {checked:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
