//! Owning key/value Map: actual provider, responsibility, and allocator protocol.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_path,
    compile_project,
};
use zil_ir::{InstructionKind, Type};

const MAP: &str = include_str!("../../../lib/std/owning_map.zil");
const ENTRIES: &str = include_str!("../../../lib/std/owning_entries.zil");
const STORE: &str = include_str!("../../../lib/std/store.zil");
const CONSUMER: &str = r#"
import std.owning_map
import std.owning_entries
extern "C" fn putchar(n:i32)->i32;
resource Ticket { code:i32 }
impl Drop for Ticket { fn drop(self:Self){unsafe{putchar(.code)}} }
package Key { manages ticket:Ticket; id:i64 }
impl owning_map.KeyEq for Key { fn same(self:&Self,other:&Self)->bool{return self.id==other.id} }
fn main()->i32 {
 var map=owning_map.new::<Key,Managed<Ticket>>()
 let first=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:65},id:7},value:Ticket{code:66}}
 match owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,first){.err(_)=>{return 1},.ok(replaced)=>{if replaced{return 2}}}
 let second=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:67},id:7},value:Ticket{code:68}}
 match owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,second){.err(_)=>{return 3},.ok(replaced)=>{if !replaced{return 4}}}
 let query=Key{ticket:Ticket{code:69},id:7}
 if owning_map.len::<Key,Managed<Ticket>>(&map)!=1 || owning_map.find::<Key,Managed<Ticket>>(&map,&query)!=0 || !owning_map.contains::<Key,Managed<Ticket>>(&map,&query){return 5}
 match owning_map.take::<Key,Managed<Ticket>>(&mut map,&query){.err(_)=>{return 6},.ok(old)=>{return old.key.ticket.code+old.value.code-135}}
}
"#;
const COPY_VALUE: &str = r#"
import std.owning_map
import std.owning_entries
resource Ticket{code:i32}
package Key{manages ticket:Ticket;id:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
fn main()->i32{
 var map=owning_map.new::<Key,i32>()
 let incoming=owning_entries.Entry::<Key,i32>{key:Key{ticket:Ticket{code:65},id:9},value:42}
 match owning_map.insert_or_replace::<Key,i32>(&mut map,incoming){.err(_)=>{return 1},.ok(_)=>{}}
 let found=Key{ticket:Ticket{code:66},id:9}
 match owning_map.get_copy::<Key,i32>(&map,&found){Option.None()=>{return 2},Option.Some(value)=>{if value!=42{return 3}}}
 let absent=Key{ticket:Ticket{code:67},id:10}
 match owning_map.get_copy::<Key,i32>(&map,&absent){Option.None()=>{},Option.Some(_)=>{return 4}}
 match owning_map.take::<Key,i32>(&mut map,&absent){.err(_)=>{return i32(owning_map.len::<Key,i32>(&map))-1},.ok(_)=>{return 5}}
}
"#;
const TRANSFER_REPLACED_ENTRY: &str = r#"
import std.owning_map
import std.owning_entries
resource Ticket{code:i32}
package Key{manages ticket:Ticket;id:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
fn main()->i32{
 var map=owning_map.new::<Key,Managed<Ticket>>()
 var replaced=true
 let missing=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:65},id:1},value:Ticket{code:66}}
 let returned=owning_map.replace_existing::<Key,Managed<Ticket>>(&mut map,missing,&mut replaced)
 if replaced || returned.key.ticket.code+returned.value.code!=131 || owning_map.len::<Key,Managed<Ticket>>(&map)!=0{return 1}
 let first=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:67},id:1},value:Ticket{code:68}}
 match owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,first){.err(_)=>{return 2},.ok(_)=>{}}
 let incoming=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:69},id:1},value:Ticket{code:70}}
 let old=owning_map.replace_existing::<Key,Managed<Ticket>>(&mut map,incoming,&mut replaced)
 if !replaced || old.key.ticket.code+old.value.code!=135 || owning_map.len::<Key,Managed<Ticket>>(&map)!=1{return 3}
 return 0
}
"#;
const BYTE_BUFFER_KEY_AND_VALUE: &str = r#"
import std.owning_map
import std.owning_entries
package Key{manages text:ByteBuffer;id:i64}
package Value{manages bytes:ByteBuffer}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
fn key(id:i64,letter:u8)->Key{let text=bytes_new();bytes_reserve(text,1);bytes_push(text,letter);return Key{text:give text,id:id}}
fn value(letter:u8)->Value{let bytes=bytes_new();bytes_reserve(bytes,1);bytes_push(bytes,letter);return Value{bytes:give bytes}}
fn main()->i32{
 var map=owning_map.new::<Key,Value>()
 let entry=owning_entries.Entry::<Key,Value>{key:key(7,65u8),value:value(66u8)}
 match owning_map.insert_or_replace::<Key,Value>(&mut map,entry){.err(_)=>{return 1},.ok(_)=>{}}
 let query=key(7,67u8)
 match owning_map.take::<Key,Value>(&mut map,&query){
  .err(_)=>{return 2},
  .ok(old)=>{let first=i32(bytes_view(old.key.text)[0]);let second=i32(bytes_view(old.value.bytes)[0]);return first+second-131}
 }
}
"#;
const ALLOCATION: &str = r#"
import std.owning_map
import std.owning_entries
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Key{manages ticket:Ticket;id:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
fn work()->owning_entries.Entry<Key,Managed<Ticket>>!i32{
 var map=owning_map.new::<Key,Managed<Ticket>>()
 owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:65},id:1},value:Ticket{code:66}})?
 owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:67},id:2},value:Ticket{code:68}})?
 let incoming=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:69},id:1},value:Ticket{code:70}}
 match owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,incoming){
  .err(returned)=>{
   let query=Key{ticket:Ticket{code:71},id:1}
   if owning_map.len::<Key,Managed<Ticket>>(&map)!=2 || owning_map.find::<Key,Managed<Ticket>>(&map,&query)!=0 {return 95}
   return returned
  },
  .ok(replaced)=>{if !replaced{return 96}return 0}
 }
}
fn main()->i32{
 match work(){
  .err(returned)=>{return returned.key.ticket.code-55},
  .ok(code)=>{return code}
 }
}
"#;
const REPLACEMENT_ALLOCATION: &str = r#"
import std.owning_map
import std.owning_entries
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Key{manages ticket:Ticket;id:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
fn work()->owning_entries.Entry<Key,Managed<Ticket>>!i32{
 var map=owning_map.new::<Key,Managed<Ticket>>()
 owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:65},id:1},value:Ticket{code:66}})?
 owning_map.insert_or_replace::<Key,Managed<Ticket>>(&mut map,owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:67},id:2},value:Ticket{code:68}})?
 let incoming=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:69},id:1},value:Ticket{code:70}}
 var replaced=false
 let returned=owning_map.replace_existing::<Key,Managed<Ticket>>(&mut map,incoming,&mut replaced)
 if !replaced {
  let query=Key{ticket:Ticket{code:71},id:1}
  if owning_map.len::<Key,Managed<Ticket>>(&map)!=2 || owning_map.find::<Key,Managed<Ticket>>(&map,&query)!=0 {return 95}
  return returned
 }
 if returned.key.ticket.code!=65 || returned.value.code!=66 {return 96}
 return 0
}
fn main()->i32{
 match work(){
  .err(returned)=>{return returned.key.ticket.code-55},
  .ok(code)=>{return code}
 }
}
"#;
const RAW_OWNING_ENTRIES: &str = r#"
resource Ticket{code:i32}
package Key{manages ticket:Ticket;id:i64}
trait KeyEq{fn same(self:&Self,other:&Self)->bool;}
impl KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
package Map<K:StoreElement+KeyEq,V:StoreElement>{manages items:Store<Entry<K,V>>}
fn main()->i32{
 let items:Store<Entry<Key,Managed<Ticket>>>=store_new()
 let map=Map::<Key,Managed<Ticket>>{items:give items}
 if !store_reserve(map.items,2){return 1}
 let first=Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:65},id:1},value:Ticket{code:66}}
 let second=Entry::<Key,Managed<Ticket>>{key:Key{ticket:Ticket{code:67},id:2},value:Ticket{code:68}}
 store_push(map.items,first)
 store_push(map.items,second)
 return i32(store_len(map.items))-2
}
"#;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn project(main: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "std/owning_map.zil", MAP),
            SourceUnit::new(FileId(2), ModuleId(2), "std/owning_entries.zil", ENTRIES),
            SourceUnit::new(FileId(3), ModuleId(3), "std/store.zil", STORE),
        ],
        paths: [
            (vec!["std".into(), "owning_map".into()], ModuleId(1)),
            (vec!["std".into(), "owning_entries".into()], ModuleId(2)),
            (vec!["std".into(), "store".into()], ModuleId(3)),
        ]
        .into(),
    }
}

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "owning-map-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn backends() -> Vec<NativeBackend> {
    let mut values = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        values.push(NativeBackend::LlvmApi);
    }
    values
}

