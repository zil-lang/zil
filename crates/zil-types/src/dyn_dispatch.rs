//! Finite shared dyn admission. Core tables and slot claims are untrusted.
use super::*;

fn receiver_position() -> StoragePosition {
    StoragePosition {
        base: StorageBase::Parameter(0),
        path: Vec::new(),
    }
}

fn scalar_contract() -> DynContract {
    let receiver = receiver_position();
    DynContract {
        requires: Some(vec![DynRequirement::Read(receiver.clone())]),
        guarantees: Some(Vec::new()),
        hold: Some(Vec::new()),
        call: Some(DynCallAccess {
            reads: vec![receiver],
            writes: Vec::new(),
        }),
        result: Some(DynResultSource::Value),
        cleanup: Some(DynCleanupContract {
            environment: false,
            positions: Vec::new(),
            access: DynCallAccess {
                reads: Vec::new(),
                writes: Vec::new(),
            },
            effects: Vec::new(),
        }),
        effects: Some(Vec::new()),
        resume: Some(DynResumeBound::Local),
        threads: Some(DynThreadRequirements {
            transfer: false,
            share: false,
        }),
    }
}

pub(super) fn inventory(
    core: &CoreModule,
    signatures: &BTreeMap<String, Signature>,
    limits: &Limits,
) -> R<Vec<DynVtable>> {
    if core.dyn_declarations.is_empty()
        && core.dyn_interfaces.is_empty()
        && core.dyn_vtables.is_empty()
    {
        return Ok(Vec::new());
    }
    let mut left = limits.max_analysis_steps;
    let spend = |left: &mut usize, cost: usize| {
        *left = left.checked_sub(cost).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "dyn inventory work budget exceeded", None)
        })?;
        Ok::<(), Diagnostic>(())
    };
    let mut declarations = std::collections::BTreeSet::new();
    for declaration in &core.dyn_declarations {
        spend(
            &mut left,
            1 + declaration.members.len() + declaration.parameters.len(),
        )?;
        if !declarations.insert(declaration.id) {
            return Err(error(
                "E-DYN-DECLARATION",
                Span::default(),
                "duplicate dyn declaration identity",
            ));
        }
    }
    let mut interfaces = std::collections::BTreeSet::new();
    for interface in &core.dyn_interfaces {
        spend(&mut left, 1 + interface.slots.len())?;
        if !interfaces.insert(interface.id)
            || !declarations.contains(&interface.application.declaration)
            || !interface.application.actuals.is_empty()
            || !interface.inherited.is_empty()
            || interface.slots.is_empty()
            || !interface.associated_types.is_empty()
            || !interface.associated_constants.is_empty()
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
        {
            return Err(unsupported(
                Span::default(),
                "dyn interface is outside closed shared profile",
            ));
        }
        for slot in &interface.slots {
            spend(&mut left, 1 + slot.parameters.len())?;
            if slot.contract != scalar_contract()
                || slot.receiver.mutable
                || slot.receiver.mode != PassingMode::Borrow
                || !slot.actuals.is_empty()
                || slot.parameters.iter().any(|ty| !ty.is_b0_scalar())
                || !slot.result.is_b0_scalar()
                || slot
                    .parameter_modes
                    .iter()
                    .any(|mode| *mode != PassingMode::Infer)
                || slot.call_kind != CallKind::Direct
                || slot.storage_life.is_none()
            {
                return Err(unsupported(
                    Span::default(),
                    "dyn slot has unknown or unproven closed contract",
                ));
            }
        }
    }
    let mut output = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for table in &core.dyn_vtables {
        spend(&mut left, 1 + table.entries.len())?;
        let interface = core
            .dyn_interfaces
            .iter()
            .find(|interface| interface.id == table.interface)
            .ok_or_else(|| {
                error(
                    "E-DYN-INTERFACE",
                    Span::default(),
                    "dyn table has no actual interface",
                )
            })?;
        if !ids.insert(table.id)
            || table.entries.len() != interface.slots.len()
            || table.cleanup.is_some()
            || !table.ports.is_empty()
        {
            return Err(unsupported(
                Span::default(),
                "dyn table has invalid slot or cleanup inventory",
            ));
        }
        let mut entries = Vec::new();
        let mut canonical_self = None;
        for (ordinal, entry) in table.entries.iter().enumerate() {
            spend(&mut left, 1)?;
            if entry.slot as usize != ordinal || entry.module != core.module_id {
                return Err(unsupported(
                    Span::default(),
                    "dyn entry needs exact local target identity",
                ));
            }
            let function = core
                .functions
                .iter()
                .find(|function| function.id == entry.function)
                .ok_or_else(|| {
                    error(
                        "E-DYN-ENTRY",
                        Span::default(),
                        "dyn entry target body absent",
                    )
                })?;
            let typed_id = signatures
                .get(&function.name)
                .ok_or_else(|| error("E-DYN-ENTRY", function.span, "dyn entry signature absent"))?
                .concrete_id(function.span)?;
            let Some(Type::Address {
                mutable: false,
                inner,
            }) = function.parameters.first().map(|p| &p.ty)
            else {
                return Err(unsupported(
                    function.span,
                    "dyn provider receiver must be shared concrete Address",
                ));
            };
            let Type::Record { name, fields } = inner.as_ref() else {
                return Err(unsupported(
                    function.span,
                    "dyn provider Self must resolve to concrete Copy struct",
                ));
            };
            let Type::Named(raw_self) = &table.self_type else {
                return Err(unsupported(
                    function.span,
                    "dyn raw Self needs nominal declaration",
                ));
            };
            if *name != format!("@module{}:{}", table.id.module.0, raw_self)
                || fields.iter().any(|(_, ty)| !ty.is_b0_scalar())
                || canonical_self
                    .as_ref()
                    .is_some_and(|previous| previous != inner.as_ref())
            {
                return Err(error(
                    "E-DYN-SELF",
                    function.span,
                    "dyn provider Self differs from actual table declaration",
                ));
            }
            canonical_self = Some(inner.as_ref().clone());
            let slot = &interface.slots[ordinal];
            if function.foreign.is_some()
                || !function.type_parameters.is_empty()
                || !function.state_parameters.is_empty()
                || !function.callable_parameters.is_empty()
                || function.return_type != slot.result
                || function.parameters.len() != slot.parameters.len() + 1
                || function.parameters[0].mode != slot.receiver.mode
                || function
                    .parameters
                    .iter()
                    .skip(1)
                    .zip(&slot.parameters)
                    .any(|(p, ty)| p.ty != *ty)
                || !scalar_body(function, fields, &slot.result)
            {
                return Err(unsupported(
                    function.span,
                    "dyn provider body or signature is outside proven scalar profile",
                ));
            }
            entries.push(DynEntry {
                slot: entry.slot,
                function: typed_id,
            });
        }
        output.push(DynVtable {
            id: table.id,
            interface: table.interface,
            self_type: canonical_self
                .ok_or_else(|| unsupported(Span::default(), "dyn table is empty"))?,
            implementation: table.implementation,
            actuals: table.actuals.clone(),
            ports: Vec::new(),
            entries,
            cleanup: None,
        });
    }
    Ok(output)
}

