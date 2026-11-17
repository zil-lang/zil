//! The original semantic interpreter is retained only after the complete
//! immutable programme proof. Every W application supplies its own full input
//! domain; this cache supplies no runtime source or loan permission.
use super::*;
use crate::semantic::{self,api::*};
use std::cell::RefCell;

pub(crate) struct Session {
    module:usize,
    context:semantic::CheckedContext,
    active:RefCell<semantic::ActiveQueries>,
    worlds:Vec<(SemanticInstanceId,EvalEnv)>,
    results:Vec<(SemanticInstanceId,CheckedType)>,
}
impl Session {
    pub(crate) fn new(module:&TypedModule,context:semantic::CheckedContext,active:semantic::ActiveQueries,worlds:Vec<(SemanticInstanceId,EvalEnv)>,results:Vec<(SemanticInstanceId,CheckedType)>)->Self{
        Self{module:module as *const TypedModule as usize,context,active:RefCell::new(active),worlds,results}
    }
}

/// Bind a source parameter to its original universal capability theorem.
/// World substitution is deliberately not applied to the original subject.
pub(crate) fn closed_task_input(module:&TypedModule,function:&TypedFunction,position:&StoragePosition,b:&mut Budget)->Check<bool>{
    let StorageBase::Parameter(ordinal)=position.base else{return Ok(false);};
    if module.semantic.is_none()||function.parameters.get(ordinal as usize).is_none(){return Ok(false);}
    let Some(session)=b.semantic_checked.clone()else{return Ok(false);};
    if session.module!=module as *const TypedModule as usize{return Err(bad("ClosedEnvironment source session belongs to another module"));}
    let mut worlds=0usize;
    for(id,_)in &session.worlds{
        let instance=session.context.instance(module,*id,b)?;
        if instance.target!=SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id)){continue;}
        if !session.context.closed_input_original(module,instance.definition,ordinal,&position.path,&mut session.active.borrow_mut(),b)?{return Ok(false);}
        worlds+=1;
    }
    Ok(worlds!=0)
}

/// The caller runs this only after the complete resource/Task/phase passes;
/// no individual successful consuming call discharges another world's owner.
pub(crate) fn finish_original_callable_retirements(module:&TypedModule,b:&mut Budget)->Check<()>{
    if module.semantic.is_none(){return Ok(());}
    let session=b.semantic_checked.clone().ok_or_else(||bad("callable retirement has no completed semantic session"))?;
    if session.module!=module as *const TypedModule as usize{return Err(bad("callable retirement session belongs to another actual module"));}
    session.context.finish_original_callable_retirements(module,&session.worlds,b)
}

/// Only the completed exact-module semantic session exposes ordinary member
/// execution. This proves no vtable contract, source envelope or permission.
pub(crate) fn checked_static_specialization(module:&TypedModule,function:&TypedFunction,b:&mut Budget)->Check<bool>{
    if module.semantic.is_none(){return Ok(false);}
    b.spend(1)?;
    let Some(session)=b.semantic_checked.clone()else{return Ok(false);};
    if session.module!=module as *const TypedModule as usize{return Ok(false);}
    session.context.checked_static_specialization(module,function,b)
}

/// A directed root result view is established in every completed original
/// world of this exact entry. It never substitutes an invariant child class.
pub(crate) fn return_borrow_view(module:&TypedModule,function:&TypedFunction,block:BlockId,value:ValueId,header:&StorageLifeContract,b:&mut Budget)->Check<bool>{
    if module.semantic.is_none()||function.storage_life.as_ref()!=Some(header){return Ok(false);}
    let Some(session)=b.semantic_checked.clone()else{return Ok(false);};
    if session.module!=module as *const TypedModule as usize{return Ok(false);}
    let mut worlds=0usize;let mut directed=false;
    for(id,environment)in &session.worlds{
        let instance=session.context.instance(module,*id,b)?;
        if instance.target!=SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id)){continue;}
        b.spend(session.results.len())?;
        let result=&session.results.iter().find(|(world,_)|world==id).ok_or_else(||bad("completed return view loses its exact original result frame"))?.1;
        let Some(view)=session.context.return_borrow_view(module,function,instance,block,value,result,environment,b)?else{return Ok(false);};
        directed|=view;worlds+=1;
    }
    Ok(worlds!=0&&directed)
}

