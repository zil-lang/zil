//! Resolution and construction of finite Copy records and tuples.
use super::*;
use std::collections::BTreeSet;
use zil_base::ModuleId;

/// Visibility is checked on the actual resolved field indices. It is a source
/// module restriction, not a runtime capability or a change to record layout.
pub(super) struct Visibility<'a> {
    records: BTreeMap<String, (ModuleId, &'a [CoreField])>,
    remaining: usize,
}
impl<'a> Visibility<'a> {
    pub(super) fn new(program: &'a CoreProgram, limits: &Limits) -> CompileResult<Self> {
        let mut result = Self {
            records: BTreeMap::new(),
            remaining: limits.max_analysis_steps,
        };
        for module in &program.modules {
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
        let (Type::Record { name, .. } | Type::Package { name, .. }) = ty else {
            return Ok(());
        };
        self.spend(name.len(), span)?;
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
    pub(super) fn check(
        &mut self,
        function: &TypedFunction,
        module: ModuleId,
    ) -> CompileResult<()> {
        for block in &function.blocks {
            for instruction in &block.instructions {
                self.spend(1, instruction.span).map_err(|e| vec![e])?;
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
    name.starts_with("$module") || name.starts_with("$generic_$module")
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
        let core = resolve_indexed(module.core.clone(), &modules, limits, &mut remaining)?;
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
            if project_symbol(&declaration.name) {
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
            if project_symbol(&declaration.name) {
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
    let mut resolver = Resolver {
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
    for d in &core.aggregates {
        resolver
            .ty(&Type::Named(d.name.clone()), 0, d.span)
            .map_err(|e| vec![e])?;
    }
    let mut resolved_enums = core.enums.clone();
    for declaration in &mut resolved_enums {
        let Type::Enum { variants, .. } = resolver
            .ty(&Type::Named(declaration.name.clone()), 0, declaration.span)
            .map_err(|e| vec![e])?
        else {
            unreachable!()
        };
        declaration.variants = variants;
    }
    // Resolve using the original declaration headers while moving function bodies.
    // State selectors need the unmodified headers, but cloning every body is unnecessary.
    let original_headers: Vec<_> = core
        .functions
        .iter()
        .map(|f| (f.callable_parameters.clone(), f.state_parameters.clone()))
        .collect();
    let mut functions = std::mem::take(&mut core.functions);
    for (f, (callable_parameters, state_parameters)) in functions.iter_mut().zip(&original_headers)
    {
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
    *remaining = resolver.remaining;
    let mut aggregates = core.aggregates.clone();
    for aggregate in &mut aggregates {
        for field in &mut aggregate.fields {
            field.ty = resolver.field_type(field, 0).map_err(|e| vec![e])?;
        }
    }
    *remaining = resolver.remaining;
    core.aggregates = aggregates;
    core.functions = functions;
    core.enums = resolved_enums;
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
    fn ty(&mut self, ty: &Type, depth: usize, span: Span) -> R<Type> {
        self.charge(depth, span)?;
        Ok(match ty {
            Type::Named(name) if self.ordinary_parameter.as_ref() == Some(name) => ty.clone(),
            Type::Named(name) if name == "ByteBuffer" => Type::Desc(Box::new(Type::ByteBuffer)),
            Type::Named(name)
                if self.declarations.contains_key(name)
                    || self.enum_declarations.contains_key(name) =>
            {
                if self.active.contains(name) {
                    return Err(error(
                        "E-INFINITE-LAYOUT",
                        span,
                        "recursive by-value aggregate has infinite layout",
                    ));
                }
                if let Some(cached) = self.cache.get(name) {
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
                self.active.insert(name.clone());
                let result = if let Some(declaration) = self.enum_declarations.get(name).copied() {
                    let mut variants = Vec::new();
                    for (variant, raw_fields) in &declaration.variants {
                        let mut fields = Vec::new();
                        for field in raw_fields {
                            let ty = self.ty(field, depth + 1, declaration.span)?;
                            if !ty.is_copy_value() {
                                return Err(unsupported(
                                    declaration.span,
                                    "enum payload requires a finite Copy value",
                                ));
                            }
                            fields.push(ty);
                        }
                        variants.push((variant.clone(), fields));
                    }
                    Type::Enum {
                        name: format!("@module{}:{}", self.owners[name], name),
                        variants,
                    }
                } else {
                    let d = self.declarations[name];
                    let mut fields = Vec::new();
                    for f in &d.fields {
                        let ty = self.field_type(f, depth + 1)?;
                        if !(ty.is_copy_value()
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
                            return Err(unsupported(
                                f.span,
                                "aggregate field requires finite Copy, an explicit package manages resource, or a complete fixed Package array",
                            ));
                        }
                        fields.push((f.name.clone(), ty));
                    }
                    let name = format!("@module{}:{}", self.owners[name], name);
                    if d.kind == AggregateKind::Package {
                        let ty = Type::Package { name, fields };
                        if packages::product::nested(&ty) {
                            packages::product::validate_owned(
                                &ty,
                                &mut self.remaining,
                                self.depth_limit,
                                span,
                            )?;
                        } else if !d.fields.iter().any(|f| f.manages) {
                            return Err(unsupported(
                                span,
                                "package requires at least one manages field or complete owning child",
                            ));
                        }
                        ty
                    } else {
                        let payload = Type::Record { name, fields };
                        if d.kind == AggregateKind::Resource {
                            Type::Desc(Box::new(payload))
                        } else {
                            payload
                        }
                    }
                };
                self.active.remove(name);
                self.cache.insert(name.clone(), result.clone());
                result
            }
            ty @ Type::Package { .. } if packages::product::nested(ty) => {
                packages::product::validate_owned(ty, &mut self.remaining, self.depth_limit, span)?;
                ty.clone()
            }
            Type::OwnedStore { element } => {
                let element = self.ty(element, depth + 1, span)?;
                if !self.store_element_shape(&element, depth, span)? {
                    return Err(unsupported(
                        span,
                        "Store elements require nonzero complete direct-Managed Packages",
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
                    .map(|t| self.ty(t, depth + 1, span))
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
                        if !*mutable
                            && matches!(payload.as_ref(), Type::Record { .. })
                            && payload.is_copy_value() =>
                    {
                        Type::Managed(payload)
                    }
                    Type::Package { .. }
                        if packages::product::nested(&inner)
                            || (!*mutable && packages::product::supported(&inner)) =>
                    {
                        if *mutable {
                            return Err(unsupported(
                                span,
                                "ordinary Product addresses are shared synchronous inputs",
                            ));
                        }
                        packages::product::validate(
                            &inner,
                            &mut self.remaining,
                            self.depth_limit,
                            span,
                        )?;
                        inner
                    }
                    Type::Package { .. } if self.store_element_shape(&inner, depth, span)? => inner,
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
            Type::ErrorUnion {
                error: failure,
                result,
            } => {
                let failure = self.ty(failure, depth + 1, span)?;
                let result = self.ty(result, depth + 1, span)?;
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
            CorePatternKind::Variant { ty, fields, .. } => {
                *ty = self.ty(ty, depth + 1, p.span)?;
                for field in fields {
                    self.pattern(field, depth + 1)?;
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
            CoreExprKind::Ascribed { ty, value }
            | CoreExprKind::IntegerConvert { target: ty, value } => {
                *ty = self.ty(ty, depth + 1, e.span)?;
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Record { ty, fields } => {
                *ty = match self.ty(ty, depth + 1, e.span)? {
                    Type::Desc(payload) => Type::Managed(payload),
                    other => other,
                };
                for (_, v) in fields {
                    self.expr(v, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                if let Some(t) = ty {
                    *t = self.ty(t, depth + 1, e.span)?;
                }
                self.expr(base, depth + 1)?;
                for (_, v) in fields {
                    self.expr(v, depth + 1)?;
                }
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand) => self.expr(operand, depth + 1)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                for argument in type_arguments {
                    *argument = self.ty(argument, depth + 1, e.span)?;
                }
                self.expr(callee, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.expr(callee, depth + 1)?;
                for e in arguments {
                    self.expr(e, depth + 1)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.expr(object, depth + 1)?;
                self.expr(index, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(endpoint, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for e in values {
                    self.expr(e, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, .. } => self.expr(object, depth + 1)?,
            CoreExprKind::Sequence { bindings, value } => {
                for (_, e) in bindings {
                    self.expr(e, depth + 1)?;
                }
                self.expr(value, depth + 1)?;
            }
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
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            _ => {}
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
        Type::ByteBuffer => local
            .get(&format!("ByteBuffer.{member}"))
            .filter(|s| s.receiver.is_some() && !s.cleanup),
        _ => None,
    }
}

impl Builder<'_, '_> {
    pub(super) fn method_signature(&self, object: &CoreExpr, member: &str) -> Option<&Signature> {
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
        let (first, actual) = if matches!(receiver, Type::Address { .. })
            && !matches!(self.known_type(object), Some(Type::Address { .. }))
        {
            let (value, ty) = self.borrow_place_mode(
                object,
                matches!(receiver, Type::Address { mutable: true, .. }),
            )?;
            (value, ty)
        } else {
            self.argument_expr(object, receiver)?
        };
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
            CoreExprKind::Array(items) => {
                let (mut element, length) = match expected {
                    Some(Type::Array { element, length }) => (Some((**element).clone()), *length),
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
                if !owned && !ty.is_copy_value() {
                    return Err(unsupported(
                        e.span,
                        "array elements require finite Copy values or a nonempty homogeneous Package or self-contained Closure array",
                    ));
                }
                let fields = self.commit_arguments(staged, e.span)?;
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
            CoreExprKind::Index { object, index } => {
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
                if matches!(self.known_type(object), Some(Type::Address { .. }))
                    || dynamic_store::borrow_expression(object)
                {
                    let (address, ty) = self.borrow_place(e)?;
                    let Type::Address { inner, .. } = ty else {
                        unreachable!()
                    };
                    let ty = *inner;
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
                Ok((
                    self.value(
                        InstructionKind::ExtractIndex { aggregate, index },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ))
            }
            CoreExprKind::Tuple(items) => {
                let expected_fields = match expected {
                    Some(Type::Tuple(fs)) if fs.len() == items.len() => Some(fs),
                    _ => None,
                };
                let mut fields = Vec::new();
                let mut types = Vec::new();
                for (i, v) in items.iter().enumerate() {
                    let (id, t) = self.expr(v, expected_fields.map(|fs| &fs[i]))?;
                    fields.push(id);
                    types.push(t);
                }
                let ty = Type::Tuple(types);
                if !ty.is_copy_value() {
                    return Err(unsupported(e.span, "tuple with non-Copy fields"));
                }
                Ok((
                    self.value(
                        InstructionKind::MakeAggregate { fields },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ))
            }
            CoreExprKind::Record { ty, fields } => {
                let payload = if let Type::Managed(payload) = ty {
                    payload.as_ref()
                } else {
                    ty
                };
                let (Type::Record { fields: decl, .. } | Type::Package { fields: decl, .. }) =
                    payload
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
                    let (i, t) = field(payload, name, value.span)?;
                    if slots[i as usize].is_some() {
                        return Err(error(
                            "E-FIELD-DUPLICATE",
                            value.span,
                            "duplicate record field",
                        ));
                    }
                    slots[i as usize] = Some(if matches!(ty, Type::Package { .. }) {
                        let (prepared, actual) = if matches!(t, Type::Closure(_)) {
                            self.check_owned_array_closure(&t, 1, value.span)?;
                            self.owned_array_input(value, Some(&t))?
                        } else {
                            self.argument_expr(value, &t)?
                        };
                        let (prepared, guard) =
                            self.stage_argument(prepared, &actual, value.span)?;
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
                let fields = if matches!(ty, Type::Package { .. }) {
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
                Ok((
                    self.value(
                        if matches!(ty, Type::Managed(_)) {
                            InstructionKind::MakeManaged { fields }
                        } else if matches!(ty, Type::Package { .. }) {
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
            CoreExprKind::RecordUpdate { ty, base, fields } => {
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
                    let (index, t) = field(&actual, name, value.span)?;
                    let value = self.expr(value, Some(&t))?.0;
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
            CoreExprKind::Member { object, member } => {
                if matches!(self.known_type(object), Some(Type::Package { .. }))
                    && !self.indirect_place(e)
                    && self.method_signature(object, member).is_none()
                {
                    return self.package_field(object, member, e.span);
                }
                if matches!(
                    self.known_type(object),
                    Some(Type::Managed(_) | Type::Desc(_))
                ) && !self.indirect_place(e)
                    && self.method_signature(object, member).is_none()
                {
                    return self.resource_field(object, member, e.span);
                }
                if self.method_signature(object, member).is_some() {
                    return Err(error(
                        "E-RECEIVER-METHOD-VALUE",
                        e.span,
                        "instance method value requires an explicit closure",
                    ));
                }
                if matches!(self.known_type(object), Some(Type::Address { .. }))
                    || dynamic_store::borrow_expression(object)
                    || self.indirect_place(e)
                {
                    let (address, ty) = self.borrow_place(e)?;
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
            _ => unreachable!(),
        }
    }
    pub(super) fn assign_field(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<()> {
        enum Written<'a> {
            Field(&'a str),
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
                CoreExprKind::Member { object, member } => {
                    members.push(Written::Field(member));
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
                Written::Field(name) => {
                    let (index, field_ty) = field(&ty, name, target.span)?;
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