pub(super) fn inventory_program(
    program: &CoreProgram,
    signatures: &BTreeMap<zil_base::ModuleId, BTreeMap<String, Signature>>,
    limits: &Limits,
) -> R<(Vec<DynDeclaration>, Vec<DynInterface>, Vec<DynVtable>)> {
    let mut declarations = Vec::new();
    let mut interfaces = Vec::new();
    let mut left = limits.max_analysis_steps;
    for module in &program.modules {
        let core = &module.core;
        left = left
            .checked_sub(
                core.dyn_declarations
                    .len()
                    .saturating_add(core.dyn_interfaces.len()),
            )
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "dyn project inventory budget exceeded", None)
            })?;
        declarations.extend(core.dyn_declarations.iter().cloned());
        interfaces.extend(core.dyn_interfaces.iter().cloned());
    }
    if declarations.is_empty()
        && interfaces.is_empty()
        && program
            .modules
            .iter()
            .all(|module| module.core.dyn_vtables.is_empty())
    {
        return Ok((declarations, interfaces, Vec::new()));
    }
    let mut tables = Vec::new();
    for module in &program.modules {
        if module.core.dyn_vtables.is_empty() {
            continue;
        }
        // A provider table belongs to its implementation module, while its
        // exact interface declaration may be in a different module.
        left = left
            .checked_sub(
                module
                    .core
                    .functions
                    .len()
                    .saturating_add(interfaces.len())
                    .saturating_add(declarations.len()),
            )
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "dyn project snapshot budget exceeded", None)
            })?;
        let mut local = module.core.clone();
        local.dyn_declarations = declarations.clone();
        local.dyn_interfaces = interfaces.clone();
        tables.extend(inventory(
            &local,
            &signatures[&module.core.module_id],
            limits,
        )?);
    }
    if interfaces
        .iter()
        .any(|interface| !tables.iter().any(|table| table.interface == interface.id))
    {
        return Err(unsupported(
            Span::default(),
            "dyn interface has no actual provider table",
        ));
    }
    Ok((declarations, interfaces, tables))
}

