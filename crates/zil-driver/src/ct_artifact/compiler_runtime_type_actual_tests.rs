//! Type actuals carry source-authenticated metadata, without inventing a CT
//! value-schema caller for a native parent that is absent from the CT CFG.
use super::*;
use crate::ct_artifact::{invocation_context::FrozenInvocationContext, source_index::SourceIndex};
use std::collections::BTreeMap;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, NodeId, SourceUnit};
use zil_expand::runtime_ct::RuntimeArgument;

struct Fixture {
    context: FrozenInvocationContext,
    raw: CtArtifactProgram,
    facts: SourceCompilation,
    source: RuntimeEntrySource,
    index: SourceIndex,
    budget: zil_ct::CtExecutionBudget,
}
fn fixture(field: &str) -> Result<Fixture, Vec<Diagnostic>> {
    let text = format!(
        "struct Phantom<life s,T:Copy,#N:i32>{{n:{field}}}\n\
         fn leaf<U:Copy>()->i32{{let value=#(7+1);return value}}\n\
         fn outer<T:Copy,#N:i32,life a>(p:&'a i32)->i32{{let value=leaf::<Phantom<'a,T,N>>();return value}}\n\
         fn main()->i32{{let local:i32=0;let value=outer::<bool,7>(&local);return value}}"
    );
    let sources = vec![SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "runtime-type.zil",
        &text,
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
    let (origins, _) = zil_expand::source_context::capture_original_required(
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
    let origins = Arc::new(origins);
    let symbol = |ordinal| CtSymbolId {
        module: ModuleId(0),
        declaration: NodeId(ordinal),
        generation_path: None,
    };
    let function = |ordinal| {
        source_scope::original_function(syntax[0], NodeId(ordinal), limits.max_call_depth, |_| {
            Ok(())
        })
        .unwrap()
        .unwrap()
        .function
    };
    fn binding(function: &s::Function, ordinal: usize) -> &s::Expr {
        let s::FunctionBody::Block(block) = &function.body else {
            panic!("function block")
        };
        let s::StmtKind::Binding { value, .. } = &block.statements[ordinal].kind else {
            panic!("source binding")
        };
        value
    }
    let s::ExprKind::MetaEval(expression) = &binding(function(1), 0).kind else {
        panic!("source query")
    };
    let parent_call = binding(function(3), 1);
    let child_call = binding(function(2), 0);
    fn explicit(call: &s::Expr) -> &[s::GenericArg] {
        let s::ExprKind::Call { callee, .. } = &call.kind else {
            panic!("source call")
        };
        let s::ExprKind::TypeApply { arguments, .. } = &callee.kind else {
            panic!("source TypeApply")
        };
        arguments.as_slice()
    }
    let [
        s::GenericArg::Type(parent_type),
        s::GenericArg::Const(parent_const),
    ] = explicit(parent_call)
    else {
        panic!("parent vector")
    };
    let [s::GenericArg::Type(child_type)] = explicit(child_call) else {
        panic!("child vector")
    };
    let site = |node: &s::Expr| {
        source_sites::select(
            syntax[0],
            source_sites::Selection::Expr(node as *const s::Expr as usize),
            limits.max_call_depth,
            |_| Ok(()),
        )
        .unwrap()
        .unwrap()
        .site
    };
    let source = RuntimeEntrySource {
        parents: vec![RuntimeEntryFrame {
            definition: symbol(2),
            caller: symbol(3),
            call_site: site(parent_call),
            arguments: vec![
                RuntimeArgument::Type(parent_type.clone()),
                RuntimeArgument::Const(parent_const.clone()),
                RuntimeArgument::LifeDefinition { ordinal: 2 },
            ],
        }],
        definition: symbol(1),
        caller: symbol(2),
        call_site: site(child_call),
        expression_site: site(expression),
        arguments: vec![RuntimeArgument::Type(child_type.clone())],
    };
    let entry = || Entry::Expression {
        module: ModuleId(0),
        expression,
        definition: symbol(1),
        lexical_owner: Some(symbol(1)),
        expected: None,
        parameters: &[],
    };
    let input = CtInputTable {
        invocation_fingerprint: Fingerprint::of(b"runtime-context-Type-actual"),
        tokens: Vec::new(),
    };
    let snapshot = CtSnapshotRequest {
        fingerprint: sources[0].fingerprint(),
        ready: CtReadyStage::Tokens,
        target_fingerprint: None,
        requested_queries: Vec::new(),
    };
    let modules = index.modules(&sources, &syntax);
    let (raw, facts) = compile_scoped_runtime_at_source(
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
    )?;
    let mut budget = zil_ct::CtExecutionBudget::new(raw.budget);
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
    let context = Arc::try_unwrap(context)
        .ok()
        .expect("unique frozen context");
    Ok(Fixture {
        context,
        raw,
        facts,
        source,
        index,
        budget,
    })
}

#[test]
fn runtime_qualified_type_actual_has_no_forged_value_schema_caller() {
    let mut f = fixture("i32").unwrap_or_else(|errors| panic!("{errors:?}"));
    assert!(f.raw.records.is_empty());
    assert!(f.raw.enums.is_empty());
    assert!(f.facts.bindings.is_empty());
    assert_eq!(f.facts.runtime_frames.len(), 2);
    f.context
        .extend_runtime_entry(
            &f.source,
            &f.facts.runtime_frames,
            &f.raw,
            &f.index,
            &mut f.budget,
        )
        .unwrap();
}

#[test]
fn runtime_qualified_type_actual_still_checks_the_real_nominal_fields() {
    assert!(
        fixture("&'s T").is_err(),
        "a metadata graph cannot grant Copy to an address field"
    );
}

#[test]
fn runtime_qualified_type_actual_rejects_coordinated_full_argument_changes() {
    for life in [false, true] {
        let mut f = fixture("i32").unwrap_or_else(|errors| panic!("{errors:?}"));
        let instance = f.facts.runtime_frames.last().unwrap().instance;
        let CtScopeInstance::Instantiated(actuals) =
            &mut f.raw.ambient_life_scopes[instance.0 as usize].instance
        else {
            panic!("actual instance")
        };
        let [CtScopeActual::Type(graph)] = actuals.as_mut_slice() else {
            panic!("Type actual")
        };
        if life {
            graph.ambient_classes[0].source.parameter = 0;
        } else {
            let nominal = graph
                .nodes
                .iter_mut()
                .find_map(|node| match node {
                    CtTypeNode::Nominal { arguments, .. } => Some(arguments),
                    _ => None,
                })
                .unwrap();
            let CtTypeArgument::Scalar(value) = &mut nominal[2] else {
                panic!("const actual")
            };
            *value = Constant::I32(8);
        }
        assert!(
            f.context
                .extend_runtime_entry(
                    &f.source,
                    &f.facts.runtime_frames,
                    &f.raw,
                    &f.index,
                    &mut f.budget
                )
                .is_err()
        );
    }
}
