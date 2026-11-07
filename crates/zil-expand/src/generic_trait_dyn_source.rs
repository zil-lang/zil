//! Actual source declaration headers and concrete interface rows. These are
//! untrusted Core inputs; downstream kernels must reconstruct their obligations.
use super::*;
use zil_ir::{
    DynDeclaration, DynDeclarationKind, DynGenericActual, DynInterface, DynInterfaceId, DynMember,
    DynMemberDeclaration, DynMemberKind, DynReceiver, DynSlot, DynTraitApplication,
    LifeDeclarationId, LifeParameterKind,
};

pub(super) struct Declaration {
    origin: origins::DeclId,
    symbol: Option<String>,
    kind: DynDeclarationKind,
    parameters: Vec<LifeParameterKind>,
    members: Vec<DynMemberDeclaration>,
}

pub(super) struct Target {
    table: usize,
    slot: usize,
    module: zil_base::ModuleId,
    function: zil_base::NodeId,
}

fn receiver_position() -> zil_ir::StoragePosition {
    zil_ir::StoragePosition {
        base: zil_ir::StorageBase::Parameter(0),
        path: Vec::new(),
    }
}

fn scalar_read_contract(mutable: bool) -> zil_ir::DynContract {
    let receiver = receiver_position();
    zil_ir::DynContract {
        requires: Some(vec![if mutable {
            zil_ir::DynRequirement::Write(receiver.clone())
        } else {
            zil_ir::DynRequirement::Read(receiver.clone())
        }]),
        guarantees: Some(Vec::new()),
        hold: Some(Vec::new()),
        call: Some(zil_ir::DynCallAccess {
            reads: vec![receiver.clone()],
            writes: if mutable { vec![receiver] } else { Vec::new() },
        }),
        result: Some(zil_ir::DynResultSource::Value),
        cleanup: Some(zil_ir::DynCleanupContract {
            environment: false,
            positions: Vec::new(),
            access: zil_ir::DynCallAccess {
                reads: Vec::new(),
                writes: Vec::new(),
            },
            effects: Vec::new(),
        }),
        effects: Some(Vec::new()),
        resume: Some(zil_ir::DynResumeBound::Local),
        threads: Some(zil_ir::DynThreadRequirements {
            transfer: false,
            share: false,
        }),
    }
}

