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
    let mut low = 1;
    let mut high = limits.max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        let tight = Limits {
            max_analysis_steps: middle,
            ..limits.clone()
        };
        if zil_types::check(&core, &target, &tight).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let tight = Limits {
        max_analysis_steps: high,
        ..limits.clone()
    };
    assert_eq!(zil_types::check(&core, &target, &tight).unwrap(), typed);
    assert_eq!(
        zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: high - 1,
                ..limits.clone()
            }
        )
        .unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
    assert_eq!(zil_types::check(&core, &target, &tight).unwrap(), typed);
    eprintln!("stored source Types work N={high}; N-1 ResourceLimit; fresh N stable");
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
    let mut exported = program.clone();
    let make = exported.modules[0]
        .core
        .functions
        .iter()
        .position(|f| f.name == "make")
        .unwrap();
    let make_id = exported.modules[0].core.functions[make].id;
    exported.modules[0].exports.push(make_id);
    let first = zil_types::check_program(&exported, &target, &limits).unwrap();
    let contract = exported.modules[0].core.functions[make]
        .storage_life
        .as_mut()
        .unwrap();
    contract.invocation_binder_count += 1;
    contract.bindings.last_mut().unwrap().binder = zil_ir::StorageBinder::Invocation(1);
    let altered = zil_types::check_program(&exported, &target, &limits).unwrap();
    assert_ne!(
        first.interfaces[0].signature_fingerprint, altered.interfaces[0].signature_fingerprint,
        "an untrusted exported relation was erased from interface identity"
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

include!("../../../tests/language-readiness/terminal/181/interface_source.rs");

#[test]
fn terminal_static_function_value_keeps_qualified_input_classes() {
    use zil_driver::{CompilationSession, NativeBackend, Optimization};
    let text = "struct Tag<life a>{n:i32} fn read<life a>(v:&Tag<'a>)->i32{return v.n} fn apply<life a>(p:&'a i32)->i32{let v:Tag<'a>=Tag::<'a>{n:*p};let f=read;return f(&v)} fn result()->i32{let n=42;return apply(&n)} fn main()->i32{return result()}";
    let explicit = format!(
        "type Code=for<life a>for<life b> fn(&'a Tag<'b>)->i32;{}",
        text.replace("let f=read", "let f:Code=read")
    );
    for (label, text) in [("inferred", text), ("explicit", explicit.as_str())] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let bad = "struct Tag<life a>{n:i32} fn both<life a>(p:&Tag<'a>,q:&Tag<'a>)->i32{return p.n+q.n} fn bad<life a,life b>(p:&Tag<'a>,q:&Tag<'b>)->i32{let f=both;return f(p,q)} fn main()->i32{return 0}";
        let report = compile_source(&source(bad), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = std::env::temp_dir().join(format!(
                    "zil-static-rich-{}-{}",
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
                        backend,
                        optimization,
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let actual = report.native_codegen.as_ref().unwrap();
                assert_eq!(actual.backend, backend);
                assert!(actual.fallback.is_none());
                run(&report.artifacts["executable"], 42);
                fs::remove_dir_all(out).unwrap();
            }
        }
        let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
        let options = |steps| CompileOptions {
            limits: zil_base::Limits {
                max_analysis_steps: steps,
                ..Default::default()
            },
            ..Default::default()
        };
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_source(&source(text), &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut warm = CompilationSession::default();
        for (steps, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                warm.compile_source(&source(text), &options(steps)),
                compile_source(&source(text), &options(steps)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!(
            "static qualified pointer {label} source Types/analysis whole request N={low}/N-1/warm/fresh"
        );
    }
}

#[test]
fn terminal_static_function_formation_prepays_wide_copy_result_types() {
    use zil_base::{Limits, TargetProfile};
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let mut boundaries = Vec::new();
    for width in [4, 32] {
        let fields = (0..width)
            .map(|i| format!("n{i}:i32"))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!(
            "struct Tag<life a>{{n:i32}} struct Inner{{{fields}}} struct Wide{{left:Inner,right:Inner}} fn produce<life a>(tag:&Tag<'a>,value:Wide)->Wide{{return value}} fn apply<life a>(tag:&Tag<'a>,value:Wide)->Wide{{let code=produce;return code(tag,value)}} fn main()->i32{{return 0}}"
        );
        let unit = source(&text);
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let aliases = Default::default();
        let target = TargetProfile::default();
        let limits = Limits::default();
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
        let run = |steps| {
            zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: steps,
                    ..limits.clone()
                },
            )
        };
        let typed = run(limits.max_analysis_steps).unwrap();
        assert!(typed.functions.iter().flat_map(|f|&f.values).any(|ty|matches!(ty,zil_ir::Type::BorrowedFunctionPointer{result,..} if matches!(result.as_ref(),zil_ir::Type::Record{fields,..} if fields.len()==2))));
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert_eq!(
                        errors[0].category,
                        ResultCategory::ResourceLimit,
                        "{errors:?}"
                    );
                    low = middle + 1
                }
            }
        }
        let exact = run(low).unwrap();
        assert_eq!(exact, typed);
        assert_eq!(
            run(low - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        assert_eq!(run(low).unwrap(), typed);
        eprintln!("static function wide Copy result width={width} Types N={low}/N-1/fresh");
        boundaries.push(low);
    }
    assert!(boundaries[1] > boundaries[0]);
}

#[test]
fn terminal_explicit_stored_closure_annotations_require_their_context() {
    let closed = "struct Tag<life a>{n:i32} type Code=for<life a>for<life b>fn(&'a Tag<'b>)->i32;fn read<life a>(v:&Tag<'a>)->i32{return v.n}fn main()->i32{let callback=(code:Code)->i32=>42;return callback(read)}";
    let report = compile_source(&source(closed), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );

    terminal_native_profiles(closed, "main", 42);
    for body in [
        "let f=(p:&Tag<'s>)->i32=>p.n;return f(&v)",
        "let f=()->Tag<'s> => v;return f().n",
    ] {
        let text = format!(
            "struct Tag<life s>{{n:i32}} fn apply<life s>(x:&'s i32)->i32{{let v:Tag<'s>=Tag::<'s>{{n:*x}};{body}}} fn main()->i32{{let n=42;return apply(&n)}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
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
                .any(|d| d.message.contains("retained enclosing life context")
                    || d.message.contains("genuine enclosing definition context")),
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn terminal_qualified_callable_helper_retains_its_own_input_relation() {
    for body in ["42", "p.n+*q"] {
        let text = format!(
            "struct Tag<life s>{{n:i32}} fn invoke<life s,F:for<life a>for<life b>Fn(&'a Tag<'b>,&'b i32)->i32>(f:&F,p:&Tag<'s>,q:&'s i32)->i32{{return f(p,q)}} fn apply<life s>(x:&'s i32)->i32{{let v:Tag<'s>=Tag::<'s>{{n:*x}};let f=(p,q)=>{body};return invoke(&f,&v,x)}} fn result()->i32{{let n=21;return apply(&n)}} fn main()->i32{{return result()}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        terminal_native_profiles(&text, "result", 42);
        if body == "p.n+*q" {
            let options = |steps| CompileOptions {
                stop_after: Stage::Lower,
                limits: zil_base::Limits {
                    max_analysis_steps: steps,
                    ..Default::default()
                },
                ..Default::default()
            };
            let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
            while low < high {
                let mid = low + (high - low) / 2;
                let report = compile_source(&source(&text), &options(mid));
                match report.status {
                    ResultCategory::Success => high = mid,
                    ResultCategory::ResourceLimit => low = mid + 1,
                    _ => panic!("{}", report.to_json()),
                }
            }
            let mut warm = zil_driver::CompilationSession::default();
            for (steps, expected) in [
                (low, ResultCategory::Success),
                (low - 1, ResultCategory::ResourceLimit),
                (low, ResultCategory::Success),
            ] {
                for report in [
                    warm.compile_source(&source(&text), &options(steps)),
                    compile_source(&source(&text), &options(steps)),
                ] {
                    assert_eq!(report.status, expected, "{}", report.to_json());
                }
                assert_eq!(warm.active_query_count(), 0);
            }
            eprintln!(
                "qualified life/F helper actual read Source-through-Lower whole request N={low}/N-1/warm/fresh"
            );
        }
    }
    let unused = "struct Tag<life s>{n:i32} fn invoke<life s,life t,F:for<life a>for<life b>Fn(&'a Tag<'b>,&'b i32)->i32>(f:&F,p:&Tag<'s>,q:&'t i32)->i32{return f(p,q)} fn main()->i32{return 0}";
    let report = compile_source(&source(unused), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d
            .message
            .contains("cannot equate distinct declared lifetime classes")),
        "{}",
        report.to_json()
    );
}

fn terminal_native_profiles(text: &str, entry: &str, expected: i64) {
    use zil_driver::{NativeBackend, Optimization};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = std::env::temp_dir().join(format!(
                "zil-rich-role-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&out).unwrap();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    entry: Some(entry.into()),
                    output: Some(out.join("program")),
                    backend,
                    optimization,
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let actual = report.native_codegen.as_ref().unwrap();
            assert_eq!(actual.backend, backend);
            assert!(actual.fallback.is_none());
            run(&report.artifacts["executable"], expected);
            fs::remove_dir_all(out).unwrap();
        }
    }
}

#[test]
fn g3_3_qualified_inline_environment_modes() {
    for (kind, passing, argument, closure, body) in [
        (
            "Fn",
            "&F",
            "&f",
            "let marker=1;let f=move (p,q)=>p.n+*q+marker",
            "invoke(&f,&v,x)-1",
        ),
        (
            "FnMut",
            "&mut F",
            "&mut f",
            "var marker=0;var f=move (p,q)=>{marker=marker+1;return p.n+*q+marker}",
            "invoke(&mut f,&v,x)+invoke(&mut f,&v,x)-45",
        ),
        (
            "FnOnce",
            "F",
            "f",
            "var marker=0;let f=move (p,q)=>{marker=marker+1;return p.n+*q+marker}",
            "invoke(f,&v,x)-1",
        ),
    ] {
        let text = format!(
            "struct Tag<life s>{{n:i32}} fn invoke<life s,F:for<life a>for<life b>{kind}(&'a Tag<'b>,&'b i32)->i32>(f:{passing},p:&Tag<'s>,q:&'s i32)->i32{{return f(p,q)}} fn apply<life s>(x:&'s i32)->i32{{let v:Tag<'s>=Tag::<'s>{{n:*x}};{closure};return {body}}} fn result()->i32{{let n=21;return apply(&n)}} fn main()->i32{{return result()}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{kind} {argument}: {}",
            report.to_json()
        );
        terminal_native_profiles(&text, "result", 42);
        if kind == "FnMut" {
            // The environment is exclusive; its two shared input leaves may
            // still name the same actual n. They do not inherit receiver mode.
            let shared = text
                .replace("Tag<life s>{n:i32}", "Tag<life s>{p:&'s i32}")
                .replace("Tag::<'s>{n:*x}", "Tag::<'s>{p:x}")
                .replace("p.n+*q", "*p.p+*q");
            let report = compile_source(&source(&shared), &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            terminal_native_profiles(&shared, "result", 42);
        }
    }
}

#[test]
fn g3_3_qualified_environment_raw_and_wire_neighbors() {
    use zil_base::{Limits, TargetProfile};
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_ir::{CallableKind, InstructionKind, Type};
    let text = "struct Tag<life s>{n:i32} fn invoke<life s,F:for<life a>for<life b>FnMut(&'a Tag<'b>,&'b i32)->i32>(f:&mut F,p:&Tag<'s>,q:&'s i32)->i32{return f(p,q)} fn apply<life s>(x:&'s i32)->i32{let v:Tag<'s>=Tag::<'s>{n:*x};var marker=0;var f=move (p,q)=>{marker=marker+1;return p.n+*q+marker};return invoke(&mut f,&v,x)-1} fn main()->i32{let n=21;return apply(&n)}";
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
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
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    // Whole request thresholds include the real environment and role walks.
    for phase in ["Analyze", "Verify", "Source-through-Lower"] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            match phase {
                "Analyze" => zil_analyze::analyze(&typed, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                "Verify" => {
                    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
                        .map(|_| ())
                        .map_err(|e| e[0].category)
                }
                _ => {
                    let report = compile_source(
                        &source(text),
                        &CompileOptions {
                            stop_after: Stage::Lower,
                            limits,
                            ..Default::default()
                        },
                    );
                    if report.status == ResultCategory::Success {
                        Ok(())
                    } else {
                        Err(report.status)
                    }
                }
            }
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(()) => high = mid,
                Err(ResultCategory::ResourceLimit) => low = mid + 1,
                Err(error) => panic!("{phase} unexpected boundary result {error:?}"),
            }
        }
        assert_eq!(run(low), Ok(()));
        assert_eq!(run(low - 1), Err(ResultCategory::ResourceLimit));
        assert_eq!(run(low), Ok(()));
        if phase == "Source-through-Lower" {
            let mut session = zil_driver::CompilationSession::default();
            for (steps, expected) in [
                (low, ResultCategory::Success),
                (low - 1, ResultCategory::ResourceLimit),
                (low, ResultCategory::Success),
            ] {
                let report = session.compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Lower,
                        limits: Limits {
                            max_analysis_steps: steps,
                            ..Limits::default()
                        },
                        ..Default::default()
                    },
                );
                assert_eq!(report.status, expected, "{}", report.to_json());
                assert_eq!(session.active_query_count(), 0);
            }
        }
        eprintln!("G3.3 qualified scalar FnMut {phase} whole request N={low}/N-1/fresh");
    }
    assert_eq!(typed.closures.len(), 1);
    assert_eq!(typed.closures[0].captures, vec![Type::I32]);
    assert_eq!(typed.closures[0].kind, CallableKind::FnMut);
    assert!(
        typed
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::WriteCapture { .. }))
    );
    for mutation in 0..5 {
        let mut wrong = typed.clone();
        match mutation {
            0 => wrong.closures[0].kind = CallableKind::Fn,
            1 => wrong.closures[0].captures[0] = Type::I64,
            2 => {
                let i = wrong
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
                    .unwrap();
                let InstructionKind::MakeClosure { captures, .. } = &mut i.kind else {
                    unreachable!()
                };
                captures.clear();
            }
            3 => {
                let i = wrong
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::ReadCapture { .. }))
                    .unwrap();
                let InstructionKind::ReadCapture { index } = &mut i.kind else {
                    unreachable!()
                };
                *index = 1;
            }
            4 => {
                let role = wrong
                    .functions
                    .iter_mut()
                    .filter_map(|f| f.callable_contract.as_mut())
                    .flat_map(|c| &mut c.roles)
                    .find(|r| r.life.is_some())
                    .unwrap();
                role.life.as_mut().unwrap().classes[0].binder =
                    zil_ir::StorageBinder::Invocation(0);
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&wrong, &limits).is_err(),
            "Analyze accepted mutation {mutation}"
        );
        let mut candidate = plan.clone();
        candidate.module_fingerprint = wrong.fingerprint();
        assert!(
            zil_verify::verify_module(wrong.clone(), candidate.clone(), &target, &limits).is_err(),
            "Verify accepted mutation {mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&wrong, &candidate, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire Verify accepted mutation {mutation}"
        );
    }
    for (before, after) in [
        ("var f=move", "let f=move"),
        ("f:&mut F", "f:&F"),
        ("FnMut(", "FnOnce("),
    ] {
        let report = compile_source(
            &source(&text.replace(before, after)),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
    let unused = "struct Tag<life s>{n:i32} fn bad<life s,F:for<life a>for<life b>FnOnce(&'a Tag<'b>,&'b i32)->i32>(f:F,p:&Tag<'s>,q:&'s i32)->i32{let first=f(p,q);return f(p,q)} fn main()->i32{return 0}";
    let report = compile_source(&source(unused), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
}

#[test]
fn g3_3_once_consumption_is_proved_after_source_in_raw_and_wire() {
    use zil_base::{Limits, TargetProfile};
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_ir::{CallableKind, InstructionKind, Type, ValueId};
    let text = "struct Tag<life s>{n:i32} fn invoke<life s,F:for<life a>for<life b>FnOnce(&'a Tag<'b>,&'b i32)->i32>(f:F,p:&Tag<'s>,q:&'s i32)->i32{return f(p,q)} fn apply<life s>(x:&'s i32)->i32{let v:Tag<'s>=Tag::<'s>{n:*x};var marker=0;let f=move (p,q)=>{marker=marker+1;return p.n+*q+marker};return invoke(f,&v,x)-1} fn main()->i32{let n=21;return apply(&n)}";
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let (limits, target, aliases) = (
        Limits::default(),
        TargetProfile::default(),
        Default::default(),
    );
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
    let core = zil_types::infer_source_closures(&core, &limits).unwrap();
    let adapted = zil_types::check(&core, &target, &limits).unwrap();
    assert_eq!(adapted.closures[0].kind, CallableKind::FnMut);
    let check = |module: &zil_ir::TypedModule| {
        let plan = zil_analyze::analyze(module, &limits).unwrap();
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        plan
    };
    let adapted_plan = check(&adapted);
    // The same real FnMut environment is consumed through its declared Once
    // parameter. Permit the claimed trace to match the forged two actual calls;
    // the independent per-role allowance still must reject it.
    let mut twice = adapted.clone();
    let helper = twice
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    let new_value = ValueId(helper.values.len() as u32);
    helper.values.push(Type::I32);
    let original = helper
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
        .unwrap()
        .clone();
    let mut duplicate = original;
    duplicate.result = Some(new_value);
    helper.blocks[0].instructions.push(duplicate);
    let zil_ir::CallableTraces::Finite(traces) =
        &mut helper.callable_contract.as_mut().unwrap().traces
    else {
        panic!("expected finite source helper")
    };
    for trace in traces {
        trace.calls.push(trace.calls[0]);
    }
    let helper_index = helper.id.0 as usize;
    let mut candidate = adapted_plan.clone();
    candidate.module_fingerprint = twice.fingerprint();
    candidate.functions[helper_index]
        .holds
        .push(zil_ir::ValueHold {
            value: new_value,
            roots: vec![],
        });
    assert!(zil_analyze::analyze(&twice, &limits).is_err());
    assert!(zil_verify::verify_module(twice.clone(), candidate.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&twice, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());

    // A concrete Once definition needs a responsibility ledger even when its
    // actual inline fields are scalars. Derive it from this real source entry;
    // use TakeLocal for its now non-Copy environments and call the actual entry
    // directly at the former helper site, so no helper role can mask the test.
    let mut actual = adapted.clone();
    actual.closures[0].kind = CallableKind::FnOnce;
    let helper_id = actual
        .functions
        .iter()
        .find(|f| f.callable_contract.is_some())
        .unwrap()
        .id;
    let mut direct_site = None;
    for f in &mut actual.functions {
        for block in &mut f.blocks {
            for i in &mut block.instructions {
                if let InstructionKind::Load(local) = i.kind
                    && matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                {
                    if f.callable_contract.is_some() {
                        // The source callee pre-evaluation Load is unused;
                        // the actual Once call already has its own Take.
                        i.kind = InstructionKind::Constant(zil_ir::Constant::I32(0));
                        f.values[i.result.unwrap().0 as usize] = Type::I32;
                    } else {
                        i.kind = InstructionKind::TakeLocal { local };
                    }
                }
                if let InstructionKind::Call {
                    function,
                    arguments,
                } = &i.kind
                    && *function == helper_id
                {
                    i.kind = InstructionKind::CallClosureOnce {
                        callee: arguments[0],
                        arguments: arguments[1..].to_vec(),
                    };
                    direct_site = Some((f.id, block.id));
                }
            }
        }
    }
    let actual_plan = check(&actual);
    let (function, block) = direct_site.unwrap();
    assert!(
        actual.functions[function.0 as usize]
            .callable_contract
            .is_none()
    );
    for mutation in ["same SSA consumed twice", "non-Copy Load"] {
        let mut wrong = actual.clone();
        let f = &mut wrong.functions[function.0 as usize];
        let mut candidate = actual_plan.clone();
        if mutation == "same SSA consumed twice" {
            let mut copy = f.blocks[block.0 as usize]
                .instructions
                .iter()
                .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                .unwrap()
                .clone();
            let v = ValueId(f.values.len() as u32);
            f.values.push(Type::I32);
            copy.result = Some(v);
            let position = f.blocks[block.0 as usize]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                .unwrap();
            f.blocks[block.0 as usize]
                .instructions
                .insert(position + 1, copy);
            candidate.functions[function.0 as usize]
                .holds
                .push(zil_ir::ValueHold {
                    value: v,
                    roots: vec![],
                });
        } else {
            let i=f.blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|matches!(i.kind,InstructionKind::TakeLocal{local}if matches!(f.locals[local.0 as usize].ty,Type::Closure(_)))).unwrap();
            let InstructionKind::TakeLocal { local } = i.kind else {
                unreachable!()
            };
            i.kind = InstructionKind::Load(local);
        }
        candidate.module_fingerprint = wrong.fingerprint();
        let errors = zil_analyze::analyze(&wrong, &limits).unwrap_err();
        eprintln!("G3.3 actual Once {mutation}: Analyze {}", errors[0].message);
        let reason = if mutation == "same SSA consumed twice" {
            "call uses consumed environment"
        } else {
            "Load cannot copy an owned carrier"
        };
        assert!(
            errors.iter().any(|e| e.message.contains(reason)),
            "{errors:?}"
        );
        let verified =
            zil_verify::verify_module(wrong.clone(), candidate.clone(), &target, &limits)
                .unwrap_err();
        eprintln!(
            "G3.3 actual Once {mutation}: Verify {}",
            verified[0].message
        );
        let bytes = zil_ir::wire::encode_module_candidate(&wrong, &candidate, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        assert_eq!(decoded[0].code, verified[0].code);
        assert_eq!(decoded[0].message, verified[0].message);
    }
}

#[test]
fn g3_3_1_source_annotations_preserve_actual_ambient_classes() {
    let constructors = [
        "Phantom::<'CLASS>{n:42}",
        "#(Phantom::<'CLASS>{n:scalar(i32,42)})",
        "#((Phantom::<'a>{n:scalar(i32,0)},Phantom::<'CLASS>{n:scalar(i32,42)}).1)",
        "#(#{yield Phantom::<'CLASS>{n:scalar(i32,42)}})",
    ];
    for constructor in constructors {
        for intermediate in [false, true] {
            for actual in ["a", "b"] {
                let init = constructor.replace("CLASS", actual);
                let binding = if intermediate {
                    format!("let saved={init};let x:Phantom<'a>=saved;")
                } else {
                    format!("let x:Phantom<'a>={init};")
                };
                let text = format!(
                    "struct Phantom<life s>{{n:i32}}fn scalar(t:Type,n:i32)->i32{{return n}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{{{binding}return x.n}}fn main()->i32{{let a=0;let b=0;return value(&a,&b)}}"
                );
                let report = compile_source(&source(&text), &CompileOptions::default());
                if actual == "a" {
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    terminal_native_profiles(&text, "main", 42);
                } else {
                    assert_eq!(
                        report.status,
                        ResultCategory::SafeReject,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
                    assert!(
                        report.diagnostics.iter().any(|d| d.message.contains(
                            "source annotation cannot equate distinct declared lifetime classes"
                        )),
                        "{}",
                        report.to_json()
                    );
                }
            }
        }
    }
    // The outer reference relation is not nominal class equality. Preserve a
    // genuine borrow return through a helper, along with a class-bearing helper.
    let compatible = "struct Phantom<life s>{n:i32}fn borrow<life x>(p:&'x i32)->&'x i32{return p}fn make<life x>(p:&'x i32)->Phantom<'x>{return Phantom::<'x>{n:*p}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{let x:Phantom<'a>=make(borrow(pa));return x.n}fn main()->i32{let a=42;let b=0;return value(&a,&b)}";
    let report = compile_source(&source(compatible), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    terminal_native_profiles(compatible, "main", 42);
    let wrong = compatible.replace("make(borrow(pa))", "make(borrow(pb))");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
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
            .any(|d| d.message.contains("distinct declared lifetime classes")),
        "{}",
        report.to_json()
    );
}

#[test]
fn g3_3_1_aliases_nested_fields_assignments_and_unused_methods_keep_classes() {
    let prefix = "struct Phantom<life s>{n:i32}type Id<T>=T;struct Reader{}";
    for (good, bad, executable) in [
        (
            "let x:Id<Phantom<'a>>=Phantom::<'a>{n:42};return x.n",
            "let x:Id<Phantom<'a>>=Phantom::<'b>{n:42};return x.n",
            true,
        ),
        (
            "let values=[Phantom::<'a>{n:42}];let x:Phantom<'a>=values[0];return x.n",
            "let values=[Phantom::<'b>{n:42}];let x:Phantom<'a>=values[0];return x.n",
            false,
        ),
        (
            "var x=Phantom::<'a>{n:0};x=Phantom::<'a>{n:42};return x.n",
            "var x=Phantom::<'a>{n:0};x=Phantom::<'b>{n:42};return x.n",
            true,
        ),
        (
            "let pair=(0,Phantom::<'a>{n:42});let (_,saved)=pair;let x:Phantom<'a>=saved;return x.n",
            "let pair=(0,Phantom::<'b>{n:42});let (_,saved)=pair;let x:Phantom<'a>=saved;return x.n",
            false,
        ),
    ] {
        for (body, expected) in [
            (good, ResultCategory::Success),
            (bad, ResultCategory::SafeReject),
        ] {
            let text = format!(
                "{prefix}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{{{body}}}fn main()->i32{{let a=0;let b=0;return value(&a,&b)}}"
            );
            let report = compile_source(
                &source(&text),
                &CompileOptions {
                    stop_after: if executable {
                        Stage::Verify
                    } else {
                        Stage::Core
                    },
                    ..Default::default()
                },
            );
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::Success && executable {
                terminal_native_profiles(&text, "main", 42);
            }
            if expected == ResultCategory::Success && !executable {
                let full = compile_source(&source(&text), &CompileOptions::default());
                assert_eq!(
                    full.status,
                    ResultCategory::Unsupported,
                    "{}",
                    full.to_json()
                );
                assert_eq!(full.phase, Phase::Types, "{}", full.to_json());
            }
            if expected == ResultCategory::SafeReject {
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .any(|d| d.message.contains("distinct declared lifetime classes")),
                    "{}",
                    report.to_json()
                );
            }
        }
    }
    for host in ["struct Reader", "struct Reader<life outer>"] {
        for actual in ["a", "b"] {
            let text = format!(
                "struct Phantom<life s>{{n:i32}}{host}{{fn read<life a,life b>(self:Self,pa:&'a i32,pb:&'b i32)->i32{{let x:Phantom<'a>=Phantom::<'{actual}>{{n:42}};return x.n}}}}fn main()->i32{{return 0}}"
            );
            let report = compile_source(&source(&text), &CompileOptions::default());
            if actual == "b" {
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
                        .any(|d| d.message.contains("distinct declared lifetime classes")),
                    "{}",
                    report.to_json()
                );
            } else if host.contains('<') {
                // Existing generic-host method emission is a separate consumer
                // fence; the class check itself must not reject equal classes.
                assert_eq!(
                    report.status,
                    ResultCategory::Unsupported,
                    "{}",
                    report.to_json()
                );
                assert!(
                    report.diagnostics.iter().any(|d| d
                        .message
                        .contains("generic resource or methods require capability resolution")),
                    "{}",
                    report.to_json()
                );
            } else {
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
            }
        }
    }
    let reborrow = "struct Phantom<life s>{n:i32}fn make<life x>(p:&'x i32)->Phantom<'x>{return Phantom::<'x>{n:*p}}fn both<life x>(p:&'x i32,q:&'x i32)->i32{return *p+*q}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{let x:Phantom<'a>=make(&*pa);return x.n+both(pa,pb)}fn main()->i32{let a=21;let b=0;return value(&a,&b)}";
    let report = compile_source(&source(reborrow), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    terminal_native_profiles(reborrow, "main", 42);
}

#[test]
fn g3_3_1_full_nominal_ordinals_and_enclosing_method_life_are_not_class_zero() {
    for actual in ["a", "b"] {
        let text = format!(
            "struct Phantom<T,life s>{{n:i32}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{{let x:Phantom<i32,'a>=Phantom::<i32,'{actual}>{{n:42}};return x.n}}fn main()->i32{{let a=0;let b=0;return value(&a,&b)}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        if actual == "a" {
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            terminal_native_profiles(&text, "main", 42);
        } else {
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
                    .any(|d| d.message.contains("distinct declared lifetime classes")),
                "{}",
                report.to_json()
            );
        }
    }
    let wrong = "struct Phantom<life s>{n:i32}struct Reader<life outer>{fn bad<life inner>(self:Self,p:&'inner i32)->i32{let x:Phantom<'outer>=Phantom::<'inner>{n:42};return x.n}}fn main()->i32{return 0}";
    let report = compile_source(&source(wrong), &CompileOptions::default());
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
            .any(|d| d.message.contains("distinct declared lifetime classes")),
        "{}",
        report.to_json()
    );
}

#[test]
fn g3_3_1_borrow_intersection_keeps_unknown_class_without_choosing_an_upper() {
    let text = "struct Phantom<life s>{n:i32}fn make<life x>(p:&'x i32,q:&'x i32)->Phantom<'x>{return Phantom::<'x>{n:*p+*q}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{let x=make(pa,pb);return x.n}fn main()->i32{let a=20;let b=22;return value(&a,&b)}";
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    terminal_native_profiles(text, "main", 42);
    for class in ["a", "b"] {
        let annotated = text.replace("let x=", &format!("let x:Phantom<'{class}>="));
        let report = compile_source(&source(&annotated), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
    }
}

#[test]
fn g3_3_1_flat_fact_growth_is_bounded_on_the_default_worker() {
    const CHILD: &str = "ZIL_G3_3_1_FACT_DEFAULT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Each expression is shallow. Only inferred source facts become deep.
        let mut text = "struct Phantom<life s>{n:i32}struct Wrap<T:Copy>{item:T}fn wrap<T:Copy>(x:T)->Wrap<T>{return Wrap::<T>{item:x}}fn value<life a>(p:&'a i32)->i32{let saved=Phantom::<'a>{n:42};".to_owned();
        for _ in 0..512 {
            text.push_str("let saved=wrap(saved);");
        }
        text.push_str("return 0}fn main()->i32{let n=0;return value(&n)}");
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::ResourceLimit,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        return;
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "g3_3_1_flat_fact_growth_is_bounded_on_the_default_worker",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_remove("RUST_MIN_STACK")
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "default worker subprocess: {:?}\n{}\n{}",
        child.status,
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
fn g3_3_1_source_class_work_has_exact_fresh_and_warm_limits() {
    for (label, init) in [
        ("ordinary", "Phantom::<'a>{n:42}"),
        (
            "CT-projection",
            "#((Phantom::<'b>{n:scalar(i32,0)},Phantom::<'a>{n:scalar(i32,42)}).1)",
        ),
    ] {
        let text = format!(
            "struct Phantom<life s>{{n:i32}}fn scalar(t:Type,n:i32)->i32{{return n}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{{let saved={init};let x:Phantom<'a>=saved;return x.n}}fn main()->i32{{let a=0;let b=0;return value(&a,&b)}}"
        );
        let options = |steps| CompileOptions {
            stop_after: Stage::Lower,
            limits: zil_base::Limits {
                max_analysis_steps: steps,
                ..Default::default()
            },
            ..Default::default()
        };
        let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_source(&source(&text), &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = zil_driver::CompilationSession::default();
        for (steps, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                compile_source(&source(&text), &options(steps)),
                session.compile_source(&source(&text), &options(steps)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
            }
            assert_eq!(session.active_query_count(), 0);
        }
        eprintln!("G3.3.1 {label} source-through-Lower N={low}/N-1/fresh/warm");
    }
}

#[test]
fn g3_3_1_repeated_host_headers_prepay_each_method_before_the_existing_fence() {
    let mut boundaries = Vec::new();
    for width in [16, 64] {
        let names = (0..width)
            .map(|n| format!("life h{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let methods = (0..8)
            .map(|n| format!("fn inspect{n}(self:Self)->i32{{return 0}}"))
            .collect::<String>();
        let text = format!("struct Reader<{names}>{{{methods}}}fn main()->i32{{return 42}}");
        let options = |steps| CompileOptions {
            limits: zil_base::Limits {
                max_analysis_steps: steps,
                ..Default::default()
            },
            ..Default::default()
        };
        let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_source(&source(&text), &options(mid));
            match report.status {
                ResultCategory::Unsupported => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = zil_driver::CompilationSession::default();
        for (steps, expected) in [
            (low, ResultCategory::Unsupported),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Unsupported),
        ] {
            for report in [
                compile_source(&source(&text), &options(steps)),
                session.compile_source(&source(&text), &options(steps)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                assert_eq!(report.phase, Phase::Expand);
                if expected == ResultCategory::Unsupported {
                    assert!(
                        report.diagnostics.iter().any(|d| d
                            .message
                            .contains("generic resource or methods require capability resolution")),
                        "{}",
                        report.to_json()
                    );
                }
            }
            assert_eq!(session.active_query_count(), 0);
        }
        eprintln!(
            "G3.3.1 inherited host width={width}, methods=8 request N={low}/N-1/fresh/warm into existing Unsupported fence"
        );
        boundaries.push(low);
    }
    assert!(boundaries[1] > boundaries[0]);
}

#[test]
fn g10_1_8_nested_source_classes_keep_default_stack_and_compatibility() {
    const CHILD: &str = "ZIL_G10_1_8_CLASS_CHILD";
    if let Ok(actual) = std::env::var(CHILD) {
        std::thread::spawn(move || {
            for depth in [32, 96] {
                let initializer = format!("{}Phantom::<'{actual}>{{n:42}}{}", "(".repeat(depth), ")".repeat(depth));
                let text = format!("struct Phantom<life s>{{n:i32}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{{let x:Phantom<'a>={initializer};return x.n}}fn main()->i32{{let a=0;let b=0;return value(&a,&b)}}");
                let report = compile_source(&source(&text), &CompileOptions::default());
                if actual == "a" {
                    assert_eq!(report.status, ResultCategory::Success, "{depth}: {}", report.to_json());
                    assert_eq!(report.verification_runs, 1);
                } else {
                    assert_eq!(report.status, ResultCategory::SafeReject, "{depth}: {}", report.to_json());
                    assert_eq!(report.phase, Phase::Expand);
                    assert!(report.diagnostics.iter().any(|d| d.message.contains("source annotation cannot equate distinct declared lifetime classes")), "{}", report.to_json());
                }
            }
            for left in [false, true] {
                let payload = format!("((()->i32=>{{let x:Phantom<'a>=Phantom::<'{actual}>{{n:41}};return x.n}})())");
                let binary = if left { format!("{payload}+1") } else { format!("1+{payload}") };
                let text = format!("struct Phantom<life s>{{n:i32}}fn value<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{{return {binary}}}fn main()->i32{{let a=0;let b=0;return value(&a,&b)}}");
                let report = compile_source(&source(&text), &CompileOptions::default());
                if actual == "a" {
                    assert_eq!(report.status, ResultCategory::Success, "binary left={left}: {}", report.to_json());
                    assert_eq!(report.verification_runs, 1);
                } else {
                    assert_eq!(report.status, ResultCategory::SafeReject, "binary left={left}: {}", report.to_json());
                    assert_eq!(report.phase, Phase::Expand);
                    assert!(report.diagnostics.iter().any(|d| d.message.contains("source annotation cannot equate distinct declared lifetime classes")), "{}", report.to_json());
                }
            }
        }).join().unwrap();
        return;
    }
    for actual in ["a", "b"] {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "g10_1_8_nested_source_classes_keep_default_stack_and_compatibility",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("RUST_MIN_STACK")
            .env_remove("LD_PRELOAD")
            .env(CHILD, actual)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{actual}: {} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

fn g3_3_2_source_core(text: &str) -> zil_ir::CoreModule {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand_with_map_limits_target_context(
        &unit,
        &syntax,
        &zil_base::Limits::default(),
        &zil_base::TargetProfile::host_llvm(),
        SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &Default::default(),
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core
}

#[test]
fn g3_3_2_actual_source_forms_complete_capture_relations_in_three_modes() {
    use zil_ir::{CallableKind, InstructionKind, StorageBase, StorageBinder, Type};
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let cases = [
        (
            CallableKind::Fn,
            0,
            include_str!(
                "../../../tests/language-readiness/terminal/342/fixed-context-producer/fn-input-capture.zil"
            ),
        ),
        (
            CallableKind::FnMut,
            1,
            include_str!(
                "../../../tests/language-readiness/terminal/342/fixed-context-producer/fnmut-move-input-capture.zil"
            ),
        ),
        (
            CallableKind::FnOnce,
            1,
            include_str!(
                "../../../tests/language-readiness/terminal/342/fixed-context-producer/fnonce-input-capture.zil"
            ),
        ),
    ];
    for (kind, tag, text) in cases {
        let core = g3_3_2_source_core(text);
        assert_eq!(core.closure_life_contexts.len(), 1);
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let d = &typed.closures[0];
        assert_eq!(d.kind, kind);
        assert!(
            matches!(&d.captures[tag], Type::Address { mutable: false, inner } if matches!(inner.as_ref(), Type::BorrowedStorage { .. }))
        );
        let c = d.storage_life.as_ref().unwrap();
        assert_eq!((c.fixed_binder_count, c.invocation_binder_count), (2, 1));
        assert_eq!(c.bindings[0].position.base, StorageBase::Parameter(0));
        assert_eq!(c.bindings[0].binder, StorageBinder::Invocation(0));
        assert_eq!(
            c.bindings[1].position.base,
            StorageBase::Capture(tag as u32)
        );
        assert_eq!(c.bindings[1].binder, StorageBinder::Fixed(1));
        assert_eq!(c.classes.len(), 2);
        assert_eq!(c.classes[0].binder, StorageBinder::Fixed(0));
        assert_eq!(c.classes[1].position.base, StorageBase::Capture(tag as u32));
        assert_eq!(c.classes[1].binder, StorageBinder::Fixed(0));
        let entry = &typed.functions[d.function.0 as usize];
        assert_eq!(entry.storage_life.as_ref(), Some(c));
        if kind == CallableKind::FnMut {
            assert_eq!(d.captures[0], Type::I32);
            assert!(
                entry
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::WriteCapture { index: 0, .. }))
            );
        }
        if kind == CallableKind::FnOnce {
            assert!(matches!(d.captures[0], Type::Managed(_)));
            assert!(
                entry
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::TakeCapture { index: 0 }))
            );
        }
        // Original generic ordinal is independent from the final Fixed index.
        let changed = text.replace("apply<life s>", "apply<life unused,life s>");
        let core = g3_3_2_source_core(&changed);
        assert_eq!(core.closure_life_contexts[0].named_fixed[0].parameter, 1);
        let changed = zil_types::check(&core, &target, &limits).unwrap();
        assert_eq!(changed.closures[0].storage_life.as_ref(), Some(c));
    }
    let original_mut = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fnmut-input-capture.zil"
    );
    let error = zil_types::check(&g3_3_2_source_core(original_mut), &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported)
    );
}

#[test]
fn g3_3_2_raw_source_context_rows_are_consumed_from_actual_creators() {
    use zil_ir::{CoreClosureHeaderPosition, StorageBinder};
    let text = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fn-input-capture.zil"
    );
    let core = g3_3_2_source_core(text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for mutation in 0..9 {
        let mut bad = core.clone();
        match mutation {
            0 => bad
                .closure_life_contexts
                .push(bad.closure_life_contexts[0].clone()),
            1 => bad.closure_life_contexts.clear(),
            2 => bad.closure_life_contexts[0].creator.0 = u32::MAX,
            3 => bad.closure_life_contexts[0].expression.0 = u32::MAX,
            4 => bad.closure_life_contexts[0].named_fixed[0].parameter = u32::MAX,
            5 => bad.closure_life_contexts[0].inputs[0].header_binder = StorageBinder::Fixed(999),
            6 => {
                let mut extra = bad.closure_life_contexts[0].clone();
                extra.expression.0 = u32::MAX;
                bad.closure_life_contexts.push(extra);
            }
            7 => {
                let duplicate = bad.closure_life_contexts[0].inputs[0].clone();
                bad.closure_life_contexts[0].inputs.push(duplicate);
            }
            _ => {
                let input = &mut bad.closure_life_contexts[0].inputs[0];
                match &mut input.position {
                    CoreClosureHeaderPosition::Borrow(p) => p.base = zil_ir::StorageBase::Result,
                    CoreClosureHeaderPosition::Class(p) => p.base = zil_ir::StorageBase::Result,
                }
            }
        }
        let error = zil_types::check(&bad, &target, &limits).unwrap_err();
        assert!(
            error
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{mutation}: {error:?}"
        );
        eprintln!(
            "G3.3.2 source-derived malformed context {mutation}: {}",
            error[0].message
        );
    }
    let apply = core
        .functions
        .iter()
        .find(|f| f.id == core.closure_life_contexts[0].creator)
        .unwrap();
    let mut unused = core.clone();
    unused.functions.retain(|f| f.id == apply.id); // definition is never invoked
    zil_types::check(&unused, &target, &limits).unwrap();
    // Same diagnostic spans do not merge distinct real declaration/expression IDs.
    let extra = text
        .split_once("fn apply")
        .unwrap()
        .1
        .split_once("fn setup")
        .unwrap()
        .0;
    let mut two = g3_3_2_source_core(&format!("{text}fn other{extra}"));
    assert_eq!(two.closure_life_contexts.len(), 2);
    for f in &mut two.functions {
        f.span = zil_base::Span::default();
    }
    let typed = zil_types::check(&two, &target, &limits).unwrap();
    assert_eq!(typed.closures.len(), 2);
    assert_ne!(typed.closures[0].function, typed.closures[1].function);
}

#[test]
fn g3_3_2_context_clone_preflight_and_warm_scalar_orphan_stay_closed() {
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let text = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fn-input-capture.zil"
    );
    let core = g3_3_2_source_core(text);
    for mutation in 0..3 {
        let mut huge = core.clone();
        match mutation {
            0 => huge.closure_life_contexts[0].identity.package = "x".repeat(4096),
            1 => huge.closure_life_contexts[0].signature.fixed_binder_count = u32::MAX,
            _ => {
                huge.closure_life_contexts[0].inputs[0].position =
                    zil_ir::CoreClosureHeaderPosition::Borrow(zil_ir::StoragePosition {
                        base: zil_ir::StorageBase::Parameter(0),
                        path: vec![
                            zil_ir::ContractProjection::Place(
                                zil_ir::PlaceProjection::Field(0)
                            );
                            129
                        ],
                    })
            }
        }
        let tiny = zil_base::Limits {
            max_analysis_steps: 128,
            ..limits.clone()
        };
        let error = zil_types::infer_source_closures(&huge, &tiny).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{mutation}: {error:?}"
        );
    }
    let scalar = g3_3_2_source_core("fn main()->i32{return 42}");
    let mut program = zil_ir::CoreProgram {
        root: scalar.module_id,
        source_fingerprint: scalar.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core: scalar,
            imports: vec![],
            exports: vec![],
        }],
    };
    let mut session = zil_types::FunctionSession::default();
    for _ in 0..2 {
        let prepared = zil_types::infer_program_for_ct(&program, &limits)
            .unwrap()
            .into_prepared();
        session
            .check_prepared_program(&prepared, &target, &limits)
            .unwrap();
    }
    let mut orphan = core.closure_life_contexts[0].clone();
    orphan.creator = program.modules[0].core.functions[0].id;
    orphan.expression.0 = u32::MAX;
    program.modules[0].core.closure_life_contexts.push(orphan);
    let prepared = zil_types::infer_program_for_ct(&program, &limits)
        .unwrap()
        .into_prepared();
    let error = session
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.message.contains("no actual consumed expression")),
        "{error:?}"
    );
}

