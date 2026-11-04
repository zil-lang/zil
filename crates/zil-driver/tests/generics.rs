use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

const STORED_VIEW: &str =
    include_str!("../../../tests/language-readiness/terminal/342/stored_view.zil");

#[test]
fn terminal_stored_view_source_keeps_actual_targets_through_direct_calls() {
    for text in [
        STORED_VIEW.to_owned(),
        STORED_VIEW
            .replace("life a", "life renamed")
            .replace("'a", "'renamed"),
        STORED_VIEW
            .replace(
                "fn make",
                "fn relay<life b>(p:&'b i64)->View<'b>{return make(p)}\nfn make",
            )
            .replace("let v = make(&n)", "let v = relay(&n)"),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for text in [
        STORED_VIEW
            .replace("let n: i64", "var n: i64")
            .replace("return i32(*q);", "n=43; return i32(*q);"),
        STORED_VIEW.replace(
            "return View::<'a> { p: p };",
            "let local:i64=0; return View::<'a> { p: &local };",
        ),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| matches!(d.phase, Phase::Analyze | Phase::Verify)),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn terminal_stored_project_same_named_helpers_keep_distinct_relations() {
    let modules = [
        (
            "main.zil",
            "import a;import b;fn main()->i32{let x:i64=20;let y:i64=22;let va=a.make(&x);let vb=b.make(&x,&y);let pa=a.read(&va);let pb=b.read(&vb);return i32(*pa+*pb)}",
        ),
        (
            "a.zil",
            "struct View<life a>{p:&'a i64}pub fn make<life a>(p:&'a i64)->View<'a>{return View::<'a>{p:p}}pub fn read<life a>(v:&View<'a>)->&'a i64{return v.p}",
        ),
        (
            "b.zil",
            "struct View<life a>{p:&'a i64}pub fn make<life a>(unused:&i64,p:&'a i64)->View<'a>{return View::<'a>{p:p}}pub fn read<life a>(v:&View<'a>)->&'a i64{return v.p}",
        ),
    ];
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: modules
            .into_iter()
            .enumerate()
            .map(|(index, (name, text))| {
                SourceUnit::new(FileId(index as u32), ModuleId(index as u32), name, text)
            })
            .collect(),
        paths: [
            (vec!["a".into()], ModuleId(1)),
            (vec!["b".into()], ModuleId(2)),
        ]
        .into_iter()
        .collect(),
    };
    let report = zil_driver::compile_project(&project, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn terminal_stored_core_requires_matching_actual_nominal_inventory() {
    use zil_base::{Limits, TargetProfile};
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let unit = source(STORED_VIEW);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let aliases = Default::default();
    let core = zil_expand::expand_with_map_limits_target_context(
        &unit,
        &syntax,
        &limits,
        &target,
        SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    assert_eq!(typed.life_instances.len(), 1);
    assert!(typed.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, zil_ir::InstructionKind::LoadStoredBorrow { .. }))
    }));
    let mut wrong = core.clone();
    wrong.aggregates[0].fields[0].ty = zil_ir::Type::I64;
    assert!(zil_types::check(&wrong, &target, &limits).is_err());
    let mut wrong = core.clone();
    let mut duplicate = wrong.life_declarations[0].clone();
    duplicate.id.declaration.0 += 1;
    wrong.life_declarations.push(duplicate);
    assert!(zil_types::check(&wrong, &target, &limits).is_err());
    let program = zil_ir::CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    let normalized = zil_types::resolve_program_shapes(&program, &limits).unwrap();
    let again = zil_types::resolve_program_shapes(&normalized, &limits).unwrap();
    assert_eq!(
        again, normalized,
        "normalization changes the actual stored identity"
    );
}

