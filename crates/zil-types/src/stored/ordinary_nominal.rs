//! Complete class-free nominal catalogue entries. Plain layout is accepted
//! only after every nominal actual and physical child has a known zero domain.
//! This proves no Copy, ownership, cleanup, address permission or task state.
use super::*;

struct Check<'a, 'b> {
    module: Option<&'a TypedModule>,
    declarations: &'a [LifeNominalDeclaration],
    instances: &'a [LifeNominalInstance],
    limits: &'b Limits,
    left: &'b mut usize,
    phase: InventoryPhase,
    active: Vec<usize>,
    active_closures: Vec<ClosureId>,
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
            if declaration.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
                || declaration.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
            {
                self.failure_instance(declaration, instance, depth + 1)?;
            } else if compiler_declaration(declaration) {
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
    fn failure_instance(
        &mut self,
        declaration: &LifeNominalDeclaration,
        instance: &LifeNominalInstance,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if !compiler_declaration(declaration) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure original declaration is not canonical",
            ));
        }
        fn get_type(actual: &LifeNominalActual) -> Option<&Type> {
            match actual {
                LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. } => {
                    Some(ty)
                }
                _ => None,
            }
        }
        let [inputs, context] = instance.arguments.as_slice() else {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure omits full input/context Type actuals",
            ));
        };
        let inputs = get_type(inputs).ok_or_else(|| {
            error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure input actual has another mixed kind",
            )
        })?;
        let context = get_type(context).ok_or_else(|| {
            error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure context actual has another mixed kind",
            )
        })?;
        if !matches!(inputs, Type::Tuple(_)) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure input actual is not its whole ordered Tuple",
            ));
        }
        let Type::Task {
            result,
            retained_life,
        } = context
        else {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure context actual is not its actual Task context",
            ));
        };
        fee(
            self.left,
            retained_life
                .ports
                .len()
                .saturating_add(retained_life.retained.len())
                .saturating_add(retained_life.nominal_arguments.len())
                .saturating_add(1),
            Span::default(),
        )?;
        if **result != Type::Unit
            || retained_life.binder_count != 0
            || !retained_life.ports.is_empty()
            || !retained_life.retained.is_empty()
            || !retained_life.nominal_arguments.is_empty()
        {
            return Err(unknown(
                "nonzero TaskCreateFailure context requires its full qualified input/port/phantom projection proof",
            ));
        }
        let (name, fields) = match (declaration.id, physical(&instance.ty)) {
            (id, Type::Record { name, fields })
                if id == zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD =>
            {
                (name, fields)
            }
            (id, Type::Package { name, fields })
                if id == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE =>
            {
                (name, fields)
            }
            _ => {
                return Err(error(
                    "E-NOMINAL-DOMAIN",
                    Span::default(),
                    "TaskCreateFailure changes its actual Record/Package ownership constructor",
                ));
            }
        };
        fee(
            self.left,
            name.len().saturating_add(fields.len()),
            Span::default(),
        )?;
        let [(reason_name, reason), (inputs_name, actual_inputs)] = fields.as_slice() else {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure changes its exact field inventory",
            ));
        };
        if reason_name != "reason" || inputs_name != "inputs" {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure changes field order or identity",
            ));
        }
        for ty in [inputs, actual_inputs] {
            inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 2)?;
        }
        if inputs != actual_inputs
            || name != &format!("@std.task:v1:TaskCreateFailure:{}", inputs.fingerprint())
        {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure changes its complete input payload or canonical application identity",
            ));
        }
        fee(self.left, self.declarations.len(), Span::default())?;
        let mut declarations = self
            .declarations
            .iter()
            .filter(|row| row.id == zil_ir::builtin_life::TASK_CREATE_ERROR);
        let reason_declaration = declarations.next().ok_or_else(|| {
            error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure has no actual canonical reason declaration",
            )
        })?;
        if declarations.next().is_some() {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "TaskCreateFailure reason declaration is duplicated",
            ));
        }
        compiler_instance(reason_declaration, reason, self.left)?;
        // The ordinary row pass independently walks both full actuals and all
        // physical fields. No omitted class/phantom or unknown child is zero.
        self.ty(inputs, depth + 1)?;
        self.ty(context, depth + 1)
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
    fn closure(&mut self, id: ClosureId, depth: usize) -> R<()> {
        self.node(depth)?;
        let module = self.module.ok_or_else(|| unknown(
            "Core closure placeholder requires its actual emitted definition and complete domain",
        ))?;
        fee(
            self.left,
            module
                .closures
                .len()
                .saturating_add(module.functions.len())
                .saturating_add(self.active_closures.len()),
            Span::default(),
        )?;
        if self.active_closures.contains(&id) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "closure domain depends cyclically on its own asserted environment",
            ));
        }
        let mut definitions = module.closures.iter().filter(|d| d.id == id);
        let definition = definitions
            .next()
            .ok_or_else(|| unknown("nominal closure child has no actual definition"))?;
        if definitions.next().is_some() {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "nominal closure child has ambiguous actual identity",
            ));
        }
        let life = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| unknown("nominal closure child has unknown complete domain"))?;
        if life.fixed_binder_count != 0 {
            return Err(unknown(
                "nominal closure child retains a nonzero complete environment",
            ));
        }
        let mut entries = module
            .functions
            .iter()
            .filter(|f| f.id == definition.function);
        let entry = entries
            .next()
            .ok_or_else(|| unknown("nominal closure child has no actual entry"))?;
        if entries.next().is_some() || entry.environment != Some(id) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "nominal closure child has a different actual entry environment",
            ));
        }
        // Pay both complete contracts before equality. Invocation P/R rows
        // describe each call, while the environment's persistent domain is
        // rebuilt independently from every capture below.
        for contract in [Some(life), entry.storage_life.as_ref()]
            .into_iter()
            .flatten()
        {
            fee(self.left, 1, Span::default())?;
            for binding in &contract.bindings {
                fee(
                    self.left,
                    binding.position.path.len().saturating_add(1),
                    Span::default(),
                )?;
            }
            for position in &contract.static_bindings {
                fee(
                    self.left,
                    position.path.len().saturating_add(1),
                    Span::default(),
                )?;
            }
            for class in &contract.classes {
                fee(
                    self.left,
                    class.position.path.len().saturating_add(1),
                    Span::default(),
                )?;
            }
        }
        if entry.storage_life.as_ref() != Some(life) {
            return Err(error(
                "E-NOMINAL-DOMAIN",
                Span::default(),
                "closure and actual entry disagree about the complete domain",
            ));
        }
        for binding in &life.bindings {
            if matches!(binding.position.base, StorageBase::Capture(_))
                || matches!(binding.binder, StorageBinder::Fixed(_))
            {
                return Err(error(
                    "E-NOMINAL-DOMAIN",
                    Span::default(),
                    "zero-domain closure has an unaccounted variable capture class",
                ));
            }
        }
        for binding in &life.classes {
            if matches!(binding.position.base, StorageBase::Capture(_))
                || matches!(binding.binder, StorageBinder::Fixed(_))
            {
                return Err(error(
                    "E-NOMINAL-DOMAIN",
                    Span::default(),
                    "zero-domain closure has an unaccounted intrinsic capture class",
                ));
            }
        }
        self.active_closures.push(id);
        fee(self.left, definition.captures.len(), Span::default())?;
        for (index, capture) in definition.captures.iter().enumerate() {
            fee(self.left, life.static_bindings.len(), Span::default())?;
            let static_occurrences = life
                .static_bindings
                .iter()
                .filter(|position| {
                    position.base == StorageBase::Capture(index as u32) && position.path.is_empty()
                })
                .count();
            match capture {
                Type::Address {
                    mutable: false,
                    inner,
                } if static_occurrences == 1 => self.ty(inner, depth + 1)?,
                _ if static_occurrences == 0 => self.ty(capture, depth + 1)?,
                _ => {
                    return Err(error(
                        "E-NOMINAL-DOMAIN",
                        Span::default(),
                        "static capture does not select one immutable complete borrow occurrence",
                    ));
                }
            }
        }
        for position in &life.static_bindings {
            if let StorageBase::Capture(index) = position.base {
                if !position.path.is_empty()
                    || !matches!(
                        definition.captures.get(index as usize),
                        Some(Type::Address { mutable: false, .. })
                    )
                {
                    return Err(unknown(
                        "nested static capture requires its complete occurrence and nominal-class projection",
                    ));
                }
            }
        }
        self.active_closures.pop();
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> R<()> {
        self.node(depth)?;
        match ty {
            Type::Closure(id) => self.closure(*id, depth + 1),
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
                if canonical_nullary_enum(ty, self.limits, self.left)? {
                    return Ok(());
                }
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
                                LifeNominalActual::Type(ty) => inference::charge_type_snapshot(
                                    ty,
                                    Span::default(),
                                    self.limits,
                                    self.left,
                                    1,
                                )?,
                                LifeNominalActual::QualifiedType { ty, life } => {
                                    inference::charge_type_snapshot(
                                        ty,
                                        Span::default(),
                                        self.limits,
                                        self.left,
                                        1,
                                    )?;
                                    fee(self.left, 1, Span::default())?;
                                    for slot in &life.slots {
                                        self.node(slot.path.len())?;
                                        fee(self.left, slot.path.len(), Span::default())?;
                                    }
                                    for row in &life.retained {
                                        self.node(row.path.len())?;
                                        fee(self.left, row.path.len(), Span::default())?;
                                    }
                                    for row in &life.nominal_arguments {
                                        self.node(row.path.len())?;
                                        fee(self.left, row.path.len(), Span::default())?;
                                    }
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
                let Some(row) = selected else {
                    if self.phase == InventoryPhase::Core {
                        // CT may already have expanded an imported whole Type
                        // actual, while this Core module contains only its own
                        // declaration directory. This stage checks structure;
                        // it cannot prove the imported nominal's domain zero.
                        // The mandatory merged Typed-module pass below must
                        // select its exact owner/actual row and discharge it.
                        inference::charge_type_snapshot(
                            ty,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                        return self.fields(physical(ty), depth + 1);
                    }
                    let name = match physical(ty) {
                        Type::Record { name, .. }
                        | Type::Package { name, .. }
                        | Type::Enum { name, .. } => name.as_str(),
                        _ => "qualified compound",
                    };
                    inference::charge_type_snapshot(
                        ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        2,
                    )?;
                    fee(
                        self.left,
                        name.len().saturating_add(self.instances.len()),
                        Span::default(),
                    )?;
                    let mut same_name = Vec::new();
                    for row in self.instances {
                        let other = match physical(&row.ty) {
                            Type::Record { name, .. }
                            | Type::Package { name, .. }
                            | Type::Enum { name, .. } => name.as_str(),
                            _ => continue,
                        };
                        fee(
                            self.left,
                            name.len().saturating_add(other.len()),
                            Span::default(),
                        )?;
                        if other == name {
                            inference::charge_type_snapshot(
                                &row.ty,
                                Span::default(),
                                self.limits,
                                self.left,
                                2,
                            )?;
                            same_name.push((row.declaration, &row.ty));
                        }
                    }
                    return Err(unknown(&format!(
                        "nominal child has no complete original catalogue entry: {name}; requested {ty:?}; same-name catalogue {same_name:?}"
                    )));
                };
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

/// The two sealed error types have a canonical, complete nullary Type
/// identity rather than a user nominal application. This predicate checks the
/// entire language carrier and supplies no constructor/operation permission.
pub(super) fn canonical_nullary_enum(ty: &Type, limits: &Limits, left: &mut usize) -> R<bool> {
    if !matches!(ty, Type::Enum { .. }) {
        return Ok(false);
    }
    inference::charge_type_snapshot(ty, Span::default(), limits, left, 2)?;
    fee(left, 128, Span::default())?;
    Ok(ty == &crate::generic_mutex::create_error() || ty == &crate::dyn_dispatch::create_error())
}

pub(super) fn rows(
    core: &CoreModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    left: &mut usize,
    phase: InventoryPhase,
) -> R<()> {
    Check {
        module: None,
        declarations: &core.life_declarations,
        instances: &core.life_instances,
        limits,
        left,
        phase,
        active: Vec::new(),
        active_closures: Vec::new(),
    }
    .row(instance, 0)
}

/// Mandatory global discharge after actual types and closure domains have
/// materialized. Raw Core name placeholders never reach this phase.
pub(super) fn module(module: &TypedModule, limits: &Limits, left: &mut usize) -> R<()> {
    fee(left, module.life_instances.len(), Span::default())?;
    let mut check = Check {
        module: Some(module),
        declarations: &module.life_declarations,
        instances: &module.life_instances,
        limits,
        left,
        phase: InventoryPhase::Resolved,
        active: Vec::new(),
        active_closures: Vec::new(),
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
        module: Some(module),
        declarations: &module.life_declarations,
        instances: &module.life_instances,
        limits,
        left,
        phase: InventoryPhase::Resolved,
        active: Vec::new(),
        active_closures: Vec::new(),
    }
    .ty(ty, 0)
}

/// Compiler IDs only select the full canonical declaration predicate. They
/// grant no worker, provider, frame or descriptor authority.
pub(super) fn compiler_declaration(d: &LifeNominalDeclaration) -> bool {
    if d.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
        || d.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
    {
        let package = d.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE;
        return d.identity.package == "@zil.builtin"
            && d.identity.module_path == ["task"]
            && d.identity.declaration_path
                == [
                    if package { "package" } else { "record" },
                    "TaskCreateFailure",
                ]
            && d.identity.generation_path.is_empty()
            && d.kind
                == if package {
                    LifeNominalKind::Package
                } else {
                    LifeNominalKind::Record
                }
            && d.parameters == [LifeParameterKind::Type, LifeParameterKind::Type];
    }
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
    if d.id != zil_ir::builtin_life::TASK_CREATE_ERROR
        && d.id != zil_ir::builtin_life::TASK_POLL_STATE
    {
        return Err(unknown(
            "TaskCreateFailure instance requires its complete input/context nominal and actual constructor proof",
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
