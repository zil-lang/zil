//! Real-source controls for independent source authentication, including
//! coordinated mutations of raw metadata and the private producer ledger.
use super::*;
use crate::ct_artifact::compiler::{self, CompileRequest, Entry};
use std::collections::BTreeMap;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, NodeId, SourceUnit};

struct Fixture {
    context: FrozenInvocationContext,
    raw: CtArtifactProgram,
    facts: SourceCompilation,
    index: SourceIndex,
    budget: CtExecutionBudget,
}
fn fixture(text: &str, entry: u32) -> Fixture {
    let sources = vec![SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "independent-source.zil",
        text,
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
        declaration: NodeId(entry),
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
        panic!("source fixture body");
    };
    let s::StmtKind::Binding { value, .. } = &block.statements[0].kind else {
        panic!("source fixture binding");
    };
    let s::ExprKind::MetaEval(expression) = &value.kind else {
        panic!("source fixture CT expression");
    };
    let input = CtInputTable {
        invocation_fingerprint: Fingerprint::of(b"source-auth-test"),
        tokens: Vec::new(),
    };
    let snapshot = CtSnapshotRequest {
        fingerprint: sources[0].fingerprint(),
        ready: CtReadyStage::Tokens,
        target_fingerprint: None,
        requested_queries: Vec::new(),
    };
    let modules = index.modules(&sources, &syntax);
    let (raw, facts) = compiler::compile_scoped(CompileRequest {
        target_pointer_width: 64,
        modules: &modules,
        source_fingerprint: sources[0].fingerprint(),
        entry: Entry::Expression {
            module: ModuleId(0),
            expression,
            definition: definition.clone(),
            lexical_owner: Some(definition.clone()),
            expected: None,
            parameters: &[],
        },
        input_tokens: &input,
        snapshot: &snapshot,
        limits: &limits,
    })
    .unwrap();
    let mut budget = CtExecutionBudget::new(CtDeclaredBudget {
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth as u32,
        max_live_cells: limits.max_values as u64,
        max_temporary_bytes: limits.max_source_bytes as u64,
        max_generated_nodes: limits.max_nodes as u64,
        max_instances: limits.max_functions as u64,
        max_analysis_work: limits.max_analysis_steps as u64,
    });
    let context = FrozenInvocationContext::freeze(
        &Entry::Function(definition),
        &parsed,
        &index,
        Some(&origins),
        &mut budget,
    )
    .unwrap()
    .unwrap();
    let context = match Arc::try_unwrap(context) {
        Ok(context) => context,
        Err(_) => panic!("fixture context should be unique"),
    };
    Fixture {
        context,
        raw,
        facts,
        index,
        budget,
    }
}
#[test]
fn source_schema_rejects_coordinated_life_and_witness_mutation() {
    let mut f = fixture(
        "struct Phantom<life a>{n:i32} fn query<T:Copy,life a,life b>(p:&'a i32)->i32{let value:Phantom<'a>=#(Phantom::<'a>{n:42});return value.n}",
        1,
    );
    f.context
        .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
        .unwrap();
    let graph = f.raw.record_schemas[0].life_context.as_mut().unwrap();
    assert_eq!(graph.ambient_classes[0].source.parameter, 1);
    graph.ambient_classes[0].source.parameter = 2;
    f.facts.bindings[0].parameters[0] = 2;
    let error = f
        .context
        .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
        .unwrap_err();
    assert!(
        error[0].message.contains("original complete ordinal"),
        "{error:?}"
    );
}
#[test]
fn source_calls_reject_coordinated_full_actual_mutations() {
    let source = "fn factory<T:Copy,const N:i32,life a>()->i32{return N} fn query<T:Copy,life a,life b>(p:&'a i32)->i32{let value=#(factory::<i32,42,'a>());return value}";
    for attack in 0..3 {
        let mut f = fixture(source, 1);
        f.context
            .extend_calls(
                &f.facts.calls,
                &f.facts.scopes,
                &f.raw,
                &f.index,
                &mut f.budget,
            )
            .unwrap();
        match attack {
            0 => {
                let CtScopeActual::Type(graph) = &mut f.facts.calls[0].actuals[0] else {
                    unreachable!()
                };
                graph.nodes[0] = CtTypeNode::Primitive(CtScalarTy::U64);
            }
            1 => f.facts.calls[0].actuals[1] = CtScopeActual::Scalar(zil_ir::Constant::I32(43)),
            _ => {
                let CtScopeActual::Lifetime(reference) = &mut f.facts.calls[0].actuals[2] else {
                    unreachable!()
                };
                reference.parameter = 2;
            }
        }
        assert!(
            f.context
                .extend_calls(
                    &f.facts.calls,
                    &f.facts.scopes,
                    &f.raw,
                    &f.index,
                    &mut f.budget
                )
                .is_err(),
            "source actual mutation {attack}"
        );
    }
}
#[test]
fn source_type_instruction_requires_the_original_type_site() {
    let mut f = fixture(
        "struct Phantom<life a>{n:i32} fn query<T:Copy,life a,life b>(p:&'a i32)->i32{let value:Type=#(Phantom::<'a>);return 0}",
        1,
    );
    f.context
        .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
        .unwrap();
    assert_eq!(f.facts.type_sites.len(), 1);
    f.facts.type_sites[0].site.ordinal = u32::MAX;
    assert!(
        f.context
            .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
            .is_err()
    );
}