#[cfg(unix)]
#[test]
fn borrowed_task_and_owning_map_share_one_lending_and_cleanup_cfg() {
    task_map_native(
        include_str!("../../../tests/fixtures/task-borrow-results/owning-map-lending.zil"),
        "borrowed-task-lending",
    );
}

#[cfg(unix)]
#[test]
fn temporary_task_match_preserves_each_actual_source_and_cleanup_path() {
    task_map_native(
        include_str!(
            "../../../tests/fixtures/task-borrow-results/owning-map-temporary-lending.zil"
        ),
        "temporary-task-lending",
    );
}

#[cfg(unix)]
#[test]
fn conditional_task_input_keeps_both_actual_sources_and_resource_cleanup() {
    let source = include_str!(
        "../../../tests/fixtures/task-borrow-results/owning-map-conditional-lending.zil"
    );
    task_map_native(source, "conditional-task-parent-input");
    let local_input = source
        .replace("var number=42", "var number=43")
        .replace("return number-43", "return number-44");
    task_map_native(&local_input, "conditional-task-local-input");
}

#[test]
fn conditional_task_map_analyze_work_preserves_tags_and_live_inputs() {
    let source = include_str!(
        "../../../tests/fixtures/task-borrow-results/owning-map-conditional-lending.zil"
    );
    let report = compile_project(
        &project(source),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    let out = directory("conditional-task-analysis");
    fs::write(out.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    let module = raw_task_map_lending(source);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    // The independent verifier still uses its old work schedule in this
    // Analyze-only increment; neither its authority nor its checks change.
    let verify_limits = Limits {
        max_analysis_steps: 100_000_000,
        ..limits.clone()
    };
    let target = TargetProfile::host_llvm();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &verify_limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &verify_limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &verify_limits).unwrap();
    let mut lower = 0;
    let mut upper = limits.max_analysis_steps;
    while lower + 1 < upper {
        let middle = lower + (upper - lower) / 2;
        let bounded = Limits {
            max_analysis_steps: middle,
            ..limits.clone()
        };
        match zil_analyze::analyze(&module, &bounded) {
            Ok(actual) => {
                assert_eq!(actual, plan);
                upper = middle;
            }
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                lower = middle;
            }
        }
    }
    fs::write(
        out.join("analyze-work.json"),
        format!("{{\"N\":{upper},\"N_minus_one\":{lower}}}\n"),
    )
    .unwrap();
    eprintln!("conditional Task Map Analyze N={upper}");
    for _ in 0..2 {
        assert_eq!(
            zil_analyze::analyze(
                &module,
                &Limits {
                    max_analysis_steps: upper,
                    ..limits.clone()
                }
            )
            .unwrap(),
            plan
        );
        let errors = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: lower,
                ..limits.clone()
            },
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
    }
    for (label, bad) in [
        (
            "conditional-pending-input-write",
            source.replace("   var map=", "   *parent=9\n   var map="),
        ),
        (
            "conditional-ready-input-write",
            source.replace(
                "      let observed=*pointer",
                "      *parent=9\n      let observed=*pointer",
            ),
        ),
        (
            "conditional-live-option-move",
            source.replace(
                "   let observed=inspect(found)",
                "   let retired_early=map\n   let observed=inspect(found)",
            ),
        ),
    ] {
        let report = compile_project(
            &project(&bad),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        let failure = directory(label);
        fs::write(failure.join("main.zil"), bad).unwrap();
        fs::write(failure.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            Phase::Analyze,
            "{label}: {}",
            report.to_json()
        );
    }
}

