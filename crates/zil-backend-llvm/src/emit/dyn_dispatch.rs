//! Physical borrowed dyn ABI. Admission comes from the exactly bound capability.
use super::*;
use zil_ir::{DynInterfaceId, DynOwnedStorage, DynVtableId, TypedModule};

pub(super) fn address(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. }))
}

pub(super) fn pair() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Ptr])
}

fn table_symbol(id: DynVtableId) -> String {
    format!("zil.dyn.{}.{}", id.module.0, id.index)
}

fn payload_drop_symbol(id: DynVtableId) -> String {
    format!("zil.dyn.payload.drop.{}.{}", id.module.0, id.index)
}

pub(super) fn owner_drop_symbol(id: DynInterfaceId) -> String {
    format!("zil.dyn.owner.drop.{}.{}", id.module.0, id.index)
}

pub(super) fn create_error() -> Type {
    Type::Enum {
        name: "@std.dyn:v1:DynCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}

fn inst(name: Option<String>, op: m::Op) -> m::Event {
    m::Event::Instruction(m::Inst::new(name, op))
}

fn call(name: Option<String>, result: m::Ty, callee: m::Val, args: Vec<m::Arg>) -> m::Event {
    inst(
        name,
        m::Op::Call {
            result,
            result_attrs: vec![],
            callee,
            args,
            vararg: None,
        },
    )
}

pub(super) fn runtime(lir: &LirView<'_>) -> CompileResult<m::Code> {
    let owning = lir.functions.iter().any(|f| {
        f.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.kind,
                InstructionKind::MakeDynOwned { .. } | InstructionKind::DropDynOwned { .. }
            )
        })
    });
    if !owning {
        return Ok(m::Code::new());
    }
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return unsupported("owned dyn Heap requires Linux x86-64 allocator ABI", None);
    }
    let mutex = lir.functions.iter().any(|f| {
        f.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.kind,
                InstructionKind::MutexCreate { .. }
                    | InstructionKind::MutexLock { .. }
                    | InstructionKind::MutexValue { .. }
                    | InstructionKind::MutexUnlock { .. }
                    | InstructionKind::DropMutexGuard { .. }
            )
        })
    });
    let byte_free = lir
        .resources
        .iter()
        .any(|r| r.payload == Type::ByteBuffer || matches!(r.payload, Type::OwnedStore { .. }));
    let mut code = m::Code::new();
    for (name, output, input, already) in [
        ("malloc", m::Ty::Ptr, m::Ty::Int(64), mutex),
        ("free", m::Ty::Void, m::Ty::Ptr, mutex || byte_free),
    ] {
        if let Some(existing) = lir
            .functions
            .iter()
            .find(|f| f.foreign.as_ref().is_some_and(|x| x.symbol == name))
        {
            let actual = existing
                .parameters
                .iter()
                .map(|id| llvm_type(&existing.locals[id.0 as usize].ty))
                .collect::<CompileResult<Vec<_>>>()?;
            if actual != vec![input.clone()] || llvm_type(&existing.return_type)? != output {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{name} conflicts with owned dyn Heap ABI"),
                    Some(existing.span),
                );
            }
        } else if !already {
            code.push(m::Event::Declare(m::Function {
                name: name.into(),
                linkage: m::Linkage::External,
                signature: m::Signature::new(output, vec![m::Param::unnamed(input)]),
                attrs: vec![],
                blocks: None,
            }));
        }
    }
    Ok(code)
}

