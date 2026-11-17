//! Instantiate only independently checked callable source relations. A relation
//! selects an actual argument's existing roots; it never constructs a root.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
type Slots = BTreeMap<u32, BTreeSet<captures::Source>>;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-CALLABLE-SOURCE", None, message)
}
fn static_source(module:&TypedModule,closure:ClosureId,binding:&StorageSourceBinding,atom:&StorageSource,b:&mut Budget)->Check<bool>{
    let StorageSource::Static{leaf,mode}=atom else{return Ok(false);};
    if !matches!(binding.position.base,StorageBase::Capture(_))||*mode!=StoredBorrowMode::SharedAddress||!matches!(leaf,Type::Address{mutable:false,..}){return Err(bad("returned Static source is not its exact immutable capture leaf"));}
    crate::dynamic_metadata::ty(leaf,b)?;crate::dynamic_metadata::position(&binding.position,b)?;
    let row=crate::semantic::StaticOccurrence::Logical{closure,position:binding.position.clone(),leaf:leaf.clone(),mode:*mode};
    crate::semantic::task_domains::require_static_occurrence(module,&row,b)?;Ok(true)
}
pub(super) fn inputs(
    module: &TypedModule,
    function: FunctionId,
    budget: &mut Budget,
) -> Check<BTreeSet<u32>> {
    let mut out = BTreeSet::new();
    let Some(claim) = &module.functions[function.0 as usize].callable_return_contract else {
        return Ok(out);
    };
    let sources = claim
        .capture_sources
        .as_ref()
        .ok_or_else(|| bad("returned environment sources are unknown"))?;
    budget.spend(sources.len())?;
    for binding in sources {
        budget.spend(binding.sources.len())?;
        for atom in &binding.sources {
            if static_source(module,claim.closure,binding,atom,budget)?{continue;}
            let source = crate::exact_storage_source(atom)?;
            let StorageBase::Parameter(index) = source.base else {
                return Err(bad("returned environment source is not an input"));
            };
            out.insert(index);
        }
    }
    Ok(out)
}
pub(super) fn instantiate(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    ordinary: &[Roots],
    captured: Option<&[captures::Slots]>,
    budget: &mut Budget,
) -> Check<Slots> {
    let callee = &module.functions[function.0 as usize];
    let Some(claim) = &callee.callable_return_contract else {
        return Ok(Slots::new());
    };
    let Some(bindings) = &claim.capture_sources else {
        return Err(bad("returned environment sources are unknown"));
    };
    let mut slots = Slots::new();
    budget.spend(bindings.len())?;
    for binding in bindings {
        let StorageBase::Capture(capture) = binding.position.base else {
            return Err(bad("returned environment target is not a capture"));
        };
        let mut ordinary_sources=Vec::new();
        budget.spend(binding.sources.len())?;
        for atom in &binding.sources{
            if static_source(module,claim.closure,binding,atom,budget)?{
                let row=slots.entry(capture).or_insert_with(BTreeSet::new);budget.spend(row.len().saturating_add(1))?;
                row.insert(captures::Source::Root(Root::Static));
            }else{ordinary_sources.push(atom);}
        }
        if ordinary_sources.is_empty(){continue;}
        if matches!(
            binding.position.path.last(),
            Some(ContractProjection::RetainedPort(_))
        ) {
            if !budget.task_sources() {
                return Err(bad(
                    "returned Task capture lacks complete stored source proof",
                ));
            }
            for atom in ordinary_sources {
                let source = crate::exact_storage_source(atom)?;
                let StorageBase::Parameter(parameter) = source.base else {
                    return Err(bad("returned Task port source is not an input"));
                };
                let argument = *arguments
                    .get(parameter as usize)
                    .ok_or_else(|| bad("returned Task port argument absent"))?;
                let (formal, _) =
                    crate::retained::sources::source_type(module, callee, source, budget)?;
                if !crate::stored_flow::task_capture_port(
                    module,
                    claim.closure,
                    &binding.position,
                    formal,
                    budget,
                )? {
                    return Err(bad("returned Task endpoint is not opaque port"));
                }
                let actual = budget
                    .task_stored_sources
                    .as_ref()
                    .and_then(|rows| rows.get(f.id.0 as usize))
                    .filter(|row| row.covered)
                    .and_then(|row| row.values.get(argument.0 as usize))
                    .ok_or_else(|| {
                        bad("returned Task capture omitted its actual argument source proof")
                    })?;
                let work = actual
                    .retained
                    .iter()
                    .fold(source.path.len(), |n, (path, roots)| {
                        n.saturating_add(path.len())
                            .saturating_add(source.path.len())
                            .saturating_add(roots.iter().fold(0usize, |n, row| {
                                n.saturating_add(row.carrier.len()).saturating_add(1)
                            }))
                    });
                budget.spend(work)?;
                let actual = budget.task_stored_sources.as_ref().unwrap()[f.id.0 as usize].values
                    [argument.0 as usize]
                    .retained
                    .iter()
                    .find(|(path, _)| path == &source.path)
                    .map(|(_, roots)| roots.clone())
                    .filter(|roots| !roots.is_empty())
                    .ok_or_else(|| {
                        bad("returned Task capture selected no actual retained input port")
                    })?;
                for source in actual {
                    let root = roots::task_source(&source, budget)?;
                    let row = slots.entry(capture).or_default();
                    budget.spend(row.len().saturating_add(1))?;
                    row.insert(captures::Source::Root(root));
                }
            }
            continue;
        }
        if !binding.position.path.is_empty() {
            return Err(unsupported(
                "nested returned environment slots require actual nested slot transport",
                Some(f.span),
            ));
        }
        let capture_type = &module.closures[claim.closure.0 as usize].captures[capture as usize];
        let mut roots = Roots::new();
        for atom in ordinary_sources {
            let source = crate::exact_storage_source(atom)?;
            budget.spend(source.path.len().saturating_add(1))?;
            let StorageBase::Parameter(parameter) = source.base else {
                return Err(bad("returned environment source is not an input"));
            };
            let argument = *arguments
                .get(parameter as usize)
                .ok_or_else(|| bad("returned environment actual argument is absent"))?;
            let actual = &ordinary[argument.0 as usize];
            if let [ContractProjection::Capture(selected)] = source.path.as_slice() {
                // A Capture selector names an actual environment value slot,
                // never the bytes of the callable argument or all its roots.
                let (id, _) = helpers::parameter_type(&f.values[argument.0 as usize])
                    .ok_or_else(|| bad("capture source actual is not a concrete environment"))?;
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("capture source environment is absent"))?;
                let actual_type = definition
                    .captures
                    .get(*selected as usize)
                    .ok_or_else(|| bad("capture source actual slot is absent"))?;
                let (formal_type, _) =
                    crate::retained::sources::source_type(module, callee, source, budget)?;
                for ty in [actual_type, formal_type, capture_type] {
                    crate::stored_flow::type_work(ty, &budget.limits.clone(), budget)?;
                }
                if actual_type != formal_type {
                    return Err(bad("capture source changes the complete actual leaf type"));
                }
                let compatible = actual_type == capture_type
                    || matches!((actual_type,capture_type),
                    (Type::Address {mutable:from,inner:a},Type::Address {mutable:to,inner:b})
                    if (!*to||*from) && a==b);
                if !compatible {
                    return Err(bad(
                        "capture source leaf differs from the returned borrowed capture",
                    ));
                }
                let values = captured
                    .ok_or_else(|| bad("capture source lacks actual CFG slot reconstruction"))?;
                let actual = values
                    .get(argument.0 as usize)
                    .ok_or_else(|| bad("capture source actual value is absent"))?;
                lookup(budget, actual.len())?;
                if let Some(selected) = actual.get(selected) {
                    budget.spend(selected.len())?;
                    let row = slots.entry(capture).or_default();
                    budget.spend(
                        row.len().saturating_add(selected.len()).saturating_mul(
                            row.len()
                                .saturating_add(selected.len())
                                .checked_ilog2()
                                .map_or(1, |n| n as usize + 1),
                        ),
                    )?;
                    // Preserve symbolic InputCapture positions and instance
                    // identity through forwarding. Empty is fixed-point bottom;
                    // independent completed-source/access checks must ground it.
                    row.extend(selected.iter().copied());
                }
                continue;
            }
            if source.path.is_empty() {
                roots_join(&mut roots, actual, budget)?;
                continue;
            }
            let projections = source
                .path
                .iter()
                .map(|p| match p {
                    ContractProjection::Place(p) => Ok(*p),
                    _ => Err(unsupported(
                        "returned environment source needs actual nested/retained slot transport",
                        Some(f.span),
                    )),
                })
                .collect::<Check<Vec<_>>>()?;
            let (endpoint, _) =
                crate::retained::sources::source_type(module, callee, source, budget)?;
            // A stored pointer's target is not its physical pointer slot.
            if endpoint == capture_type {
                return Err(unsupported(
                    "returned stored-borrow leaves require independently reconstructed target transport",
                    Some(f.span),
                ));
            }
            let Type::Address { .. } = f.values[argument.0 as usize] else {
                return Err(unsupported(
                    "returned field source is not an actual address projection",
                    Some(f.span),
                ));
            };
            let mut normalized = Roots::new();
            for root in actual {
                let root = match *root {
                    Root::Field(local, index) => roots::mixed_root(
                        f,
                        local,
                        &[PlaceProjection::Field(index)],
                        false,
                        budget,
                    )?,
                    other => other,
                };
                roots_join(&mut normalized, &[root], budget)?;
            }
            let selected =
                roots::mixed_address_roots(f, argument, &projections, false, &normalized, budget)?;
            roots_join(&mut roots, &selected, budget)?;
        }
        budget.spend(roots.len())?;
        slots
            .entry(capture)
            .or_default()
            .extend(roots.into_iter().map(captures::Source::Root));
    }
    Ok(slots)
}
pub(super) fn roots(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    ordinary: &[Roots],
    captured: Option<&[captures::Slots]>,
    budget: &mut Budget,
) -> Check<Roots> {
    let mut out = Roots::new();
    for sources in instantiate(module, f, function, arguments, ordinary, captured, budget)?.values()
    {
        for source in sources {
            roots_join(&mut out, &[source.position()], budget)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod static_capture_tests{
    use super::*;
    #[test]
    fn returned_static_atom_requires_the_exact_completed_occurrence_not_a_declaration(){
        let(mut module,plan)=crate::tests::baseline();
        let leaf=Type::Address{mutable:false,inner:Box::new(Type::Slice{element:Box::new(Type::U8)})};
        let position=StoragePosition{base:StorageBase::Capture(0),path:vec![]};
        let header=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:0,bindings:vec![],classes:vec![],static_bindings:vec![position.clone()]};
        module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=Some(header.clone());
        module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![leaf.clone()],parameters:vec![],result:Type::I32,kind:CallableKind::Fn,storage_life:Some(header),lending_captures:vec![]});
        let atom=StorageSource::Static{leaf,mode:StoredBorrowMode::SharedAddress};let binding=StorageSourceBinding{position:position.clone(),sources:vec![atom.clone()]};
        for wire in [false,true]{
            let limits=Limits::default();let decoded;
            let module=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};
            let mut b=Budget::new(&limits);
            assert!(static_source(module,ClosureId(0),&binding,&atom,&mut b).is_err());
            assert!(b.proven_static_occurrences.is_empty(),"a pending declaration cannot prove its own data source");
            // Simulate only the private post-solver boundary for this leaf;
            // the full-source tests exercise the actual least-root publisher.
            b.proven_static_occurrences.insert((ClosureId(0),position.clone()));
            assert!(static_source(module,ClosureId(0),&binding,&atom,&mut b).unwrap());
            let wrong=StorageSource::Static{leaf:Type::Address{mutable:false,inner:Box::new(Type::I32)},mode:StoredBorrowMode::SharedAddress};
            assert!(static_source(module,ClosureId(0),&binding,&wrong,&mut b).is_err());
            let mut wrong=binding.clone();wrong.position.base=StorageBase::Capture(1);
            assert!(static_source(module,ClosureId(0),&wrong,&atom,&mut b).is_err());
            let wrong=StorageSource::Static{leaf:Type::Address{mutable:true,inner:Box::new(Type::I32)},mode:StoredBorrowMode::ExclusiveAddress};
            assert!(static_source(module,ClosureId(0),&binding,&wrong,&mut b).is_err());
        }
    }
}
