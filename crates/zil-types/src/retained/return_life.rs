//! Resolve a public returned-callable bound against its complete actual
//! environment domain. This produces an untrusted expectation; kernels still
//! reconstruct the concrete signature, origins, Take operations and loans.
use super::*;

fn invalid(span: Span, text: &str) -> Diagnostic {
    error("E-CALLABLE-RETURN-LIFE", span, text)
}

fn anchor(
    input: &CoreClosureInputAnchor,
    context: &CoreCallableReturnLifeContext,
    f: &TypedFunction,
    left: &mut usize,
) -> R<StorageBinder> {
    if input.parameter as usize >= f.parameters.len()
        || input.source_parameter.is_some_and(|index| {
            context.parameters.get(index as usize) != Some(&LifeParameterKind::Lifetime)
        })
    {
        return Err(invalid(f.span, "return-bound anchor is not an actual source input"));
    }
    let header = f.storage_life.as_ref().ok_or_else(|| {
        invalid(f.span, "return-bound anchor has no complete actual creator header")
    })?;
    let actual = match &input.position {
        CoreClosureHeaderPosition::Borrow(position) => {
            if position.base != StorageBase::Parameter(input.parameter) {
                return Err(invalid(f.span, "return-bound borrow selects a different input"));
            }
            spend(left, header.bindings.len().saturating_add(position.path.len()), f.span)?;
            let mut matches = header.bindings.iter().filter(|r| r.position == *position);
            let found = matches.next().map(|r| r.binder);
            if matches.next().is_some() { return Err(invalid(f.span,"ambiguous actual creator borrow row")); }
            found
        }
        CoreClosureHeaderPosition::Class(position) => {
            if position.base != StorageBase::Parameter(input.parameter) {
                return Err(invalid(f.span, "return-bound class selects a different input"));
            }
            spend(left, header.classes.len().saturating_add(position.path.len()), f.span)?;
            let mut matches = header.classes.iter().filter(|r| r.position == *position);
            let found = matches.next().map(|r| r.binder);
            if matches.next().is_some() { return Err(invalid(f.span,"ambiguous actual creator class row")); }
            found
        }
    }.ok_or_else(|| invalid(f.span,"return-bound anchor omits its actual header position"))?;
    if actual != input.header_binder {
        return Err(invalid(f.span,"return-bound anchor contradicts its actual header binder"));
    }
    Ok(actual)
}

fn external(base: StorageBase) -> bool { !matches!(base,StorageBase::Capture(_)) }

