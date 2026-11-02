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