/// An independently reconstructed physical stored source may be returned at
/// a shorter original root role. This authenticates only that exact selector's
/// outside-life bound; actual targets/parents remain with the stored solver.
pub(crate) fn return_slot_borrow_view(module:&TypedModule,function:&TypedFunction,parameter:u32,path:&[PlaceProjection],b:&mut Budget)->Check<bool>{
    if module.semantic.is_none()||function.storage_life.is_none(){return Ok(false);}
    let Some(session)=b.semantic_checked.clone()else{return Ok(false);};
    if session.module!=module as *const TypedModule as usize{return Ok(false);}
    let mut worlds=0usize;
    for(id,environment)in &session.worlds{
        let instance=session.context.instance(module,*id,b)?;
        if instance.target!=SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id)){continue;}
        b.spend(session.results.len())?;
        let result=&session.results.iter().find(|(world,_)|world==id).ok_or_else(||bad("stored Return source loses its completed original result frame"))?.1;
        if !session.context.return_slot_borrow_view(module,function,instance,parameter,path,result,environment,b)?{return Ok(false);}
        worlds+=1;
    }
    Ok(worlds!=0)
}

/// Exact shared-local transitions from the completed original programme and
/// every actual world. Raw headers/sites alone cannot create these rows.
pub(crate) fn local_role_transfers(module:&TypedModule,function:&TypedFunction,b:&mut Budget)->Check<Vec<semantic::local_roles::Transfer>>{
    if module.semantic.is_none(){return Ok(Vec::new());}
    b.spend(1)?;
    let Some(session)=b.semantic_checked.clone()else{return Ok(Vec::new());};
    if session.module!=module as *const TypedModule as usize{return Ok(Vec::new());}
    session.context.checked_local_role_transfers(module,function,b)
}

/// Static source correspondence is recomputed against the immutable actual
/// body. No cached W result or source class can activate an assignment loan.
pub(crate) fn assignment_writes(module:&TypedModule,f:&TypedFunction,b:&mut Budget)->Check<Vec<ValueId>> {
    if module.semantic.is_none()||f.foreign.is_some(){return Ok(Vec::new());}
    b.spend(1)?;
    let session=b.semantic_checked.clone().ok_or_else(||bad("assignment lacks the completely checked original programme"))?;
    if session.module!=module as *const TypedModule as usize{return Err(bad("assignment interpreter belongs to another immutable module"));}
    session.context.assignment_writes(module,f,b)
}

pub(crate) fn provider(module:&TypedModule,table:&DynVtable,declaration:&DynDeclaration,interface:&DynInterface,b:&mut Budget)->Check<()>{
    if table.implementation!=declaration.id||declaration.kind!=DynDeclarationKind::Impl{return Err(bad("semantic provider selects another complete original implementation"));}
    let life=table.self_life.as_ref().ok_or_else(||bad("semantic provider complete Self intrinsic domain is Unknown"))?;
    dynamic_metadata::life_shape(life,b)?;dynamic_metadata::ty(&table.self_type,b)?;b.spend(table.self_classes.len())?;
    let receiver=DynTypeValue{ty:table.self_type.clone(),life:life.clone(),classes:table.self_classes.clone()};
    let scope=DynInstantiationScope{fixed_binder_count:interface.retained_life.binder_count,invocation_binder_count:0};
    values::check(module,&receiver,scope,b)?;graph::kinds(module,&declaration.parameters,&table.actuals,scope,b)?;
    let count=declaration.implicit_lifetimes.as_ref().ok_or_else(||bad("semantic provider original anonymous occurrence directory is Unknown"))?.len();b.spend(count)?;
    let mut args=matching::Arguments{semantic_owner:Some(SemanticDynOwner::Declaration(declaration.id)),owner:terms::Owner::Witness(table.id,u32::MAX),terms:terms::Substitution::default(),projections:vec![],constants:vec![],collecting_anchors:false,declaration:std::borrow::Cow::Borrowed(&table.actuals),method:std::borrow::Cow::Borrowed(&[]),invocation:&[],implicit:vec![None;count]};
    let pattern=declaration.self_pattern.as_ref().ok_or_else(||bad("semantic provider original Self recipe is Unknown"))?;
    matching::ty(module,pattern,&receiver,&receiver,&mut args,b)?;
    matching::complete_anchors(module,declaration,&interface.application,&receiver,&mut args,b)?;
    let application=declaration.implemented_trait.as_ref().ok_or_else(||bad("semantic provider original trait application is Unknown"))?;
    matching::application(module,application,&interface.application,&receiver,&mut args,b)
}

