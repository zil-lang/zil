use zil_base::*;
use zil_ir::*;
const SOURCE:&str=include_str!("../../SAMPLE.zil");
fn source()->SourceUnit{SourceUnit::new(FileId(0),ModuleId(0),"copy-generic-enum.zil",SOURCE)}
fn run(n:usize,nodes:bool,fresh:bool)->Result<CoreModule,Vec<Diagnostic>>{
 let u=source();let ast=zil_syntax::parse(&u).into_result().unwrap();
 let limits=if nodes{Limits{max_nodes:n,..Limits::default()}}else{Limits{max_analysis_steps:n,..Limits::default()}};
 let _=fresh;
 zil_expand::expand_with_map_limits(&u,&ast,&limits).map(|e|e.core)
}
fn main(){
 let out=std::path::PathBuf::from(std::env::args().nth(1).unwrap());std::fs::create_dir_all(&out).unwrap();
 let u=source();let ast=zil_syntax::parse(&u).into_result().unwrap();
 let expected=zil_expand::expand_with_map_limits(&u,&ast,&Limits::default()).unwrap().core;
 std::fs::write(out.join("core.txt"),format!("{expected:#?}")).unwrap();
 for nodes in [false,true]{
  let mut boundaries=Vec::new();
  for cycle in 0..3{
   let(mut lo,mut hi)=(0,if nodes{Limits::default().max_nodes}else{Limits::default().max_analysis_steps});
   while lo+1<hi{let n=lo+(hi-lo)/2;match run(n,nodes,cycle==2){Ok(core)=>{assert_eq!(core,expected);hi=n},Err(e)=>{assert!(e.iter().all(|e|e.category==ResultCategory::ResourceLimit),"{e:?}");lo=n}}}
   assert!(run(hi-1,nodes,true).unwrap_err().iter().all(|e|e.category==ResultCategory::ResourceLimit));
   let at=run(hi,nodes,true).unwrap();assert_eq!(at,expected);
   // Reuse the same parsed input in a subsequent public call as the warm control.
   let limits=if nodes{Limits{max_nodes:hi,..Limits::default()}}else{Limits{max_analysis_steps:hi,..Limits::default()}};
   assert_eq!(zil_expand::expand_with_map_limits(&u,&ast,&limits).unwrap().core,expected);
   boundaries.push(hi);
  }
  assert!(boundaries.windows(2).all(|w|w[0]==w[1]));
  println!("{} N={} cycles={boundaries:?}; N-1 ResourceLimit, warm/fresh Core identical",if nodes{"nodes"}else{"work"},boundaries[0]);
 }
 let text="resource R{n:i32}package P{manages r:R}enum E<T>{V(T)}fn main(){let e=E.V(P{r:R{n:1}})}";
 let u=SourceUnit::new(FileId(0),ModuleId(0),"owner-control.zil",text);
 let ast=zil_syntax::parse(&u).into_result().unwrap();
 let result=zil_expand::expand(&u,&ast);
 std::fs::write(out.join("owner-control.txt"),format!("{result:#?}")).unwrap();
 println!("owner Expand accepted={}",result.is_ok());
}
