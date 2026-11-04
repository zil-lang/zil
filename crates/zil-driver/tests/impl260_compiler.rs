//! Actual source -> ordinary parser AST -> raw CT CFG. Independent verification
//! and VM execution are exercised once the frozen 259 delivery is composed.
#[expect(
    dead_code,
    reason = "This standalone compiler harness omits the project associated-constant caller"
)]
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
        target_pointer_width: 64,
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
        if matches!(
            usage,
            zil_verify::CtSymbolUsage::Schema
                | zil_verify::CtSymbolUsage::TypeName
                | zil_verify::CtSymbolUsage::TypeApply
        ) {
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
        (Some(symbols[1].1.clone()), Vec::new())
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
        target_pointer_width: 64,
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

#[test]
fn typed_range_and_reverse_literal_operands_keep_i64_context() {
    let value = run(
        "fn sum(n:i64)->i64{var v:i64=0;for i in 0..n{v+=i}if 1<v{return 1+v}return 0}",
        "sum",
        &[CtValue::Scalar(zil_ir::Constant::I64(10))],
    );
    assert_eq!(value, CtValue::Scalar(zil_ir::Constant::I64(46)));
}

#[test]
fn copy_generic_type_function_infers_and_keeps_all_instances() {
    let source = "fn select<T:Copy>(x:T)->Type{return T} fn entry()->Type{let a=select(7);let b=select::<i64>(7);let c=select(8);return b}";
    assert_eq!(
        run(source, "entry", &[]),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I64)],
            root: CtTypeNodeId(0),
        })
    );
    let raw = build(source, "entry");
    assert_eq!(
        raw.functions.len(),
        3,
        "one wrapper and two actual select instances"
    );
    assert_eq!(
        raw.functions
            .iter()
            .filter(|f| f.parameter_types == vec![CtTy::Scalar(CtScalarTy::I32)])
            .count(),
        1
    );
    assert_eq!(
        raw.functions
            .iter()
            .filter(|f| f.parameter_types == vec![CtTy::Scalar(CtScalarTy::I64)])
            .count(),
        1
    );
}

#[test]
fn copy_generic_record_and_nested_calls_execute_in_verified_vm() {
    let source = "struct Box<T:Copy>{value:T} fn id<T>(x:T)->T where T:Copy,T:Copy{return x} fn select(x:Box<i32>)->Type{if id(x.value)==7{return i32}return i64} fn entry()->Type{return select(Box::<i32>{value:7})}";
    assert_eq!(
        run(source, "entry", &[]),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        })
    );
    let source = "fn select<T:Copy>(x:T,n:i64)->Type{if n==1{return T}return bool}fn entry()->Type{return select(7,1)}";
    assert_eq!(
        run(source, "entry", &[]),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0)
        })
    );
}

#[test]
fn ct_generic_nominal_and_phantom_arguments_do_not_merge() {
    let raw = build(
        "struct A{x:i32}struct B{x:i32}type Alias=A;struct Config<T:Copy,P:Copy>{value:T} fn take<T:Copy,P:Copy>(x:Config<T,P>)->Type{return T} fn entry()->Type{let a=take(Config::<A,bool>{value:A{x:1}});let b=take(Config::<B,bool>{value:B{x:1}});let c=take(Config::<Alias,bool>{value:A{x:1}});let d=take(Config::<A,i32>{value:A{x:1}});return d}",
        "entry",
    );
    assert_eq!(
        raw.functions.len(),
        4,
        "A,bool and Alias,bool normalize; B,bool and A,i32 remain separate"
    );
    assert_eq!(
        raw.record_schemas.len(),
        5,
        "A/B plus three complete Config identities"
    );
    let authority = Authority { raw: &raw };
    let bytes = zil_ir::ct_wire::encode_program(&raw, &Limits::default()).unwrap();
    let decoded = zil_ir::ct_wire::decode_program(&bytes, &Limits::default()).unwrap();
    assert_eq!(decoded, raw);
    zil_verify::verify_ct_artifact(decoded, &Limits::default(), &authority).unwrap();
    zil_verify::verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
    let mut tampered = raw.clone();
    let last = tampered.functions.len() - 1;
    tampered.functions[last].parameter_types = tampered.functions[1].parameter_types.clone();
    assert!(zil_verify::verify_ct_artifact(tampered, &Limits::default(), &authority).is_err());
}