#[cfg(unix)]
fn task_map_native(source: &str, label: &str) {
    use std::os::unix::fs::PermissionsExt;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/task-map-lending/allocator_probe.c");
    let quote =
        |path: &std::path::Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory(label);
            let object = out.join("actual-api.o");
            let wrapper = out.join("capture-clang.sh");
            fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do case \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac; done\nexec {} \"$@\"\n", quote(&object), quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_project(
                &project(source),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    output: Some(out.join("normal")),
                    emit_dir: Some(out.join("phases")),
                    limits: Limits {
                        max_analysis_steps: 100_000_000,
                        ..Limits::default()
                    },
                    ..CompileOptions::default()
                },
            );
            fs::write(out.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let normal = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(out.join("normal.stdout"), &normal.stdout).unwrap();
            fs::write(out.join("normal.stderr"), &normal.stderr).unwrap();
            assert_eq!(
                normal.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {normal:?}"
            );
            assert_eq!(normal.stdout, b"ABDEC");
            assert!(normal.stderr.is_empty());
            let input = if backend == NativeBackend::LlvmApi {
                assert!(object.is_file());
                object
            } else {
                out.join("phases/llvm.ll")
            };
            let probe = out.join("probe");
            let linked = Command::new(&clang)
                .arg(if optimization == Optimization::O0 {
                    "-O0"
                } else {
                    "-O2"
                })
                .arg(input)
                .arg(&shim)
                .args([
                    "-Wl,--wrap=calloc",
                    "-Wl,--wrap=realloc",
                    "-Wl,--wrap=free",
                    "-o",
                ])
                .arg(&probe)
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            for (fail, exit, stdout, allocations, failures) in [
                (0, 0, "ABDEC", 2, 0),
                (1, 11, "AB", 1, 1),
                (2, 13, "DECAB", 2, 1),
            ] {
                let run = Command::new(&probe)
                    .env("ZIL_TEST_FAIL_AT", fail.to_string())
                    .output()
                    .unwrap();
                fs::write(out.join(format!("fail-{fail}.stdout")), &run.stdout).unwrap();
                fs::write(out.join(format!("fail-{fail}.stderr")), &run.stderr).unwrap();
                assert_eq!(
                    run.status.code(),
                    Some(exit),
                    "{backend:?}/{optimization:?}/fail={fail}: {run:?}"
                );
                assert_eq!(
                    run.stdout,
                    stdout.as_bytes(),
                    "{backend:?}/{optimization:?}/fail={fail}: {run:?}"
                );
                let stats = String::from_utf8(run.stderr).unwrap();
                assert!(stats.contains("\"live\":0"), "{stats}");
                assert!(
                    stats.contains(&format!("\"allocated\":{allocations}")),
                    "{stats}"
                );
                assert!(
                    stats.contains(&format!("\"frees\":{allocations}")),
                    "{stats}"
                );
                assert!(
                    stats.contains(&format!("\"failures\":{failures}")),
                    "{stats}"
                );
            }
            let failed_task = Command::new(&probe)
                .env("ZIL_TEST_FAIL_TASK_ALLOC", "1")
                .output()
                .unwrap();
            fs::write(out.join("failed-task.stdout"), &failed_task.stdout).unwrap();
            fs::write(out.join("failed-task.stderr"), &failed_task.stderr).unwrap();
            assert_eq!(failed_task.status.code(), Some(0), "{failed_task:?}");
            assert!(failed_task.stdout.is_empty());
            assert_eq!(
                String::from_utf8(failed_task.stderr).unwrap(),
                "{\"alloc_calls\":0,\"allocated\":0,\"frees\":0,\"failures\":1,\"live\":0,\"peak_live\":0,\"peak_bytes\":0}\n"
            );
        }
    }
}

