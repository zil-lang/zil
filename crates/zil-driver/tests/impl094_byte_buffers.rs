//! Real source admission, candidate provenance and independently checked IR.
use std::collections::BTreeMap;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, ProjectSources, compile_project, compile_source};
use zil_ir::{HoldRoot, InstructionKind, Type};

fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "impl094.zil", text)
}
fn checked(text: &str) -> CompileReport {
    let Some(root) = std::env::var_os("ZIL_IMPL094_RECORD") else {
        return compile_source(&input(text), &CompileOptions::default());
    };
    let dir = std::path::PathBuf::from(root).join(Fingerprint::of(text.as_bytes()).to_hex());
    std::fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("input.zil");
    std::fs::write(&source_path, text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), source_path.to_str().unwrap(), text);
    let report = compile_source(
        &source,
        &CompileOptions {
            emit_dir: Some(dir.join("stages")),
            ..CompileOptions::default()
        },
    );
    std::fs::write(dir.join("report.json"), report.to_json()).unwrap();
    report
}
fn passes(text: &str) {
    let report = checked(text);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{text}\n{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
}
fn rejects(text: &str, phase: Phase, code: &str) {
    let report = checked(text);
    assert_eq!(
        report.status,
        if code == "E-UNSUPPORTED" {
            ResultCategory::Unsupported
        } else {
            ResultCategory::SafeReject
        },
        "{text}\n{}",
        report.to_json()
    );
    assert_eq!(report.phase, phase, "{text}\n{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        code,
        "{text}\n{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}

fn typed(text: &str) -> zil_ir::TypedModule {
    let source = input(text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default())
        .unwrap_or_else(|e| panic!("{text}: {e:#?}"))
}

#[test]
fn byte_storage_has_real_source_operations_and_sealed_single_definition() {
    let text = "fn main()->i32{let b=bytes_new();if !bytes_reserve(b,4){return 1;}if !bytes_push(b,7u8){return 2;}let p=bytes_view_mut(b);p[0]=9u8;return i32(bytes_len(b)+bytes_capacity(b))+i32(bytes_view(b)[0]);}";
    passes(text);
    let module = typed(text);
    assert_eq!(
        module
            .resources
            .iter()
            .filter(|r| r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME)
            .count(),
        1
    );
    let resource = module
        .resources
        .iter()
        .find(|r| r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME)
        .unwrap();
    assert_eq!(resource.payload, Type::ByteBuffer);
    assert_eq!(resource.teardown, None);
    let instructions = module.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::ByteBufferNew))
    );
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::ByteBufferReserve { .. }))
    );
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::ByteBufferPush { .. }))
    );
    assert!(instructions.iter().any(|i| matches!(
        i.kind,
        InstructionKind::ByteBufferBorrow { mutable: true, .. }
    )));
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
    );
}

#[test]
fn create_give_and_description_parameter_window_returns_compose() {
    passes(
        "fn fresh()->ByteBuffer contract{result(create);}{return bytes_new();}fn relay(give b:ByteBuffer)->ByteBuffer contract{result(give);}{return give b;}fn fill(b:ByteBuffer)->bool{return bytes_reserve(b,3)&&bytes_push(b,8u8);}fn view(b:ByteBuffer)->&[u8]{return bytes_view(b);}fn consume(give b:ByteBuffer)->i32{return i32(bytes_len(b));}fn main()->i32{let a=fresh();let b=relay(give a);if !fill(b){return 5;}let s=view(b);let value=i32(s[0]);return value+consume(give b);}",
    );
}

#[test]
fn byte_descriptions_and_borrowed_windows_preserve_actual_roots() {
    let module = typed(
        "fn window(b:ByteBuffer)->&[u8]{return bytes_view(b);}fn result()->i64{let b=bytes_new();return bytes_view(b).len();}",
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for (function, plan) in module.functions.iter().zip(&plan.functions) {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if matches!(instruction.kind, InstructionKind::ByteBufferBorrow { .. }) {
                let roots = &plan.holds[instruction.result.unwrap().0 as usize].roots;
                if function.name == "window" {
                    assert_eq!(roots, &[HoldRoot::Parameter(0)]);
                } else {
                    assert!(matches!(roots.as_slice(), [HoldRoot::Local(_)]));
                }
            }
        }
    }
    zil_verify::verify_module(module, plan, &TargetProfile::default(), &Limits::default()).unwrap();
}

