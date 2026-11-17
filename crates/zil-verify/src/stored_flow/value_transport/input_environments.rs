//! Concrete closure inputs inside finite inline containers. A field prefix
//! locates the actual input occupant; it never changes the child's full class
//! domain, invents a literal, or turns a pointee into an inline field.
use super::*;

fn prefix_source(source: &mut Source, prefix: &[PlaceProjection], b: &mut Budget) -> Check<()> {
    if source.base == HoldRoot::Static {
        return Ok(());
    }
    if source.retained_port.is_some() {
        return Err(err(
            "physical environment input mixes a retained-port namespace",
        ));
    }
    b.spend(prefix.len().saturating_add(source.leaf.len()))?;
    let mut leaf = prefix.to_vec();
    leaf.extend(&source.leaf);
    source.leaf = leaf;
    Ok(())
}
fn input_loan(
    source: &Source,
    mode: StoredBorrowMode,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Permission> {
    source_work(std::slice::from_ref(source), b)?;
    b.spend(3)?;
    let id = loans.len();
    let exclusive = mode == StoredBorrowMode::ExclusiveAddress;
    loans.push(Loan {
        sources: vec![source.clone()],
        parents: vec![],
        exclusive,
        receiver_head: false,
    });
    b.stored_exclusive_loans |= exclusive;
    Ok(Permission {
        sources: vec![source.clone()],
        loans: vec![id],
    })
}
fn same_input(
    permission: &Permission,
    source: &Source,
    mode: StoredBorrowMode,
    loans: &[Loan],
    b: &mut Budget,
) -> Check<()> {
    permission_work(permission, b)?;
    source_work(std::slice::from_ref(source), b)?;
    if permission.sources.as_slice() != std::slice::from_ref(source) || permission.loans.is_empty()
    {
        return Err(err(
            "nested environment input port differs from its actual complete input source",
        ));
    }
    b.spend(permission.loans.len())?;
    for id in &permission.loans {
        b.spend(1)?;
        let loan = loans
            .get(*id)
            .ok_or_else(|| err("nested environment input port loan is absent"))?;
        source_work(&loan.sources, b)?;
        if loan.sources.as_slice() != std::slice::from_ref(source)
            || !loan.parents.is_empty()
            || loan.exclusive != (mode == StoredBorrowMode::ExclusiveAddress)
        {
            return Err(err(
                "nested environment input port changes its actual loan role",
            ));
        }
    }
    Ok(())
}
fn logical_inputs(
    module: &TypedModule,
    id: ClosureId,
    prefix: &[PlaceProjection],
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    b.spend(prefix.len())?;
    let prefix = prefix
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect::<Vec<_>>();
    let ty = Type::Closure(id);
    if b.task_sources() {
        for (path, mode) in task_address::input_ports(module, &ty, b)? {
            let Some((ContractProjection::RetainedPort(port), carrier)) = path.split_last() else {
                return Err(err("nested Task input lacks its actual terminal port"));
            };
            b.spend(prefix.len().saturating_add(carrier.len()).saturating_add(2))?;
            let mut full = prefix.clone();
            full.extend(carrier);
            let source = Source {
                base,
                leaf: vec![],
                inline: vec![],
                retained_port: Some(PortOrigin::Task {
                    carrier: full.clone(),
                    port: *port,
                }),
                heap_parents: vec![],
                heap_element: None,
                static_dependency: StaticProvenance::NotStatic,
            };
            full.push(ContractProjection::RetainedPort(*port));
            b.spend(
                out.task_ports
                    .len()
                    .saturating_mul(full.len().saturating_add(1)),
            )?;
            if let Some((_, permission)) = out.task_ports.iter().find(|(path, _)| path == &full) {
                same_input(permission, &source, mode, loans, b)?;
            } else {
                let permission = input_loan(&source, mode, loans, b)?;
                task_address::insert(out, &full, &permission, b)?;
            }
        }
    }
    if b.dyn_contexts {
        for (path, mode) in dyn_ports::shape(module, &ty, b)? {
            let Some((ContractProjection::RetainedPort(port), carrier)) = path.split_last() else {
                return Err(err("nested Dyn input lacks its actual terminal port"));
            };
            b.spend(prefix.len().saturating_add(carrier.len()).saturating_add(2))?;
            let mut full = prefix.clone();
            full.extend(carrier);
            let source = Source {
                base,
                leaf: vec![],
                inline: vec![],
                retained_port: Some(PortOrigin::Dyn {
                    carrier: full.clone(),
                    port: *port,
                }),
                heap_parents: vec![],
                heap_element: None,
                static_dependency: StaticProvenance::NotStatic,
            };
            full.push(ContractProjection::RetainedPort(*port));
            if let Some(index) = dyn_ports::find(&out.dyn_ports, &full, b)? {
                let row = &out.dyn_ports.0[index].1;
                if !row.known || !row.parent_sources.is_empty() || !row.head_sources.is_empty() {
                    return Err(err(
                        "nested Dyn input is not a complete initialized input row",
                    ));
                }
                same_input(&row.permission, &source, mode, loans, b)?;
            } else {
                let permission = input_loan(&source, mode, loans, b)?;
                dyn_ports::insert(
                    &mut out.dyn_ports,
                    &full,
                    &dyn_ports::Port {
                        permission,
                        known: true,
                        ..Default::default()
                    },
                    b,
                )?;
            }
        }
    }
    Ok(())
}
pub(in crate::stored_flow) fn seed_environments(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !matches!(base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)) {
        return Err(err("nested environment seed has no actual input premise"));
    }
    let mut pending = vec![(ty, Path::new(), 0usize, true)];
    while let Some((ty, path, depth, root)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("nested environment input directory depth exceeded"));
        }
        match ty {
            // These values expose no inline pointee. Root beta input memory
            // is independently seeded by the memory kernel at its own type.
            Type::Address { .. } | Type::Desc(_) => {}
            Type::Closure(_) if root => {}
            Type::Closure(id) => {
                crate::dynamic_witness::intrinsic_domain(module, ty, b)?;
                let directory = returned_environment::leaves(module, *id, b)?;
                let mut child = Payload::default();
                returned_environment::seed(module, *id, base, &mut child, loans, b)?;
                for (local, mut permission) in child.leaves.into_vec(b)? {
                    for source in &mut permission.sources {
                        prefix_source(source, &path, b)?;
                    }
                    for loan in &permission.loans {
                        source_work(&permission.sources, b)?;
                        let loan = loans.get_mut(*loan).ok_or_else(|| {
                            err("nested environment input physical loan is absent")
                        })?;
                        loan.sources = permission.sources.clone();
                    }
                    b.spend(path.len().saturating_add(local.len()))?;
                    let mut full = path.clone();
                    full.extend(&local);
                    add_leaf(out, &full, &permission, b)?;
                    b.spend(
                        directory
                            .len()
                            .saturating_mul(local.len().saturating_add(1)),
                    )?;
                    let leaf_type=directory.iter().find(|(key,_)|key==&local).map(|(_,ty)|*ty).ok_or_else(||err("nested environment input leaf changes its complete actual directory"))?;
                    let mut facts = FieldFacts::default();
                    if matches!(leaf_type, Type::Desc(_)) {
                        permission_work(&permission, b)?;
                        facts.description = permission.clone();
                    }
                    if matches!(leaf_type,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Dyn{..}))
                    {
                        permission_work(&permission, b)?;
                        b.spend(1)?;
                        facts.views.push(dyn_ports::View {
                            table: None,
                            head: permission.clone(),
                        });
                    }
                    put(out, &full, &facts, b)?;
                }
                logical_inputs(module, *id, &path, base, out, loans, b)?;
            }
            _ => {
                let mut fields = vec![];
                match ty {
                    Type::BorrowedStorage { storage, .. } => {
                        b.spend(path.len().saturating_add(1))?;
                        pending.push((storage, path, depth + 1, false));
                        continue;
                    }
                    Type::Tuple(children) => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Record {
                        fields: children, ..
                    }
                    | Type::Package {
                        fields: children, ..
                    } => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, (_, t))| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Enum { variants, .. } => {
                        b.spend(variants.len())?;
                        for (v, (_, children)) in variants.iter().enumerate() {
                            b.spend(children.len())?;
                            fields.extend(children.iter().enumerate().map(|(i, t)| {
                                (
                                    PlaceProjection::Variant(VariantProjection {
                                        variant: v as u32,
                                        index: i as u32,
                                    }),
                                    t,
                                )
                            }));
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        b.spend(2)?;
                        fields.extend([
                            (PlaceProjection::ErrorArm(true), error.as_ref()),
                            (PlaceProjection::ErrorArm(false), result.as_ref()),
                        ]);
                    }
                    Type::Array { element, length } => {
                        b.spend(*length as usize)?;
                        fields.extend(
                            (0..*length).map(|i| (PlaceProjection::Field(i), element.as_ref())),
                        );
                    }
                    _ => {}
                }
                b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (step, child) in fields {
                    let mut full = path.clone();
                    full.push(step);
                    pending.push((child, full, depth + 1, false));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn module() -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        let mut entry = module.functions[0].clone();
        let pointer = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let header = StorageLifeContract {
            fixed_binder_count: 1,
            invocation_binder_count: 0,
            bindings: vec![StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Capture(0),
                    path: vec![],
                },
                binder: StorageBinder::Fixed(0),
            }],
            classes: vec![],
            static_bindings: vec![],
        };
        entry.id = FunctionId(0);
        entry.environment = Some(ClosureId(0));
        entry.foreign = None;
        entry.call_kind = CallKind::Direct;
        entry.result_mode = PassingMode::Infer;
        entry.parameters.clear();
        entry.locals.clear();
        entry.values.clear();
        entry.return_type = Type::Unit;
        entry.storage_life = Some(header.clone());
        entry.blocks.truncate(1);
        entry.blocks[0].instructions.clear();
        entry.blocks[0].terminator = Terminator::Return(None);
        entry.entry = entry.blocks[0].id;
        module.functions = vec![entry];
        module.closures = vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![pointer],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::FnOnce,
            storage_life: Some(header),
            lending_captures: vec![],
        }];
        module
    }
    // Input-memory components only: module/creator/source/cleanup acceptance
    // remains with their independent complete-program checkers.
    #[test]
    fn array_error_product_closure_inputs_preserve_actual_prefixed_heads_and_loan_sources_raw_wire()
    {
        let module = module();
        let limits = Limits::default();
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0;
        for module in [&module, &decoded] {
            let ty = Type::Array {
                element: Box::new(Type::ErrorUnion {
                    error: Box::new(Type::I32),
                    result: Box::new(Type::Closure(ClosureId(0))),
                }),
                length: 2,
            };
            let mut b = Budget::new(&limits);
            let mut out = Payload::default();
            let mut loans = vec![];
            seed_environments(
                module,
                &ty,
                HoldRoot::Parameter(0),
                &mut out,
                &mut loans,
                &mut b,
            )
            .unwrap();
            assert_eq!(out.leaves.len(), 2);
            for index in 0..2 {
                let path = vec![
                    PlaceProjection::Field(index),
                    PlaceProjection::ErrorArm(false),
                    PlaceProjection::Field(0),
                ];
                let actual = leaf(&out, &path, &mut b).unwrap().unwrap();
                assert_eq!(actual.sources.len(), 1);
                assert_eq!(actual.sources[0].base, HoldRoot::Parameter(0));
                assert_eq!(actual.sources[0].leaf, path);
                assert_eq!(
                    actual.sources[0].static_dependency,
                    StaticProvenance::NotStatic
                );
                for id in &actual.loans {
                    assert_eq!(loans[*id].sources, actual.sources);
                }
            }
            let pointer = Type::Address {
                mutable: false,
                inner: Box::new(ty.clone()),
            };
            let mut skipped = Payload::default();
            seed_environments(
                module,
                &pointer,
                HoldRoot::Parameter(0),
                &mut skipped,
                &mut loans,
                &mut b,
            )
            .unwrap();
            assert!(
                skipped.leaves.is_empty(),
                "a pointee is seeded only by its separate real beta input"
            );
            let mut unknown = module.clone();
            unknown.closures[0].storage_life = None;
            assert!(
                seed_environments(
                    &unknown,
                    &ty,
                    HoldRoot::Parameter(0),
                    &mut Payload::default(),
                    &mut vec![],
                    &mut b
                )
                .is_err(),
                "a missing complete closure relation is not Known0"
            );
        }
    }
    #[test]
    fn private_pointee_sources_prefix_without_changing_static_or_retained_namespaces() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let prefix = vec![PlaceProjection::Field(2)];
        let mut source = Source {
            base: HoldRoot::Capture(1),
            leaf: vec![PlaceProjection::Field(0), PlaceProjection::Field(3)],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        };
        prefix_source(&mut source, &prefix, &mut b).unwrap();
        assert_eq!(
            source.leaf,
            vec![
                PlaceProjection::Field(2),
                PlaceProjection::Field(0),
                PlaceProjection::Field(3)
            ]
        );
        let mut conditional = Source {
            base: HoldRoot::Static,
            leaf: vec![],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::Conditional((
                ClosureId(0),
                StoragePosition {
                    base: StorageBase::Capture(0),
                    path: vec![],
                },
            )),
        };
        let original = conditional.clone();
        prefix_source(&mut conditional, &prefix, &mut b).unwrap();
        assert_eq!(
            conditional, original,
            "input Static is still the same conditional obligation, never a new literal"
        );
        source.retained_port = Some(PortOrigin::Task {
            carrier: vec![],
            port: 0,
        });
        assert!(
            prefix_source(&mut source, &prefix, &mut b).is_err(),
            "logical carriers are never encoded as physical leaf prefixes"
        );
    }
}
