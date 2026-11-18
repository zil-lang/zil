use super::{TWO_REGISTRATIONS, source};

#[test]
fn distinct_calls_to_the_same_factory_supply_their_own_registration() {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_ir::{
        InstructionKind, SemanticFunctionTarget, SemanticInstanceTarget, SemanticSiteAddress,
    };

    let input = source(TWO_REGISTRATIONS);
    let target = zil_base::TargetProfile::default();
    let limits = zil_base::Limits::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_semantics(
        &input,
        &ast,
        &limits,
        &target,
        SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &std::collections::BTreeMap::new(),
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let check = |module: &zil_ir::TypedModule, plan: &zil_ir::CandidatePlan| {
        let bytes = zil_ir::wire::encode_module_candidate(module, plan, &limits).unwrap();
        [
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).map(|_| ()),
            zil_verify::verify_module_bytes(&bytes, &target, &limits).map(|_| ()),
        ]
    };
    for valid in check(&module, &plan) {
        valid.unwrap();
    }

    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let mut starts = function.blocks.iter().flat_map(|block| {
        block
            .instructions
            .iter()
            .enumerate()
            .filter_map(move |(index, instruction)| {
                matches!(
                    instruction.kind,
                    InstructionKind::AsyncRegistrationStart { .. }
                )
                .then_some((block.id, index as u32))
            })
    });
    let first = starts.next().unwrap();
    let second = starts.next().unwrap();
    assert!(starts.next().is_none());
    let original = module.semantic.as_mut().unwrap();
    let target = SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id));
    let instance = original
        .instances
        .iter()
        .find(|row| row.target == target)
        .unwrap()
        .id;
    let mut changed = 0;
    for site in original
        .sites
        .iter_mut()
        .filter(|site| site.instance == instance)
    {
        if let SemanticSiteAddress::Typed {
            block, instruction, ..
        } = &mut site.at
        {
            let at = (*block, *instruction);
            if at == first || at == second {
                (*block, *instruction) = if at == first { second } else { first };
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 2);
    let a = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert_eq!(a[0].code, "E-ANALYZE-IR");
    assert!(
        a[0].message.contains("another original factory result"),
        "{a:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    for invalid in check(&module, &plan) {
        let errors = invalid.unwrap_err();
        assert_eq!(errors[0].code, "E-VERIFY-SEMANTIC");
        assert!(
            errors[0]
                .notes
                .iter()
                .any(|note| note.contains("registration callback operand")),
            "{errors:?}"
        );
    }
}