fn prove_scalar_read(
    provider: &zil_ir::CoreFunction,
    table: &zil_ir::CoreDynVtable,
    slot: &zil_ir::DynSlot,
    modules: &[&zil_ir::CoreModule],
    budget: &mut Budget<'_>,
) -> R<()> {
    budget.node(provider.span, 0)?;
    let zil_ir::Type::Named(self_name) = &table.self_type else {
        return Err(unsupported(
            provider.span,
            "dyn provider Self needs a concrete named Copy struct",
        ));
    };
    budget.text(self_name, provider.span)?;
    if provider.foreign.is_some()
        || !provider.type_parameters.is_empty()
        || !provider.state_parameters.is_empty()
        || !provider.callable_parameters.is_empty()
        || provider.parameters.len() != slot.parameters.len() + 1
        || provider.return_type != slot.result
        || provider.result_mode != slot.result_mode
        || !(slot.result.is_b0_scalar() || slot.result == zil_ir::Type::Unit)
        || provider.parameters[0].mode != slot.receiver.mode
        || provider.parameters[0].ty
            != (zil_ir::Type::Address {
                mutable: slot.receiver.mutable,
                inner: Box::new(table.self_type.clone()),
            })
    {
        return Err(unsupported(
            provider.span,
            "dyn provider is outside closed scalar read profile",
        ));
    }
    for (actual, expected) in provider.parameters.iter().skip(1).zip(&slot.parameters) {
        budget.node(actual.span, 0)?;
        if actual.ty != *expected {
            return Err(error(
                actual.span,
                "dyn provider parameter differs from slot",
            ));
        }
    }
    let owner = modules
        .iter()
        .find(|module| module.module_id == table.id.module)
        .ok_or_else(|| unsupported(provider.span, "dyn provider module absent"))?;
    let mut current = provider;
    let mut seen = Vec::new();
    let value = loop {
        budget.node(current.span, 0)?;
        for id in &seen {
            budget.node(current.span, 0)?;
            if *id == current.id {
                return Err(unsupported(
                    current.span,
                    "dyn provider call cycle is unknown",
                ));
            }
        }
        seen.push(current.id);
        if slot.receiver.mutable && current.body.statements.len() == 2 {
            for aggregate in &owner.aggregates {
                budget.node(aggregate.span, 0)?;
            }
            for parameter in &current.parameters {
                budget.node(parameter.span, 0)?;
            }
            let [assignment, returned] = current.body.statements.as_slice() else {
                return Err(unsupported(
                    current.span,
                    "mutable dyn provider needs a proven field update and return",
                ));
            };
            let zil_ir::CoreStatementKind::Assign { target, value } = &assignment.kind else {
                return Err(unsupported(
                    assignment.span,
                    "mutable dyn provider has unproven write",
                ));
            };
            let zil_ir::CoreExprKind::Member {
                object,
                member,
                owner: None,
            } = &target.kind
            else {
                return Err(unsupported(
                    target.span,
                    "mutable dyn provider must write a real Self field",
                ));
            };
            if !matches!(&object.kind, zil_ir::CoreExprKind::Name(name) if name == &current.parameters[0].name)
                || !matches!(
                    &value.kind,
                    zil_ir::CoreExprKind::Integer(_)
                        | zil_ir::CoreExprKind::Bool(_)
                        | zil_ir::CoreExprKind::Name(_)
                )
            {
                return Err(unsupported(
                    assignment.span,
                    "mutable dyn write value or source is unproven",
                ));
            }
            let aggregate = owner
                .aggregates
                .iter()
                .find(|a| &a.name == self_name)
                .ok_or_else(|| unsupported(current.span, "mutable dyn Self aggregate absent"))?;
            for _ in 0..4 {
                for field in &aggregate.fields {
                    budget.node(field.span, 0)?;
                    budget.text(&field.name, field.span)?;
                }
            }
            let field = aggregate
                .fields
                .iter()
                .find(|f| f.name == *member)
                .ok_or_else(|| unsupported(target.span, "mutable dyn field absent"))?;
            if aggregate.kind != zil_ir::AggregateKind::Struct
                || aggregate
                    .fields
                    .iter()
                    .any(|f| f.manages || !f.ty.is_b0_scalar())
                || !field.ty.is_b0_scalar()
                || (matches!(&value.kind, zil_ir::CoreExprKind::Name(name)
                    if !current.parameters.iter().skip(1).any(|p| p.name == *name && p.ty == field.ty)))
            {
                return Err(unsupported(
                    assignment.span,
                    "mutable dyn field/value lacks finite Copy proof",
                ));
            }
            let returns_unit = matches!(
                &returned.kind,
                zil_ir::CoreStatementKind::Return(None)
                    | zil_ir::CoreStatementKind::Return(Some(zil_ir::CoreExpr {
                        kind: zil_ir::CoreExprKind::Unit,
                        ..
                    }))
            ) && slot.result == zil_ir::Type::Unit;
            let returns_field = matches!(&returned.kind, zil_ir::CoreStatementKind::Return(Some(zil_ir::CoreExpr { kind: zil_ir::CoreExprKind::Member { object, member: result_member, owner: None }, .. }))
                if matches!(&object.kind, zil_ir::CoreExprKind::Name(name) if name == &current.parameters[0].name)
                    && aggregate.fields.iter().any(|f| f.name == *result_member && f.ty == slot.result));
            if !returns_unit && !returns_field {
                return Err(unsupported(
                    returned.span,
                    "mutable dyn result lacks actual Self source",
                ));
            }
            return Ok(());
        }

        let [statement] = current.body.statements.as_slice() else {
            return Err(unsupported(
                current.span,
                "dyn provider body needs one proven return",
            ));
        };
        if !slot.receiver.mutable
            && slot.result == zil_ir::Type::Unit
            && matches!(
                &statement.kind,
                zil_ir::CoreStatementKind::Return(None)
                    | zil_ir::CoreStatementKind::Return(Some(zil_ir::CoreExpr {
                        kind: zil_ir::CoreExprKind::Unit,
                        ..
                    }))
            )
        {
            return Ok(());
        }
        let zil_ir::CoreStatementKind::Return(Some(value)) = &statement.kind else {
            return Err(unsupported(
                statement.span,
                "dyn provider body has unproven effects",
            ));
        };
        let zil_ir::CoreExprKind::Call { callee, arguments } = &value.kind else {
            break value;
        };
        // Default methods have a real wrapper. Resolve its exact callee in
        // the originating module and carry the actual Self root through each
        // reachable direct call. No callee-name spelling grants authority.
        if !slot.parameters.is_empty() || current.parameters.len() != 1 {
            return Err(unsupported(
                value.span,
                "dyn wrapper has unproven extra arguments",
            ));
        }
        let mut target = None;
        match &callee.kind {
            zil_ir::CoreExprKind::Name(target_name)
                if arguments.len() == 1
                    && matches!(&arguments[0].kind, zil_ir::CoreExprKind::Name(name) if name == &current.parameters[0].name) =>
            {
                budget.text(target_name, callee.span)?;
                for module in modules {
                    for function in &module.functions {
                        budget.node(function.span, 0)?;
                        if function.name == *target_name {
                            if target.is_some() {
                                return Err(unsupported(
                                    value.span,
                                    "dyn direct wrapper target is ambiguous",
                                ));
                            }
                            target = Some(function);
                        }
                    }
                }
            }
            zil_ir::CoreExprKind::Member {
                object,
                member,
                owner: None,
            } if arguments.len() == 1
                && matches!(&object.kind, zil_ir::CoreExprKind::Name(_))
                && matches!(&arguments[0].kind, zil_ir::CoreExprKind::Name(name)
                        if name == &current.parameters[0].name) =>
            {
                budget.text(member, callee.span)?;
                for _ in modules {
                    budget.node(callee.span, 0)?;
                }
                let trait_module = modules
                    .iter()
                    .find(|m| m.module_id == table.interface.module)
                    .ok_or_else(|| {
                        unsupported(callee.span, "dyn default definition module absent")
                    })?;
                for function in &trait_module.functions {
                    budget.node(function.span, 0)?;
                }
                let mut targets = trait_module.functions.iter().filter(|f| f.name == *member);
                target = targets.next();
                if targets.next().is_some() {
                    return Err(unsupported(
                        callee.span,
                        "dyn default definition is ambiguous",
                    ));
                }
            }
            zil_ir::CoreExprKind::Member {
                object,
                member,
                owner: None,
            } if arguments.is_empty()
                && matches!(&object.kind, zil_ir::CoreExprKind::Name(name) if name == &current.parameters[0].name) =>
            {
                budget.text(member, callee.span)?;
                for module in modules {
                    budget.node(callee.span, 0)?;
                    for row in &module.dyn_interfaces {
                        budget.node(callee.span, 0)?;
                        for _ in &row.slots {
                            budget.node(callee.span, 0)?;
                        }
                    }
                    for declaration in &module.dyn_declarations {
                        budget.node(callee.span, 0)?;
                        for part in &declaration.members {
                            budget.node(callee.span, 0)?;
                            budget.text(&part.name, callee.span)?;
                        }
                    }
                    for function in &module.functions {
                        budget.node(function.span, 0)?;
                    }
                }
                for _ in &table.entries {
                    budget.node(callee.span, 0)?;
                }
                let interface = modules
                    .iter()
                    .flat_map(|m| &m.dyn_interfaces)
                    .find(|i| i.id == table.interface)
                    .ok_or_else(|| unsupported(callee.span, "dyn wrapper interface absent"))?;
                let declaration = modules
                    .iter()
                    .flat_map(|m| &m.dyn_declarations)
                    .find(|d| d.id == interface.application.declaration)
                    .ok_or_else(|| unsupported(callee.span, "dyn wrapper declaration absent"))?;
                let ordinal = declaration
                    .members
                    .iter()
                    .position(|m| m.name == *member)
                    .ok_or_else(|| unsupported(callee.span, "dyn wrapper member absent"))?;
                let entry = table
                    .entries
                    .get(ordinal)
                    .filter(|e| e.slot as usize == ordinal)
                    .ok_or_else(|| {
                        unsupported(callee.span, "dyn wrapper actual provider entry absent")
                    })?;
                target = modules
                    .iter()
                    .find(|m| m.module_id == entry.module)
                    .and_then(|m| m.functions.iter().find(|f| f.id == entry.function));
            }
            _ => {
                return Err(unsupported(
                    callee.span,
                    "dyn provider callee or receiver is not direct",
                ));
            }
        }
        let next =
            target.ok_or_else(|| unsupported(value.span, "dyn provider call target absent"))?;
        if next.foreign.is_some()
            || !next.type_parameters.is_empty()
            || !next.state_parameters.is_empty()
            || !next.callable_parameters.is_empty()
            || next.parameters.len() != 1
            || next.parameters[0].ty != current.parameters[0].ty
            || next.parameters[0].mode != current.parameters[0].mode
            || next.return_type != current.return_type
            || next.result_mode != current.result_mode
        {
            return Err(unsupported(
                value.span,
                "dyn provider call target is not closed",
            ));
        }
        current = next;
    };
    if slot.receiver.mutable {
        return Err(unsupported(
            value.span,
            "mutable dyn provider chain has no proven receiver write",
        ));
    }
    if matches!(
        &value.kind,
        zil_ir::CoreExprKind::Integer(_)
            | zil_ir::CoreExprKind::Float(_)
            | zil_ir::CoreExprKind::Bool(_)
            | zil_ir::CoreExprKind::Literal(_)
    ) {
        // Ordinary Types still checks the exact literal's value and result
        // type. Its syntax has no receiver access or reachable call.
        return Ok(());
    }
    let zil_ir::CoreExprKind::Member {
        object,
        member,
        owner: None,
    } = &value.kind
    else {
        return Err(unsupported(
            value.span,
            "dyn provider result must read receiver scalar field",
        ));
    };
    if !matches!(&object.kind, zil_ir::CoreExprKind::Name(name) if name == &current.parameters[0].name)
    {
        return Err(unsupported(
            object.span,
            "dyn provider read must originate at actual receiver",
        ));
    }
    let zil_ir::Type::Named(name) = &table.self_type else {
        return Err(unsupported(
            provider.span,
            "dyn provider Self needs a concrete named Copy struct",
        ));
    };
    for module in modules {
        budget.node(provider.span, 0)?;
        if module.module_id == table.id.module {
            for aggregate in &module.aggregates {
                budget.node(aggregate.span, 0)?;
            }
        }
    }
    let aggregate = modules
        .iter()
        .find(|module| module.module_id == table.id.module)
        .and_then(|module| {
            module
                .aggregates
                .iter()
                .find(|aggregate| &aggregate.name == name)
        })
        .ok_or_else(|| unsupported(provider.span, "dyn provider concrete Self aggregate absent"))?;
    for field in &aggregate.fields {
        budget.node(field.span, 0)?;
        budget.text(&field.name, field.span)?;
    }
    if aggregate.kind != zil_ir::AggregateKind::Struct
        || aggregate
            .fields
            .iter()
            .any(|field| field.manages || !field.ty.is_b0_scalar())
        || !aggregate
            .fields
            .iter()
            .any(|field| field.name == *member && field.ty == slot.result)
    {
        return Err(unsupported(
            value.span,
            "dyn provider field read lacks finite Copy proof",
        ));
    }
    Ok(())
}