fn method(owner:&SemanticDynOwner)->bool{!matches!(owner,SemanticDynOwner::Declaration(_))}
fn formal_role(owner:&SemanticDynOwner,scope:SemanticScopeRef)->Check<bool>{
    let SemanticScopeRef::Ancestor(depth)=scope;
    match(method(owner),depth){(false,0)|(true,1)=>Ok(false),(true,0)=>Ok(true),_=>Err(bad("semantic W formal escapes its complete declaration/method input frames"))}
}
fn bind_life(owner:&SemanticDynOwner,recipe:&SemanticLifeRecipe,term:terms::LifeTerm,args:&mut matching::Arguments<'_>,implicit:&mut Vec<(SemanticLifeRecipe,terms::LifeTerm)>,b:&mut Budget)->Check<()>{
    match recipe{
        SemanticLifeRecipe::Formal(reference)=>args.life(if formal_role(owner,reference.scope)?{DynLifePattern::MethodParameter(reference.ordinal)}else{DynLifePattern::DeclarationParameter(reference.ordinal)},term,b),
        SemanticLifeRecipe::Invocation(index)=>args.life(DynLifePattern::Invocation(*index),term,b),
        SemanticLifeRecipe::Implicit{..}=>{
            b.spend(implicit.len().saturating_add(1))?;
            if let Some((_,prior))=implicit.iter().find(|(known,_)|known==recipe){if *prior!=term{return Err(bad("one original semantic implicit occurrence has different intrinsic input origins"));}}
            else{implicit.push((recipe.clone(),term));}Ok(())
        },
        _=>Err(bad("semantic W match needs its complete bound/environment lifetime input frame")),
    }
}

