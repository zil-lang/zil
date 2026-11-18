//! Reconstruct the rejected call's candidate from the corresponding legal
//! source so that Verify can inspect the unchanged, rejected Typed function.
use super::*;
use zil_ir::{HoldRoot, TypedFunction, ValueHold, ValueId};

fn typed_original(text: &str, limits: &Limits, target: &TargetProfile) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let aliases = BTreeMap::new();
    let context = zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
        root: ModuleId(0),
        aliases: &aliases,
        declarations: zil_expand::source_context::DeclarationPlacement::Original,
    };
    let core = zil_expand::expand_program_with_semantics(
        &[source],
        &[&syntax],
        &[Vec::new()],
        limits,
        target,
        context,
    )
    .unwrap()
    .remove(0)
    .0
    .core;
    let typed = zil_types::check(&core, target, limits).unwrap();
    assert!(typed.semantic.is_some());
    typed
}

// A single entry block forms Store descriptors, saves window addresses, and
// reborrows them for the actual call. Track each local and SSA loan separately.
fn window_holds(function: &TypedFunction) -> Vec<ValueHold> {
    assert!(function.parameters.is_empty() && function.blocks.len() == 1);
    let mut descriptors = vec![None; function.values.len()];
    let mut values = vec![None; function.values.len()];
    let mut locals = vec![None; function.locals.len()];
    for instruction in &function.blocks[0].instructions {
        let root = match instruction.kind {
            I::DescribeLocal { local } => {
                descriptors[instruction.result.unwrap().0 as usize] = Some(local);
                None
            }
            I::OwnedStoreBorrowMut { store, .. } | I::OwnedStoreBorrow { store, .. } => {
                Some(HoldRoot::Local(descriptors[store.0 as usize].unwrap()))
            }
            I::Store { local, value } => {
                locals[local.0 as usize] = values[value.0 as usize];
                None
            }
            I::StorageLive(local) | I::StorageDead(local) => {
                locals[local.0 as usize] = None;
                None
            }
            I::Load(local) => locals[local.0 as usize],
            I::ReborrowAddress { address, .. } => values[address.0 as usize],
            _ => {
                assert!(instruction.result.is_none_or(|id| !matches!(
                    function.values[id.0 as usize],
                    Type::Address { .. }
                )));
                None
            }
        };
        if let Some(id) = instruction.result {
            values[id.0 as usize] = root;
        }
    }
    values
        .into_iter()
        .enumerate()
        .map(|(index, root)| ValueHold {
            value: ValueId(index as u32),
            roots: root.into_iter().collect(),
        })
        .collect()
}

pub(super) fn verify_original(label: &str, bad_source: &str, control: &str) {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let good = typed_original(control, &limits, &target);
    let mut plan = zil_analyze::analyze(&good, &limits).unwrap();
    zil_verify::verify_module(good.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&good, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();

    let bad = typed_original(bad_source, &limits, &target);
    let errors = zil_analyze::analyze(&bad, &limits).unwrap_err();
    assert!(
        errors.iter().any(|e| {
            e.code == "E-ANALYZE-IR"
                && e.category == ResultCategory::SafeReject
                && if label == "current-call-structural-effect" {
                    e.message
                        .contains("actual caller aliases storage required separate by the callee")
                } else {
                    e.notes.iter().any(|note| {
                        note.contains("access roots=[Local(LocalId(0))]")
                            && note.contains("live roots=[Local(LocalId(0))]")
                    }) && e
                        .span
                        .is_some_and(|s| &bad_source[s.start as usize..s.end as usize] == "p")
                }
        }),
        "{label}: {errors:?}"
    );

    assert_eq!(good.functions.len(), bad.functions.len());
    let main = good
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    for (index, (a, b)) in good.functions.iter().zip(&bad.functions).enumerate() {
        if index != main {
            assert_eq!(a, b);
        }
    }
    assert!(
        plan.origins.is_empty() && plan.responsibilities.is_empty() && plan.unsupported.is_empty()
    );
    let claim = &mut plan.functions[main];
    assert_eq!(claim.blocks.len(), 1);
    assert!(
        claim.blocks[0].initialized_in.is_empty() && claim.blocks[0].initialized_out.is_empty()
    );
    assert_eq!(window_holds(&good.functions[main]), claim.holds);
    claim.holds = window_holds(&bad.functions[main]);
    plan.module_fingerprint = bad.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&bad, &plan, &limits).unwrap();
    let raw = zil_verify::verify_module(bad, plan, &target, &limits).unwrap_err();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        raw
    );
    let occurrence = if label == "current-call-structural-effect" {
        bad_source.rfind("pair(p,s)").unwrap()..bad_source.rfind('}').unwrap()
    } else {
        let second = bad_source.rfind(",p)").unwrap() + 1;
        second..second + 1
    };
    assert!(
        raw.iter().any(|e| {
            e.phase == Phase::Verify
                && e.category == ResultCategory::SafeReject
                && e.code == "E-BORROW-CONFLICT"
                && e.message.matches("base: Local(LocalId(0))").count() == 2
                && e.span.is_some_and(|s| (s.start..s.end) == occurrence)
                && e.related.iter().any(|row| {
                    row.role == zil_base::DiagnosticRole::ConflictingAccess && row.span == e.span
                })
        }),
        "{label}: {raw:?}"
    );
}
