//! Original-call controls for runtime Type/Const specialization. The life
//! binder remains the callee's original definition binder at its full ordinal.
use super::*;
use crate::ct_artifact::compiler::{
    self, CompileRequest,
    source_sites::{self, Selection, SourceSite},
};
use std::collections::BTreeMap;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, NodeId, SourceUnit};

struct Fixture {
    context: FrozenInvocationContext,
    raw: CtArtifactProgram,
    source: RuntimeEntrySource,
    rows: Vec<compiler::SourceRuntimeFrame>,
    index: SourceIndex,
    budget: CtExecutionBudget,
}
fn fixture() -> Fixture {
    let sources = vec![SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "runtime-source.zil",
        "fn factory<T:Copy,#N:i32,life a>(p:&'a T)->i32{let value=#(N);return value} fn main()->i32{let local:i32=1;let value=factory::<i32,7>(&local);return value}",
    )];
    let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
    assert!(
        parsed[0].diagnostics.is_empty(),
        "{:?}",
        parsed[0].diagnostics
    );
    let syntax = vec![&parsed[0].module];
    let paths = BTreeMap::new();
    let limits = Limits::default();
    let index = SourceIndex::freeze(&sources, &syntax, &paths).unwrap();
    let (origins, _) = zil_expand::source_context::capture_original(
        &sources,
        &syntax,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &paths,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
        &limits,
    )
    .unwrap();
    let origins = Arc::new(origins.unwrap());
    let definition = CtSymbolId {
        module: ModuleId(0),
        declaration: NodeId(0),
        generation_path: None,
    };
    let caller = CtSymbolId {
        module: ModuleId(0),
        declaration: NodeId(1),
        generation_path: None,
    };
    let original = compiler::source_scope::original_function(
        syntax[0],
        definition.declaration,
        limits.max_call_depth,
        |_| Ok(()),
    )
    .unwrap()
    .unwrap();
    let s::FunctionBody::Block(block) = &original.function.body else {
        panic!("runtime template body")
    };
    let s::StmtKind::Binding { value, .. } = &block.statements[0].kind else {
        panic!("runtime query binding")
    };
    let s::ExprKind::MetaEval(expression) = &value.kind else {
        panic!("runtime query")
    };
    let original_caller = compiler::source_scope::original_function(
        syntax[0],
        caller.declaration,
        limits.max_call_depth,
        |_| Ok(()),
    )
    .unwrap()
    .unwrap();
    let s::FunctionBody::Block(block) = &original_caller.function.body else {
        panic!("runtime caller body")
    };
    let s::StmtKind::Binding { value: call, .. } = &block.statements[1].kind else {
        panic!("runtime call binding")
    };
    let s::ExprKind::Call { callee, .. } = &call.kind else {
        panic!("runtime call")
    };
    let s::ExprKind::TypeApply { arguments, .. } = &callee.kind else {
        panic!("runtime call actuals")
    };
    let [s::GenericArg::Type(ty), s::GenericArg::Const(value)] = arguments.as_slice() else {
        panic!("runtime Type/Const source arguments")
    };
    let site = |expression: &s::Expr| -> SourceSite {
        source_sites::select(
            syntax[0],
            Selection::Expr(expression as *const s::Expr as usize),
            limits.max_call_depth,
            |_| Ok(()),
        )
        .unwrap()
        .unwrap()
        .site
    };
    let source = RuntimeEntrySource {
        parents: Vec::new(),
        parents: Vec::new(),
        definition: definition.clone(),
        caller,
        call_site: site(call),
        expression_site: site(expression),
        arguments: vec![
            RuntimeArgument::Type(ty.clone()),
            RuntimeArgument::Const(value.clone()),
            RuntimeArgument::LifeDefinition { ordinal: 2 },
        ],
    };
    let entry = || Entry::Expression {
        module: ModuleId(0),
        expression,
        definition: definition.clone(),
        lexical_owner: Some(definition.clone()),
        expected: None,
        parameters: &[],
    };
    let input = CtInputTable {
        invocation_fingerprint: Fingerprint::of(b"runtime-source-auth-test"),
        tokens: Vec::new(),
    };
    let snapshot = CtSnapshotRequest {
        fingerprint: sources[0].fingerprint(),
        ready: CtReadyStage::Tokens,
        target_fingerprint: None,
        requested_queries: Vec::new(),
    };
    let modules = index.modules(&sources, &syntax);
    let (raw, facts) = compiler::compile_scoped_runtime_at_source(
        CompileRequest {
            target_pointer_width: 64,
            modules: &modules,
            source_fingerprint: sources[0].fingerprint(),
            entry: entry(),
            input_tokens: &input,
            snapshot: &snapshot,
            limits: &limits,
        },
        None,
        &source,
    )
    .unwrap();
    let mut budget = CtExecutionBudget::new(raw.budget);
    let context = FrozenInvocationContext::freeze_runtime_at_source(
        &entry(),
        &parsed,
        &index,
        Some(&origins),
        None,
        &source,
        &mut budget,
    )
    .unwrap()
    .unwrap();
    let context = match Arc::try_unwrap(context) {
        Ok(context) => context,
        Err(_) => panic!("unique runtime context"),
    };
    Fixture {
        context,
        raw,
        source,
        rows: facts.runtime_frames,
        index,
        budget,
    }
}
#[test]
fn runtime_original_call_authenticates_complete_type_const_and_definition_life() {
    let mut f = fixture();
    f.context
        .extend_runtime_entry(&f.source, &f.rows, &f.raw, &f.index, &mut f.budget)
        .unwrap();
    assert_eq!(f.raw.ambient_life_scopes.len(), 2);
    assert_eq!(f.context.authenticated_runtime_scope_root_count(), 2);
    assert!(
        f.raw
            .ambient_life_scopes
            .iter()
            .enumerate()
            .all(|(index, scope)| f.context.authorized_runtime_scope_root(index, scope))
    );
}
#[test]
fn runtime_original_call_rejects_coordinated_actual_kind_and_ordinal_substitutions() {
    for attack in 0..4 {
        let mut f = fixture();
        let instance = f
            .raw
            .ambient_life_scopes
            .iter_mut()
            .find_map(|scope| match &mut scope.instance {
                CtScopeInstance::Instantiated(actuals) => Some(actuals),
                _ => None,
            })
            .unwrap();
        match attack {
            0 => {
                let CtScopeActual::Type(graph) = &mut instance[0] else {
                    unreachable!()
                };
                graph.nodes[0] = CtTypeNode::Primitive(CtScalarTy::Bool);
            }
            1 => instance[1] = CtScopeActual::Scalar(zil_ir::Constant::I32(8)),
            2 => {
                let CtScopeActual::Lifetime(reference) = &mut instance[2] else {
                    unreachable!()
                };
                reference.parameter = 0;
            }
            _ => f.source.arguments.pop().map(|_| ()).unwrap(),
        }
        assert!(
            f.context
                .extend_runtime_entry(&f.source, &f.rows, &f.raw, &f.index, &mut f.budget)
                .is_err(),
            "runtime original-call substitution {attack}"
        );
    }
}