#[allow(clippy::too_many_arguments)]
fn anchor(module:&TypedModule,owner:&SemanticDynOwner,recipe:&SemanticTypeRecipe,actual:&terms::Value,receiver:&terms::Value,args:&mut matching::Arguments<'_>,implicit:&mut Vec<(SemanticLifeRecipe,terms::LifeTerm)>,b:&mut Budget,depth:usize)->Check<()>{
    b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("semantic W input anchoring exceeds selected depth"));}
    match recipe{
        SemanticTypeRecipe::Formal(reference)=>{
            let pattern=if formal_role(owner,reference.scope)?{DynTypePattern::MethodParameter(reference.ordinal)}else{DynTypePattern::Parameter(reference.ordinal)};
            matching::walk(module,&pattern,actual,receiver,args,b,depth)?;
        },
        SemanticTypeRecipe::SelfType{..}=>matching::walk(module,&DynTypePattern::SelfType,actual,receiver,args,b,depth)?,
        SemanticTypeRecipe::Address{lifetime,inner,..}|SemanticTypeRecipe::Desc{lifetime,inner}=>{
            let physical=if let Type::BorrowedStorage{storage,..}=&actual.ty{storage.as_ref()}else{&actual.ty};
            let child=match physical{Type::Address{inner,..}|Type::Desc(inner)=>inner.as_ref(),_=>return Err(bad("semantic W outer recipe does not select an actual Address/Desc"))};
            if let SemanticOuterLifeRecipe::Class(life)=lifetime{
                let mut rows=actual.life.slots.iter().filter(|row|row.path.is_empty());let row=rows.next().ok_or_else(||bad("semantic W input lacks its complete actual outer occurrence"))?;
                if rows.next().is_some(){return Err(bad("semantic W input repeats its actual outer occurrence"));}
                bind_life(owner,life,actual.term(row.binder,b)?,args,implicit,b)?;
            }
            let child=matching::intrinsic_child(module,actual,child,LifeTypeProjection::Pointee,b)?;
            anchor(module,owner,inner,&child,receiver,args,implicit,b,depth+1)?;
        },
        SemanticTypeRecipe::Nominal{definition,arguments}=>{
            let semantic=module.semantic.as_ref().ok_or_else(||bad("semantic W nominal has no original programme"))?;b.spend(semantic.definitions.len())?;
            let original=semantic.definitions.iter().find(|row|row.id==*definition).ok_or_else(||bad("semantic W nominal original declaration is absent"))?;
            let SemanticDefinitionBody::Nominal{declaration,..}=original.body else{return Err(bad("semantic W nominal selects another original kind"));};
            let instance=matching::nominal(module,&actual.ty,b)?;
            if instance.declaration!=declaration||instance.arguments.len()!=arguments.len(){return Err(bad("semantic W nominal changes its original identity or full mixed arity"));}
            for(index,(recipe,argument))in arguments.iter().zip(&instance.arguments).enumerate(){b.spend(1)?;match(recipe,argument){
                (SemanticActualRecipe::Life(life),LifeNominalActual::Lifetime(class))=>{
                    b.spend(actual.life.nominal_arguments.len())?;let mut rows=actual.life.nominal_arguments.iter().filter(|row|row.path.is_empty()&&row.declaration==declaration&&row.argument==index as u32);
                    let row=rows.next().ok_or_else(||bad("semantic nominal Life input lacks its original full-formal occurrence"))?;
                    if rows.next().is_some()||row.binder!=*class{return Err(bad("semantic nominal Life input has another intrinsic occurrence"));}
                    bind_life(owner,life,actual.term(*class,b)?,args,implicit,b)?;
                },
                (SemanticActualRecipe::Type(recipe),LifeNominalActual::QualifiedType{ty,life})=>{
                    let child=matching::nested(actual,ty,life,LifeTypeProjection::TypeArgument(index as u32),b)?;anchor(module,owner,recipe,&child,receiver,args,implicit,b,depth+1)?;
                },
                (SemanticActualRecipe::Type(recipe),LifeNominalActual::Type(ty))=>{
                    let child=matching::intrinsic_child(module,actual,ty,LifeTypeProjection::TypeArgument(index as u32),b)?;anchor(module,owner,recipe,&child,receiver,args,implicit,b,depth+1)?;
                },
                (SemanticActualRecipe::Const{..},LifeNominalActual::Const(_))=>{},
                _=>return Err(bad("semantic W nominal changes a complete mixed actual kind")),
            }}
        },
        SemanticTypeRecipe::Managed(recipe)=>{let Type::Managed(child)=&actual.ty else{return Err(bad("semantic W Managed input changes its actual category"));};let child=matching::intrinsic_child(module,actual,child,LifeTypeProjection::Pointee,b)?;anchor(module,owner,recipe,&child,receiver,args,implicit,b,depth+1)?;},
        SemanticTypeRecipe::Tuple(recipes)=>{let Type::Tuple(children)=&actual.ty else{if recipes.is_empty()&&actual.ty==Type::Unit{return Ok(());}return Err(bad("semantic W Tuple input changes its actual category"));};if recipes.len()!=children.len(){return Err(bad("semantic W Tuple full arity differs"));}for(index,(recipe,child))in recipes.iter().zip(children).enumerate(){let child=matching::intrinsic_child(module,actual,child,LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),b)?;anchor(module,owner,recipe,&child,receiver,args,implicit,b,depth+1)?;}},
        SemanticTypeRecipe::Array{element,..}|SemanticTypeRecipe::Slice(element)|SemanticTypeRecipe::OwnedStore(element)=>{
            let child=match &actual.ty{Type::Array{element,..}|Type::Slice{element}|Type::OwnedStore{element}=>element,_=>return Err(bad("semantic W element input changes its actual constructor"))};let child=matching::intrinsic_child(module,actual,child,LifeTypeProjection::Element,b)?;anchor(module,owner,element,&child,receiver,args,implicit,b,depth+1)?;
        },
        SemanticTypeRecipe::ErrorUnion{error,result}=>{let Type::ErrorUnion{error:ae,result:ar}=&actual.ty else{return Err(bad("semantic W sum input changes its actual category"));};for(arm,recipe,ty)in[(true,error,ae),(false,result,ar)]{let child=matching::intrinsic_child(module,actual,ty,LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)),b)?;anchor(module,owner,recipe,&child,receiver,args,implicit,b,depth+1)?;}},
        // All remaining constructors and exact constants are interpreted by
        // the shared checker below; no branch is assumed to be Known0 here.
        _=>{},
    }Ok(())
}

