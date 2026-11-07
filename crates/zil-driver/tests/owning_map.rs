//! Owning key/value Map: actual provider, responsibility, and allocator protocol.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
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
    let base = std::env::var_os("TMPDIR")
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
fn key_without_actual_keyeq_provider_is_rejected() {
    let source = project(COPY_VALUE.replace("impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}", "").as_str());
    let report = compile_project(
        &source,
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
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory("allocator");
            let actual_object = out.join("actual-api.o");
            let wrapper = out.join("capture-clang.sh");
            fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do case \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac; done\nexec {} \"$@\"\n", quote(&actual_object), quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_project(
                &project(ALLOCATION),
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
                    "{backend:?}/{optimization:?}/fail={fail}: {run:?}"
                );
                assert_eq!(
                    run.stdout, stdout,
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
                    stats.contains(&format!("\"failures\":{}", usize::from(fail != 0))),
                    "{stats}"
                );
            }
        }
    }
}
