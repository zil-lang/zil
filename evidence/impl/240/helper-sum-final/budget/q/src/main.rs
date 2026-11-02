use zil_base::*;
fn main(){
 let root=std::path::PathBuf::from(std::env::args().nth(1).unwrap());
 let out=std::path::PathBuf::from(std::env::args().nth(2).unwrap());std::fs::create_dir_all(&out).unwrap();
 for arg in std::env::args().skip(3){
  let text=std::fs::read_to_string(root.join(format!("{arg}.zil"))).unwrap();
  let source=SourceUnit::new(FileId(0),ModuleId(0),"helper-budget.zil",text);
  let ast=zil_syntax::parse(&source).into_result().unwrap();
  let core=zil_expand::expand(&source,&ast).unwrap();
  let target=TargetProfile::host_llvm();
  let typed=zil_types::check(&core,&target,&Limits::default()).unwrap();
  std::fs::write(out.join(format!("{arg}.core")),format!("{core:#?}")).unwrap();
  std::fs::write(out.join(format!("{arg}.typed")),format!("{typed:#?}")).unwrap();
  for types in [false,true]{
   let mut ns=Vec::new();
   for _ in 0..2{
    let(mut lo,mut hi)=(0,Limits::default().max_analysis_steps);
    while lo+1<hi{
     let n=lo+(hi-lo)/2;let limits=Limits{max_analysis_steps:n,..Limits::default()};
     let result=if types{zil_types::check(&core,&target,&limits).map(|v|{assert_eq!(v,typed);})}else{zil_expand::expand_with_map_limits(&source,&ast,&limits).map(|v|{assert_eq!(v.core,core);})};
     match result{Ok(())=>hi=n,Err(e)=>{assert!(e.iter().all(|d|d.category==ResultCategory::ResourceLimit),"{arg}/{types}/{n}: {e:?}");lo=n}}
    }
    let limits=Limits{max_analysis_steps:hi-1,..Limits::default()};
    let error=if types{zil_types::check(&core,&target,&limits).unwrap_err()}else{zil_expand::expand_with_map_limits(&source,&ast,&limits).unwrap_err()};
    assert!(error.iter().all(|d|d.category==ResultCategory::ResourceLimit));
    std::fs::write(out.join(format!("{arg}-{}-Nminus1.txt",if types{"types"}else{"expand"})),format!("{error:#?}")).unwrap();
    let limits=Limits{max_analysis_steps:hi,..Limits::default()};
    // A fresh syntax/Core input after the exact N-1 failed request.
    let fresh_ast=zil_syntax::parse(&source).into_result().unwrap();
    let fresh_core=zil_expand::expand(&source,&fresh_ast).unwrap();
    assert_eq!(fresh_core,core);
    if types{assert_eq!(zil_types::check(&fresh_core,&target,&limits).unwrap(),typed)}else{assert_eq!(zil_expand::expand_with_map_limits(&source,&fresh_ast,&limits).unwrap().core,core)};
    ns.push(hi);
   }
   assert_eq!(ns[0],ns[1]);println!("{arg} {} N={} N-1 ResourceLimit, cold/warm/fresh output equal",if types{"Types"}else{"Expand"},ns[0]);
  }
 }
}