fn signature_implicits(signature:&SemanticSignatureRecipe,b:&mut Budget)->Check<Vec<(SemanticScopeRef,u32)>>{
    enum Part<'a>{Type(&'a SemanticTypeRecipe),Life(&'a SemanticLifeRecipe)}
    let mut pending=vec![Part::Type(&signature.result)];for parameter in signature.parameters.iter().rev(){pending.push(Part::Type(&parameter.ty));}
    let mut result=Vec::new();
    while let Some(part)=pending.pop(){b.spend(1)?;match part{
        Part::Life(SemanticLifeRecipe::Implicit{scope,ordinal})=>{b.spend(result.len().saturating_add(1))?;if !result.contains(&(*scope,*ordinal)){result.push((*scope,*ordinal));}},
        Part::Life(_)=>{},
        Part::Type(recipe)=>match recipe{
            SemanticTypeRecipe::Address{lifetime,inner,..}|SemanticTypeRecipe::Desc{lifetime,inner}=>{pending.push(Part::Type(inner));if let SemanticOuterLifeRecipe::Class(life)=lifetime{pending.push(Part::Life(life));}},
            SemanticTypeRecipe::Managed(inner)|SemanticTypeRecipe::Slice(inner)|SemanticTypeRecipe::OwnedStore(inner)|SemanticTypeRecipe::Array{element:inner,..}=>pending.push(Part::Type(inner)),
            SemanticTypeRecipe::Tuple(fields)=>pending.extend(fields.iter().rev().map(Part::Type)),
            SemanticTypeRecipe::ErrorUnion{error,result}=>{pending.push(Part::Type(result));pending.push(Part::Type(error));},
            SemanticTypeRecipe::Nominal{arguments,..}=>for argument in arguments.iter().rev(){match argument{SemanticActualRecipe::Type(ty)=>pending.push(Part::Type(ty)),SemanticActualRecipe::Life(life)=>pending.push(Part::Life(life)),_=>{}}},
            _=>{},
        },
    }}Ok(result)
}

pub(super) fn value(module:&TypedModule,scope:SemanticDefinitionId,recipe:&SemanticTypeRecipe,receiver:&terms::Value,args:&mut matching::Arguments<'_>,actual:Option<&terms::Value>,b:&mut Budget)->Check<terms::Value>{
    Ok(value_inner(module,scope,recipe,receiver,args,actual,false,b)?.0)
}

