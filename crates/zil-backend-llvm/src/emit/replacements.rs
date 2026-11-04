//! Reservation IDs have no machine storage. Commit uses the original SSA
//! base and physical path whose activation/lifetime Verify has reconstructed.
use super::*;

pub(super) fn reserve(
    f: &TypedFunction,
    resources: &[ResourceDefinition],
    limits: &zil_base::Limits,
    meter: &tasks::Meter,
) -> CompileResult<()> {
    if !f.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
        matches!(
            i.kind,
            InstructionKind::ReservePlaceLocal { .. } | InstructionKind::ReservePlaceAddress { .. }
        )
    }) {
        return Ok(());
    }
    let mut limits = limits.clone();
    limits.max_analysis_steps = meter.remaining();
    let mut budget = drop_plan::Budget::new(&limits);
    // Bound the static map and all uses before cloning paths/types or emitting.
    let sites = f.blocks.iter().map(|b| b.instructions.len()).sum::<usize>();
    budget.pay(0, sites.saturating_mul(32))?;
    for i in f.blocks.iter().flat_map(|b| &b.instructions) {
        let (root, path) = match &i.kind {
            InstructionKind::ReservePlaceLocal { local, path, .. } => {
                (&f.locals[local.0 as usize].ty, path)
            }
            InstructionKind::ReservePlaceAddress { address, path, .. } => {
                let Type::Address { inner, .. } = &f.values[address.0 as usize] else {
                    unreachable!("verified reserve address")
                };
                (inner.as_ref(), path)
            }
            _ => continue,
        };
        for _ in 0..=path.len() {
            budget.tree(root, 0)?;
        }
        // Each static recipe can serve several mutually exclusive settlements.
        let before = budget.spent();
        for r in resources {
            budget.tree(&r.payload, 0)?;
            budget.pay(0, r.name.len())?;
        }
        let metadata = budget.spent() - before;
        budget.pay(
            path.len(),
            sites.saturating_mul(
                path.len()
                    .saturating_add(1)
                    .saturating_mul(128)
                    .saturating_add(metadata),
            ),
        )?;
    }
    meter.spend(budget.spent())
}

impl FunctionEmitter<'_> {
    pub(super) fn commit_replacement(
        &mut self,
        reservation: zil_ir::ReservationId,
        input: ValueId,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        let Some(definition) = self.replacements.get(&reservation).copied() else {
            return invalid(
                "E-CODEGEN-CFG",
                "replacement has no verified reserve definition",
                Some(instruction.span),
            );
        };
        let (root, pointer, path) = match definition {
            InstructionKind::ReservePlaceLocal { local, path, .. } => (
                self.local_type(*local)?.clone(),
                format!("%local.{}", local.0),
                path,
            ),
            InstructionKind::ReservePlaceAddress { address, path, .. } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    unreachable!("verified address")
                };
                (inner.as_ref().clone(), value(*address), path)
            }
            _ => unreachable!("static reservation recipe"),
        };
        let (ty, pointer) = self.actual_place(&root, &pointer, path)?;
        // This is the currently verified producer profile. Other owning trees
        // retain their explicit independent Hold/debt consumer boundary.
        if !matches!(&ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
        {
            return unsupported(
                "replacement machine cleanup requires an admitted complete managed leaf",
                Some(instruction.span),
            );
        }
        for (path, teardown) in packages::teardown_paths(&ty, self.resources)? {
            debug_assert!(path.is_empty());
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(teardown),
                    args: vec![m::Arg::new(m::Ty::Ptr, m::val(&pointer))],
                    vararg: None,
                },
            )));
        }
        // RHS was fully prepared before Commit. No recoverable operation is
        // emitted between retiring the old leaf and publishing the new one.
        // Parent flags and lexical shell cleanup age remain unchanged.
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(llvm_type(&ty)?, m::val(value(input))),
                ptr: m::val(pointer),
                align: alignment(&ty, self.closures)?,
            },
        )));
        Ok(())
    }
}
