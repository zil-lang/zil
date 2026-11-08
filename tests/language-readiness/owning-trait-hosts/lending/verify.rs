//! Real Store windows retain their manager identity across both admission kernels.
use super::*;

#[test]
fn owning_item_lending_iterator_raw_and_wire_reject_wrong_sources() {
    let library = OWNING_LENDING_LIBRARY
        .replace("import protocol", "")
        .replace("protocol.LendingIterator", "LendingIterator")
        .replace("fn next<", "fn next_forward<");
    let model = OWNING_LENDING_MODEL
        .replace("import protocol", "")
        .replace("import library", "")
        .replace("protocol.LendingIterator", "LendingIterator")
        .replace("library.next", "next_forward")
        .replace(
            "let index=self.index",
            "let local=Item{data:bytes_new(),value:8};let unrelated:Store<Item>=store_new();let capacity=store_len(unrelated);let index=self.index",
        );
    let text =
        format!("{OWNING_LENDING_PROTOCOL}\n{library}\n{model}\nfn main()->i32{{return run()}}");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owning-item-lending.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap();

    for substitute_manager in [false, true] {
        let mut forged = typed.clone();
        let provider = forged
            .functions
            .iter_mut()
            .find(|function| {
                function
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. }))
            })
            .unwrap();
        let local = provider
            .locals
            .iter()
            .position(|local| local.name == "local")
            .expect("source retains the complete unrelated Item");
        let other = provider
            .locals
            .iter()
            .position(|local| local.name == "unrelated")
            .expect("source retains the unrelated Store<Item>");
        let manager = provider
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::DescribeLocal { local } if local.0 as usize == other => i.result,
                _ => None,
            })
            .expect("real Store description precedes the element borrow");
        let instruction = provider
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. }))
            .unwrap();
        let result = instruction.result.unwrap();
        assert!(matches!(&provider.values[result.0 as usize],
            zil_ir::Type::Address { mutable: false, inner } if **inner == provider.locals[local].ty));
        if substitute_manager {
            let InstructionKind::OwnedStoreBorrow { store, .. } = &mut instruction.kind else {
                unreachable!()
            };
            assert_eq!(
                provider.values[store.0 as usize],
                provider.values[manager.0 as usize]
            );
            *store = manager;
        } else {
            instruction.kind = InstructionKind::BorrowLocal {
                local: zil_ir::LocalId(local as u32),
            };
        }
        let errors =
            zil_analyze::analyze(&forged, &Limits::default()).expect_err("unrelated actual source");
        eprintln!("Analyze forged manager={substitute_manager}: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject
                    && d.message.contains("stored target proof")),
            "{errors:?}"
        );
        let mut forged_plan = plan.clone();
        forged_plan.module_fingerprint = forged.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&forged, &forged_plan, &Limits::default())
            .unwrap();
        for errors in [
            zil_verify::verify_module(
                forged.clone(),
                forged_plan,
                &forged.target,
                &Limits::default(),
            )
            .unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default()).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|d| d.phase == zil_base::Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.code == "E-VERIFY-STORED"),
                "{errors:?}"
            );
            eprintln!("forged source manager={substitute_manager}: {errors:?}");
        }
    }
    for wrong_variant in [false, true] {
        let mut forged = typed.clone();
        let mut changed = false;
        for function in &mut forged.functions {
            for ty in function
                .locals
                .iter_mut()
                .map(|local| &mut local.ty)
                .chain(&mut function.values)
            {
                let zil_ir::Type::BorrowedStorage { life, .. } = ty else {
                    continue;
                };
                for row in &mut life.slots {
                    for step in &mut row.path {
                        if let zil_ir::PlaceProjection::Variant(selector) = step {
                            if wrong_variant {
                                selector.variant = u32::MAX
                            } else {
                                selector.index = u32::MAX
                            }
                            changed = true;
                        }
                    }
                }
            }
        }
        assert!(changed, "real Option slot inventory must be present");
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&forged, &claim, &Limits::default()).unwrap();
        for result in [
            zil_verify::verify_module(forged.clone(), claim, &forged.target, &Limits::default()),
            zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default()),
        ] {
            let errors = result.expect_err("invalid actual variant selector");
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::SafeReject),
                "{errors:?}"
            );
            eprintln!("invalid Option selector variant={wrong_variant}: {errors:?}");
        }
    }
}