fn scalar_body(function: &CoreFunction, fields: &[(String, Type)], result: &Type) -> bool {
    let [statement] = function.body.statements.as_slice() else {
        return false;
    };
    let CoreStatementKind::Return(Some(value)) = &statement.kind else {
        return false;
    };
    match &value.kind {
        CoreExprKind::Member {
            object,
            member,
            owner: None,
        } => {
            matches!(&object.kind, CoreExprKind::Name(name) if name == &function.parameters[0].name)
                && fields
                    .iter()
                    .any(|(name, ty)| name == member && ty == result)
        }
        CoreExprKind::Integer(_)
        | CoreExprKind::Float(_)
        | CoreExprKind::Bool(_)
        | CoreExprKind::Literal(_) => true,
        _ => false,
    }
}

impl Builder<'_, '_> {
    pub(super) fn dyn_method_slot(
        &self,
        object: &CoreExpr,
        member: &str,
    ) -> Option<(DynInterfaceId, u32)> {
        let Type::Address {
            mutable: false,
            inner,
        } = self.known_type(object)?
        else {
            return None;
        };
        let Type::Dyn { interface } = *inner else {
            return None;
        };
        let definition = self
            .dyn_interfaces
            .iter()
            .find(|candidate| candidate.id == interface)?;
        let mut found = None;
        for (index, slot) in definition.slots.iter().enumerate() {
            let declaration = self
                .dyn_declarations
                .iter()
                .find(|declaration| declaration.id == slot.member.application.declaration)?;
            if declaration.members.get(slot.member.ordinal as usize)?.name == member {
                if found.is_some() {
                    return None;
                }
                found = Some((interface, index as u32));
            }
        }
        found
    }

    pub(super) fn dyn_coerce(
        &mut self,
        source: ValueId,
        actual: &Type,
        interface: DynInterfaceId,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let table = self
            .dyn_vtables
            .iter()
            .find(|table| table.interface == interface && &table.self_type == actual)
            .ok_or_else(|| {
                error(
                    "E-DYN-PROVIDER",
                    span,
                    "concrete shared receiver has no exact dyn provider",
                )
            })?;
        let ty = Type::Address {
            mutable: false,
            inner: Box::new(Type::Dyn { interface }),
        };
        let value = self.value(
            InstructionKind::MakeDynBorrow {
                interface,
                vtable: table.id,
                source,
                mutable: false,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(super) fn dyn_make_borrow(
        &mut self,
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: &CoreExpr,
        mutable: bool,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if mutable {
            return Err(unsupported(
                span,
                "mutable dyn borrow is outside shared profile",
            ));
        }
        let (source, ty) = self.expr(source, None)?;
        let Type::Address {
            mutable: false,
            inner,
        } = ty
        else {
            return Err(error(
                "E-DYN-RECEIVER",
                span,
                "dyn source must be a real shared concrete address",
            ));
        };
        let table = self
            .dyn_vtables
            .iter()
            .find(|table| {
                table.id == vtable && table.interface == interface && table.self_type == *inner
            })
            .ok_or_else(|| {
                error(
                    "E-DYN-PROVIDER",
                    span,
                    "dyn source does not match exact selected table",
                )
            })?;
        let ty = Type::Address {
            mutable: false,
            inner: Box::new(Type::Dyn { interface }),
        };
        let value = self.value(
            InstructionKind::MakeDynBorrow {
                interface,
                vtable: table.id,
                source,
                mutable: false,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(super) fn dyn_call(
        &mut self,
        callee: &CoreExpr,
        interface: DynInterfaceId,
        ordinal: u32,
        arguments: &[CoreExpr],
        span: Span,
    ) -> R<(ValueId, Type)> {
        let slot = self
            .dyn_interfaces
            .iter()
            .find(|candidate| candidate.id == interface)
            .and_then(|interface| interface.slots.get(ordinal as usize))
            .ok_or_else(|| {
                error(
                    "E-DYN-SLOT",
                    span,
                    "dyn call slot absent from actual interface",
                )
            })?;
        self.closures
            .snapshot(slot.parameters.len().saturating_add(1), span)?;
        let parameters = slot.parameters.clone();
        let result = slot.result.clone();
        if arguments.len() != parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "dyn slot argument count mismatch",
            ));
        }
        let (receiver, actual) = self.expr(callee, None)?;
        let expected = Type::Address {
            mutable: false,
            inner: Box::new(Type::Dyn { interface }),
        };
        self.same(&actual, &expected, callee.span)?;
        let mut values = Vec::new();
        for (argument, parameter) in arguments.iter().zip(&parameters) {
            let (value, actual) = self.argument_expr(argument, parameter)?;
            self.same(&actual, parameter, argument.span)?;
            values.push(self.stage_argument(value, &actual, argument.span)?);
        }
        let values = self.commit_arguments(values, span)?;
        let instruction = InstructionKind::CallDynSlot {
            callee: receiver,
            interface,
            slot: ordinal,
            arguments: values,
        };
        if result == Type::Unit {
            self.emit(instruction, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            Ok((self.value(instruction, result.clone(), span)?, result))
        }
    }
}