#[test]
fn ordered_alias_helpers_do_not_confuse_parameter_names_with_disjointness() {
    passes(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);let n=s.len();bytes_reserve(b,4);return n;}fn result()->i64{let b=bytes_new();return act(b,b);}",
    );
    rejects(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,4);return s.len();}fn result()->i64{let b=bytes_new();return act(b,b);}",
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    passes(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,4);return s.len();}fn result()->i64{let a=bytes_new();let b=bytes_new();return act(a,b);}",
    );
}

#[test]
fn active_windows_block_growth_push_move_and_conflicting_metadata() {
    for text in [
        "fn result()->i64{let b=bytes_new();let s=bytes_view(b);bytes_reserve(b,0);return s.len();}",
        "fn result()->i64{let b=bytes_new();let s=bytes_view(b);bytes_push(b,1u8);return s.len();}",
        "fn take(give b:ByteBuffer){}fn result()->i64{let b=bytes_new();let s=bytes_view(b);take(give b);return s.len();}",
        "fn result()->i64{let b=bytes_new();let s=bytes_view_mut(b);let n=bytes_len(b);return s.len()+n;}",
        "fn result()->i64{let b=bytes_new();let s=bytes_view_mut(b);let t=bytes_view(b);return s.len()+t.len();}",
    ] {
        rejects(text, Phase::Verify, "E-BORROW-CONFLICT");
    }
    passes(
        "fn result()->i64{let b=bytes_new();let s=bytes_view(b);let n=bytes_len(b);return s.len()+n;}",
    );
}

#[test]
fn local_or_consumed_carriers_cannot_export_unowned_windows() {
    for (text, phase, code) in [
        (
            "fn bad()->&[u8]{let b=bytes_new();return bytes_view(b);}fn main(){}",
            Phase::Verify,
            "E-BORROW-ESCAPE",
        ),
        (
            "fn bad(give b:ByteBuffer)->&[u8]{return bytes_view(b);}fn main(){}",
            Phase::Verify,
            "E-BORROW-ESCAPE",
        ),
        (
            "fn take(give b:ByteBuffer){}fn result()->i64{let b=bytes_new();let a=b;take(give b);return bytes_len(a);}",
            Phase::Analyze,
            "E-ANALYZE-IR",
        ),
        (
            "fn take(give b:ByteBuffer){}fn main(){let b=bytes_new();take(give b);take(give b);}",
            Phase::Analyze,
            "E-UNINITIALIZED",
        ),
    ] {
        rejects(text, phase, code);
    }
}

#[test]
fn storage_heads_and_unsupported_escape_routes_fail_closed() {
    for (text, phase, code) in [
        (
            "fn main(){let b=ByteBuffer{};}",
            Phase::Types,
            "E-RECORD-TYPE",
        ),
        (
            "fn main(){let b=bytes_new();b.len=2;}",
            Phase::Types,
            "E-FIELD-TYPE",
        ),
        (
            "package P{data:ByteBuffer;}fn main(){}",
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "struct P{data:ByteBuffer;}fn main(){}",
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "extern \"C\" fn f(b:ByteBuffer);fn main(){}",
            Phase::Types,
            "E-FFI-DECLARATION",
        ),
        (
            "fn main(){let n=#(bytes_new());}",
            Phase::Ct,
            "E-CT-FORBIDDEN",
        ),
        (
            "fn main(){let b=bytes_new();bytes_reserve(b,true);}",
            Phase::Types,
            "E-TYPE-MISMATCH",
        ),
        (
            "fn main(){let b=bytes_new();bytes_push(b,256);}",
            Phase::Types,
            "E-INTEGER-RANGE",
        ),
        (
            "fn main(){let b=bytes_new();let s=&b;}",
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "fn main(){let bytes_push=2;}",
            Phase::Types,
            "E-BYTE-BUFFER-NAME",
        ),
    ] {
        rejects(text, phase, code);
    }
}

#[test]
fn an_uncalled_local_byte_descriptor_capture_keeps_its_owner_in_scope() {
    // The original 094 input is unchanged. Slice187 now admits the local
    // descriptor profile; nonescaping cleanup is checked independently.
    passes("fn main(){let b=bytes_new();let d=b;let f=move()=>bytes_len(d);}");
}

