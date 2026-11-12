#![cfg(target_os = "linux")]

use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_path};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CONSUMER: &str = r#"
import std.owning_hash_map
import std.owning_hash_set
import std.owning_entries
import std.owning_map
package Key { manages bytes:ByteBuffer; id:i64 }
package Value { manages bytes:ByteBuffer; id:i64 }
impl owning_map.KeyEq for Key {fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Key {fn hash(self:&Self)->i64{return -9223372036854775808}}
fn key(id:i64)->Key {let bytes=bytes_new();bytes_reserve(bytes,1);bytes_push(bytes,u8(id));return Key{bytes:give bytes,id:id}}
fn value(id:i64)->Value {let bytes=bytes_new();bytes_reserve(bytes,1);bytes_push(bytes,u8(id));return Value{bytes:give bytes,id:id}}
fn main()->i32 {
    var map=owning_hash_map.new::<Key,Value>()
    var at:i64=0
    while at<20 {
        let entry=owning_entries.Entry::<Key,Value>{key:key(at),value:value(at+10)}
        match owning_hash_map.insert::<Key,Value>(&mut map,entry) {.err(_)=>{return 1},.ok(replaced)=>{if replaced{return 2}}}
        at+=1
    }
    if owning_hash_map.len::<Key,Value>(&map)!=20 || owning_hash_map.capacity::<Key,Value>(&map)!=32 {return 3}
    let query=key(7)
    match owning_hash_map.get::<Key,Value>(&map,&query) {Option.None()=>{return 4},Option.Some(found)=>{if found.id!=17{return 5}}}
    let replacement=owning_entries.Entry::<Key,Value>{key:key(7),value:value(99)}
    match owning_hash_map.insert::<Key,Value>(&mut map,replacement) {.err(_)=>{return 6},.ok(replaced)=>{if !replaced{return 7}}}
    match owning_hash_map.take::<Key,Value>(&mut map,&query) {.err(_)=>{return 8},.ok(old)=>{if old.key.id!=7 || old.value.id!=99 || bytes_view(old.value.bytes)[0]!=99u8{return 9}}}
    at=0
    while at<20 {
        let query=key(at)
        if owning_hash_map.contains::<Key,Value>(&map,&query)!=(at!=7) {return 10}
        at+=1
    }
    if owning_hash_map.clear::<Key,Value>(&mut map)!=19 {return 11}
    var set=owning_hash_set.new::<Key>()
    at=0
    while at<20 {
        match owning_hash_set.insert::<Key>(&mut set,key(at)) {.err(_)=>{return 12},.ok(inserted)=>{if !inserted{return 13}}}
        at+=1
    }
    match owning_hash_set.insert::<Key>(&mut set,key(7)) {.err(_)=>{return 14},.ok(inserted)=>{if inserted{return 15}}}
    match owning_hash_set.take::<Key>(&mut set,&query) {.err(_)=>{return 16},.ok(old)=>{if old.id!=7{return 17}}}
    match owning_hash_set.key_at::<Key>(&set,0) {Option.None()=>{return 18},Option.Some(first)=>{if first.id!=0{return 19}}}
    if owning_hash_set.clear::<Key>(&mut set)!=19 {return 20}
    return 0
}
"#;

// A real Source candidate is intentionally untrusted input for independent V.
// Keep the full owning-map/set consumer and the ordinary default budget here.
#[test]
fn complete_owning_hash_source_reconstructs_default_analyze_candidate() {
    let typed = typed_stdlib(&[
        ("main", CONSUMER),
        ("owning_hash_map", include_str!("../../../lib/std/owning_hash_map.zil")),
        ("owning_hash_set", include_str!("../../../lib/std/owning_hash_set.zil")),
        ("owning_entries", include_str!("../../../lib/std/owning_entries.zil")),
        ("owning_map", include_str!("../../../lib/std/owning_map.zil")),
        ("store", include_str!("../../../lib/std/store.zil")),
    ]);
    let limits = zil_base::Limits::default();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (typed.clone(), plan.clone())
    );
    if let Some(path) = std::env::var_os("ZIL_STDLIB_ANALYZE_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("main.zil"), CONSUMER).unwrap();
        fs::write(path.join("typed.txt"), format!("{typed:#?}")).unwrap();
        fs::write(path.join("candidate.txt"), format!("{plan:#?}")).unwrap();
        fs::write(path.join("candidate.wire"), wire).unwrap();
    }
}

