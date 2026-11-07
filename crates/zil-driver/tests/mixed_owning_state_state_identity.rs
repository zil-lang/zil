//! Independent consumers check family identity across constructors and data shapes.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Optimization, ProjectSources, Stage, compile_project};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn run(source: &str, expected: i32) {
    let project = ProjectSources {
        root: ModuleId(0),
        paths: Default::default(),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "review197.zil",
            source,
        )],
    };
    for optimization in [Optimization::O0, Optimization::O2] {
        let directory = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "review197-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("source.zil"), source).unwrap();
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(directory.join("program")),
                emit_dir: Some(directory.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(directory.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(
            directory.join("native.txt"),
            format!("optimization={optimization:?}\nexpected={expected}\nactual={output:?}\n"),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(expected), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
        if std::env::var_os("ZIL_TEST_REPORT_DIR").is_none() {
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
#[test]
fn two_constructors_and_lexical_binders_share_complete_data_families() {
    run(
        r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn first<T:Copy,F:Fn()->i32>(f:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:f,value:value}}
fn second<G:Fn()->i32,U:Copy>(g:G,value:U)->State<U,G>{return State::<U,G>{bytes:bytes_new(),callback:g,value:value}}
fn pick<H:Fn()->i32,V:Copy>(choose:bool,f:H,value:V)->State<V,H>{if choose{return first(f,value)}return second(f,value)}
fn run<T:Copy,F:Fn()->i32>(state:State<T,F>)->T{state.callback();return state.value}
fn factory()->impl Fn()->i32{return move()=>2}
fn main()->i32{let x=run(pick(true,factory(),(i64(9),i32(2))));let y=run(pick(false,factory(),[i64(4),i64(5)]));return i32(x.0)+x.1+i32(y[0])+i32(y[1])-20}
"#,
        0,
    );
}
#[test]
fn mixed_state_copy_array_and_callback_combine_with_break_and_continue() {
    run(
        r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:FnMut(&mut T)->i32>(f:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:f,value:value}}
fn factory()->impl FnMut(&mut [i32;2])->i32{var count=0;return move(value:&mut [i32;2])=>{count+=1;(*value)[0]+=1;return count}}
fn run<T:Copy,F:FnMut(&mut T)->i32>(state:State<T,F>)->i32{var current=state;var total=0;for i in 0..7{if i==1{continue}if i==4{break}let f=&mut current.callback;let data=&mut current.value;total+=f(data)}return total}
fn main()->i32{return run(make(factory(),[10,20]))-6}
"#,
        0,
    );
}
