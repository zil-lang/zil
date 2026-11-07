//! Physical shared dyn ABI. Admission comes from the exactly bound capability.
use super::*;
use zil_ir::{DynInterfaceId, DynVtableId, TypedModule};

pub(super) fn address(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if matches!(inner.as_ref(), Type::Dyn { .. }))
}

pub(super) fn pair() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Ptr])
}

fn table_symbol(id: DynVtableId) -> String {
    format!("zil.dyn.{}.{}", id.module.0, id.index)
}

/// Pay only the new table/argument/event work. No table exists on the old path.
pub(super) fn reserve(module: &TypedModule, work: &mut usize) -> CompileResult<()> {
    if module.dyn_interfaces.is_empty() && module.dyn_vtables.is_empty() {
        return Ok(());
    }
    let mut cost = module
        .dyn_interfaces
        .len()
        .saturating_add(module.dyn_vtables.len());
    for interface in &module.dyn_interfaces {
        cost = cost.saturating_add(interface.slots.len().saturating_mul(128));
        if interface.retained_life.binder_count != 0
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
            || !interface.retained_life.nominal_arguments.is_empty()
        {
            return unsupported("shared dyn ABI has no retained environment ports", None);
        }
        for slot in &interface.slots {
            cost = cost.saturating_add(slot.parameters.len().saturating_mul(64));
            if slot.receiver.mutable
                || !slot.result.is_b0_scalar()
                || !slot.parameters.iter().all(Type::is_b0_scalar)
            {
                return unsupported(
                    "shared dyn ABI requires a shared receiver and scalar slots",
                    None,
                );
            }
        }
    }
    for table in &module.dyn_vtables {
        cost = cost.saturating_add(table.entries.len().saturating_mul(128));
        if !table.ports.is_empty() || table.cleanup.is_some() {
            return unsupported(
                "borrowed dyn ABI has no owning cleanup or retained ports",
                None,
            );
        }
    }
    for function in &module.functions {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            let extra = match &instruction.kind {
                InstructionKind::MakeDynBorrow { .. } => module.dyn_vtables.len(),
                InstructionKind::CallDynSlot { arguments, .. } => module
                    .dyn_interfaces
                    .len()
                    .saturating_add(arguments.len().saturating_mul(64)),
                _ => continue,
            };
            cost = cost.saturating_add(extra.saturating_add(256));
        }
    }
    *work = work.checked_sub(cost).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "shared dyn table emission budget exceeded",
            None,
        )]
    })?;
    Ok(())
}

pub(super) fn tables(
    module: &TypedModule,
    plans: &large_copy_abi::Plans,
) -> CompileResult<m::Code> {
    let mut code = m::Code::new();
    for table in &module.dyn_vtables {
        let mut entries = Vec::with_capacity(table.entries.len());
        for (ordinal, entry) in table.entries.iter().enumerate() {
            if entry.slot as usize != ordinal {
                return invalid(
                    "E-CODEGEN-DYN",
                    "dyn table entries must retain slot order",
                    None,
                );
            }
            let mut name = symbol(entry.function);
            if plans
                .functions
                .get(&entry.function)
                .is_some_and(|p| p.versioned)
            {
                name.push_str(".lc1");
            }
            entries.push(m::Arg::new(m::Ty::Ptr, m::global(name)));
        }
        code.push(m::Event::Global(m::Global {
            name: table_symbol(table.id),
            linkage: m::Linkage::Private,
            ty: m::Ty::array(entries.len() as u64, m::Ty::Ptr),
            initializer: Some(m::Val::Array(entries)),
            constant: true,
            unnamed_addr: true,
            alignment: 8,
        }));
    }
    Ok(code)
}

impl FunctionEmitter<'_> {
    pub(super) fn make_dyn_borrow(
        &mut self,
        instruction: &Instruction,
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: ValueId,
        mutable: bool,
    ) -> CompileResult<()> {
        if mutable
            || !self
                .dyn_module
                .dyn_vtables
                .iter()
                .any(|t| t.id == vtable && t.interface == interface)
        {
            return invalid(
                "E-CODEGEN-DYN",
                "shared dyn construction lacks its verified table",
                Some(instruction.span),
            );
        }
        let temporary = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(&temporary)),
            m::Op::Insert {
                arg: m::Arg::new(pair(), m::Val::Poison),
                field: m::Arg::new(m::Ty::Ptr, m::local(format!("v.{}", source.0))),
                index: 0,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Insert {
                arg: m::Arg::new(pair(), m::local(m::name(&temporary))),
                field: m::Arg::new(m::Ty::Ptr, m::global(table_symbol(vtable))),
                index: 1,
            },
        )));
        Ok(())
    }

    pub(super) fn call_dyn_slot(
        &mut self,
        instruction: &Instruction,
        callee: ValueId,
        interface: DynInterfaceId,
        slot: u32,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let signature = self
            .dyn_module
            .dyn_interfaces
            .iter()
            .find(|row| row.id == interface)
            .and_then(|row| row.slots.get(slot as usize))
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-DYN",
                    Some(instruction.span),
                    "dyn call slot is absent",
                )]
            })?;
        let return_type = llvm_type(&signature.result)?;
        let environment = self.temp();
        let table = self.temp();
        for (name, index) in [(&environment, 0), (&table, 1)] {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name(name)),
                m::Op::Extract {
                    arg: m::Arg::new(pair(), m::local(format!("v.{}", callee.0))),
                    index,
                },
            )));
        }
        let slot_address = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(&slot_address)),
            m::Op::Gep {
                ty: m::Ty::Ptr,
                ptr: m::local(m::name(&table)),
                indices: vec![m::Arg::new(m::Ty::Int(64), u64::from(slot))],
            },
        )));
        let target = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(&target)),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local(m::name(&slot_address)),
                align: 8,
            },
        )));
        let mut args = Vec::with_capacity(arguments.len().saturating_add(1));
        args.push(m::Arg::new(m::Ty::Ptr, m::local(m::name(&environment))));
        for argument in arguments {
            let ty = self.value_type(*argument)?;
            if *ty != Type::Unit {
                args.push(m::Arg::new(
                    llvm_type(ty)?,
                    m::local(format!("v.{}", argument.0)),
                ));
            }
        }
        self.event(m::Event::Instruction(m::Inst::new(
            if return_type == m::Ty::Void {
                None
            } else {
                Some(format!("v.{}", result(instruction)?.0))
            },
            m::Op::Call {
                result: return_type,
                result_attrs: vec![],
                callee: m::local(m::name(&target)),
                args,
                vararg: None,
            },
        )));
        Ok(())
    }
}
