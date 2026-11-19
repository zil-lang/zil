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
    let sources=result_sources(module,context,environment,function,signature,&mut domain,&result,b)?;
    let borrows=input_borrow_sources(signature,&mut domain,b)?;
    let(_,header)=headers::rebased_function_with_sources(module,function,&parameters,&result,&sources,&borrows,b)?;
    // Match all original mixed actuals and every header embedding jointly.
    // This also requires every lexical (including phantom) class to be
    // anchored; no single same-shaped header can select the provider/body.
    signatures::specialization(module,function,receipt.definition,&receiver,&declaration_actuals,&method_actuals,&header,b)?;
    Ok(())
}

/// Preserve result κ and derive only a separate outside-source row. The body
/// and every Return are still pending until static-member completion.
fn result_sources(module:&TypedModule,context:&semantic::CheckedContext,environment:&EvalEnv,
    function:&TypedFunction,signature:&semantic::CheckedSignature,domain:&mut Domain<'_>,
    result:&terms::Value,b:&mut Budget)->Check<Vec<(terms::LifeTerm,terms::LifeTerm)>>{
    let Some((descriptor,fresh))=context.nominal_result_sources(environment,signature,b)?else{return Ok(vec![]);};
    if context.nominal_result_constructors(module,function,b)?.is_none(){return Ok(vec![]);}
    domain.value(&descriptor,b)?;
    let outside=domain.symbols.incoming(&descriptor,b)?;
    b.spend(fresh.len())?;
    if result.terms.len()!=fresh.len()||outside.terms.len()!=fresh.len(){return Err(bad("nominal result outside directory changes its full intrinsic cardinality"));}
    let mut sources=Vec::new();
    for(index,((term,source),fresh))in result.terms.iter().zip(&outside.terms).zip(fresh).enumerate(){
        if !fresh{continue;}
        if let IntrinsicOrigin::DerivedClass{source:key}=&descriptor.origins[index]{if domain.symbols.class(key,b)?.image!=term.image{return Err(bad("nominal result outside source changes its independently resolved image"));}}
        sources.push((*term,*source));
    }
    Ok(sources)
}

/// A saved physical pointer has an outside σ in addition to its holder κ.
/// This directory only authenticates borrow outputs; full input/class output
/// domains and the actual LoadStoredBorrow/loan/parent proof stay independent.
fn input_borrow_sources(signature:&semantic::CheckedSignature,domain:&mut Domain<'_>,b:&mut Budget)->Check<Vec<(terms::LifeTerm,terms::LifeTerm)>>{
    let mut rows=Vec::new();
    for(parameter,_)in &signature.parameters{
        for origin in &parameter.origins{if let IntrinsicOrigin::Nominal{source,class}=origin{
            b.spend(source.domain.slots.len())?;
            if source.domain.slots.iter().any(|slot|slot.binder==*class){
                if let Some(IntrinsicOrigin::DerivedClass{source})=source.class_sources.get(*class as usize){domain.key(source,false,b)?;}
            }
        }}
        for(input,source,physical)in domain.symbols.nominal_sources(parameter,b)?{
            if physical{b.spend(1)?;rows.push((input,source));}
        }
    }Ok(rows)
}

fn original_recipe<'a>(module:&'a TypedModule,owner:&SemanticDynOwner,b:&mut Budget)->Check<&'a DynSignatureRecipe>{
    b.spend(module.dyn_declarations.iter().fold(module.dyn_declarations.len(),|n,row|n.saturating_add(row.method_signatures.as_ref().map_or(0,Vec::len)).saturating_add(row.method_definitions.as_ref().map_or(0,Vec::len))))?;
    match owner{
        SemanticDynOwner::MethodDefinition(id)=>{
            let mut rows=module.dyn_declarations.iter().flat_map(|owner|owner.method_definitions.iter().flatten()).filter(|row|row.id==*id);
            let row=rows.next().ok_or_else(||bad("definition contract original signature is absent"))?;
            if rows.next().is_some(){return Err(bad("definition contract original signature is ambiguous"));}Ok(&row.signature)
        },
        SemanticDynOwner::MethodSignature{declaration,member}=>{
            let mut owners=module.dyn_declarations.iter().filter(|row|row.id==*declaration);
            let owner=owners.next().ok_or_else(||bad("definition contract original trait is absent"))?;
            if owners.next().is_some(){return Err(bad("definition contract original trait is ambiguous"));}
            let mut rows=owner.method_signatures.iter().flatten().filter(|row|row.member==*member);
            let row=rows.next().ok_or_else(||bad("definition contract original trait signature is absent"))?;
            if rows.next().is_some(){return Err(bad("definition contract original trait signature is ambiguous"));}Ok(&row.signature)
        },
        _=>Err(bad("definition contract has another original signature role")),
    }
}