fn parameters(parameters: &[GenericParam], budget: &mut Budget<'_>) -> R<Vec<LifeParameterKind>> {
    let mut result = Vec::new();
    for parameter in parameters {
        budget.node(parameter.span, 0)?;
        result.push(match &parameter.kind {
            GenericParamKind::Type(_) => LifeParameterKind::Type,
            GenericParamKind::Lifetime => LifeParameterKind::Lifetime,
            GenericParamKind::Const(ty) => LifeParameterKind::Const(lower_type(ty, budget)?),
        });
    }
    Ok(result)
}

pub(super) fn declarations(
    module: &Module,
    origins: &origins::Origins,
    budget: &mut Budget<'_>,
) -> R<Vec<Declaration>> {
    let mut result = Vec::new();
    for (ordinal, item) in module.items.iter().enumerate() {
        budget.node(item.span, 0)?;
        let (kind, symbol, generics, members) = match &item.kind {
            ItemKind::Trait(declaration) => (
                DynDeclarationKind::Trait,
                Some(&declaration.name),
                &declaration.generics,
                &declaration.members,
            ),
            ItemKind::Impl(implementation) if implementation.trait_type.is_some() => (
                DynDeclarationKind::Impl,
                None,
                &implementation.generics,
                &implementation.members,
            ),
            _ => continue,
        };
        let generic_parameters = parameters(generics, budget)?;
        let mut output_members = Vec::new();
        for member in members {
            budget.node(member.span, 0)?;
            let (name, kind) = match &member.kind {
                ItemKind::TypeAlias(alias) => (&alias.name, DynMemberKind::Type),
                ItemKind::Const(constant) => (
                    &constant.name,
                    DynMemberKind::Constant(lower_type(
                        constant.ty.as_ref().ok_or_else(|| {
                            unsupported(
                                member.span,
                                "dyn impl constant header needs its substituted declared type",
                            )
                        })?,
                        budget,
                    )?),
                ),
                ItemKind::Function(function) => (
                    &function.name,
                    DynMemberKind::Method(parameters(&function.generics, budget)?),
                ),
                _ => {
                    return Err(unsupported(
                        member.span,
                        "dyn declaration member needs an explicit semantic kind",
                    ));
                }
            };
            budget.text(name, member.span)?;
            output_members.push(DynMemberDeclaration {
                name: name.clone(),
                kind,
            });
        }
        result.push(Declaration {
            origin: origins.get(ordinal, item.span)?.declaration,
            symbol: symbol
                .map(|symbol| {
                    budget.text(symbol, item.span)?;
                    Ok::<_, Diagnostic>(symbol.clone())
                })
                .transpose()?,
            kind,
            parameters: generic_parameters,
            members: output_members,
        });
    }
    Ok(result)
}

