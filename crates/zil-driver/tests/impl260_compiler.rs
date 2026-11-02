//! Actual source -> ordinary parser AST -> raw CT CFG. Independent verification
//! and VM execution are exercised once the frozen 259 delivery is composed.
#[path = "../src/ct_artifact/compiler.rs"]
mod compiler;
use compiler::*;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, NodeId, SourceUnit};
use zil_ir::ct_artifact::*;
fn try_build(text: &str, entry: &str) -> zil_base::CompileResult<CtArtifactProgram> {
    try_build_with_limits(text, entry, &Limits::default())
}
fn try_build_with_limits(
    text: &str,
    entry: &str,
    limits: &Limits,
) -> zil_base::CompileResult<CtArtifactProgram> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "260.zil", text);
    let syntax = zil_syntax::parse(&source).into_result()?;
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
    let imports = syntax
        .items
        .iter()
        .filter_map(|item| {
            if let zil_syntax::ast::ItemKind::Import(import) = &item.kind {
                let target = match import
                    .path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["std", "meta"] => ImportTarget::Meta,
                    ["std", "syntax"] => ImportTarget::Syntax,
                    _ => return None,
                };
                Some(SourceImport {
                    alias: import
                        .alias
                        .clone()
                        .unwrap_or_else(|| import.path.last().unwrap().clone()),
                    target,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
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
        limits,
    })
}
fn build(text: &str, entry: &str) -> CtArtifactProgram {
    try_build(text, entry).unwrap()
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
    fn authenticate_artifact(&self, artifact: &CtArtifactProgram) -> bool {
        artifact == self.raw
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
        if usage == zil_verify::CtSymbolUsage::Schema {
            return self.raw.record_schemas.iter().any(|s| &s.name == symbol)
                || self.raw.enum_schemas.iter().any(|s| &s.name == symbol);
        }
        matches!(
            usage,
            zil_verify::CtSymbolUsage::FunctionDefinition
                | zil_verify::CtSymbolUsage::Call
                | zil_verify::CtSymbolUsage::QuoteDefinition
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

#[test]
fn ct_domain_derives_from_actual_types_and_keeps_scalar_bridge_runtime_available() {
    let mut raw = build(
        "fn increment(n:i32)->i32{return n+1} fn choose(n:i32)->Type{let x=increment(n);return i32}",
        "choose",
    );
    let before = raw.budget.max_analysis_work;
    let expected = raw.functions[raw.entry.0 as usize].definition.clone();
    let actual = ct_only_definitions(&mut raw).unwrap();
    assert_eq!(actual, vec![expected]);
    assert!(raw.budget.max_analysis_work < before);
}
#[test]
fn no_invocation_definition_entry_uses_real_source_identity_and_request_remainder() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "unused.zil",
        "fn main()->i32{return 0} fn unused(t:Type)->Type{return t}",
    );
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
    let modules = [SourceModule {
        source: &source,
        syntax: &syntax,
        imports: &[],
        symbols: &symbols,
    }];
    let mut limits = Limits::default();
    let old = limits.max_analysis_steps;
    assert_eq!(
        definitions_entry(&modules, &mut limits).unwrap(),
        Some(symbols[1].1.clone())
    );
    assert!(limits.max_analysis_steps < old);
}

#[test]
fn standard_module_aliases_are_shadowed_by_actual_parameters_and_locals() {
    for source in [
        "import std.meta as meta; fn bad(meta:i32)->Type{return meta.array_type(i32,2)}",
        "import std.syntax as syntax; fn bad(syntax:i32)->Syntax<Expr>{return syntax.literal(1)}",
        "import std.meta as meta; fn bad()->Type{let meta=0;return meta.array_type(i32,2)}",
        "import std.syntax as syntax; fn bad()->Syntax<Expr>{let syntax=0;return syntax.literal(1)}",
    ] {
        let errors = try_build(source, "bad").unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.phase == zil_base::Phase::Ct && e.code == "E-CT-SOURCE"),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("shadows the function/module name")),
            "{errors:?}"
        );
    }
}
#[test]
fn actual_value_binding_shadows_primitive_type_name() {
    assert_eq!(
        run(
            "fn type_id(t:Type)->Type{let i32=t;return i32} fn main()->Type{return type_id(i64)}",
            "main",
            &[]
        ),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I64)],
            root: CtTypeNodeId(0)
        })
    );
}
#[test]
fn ct_record_alias_and_enum_fields_preserve_real_type_values() {
    for source in [
        "struct Holder{pub value:Type} type Alias=Holder; fn main()->Type{let h:Alias=Holder{value:i64};return h.value}",
        "enum Pick{First(Type),Second(Type)} fn main()->Type{let value=Pick.Second(i64);match value{Pick.First(t)=>{return t},Pick.Second(t)=>{return t}}}",
    ] {
        assert_eq!(
            run(source, "main", &[]),
            CtValue::Type(CtTypeRef {
                nodes: vec![CtTypeNode::Primitive(CtScalarTy::I64)],
                root: CtTypeNodeId(0)
            })
        );
    }
}
#[test]
fn indirect_ct_only_unused_definition_is_checked_before_any_vm_execution() {
    let errors=try_build("struct Holder{pub value:Type} type Alias=Holder; fn unused(h:Alias)->Type{return h.value+1} fn main()->i32{return 0}","main").unwrap_err();
    assert!(errors.iter().all(|e| e.phase == zil_base::Phase::Ct));
}
#[test]
fn empty_explicit_ct_array_retains_its_element_type() {
    assert_eq!(
        run(
            "fn main()->Type{let empty:[Type;0]=[];return i32}",
            "main",
            &[]
        ),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0)
        })
    );
}

