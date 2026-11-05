//! Identity-only recovery for a finite owning receiver. This never establishes
//! initialization, liveness, ownership, or permission to execute the call.
use super::*;
mod cfg;
mod tail;

fn closed() -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "staged owning callback needs one unexposed acyclic parameter transfer",
        None,
    )]
}

pub(super) fn parameter(
    module: &TypedModule,
    f: &TypedFunction,
    kind: &InstructionKind,
    b: &mut Budget,
) -> CompileResult<usize> {
    let InstructionKind::CallClosureOnce { callee, .. } = kind else {
        return Err(bad(
            "callback role receiver is not an actual declared parameter",
        ));
    };
    if f.foreign.is_some() || f.environment.is_some() || f.call_kind != zil_ir::CallKind::Direct {
        return Err(closed());
    }
    let (d, _) = called(module, f, kind).ok_or_else(closed)?;
    if d.storage_life.is_none()
        || !(d.kind == zil_ir::CallableKind::FnOnce
            || matches!(
                d.kind,
                zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
            ) && scalar(&d.result)
                && d.lending_captures.is_empty())
    {
        return Err(closed());
    }
    b.spend(d.captures.len(), "staged actual owning capture inventory")?;
    let mut owner = false;
    for capture in &d.captures {
        let Type::Managed(payload) = capture else {
            continue;
        };
        if !resource_payload(payload, b)? {
            return Err(closed());
        }
        b.spend(module.resources.len(), "staged actual owner definitions")?;
        let mut matched = false;
        for resource in &module.resources {
            b.type_work(payload, true, "staged actual owner payload comparison")?;
            b.type_work(
                &resource.payload,
                true,
                "staged declared owner payload comparison",
            )?;
            if resource.payload == **payload {
                if matched {
                    return Err(closed());
                }
                super::super::stored_flow::owned::teardown(module, resource, b)?;
                matched = true;
            }
        }
        if !matched {
            return Err(closed());
        }
        owner = true;
    }
    if !owner {
        return Err(closed());
    }
    b.spend(f.blocks.len(), "staged single-block control inventory")?;
    let fork = f.blocks.len() != 1;
    let block = if fork {
        match tail::entry(f, b) {
            Ok(block) => block,
            Err(errors)
                if errors
                    .iter()
                    .all(|d| d.category != zil_base::ResultCategory::ResourceLimit) =>
            {
                return cfg::parameter(f, kind, d.id, b);
            }
            Err(errors) => return Err(errors),
        }
    } else {
        let block = &f.blocks[0];
        if block.id != f.entry
            || !matches!(block.terminator, Terminator::Return(_) | Terminator::Trap)
        {
            return Err(closed());
        }
        block
    };
    b.spend(
        block.instructions.len(),
        "staged actual consuming call position",
    )?;
    let mut call_index = None;
    for (index, i) in block.instructions.iter().enumerate() {
        if matches!(i.kind, InstructionKind::CallClosureOnce {callee: actual,..} if actual == *callee)
            && call_index.replace(index).is_some()
        {
            return Err(closed());
        }
    }
    let call_index = call_index.ok_or_else(closed)?;
    b.spend(
        f.values.len().saturating_add(f.locals.len()),
        "staged receiver visited allocation",
    )?;
    let mut values = vec![false; f.values.len()];
    let mut locals = vec![false; f.locals.len()];
    let mut current = *callee;
    let mut upper = call_index;
    let mut depth = 0usize;
    loop {
        if depth >= b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "staged owning receiver depth exceeded",
                None,
            )]);
        }
        depth += 1;
        b.spend(1, "staged receiver chain step")?;
        if fork {
            tail::unique_use(f, current, upper, b)?;
        }
        let seen = values.get_mut(current.0 as usize).ok_or_else(closed)?;
        if *seen {
            return Err(closed());
        }
        *seen = true;
        b.spend(block.instructions.len(), "staged exact SSA definition scan")?;
        let mut definition = None;
        for (index, i) in block.instructions.iter().enumerate() {
            if i.result == Some(current) && definition.replace((index, &i.kind)).is_some() {
                return Err(closed());
            }
        }
        let Some((take_index, InstructionKind::TakeLocal { local })) = definition else {
            return Err(closed());
        };
        if take_index >= upper {
            return Err(closed());
        }
        let seen = locals.get_mut(local.0 as usize).ok_or_else(closed)?;
        if *seen {
            return Err(closed());
        }
        *seen = true;
        let local_type = f.locals.get(local.0 as usize).ok_or_else(closed)?;
        if local_type.ty != Type::Closure(d.id)
            || f.values.get(current.0 as usize) != Some(&local_type.ty)
        {
            return Err(closed());
        }
        b.spend(f.parameters.len(), "staged actual parameter identity")?;
        let parameter = f.parameters.iter().position(|p| p == local);
        b.spend(
            block.instructions.len(),
            "staged complete local writer and exposure scan",
        )?;
        let mut store = None;
        let mut takes = 0usize;
        let mut lives = 0usize;
        for (index, i) in block.instructions.iter().enumerate() {
            match &i.kind {
                InstructionKind::Store {
                    local: written,
                    value,
                } if written == local => {
                    if parameter.is_some() || store.replace((index, *value)).is_some() {
                        return Err(closed());
                    }
                }
                InstructionKind::TakeLocal { local: read } if read == local => {
                    takes += 1;
                }
                InstructionKind::Load(read)
                | InstructionKind::BorrowLocal { local: read }
                | InstructionKind::BorrowLocalMut { local: read }
                | InstructionKind::BorrowPlaceLocal { local: read, .. }
                    if read == local =>
                {
                    return Err(closed());
                }
                InstructionKind::DropClosure { local: read }
                | InstructionKind::StorageDead(read)
                    if read == local && index < take_index =>
                {
                    return Err(closed());
                }
                InstructionKind::StorageLive(read) if read == local => {
                    lives += 1;
                    if lives > 1 || parameter.is_some() || index >= take_index {
                        return Err(closed());
                    }
                    if store.is_some() {
                        return Err(closed());
                    }
                }
                _ => {}
            }
        }
        if takes != 1 {
            return Err(closed());
        }
        if let Some(parameter) = parameter {
            return Ok(parameter);
        }
        let Some((store_index, source)) = store else {
            return Err(closed());
        };
        if store_index >= take_index || f.values.get(source.0 as usize) != Some(&local_type.ty) {
            return Err(closed());
        }
        // The next definition must precede the unique Store, so a backward
        // edge cannot be laundered merely by revisiting an earlier LocalId.
        current = source;
        upper = store_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_ir::*;
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/342/stored_flow_fixture.rs"
        ));
    }
    // Neutral raw identity fixture; it is not a Source or resource-proof oracle.
    fn model(stage: bool) -> (TypedModule, TypedFunction, InstructionKind) {
        let m = fixture::captured_owned_fixed(false);
        let mut h = fixture::captured_fixed_helper(CallableKind::FnOnce, false)
            .functions
            .remove(2);
        let role = &mut h.callable_contract.as_mut().unwrap().roles[0];
        role.life = m.closures[0].storage_life.clone();
        role.hold = vec![1];
        role.call = vec![1];
        let mut call = h.blocks[0].instructions[2].kind.clone();
        if stage {
            let local = LocalId(h.locals.len() as u32);
            let mut decl = h.locals[0].clone();
            decl.id = local;
            decl.name = "compiler_stage".into();
            h.locals.push(decl);
            let value = ValueId(h.values.len() as u32);
            h.values.push(Type::Closure(ClosureId(0)));
            let span = h.span;
            h.blocks[0].instructions.insert(
                1,
                Instruction {
                    result: None,
                    kind: InstructionKind::Store {
                        local,
                        value: ValueId(0),
                    },
                    span,
                },
            );
            h.blocks[0].instructions.insert(
                3,
                Instruction {
                    result: Some(value),
                    kind: InstructionKind::TakeLocal { local },
                    span,
                },
            );
            let InstructionKind::CallClosureOnce { callee, .. } =
                &mut h.blocks[0].instructions[4].kind
            else {
                unreachable!()
            };
            *callee = value;
            call = h.blocks[0].instructions[4].kind.clone();
        }
        (m, h, call)
    }
    #[test]
    fn staged_owning_receiver_uses_actual_parameter_and_rejects_ambiguous_origins() {
        let limits = Limits::default();
        let (m, h, kind) = model(true);
        assert_eq!(
            parameter(&m, &h, &kind, &mut Budget::new(&limits)).unwrap(),
            0
        );
        assert!(
            super::super::role(&m, &h, &kind, &mut Budget::new(&limits))
                .unwrap()
                .is_some()
        );
        for case in 0..9 {
            let mut m = m.clone();
            let mut f = h.clone();
            let stage = LocalId(2);
            match case {
                0 => {
                    let i = f.blocks[0].instructions[1].clone();
                    f.blocks[0].instructions.insert(2, i);
                }
                1 => {
                    f.blocks[0].instructions[0].kind = InstructionKind::TakeLocal { local: stage };
                }
                2 => {
                    f.blocks[0].instructions[3].kind = InstructionKind::Load(stage);
                }
                3 => {
                    let mut i = f.blocks[0].instructions[1].clone();
                    i.kind = InstructionKind::BorrowLocalMut { local: stage };
                    f.blocks[0].instructions.insert(2, i);
                }
                4 => {
                    let id = LocalId(f.locals.len() as u32);
                    let mut decl = f.locals[0].clone();
                    decl.id = id;
                    f.locals.push(decl);
                    f.parameters.push(id);
                    f.blocks[0].instructions[0].kind = InstructionKind::TakeLocal { local: id };
                    // Origin is precisely Parameter2, not the first equal type.
                    assert_eq!(
                        parameter(&m, &f, &kind, &mut Budget::new(&limits)).unwrap(),
                        2
                    );
                    assert!(super::super::role(&m, &f, &kind, &mut Budget::new(&limits)).is_err());
                    continue;
                }
                5 => f.blocks[0].terminator = Terminator::Jump(f.blocks[0].id),
                6 => m.resources[0].teardown = Some(FunctionId(0)),
                7 => {
                    let mut i = f.blocks[0].instructions[1].clone();
                    i.kind = InstructionKind::DropClosure { local: stage };
                    f.blocks[0].instructions.insert(2, i);
                }
                _ => {
                    let mut i = f.blocks[0].instructions[1].clone();
                    i.kind = InstructionKind::StorageDead(stage);
                    f.blocks[0].instructions.insert(2, i);
                }
            }
            assert!(
                parameter(&m, &f, &kind, &mut Budget::new(&limits)).is_err(),
                "case{case}"
            );
        }
    }
    #[test]
    fn staged_owning_receiver_budget_is_paid_and_direct_fee_is_unchanged() {
        let limits = Limits::default();
        let (m, h, kind) = model(true);
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        while low < high {
            let n = low + (high - low) / 2;
            let mut b = Budget::new(&Limits {
                max_analysis_steps: n,
                ..limits.clone()
            });
            if parameter(&m, &h, &kind, &mut b).is_ok() {
                high = n
            } else {
                low = n + 1
            }
        }
        for n in [low, low - 1, low] {
            let result = parameter(
                &m,
                &h,
                &kind,
                &mut Budget::new(&Limits {
                    max_analysis_steps: n,
                    ..limits.clone()
                }),
            );
            if n == low {
                assert_eq!(result.unwrap(), 0)
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .any(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                )
            }
        }
        let (m, h, kind) = model(false);
        let mut b = Budget::new(&Limits {
            max_analysis_steps: 10,
            ..limits.clone()
        });
        assert!(super::super::role(&m, &h, &kind, &mut b).unwrap().is_some());
        assert!(
            super::super::role(
                &m,
                &h,
                &kind,
                &mut Budget::new(&Limits {
                    max_analysis_steps: 9,
                    ..limits.clone()
                })
            )
            .is_err()
        );
        eprintln!(
            "Analyze staged role identity N={low}/N-1/fresh; direct owned role retains exact legacy N10"
        );
    }
    #[allow(dead_code)]
    mod tail_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/342/fnonce-helper-next/tail_neighbors.rs"
        ));
    }
    #[test]
    fn g337_analyze_tail_origin_checks_whole_graph_and_unique_consumption() {
        let limits = Limits::default();
        let (m, mut f, kind) = model(true);
        tail_fixture::fork(&mut f);
        assert!(
            super::super::role(&m, &f, &kind, &mut Budget::new(&limits))
                .unwrap()
                .is_some()
        );
        let mut reordered = f.clone();
        reordered.blocks.swap(0, 2);
        assert!(
            super::super::role(&m, &reordered, &kind, &mut Budget::new(&limits))
                .unwrap()
                .is_some()
        );
        for (name, changed) in tail_fixture::hostile(&f) {
            assert!(
                super::super::role(&m, &changed, &kind, &mut Budget::new(&limits)).is_err(),
                "{name}"
            );
        }
    }
    #[allow(dead_code)]
    mod cfg_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/342/fnonce-helper-cfg/cfg_neighbors.rs"
        ));
    }
    #[test]
    fn g338_analyze_actual_origin_dag_joins_and_path_retirement() {
        let limits = Limits::default();
        for drop_branch in [false, true] {
            let (m, mut f, _) = model(true);
            cfg_fixture::diamond(&mut f, drop_branch);
            let call = &f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                .unwrap()
                .kind;
            assert!(
                super::super::role(&m, &f, call, &mut Budget::new(&limits))
                    .unwrap()
                    .is_some()
            );
            if drop_branch {
                continue;
            }
            for (name, changed) in cfg_fixture::hostile(&f) {
                let call = &changed
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                    .unwrap()
                    .kind;
                assert!(
                    super::super::role(&m, &changed, call, &mut Budget::new(&limits)).is_ok()
                        == (name == "self-loop"),
                    "{name}"
                );
            }
        }
    }
}