#[test]
fn g3_3_2_source_formation_request_budget_and_default_worker_are_reproducible() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fn-input-capture.zil"
    );
    let core = g3_3_2_source_core(text);
    let target = zil_base::TargetProfile::host_llvm();
    let limits = zil_base::Limits::default();
    let expected = zil_types::check(&core, &target, &limits).unwrap();
    let (mut low, mut high) = (1, limits.max_analysis_steps);
    while low < high {
        let mid = low + (high - low) / 2;
        let probe = zil_base::Limits {
            max_analysis_steps: mid,
            ..limits.clone()
        };
        match zil_types::check(&core, &target, &probe) {
            Ok(_) => high = mid,
            Err(e)
                if e.iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = mid + 1
            }
            Err(e) => panic!("{e:?}"),
        }
    }
    for (n, ok) in [(low, true), (low - 1, false), (low, true)] {
        let probe = zil_base::Limits {
            max_analysis_steps: n,
            ..limits.clone()
        };
        if ok {
            assert_eq!(zil_types::check(&core, &target, &probe).unwrap(), expected);
        } else {
            assert!(
                zil_types::check(&core, &target, &probe)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            );
        }
    }
    eprintln!("G3.3.2 source-derived whole Types N={low}; N-1/fresh stable");
    // Real shallow Source repeated constructors in the default thread; no
    // custom stack or reduced Source depth. No native claim in this target.
    let declarations = (0..32)
        .map(|i| format!("let f{i}=(p:&Tag<'s>)->i32=>i32(*p.p+*seed.p);"))
        .collect::<String>();
    let repeated = format!(
        "struct Tag<life s>{{p:&'s i64}} fn apply<life s>(seed:&Tag<'s>)->i32{{{declarations}return f31(seed)}}"
    );
    std::thread::spawn(move || {
        let core = g3_3_2_source_core(&repeated);
        let result = zil_types::check(
            &core,
            &zil_base::TargetProfile::host_llvm(),
            &zil_base::Limits::default(),
        )
        .unwrap();
        assert_eq!(result.closures.len(), 32);
    })
    .join()
    .unwrap();
}
