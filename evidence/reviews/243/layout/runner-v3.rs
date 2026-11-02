#[cfg(test)]
mod tests {
use std::{collections::BTreeMap,fs,io::Write,path::{Path,PathBuf},process::{Command,Stdio}};
use zil_base::{FileId,ModuleId,SourceUnit,TargetProfile,Limits,ResultCategory};
use zil_driver::{CompilationSession,ProjectSources,CompileOptions,Stage,NativeBackend,Optimization};
use zil_ir::*;
struct Case{name:&'static str,files:&'static[(&'static str,&'static str)],rows:&'static[(&'static str,&'static str,i32)]}
include!("cases.rs");
fn base()->PathBuf{PathBuf::from("/tmp/review243-layout/results")}
fn project(c:&Case)->ProjectSources{
 let mut files=c.files.to_vec();files.sort_by_key(|(n,_)|if *n=="main.zil"{String::new()}else{n.to_string()});
 let mut paths=BTreeMap::new();let mut modules=Vec::new();
 for (i,(name,text)) in files.iter().enumerate(){let id=ModuleId(i as u32);let path=if c.name.starts_with("context_"){"review243-context.zil".to_string()}else{format!("{}/{name}",c.name)};
  if i!=0{paths.insert(vec![name.trim_end_matches(".zil").to_string()],id);}
  modules.push(SourceUnit::new(FileId(i as u32),id,path,*text));
 }
 ProjectSources{root:ModuleId(0),modules,paths}
}
fn typed(text:&str)->TypedModule{
 let u=SourceUnit::new(FileId(0),ModuleId(0),"review243-context.zil",text);
 let ast=zil_syntax::parse(&u).into_result().unwrap();let core=zil_expand::expand(&u,&ast).unwrap();
 zil_types::check(&core,&TargetProfile::host_llvm(),&Limits::default()).unwrap()
}
fn find_choice(ty:&Type)->Option<Type>{match ty{
 Type::Enum{name,..} if name.ends_with(":Choice")=>Some(ty.clone()),
 Type::Package{fields,..}|Type::Record{fields,..}=>fields.iter().find_map(|(_,t)|find_choice(t)),
 Type::Managed(t)|Type::Desc(t)=>find_choice(t),_=>None}}
fn context_metadata(){
 let a=typed(CASES.iter().find(|c|c.name=="context_a").unwrap().files[0].1);
 let b=typed(CASES.iter().find(|c|c.name=="context_b").unwrap().files[0].1);
 let ca=a.functions.iter().flat_map(|f|&f.values).find_map(find_choice).unwrap();
 let cb=b.functions.iter().flat_map(|f|&f.values).find_map(find_choice).unwrap();assert_eq!(ca,cb);
 let ra=a.resources.iter().find(|r|r.name.ends_with(":R")).unwrap();let rb=b.resources.iter().find(|r|r.name.ends_with(":R")).unwrap();
 assert_eq!(ra.payload,rb.payload);assert_ne!(ra.teardown,rb.teardown);
 fs::write(base().join("context-binding.txt"),format!("same_complete_type={ca:#?}\nA={ra:#?}\nB={rb:#?}\n")).unwrap();
}
fn raw_unbound(){
 let text=CASES.iter().find(|c|c.name=="mixed_i32").unwrap().files[0].1;
 let module=typed(text);let limits=Limits::default();let target=module.target.clone();let plan=zil_analyze::analyze(&module,&limits).unwrap();
 let verified=zil_verify::verify_module(module,plan,&target,&limits).unwrap();let lir=zil_lower::lower(&verified).unwrap();
 let positive=zil_backend_llvm::emit(&verified,&lir).unwrap();fs::write(base().join("raw-positive.ll"),positive).unwrap();
 let mut mutants=vec![];let mut m=lir.clone();m.target=TargetProfile::windows_x64_llvm();mutants.push(("target",m));
 let mut m=lir.clone();m.resources.iter_mut().find(|r|r.teardown.is_some()).unwrap().teardown=None;mutants.push(("resource",m));
 let mut m=lir.clone();let mut changed=false;
 for f in &mut m.functions{for t in &mut f.values{if let Type::Enum{name,..}=t{name.push_str("_forged");changed=true;break;}}if changed{break}}
 assert!(changed);mutants.push(("complete_enum_type",m));
 let mut m=lir.clone();let mut changed=false;
 for f in &mut m.functions{for b in &mut f.blocks{for i in &mut b.instructions{if let InstructionKind::UnpackVariant{fields,..}=&mut i.kind{if fields.len()==3{fields.swap(0,2);changed=true;break;}}}if changed{break}}if changed{break}}
 assert!(changed);mutants.push(("unpack_mapping",m));
 let mut m=lir.clone();m.functions.iter_mut().flat_map(|f| &mut f.blocks).find(|b| !b.instructions.is_empty()).unwrap().instructions[0].span.end+=1;mutants.push(("cfg_with_copied_fingerprint",m));
 for (name,m) in mutants{assert_eq!(m.verified_fingerprint,lir.verified_fingerprint);let e=zil_backend_llvm::emit(&verified,&m).unwrap_err();
  fs::write(base().join(format!("raw-{name}.txt")),format!("{e:#?}")).unwrap();assert!(e.iter().any(|d|d.code=="E-CODEGEN-UNBOUND-LIR"),"{name}: {e:#?}");}
}
fn save_sources(out:&Path,p:&ProjectSources){for u in &p.modules{fs::write(out.join(format!("module{}.zil",u.module_id.0)),&u.text).unwrap();}}
#[test]
fn frozen_layout_cases_and_raw_binding(){
 fs::create_dir_all(base()).unwrap();
 assert_eq!(zil_base::Fingerprint::of(fs::read("/tmp/review243-layout/ORACLE.json").unwrap().as_slice()).to_hex(),"2e7820e025e6d3a1fd32e2ddd340ebb124aa278c4f365c063c70dc2610cb2153");
 context_metadata();raw_unbound();
 let mut session=CompilationSession::with_capacity(128);
 for opt in [Optimization::O0,Optimization::O2]{for c in CASES{
  let p=project(c);let out=base().join(format!("{opt:?}")).join(c.name);fs::create_dir_all(&out).unwrap();save_sources(&out,&p);
  let completed=!c.name.starts_with("context_") && out.join("report.json").exists()
    && c.rows.iter().enumerate().all(|(i,(_,expected,exit))|fs::read(out.join(format!("row{i}.stdout"))).is_ok_and(|v|v==expected.as_bytes()) && fs::read_to_string(out.join(format!("row{i}.exit"))).is_ok_and(|v|v==format!("Some({exit})\n")));
  if completed { println!("reused exact prior successful observation {} {opt:?}",c.name);continue; }
  let r=session.compile_project(&p,&CompileOptions{stop_after:Stage::Build,backend:NativeBackend::LlvmApi,optimization:opt,output:Some(out.join("program")),emit_dir:Some(out.join("phases")),..CompileOptions::default()});
  fs::write(out.join("report.json"),r.to_json()).unwrap();assert_eq!(r.status,ResultCategory::Success,"{}: {}",c.name,r.to_json());assert_eq!(r.verification_runs,1);
  assert_eq!(r.native_codegen.as_ref().unwrap().backend,NativeBackend::LlvmApi);assert!(r.native_codegen.as_ref().unwrap().fallback.is_none());assert_eq!(session.active_query_count(),0);
  for (index,(input,expected,exit)) in c.rows.iter().enumerate(){let mut child=Command::new(&r.artifacts["executable"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();let got=child.wait_with_output().unwrap();
   fs::write(out.join(format!("row{index}.stdout")),&got.stdout).unwrap();fs::write(out.join(format!("row{index}.stderr")),&got.stderr).unwrap();fs::write(out.join(format!("row{index}.exit")),format!("{:?}\n",got.status.code())).unwrap();
   assert_eq!(got.status.code(),Some(*exit),"{} {opt:?} row{index}: {got:?}",c.name);assert_eq!(got.stdout,expected.as_bytes(),"{} {opt:?} row{index}",c.name);assert!(got.stderr.is_empty());
  }
 }}
 let c=CASES.iter().find(|c|c.name=="opaque_app").unwrap();let mut p=project(c);p.modules[0].text=fs::read_to_string("/tmp/review243-layout/negative/opaque_app_private_read.zil").unwrap();
 let out=base().join("private-negative");fs::create_dir_all(&out).unwrap();save_sources(&out,&p);
 let r=CompilationSession::default().compile_project(&p,&CompileOptions{stop_after:Stage::Verify,emit_dir:Some(out.join("phases")),..CompileOptions::default()});fs::write(out.join("report.json"),r.to_json()).unwrap();
 assert_eq!(r.status,ResultCategory::SafeReject);assert!(r.diagnostics.iter().any(|d|d.code=="E-FIELD-PRIVATE"));assert_eq!(r.verification_runs,0);assert!(!r.artifacts.contains_key("executable"));
}
}
