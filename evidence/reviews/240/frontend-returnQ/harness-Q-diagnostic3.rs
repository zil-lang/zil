use std::{fs,path::Path};
use zil_base::{FileId,ModuleId,NodeId,SourceUnit,Limits,TargetProfile};
use zil_ir::*;
fn base()->std::path::PathBuf{std::env::var_os("REVIEW240_OUTPUT").map(std::path::PathBuf::from).unwrap_or_else(||Path::new("/tmp/review240-boundaries/results").to_path_buf())}

fn outcome(phase:&str,accepted:bool,diagnostics:&str)->String{format!("{{\"phase\":{phase:?},\"accepted\":{accepted},\"diagnostics\":{diagnostics:?}}}")}
fn parse_expand(text:&str)->Result<CoreModule,String>{
    let source=SourceUnit::new(FileId(0),ModuleId(0),"review240.zil",text);
    let ast=zil_syntax::parse(&source).into_result().map_err(|e|outcome("Parse",false,&format!("{e:?}")))?;
    zil_expand::expand(&source,&ast).map_err(|e|outcome("Expand",false,&format!("{e:?}")))
}
fn check(name:&str,text:&str)->String{
    let dir=base().join(name);fs::create_dir_all(&dir).unwrap();fs::write(dir.join("source.zil"),text).unwrap();
    let result=match parse_expand(text){Err(e)=>e,Ok(core)=>{fs::write(dir.join("core.txt"),format!("{core:#?}")).unwrap();typed(&dir,&core)}};
    fs::write(dir.join("result.json"),&result).unwrap();format!("{{\"name\":{name:?},\"result\":{result}}}")
}
fn typed(dir:&Path,core:&CoreModule)->String{
    match zil_types::check(core,&TargetProfile::host_llvm(),&Limits::default()){
        Err(e)=>outcome("Types",false,&format!("{e:?}")),
        Ok(module)=>{fs::write(dir.join("typed.txt"),format!("{module:#?}")).unwrap();if std::env::var_os("REVIEW240_ANALYZE_CONTROL").is_some(){let analysis=zil_analyze::analyze(&module,&Limits::default());fs::write(dir.join("analysis-control.txt"),format!("{analysis:#?}")).unwrap();}outcome("Types",true,"")}
    }
}
fn project(name:&str,main:&str,api:&str)->String{
    let dir=base().join(name);fs::create_dir_all(&dir).unwrap();fs::write(dir.join("main.zil"),main).unwrap();fs::write(dir.join("api.zil"),api).unwrap();
    let sources=[SourceUnit::new(FileId(0),ModuleId(0),"main.zil",main),SourceUnit::new(FileId(1),ModuleId(1),"api.zil",api)];let limits=Limits::default();
    let result=(||->Result<String,String>{let syntax=sources.iter().map(|s|zil_syntax::parse(s).into_result().map_err(|e|outcome("Parse",false,&format!("{e:?}")))).collect::<Result<Vec<_>,_>>()?;
        let imports=vec![vec![CoreImport{alias:"api".into(),module:ModuleId(1),span:syntax[0].items[0].span}],vec![]];
        let expanded=zil_expand::expand_program_with_limits(&sources,&syntax.iter().collect::<Vec<_>>(),&imports,&limits).map_err(|e|outcome("Expand",false,&format!("{e:?}")))?;
        let core=CoreProgram{root:ModuleId(0),source_fingerprint:zil_base::Fingerprint::ZERO,modules:expanded.into_iter().map(|(m,_)|m).collect()};fs::write(dir.join("core-program.txt"),format!("{core:#?}")).unwrap();
        match zil_types::check_program(&core,&TargetProfile::host_llvm(),&limits){Err(e)=>Ok(outcome("Types",false,&format!("{e:?}"))),Ok(typed)=>{fs::write(dir.join("typed-program.txt"),format!("{typed:#?}")).unwrap();Ok(outcome("Types",true,""))}}
    })().unwrap_or_else(|e|e);fs::write(dir.join("result.json"),&result).unwrap();format!("{{\"name\":{name:?},\"project\":true,\"result\":{result}}}")
}
fn main(){
    let r="resource R{n:i32}";
    let head="resource R{n:i32}enum E{V(manages R),Nil}package P{state:E}";
    let mut cases=vec![
        ("explicit_unused",format!("{r}enum E{{V(manages R)}}fn main(){{}}")),
        ("bare_unused",format!("{r}enum E{{V(R)}}fn main(){{}}")),
        ("ref_unused",format!("{r}enum E{{V(Ref<R>)}}fn main(){{}}")),
        ("manages_ref_unused",format!("{r}enum E{{V(manages Ref<R>)}}fn main(){{}}")),
        ("manages_ref_alias",format!("{r}type B=Ref<R>;enum E{{V(manages B)}}fn main(){{}}")),
        ("manages_resource_alias",format!("{r}type B=R;enum E{{V(manages B)}}fn main(){{}}")),
        ("manages_copy_alias",format!("type B=i32;enum E{{V(manages B)}}fn main(){{}}")),
        ("unused_recursive_enum",format!("{r}enum E{{V(E,manages R)}}fn main(){{}}")),
        ("unused_recursive_product",format!("{r}enum E{{V(P)}}package P{{state:E;manages r:R}}fn main(){{}}")),
        ("copy_generic_actual",format!("{r}enum Owned{{V(manages R)}}enum E<T>{{V(T),Nil}}fn main(){{let e=E.V(3)}}")),
        ("generic_concrete_owner_unused",format!("{r}package P{{manages r:R}}enum E<T>{{V(P)}}fn main(){{}}")),
        ("generic_actual_owner",format!("{r}package P{{manages r:R}}enum E<T>{{V(T)}}fn main(){{let e=E.V(P{{r:R{{n:1}}}})}}")),
        ("generic_phantom_owner",format!("{r}package P{{manages r:R}}enum E<T>{{Nil}}fn use_it(e:E<P>){{}}fn main(){{}}")),
        ("generic_decl_manages_unused",format!("{r}enum E<T>{{V(manages R,T)}}fn main(){{}}")),
        ("sum_capture",format!("{head}fn main(){{let e=E.V(R{{n:1}});let f=move()=>{{let value=e;}};f()}}")),
        ("product_sum_capture",format!("{head}fn main(){{let p=P{{state:E.V(R{{n:1}})}};let f=move()=>{{let value=p;}};f()}}")),
        ("sum_array",format!("{head}fn main(){{let a=[E.V(R{{n:1}})]}}")),
        ("product_sum_array",format!("{head}fn main(){{let a=[P{{state:E.V(R{{n:1}})}}]}}")),
        ("sum_store_element_unused",format!("{head}fn unused(give s:Store<P>){{}}fn main(){{}}")),
        ("sum_shared_unused",format!("{head}fn unused(e:&E){{}}fn main(){{}}")),
        ("product_sum_shared_unused",format!("{head}fn unused(e:&P){{}}fn main(){{}}")),
        ("sum_ffi_unused",format!("{head}extern \"C\" fn send(e:E);fn main(){{}}")),
        ("resource_sum_header_unused",format!("{head}resource Bad{{state:E}}fn main(){{}}")),
        ("sum_replacement_valid",format!("{head}fn relay(e:E)->E{{return e}}fn main(){{var x=E.V(R{{n:1}});x=relay(x);}}")),
        ("sum_moved_then_replace",format!("{head}fn relay(e:E)->E{{return e}}fn main(){{var x=E.V(R{{n:1}});let moved=x;x=E.V(R{{n:2}});}}")),
        ("sum_shadow_binding",format!("{head}fn relay(e:E)->E{{return e}}fn main(){{var x=E.V(R{{n:1}});if true{{let x=3;let y=x;}}x=relay(x);}}")),
        ("sum_rhs_shadow",format!("{head}fn relay(e:E)->E{{return e}}fn main(){{var x=E.V(R{{n:1}});x=relay(if true{{let x=E.Nil;x}}else{{E.Nil}});}}")),
        ("borrowed_param_into_manages",format!("{r}enum E{{V(manages R)}}fn bad(r:R)->E{{return E.V(give r)}}fn main(){{}}")),
        ("ref_param_into_manages",format!("{r}enum E{{V(manages R)}}fn bad(r:Ref<R>)->E{{return E.V(give r)}}fn main(){{}}")),
        ("owned_param_into_manages",format!("{r}enum E{{V(manages R)}}fn good(give r:R)->E{{return E.V(give r)}}fn main(){{}}")),
        ("callback_owned_value_used",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};return cb()+p.r.n}}}}}}fn main()->i32{{return consume(E.V(R{{n:2}}),()=>3)-5}}")),
        ("callback_owned_value_unused",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};return cb()+p.r.n}}}}}}fn main(){{}}")),
        ("callback_owned_forward",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let saved=give r;let p=P{{r:give saved}};return cb()+p.r.n}}}}}}fn main()->i32{{return consume(E.V(R{{n:2}}),()=>3)-5}}")),
        ("callback_forward_unused",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let saved=give r;let p=P{{r:give saved}};return cb()+p.r.n}}}}}}fn main(){{}}")),
        ("callback_bare_alias",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let d=r;let p=P{{r:give d}};return cb()+p.r.n}}}}}}fn main(){{}}")),
        ("callback_read_after_give",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};return cb()+r.n}}}}}}fn main(){{}}")),
        ("callback_repeat_give",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};let q=P{{r:give r}};return cb()}}}}}}fn main(){{}}")),
        ("callback_bare_resource",format!("{r}package P{{manages r:R}}enum E{{V(R)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};return cb()}}}}}}fn main(){{}}")),
        ("callback_ref_resource",format!("{r}package P{{manages r:R}}enum E{{V(Ref<R>)}}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};return cb()}}}}}}fn main(){{}}")),
        ("callback_duplicate_enum_unused",format!("{r}enum E{{V(manages R)}}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{{let first=e;let second=e;return cb()}}fn main(){{}}")),
        ("callback_duplicate_enum_used",format!("{r}enum E{{V(manages R)}}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{{let first=e;let second=e;return cb()}}fn main()->i32{{return duplicate(()=>0,E.V(R{{n:1}}))}}")),
        ("callback_duplicate_nested_enum_unused",format!("{r}package P{{manages r:R}}enum E{{V(P)}}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{{let first=e;let second=e;return cb()}}fn main(){{}}")),
        ("callback_duplicate_union_unused",format!("{r}enum E{{V(manages R)}}fn duplicate<F:Fn()->i32>(cb:F,e:void!E)->i32{{let first=e;let second=e;return cb()}}fn main(){{}}")),
        ("callback_duplicate_copy_enum_control",format!("enum E{{V(i32)}}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{{let first=e;let second=e;return cb()}}fn main(){{}}")),
        ("callback_duplicate_package_control",format!("{r}package P{{manages r:R}}fn duplicate<F:Fn()->i32>(cb:F,e:P)->i32{{let first=e;let second=e;return cb()}}fn main(){{}}")),
        ("callback_rematch_enum_unused",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{{match e{{E.V(r)=>{{let first=P{{r:give r}};}}}}match e{{E.V(r)=>{{let second=P{{r:give r}};}}}}return cb()}}fn main(){{}}")),
        ("callback_desc_before_give_control",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn use_value<F:Fn()->i32>(cb:F,e:E)->i32{{match e{{E.V(r)=>{{let description=r;let n=description.n;let p=P{{r:give r}};return cb()+n}}}}}}fn main(){{}}")),
        ("callback_desc_after_give_unused",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn use_value<F:Fn()->i32>(cb:F,e:E)->i32{{match e{{E.V(r)=>{{let description=r;let p=P{{r:give r}};return cb()+description.n}}}}}}fn main(){{}}")),
        ("callback_desc_chain_after_give_unused",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn use_value<F:Fn()->i32>(cb:F,e:E)->i32{{match e{{E.V(r)=>{{let description=r;let alias=description;let p=P{{r:give r}};return cb()+alias.n}}}}}}fn main(){{}}")),
        ("callback_desc_relay_after_give_unused",format!("{r}package P{{manages r:R}}enum E{{V(manages R)}}fn use_value<F:Fn()->i32>(cb:F,e:E)->i32{{match e{{E.V(r)=>{{let description=r;let saved=give r;let p=P{{r:give saved}};return cb()+description.n}}}}}}fn main(){{}}")),
        ("callback_owned_signature_closed",format!("{r}enum E{{V(manages R)}}fn consume<F:Fn(E)->i32>(value:E,cb:F)->i32{{return cb(value)}}fn main(){{}}")),
    ];
    cases.push(("shadowed_owner_scope_exit_helper","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{match e{E.V(a,b)=>{var d=a;if flag{let saved=give b;d=saved;let saved=0;}return cb()+d.n}}}fn main(){}".to_owned()));
    cases.push(("shadowed_owner_scope_exit_concrete","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{var d=a;if flag{let saved=give b;d=saved;let saved=0;}return cb()+d.n}}}fn main(){}".to_owned()));
    cases.push(("owner_branch_replacement_current_helper","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{match e{E.V(a,b)=>{var saved=give a;if flag{saved=give b}else{}return cb()+saved.n}}}fn main(){}".to_owned()));
    cases.push(("owner_branch_replacement_current_concrete","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{var saved=give a;if flag{saved=give b}else{}return cb()+saved.n}}}fn main(){}".to_owned()));
    cases.push(("tuple_descriptions_closed_helper","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{match e{E.V(a,b)=>{let tuple=(a,b);return cb()+tuple.0.n+tuple.1.n}}}fn main(){}".to_owned()));
    cases.push(("tuple_descriptions_closed_concrete","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{let tuple=(a,b);return cb()+tuple.0.n+tuple.1.n}}}fn main(){}".to_owned()));
    cases.push(("array_descriptions_closed_helper","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{match e{E.V(a,b)=>{let array=[a,b];return cb()+array[0].n}}}fn main(){}".to_owned()));
    cases.push(("array_descriptions_closed_concrete","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{let array=[a,b];return cb()+array[0].n}}}fn main(){}".to_owned()));
    cases.push(("address_description_before_give_helper","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{match e{E.V(a,b)=>{let d=&a;let n=(*d).n;let p=P{r:give a};return cb()+n}}}fn main(){}".to_owned()));
    cases.push(("address_description_before_give_concrete","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{let d=&a;let n=(*d).n;let p=P{r:give a};return cb()+n}}}fn main(){}".to_owned()));
    cases.push(("diagnostic_owner_branch_main","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn main()->i32{let e=E.V(R{n:1},R{n:2});let cb=()=>0;let flag=true;match e{E.V(a,b)=>{var saved=give a;if flag{saved=give b}else{}return cb()+saved.n}}}".to_owned()));
    cases.push(("diagnostic_owner_branch_move_parameter","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(move e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{var saved=give a;if flag{saved=give b}else{}return cb()+saved.n}}}fn main(){}".to_owned()));
    cases.push(("diagnostic_address_description_main","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn main()->i32{let e=E.V(R{n:1},R{n:2});let cb=()=>0;let flag=true;match e{E.V(a,b)=>{let d=&a;let n=(*d).n;let p=P{r:give a};return cb()+n}}}".to_owned()));
    cases.push(("diagnostic_address_description_move_parameter","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(move e:E,flag:bool)->i32{let cb=()=>0;match e{E.V(a,b)=>{let d=&a;let n=(*d).n;let p=P{r:give a};return cb()+n}}}fn main(){}".to_owned()));
    cases.push(("diagnostic_plain_parameter_give","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E)->i32{match e{E.V(a,b)=>{let p=P{r:give a};return b.n}}}fn main(){}".to_owned()));
    cases.push(("diagnostic_corrected_owner_branch","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{match e{E.V(a,b)=>{var saved=give a;if flag{saved=give b}else{}return saved.n}}}fn main(){}".to_owned()));
    cases.push(("diagnostic_corrected_address_description","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value(e:E,flag:bool)->i32{match e{E.V(a,b)=>{let d=&a;let n=(*d).n;let p=P{r:give a};return n}}}fn main(){}".to_owned()));
    cases.push(("diagnostic_final_helper_no_call","resource R{n:i32}package P{manages r:R}enum E{V(manages R,manages R)}fn use_value<F:Fn()->i32>(cb:F,e:E,flag:bool)->i32{match e{E.V(a,b)=>{var saved=give a;if flag{saved=give b}else{}return saved.n}}}fn main(){}".to_owned()));
    let mut rows=Vec::new();for(name,text)in cases{if std::env::var_os("REVIEW240_DIAGNOSTIC").is_some()&&!name.starts_with("diagnostic_corrected_")&&!name.starts_with("diagnostic_final_"){continue;}if matches!(name,"sum_moved_then_replace"|"sum_rhs_shadow"){continue;}let result=check(name,&text);println!("{result}");rows.push(result);}
    if std::env::var_os("REVIEW240_DIAGNOSTIC").is_some(){fs::write(base().join("RESULTS.json"),format!("[\n{}\n]",rows.join(",\n"))).unwrap();return;}
    for (name,field,twice,used) in [("project_callback_owned_used","manages R",false,true),("project_callback_owned_unused","manages R",false,false),("project_callback_bare","R",false,true),("project_callback_ref","Ref<R>",false,true),("project_callback_repeat","manages R",true,true)]{
        let repeat=if twice{"let q=P{r:give r};"}else{""};let api=format!("pub resource R{{pub n:i32}}pub package P{{manages r:R}}pub enum E{{V({field})}}pub fn consume<F:Fn()->i32>(value:E,cb:F)->i32{{match value{{E.V(r)=>{{let p=P{{r:give r}};{repeat}return cb()+p.r.n}}}}}}");
        let main=if used{"import api\nfn main()->i32{return api.consume(api.E.V(api.R{n:2}),()=>3)-5}"}else{"import api\nfn main(){ }"};let row=project(name,main,&api);println!("{row}");rows.push(row);
    }
    let api="pub resource R{pub n:i32}pub package P{tag:i32;manages r:R}pub enum E{V(P)}pub fn make()->E{return E.V(P{tag:7,r:R{n:1}})}pub fn relay(e:E)->E{return e}";
    for (name,main)in[("project_opaque_replacement","import api\nfn main(){var e=api.make();e=api.relay(e);}"),("project_opaque_private_read","import api\nfn main()->i32{var e=api.make();e=api.relay(e);match e{api.E.V(p)=>{return p.tag}}}")]{let row=project(name,main,api);println!("{row}");rows.push(row);}
    let valid=format!("{r}enum E{{V(manages R)}}fn main(){{}}");
    for (name,ty) in [("raw_managed_desc",Type::Managed(Box::new(Type::Desc(Box::new(Type::Named("R".into())))))),("raw_managed_managed",Type::Managed(Box::new(Type::Managed(Box::new(Type::Named("R".into())))))),("raw_managed_address",Type::Managed(Box::new(Type::Address{mutable:false,inner:Box::new(Type::Named("R".into()))}))) ]{
        if matches!(name,"raw_managed_desc"|"raw_managed_managed"){continue;}
        let mut core=parse_expand(&valid).unwrap();core.enums[0].variants[0].1[0]=ty;let dir=base().join(name);fs::create_dir_all(&dir).unwrap();fs::write(dir.join("raw-core.txt"),format!("{core:#?}")).unwrap();let result=typed(&dir,&core);let row=format!("{{\"name\":{name:?},\"raw_core\":true,\"result\":{result}}}");println!("{row}");rows.push(row);
    }
    let source=format!("{head}fn main(){{var x=E.V(R{{n:1}});let leaked=x;x=E.Nil;}}");let mut core=parse_expand(&source).unwrap();
    let function=core.functions.iter_mut().find(|f|f.name=="main").unwrap();
    let statement=function.body.statements.iter_mut().find(|s|matches!(&s.kind,CoreStatementKind::Binding{name,..} if name=="leaked")).unwrap();
    if let CoreStatementKind::Binding{initializer:Some(value),..}=&mut statement.kind{let mut object=value.clone();object.id=NodeId(999999);value.kind=CoreExprKind::Member{object:Box::new(object),member:"$value240".into()};}
    let dir=base().join("raw_hidden_field");fs::create_dir_all(&dir).unwrap();fs::write(dir.join("raw-core.txt"),format!("{core:#?}")).unwrap();let result=typed(&dir,&core);let row=format!("{{\"name\":\"raw_hidden_field\",\"raw_core\":true,\"result\":{result}}}");println!("{row}");rows.push(row);
    fs::write(base().join("RESULTS.json"),format!("[\n{}\n]\n",rows.join(",\n"))).unwrap();
}
