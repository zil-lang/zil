//! Resolution and construction of finite Copy records and tuples.
use super::*;
use std::collections::BTreeSet;
use zil_base::ModuleId;

/// Visibility is checked on the actual resolved field indices. It is a source
/// module restriction, not a runtime capability or a change to record layout.
pub(super) struct Visibility<'a> {
    records: BTreeMap<String, (ModuleId, &'a [CoreField])>,
    generic_records: BTreeMap<String, (ModuleId, &'a [CoreAggregateFieldRecipe])>,
    remaining: usize,
}
impl<'a> Visibility<'a> {
    pub(super) fn new(program: &'a CoreProgram, limits: &Limits) -> CompileResult<Self> {
        let mut result = Self {
            records: BTreeMap::new(),
            generic_records: BTreeMap::new(),
            remaining: limits.max_analysis_steps,
        };
        for module in &program.modules {
            for template in &module.core.aggregate_templates {
                result
                    .spend(
                        template
                            .name
                            .len()
                            .saturating_add(template.fields.len())
                            .saturating_add(96),
                        template.span,
                    )
                    .map_err(|e| vec![e])?;
                result.generic_records.insert(
                    format!(
                        "@module{}:product-template{}",
                        module.core.module_id.0, template.id.0
                    ),
                    (module.core.module_id, &template.fields),
                );
            }
            for record in &module.core.aggregates {
                result
                    .spend(
                        record.name.len().saturating_add(record.fields.len()),
                        record.span,
                    )
                    .map_err(|e| vec![e])?;
                result.records.insert(
                    format!("@module{}:{}", module.core.module_id.0, record.name),
                    (module.core.module_id, &record.fields),
                );
            }
        }
        Ok(result)
    }
    fn spend(&mut self, count: usize, span: Span) -> R<()> {
        self.remaining = self.remaining.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "record visibility budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    fn field(&mut self, ty: &Type, field: u32, module: ModuleId, span: Span) -> R<()> {
        let ty = match ty {
            Type::Managed(p) | Type::Desc(p) => p.as_ref(),
            other => other,
        };
        let ty = stored::physical(ty);
        let (Type::Record { name, .. } | Type::Package { name, .. }) = ty else {
            return Ok(());
        };
        self.spend(name.len(), span)?;
        if !self.generic_records.is_empty()
            && let Some((family, _)) = name.rsplit_once(':')
        {
            self.spend(family.len().saturating_add(1), span)?;
            if let Some((owner, fields)) = self.generic_records.get(family) {
                if *owner != module && fields.get(field as usize).is_some_and(|f| !f.public) {
                    return Err(error(
                        "E-FIELD-PRIVATE",
                        span,
                        "record field is private to its declaring module",
                    ));
                }
                return Ok(());
            }
        }
        if !self.records.contains_key(name) {
            self.spend(
                self.records
                    .keys()
                    .map(|key| key.len().saturating_add(1))
                    .sum(),
                span,
            )?;
        }
        let record = self.records.get(name).or_else(|| {
            self.records
                .iter()
                .find(|(key, _)| {
                    callable_fields::nominal_base(key) == callable_fields::nominal_base(name)
                })
                .map(|(_, record)| record)
        });
        let Some((owner, fields)) = record else {
            return Ok(());
        };
        if *owner == module {
            return Ok(());
        }
        if fields.get(field as usize).is_some_and(|f| !f.public) {
            return Err(error(
                "E-FIELD-PRIVATE",
                span,
                "record field is private to its declaring module",
            ));
        }
        Ok(())
    }
    fn place_fields(
        &mut self,
        root: &Type,
        path: impl IntoIterator<Item = PlaceProjection>,
        module: ModuleId,
        span: Span,
    ) -> R<()> {
        let mut ty = root;
        for step in path {
            self.spend(1, span)?;
            ty = match (stored::physical(ty), step) {
                (Type::Tuple(fields), PlaceProjection::Field(index)) => {
                    match fields.get(index as usize) {
                        Some(next) => next,
                        None => break,
                    }
                }
                (
                    record @ (Type::Record { .. } | Type::Package { .. }),
                    PlaceProjection::Field(index),
                ) => {
                    self.field(record, index, module, span)?;
                    match record {
                        Type::Record { fields, .. } | Type::Package { fields, .. } => {
                            match fields.get(index as usize) {
                                Some((_, next)) => next,
                                None => break,
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(slot)) => match variants
                    .get(slot.variant as usize)
                    .and_then(|(_, fields)| fields.get(slot.index as usize))
                {
                    Some(next) => next,
                    None => break,
                },
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
                    if is_error {
                        error
                    } else {
                        result
                    }
                }
                _ => break,
            };
        }
        Ok(())
    }
    pub(super) fn check(
        &mut self,
        function: &TypedFunction,
        module: ModuleId,
        internal: &BTreeMap<FunctionId, BTreeMap<(BlockId, usize), InstructionKind>>,
    ) -> CompileResult<()> {
        if !internal.is_empty() {
            self.spend(
                internal.len().saturating_add(1).ilog2() as usize + 1,
                function.span,
            )
            .map_err(|e| vec![e])?;
        }
        let internal = internal.get(&function.id);
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                self.spend(1, instruction.span).map_err(|e| vec![e])?;
                if let Some(internal) = internal {
                    self.spend(
                        internal.len().saturating_add(1).ilog2() as usize + 1,
                        instruction.span,
                    )
                    .map_err(|e| vec![e])?;
                    if internal.get(&(block.id, index)) == Some(&instruction.kind) {
                        continue;
                    }
                }
                match &instruction.kind {
                    InstructionKind::BorrowFieldLocal { local, path, .. } => {
                        if let Some(root) = function.locals.get(local.0 as usize) {
                            self.place_fields(
                                &root.ty,
                                path.iter().copied().map(PlaceProjection::Field),
                                module,
                                instruction.span,
                            )
                            .map_err(|e| vec![e])?;
                        }
                    }
                    InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                        if let Some(root) = function.locals.get(local.0 as usize) {
                            self.place_fields(
                                &root.ty,
                                path.iter().copied(),
                                module,
                                instruction.span,
                            )
                            .map_err(|e| vec![e])?;
                        }
                    }
                    _ => {}
                }
                let access = match &instruction.kind {
                    InstructionKind::ExtractField { aggregate, index }
                    | InstructionKind::UpdateField {
                        aggregate, index, ..
                    } => function
                        .values
                        .get(aggregate.0 as usize)
                        .map(|t| (t, *index)),
                    InstructionKind::ReadDesc { desc, index }
                    | InstructionKind::WriteDesc { desc, index, .. } => {
                        function.values.get(desc.0 as usize).map(|ty| (ty, *index))
                    }
                    InstructionKind::AddressField { address, index } => {
                        match function.values.get(address.0 as usize) {
                            Some(Type::Address { inner, .. }) => Some((inner.as_ref(), *index)),
                            _ => None,
                        }
                    }
                    InstructionKind::LoadStoredBorrow { storage, path, .. }
                    | InstructionKind::StoreStoredBorrow { storage, path, .. } => {
                        match (function.values.get(storage.0 as usize), path.first()) {
                            (
                                Some(Type::Address { inner, .. }),
                                Some(PlaceProjection::Field(index)),
                            ) => Some((inner.as_ref(), *index)),
                            _ => None,
                        }
                    }
                    InstructionKind::ReadPackageField { local, index }
                    | InstructionKind::DescribePackageField { local, index }
                    | InstructionKind::TakePackageField { local, index }
                    | InstructionKind::DropPackageField { local, index }
                    | InstructionKind::StorePackageField { local, index, .. }
                    | InstructionKind::BorrowPackageField { local, index, .. } => function
                        .locals
                        .get(local.0 as usize)
                        .map(|local| (&local.ty, *index)),
                    InstructionKind::ProjectOwnedArrayField { local, field, .. } => function
                        .locals
                        .get(local.0 as usize)
                        .and_then(|local| match &local.ty {
                            Type::Array { element, .. } => Some((element.as_ref(), *field)),
                            _ => None,
                        }),
                    _ => None,
                };
                if let Some((ty, field)) = access {
                    self.field(ty, field, module, instruction.span)
                        .map_err(|e| vec![e])?;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn resolve(core: CoreModule, limits: &Limits) -> CompileResult<CoreModule> {
    let mut remaining = limits.max_analysis_steps;
    resolve_indexed(core, &[], limits, &mut remaining)
}

pub(super) fn project_symbol(name: &str) -> bool {
    name.starts_with("$module")
        || name.starts_with("$generic_$module")
        || name.starts_with("$generic_$builtinOption340_")
        || name.starts_with("$generic_$stdTaskPoll343_")
}

// Share an actual lifetime-bearing declaration through its complete inventory,
// rather than recognizing a generated spelling as storage authority.
fn project_stored_declaration(core: &CoreModule, name: &str, remaining: &mut usize) -> R<bool> {
    for instance in &core.life_instances {
        aggregate_templates::spend(remaining, 1, Span::default())?;
        let Type::BorrowedStorage { storage, .. } = &instance.ty else {
            continue;
        };
        let actual = match storage.as_ref() {
            Type::Record { name, .. } | Type::Package { name, .. } => {
                &stored::aggregate(&core.aggregates, core.module_id, name, remaining)?.name
            }
            Type::Enum { name, .. } => {
                &stored::enum_from(&core.enums, core.module_id, name, remaining)?.name
            }
            _ => continue,
        };
        aggregate_templates::spend(
            remaining,
            actual.len().saturating_add(name.len()),
            Span::default(),
        )?;
        if actual == name {
            return Ok(true);
        }
    }
    Ok(false)
}

// Project specialization emits the sealed task prelude once, in its actual
// declaration module. Other modules may reference this ordinary error type in
// a public factory header. Import its complete declaration, not an unresolved
// name or a general permission for resource error-union payloads.
fn shared_task_create_error(declaration: &CoreEnum, remaining: &mut usize) -> R<bool> {
    if declaration.name != "$stdTaskCreateError343" {
        return Ok(false);
    }
    aggregate_templates::spend(remaining, 32, declaration.span)?;
    if !matches!(declaration.variants.as_slice(), [(name, fields)] if name == "Allocation" && fields.is_empty())
    {
        return Err(error(
            "E-TASK-CREATE-ERROR-TYPE",
            declaration.span,
            "shared TaskCreateError requires its exact Allocation declaration",
        ));
    }
    Ok(true)
}

/// Resolve project syntax names against actual declarations, not a parsed
/// module number in a generated string. Nominal identity belongs to the module
/// which contains the declaration; importer aliases never participate.
pub(super) fn resolve_program(
    program: &CoreProgram,
    limits: &Limits,
) -> CompileResult<CoreProgram> {
    let mut remaining = limits.max_analysis_steps;
    let modules = program.modules.iter().map(|m| &m.core).collect::<Vec<_>>();
    let mut normalized = Vec::new();
    for module in &program.modules {
        inference::unconsumed_inventory(&module.core).map_err(|e| vec![e])?;
        let work = inference::life_inventory_snapshot_work(
            &module.core,
            &Limits {
                max_analysis_steps: remaining,
                ..limits.clone()
            },
        )
        .map_err(|e| vec![e])?;
        remaining = remaining.checked_sub(work).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Types,
                "stored inventory snapshot budget exceeded",
                None,
            )]
        })?;
        let snapshot = inference::snapshot_context_module(&module.core, limits, &mut remaining)
            .map_err(|e| vec![e])?;
        let core = resolve_indexed(snapshot, &modules, limits, &mut remaining)?;
        normalized.push(ProgramModule {
            core,
            imports: module.imports.clone(),
            exports: module.exports.clone(),
        });
    }
    Ok(CoreProgram {
        root: program.root,
        source_fingerprint: program.source_fingerprint,
        modules: normalized,
    })
}

/// The prepared input is immutable and already bounded by current inference.
/// A borrowed result still performs shape resolution's complete ordered work;
/// it carries no Types, Analyze or Verify capability.
pub(super) fn resolve_prepared_program<'a>(
    program: &'a CoreProgram,
    limits: &Limits,
) -> CompileResult<std::borrow::Cow<'a, CoreProgram>> {
    match scalar_shapes::validate(program, limits) {
        Some(result) => result.map(|()| std::borrow::Cow::Borrowed(program)),
        // Unsupported shapes restart with the original allowance. No partial
        // speculative work changes the existing resolver's diagnostic order.
        None => resolve_program(program, limits).map(std::borrow::Cow::Owned),
    }
}