/// Selection/refinement is not a runtime contract proof. Preserve the trait
/// and body contracts as separate obligations for this exact executable entry.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_requests(
    proofs:&mut Proofs,module:&TypedModule,context:&semantic::CheckedContext,
    function:&TypedFunction,receipt:&OriginalMemberReceipt,
    active:&mut semantic::ActiveQueries,b:&mut Budget,
)->Check<Vec<requests::ContractRequest>>{
    b.spend(2)?;let mut out=Vec::with_capacity(2);
    let body=SemanticDynOwner::MethodDefinition(receipt.definition);
    let declaration=SemanticDynOwner::MethodSignature{declaration:receipt.application.declaration,member:receipt.member};
    for(signature,environment,owner,recipe)in [
        (&receipt.actual_signature,&receipt.environment,&body,&receipt.body_contract),
        (&receipt.expected_signature,&receipt.trait_environment,&declaration,&receipt.trait_contract),
    ]{
        let mut domain=Domain::new(context,environment,active,b)?;
        for key in signature.bound.iter().chain(&signature.invocation){domain.key(key,true,b)?;}
        domain.value(&receipt.receiver,b)?;
        for value in receipt.declaration_actuals.iter().chain(&receipt.method_actuals){include(&mut domain,value,b)?;}
        for(value,_)in &signature.parameters{domain.value(value,b)?;}domain.value(&signature.result,b)?;
        b.spend(signature.parameters.len().saturating_mul(2))?;
        let mut parameters=Vec::with_capacity(signature.parameters.len());
        let mut parameter_modes=Vec::with_capacity(signature.parameters.len());
        for(value,mode)in &signature.parameters{parameters.push(domain.symbols.incoming(value,b)?);parameter_modes.push(*mode);}
        let result=domain.symbols.incoming(&signature.result,b)?;
        let sources=result_sources(module,context,environment,function,signature,&mut domain,&result,b)?;
        let(contract,operands)=contracts::materialize(recipe,|pattern,b|{
            let recipe=associated::scoped_type_recipe_view(module,owner,pattern,b)?;
            let value=context.evaluate_type(module,&recipe,environment,proofs,active,b)?;
            domain.value(&value,b)?;domain.symbols.incoming(&value,b)
        },b)?;
        contracts::charge(module,&contract,b)?;
        for parameter in &parameters{values::check(module,&parameter.raw,domain.scope,b)?;}
        values::check(module,&result.raw,domain.scope,b)?;
        for operand in &operands{values::check(module,&operand.value,domain.scope,b)?;}
        let mut resolved=signatures::Resolved{parameters,parameter_modes,result,
            result_mode:signature.result_mode,call_kind:signature.call_kind,contract,operands};
        let receiver=domain.symbols.incoming(&receipt.receiver,b)?;
        let original=original_recipe(module,owner,b)?;
        dynamic_metadata::recipe(original,b)?;dynamic_metadata::recipe(recipe,b)?;
        if original.contract!=*recipe{return Err(bad("definition contract differs from its exact original signature"));}
        contracts::defaults::instantiate_definition(module,owner,original,&mut resolved,&receiver,&sources,domain.scope,b)?;
        let borrows=input_borrow_sources(signature,&mut domain,b)?;
        let(expected_header,embedding)=resolved.function(module,function,&sources,&borrows,b)?;
        let leaves=inventory::self_leaves(module,&receiver,domain.scope,b)?;
        out.push(resolved.request(requests::Origin::Definition{owner:receipt.body_owner,
            member:receipt.body_member,definition:receipt.definition},function,domain.scope,
            expected_header,embedding,leaves,b)?);
    }
    Ok(out)
}
