use zil_base::{FileId,ModuleId,SourceUnit,Limits,TargetProfile,Phase,ResultCategory};
use zil_ir::InstructionKind;
fn main(){
 let source=SourceUnit::new(FileId(0),ModuleId(0),"raw-review161.zil","resource R{n:i32}fn main()->i32{let a=R{n:7};let f=()=>{let g=move()=>a.n;return 0};return f()}");
 let syntax=zil_syntax::parse(&source).into_result().unwrap();
 let core=zil_expand::expand(&source,&syntax).unwrap();
 let target=TargetProfile::default();let limits=Limits::default();
 let module=zil_types::check(&core,&target,&limits).unwrap();
 let plan=zil_analyze::analyze(&module,&limits).unwrap();
 zil_verify::verify_module(module.clone(),plan.clone(),&target,&limits).unwrap();
 let outer=module.closures[0].function;
 for name in ["omit-unused-inner-cleanup","copy-owned-capture","describe-as-owned-capture"]{
  let mut candidate=module.clone();let mut proof=plan.clone();
  let function=&mut candidate.functions[outer.0 as usize];let mut altered=0;
  for block in &mut function.blocks{
   if name=="omit-unused-inner-cleanup"{block.instructions.retain(|instruction|{if matches!(instruction.kind,InstructionKind::DropClosure{..}){altered+=1;false}else{true}});}
   else{for instruction in &mut block.instructions{if let InstructionKind::TakeCapture{index}=instruction.kind{instruction.kind=if name=="copy-owned-capture"{InstructionKind::ReadCapture{index}}else{InstructionKind::DescribeCapture{index}};altered+=1;}}}
  }
  assert!(altered>0);
  proof.module_fingerprint=candidate.fingerprint();
  let wire=zil_ir::wire::encode_module_candidate(&candidate,&proof,&limits).unwrap();
  for (route,result) in [("raw",zil_verify::verify_module(candidate,proof,&target,&limits)),("wire",zil_verify::verify_module_bytes(&wire,&target,&limits))]{
   let errors=result.unwrap_err();assert!(errors.iter().any(|d|d.phase==Phase::Verify&&d.category==ResultCategory::SafeReject),"{name}/{route}: {errors:?}");
   println!("{name}/{route}: {errors:?}");
  }
 }
}