fn resolve_indexed(
    mut core: CoreModule,
    project: &[&CoreModule],
    limits: &Limits,
    remaining: &mut usize,
) -> CompileResult<CoreModule> {
    stored::inventory(&core, remaining, limits).map_err(|e| vec![e])?;
    let mut declarations = BTreeMap::new();
    for d in &core.aggregates {
        if d.kind != AggregateKind::Package && d.fields.iter().any(|f| f.manages) {
            return Err(vec![unsupported(
                d.span,
                "manages fields require a package",
            )]);
        }
        if d.kind == AggregateKind::Package && d.teardown.is_some() {
            return Err(vec![unsupported(
                d.span,
                "package requires at least one manages field and has no custom whole Drop",
            )]);
        }
        if declarations.insert(d.name.clone(), d).is_some() {
            return Err(vec![error(
                "E-NAME-DUPLICATE",
                d.span,
                "duplicate aggregate name",
            )]);
        }
        let mut names = BTreeSet::new();
        for f in &d.fields {
            if !names.insert(&f.name) {
                return Err(vec![error(
                    "E-FIELD-DUPLICATE",
                    f.span,
                    "duplicate declared field",
                )]);
            }
        }
    }
    let mut enum_declarations = BTreeMap::new();
    for declaration in &core.enums {
        if declarations.contains_key(&declaration.name)
            || enum_declarations
                .insert(declaration.name.clone(), declaration)
                .is_some()
        {
            return Err(vec![error(
                "E-NAME-DUPLICATE",
                declaration.span,
                "duplicate type name",
            )]);
        }
        let mut variants = BTreeSet::new();
        for (name, _) in &declaration.variants {
            if !variants.insert(name) {
                return Err(vec![error(
                    "E-ENUM-VARIANT-DUPLICATE",
                    declaration.span,
                    "duplicate enum variant",
                )]);
            }
        }
    }
    for d in &core.aggregates {
        for f in &d.fields {
            if core
                .functions
                .iter()
                .any(|method| method.name == format!("{}.{}", d.name, f.name))
            {
                return Err(vec![error(
                    "E-RECEIVER-MEMBER-CONFLICT",
                    f.span,
                    "field and inherent method share a name",
                )]);
            }
        }
    }
    let mut owners = declarations
        .keys()
        .chain(enum_declarations.keys())
        .map(|name| (name.clone(), core.module_id.0))
        .collect::<BTreeMap<_, _>>();
    for module in project {
        if module.module_id == core.module_id {
            continue;
        }
        for declaration in &module.aggregates {
            *remaining = remaining
                .checked_sub(declaration.name.len().saturating_add(1))
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Types,
                        "project type index budget exceeded",
                        Some(declaration.span),
                    )]
                })?;
            if project_symbol(&declaration.name)
                || project_stored_declaration(module, &declaration.name, remaining)
                    .map_err(|e| vec![e])?
            {
                if owners
                    .insert(declaration.name.clone(), module.module_id.0)
                    .is_some()
                {
                    return Err(vec![error(
                        "E-NAME-DUPLICATE",
                        declaration.span,
                        "duplicate project type identity",
                    )]);
                }
                declarations.insert(declaration.name.clone(), declaration);
            }
        }
        for declaration in &module.enums {
            *remaining = remaining
                .checked_sub(declaration.name.len().saturating_add(1))
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Types,
                        "project type index budget exceeded",
                        Some(declaration.span),
                    )]
                })?;
            let shared_task_error =
                shared_task_create_error(declaration, remaining).map_err(|e| vec![e])?;
            if shared_task_error {
                if declarations.contains_key(&declaration.name) {
                    return Err(vec![error(
                        "E-NAME-DUPLICATE",
                        declaration.span,
                        "shared TaskCreateError conflicts with an aggregate declaration",
                    )]);
                }
                if let Some(existing) = enum_declarations.get(&declaration.name) {
                    shared_task_create_error(existing, remaining).map_err(|e| vec![e])?;
                    // Repeated exact prelude declarations have the same sealed
                    // semantic identity; unrelated project names remain unique.
                    continue;
                }
            }
            if shared_task_error
                || project_symbol(&declaration.name)
                || project_stored_declaration(module, &declaration.name, remaining)
                    .map_err(|e| vec![e])?
                || stored::project_option_enum(module, declaration, remaining, limits)
                    .map_err(|e| vec![e])?
            {
                if owners
                    .insert(declaration.name.clone(), module.module_id.0)
                    .is_some()
                {
                    return Err(vec![error(
                        "E-NAME-DUPLICATE",
                        declaration.span,
                        "duplicate project type identity",
                    )]);
                }
                enum_declarations.insert(declaration.name.clone(), declaration);
            }
        }
    }
    let mut generic_modules = Vec::new();
    if !core.aggregate_templates.is_empty()
        || !core.aggregate_applications.is_empty()
        || project
            .iter()
            .any(|m| !m.aggregate_templates.is_empty() || !m.aggregate_applications.is_empty())
    {
        aggregate_templates::spend(remaining, project.len().saturating_add(1), Span::default())
            .map_err(|e| vec![e])?;
        generic_modules.push(&core);
        generic_modules.extend(
            project
                .iter()
                .copied()
                .filter(|m| m.module_id != core.module_id),
        );
    }
    let generic = aggregate_templates::Index::new(&generic_modules, limits, remaining)
        .map_err(|e| vec![e])?;
    let mut stored_instances = BTreeMap::new();
    for (inventory, aggregates, enums, module) in std::iter::once((
        core.life_instances.as_slice(),
        core.aggregates.as_slice(),
        core.enums.as_slice(),
        core.module_id,
    ))
    .chain(
        project
            .iter()
            .copied()
            .filter(|source| source.module_id != core.module_id)
            .map(|source| {
                (
                    source.life_instances.as_slice(),
                    source.aggregates.as_slice(),
                    source.enums.as_slice(),
                    source.module_id,
                )
            }),
    ) {
        for instance in inventory {
            let Type::BorrowedStorage { storage, .. } = &instance.ty else {
                continue;
            };
            let name = match storage.as_ref() {
                Type::Record { name, .. }
                | Type::Package { name, .. }
                | Type::Enum { name, .. } => name,
                _ => continue,
            };
            aggregate_templates::spend(
                remaining,
                name.len()
                    .saturating_add(1)
                    .saturating_mul(stored_instances.len().saturating_add(1)),
                Span::default(),
            )
            .map_err(|e| vec![e])?;
            let actual_name = match storage.as_ref() {
                Type::Record { .. } | Type::Package { .. } => {
                    stored::aggregate(aggregates, module, name, remaining)
                        .map_err(|e| vec![e])?
                        .name
                        .as_str()
                }
                Type::Enum { .. } => stored::enum_from(enums, module, name, remaining)
                    .map_err(|e| vec![e])?
                    .name
                    .as_str(),
                _ => unreachable!(),
            };
            if stored_instances.insert(actual_name, instance).is_some() {
                return Err(vec![error(
                    "E-STORED-IDENTITY",
                    Span::default(),
                    "duplicate project stored instance",
                )]);
            }
        }
    }
    let mut resolver = Resolver {
        stored_instances,
        generic,
        generic_types: BTreeMap::new(),
        generic_active: BTreeSet::new(),
        current_owner: None,
        metadata_context: true,
        generic_limits: limits.clone(),
        ordinary_parameter: None,
        active_callable_parameters: &[],
        active_state_parameters: &[],
        callable_names: core
            .functions
            .iter()
            .flat_map(|f| f.callable_parameters.iter().map(|b| b.generic_name.clone()))
            .chain(
                project
                    .iter()
                    .flat_map(|m| m.functions.iter())
                    .flat_map(|f| f.callable_parameters.iter().map(|b| b.generic_name.clone())),
            )
            .chain(
                core.functions
                    .iter()
                    .flat_map(|f| f.state_parameters.iter())
                    .flat_map(|state| {
                        state
                            .bindings
                            .iter()
                            .map(|binding| binding.generic_name.clone())
                    }),
            )
            .chain(
                project
                    .iter()
                    .flat_map(|m| m.functions.iter())
                    .flat_map(|f| f.state_parameters.iter())
                    .flat_map(|state| {
                        state
                            .bindings
                            .iter()
                            .map(|binding| binding.generic_name.clone())
                    }),
            )
            .collect(),
        declarations,
        enum_declarations,
        owners,
        active: BTreeSet::new(),
        cache: BTreeMap::new(),
        remaining: *remaining,
        depth_limit: limits.max_call_depth,
    };
    if !resolver.generic.templates.is_empty() {
        aggregate_templates::spend(
            &mut resolver.remaining,
            resolver.generic.templates.len(),
            Span::default(),
        )
        .map_err(|e| vec![e])?;
        let templates = resolver
            .generic
            .templates
            .iter()
            .map(|(key, t)| (*key, t.clone()))
            .collect::<Vec<_>>();
        for (family, template) in templates {
            resolver.ordinary_parameter = Some(template.parameter.name.clone());
            resolver
                .generic_materialize(
                    family,
                    &[Type::Named(template.parameter.name.clone())],
                    None,
                    0,
                    template.span,
                )
                .map_err(|e| vec![e])?;
        }
        resolver.ordinary_parameter = None;
    }
    for d in &core.aggregates {
        resolver
            .ty(&Type::Named(d.name.clone()), 0, d.span)
            .map_err(|e| vec![e])?;
    }
    let mut resolved_enums = core.enums.clone();
    for declaration in &mut resolved_enums {
        let resolved = resolver
            .ty(&Type::Named(declaration.name.clone()), 0, declaration.span)
            .map_err(|e| vec![e])?;
        let variants = match resolved {
            Type::Enum { variants, .. } => variants,
            Type::BorrowedStorage { storage, .. } => match *storage {
                Type::Enum { variants, .. } => variants,
                _ => {
                    return Err(vec![error(
                        "E-STORED-TYPE",
                        declaration.span,
                        "resolved stored Enum has no physical variant layout",
                    )]);
                }
            },
            _ => {
                return Err(vec![error(
                    "E-ENUM-TYPE",
                    declaration.span,
                    "resolved Enum has no physical variant layout",
                )]);
            }
        };
        declaration.variants = variants;
    }
    // Resolve using the original declaration headers while moving function bodies.
    // State selectors need the unmodified headers, but cloning every body is unnecessary.
    let mut original_headers = Vec::with_capacity(core.functions.len());
    for function in &core.functions {
        let callable = inference::snapshot_callable_parameters(
            &function.callable_parameters,
            limits,
            remaining,
            function.span,
        )
        .map_err(|e| vec![e])?;
        original_headers.push((callable, function.state_parameters.clone()));
    }
    let mut functions = std::mem::take(&mut core.functions);
    for (f, (callable_parameters, state_parameters)) in functions.iter_mut().zip(&original_headers)
    {
        resolver.current_owner = Some((core.module_id, f.id));
        resolver.metadata_context = false;
        resolver.active_callable_parameters = callable_parameters;
        resolver.active_state_parameters = state_parameters;
        store_templates::validate_header(f).map_err(|e| vec![e])?;
        if let Some(parameter) = f.type_parameters.first() {
            resolver.remaining = resolver
                .remaining
                .checked_sub(parameter.name.len().saturating_add(1))
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Types,
                        "ordinary binder resolution budget exceeded",
                        Some(f.span),
                    )]
                })?;
            resolver.ordinary_parameter = Some(parameter.name.clone());
        }
        for state in &mut f.state_parameters {
            for bound in &mut state.bindings {
                bound.result = resolver.ty(&bound.result, 0, f.span).map_err(|e| vec![e])?;
                for ty in &mut bound.parameters {
                    *ty = resolver.ty(ty, 0, f.span).map_err(|e| vec![e])?;
                }
            }
        }
        for bound in &mut f.callable_parameters {
            bound.result = resolver.ty(&bound.result, 0, f.span).map_err(|e| vec![e])?;
            for ty in &mut bound.parameters {
                *ty = resolver.ty(ty, 0, f.span).map_err(|e| vec![e])?;
            }
        }
        if let Some(bound) = &mut f.callable_return {
            bound.result = resolver.ty(&bound.result, 0, f.span).map_err(|e| vec![e])?;
            for ty in &mut bound.parameters {
                *ty = resolver.ty(ty, 0, f.span).map_err(|e| vec![e])?;
            }
        }
        let raw_result = if matches!(f.result_mode, PassingMode::Create | PassingMode::Give) {
            owned_mode_type(&f.return_type)
        } else {
            f.return_type.clone()
        };
        f.return_type = resolver.ty(&raw_result, 0, f.span).map_err(|e| vec![e])?;
        for p in &mut f.parameters {
            let raw = if p.mode == PassingMode::Give {
                owned_mode_type(&p.ty)
            } else {
                p.ty.clone()
            };
            p.ty = resolver.ty(&raw, 0, p.span).map_err(|e| vec![e])?;
            if matches!(p.ty, Type::Desc(_)) {
                p.mode = PassingMode::Describe;
            }
        }
        if let Some(r) = &mut f.receiver {
            let raw = if r.mode == PassingMode::Give {
                Type::Managed(Box::new(r.ty.clone()))
            } else {
                r.ty.clone()
            };
            r.ty = resolver.ty(&raw, 0, f.span).map_err(|e| vec![e])?;
            if matches!(r.ty, Type::Desc(_)) {
                r.mode = PassingMode::Describe;
            }
        }
        resolver.block(&mut f.body, 0).map_err(|e| vec![e])?;
        resolver.ordinary_parameter = None;
        resolver.active_callable_parameters = &[];
        resolver.active_state_parameters = &[];
    }
    resolver.current_owner = None;
    resolver.metadata_context = true;
    let resolved_arguments = std::mem::take(&mut resolver.generic.resolved);
    *remaining = resolver.remaining;
    let mut aggregates = core.aggregates.clone();
    for aggregate in &mut aggregates {
        let application = resolver
            .generic
            .applications
            .get(&aggregate.name)
            .map(|(_, a, _)| a.clone());
        resolver.current_owner =
            application.and_then(|a| a.owner.as_ref().map(|o| (o.module, o.function)));
        resolver.ordinary_parameter = resolver
            .current_owner
            .and_then(|o| resolver.generic.owners.get(&o))
            .and_then(|b| b.ordinary.clone());
        for field in &mut aggregate.fields {
            field.ty = resolver.field_type(field, 0).map_err(|e| vec![e])?;
        }
    }
    let mut resolved_life_instances = Vec::new();
    for instance in &core.life_instances {
        let Type::BorrowedStorage { storage, .. } = &instance.ty else {
            unreachable!("checked stored inventory");
        };
        let name = match storage.as_ref() {
            Type::Record { name, .. } | Type::Package { name, .. } | Type::Enum { name, .. } => {
                name
            }
            _ => unreachable!("checked stored inventory"),
        };
        aggregate_templates::spend(&mut resolver.remaining, name.len(), Span::default())
            .map_err(|e| vec![e])?;
        let actual_name = match storage.as_ref() {
            Type::Record { .. } | Type::Package { .. } => stored::aggregate(
                &core.aggregates,
                core.module_id,
                name,
                &mut resolver.remaining,
            )
            .map_err(|e| vec![e])?
            .name
            .as_str(),
            Type::Enum { .. } => stored::enum_declaration(&core, name, &mut resolver.remaining)
                .map_err(|e| vec![e])?
                .name
                .as_str(),
            _ => unreachable!("checked stored inventory"),
        };
        let ty = resolver
            .ty(&Type::Named(actual_name.to_owned()), 0, Span::default())
            .map_err(|e| vec![e])?;
        let mut arguments = Vec::new();
        for argument in &instance.arguments {
            resolver.charge(0, Span::default()).map_err(|e| vec![e])?;
            arguments.push(match argument {
                LifeNominalActual::Type(ty) => LifeNominalActual::Type(
                    resolver.ty(ty, 0, Span::default()).map_err(|e| vec![e])?,
                ),
                LifeNominalActual::Const(value) => LifeNominalActual::Const(value.clone()),
                LifeNominalActual::Lifetime(class) => LifeNominalActual::Lifetime(*class),
            });
        }
        resolved_life_instances.push(LifeNominalInstance {
            declaration: instance.declaration,
            ty,
            arguments,
        });
    }
    *remaining = resolver.remaining;
    drop(resolver);
    for application in &mut core.aggregate_applications {
        if let Some(arguments) = resolved_arguments.get(&(core.module_id, application.aggregate)) {
            for (arg, resolved) in application.arguments.iter_mut().zip(arguments) {
                if matches!(arg, CoreAggregateArgument::Concrete(_)) {
                    *arg = CoreAggregateArgument::Concrete(resolved.clone());
                }
            }
        }
    }
    core.aggregates = aggregates;
    core.functions = functions;
    core.enums = resolved_enums;
    core.life_instances = resolved_life_instances;
    stored::resolved_inventory(&core, remaining, limits).map_err(|e| vec![e])?;
    Ok(core)
}