#[test]
fn task_map_lending_keeps_each_live_source_window() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/owning-map-lending.zil");
    for (label, bad) in [
        ("pending-input-write", source.replace("   var map=", "   *parent=9\n   var map=")),
        ("ready-input-write", source.replace("      let observed=*pointer", "      *parent=9\n      let observed=*pointer")),
        ("live-option-owner-move", source.replace("   let observed=inspect(found)", "   let retired_early=map\n   let observed=inspect(found)")),
        ("live-option-insert", source.replace("   let observed=inspect(found)", "   let inserted=owning_map.insert_or_replace::<Key,Value>(&mut map,owning_entries.Entry::<Key,Value>{key:Key{ticket:Ticket{code:88},id:9},value:Value{ticket:Ticket{code:89},number:4}})\n   let observed=inspect(found)")),
    ] {
        assert_ne!(bad, source, "{label}");
        let out = directory(label);
        fs::write(out.join("main.zil"), &bad).unwrap();
        let report = compile_project(&project(&bad), &CompileOptions {
            limits: Limits { max_analysis_steps: 100_000_000, ..Limits::default() },
            ..CompileOptions::default()
        });
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(report.status, ResultCategory::SafeReject, "{label}: {}", report.to_json());
        assert_eq!(report.phase, Phase::Analyze, "{label}: {}", report.to_json());
    }
}