#[test]
fn unused_ct_generic_definitions_check_only_declared_copy_capabilities() {
    for bad in [
        "fn bad<T:Copy>(x:T)->Type{let y=x+1;return T}",
        "fn bad<T:Copy>(x:T)->Type{let y=x==x;return T}",
        "fn bad<T:Copy>(x:T)->Type{let y=-x;return T}",
        "fn bad<T:Copy>(x:T)->Type{let y=i32(x);return T}",
        "struct T{field:i32}fn bad<T:Copy>(x:T)->Type{let y=x.field;return T}",
        "struct T{}fn bad<T:Copy>(x:T)->Type{let y=T{};return T}",
        "fn bad<T:Copy>(x:T)->Type{let y:T={};return T}",
        "struct T<U>{value:U}fn bad<T:Copy>(x:T)->Type{return T::<i32>}",
        "fn bad<T:Copy,U:Copy>(x:T)->Type{let y:U=x;return T}",
        "fn bad<T>(x:T)->Type{return T}",
        "trait Copy{}fn bad<T:Copy>(x:T)->Type{let y=x+1;return T}",
        "fn bad<T:Copy>(x:T)->Type where Missing:Copy{return T}",
        "fn bad<T:Copy>(x:T)->Type where T:Unknown{return T}",
        "fn bad<T:Copy>(x:T)->Type where T:i32{return T}",
        "fn bad<T:Copy>(x:T)->Type{if false{let y=x.field}return T}",
    ] {
        let source = format!("{bad} fn entry()->Type{{return i32}}");
        assert!(try_build(&source, "entry").is_err(), "{source}");
    }
    build(
        "fn okay<T:Copy,U:Copy>(x:T,y:U)->Type{let z=x;var a=x;a=z;return T}fn entry()->Type{return i32}",
        "entry",
    );
}

#[test]
fn generic_value_helpers_reached_from_ct_still_check_symbolic_bounds() {
    for source in [
        "fn bad<T:Copy>(x:T)->T{return x+1}fn entry()->Type{let y=bad(7);return i32}",
        "fn bad<T:Copy>(x:T)->T{return x+1}fn outer<T:Copy>(x:T)->Type{let y=bad(7);return T}fn entry()->Type{return outer(1)}",
    ] {
        assert!(try_build(source, "entry").is_err(), "{source}");
    }
}

#[test]
fn generic_type_binders_shadow_ct_builtins_and_outer_ct_nominals() {
    use zil_driver::{CompileOptions, Stage, compile_source};
    for source in [
        "fn identity<Type:Copy>(value:Type)->Type{return value}fn main()->i32{return identity(7)-7}",
        "fn identity<Syntax:Copy>(value:Syntax)->Syntax{return value}fn main()->i32{return identity(7)-7}",
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "314-shadow.zil", source),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{source}: {}",
            report.to_json()
        );
    }
    let mut raw = build(
        "struct T{value:Type}fn identity<T:Copy>(value:T)->T{return value}fn entry()->Type{let y=identity(7);return i32}",
        "entry",
    );
    let definitions = ct_only_definitions(&mut raw).unwrap();
    assert!(
        !definitions.iter().any(|s| s.declaration == NodeId(1)),
        "outer T must not classify identity's binder"
    );
    assert!(
        raw.functions
            .iter()
            .any(|f| f.definition.declaration == NodeId(1)
                && f.parameter_types == vec![CtTy::Scalar(CtScalarTy::I32)])
    );
}

#[test]
fn generic_record_array_argument_keeps_ordinary_copy_shape() {
    let source = "struct Box<T:Copy>{value:T}fn inspect(x:Box<[i32;2]>)->Type{if x.value[1]==2{return i32}return i64}fn entry()->Type{return inspect(Box::<[i32;2]>{value:[1,2]})}";
    assert_eq!(
        run(source, "entry", &[]),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0)
        })
    );
}

