//! A provenance-transfer regression, independent of Source type admission.
//! A current raw constructor supplies a checked original declaration world;
//! this test exercises its protocol interpreter with one Closed owner context.
//! It neither decodes historical Wire nor claims Source/Types acceptance.
use super::*;

fn expr(kind: CoreExprKind<SemanticTypeRecipe>) -> SemanticOriginalExpr {
    CoreExpr { id: NodeId(0), kind, span: zil_base::Span::default() }
}

#[test]
fn returned_input_owner_cannot_disappear_through_a_nested_aggregate_or_opaque_call() {
    let limits = zil_base::Limits::default();
    // The provenance interpreter needs a checked original lexical world,
    // not a particular producer candidate or historical wire layout.
    let mut module = crate::static_utf8_tests::module();
    module.functions.clear();
    module.semantic = Some(SemanticModule {
        definitions: vec![SemanticDefinition {
            id: SemanticDefinitionId { module: zil_base::ModuleId(0), index: 0 },
            identity: LifeDeclarationIdentity { package: "raw-owner-transfer-control".into(),
                module_path: vec!["protocol".into()], declaration_path: vec!["context".into()],
                generation_path: vec![] },
            parent: None, kind: SemanticDefinitionKind::Function, parameters: vec![],
            implicit_lifetimes: vec![], requirements: vec![], environment: vec![],
            body: SemanticDefinitionBody::FunctionWithBody {
                signature: SemanticSignatureRecipe { bound_lifetimes: 0, invocation_lifetimes: 0,
                    parameters: vec![], result: SemanticTypeRecipe::Atom(Type::Unit),
                    result_mode: PassingMode::Infer, call_kind: CallKind::Direct },
                protocol_contracts: vec![], foreign: None, resume_bound: TaskResumeBound::Local,
                sites: vec![], parameters: vec![], receiver_parameter: None, names: vec![], site_nodes: vec![],
                body: SemanticOriginalFunctionBody::Defined(CoreBlock {
                    statements: vec![], span: zil_base::Span::default(),
                }),
            },
        }],
        instances: vec![], sites: vec![], dyn_scopes: vec![],
    });
    let program = module.semantic.as_ref().unwrap();
    let mut budget = Budget::new(&limits);
    let inventory = Inventory::new(&module, &limits, program, &mut budget).unwrap();
    let checked = super::super::check(&module, program, &inventory, &mut budget).unwrap();
    let definition = program.definitions.iter().find(|definition|
        matches!(definition.body, SemanticDefinitionBody::FunctionWithBody { .. })).unwrap();
    let proof = Proof { definition, inventory: &inventory, checked: &checked,
        contract: None, input: None, loops: std::cell::RefCell::new(Vec::new()) };

    // A successful Task owner and a returned input are distinct occurrences
    // in mutually exclusive arms, even when their storage classes coincide.
    let input_role = 71;
    let task_role = 72;
    let mut carrier = Data { sources: OwnerSources::exact(task_role), ..Data::default() };
    carrier.sources.append_under(&[PlaceProjection::ErrorArm(true), PlaceProjection::Field(1),
        PlaceProjection::Field(0)], &OwnerSources::exact(input_role).with_opaque(true), &mut budget).unwrap();
    let failure = carrier.error_payload(true, &mut budget).unwrap();
    let inputs = failure.project(PlaceProjection::Field(1), &mut budget).unwrap();
    assert!(inputs.sources.has_any());
    assert!(inputs.sources.opaque_exposure(), "returned opaque input exposure survives the same selected carrier");
    assert_eq!(inputs.sources.only_exact_root(), None);
    let mut state = State { phases: BTreeMap::from([(input_role, CLOSED), (task_role, 0)]),
        locals: vec![("inputs".into(), inputs)], alive: true, cleanup_unknown: false };
    let name = CorePattern { kind: CorePatternKind::Name("returned".into()),
        span: zil_base::Span::default() };
    let pattern = CorePattern { kind: CorePatternKind::Tuple(vec![name]),
        span: zil_base::Span::default() };
    let inputs = state.locals[0].1.payload(&mut budget).unwrap();
    proof.bind_pattern(&mut state, &pattern, &inputs, &mut budget, 0).unwrap();
    assert_eq!(state.locals[1].1.sources.only_exact_root(), None);
    let returned = state.locals[1].1.sources.clone();
    assert_eq!(returned.receiver_with_effect(&mut state, &mut budget).unwrap(), input_role);
    assert_eq!(state.phases[&input_role], CLOSED);

    // Previously Tuple accepted alias=None, silently erased projected owners,
    // then supplied a source-free argument to the unknown call.
    let hidden = expr(CoreExprKind::Tuple(vec![expr(CoreExprKind::Tuple(vec![
        expr(CoreExprKind::Name("inputs".into()))]))]));
    let call = expr(CoreExprKind::Call { callee: Box::new(expr(CoreExprKind::Name("opaque".into()))),
        arguments: vec![hidden] });
    let errors = proof.expression(&mut state, &call, &mut budget, 0)
        .err().expect("nested projected owners must not become a source-free opaque call");
    assert!(errors.iter().any(|error| error.message.contains("complete nested owner-place identity")));

    // A projection which intentionally loses the exact selector must instead
    // expose it. The actual call-effect consumer invalidates every saved tag
    // relation and cannot retain Closed for that original owner.
    let projected = expr(CoreExprKind::Index { object: Box::new(expr(CoreExprKind::Name("inputs".into()))),
        index: Box::new(expr(CoreExprKind::Integer("0".into()))) });
    let argument = proof.expression(&mut state, &projected, &mut budget, 0).unwrap();
    state.locals.push(("saved_ready".into(), Data { outcomes:
        vec![(ProtocolExit::PollReady, state.phases.clone())], ..Data::default() }));
    state.call_effect(&[argument], &[], &mut budget).unwrap();
    assert_eq!(state.phases[&input_role], ANY);
    assert!(state.locals.last().unwrap().1.outcomes.is_empty());
}