#[test]
fn temporary_task_match_requires_one_real_hidden_owner_and_live_input() {
    let source = include_str!(
        "../../../tests/fixtures/task-borrow-results/owning-map-temporary-lending.zil"
    );
    let module = raw_task_map_lending(source);
    let target = TargetProfile::host_llvm();
    let limits = Limits {
        max_analysis_steps: 100_000_000,
        ..Limits::default()
    };
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let attempt = module
        .functions
        .iter()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::TaskCreate { .. }))
        })
        .unwrap();
    let creates: Vec<_> = attempt
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.kind, InstructionKind::TaskCreate { .. }))
        .collect();
    assert_eq!(
        creates.len(),
        1,
        "the temporary scrutinee is evaluated once"
    );
    let created = creates[0].result.unwrap();
    let owner = attempt
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::Store { local, value } if value == created => Some(local),
            _ => None,
        })
        .expect("the actual Task error wrapper has a local owner before tag selection");
    assert!(matches!(
        attempt.locals[owner.0 as usize].ty,
        Type::ErrorUnion { .. }
    ));
    assert!(
        attempt
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind,
        InstructionKind::PlaceTagLocal { local, .. } if local == owner))
    );
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let mut missing = module.clone();
    let function = &mut missing.functions[attempt.id.0 as usize];
    let mut removed = 0;
    for block in &mut function.blocks {
        block.instructions.retain(|i| {
            let selected = matches!(i.kind, InstructionKind::Store { local, value } if local == owner && value == created);
            removed += usize::from(selected);
            !selected
        });
    }
    assert_eq!(removed, 1);
    let errors = zil_analyze::analyze(&missing, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    let mut candidate = plan;
    candidate.module_fingerprint = missing.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&missing, &candidate, &limits).unwrap();
    for result in [
        zil_verify::verify_module(missing, candidate, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let errors = result
            .err()
            .expect("a tag cannot bypass the missing actual temporary owner");
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
    }
    for (label, bad) in [
        (
            "temporary-pending-input-write",
            source.replace("   var map=", "   *parent=9\n   var map="),
        ),
        (
            "temporary-ready-input-write",
            source.replace(
                "      let observed=*pointer",
                "      *parent=9\n      let observed=*pointer",
            ),
        ),
    ] {
        let report = compile_project(
            &project(&bad),
            &CompileOptions {
                limits: limits.clone(),
                ..CompileOptions::default()
            },
        );
        let out = directory(label);
        fs::write(out.join("main.zil"), bad).unwrap();
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            Phase::Analyze,
            "{label}: {}",
            report.to_json()
        );
    }
}

fn raw_task_map_lending(source: &str) -> zil_ir::TypedModule {
    let project = project(source);
    let parsed: Vec<_> = project
        .modules
        .iter()
        .map(|unit| zil_syntax::parse(unit).into_result().unwrap())
        .collect();
    let syntax: Vec<_> = parsed.iter().collect();
    let imports: Vec<Vec<_>> = syntax
        .iter()
        .map(|module| {
            module
                .items
                .iter()
                .filter_map(|item| {
                    let zil_syntax::ast::ItemKind::Import(import) = &item.kind else {
                        return None;
                    };
                    Some(zil_ir::CoreImport {
                        alias: import
                            .alias
                            .clone()
                            .unwrap_or_else(|| import.path.last().unwrap().clone()),
                        module: project.paths[&import.path],
                        span: item.span,
                    })
                })
                .collect()
        })
        .collect();
    let limits = Limits {
        max_analysis_steps: 100_000_000,
        ..Limits::default()
    };
    let modules = zil_expand::expand_program_with_limits_target(
        &project.modules,
        &syntax,
        &imports,
        &limits,
        &TargetProfile::host_llvm(),
    )
    .unwrap()
    .into_iter()
    .map(|(module, _)| module)
    .collect();
    zil_types::check_program(
        &zil_ir::CoreProgram {
            root: project.root,
            source_fingerprint: zil_base::Fingerprint::of(source.as_bytes()),
            modules,
        },
        &TargetProfile::host_llvm(),
        &limits,
    )
    .unwrap()
    .module
}

