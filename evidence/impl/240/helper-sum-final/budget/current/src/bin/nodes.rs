use zil_base::*;
fn main(){
 let text=std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
 let source=SourceUnit::new(FileId(0),ModuleId(0),"helper-budget.zil",text);
 let ast=zil_syntax::parse(&source).into_result().unwrap();let expected=zil_expand::expand(&source,&ast).unwrap();
 for cycle in 0..2{let(mut lo,mut hi)=(0,Limits::default().max_nodes);
 while lo+1<hi{let n=lo+(hi-lo)/2;let l=Limits{max_nodes:n,..Limits::default()};match zil_expand::expand_with_map_limits(&source,&ast,&l){Ok(v)=>{assert_eq!(v.core,expected);hi=n},Err(e)=>{assert!(e.iter().all(|d|d.category==ResultCategory::ResourceLimit));lo=n}}}
 let l=Limits{max_nodes:hi-1,..Limits::default()};assert!(zil_expand::expand_with_map_limits(&source,&ast,&l).unwrap_err().iter().all(|d|d.category==ResultCategory::ResourceLimit));
 let fresh=zil_syntax::parse(&source).into_result().unwrap();let l=Limits{max_nodes:hi,..Limits::default()};assert_eq!(zil_expand::expand_with_map_limits(&source,&fresh,&l).unwrap().core,expected);
 println!("cycle{cycle} Expand nodes N={hi}; N-1 ResourceLimit, fresh equal");}
}