#[test]
fn generic_type_binder_cannot_capture_alias_definition_scope() {
    let raw = build(
        "struct T{x:i32}type Alias=T;fn select<T:Copy>(x:T)->Type{return Alias}fn entry()->Type{return select(7)}",
        "entry",
    );
    // Source declaration 0 is the outer nominal T, not select's scalar binder.
    assert!(
        raw.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(&i.kind,CtInstructionKind::TypeName(s) if s.declaration==NodeId(0)))
    );
    assert!(
        !raw.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, CtInstructionKind::TypePrimitive(CtScalarTy::I32)))
    );
}

#[test]
fn function_phantom_copy_arguments_and_recursive_cache_hits_keep_identity() {
    let raw = build(
        "struct A{x:i32}struct B{x:i32}type Alias=A;fn phantom<T:Copy>()->Type{return i32}fn entry()->Type{let a=phantom::<A>();let b=phantom::<B>();return phantom::<Alias>()}",
        "entry",
    );
    assert_eq!(raw.functions.len(), 3);
    assert_eq!(
        raw.functions[1].parameter_types,
        raw.functions[2].parameter_types
    );
    assert_ne!(raw.functions[1].id, raw.functions[2].id);
    let source = "fn recurse<T:Copy>(x:T,n:i32)->Type{if n==0{return T}return recurse(x,n-1)}fn entry()->Type{return recurse(7,4)}";
    assert_eq!(
        run(source, "entry", &[]),
        CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0)
        })
    );
    assert_eq!(build(source, "entry").functions.len(), 2);
}

#[test]
fn ct_generic_project_resolution_keeps_source_visibility() {
    use std::collections::BTreeMap;
    use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let main = "import model;fn main()->i32{let n:#(model.select(7))=7;return n-7}";
    for (library, success) in [
        ("pub fn select<T>(x:T)->Type where T:Copy{return T}", true),
        ("fn select<T:Copy>(x:T)->Type{return T}", false),
        (
            "pub fn select<T:Copy>(x:T)->Type{return T}fn unused<U:Copy>(x:U)->Type{let y=x.field;return U}",
            false,
        ),
        (
            "struct T{field:i32}type Alias=T;pub fn select<T:Copy>(x:T)->Type{return Alias}",
            false,
        ),
    ] {
        let report = compile_project(
            &ProjectSources {
                root: ModuleId(0),
                modules: vec![
                    SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
                    SourceUnit::new(FileId(1), ModuleId(1), "model.zil", library),
                ],
                paths: BTreeMap::from([(vec!["model".into()], ModuleId(1))]),
            },
            &options,
        );
        assert_eq!(
            report.status == zil_base::ResultCategory::Success,
            success,
            "{library}: {}",
            report.to_json()
        );
    }
}

#[test]
fn generic_definition_without_invocation_rejects_before_runtime_pipeline() {
    use zil_driver::{CompileOptions, Stage, compile_source};
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    for (body, success) in [
        ("let y=x;return T", true),
        ("let y:T={};return T", false),
        ("let y=x.field;return T", false),
        ("return T;let y=x+1", false),
    ] {
        let text = format!("fn unused<T:Copy>(x:T)->Type{{{body}}}fn main()->i32{{return 0}}");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "314-unused.zil", text),
            &options,
        );
        assert_eq!(
            report.status == zil_base::ResultCategory::Success,
            success,
            "{}",
            report.to_json()
        );
        if !success {
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.phase == zil_base::Phase::Ct)
            );
        }
    }
}