fn closed_owner_shape(
    ty: &Type,
    module: &TypedModule,
    limits: &zil_base::Limits,
    depth: usize,
) -> CompileResult<Option<(usize, usize)>> {
    if depth > limits.max_call_depth {
        return unsupported("owned dyn provider depth exceeds native limit", None);
    }
    if ty.is_b0_scalar() || *ty == Type::Unit {
        return Ok(Some((0, 1)));
    }
    let mut owners = 0usize;
    let mut cost = 1usize;
    match ty {
        Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer => {
            return Ok(module
                .resources
                .iter()
                .any(|r| {
                    r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
                        && r.payload == Type::ByteBuffer
                        && r.teardown.is_none()
                })
                .then_some((1, 32)));
        }
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                let Some((n, work)) = closed_owner_shape(field, module, limits, depth + 1)? else {
                    return Ok(None);
                };
                if n != 0 {
                    return Ok(None);
                }
                cost = cost.saturating_add(work);
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                let Some((n, work)) = closed_owner_shape(field, module, limits, depth + 1)? else {
                    return Ok(None);
                };
                if n != 0 {
                    return Ok(None);
                }
                cost = cost.saturating_add(work);
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    let Some((n, work)) = closed_owner_shape(field, module, limits, depth + 1)?
                    else {
                        return Ok(None);
                    };
                    if n != 0 {
                        return Ok(None);
                    }
                    cost = cost.saturating_add(work);
                }
            }
        }
        Type::Array { element, length }
            if *length > 0 && (*length as usize) <= limits.max_nodes =>
        {
            let Some((n, work)) = closed_owner_shape(element, module, limits, depth + 1)? else {
                return Ok(None);
            };
            if n != 0 {
                return Ok(None);
            }
            cost = cost.saturating_add(work).saturating_add(*length as usize);
        }
        Type::Package { fields, .. } => {
            for (_, field) in fields {
                let Some((n, work)) = closed_owner_shape(field, module, limits, depth + 1)? else {
                    return Ok(None);
                };
                owners = owners.saturating_add(n);
                cost = cost.saturating_add(work);
            }
            if owners == 0 {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    Ok(Some((owners, cost)))
}

/// Pay only the new table/argument/event work. No table exists on the old path.
pub(super) fn reserve(
    module: &TypedModule,
    limits: &zil_base::Limits,
    work: &mut usize,
) -> CompileResult<()> {
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
            if !(slot.result.is_b0_scalar() || slot.result == Type::Unit)
                || !slot.parameters.iter().all(Type::is_b0_scalar)
            {
                return unsupported("borrowed dyn ABI requires fixed scalar or Unit slots", None);
            }
        }
    }
    for table in &module.dyn_vtables {
        cost = cost.saturating_add(table.entries.len().saturating_mul(128));
        if table.owned_storage == Some(DynOwnedStorage::Heap) {
            let shape = match &table.self_type {
                Type::Package { .. } => closed_owner_shape(&table.self_type, module, limits, 0)?,
                Type::Record { fields, .. } if fields.iter().all(|(_, ty)| ty.is_b0_scalar()) => {
                    Some((0, fields.len()))
                }
                _ => None,
            };
            let Some((owners, shape_cost)) = shape else {
                return unsupported("owned dyn glue requires closed builtin cleanup", None);
            };
            cost = cost
                .saturating_add(shape_cost.saturating_mul(16))
                .saturating_add(owners.saturating_mul(256))
                .saturating_add(512);
        }
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
                InstructionKind::MakeDynOwned { .. } => {
                    module.dyn_vtables.len().saturating_add(1024)
                }
                InstructionKind::BorrowDynOwned { .. } | InstructionKind::DropDynOwned { .. } => {
                    128
                }
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
    resources: &[ResourceDefinition],
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
        if table.owned_storage == Some(DynOwnedStorage::Heap) {
            let glue = payload_drop_symbol(table.id);
            entries.push(m::Arg::new(m::Ty::Ptr, m::global(glue.clone())));
            code.push(m::Event::Begin(m::Function {
                name: glue,
                linkage: m::Linkage::Internal,
                signature: m::Signature::new(
                    m::Ty::Void,
                    vec![m::Param::new(m::Ty::Ptr, "payload")],
                ),
                attrs: vec![],
                blocks: None,
            }));
            code.push(m::Event::Label("entry".into()));
            for (ordinal, (path, drop)) in packages::teardown_paths(&table.self_type, resources)?
                .iter()
                .enumerate()
            {
                let pointer = if path.is_empty() {
                    m::local("payload".into())
                } else {
                    let name = format!("field.{ordinal}");
                    let mut indices = vec![m::Arg::new(m::Ty::Int(32), 0)];
                    indices.extend(path.iter().map(|index| m::Arg::new(m::Ty::Int(32), *index)));
                    code.push(inst(
                        Some(name.clone()),
                        m::Op::Gep {
                            ty: llvm_type(&table.self_type)?,
                            ptr: m::local("payload".into()),
                            indices,
                        },
                    ));
                    m::local(name)
                };
                code.push(call(
                    None,
                    m::Ty::Void,
                    m::global(drop.clone()),
                    vec![m::Arg::new(m::Ty::Ptr, pointer)],
                ));
            }
            code.push(inst(None, m::Op::Return(None)));
            code.push(m::Event::End);
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
    for interface in &module.dyn_interfaces {
        if !module
            .dyn_vtables
            .iter()
            .any(|t| t.interface == interface.id && t.owned_storage == Some(DynOwnedStorage::Heap))
        {
            continue;
        }
        code.push(m::Event::Begin(m::Function {
            name: owner_drop_symbol(interface.id),
            linkage: m::Linkage::Internal,
            signature: m::Signature::new(m::Ty::Void, vec![m::Param::new(m::Ty::Ptr, "owner")]),
            attrs: vec![],
            blocks: None,
        }));
        code.push(m::Event::Label("entry".into()));
        code.push(inst(
            Some("pair".into()),
            m::Op::Load {
                ty: pair(),
                ptr: m::local("owner".into()),
                align: 8,
            },
        ));
        for (name, index) in [("payload", 0), ("table", 1)] {
            code.push(inst(
                Some(name.into()),
                m::Op::Extract {
                    arg: m::Arg::new(pair(), m::local("pair".into())),
                    index,
                },
            ));
        }
        code.push(inst(
            Some("glue.slot".into()),
            m::Op::Gep {
                ty: m::Ty::Ptr,
                ptr: m::local("table".into()),
                indices: vec![m::Arg::new(m::Ty::Int(64), interface.slots.len() as u64)],
            },
        ));
        code.push(inst(
            Some("glue".into()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local("glue.slot".into()),
                align: 8,
            },
        ));
        code.push(call(
            None,
            m::Ty::Void,
            m::local("glue".into()),
            vec![m::Arg::new(m::Ty::Ptr, m::local("payload".into()))],
        ));
        code.push(call(
            None,
            m::Ty::Void,
            m::global("free".into()),
            vec![m::Arg::new(m::Ty::Ptr, m::local("payload".into()))],
        ));
        code.push(inst(None, m::Op::Return(None)));
        code.push(m::Event::End);
    }
    Ok(code)
}

impl FunctionEmitter<'_> {
    pub(super) fn make_dyn_owned(
        &mut self,
        instruction: &Instruction,
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: ValueId,
        route: DynOwnedStorage,
    ) -> CompileResult<()> {
        if route != DynOwnedStorage::Heap {
            return unsupported(
                "owned dyn has no checked storage route",
                Some(instruction.span),
            );
        }
        let actual = self.value_type(source)?.clone();
        if !self.dyn_module.dyn_vtables.iter().any(|t| {
            t.id == vtable
                && t.interface == interface
                && t.self_type == actual
                && t.owned_storage == Some(route)
        }) {
            return invalid(
                "E-CODEGEN-DYN",
                "owned dyn construction lacks exact verified provider",
                Some(instruction.span),
            );
        }
        let result = result(instruction)?;
        let union = llvm_type(self.value_type(result)?)?;
        let concrete = llvm_type(&actual)?;
        let label = self.continuation;
        self.continuation += 1;
        let name = |part: &str| format!("dyn.owner.{label}.{part}");
        self.event(inst(
            Some(name("size")),
            m::Op::Cast {
                opcode: "ptrtoint".into(),
                arg: m::Arg::new(
                    m::Ty::Ptr,
                    m::Val::Gep {
                        ty: concrete.clone(),
                        ptr: Box::new(m::Val::Null),
                        indices: vec![m::Arg::new(m::Ty::Int(64), 1)],
                    },
                ),
                to: m::Ty::Int(64),
            },
        ));
        self.event(call(
            Some(name("payload")),
            m::Ty::Ptr,
            m::global("malloc".into()),
            vec![m::Arg::new(m::Ty::Int(64), m::local(name("size")))],
        ));
        self.event(inst(
            Some(name("failed")),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: m::local(name("payload")),
                right: m::Val::Null,
            },
        ));
        self.event(inst(
            None,
            m::Op::CondBranch {
                condition: m::local(name("failed")),
                yes: name("failure"),
                no: name("ready"),
            },
        ));
        self.event(m::Event::Label(name("ready")));
        self.event(inst(
            None,
            m::Op::Store {
                arg: m::Arg::new(concrete.clone(), m::val(value(source))),
                ptr: m::local(name("payload")),
                align: alignment(&actual, self.closures)?,
            },
        ));
        self.event(inst(
            Some(name("pair.payload")),
            m::Op::Insert {
                arg: m::Arg::new(pair(), m::Val::Poison),
                field: m::Arg::new(m::Ty::Ptr, m::local(name("payload"))),
                index: 0,
            },
        ));
        self.event(inst(
            Some(name("pair")),
            m::Op::Insert {
                arg: m::Arg::new(pair(), m::local(name("pair.payload"))),
                field: m::Arg::new(m::Ty::Ptr, m::global(table_symbol(vtable))),
                index: 1,
            },
        ));
        self.event(inst(
            Some(name("success")),
            m::Op::Insert {
                arg: m::Arg::new(union.clone(), m::Val::Zero),
                field: m::Arg::new(pair(), m::local(name("pair"))),
                index: 2,
            },
        ));
        self.event(inst(None, m::Op::Branch(name("done"))));
        self.event(m::Event::Label(name("failure")));
        self.event(inst(
            Some(name("source")),
            m::Op::Alloca {
                ty: concrete.clone(),
                align: alignment(&actual, self.closures)?,
            },
        ));
        self.event(inst(
            None,
            m::Op::Store {
                arg: m::Arg::new(concrete, m::val(value(source))),
                ptr: m::local(name("source")),
                align: alignment(&actual, self.closures)?,
            },
        ));
        self.event(call(
            None,
            m::Ty::Void,
            m::global(payload_drop_symbol(vtable)),
            vec![m::Arg::new(m::Ty::Ptr, m::local(name("source")))],
        ));
        self.event(inst(
            Some(name("error")),
            m::Op::Insert {
                arg: m::Arg::new(union.clone(), m::Val::Zero),
                field: m::Arg::new(m::Ty::Int(1), true),
                index: 0,
            },
        ));
        self.event(inst(None, m::Op::Branch(name("done"))));
        self.event(m::Event::Label(name("done")));
        self.event(inst(
            Some(format!("v.{}", result.0)),
            m::Op::Phi {
                ty: union,
                incoming: vec![
                    (m::local(name("success")), name("ready")),
                    (m::local(name("error")), name("failure")),
                ],
            },
        ));
        Ok(())
    }

    pub(super) fn borrow_dyn_owned(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        _mutable: bool,
    ) -> CompileResult<()> {
        let Type::OwnedDyn { .. } = self.local_type(local)? else {
            return invalid(
                "E-CODEGEN-DYN",
                "owned dyn borrow requires owner local",
                Some(instruction.span),
            );
        };
        self.event(inst(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Load {
                ty: pair(),
                ptr: m::local(format!("local.{}", local.0)),
                align: 8,
            },
        ));
        Ok(())
    }

    pub(super) fn drop_dyn_owned(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let Type::OwnedDyn { interface } = self.local_type(local)?.clone() else {
            return invalid(
                "E-CODEGEN-DYN",
                "owned dyn drop requires owner local",
                Some(instruction.span),
            );
        };
        let label = self.continuation;
        self.continuation += 1;
        let live = self.temp();
        self.event(inst(
            Some(m::name(&live)),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        ));
        self.event(inst(
            None,
            m::Op::CondBranch {
                condition: m::val(live),
                yes: format!("dyn.drop.{label}"),
                no: format!("dyn.done.{label}"),
            },
        ));
        self.event(m::Event::Label(format!("dyn.drop.{label}")));
        self.event(inst(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        ));
        self.event(call(
            None,
            m::Ty::Void,
            m::global(owner_drop_symbol(interface)),
            vec![m::Arg::new(
                m::Ty::Ptr,
                m::local(format!("local.{}", local.0)),
            )],
        ));
        self.event(inst(None, m::Op::Branch(format!("dyn.done.{label}"))));
        self.event(m::Event::Label(format!("dyn.done.{label}")));
        Ok(())
    }

    pub(super) fn make_dyn_borrow(
        &mut self,
        instruction: &Instruction,
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: ValueId,
        _mutable: bool,
    ) -> CompileResult<()> {
        if !self
            .dyn_module
            .dyn_vtables
            .iter()
            .any(|t| t.id == vtable && t.interface == interface)
        {
            return invalid(
                "E-CODEGEN-DYN",
                "borrowed dyn construction lacks its verified table",
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