#[test]
fn mixed_task_map_raw_wire_and_exact_verifier_work_are_independent() {
    let module = raw_task_map_lending(include_str!(
        "../../../tests/fixtures/task-borrow-results/owning-map-lending.zil"
    ));
    let target = TargetProfile::host_llvm();
    let limits = Limits {
        max_analysis_steps: 100_000_000,
        ..Limits::default()
    };
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let mut lower = 0;
    let mut upper = limits.max_analysis_steps;
    while lower + 1 < upper {
        let middle = lower + (upper - lower) / 2;
        let bounded = Limits {
            max_analysis_steps: middle,
            ..limits.clone()
        };
        match zil_verify::verify_module(module.clone(), plan.clone(), &target, &bounded) {
            Ok(_) => upper = middle,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                lower = middle;
            }
        }
    }
    let out = directory("mixed-source-proof");
    fs::write(
        out.join("verifier-work.json"),
        format!("{{\"N\":{upper},\"N_minus_one\":{lower}}}\n"),
    )
    .unwrap();
    eprintln!("mixed verifier exact work N={upper}");
    for _ in 0..2 {
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &target,
            &Limits {
                max_analysis_steps: upper,
                ..limits.clone()
            },
        )
        .unwrap();
        let errors = zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &target,
            &Limits {
                max_analysis_steps: lower,
                ..limits.clone()
            },
        )
        .err()
        .unwrap();
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
    }
    let attempt = module
        .functions
        .iter()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::TaskPoll { .. }))
        })
        .unwrap();
    let exported = attempt
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            let result = i.result?;
            (matches!(i.kind, InstructionKind::ReadVariant { .. })
                && matches!(attempt.values[result.0 as usize], Type::Address { .. }))
            .then_some(result)
        })
        .unwrap();
    let owner = attempt.locals.iter().position(|l| matches!(&l.ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { .. }))).unwrap();
    let mut frame_claim = plan.clone();
    let claim = &mut frame_claim.functions[attempt.id.0 as usize].holds[exported.0 as usize];
    assert_eq!(claim.roots, vec![zil_ir::HoldRoot::Parameter(0)]);
    claim.roots = vec![zil_ir::HoldRoot::Local(zil_ir::LocalId(owner as u32))];
    let frame_wire = zil_ir::wire::encode_module_candidate(&module, &frame_claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), frame_claim, &target, &limits),
        zil_verify::verify_module_bytes(&frame_wire, &target, &limits),
    ] {
        let errors = result
            .err()
            .expect("Ready external source cannot be forged into its frame");
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Verify && e.code == "E-VERIFY-HOLD"),
            "{errors:?}"
        );
    }
    let mut forged = plan;
    let helper = module
        .functions
        .iter()
        .find(|f| matches!(&f.return_type, Type::ErrorUnion { error, .. } if matches!(error.as_ref(), Type::Package { .. }))
            && f.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i.kind, InstructionKind::BorrowPackageField { .. })))
        .unwrap();
    let value = helper
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::BorrowPackageField { .. } => i.result,
            _ => None,
        })
        .unwrap();
    let claim = &mut forged.functions[helper.id.0 as usize].holds[value.0 as usize];
    assert!(!claim.roots.is_empty());
    claim.roots.clear();
    let forged_wire = zil_ir::wire::encode_module_candidate(&module, &forged, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module, forged, &target, &limits),
        zil_verify::verify_module_bytes(&forged_wire, &target, &limits),
    ] {
        let errors = result
            .err()
            .expect("skipped ordinary helper is independently checked");
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Verify && e.code == "E-VERIFY-HOLD"),
            "{errors:?}"
        );
    }
}

#[test]
fn actual_owned_key_and_value_replace_and_take_across_modules() {
    let source = project(CONSUMER);
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory("native");
            let report = compile_project(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
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
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let run = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {run:?}"
            );
            assert_eq!(
                run.stdout, b"ABCDE",
                "{backend:?}/{optimization:?}: {run:?}"
            );
            assert!(run.stderr.is_empty());
        }
    }
}