/// Same-evaluation nominal outside sources, kept separate from the value's κ.
/// The bool records a genuine physical borrow slot in that nominal domain.
pub(super) fn value_with_sources(module:&TypedModule,scope:SemanticDefinitionId,recipe:&SemanticTypeRecipe,receiver:&terms::Value,args:&mut matching::Arguments<'_>,actual:Option<&terms::Value>,b:&mut Budget)->Check<(terms::Value,Vec<(terms::LifeTerm,terms::LifeTerm,bool)>)>{
    value_inner(module,scope,recipe,receiver,args,actual,true,b)
}
fn value_inner(module:&TypedModule,scope:SemanticDefinitionId,recipe:&SemanticTypeRecipe,receiver:&terms::Value,args:&mut matching::Arguments<'_>,actual:Option<&terms::Value>,collect_sources:bool,b:&mut Budget)->Check<(terms::Value,Vec<(terms::LifeTerm,terms::LifeTerm,bool)>)>{
    let owner=args.semantic_owner.clone().ok_or_else(||bad("semantic W value has no independently selected exact original owner"))?;
    bridge::owner(module,&owner,scope,b)?;
    b.spend(1)?;let session=b.semantic_checked.clone().ok_or_else(||bad("semantic W value lacks the completely checked original programme interpreter"))?;
    if session.module!=module as *const TypedModule as usize{return Err(bad("semantic W interpreter belongs to another immutable module"));}
    let mut active=session.active.try_borrow_mut().map_err(|_|bad("semantic W interpreter recursively reentered its active request"))?;
    let context=&session.context;
    let mut implicit=Vec::new();if let Some(actual)=actual{anchor(module,&owner,recipe,actual,receiver,args,&mut implicit,b,0)?;}
    let token=active.fresh(b)?;let mut symbols=symbols::Symbols::new(args.owner,token);
    let current_key=ScopeKey::Witness{definition:scope,frame:token};
    let receiver=symbols.outgoing(receiver,current_key,OperandKey::Receiver,b)?;
    let mut definitions=Vec::new();let mut current=Some(scope);
    while let Some(id)=current{b.spend(1)?;if definitions.len()>=b.limits.max_call_depth.min(128)||definitions.contains(&id){return Err(limit("semantic W original lexical chain exceeds selected bound"));}definitions.push(id);current=context.definition(module,id,b)?.parent;}
    let mut env=EvalEnv{lexical:vec![],assumptions:vec![],images:vec![],bounds:vec![],invocations:vec![],at:None};
    let mut proofs=semantic_external::Proofs::default();
    for(index,id)in definitions.iter().copied().enumerate().rev(){
        let definition=context.definition(module,id,b)?;let key=if index==0{current_key}else{ScopeKey::Witness{definition:id,frame:active.fresh(b)?}};
        let is_method=method(&owner)&&index==0;
        let supplied=if index==0&&!method(&owner)||method(&owner)&&index==1{args.declaration.as_ref()}else if is_method{args.method.as_ref()}else{&[]};
        if supplied.len()!=definition.parameters.len(){return Err(bad("semantic W lexical input omits an original ancestor's full mixed actuals"));}
        b.spend(supplied.len())?;for actual in supplied{dynamic_metadata::recipe(actual,b)?;}let supplied=supplied.to_vec();
        let mut actuals=Vec::new();
        for(ordinal,actual)in supplied.iter().enumerate(){b.spend(1)?;actuals.push(match actual{
            DynGenericActual::Type(_)|DynGenericActual::QualifiedType(_)=>{let value=instantiate::actual_type(module,is_method,ordinal as u32,args,b)?;CheckedActual::Type(symbols.outgoing(&value,key,OperandKey::TypeArgument{scope:key,ordinal:ordinal as u32},b)?)},
            DynGenericActual::Life(image)=>{let pattern=if is_method{DynLifePattern::MethodParameter(ordinal as u32)}else{DynLifePattern::DeclarationParameter(ordinal as u32)};let term=args.terms.optional_life(pattern,b)?.unwrap_or(terms::LifeTerm{origin:terms::Origin{owner:args.owner,root:terms::Root::Lexical(*image),class:0},image:*image});let class=symbols.life(ClassKey::Formal{scope:key,ordinal:ordinal as u32},term,b)?;CheckedActual::Life(class)},
            DynGenericActual::Const(value)=>CheckedActual::Const{carrier:CheckedType{type_cell: crate::semantic::CheckedTypeData::new(value.ty()),life:StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:vec![],origins:vec![],statics:vec![]},value:value.clone()},
        });}
        let mut sealed=Vec::new();for formal in &definition.environment{sealed.push(context.evaluate_actual(module,&formal.source,&env,&mut proofs,&mut active,b)?);}
        let self_type=matches!(definition.body,SemanticDefinitionBody::Scope(_)).then(||receiver.clone());
        env.lexical.insert(0,LexicalFrame{definition:id,key,actuals,environment:sealed,self_type});
    }
    let definition=context.definition(module,scope,b)?;
    if let SemanticDefinitionBody::FunctionWithBody{signature,..}|SemanticDefinitionBody::Function{signature,..}=&definition.body{
        for(index,(scope,ordinal))in signature_implicits(signature,b)?.into_iter().enumerate(){
            let SemanticScopeRef::Ancestor(depth)=scope;let frame=env.lexical.get(depth as usize).ok_or_else(||bad("semantic method implicit header escapes its original lexical scope"))?;
            let position=signature.invocation_lifetimes.checked_add(index as u32).ok_or_else(||limit("semantic method implicit invocation domain overflow"))?;
            let image=StorageBinder::Invocation(position);symbols.bind(ClassKey::Implicit{scope:frame.key,ordinal},terms::LifeTerm{origin:terms::Origin{owner:args.owner,root:terms::Root::Lexical(image),class:0},image},b)?;
        }
    }
    for(recipe,term)in implicit{let key=context.life(module,&recipe,&env,b)?;symbols.bind(key,term,b)?;}
    if let SemanticDefinitionBody::FunctionWithBody{signature,..}|SemanticDefinitionBody::Function{signature,..}=&definition.body{
        for index in 0..signature.invocation_lifetimes{let class=ClassKey::Invocation{frame:token,index};symbols.bind(class.clone(),args.terms.life(DynLifePattern::Invocation(index),b)?,b)?;env.invocations.push(class);}
    }
    let value=context.evaluate_type(module,recipe,&env,&mut proofs,&mut active,b)?;
    if !proofs.callable_shapes.is_empty()||!proofs.pending_statics.is_empty()||!proofs.pending_original_methods.is_empty()||!proofs.pending_original_method_relations.is_empty(){return Err(bad("semantic W recipe retains an undischarged external proof obligation"));}
    let result=symbols.incoming(&value,b)?;
    if !collect_sources{return Ok((result,vec![]));}
    let sources=symbols.nominal_sources(&value,b)?;
    Ok((result,sources))
}

