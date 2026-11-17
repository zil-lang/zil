//! The existing one-element callback carrier. No new public storage namespace:
//! real capture leaves retain Field(0), exact source identity and parent loans.
use super::*;

pub(super) fn operation(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let ty = match &i.kind {
        InstructionKind::MakeOwnedArray { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("callback cell constructor lacks result"))?,
        )?,
        InstructionKind::TakeOwnedArrayElement { local, .. }
        | InstructionKind::StoreOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::BorrowOwnedArrayElement { local, .. }
        | InstructionKind::ProjectOwnedArrayField { local, .. } => {
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| err("callback cell local is absent"))?
                .ty
        }
        _ => return Ok(false),
    };
    if !admitted_value(module,ty,b)?{return Ok(false);}
    crate::check_instruction(module, f, i, b)?;
    Ok(true)
}
/// Type integration reuses the same complete callback-cell qualification as
/// actual operations. Initialization, original slot identity and replacement
/// epochs are checked for every operation, never inferred from this result.
pub(super) fn admitted_value(module:&TypedModule,ty:&Type,b:&mut Budget)->Check<bool>{
    let Type::Array { element, length: 1 } = ty else {
        return Ok(false);
    };
    let Type::Closure(id) = element.as_ref() else {
        return Ok(false);
    };
    // This actual carrier owns the complete closure. Zero persistent classes
    // is metadata only: real Static captures still have physical source rows.
    if crate::dynamic_witness::intrinsic_domain(module, element, b)?.0 != 0 {
        return Ok(false);
    }
    returned_environment::leaves(module, *id, b)?;
    crate::resources::arrays::check_internal_single_slot_type_for(module,ty,None,b)?;
    Ok(true)
}
fn drop_field(holder: &mut Payload, index: u32, ty:&Type, b: &mut Budget) -> Check<Payload> {
    pure_copy::physical_tuple_payload(holder, b)?;
    value_transport::take(holder,&[PlaceProjection::Field(index)],ty,b)
}
pub(super) fn transfer(
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    match &i.kind {
        InstructionKind::MakeOwnedArray { elements } => {
            for (index, value) in elements.iter().enumerate() {
                let actual = values
                    .get(value.0 as usize)
                    .ok_or_else(|| err("callback cell input is absent"))?;
                pure_copy::physical_tuple_payload(actual, b)?;
                value_transport::attach(out,&[PlaceProjection::Field(index as u32)],vt(f,*value)?,actual,b)?;
            }
        }
        InstructionKind::TakeOwnedArrayElement { local, index } => {
            *out = drop_field(&mut state.locals[local.0 as usize], *index, value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?, b)?;
        }
        InstructionKind::DropOwnedArrayElement { local, index } => {
            drop_field(&mut state.locals[local.0 as usize], *index, value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?, b)?;
        }
        InstructionKind::StoreOwnedArrayElement {
            local,
            index,
            value,
        } => {
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| err("callback cell replacement is absent"))?;
            pure_copy::physical_tuple_payload(actual, b)?;
            let holder = &mut state.locals[local.0 as usize];
            drop_field(holder, *index, value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?, b)?;
            // Resource checking independently requires the element vacant and
            // consumes exactly this replacement. No live old owner is erased.
            value_transport::attach(holder,&[PlaceProjection::Field(*index)],vt(f,*value)?,actual,b)?;
        }
        InstructionKind::BorrowOwnedArrayElement { local, index, .. } => {
            let holder = &state.locals[local.0 as usize];
            pure_copy::physical_tuple_payload(holder, b)?;
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Local(*local),
                leaf: vec![],
                inline: vec![PlaceProjection::Field(*index)],
                heap_parents: vec![],
                heap_element: None,
                retained_port: None,
                static_dependency: StaticProvenance::NotStatic,
            });
            let mut child=value_transport::project(holder,&[PlaceProjection::Field(*index)],value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?,b)?;
            // This is a pointer to current cell storage, not a copied child.
            child.field_facts.clear();child.address=out.address.clone();*out=child;
        }
        InstructionKind::DropOwnedArray { local } => {
            pure_copy::physical_tuple_payload(&state.locals[local.0 as usize], b)?;
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        // The resource/type kernel rejects a Package-field projection on an
        // opaque Closure element. Do not fabricate a closure-environment GEP.
        InstructionKind::ProjectOwnedArrayField { .. } => {
            return Err(err("callback cell cannot project a public Package field"));
        }
        _ => return Err(err("callback cell transfer selected another opcode")),
    }
    let _ = f;
    Ok(())
}
pub(super) fn local(kind: &InstructionKind) -> Option<(LocalId, Option<u32>, bool)> {
    match kind {
        InstructionKind::BorrowOwnedArrayElement {
            local,
            index,
            mutable,
        } => Some((*local, Some(*index), *mutable)),
        InstructionKind::TakeOwnedArrayElement { local, index }
        | InstructionKind::DropOwnedArrayElement { local, index }
        | InstructionKind::StoreOwnedArrayElement { local, index, .. }
        | InstructionKind::ProjectOwnedArrayField { local, index, .. } => {
            Some((*local, Some(*index), true))
        }
        InstructionKind::DropOwnedArray { local } => Some((*local, None, true)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn module() -> TypedModule {
        let (mut m, _) = crate::tests::baseline();
        let header = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            static_bindings: vec![],
            classes: vec![],
        };
        let task = Type::Managed(Box::new(Type::Task {
            result: Box::new(Type::I32),
            retained_life: RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        }));
        m.closures.push(ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![task],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnMut,
            storage_life: Some(header.clone()),
            lending_captures: vec![],
        });
        let f = &mut m.functions[0];
        f.environment = Some(ClosureId(0));
        f.storage_life = Some(header);
        f.locals[0].ty = Type::Array {
            element: Box::new(Type::Closure(ClosureId(0))),
            length: 1,
        };
        f.locals[0].mutable = true;
        f.values = vec![
            Type::Closure(ClosureId(0)),
            f.locals[0].ty.clone(),
            Type::Closure(ClosureId(0)),
            Type::Address {
                mutable: true,
                inner: Box::new(Type::Closure(ClosureId(0))),
            },
        ];
        m
    }
    #[test]
    fn callback_cell_construct_take_refill_borrow_and_retire_preserve_real_rows_raw_wire() {
        // Actual source facts are supplied to this private transfer component;
        // neither metadata nor this test certifies their origin/phase.
        let m = module();
        let limits = Limits::default();
        let bytes =
            zil_ir::wire::encode_module_candidate(&m, &CandidatePlan::empty_for(&m), &limits)
                .unwrap();
        let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for m in [&m, &wire] {
            let f = &m.functions[0];
            let mut b = Budget::new(&limits);
            assert!(admitted_value(m,&f.locals[0].ty,&mut b).unwrap());
            b.function_count = m.functions.len();
            let op = |result: Option<u32>, kind| Instruction {
                result: result.map(ValueId),
                kind,
                span: f.span,
            };
            let make = op(
                Some(1),
                InstructionKind::MakeOwnedArray {
                    elements: vec![ValueId(0)],
                },
            );
            let take = op(
                Some(2),
                InstructionKind::TakeOwnedArrayElement {
                    local: LocalId(0),
                    index: 0,
                },
            );
            let refill = op(
                None,
                InstructionKind::StoreOwnedArrayElement {
                    local: LocalId(0),
                    index: 0,
                    value: ValueId(0),
                },
            );
            let borrow = op(
                Some(3),
                InstructionKind::BorrowOwnedArrayElement {
                    local: LocalId(0),
                    index: 0,
                    mutable: true,
                },
            );
            let drop = op(None, InstructionKind::DropOwnedArray { local: LocalId(0) });
            for i in [&make, &take, &refill, &borrow, &drop] {
                assert!(operation(m, f, i, &mut b).unwrap());
            }
            let bad = op(
                Some(2),
                InstructionKind::TakeOwnedArrayElement {
                    local: LocalId(0),
                    index: 1,
                },
            );
            assert!(operation(m, f, &bad, &mut b).is_err());
            let p = Permission {
                sources: vec![Source {
                    base: HoldRoot::Static,
                    leaf: vec![],
                    inline: vec![],
                    heap_parents: vec![],
                    heap_element: None,
                    retained_port: None,
                    static_dependency: StaticProvenance::UpperOnly,
                }],
                loans: vec![7, 11],
            };
            let mut input = Payload::default();
            add_leaf(&mut input, &[PlaceProjection::Field(0)], &p, &mut b).unwrap();
            let mut state = Frame {
                locals: vec![Payload::default()],
                ..Frame::default()
            };
            let mut constructed = Payload::default();
            transfer(
                f,
                &make,
                &mut state,
                &[input.clone()],
                &mut constructed,
                &mut b,
            )
            .unwrap();
            state.locals[0] = constructed;
            let mut taken = Payload::default();
            transfer(f, &take, &mut state, &[], &mut taken, &mut b).unwrap();
            assert_eq!(
                leaf(&taken, &[PlaceProjection::Field(0)], &mut b).unwrap(),
                Some(&p)
            );
            assert!(state.locals[0].leaves.is_empty());
            transfer(
                f,
                &refill,
                &mut state,
                &[taken],
                &mut Payload::default(),
                &mut b,
            )
            .unwrap();
            let mut borrowed = Payload::default();
            transfer(f, &borrow, &mut state, &[], &mut borrowed, &mut b).unwrap();
            assert_eq!(
                borrowed.address.sources[0].base,
                HoldRoot::Local(LocalId(0))
            );
            assert_eq!(
                borrowed.address.sources[0].inline,
                vec![PlaceProjection::Field(0)]
            );
            assert_eq!(
                leaf(&borrowed, &[PlaceProjection::Field(0)], &mut b).unwrap(),
                Some(&p),
                "UpperOnly and all old parents stay unchanged"
            );
            transfer(f, &drop, &mut state, &[], &mut Payload::default(), &mut b).unwrap();
            assert!(state.locals[0].leaves.is_empty());
            let mut wrong = m.clone();
            wrong.closures[0].storage_life = None;
            assert!(
                operation(
                    &wrong,
                    &wrong.functions[0],
                    &make,
                    &mut Budget::new(&limits)
                )
                .is_err()
            );
        }
    }
}