#[test]
fn ct_generic_other_ledgers_have_exact_fresh_boundaries() {
    let source = "struct Box<T:Copy,P:Copy>{value:T}fn select<T:Copy,P:Copy>(x:Box<T,P>)->Type{return T}fn entry()->Type{let a=select(Box::<i32,bool>{value:7});let b=select(Box::<i64,bool>{value:7});return a}";
    for name in ["nodes", "bytes", "instances", "depth"] {
        let mut low = 0;
        let mut high = match name {
            "nodes" => Limits::default().max_nodes,
            "bytes" => Limits::default().max_source_bytes,
            "instances" => Limits::default().max_functions,
            _ => Limits::default().max_call_depth,
        };
        let set = |n| {
            let mut l = Limits::default();
            match name {
                "nodes" => l.max_nodes = n,
                "bytes" => l.max_source_bytes = n,
                "instances" => l.max_functions = n,
                _ => l.max_call_depth = n,
            };
            l
        };
        while low < high {
            let n = low + (high - low) / 2;
            match try_build_with_limits(source, "entry", &set(n)) {
                Ok(_) => high = n,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit),
                        "{name}: {errors:?}"
                    );
                    low = n + 1;
                }
            }
        }
        let a = try_build_with_limits(source, "entry", &set(low)).unwrap();
        assert_eq!(
            a,
            try_build_with_limits(source, "entry", &set(low)).unwrap()
        );
        assert!(
            try_build_with_limits(source, "entry", &set(low - 1))
                .unwrap_err()
                .iter()
                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        println!("314 {name} N={low}, N-1 ResourceLimit, fresh exact identity");
    }
}

#[test]
fn ordinary_copy_ct_actuals_reject_resource_address_and_ct_data() {
    for bad in [
        "select::<Type>(i32)",
        "select::<&i32>(7)",
        "select::<ByteBuffer>(7)",
        "select::<Syntax<Expr>>(quote<Expr>{1})",
        "select(Holder{value:i32})",
        "select::<i32,bool>(7)",
    ] {
        let source = format!(
            "struct Holder{{value:Type}}fn select<T:Copy>(x:T)->Type{{return T}}fn entry()->Type{{return {bad}}}"
        );
        assert!(try_build(&source, "entry").is_err(), "{source}");
    }
}

