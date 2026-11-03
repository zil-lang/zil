extern crate zil_base;
extern crate zil_syntax;
extern crate zil_expand;
extern crate zil_types;
extern crate zil_analyze;
extern crate zil_verify;
fn main() {
 let text=std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
 let src=zil_base::SourceUnit::new(zil_base::FileId(0),zil_base::ModuleId(0),"review.zil",text);
 let ast=zil_syntax::parse(&src).into_result().unwrap();
 let core=zil_expand::expand(&src,&ast).unwrap();
 let limits=zil_base::Limits::default();let target=zil_base::TargetProfile::default();
 let typed=zil_types::check(&core,&target,&limits).unwrap();
 let plan=zil_analyze::analyze(&typed,&limits).unwrap();
 for phase in 0..3 {
  let run=|n| { let selected=zil_base::Limits{max_analysis_steps:n,..limits.clone()};match phase {
   0=>zil_types::check(&core,&target,&selected).map(|_|()),
   1=>zil_analyze::analyze(&typed,&selected).map(|_|()),
   _=>zil_verify::verify_module(typed.clone(),plan.clone(),&target,&selected).map(|_|())
  }};
  let(mut low,mut high)=(0,limits.max_analysis_steps);
  while low+1<high {let n=low+(high-low)/2;match run(n){Ok(())=>high=n,Err(e)=>{assert_eq!(e[0].category,zil_base::ResultCategory::ResourceLimit,"{:?}",e);low=n}}}
  for n in [high,high-1,high] {match run(n){Ok(())=>assert_eq!(n,high),Err(e)=>{assert_eq!(n,high-1);assert_eq!(e[0].category,zil_base::ResultCategory::ResourceLimit);}}}
  println!("{} N={} N-1 ResourceLimit fresh N Pass",["Types","Analyze","Verify"][phase],high);
 }
}
