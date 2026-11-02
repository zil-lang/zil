//! Actual source -> ordinary parser AST -> raw CT CFG. Independent verification
//! and VM execution are exercised once the frozen 259 delivery is composed.
#[path = "../src/ct_artifact/compiler.rs"]
mod compiler;
use compiler::*;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, NodeId, SourceUnit};
use zil_ir::ct_artifact::*;
fn build(text: &str, entry: &str) -> CtArtifactProgram {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "260.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let symbols: Vec<_> = syntax
        .items
        .iter()
        .enumerate()
        .map(|(n, i)| {
            (
                i.span,
                CtSymbolId {
                    module: ModuleId(0),
                    declaration: NodeId(n as u32),
                    generation_path: None,
                },
            )
        })
        .collect();
    let index = syntax
        .items
        .iter()
        .position(|i| matches!(&i.kind,zil_syntax::ast::ItemKind::Function(f) if f.name==entry))
        .unwrap();
    let imports = [
        SourceImport {
            alias: "meta".into(),
            target: ImportTarget::Meta,
        },
        SourceImport {
            alias: "syntax".into(),
            target: ImportTarget::Syntax,
        },
    ];
    let modules = [SourceModule {
        source: &source,
        syntax: &syntax,
        imports: &imports,
        symbols: &symbols,
    }];
    let input = CtInputTable {
        invocation_fingerprint: Fingerprint::of(b"260"),
        tokens: Vec::new(),
    };
    let snapshot = CtSnapshotRequest {
        fingerprint: Fingerprint::of(b"260-snapshot"),
        ready: CtReadyStage::Config,
        target_fingerprint: None,
        requested_queries: Vec::new(),
    };
    compile(CompileRequest {
        modules: &modules,
        source_fingerprint: source.fingerprint(),
        entry: Entry::Function(symbols[index].1.clone()),
        input_tokens: &input,
        snapshot: &snapshot,
        limits: &Limits::default(),
    })
    .unwrap()
}
#[test]
fn ordinary_type_function_uses_real_control_flow_and_scalar_calls() {
    let program = build(
        "fn add(a:i32,b:i32)->i32{return a+b} fn choose(t:Type,n:i32)->Type{var result=t;var i=0;while i<n{if i==2{i+=1;continue};result=t;i=add(i,1)}for x in 0..2{result=t}return result}",
        "choose",
    );
    assert_eq!(
        program.functions[program.entry.0 as usize].result_type,
        CtTy::Type
    );
    assert!(
        program
            .functions
            .iter()
            .any(|f| f.parameter_types == vec![CtTy::Scalar(CtScalarTy::I32); 2])
    );
}

