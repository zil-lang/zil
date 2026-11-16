//! A static call authenticates its original provider and entry independently.
//! The optional dynamic-specialization directory must agree with that proof;
//! it supplies neither a vtable, a callable contract nor memory authority.
use super::*;

fn include(domain:&mut Domain<'_>,actual:&CheckedActual,b:&mut Budget)->Check<()>{
    match actual{
        CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>domain.value(value,b),
        CheckedActual::Life(key)=>domain.key(key,false,b),
    }
}
fn actual(domain:&mut Domain<'_>,actual:&CheckedActual,b:&mut Budget)->Check<DynGenericActual>{
    charge_actual(actual,b)?;
    Ok(match actual{
        CheckedActual::Type(value)=>DynGenericActual::QualifiedType(domain.symbols.incoming(value,b)?.raw),
        CheckedActual::Life(key)=>DynGenericActual::Life(domain.symbols.class(key,b)?.image),
        CheckedActual::Const{carrier,value}=>{
            if carrier.ty!=value.ty()||carrier.life!=semantic::empty_life()||!carrier.classes.is_empty()||!carrier.origins.is_empty()||!carrier.statics.is_empty(){return Err(bad("static member constant changes its complete closed carrier"));}
            dynamic_metadata::constant(value,b)?;DynGenericActual::Const(value.clone())
        },
    })
}

pub(crate) fn check(
    module:&TypedModule,context:&semantic::CheckedContext,environment:&EvalEnv,
    function:&TypedFunction,receipt:&OriginalMemberReceipt,active:&mut semantic::ActiveQueries,b:&mut Budget,
)->Check<()>{
    let Some(instance)=&function.dyn_instance else{return Ok(());};
    dynamic_metadata::bounds_recipe(instance,b)?;
    if instance.definition!=receipt.definition{return Err(bad("actual static member instance selects another original definition"));}
    b.spend(module.dyn_declarations.len())?;
    let mut owners=module.dyn_declarations.iter().filter(|row|row.id==receipt.body_owner);
    let owner=owners.next().ok_or_else(||bad("static member definition owner is absent"))?;
    if owners.next().is_some(){return Err(bad("static member definition owner is ambiguous"));}
    let rows=owner.method_definitions.as_ref().ok_or_else(||bad("static member definition inventory is Unknown"))?;
    b.spend(rows.len())?;
    let mut definitions=rows.iter().filter(|row|row.id==receipt.definition);
    let definition=definitions.next().ok_or_else(||bad("static member definition is absent from its exact original owner"))?;
    if definitions.next().is_some()||definition.member!=receipt.body_member||definition.scope!=receipt.source{return Err(bad("static member definition changes its exact original member/source identity"));}
    dynamic_metadata::bounds_recipe(definition,b)?;
    b.spend(definition.instances.len())?;
    let mut targets=definition.instances.iter().filter(|row|row.function==DynFunctionTarget::Typed(function.id));
    let target=targets.next().ok_or_else(||bad("static member entry is absent from its original specialization inventory"))?;
    if targets.next().is_some(){return Err(bad("static member entry has duplicate original specialization claims"));}
    dynamic_metadata::bounds_recipe(target,b)?;
    if !external::equal(&instance.receiver,&target.receiver,b)?
        ||!external::actuals_equal(&instance.declaration_actuals,&target.declaration_actuals,b)?
        ||!external::actuals_equal(&instance.method_actuals,&target.method_actuals,b)?{
        return Err(bad("static member instance differs from its original complete specialization"));
    }
    let signature=&receipt.actual_signature;
    let mut domain=Domain::new(context,environment,active,b)?;
    // The method's own quantified frame is installed before any outside key.
    // Equal outside images cannot turn a caller Fixed into this invocation.
    for key in signature.bound.iter().chain(&signature.invocation){domain.key(key,true,b)?;}
    domain.value(&receipt.receiver,b)?;
    for value in receipt.declaration_actuals.iter().chain(&receipt.method_actuals){include(&mut domain,value,b)?;}
    for(value,_)in &signature.parameters{domain.value(value,b)?;}
    domain.value(&signature.result,b)?;
    scopes::bounded(domain.scope,b)?;
    let receiver=domain.symbols.incoming(&receipt.receiver,b)?;
    let mut declaration_actuals=Vec::new();let mut method_actuals=Vec::new();
    for value in &receipt.declaration_actuals{declaration_actuals.push(actual(&mut domain,value,b)?);}
    for value in &receipt.method_actuals{method_actuals.push(actual(&mut domain,value,b)?);}
    b.spend(signature.parameters.len())?;
    let mut parameters=Vec::new();for(value,_)in &signature.parameters{parameters.push(domain.symbols.incoming(value,b)?);}
    let result=domain.symbols.incoming(&signature.result,b)?;
    let(_,header)=headers::rebased_function(module,function,&parameters,&result,b)?;
    // Match all original mixed actuals and every header embedding jointly.
    // This also requires every lexical (including phantom) class to be
    // anchored; no single same-shaped header can select the provider/body.
    signatures::specialization(module,function,receipt.definition,&receiver,&declaration_actuals,&method_actuals,&header,b)?;
    Ok(())
}
