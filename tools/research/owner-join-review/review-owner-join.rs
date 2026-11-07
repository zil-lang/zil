use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId,ModuleId,SourceUnit,ResultCategory};
use zil_driver::{compile_source,CompileOptions,Stage,NativeBackend,Optimization};
fn main(){
 let out=PathBuf::from(std::env::args().nth(1).expect("output"));fs::create_dir_all(&out).unwrap();
 let original=PathBuf::from("/tmp/review240-boundaries/results-Q-b51-diagnostic-final");
 let mut sources:Vec<(String,String,bool)>=Vec::new();
 for name in ["diagnostic_corrected_owner_branch","diagnostic_corrected_address_description","diagnostic_final_helper_no_call"] {
  sources.push((name.into(),fs::read_to_string(original.join(name).join("source.zil")).unwrap(),true));
 }
 let head="resource R{n:i32}enum E{V(manages R,manages R)}fn sink(give r:R){}";
 for (name,body,accept) in [
  ("current-owner-join","var saved=give a;if flag{saved=give b}else{}return saved.n",true),
  ("desc-after-join","var saved=give a;if flag{saved=give b}else{}let d=saved;return d.n",true),
  ("current-owner-give","var saved=give a;if flag{saved=give b}else{}sink(give saved);return 0",true),
  ("desc-before-join-stale","var saved=give a;let d=saved;if flag{saved=give b}else{}return d.n",false),
  ("desc-before-no-change","var saved=give a;let d=saved;if flag{}else{}return d.n",true),
  ("owner-moved-one-path","var saved=give a;if flag{sink(give saved)}else{}return saved.n",false),
  ("alias-cannot-be-given","var saved=give a;if flag{saved=give b}else{}let d=saved;sink(give d);return 0",false),
  ("desc-join-then-give-stale","var saved=give a;if flag{saved=give b}else{}let d=saved;sink(give saved);return d.n",false),
  ("nested-join-current","var saved=give a;if flag{saved=give b;if flag{}else{}}else{}return saved.n",true),
  ("branch-shadow-keeps-owner","var saved=give a;if flag{let saved=give b;let n=saved.n}else{}return saved.n",true),
  ("old-input-desc-stale","let d=a;var saved=give a;if flag{saved=give b}else{}return d.n",false),
 ]{
  sources.push((name.into(),format!("{head}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{{match e{{E.V(a,b)=>{{{body}}}}}}}fn main(){{}}"),accept));
 }
 let mut lines=Vec::new();
 for(name,text,accept)in sources{
  let dir=out.join(&name);fs::create_dir_all(&dir).unwrap();fs::write(dir.join("source.zil"),&text).unwrap();
  fs::write(dir.join("expected.txt"),if accept{"Analyze Success\n"}else if name=="alias-cannot-be-given"{"Expand Unsupported E-UNSUPPORTED: generic managed give requires a direct Package manages field\n"}else{"Expand SafeReject E-GENERIC-ARGUMENT: invalid actual carrier/description\n"}).unwrap();
  let report=compile_source(&SourceUnit::new(FileId(0),ModuleId(0),"owner-join.zil",text),&CompileOptions{stop_after:Stage::Analyze,emit_dir:Some(dir.join("phases")),..CompileOptions::default()});
  fs::write(dir.join("report.json"),report.to_json()).unwrap();
  if accept {assert_eq!(report.status,ResultCategory::Success,"{name}:{}",report.to_json());}
  else {assert_ne!(report.status,ResultCategory::Success,"{name}: invalid source accepted");let expected_code=if name=="alias-cannot-be-given"{"E-UNSUPPORTED"}else{"E-GENERIC-ARGUMENT"};assert!(report.diagnostics.iter().any(|d|d.phase==zil_base::Phase::Expand&&d.code==expected_code),"{name}:{}",report.to_json());if name=="alias-cannot-be-given"{assert!(report.diagnostics.iter().any(|d|d.message=="generic managed give requires a direct Package manages field"));}}
  lines.push(format!("{name}:{:?}",report.status));
 }
 // Two flags observe current owner A/B; resource Drop prints the real sequence.
 // At flag=true assignment drops A, then exit drops B; false exit drops A,B.
 for flag in [false,true]{for opt in [Optimization::O0,Optimization::O2]{
  let name=format!("native-{flag}-{opt:?}");let dir=out.join(&name);fs::create_dir_all(&dir).unwrap();
  let expected=if flag{66}else{65};
  let text=format!("extern \"C\" fn putchar(n:i32)->i32;resource R{{n:i32}}impl Drop for R{{fn drop(self:Self){{unsafe{{putchar(.n)}}}}}}enum E{{V(manages R,manages R)}}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{{match e{{E.V(a,b)=>{{var saved=give a;if flag{{saved=give b}}else{{}}return saved.n}}}}}}fn main()->i32{{return use_value(()=>0,E.V(R{{n:65}},R{{n:66}}),{flag})-{expected}}}");
  fs::write(dir.join("source.zil"),&text).unwrap();fs::write(dir.join("expected.txt"),"stdout=AB\nexit=0\n").unwrap();
  let report=compile_source(&SourceUnit::new(FileId(0),ModuleId(0),"owner-join-native.zil",text),&CompileOptions{stop_after:Stage::Build,backend:NativeBackend::LlvmApi,optimization:opt,emit_dir:Some(dir.join("phases")),output:Some(dir.join("program")),..CompileOptions::default()});
  fs::write(dir.join("report.json"),report.to_json()).unwrap();assert_eq!(report.status,ResultCategory::Success,"{name}:{}",report.to_json());assert_eq!(report.verification_runs,1);assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
  let result=Command::new(&report.artifacts["executable"]).output().unwrap();fs::write(dir.join("stdout"),&result.stdout).unwrap();fs::write(dir.join("stderr"),&result.stderr).unwrap();fs::write(dir.join("exit.txt"),format!("{:?}",result.status.code())).unwrap();assert_eq!(result.status.code(),Some(0));assert_eq!(result.stdout,b"AB");assert!(result.stderr.is_empty());lines.push(format!("{name}:AB/0"));
 }}
 fs::write(out.join("RESULTS.txt"),format!("{}\n",lines.join("\n"))).unwrap();println!("{}",lines.join("\n"));
}