struct Authority<'a> {
    raw: &'a CtArtifactProgram,
}
impl zil_verify::CtArtifactAuthority for Authority<'_> {
    fn source_fingerprint(&self) -> Fingerprint {
        self.raw.source_fingerprint
    }
    fn snapshot(&self) -> &CtSnapshotRequest {
        &self.raw.snapshot
    }
    fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
        input == &self.raw.input_tokens
    }
    fn authenticate_function(&self, function: &CtFunction) -> bool {
        self.raw.functions.iter().any(|f| f == function)
    }
    fn authorize_symbol(
        &self,
        _: &CtSymbolId,
        symbol: &CtSymbolId,
        usage: zil_verify::CtSymbolUsage,
    ) -> bool {
        matches!(
            usage,
            zil_verify::CtSymbolUsage::FunctionDefinition | zil_verify::CtSymbolUsage::Call
        ) && self.raw.functions.iter().any(|f| &f.definition == symbol)
    }
    fn authorize_query(&self, _: &CtSymbolId, _: CtQueryKind) -> bool {
        false
    }
}
struct Reader;
fn unused<T>() -> zil_base::CompileResult<T> {
    Err(vec![zil_base::Diagnostic::error(
        zil_base::Phase::Ct,
        "E-TEST-READER",
        None,
        "closed CT source fixture does not use a host service",
    )])
}
impl zil_ct::CtReadView for Reader {
    fn authenticate_argument(
        &self,
        _: u32,
        ty: &CtTy,
        value: &CtValue,
    ) -> zil_base::CompileResult<()> {
        if matches!(
            (ty, value),
            (
                CtTy::Scalar(CtScalarTy::I32),
                CtValue::Scalar(zil_ir::Constant::I32(_))
            ) | (
                CtTy::Scalar(CtScalarTy::I64),
                CtValue::Scalar(zil_ir::Constant::I64(_))
            ) | (
                CtTy::Scalar(CtScalarTy::Bool),
                CtValue::Scalar(zil_ir::Constant::Bool(_))
            )
        ) {
            Ok(())
        } else {
            unused()
        }
    }
    fn type_name(
        &self,
        _: &CtSymbolId,
        _: &CtSymbolId,
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtTypeRef> {
        unused()
    }
    fn type_apply(
        &self,
        _: &CtSymbolId,
        _: &CtSymbolId,
        _: &[CtValue],
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtTypeRef> {
        unused()
    }
    fn definition_ident(
        &self,
        _: &CtSymbolId,
        _: &CtSymbolId,
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtIdent> {
        unused()
    }
    fn parse(
        &self,
        _: CtSyntaxCategory,
        _: &CtTokenTree,
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtSyntaxTree> {
        unused()
    }
    fn parse_ident(
        &self,
        _: &CtTokenTree,
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtIdent> {
        unused()
    }
    fn query(
        &self,
        _: &CtSymbolId,
        _: CtQueryKind,
        _: &[CtValue],
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<(CtValue, CtQueryDependency)> {
        unused()
    }
    fn build_syntax(
        &self,
        _: CtSyntaxBuilder,
        _: &[CtValue],
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtSyntaxTree> {
        unused()
    }
    fn instantiate_quote(
        &self,
        _: &CtTemplate,
        _: &[CtValue],
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtSyntaxTree> {
        unused()
    }
    fn token_inspect(
        &self,
        _: &CtValue,
        _: CtTokenProjection,
        _: Option<i64>,
        _: &mut zil_ct::CtExecutionBudget,
    ) -> zil_base::CompileResult<CtValue> {
        unused()
    }
}
fn run(text: &str, entry: &str, args: &[CtValue]) -> CtValue {
    let raw = build(text, entry);
    let authority = Authority { raw: &raw };
    let verified =
        zil_verify::verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
    let mut budget = zil_ct::CtExecutionBudget::new(raw.budget);
    let (value, _usage) =
        zil_ct::evaluate_ct_artifact(&verified, args, &Reader, &mut budget).unwrap();
    value
}
#[test]
fn source_type_loop_and_scalar_bridge_execute_only_in_verified_vm() {
    let source = "fn add(a:i32,b:i32)->i32{return a+b} fn choose(n:i32)->Type{var result=i32;var i=0;while i<n{if i==2{i+=1;continue};result=i64;i=add(i,1)}for x in 0..2{result=i64}return result}";
    assert_eq!(
        run(
            source,
            "choose",
            &[CtValue::Scalar(zil_ir::Constant::I32(4))]
        ),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I64)],
            root: CtTypeNodeId(0)
        })
    );
}
#[test]
fn both_returning_branches_are_closed_without_unreachable_ir() {
    let source = "fn choose(flag:bool)->Type{if flag{return i32}else{return i64}}";
    for (flag, expected) in [(true, CtScalarTy::I32), (false, CtScalarTy::I64)] {
        assert_eq!(
            run(
                source,
                "choose",
                &[CtValue::Scalar(zil_ir::Constant::Bool(flag))]
            ),
            CtValue::Type(CtTypeRef {
                nodes: vec![CtTypeNode::Primitive(expected)],
                root: CtTypeNodeId(0)
            })
        );
    }
}
