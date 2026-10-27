#![cfg(test)]
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command, sync::atomic::{AtomicUsize, Ordering}};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project};
use zil_ir::{Instruction, InstructionKind as I, Type};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let dir = PathBuf::from(std::env::var_os("REVIEW171_OUT").expect("set reviewer evidence output"))
        .join(format!("{label}-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(&dir).unwrap(); dir
}
fn project(files: &[(&str,&str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files.iter().enumerate().map(|(i,(name,text))|SourceUnit::new(FileId(i as u32),ModuleId(i as u32),format!("{name}.zil"),*text)).collect(),
        paths: files.iter().enumerate().skip(1).map(|(i,(name,_))|(vec![name.to_string()],ModuleId(i as u32))).collect::<BTreeMap<_,_>>(),
    }
}
fn check(label: &str, files: &[(&str,&str)]) -> CompileReport {
    let dir=directory(label);
    for (name,text) in files {fs::write(dir.join(format!("{name}.zil")),text).unwrap();}
    let report=compile_project(&project(files),&CompileOptions{emit_dir:Some(dir.join("phases")),..CompileOptions::default()});
    fs::write(dir.join("compile.json"),report.to_json()).unwrap(); report
}
fn rejection(label:&str,files:&[(&str,&str)],category:ResultCategory,message:&str) {
    let report=check(label,files);
    assert_eq!(report.status,category,"{}",report.to_json());
    assert_eq!(report.phase,Phase::Verify,"{}",report.to_json());
    assert!(report.diagnostics.iter().any(|d|d.message.contains(message)),"{}",report.to_json());
    println!("{label}: {:?} {:?} {:?}",report.phase,report.status,report.diagnostics.iter().map(|d|(&d.code,&d.message)).collect::<Vec<_>>());
}
fn native(label:&str,files:&[(&str,&str)],stdout:&[u8]) {
    for optimization in [Optimization::O0,Optimization::O2] {
        let dir=directory(label);let executable=dir.join("program");
        for (name,text) in files {fs::write(dir.join(format!("{name}.zil")),text).unwrap();}
        let report=compile_project(&project(files),&CompileOptions{stop_after:Stage::Build,optimization,output:Some(executable.clone()),emit_dir:Some(dir.join("phases")),..CompileOptions::default()});
        fs::write(dir.join("compile.json"),report.to_json()).unwrap();
        assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
        assert_eq!(report.verification_runs,1);
        let output=Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"),&output.stdout).unwrap();fs::write(dir.join("stderr"),&output.stderr).unwrap();
        fs::write(dir.join("native.json"),format!("{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"stdout\":{:?}}}\n",output.status.code().unwrap_or(-999),String::from_utf8_lossy(&output.stdout))).unwrap();
        assert_eq!(output.status.code(),Some(0),"{output:?}");assert_eq!(output.stdout,stdout);
        println!("{label}/{optimization:?}: exit0 stdout={:?}",String::from_utf8_lossy(stdout));
    }
}

const R:&str=r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}"#;

#[test]
fn nested_copy_payload_head_uses_natural_field_offsets() {
    native("nested-layout",&[("main",r#"
extern "C" fn putchar(x:i32)->i32;
struct Data{small:u8;n:i32;lanes:[i32;2]}
resource R{code:i32;data:Data}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn inspect(h:&R)->i32{let p=&h.data.n;return *p+h.data.lanes[1]}
fn main()->i32{let r=R{code:65,data:Data{small:u8(1),n:20,lanes:[2,22]}};return inspect(&r)-42}
"#)],b"A");
}

#[test]
fn projected_field_keeps_head_window_until_its_actual_last_use() {
    let blocked=format!("{R}fn main()->i32{{let r=R{{code:65,n:42}};let p=&r;let q=&p.n;r.n=43;return *q}}");
    rejection("projected-window",&[("main",&blocked)],ResultCategory::SafeReject,"window");
    let after=format!("{R}fn main()->i32{{let r=R{{code:65,n:42}};let p=&r;let q=&p.n;if *q!=42{{return 1}};r.n=43;return r.n-43}}");
    native("projected-last-use",&[("main",&after)],b"A");
}

#[test]
fn branching_internal_descriptor_callback_exports_effect_to_other_module() {
    let model=r#"extern "C" fn putchar(x:i32)->i32;pub resource R{code:i32;pub n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}pub fn make(code:i32)->R contract{result(create)}{return R{code:code,n:42}}"#;
    let writer="import model;fn hidden(d:model.R,flag:bool){if flag{let callback=()=>{d.n=43};callback()}else{let callback=()=>{d.n=44};callback()}}pub fn relay(d:model.R,flag:bool){hidden(d,flag)}";
    let blocked="import model;import writer;fn main()->i32{let r=model.make(65);let h=&r;writer.relay(r,true);return h.n}";
    rejection("branch-helper",&[("main",blocked),("model",model),("writer",writer)],ResultCategory::SafeReject,"window");
    let other="import model;import writer;fn main()->i32{let r=model.make(65);let other=model.make(66);let h=&r;writer.relay(other,true);return h.n+other.n-85}";
    native("branch-helper-disjoint",&[("main",other),("model",model),("writer",writer)],b"BA");
}

#[test]
fn final_mixed_capture_multilevel_reborrow_is_explicitly_closed_only_in_head_lane() {
    let helpers="fn apply<F:Fn()->i32>(f:&F)->i32{return f()}";
    let closed=format!("{R}{helpers}fn main()->i32{{let r=R{{code:65,n:42}};let d=r;let p=&r;let q=&p.n;let f=()=>{{d.n=43;return *q}};let shared=&f;let copied=shared;return apply(copied)}}");
    rejection("mixed-head-closed",&[("main",&closed)],ResultCategory::Unsupported,"per-capture effect contract");
    let old=format!("{R}{helpers}fn main()->i32{{let r=R{{code:65,n:1}};let d=r;let n=42;let q=&n;let f=()=>{{d.n=43;return *q}};let shared=&f;let copied=shared;return apply(copied)-42}}");
    native("mixed-no-head-preserved",&[("main",&old)],b"A");
}

#[test]
fn raw_and_wire_projected_roots_and_exact_verification_budget_are_independent() {
    let source=SourceUnit::new(FileId(0),ModuleId(0),"raw-review171.zil","resource R{n:i32}fn main()->i32{let r=R{n:42};let p=&r;let q=&p.n;return *q-42}");
    let syntax=zil_syntax::parse(&source).into_result().unwrap();
    let core=zil_expand::expand(&source,&syntax).unwrap();
    let defaults=Limits::default();let target=TargetProfile::host_llvm();
    let module=zil_types::check(&core,&target,&defaults).unwrap();
    let plan=zil_analyze::analyze(&module,&defaults).unwrap();
    zil_verify::verify_module(module.clone(),plan.clone(),&target,&defaults).unwrap();
    let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&defaults).unwrap();
    let (mut low,mut high)=(0,defaults.max_analysis_steps);
    while low+1<high {
        let middle=(low+high)/2;let limits=Limits{max_analysis_steps:middle,..defaults.clone()};
        match zil_verify::verify_module(module.clone(),plan.clone(),&target,&limits) {
            Ok(_)=>high=middle,
            Err(errors)=>{assert!(errors.iter().all(|d|d.category==ResultCategory::ResourceLimit),"{errors:?}");low=middle;}
        }
    }
    for allowance in [high,high-1] {
        let limits=Limits{max_analysis_steps:allowance,..defaults.clone()};
        for (route,result) in [("raw",zil_verify::verify_module(module.clone(),plan.clone(),&target,&limits)),("wire",zil_verify::verify_module_bytes(&bytes,&target,&limits))] {
            if allowance==high {result.unwrap();}else{let errors=result.unwrap_err();assert!(errors.iter().all(|d|d.category==ResultCategory::ResourceLimit),"{errors:?}");}
            println!("projected-head budget {route} N={allowance}: {}",if allowance==high{"Pass"}else{"ResourceLimit"});
        }
    }
    let mut forged=module;
    let f=forged.functions.iter_mut().find(|f|f.name=="main").unwrap();
    let owner=f.locals.iter().find(|l|matches!(l.ty,Type::Managed(_))).unwrap().id;
    let block=&mut f.blocks[0];
    let position=block.instructions.iter().position(|i|matches!(i.kind,I::AddressField{..})).unwrap()+1;
    let span=block.instructions[position-1].span;
    block.instructions.insert(position,Instruction{result:None,kind:I::DropManaged{local:owner},span});
    let mut rebound=plan;rebound.module_fingerprint=forged.fingerprint();
    let bytes=zil_ir::wire::encode_module_candidate(&forged,&rebound,&defaults).unwrap();
    let dir=directory("raw-projected-drop");fs::write(dir.join("input.zil"),&source.text).unwrap();
    fs::write(dir.join("candidate.bin"),&bytes).unwrap();
    for (route,result) in [("raw",zil_verify::verify_module(forged,rebound,&target,&defaults)),("wire",zil_verify::verify_module_bytes(&bytes,&target,&defaults))] {
        let errors=result.unwrap_err();fs::write(dir.join(format!("{route}.txt")),format!("{errors:#?}")).unwrap();
        assert!(errors.iter().any(|d|d.phase==Phase::Verify&&d.code=="E-BORROW-CONFLICT"),"{errors:?}");
        println!("drop while only projected child survives/{route}: {errors:?}");
    }
}
