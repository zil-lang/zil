//! Checked graph-relative tickets over actual owning Store<Node> storage.
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_path};

static NEXT: AtomicU64 = AtomicU64::new(0);
const GRAPH_HELPER: &str = r#"import std.graph
pub struct Saved { pub from:graph.NodeKey; pub to:graph.NodeKey }
pub fn save(from:graph.NodeKey,to:graph.NodeKey)->Saved {return Saved{from:from,to:to}}
pub fn source(saved:Saved)->graph.NodeKey {return saved.from}
pub fn target(saved:Saved)->graph.NodeKey {return saved.to}
"#;
const PAYLOAD_SMOKE: &str = r#"import std.graph
fn check()->graph.Error!i32 {
    var graph_value=graph.with_capacity(1,0,1)?
    let payload=bytes_new()
    if !bytes_reserve(payload,2) || !bytes_push(payload,65u8) || !bytes_push(payload,66u8) {return 1}
    match graph.add_with_payload(&mut graph_value,5,give payload) {
        .err(failed)=>{let returned=graph.recover_payload(failed);return 2},
        .ok(key)=>{
            match graph.payload_len(&graph_value,key) {
                Option.None()=>{return 3},
                Option.Some(length)=>{if length!=2 {return 4}}
            }
            match graph.payload_byte(&graph_value,key,1) {
                Option.None()=>{return 5},
                Option.Some(value)=>{if value!=66u8 {return 6}}
            }
            return 0
        }
    }
}
fn main()->i32 {
    match check() {
        .err(_)=>{return 7},
        .ok(status)=>{return status}
    }
}"#;
const GRAPH: &str = r#"import std.graph
import graph_helper
extern "C" fn getchar()->i32;
extern "C" fn putchar(value:i32)->i32;
fn reuse_failed(failed:graph.AddError)->graph.Error!bool {
    match failed.reason {
        graph.AddFailure.Capacity()=>{},
        graph.AddFailure.Allocation()=>{return false}
    }
    let returned=graph.recover_payload(failed)
    if bytes_len(returned)!=1 || bytes_view(returned)[0]!=81u8 {return false}
    var other=graph.with_capacity(1,0,1)?
    match graph.add_with_payload(&mut other,101,give returned) {
        .err(failed)=>{let recovered=graph.recover_payload(failed);return false},
        .ok(key)=>{
            match graph.payload_byte(&other,key,0) {
                Option.None()=>{return false},
                Option.Some(value)=>{return value==81u8}
            }
        }
    }
}
fn probe_capacity(graph_value:&mut graph.Graph)->graph.Error!bool {
    let extra=bytes_new()
    if !bytes_reserve(extra,1) || !bytes_push(extra,81u8) {return false}
    let overflow=graph.add_with_payload(graph_value,100,give extra)
    match overflow {
        .err(failed)=>{return reuse_failed(failed)?},
        .ok(_)=>{return false}
    }
}
fn run(n:i64)->graph.Error!i32 {
    var g=graph.with_capacity(n,n+2,n+1)?
    var first=graph.NodeKey{ticket:-1}
    var last=first
    var previous=first
    var at:i64=0
    while at<n {
        let key=graph.add(&mut g,at)?
        if at==0 {first=key}
        if at>0 {graph.link(&mut g,previous,key)?}
        previous=key
        last=key
        at+=1
    }
    let saved=graph_helper.save(last,first)
    graph.link(&mut g,graph_helper.source(saved),graph_helper.target(saved))?
    graph.link(&mut g,graph_helper.target(saved),graph_helper.source(saved))?
    if graph.edge_count(&g)!=n+1 {return 1}
    let old_slot=graph.physical_slot(&g,last)
    if !graph.remove(&mut g,last) {return 2}
    if graph.contains(&g,last) {return 3}
    if graph.edge_count(&g)!=n-2 {return 4}
    if graph.collect(&mut g)?!=1 {return 5}
    let payload=bytes_new()
    if !bytes_reserve(payload,2) || !bytes_push(payload,65u8) || !bytes_push(payload,66u8) {return 20}
    var replacement=first
    match graph.add_with_payload(&mut g,99,give payload) {
        .err(failed)=>{let recovered=graph.recover_payload(failed);return 21},
        .ok(key)=>{replacement=key}
    }
    match graph.payload_len(&g,replacement) {
        Option.None()=>{return 22},
        Option.Some(length)=>{if length!=2 {return 23}}
    }
    match graph.payload_byte(&g,replacement,0) {
        Option.None()=>{return 24},
        Option.Some(value)=>{if value!=65u8 {return 25}}
    }
    match graph.payload_byte(&g,replacement,1) {
        Option.None()=>{return 26},
        Option.Some(value)=>{if value!=66u8 {return 27}}
    }
    if replacement.ticket==last.ticket {return 6}
    if graph.contains(&g,last) {return 7}
    if graph.edge_count(&g)!=n-2 {return 8}
    match old_slot {
        Option.None()=>{return 9},
        Option.Some(slot)=>{
            match graph.physical_slot(&g,replacement) {
                Option.None()=>{return 10},
                Option.Some(new_slot)=>{if slot!=new_slot {return 11}}
            }
        }
    }
    graph.link(&mut g,replacement,first)?
    if graph.edge_count(&g)!=n-1 {return 12}
    if graph.node_count(&g)!=n || graph.issued_count(&g)!=n+1 {return 13}
    match graph.get(&g,replacement) {
        Option.None()=>{return 14},
        Option.Some(node)=>{if node.value!=99 {return 15}}
    }
    if !probe_capacity(&mut g)? {return 29}
    if graph.node_count(&g)!=n || graph.issued_count(&g)!=n+1 {return 17}
    match graph.link(&mut g,last,first) {
        .err(_)=>{},
        .ok(_)=>{return 18}
    }
    if graph.edge_count(&g)!=n-1 {return 19}
    unsafe{putchar(80)}
    return 0
}
fn main()->i32 {
    var n=0
    unsafe{n=getchar()}
    run(i64(n-48))
    return 0
}"#;

