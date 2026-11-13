//! Complete class-free nominal catalogue entries. Plain layout is accepted
//! only after every nominal actual and physical child has a known zero domain.
//! This proves no Copy, ownership, cleanup, address permission or task state.
use super::*;

struct Check<'a, 'b> {
    declarations: &'a [LifeNominalDeclaration],
    instances: &'a [LifeNominalInstance],
    limits: &'b Limits,
    left: &'b mut usize,
    phase: InventoryPhase,
    active: Vec<usize>,
}
fn unknown(message: &str) -> Diagnostic {
    unsupported(Span::default(), message)
}
impl<'a> Check<'a, '_> {
    fn node(&mut self, depth: usize) -> R<()> {
        fee(self.left, 1, Span::default())?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "ordinary nominal domain depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn row(&mut self, instance: &'a LifeNominalInstance, depth: usize) -> R<()> {
        self.node(depth)?;
        let key = instance as *const LifeNominalInstance as usize;
        fee(
            self.left,
            self.active.len().saturating_add(1),
            Span::default(),
        )?;
        if self.active.contains(&key) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "ordinary nominal domain depends cyclically on its own asserted catalogue",
            ));
        }
        self.active.push(key);
        fee(self.left, self.declarations.len(), Span::default())?;
        let mut declarations = self
            .declarations
            .iter()
            .filter(|row| row.id == instance.declaration);
        let declaration = declarations.next().ok_or_else(|| {
            error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "ordinary nominal lacks its exact original declaration",
            )
        })?;
        if declarations.next().is_some() || declaration.parameters.len() != instance.arguments.len()
        {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "ordinary nominal declaration or full mixed actual inventory is ambiguous",
            ));
        }
        let physical = physical(&instance.ty);
        if declaration.id.compiler {
            if compiler_declaration(declaration) {
                compiler_instance(declaration, physical, self.left)?;
            } else if option_address::declaration(declaration)
                || enum_address::poll_declaration(declaration)
            {
                let Type::Enum { variants, .. } = physical else {
                    return Err(error(
                        "E-NOMINAL-DOMAIN",
                        Span::default(),
                        "compiler nominal lost its actual enum",
                    ));
                };
                let actual = match instance.arguments.as_slice() {
                    [LifeNominalActual::Type(ty)]
                    | [LifeNominalActual::QualifiedType { ty, .. }] => ty,
                    _ => {
                        return Err(error(
                            "E-NOMINAL-DOMAIN",
                            Span::default(),
                            "compiler nominal lost its whole Type actual",
                        ));
                    }
                };
                let (empty, full) = if option_address::declaration(declaration) {
                    ("None", "Some")
                } else {
                    ("Pending", "Ready")
                };
                fee(self.left, variants.len().saturating_add(2), Span::default())?;
                if variants.len() != 2
                    || variants[0].0 != empty
                    || !variants[0].1.is_empty()
                    || variants[1].0 != full
                    || variants[1].1.len() != 1
                {
                    return Err(error(
                        "E-NOMINAL-DOMAIN",
                        Span::default(),
                        "compiler nominal changes original alternatives or payload arity",
                    ));
                }
                inference::charge_type_snapshot(
                    actual,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                inference::charge_type_snapshot(
                    &variants[1].1[0],
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                if actual != &variants[1].1[0] {
                    return Err(error(
                        "E-NOMINAL-DOMAIN",
                        Span::default(),
                        "compiler nominal payload differs from its complete Type actual",
                    ));
                }
            } else {
                return Err(error(
                    "E-NOMINAL-DOMAIN",
                    Span::default(),
                    "compiler nominal has no complete canonical original declaration",
                ));
            }
        }
        if !matches!(
            (declaration.kind, physical),
            (
                LifeNominalKind::Record | LifeNominalKind::Resource,
                Type::Record { .. }
            ) | (LifeNominalKind::Package, Type::Package { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        ) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "ordinary nominal changes its actual declaration kind",
            ));
        }
        if let Type::BorrowedStorage { life, .. } = &instance.ty {
            fee(
                self.left,
                life.slots
                    .len()
                    .saturating_add(life.retained.len())
                    .saturating_add(life.nominal_arguments.len()),
                Span::default(),
            )?;
            if life.binder_count != 0
                || !life.slots.is_empty()
                || !life.retained.is_empty()
                || !life.nominal_arguments.is_empty()
            {
                return Err(error(
                    "E-NOMINAL-DOMAIN",
                    Span::default(),
                    "plain nominal child retains a nonzero complete qualified domain",
                ));
            }
            // Preserve and independently check an existing qualified N0;
            // never remove its nominal wrapper to manufacture a plain entry.
            if self.phase == InventoryPhase::Resolved {
                shape(
                    &instance.ty,
                    self.left,
                    self.limits.max_call_depth,
                    Span::default(),
                )?;
            }
        }
        for (formal, actual) in declaration.parameters.iter().zip(&instance.arguments) {
            self.node(depth + 1)?;
            match (formal, actual) {
                (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => self.ty(ty, depth + 1)?,
                (LifeParameterKind::Type, LifeNominalActual::QualifiedType { ty, life }) => {
                    fee(
                        self.left,
                        life.slots
                            .len()
                            .saturating_add(life.retained.len())
                            .saturating_add(life.nominal_arguments.len()),
                        Span::default(),
                    )?;
                    if life.binder_count != 0
                        || !life.slots.is_empty()
                        || !life.retained.is_empty()
                        || !life.nominal_arguments.is_empty()
                    {
                        return Err(error(
                            "E-NOMINAL-DOMAIN",
                            Span::default(),
                            "ordinary nominal Type actual has nonzero or incomplete intrinsic classes",
                        ));
                    }
                    if let Type::BorrowedStorage { life: actual, .. } = ty {
                        if actual != life {
                            return Err(error(
                                "E-NOMINAL-DOMAIN",
                                Span::default(),
                                "qualified Type actual contradicts its actual complete type",
                            ));
                        }
                    }
                    self.ty(ty, depth + 1)?;
                }
                (LifeParameterKind::Const(carrier), LifeNominalActual::Const(value)) => {
                    self.ty(carrier, depth + 1)?;
                    let actual = value.ty();
                    self.ty(&actual, depth + 1)?;
                    if &actual != carrier {
                        return Err(error(
                            "E-NOMINAL-DOMAIN",
                            Span::default(),
                            "ordinary nominal Const actual has another carrier",
                        ));
                    }
                    if let Constant::WideString(units) = value {
                        fee(self.left, units.len(), Span::default())?;
                    }
                }
                (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(_)) => {
                    return Err(error(
                        "E-NOMINAL-DOMAIN",
                        Span::default(),
                        "a declared phantom lifetime cannot disappear from a plain nominal",
                    ));
                }
                _ => {
                    return Err(error(
                        "E-NOMINAL-DOMAIN",
                        Span::default(),
                        "ordinary nominal changes a full mixed actual kind",
                    ));
                }
            }
        }
        self.fields(physical, depth + 1)?;
        self.active.pop();
        Ok(())
    }
    fn fields(&mut self, ty: &Type, depth: usize) -> R<()> {
        self.node(depth)?;
        match ty {
            Type::Record { name, fields } | Type::Package { name, fields } => {
                fee(
                    self.left,
                    name.len().saturating_add(fields.len()),
                    Span::default(),
                )?;
                for (name, field) in fields {
                    fee(self.left, name.len(), Span::default())?;
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Enum { name, variants } => {
                fee(
                    self.left,
                    name.len().saturating_add(variants.len()),
                    Span::default(),
                )?;
                for (name, fields) in variants {
                    fee(
                        self.left,
                        name.len().saturating_add(fields.len()),
                        Span::default(),
                    )?;
                    for field in fields {
                        self.ty(field, depth + 1)?;
                    }
                }
            }
            _ => {
                return Err(error(
                    "E-NOMINAL-DOMAIN",
                    Span::default(),
                    "ordinary nominal has no complete actual physical layout",
                ));
            }
        }
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> R<()> {
        self.node(depth)?;
        match ty {
            ty if ty.is_b0_scalar() => Ok(()),
            Type::TaskI32
            | Type::ByteBuffer
            | Type::ForeignHandle
            | Type::CWideString
            | Type::SyncCell(_) => Ok(()),
            Type::Named(name) if self.phase == InventoryPhase::Core => {
                // Structural-only before the real Resolver. It MUST resolve
                // and discharge at Resolved; this branch yields no Known0.
                fee(self.left, name.len(), Span::default())?;
                Ok(())
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
                Ok(())
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::OwnedStore { element }
            | Type::Mutex { payload: element } => {
                if matches!(ty, Type::Desc(_)) {
                    return Err(unknown(
                        "description occurrence has no complete zero-domain proof",
                    ));
                }
                self.ty(element, depth + 1)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.ty(parameter, depth + 1)?;
                }
                self.ty(result, depth + 1)
            }
            Type::BorrowedStorage { storage, life }
                if matches!(storage.as_ref(), Type::Tuple(_)) =>
            {
                fee(
                    self.left,
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len()),
                    Span::default(),
                )?;
                if life.binder_count != 0
                    || !life.slots.is_empty()
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                {
                    return Err(unknown(
                        "anonymous qualified child has a nonzero or incomplete domain",
                    ));
                }
                if self.phase == InventoryPhase::Resolved {
                    shape(ty, self.left, self.limits.max_call_depth, Span::default())?;
                }
                let Type::Tuple(fields) = storage.as_ref() else {
                    unreachable!()
                };
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
                Ok(())
            }
            Type::ErrorUnion { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)
            }
            Type::Task {
                result,
                retained_life,
            } => {
                fee(
                    self.left,
                    retained_life
                        .ports
                        .len()
                        .saturating_add(retained_life.retained.len())
                        .saturating_add(retained_life.nominal_arguments.len()),
                    Span::default(),
                )?;
                if retained_life.binder_count != 0
                    || !retained_life.ports.is_empty()
                    || !retained_life.retained.is_empty()
                    || !retained_life.nominal_arguments.is_empty()
                {
                    return Err(unknown("Task child has a nonzero complete context"));
                }
                self.ty(result, depth + 1)
            }
            Type::Record { .. }
            | Type::Package { .. }
            | Type::Enum { .. }
            | Type::BorrowedStorage { .. } => {
                fee(self.left, self.instances.len(), Span::default())?;
                let mut selected: Option<&LifeNominalInstance> = None;
                for row in self.instances {
                    inference::charge_type_snapshot(
                        &row.ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if &row.ty != ty {
                        continue;
                    }
                    if let Some(prior) = selected {
                        fee(
                            self.left,
                            row.arguments.len().saturating_add(prior.arguments.len()),
                            Span::default(),
                        )?;
                        for actual in row.arguments.iter().chain(&prior.arguments) {
                            match actual {
                                LifeNominalActual::Type(ty)
                                | LifeNominalActual::QualifiedType { ty, .. } => {
                                    inference::charge_type_snapshot(
                                        ty,
                                        Span::default(),
                                        self.limits,
                                        self.left,
                                        1,
                                    )?
                                }
                                LifeNominalActual::Const(Constant::WideString(units)) => {
                                    fee(self.left, units.len(), Span::default())?
                                }
                                _ => {}
                            }
                        }
                        if row.declaration != prior.declaration || row.arguments != prior.arguments
                        {
                            return Err(error(
                                "E-NOMINAL-DOMAIN",
                                Span::default(),
                                "nominal child type selects conflicting declaration/actual identities",
                            ));
                        }
                    } else {
                        selected = Some(row);
                    }
                }
                let row = selected.ok_or_else(|| {
                    unknown("nominal child has no complete original catalogue entry")
                })?;
                self.row(row, depth + 1)
            }
            // Missing opaque, callable, address, declaration or phantom
            // inventories are Unknown. Empty visible fields are no substitute.
            _ => Err(unknown(
                "ordinary nominal child has no complete known-zero persistent domain",
            )),
        }
    }
}

pub(super) fn rows(
    core: &CoreModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    left: &mut usize,
    phase: InventoryPhase,
) -> R<()> {
    Check {
        declarations: &core.life_declarations,
        instances: &core.life_instances,
        limits,
        left,
        phase,
        active: Vec::new(),
    }
    .row(instance, 0)
}

/// Mandatory global discharge after actual types and closure domains have
/// materialized. Raw Core name placeholders never reach this phase.
pub(super) fn module(module: &TypedModule, limits: &Limits, left: &mut usize) -> R<()> {
    fee(left, module.life_instances.len(), Span::default())?;
    let mut check = Check {
        declarations: &module.life_declarations,
        instances: &module.life_instances,
        limits,
        left,
        phase: InventoryPhase::Resolved,
        active: Vec::new(),
    };
    for row in &module.life_instances {
        if !matches!(row.ty, Type::BorrowedStorage { .. }) {
            check.row(row, 0)?;
        }
    }
    Ok(())
}

/// Rebuild the complete zero-domain fact for a current actual type. Used by
/// the semantic world after the mandatory original nominal field/where pass;
/// this returns no Copy, ownership, cleanup or access capability.
pub(super) fn zero_type(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    Check {
        declarations: &module.life_declarations,
        instances: &module.life_instances,
        limits,
        left,
        phase: InventoryPhase::Resolved,
        active: Vec::new(),
    }
    .ty(ty, 0)
}

/// Compiler IDs only select the full canonical declaration predicate. They
/// grant no worker, provider, frame or descriptor authority.
pub(super) fn compiler_declaration(d: &LifeNominalDeclaration) -> bool {
    let name = if d.id == zil_ir::builtin_life::TASK_CREATE_ERROR {
        "TaskCreateError"
    } else if d.id == zil_ir::builtin_life::TASK_POLL_STATE {
        "TaskPollState"
    } else {
        return false;
    };
    d.identity.package == "@zil.builtin"
        && d.identity.module_path == ["task"]
        && d.identity.declaration_path == ["enum", name]
        && d.identity.generation_path.is_empty()
        && d.kind == LifeNominalKind::Enum
        && d.parameters.is_empty()
}
fn compiler_instance(d: &LifeNominalDeclaration, ty: &Type, left: &mut usize) -> R<()> {
    if !compiler_declaration(d) {
        return Err(error(
            "E-NOMINAL-DOMAIN",
            Span::default(),
            "compiler nominal has no complete canonical declaration",
        ));
    }
    let Type::Enum { name, variants } = ty else {
        return Err(error(
            "E-NOMINAL-DOMAIN",
            Span::default(),
            "compiler closed nominal is not its actual enum",
        ));
    };
    let (raw, canonical, expected): (&str, &str, &[&str]) =
        if d.id == zil_ir::builtin_life::TASK_CREATE_ERROR {
            (
                "$stdTaskCreateError343",
                "@std.task:v1:TaskCreateError",
                &["Allocation"],
            )
        } else {
            (
                "$stdTaskPollState343",
                "@std.task:v1:TaskPollState",
                &["Complete", "Cancelled", "Invalid"],
            )
        };
    fee(
        left,
        name.len()
            .saturating_add(variants.len())
            .saturating_add(expected.len()),
        Span::default(),
    )?;
    if (name != raw && name != canonical) || variants.len() != expected.len() {
        return Err(error(
            "E-NOMINAL-DOMAIN",
            Span::default(),
            "compiler closed nominal changes its actual identity or tag count",
        ));
    }
    for ((name, fields), expected) in variants.iter().zip(expected) {
        fee(
            left,
            name.len().saturating_add(fields.len()),
            Span::default(),
        )?;
        if name != expected || !fields.is_empty() {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "compiler closed nominal changes its actual tags or payloads",
            ));
        }
    }
    Ok(())
}