#[test]
fn direct_source_alias_cycles_fail_before_recursive_attribution() {
    std::thread::Builder::new().stack_size(2 * 1024 * 1024).spawn(|| {
    for source in [
        "type Recursive=Recursive;fn build()->Type{return Recursive}",
        "import std.meta as meta;type Recursive=#(meta.array_type(i32,i32(meta.layout(Recursive).size)));fn build()->Type{return Recursive}",
        "type A=B;type B=A;fn build(value:A)->Type{return i32}",
    ] {
        let errors = try_build(source, "build").unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-CT-PHASE-CYCLE"),
            "{errors:?}"
        );
    }
    let raw = build(
        "type A=i64;type B=A;fn build()->Type{let first=B;return A}",
        "build",
    );
    assert_eq!(raw.functions[raw.entry.0 as usize].result_type, CtTy::Type);
    assert_eq!(build("type Unused=Unused;fn build()->Type{return i32}", "build").functions[0].result_type, CtTy::Type);
    }).unwrap().join().unwrap();
}

#[test]
fn alias_attribution_has_a_request_exact_work_boundary() {
    for source in [
        "fn build()->Type{return i64}",
        "type A=i64;type B=A;fn build()->Type{let first=B;return A}",
    ] {
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            };
            match try_build_with_limits(source, "build", &limits) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == zil_base::ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle + 1;
                }
            }
        }
        let limits = Limits {
            max_analysis_steps: low,
            ..Limits::default()
        };
        let raw = try_build_with_limits(source, "build", &limits).unwrap();
        assert_eq!(
            raw,
            try_build_with_limits(source, "build", &limits).unwrap()
        );
        let errors = try_build_with_limits(
            source,
            "build",
            &Limits {
                max_analysis_steps: low - 1,
                ..limits
            },
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
        );
        println!(
            "source alias control {source}: N={low}; N-1 ResourceLimit; fresh N identical raw"
        );
    }
}

#[test]
fn cross_module_alias_uses_its_definition_imports_and_distinct_identity() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import child as child;type Alias=child.Alias;fn build()->Type{return Alias}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "child.zil",
            "import std.meta as meta;pub type Alias=#(meta.array_type(i32,2))",
        ),
    ];
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let symbols = syntax
        .iter()
        .enumerate()
        .map(|(m, s)| {
            s.items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    (
                        item.span,
                        CtSymbolId {
                            module: ModuleId(m as u32),
                            declaration: NodeId(i as u32),
                            generation_path: None,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let imports = [
        vec![SourceImport {
            alias: "child".into(),
            target: ImportTarget::Module(ModuleId(1)),
        }],
        vec![SourceImport {
            alias: "meta".into(),
            target: ImportTarget::Meta,
        }],
    ];
    let modules = (0..2)
        .map(|i| SourceModule {
            source: &sources[i],
            syntax: &syntax[i],
            imports: &imports[i],
            symbols: &symbols[i],
        })
        .collect::<Vec<_>>();
    let entry = symbols[0][2].1.clone();
    let context = build("fn build()->Type{return i32}", "build");
    let raw = compile(CompileRequest {
        modules: &modules,
        source_fingerprint: Fingerprint::of(
            format!(
                "{:?}{:?}",
                sources[0].fingerprint(),
                sources[1].fingerprint()
            )
            .as_bytes(),
        ),
        entry: Entry::Function(entry),
        input_tokens: &context.input_tokens,
        snapshot: &context.snapshot,
        limits: &Limits::default(),
    })
    .unwrap();
    let authority = Authority { raw: &raw };
    let verified =
        zil_verify::verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
    let mut budget = zil_ct::CtExecutionBudget::new(raw.budget);
    let (value, _) = zil_ct::evaluate_ct_artifact(&verified, &[], &Reader, &mut budget).unwrap();
    let CtValue::Type(ty) = value else {
        panic!("{value:?}")
    };
    assert!(matches!(
        &ty.nodes[ty.root.0 as usize],
        CtTypeNode::Array { length: 2, .. }
    ));
}

#[test]
fn source_alias_cannot_capture_an_unrelated_caller_parameter() {
    let errors = try_build("type A=#(t);fn build(t:Type)->Type{return A}", "build").unwrap_err();
    assert!(errors.iter().any(|e| e.code == "E-CT-SOURCE"), "{errors:?}");
}

#[test]
fn source_alias_owns_its_ct_block_locals_and_restores_caller_scopes() {
    let value = run(
        "type A=#{let t=i64;yield t;};fn build()->Type{let t=i32;let ignored=A;return t}",
        "build",
        &[],
    );
    assert_eq!(
        value,
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0)
        })
    );
    let value = run(
        "type A=#{let t=i64;yield t;};fn build()->Type{return A}",
        "build",
        &[],
    );
    assert_eq!(
        value,
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I64)],
            root: CtTypeNodeId(0)
        })
    );
}