#[test]
fn parametric_windows_and_assignment_enter_full_production_verification() {
    passes(
        "fn tail<T>(p:&[T])->&[T]{return &p[1..];}fn mutable_tail<T>(p:&mut[T])->&mut[T]{return &mut p[1..];}fn put<T>(p:&mut[T],v:T){p[0]=v;}fn replace<T>(p:&mut T,v:T){*p=v;}fn length<T>(p:&[T])->i64{return p.len();}fn result()->i64{var a=[1,2,3];put::<i32>(mutable_tail::<i32>(&mut a[..]),7);replace::<i32>(&mut a[0],8);return length(tail(&a[..]))+i64(a[0]+a[1]);}",
    );
    passes(
        "fn put<T>(p:&mut[T],v:T){p[0]=v;}fn length<T>(p:&[T])->i64{return p.len();}fn work<T>(p:&mut[T],v:T)->i64{put(p,v);return length(p)+p.len();}fn result()->i64{let b=bytes_new();bytes_reserve(b,2);bytes_push(b,5u8);let n=work(bytes_view_mut(b),9u8);return n+length(bytes_view(b));}",
    );
}

#[test]
fn reusable_storage_and_generic_sequence_modules_share_real_interfaces() {
    let sources = [
        (
            "root.zil",
            "import storage;import sequence;fn result()->i64{let b=storage.create();storage.fill(b);let p=storage.view_mut(b);sequence.put(sequence.tail_mut(p),9u8);let n=sequence.count(p);return n+sequence.count(storage.view(b));}",
        ),
        (
            "storage.zil",
            "pub fn create()->ByteBuffer contract{result(create);}{return bytes_new();}pub fn fill(b:ByteBuffer)->bool{return bytes_reserve(b,2)&&bytes_push(b,4u8)&&bytes_push(b,5u8);}pub fn view(b:ByteBuffer)->&[u8]{return bytes_view(b);}pub fn view_mut(b:ByteBuffer)->&mut[u8]{return bytes_view_mut(b);}",
        ),
        (
            "sequence.zil",
            "pub fn tail_mut<T>(p:&mut[T])->&mut[T]{return &mut p[1..];}pub fn put<T>(p:&mut[T],v:T){p[0]=v;}pub fn count<T>(p:&[T])->i64{return p.len();}",
        ),
    ];
    let recorded = std::env::var_os("ZIL_IMPL094_RECORD")
        .map(|root| std::path::PathBuf::from(root).join("three-modules"));
    let modules = sources
        .iter()
        .enumerate()
        .map(|(index, (path, text))| {
            let path = if let Some(dir) = &recorded {
                std::fs::create_dir_all(dir).unwrap();
                let path = dir.join(path);
                std::fs::write(&path, text).unwrap();
                path.to_str().unwrap().to_owned()
            } else {
                (*path).into()
            };
            SourceUnit::new(FileId(index as u32), ModuleId(index as u32), path, *text)
        })
        .collect();
    let project = ProjectSources {
        root: ModuleId(0),
        modules,
        paths: BTreeMap::from([
            (vec!["storage".into()], ModuleId(1)),
            (vec!["sequence".into()], ModuleId(2)),
        ]),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            emit_dir: recorded.as_ref().map(|dir| dir.join("stages")),
            ..CompileOptions::default()
        },
    );
    if let Some(dir) = recorded {
        std::fs::write(dir.join("report.json"), report.to_json()).unwrap();
    }
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify);
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn dynamic_storage_never_executes_through_compile_time_helpers() {
    for text in [
        "fn effect()->i32{let b=bytes_new();return 0;}fn result()->i32{return #(effect());}",
        "fn effect()->i32{if false{let b=bytes_new();}return 0;}fn result()->i32{return #(effect());}",
    ] {
        rejects(text, Phase::Ct, "E-CT-FORBIDDEN");
    }
    passes(
        "fn storage(b:ByteBuffer)->i64{return bytes_len(b);}fn pure()->u8{return 12u8;}fn result()->u8{return #(pure());}",
    );
}

#[test]
fn new_language_paths_keep_resource_limits() {
    let source = input("fn main(){let b=bytes_new();bytes_reserve(b,10);}");
    for limits in [
        Limits {
            max_nodes: 0,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        },
    ] {
        let report = compile_source(
            &source,
            &CompileOptions {
                limits,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::ResourceLimit,
            "{}",
            report.to_json()
        );
    }
}