#[test]
fn payload_api_runs_with_default_analysis_budget() {
    let dir = std::env::temp_dir().join(format!(
        "zil-graph-smoke-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("graph_smoke.zil");
    fs::write(&source, PAYLOAD_SMOKE).unwrap();
    let executable = dir.join("graph");
    let report = compile_path(
        &source,
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(executable.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dynamic_cycle_shared_target_deletion_and_ticket_history_execute() {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-graph-library-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            let source = dir.join("graph_library.zil");
            fs::write(&source, GRAPH).unwrap();
            fs::write(dir.join("graph_helper.zil"), GRAPH_HELPER).unwrap();
            let executable = dir.join("graph");
            let report = compile_path(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
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
            for input in *b"358" {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(&[input]).unwrap();
                let result = child.wait_with_output().unwrap();
                assert!(
                    result.status.success(),
                    "{backend:?} {optimization:?} {input} {result:?}"
                );
                assert_eq!(result.stdout, b"P", "{backend:?} {optimization:?} {input}");
                assert!(result.stderr.is_empty());
            }
            fs::remove_dir_all(&dir).unwrap();
        }
    }
}

#[test]
fn allocation_failure_releases_prepared_graph_and_nodes() {
    for (failure_size, expected) in [
        (1usize, b"0".as_slice()),
        (2, b"0".as_slice()),
        (13, b"P0".as_slice()),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-graph-failure-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            let allocator =
                include_str!("../../../tools/byte-buffer/owned-storage-abi/fault-allocator.c")
                    .replace(
                        "static int fail_size = 13;",
                        &format!("static int fail_size = {failure_size};"),
                    );
            let allocator_path = dir.join("fault-allocator.c");
            fs::write(&allocator_path, allocator).unwrap();
            let wrapper = dir.join("clang-wrapper.py");
            fs::write(&wrapper, format!(
                "#!/usr/bin/env python3\nimport os,sys\nos.execvp('clang',['clang']+sys.argv[1:]+['-x','c',{:?},'-Wl,--wrap=malloc','-Wl,--wrap=realloc','-Wl,--wrap=free'])\n",
                allocator_path.to_string_lossy()
            )).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let source = dir.join("graph_failure.zil");
            let instrumented = GRAPH
                .replace(
                    "extern \"C\" fn getchar()->i32;",
                    "extern \"C\" fn getchar()->i32;extern \"C\" fn byte_test_live()->i32;",
                )
                .replace(
                    "run(i64(n-48))\n    return 0",
                    "run(i64(n-48))\n    unsafe{putchar(48+byte_test_live())}\n    return 0",
                );
            fs::write(&source, instrumented).unwrap();
            fs::write(dir.join("graph_helper.zil"), GRAPH_HELPER).unwrap();
            let executable = dir.join("graph");
            let report = compile_path(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::Clang,
                    optimization,
                    clang: Some(wrapper),
                    output: Some(executable.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let mut child = Command::new(&executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(b"3").unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(
                output.stdout, expected,
                "failure_size={failure_size} {optimization:?}"
            );
            assert!(output.stderr.is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