#[test]
fn original_pending_ct_generics_reach_runtime_verify() {
    use zil_driver::{CompileOptions, Stage, compile_source};
    for (name, text) in [
        (
            "function",
            include_str!("../../../tests/language-readiness/pending/ct-type-generic-function.zil"),
        ),
        (
            "record",
            include_str!("../../../tests/language-readiness/pending/ct-type-generic-record.zil"),
        ),
        (
            "syntax",
            "import std.syntax as syntax;struct Config<T:Copy,P:Copy>{value:T}fn value_type<T:Copy>(x:T)->Type{return T}fn expression(x:Config<i32,bool>)->Syntax<Expr>{return quote<Expr>{$(syntax.literal(x.value))}}fn main()->i32{let n:#(value_type(7))=#splice(expression(Config::<i32,bool>{value:7}));return n-7}",
        ),
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), format!("314-{name}.zil"), text);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert!(
            report.diagnostics.is_empty(),
            "{name}: {:?}",
            report.diagnostics
        );
        assert_eq!(report.status, zil_base::ResultCategory::Success);
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn ct_generic_analysis_budget_has_exact_fresh_boundary() {
    let source = "struct Box<T:Copy,P:Copy>{value:T}fn select<T:Copy,P:Copy>(x:Box<T,P>)->Type{return T}fn entry()->Type{let a=select(Box::<i32,bool>{value:7});let b=select(Box::<i64,bool>{value:7});return a}";
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        };
        match try_build_with_limits(source, "entry", &limits) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == zil_base::ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = mid + 1;
            }
        }
    }
    let limits = Limits {
        max_analysis_steps: low,
        ..Limits::default()
    };
    let a = try_build_with_limits(source, "entry", &limits).unwrap();
    let b = try_build_with_limits(source, "entry", &limits).unwrap();
    assert_eq!(a, b);
    let errors = try_build_with_limits(
        source,
        "entry",
        &Limits {
            max_analysis_steps: low - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    println!("314 generic source attribution N={low}, N-1 ResourceLimit, fresh exact identity");
}

#[test]
fn generic_wire_roundtrip_executes_concrete_phantom_record_instances() {
    let source = "struct Config<T:Copy,P:Copy>{value:T}fn value<T:Copy,P:Copy>(x:Config<T,P>)->T{return x.value}fn entry()->i32{let a=value(Config::<i32,bool>{value:7});let b=value(Config::<i64,i32>{value:8});return a}";
    let raw = build(source, "entry");
    let limits = Limits::default();
    let bytes = zil_ir::ct_wire::encode_program(&raw, &limits).unwrap();
    let decoded = zil_ir::ct_wire::decode_program(&bytes, &limits).unwrap();
    assert_eq!(decoded, raw);
    let authority = Authority { raw: &raw };
    let verified = zil_verify::verify_ct_artifact(decoded.clone(), &limits, &authority).unwrap();
    let mut budget = zil_ct::CtExecutionBudget::new(raw.budget);
    let (value, _) = zil_ct::evaluate_ct_artifact(&verified, &[], &Reader, &mut budget).unwrap();
    assert_eq!(value, CtValue::Scalar(zil_ir::Constant::I32(7)));
    let mut wrong = decoded;
    wrong.record_schemas.swap(0, 1);
    assert!(zil_verify::verify_ct_artifact(wrong, &limits, &authority).is_err());
}

#[test]
fn multiple_generic_queries_share_request_budget_on_cold_and_warm_sessions() {
    use zil_driver::{CompilationSession, CompileOptions, Stage, compile_source};
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "314-two-queries.zil",
        "fn select<T:Copy>(x:T)->Type{return T}fn main()->i32{let a:#(select(7))=7;let b:#(select(i64(8)))=8;return a-7}",
    );
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    let options = |n| CompileOptions {
        stop_after: Stage::Verify,
        limits: Limits {
            max_analysis_steps: n,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    while low < high {
        let n = low + (high - low) / 2;
        let report = compile_source(&source, &options(n));
        if report.status == zil_base::ResultCategory::Success {
            high = n;
        } else {
            assert_eq!(
                report.status,
                zil_base::ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            low = n + 1;
        }
    }
    let mut session = CompilationSession::default();
    for _ in 0..2 {
        let report = session.compile_source(&source, &options(low));
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for _ in 0..2 {
        let report = session.compile_source(&source, &options(low - 1));
        assert_eq!(
            report.status,
            zil_base::ResultCategory::ResourceLimit,
            "{}",
            report.to_json()
        );
    }
    assert_eq!(
        compile_source(&source, &options(low)).status,
        zil_base::ResultCategory::Success
    );
    println!("314 two-query full request N={low}; cold/warm/fresh N success, N-1 ResourceLimit");
}

#[test]
fn review319_lexical_alias_and_phantom_result_types_reach_real_consumers() {
    use zil_driver::{CompileOptions, Stage, compile_source};
    for (name, text) in [
        (
            "alias",
            "struct T{value:i32}type Alias=T;fn select<T:Copy>(value:T)->Type{return Alias}fn main()->i32{let value:#(select(7))=T{value:7};return value.value-7}",
        ),
        (
            "phantom",
            "struct Config<T:Copy,P:Copy>{value:T}fn actual<T:Copy>(value:T)->Type{return T}fn chosen()->Type{let left=actual(Config::<i32,bool>{value:7});let right=actual(Config::<i32,i32>{value:7});if left==right{return i64}return i32}fn needs_i32(value:i32)->i32{return value}fn main()->i32{let value:#(chosen())=7;return needs_i32(value)-7}",
        ),
    ] {
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                format!("review319-{name}.zil"),
                text,
            ),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn review319_generic_binders_shadow_ct_domain_names() {
    use zil_driver::{CompileOptions, Stage, compile_source};
    let mut failures = Vec::new();
    for name in ["Type", "Text", "Ident", "Syntax"] {
        let text = format!(
            "fn identity<{name}:Copy>(value:{name})->{name}{{return value}}fn main()->i32{{return identity(7)-7}}"
        );
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                format!("review319-{name}.zil"),
                text,
            ),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        if report.status != zil_base::ResultCategory::Success || report.verification_runs != 1 {
            failures.push(format!("{name}: {}", report.to_json()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
