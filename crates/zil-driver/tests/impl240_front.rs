//! Author public source-to-Types checks. Independent kernels are tested separately.
use zil_base::{FileId,Limits,ModuleId,SourceUnit,TargetProfile};
use zil_ir::*;
fn core(text:&str)->CoreModule{
 let source=SourceUnit::new(FileId(0),ModuleId(0),"front240.zil",text);
 let ast=zil_syntax::parse(&source).into_result().unwrap();
 zil_expand::expand(&source,&ast).unwrap()
}
fn checked(text:&str)->TypedModule{
 zil_types::check(&core(text),&TargetProfile::host_llvm(),&Limits::default()).unwrap()
}
fn rejected(text:&str){
 let source=SourceUnit::new(FileId(0),ModuleId(0),"reject240.zil",text);
 let ast=zil_syntax::parse(&source).into_result().unwrap();
 if let Ok(c)=zil_expand::expand(&source,&ast){assert!(zil_types::check(&c,&TargetProfile::host_llvm(),&Limits::default()).is_err(),"accepted: {text}");}
}
#[test]
fn original_q_sources_reach_types_without_source_changes(){
 for text in [include_str!("impl240_fixtures/original03.zil"),include_str!("impl240_fixtures/original05.zil")]{checked(text);}
}
#[test]
fn right_nested_pattern_uses_local_observation_and_real_reverse_dfs_publication(){
 let t=checked(include_str!("impl240_fixtures/nested.zil"));
 let f=t.functions.iter().find(|f|f.name=="main").unwrap();
 let names=f.locals.iter().map(|l|l.name.as_str()).filter(|n|["a","c","d"].contains(n)).collect::<Vec<_>>();
 assert_eq!(names,vec!["d","c","a"]);
 assert!(f.blocks.iter().flat_map(|b|&b.instructions).any(|i|matches!(&i.kind,InstructionKind::VariantTagLocal{path,..} if !path.is_empty())));
 assert!(f.blocks.iter().flat_map(|b|&b.instructions).filter(|i|matches!(i.kind,InstructionKind::UnpackVariant{..})).count()>=2);
 assert!(f.locals.iter().any(|l|matches!(&l.ty,Type::Package{fields,..} if fields.iter().any(|(_,t)|matches!(t,Type::Enum{..})))));
}
#[test]
fn product_sum_fields_and_error_payloads_are_complete_values(){checked(include_str!("impl240_fixtures/product.zil"));}
#[test]
fn resource_field_requires_explicit_ownership_and_cannot_upgrade_ref(){
 for field in ["R","Ref<R>","manages Ref<R>","manages &R"]{
  rejected(&format!("resource R{{n:i32}}enum E{{V({field})}}fn main(){{}}"));
 }
 checked("resource R{n:i32}enum E{V(manages R,manages ByteBuffer)}fn main(){let r=R{n:1};let b=bytes_new();let e=E.V(give r,give b)}");
}
#[test]
fn copy_generic_enum_is_preserved_but_new_owning_generic_enum_is_closed(){
 checked("enum E<T>{V(T),Nil}fn main()->i32{let e=E.V(3);match e{E.V(n)=>{return n}\nE.Nil()=>{return 0}}}");
 rejected("resource R{n:i32}package P{manages r:R}enum E<T>{V(T)}fn main(){let e=E.V(P{r:R{n:1}})}");
 rejected("resource R{n:i32}enum E<T>{V(manages R,T)}fn main(){}");
}
#[test]
fn ordinary_sum_does_not_acquire_store_element_or_resource_header_permission(){
 rejected("resource R{n:i32}enum E{V(manages R)}package P{state:E}fn main(){let store:Store<P>=store_new()}");
 rejected("resource R{n:i32}enum E{V(manages R)}resource Bad{state:E}fn main(){}");
}
