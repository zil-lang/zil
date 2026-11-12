//! Exact identity through finite owning staging, independently of producer facts.
//! This does not prove initialization, ownership, loans or once-consumption.
use super::*;

fn profile(module: &TypedModule, d: &ClosureDefinition, b: &mut Budget) -> Check<()> {
    match legacy_profile(module, d, b) {
        Err(error) if error.category != zil_base::ResultCategory::ResourceLimit => {
            extended_profile(module, d, b)
        }
        result => result,
    }
}
fn legacy_profile(module: &TypedModule, d: &ClosureDefinition, b: &mut Budget) -> Check<()> {
    b.spend(d.captures.len().saturating_add(1))?;
    for ty in &d.captures {
        super::super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
    }
    let [
        Type::Managed(payload),
        Type::Address {
            mutable: false,
            inner,
        },
    ] = d.captures.as_slice()
    else {
        return Err(bad(
            "staged callback requires one finite owner and shared Tag",
        ));
    };
    let Type::BorrowedStorage { storage, life } = inner.as_ref() else {
        return Err(bad(
            "staged callback capture lacks actual shared Tag storage",
        ));
    };
    let Type::Record { fields, .. } = storage.as_ref() else {
        return Err(bad("staged callback capture requires a shared Tag record"));
    };
    if d.kind != CallableKind::FnOnce
        || d.storage_life.is_none()
        || !d.lending_captures.is_empty()
        || !scalar(&d.result)
        || !super::super::stored_owned::payload(payload)
        || life.binder_count != 1
        || !life.retained.is_empty()
        || !matches!(life.slots.as_slice(), [slot]
            if slot.binder == 0 && slot.path.as_slice() == [PlaceProjection::Field(0)]
                && slot.mode == StoredBorrowMode::SharedAddress)
        || !matches!(fields.as_slice(), [(_, Type::Address { mutable: false, inner })]
            if scalar(inner))
    {
        return Err(bad(
            "staged callback is outside the finite owning Fixed profile",
        ));
    }
    b.spend(module.resources.len())?;
    let mut found = false;
    for resource in &module.resources {
        super::super::stored_flow::type_work(payload, &b.limits.clone(), b)?;
        super::super::stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
        if payload.as_ref() == &resource.payload {
            if found || resource.teardown.is_some() {
                return Err(bad(
                    "staged callback owner requires one teardown-free resource",
                ));
            }
            found = true;
        }
    }
    if !found {
        return Err(bad(
            "staged callback owner has no actual resource definition",
        ));
    }
    Ok(())
}

fn extended_profile(module: &TypedModule, d: &ClosureDefinition, b: &mut Budget) -> Check<()> {
    b.spend(d.captures.len().saturating_add(1))?;
    for ty in &d.captures {
        super::super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
    }
    super::captured_profile(module, d, b)?;
    if d.storage_life.is_none()
        || !matches!(
            d.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        )
        || !scalar(&d.result)
        || !d.lending_captures.is_empty()
    {
        return Err(bad(
            "staged callback requires an actual scalar nonlending owning Fixed definition",
        ));
    }
    let mut owners = 0usize;
    for capture in &d.captures {
        let Type::Managed(payload) = capture else {
            continue;
        };
        owners += 1;
        b.spend(module.resources.len())?;
        let mut found = false;
        for resource in &module.resources {
            super::super::stored_flow::type_work(payload, &b.limits.clone(), b)?;
            super::super::stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
            if payload.as_ref() == &resource.payload {
                if found {
                    return Err(bad("staged callback owner definition repeats"));
                }
                checked_teardown(module, resource, b)?;
                found = true;
            }
        }
        if !found {
            return Err(bad(
                "staged callback owner has no actual resource definition",
            ));
        }
    }
    if owners == 0 {
        return Err(bad("staged callback requires an actual owned capture"));
    }
    Ok(())
}
fn checked_teardown(
    module: &TypedModule,
    resource: &ResourceDefinition,
    b: &mut Budget,
) -> Check<()> {
    let Some(id) = resource.teardown else {
        return Ok(());
    };
    b.spend(1)?;
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| bad("staged owner teardown is absent"))?;
    if f.id != id
        || f.foreign.is_some()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.parameters.len() != 1
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.storage_life.is_some()
        || f.reference_return_bound.is_some()
        || f.return_type != Type::Unit
        || f.result_mode != PassingMode::Infer
    {
        return Err(bad(
            "staged owner teardown has no ordinary exact Desc-to-Unit entry",
        ));
    }
    let Some(Type::Desc(actual)) = f
        .parameters
        .first()
        .and_then(|p| f.locals.get(p.0 as usize))
        .map(|l| &l.ty)
    else {
        return Err(bad("staged owner teardown has no actual Desc receiver"));
    };
    super::super::stored_flow::type_work(actual, &b.limits.clone(), b)?;
    super::super::stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
    if **actual != resource.payload {
        return Err(bad("staged owner teardown has a different nominal payload"));
    }
    Ok(())
}
// No local address can be exposed in this first fallback. In particular a
// WriteAddress cannot silently change the whole-local Store correspondence.
pub(super) fn finite_instruction(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::ScopeEnter { .. }
            | InstructionKind::ScopeExit { .. }
            | InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::DropClosure { .. }
            | InstructionKind::Constant(_)
            | InstructionKind::Load(_)
            | InstructionKind::TakeLocal { .. }
            | InstructionKind::Store { .. }
            | InstructionKind::ReborrowAddress { .. }
            | InstructionKind::CallClosureOnce { .. }
            | InstructionKind::CallClosureBorrowed { .. }
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ConvertInteger { .. }
            | InstructionKind::ConvertNumber { .. }
    )
}