#[test]
fn copy_value_read_does_not_move_owning_key() {
    let source = project(COPY_VALUE);
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory("copy-value");
            let report = compile_project(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
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
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let run = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {run:?}"
            );
        }
    }
}

#[test]
fn replacement_transfers_complete_old_key_and_value() {
    let source = project(TRANSFER_REPLACED_ENTRY);
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory("transfer-replaced");
            let report = compile_project(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
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
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let run = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {run:?}"
            );
        }
    }
}

#[test]
fn byte_buffer_owning_key_and_value_survive_map_round_trip() {
    let source = project(BYTE_BUFFER_KEY_AND_VALUE);
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory("byte-buffer-key-value");
            let report = compile_project(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
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
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let run = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {run:?}"
            );
        }
    }
}

#[test]
fn key_without_actual_keyeq_provider_is_rejected() {
    for implementation in [
        "",
        "trait KeyEq{fn same(self:&Self,other:&Self)->bool;} impl KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}",
    ] {
        let main = COPY_VALUE.replace(
            "impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}",
            implementation,
        );
        let report = compile_project(
            &project(&main),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("KeyEq")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn ordinary_path_import_resolves_registered_owning_map() {
    let out = directory("standard-import");
    let input = out.join("main.zil");
    fs::write(&input, COPY_VALUE).unwrap();
    let report = compile_path(
        &input,
        &CompileOptions {
            stop_after: Stage::Verify,
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
}

#[test]
fn raw_and_wire_cannot_push_one_complete_entry_twice() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owning_map_raw.zil",
        RAW_OWNING_ENTRIES,
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &parsed).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let mut duplicate = module;
    let main = duplicate
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .unwrap();
    let pushes: Vec<_> = main
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block, basic)| {
            basic
                .instructions
                .iter()
                .enumerate()
                .filter_map(move |(instruction, item)| {
                    if let InstructionKind::OwnedStorePush { value, .. } = item.kind {
                        Some((block, instruction, value))
                    } else {
                        None
                    }
                })
        })
        .collect();
    assert_eq!(pushes.len(), 2);
    assert!(matches!(
        &main.values[pushes[0].2.0 as usize],
        Type::Package { .. }
    ));
    let InstructionKind::OwnedStorePush { value, .. } =
        &mut main.blocks[pushes[1].0].instructions[pushes[1].1].kind
    else {
        unreachable!()
    };
    *value = pushes[0].2;
    let analyze = zil_analyze::analyze(&duplicate, &limits)
        .expect_err("duplicate owning Entry must fail Analyze");
    assert!(
        analyze.iter().any(|error| error.phase == Phase::Analyze),
        "{analyze:#?}"
    );
    let mut candidate = plan;
    candidate.module_fingerprint = duplicate.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&duplicate, &candidate, &limits).unwrap();
    for result in [
        zil_verify::verify_module(duplicate, candidate, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let errors = result.expect_err("duplicate owning Entry must fail Verify and Wire");
        assert!(
            errors.iter().any(|error| error.phase == Phase::Verify),
            "{errors:#?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn allocator_failures_return_actual_entry_and_preserve_old_map() {
    use std::os::unix::fs::PermissionsExt;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/composed-store-closures/allocator_probe.c");
    let quote =
        |path: &std::path::Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for (label, source) in [("upsert", ALLOCATION), ("replace", REPLACEMENT_ALLOCATION)] {
        for backend in backends() {
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = directory(label);
                let actual_object = out.join("actual-api.o");
                let wrapper = out.join("capture-clang.sh");
                fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do case \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac; done\nexec {} \"$@\"\n", quote(&actual_object), quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                let report = compile_project(
                    &project(source),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper),
                        output: Some(out.join("normal")),
                        emit_dir: Some(out.join("phases")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let input = if backend == NativeBackend::LlvmApi {
                    assert!(actual_object.is_file());
                    actual_object
                } else {
                    out.join("phases/llvm.ll")
                };
                let probe = out.join("probe");
                let opt = if optimization == Optimization::O0 {
                    "-O0"
                } else {
                    "-O2"
                };
                let link = Command::new(&clang)
                    .arg(opt)
                    .arg(input)
                    .arg(&shim)
                    .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
                    .arg(&probe)
                    .output()
                    .unwrap();
                assert!(
                    link.status.success(),
                    "{}",
                    String::from_utf8_lossy(&link.stderr)
                );
                for (fail, exit, stdout, allocations) in [
                    (0, 0, b"ABCDEF".as_slice(), 2),
                    (1, 10, b"AB".as_slice(), 0),
                    (2, 12, b"ABCD".as_slice(), 1),
                    (3, 14, b"GABCDEF".as_slice(), 1),
                ] {
                    let run = Command::new(&probe)
                        .env("ZIL_TEST_FAIL_AT", fail.to_string())
                        .output()
                        .unwrap();
                    assert_eq!(
                        run.status.code(),
                        Some(exit),
                        "{label}/{backend:?}/{optimization:?}/fail={fail}: {run:?}"
                    );
                    assert_eq!(
                        run.stdout, stdout,
                        "{label}/{backend:?}/{optimization:?}/fail={fail}: {run:?}"
                    );
                    let stats = String::from_utf8(run.stderr).unwrap();
                    assert!(stats.contains("\"live\":0"), "{stats}");
                    assert!(
                        stats.contains(&format!("\"allocated\":{allocations}")),
                        "{stats}"
                    );
                    assert!(
                        stats.contains(&format!("\"frees\":{allocations}")),
                        "{stats}"
                    );
                    assert!(
                        stats.contains(&format!("\"failures\":{}", usize::from(fail != 0))),
                        "{stats}"
                    );
                }
            }
        }
    }
}

#[test]
fn shared_owning_map_lookup_and_optional_helper_execute_on_both_backends() {
    for (source, expected_output) in [
        (
            include_str!("../../../tests/fixtures/optional-store-address/owning-map.zil"),
            "",
        ),
        (
            include_str!("../../../tests/fixtures/optional-store-address/helper-input.zil"),
            "",
        ),
        (
            include_str!("../../../tests/fixtures/optional-store-address/identity.zil"),
            "",
        ),
        (
            include_str!("../../../tests/fixtures/optional-store-address/field-helper.zil"),
            "",
        ),
        (
            include_str!("../../../tests/fixtures/optional-store-address/separate-managers.zil"),
            "",
        ),
        (
            include_str!("../../../tests/fixtures/optional-store-address/owning-map-cleanup.zil"),
            "CAB",
        ),
    ] {
        for backend in backends() {
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = directory("shared-owning-lookup");
                fs::write(out.join("main.zil"), source).unwrap();
                let report = compile_project(
                    &project(source),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        output: Some(out.join("program")),
                        ..CompileOptions::default()
                    },
                );
                fs::write(out.join("compile.json"), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let run = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                fs::write(out.join("stdout"), &run.stdout).unwrap();
                fs::write(out.join("stderr"), &run.stderr).unwrap();
                assert_eq!(
                    run.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}: {run:?}"
                );
                assert_eq!(run.stdout, expected_output.as_bytes());
                assert!(run.stderr.is_empty());
            }
        }
    }
}

#[test]
fn optional_store_some_blocks_mutation_and_owner_retirement_until_last_use() {
    let source = include_str!("../../../tests/fixtures/optional-store-address/helper-input.zil");
    for operation in [
        "store_push(map.items,Item{data:bytes_new(),value:9})",
        "store_reserve(map.items,20)",
        "store_pop(map.items)",
        "let retired=map",
        "map.items=store_new()",
        "map=Map{items:store_new()}",
    ] {
        let bad = source
            .replace("let map=Map", "var map=Map")
            .replace(
                "    store_push(map.items,Item{data:bytes_new(),value:8})\n",
                "",
            )
            .replace(
                "    let observed=inspect(found)",
                &format!("    {operation}\n    let observed=inspect(found)"),
            );
        let report = compile_project(
            &project(&bad),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        let out = directory("optional-live-some");
        fs::write(out.join("main.zil"), &bad).unwrap();
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{operation}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            Phase::Analyze,
            "{operation}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-BORROW-CONFLICT"
                    || d.message.contains("live child")
                    || d.message.contains("actual stored target/parent loan")),
            "{operation}: {}",
            report.to_json()
        );
    }
}