fn run(source: &str, name: &str) {
    run_output(source, name, b"")
}
fn run_output(source: &str, name: &str, expected: &[u8]) {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-owning-hash-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("main.zil");
            fs::write(
                &path,
                source.replace("TEST_FILE_PATH", dir.join("buffered.bin").to_str().unwrap()),
            )
            .unwrap();
            let report = compile_path(
                &path,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("main")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert_eq!(
                output.stdout, expected,
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert!(output.stderr.is_empty(), "{output:?}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
#[test]
fn owning_collision_growth_replacement_take_set_and_reuse() {
    run(CONSUMER, "collections");
}
#[test]
fn full_width_unsigned_decimal_roundtrip_and_range() {
    run(
        r#"
import std.int
fn main()->i32 {
    let formatted=int.format_u64(18446744073709551615u64)
    if formatted.len!=20 || formatted.bytes[0]!=49u8 || formatted.bytes[19]!=53u8 {return 1}
    match int.parse_u64(&formatted.bytes[..formatted.len]) {.err(_)=>{return 2},.ok(value)=>{if value!=18446744073709551615u64{return 3}}}
    match int.parse_u64("18446744073709551616") {.err(error)=>{match error{int.ParseError.Overflow()=>{},_=>{return 4}}},.ok(_)=>{return 5}}
    match int.parse_u64("-1") {.err(_)=>{},.ok(_)=>{return 6}}
    let zero=int.format_u64(0u64)
    if zero.len!=1 || zero.bytes[0]!=48u8 {return 7}
    match int.parse_u8("255") {.err(_)=>{return 8},.ok(value)=>{if value!=255u8{return 9}}}
    match int.parse_u8("256") {.err(_)=>{},.ok(_)=>{return 10}}
    match int.parse_u16("65535") {.err(_)=>{return 11},.ok(value)=>{if value!=65535u16{return 12}}}
    match int.parse_usize("18446744073709551615") {.err(_)=>{return 13},.ok(value)=>{if value!= ~0usize{return 14}}}
    match int.parse_i32("-2147483648") {.err(_)=>{return 15},.ok(value)=>{if value!= -2147483648{return 16}}}
    match int.parse_i32("2147483648") {.err(_)=>{},.ok(_)=>{return 17}}
    return 0
}
"#,
        "unsigned",
    );
}
#[test]
fn buffered_file_roundtrip_and_finite_float_roundtrip() {
    run(
        include_str!("../../../tests/fixtures/buffered-io/float_and_file.zil"),
        "buffered-float",
    );
}
#[test]
fn graph_reuses_bounded_metadata_and_rejects_stale_tickets() {
    run(
        r#"
import std.graph
fn check()->graph.Error!i32 {
    var first=graph.with_capacity(2,2,2)?
    var other=graph.with_capacity(1,0,1)?
    let other_key=graph.add(&mut other,99)?
    var previous=graph.NodeKey{ticket:-1}
    var count:i64=0
    while count<100 {
        let key=graph.add(&mut first,count)?
        if graph.contains(&first,previous) {return 1}
        if graph.ticket_slots(&first)!=2 {return 2}
        graph.link(&mut first,key,key)?
        if graph.edge_count(&first)!=1 {return 3}
        if !graph.remove(&mut first,key) {return 4}
        if graph.collect(&mut first)?!=1 {return 5}
        if graph.edge_count(&first)!=0 || graph.contains(&first,key) {return 6}
        previous=key
        count+=1
    }
    if graph.issued_count(&first)!=100 || graph.ticket_slots(&first)!=2 {return 7}
    // Values are relative to the receiving graph. The same numeric key can
    // denote that graph's own node, never an address/permission in another graph.
    var same=graph.with_capacity(1,0,1)?
    let same_key=graph.add(&mut same,7)?
    if same_key.ticket!=other_key.ticket {return 8}
    match graph.get(&same,other_key){Option.None()=>{return 9},Option.Some(value)=>{if value.value!=7{return 10}}}
    let payload=bytes_new()
    bytes_reserve(payload,1)
    bytes_push(payload,42u8)
    match graph.add_with_payload(&mut first,42,give payload) {
        .err(_)=>{return 12},
        .ok(key)=>{
            let taken=graph.take_payload(&mut first,key)?
            if bytes_len(taken)!=1 || bytes_view(taken)[0]!=42u8 || graph.contains(&first,key) {return 13}
        }
    }
    let old=graph.add(&mut first,1)?
    if graph.clear(&mut first)!=1 || graph.contains(&first,old) {return 14}
    let fresh=graph.add(&mut first,2)?
    if graph.contains(&first,old) || fresh.ticket==old.ticket {return 15}
    var mixed=graph.with_capacity(3,0,2)?
    let before=graph.add(&mut mixed,1)?
    let deleted=graph.add(&mut mixed,2)?
    if !graph.remove(&mut mixed,deleted) {return 16}
    let removed=graph.take_payload(&mut mixed,before)?
    let next=graph.add(&mut mixed,3)?
    let final_key=graph.add(&mut mixed,4)?
    if graph.node_count(&mixed)!=2 || graph.contains(&mixed,deleted) || graph.contains(&mixed,before) || !graph.contains(&mixed,next) || !graph.contains(&mixed,final_key) {return 17}
    return 0
}
fn main()->i32 {match check(){.err(_)=>{return 11},.ok(code)=>{return code}}}
"#,
        "graph-reuse",
    );
}

const FAILURE_MAP: &str = r#"
import std.owning_hash_map
import std.owning_map
import std.owning_entries
extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Key{manages token:Ticket;id:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Key{fn hash(self:&Self)->i64{return -9223372036854775808}}
fn main()->i32 {
    var map=owning_hash_map.new::<Key,Managed<Ticket>>()
    var at:i64=0
    while at<5 {
        let cap=owning_hash_map.capacity::<Key,Managed<Ticket>>(&map)
        let incoming=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{token:Ticket{code:65+i32(at)},id:at},value:Ticket{code:97+i32(at)}}
        match owning_hash_map.insert::<Key,Managed<Ticket>>(&mut map,incoming) {
            .err(returned)=>{
                if returned.key.id!=at || returned.value.code!=97+i32(at) || owning_hash_map.len::<Key,Managed<Ticket>>(&map)!=at || owning_hash_map.capacity::<Key,Managed<Ticket>>(&map)!=cap {return 1}
                match owning_hash_map.insert::<Key,Managed<Ticket>>(&mut map,returned){.err(_)=>{return 2},.ok(_)=>{}}
            },
            .ok(_)=>{}
        }
        at+=1
    }
    let cap=owning_hash_map.capacity::<Key,Managed<Ticket>>(&map)
    match owning_hash_map.reserve::<Key,Managed<Ticket>>(&mut map,64) {
        .err(_)=>{
            if owning_hash_map.len::<Key,Managed<Ticket>>(&map)!=5 || owning_hash_map.capacity::<Key,Managed<Ticket>>(&map)!=cap {return 3}
            match owning_hash_map.reserve::<Key,Managed<Ticket>>(&mut map,64){.err(_)=>{return 4},.ok(_)=>{}}
        },
        .ok(_)=>{}
    }
    let query=Key{token:Ticket{code:90},id:0}
    match owning_hash_map.take::<Key,Managed<Ticket>>(&mut map,&query) {
        .err(_)=>{
            if owning_hash_map.len::<Key,Managed<Ticket>>(&map)!=5 || !owning_hash_map.contains::<Key,Managed<Ticket>>(&map,&query) {return 5}
            match owning_hash_map.take::<Key,Managed<Ticket>>(&mut map,&query){.err(_)=>{return 6},.ok(old)=>{if old.value.code!=97{return 7}}}
        },
        .ok(old)=>{if old.value.code!=97{return 8}}
    }
    if owning_hash_map.clear::<Key,Managed<Ticket>>(&mut map)!=4 {return 9}
    return 0
}
"#;

#[cfg(unix)]
#[test]
fn every_owning_hash_allocation_failure_returns_owners_and_preserves_map() {
    use std::{
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let dir = std::env::temp_dir().join(format!(
        "zil-owning-hash-fault-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let observer = dir.join("observer.o");
    let output = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/composed-store-closures/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = dir.join("clang.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
            quote(&clang),
            quote(&observer)
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.join("main.zil");
    fs::write(&source, FAILURE_MAP).unwrap();
    let field = |audit: &str, key: &str| -> usize {
        audit
            .split_once(&format!("\"{key}\":"))
            .unwrap()
            .1
            .split(|ch: char| !ch.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_path(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(dir.join("main")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let base = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(base.status.code(), Some(0), "{base:?}");
            let calls = field(&String::from_utf8_lossy(&base.stderr), "alloc_calls");
            assert!(calls >= 10, "{base:?}");
            for failure in 0..=calls {
                let output = Command::new(&report.artifacts["executable"])
                    .env("ZIL_TEST_FAIL_AT", failure.to_string())
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/failure={failure}: {output:?}"
                );
                let mut drops = output.stdout.clone();
                drops.sort_unstable();
                let mut expected = b"ABCDEabcdeZ".to_vec();
                expected.sort_unstable();
                assert_eq!(drops, expected, "failure={failure}");
                let stats = String::from_utf8(output.stderr).unwrap();
                assert_eq!(field(&stats, "live"), 0, "{stats}");
                assert_eq!(
                    field(&stats, "allocated"),
                    field(&stats, "frees"),
                    "{stats}"
                );
                assert_eq!(
                    field(&stats, "failures"),
                    usize::from(failure != 0),
                    "{stats}"
                );
            }
        }
    }

    let replacement =
        include_str!("../../../tests/fixtures/owning-hash-collections/replacement-recovery.zil");
    fs::write(&source, replacement).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_path(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(dir.join("replacement")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert!(output.stdout.is_empty(), "{output:?}");
            let stats = String::from_utf8(output.stderr).unwrap();
            assert_eq!(field(&stats, "live"), 0, "{stats}");
            assert_eq!(field(&stats, "failures"), 1, "{stats}");
            assert_eq!(
                field(&stats, "allocated"),
                field(&stats, "frees"),
                "{stats}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn buffered_short_io_and_retry_preserve_progress_and_close_once() {
    use std::{
        io::Write,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Stdio,
    };
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let dir = std::env::temp_dir().join(format!(
        "zil-buffered-short-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/buffered-io");
    let observer = dir.join("observer.o");
    let output = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(root.join("short_io.c"))
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = dir.join("clang.sh");
    fs::write(&wrapper,format!("#!/bin/sh\nexec {} \"$@\" -x none {} -Wl,--wrap=read -Wl,--wrap=write -Wl,--wrap=close\n",quote(&clang),quote(&observer))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.join("main.zil");
    fs::copy(root.join("short_io.zil"), &source).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_path(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(dir.join("main")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            for mode in 0..=3 {
                let mut child = Command::new(&report.artifacts["executable"])
                    .env("ZIL_BUFFER_MODE", mode.to_string())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(b"1234567").unwrap();
                let output = child.wait_with_output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/mode={mode}: {output:?}"
                );
                assert_eq!(
                    output.stdout,
                    if mode == 3 {
                        b"".as_slice()
                    } else {
                        b"ABCDEFGHIJ".as_slice()
                    },
                    "mode={mode}"
                );
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("closes=2 reads=4"),
                    "{output:?}"
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

fn float_buffer_typed() -> zil_ir::TypedModule {
    typed_stdlib(&[
        (
            "main",
            include_str!("../../../tests/fixtures/buffered-io/float_and_file.zil"),
        ),
        ("float", include_str!("../../../lib/std/float.zil")),
        ("io", include_str!("../../../lib/std/io.zil")),
        (
            "buffered_io",
            include_str!("../../../lib/std/buffered_io.zil"),
        ),
    ])
}

fn typed_stdlib(sources: &[(&str, &str)]) -> zil_ir::TypedModule {
    use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};
    let units: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(index, (name, source))| {
            SourceUnit::new(
                FileId(index as u32),
                ModuleId(index as u32),
                format!("{name}.zil"),
                *source,
            )
        })
        .collect();
    let parsed: Vec<_> = units
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
                    let name = import.path.last().unwrap();
                    Some(zil_ir::CoreImport {
                        alias: import.alias.clone().unwrap_or_else(|| name.clone()),
                        module: ModuleId(
                            sources.iter().position(|(key, _)| *key == name).unwrap() as u32
                        ),
                        span: item.span,
                    })
                })
                .collect()
        })
        .collect();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let modules =
        zil_expand::expand_program_with_limits_target(&units, &syntax, &imports, &limits, &target)
            .unwrap()
            .into_iter()
            .map(|(module, _)| module)
            .collect();
    zil_types::check_program(
        &zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::of(sources[0].1.as_bytes()),
            modules,
        },
        &target,
        &limits,
    )
    .unwrap()
    .module
}

#[test]
fn complete_float_buffer_consumer_has_fresh_exact_analysis_and_verifier_boundaries() {
    use zil_base::{Limits, TargetProfile};
    let module = float_buffer_typed();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let success = |verify: bool, n: usize| {
        let selected = Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        if verify {
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &selected).map(|_| ())
        } else {
            zil_analyze::analyze(&module, &selected).map(|_| ())
        }
    };
    for verify in [false, true] {
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        success(verify, high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match success(verify, middle) {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|error| error.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert!(high > 0);
        success(verify, high).unwrap();
        assert!(
            success(verify, high - 1)
                .unwrap_err()
                .iter()
                .all(|error| error.category == ResultCategory::ResourceLimit)
        );
        success(verify, high).unwrap();
        eprintln!(
            "complete float/buffer {} N={high}; N-1/fresh N checked; default10M raw/Wire checked",
            if verify { "Verify" } else { "Analyze" }
        );
    }
}

#[cfg(unix)]
#[test]
fn float_ieee_bits_roundtrip_rounding_and_locale_failure_cleanup() {
    use std::{
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let dir = std::env::temp_dir().join(format!(
        "zil-float-bits-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/numeric-text");
    let observer = dir.join("observer.o");
    let output = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(root.join("float_probe.c"))
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = dir.join("clang.sh");
    fs::write(&wrapper,format!("#!/bin/sh\nexec {} \"$@\" -x none {} -Wl,--wrap=newlocale -Wl,--wrap=uselocale -Wl,--wrap=freelocale\n",quote(&clang),quote(&observer))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.join("main.zil");
    fs::copy(root.join("float_roundtrip.zil"), &source).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_path(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(dir.join("main")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            for mode in 0..=2 {
                let output = Command::new(&report.artifacts["executable"])
                    .env("ZIL_FLOAT_MODE", mode.to_string())
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/mode={mode}: {output:?}"
                );
                assert!(output.stdout.is_empty(), "{output:?}");
                let stats = String::from_utf8_lossy(&output.stderr);
                assert!(
                    stats.contains(if mode == 0 {
                        "created=8002 released=8002 installs=8002 restores=8002"
                    } else if mode == 1 {
                        "created=0 released=0 installs=0 restores=0"
                    } else {
                        "created=1 released=1 installs=0 restores=0"
                    }),
                    "{stats}"
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owning_hash_value_windows_block_actual_owner_mutation_until_last_use() {
    let prefix = CONSUMER.split("fn main()->i32").next().unwrap();
    for operation in [
        "owning_hash_map.clear::<Key,Value>(&mut map)",
        "map=owning_hash_map.new::<Key,Value>()",
    ] {
        let source = format!(
            r#"{prefix}
fn main()->i32 {{
    var map=owning_hash_map.new::<Key,Value>()
    let incoming=owning_entries.Entry::<Key,Value>{{key:key(7),value:value(8)}}
    match owning_hash_map.insert::<Key,Value>(&mut map,incoming){{.err(_)=>{{return 1}},.ok(_)=>{{}}}}
    let query=key(7)
    match owning_hash_map.get::<Key,Value>(&map,&query) {{
        Option.None()=>{{return 2}},
        Option.Some(window)=>{{{operation};return i32(window.id)}}
    }}
}}
"#
        );
        let dir = std::env::temp_dir().join(format!(
            "zil-owning-hash-pin-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.zil");
        fs::write(&path, &source).unwrap();
        let report = compile_path(
            &path,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "operation={operation}: {}",
            report.to_json()
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn owning_hash_raw_and_wire_require_actual_store_definitions_and_selected_copy_arm() {
    use zil_base::{Limits, ResultCategory, TargetProfile};
    use zil_ir::{InstructionKind, PlaceProjection, Type};
    let module = typed_stdlib(&[
        ("main", CONSUMER),
        (
            "owning_hash_map",
            include_str!("../../../lib/std/owning_hash_map.zil"),
        ),
        (
            "owning_hash_set",
            include_str!("../../../lib/std/owning_hash_set.zil"),
        ),
        (
            "owning_entries",
            include_str!("../../../lib/std/owning_entries.zil"),
        ),
        (
            "owning_map",
            include_str!("../../../lib/std/owning_map.zil"),
        ),
        ("store", include_str!("../../../lib/std/store.zil")),
    ]);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let mut missing = module.clone();
    let owning=missing.resources.iter().position(|definition|matches!(&definition.payload,Type::OwnedStore{element} if !element.is_copy_value())).expect("real owning Entry Store");
    missing.resources.remove(owning);
    let mut wrong_arm = module;
    let mut changed = false;
    for function in &mut wrong_arm.functions {
        for instruction in function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            if let InstructionKind::BorrowPlaceLocal {
                local,
                path,
                mutable: false,
            } = &mut instruction.kind
                && matches!(path.as_slice(), [PlaceProjection::ErrorArm(false)])
                && matches!(&function.locals[local.0 as usize].ty,Type::ErrorUnion{error,result} if matches!(error.as_ref(),Type::Package{..}) && result.is_copy_value())
            {
                path[0] = PlaceProjection::ErrorArm(true);
                changed = true;
                break;
            }
        }
        if changed {
            break;
        }
    }
    assert!(changed, "actual Copy success-arm borrow must be exercised");
    for (label, attacked) in [
        ("missing-owning-store", missing),
        ("wrong-selected-arm", wrong_arm),
    ] {
        let errors = zil_analyze::analyze(&attacked, &limits).expect_err(label);
        assert!(
            errors
                .iter()
                .all(|error| error.category == ResultCategory::SafeReject
                    || error.category == ResultCategory::Unsupported),
            "{label}: {errors:?}"
        );
        let mut candidate = plan.clone();
        candidate.module_fingerprint = attacked.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&attacked, &candidate, &limits).unwrap();
        assert!(
            zil_verify::verify_module(attacked, candidate, &target, &limits).is_err(),
            "{label}"
        );
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "{label}"
        );
    }
}

#[test]
fn equal_typed_hash_map_inputs_cannot_substitute_the_wrong_borrowed_result_owner() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::InstructionKind;
    let source = r#"
import std.owning_hash_map
import std.owning_map
package Item{manages bytes:ByteBuffer;id:i64}
impl owning_map.KeyEq for Item{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Item{fn hash(self:&Self)->i64{return self.id}}
fn select<life a>(left:&'a owning_hash_map.OwningHashMap<Item,Item>,right:&owning_hash_map.OwningHashMap<Item,Item>,query:&Item)->Option<&'a Item>{
    let present=owning_hash_map.contains::<Item,Item>(right,query)
    return owning_hash_map.get::<Item,Item>(left,query)
}
fn main()->i32{return 0}
"#;
    let module = typed_stdlib(&[
        ("main", source),
        (
            "owning_hash_map",
            include_str!("../../../lib/std/owning_hash_map.zil"),
        ),
        (
            "owning_map",
            include_str!("../../../lib/std/owning_map.zil"),
        ),
        (
            "owning_entries",
            include_str!("../../../lib/std/owning_entries.zil"),
        ),
        ("store", include_str!("../../../lib/std/store.zil")),
    ]);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let getter = module
        .functions
        .iter()
        .find(|function| function.name.contains("_get_"))
        .expect("actual generic map getter")
        .id;
    let mut attacked = module;
    let function = attacked
        .functions
        .iter_mut()
        .find(|function| function.name == "select" || function.name.ends_with("_select"))
        .expect("actual ordinary select helper");
    let wrong = function.parameters[1];
    let wrong_value = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| {
            matches!(instruction.kind,InstructionKind::Load(local) if local==wrong)
                .then_some(instruction.result)
                .flatten()
        })
        .expect("actual right map input load");
    let mut changed = false;
    for instruction in function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        if let InstructionKind::Call {
            function: callee,
            arguments,
        } = &mut instruction.kind
            && *callee == getter
        {
            arguments[0] = wrong_value;
            changed = true;
            break;
        }
    }
    assert!(changed);
    assert!(
        zil_analyze::analyze(&attacked, &limits).is_err(),
        "wrong same-typed map must fail Analyze"
    );
    plan.module_fingerprint = attacked.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&attacked, &plan, &limits).unwrap();
    assert!(
        zil_verify::verify_module(attacked, plan, &target, &limits).is_err(),
        "wrong same-typed map must fail Verify"
    );
    assert!(
        zil_verify::verify_module_bytes(&wire, &target, &limits).is_err(),
        "wrong same-typed map must fail Wire Verify"
    );
}

#[test]
fn owning_hash_reserve_and_recovery_keep_raw_canonical_store_cleanup_checks() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::Type;
    let module = typed_stdlib(&[
        ("main", FAILURE_MAP),
        (
            "owning_hash_map",
            include_str!("../../../lib/std/owning_hash_map.zil"),
        ),
        (
            "owning_map",
            include_str!("../../../lib/std/owning_map.zil"),
        ),
        (
            "owning_entries",
            include_str!("../../../lib/std/owning_entries.zil"),
        ),
        ("store", include_str!("../../../lib/std/store.zil")),
    ]);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let owner=module.resources.iter().position(|definition|matches!(&definition.payload,Type::OwnedStore{element} if !element.is_copy_value())).expect("actual Store<Entry<owning K, owning V>> definition");
    let mut absent = module.clone();
    absent.resources.remove(owner);
    let mut mismatched = module;
    mismatched.resources[owner].payload = Type::OwnedStore {
        element: Box::new(Type::I64),
    };
    for (label, attacked) in [
        ("missing owning Store definition", absent),
        ("forged owning Store element", mismatched),
    ] {
        assert!(zil_analyze::analyze(&attacked, &limits).is_err(), "{label}");
        let mut candidate = plan.clone();
        candidate.module_fingerprint = attacked.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&attacked, &candidate, &limits).unwrap();
        assert!(
            zil_verify::verify_module(attacked, candidate, &target, &limits).is_err(),
            "{label}"
        );
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "{label}"
        );
    }
}

#[test]
fn consuming_hash_collections_transfers_real_entries_and_releases_each_owner_once() {
    run_output(
        r#"
import std.owning_hash_map
import std.owning_hash_set
import std.owning_entries
import std.owning_map
extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Key{manages token:Ticket;id:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Key{fn hash(self:&Self)->i64{return self.id}}
fn main()->i32 {
    var map=owning_hash_map.new::<Key,Managed<Ticket>>()
    let first=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{token:Ticket{code:65},id:1},value:Ticket{code:66}}
    match owning_hash_map.insert::<Key,Managed<Ticket>>(&mut map,first){.err(_)=>{return 1},.ok(_)=>{}}
    let second=owning_entries.Entry::<Key,Managed<Ticket>>{key:Key{token:Ticket{code:67},id:2},value:Ticket{code:68}}
    match owning_hash_map.insert::<Key,Managed<Ticket>>(&mut map,second){.err(_)=>{return 2},.ok(_)=>{}}
    let entries=owning_hash_map.into_entries::<Key,Managed<Ticket>>(map)
    if store_len(entries)!=2 {return 3}
    match store_pop(entries){.err(_)=>{return 4},.ok(old)=>{if old.key.id!=2 || old.value.code!=68{return 5}}}
    var set=owning_hash_set.new::<Key>()
    match owning_hash_set.insert::<Key>(&mut set,Key{token:Ticket{code:69},id:3}){.err(_)=>{return 6},.ok(_)=>{}}
    let keys=owning_hash_set.into_entries::<Key>(set)
    match store_pop(keys){.err(_)=>{return 7},.ok(old)=>{if old.key.id!=3{return 8}}}
    return 0
}
"#,
        "into-entries",
        b"CDEAB",
    );
}

#[test]
fn complete_owning_hash_consumer_keeps_default_and_exact_fresh_kernel_budgets() {
    use zil_base::{Limits, TargetProfile};
    let module = typed_stdlib(&[
        ("main", CONSUMER),
        (
            "owning_hash_map",
            include_str!("../../../lib/std/owning_hash_map.zil"),
        ),
        (
            "owning_hash_set",
            include_str!("../../../lib/std/owning_hash_set.zil"),
        ),
        (
            "owning_entries",
            include_str!("../../../lib/std/owning_entries.zil"),
        ),
        (
            "owning_map",
            include_str!("../../../lib/std/owning_map.zil"),
        ),
        ("store", include_str!("../../../lib/std/store.zil")),
    ]);
    let target = TargetProfile::host_llvm();
    let diagnostic = Limits {
        max_analysis_steps: 50_000_000,
        ..Limits::default()
    };
    let plan = zil_analyze::analyze(&module, &diagnostic).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &diagnostic).unwrap();
    let success = |verify: bool, n: usize| {
        let selected = Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        if verify {
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &selected).map(|_| ())
        } else {
            zil_analyze::analyze(&module, &selected).map(|_| ())
        }
    };
    let mut boundaries = Vec::new();
    for verify in [false, true] {
        let (mut low, mut high) = (0, diagnostic.max_analysis_steps);
        success(verify, high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match success(verify, middle) {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|error| error.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        success(verify, high).unwrap();
        assert!(
            success(verify, high - 1)
                .unwrap_err()
                .iter()
                .all(|error| error.category == ResultCategory::ResourceLimit)
        );
        success(verify, high).unwrap();
        eprintln!(
            "complete owning hash {} N={high}; N-1/fresh N checked",
            if verify { "Verify" } else { "Analyze" }
        );
        boundaries.push(high);
    }
    assert!(
        boundaries
            .iter()
            .all(|n| *n <= Limits::default().max_analysis_steps),
        "complete original consumer must fit default; diagnostic boundaries={boundaries:?}"
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).unwrap();
}

#[test]
fn every_integer_width_text_conversion_checks_its_range_and_roundtrips() {
    run(
        r#"
import std.int
fn main()->i32 {
    match int.parse_u32("4294967295") {.err(_)=>{return 1},.ok(value)=>{if value!=4294967295u32{return 2}}}
    match int.parse_u32("4294967296") {.err(_)=>{},.ok(_)=>{return 3}}
    let unsigned=int.format_u32(4294967295u32)
    match int.parse_u32(&unsigned.bytes[..unsigned.len]) {.err(_)=>{return 4},.ok(value)=>{if value!=4294967295u32{return 5}}}
    match int.parse_i8("-128") {.err(_)=>{return 6},.ok(value)=>{if value!=i8(-128){return 7}}}
    match int.parse_i8("128") {.err(_)=>{},.ok(_)=>{return 8}}
    match int.parse_i8("-129") {.err(_)=>{},.ok(_)=>{return 9}}
    let tiny=int.format_i8(i8(-128))
    match int.parse_i8(&tiny.bytes[..tiny.len]) {.err(_)=>{return 10},.ok(value)=>{if value!=i8(-128){return 11}}}
    match int.parse_i16("-32768") {.err(_)=>{return 12},.ok(value)=>{if value!=i16(-32768){return 13}}}
    match int.parse_i16("32768") {.err(_)=>{},.ok(_)=>{return 14}}
    match int.parse_i16("-32769") {.err(_)=>{},.ok(_)=>{return 15}}
    let small=int.format_i16(i16(32767))
    match int.parse_i16(&small.bytes[..small.len]) {.err(_)=>{return 16},.ok(value)=>{if value!=i16(32767){return 17}}}
    let minimum= -i64(u64(~0usize)>>1u64)-1
    let word=int.format_isize(isize(minimum))
    match int.parse_isize(&word.bytes[..word.len]) {.err(_)=>{return 18},.ok(value)=>{if value!=isize(minimum){return 19}}}
    match int.parse_isize("9223372036854775808") {.err(_)=>{},.ok(_)=>{return 20}}
    return 0
}
"#,
        "all-integer-widths",
    );
}