pub(crate) fn populate(
    module: &mut TypedModule,
    bounds: &BTreeMap<FunctionId, CoreCallableReturn>,
    left: &mut usize,
) -> R<()> {
    let mut updates = Vec::new();
    for (id,bound) in bounds {
        let Some(proposal)=&bound.life else {
            if bound.source_life.is_some() {return Err(invalid(Span::default(),"return context omits its bound relation"));}
            continue;
        };
        let f=module.functions.get(id.0 as usize).ok_or_else(||invalid(Span::default(),"return bound has no actual creator"))?;
        let returned=f.callable_return_contract.as_ref().ok_or_else(||invalid(f.span,"return bound has no actual returned environment"))?;
        let d=module.closures.get(returned.closure.0 as usize).ok_or_else(||invalid(f.span,"returned environment definition is absent"))?;
        let actual=d.storage_life.as_ref().ok_or_else(||invalid(f.span,"return bound requires the full actual environment class domain"))?;
        spend(left,borrowed_fp::metadata_work(proposal).saturating_add(borrowed_fp::metadata_work(actual)),f.span)?;
        if d.kind!=bound.kind || d.parameters!=bound.parameters || d.result!=bound.result {
            return Err(invalid(f.span,"return bound differs from the actual callable signature"));
        }
        if proposal.bindings.iter().any(|r|!external(r.position.base)) || proposal.classes.iter().any(|r|!external(r.position.base)) {
            return Err(invalid(f.span,"public return bound cannot declare environment capture rows"));
        }
        let mut fixed_inputs=Vec::new();
        if let Some(context)=&bound.source_life {
            if context.declaration.compiler || !context.identity.generation_path.is_empty() || context.fixed.len()!=proposal.fixed_binder_count as usize {
                return Err(invalid(f.span,"return-bound source context is incomplete or generated"));
            }
            spend(left,context.fixed.len(),f.span)?;
            for (index,row) in context.fixed.iter().enumerate() {
                if row.fixed as usize!=index {return Err(invalid(f.span,"return Fixed anchors are not complete and canonical"));}
                fixed_inputs.push(anchor(&row.input,context,f,left)?);
            }
        } else if proposal.fixed_binder_count!=0 {
            return Err(invalid(f.span,"return bound Fixed classes require actual source input anchors"));
        }
        let header=f.storage_life.as_ref();
        let mut fixed_map=vec![None;proposal.fixed_binder_count as usize];
        let mut invocation_map=vec![None;proposal.invocation_binder_count as usize];
        let mut seen_actual_invocation=BTreeMap::new();
        let mut resolve = |proposed:StorageBinder, found:StorageBinder, output:bool|->R<StorageBinder> {
            match (proposed,found) {
                (StorageBinder::Fixed(old),StorageBinder::Fixed(real))=>{
                    if real>=actual.fixed_binder_count {return Err(invalid(f.span,"actual return signature Fixed index exceeds full domain"));}
                    let selected=*fixed_inputs.get(old as usize).ok_or_else(||invalid(f.span,"bound Fixed index has no source anchor"))?;
                    let header=header.ok_or_else(||invalid(f.span,"returned Fixed class omits creator result map"))?;
                    spend(left,header.classes.len(),f.span)?;
                    let mut rows=header.classes.iter().filter(|r|r.position.base==StorageBase::Result && r.position.path.is_empty() && r.position.class==real);
                    let row=rows.next().ok_or_else(||invalid(f.span,"actual returned class has no complete creator result mapping"))?;
                    if row.binder!=selected || rows.next().is_some() {return Err(invalid(f.span,"returned bound selects a different actual creator lifetime"));}
                    let entry=&mut fixed_map[old as usize];
                    if entry.replace(real).is_some_and(|previous|previous!=real) {return Err(invalid(f.span,"one bound Fixed class maps to distinct actual environment classes"));}
                    Ok(StorageBinder::Fixed(real))
                }
                (StorageBinder::Invocation(old),StorageBinder::Invocation(real))=>{
                    let entry=invocation_map.get_mut(old as usize).ok_or_else(||invalid(f.span,"bound invocation index exceeds its domain"))?;
                    if real>=actual.invocation_binder_count || (output && entry.is_none()) {return Err(invalid(f.span,"return invocation is not grounded in an actual input or receiver"));}
                    if entry.replace(real).is_some_and(|prior|prior!=real) {return Err(invalid(f.span,"bound invocation equality differs from actual signature"));}
                    if seen_actual_invocation.insert(real,old).is_some_and(|prior|prior!=old) {return Err(invalid(f.span,"independent bound invocations collapse into one actual class"));}
                    Ok(StorageBinder::Invocation(real))
                }
                _=>Err(invalid(f.span,"return bound cannot relabel a Fixed class as an invocation or erase a receiver dependency")),
            }
        };
        let actual_bindings=actual.bindings.iter().filter(|r|external(r.position.base)).collect::<Vec<_>>();
        let actual_classes=actual.classes.iter().filter(|r|external(r.position.base)).collect::<Vec<_>>();
        if actual_bindings.len()!=proposal.bindings.len() || actual_classes.len()!=proposal.classes.len() {return Err(invalid(f.span,"public returned callable omits a real input/result/receiver position"));}
        let mut expectation=proposal.clone();
        for output in [false,true] {
            for (proposed,found) in expectation.bindings.iter_mut().zip(&actual_bindings) {
                if proposed.position!=found.position {return Err(invalid(f.span,"return bound borrow positions differ from actual signature"));}
                if (proposed.position.base==StorageBase::Result)==output {
                    proposed.binder=resolve(proposed.binder,found.binder,output)?;
                }
            }
            for (proposed,found) in expectation.classes.iter_mut().zip(&actual_classes) {
                if proposed.position!=found.position {return Err(invalid(f.span,"return bound class positions differ from actual signature"));}
                if (proposed.position.base==StorageBase::Result)==output {
                    proposed.binder=resolve(proposed.binder,found.binder,output)?;
                }
            }
        }
        if fixed_map.iter().any(Option::is_none) || invocation_map.iter().any(Option::is_none) {return Err(invalid(f.span,"return bound declares an ungrounded lifetime domain"));}
        // Extra captured classes remain rigid and present even if the bound's
        // Parameter/Result positions never mention them.
        expectation.fixed_binder_count=actual.fixed_binder_count;
        expectation.invocation_binder_count=actual.invocation_binder_count;
        updates.push((*id,expectation));
    }
    for (id,life) in updates {module.functions[id.0 as usize].callable_return_contract.as_mut().unwrap().life=Some(life);}
    Ok(())
}