/// Query signatures may use fresh nominal result provenance only from this
/// already-completed immutable original/actual session and true Return ops.
pub(super) fn checked_result_sources(module:&TypedModule,function:&TypedFunction,parameters:&[terms::Value],result:&terms::Value,rows:&[(terms::LifeTerm,terms::LifeTerm,bool)],b:&mut Budget)->Check<Vec<(terms::LifeTerm,terms::LifeTerm)>>{
    if !matches!(result.raw.ty,Type::BorrowedStorage{..}){return Ok(vec![]);}
    let session=b.semantic_checked.clone().ok_or_else(||bad("query nominal result has no completed original programme"))?;
    if session.module!=module as *const TypedModule as usize{return Err(bad("query nominal result belongs to another immutable programme"));}
    let inputs=parameters.iter().map(|value|&value.raw).collect::<Vec<_>>();
    let fresh=session.context.checked_nominal_result_classes(module,function,&inputs,&result.raw,b)?;
    b.spend(rows.len().saturating_mul(fresh.len().saturating_add(1)))?;
    let mut sources=Vec::new();
    for class in fresh{
        let term=result.term(class,b)?;
        if parameters.iter().any(|value|value.terms.contains(&term)){continue;}
        for(result,source,_)in rows{if *result==term{sources.push((*result,*source));}}
    }
    Ok(sources)
}