#[test]
fn runtime_source_authority_rejects_unrelated_and_reordered_scope_roots_after_wire() {
    let mut f = fixture();
    f.context
        .extend_runtime_entry(&f.source, &f.rows, &f.raw, &f.index, &mut f.budget)
        .unwrap();
    let limits = Limits::default();
    let verify = |raw: &CtArtifactProgram| {
        let transported = zil_ir::ct_wire::decode_program(
            &zil_ir::ct_wire::encode_program(raw, &limits).unwrap(),
            &limits,
        )
        .unwrap();
        let authority = crate::ct_artifact::authority::FrozenAuthority {
            source_fingerprint: raw.source_fingerprint,
            snapshot: &raw.snapshot,
            input: &raw.input_tokens,
            index: &f.index,
            expected_functions: &raw.functions,
            // Deliberately let whole-producer equality agree with the mutation;
            // independently frozen scope attribution must still reject it.
            expected_artifact: raw,
            invocation_context: Some(&f.context),
        };
        zil_verify::verify_ct_artifact(transported, &limits, &authority)
    };
    verify(&f.raw).unwrap();
    for attack in 0..4 {
        let mut raw = f.raw.clone();
        match attack {
            0 => {
                let mut row = raw.ambient_life_scopes[0].clone();
                row.declaration = f.source.caller.clone();
                assert!(
                    !f.context
                        .authorized_runtime_scope_root(raw.ambient_life_scopes.len(), &row)
                );
                raw.ambient_life_scopes.push(row);
            }
            1 => raw.ambient_life_scopes.swap(0, 1),
            2 => {
                let mut row = raw.ambient_life_scopes[1].clone();
                row.binder_site.push(0);
                assert!(
                    !f.context
                        .authorized_runtime_scope_root(raw.ambient_life_scopes.len(), &row)
                );
                raw.ambient_life_scopes.push(row);
            }
            _ => raw
                .ambient_life_scopes
                .push(raw.ambient_life_scopes[0].clone()),
        }
        assert!(
            verify(&raw).is_err(),
            "untrusted runtime root mutation {attack}"
        );
    }
}
