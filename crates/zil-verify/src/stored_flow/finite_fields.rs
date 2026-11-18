//! Borrow actual fields of a finite callable-bearing Package. Qualification
//! proves the complete value directory, never Copy, initialization, SyncDrop,
//! a target pointer, or permission to discard an owning sibling.
use super::*;
fn has_callable(ty: &Type, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![(ty, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("finite field input depth exceeded"));
        }
        match ty {
            Type::Closure(_) => return Ok(true),
            Type::BorrowedStorage { storage, .. } => pending.push((storage, depth + 1)),
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Array { element, .. } => pending.push((element, depth + 1)),
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([(error.as_ref(), depth + 1), (result.as_ref(), depth + 1)]);
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
                }
            }
            // These allocations/pointees are not inline Package fields.
            _ => {}
        }
    }
    Ok(false)
}
enum FieldPath<'a> {
    Direct([PlaceProjection; 1]),
    Nested(&'a [PlaceProjection]),
}
impl FieldPath<'_> {
    fn steps(&self) -> &[PlaceProjection] {
        match self {
            Self::Direct(step) => step,
            Self::Nested(path) => path,
        }
    }
}
fn selected<'a>(
    module: &TypedModule,
    f: &'a TypedFunction,
    i: &'a Instruction,
    b: &mut Budget,
) -> Check<Option<(LocalId, FieldPath<'a>, bool, &'a Type, &'a Type)>> {
    let (local, path, mutable) = match &i.kind {
        InstructionKind::BorrowPackageField { local, index, mutable } => {
            (*local, FieldPath::Direct([PlaceProjection::Field(*index)]), *mutable)
        }
        InstructionKind::BorrowPlaceLocal { local, path, mutable } if path.len() > 1 => {
            if path.len() > b.limits.max_call_depth {
                return Err(limit("finite field borrow path exceeds depth"));
            }
            (*local, FieldPath::Nested(path), *mutable)
        }
        _ => return Ok(None),
    };
    let declaration = f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("finite field borrow local is absent"))?;
    let parent = &declaration.ty;
    let storage = qualified(parent).map_or(parent, |(storage, _)| storage);
    if !matches!(storage, Type::Package { .. }) || !has_callable(parent, b)? {
        return Ok(None);
    }
    let mut child = storage;
    for step in path.steps() {
        b.spend(1)?;
        let (Type::Package { fields, .. }, PlaceProjection::Field(index)) = (child, step) else {
            return Ok(None);
        };
        child = &fields.get(*index as usize)
            .ok_or_else(|| err("finite field borrow selects no actual field"))?.1;
    }
    if !matches!(child, Type::Closure(_))
        && !(matches!(path, FieldPath::Direct(_)) && child.is_copy_value())
    {
        return Ok(None);
    }
    if mutable && !declaration.mutable {
        return Err(err(
            "finite exclusive field borrow requires mutable actual storage",
        ));
    }
    if !crate::stored_callables::complete_value_input(module, parent, b)?
        || !crate::stored_callables::complete_value_input(module, child, b)?
    {
        return Ok(None);
    }
    if let Type::Closure(id) = child {
        if !crate::stored_callables::finite(module, *id) {
            return Err(err(
                "finite field callback lacks its complete actual entry protocol",
            ));
        }
    }
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("finite field borrow has no actual result"))?,
    )?;
    type_work(output, &b.limits.clone(), b)?;
    type_work(child, &b.limits.clone(), b)?;
    if !matches!(output,Type::Address{mutable:actual,inner}if *actual==mutable&&inner.as_ref()==child)
    {
        return Err(err(
            "finite field borrow changes complete child type or access mode",
        ));
    }
    Ok(Some((local, path, mutable, parent, child)))
}
pub(super) fn operation(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    Ok(selected(module, f, i, b)?.is_some())
}
/// The returned indices select this exact child's complete intrinsic domain
/// from its owning parent's domain. They are neither lifetime images nor new
/// class equalities; the class kernel applies each verified correspondence.
pub(super) fn classes(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Vec<u32>> {
    let (_, path, _, parent, child) = selected(module, f, i, b)?
        .ok_or_else(|| err("finite field class query is not an admitted actual borrow"))?;
    let count = crate::dynamic_witness::intrinsic_domain(module, child, b)?.0;
    let life = crate::tasks::frame_shape::declared_life(module, parent, b)?;
    class_indices(&life, path.steps(), count, b)
}
fn class_indices(
    life: &StorageLifeShape,
    path: &[PlaceProjection],
    count: u32,
    b: &mut Budget,
) -> Check<Vec<u32>> {
    b.spend((count as usize).saturating_mul(life.retained.len().saturating_add(2)))?;
    b.spend(path.len().saturating_mul(1 + (count as usize).saturating_mul(life.retained.len())))?;
    let prefix: Vec<_> = path.iter().copied().map(LifeTypeProjection::Place).collect();
    let mut out = Vec::with_capacity(count as usize);
    for class in 0..count {
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == prefix && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| err("finite field omits its complete child class occurrence"))?;
        if rows.next().is_some() || row.binder >= life.binder_count || out.contains(&row.binder) {
            return Err(err(
                "finite field repeats or coalesces its actual child class occurrence",
            ));
        }
        out.push(row.binder);
    }
    Ok(out)
}
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    b: &mut Budget,
) -> Check<Payload> {
    let (local, path, _, _, _) = selected(module, f, i, b)?
        .ok_or_else(|| err("finite field transfer is not an admitted actual borrow"))?;
    if state.locals.get(local.0 as usize).is_none() {
        return Err(err("finite field borrow actual holder is absent"));
    }
    b.spend(2 + path.steps().len())?;
    let permission = Permission {
        sources: vec![Source {
            base: HoldRoot::Local(local),
            leaf: vec![],
            inline: path.steps().to_vec(),
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }],
        loans: vec![],
    };
    require_filled(state, &permission, &[], b)?;
    // The live owner/field and alias authority are checked by the independent
    // resource and borrow kernels. No pointee facts are fabricated here.
    Ok(Payload {
        address: permission,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn module() -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        let mut entry = module.functions[0].clone();
        let ticket = Type::Record {
            name: "@finite_ticket".into(),
            fields: vec![("code".into(), Type::I32)],
        };
        let header = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            static_bindings: vec![],
            classes: vec![],
        };
        entry.id = FunctionId(0);
        entry.name = "callback".into();
        entry.environment = Some(ClosureId(0));
        entry.foreign = None;
        entry.call_kind = CallKind::Direct;
        entry.result_mode = PassingMode::Infer;
        entry.return_type = Type::I32;
        entry.storage_life = Some(header.clone());
        entry.parameters = vec![LocalId(0)];
        entry.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "n".into(),
            ty: Type::I32,
            mutable: false,
            span: entry.span,
        }];
        entry.values = vec![Type::I32];
        entry.blocks.truncate(1);
        entry.blocks[0].instructions = vec![Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Load(LocalId(0)),
            span: entry.span,
        }];
        entry.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        entry.entry = entry.blocks[0].id;
        module.functions = vec![entry];
        module.closures = vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![Type::Managed(Box::new(ticket.clone())), Type::I32],
            parameters: vec![Type::I32],
            result: Type::I32,
            kind: CallableKind::FnMut,
            storage_life: Some(header),
            lending_captures: vec![],
        }];
        module.resources = vec![ResourceDefinition {
            name: "@finite_ticket".into(),
            payload: ticket,
            teardown: None,
        }];
        module
    }
    fn caller(module: &TypedModule) -> TypedFunction {
        let mut f = module.functions[0].clone();
        let ticket = module.resources[0].payload.clone();
        let state = Type::Package {
            name: "@finite_state".into(),
            fields: vec![
                ("guard".into(), Type::Managed(Box::new(ticket))),
                ("value".into(), Type::I32),
                ("callback".into(), Type::Closure(ClosureId(0))),
            ],
        };
        f.id = FunctionId(1);
        f.environment = None;
        f.parameters.clear();
        f.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "state".into(),
            ty: state,
            mutable: true,
            span: f.span,
        }];
        f.values = vec![
            Type::Address {
                mutable: true,
                inner: Box::new(Type::Closure(ClosureId(0))),
            },
            Type::Address {
                mutable: true,
                inner: Box::new(Type::I32),
            },
        ];
        f
    }
    #[test]
    fn finite_state_callback_and_scalar_borrows_keep_real_disjoint_fields_and_owner_boundary_raw_wire()
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
            let f = caller(module);
            let mut b = Budget::new(&limits);
            let mut state = Frame {
                locals: vec![Payload::default()],
                ..Default::default()
            };
            let callback = Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::BorrowPackageField {
                    local: LocalId(0),
                    index: 2,
                    mutable: true,
                },
                span: f.span,
            };
            let scalar = Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::BorrowPackageField {
                    local: LocalId(0),
                    index: 1,
                    mutable: true,
                },
                span: f.span,
            };
            assert!(operation(module, &f, &callback, &mut b).unwrap());
            assert!(
                classes(module, &f, &callback, &mut b).unwrap().is_empty(),
                "the complete actual Closure relation is Known0; this grants no cleanup or Copy fact"
            );
            let a = transfer(module, &f, &callback, &state, &mut b).unwrap();
            let z = transfer(module, &f, &scalar, &state, &mut b).unwrap();
            assert_eq!(a.address.sources[0].inline, vec![PlaceProjection::Field(2)]);
            assert_eq!(z.address.sources[0].inline, vec![PlaceProjection::Field(1)]);
            assert!(
                !overlap(&a.address.sources[0], &z.address.sources[0], &mut b).unwrap(),
                "nested call evaluation can reserve the callback while accessing another real field"
            );
            let mut owner = a.address.sources[0].clone();
            owner.inline.clear();
            assert!(overlap(&owner, &a.address.sources[0], &mut b).unwrap());
            assert!(
                overlap(&owner, &z.address.sources[0], &mut b).unwrap(),
                "retiring the complete owner still overlaps either borrowed field"
            );
            state.vacant.push(a.address.sources[0].clone());
            assert!(
                transfer(module, &f, &callback, &state, &mut b).is_err(),
                "a taken callback slot cannot be borrowed before actual refill"
            );
            let mut immutable = f.clone();
            immutable.locals[0].mutable = false;
            assert!(operation(module, &immutable, &callback, &mut b).is_err());
            let mut retagged = f.clone();
            retagged.values[0] = Type::Address {
                mutable: false,
                inner: Box::new(Type::Closure(ClosureId(0))),
            };
            assert!(operation(module, &retagged, &callback, &mut b).is_err());
            let mut missing = module.clone();
            missing.closures[0].storage_life = None;
            assert!(
                operation(&missing, &f, &callback, &mut b).is_err(),
                "missing child relation remains Unknown"
            );
        }
    }
    #[test]
    fn finite_field_complete_class_projection_keeps_phantom_and_distinct_intrinsic_classes() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let path = vec![LifeTypeProjection::Place(PlaceProjection::Field(2))];
        let life = StorageLifeShape {
            binder_count: 4,
            slots: vec![],
            retained: vec![
                StorageRetainedBinding {
                    path: path.clone(),
                    class: 0,
                    binder: 1,
                },
                StorageRetainedBinding {
                    path,
                    class: 1,
                    binder: 3,
                },
            ],
            nominal_arguments: vec![],
        };
        assert_eq!(class_indices(&life, &[PlaceProjection::Field(2)], 2, &mut b).unwrap(), vec![1, 3]);
        let mut missing = life.clone();
        missing.retained.pop();
        assert!(class_indices(&missing, &[PlaceProjection::Field(2)], 2, &mut b).is_err());
        let mut coalesced = life.clone();
        coalesced.retained[1].binder = 1;
        assert!(class_indices(&coalesced, &[PlaceProjection::Field(2)], 2, &mut b).is_err());
        assert!(class_indices(&life, &[PlaceProjection::Field(1)], 2, &mut b).is_err());
    }
}
