//! Independent synthetic raw/Wire controls. The inventory proxy is deliberately
//! permissive and is NOT evidence of Source invocation authority.
use zil_base::{Fingerprint, Limits, ModuleId, NodeId, Span};
use zil_ir::{Constant, NominalLifeArgument, LifeDeclarationId, StorageBinder, StorageLifeShape};
use zil_ir::ct_artifact::*;
use zil_verify::{CtArtifactAuthority, CtScopeParameterKind, CtSymbolUsage, CtVerificationMeter};

struct InventoryProxy { snapshot: CtSnapshotRequest, count: usize }
impl CtArtifactAuthority for InventoryProxy {
    fn active_scope_count(&self) -> Option<usize> { Some(self.count) }
    fn scope_parameter_count(&self, _: &CtAmbientLifeScope) -> Option<usize> { Some(3) }
    fn scope_parameter_kind(&self, _: &CtAmbientLifeScope, n: usize) -> Option<CtScopeParameterKind> {
        [CtScopeParameterKind::Type, CtScopeParameterKind::Scalar(CtScalarTy::I32), CtScopeParameterKind::Lifetime].get(n).copied()
    }
    fn source_fingerprint(&self) -> Fingerprint { Fingerprint::ZERO }
    fn snapshot(&self) -> &CtSnapshotRequest { &self.snapshot }
    fn authenticate_input_table(&self, _: &CtInputTable) -> bool { true }
    fn authenticate_artifact(&self, _: &CtArtifactProgram) -> bool { true }
    fn authenticate_function(&self, _: &CtFunction) -> bool { true }
    fn authorize_symbol(&self, _: &CtSymbolId, _: &CtSymbolId, _: CtSymbolUsage) -> bool { true }
    fn authorize_query(&self, _: &CtSymbolId, _: CtQueryKind) -> bool { true }
}
fn symbol(n: u32) -> CtSymbolId { CtSymbolId { module: ModuleId(0), declaration: NodeId(n), generation_path: None } }
fn life(scope: u32) -> CtAmbientLifeReference { CtAmbientLifeReference { scope: CtAmbientScopeId(scope), parameter: 2 } }
fn scalar_graph() -> CtTypeRef { CtTypeRef { nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)], root: CtTypeNodeId(0), ambient_classes: vec![] } }
fn schema(scope: u32) -> CtRecordSchema {
    let argument = CtLifetimeArgument { owner: CtTypeNodeId(1), binder: StorageBinder::Fixed(0) };
    CtRecordSchema {
        name: symbol(3),
        life_context: Some(CtTypeRef {
            nodes: vec![
                CtTypeNode::Nominal { symbol: symbol(3), arguments: vec![CtTypeArgument::Lifetime(argument)] },
                CtTypeNode::BorrowedStorage { storage: CtTypeNodeId(0), life: StorageLifeShape {
                    binder_count: 1, slots: vec![], retained: vec![], nominal_arguments: vec![NominalLifeArgument {
                        path: vec![], declaration: LifeDeclarationId { module: ModuleId(0), declaration: NodeId(3), compiler: false }, argument: 0, binder: 0,
                    }],
                } },
            ],
            root: CtTypeNodeId(1),
            ambient_classes: vec![CtAmbientClassBinding { owner: CtTypeNodeId(1), class: 0, source: life(scope) }],
        }),
        arguments: vec![CtSchemaArgument::Lifetime(argument)], fields: vec![], origin: CtSchemaOrigin::UserPure,
    }
}
fn dag(depth: u32) -> CtArtifactProgram {
    let snapshot = CtSnapshotRequest { fingerprint: Fingerprint::ZERO, ready: CtReadyStage::Tokens, target_fingerprint: None, requested_queries: vec![] };
    let mut scopes = vec![CtAmbientLifeScope { declaration: symbol(1), binder_site: vec![], instance: CtScopeInstance::Definition }];
    // A2 -> A1 -> Definition is first visited completely. B4 -> B3 -> A2
    // then reuses A2: the complete second path has four dependency edges.
    for n in 1..=4 {
        scopes.push(CtAmbientLifeScope { declaration: symbol(2), binder_site: vec![], instance: CtScopeInstance::Instantiated(vec![
            CtScopeActual::Type(scalar_graph()), CtScopeActual::Scalar(Constant::I32(n as i32)), CtScopeActual::Lifetime(life(n - 1)),
        ]) });
    }
    CtArtifactProgram {
        schema_version: CT_ARTIFACT_SCHEMA_VERSION, target_pointer_width: 64, source_fingerprint: Fingerprint::ZERO,
        ambient_life_scopes: scopes,
        functions: vec![CtFunction {
            id: CtFunctionId(0), definition: symbol(1), parameters: vec![], parameter_types: vec![], result_type: CtTy::Scalar(CtScalarTy::I32), locals: vec![], values: vec![CtTy::Scalar(CtScalarTy::I32)],
            blocks: vec![CtBlock { id: CtBlockId(0), instructions: vec![CtInstruction { result: Some(CtValueId(0)), kind: CtInstructionKind::Constant(CtConstantId(0)), span: Span::default() }], terminator: CtTerminator::Return(Some(CtValueId(0))) }], entry: CtBlockId(0), span: Span::default(),
        }],
        entry: CtFunctionId(0), constants: vec![CtConstant::Scalar(Constant::I32(42))], record_schemas: vec![schema(2), schema(4)], enum_schemas: vec![], templates: vec![],
        input_tokens: CtInputTable { invocation_fingerprint: Fingerprint::ZERO, tokens: vec![] }, snapshot,
        budget: CtDeclaredBudget { max_steps: 100, max_call_depth: depth, max_live_cells: 100, max_temporary_bytes: 1_000_000, max_generated_nodes: 100, max_instances: 10, max_analysis_work: 1_000_000 },
    }
}
fn candidates(raw: &CtArtifactProgram, limits: &Limits) -> [CtArtifactProgram; 2] {
    let bytes = zil_ir::ct_wire::encode_program(raw, limits).unwrap();
    [raw.clone(), zil_ir::ct_wire::decode_program(&bytes, limits).unwrap()]
}
fn check(raw: CtArtifactProgram, limits: &Limits) -> zil_base::CompileResult<zil_verify::VerifiedCTArtifactProgram> {
    let authority = InventoryProxy { snapshot: raw.snapshot.clone(), count: raw.ambient_life_scopes.len() };
    zil_verify::verify_ct_artifact(raw, limits, &authority)
}
fn dag_with_edge(depth: u32, type_edge: bool) -> CtArtifactProgram {
    let mut raw = dag(depth);
    if type_edge {
        for n in 1..=4 {
            let CtScopeInstance::Instantiated(actuals) = &mut raw.ambient_life_scopes[n].instance else { unreachable!() };
            actuals[0] = CtScopeActual::Type(schema((n - 1) as u32).life_context.unwrap());
            actuals[2] = CtScopeActual::Lifetime(life(0));
        }
    }
    raw
}
#[test]
fn review_g442_shared_dag_depth_is_the_complete_path_raw_wire() {
    let limits = Limits::default();
    for type_edge in [false, true] {
        for raw in candidates(&dag_with_edge(4, type_edge), &limits) { check(raw, &limits).unwrap(); }
        for raw in candidates(&dag_with_edge(2, type_edge), &limits) {
            let errors = check(raw, &limits).unwrap_err();
            assert!(errors.iter().any(|d| d.code == "E-RESOURCE-LIMIT" && d.message.contains("scope dependency depth")), "type_edge={type_edge}: {errors:?}");
        }
    }
}
#[test]
fn review_g442_cycle_unused_wrong_ordinal_and_type_depth_raw_wire() {
    let limits = Limits::default();
    for attack in 0..5 {
        let mut raw = dag(8);
        let wanted = match attack {
            0 => {
                let CtScopeInstance::Instantiated(a) = &mut raw.ambient_life_scopes[1].instance else { unreachable!() };
                a[2] = CtScopeActual::Lifetime(life(2)); "dependency cycle"
            }
            1 => { raw.ambient_life_scopes.push(CtAmbientLifeScope { declaration: symbol(9), binder_site: vec![], instance: CtScopeInstance::Definition }); "unused rows" }
            2 => {
                let CtScopeInstance::Instantiated(a) = &mut raw.ambient_life_scopes[1].instance else { unreachable!() };
                a[2] = CtScopeActual::Lifetime(CtAmbientLifeReference { scope: CtAmbientScopeId(0), parameter: 0 }); "actual lifetime ordinal"
            }
            3 => {
                let CtScopeInstance::Instantiated(a) = &mut raw.ambient_life_scopes[1].instance else { unreachable!() };
                a[0] = CtScopeActual::Type(CtTypeRef { nodes: vec![CtTypeNode::Array { element: CtTypeNodeId(0), length: 1 }], root: CtTypeNodeId(0), ambient_classes: vec![] }); "cycle or forward edge"
            }
            _ => {
                let CtScopeInstance::Instantiated(a) = &mut raw.ambient_life_scopes[1].instance else { unreachable!() };
                let mut graph = scalar_graph();
                for id in 0..10 { graph.nodes.push(CtTypeNode::Array { element: CtTypeNodeId(id), length: 1 }); }
                graph.root = CtTypeNodeId(10); a[0] = CtScopeActual::Type(graph); "metadata depth"
            }
        };
        for candidate in candidates(&raw, &limits) {
            let errors = check(candidate, &limits).unwrap_err();
            assert!(errors.iter().any(|d| d.message.contains(wanted)), "attack={attack}: {errors:?}");
        }
    }
}
#[test]
fn review_g442_independent_current_verifier_work_n_and_n_minus_one_raw_wire() {
    let limits = Limits::default();
    let raw = dag(4);
    let authority = InventoryProxy { snapshot: raw.snapshot.clone(), count: raw.ambient_life_scopes.len() };
    let mut measured = CtVerificationMeter::new(raw.budget.max_analysis_work);
    zil_verify::verify_ct_artifact_metered(raw.clone(), &limits, &authority, &mut measured).unwrap();
    let n = measured.spent();
    for candidate in candidates(&raw, &limits) {
        for (limit, good) in [(n, true), (n - 1, false), (n, true)] {
            let mut meter = CtVerificationMeter::new(limit);
            let result = zil_verify::verify_ct_artifact_metered(candidate.clone(), &limits, &authority, &mut meter);
            assert_eq!(result.is_ok(), good, "limit={limit}, N={n}: {result:?}");
            if !good { assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT"); }
        }
    }
    eprintln!("review G4.4.2 synthetic scope DAG raw/Wire exact verifier N={n}/N-1; proxy is not Source authority");
}