#[test]
fn terminal341_retained_declaration_rows_survive_ct_removal() {
    let scalar = "fn scalar(marker:Type,n:i32)->i32{return n}";
    let view = "struct View<life a>{p:&'a i64}fn make<life a>(p:&'a i64)->View<'a>{return View::<'a>{p:p}}";
    let main = "fn main()->i32{return #(scalar(i32,0))}";
    let directory = std::env::temp_dir().join(format!(
        "zil-retained-context341-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    for (index, (text, expected)) in [
        (format!("{scalar}{view}{main}"), 1),
        (format!("import std.meta;{scalar}{view}{main}"), 2),
        (format!("{view}{scalar}{main}"), 0),
    ]
    .into_iter()
    .enumerate()
    {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Core,
                emit_dir: Some(directory.join(index.to_string())),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let dump = fs::read_to_string(&report.artifacts["core"]).unwrap();
        assert!(dump.contains("life_declarations: ["), "{dump}");
        let declaration = dump.split("life_declarations: [").nth(1).unwrap();
        assert!(
            declaration
                .split_whitespace()
                .collect::<String>()
                .contains(&format!("declaration:NodeId({expected},)")),
            "{declaration}"
        );
        assert!(declaration.contains("\"View\""), "{declaration}");
        assert!(!dump.contains("name: \"$module0_scalar\""), "{dump}");
        if index == 1 {
            assert!(dump.contains("imports: []"), "{dump}");
        }
    }
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn terminal341_driver_declaration_context_preserves_original_and_closes_unknown_lineage() {
    let text = "struct View<life a>{p:&'a i64}fn make<life a>(p:&'a i64)->View<'a>{return View::<'a>{p:p}}fn main(){}";
    let directory = std::env::temp_dir().join(format!(
        "zil-context341-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut packages = Vec::new();
    for path in ["first-location.zil", "other/location.zil"] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), path, text);
        let options = CompileOptions {
            stop_after: Stage::Core,
            emit_dir: Some(directory.join(packages.len().to_string())),
            ..CompileOptions::default()
        };
        let report = compile_source(&source, &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let dump = fs::read_to_string(&report.artifacts["core"]).unwrap();
        let package = dump
            .split("package: \"")
            .filter_map(|part| part.strip_prefix("closed-snapshot:v1:"))
            .next()
            .expect("Driver did not pass original context")
            .split('"')
            .next()
            .unwrap()
            .to_owned();
        packages.push(package);
    }
    assert_eq!(
        packages[0], packages[1],
        "native file labels changed logical snapshot identity"
    );
    let transformed = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "transformed.zil",
        format!("#if(true){{fn selected(){{}}}}{text}"),
    );
    let report = compile_source(
        &transformed,
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(directory.join("must-not-publish")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("explicit source lineage")),
        "{}",
        report.to_json()
    );
    assert!(report.commands.is_empty());
    assert!(!directory.join("must-not-publish").exists());
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn terminal341_generic_methods_specialize_ordinary_calls() {
    for text in [
        include_str!("../../../tests/language-readiness/terminal/341/generic-methods.zil"),
        "struct P{}trait Echo{type Item:Copy;fn echo<T:Copy>(self:&Self,x:Self.Item)->Self.Item{return x}}impl Echo for P{type Item=i32;}fn main()->i32{let p=P{};return p.echo::<bool>(7)-7}",
        "struct Box<T:Copy>{value:T}trait Echo{type Item:Copy;fn echo<T:Copy>(self:&Self,x:Self.Item)->Self.Item;}impl<T:Copy> Echo for Box<T>{type Item=T;fn echo<U:Copy>(x:Self.Item)->Self.Item{return x}}fn main()->i32{let p=Box::<bool>{value:true};if p.echo::<i32>(true){return 0}return 1}",
        "struct P{}trait Echo{fn echo<T>(self:&Self,x:T)->T where T:Copy;}impl Echo for P{fn echo<U>(x:U)->U{return x}}fn main()->i32{let p=P{};return p.echo(7)-7}",
        "struct P{}trait Echo{fn echo<T>(self:&Self,x:T)->T where T:Copy;}impl Echo for P{fn echo<U:Copy>(x:U)->U{return x}}fn main()->i32{let p=P{};return p.echo(7)-7}",
        "trait Item{type Value:Copy;}struct P{}impl Item for P{type Value=i32;}struct Host{}trait Echo{fn echo<T:Item>(self:&Self,x:T.Value)->T.Value;}impl Echo for Host{fn echo<U>(x:U.Value)->U.Value{return x}}fn main()->i32{let p=Host{};return p.echo::<P>(7)-7}",
        "struct P{n:i32}trait Echo<T:Copy>{fn echo<T:Copy>(self:&Self,x:T)->T;}impl Echo<bool> for P{fn echo<U>(x:U)->U{return x}}fn main()->i32{let p=P{n:1};return p.echo(7)-7}",
        "struct P{n:i32}trait Echo{fn echo<T:Copy>(self:&Self,x:T)->T;}impl Echo for P{fn echo<U:Copy>(x:U)->U{return x}}fn main()->i32{let p=P{n:1};return p.echo(7)-7}",
        "struct P{n:i32}trait Echo{fn echo<T:Copy>(self:&Self,x:T)->T;}impl Echo for P{fn echo<U:Copy>(x:U)->U{return x}}fn main()->i32{let p=P{n:1};return p.echo::<i32>(7)-7}",
        "struct P{n:i32}trait Echo{fn echo<T:Copy>(self:&Self,x:T)->T{return x}}impl Echo for P{}fn main()->i32{let p=P{n:1};return p.echo(7)-7}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn terminal341_generic_methods_reject_unused_invalid_contracts() {
    for text in [
        "trait Echo{fn bad<T:Copy>(self:&Self,x:T)->T{return x+1}}fn main(){}",
        "struct P{}trait Echo{fn echo<T>(self:&Self,x:T)->T;}impl Echo for P{fn echo<U:Copy>(x:U)->U{return x}}fn main(){}",
        "struct P{}trait Echo{fn echo<T:Copy>(self:&Self,x:T)->T;}impl Echo for P{fn echo<#N:i32>(x:i32)->i32{return x}}fn main(){}",
        "struct P{}trait Echo{fn echo<#N:usize>(self:&Self)->usize;}impl Echo for P{fn echo<#M:u64>()->usize{return 3usize}}fn main(){}",
        "struct P{}trait Echo{fn echo<T:Copy>(self:&Self,x:T)->T;}impl Echo for P{fn echo<U:Copy>(x:U)->U{return x+1}}fn main(){}",
        "trait Echo{fn bad<#N:i32>(self:&Self)->&i32{return &N}}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    let output = std::env::temp_dir()
        .join(format!(
            "341-method-failed-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .join("program");
    let report = compile_source(
        &source(
            "struct P{}trait Echo{fn echo<T>(self:&Self,x:T)->T;}impl Echo for P{fn echo<U:Copy>(x:U)->U{return x}}fn main(){}",
        ),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(output.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(report.commands.is_empty());
    assert!(!report.artifacts.contains_key("executable"));
    assert!(!output.exists());
}

#[test]
fn terminal341_unsigned_const_domains_keep_full_target_bits() {
    let positive =
        include_str!("../../../tests/language-readiness/terminal/341/const-unsigned.zil");
    for text in [
        positive,
        "fn value<#N:usize>()->usize{return N}fn main()->i32{if value::<18446744073709551615usize>()==18446744073709551615usize{return 0}return 1}",
        "fn unused<T:Copy>(x:T)->u64{return 18446744073709551615u64}fn main(){}",
        "fn unused<T:Copy>(x:T,v:u64)->i32{match v{18446744073709551615u64=>{return 0},_=>{return 1}}}fn main()->i32{return unused(true,18446744073709551615u64)}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for target in [
        zil_base::TargetProfile::default(),
        zil_base::TargetProfile::windows_x64_llvm(),
    ] {
        let report = compile_source(
            &source(positive),
            &CompileOptions {
                target,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    let target = zil_base::TargetProfile {
        pointer_width: 32,
        ..Default::default()
    };
    for (text, expected) in [
        (positive, ResultCategory::Success),
        (
            "fn value<#N:usize>(){}fn main(){value::<4294967295>()}",
            ResultCategory::Success,
        ),
        (
            "fn value<#N:usize>(){}fn main(){value::<4294967296>()}",
            ResultCategory::SafeReject,
        ),
        (
            "fn value<#N:usize>(){}fn main(){value::<4294967296usize>()}",
            ResultCategory::SafeReject,
        ),
        (
            "fn value<T:Copy>(x:T)->usize{return 4294967295usize}fn main(){}",
            ResultCategory::Success,
        ),
        (
            "fn value<T:Copy>(x:T)->usize{return 4294967296usize}fn main(){}",
            ResultCategory::SafeReject,
        ),
        (
            "fn value<T:Copy>(x:T,n:usize){match n{4294967296usize=>{},_=>{}}}fn main(){}",
            ResultCategory::SafeReject,
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                target: target.clone(),
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, expected, "{text}: {}", report.to_json());
    }
    for text in [
        "fn value<#N:u64>(){}fn main(){value::<18446744073709551616u64>()}",
        "fn value<#N:usize>(){}fn main(){value::<18446744073709551616usize>()}",
        "fn value<#N:usize>(){}fn main(){value::<3u64>()}",
        "fn bad<T:Copy>(x:T)->u64{return 18446744073709551616u64}fn main(){}",
        "fn bad<T:Copy>(x:T)->u64{return -1u64}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            zil_base::ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn terminal341_ct_generic_type_return_record_reflection_and_syntax() {
    for text in [
        include_str!("../../../tests/language-readiness/pending/ct-type-generic-function.zil"),
        include_str!("../../../tests/language-readiness/pending/ct-type-generic-record.zil"),
        "struct Box<T:Copy>{value:T}fn select<T:Copy>(x:T)->Type{return T}fn main()->i32{let value:#(select(Box::<i32>{value:7}))=Box::<i32>{value:7};return value.value-7}",
        "import std.meta as meta;struct Box<T:Copy>{value:T}fn selected()->Type{return meta.field_type(meta.field(Box::<i64>,\"value\"))}fn main()->i32{let n:#(selected())=3000000000;return i32(n-3000000000)}",
        "import std.meta as meta;struct Box<T:Copy>{value:T}fn selected<T:Copy>(x:T)->Type{return meta.field_type(meta.field(T,\"value\"))}fn main()->i32{let n:#(selected(Box::<i64>{value:7}))=3000000000;return i32(n-3000000000)}",
        include_str!(
            "../../../tests/language-readiness/terminal/341/ct-generics-reflection-syntax.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        "import std.syntax as syntax;struct Config<T:Copy,P:Copy>{value:T}fn expression<T:Copy>(x:Config<T,bool>)->Syntax<Expr>{return quote<Expr>{$(syntax.literal(x.value))}}fn main(){}",
        "fn invalid<T:Copy>(x:T)->Type{let n=x+1;return T}fn main(){}",
        "import std.meta as meta;struct Box<T:Copy>{value:T}fn selected()->Type{return meta.field_type(meta.field(Box::<i64>,\"absent\"))}fn main(){let n:#(selected())=7}",
        "import std.syntax as syntax;struct Box<T:Copy,P:Copy>{value:T}fn selected<T:Copy>(x:T)->Syntax<Type>{return syntax.type_ref(T)}fn main(){let value:#splice(selected(Box::<i32,bool>{value:7}))=Box::<i32,u8>{value:7}}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn terminal341_const_scalar_functions_check_unused_definitions_and_substitute_values() {
    for text in [
        include_str!("../../../tests/language-readiness/terminal/341/const-functions.zil"),
        "fn constant<#N:i32>()->i32{return N}fn main()->i32{return constant::<7>()-7}",
        "fn pick<T:Copy,#N:i32>(x:T)->T{let n:i32=N;return x}fn main()->i32{return pick::<i32,3>(7)-7}",
        "fn flag<#B:bool>()->bool{return B}fn main()->i32{if flag::<true>(){return 0}return 1}",
        "fn constant<#N:u8>()->u8{return N}fn main()->i32{let x:u8=constant::<255>();return 0}",
        "fn scoped<#N:i32>()->i32{let N:i32=8;return N}fn main()->i32{return scoped::<7>()-8}",
        "fn scoped<#N:i32>()->i32{let N:i32=8;let p=&N;return *p}fn main()->i32{return scoped::<7>()-8}",
        "fn array<#N:i32>(x:[i32;N])->[i32;N]{return x}fn main()->i32{let x=array::<2>([3,4]);return x[0]-3}",
        "fn array<#N:i32>(x:[i32;N])->[i32;N]{let M=3;let a:[i32;N]=x;return a}fn main()->i32{let x=array::<2>([3,4]);return x[0]-3}",
        "fn scoped<#N:i32>(N:i32)->i32{return N}fn main()->i32{return scoped::<7>(8)-8}",
        "fn inner<#N:i32>()->i32{return N}fn outer<#M:i32>()->i32{return inner::<M>()}fn main()->i32{return outer::<7>()-7}",
        "enum E{A(i32),B(i32),C(i32),D(i32)}fn sink(n:i32){}fn arms<#N:i32>(e:E)->i32{match e{E.A(N)=>sink(N),E.B(N)=>sink(N),E.C(N)=>sink(N),E.D(N)=>sink(N)}return N}fn main()->i32{return arms::<7>(E.B(9))-7}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for text in [
        include_str!("../../../tests/language-readiness/terminal/341/const-unused-invalid.zil"),
        include_str!(
            "../../../tests/language-readiness/terminal/341/const-shadow-length-invalid.zil"
        ),
        "fn bad<#B:bool>()->i32{return B}fn main()->i32{return 0}",
        "fn bad<T,#N:i32>(x:T)->T{return x+x}fn main()->i32{return 0}",
        "fn value<#N:u8>()->u8{return N}fn main()->i32{value::<256>();return 0}",
        "fn value<#N:u8>()->u8{return N}fn main()->i32{value::<3i32>();return 0}",
        "fn value<#N:bool>()->bool{return N}fn main()->i32{value::<1>();return 0}",
        "fn value<T>(x:T)->T{return x}fn main()->i32{return value::<3>(7)}",
        "fn inner<#N:i32>()->i32{return N}fn bad<#M:i32>(x:i32)->i32{return inner::<x>()}fn main(){}",
        "fn inner<#N:i32>()->i32{return N}fn bad<#M:i32>()->i32{let M=2;return inner::<M>()}fn main(){}",
        "fn bad<#N:bool>(x:[i32;N])->[i32;N]{return x}fn main(){}",
        "fn bad<#N:i32>()->&i32{return &N}fn main(){}",
        "fn bad<#N:i32>(){let p=&N;}fn main(){}",
        "fn bad<#N:i32>(x:[i32;N])->[i32;N]{let N=3;let a:[i32;N]=x;return a}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn terminal341_const_instances_reach_wire_native_and_exact_budget() {
    use zil_base::{Limits, TargetProfile};
    let text =
        include_str!("../../../tests/language-readiness/terminal/341/const-array-machine.zil");
    let src = source(text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &typed.target, &Limits::default()).unwrap();
    let llvm = native(text, 7);
    assert!(!llvm.contains("dictionary"));
    let check = |steps| {
        zil_expand::expand_with_map_limits(
            &src,
            &ast,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        )
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    assert!(check(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if check(middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert_eq!(
        check(low - 1).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    assert!(check(low).is_ok());
    assert!(check(low).is_ok());
    eprintln!("341 const Expand exact work limit: {low}");
}

#[test]
fn terminal341_const_generated_binders_use_declaration_ordinals_with_shared_spans() {
    let src =
        source("fn pair<#A:i32,#B:i32>()->i32{return A+B}fn main()->i32{return pair::<3,4>()-7}");
    let mut ast = zil_syntax::parse(&src).into_result().unwrap();
    let zil_syntax::ast::ItemKind::Function(function) = &mut ast.items[0].kind else {
        unreachable!()
    };
    function.generics[1].span = function.generics[0].span;
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &zil_base::Limits::default(),
    )
    .unwrap();
    let plan = zil_analyze::analyze(&typed, &zil_base::Limits::default()).unwrap();
    zil_verify::verify_module(
        typed.clone(),
        plan,
        &typed.target,
        &zil_base::Limits::default(),
    )
    .unwrap();
}

#[test]
fn generic_enum_error_injection_preserves_native_error_and_success_arms() {
    use zil_driver::{NativeBackend, Optimization};
    for text in [
        include_str!("../../../tests/generic-error-enum/direct.zil"),
        include_str!("../../../tests/generic-error-enum/concrete.zil"),
        include_str!("../../../tests/generic-error-enum/helper.zil"),
        include_str!("../../../tests/generic-error-enum/annotated.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context-if.zil"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for (call, declaration) in [
        ("Fault.NoMemory", ""),
        ("fail()", "fn fail()->Fault!i32{return Fault.NoMemory}"),
        ("error", ""),
    ] {
        for (input, status, stdout) in [(0, 0, b"7".as_slice()), (1, 1, b"".as_slice())] {
            let binding = if call == "error" {
                "let error:Fault=Fault.NoMemory;"
            } else {
                ""
            };
            let text = format!(
                "extern \"C\" fn putchar(code:i32)->i32;enum Fault{{NoMemory}}{declaration}fn run<F:Fn()->i32>(f:F)->Fault!i32{{if f()!=0{{{binding}return {call}}}return 7}}fn main()->Fault!i32{{let f=()->i32=>{input};let value=run(f)?;unsafe{{putchar(48+value)}}return value-7}}"
            );
            for backend in &backends {
                for optimization in [Optimization::O0, Optimization::O2] {
                    let out = std::env::temp_dir().join(format!(
                        "zil-generic-enum-{}-{}",
                        std::process::id(),
                        NEXT.fetch_add(1, Ordering::Relaxed)
                    ));
                    fs::create_dir(&out).unwrap();
                    let report = compile_source(
                        &source(&text),
                        &CompileOptions {
                            backend: *backend,
                            optimization,
                            stop_after: Stage::Build,
                            output: Some(out.join("program")),
                            ..CompileOptions::default()
                        },
                    );
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(report.verification_runs, 1);
                    let codegen = report.native_codegen.as_ref().unwrap();
                    assert_eq!(codegen.backend, *backend);
                    assert!(codegen.fallback.is_none());
                    let actual = Command::new(&report.artifacts["executable"])
                        .output()
                        .unwrap();
                    assert_eq!(actual.status.code(), Some(status), "{text}: {actual:?}");
                    assert_eq!(actual.stdout, stdout, "{text}");
                    assert!(actual.stderr.is_empty(), "{actual:?}");
                    fs::remove_dir_all(out).unwrap();
                }
            }
        }
    }
    // Preserve the frozen inputs; expose their union return at the native
    // entry so the observed error tag depends on the generic constructor.
    for original in [
        include_str!("../../../tests/generic-error-enum/unique-context.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context-if.zil"),
    ] {
        let main = "fn main()->i32{let f=()->i32=>1;run(f);return 0}";
        assert_eq!(original.matches(main).count(), 1);
        let text = original.replace(
            main,
            "fn main()->Choice<i32>!i32{let f=()->i32=>1;return run(f)}",
        );
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = std::env::temp_dir().join(format!(
                    "zil-generic-enum-unique-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&out).unwrap();
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend: *backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(out.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, *backend);
                assert!(codegen.fallback.is_none());
                let actual = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(1), "{text}: {actual:?}");
                assert!(actual.stdout.is_empty(), "{actual:?}");
                assert!(actual.stderr.is_empty(), "{actual:?}");
                fs::remove_dir_all(out).unwrap();
            }
        }
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn review318_copy_record_binders_and_nested_unused_obligations() {
    for text in [
        "struct Box<T:Copy>{value:T} fn identity<Box>(value:Box)->Box{return value} fn result()->i32{return identity(17)}",
        "struct Box<T:Copy>{value:T} struct Holder<Box>{value:Box} fn result()->i32{return 0}",
        "struct Pair<T:Copy,U>{value:T} struct Holder<V>{inner:Pair<i32,V>} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review318 Copy-bound control: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for text in [
        "struct Tag<T:Copy>{n:i32} struct Outer<V>{value:Tag<(i32,Tag<V>)>} fn result()->i32{return 0}",
        "struct Tag<T:Copy>{n:i32} struct Pair<T:Copy,U>{value:T} struct Outer<V>{value:Pair<Tag<V>,bool>} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT")
        );
    }
}

#[test]
fn copy_record_bound_runtime_nested_and_phantom_arguments() {
    native(
        "struct Box<T:Copy>{value:T} struct Pair<T:Copy,U>{first:T;second:U} struct Tag<T:Copy>{n:i32} struct A{x:i32} struct B{x:i32} fn wrap<T:Copy>(value:T)->Box<T>{return Box::<T>{value}} fn result()->i32{let a=wrap(7);let b=Box::<Box<i32>>{value:a};let p=Pair::<i32,bool>{first:3,second:true};let left=Tag::<A>{n:2};let right=Tag::<B>{n:5};return b.value.value+p.first+left.n+right.n}",
        17,
    );
    for text in [
        "struct Box<T:Copy>{value:T} struct Outer<U:Copy>{inner:Box<U>} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} struct Pair<T:Copy,U>{first:T;second:U} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn common320_record_enum_phantom_cannot_hide_a_missing_struct_copy_obligation() {
    let prefix = "enum Ghost<T:Copy>{Empty{n:i32}}struct Box<T:Copy>{value:T}";
    let bad = compile_source(
        &source(&format!(
            "{prefix}struct Outer<U>{{value:Box<Ghost<U>>}}fn result()->i32{{return 0}}"
        )),
        &CompileOptions::default(),
    );
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert_eq!(bad.phase, Phase::Expand, "{}", bad.to_json());
    assert_eq!(bad.diagnostics[0].code, "E-GENERIC-ARGUMENT");
    assert_eq!(bad.verification_runs, 0);
    let good = compile_source(
        &source(&format!(
            "{prefix}struct Outer<U:Copy>{{value:Box<Ghost<U>>}}fn result()->i32{{return 0}}"
        )),
        &CompileOptions::default(),
    );
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    native(
        &format!(
            "{prefix}fn wrap<U:Copy>(value:Ghost<U>)->Box<Ghost<U>>{{return Box::<Ghost<U>>{{value}}}}fn result()->i32{{let wrapped=wrap(Ghost::<bool>.Empty{{n:7}});match wrapped.value{{Ghost.Empty{{n:value}}=>{{return value}}}}}}"
        ),
        7,
    );
}

#[test]
fn common320_primitive_type_binder_keeps_copy_obligations_and_concrete_neighbors() {
    let declaration = "struct Cell<i32:Copy>{value:i32}";
    for (text, category, code) in [
        (
            format!("{declaration}struct Outer<U>{{value:Cell<U>}}fn result()->i32{{return 0}}"),
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            format!(
                "{declaration}fn bad<i32:Copy>(value:Cell<i32>)->i32{{return value.value+1}}fn result()->i32{{return 0}}"
            ),
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert_eq!(report.status, category, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    }
    for alias in ["", "type Number=i32;"] {
        let text = format!(
            "{alias}{declaration}struct Allowed<U>{{value:Cell<i32>}}fn concrete()->i32{{return 7}}fn keep<T:Copy>(value:Cell<T>)->Cell<T>{{return value}}fn result()->i32{{let value=keep(Cell::<bool>{{value:true}});if !value.value{{return 99}}return concrete()}}"
        );
        native(&text, 7);
    }
    let text = format!(
        "{declaration}fn keep<T:Copy>(value:Cell<T>)->Cell<T>{{return value}}fn result()->i32{{let value=keep(Cell::<bool>{{value:true}});if value.value{{return 7}}return 0}}"
    );
    let unit = source(&text);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let run = |n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &ast,
            &zil_base::Limits {
                max_analysis_steps: n,
                ..zil_base::Limits::default()
            },
        )
        .map(|m| m.core)
    };
    let (mut low, mut high) = (0, zil_base::Limits::default().max_analysis_steps);
    let expected = run(high).unwrap();
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = middle;
            }
            Err(errors) => {
                assert!(!errors.is_empty());
                assert!(
                    errors
                        .iter()
                        .all(|d| d.phase == Phase::Expand
                            && d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = middle;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    let errors = run(high - 1).unwrap_err();
    assert!(!errors.is_empty());
    assert!(
        errors
            .iter()
            .all(|d| d.phase == Phase::Expand && d.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );
    assert_eq!(run(high).unwrap(), expected);
    eprintln!("common320 primitive binder Expand N={high}; N-1 ResourceLimit; fresh N identical");
}

#[test]
fn copy_record_bound_original_ct_source_reaches_native() {
    use zil_driver::Optimization;
    for text in [
        include_str!("../../../tests/language-readiness/pending/ct-type-generic-record.zil"),
        "struct Box<T:Copy>{value:T} fn select(x:Box<i32>)->Type{if x.value==7{return i32}return i64} fn main()->i32{let n:#(select(Box::<i32>{value:8}))=3000000000;return i32(n-3000000000)}",
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = std::env::temp_dir().join(format!(
                "zil-copy-record-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&out).unwrap();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    output: Some(out.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let actual = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), Some(0), "{actual:?}");
            assert!(actual.stdout.is_empty(), "{actual:?}");
            assert!(actual.stderr.is_empty(), "{actual:?}");
            fs::remove_dir_all(out).unwrap();
        }
    }
}

#[test]
fn copy_record_bound_unused_and_phantom_obligations_are_checked() {
    for text in [
        "struct Box<T:Copy>{value:T} struct Bad<U>{inner:Box<U>} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} struct Bad<U>{inner:Phantom<U>} fn result()->i32{return 0}",
        "struct U{n:i32} struct Phantom<T:Copy>{n:i32} struct Bad<U>{inner:Phantom<U>} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} fn unused<U>(v:Phantom<U>)->i32{return 0} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} fn unused(v:Phantom<&i32>)->i32{return 0} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Phantom::<&i32>{n:1};return x.n}",
        "struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Phantom::<[&i32;0]>{n:1};return x.n}",
        "resource R{n:i32} struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Phantom::<R>{n:1};return x.n}",
        "struct Box<T:Copy>{value:T} fn result()->i32{var n=1;let x=Box::<&mut i32>{value:&mut n};return n}",
        "struct Box<T:Copy>{value:T} struct Bad<T:Copy>{inner:Box<T,i32>} fn result()->i32{return 0}",
        "struct Box<T:Copy>{value:T} struct Bad<T:Copy>{inner:Box} fn result()->i32{return 0}",
        "struct Bad<T:Copy+Copy>{value:T} fn result()->i32{return 0}",
        "struct Bad<Copy,T:Copy>{n:i32} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            "E-GENERIC-ARGUMENT",
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    for text in [
        "package Box<T:Copy>{value:T} fn result()->i32{return 0}",
        "resource Box<T:Copy>{value:T} fn result()->i32{return 0}",
        "trait Mark{} struct Box<T:Mark>{value:T} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn copy_record_bound_phantom_identity_is_nominal() {
    let report = compile_source(
        &source(
            "struct Phantom<T:Copy>{n:i32} fn read(x:Phantom<i32>)->i32{return x.n} fn result()->i32{return read(Phantom::<bool>{n:7})}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        "E-GENERIC-ARGUMENT",
        "{}",
        report.to_json()
    );
}

#[test]
fn copy_record_bound_expand_budget_is_exact_and_fresh() {
    for (label, text) in [
        (
            "bound",
            "struct Box<T:Copy>{value:T} struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Box::<Phantom<i32>>{value:Phantom::<i32>{n:7}};return x.value.n}",
        ),
        (
            "old-unbound",
            "struct Box<T>{value:T} fn result()->i32{let x=Box::<i32>{value:7};return x.value}",
        ),
    ] {
        let src = source(text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let check = |steps| {
            zil_expand::expand_with_map_limits(
                &src,
                &ast,
                &zil_base::Limits {
                    max_analysis_steps: steps,
                    ..zil_base::Limits::default()
                },
            )
        };
        let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
        assert!(check(high).is_ok());
        while low < high {
            let middle = low + (high - low) / 2;
            if check(middle).is_ok() {
                high = middle
            } else {
                low = middle + 1
            }
        }
        let failed = check(low - 1).unwrap_err();
        assert_eq!(failed[0].phase, Phase::Expand);
        assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
        assert!(check(low).is_ok());
        assert!(check(low).is_ok());
        eprintln!("320 {label} Expand exact work limit: {low}");
    }
}

#[test]
fn generic_error_union_templates_defer_potential_equality_to_actual_instances() {
    for (definition, call, phase, code) in [
        (
            "fn run<U:Copy>(w:U)->U!i32{let n:i32=7;return n}",
            "run(INPUT)",
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "fn run<U:Copy>(w:U)->U!i32=>7;",
            "run(INPUT)",
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "fn run<U:Copy>(w:U)->U!i32{let n:U!i32=7;return n}",
            "run(INPUT)",
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "fn sink<T:Copy>(v:T!i32,w:T)->i32{return 0}fn run<U:Copy>(w:U)->i32{return sink(7,w)}",
            "run(INPUT)",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "fn sink<T:Copy>(w:T,v:T!i32)->i32{return 0}fn run<U:Copy>(w:U)->i32{return sink(w,7)}",
            "run(INPUT)",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        for text in [
            format!("{definition}fn main(){{}}"),
            format!(
                "{definition}fn main()->i32{{{};return 0}}",
                call.replace("INPUT", "true")
            ),
        ] {
            let report = compile_source(&source(&text), &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
        }
        let text = format!(
            "{definition}fn main()->i32{{{};return 0}}",
            call.replace("INPUT", "0")
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generic.zil", text)
}
fn run(path: &Path, expected: i64) {
    let mut child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native generic program timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{result:?}");
    assert_eq!(result.stdout, format!("{expected}\n").as_bytes());
}
fn native(text: &str, expected: i64) -> String {
    let out = std::env::temp_dir().join(format!(
        "zil-generics-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&out).unwrap();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(out.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    run(&report.artifacts["executable"], expected);
    let optimized = out.join("optimized");
    let output = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    run(&optimized, expected);
    fs::remove_dir_all(&out).unwrap();
    llvm
}

#[test]
fn concrete_instantiations_inference_recursive_reuse_and_hygiene() {
    let llvm = native(
        "fn identity<T>(T: T)->T => T\nfn relay<T>(x:T,n:i32)->T { if n==0 {return x} return relay::<T>(x,n-1) }\nfn choose<T>(x:T,y:T,b:bool)->T {if b {return x} return y}\nfn result()->i64 {let x:i64=4000; let y:i64=23; let a=choose(x,y,true);return relay(identity(a),4)+identity::<i64>(23)}",
        4023,
    );
    assert!(llvm.contains("call i64"));
    // Repeated same-type calls share one emitted instance.
    let count = llvm
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("zil_fn_"))
        .count();
    assert!(count <= 6, "unbounded duplicate instances: {count}");
}
#[test]
fn bool_and_distinct_integer_instantiations() {
    native(
        "fn id<T>(x:T)->T=>x\nfn result()->i64 {let small=id::<i32>(7);let big=id::<i64>(2147483648);if id(true) && small==7 {return big} return 0}",
        2147483648,
    );
}
#[test]
fn generic_body_cannot_gain_operations_or_copy_from_an_instance() {
    for text in [
        "fn bad<T>(x:T)->T=>x+x\nfn result()->i32=>bad(2)",
        "fn bad<T>(x:T)->(T,T)=>(x,x)\nfn result()->i32=>1",
        "fn bad<T>(x:T)->T=>42\nfn result()->i32=>1",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn symbolic_result_and_nested_calls_do_not_collapse_distinct_parameters() {
    for text in [
        "fn bad<T,U>(x:T)->U=>x\nfn result()->i32=>bad::<i32,i32>(1)",
        "fn fixed(x:i32)->i32=>x\nfn bad<T>(x:T)->i32=>fixed(x)\nfn result()->i32=>bad(1)",
        "fn id<T>(x:T)->T=>x\nfn bad<T,U>(x:T)->U=>id::<U>(x)\nfn result()->i32=>bad::<i32,i32>(1)",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.diagnostics[0].code,
            "E-GENERIC-ARGUMENT",
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn generic_arity_constraints_and_structural_copy_bound() {
    for (index, text) in [
        "fn id<T>(x:T)->T=>x\nfn result()->i32=>id::<i32,i64>(1)",
        "fn choose<T>(x:T,y:T)->T=>x\nfn result()->i32 {let x:i32=1;let y:i64=2;return choose(x,y)}",
        "fn id<T: Copy>(x:T)->T=>x\nfn result()->i32=>1",
    ]
    .into_iter()
    .enumerate()
    {
        let report = compile_source(&source(text), &CompileOptions::default());
        if index == 2 {
            // 194 admits this exact previously unused structural Copy bound.
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
            assert_eq!(report.verification_runs, 1);
            continue;
        }
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn generic_instance_growth_is_bounded_before_type_building() {
    let source = source("fn id<T>(x:T)->T=>x\nfn result()->i32 {id::<i64>(2);return id::<i32>(1)}");
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let limits = zil_base::Limits {
        max_functions: 1,
        ..zil_base::Limits::default()
    };
    let error = zil_expand::expand_with_map_limits(&source, &parsed, &limits).unwrap_err();
    assert_eq!(error[0].phase, Phase::Expand);
    assert_eq!(error[0].code, "E-RESOURCE-LIMIT");
}
