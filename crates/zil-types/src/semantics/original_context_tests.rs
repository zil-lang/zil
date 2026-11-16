use super::*;

fn core() -> CoreModule {
    CoreModule {
        semantic: Some(SemanticModule {
            definitions: vec![SemanticDefinition {
                id: SemanticDefinitionId::default(),
                identity: LifeDeclarationIdentity {
                    package: "lease-test".into(),
                    module_path: vec![],
                    declaration_path: vec!["f".into()],
                    generation_path: vec![],
                },
                parent: None,
                kind: SemanticDefinitionKind::Function,
                parameters: vec![],
                implicit_lifetimes: vec![],
                requirements: vec![],
                environment: vec![],
                body: SemanticDefinitionBody::Function {
                    signature: SemanticSignatureRecipe {
                        bound_lifetimes: 0,
                        invocation_lifetimes: 0,
                        parameters: vec![],
                        result: SemanticTypeRecipe::Atom(Type::I32),
                        result_mode: PassingMode::Infer,
                        call_kind: CallKind::Direct,
                    },
                    sites: vec![],
                },
            }],
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![],
        }),
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        closure_life_contexts: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: zil_base::Fingerprint::ZERO,
        functions: vec![],
        aggregates: vec![],
        aggregate_templates: vec![],
        aggregate_applications: vec![],
        enums: vec![],
        unsupported: vec![],
    }
}
// These tests exercise private transport after the separate original checker;
// the input proof is a fixture, never an executable acceptance result.
fn fixture_proof(input: &Input) -> super::super::original::Proof {
    let mut proof = super::super::original::Proof::default();
    proof.declarations = input.declarations.clone();
    proof.definition_positions = Rc::new(
        input
            .semantic
            .definitions
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id, index))
            .collect(),
    );
    proof
        .complete
        .extend(input.semantic.definitions.iter().map(|row| row.id));
    proof
}
fn leased<'a>(
    core: &'a CoreModule,
    limits: &Limits,
    left: &mut usize,
) -> (Rc<OriginalContext>, InputLease<'a>) {
    let (input, pending) = Input::collect_leased(&[core], limits, left).unwrap();
    let proof = fixture_proof(&input);
    OriginalContext::checked_leased(input, proof, None, pending, left).unwrap()
}

#[test]
fn immutable_input_lease_keeps_exact_live_context_and_core() {
    let source = core();
    let limits = Limits::default();
    let mut left = limits.max_analysis_steps;
    let (context, lease) = leased(&source, &limits, &mut left);
    assert!(
        lease
            .matches(&context, &[&source], None, &limits, &mut left)
            .unwrap()
    );
    let mut other = source.clone();
    assert!(
        !lease
            .matches(&context, &[&other], None, &limits, &mut left)
            .unwrap()
    );
    context
        .verify_leased_inputs(&[&other], None, Some(&lease), &limits, &mut left)
        .unwrap();
    other.semantic.as_mut().unwrap().definitions[0]
        .identity
        .declaration_path
        .push("changed".into());
    assert!(
        context
            .verify_leased_inputs(&[&other], None, Some(&lease), &limits, &mut left)
            .is_err()
    );
    other.semantic = None;
    assert!(
        context
            .verify_leased_inputs(&[&other], None, Some(&lease), &limits, &mut left)
            .is_err()
    );
    let (other_context, _) = leased(&source, &limits, &mut left);
    assert!(
        !lease
            .matches(&other_context, &[&source], None, &limits, &mut left)
            .unwrap()
    );
    let mut tighter = limits.clone();
    tighter.max_call_depth = 0;
    assert!(
        !lease
            .matches(&context, &[&source], None, &tighter, &mut left)
            .unwrap()
    );
    assert!(
        context
            .verify_leased_inputs(&[&source], None, Some(&lease), &tighter, &mut left)
            .is_err()
    );
}

#[test]
fn lease_does_not_authorize_modified_publication_definitions() {
    let source = core();
    let limits = Limits::default();
    let mut left = limits.max_analysis_steps;
    let (context, _lease) = leased(&source, &limits, &mut left);
    let mut publication = context.publication_copy(&limits, &mut left).unwrap();
    context
        .verify_definitions(&publication, &limits, &mut left)
        .unwrap();
    let SemanticDefinitionBody::Function { signature, .. } = &mut publication.definitions[0].body
    else {
        panic!("fixture is a function")
    };
    signature.result = SemanticTypeRecipe::Atom(Type::I64);
    assert!(
        context
            .verify_definitions(&publication, &limits, &mut left)
            .is_err()
    );
}

#[test]
fn pending_lease_cannot_be_spliced_onto_another_snapshot() {
    let source = core();
    let limits = Limits::default();
    let mut left = limits.max_analysis_steps;
    let (_first, pending) = Input::collect_leased(&[&source], &limits, &mut left).unwrap();
    let second = Input::collect(&[&source], &limits, &mut left).unwrap();
    let proof = fixture_proof(&second);
    assert!(OriginalContext::checked_leased(second, proof, None, pending, &mut left).is_err());
}