pub(super) fn parameter(
    module: &TypedModule,
    f: &TypedFunction,
    call: &InstructionKind,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<usize> {
    profile(module, d, b)?;
    b.spend(f.blocks.len().saturating_add(1))?;
    if f.foreign.is_some() || f.environment.is_some() || f.call_kind != CallKind::Direct {
        return Err(bad(
            "staged callback requires an actual ordinary free helper",
        ));
    }
    let fork = f.blocks.len() != 1;
    let block = if fork {
        match super::staged_tail::entry(f, b) {
            Ok(block) => block,
            Err(error) if error.category != zil_base::ResultCategory::ResourceLimit => {
                return super::staged_cfg::parameter(f, call, d, b);
            }
            Err(error) => return Err(error),
        }
    } else {
        let block = &f.blocks[0];
        if f.entry != block.id
            || !matches!(block.terminator, Terminator::Return(_) | Terminator::Trap)
        {
            return Err(bad("staged callback requires an actual Return/Trap exit"));
        }
        block
    };
    let InstructionKind::CallClosureOnce { callee, .. } = call else {
        return Err(bad("staged callback requires an actual consuming call"));
    };
    b.spend(block.instructions.len())?;
    let mut before = None;
    for (index, instruction) in block.instructions.iter().enumerate() {
        if !finite_instruction(&instruction.kind)
            && !super::super::stored_owned::instruction(f, &instruction.kind)
        {
            return Err(bad(
                "staged callback body exposes storage or unsupported effects",
            ));
        }
        if std::ptr::eq(&instruction.kind, call) {
            before = Some(index);
        }
    }
    let mut before =
        before.ok_or_else(|| bad("staged callback call is not an actual instruction"))?;
    let mut current = *callee;
    // Every iteration decreases the actual instruction position. No recursive
    // walk, identity table, uncharged temporary clone or guessed Store order.
    for depth in 0..=f.values.len() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth {
            return Err(limit("staged callback origin depth exhausted"));
        }
        if fork {
            super::staged_tail::value(f, current, before, b)?;
        }
        if f.values.get(current.0 as usize) != Some(&Type::Closure(d.id)) {
            return Err(bad(
                "staged callback SSA has a different actual environment",
            ));
        }
        let mut definition = None;
        let mut use_count = 0usize;
        b.spend(block.instructions.len())?;
        for (index, instruction) in block.instructions.iter().enumerate() {
            if instruction.result == Some(current)
                && definition.replace((index, &instruction.kind)).is_some()
            {
                return Err(bad("staged callback SSA has duplicate definitions"));
            }
            for operand in instruction_operands(&instruction.kind) {
                b.spend(1)?;
                if operand == current {
                    use_count += 1;
                    if index != before {
                        return Err(bad("staged callback SSA escapes its unique transfer"));
                    }
                }
            }
        }
        b.spend(1)?;
        if use_count != 1
            || matches!(block.terminator, Terminator::Return(Some(value)) if value == current)
        {
            return Err(bad(
                "staged callback SSA lacks one exact consuming transfer",
            ));
        }
        let Some((take_at, InstructionKind::TakeLocal { local })) = definition else {
            return Err(bad(
                "staged callback requires TakeLocal, never a copied Load",
            ));
        };
        if take_at >= before
            || f.locals.get(local.0 as usize).map(|local| &local.ty) != Some(&Type::Closure(d.id))
        {
            return Err(bad(
                "staged callback transfer is cyclic or changes environment",
            ));
        }
        b.spend(f.parameters.len())?;
        let mut parameter = None;
        for (position, actual) in f.parameters.iter().enumerate() {
            if actual == local && parameter.replace(position).is_some() {
                return Err(bad("staged callback parameter identity is duplicated"));
            }
        }
        let mut store = None;
        let mut live = None;
        let mut takes = 0usize;
        b.spend(block.instructions.len())?;
        for (index, instruction) in block.instructions.iter().enumerate() {
            match instruction.kind {
                InstructionKind::Store {
                    local: target,
                    value,
                } if target == *local => {
                    if store.replace((index, value)).is_some() {
                        return Err(bad(
                            "staged callback local has ambiguous or repeated Stores",
                        ));
                    }
                }
                InstructionKind::TakeLocal { local: target } if target == *local => takes += 1,
                InstructionKind::Load(target) if target == *local => {
                    return Err(bad("staged callback owner is also copied by Load"));
                }
                InstructionKind::StorageLive(target) if target == *local => {
                    if live.replace(index).is_some() || index >= take_at {
                        return Err(bad("staged callback local activation is repeated or late"));
                    }
                }
                InstructionKind::StorageDead(target)
                | InstructionKind::DropClosure { local: target }
                    if target == *local && index < take_at =>
                {
                    return Err(bad("staged callback local is retired before its Take"));
                }
                _ => {}
            }
        }
        if takes != 1 {
            return Err(bad(
                "staged callback local has repeated consuming transfers",
            ));
        }
        if let Some(position) = parameter {
            if store.is_some() {
                return Err(bad("staged callback parameter was rebound"));
            }
            return Ok(position);
        }
        let (store_at, source) =
            store.ok_or_else(|| bad("staged callback local has no unique Store"))?;
        if store_at >= take_at || live.is_some_and(|index| index >= store_at) {
            return Err(bad("staged callback Store does not precede its Take"));
        }
        current = source;
        before = store_at;
    }
    Err(bad(
        "staged callback origin did not reach an actual parameter",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Function-local resolver models, not Source/full Verify acceptance. The
    // complete Source/raw/Wire path is exercised in generic_regressions.
    fn model() -> TypedModule {
        let mut m = crate::resource_closure_tests::fixture(CallableKind::FnOnce, false, 1);
        let tag = Type::Address {
            mutable: false,
            inner: Box::new(Type::BorrowedStorage {
                storage: Box::new(Type::Record {
                    name: "Tag".into(),
                    fields: vec![(
                        "p".into(),
                        Type::Address {
                            mutable: false,
                            inner: Box::new(Type::I64),
                        },
                    )],
                }),
                life: StorageLifeShape {
                    binder_count: 1,
                    slots: vec![StorageLifeSlot {
                        path: vec![PlaceProjection::Field(0)],
                        binder: 0,
                        mode: StoredBorrowMode::SharedAddress,
                    }],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            }),
        };
        let relation = StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
        };
        m.closures[0].captures.push(tag.clone());
        m.closures[0].parameters = vec![tag.clone()];
        m.closures[0].storage_life = Some(relation.clone());
        let f = &mut m.functions[0];
        f.parameters = vec![LocalId(0), LocalId(1)];
        f.locals = [
            Type::Closure(ClosureId(0)),
            tag.clone(),
            Type::Closure(ClosureId(0)),
        ]
        .into_iter()
        .enumerate()
        .map(|(n, ty)| LocalDecl {
            id: LocalId(n as u32),
            name: format!("l{n}"),
            ty,
            mutable: false,
            span: Span::default(),
        })
        .collect();
        f.values = vec![
            Type::Closure(ClosureId(0)),
            tag,
            Type::Closure(ClosureId(0)),
            Type::I32,
        ];
        f.callable_contract = Some(CallableParameterContract {
            roles: vec![CallableRole {
                life: Some(relation),
                parameter: 0,
                closure: ClosureId(0),
                kind: CallableKind::FnOnce,
                hold: vec![1],
                call: vec![1],
                writes: vec![],
                cleanup: CallableCleanup::Owned,
            }],
            traces: CallableTraces::Finite(vec![CallableTrace {
                calls: vec![CallEvent {
                    parameter: 0,
                    kind: CallableKind::FnOnce,
                }],
                exit: CallableExit::Return,
            }]),
        });
        f.blocks[0].instructions = vec![
            op(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(2),
                    value: ValueId(0),
                },
            ),
            op(Some(1), InstructionKind::Load(LocalId(1))),
            op(Some(2), InstructionKind::TakeLocal { local: LocalId(2) }),
            op(
                Some(3),
                InstructionKind::CallClosureOnce {
                    callee: ValueId(2),
                    arguments: vec![ValueId(1)],
                },
            ),
        ];
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
        m
    }
    fn op(result: Option<u32>, kind: InstructionKind) -> Instruction {
        Instruction {
            result: result.map(ValueId),
            kind,
            span: Span::default(),
        }
    }
    fn resolve(m: &TypedModule, b: &mut Budget) -> Check<bool> {
        let f = &m.functions[0];
        let call = &f.blocks[0]
            .instructions
            .iter()
            .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
            .unwrap()
            .kind;
        super::super::role(m, f, call, &m.closures[0], b).map(|r| r.is_some())
    }

    #[test]
    fn staged_verify_local_origin_and_hostile_neighbors() {
        let original = model();
        assert!(resolve(&original, &mut Budget::new(&Limits::default())).unwrap());
        for case in 0..13 {
            let mut m = original.clone();
            let f = &mut m.functions[0];
            let instructions = &mut f.blocks[0].instructions;
            match case {
                0 => {
                    let duplicate = instructions[1].clone();
                    instructions.insert(2, duplicate);
                }
                1 => instructions[0].kind = InstructionKind::TakeLocal { local: LocalId(2) },
                2 => instructions[3].kind = InstructionKind::Load(LocalId(2)),
                3 => instructions.insert(
                    2,
                    op(None, InstructionKind::BorrowLocalMut { local: LocalId(2) }),
                ),
                4 => {
                    let mut other = f.locals[0].clone();
                    other.id = LocalId(3);
                    f.locals.push(other);
                    f.parameters.push(LocalId(3));
                    instructions[0].kind = InstructionKind::TakeLocal { local: LocalId(3) };
                }
                5 => f.blocks[0].terminator = Terminator::Jump(BlockId(0)),
                6 => {
                    instructions[1].kind = InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(2),
                    }
                }
                7 => instructions.insert(
                    2,
                    op(
                        None,
                        InstructionKind::WriteAddress {
                            address: ValueId(1),
                            value: ValueId(0),
                        },
                    ),
                ),
                8 => m.resources[0].teardown = Some(FunctionId(1)),
                9 => {
                    f.callable_contract.as_mut().unwrap().roles[0].cleanup =
                        CallableCleanup::Borrowed
                }
                10 => instructions.insert(
                    2,
                    op(None, InstructionKind::DropClosure { local: LocalId(2) }),
                ),
                11 => instructions.insert(2, op(None, InstructionKind::StorageDead(LocalId(2)))),
                12 => {
                    f.values.push(Type::I32);
                    let mut duplicate = instructions[4].clone();
                    duplicate.result = Some(ValueId(4));
                    instructions.push(duplicate);
                }
                _ => unreachable!(),
            }
            assert!(
                resolve(&m, &mut Budget::new(&Limits::default())).is_err(),
                "case {case}"
            );
        }
    }

    #[test]
    fn staged_verify_local_budget_depth_and_old_direct_fee() {
        let m = model();
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        assert!(resolve(&m, &mut budget).unwrap());
        let n = limits.max_analysis_steps - budget.remaining;
        for (work, pass) in [(n, true), (n - 1, false), (n, true)] {
            let tight = Limits {
                max_analysis_steps: work,
                ..limits.clone()
            };
            let result = resolve(&m, &mut Budget::new(&tight));
            if pass {
                assert!(result.unwrap());
            } else {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
        let shallow = Limits {
            max_call_depth: 0,
            ..limits.clone()
        };
        assert_eq!(
            resolve(&m, &mut Budget::new(&shallow))
                .unwrap_err()
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        let mut direct = m;
        let f = &mut direct.functions[0];
        f.values.remove(2);
        f.blocks[0].instructions.remove(3);
        f.blocks[0].instructions.remove(1);
        f.blocks[0].instructions[2] = op(
            Some(2),
            InstructionKind::CallClosureOnce {
                callee: ValueId(0),
                arguments: vec![ValueId(1)],
            },
        );
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        let mut budget = Budget::new(&limits);
        assert!(resolve(&direct, &mut budget).unwrap());
        assert_eq!(
            limits.max_analysis_steps - budget.remaining,
            10,
            "old direct role cost changed"
        );
        eprintln!("Verify local staged origin work N={n}, N-1/depth reject; direct old role=10");
    }
    #[allow(dead_code)]
    mod tail_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/fnonce-helper-next/tail_neighbors.rs"
        ));
    }
    #[test]
    fn verify_tail_origin_checks_whole_graph_and_unique_consumption() {
        let limits = Limits::default();
        let mut m = model();
        tail_fixture::fork(&mut m.functions[0]);
        assert!(resolve(&m, &mut Budget::new(&limits)).unwrap());
        let mut reordered = m.clone();
        reordered.functions[0].blocks.swap(0, 2);
        let f = &reordered.functions[0];
        let call = f
            .blocks
            .iter()
            .flat_map(|bb| &bb.instructions)
            .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
            .unwrap();
        assert!(
            super::super::role(
                &reordered,
                f,
                &call.kind,
                &reordered.closures[0],
                &mut Budget::new(&limits)
            )
            .unwrap()
            .is_some()
        );
        for (name, changed) in tail_fixture::hostile(&m.functions[0]) {
            let mut candidate = m.clone();
            candidate.functions[0] = changed;
            // A moved call is rejected by the actual staged resolver, rather
            // than requiring this test harness to find it in the entry.
            let f = &candidate.functions[0];
            let call = f
                .blocks
                .iter()
                .flat_map(|bb| &bb.instructions)
                .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                .unwrap();
            assert!(
                super::super::role(
                    &candidate,
                    f,
                    &call.kind,
                    &candidate.closures[0],
                    &mut Budget::new(&limits)
                )
                .is_err(),
                "{name}"
            );
        }
    }
    #[allow(dead_code)]
    mod cfg_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/fnonce-helper-cfg/cfg_neighbors.rs"
        ));
    }
    #[test]
    fn verify_actual_origin_dag_joins_and_path_retirement() {
        let limits = Limits::default();
        for drop_branch in [false, true] {
            let mut m = model();
            cfg_fixture::diamond(&mut m.functions[0], drop_branch);
            let f = &m.functions[0];
            let call = &f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                .unwrap()
                .kind;
            assert!(
                super::super::role(&m, f, call, &m.closures[0], &mut Budget::new(&limits))
                    .unwrap()
                    .is_some()
            );
            if drop_branch {
                continue;
            }
            for (name, changed) in cfg_fixture::hostile(f) {
                let call = &changed
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                    .unwrap()
                    .kind;
                assert!(
                    super::super::role(
                        &m,
                        &changed,
                        call,
                        &m.closures[0],
                        &mut Budget::new(&limits)
                    )
                    .is_ok()
                        == (name == "self-loop"),
                    "{name}"
                );
            }
        }
    }
}
