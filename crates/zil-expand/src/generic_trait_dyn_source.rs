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
                    parameters.push(lower_type(&parameter.ty, budget)?);
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
                let mut parameter_lives = vec![Some(0)];
                let mut bindings = vec![zil_ir::StorageLifeBinding {
                    position: zil_ir::StoragePosition {
                        base: zil_ir::StorageBase::Parameter(0),
                        path: vec![],
                    },
                    binder: zil_ir::StorageBinder::Invocation(0),
                }];
                let mut binder_count = 1u32;
                for (index, parameter) in parameters.iter().enumerate() {
                    budget.node(slot.signature.span, 0)?;
                    if matches!(parameter, zil_ir::Type::Address { .. }) {
                        parameter_lives.push(Some(binder_count));
                        bindings.push(zil_ir::StorageLifeBinding {
                            position: zil_ir::StoragePosition {
                                base: zil_ir::StorageBase::Parameter(
                                    u32::try_from(index + 1).map_err(|_| {
                                        error(
                                            slot.signature.span,
                                            "dyn parameter ordinal exhausted",
                                        )
                                    })?,
                                ),
                                path: vec![],
                            },
                            binder: zil_ir::StorageBinder::Invocation(binder_count),
                        });
                        binder_count = binder_count.checked_add(1).ok_or_else(|| {
                            error(
                                slot.signature.span,
                                "dyn invocation binder ordinal exhausted",
                            )
                        })?;
                    } else {
                        parameter_lives.push(None);
                    }
                }
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
                    invocation_life: Some(zil_ir::FunctionLifeContract {
                        binder_count,
                        parameter_lives,
                        result_life: None,
                    }),
                    storage_life: Some(zil_ir::StorageLifeContract {
                        fixed_binder_count: 0,
                        invocation_binder_count: binder_count,
                        bindings,
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
            actuals.push(DynGenericActual::Type(lower_type(actual, budget)?));
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