/// A source ownership mode selects the direct success resource of a fallible
/// signature. The outer union remains an ordinary Value, never a Managed head.
fn owned_mode_type(ty: &Type) -> Type {
    match ty {
        Type::ErrorUnion { error, result } => Type::ErrorUnion {
            error: error.clone(),
            result: Box::new(Type::Managed(result.clone())),
        },
        _ => Type::Managed(Box::new(ty.clone())),
    }
}

struct Resolver<'a> {
    stored_instances: BTreeMap<&'a str, &'a LifeNominalInstance>,
    generic: aggregate_templates::Index,
    generic_types: BTreeMap<String, Type>,
    generic_active: BTreeSet<aggregate_templates::Family>,
    current_owner: Option<aggregate_templates::Owner>,
    metadata_context: bool,
    generic_limits: Limits,
    callable_names: BTreeSet<String>,
    ordinary_parameter: Option<String>,
    active_callable_parameters: &'a [CoreCallableParameter],
    active_state_parameters: &'a [CoreStateParameter],
    declarations: BTreeMap<String, &'a CoreAggregate>,
    enum_declarations: BTreeMap<String, &'a CoreEnum>,
    owners: BTreeMap<String, u32>,
    active: BTreeSet<String>,
    cache: BTreeMap<String, Type>,
    remaining: usize,
    depth_limit: usize,
}
// An inline Store written as an ordinary Sum/Tuple payload is an owned value.
// Direct standalone descriptions, Ref<Store>, Store elements and arrays retain
// their distinct gates. Resolution has already checked the complete element.
fn ordinary_inline_store(raw: &Type, resolved: Type) -> Type {
    match resolved {
        Type::Desc(payload)
            if matches!(raw, Type::Named(_) | Type::Desc(_))
                && matches!(payload.as_ref(), Type::OwnedStore { element }
                    if dynamic_store::inline_element(element)) =>
        {
            Type::Managed(payload)
        }
        other => other,
    }
}
impl Resolver<'_> {
    fn callback_error_payload(&mut self, ty: &Type, depth: usize, span: Span) -> R<bool> {
        let Type::Package { fields, .. } = ty else {
            return Ok(false);
        };
        if self.active_callable_parameters.is_empty() && self.active_state_parameters.is_empty() {
            return Ok(false);
        }
        for (_, field) in fields {
            self.charge(depth, span)?;
            if let Type::Named(name) = field {
                self.remaining = self
                    .remaining
                    .checked_sub(name.len().saturating_add(1).saturating_mul(
                        self.active_callable_parameters.len().saturating_add(
                            if self.active_state_parameters.is_empty() {
                                0
                            } else {
                                2
                            },
                        ),
                    ))
                    .ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "callback error payload budget exceeded",
                            Some(span),
                        )
                    })?;
            }
        }
        Ok(dynamic_store::callback_element(ty, |field| {
            matches!(field, Type::Named(name) if self.active_callable_parameters.iter().any(|b| &b.generic_name == name)
                || self.active_state_parameters.iter().flat_map(|s| &s.bindings).any(|b| &b.generic_name == name))
        }))
    }
    fn field_type(&mut self, field: &CoreField, depth: usize) -> R<Type> {
        let resolved = self.ty(&field.ty, depth, field.span)?;
        if dynamic_store::is_carrier(&resolved) && !field.manages {
            return Err(unsupported(
                field.span,
                "Store fields require an explicit package manages declaration",
            ));
        }
        if matches!(&resolved, Type::Array { element, .. } if packages::has_store_fields(element)) {
            return Err(unsupported(
                field.span,
                "Store fields require direct manages ownership outside owning array fields",
            ));
        }
        if byte_buffer::resource(&resolved) && !field.manages {
            return Err(unsupported(
                field.span,
                "ByteBuffer fields require an explicit package manages declaration",
            ));
        }
        if field.manages {
            // A source Ref/Desc spelling cannot turn a borrowed entry into an
            // ownership declaration. Re-resolution of normalized Core is safe.
            // The reserved Store<T> source spelling expands directly to its
            // sealed Desc payload. Ref<Store<T>> instead has an additional
            // wrapper and cannot manufacture an ownership declaration.
            if !matches!(field.ty, Type::Named(_) | Type::Managed(_))
                && !matches!(&field.ty, Type::Desc(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }))
            {
                return Err(error(
                    "E-RESOURCE-TYPE",
                    field.span,
                    "manages requires a resource nominal type",
                ));
            }
            match resolved {
                Type::Desc(payload) | Type::Managed(payload) => Ok(Type::Managed(payload)),
                _ => Err(error(
                    "E-RESOURCE-TYPE",
                    field.span,
                    "manages requires a resource nominal type",
                )),
            }
        } else {
            Ok(resolved)
        }
    }
    fn store_element_shape(&mut self, ty: &Type, depth: usize, span: Span) -> R<bool> {
        if let Type::Named(name) = ty
            && self.ordinary_parameter.as_ref() == Some(name)
        {
            return Ok(true);
        }
        dynamic_store::charge_pod(ty, &self.generic_limits, span, &mut self.remaining)?;
        if dynamic_store::supported_element(ty) {
            return Ok(true);
        }
        let Type::Package { fields, .. } = ty else {
            return Ok(false);
        };
        // Only the unresolved callback path adds work. The old Store shell
        // retains exactly its previous resolution cost.
        let lookup = self
            .callable_names
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2);
        for (_, field) in fields {
            self.charge(depth, span)?;
            if let Type::Named(name) = field {
                self.remaining = self
                    .remaining
                    .checked_sub(lookup.saturating_mul(name.len().saturating_add(1)))
                    .ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "Store callback placeholder budget exceeded",
                            Some(span),
                        )
                    })?;
            }
        }
        Ok(dynamic_store::callback_element(
            ty,
            |field| matches!(field, Type::Named(name) if self.callable_names.contains(name)),
        ))
    }

    fn charge(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.depth_limit || self.remaining == 0 {
            return Err(Diagnostic::limit(
                Phase::Types,
                "aggregate resolution budget exceeded",
                Some(span),
            ));
        }
        self.remaining -= 1;
        Ok(())
    }
    // Keep name/declaration construction temporaries out of each ordinary
    // Array/Tuple recursion frame. In debug/default-stack builds this branch's
    // full Type/metadata locals otherwise live in every call to ty(). The
    // dispatcher already charged the actual visit; keep the branch's original
    // budget, validation, cache and active-declaration order unchanged.
    #[inline(never)]
    fn nominal_type(&mut self, name: &str, depth: usize, span: Span) -> R<Type> {
        if self.active.contains(name) {
            return Err(error(
                "E-INFINITE-LAYOUT",
                span,
                "recursive by-value aggregate has infinite layout",
            ));
        }
        if let Some(cached) = self.cache.get(name) {
            if matches!(cached, Type::BorrowedStorage { .. }) {
                let cost = inference::type_snapshot_work(
                    cached,
                    span,
                    &Limits {
                        max_analysis_steps: self.remaining,
                        ..self.generic_limits.clone()
                    },
                )?;
                aggregate_templates::spend(&mut self.remaining, cost, span)?;
                return Ok(cached.clone());
            }
            let mut pending = vec![cached];
            while let Some(t) = pending.pop() {
                if self.remaining == 0 {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "aggregate expansion budget exceeded",
                        Some(span),
                    ));
                }
                self.remaining -= 1;
                match t {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        pending.extend(fields.iter().map(|(_, t)| t))
                    }
                    Type::Enum { variants, .. } => {
                        pending.extend(variants.iter().flat_map(|(_, fields)| fields))
                    }
                    Type::Tuple(fields) => pending.extend(fields),
                    Type::Array { element, .. }
                    | Type::Slice { element }
                    | Type::OwnedStore { element } => pending.push(element),
                    Type::ErrorUnion { error, result } => {
                        pending.push(error);
                        pending.push(result);
                    }
                    Type::Managed(payload) | Type::Desc(payload) => pending.push(payload),
                    _ => {}
                }
            }
            return Ok(cached.clone());
        }
        self.active.insert(name.to_owned());
        let result = if let Some(declaration) = self.enum_declarations.get(name).copied() {
            if !self.stored_instances.is_empty() {
                aggregate_templates::spend(
                    &mut self.remaining,
                    name.len()
                        .saturating_add(1)
                        .saturating_mul(self.stored_instances.len().saturating_add(1)),
                    span,
                )?;
            }
            let stored_instance = self.stored_instances.get(name).copied();
            let mut variants = Vec::new();
            let mut owns = false;
            for (variant, raw_fields) in &declaration.variants {
                let mut fields = Vec::new();
                for field in raw_fields {
                    let resolved = self.ty(field, depth + 1, declaration.span)?;
                    let ty = ordinary_inline_store(field, resolved);
                    let copy = ty.is_copy_value()
                        || stored_instance
                            // The instance still contains its Core actual here;
                            // `ty` has already expanded imported nominal names.
                            // Compare the actual declaration field first, then
                            // resolved_inventory checks the complete layout.
                            .is_some_and(|instance| {
                                stored::option_address_payload(instance, field)
                            })
                        || (stored_instance.is_some()
                            && stored::pure_inline(
                                &ty,
                                &mut self.remaining,
                                &self.generic_limits,
                                declaration.span,
                            )?);
                    owns |= !copy;
                    if !copy && !owned_shapes::value(&ty) {
                        return Err(unsupported(
                            declaration.span,
                            "enum payload requires finite Copy or an ordinary complete owned value",
                        ));
                    }
                    fields.push(ty);
                }
                variants.push((variant.clone(), fields));
            }
            if name.starts_with("$generic_$builtinOption340_")
                || name.starts_with("$generic_$stdTaskPoll343_")
            {
                for (_, fields) in &variants {
                    for field in fields {
                        let work =
                            inference::type_snapshot_work(field, span, &self.generic_limits)?;
                        self.remaining = self
                            .remaining
                            .checked_sub(work.saturating_mul(4))
                            .ok_or_else(|| {
                                Diagnostic::limit(
                                    Phase::Types,
                                    "Option identity work budget exceeded",
                                    Some(span),
                                )
                            })?;
                    }
                }
            }
            let ty = Type::Enum {
                name: enums::nominal_name(self.owners[name], name, &variants),
                variants,
            };
            if owns {
                owned_shapes::validate(&ty, &mut self.remaining, self.depth_limit, span)?;
            }
            if let Some(instance) = stored_instance {
                let Type::BorrowedStorage { life, .. } = &instance.ty else {
                    unreachable!()
                };
                let work = inference::type_snapshot_work(
                    &instance.ty,
                    span,
                    &Limits {
                        max_analysis_steps: self.remaining,
                        ..self.generic_limits.clone()
                    },
                )?;
                aggregate_templates::spend(&mut self.remaining, work, span)?;
                let qualified = Type::BorrowedStorage {
                    storage: Box::new(ty),
                    life: life.clone(),
                };
                stored::shape(&qualified, &mut self.remaining, self.depth_limit, span)?;
                qualified
            } else {
                ty
            }
        } else {
            let d = self.declarations[name];
            if !self.stored_instances.is_empty() {
                aggregate_templates::spend(
                    &mut self.remaining,
                    name.len()
                        .saturating_add(1)
                        .saturating_mul(self.stored_instances.len().saturating_add(1)),
                    span,
                )?;
            }
            let stored_instance = self.stored_instances.get(name).copied();
            let mut fields = Vec::new();
            let mut new_shape = false;
            for f in &d.fields {
                let ty = self.field_type(f, depth + 1)?;
                if !(ty.is_copy_value()
                    || (d.kind == AggregateKind::Struct
                        && stored::pure_inline(
                            &ty,
                            &mut self.remaining,
                            &self.generic_limits,
                            f.span,
                        )?)
                    || (stored_instance.is_some()
                        && stored::pure_value(
                            &ty,
                            &mut self.remaining,
                            &self.generic_limits,
                            f.span,
                        )?)
                    || (stored_instance.is_some() && matches!(ty, Type::Address { .. }))
                    || (d.kind == AggregateKind::Package
                        && !f.manages
                        && d.name.starts_with("$generic_")
                        && matches!(&ty, Type::Named(name) if self.callable_names.contains(name)))
                    || (d.kind == AggregateKind::Package
                        && !f.manages
                        && packages::array_field_supported(&ty))
                    || (d.kind == AggregateKind::Package
                        && !f.manages
                        && matches!(ty, Type::Package { .. })
                        && packages::product::owned_supported(&ty))
                    || (d.kind == AggregateKind::Package
                        && f.manages
                        && matches!(ty, Type::Managed(_))
                        && resources::supported(&ty)))
                {
                    if d.kind == AggregateKind::Package
                        && !f.manages
                        && ((matches!(ty, Type::Package { .. }) && owned_shapes::value(&ty))
                            || owned_shapes::sum(&ty))
                    {
                        new_shape = true;
                    } else {
                        return Err(unsupported(
                            f.span,
                            "aggregate field requires finite Copy, explicit manages, or an ordinary complete owned child",
                        ));
                    }
                }
                fields.push((f.name.clone(), ty));
            }
            let name = format!("@module{}:{}", self.owners[name], name);
            if d.kind == AggregateKind::Package {
                let ty = Type::Package { name, fields };
                if new_shape {
                    owned_shapes::validate(&ty, &mut self.remaining, self.depth_limit, span)?;
                }
                if packages::product::nested(&ty) && packages::product::owned_supported(&ty) {
                    packages::product::validate_owned(
                        &ty,
                        &mut self.remaining,
                        self.depth_limit,
                        span,
                    )?;
                } else if !d.fields.iter().any(|f| f.manages) && !owned_shapes::value(&ty) {
                    return Err(unsupported(
                        span,
                        "package requires at least one manages field or complete owning child",
                    ));
                }
                if let Some(instance) = stored_instance {
                    let Type::BorrowedStorage { life, .. } = &instance.ty else {
                        unreachable!()
                    };
                    let work = inference::type_snapshot_work(
                        &instance.ty,
                        span,
                        &Limits {
                            max_analysis_steps: self.remaining,
                            ..self.generic_limits.clone()
                        },
                    )?;
                    aggregate_templates::spend(&mut self.remaining, work, span)?;
                    let qualified = Type::BorrowedStorage {
                        storage: Box::new(ty),
                        life: life.clone(),
                    };
                    stored::shape(&qualified, &mut self.remaining, self.depth_limit, span)?;
                    qualified
                } else {
                    ty
                }
            } else {
                let payload = Type::Record { name, fields };
                if d.kind == AggregateKind::Resource {
                    Type::Desc(Box::new(payload))
                } else if let Some(instance) = stored_instance {
                    let Type::BorrowedStorage { life, .. } = &instance.ty else {
                        unreachable!()
                    };
                    let work = inference::type_snapshot_work(
                        &instance.ty,
                        span,
                        &Limits {
                            max_analysis_steps: self.remaining,
                            ..self.generic_limits.clone()
                        },
                    )?;
                    aggregate_templates::spend(&mut self.remaining, work, span)?;
                    let qualified = Type::BorrowedStorage {
                        storage: Box::new(payload),
                        life: life.clone(),
                    };
                    stored::shape(&qualified, &mut self.remaining, self.depth_limit, span)?;
                    qualified
                } else {
                    payload
                }
            }
        };
        self.active.remove(name);
        if matches!(result, Type::BorrowedStorage { .. }) {
            let cost = inference::type_snapshot_work(
                &result,
                span,
                &Limits {
                    max_analysis_steps: self.remaining,
                    ..self.generic_limits.clone()
                },
            )?;
            aggregate_templates::spend(&mut self.remaining, cost, span)?;
        }
        self.cache.insert(name.to_owned(), result.clone());
        Ok(result)
    }

    fn ty(&mut self, ty: &Type, depth: usize, span: Span) -> R<Type> {
        self.charge(depth, span)?;
        if let Type::Named(name) = ty
            && !self.generic.applications.is_empty()
        {
            aggregate_templates::spend(
                &mut self.remaining,
                name.len().saturating_mul(
                    self.generic.applications.len().saturating_add(1).ilog2() as usize + 2,
                ),
                span,
            )?;
            if self.generic.applications.contains_key(name) {
                return self.generic_application(name, depth, span);
            }
        }
        if let Type::Package { name, .. } = ty
            && !self.metadata_context
            && self
                .generic
                .symbolic_names
                .get(name)
                .is_some_and(|owner| Some(*owner) != self.current_owner)
        {
            return Err(error(
                "E-PRODUCT-TEMPLATE",
                span,
                "symbolic Product used outside its exact lexical owner",
            ));
        }
        Ok(match ty {
            Type::Named(name) if self.ordinary_parameter.as_ref() == Some(name) => ty.clone(),
            Type::Named(name) if name == "TaskI32" => Type::Desc(Box::new(Type::TaskI32)),
            Type::Named(name) if name == "ByteBuffer" => Type::Desc(Box::new(Type::ByteBuffer)),
            Type::Named(name)
                if self.declarations.contains_key(name)
                    || self.enum_declarations.contains_key(name) =>
            {
                self.nominal_type(name, depth, span)?
            }
            ty @ Type::Package { .. } if packages::product::nested(ty) => {
                if packages::product::owned_supported(ty) {
                    packages::product::validate_owned(
                        ty,
                        &mut self.remaining,
                        self.depth_limit,
                        span,
                    )?;
                } else {
                    owned_shapes::validate(ty, &mut self.remaining, self.depth_limit, span)?;
                }
                ty.clone()
            }
            Type::Enum { name, variants } if name == enums::ANONYMOUS_SUM => {
                let mut members = Vec::new();
                for (_, fields) in variants {
                    if fields.len() != 1 {
                        return Err(error(
                            "E-ANONYMOUS-SUM",
                            span,
                            "anonymous sum variants require exactly one member type",
                        ));
                    }
                    let member = self.ty(&fields[0], depth + 1, span)?;
                    match member {
                        Type::Enum { name, variants } if name == enums::ANONYMOUS_SUM => {
                            for (_, mut fields) in variants {
                                members.push(fields.pop().expect("canonical anonymous member"));
                            }
                        }
                        member => members.push(member),
                    }
                }
                enums::canonical_anonymous(
                    members,
                    &mut self.remaining,
                    &self.generic_limits,
                    span,
                )?
            }
            Type::Task {
                result,
                retained_life,
            } => {
                let result = self.ty(result, depth + 1, span)?;
                let ports = retained_life
                    .ports
                    .iter()
                    .map(|port| {
                        Ok(zil_ir::RetainedBorrowPort {
                            leaf: self.ty(&port.leaf, depth + 1, span)?,
                            mode: port.mode,
                            binder: port.binder,
                        })
                    })
                    .collect::<R<Vec<_>>>()?;
                let task = Type::Task {
                    result: Box::new(result),
                    retained_life: zil_ir::RetainedLifeShape {
                        binder_count: retained_life.binder_count,
                        ports,
                        retained: retained_life.retained.clone(),
                        nominal_arguments: retained_life.nominal_arguments.clone(),
                    },
                };
                tasks::require_closed_context(&task, span)?;
                task
            }
            Type::Mutex { payload } => Type::Mutex {
                payload: Box::new(self.ty(payload, depth + 1, span)?),
            },
            Type::MutexGuard { payload, .. } => {
                let payload = self.ty(payload, depth + 1, span)?;
                generic_mutex::guard(&payload)
            }
            Type::AsyncRegistration { environment } => Type::AsyncRegistration {
                environment: Box::new(self.ty(environment, depth + 1, span)?),
            },
            Type::Registration { environment } => Type::Registration {
                environment: Box::new(self.ty(environment, depth + 1, span)?),
            },
            Type::OwnedStore { element } => {
                let element = self.ty(element, depth + 1, span)?;
                if !self.store_element_shape(&element, depth, span)? {
                    return Err(unsupported(
                        span,
                        "Store elements require nonzero complete direct-Managed Packages or finite POD Records",
                    ));
                }
                Type::OwnedStore {
                    element: Box::new(element),
                }
            }
            Type::Array { element, length } => {
                if *length as usize > self.remaining {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "array length exceeds aggregate budget",
                        Some(span),
                    ));
                }
                self.remaining -= *length as usize;
                Type::Array {
                    element: Box::new(self.ty(element, depth + 1, span)?),
                    length: *length,
                }
            }
            Type::Slice { element } => {
                let element = self.ty(element, depth + 1, span)?;
                if !element.is_copy_value() {
                    return Err(unsupported(
                        span,
                        "slice elements require finite Copy values",
                    ));
                }
                Type::Slice {
                    element: Box::new(element),
                }
            }
            Type::Tuple(fields) => Type::Tuple(
                fields
                    .iter()
                    .map(|t| {
                        let resolved = self.ty(t, depth + 1, span)?;
                        Ok(ordinary_inline_store(t, resolved))
                    })
                    .collect::<R<_>>()?,
            ),
            Type::Managed(inner) | Type::Desc(inner) => {
                let resolved = self.ty(inner, depth + 1, span)?;
                let payload = match resolved {
                    Type::Managed(payload) | Type::Desc(payload) => payload,
                    Type::OwnedStore { .. }
                        if matches!(inner.as_ref(), Type::OwnedStore { .. }) =>
                    {
                        Box::new(resolved)
                    }
                    Type::Registration { .. } | Type::AsyncRegistration { .. }
                        if matches!(
                            inner.as_ref(),
                            Type::Registration { .. } | Type::AsyncRegistration { .. }
                        ) =>
                    {
                        Box::new(resolved)
                    }
                    Type::Task { .. } if matches!(inner.as_ref(), Type::Task { .. }) => {
                        Box::new(resolved)
                    }
                    Type::Mutex { .. } if matches!(inner.as_ref(), Type::Mutex { .. }) => {
                        Box::new(resolved)
                    }
                    Type::TaskI32 if matches!(inner.as_ref(), Type::TaskI32) => {
                        Box::new(Type::TaskI32)
                    }
                    Type::ByteBuffer if matches!(inner.as_ref(), Type::ByteBuffer) => {
                        Box::new(Type::ByteBuffer)
                    }
                    Type::Record { .. } if matches!(inner.as_ref(), Type::Record { .. }) => {
                        Box::new(resolved)
                    }
                    _ => {
                        return Err(error(
                            "E-RESOURCE-TYPE",
                            span,
                            "resource mode requires a resource nominal type",
                        ));
                    }
                };
                if matches!(ty, Type::Managed(_)) {
                    Type::Managed(payload)
                } else {
                    Type::Desc(payload)
                }
            }
            Type::Address { mutable, inner } => {
                let inner = self.ty(inner, depth + 1, span)?;
                let inner = match inner {
                    Type::Managed(payload) | Type::Desc(payload)
                        if *mutable
                            && matches!(
                                payload.as_ref(),
                                Type::TaskI32 | Type::Task { .. } | Type::AsyncRegistration { .. }
                            ) =>
                    {
                        Type::Managed(payload)
                    }
                    Type::Managed(payload) | Type::Desc(payload)
                        if !*mutable
                            && matches!(payload.as_ref(), Type::Record { .. })
                            && payload.is_copy_value() =>
                    {
                        Type::Managed(payload)
                    }
                    Type::Package { .. }
                        if packages::product::nested(&inner)
                            || packages::product::supported(&inner)
                            || (dynamic_store::contains_inline_carrier(&inner)
                                && packages::product::owned_supported(&inner)) =>
                    {
                        if packages::product::supported(&inner) {
                            packages::product::validate(
                                &inner,
                                &mut self.remaining,
                                self.depth_limit,
                                span,
                            )?;
                        } else {
                            packages::product::validate_owned(
                                &inner,
                                &mut self.remaining,
                                self.depth_limit,
                                span,
                            )?;
                        }
                        inner
                    }
                    Type::Package { .. } if self.store_element_shape(&inner, depth, span)? => inner,
                    Type::Package { .. }
                        if packages::has_store_fields(&inner)
                            && packages::product::owned_supported(&inner) =>
                    {
                        packages::product::validate_owned(
                            &inner,
                            &mut self.remaining,
                            self.depth_limit,
                            span,
                        )?;
                        inner
                    }
                    Type::Managed(_) | Type::Desc(_) | Type::Package { .. } => {
                        return Err(unsupported(
                            span,
                            "head addresses require a shared finite user-resource head or a supported Store element; byte-storage and other Package heads are not implemented",
                        ));
                    }
                    other => other,
                };
                Type::Address {
                    mutable: *mutable,
                    inner: Box::new(inner),
                }
            }
            Type::BorrowedStorage { .. } => {
                let work = inference::type_snapshot_work(
                    ty,
                    span,
                    &Limits {
                        max_analysis_steps: self.remaining,
                        ..self.generic_limits.clone()
                    },
                )?;
                aggregate_templates::spend(
                    &mut self.remaining,
                    work.saturating_mul(self.stored_instances.len().saturating_add(1)),
                    span,
                )?;
                let name = self
                    .stored_instances
                    .iter()
                    .find_map(|(name, instance)| (&instance.ty == ty).then_some(*name))
                    .ok_or_else(|| {
                        error(
                            "E-STORED-IDENTITY",
                            span,
                            "qualified stored type has no matching actual nominal inventory",
                        )
                    })?;
                aggregate_templates::spend(&mut self.remaining, name.len(), span)?;
                self.ty(&Type::Named(name.to_owned()), depth + 1, span)?
            }
            Type::ErrorUnion {
                error: failure,
                result,
            } => {
                let failure = ordinary_inline_store(failure, self.ty(failure, depth + 1, span)?);
                let result = ordinary_inline_store(result, self.ty(result, depth + 1, span)?);
                let union = Type::ErrorUnion {
                    error: Box::new(failure),
                    result: Box::new(result),
                };
                if !errors::supported(&union) {
                    let mut symbolic = self
                        .ordinary_parameter
                        .as_ref()
                        .is_some_and(|name| store_templates::symbolic_type(&union, name));
                    if !symbolic && let Type::ErrorUnion { error, result } = &union {
                        let error_ok = error.is_copy_value()
                            || packages::supported(error)
                            || self.callback_error_payload(error, depth + 1, span)?;
                        let result_ok = result.is_copy_value()
                            || packages::supported(result)
                            || self.callback_error_payload(result, depth + 1, span)?;
                        symbolic = error_ok && result_ok;
                    }
                    if !symbolic {
                        return Err(unsupported(
                            span,
                            "error union payloads require finite Copy values or complete owned Packages; direct owned resource success requires explicit mode and a Copy error",
                        ));
                    }
                }
                union
            }
            Type::FunctionPointer { parameters, result } => Type::FunctionPointer {
                parameters: parameters
                    .iter()
                    .map(|t| self.ty(t, depth + 1, span))
                    .collect::<R<_>>()?,
                result: Box::new(self.ty(result, depth + 1, span)?),
            },
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|ty| self.ty(ty, depth + 1, span))
                    .collect::<R<Vec<_>>>()?;
                let result = Box::new(self.ty(result, depth + 1, span)?);
                aggregate_templates::spend(
                    &mut self.remaining,
                    borrowed_fp::metadata_work(life),
                    span,
                )?;
                borrowed_fp::qualified_inputs(life, &parameters, &result, |work| {
                    aggregate_templates::spend(&mut self.remaining, work, span)
                })?;
                borrowed_fp::relation(&parameters, &result, life, span)?;
                Type::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life: life.clone(),
                }
            }
            Type::ForeignCallback { parameters, result } => {
                let parameters = parameters
                    .iter()
                    .map(|ty| self.ty(ty, depth + 1, span))
                    .collect::<R<Vec<_>>>()?;
                let result = self.ty(result, depth + 1, span)?;
                if !foreign::callback_signature(&parameters, &result) {
                    return Err(error(
                        "E-FFI-CALLBACK-TYPE",
                        span,
                        "C callback parameters require i32/i64/c_handle and result may additionally be unit",
                    ));
                }
                Type::ForeignCallback {
                    parameters,
                    result: Box::new(result),
                }
            }
            Type::Opaque { identity, hidden } => Type::Opaque {
                identity: *identity,
                hidden: Box::new(self.ty(hidden, depth + 1, span)?),
            },
            _ => ty.clone(),
        })
    }
    fn block(&mut self, b: &mut CoreBlock, depth: usize) -> R<()> {
        self.charge(depth, b.span)?;
        for s in &mut b.statements {
            match &mut s.kind {
                CoreStatementKind::TupleBinding {
                    pattern,
                    ty,
                    initializer,
                    ..
                } => {
                    if let Some(t) = ty {
                        *t = self.ty(t, depth + 1, s.span)?;
                    }
                    self.expr(initializer, depth + 1)?;
                    self.pattern(pattern, depth + 1)?;
                }
                CoreStatementKind::Binding {
                    ty, initializer, ..
                } => {
                    if let Some(t) = ty {
                        *t = self.ty(t, depth + 1, s.span)?;
                    }
                    if let Some(e) = initializer {
                        self.expr(e, depth + 1)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                    self.expr(e, depth + 1)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(b) = else_body {
                        self.block(b, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    self.expr(value, depth + 1)?;
                    self.pattern(pattern, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    self.expr(value, depth + 1)?;
                    self.pattern(pattern, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expr(value, depth + 1)?;
                    for arm in arms {
                        self.pattern(&mut arm.pattern, depth + 1)?;
                        self.block(&mut arm.body, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(b)
                | CoreStatementKind::Unsafe(b)
                | CoreStatementKind::Defer(b) => self.block(b, depth + 1)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn pattern(&mut self, p: &mut CorePattern, depth: usize) -> R<()> {
        self.charge(depth, p.span)?;
        match &mut p.kind {
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => self.pattern(pattern, depth + 1)?,
            CorePatternKind::Variant { ty, fields, .. } => {
                *ty = self.ty(ty, depth + 1, p.span)?;
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Record { ty, fields } => {
                *ty = self.ty(ty, depth + 1, p.span)?;
                for (name, field) in fields {
                    self.charge(depth + 1, p.span)?;
                    self.pattern(field, depth + 1)?;
                    let _ = name;
                }
            }
            CorePatternKind::Tuple(fields) => {
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn expr(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        self.charge(depth, e.span)?;
        match &mut e.kind {
            CoreExprKind::MakeDynBorrow {
                source: operand, ..
            }
            | CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand) => self.expr(operand, depth + 1),
            CoreExprKind::Binary { left, right, .. } => self.expr_pair(left, right, depth + 1),
            CoreExprKind::Index { object, index } => self.expr_pair(object, index, depth + 1),
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                self.expr_list(values, depth + 1)
            }
            CoreExprKind::Ascribed { .. }
            | CoreExprKind::IntegerConvert { .. }
            | CoreExprKind::NumericConvert { .. } => self.expr_variant_ascribed(e, depth),
            CoreExprKind::Record { .. } => self.expr_variant_record(e, depth),
            CoreExprKind::RecordUpdate { .. } => self.expr_variant_recordupdate(e, depth),
            CoreExprKind::TypeApply { .. } => self.expr_variant_typeapply(e, depth),
            CoreExprKind::Call { .. } | CoreExprKind::CallDynSlot { .. } => {
                self.expr_variant_call(e, depth)
            }
            CoreExprKind::Slice { .. } => self.expr_variant_slice(e, depth),
            CoreExprKind::Member { .. } => self.expr_variant_member(e, depth),
            CoreExprKind::Sequence { .. } => self.expr_variant_sequence(e, depth),
            CoreExprKind::Closure { .. } => self.expr_variant_closure(e, depth),
            CoreExprKind::If { .. } => self.expr_variant_if(e, depth),
            _ => Ok(()),
        }
    }

    #[inline(never)]
    fn expr_pair(&mut self, first: &mut CoreExpr, second: &mut CoreExpr, depth: usize) -> R<()> {
        self.expr(first, depth)?;
        self.expr(second, depth)
    }

    #[inline(never)]
    fn expr_list(&mut self, values: &mut [CoreExpr], depth: usize) -> R<()> {
        for value in values {
            self.expr(value, depth)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_ascribed(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Ascribed { ty, value }
            | CoreExprKind::IntegerConvert { target: ty, value }
            | CoreExprKind::NumericConvert { target: ty, value } => {
                *ty = self.ty(ty, depth + 1, e.span)?;
                self.expr(value, depth + 1)?;
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_record(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Record { ty, fields } => {
                *ty = match self.ty(ty, depth + 1, e.span)? {
                    Type::Desc(payload) => Type::Managed(payload),
                    other => other,
                };
                for (_, v) in fields {
                    self.expr(v, depth + 1)?;
                }
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_recordupdate(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                if let Some(t) = ty {
                    *t = self.ty(t, depth + 1, e.span)?;
                }
                self.expr(base, depth + 1)?;
                for (_, v) in fields {
                    self.expr(v, depth + 1)?;
                }
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_typeapply(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                for argument in type_arguments {
                    *argument = self.ty(argument, depth + 1, e.span)?;
                }
                self.expr(callee, depth + 1)?;
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_call(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Call { callee, arguments }
            | CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expr(callee, depth + 1)?;
                for e in arguments {
                    self.expr(e, depth + 1)?;
                }
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_slice(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(endpoint, depth + 1)?;
                }
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_member(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Member { object, owner, .. } => {
                if let Some(ty) = owner {
                    *ty = self.ty(ty, depth + 1, e.span)?;
                }
                self.expr(object, depth + 1)?;
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_sequence(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Sequence { bindings, value } => {
                for (_, e) in bindings {
                    self.expr(e, depth + 1)?;
                }
                self.expr(value, depth + 1)?;
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_closure(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
            } => {
                for p in parameters {
                    if let Some(t) = &mut p.ty {
                        *t = self.ty(t, depth + 1, p.span)?;
                    }
                }
                if let Some(t) = return_type {
                    *t = self.ty(t, depth + 1, e.span)?;
                }
                self.block(body, depth + 1)?;
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_variant_if(&mut self, e: &mut CoreExpr, depth: usize) -> R<()> {
        match &mut e.kind {
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            _ => unreachable!("aggregate expression dispatcher"),
        }
        Ok(())
    }
}

pub(super) fn field(ty: &Type, name: &str, span: Span) -> R<(u32, Type)> {
    // Address<Managed<R>> projects the actual inline payload, not a copied
    // head or a descriptor. Other callers still decide the access operation.
    let ty = match ty {
        Type::Managed(payload) => payload.as_ref(),
        other => other,
    };
    let ty = stored::physical(ty);
    let selected = match ty {
        Type::Tuple(fields) => name
            .parse::<usize>()
            .ok()
            .and_then(|i| fields.get(i).map(|t| (i, t))),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .enumerate()
            .find(|(_, (n, _))| n == name)
            .map(|(i, (_, t))| (i, t)),
        _ => {
            return Err(error(
                "E-FIELD-TYPE",
                span,
                "field access requires a record or tuple",
            ));
        }
    };
    selected
        .map(|(i, t)| (i as u32, t.clone()))
        .ok_or_else(|| error("E-UNKNOWN-FIELD", span, format!("unknown field {name}")))
}
/// Look up an exact nominal owner's method, never a global same-name method.
/// Imported lookup observes actual exports and an explicit module edge; the
/// later ordinary Call still carries its original FunctionId and full ABI.
pub(super) fn resolve_method<'a>(
    name: &str,
    member: &str,
    local: &'a BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'a>>,
) -> Option<&'a Signature> {
    let (owner, nominal) = name.split_once(':')?;
    let key = format!("{nominal}.{member}");
    if let Some(signature) = local.get(&key).filter(|s| s.receiver.is_some()) {
        return Some(signature);
    }
    let module = zil_base::ModuleId(owner.strip_prefix("@module")?.parse().ok()?);
    let imports = imports?;
    if !imports.aliases.values().any(|id| *id == module) {
        return None;
    }
    imports
        .signatures
        .get(&module)?
        .get(&key)
        .filter(|s| s.public && s.receiver.is_some() && !s.cleanup)
}

/// One concrete nominal receiver, including the sealed builtin resource.
/// ByteBuffer inherent methods are local: it has no user defining module from
/// which to infer an imported extension. No global same-name search is used.
pub(super) fn resolve_type_method<'a>(
    ty: &Type,
    member: &str,
    local: &'a BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'a>>,
) -> Option<&'a Signature> {
    match ty {
        Type::Record { name, fields } | Type::Package { name, fields }
            if !fields.iter().any(|(field, _)| field == member) =>
        {
            resolve_method(name, member, local, imports)
        }
        Type::Enum { name, .. } => resolve_method(name, member, local, imports),
        Type::ByteBuffer => local
            .get(&format!("ByteBuffer.{member}"))
            .filter(|s| s.receiver.is_some() && !s.cleanup),
        _ => None,
    }
}

impl<'a> Builder<'a, '_> {
    pub(super) fn method_signature(
        &self,
        object: &CoreExpr,
        member: &str,
    ) -> Option<&'a Signature> {
        if let Some(ty) = self.known_type_ref(object) {
            let ty = match ty {
                Type::Address { inner, .. } => match inner.as_ref() {
                    Type::Managed(payload) => payload.as_ref(),
                    other => other,
                },
                Type::Managed(inner) | Type::Desc(inner) => inner.as_ref(),
                other => other,
            };
            return resolve_type_method(ty, member, self.signatures, self.imports);
        }
        let ty = self.known_type(object)?;
        let ty = match ty {
            Type::Address { inner, .. } => match *inner {
                Type::Managed(payload) => *payload,
                other => other,
            },
            Type::Managed(inner) | Type::Desc(inner) => *inner,
            other => other,
        };
        resolve_type_method(&ty, member, self.signatures, self.imports)
    }
    pub(super) fn method_call(
        &mut self,
        object: &CoreExpr,
        args: &[CoreExpr],
        signature: &Signature,
        owner: Option<&Type>,
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if signature.parameters.len() != args.len() + 1 {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "method argument count mismatch",
            ));
        }
        let receiver = &signature.parameters[0];
        let (first, actual) =
            if matches!(receiver, Type::Address { .. }) && !self.known_address(object) {
                let (value, ty) = self.borrow_place_mode(
                    object,
                    matches!(receiver, Type::Address { mutable: true, .. }),
                )?;
                (value, ty)
            } else {
                self.argument_expr(object, receiver)?
            };
        self.member_owner(&actual, owner, span)?;
        self.same(&actual, receiver, object.span)?;
        let mut arguments = vec![self.stage_argument(first, receiver, object.span)?];
        for (arg, ty) in args.iter().zip(&signature.parameters[1..]) {
            let (value, actual) = self.argument_expr(arg, ty)?;
            self.same(&actual, ty, arg.span)?;
            arguments.push(self.stage_argument(value, &actual, arg.span)?);
        }
        let arguments = self.commit_arguments(arguments, span)?;
        let ty = signature.result.clone();
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        let kind = InstructionKind::Call {
            function: signature.concrete_id(span)?,
            arguments,
        };
        if ty == Type::Unit {
            self.emit(kind, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            Ok((self.value(kind, ty.clone(), span)?, ty))
        }
    }

    pub(super) fn aggregate(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        match &e.kind {
            CoreExprKind::Array(..) => self.aggregate_array(e, expected),
            CoreExprKind::Index { .. } => self.aggregate_index(e),
            CoreExprKind::Tuple(..) => self.aggregate_tuple(e, expected),
            CoreExprKind::Record { .. } => self.aggregate_record(e),
            CoreExprKind::RecordUpdate { .. } => self.aggregate_record_update(e, expected),
            CoreExprKind::Member { .. } => self.aggregate_member(e),
            _ => unreachable!(),
        }
    }
    #[inline(never)]
    fn aggregate_array(&mut self, e: &CoreExpr, expected: Option<&Type>) -> R<(ValueId, Type)> {
        let CoreExprKind::Array(items) = &e.kind else {
            unreachable!("expression dispatcher variant")
        };
        let (mut element, length) = match expected {
            Some(Type::Array { element, length }) => {
                if !element.is_copy_value()
                    && stored::pure_inline(element, &mut self.match_work_left, self.limits, e.span)?
                {
                    self.closures.snapshot_type(element, 1, 0, e.span)?;
                }
                (Some((**element).clone()), *length)
            }
            _ => (None, items.len() as u32),
        };
        if length as usize != items.len() {
            return Err(error(
                "E-ARRAY-LENGTH",
                e.span,
                "array literal length does not match its type",
            ));
        }
        self.closures.snapshot(items.len(), e.span)?;
        let mut staged = Vec::new();
        for v in items {
            let (id, t) = self.owned_array_input(v, element.as_ref())?;
            if element.is_none() {
                if !t.is_copy_value()
                    && stored::pure_inline(&t, &mut self.match_work_left, self.limits, v.span)?
                {
                    self.closures.snapshot_type(&t, 1, 0, v.span)?;
                }
                element = Some(t.clone());
            }
            self.same(&t, element.as_ref().expect("known array element"), v.span)?;
            staged.push(self.stage_argument(id, &t, v.span)?);
        }
        let element = element.ok_or_else(|| {
            error(
                "E-ARRAY-INFER",
                e.span,
                "empty array requires an element type",
            )
        })?;
        let ty = Type::Array {
            element: Box::new(element),
            length,
        };
        let owned = packages::owned_array_supported(&ty);
        if let Type::Array { element, length } = &ty {
            self.check_owned_array_closure(element, *length, e.span)?;
        }
        let qualified_pure = !owned
            && !ty.is_copy_value()
            && stored::pure_inline(&ty, &mut self.match_work_left, self.limits, e.span)?;
        if !owned && !ty.is_copy_value() && !qualified_pure {
            return Err(unsupported(
                e.span,
                "array elements require finite Copy values or a nonempty homogeneous Package or self-contained Closure array",
            ));
        }
        let fields = self.commit_arguments(staged, e.span)?;
        if qualified_pure {
            self.closures.snapshot_type(&ty, 1, 0, e.span)?;
        }
        Ok((
            self.value(
                if owned {
                    InstructionKind::MakeOwnedArray { elements: fields }
                } else {
                    InstructionKind::MakeAggregate { fields }
                },
                ty.clone(),
                e.span,
            )?,
            ty,
        ))
    }

    #[inline(never)]
    fn aggregate_index(&mut self, e: &CoreExpr) -> R<(ValueId, Type)> {
        let CoreExprKind::Index { object, index } = &e.kind else {
            unreachable!("expression dispatcher variant")
        };
        if let Some((local, index, ty)) = self.owned_array_place(e)? {
            return Ok((
                self.value(
                    InstructionKind::TakeOwnedArrayElement { local, index },
                    ty.clone(),
                    e.span,
                )?,
                ty,
            ));
        }
        if self.known_address(object) || dynamic_store::borrow_expression(object) {
            let (address, ty) = self.borrow_place(e)?;
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            let ty = *inner;
            if !ty.is_copy_value()
                && stored::pure_inline(&ty, &mut self.match_work_left, self.limits, e.span)?
            {
                self.closures.snapshot_type(&ty, 1, 0, e.span)?;
            }
            return Ok((
                self.value(InstructionKind::ReadAddress { address }, ty.clone(), e.span)?,
                ty,
            ));
        }
        let (aggregate, ty) = self.expr(object, None)?;
        let Type::Array { element, .. } = ty else {
            return Err(error(
                "E-INDEX-TYPE",
                e.span,
                "index requires a fixed array",
            ));
        };
        let (index, index_ty) = self.expr(index, None)?;
        if !index_ty.is_integer() {
            return Err(error(
                "E-INDEX-TYPE",
                e.span,
                "array index must be i32 or i64",
            ));
        }
        let ty = *element;
        if !ty.is_copy_value()
            && stored::pure_inline(&ty, &mut self.match_work_left, self.limits, e.span)?
        {
            self.closures.snapshot_type(&ty, 1, 0, e.span)?;
        }
        Ok((
            self.value(
                InstructionKind::ExtractIndex { aggregate, index },
                ty.clone(),
                e.span,
            )?,
            ty,
        ))
    }

    #[inline(never)]
    fn aggregate_tuple(&mut self, e: &CoreExpr, expected: Option<&Type>) -> R<(ValueId, Type)> {
        let CoreExprKind::Tuple(items) = &e.kind else {
            unreachable!("expression dispatcher variant")
        };
        let expected_fields = match expected {
            Some(Type::Tuple(fs)) if fs.len() == items.len() => Some(fs),
            _ => None,
        };
        let mut staged = Vec::new();
        let mut types = Vec::new();
        for (i, v) in items.iter().enumerate() {
            let (id, t) = if let Some(fs) = expected_fields {
                self.argument_expr(v, &fs[i])?
            } else {
                self.transfer_expr(v, None)?
            };
            staged.push(self.stage_argument(id, &t, v.span)?);
            types.push(t);
        }
        let ty = Type::Tuple(types);
        let pure = ty.is_copy_value()
            || stored::pure_inline(&ty, &mut self.match_work_left, self.limits, e.span)?;
        if !pure {
            owned_shapes::validate(
                &ty,
                &mut self.match_work_left,
                self.limits.max_call_depth,
                e.span,
            )?;
        }
        let fields = self.commit_arguments(staged, e.span)?;
        let kind = if pure {
            InstructionKind::MakeAggregate { fields }
        } else {
            InstructionKind::MakeOwnedTuple { fields }
        };
        if pure && !ty.is_copy_value() {
            self.closures.snapshot_type(&ty, 1, 0, e.span)?;
        }
        Ok((self.value(kind, ty.clone(), e.span)?, ty))
    }

    #[inline(never)]
    fn aggregate_record(&mut self, e: &CoreExpr) -> R<(ValueId, Type)> {
        let CoreExprKind::Record { ty, fields } = &e.kind else {
            unreachable!("expression dispatcher variant")
        };
        let payload = if let Type::Managed(payload) = ty {
            payload.as_ref()
        } else {
            ty
        };
        let payload = stored::physical(payload);
        let (Type::Record { fields: decl, .. } | Type::Package { fields: decl, .. }) = payload
        else {
            return Err(error(
                "E-RECORD-TYPE",
                e.span,
                "record constructor requires a resolved nominal type",
            ));
        };
        let mut slots = vec![None; decl.len()];
        let mut guarded = vec![None; decl.len()];
        for (name, value) in fields {
            if let Some((_, actual)) = decl.iter().find(|(field, _)| field == name)
                && !actual.is_copy_value()
                && stored::pure_inline(actual, &mut self.match_work_left, self.limits, value.span)?
            {
                self.closures.snapshot_type(actual, 1, 0, value.span)?;
            }
            let (i, t) = field(payload, name, value.span)?;
            if slots[i as usize].is_some() {
                return Err(error(
                    "E-FIELD-DUPLICATE",
                    value.span,
                    "duplicate record field",
                ));
            }
            slots[i as usize] = Some(if matches!(stored::physical(ty), Type::Package { .. }) {
                let (prepared, actual) = if matches!(t, Type::Closure(_)) {
                    self.check_owned_array_closure(&t, 1, value.span)?;
                    self.owned_array_input(value, Some(&t))?
                } else {
                    self.argument_expr(value, &t)?
                };
                let (prepared, guard) = self.stage_argument(prepared, &actual, value.span)?;
                guarded[i as usize] = guard;
                prepared
            } else {
                self.expr(value, Some(&t))?.0
            });
        }
        if slots.iter().any(Option::is_none) {
            return Err(error(
                "E-FIELD-MISSING",
                e.span,
                "record constructor is missing required fields",
            ));
        }
        let fields = if matches!(stored::physical(ty), Type::Package { .. }) {
            self.commit_arguments(
                slots
                    .into_iter()
                    .zip(guarded)
                    .map(|(value, guard)| (value.expect("complete package"), guard))
                    .collect(),
                e.span,
            )?
        } else {
            slots.into_iter().map(Option::unwrap).collect()
        };
        if !ty.is_copy_value()
            && stored::pure_inline(ty, &mut self.match_work_left, self.limits, e.span)?
        {
            self.closures.snapshot_type(ty, 2, 0, e.span)?;
        }
        Ok((
            self.value(
                if matches!(ty, Type::Managed(_)) {
                    InstructionKind::MakeManaged { fields }
                } else if matches!(stored::physical(ty), Type::Package { .. }) {
                    InstructionKind::MakePackage { fields }
                } else {
                    InstructionKind::MakeAggregate { fields }
                },
                ty.clone(),
                e.span,
            )?,
            ty.clone(),
        ))
    }

    #[inline(never)]
    fn aggregate_record_update(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::RecordUpdate { ty, base, fields } = &e.kind else {
            unreachable!("expression dispatcher variant")
        };
        let (mut current, actual) = self.expr(base, ty.as_ref().or(expected))?;
        if !matches!(actual, Type::Record { .. }) {
            return Err(error(
                "E-RECORD-TYPE",
                e.span,
                "record update requires a nominal record",
            ));
        }
        let mut seen = BTreeSet::new();
        for (name, value) in fields {
            if !seen.insert(name) {
                return Err(error(
                    "E-FIELD-DUPLICATE",
                    value.span,
                    "duplicate update field",
                ));
            }
            if let Type::Record { fields, .. } = &actual
                && let Some((_, field)) = fields.iter().find(|(field, _)| field == name)
                && !field.is_copy_value()
                && stored::pure_inline(field, &mut self.match_work_left, self.limits, value.span)?
            {
                self.closures.snapshot_type(field, 1, 0, value.span)?;
            }
            let (index, t) = field(&actual, name, value.span)?;
            let value = self.expr(value, Some(&t))?.0;
            if !actual.is_copy_value()
                && stored::pure_inline(&actual, &mut self.match_work_left, self.limits, e.span)?
            {
                self.closures.snapshot_type(&actual, 1, 0, e.span)?;
            }
            current = self.value(
                InstructionKind::UpdateField {
                    aggregate: current,
                    index,
                    value,
                },
                actual.clone(),
                e.span,
            )?;
        }
        Ok((current, actual))
    }

    #[inline(never)]
    fn aggregate_member(&mut self, e: &CoreExpr) -> R<(ValueId, Type)> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &e.kind
        else {
            unreachable!("expression dispatcher variant")
        };
        if self.known_type_is(object, |ty| {
            stored::shared_value(ty) || stored::shared_address(ty)
        }) {
            if self.known_type_is(object, |ty| matches!(ty,
                Type::BorrowedStorage { storage, .. }
                    if matches!(storage.as_ref(), Type::Package { fields, .. }
                        if fields.iter().any(|(name, field)| name == member && matches!(field, Type::Managed(_))))))
                && !self.indirect_place(e)
                && self.method_signature(object, member).is_none()
            {
                return self.package_field(object, member, owner.as_ref(), e.span);
            }
            let exclusive = self.known_type_is(object, |ty| {
                let ty = match ty {
                    Type::Address { inner, .. } => inner.as_ref(),
                    _ => ty,
                };
                matches!(stored::physical(ty), Type::Record { fields, .. } | Type::Package { fields, .. }
                    if fields.iter().any(|(_, field)| matches!(field, Type::Address { mutable: true, .. })))
            });
            let (storage, address_ty) = self.projection_base(object, exclusive)?;
            let Type::Address { mutable, inner } = &address_ty else {
                unreachable!()
            };
            self.closures.snapshot_type(inner, 2, 2, e.span)?;
            self.member_owner(inner, owner.as_ref(), e.span)?;
            let (index, ty) = field(inner, member, e.span)?;
            if matches!(ty, Type::Address { mutable: false, .. }) {
                return Ok((
                    self.value(
                        InstructionKind::LoadStoredBorrow {
                            storage,
                            path: vec![PlaceProjection::Field(index)],
                            mode: StoredBorrowLoad::CopyShared,
                        },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ));
            }
            if matches!(ty, Type::Address { mutable: true, .. }) {
                if !mutable {
                    return Err(unsupported(
                        e.span,
                        "exclusive stored field requires a mutable holder",
                    ));
                }
                return Ok((
                    self.value(
                        InstructionKind::LoadStoredBorrow {
                            storage,
                            path: vec![PlaceProjection::Field(index)],
                            mode: StoredBorrowLoad::Reborrow { mutable: true },
                        },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ));
            }
            if matches!(ty, Type::Managed(_))
                && matches!(stored::physical(inner), Type::Package { .. })
            {
                let field_address = self.value(
                    InstructionKind::AddressField {
                        address: storage,
                        index,
                    },
                    Type::Address {
                        mutable: *mutable,
                        inner: Box::new(ty.clone()),
                    },
                    e.span,
                )?;
                let Type::Managed(payload) = ty else {
                    unreachable!()
                };
                let description = Type::Desc(payload.clone());
                return Ok((
                    self.value(
                        InstructionKind::DescribeAddress {
                            address: field_address,
                        },
                        description.clone(),
                        e.span,
                    )?,
                    description,
                ));
            }
            if !ty.is_copy_value()
                && !stored::pure_value(&ty, &mut self.match_work_left, self.limits, e.span)?
                && !stored::pure_inline(&ty, &mut self.match_work_left, self.limits, e.span)?
            {
                return Err(unsupported(
                    e.span,
                    "nested stored field requires its complete projection consumer",
                ));
            }
            let field_address = self.value(
                InstructionKind::AddressField {
                    address: storage,
                    index,
                },
                Type::Address {
                    mutable: *mutable,
                    inner: Box::new(ty.clone()),
                },
                e.span,
            )?;
            if !ty.is_copy_value() {
                self.closures.snapshot_type(&ty, 1, 0, e.span)?;
            }
            return Ok((
                self.value(
                    InstructionKind::ReadAddress {
                        address: field_address,
                    },
                    ty.clone(),
                    e.span,
                )?,
                ty,
            ));
        }
        if self.known_type_is(object, |ty| matches!(ty, Type::Package { .. }))
            && !self.indirect_place(e)
            && self.method_signature(object, member).is_none()
        {
            return self.package_field(object, member, owner.as_ref(), e.span);
        }
        if self.known_type_is(object, |ty| matches!(ty, Type::Managed(_) | Type::Desc(_)))
            && !self.indirect_place(e)
            && self.method_signature(object, member).is_none()
        {
            return self.resource_field(object, member, owner.as_ref(), e.span);
        }
        if self.method_signature(object, member).is_some() {
            return Err(error(
                "E-RECEIVER-METHOD-VALUE",
                e.span,
                "instance method value requires an explicit closure",
            ));
        }
        let before = self.closures.remaining_work();
        let mut remaining = before;
        let owned_tuple_place = (|| -> R<bool> {
            let Some(ty) = self.borrowed_task_hint(object, &mut remaining, 0)? else {
                return Ok(false);
            };
            if !matches!(ty, Type::Tuple(_)) {
                return Ok(false);
            }
            inference::charge_type_snapshot(ty, e.span, self.limits, &mut remaining, 2)?;
            Ok(owned_shapes::tuple(ty))
        })();
        self.closures.snapshot(before - remaining, e.span)?;
        if owned_tuple_place?
            || self.known_address(object)
            || dynamic_store::borrow_expression(object)
            || self.indirect_place(e)
        {
            let (address, ty) = self.address_place(e, false)?;
            if resources::head_address(&ty) {
                return self.resource_descriptor(address, ty, e.span);
            }
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            let ty = *inner;
            if !ty.is_copy_value() {
                return Err(unsupported(
                    e.span,
                    "shared Product projections only read Copy leaves; borrow an owned child explicitly",
                ));
            }
            return Ok((
                self.value(InstructionKind::ReadAddress { address }, ty.clone(), e.span)?,
                ty,
            ));
        }
        let (aggregate, ty) = self.expr(object, None)?;
        self.member_owner(&ty, owner.as_ref(), e.span)?;
        if let Type::Record { fields, .. } = stored::physical(&ty)
            && let Some((_, actual)) = fields.iter().find(|(name, _)| name == member)
            && !actual.is_copy_value()
            && stored::pure_inline(actual, &mut self.match_work_left, self.limits, e.span)?
        {
            self.closures.snapshot_type(actual, 2, 0, e.span)?;
        }
        let (index, field_ty) = field(&ty, member, e.span)?;
        Ok((
            self.value(
                InstructionKind::ExtractField { aggregate, index },
                field_ty.clone(),
                e.span,
            )?,
            field_ty,
        ))
    }

    pub(super) fn assign_field(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<()> {
        enum Written<'a> {
            Field(&'a str, Option<&'a Type>, Span),
            Index(&'a CoreExpr),
        }
        enum Project {
            Field(u32),
            Index(ValueId),
        }
        let mut members = Vec::new();
        let mut root = target;
        loop {
            match &root.kind {
                CoreExprKind::Member {
                    object,
                    member,
                    owner,
                } => {
                    members.push(Written::Field(member, owner.as_ref(), root.span));
                    root = object;
                }
                CoreExprKind::Index { object, index } => {
                    members.push(Written::Index(index));
                    root = object;
                }
                _ => break,
            }
        }
        let CoreExprKind::Name(name) = &root.kind else {
            return Err(unsupported(
                target.span,
                "projected assignment requires a local root",
            ));
        };
        let local = self
            .lookup(name)
            .ok_or_else(|| error("E-UNKNOWN-NAME", root.span, "unknown assignment root"))?;
        let decl = self.function.locals[local.0 as usize].clone();
        if matches!(decl.ty, Type::Package { .. }) || packages::owned_array_supported(&decl.ty) {
            return Err(unsupported(
                target.span,
                "nested package Copy projection assignment is not implemented; assign the direct Copy field",
            ));
        }
        if !decl.mutable {
            return Err(error(
                "E-IMMUTABLE-ASSIGNMENT",
                target.span,
                "projected root is not mutable",
            ));
        }
        let mut ty = decl.ty.clone();
        let mut path = Vec::new();
        for member in members.into_iter().rev() {
            match member {
                Written::Field(name, owner, span) => {
                    self.member_owner(&ty, owner, span)?;
                    let (index, field_ty) = field(&ty, name, span)?;
                    path.push((ty, Project::Field(index)));
                    ty = field_ty;
                }
                Written::Index(e) => {
                    let Type::Array { element, .. } = &ty else {
                        return Err(error(
                            "E-INDEX-TYPE",
                            e.span,
                            "indexed place must be an array",
                        ));
                    };
                    let next = (**element).clone();
                    let (index, it) = self.expr(e, None)?;
                    if !it.is_integer() {
                        return Err(error(
                            "E-INDEX-TYPE",
                            e.span,
                            "array index must be an integer",
                        ));
                    }
                    path.push((ty, Project::Index(index)));
                    ty = next;
                }
            }
        }
        let (mut value, _) = self.expr(rhs, Some(&ty))?;
        let mut aggregate =
            self.value(InstructionKind::Load(local), decl.ty.clone(), target.span)?;
        let mut parents = Vec::new();
        for (i, (_, project)) in path.iter().enumerate() {
            parents.push(aggregate);
            if i + 1 < path.len() {
                let next_ty = path[i + 1].0.clone();
                let kind = match project {
                    Project::Field(index) => InstructionKind::ExtractField {
                        aggregate,
                        index: *index,
                    },
                    Project::Index(index) => InstructionKind::ExtractIndex {
                        aggregate,
                        index: *index,
                    },
                };
                aggregate = self.value(kind, next_ty, target.span)?;
            }
        }
        for ((parent_ty, project), aggregate) in path.into_iter().zip(parents).rev() {
            let kind = match project {
                Project::Field(index) => InstructionKind::UpdateField {
                    aggregate,
                    index,
                    value,
                },
                Project::Index(index) => InstructionKind::UpdateIndex {
                    aggregate,
                    index,
                    value,
                },
            };
            value = self.value(kind, parent_ty, target.span)?;
        }
        self.store(local, value, target.span)
    }
    pub(super) fn sequence_hints(&self, value: &CoreExpr) -> BTreeMap<String, Type> {
        let mut hints = BTreeMap::new();
        let (ty, fields) = match &value.kind {
            CoreExprKind::Record { ty, fields } => (Some(ty.clone()), fields),
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                (ty.clone().or_else(|| self.known_type(base)), fields)
            }
            _ => return hints,
        };
        if let Some(ty) = ty {
            let payload = match &ty {
                Type::Managed(payload) => payload.as_ref(),
                other => other,
            };
            for (name, e) in fields {
                if let CoreExprKind::Name(temp) = &e.kind
                    && let Ok((_, t)) = field(payload, name, e.span)
                {
                    hints.insert(temp.clone(), t);
                }
            }
        }
        hints
    }
}

impl Resolver<'_> {
    fn generic_check_names(
        &mut self,
        ty: &Type,
        bindings: &aggregate_templates::Bindings,
        owner: Option<aggregate_templates::Owner>,
        span: Span,
    ) -> R<bool> {
        aggregate_templates::type_work(ty, &self.generic_limits, &mut self.remaining, span)?;
        fn check(
            ty: &Type,
            b: &aggregate_templates::Bindings,
            owner: Option<aggregate_templates::Owner>,
            symbols: &BTreeMap<String, aggregate_templates::Owner>,
            left: &mut usize,
            span: Span,
        ) -> R<bool> {
            aggregate_templates::spend(left, 1, span)?;
            let mut symbolic = false;
            if let Type::Package { name, .. } = ty {
                aggregate_templates::spend(
                    left,
                    name.len()
                        .saturating_mul(symbols.len().saturating_add(1).ilog2() as usize + 2),
                    span,
                )?;
                if let Some(actual) = symbols.get(name) {
                    if Some(*actual) != owner {
                        return Err(error(
                            "E-PRODUCT-TEMPLATE",
                            span,
                            "hidden Product argument belongs to another lexical owner",
                        ));
                    }
                    symbolic = true;
                }
            }
            match ty {
                Type::Named(n) => {
                    if !b.names().any(|name| name == n) {
                        return Err(error(
                            "E-PRODUCT-TEMPLATE",
                            span,
                            "generic Product contains a free type outside its declared scope",
                        ));
                    }
                    symbolic = true;
                }
                Type::Package { fields, .. } | Type::Record { fields, .. } => {
                    for (_, t) in fields {
                        symbolic |= check(t, b, owner, symbols, left, span)?;
                    }
                }
                Type::Tuple(ts) => {
                    for t in ts {
                        symbolic |= check(t, b, owner, symbols, left, span)?;
                    }
                }
                Type::Managed(t)
                | Type::Desc(t)
                | Type::Address { inner: t, .. }
                | Type::OwnedStore { element: t }
                | Type::Mutex { payload: t }
                | Type::MutexGuard { payload: t, .. }
                | Type::Array { element: t, .. }
                | Type::Slice { element: t }
                | Type::Opaque { hidden: t, .. } => {
                    symbolic |= check(t, b, owner, symbols, left, span)?
                }
                Type::ErrorUnion { error, result } => {
                    symbolic |= check(error, b, owner, symbols, left, span)?;
                    symbolic |= check(result, b, owner, symbols, left, span)?;
                }
                Type::Enum { variants, .. } => {
                    for (_, ts) in variants {
                        for t in ts {
                            symbolic |= check(t, b, owner, symbols, left, span)?;
                        }
                    }
                }
                Type::FunctionPointer { parameters, result }
                | Type::ForeignCallback { parameters, result } => {
                    for t in parameters {
                        symbolic |= check(t, b, owner, symbols, left, span)?;
                    }
                    symbolic |= check(result, b, owner, symbols, left, span)?;
                }
                _ => {}
            }
            Ok(symbolic)
        }
        check(
            ty,
            bindings,
            owner,
            &self.generic.symbolic_names,
            &mut self.remaining,
            span,
        )
    }
    fn generic_bindings(
        &mut self,
        owner: Option<aggregate_templates::Owner>,
        span: Span,
    ) -> R<aggregate_templates::Bindings> {
        let Some(owner) = owner else {
            return Ok(aggregate_templates::Bindings::default());
        };
        aggregate_templates::spend(
            &mut self.remaining,
            self.generic.owners.len().saturating_add(1).ilog2() as usize + 2,
            span,
        )?;
        let bindings = self.generic.owners.get(&owner).ok_or_else(|| {
            error(
                "E-PRODUCT-TEMPLATE",
                span,
                "generic Product owner declaration is absent",
            )
        })?;
        aggregate_templates::spend(
            &mut self.remaining,
            bindings
                .names()
                .fold(bindings.callable.len().saturating_add(1), |n, s| {
                    n.saturating_add(s.len())
                }),
            span,
        )?;
        Ok(bindings.clone())
    }
    fn generic_application(&mut self, name: &str, depth: usize, span: Span) -> R<Type> {
        let (module, application, header) = self.generic.applications[name].clone();
        let owner = application.owner.as_ref().map(|o| (o.module, o.function));
        let bindings = self.generic_bindings(owner, span)?;
        aggregate_templates::spend(
            &mut self.remaining,
            name.len()
                .saturating_mul(self.generic_types.len().saturating_add(1).ilog2() as usize + 2),
            span,
        )?;
        if let Some(cached) = self.generic_types.get(name) {
            let Type::Package { name: nominal, .. } = cached else {
                unreachable!()
            };
            if !self.metadata_context
                && self
                    .generic
                    .symbolic_names
                    .get(nominal)
                    .is_some_and(|o| Some(*o) != self.current_owner)
            {
                return Err(error(
                    "E-PRODUCT-TEMPLATE",
                    span,
                    "symbolic Product used outside its exact lexical owner",
                ));
            }
            aggregate_templates::type_work(
                cached,
                &self.generic_limits,
                &mut self.remaining,
                span,
            )?;
            return Ok(cached.clone());
        }
        let saved_owner = self.current_owner;
        aggregate_templates::spend(
            &mut self.remaining,
            self.ordinary_parameter.as_ref().map_or(0, String::len),
            span,
        )?;
        let saved_ordinary = self.ordinary_parameter.clone();
        if self.metadata_context {
            self.current_owner = owner;
            aggregate_templates::spend(
                &mut self.remaining,
                bindings.ordinary.as_ref().map_or(0, String::len),
                span,
            )?;
            self.ordinary_parameter = bindings.ordinary.clone();
        }
        let mut args = Vec::new();
        let mut symbolic = false;
        aggregate_templates::spend(&mut self.remaining, application.arguments.len(), span)?;
        for argument in &application.arguments {
            let ty = match argument {
                CoreAggregateArgument::Concrete(t) => self.ty(t, depth + 1, span)?,
                CoreAggregateArgument::Parameter { index: 0 } => {
                    aggregate_templates::spend(
                        &mut self.remaining,
                        bindings.ordinary.as_ref().map_or(0, String::len),
                        span,
                    )?;
                    Type::Named(bindings.ordinary.clone().ok_or_else(|| {
                        error(
                            "E-PRODUCT-TEMPLATE",
                            span,
                            "Product argument has no ordinary owner binder",
                        )
                    })?)
                }
                _ => {
                    return Err(error(
                        "E-PRODUCT-TEMPLATE",
                        span,
                        "Product argument binder index is invalid",
                    ));
                }
            };
            symbolic |= self.generic_check_names(&ty, &bindings, owner, span)?;
            args.push(ty);
        }
        if symbolic && !self.metadata_context && owner != self.current_owner {
            return Err(error(
                "E-PRODUCT-TEMPLATE",
                span,
                "generic Product argument belongs to another lexical function",
            ));
        }
        let family = (application.template_module, application.template);
        let ty = self.generic_materialize(
            family,
            &args,
            if symbolic { owner } else { None },
            depth + 1,
            span,
        )?;
        let template = self.generic.templates[&family].clone();
        if header.fields.len() != template.fields.len() {
            return Err(error(
                "E-PRODUCT-TEMPLATE",
                span,
                "Product header and original recipe field counts differ",
            ));
        }
        let Type::Package { fields, .. } = &ty else {
            unreachable!()
        };
        for ((actual, recipe), (_, expected)) in
            header.fields.iter().zip(&template.fields).zip(fields)
        {
            if actual.name != recipe.name
                || actual.public != recipe.public
                || actual.manages != recipe.manages
            {
                return Err(error(
                    "E-PRODUCT-TEMPLATE",
                    span,
                    "Product header differs from its original field recipe",
                ));
            }
            let actual = self.field_type(actual, depth + 1)?;
            aggregate_templates::type_work(
                &actual,
                &self.generic_limits,
                &mut self.remaining,
                span,
            )?;
            aggregate_templates::type_work(
                expected,
                &self.generic_limits,
                &mut self.remaining,
                span,
            )?;
            if actual != *expected {
                return Err(error(
                    "E-PRODUCT-TEMPLATE",
                    span,
                    "Product header type differs from its original field recipe",
                ));
            }
        }
        self.current_owner = saved_owner;
        self.ordinary_parameter = saved_ordinary;
        aggregate_templates::type_work(&ty, &self.generic_limits, &mut self.remaining, span)?;
        self.generic
            .resolved
            .insert((module, application.aggregate), args);
        aggregate_templates::spend(
            &mut self.remaining,
            name.len()
                .saturating_mul(self.generic_types.len().saturating_add(1).ilog2() as usize + 2),
            span,
        )?;
        self.generic_types.insert(name.to_owned(), ty.clone());
        Ok(ty)
    }
    fn generic_materialize(
        &mut self,
        family: aggregate_templates::Family,
        args: &[Type],
        owner: Option<aggregate_templates::Owner>,
        depth: usize,
        span: Span,
    ) -> R<Type> {
        self.charge(depth, span)?;
        aggregate_templates::spend(
            &mut self.remaining,
            self.generic_active.len().saturating_add(1),
            span,
        )?;
        if !self.generic_active.insert(family) {
            return Err(error(
                "E-INFINITE-LAYOUT",
                span,
                "recursive generic Product recipe has infinite layout",
            ));
        }
        let owner = if let Some(owner) = owner {
            let bindings = self.generic_bindings(Some(owner), span)?;
            let mut symbolic = false;
            for arg in args {
                symbolic |= self.generic_check_names(arg, &bindings, Some(owner), span)?;
            }
            if symbolic { Some(owner) } else { None }
        } else {
            None
        };
        let template = self
            .generic
            .templates
            .get(&family)
            .cloned()
            .ok_or_else(|| {
                error(
                    "E-PRODUCT-TEMPLATE",
                    span,
                    "generic Product family is absent",
                )
            })?;
        if args.len() != 1 {
            return Err(error(
                "E-PRODUCT-TEMPLATE",
                span,
                "generic Product needs one complete type argument",
            ));
        }
        aggregate_templates::type_work(&args[0], &self.generic_limits, &mut self.remaining, span)?;
        if !self.store_element_shape(&args[0], depth, span)? {
            return Err(error(
                "E-PRODUCT-TEMPLATE",
                span,
                "generic Product actual type does not satisfy its StoreElement declaration",
            ));
        }
        aggregate_templates::spend(&mut self.remaining, template.fields.len(), span)?;
        let mut fields = Vec::new();
        for f in &template.fields {
            aggregate_templates::spend(
                &mut self.remaining,
                f.name.len().saturating_mul(3).saturating_add(1),
                f.span,
            )?;
            let field = self.generic_recipe(&f.ty, args, owner, depth + 1, f.span)?;
            let field = if f.manages {
                if let Type::Desc(p) = field {
                    Type::Managed(p)
                } else {
                    field
                }
            } else {
                field
            };
            let raw = CoreField {
                name: f.name.clone(),
                ty: field,
                public: f.public,
                manages: f.manages,
                span: f.span,
            };
            let ty = self.field_type(&raw, depth + 1)?;
            if !f.manages && matches!(ty, Type::Managed(_) | Type::Desc(_)) {
                return Err(error(
                    "E-PRODUCT-TEMPLATE",
                    span,
                    "generic Product Managed leaves require an explicit manages declaration",
                ));
            }
            fields.push((f.name.clone(), ty));
        }
        let name = aggregate_templates::canonical(
            family,
            args,
            owner,
            &self.generic_limits,
            &mut self.remaining,
            span,
        )?;
        for arg in args {
            aggregate_templates::type_work(arg, &self.generic_limits, &mut self.remaining, span)?;
        }
        aggregate_templates::spend(
            &mut self.remaining,
            name.len()
                .saturating_add(self.generic.canonical_arguments.len()),
            span,
        )?;
        if let Some((prior_family, prior_args, prior_owner)) =
            self.generic.canonical_arguments.get(&name)
        {
            if *prior_family != family || prior_args != args || *prior_owner != owner {
                return Err(error(
                    "E-PRODUCT-TEMPLATE",
                    span,
                    "generic Product canonical identity collision",
                ));
            }
        } else {
            for arg in args {
                aggregate_templates::type_work(
                    arg,
                    &self.generic_limits,
                    &mut self.remaining,
                    span,
                )?;
            }
            self.generic
                .canonical_arguments
                .insert(name.clone(), (family, args.to_vec(), owner));
        }
        if let Some(owner) = owner {
            aggregate_templates::spend(&mut self.remaining, name.len().saturating_add(1), span)?;
            self.generic.symbolic_names.insert(name.clone(), owner);
        }
        let ty = Type::Package { name, fields };
        packages::product::validate_owned(&ty, &mut self.remaining, self.depth_limit, span)?;
        self.generic_active.remove(&family);
        Ok(ty)
    }
    fn generic_recipe(
        &mut self,
        recipe: &CoreAggregateTypeRecipe,
        args: &[Type],
        owner: Option<aggregate_templates::Owner>,
        depth: usize,
        span: Span,
    ) -> R<Type> {
        self.charge(depth, span)?;
        match recipe {
            CoreAggregateTypeRecipe::AsyncRegistration { environment } => {
                let environment = self.generic_recipe(environment, args, owner, depth + 1, span)?;
                Ok(Type::Desc(Box::new(Type::AsyncRegistration {
                    environment: Box::new(environment),
                })))
            }
            CoreAggregateTypeRecipe::Concrete(t) => {
                // Concrete recipe syntax is in the aggregate declaration, not
                // the caller's T/F scope. It cannot capture a same-spelled binder.
                let saved = self.ordinary_parameter.take();
                let ty = self.ty(t, depth + 1, span)?;
                self.ordinary_parameter = saved;
                self.generic_check_names(
                    &ty,
                    &aggregate_templates::Bindings::default(),
                    None,
                    span,
                )?;
                Ok(ty)
            }
            CoreAggregateTypeRecipe::Parameter { index: 0 } => {
                aggregate_templates::type_work(
                    &args[0],
                    &self.generic_limits,
                    &mut self.remaining,
                    span,
                )?;
                Ok(args[0].clone())
            }
            CoreAggregateTypeRecipe::Parameter { .. } => Err(error(
                "E-PRODUCT-TEMPLATE",
                span,
                "generic Product formal index is invalid",
            )),
            CoreAggregateTypeRecipe::Task {
                result,
                binder_count,
                ports,
            } => {
                let result = self.generic_recipe(result, args, owner, depth + 1, span)?;
                aggregate_templates::spend(
                    &mut self.remaining,
                    ports.len().saturating_add(*binder_count as usize),
                    span,
                )?;
                let mut resolved = Vec::new();
                let mut next = 0u32;
                for port in ports {
                    let pointee =
                        self.generic_recipe(&port.pointee, args, owner, depth + 1, span)?;
                    if !tasks::retained_pointee(&pointee) || port.binder > next {
                        return Err(error(
                            "E-PRODUCT-TEMPLATE",
                            span,
                            "Task recipe retained port has an unsupported actual pointee or class",
                        ));
                    }
                    if port.binder == next {
                        next += 1;
                    }
                    resolved.push(zil_ir::RetainedBorrowPort {
                        leaf: Type::Address {
                            mutable: port.mutable,
                            inner: Box::new(pointee),
                        },
                        mode: if port.mutable {
                            zil_ir::StoredBorrowMode::ExclusiveAddress
                        } else {
                            zil_ir::StoredBorrowMode::SharedAddress
                        },
                        binder: port.binder,
                    });
                }
                if next != *binder_count {
                    return Err(error(
                        "E-PRODUCT-TEMPLATE",
                        span,
                        "Task recipe retained class domain differs from actual ports",
                    ));
                }
                Ok(Type::Desc(Box::new(Type::Task {
                    result: Box::new(result),
                    retained_life: zil_ir::RetainedLifeShape {
                        binder_count: *binder_count,
                        ports: resolved,
                        retained: vec![],
                        nominal_arguments: vec![],
                    },
                })))
            }
            CoreAggregateTypeRecipe::Registration { environment } => {
                let environment = self.generic_recipe(environment, args, owner, depth + 1, span)?;
                Ok(Type::Desc(Box::new(Type::Registration {
                    environment: Box::new(environment),
                })))
            }
            CoreAggregateTypeRecipe::Store { element } => {
                let element = self.generic_recipe(element, args, owner, depth + 1, span)?;
                Ok(Type::Desc(Box::new(Type::OwnedStore {
                    element: Box::new(element),
                })))
            }
            CoreAggregateTypeRecipe::Apply {
                module,
                template,
                arguments,
            } => {
                aggregate_templates::spend(&mut self.remaining, arguments.len(), span)?;
                let mut actual = Vec::new();
                for a in arguments {
                    actual.push(self.generic_recipe(a, args, owner, depth + 1, span)?);
                }
                self.generic_materialize((*module, *template), &actual, owner, depth + 1, span)
            }
        }
    }
}
