#[cfg(test)]
mod tests {
use zil_base::{SourceUnit,FileId,ModuleId,TargetProfile,Limits,Span};
use zil_ir::*;
use std::{fs,path::Path};
fn probe(sum:bool)->(TypedModule,CandidatePlan){
 let u=SourceUnit::new(FileId(0),ModuleId(0),"unused-array242.zil","fn main()->i32{return 0}");
 let ast=zil_syntax::parse(&u).into_result().unwrap();let c=zil_expand::expand(&u,&ast).unwrap();let limits=Limits::default();
 let mut m=zil_types::check(&c,&TargetProfile::host_llvm(),&limits).unwrap();let mut p=zil_analyze::analyze(&m,&limits).unwrap();
 zil_verify::verify_module(m.clone(),p.clone(),&m.target,&limits).unwrap();
 m.resources.push(ResourceDefinition{name:"@builtin:ByteBuffer".into(),payload:Type::ByteBuffer,teardown:None});
 let leaf=Type::Managed(Box::new(Type::ByteBuffer));
 let field=if sum {Type::Enum{name:"OwnedEnum242".into(),variants:vec![("Empty".into(),vec![]),("Some".into(),vec![leaf])]} }else{leaf};
 let element=Type::Package{name:"ArrayElement242".into(),fields:vec![("value".into(),field)]};
 let f=m.functions.iter_mut().find(|f|f.name=="main").unwrap();let local=LocalId(f.locals.len() as u32);
 f.locals.push(LocalDecl{id:local,name:"unused-array".into(),ty:Type::Array{element:Box::new(element),length:1},mutable:false,span:Span::new(FileId(0),0,1)});
 p.module_fingerprint=m.fingerprint();(m,p)
}
#[test]
fn unused_array_element_package_does_not_inherit_sum_admission(){
 let out=Path::new("/tmp/review242-profile/results");fs::create_dir_all(out).unwrap();
 let limits=Limits::default();let target=TargetProfile::host_llvm();
 for sum in [false,true]{let(m,p)=probe(sum);let bytes=zil_ir::wire::encode_module_candidate(&m,&p,&limits).unwrap();
  fs::write(out.join(format!("sum-{sum}-raw.txt")),format!("{m:#?}\n{p:#?}")).unwrap();fs::write(out.join(format!("sum-{sum}.wire")),&bytes).unwrap();
  for (route,r) in [("raw",zil_verify::verify_module(m.clone(),p.clone(),&target,&limits)),("wire",zil_verify::verify_module_bytes(&bytes,&target,&limits))]{
   let text=if !sum {r.unwrap();"ExpectedAccept: old direct Managed ByteBuffer Package array".to_string()}else{
    let errors=r.expect_err("unused Array<Package<OwnedEnum>> must not inherit ordinary Sum admission");
    assert!(errors.iter().all(|d|d.phase==zil_base::Phase::Verify && d.category==zil_base::ResultCategory::Unsupported && d.code=="E-UNSUPPORTED" && d.message=="non-Copy type requires another checker"),"{errors:#?}");
    format!("{errors:#?}")
   };
   fs::write(out.join(format!("sum-{sum}-{route}.txt")),&text).unwrap();println!("sum={sum} route={route} {text}");
  }
 }
}
}
