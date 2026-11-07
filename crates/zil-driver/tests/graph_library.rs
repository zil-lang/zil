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
const GRAPH: &str = r#"import std.graph
extern "C" fn getchar()->i32;
extern "C" fn putchar(value:i32)->i32;
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
    graph.link(&mut g,last,first)?
    graph.link(&mut g,first,last)?
    if graph.edge_count(&g)!=n+1 {return 1}
    let old_slot=graph.physical_slot(&g,last)
    if !graph.remove(&mut g,last) {return 2}
    if graph.contains(&g,last) {return 3}
    if graph.edge_count(&g)!=n-2 {return 4}
    if graph.collect(&mut g)?!=1 {return 5}
    let replacement=graph.add(&mut g,99)?
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
    match graph.add(&mut g,100) {
        .err(_)=>{},
        .ok(_)=>{return 16}
    }
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
fn dynamic_cycle_shared_target_deletion_and_ticket_history_execute() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-graph-library-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            let source = dir.join("graph_library.zil");
            fs::write(&source, GRAPH).unwrap();
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
            for input in [b'3', b'5', b'8'] {
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