fn lower_type(ty: &TypeRef, budget: &mut Budget<'_>) -> R<zil_ir::Type> {
    budget.ty(ty)?;
    // Unsupported remains explicit; concrete Core nominal spellings will be
    // resolved against the actual Core aggregate inventory in Types.
    let lowered = crate::type_ref(ty);
    if matches!(lowered, zil_ir::Type::Unsupported(_)) {
        return Err(unsupported(
            ty.span,
            "dyn declaration needs a concrete lowered type",
        ));
    }
    pay_type(&lowered, budget, ty.span, 0)?;
    Ok(lowered)
}

fn declaration_id(
    original: origins::DeclId,
    origins: &origins::Origins,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<LifeDeclarationId> {
    let captured = origins.identity(original, budget, span)?;
    Ok(LifeDeclarationId {
        module: original.module,
        declaration: captured.declaration,
        compiler: original.compiler,
    })
}

impl Catalog {
    fn require_fixed_members(
        &self,
        origin: origins::DeclId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        for declaration in &self.declarations {
            budget.node(span, 0)?;
            if declaration.origin == origin {
                for member in &declaration.members {
                    budget.node(span, 0)?;
                    if !matches!(member.kind, DynMemberKind::Method(_)) {
                        return Err(unsupported(
                            span,
                            "dyn associated member needs its explicit binding consumer",
                        ));
                    }
                }
                return Ok(());
            }
        }
        Err(error(span, "dyn trait header origin absent"))
    }

    pub(crate) fn bind_targets(
        &mut self,
        module: &Module,
        emissions: &[(usize, usize, zil_base::NodeId)],
        origins: &origins::Origins,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        if self.is_empty() {
            return Ok(());
        }
        for &(owner, member, function) in emissions {
            budget.node(module.span, 0)?;
            let item = module
                .items
                .get(owner)
                .ok_or_else(|| error(module.span, "dyn emitted declaration placement absent"))?;
            let ItemKind::Impl(implementation) = &item.kind else {
                return Err(error(
                    item.span,
                    "dyn method emission is not owned by an actual impl",
                ));
            };
            let member = implementation
                .members
                .get(member)
                .ok_or_else(|| error(item.span, "dyn emitted member placement absent"))?;
            let ItemKind::Function(method) = &member.kind else {
                return Err(error(
                    member.span,
                    "dyn emitted member is not an ordinary function",
                ));
            };
            let actual_origin = origins
                .in_module(module.id, owner, budget, item.span)?
                .declaration;
            let self_key = type_key(&implementation.target, budget)?;
            for (table, vtable) in self.vtables.iter().enumerate() {
                budget.node(item.span, 0)?;
                let expected_self = type_key(&vtable.self_type, budget)?;
                budget.text(&self_key, item.span)?;
                budget.text(&expected_self, item.span)?;
                if self_key != expected_self {
                    continue;
                }
                let interface = &self.interfaces[vtable.interface];
                for (slot, provider) in vtable.providers.iter().enumerate() {
                    budget.node(member.span, 0)?;
                    if provider.implementation != actual_origin {
                        continue;
                    }
                    let expected = &interface.slots[slot].signature.name;
                    budget.text(expected, member.span)?;
                    budget.text(&method.name, member.span)?;
                    if expected != &method.name {
                        continue;
                    }
                    for prior in &self.targets {
                        budget.node(member.span, 0)?;
                        if prior.table == table && prior.slot == slot {
                            return Err(error(
                                member.span,
                                "dyn provider has multiple actual emitted targets",
                            ));
                        }
                    }
                    self.targets.push(Target {
                        table,
                        slot,
                        module: module.id,
                        function,
                    });
                }
            }
        }
        Ok(())
    }

    fn source_vtables(
        &self,
        origins: &origins::Origins,
        budget: &mut Budget<'_>,
    ) -> R<Vec<zil_ir::CoreDynVtable>> {
        let mut output = Vec::new();
        for (ordinal, table) in self.vtables.iter().enumerate() {
            let span = table.self_type.span;
            budget.node(span, 0)?;
            let mut index = 0u32;
            for earlier in &self.vtables[..ordinal] {
                budget.node(span, 0)?;
                if earlier.implementation.module == table.implementation.module {
                    index = index
                        .checked_add(1)
                        .ok_or_else(|| error(span, "dyn vtable relocation exhausted"))?;
                }
            }
            let mut entries = Vec::new();
            for (slot, _) in table.providers.iter().enumerate() {
                budget.node(span, 0)?;
                let mut found = None;
                for target in &self.targets {
                    budget.node(span, 0)?;
                    if target.table == ordinal && target.slot == slot {
                        if found.is_some() {
                            return Err(error(span, "dyn emitted target is ambiguous"));
                        }
                        found = Some(target);
                    }
                }
                let target = found.ok_or_else(|| {
                    unsupported(span, "dyn provider has no actual emitted ordinary function")
                })?;
                entries.push(zil_ir::CoreDynEntry {
                    slot: u32::try_from(slot)
                        .map_err(|_| error(span, "dyn slot relocation exhausted"))?,
                    module: target.module,
                    function: target.function,
                });
            }
            output.push(zil_ir::CoreDynVtable {
                id: zil_ir::DynVtableId {
                    module: table.implementation.module,
                    index,
                },
                interface: self.interface_id(table.interface, budget, span)?,
                self_type: lower_type(&table.self_type, budget)?,
                implementation: declaration_id(table.implementation, origins, budget, span)?,
                actuals: Vec::new(),
                ports: Vec::new(),
                entries,
                cleanup: None,
            });
        }
        Ok(output)
    }

    pub(crate) fn publish(
        &self,
        cores: &mut [(&mut zil_ir::CoreModule, &mut zil_base::SourceMap)],
        origins: &origins::Origins,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        if self.is_empty() {
            return Ok(());
        }
        if origins.identities.is_empty() {
            return Err(unsupported(
                Span::default(),
                "dyn source declaration context is unavailable",
            ));
        }
        let span = Span::default();
        for declaration in self.source_declarations(origins, budget, span)? {
            for _ in cores.iter() {
                budget.node(span, 0)?;
            }
            let (core, _) = cores
                .iter_mut()
                .find(|(core, _)| core.module_id == declaration.id.module)
                .ok_or_else(|| error(span, "dyn source declaration output module absent"))?;
            core.dyn_declarations.push(declaration);
        }
        for interface in self.source_interfaces(origins, budget, span)? {
            for _ in cores.iter() {
                budget.node(span, 0)?;
            }
            let (core, _) = cores
                .iter_mut()
                .find(|(core, _)| core.module_id == interface.id.module)
                .ok_or_else(|| error(span, "dyn source interface output module absent"))?;
            core.dyn_interfaces.push(interface);
        }
        for table in self.source_vtables(origins, budget)? {
            for _ in cores.iter() {
                budget.node(span, 0)?;
            }
            let (core, _) = cores
                .iter_mut()
                .find(|(core, _)| core.module_id == table.id.module)
                .ok_or_else(|| error(span, "dyn source vtable output module absent"))?;
            core.dyn_vtables.push(table);
        }
        // A slot is executable only when every actual entry has a closed
        // body. Shared entries prove bounded scalar reads; exclusive entries
        // prove a real scalar Self write, including checked direct wrappers.
        // A declaration's scalar signature alone grants none of these facts.
        for _ in cores.iter() {
            budget.node(span, 0)?;
        }
        let snapshot: Vec<&zil_ir::CoreModule> = cores.iter().map(|(core, _)| &**core).collect();
        let mut decisions = Vec::new();
        for core in &snapshot {
            for interface in &core.dyn_interfaces {
                for (ordinal, slot) in interface.slots.iter().enumerate() {
                    budget.node(span, 0)?;
                    let mut found = false;
                    let mut proven = true;
                    for module in &snapshot {
                        for table in &module.dyn_vtables {
                            budget.node(span, 0)?;
                            if table.interface != interface.id {
                                continue;
                            }
                            found = true;
                            let entry = table.entries.get(ordinal).ok_or_else(|| {
                                unsupported(span, "dyn slot has no actual provider entry")
                            })?;
                            if entry.slot as usize != ordinal {
                                return Err(error(span, "dyn provider slot order differs"));
                            }
                            for owner in &snapshot {
                                budget.node(span, 0)?;
                                if owner.module_id == entry.module {
                                    for function in &owner.functions {
                                        budget.node(function.span, 0)?;
                                    }
                                }
                            }
                            let provider = snapshot
                                .iter()
                                .find(|owner| owner.module_id == entry.module)
                                .and_then(|owner| {
                                    owner.functions.iter().find(|f| f.id == entry.function)
                                })
                                .ok_or_else(|| unsupported(span, "dyn entry target body absent"))?;
                            match prove_scalar_read(provider, table, slot, &snapshot, budget) {
                                Ok(()) => {}
                                Err(diagnostic)
                                    if diagnostic.category
                                        == zil_base::ResultCategory::Unsupported =>
                                {
                                    proven = false
                                }
                                Err(diagnostic) => return Err(diagnostic),
                            }
                        }
                    }
                    decisions.push((
                        interface.id,
                        ordinal,
                        found
                            && proven
                            && interface.application.actuals.is_empty()
                            && interface.inherited.is_empty(),
                    ));
                }
            }
        }
        drop(snapshot);
        for (core, _) in cores.iter_mut() {
            for interface in &mut core.dyn_interfaces {
                for (id, ordinal, proven) in &decisions {
                    budget.node(span, 0)?;
                    if *proven && *id == interface.id {
                        interface.slots[*ordinal].contract =
                            scalar_read_contract(interface.slots[*ordinal].receiver.mutable);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.interfaces.is_empty()
    }

    pub(crate) fn contains_type(&self, ty: &TypeRef, budget: &mut Budget<'_>) -> R<bool> {
        use crate::ct_selection::Visitor;
        struct Contains<'a, 'l> {
            budget: &'a mut Budget<'l>,
            found: bool,
        }
        impl Visitor for Contains<'_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                self.budget.node(span, depth).map_err(|e| vec![e])
            }
            fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                self.budget.text(text, span).map_err(|e| vec![e])
            }
            fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
                self.found |= matches!(ty.kind, TypeKind::Dyn { .. });
                crate::ct_selection::walk_type(self, ty, depth)
            }
        }
        let mut contains = Contains {
            budget,
            found: false,
        };
        contains.ty(ty, 0).map_err(|mut errors| errors.remove(0))?;
        Ok(contains.found)
    }

    fn trait_origin(
        &self,
        reference: &trait_instances::Reference,
        budget: &mut Budget<'_>,
    ) -> R<origins::DeclId> {
        for declaration in &self.declarations {
            budget.node(reference.key.declaration, 0)?;
            if let Some(symbol) = &declaration.symbol {
                budget.text(symbol, reference.key.declaration)?;
                budget.text(&reference.key.name, reference.key.declaration)?;
                if symbol == &reference.key.name {
                    return Ok(declaration.origin);
                }
            }
        }
        Err(unsupported(
            reference.key.declaration,
            "dyn trait declaration context is absent",
        ))
    }

    fn interface_id(
        &self,
        ordinal: usize,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<DynInterfaceId> {
        let owner = self
            .interfaces
            .get(ordinal)
            .ok_or_else(|| error(span, "dyn interface ordinal absent"))?
            .declaration
            .module;
        let mut index = 0u32;
        for earlier in &self.interfaces[..ordinal] {
            budget.node(span, 0)?;
            if earlier.declaration.module == owner {
                index = index
                    .checked_add(1)
                    .ok_or_else(|| error(span, "dyn interface relocation exhausted"))?;
            }
        }
        Ok(DynInterfaceId {
            module: owner,
            index,
        })
    }

    fn source_reference(
        &self,
        ty: &TypeRef,
        budget: &mut Budget<'_>,
    ) -> R<trait_instances::Reference> {
        budget.ty(ty)?;
        let TypeKind::Dyn {
            path,
            args,
            bindings,
        } = &ty.kind
        else {
            return Err(error(ty.span, "dyn source lookup needs actual Dyn syntax"));
        };
        if !bindings.is_empty() {
            return Err(unsupported(
                ty.span,
                "dyn binding semantic consumer is not yet implemented",
            ));
        }
        if path.len() != 1 {
            return Err(unsupported(
                ty.span,
                "dyn source lookup needs its module-resolved trait declaration",
            ));
        }
        let mut arguments = Vec::new();
        let mut keys = Vec::new();
        for argument in args {
            let GenericArg::Type(actual) = argument else {
                return Err(unsupported(
                    ty.span,
                    "dyn const/life actual needs its complete source consumer",
                ));
            };
            keys.push(type_key(actual, budget)?);
            budget.ty(actual)?;
            arguments.push(actual.clone());
        }
        budget.text(&path[0], ty.span)?;
        Ok(trait_instances::Reference {
            key: trait_instances::Key {
                declaration: ty.span,
                name: path[0].clone(),
                arguments: keys,
            },
            arguments,
        })
    }

    /// Ordinary resolution of explicit Dyn syntax against the actual catalog.
    /// No generated spelling, address, Span or raw numeric ID is authority.
    pub(crate) fn core_type(
        &self,
        ty: &TypeRef,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<zil_ir::Type> {
        budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Dyn { .. } => {
                let reference = self.source_reference(ty, budget)?;
                let ordinal = self.find(&reference, budget, ty.span)?.ok_or_else(|| {
                    unsupported(ty.span, "dyn type has no actual declaration catalog")
                })?;
                Ok(zil_ir::Type::Dyn {
                    interface: self.interface_id(ordinal, budget, ty.span)?,
                })
            }
            TypeKind::Group(inner) => self.core_type(inner, budget, depth + 1),
            TypeKind::Reference {
                lifetime: None,
                mutable,
                inner,
            } => Ok(zil_ir::Type::Address {
                mutable: *mutable,
                inner: Box::new(self.core_type(inner, budget, depth + 1)?),
            }),
            TypeKind::Tuple(fields) => {
                let mut output = Vec::new();
                for field in fields {
                    output.push(self.core_type(field, budget, depth + 1)?);
                }
                Ok(zil_ir::Type::Tuple(output))
            }
            _ => lower_type(ty, budget),
        }
    }

    pub(crate) fn source_interfaces(
        &self,
        origins: &origins::Origins,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Vec<DynInterface>> {
        let mut result = Vec::new();
        for (ordinal, source) in self.interfaces.iter().enumerate() {
            budget.node(span, 0)?;
            // Unfixed equations are part of interface identity. Until explicit
            // binding rows are consumed, they cannot silently become empty.
            self.require_fixed_members(source.declaration, budget, span)?;
            let application =
                self.application(&source.reference, source.declaration, origins, budget)?;
            let mut inherited = Vec::new();
            for reference in &source.inherited {
                let declaration = self.trait_origin(reference, budget)?;
                self.require_fixed_members(declaration, budget, span)?;
                inherited.push(self.application(reference, declaration, origins, budget)?);
            }
            let mut slots = Vec::new();
            for slot in &source.slots {
                budget.node(slot.signature.span, 0)?;
                if slot.contract_written || !slot.signature.generics.is_empty() {
                    return Err(unsupported(
                        slot.signature.span,
                        "dyn slot needs its complete declared contract/application consumer",
                    ));
                }
                let receiver = slot
                    .signature
                    .params
                    .first()
                    .ok_or_else(|| error(slot.signature.span, "dyn slot receiver absent"))?;
                budget.ty(&receiver.ty)?;
                let TypeKind::Reference {
                    mutable,
                    lifetime: None,
                    ..
                } = &grouped(&receiver.ty).kind
                else {
                    return Err(unsupported(
                        receiver.span,
                        "dyn source receiver requires a checked borrowed receiver",
                    ));
                };
                let mut parameters = Vec::new();
                let mut parameter_modes = Vec::new();
                for parameter in slot.signature.params.iter().skip(1) {
                    let lowered = lower_type(&parameter.ty, budget)?;
                    if !lowered.is_b0_scalar() && lowered != zil_ir::Type::Unit {
                        return Err(unsupported(
                            parameter.span,
                            "dyn parameter needs its complete retained/lending source contract",
                        ));
                    }
                    parameters.push(lowered);
                    parameter_modes.push(crate::mode(parameter));
                }
                let result_type = match &slot.signature.return_type {
                    Some(ty) => lower_type(ty, budget)?,
                    None => zil_ir::Type::Unit,
                };
                if !result_type.is_b0_scalar() && result_type != zil_ir::Type::Unit {
                    return Err(unsupported(
                        slot.signature.span,
                        "dyn result needs its complete retained/lending source contract",
                    ));
                }
                let application = self.application(
                    &slot.reference,
                    slot.declaration.declaration,
                    origins,
                    budget,
                )?;
                // This profile has scalar nonreceiver parameters. The sole
                // invocation address is the actual receiver at position zero.
                let bindings = vec![zil_ir::StorageLifeBinding {
                    position: zil_ir::StoragePosition {
                        base: zil_ir::StorageBase::Parameter(0),
                        path: vec![],
                    },
                    binder: zil_ir::StorageBinder::Invocation(0),
                }];
                slots.push(DynSlot {
                    member: DynMember {
                        application,
                        ordinal: slot.declaration.ordinal,
                    },
                    actuals: Vec::new(),
                    receiver: DynReceiver {
                        mutable: *mutable,
                        mode: crate::mode(receiver),
                    },
                    parameters,
                    parameter_modes,
                    result: result_type,
                    result_mode: zil_ir::PassingMode::Infer,
                    call_kind: zil_ir::CallKind::Direct,
                    storage_life: Some(zil_ir::StorageLifeContract {
                        fixed_binder_count: 0,
                        invocation_binder_count: 1,
                        bindings,
                        classes: Vec::new(),
                    }),
                    contract: zil_ir::DynContract {
                        requires: None,
                        guarantees: None,
                        hold: None,
                        call: None,
                        result: None,
                        cleanup: None,
                        effects: None,
                        resume: None,
                        threads: None,
                    },
                });
            }
            result.push(DynInterface {
                id: self.interface_id(ordinal, budget, span)?,
                application,
                inherited,
                associated_types: Vec::new(),
                associated_constants: Vec::new(),
                retained_life: zil_ir::RetainedLifeShape {
                    binder_count: 0,
                    ports: Vec::new(),
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                },
                slots,
            });
        }
        Ok(result)
    }

    fn application(
        &self,
        reference: &trait_instances::Reference,
        declaration: origins::DeclId,
        origins: &origins::Origins,
        budget: &mut Budget<'_>,
    ) -> R<DynTraitApplication> {
        let mut actuals = Vec::new();
        for actual in &reference.arguments {
            budget.node(actual.span, 0)?;
            let lowered = lower_type(actual, budget)?;
            if !lowered.is_b0_scalar() && lowered != zil_ir::Type::Unit {
                return Err(unsupported(
                    actual.span,
                    "dyn trait actual needs its full retained type-class context",
                ));
            }
            actuals.push(DynGenericActual::Type(lowered));
        }
        Ok(DynTraitApplication {
            declaration: declaration_id(declaration, origins, budget, reference.key.declaration)?,
            actuals,
        })
    }

    pub(crate) fn source_declarations(
        &self,
        origins: &origins::Origins,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Vec<DynDeclaration>> {
        let mut result = Vec::new();
        for declaration in &self.declarations {
            budget.node(span, 0)?;
            let original = origins.identity(declaration.origin, budget, span)?;
            let identity = &original.identity;
            budget.text(&identity.package, span)?;
            for part in identity
                .module_path
                .iter()
                .chain(&identity.declaration_path)
            {
                budget.node(span, 0)?;
                budget.text(part, span)?;
            }
            for _ in &identity.generation_path {
                budget.node(span, 0)?;
            }
            for parameter in &declaration.parameters {
                pay_parameter(parameter, budget, span)?;
            }
            for member in &declaration.members {
                budget.node(span, 0)?;
                budget.text(&member.name, span)?;
                match &member.kind {
                    DynMemberKind::Type => {}
                    DynMemberKind::Constant(ty) => pay_type(ty, budget, span, 0)?,
                    DynMemberKind::Method(parameters) => {
                        for parameter in parameters {
                            pay_parameter(parameter, budget, span)?;
                        }
                    }
                }
            }
            result.push(DynDeclaration {
                id: declaration_id(declaration.origin, origins, budget, span)?,
                identity: identity.clone(),
                kind: declaration.kind,
                parameters: declaration.parameters.clone(),
                members: declaration.members.clone(),
            });
        }
        Ok(result)
    }
}

fn pay_parameter(parameter: &LifeParameterKind, budget: &mut Budget<'_>, span: Span) -> R<()> {
    budget.node(span, 0)?;
    if let LifeParameterKind::Const(ty) = parameter {
        pay_type(ty, budget, span, 0)?;
    }
    Ok(())
}

pub(crate) fn prepay_substitute_self(
    ty: &zil_ir::Type,
    host: Option<&zil_ir::Type>,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<()> {
    budget.node(span, depth)?;
    match ty {
        zil_ir::Type::Named(name) => {
            budget.text(name, span)?;
            if name == "Self"
                && let Some(host) = host
            {
                pay_type(host, budget, span, depth + 1)?;
            }
        }
        zil_ir::Type::Address { inner, .. }
        | zil_ir::Type::Desc(inner)
        | zil_ir::Type::Managed(inner) => {
            prepay_substitute_self(inner, host, budget, span, depth + 1)?
        }
        zil_ir::Type::Tuple(fields) => {
            for field in fields {
                prepay_substitute_self(field, host, budget, span, depth + 1)?;
            }
        }
        zil_ir::Type::Dyn { .. } => {}
        _ if ty.is_b0_scalar() || matches!(ty, zil_ir::Type::Unit) => {}
        _ => {
            return Err(unsupported(
                span,
                "dyn Self substitution needs a complete type visitor",
            ));
        }
    }
    Ok(())
}

fn pay_type(ty: &zil_ir::Type, budget: &mut Budget<'_>, span: Span, depth: usize) -> R<()> {
    budget.node(span, depth)?;
    match ty {
        zil_ir::Type::Named(name) => budget.text(name, span)?,
        zil_ir::Type::Address { inner, .. }
        | zil_ir::Type::Desc(inner)
        | zil_ir::Type::Managed(inner) => pay_type(inner, budget, span, depth + 1)?,
        zil_ir::Type::Tuple(fields) => {
            for field in fields {
                pay_type(field, budget, span, depth + 1)?;
            }
        }
        _ if ty.is_b0_scalar() => {}
        zil_ir::Type::Unit => {}
        _ => {
            return Err(unsupported(
                span,
                "dyn source clone needs a complete type visitor",
            ));
        }
    }
    Ok(())
}
