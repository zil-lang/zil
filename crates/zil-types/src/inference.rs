//! A bounded source constraint pass, before CT evaluates any integer. Its output
//! is still raw Core: neither capture legality nor executable safety is granted.
use super::closures::{
    BOOL, F32, F64, FLOAT, Graph, I8, I16, I32, I64, INTEGER, ISIZE, NUMBER, Projection, SCALAR,
    U8, U16, U32, U64, UNIT, USIZE, VALUE, Variable,
};
use super::*;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::rc::Rc;

struct Budget<'a> {
    limits: &'a Limits,
    work: usize,
    nodes: usize,
    ordinary_functions: usize,
    closures: usize,
    inside_store: bool,
    store_shapes: bool,
    direct_type_callee: bool,
    shared_arrays: bool,
    products: aggregate_templates::Products,
    current_module: zil_base::ModuleId,
}
impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            work: limits.max_analysis_steps,
            nodes: limits.max_nodes,
            ordinary_functions: 0,
            closures: 0,
            inside_store: false,
            store_shapes: false,
            direct_type_callee: false,
            shared_arrays: false,
            products: None,
            current_module: zil_base::ModuleId(0),
        }
    }
    fn work(&mut self, count: usize, span: Span) -> R<()> {
        self.work = self.work.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "source closure constraint work budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    fn node(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source closure constraint depth budget exceeded",
                Some(span),
            ));
        }
        self.nodes = self.nodes.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "source closure constraint node budget exceeded",
                Some(span),
            )
        })?;
        self.work(1, span)
    }
    fn closure(&mut self, span: Span) -> R<()> {
        self.closures = self.closures.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "source closure count overflow", Some(span))
        })?;
        self.ordinary_functions
            .checked_add(self.closures)
            .filter(|n| *n <= self.limits.max_functions)
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "source closure function budget exceeded",
                    Some(span),
                )
            })?;
        Ok(())
    }
}

/// Reserve the complete retained template before a new deep snapshot. This
/// reuses the borrowed input walk, so nested statements/types and text all count.
pub(super) fn helper_snapshot_work(function: &CoreFunction, limits: &Limits) -> R<(usize, bool)> {
    let mut budget = Budget::new(limits);
    function_scope_metadata(function.source_scope.as_ref(), function.span, &mut budget)?;
    lifetimes::preflight(function, &mut budget.work)?;
    budget.work(
        function
            .name
            .len()
            .saturating_add(function.parameters.len()),
        function.span,
    )?;
    for parameter in &function.parameters {
        budget.work(parameter.name.len(), parameter.span)?;
        validate_type(&parameter.ty, 0, parameter.span, &mut budget)?;
    }
    validate_type(&function.return_type, 0, function.span, &mut budget)?;
    for parameter in &function.type_parameters {
        budget.work(parameter.name.len().saturating_add(1), function.span)?;
    }
    state_snapshot(&function.state_parameters, function.span, &mut budget)?;
    for contract in &function.protocol_contracts {
        registration::charge_contract(contract, |work| budget.work(work, function.span))?;
    }
    for bound in &function.callable_parameters {
        callable_snapshot(bound, function.span, &mut budget)?;
    }
    if let Some(bound) = &function.callable_return {
        returned_snapshot(bound, function.span, &mut budget)?;
    }
    contains_closure(&function.body, 0, &mut budget)?;
    Ok((limits.max_analysis_steps - budget.work, budget.store_shapes))
}

pub(super) fn signature_snapshot_work(
    name: &str,
    signature: &Signature,
    span: Span,
    limits: &Limits,
) -> R<usize> {
    let mut budget = Budget::new(limits);
    if let Some(life) = &signature.storage_life {
        borrowed_fp::charge_contract(life, |cost| budget.work(cost, span), 1)?;
    }
    if let Some(life) = &signature.life {
        budget.work(life.parameter_lives.len().saturating_add(1), span)?;
    }
    budget.work(name.len().saturating_add(signature.parameters.len()), span)?;
    for ty in &signature.parameters {
        validate_type(ty, 0, span, &mut budget)?;
    }
    validate_type(&signature.result, 0, span, &mut budget)?;
    if let Some(ty) = &signature.receiver {
        validate_type(ty, 0, span, &mut budget)?;
    }
    if let Some(state) = &signature.state_parameter {
        state_snapshot(std::slice::from_ref(state), span, &mut budget)?;
    }
    for bound in &signature.callable {
        callable_snapshot(bound, span, &mut budget)?;
    }
    if let Some(parameter) = &signature.type_parameter {
        budget.work(parameter.name.len().saturating_add(1), span)?;
    }
    if let Some(bound) = &signature.callable_return {
        returned_snapshot(bound, span, &mut budget)?;
    }
    Ok(limits.max_analysis_steps - budget.work)
}

pub(super) fn type_snapshot_work(ty: &Type, span: Span, limits: &Limits) -> R<usize> {
    let mut budget = Budget::new(limits);
    validate_type(ty, 0, span, &mut budget)?;
    Ok(limits.max_analysis_steps - budget.work)
}

/// Reserve the exact owned Core expression before retaining a pattern place.
pub(super) fn charge_expression_snapshot(
    e: &CoreExpr,
    limits: &Limits,
    remaining: &mut usize,
) -> R<()> {
    let mut budget = Budget::new(limits);
    budget.work = budget.work.min(*remaining);
    let before = budget.work;
    let result = expression_has_closure(e, 0, &mut budget);
    *remaining = remaining.checked_sub(before - budget.work).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "expression snapshot caller allowance exceeded",
            Some(e.span),
        )
    })?;
    result.map(|_| ())
}

/// Borrow only the caller's currently remaining allowance. Charge successful
/// visits even when planning stops early; a failed planner cannot refund work.
#[track_caller]
pub(super) fn charge_type_snapshot(
    ty: &Type,
    span: Span,
    limits: &Limits,
    remaining: &mut usize,
    copies: usize,
) -> R<()> {
    let location = std::panic::Location::caller();
    let allowance = remaining.checked_div(copies).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "invalid type snapshot multiplier", Some(span))
    })?;
    let mut budget = Budget::new(limits);
    budget.work = budget.work.min(allowance);
    let before = budget.work;
    let result = validate_type(ty, 0, span, &mut budget);
    let consumed = before - budget.work;
    let debit = consumed.checked_mul(copies).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "type snapshot debit overflow", Some(span))
    })?;
    *remaining = remaining.checked_sub(debit).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "type snapshot caller allowance exceeded",
            Some(span),
        )
    })?;
    result.map_err(|mut diagnostic| {
        diagnostic.notes.push(format!("complete type snapshot at {}:{}: copies {}, nodes visited {}, work consumed {}, caller remaining {}, type constructor {:?}", location.file(), location.line(), copies, limits.max_nodes.saturating_sub(budget.nodes), debit, *remaining, std::mem::discriminant(ty)));
        diagnostic
    })
}

fn callable_snapshot(bound: &CoreCallableParameter, span: Span, budget: &mut Budget<'_>) -> R<()> {
    helpers::source_life::reserve(bound, budget.limits, &mut budget.work, 2, span)?;
    if let Some(life) = &bound.life {
        budget.work(borrowed_fp::metadata_work(life), span)?;
    }
    budget.work(
        bound
            .generic_name
            .len()
            .saturating_add(bound.parameters.len()),
        span,
    )?;
    let before = budget.work;
    for ty in &bound.parameters {
        validate_type(ty, 0, span, budget)?;
    }
    validate_type(&bound.result, 0, span, budget)?;
    if let Some(life) = &bound.life {
        // The local depth/work walk must precede recursive eligibility even on
        // raw mixed State/callable headers. Its complete cost also bounds the
        // ensuing eligibility traversal; reserve that second use explicitly.
        budget.work(before - budget.work, span)?;
        borrowed_fp::qualified_inputs(life, &bound.parameters, &bound.result, |work| {
            budget.work(work, span)
        })?;
        if bound.source_life.is_some() {
            helpers::source_life::relation(bound, span)?;
        } else {
            borrowed_fp::relation(&bound.parameters, &bound.result, life, span)?;
        }
    }
    Ok(())
}

fn state_snapshot(states: &[CoreStateParameter], span: Span, budget: &mut Budget<'_>) -> R<()> {
    if states.is_empty() {
        return Ok(());
    }
    let before = budget.work;
    budget.work(states.len(), span)?;
    for state in states {
        budget.work(state.bindings.len().saturating_add(1), span)?;
        for binding in &state.bindings {
            budget.work(
                binding
                    .generic_name
                    .len()
                    .saturating_add(binding.fields.len())
                    .saturating_add(binding.parameters.len())
                    .saturating_add(1),
                span,
            )?;
            for parameter in &binding.parameters {
                validate_type(parameter, 0, span, budget)?;
            }
            validate_type(&binding.result, 0, span, budget)?;
        }
    }
    // Both the public source declaration and its private bound view are copied.
    // Reserve those snapshots before either allocation, including selectors.
    budget.work((before - budget.work).saturating_mul(2), span)
}

/// Infer closure-related source types without executing CT or producing a
/// verified artifact. The caller must still run CT, ordinary types and Verify.
pub fn infer_source_closures(core: &CoreModule, limits: &Limits) -> CompileResult<CoreModule> {
    let mut budget = Budget::new(limits);
    if core.semantic.is_some() {
        crate::semantics::well_formed::originals(&[core], limits, &mut budget.work)
            .map_err(|e| vec![e])?;
    }
    closures::reserve_source_contexts(&core.closure_life_contexts, limits, &mut budget.work, 2)
        .map_err(|e| vec![e])?;
    unconsumed_inventory(core).map_err(|e| vec![e])?;
    budget.current_module = core.module_id;
    budget.ordinary_functions = core.functions.len();
    aggregate_templates::Index::new(&[core], limits, &mut budget.work).map_err(|e| vec![e])?;
    let table = headers(core, None, 0, &mut budget).map_err(|e| vec![e])?;
    let flags = core
        .functions
        .iter()
        .map(|f| {
            contains_closure(&f.body, 0, &mut budget)
                .map(|found| found || !f.state_parameters.is_empty())
        })
        .collect::<R<Vec<_>>>()
        .map_err(|e| vec![e])?;
    names::check(core, limits).map_err(|e| vec![e])?;
    let normalized = if flags.iter().any(|f| *f)
        || !core.aggregate_templates.is_empty()
        || !core.aggregate_applications.is_empty()
    {
        Some(aggregates::resolve(
            snapshot_context_module(core, limits, &mut budget.work).map_err(|e| vec![e])?,
            limits,
        )?)
    } else {
        None
    };
    let core = normalized.as_ref().unwrap_or(core);
    budget.products =
        aggregate_templates::origins(&[core], limits, &mut budget.work).map_err(|e| vec![e])?;
    let table = if normalized.is_some() {
        headers(core, None, 0, &mut budget).map_err(|e| vec![e])?
    } else {
        table
    };
    validate_module_metadata(core, &mut budget).map_err(|e| vec![e])?;
    let enum_types = enums::definitions(core, limits, &mut budget.work).map_err(|e| vec![e])?;
    let mut functions = Vec::new();
    for (function, flag) in core.functions.iter().zip(flags) {
        functions.push(
            prepare(
                function,
                &table,
                None,
                closures::Catalog {
                    enums: Some(&enum_types),
                    dyn_interfaces: &core.dyn_interfaces,
                    dyn_declarations: &core.dyn_declarations,
                },
                flag,
                &mut budget,
            )
            .map_err(|e| vec![e])?
            .into_owned(),
        );
    }
    crate::dyn_metadata::snapshot(core, limits, &mut budget.work, 2).map_err(|e| vec![e])?;
    Ok(copy_module_metadata(core, functions))
}

/// The project version uses the established ModuleId/alias graph. Generated
/// closure entries are not inserted into the graph or its exported interface.
pub fn infer_program_closures(
    program: &CoreProgram,
    limits: &Limits,
) -> CompileResult<CoreProgram> {
    infer_program_closures_borrowed(program, limits).map(Cow::into_owned)
}

pub(super) fn infer_program_closures_borrowed<'a>(
    program: &'a CoreProgram,
    limits: &Limits,
) -> CompileResult<Cow<'a, CoreProgram>> {
    infer_program_closures_borrowed_with_frozen(program, limits, None)
}
pub(super) fn infer_program_closures_borrowed_with_frozen<'a>(
    program: &'a CoreProgram,
    limits: &Limits,
    frozen: Option<&semantics::frozen_context::FrozenSource>,
) -> CompileResult<Cow<'a, CoreProgram>> {
    let original = program;
    let mut budget = Budget::new(limits);
    if program
        .modules
        .iter()
        .any(|module| module.core.semantic.is_some())
    {
        budget
            .work(program.modules.len().saturating_add(1), Span::default())
            .map_err(|e| vec![e])?;
        let modules: Vec<_> = program.modules.iter().map(|module| &module.core).collect();
        crate::semantics::well_formed::originals_with_frozen(
            &modules,
            frozen,
            limits,
            &mut budget.work,
        )
        .map_err(|e| vec![e])?;
    }
    let mut tables = BTreeMap::new();
    if program.modules.iter().any(|m| {
        !m.core.aggregate_templates.is_empty() || !m.core.aggregate_applications.is_empty()
    }) {
        budget
            .work(program.modules.len(), Span::default())
            .map_err(|e| vec![e])?;
        let modules = program.modules.iter().map(|m| &m.core).collect::<Vec<_>>();
        aggregate_templates::Index::new(&modules, limits, &mut budget.work).map_err(|e| vec![e])?;
    }
    let mut count = 0usize;
    if program.modules.len() > limits.max_nodes {
        return Err(vec![Diagnostic::limit(
            Phase::Types,
            "source closure module budget exceeded",
            None,
        )]);
    }
    for module in &program.modules {
        closures::reserve_source_contexts(
            &module.core.closure_life_contexts,
            limits,
            &mut budget.work,
            1,
        )
        .map_err(|e| vec![e])?;
        unconsumed_inventory(&module.core).map_err(|e| vec![e])?;
        count = count
            .checked_add(module.core.functions.len())
            .filter(|n| *n <= limits.max_functions)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Types,
                    "source closure function budget exceeded",
                    None,
                )]
            })?;
        let metadata_span = module
            .core
            .functions
            .first()
            .map(|f| f.span)
            .unwrap_or_default();
        budget
            .work(module.exports.len(), metadata_span)
            .map_err(|e| vec![e])?;
        budget
            .work(module.imports.len(), metadata_span)
            .map_err(|e| vec![e])?;
        budget
            .work(module.core.functions.len(), metadata_span)
            .map_err(|e| vec![e])?;
        if module.exports.len() > limits.max_nodes || module.imports.len() > limits.max_nodes {
            return Err(vec![Diagnostic::limit(
                Phase::Types,
                "source module metadata budget exceeded",
                None,
            )]);
        }
        let exports: BTreeSet<_> = module.exports.iter().copied().collect();
        let ids: BTreeSet<_> = module.core.functions.iter().map(|f| f.id).collect();
        if exports.len() != module.exports.len()
            || ids.len() != module.core.functions.len()
            || !exports.is_subset(&ids)
        {
            return Err(vec![Diagnostic::error(
                Phase::Types,
                "E-MODULE-EXPORT",
                None,
                "function identities and exports must refer to unique declarations",
            )]);
        }
        let table = headers(
            &module.core,
            Some(&exports),
            count - module.core.functions.len(),
            &mut budget,
        )
        .map_err(|e| vec![e])?;
        if tables.insert(module.core.module_id, table).is_some() {
            return Err(vec![Diagnostic::error(
                Phase::Types,
                "E-MODULE-ID",
                None,
                "duplicate module identity",
            )]);
        }
    }
    if !tables.contains_key(&program.root) {
        return Err(vec![Diagnostic::error(
            Phase::Types,
            "E-MODULE-ROOT",
            None,
            "project root module does not exist",
        )]);
    }
    budget.ordinary_functions = count;
    let mut flags = Vec::new();
    for module in &program.modules {
        flags.push(
            module
                .core
                .functions
                .iter()
                .map(|f| {
                    contains_closure(&f.body, 0, &mut budget)
                        .map(|found| found || !f.state_parameters.is_empty())
                })
                .collect::<R<Vec<_>>>()
                .map_err(|e| vec![e])?,
        );
    }
    names::check_modules(program.modules.iter().map(|m| &m.core), limits).map_err(|e| vec![e])?;
    let normalized = if flags.iter().flatten().any(|f| *f)
        || program.modules.iter().any(|m| {
            !m.core.aggregate_templates.is_empty() || !m.core.aggregate_applications.is_empty()
        }) {
        let normalized = aggregates::resolve_program(program, limits)?;
        let mut offset = 0;
        for module in &normalized.modules {
            let exports = module.exports.iter().copied().collect();
            tables.insert(
                module.core.module_id,
                headers(&module.core, Some(&exports), offset, &mut budget).map_err(|e| vec![e])?,
            );
            offset += module.core.functions.len();
        }
        Some(normalized)
    } else {
        None
    };
    let program = normalized.as_ref().unwrap_or(program);
    if program
        .modules
        .iter()
        .any(|m| !m.core.aggregate_applications.is_empty())
    {
        budget
            .work(program.modules.len(), Span::default())
            .map_err(|e| vec![e])?;
        let modules = program.modules.iter().map(|m| &m.core).collect::<Vec<_>>();
        budget.products = aggregate_templates::origins(&modules, limits, &mut budget.work)
            .map_err(|e| vec![e])?;
    }
    let mut modules = Vec::new();
    let mut enum_work = limits.max_analysis_steps;
    for (module, flags) in program.modules.iter().zip(flags) {
        let mut imports = program::Imports {
            aliases: BTreeMap::new(),
            signatures: &tables,
        };
        for import in &module.imports {
            budget
                .work(import.alias.len(), import.span)
                .map_err(|e| vec![e])?;
            budget.work(1, import.span).map_err(|e| vec![e])?;
            if !tables.contains_key(&import.module) {
                return Err(vec![error(
                    "E-MODULE-NOT-FOUND",
                    import.span,
                    "import refers to an absent module",
                )]);
            }
            if imports
                .aliases
                .insert(import.alias.clone(), import.module)
                .is_some()
            {
                return Err(vec![error(
                    "E-MODULE-ALIAS",
                    import.span,
                    "duplicate module alias",
                )]);
            }
        }
        let table = &tables[&module.core.module_id];
        validate_module_metadata(&module.core, &mut budget).map_err(|e| vec![e])?;
        let enum_types = enums::project_definitions(program, &module.core, &mut enum_work, limits)?;
        let mut functions = Vec::new();
        budget.current_module = module.core.module_id;
        for (function, flag) in module.core.functions.iter().zip(flags) {
            functions.push(
                prepare(
                    function,
                    table,
                    Some(&imports),
                    closures::Catalog {
                        enums: Some(&enum_types),
                        dyn_interfaces: &module.core.dyn_interfaces,
                        dyn_declarations: &module.core.dyn_declarations,
                    },
                    flag,
                    &mut budget,
                )
                .map_err(|e| vec![e])?,
            );
        }
        crate::dyn_metadata::snapshot(&module.core, limits, &mut budget.work, 2)
            .map_err(|e| vec![e])?;
        modules.push((module, functions));
    }
    // All header/name/import/metadata/dependency checks and logical charges ran
    // in the original order. Borrow only when no transformation ran anywhere.
    if normalized.is_none()
        && modules
            .iter()
            .all(|(_, functions)| functions.iter().all(|f| matches!(f, Cow::Borrowed(_))))
    {
        return Ok(Cow::Borrowed(original));
    }
    Ok(Cow::Owned(CoreProgram {
        root: program.root,
        source_fingerprint: program.source_fingerprint,
        modules: modules
            .into_iter()
            .map(|(module, functions)| ProgramModule {
                core: copy_module_metadata(
                    &module.core,
                    functions.into_iter().map(Cow::into_owned).collect(),
                ),
                imports: module.imports.clone(),
                exports: module.exports.clone(),
            })
            .collect(),
    }))
}

fn headers(
    core: &CoreModule,
    exports: Option<&BTreeSet<zil_base::NodeId>>,
    offset: usize,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<String, Signature>> {
    if core.schema_version != SCHEMA_VERSION {
        return Err(Diagnostic::error(
            Phase::Types,
            "E-SCHEMA-VERSION",
            None,
            "unknown Core schema",
        ));
    }
    unconsumed_inventory(core)?;
    if core.functions.len() > budget.limits.max_functions {
        return Err(Diagnostic::limit(
            Phase::Types,
            "source closure function budget exceeded",
            None,
        ));
    }
    validate_module_metadata(core, budget)?;
    let mut table = BTreeMap::new();
    for (index, function) in core.functions.iter().enumerate() {
        for contract in &function.protocol_contracts {
            registration::charge_contract(contract, |work| budget.work(work, function.span))?;
        }
        lifetimes::preflight(function, &mut budget.work)?;
        if let Some(life) = &function.storage_life {
            borrowed_fp::charge_contract(life, |cost| budget.work(cost, function.span), 1)?;
        }
        if let Some(life) = &function.life {
            budget.work(
                life.binders
                    .len()
                    .saturating_add(life.parameter_lives.len())
                    .saturating_add(1),
                function.span,
            )?;
        }
        state_snapshot(&function.state_parameters, function.span, budget)?;
        if !function.state_parameters.is_empty() && !function.callable_parameters.is_empty() {
            // The mixed header compares both declarations and the actual
            // owned parameter types. Reserve these complete inputs first.
            budget.work(
                function
                    .callable_parameters
                    .len()
                    .saturating_add(function.parameters.len()),
                function.span,
            )?;
            for bound in &function.callable_parameters {
                callable_snapshot(bound, function.span, budget)?;
            }
            for parameter in &function.parameters {
                validate_type(&parameter.ty, 0, parameter.span, budget)?;
            }
            validate_type(&function.return_type, 0, function.span, budget)?;
        }
        state_origins::validate_header(function)?;
        if !function.type_parameters.is_empty() {
            budget.work(function.type_parameters.len(), function.span)?;
            store_templates::validate_header(function)?;
            budget.work(
                function.type_parameters[0]
                    .name
                    .len()
                    .saturating_add(core.functions.len()),
                function.span,
            )?;
            if core
                .functions
                .iter()
                .filter(|f| f.id == function.id)
                .count()
                != 1
            {
                return Err(error(
                    "E-TYPE-PARAMETERS",
                    function.span,
                    "ordinary template definition identity is duplicated",
                ));
            }
        }
        function_scope_metadata(function.source_scope.as_ref(), function.span, budget)?;
        budget.work(function.name.len(), function.span)?;
        if let Some(foreign) = &function.foreign {
            budget.work(
                foreign
                    .abi
                    .len()
                    .saturating_add(foreign.symbol.len())
                    .saturating_add(foreign.noescape_parameters.len())
                    .saturating_add(1),
                function.span,
            )?;
        }
        budget.work(function.parameters.len(), function.span)?;
        budget.work(1, function.span)?;
        let id = offset
            .checked_add(index)
            .and_then(|i| u32::try_from(i).ok())
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "source function identity overflow",
                    Some(function.span),
                )
            })?;
        for bound in &function.callable_parameters {
            helpers::source_life::reserve(
                bound,
                budget.limits,
                &mut budget.work,
                2,
                function.span,
            )?;
            budget.work(
                bound
                    .generic_name
                    .len()
                    .saturating_add(bound.parameters.len())
                    .saturating_add(1),
                function.span,
            )?;
            for ty in &bound.parameters {
                validate_type(ty, 0, function.span, budget)?;
            }
            validate_type(&bound.result, 0, function.span, budget)?;
        }
        if let Some(bound) = &function.callable_return {
            returned_snapshot(bound, function.span, budget)?;
        }
        validate_type(&function.return_type, 0, function.span, budget)?;
        for parameter in &function.parameters {
            budget.work(parameter.name.len(), parameter.span)?;
            validate_type(&parameter.ty, 0, parameter.span, budget)?;
        }
        if let Some(receiver) = &function.receiver {
            validate_type(&receiver.ty, 0, function.span, budget)?;
        }
        if let Some(bindings) = &function.retained_return_sources {
            charge_return_sources(bindings, budget.limits, function.span, |n| {
                budget.work(n, function.span)
            })?;
        }
        // These raw headers provide context only. Formal signature/resource
        // checks run afterwards and remain the authority for admission.
        table.insert(
            function.name.clone(),
            Signature {
                retained_return_sources: function.retained_return_sources.clone(),
                storage_life: function.storage_life.clone(),
                life: function.life.as_ref().map(borrowed_fp::from_life),
                callable: state_origins::bounds(function).into_owned(),
                state_parameter: function.state_parameters.first().cloned(),
                callable_return: function.callable_return.clone(),
                type_parameter: function.type_parameters.first().cloned(),
                template: (!function.callable_parameters.is_empty()
                    || !function.type_parameters.is_empty()
                    || !function.state_parameters.is_empty())
                .then_some((core.module_id, function.id)),
                foreign: function.foreign.is_some(),
                scoped_callback: function.foreign.as_ref().and_then(|f| f.scoped_callback),
                cleanup: core
                    .aggregates
                    .iter()
                    .any(|a| a.teardown.as_ref() == Some(&function.name)),
                id: (function.type_parameters.is_empty() && function.state_parameters.is_empty())
                    .then_some(FunctionId(id)),
                parameters: function.parameters.iter().map(|p| p.ty.clone()).collect(),
                result: function.return_type.clone(),
                result_mode: function.result_mode,
                public: exports.is_none_or(|e| e.contains(&function.id)),
                receiver: function.receiver.as_ref().map(|r| r.ty.clone()),
            },
        );
    }
    Ok(table)
}

fn prepare<'a>(
    function: &'a CoreFunction,
    signatures: &BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'_>>,
    catalog: closures::Catalog<'_>,
    has_closure: bool,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, CoreFunction>> {
    if let Some(products) = &budget.products {
        budget.work = budget.work.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "generic Product caller binding work exceeded",
                Some(function.span),
            )
        })?;
        aggregate_templates::source_owner(products, (budget.current_module, function.id));
    }
    // Scalar-only functions retain their existing inference/default behavior.
    if !has_closure && function.type_parameters.is_empty() && function.state_parameters.is_empty() {
        let dependencies = retained::dependencies(
            function,
            signatures,
            imports,
            &mut budget.work,
            budget.limits,
        )?;
        if dependencies.is_empty() {
            return Ok(Cow::Borrowed(function));
        }
    }
    let mut solver = Solver::new(function, signatures, imports, catalog, budget)?;
    solver.block(&function.body, 0)?;
    solver.finish(function).map(Cow::Owned)
}

fn copy_module_metadata(core: &CoreModule, functions: Vec<CoreFunction>) -> CoreModule {
    // Every caller reserves the full metadata snapshot before this copy.
    // Inference changes physical shapes, not the original semantic program.
    CoreModule {
        semantic: core.semantic.clone(),
        life_declarations: core.life_declarations.clone(),
        life_instances: core.life_instances.clone(),
        closure_life_contexts: core.closure_life_contexts.clone(),
        dyn_declarations: core.dyn_declarations.clone(),
        dyn_interfaces: core.dyn_interfaces.clone(),
        dyn_vtables: core.dyn_vtables.clone(),
        aggregate_templates: core.aggregate_templates.clone(),
        aggregate_applications: core.aggregate_applications.clone(),
        schema_version: core.schema_version,
        module_id: core.module_id,
        source_fingerprint: core.source_fingerprint,
        functions,
        aggregates: core.aggregates.clone(),
        enums: core.enums.clone(),
        unsupported: core.unsupported.clone(),
    }
}
/// Clone the actual role in its existing function-vector traversal, paying
/// attached context before that role is copied. No discovery scan or debit is
/// introduced for None; the remaining original fields retain their old copy.
pub(super) fn snapshot_callable_parameters(
    bounds: &[CoreCallableParameter],
    limits: &Limits,
    left: &mut usize,
    span: Span,
) -> R<Vec<CoreCallableParameter>> {
    let mut result = Vec::with_capacity(bounds.len());
    for bound in bounds {
        helpers::source_life::reserve(bound, limits, left, 1, span)?;
        result.push(bound.clone());
    }
    Ok(result)
}
pub(super) fn charge_return_sources(
    bindings: &[StorageSourceBinding],
    limits: &Limits,
    span: Span,
    mut spend: impl FnMut(usize) -> R<()>,
) -> R<()> {
    let fail = || {
        Diagnostic::limit(
            Phase::Types,
            "retained return source metadata limit exceeded",
            Some(span),
        )
    };
    let mut nodes = bindings.len().checked_add(1).ok_or_else(fail)?;
    for binding in bindings {
        if binding.position.path.len() > limits.max_call_depth
            || binding.sources.len() > limits.max_nodes
        {
            return Err(fail());
        }
        nodes = nodes
            .checked_add(binding.position.path.len())
            .and_then(|n| n.checked_add(binding.sources.len()))
            .ok_or_else(fail)?;
        for atom in &binding.sources {
            let source = crate::exact_storage_source(atom)?;
            if source.path.len() > limits.max_call_depth {
                return Err(fail());
            }
            nodes = nodes
                .checked_add(source.path.len())
                .and_then(|n| n.checked_add(1))
                .ok_or_else(fail)?;
        }
    }
    if nodes > limits.max_nodes {
        return Err(fail());
    }
    spend(nodes.checked_mul(8).ok_or_else(fail)?)
}

pub(super) fn snapshot_context_function(
    function: &CoreFunction,
    limits: &Limits,
    left: &mut usize,
) -> R<CoreFunction> {
    charge_function_scope_snapshot(function.source_scope.as_ref(), function.span, limits, left)?;
    for contract in &function.protocol_contracts {
        registration::charge_contract(contract, |work| {
            *left = left.checked_sub(work).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "protocol context snapshot budget exceeded",
                    Some(function.span),
                )
            })?;
            Ok(())
        })?;
    }
    let callable_parameters =
        snapshot_callable_parameters(&function.callable_parameters, limits, left, function.span)?;
    if let Some(bindings) = &function.retained_return_sources {
        charge_return_sources(bindings, limits, function.span, |n| {
            *left = left.checked_sub(n).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "retained result snapshot budget exceeded",
                    Some(function.span),
                )
            })?;
            Ok(())
        })?;
    }
    Ok(CoreFunction {
        source_scope: function.source_scope.clone(),
        dyn_instance: function.dyn_instance.clone(),
        retained_return_sources: function.retained_return_sources.clone(),
        storage_life: function.storage_life.clone(),
        call_kind: function.call_kind,
        resume_bound: function.resume_bound,
        life: function.life.clone(),
        type_parameters: function.type_parameters.clone(),
        state_parameters: function.state_parameters.clone(),
        protocol_contracts: function.protocol_contracts.clone(),
        callable_return: function.callable_return.clone(),
        id: function.id,
        name: function.name.clone(),
        callable_parameters,
        foreign: function.foreign.clone(),
        parameters: function.parameters.clone(),
        return_type: function.return_type.clone(),
        result_mode: function.result_mode,
        body: function.body.clone(),
        receiver: function.receiver.clone(),
        span: function.span,
    })
}
pub(super) fn snapshot_context_module(
    core: &CoreModule,
    limits: &Limits,
    left: &mut usize,
) -> R<CoreModule> {
    crate::dyn_metadata::snapshot(core, limits, left, 2)?;
    let mut functions = Vec::with_capacity(core.functions.len());
    for function in &core.functions {
        functions.push(snapshot_context_function(function, limits, left)?);
    }
    Ok(copy_module_metadata(core, functions))
}

pub(super) fn life_inventory_snapshot_work(core: &CoreModule, limits: &Limits) -> R<usize> {
    life_inventory_snapshot_billing(core, limits).map(|(work, _)| work)
}
pub(super) fn life_inventory_snapshot_billing(
    core: &CoreModule,
    limits: &Limits,
) -> R<(usize, bool)> {
    let mut budget = Budget::new(limits);
    if !core.closure_life_contexts.is_empty() {
        closures::reserve_source_contexts(
            &core.closure_life_contexts,
            limits,
            &mut budget.work,
            1,
        )?;
    }
    validate_life_inventory_parts(&core.life_declarations, &core.life_instances, &mut budget)?;
    Ok((
        limits.max_analysis_steps - budget.work,
        budget.shared_arrays,
    ))
}
pub(super) fn life_table_snapshot_work(
    declarations: &[LifeNominalDeclaration],
    instances: &[LifeNominalInstance],
    limits: &Limits,
) -> R<usize> {
    let mut budget = Budget::new(limits);
    validate_life_inventory_parts(declarations, instances, &mut budget)?;
    Ok(limits.max_analysis_steps - budget.work)
}

fn validate_life_inventory(core: &CoreModule, budget: &mut Budget<'_>) -> R<()> {
    validate_life_inventory_parts(&core.life_declarations, &core.life_instances, budget)
}
fn validate_life_inventory_parts(
    declarations: &[LifeNominalDeclaration],
    instances: &[LifeNominalInstance],
    budget: &mut Budget<'_>,
) -> R<()> {
    let span = Span::default();
    for declaration in declarations {
        budget.node(0, span)?;
        budget.work(declaration.identity.package.len(), span)?;
        for component in declaration
            .identity
            .module_path
            .iter()
            .chain(&declaration.identity.declaration_path)
        {
            budget.node(0, span)?;
            budget.work(component.len(), span)?;
        }
        for _ in &declaration.identity.generation_path {
            budget.node(0, span)?;
        }
        for parameter in &declaration.parameters {
            budget.node(0, span)?;
            if let LifeParameterKind::Const(ty) = parameter {
                validate_type(ty, 0, span, budget)?;
            }
        }
    }
    for instance in instances {
        budget.node(0, span)?;
        validate_type(&instance.ty, 0, span, budget)?;
        for argument in &instance.arguments {
            budget.node(0, span)?;
            match argument {
                LifeNominalActual::Type(ty) => validate_type(ty, 0, span, budget)?,
                LifeNominalActual::QualifiedType { ty, life } => {
                    // The qualification is independent metadata, including
                    // when the complete physical Type also has a BS boundary.
                    // Pay every row and path before the inventory is cloned.
                    validate_type(ty, 0, span, budget)?;
                    budget.node(0, span)?;
                    for slot in &life.slots {
                        budget.node(slot.path.len().saturating_add(1), span)?;
                        budget.work(slot.path.len(), span)?;
                    }
                    for row in &life.retained {
                        budget.node(row.path.len().saturating_add(1), span)?;
                        budget.work(row.path.len(), span)?;
                    }
                    for row in &life.nominal_arguments {
                        budget.node(row.path.len().saturating_add(1), span)?;
                        budget.work(row.path.len(), span)?;
                    }
                }
                LifeNominalActual::Const(Constant::WideString(units)) => {
                    budget.work(units.len(), span)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_module_metadata(core: &CoreModule, budget: &mut Budget<'_>) -> R<()> {
    validate_life_inventory(core, budget)?;
    aggregate_templates::metadata_snapshot(core, budget.limits, &mut budget.work)?;
    for aggregate in &core.aggregates {
        budget.node(0, aggregate.span)?;
        budget.work(aggregate.name.len(), aggregate.span)?;
        budget.work(aggregate.fields.len(), aggregate.span)?;
        for field in &aggregate.fields {
            budget.work(field.name.len(), field.span)?;
            validate_type(&field.ty, 0, field.span, budget)?;
        }
    }
    for declaration in &core.enums {
        budget.node(0, declaration.span)?;
        budget.work(
            declaration
                .name
                .len()
                .saturating_add(declaration.variants.len()),
            declaration.span,
        )?;
        for (name, fields) in &declaration.variants {
            budget.work(name.len().saturating_add(fields.len()), declaration.span)?;
            for ty in fields {
                validate_type(ty, 0, declaration.span, budget)?;
            }
        }
    }
    for feature in &core.unsupported {
        let span = feature.span.unwrap_or_default();
        budget.node(0, span)?;
        budget.work(feature.feature.len(), span)?;
    }
    Ok(())
}
fn validate_type(ty: &Type, depth: usize, span: Span, budget: &mut Budget<'_>) -> R<()> {
    budget.node(depth, span)?;
    tasks::pre_resolution_context(ty, span)?;
    match ty {
        // The concrete interface ID is checked against the complete inventory
        // by the dyn admission pass after ordinary type resolution.
        Type::Dyn { .. } | Type::OwnedDyn { .. } => {}
        Type::BorrowedStorage { storage, life } => {
            for slot in &life.slots {
                budget.node(depth + 1, span)?;
                budget.work(slot.path.len(), span)?;
            }
            for row in &life.retained {
                budget.node(depth + 1, span)?;
                budget.work(row.path.len(), span)?;
            }
            for row in &life.nominal_arguments {
                budget.node(depth + 1, span)?;
                budget.work(row.path.len(), span)?;
            }
            validate_type(storage, depth + 1, span, budget)?;
        }
        Type::Array { element, length } => {
            budget.work(*length as usize, span)?;
            budget.shared_arrays |=
                *length > 0 && matches!(element.as_ref(), Type::Address { mutable: false, .. });
            validate_type(element, depth + 1, span, budget)?;
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            budget.work(name.len(), span)?;
            budget.work(fields.len(), span)?;
            for (name, ty) in fields {
                budget.work(name.len(), span)?;
                validate_type(ty, depth + 1, span, budget)?;
            }
        }
        Type::Enum { name, variants } => {
            budget.work(name.len().saturating_add(variants.len()), span)?;
            for (name, fields) in variants {
                budget.work(name.len().saturating_add(fields.len()), span)?;
                for ty in fields {
                    validate_type(ty, depth + 1, span, budget)?;
                }
            }
        }
        Type::Tuple(elements) => {
            budget.work(elements.len(), span)?;
            for element in elements {
                validate_type(element, depth + 1, span, budget)?;
            }
        }
        Type::OwnedStore { element } => {
            // Reuse the already charged complete type walk to classify the
            // new scheduling case. No extra scan or fee reaches old Stores.
            let previous = budget.inside_store;
            budget.inside_store = true;
            validate_type(element, depth + 1, span, budget)?;
            budget.inside_store = previous;
        }
        Type::Task { result: inner, .. }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
        | Type::Address { inner, .. }
        | Type::Slice { element: inner }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Opaque { hidden: inner, .. } => validate_type(inner, depth + 1, span, budget)?,
        Type::ErrorUnion { error, result } => {
            validate_type(error, depth + 1, span, budget)?;
            validate_type(result, depth + 1, span, budget)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            budget.work(parameters.len(), span)?;
            for parameter in parameters {
                validate_type(parameter, depth + 1, span, budget)?;
            }
            validate_type(result, depth + 1, span, budget)?;
        }
        Type::BorrowedFunctionPointer {
            parameters,
            result,
            life,
        } => {
            budget.work(borrowed_fp::metadata_work(life), span)?;
            for parameter in parameters {
                validate_type(parameter, depth + 1, span, budget)?;
            }
            validate_type(result, depth + 1, span, budget)?;
            borrowed_fp::qualified_inputs(life, parameters, result, |work| {
                budget.work(work, span)
            })?;
            borrowed_fp::relation(parameters, result, life, span)?;
        }
        Type::Named(name) => {
            budget.work(name.len(), span)?;
            budget.store_shapes |= budget.inside_store;
        }
        Type::Closure(_) => budget.store_shapes |= budget.inside_store,
        Type::Unsupported(name) => budget.work(name.len(), span)?,
        _ => {}
    }
    Ok(())
}
fn validate_pattern(pattern: &CorePattern, depth: usize, budget: &mut Budget<'_>) -> R<()> {
    budget.node(depth, pattern.span)?;
    match &pattern.kind {
        CorePatternKind::Constructor { .. }
        | CorePatternKind::RecordConstructor { .. }
        | CorePatternKind::Float(_)
        | CorePatternKind::String(_)
        | CorePatternKind::Character(_) => {
            return Err(error(
                "E-CORE-ELABORATION",
                pattern.span,
                "unelaborated source pattern reached executable Core",
            ));
        }

        CorePatternKind::Name(name) | CorePatternKind::Integer(name) => {
            budget.work(name.len(), pattern.span)?
        }
        CorePatternKind::Variant {
            ty,
            variant,
            fields,
        } => {
            validate_type(ty, depth + 1, pattern.span, budget)?;
            budget.work(variant.len().saturating_add(fields.len()), pattern.span)?;
            for field in fields {
                validate_pattern(field, depth + 1, budget)?;
            }
        }
        CorePatternKind::Record { ty, fields } => {
            validate_type(ty, depth + 1, pattern.span, budget)?;
            for (name, field) in fields {
                budget.work(name.len(), pattern.span)?;
                validate_pattern(field, depth + 1, budget)?;
            }
        }
        CorePatternKind::Reference { pattern, .. } | CorePatternKind::ErrorArm { pattern, .. } => {
            validate_pattern(pattern, depth + 1, budget)?
        }
        CorePatternKind::Tuple(fields) => {
            for field in fields {
                validate_pattern(field, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn charge_returned_snapshot(
    bound: &CoreCallableReturn,
    span: Span,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut budget = Budget::new(limits);
    budget.work = *left;
    returned_metadata(bound, span, &mut budget)?;
    *left = budget.work;
    Ok(())
}
pub(super) fn charge_returned_value_snapshot(
    bound: &CoreCallableReturn,
    span: Span,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut budget = Budget::new(limits);
    budget.work = *left;
    returned_snapshot(bound, span, &mut budget)?;
    *left = budget.work;
    Ok(())
}
pub(super) fn charge_function_scope_snapshot(
    scope: Option<&CoreFunctionSourceScope>,
    span: Span,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut budget = Budget::new(limits);
    budget.work = *left;
    function_scope_metadata(scope, span, &mut budget)?;
    *left = budget.work;
    Ok(())
}
fn function_scope_metadata(
    scope: Option<&CoreFunctionSourceScope>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    let Some(scope) = scope else {
        return Ok(());
    };
    let (nodes, work) = core_function_scope_metadata_cost(
        scope,
        budget.nodes,
        budget.limits.max_call_depth,
        budget.work,
    )
    .ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "function source scope exceeds metadata bounds",
            Some(span),
        )
    })?;
    budget.work(work, span)?;
    budget.nodes = budget.nodes.checked_sub(nodes).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "function source scope exceeds node budget",
            Some(span),
        )
    })?;
    for formal in &scope.parameters {
        if let LifeParameterKind::Const(ty) = formal {
            validate_type(ty, 0, span, budget)?;
        }
    }
    Ok(())
}
fn returned_metadata(bound: &CoreCallableReturn, span: Span, budget: &mut Budget<'_>) -> R<()> {
    let work = callable_return_metadata_work(
        bound,
        budget.limits.max_nodes,
        budget.limits.max_call_depth,
        budget.work,
    )
    .ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "callable return metadata exceeds bounds",
            Some(span),
        )
    })?;
    budget.work(work, span)?;
    if let Some(context) = &bound.source_life {
        for formal in &context.parameters {
            if let LifeParameterKind::Const(ty) = formal {
                validate_type(ty, 0, span, budget)?;
            }
        }
    }
    Ok(())
}
fn returned_snapshot(bound: &CoreCallableReturn, span: Span, budget: &mut Budget<'_>) -> R<()> {
    returned_metadata(bound, span, budget)?;
    budget.work(bound.parameters.len().saturating_add(1), span)?;
    for ty in &bound.parameters {
        validate_type(ty, 0, span, budget)?;
    }
    validate_type(&bound.result, 0, span, budget)
}

fn context_signature(signature: &Signature, budget: &mut Budget<'_>, span: Span) -> R<Signature> {
    if let Some(bindings) = &signature.retained_return_sources {
        charge_return_sources(bindings, budget.limits, span, |n| budget.work(n, span))?;
    }
    if let Some(life) = &signature.storage_life {
        borrowed_fp::charge_contract(life, |work| budget.work(work, span), 1)?;
    }
    // This is the complete context of a static symbol, including ordinary
    // direct calls. Snapshotting it must not impose the first-class callable
    // ABI on unrelated calls in a function that happens to contain a closure.
    // Preserve every type and life relation; scalar() separately requires the
    // actual pointer_type() when a symbol becomes a function value.
    budget.work(signature.parameters.len(), span)?;
    validate_type(&signature.result, 0, span, budget)?;
    if let Some(life) = &signature.life {
        budget.work(life.parameter_lives.len().saturating_add(1), span)?;
    }
    for parameter in &signature.parameters {
        validate_type(parameter, 0, span, budget)?;
    }
    if let Some(state) = &signature.state_parameter {
        state_snapshot(std::slice::from_ref(state), span, budget)?;
    }
    for bound in &signature.callable {
        callable_snapshot(bound, span, budget)?;
    }
    if let Some(bound) = &signature.callable_return {
        returned_snapshot(bound, span, budget)?;
    }
    Ok(signature.clone())
}
fn contains_closure(block: &CoreBlock, depth: usize, budget: &mut Budget<'_>) -> R<bool> {
    budget.node(depth, block.span)?;
    let mut found = false;
    for statement in &block.statements {
        budget.node(depth + 1, statement.span)?;
        match &statement.kind {
            CoreStatementKind::ForEach { .. }
            | CoreStatementKind::CompoundAssign { .. }
            | CoreStatementKind::Yield(_)
            | CoreStatementKind::Loop(_)
            | CoreStatementKind::With { .. }
            | CoreStatementKind::CompileTime(_) => {
                return Err(error(
                    "E-CORE-ELABORATION",
                    statement.span,
                    "unelaborated source statement reached executable Core",
                ));
            }

            CoreStatementKind::TupleBinding {
                pattern,
                ty,
                initializer,
                ..
            } => {
                validate_pattern(pattern, depth + 1, budget)?;
                if let Some(ty) = ty {
                    validate_type(ty, depth + 1, statement.span, budget)?;
                }
                found |= expression_has_closure(initializer, depth + 1, budget)?;
            }
            CoreStatementKind::Binding {
                name,
                ty,
                initializer,
                ..
            } => {
                budget.work(name.len(), statement.span)?;
                if let Some(ty) = ty {
                    validate_type(ty, depth + 1, statement.span, budget)?;
                }
                if let Some(value) = initializer {
                    found |= expression_has_closure(value, depth + 1, budget)?;
                }
            }
            CoreStatementKind::Assign { target, value } => {
                found |= expression_has_closure(target, depth + 1, budget)?;
                found |= expression_has_closure(value, depth + 1, budget)?;
            }
            CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                found |= expression_has_closure(value, depth + 1, budget)?
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                found |= expression_has_closure(condition, depth + 1, budget)?;
                found |= contains_closure(then_body, depth + 1, budget)?;
                if let Some(body) = else_body {
                    found |= contains_closure(body, depth + 1, budget)?;
                }
            }
            CoreStatementKind::While { condition, body } => {
                found |= expression_has_closure(condition, depth + 1, budget)?;
                found |= contains_closure(body, depth + 1, budget)?;
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                validate_pattern(pattern, depth + 1, budget)?;
                found |= expression_has_closure(value, depth + 1, budget)?;
                found |= contains_closure(then_body, depth + 1, budget)?;
                if let Some(body) = else_body {
                    found |= contains_closure(body, depth + 1, budget)?;
                }
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                validate_pattern(pattern, depth + 1, budget)?;
                found |= expression_has_closure(value, depth + 1, budget)?;
                found |= contains_closure(body, depth + 1, budget)?;
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                budget.work(name.len(), statement.span)?;
                found |= expression_has_closure(start, depth + 1, budget)?;
                found |= expression_has_closure(end, depth + 1, budget)?;
                found |= contains_closure(body, depth + 1, budget)?;
            }
            CoreStatementKind::Match { value, arms } => {
                found |= expression_has_closure(value, depth + 1, budget)?;
                for arm in arms {
                    validate_pattern(&arm.pattern, depth + 1, budget)?;
                    found |= contains_closure(&arm.body, depth + 1, budget)?;
                }
            }
            CoreStatementKind::Block(body)
            | CoreStatementKind::Unsafe(body)
            | CoreStatementKind::Defer(body) => found |= contains_closure(body, depth + 1, budget)?,
            CoreStatementKind::Unsupported(feature) => {
                budget.work(feature.len(), statement.span)?
            }
            _ => {}
        }
    }
    Ok(found)
}
fn expression_has_closure(e: &CoreExpr, depth: usize, budget: &mut Budget<'_>) -> R<bool> {
    budget.node(depth, e.span)?;
    let direct_type_callee = std::mem::take(&mut budget.direct_type_callee);
    match &e.kind {
        CoreExprKind::Spread(_)
        | CoreExprKind::Group(_)
        | CoreExprKind::Character(_)
        | CoreExprKind::Range { .. }
        | CoreExprKind::Pipeline { .. }
        | CoreExprKind::RecordConstructor { .. }
        | CoreExprKind::Splice(_)
        | CoreExprKind::CompileTimeBlock(_)
        | CoreExprKind::SyntaxQuote { .. }
        | CoreExprKind::SyntaxIsland { .. }
        | CoreExprKind::GenericApply { .. } => {
            return Err(error(
                "E-CORE-ELABORATION",
                e.span,
                "unelaborated source expression reached executable Core",
            ));
        }

        CoreExprKind::Literal(Constant::WideString(_)) => {
            expression_has_closure_literal(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Ascribed { .. }
        | CoreExprKind::IntegerConvert { .. }
        | CoreExprKind::NumericConvert { .. } => {
            expression_has_closure_ascribed(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Closure { .. } => {
            expression_has_closure_closure(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::MakeDynBorrow { .. }
        | CoreExprKind::MakeDynOwned { .. }
        | CoreExprKind::Unary { .. }
        | CoreExprKind::CompileTime(..)
        | CoreExprKind::Try(..) => {
            expression_has_closure_unary(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Binary { .. } => {
            expression_has_closure_binary(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Call { .. } | CoreExprKind::CallDynSlot { .. } => {
            expression_has_closure_call(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::TypeApply { .. } => {
            expression_has_closure_type_apply(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Index { .. } => {
            expression_has_closure_index(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Slice { .. } => {
            expression_has_closure_slice(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Tuple(..) | CoreExprKind::Array(..) => {
            expression_has_closure_tuple(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Member { .. } => {
            expression_has_closure_member(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Record { .. } => {
            expression_has_closure_record(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::RecordUpdate { .. } => {
            expression_has_closure_record_update(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::Sequence { .. } => {
            expression_has_closure_sequence(e, depth, budget, direct_type_callee)
        }
        CoreExprKind::If { .. } => expression_has_closure_if(e, depth, budget, direct_type_callee),
        CoreExprKind::Integer(..)
        | CoreExprKind::Float(..)
        | CoreExprKind::String(..)
        | CoreExprKind::Name(..)
        | CoreExprKind::ReceiverMember(..)
        | CoreExprKind::Unsupported(..) => {
            expression_has_closure_integer(e, depth, budget, direct_type_callee)
        }
        _ => expression_has_closure_other(e, depth, budget, direct_type_callee),
    }
}

fn expression_has_closure_literal(
    e: &CoreExpr,
    _depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let found = false;
    match &e.kind {
        CoreExprKind::Literal(Constant::WideString(units)) => {
            budget.work(units.len(), e.span)?;
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_ascribed(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Ascribed { ty, value }
        | CoreExprKind::IntegerConvert { target: ty, value }
        | CoreExprKind::NumericConvert { target: ty, value } => {
            {
                validate_type(ty, depth + 1, e.span, budget)?;
                found |= expression_has_closure(value, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_closure(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    match &e.kind {
        CoreExprKind::Closure {
            parameters,
            return_type,
            body,
            ..
        } => {
            {
                budget.work(parameters.len(), e.span)?;
                for parameter in parameters {
                    budget.work(parameter.name.len(), parameter.span)?;
                    if let Some(ty) = &parameter.ty {
                        validate_type(ty, depth + 1, parameter.span, budget)?;
                    }
                }
                if let Some(ty) = return_type {
                    validate_type(ty, depth + 1, e.span, budget)?;
                }
                contains_closure(body, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(true)
}
fn expression_has_closure_unary(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::MakeDynBorrow {
            source: operand, ..
        }
        | CoreExprKind::MakeDynOwned {
            source: operand, ..
        }
        | CoreExprKind::Unary { operand, .. }
        | CoreExprKind::CompileTime(operand)
        | CoreExprKind::Try(operand) => {
            found |= expression_has_closure(operand, depth + 1, budget)?;
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_binary(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Binary { left, right, .. } => {
            {
                found |= expression_has_closure(left, depth + 1, budget)?;
                found |= expression_has_closure(right, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_call(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Call { callee, arguments }
        | CoreExprKind::CallDynSlot {
            callee, arguments, ..
        } => {
            {
                let previous = budget.direct_type_callee;
                budget.direct_type_callee = true;
                found |= expression_has_closure(callee, depth + 1, budget)?;
                budget.direct_type_callee = false;
                for argument in arguments {
                    found |= expression_has_closure(argument, depth + 1, budget)?;
                }
                budget.direct_type_callee = previous;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_type_apply(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::TypeApply {
            callee,
            type_arguments,
        } => {
            {
                let error_constructor = matches!(&callee.kind, CoreExprKind::Name(name)
                    if matches!(name.as_str(), "error_ok" | "error_err"));
                if !direct_type_callee
                    || type_arguments.len() != if error_constructor { 2 } else { 1 }
                    || !(matches!(&callee.kind, CoreExprKind::Name(_))
                        || matches!(&callee.kind, CoreExprKind::Member { object, .. } if matches!(&object.kind, CoreExprKind::Name(_))))
                {
                    return Err(error(
                        "E-TYPE-APPLICATION",
                        e.span,
                        "one type argument is allowed only on a static call's direct callee",
                    ));
                }
                budget.work(type_arguments.len(), e.span)?;
                for argument in type_arguments {
                    validate_type(argument, depth + 1, e.span, budget)?;
                }
                budget.direct_type_callee = false;
                found |= expression_has_closure(callee, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_index(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Index { object, index } => {
            {
                found |= expression_has_closure(object, depth + 1, budget)?;
                found |= expression_has_closure(index, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_slice(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Slice { object, start, end } => {
            {
                found |= expression_has_closure(object, depth + 1, budget)?;
                for endpoint in [start, end].into_iter().flatten() {
                    found |= expression_has_closure(endpoint, depth + 1, budget)?;
                }
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_tuple(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
            {
                for value in values {
                    found |= expression_has_closure(value, depth + 1, budget)?;
                }
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_member(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Member {
            object,
            member,
            owner,
        } => {
            {
                if let Some(ty) = owner {
                    validate_type(ty, depth + 1, e.span, budget)?;
                }
                budget.work(member.len(), e.span)?;
                found |= expression_has_closure(object, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_record(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Record { ty, fields } => {
            {
                validate_type(ty, depth + 1, e.span, budget)?;
                for (name, value) in fields {
                    budget.work(name.len(), value.span)?;
                    found |= expression_has_closure(value, depth + 1, budget)?;
                }
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_record_update(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::RecordUpdate { ty, base, fields } => {
            {
                if let Some(ty) = ty {
                    validate_type(ty, depth + 1, e.span, budget)?;
                }
                found |= expression_has_closure(base, depth + 1, budget)?;
                for (name, value) in fields {
                    budget.work(name.len(), value.span)?;
                    found |= expression_has_closure(value, depth + 1, budget)?;
                }
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_sequence(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::Sequence { bindings, value } => {
            {
                for (name, binding) in bindings {
                    budget.work(name.len(), binding.span)?;
                    found |= expression_has_closure(binding, depth + 1, budget)?;
                }
                found |= expression_has_closure(value, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_if(
    e: &CoreExpr,
    depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let mut found = false;
    match &e.kind {
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            {
                found |= expression_has_closure(condition, depth + 1, budget)?;
                found |= expression_has_closure(then_value, depth + 1, budget)?;
                found |= expression_has_closure(else_value, depth + 1, budget)?;
            };
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_integer(
    e: &CoreExpr,
    _depth: usize,
    budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let found = false;
    match &e.kind {
        CoreExprKind::Integer(text)
        | CoreExprKind::Float(text)
        | CoreExprKind::String(text)
        | CoreExprKind::Name(text)
        | CoreExprKind::ReceiverMember(text)
        | CoreExprKind::Unsupported(text) => {
            budget.work(text.len(), e.span)?;
        }
        _ => unreachable!("closure presence dispatch"),
    }
    Ok(found)
}
fn expression_has_closure_other(
    _e: &CoreExpr,
    _depth: usize,
    _budget: &mut Budget<'_>,
    _direct_type_callee: bool,
) -> R<bool> {
    let found = false;
    {
        {};
    }
    Ok(found)
}

// Inference-only deferred request. It never becomes a Type, Typed/Wire row,
// retained shape or permission; the actual worker producer forms all classes.
#[derive(Clone)]
struct PendingTaskRequest {
    caller: zil_base::NodeId,
    closure_scope: Option<usize>,
    call: zil_base::NodeId,
    worker: Rc<Signature>,
    inputs: Rc<Vec<Term>>,
    recover_inputs: bool,
}
struct PendingTask {
    requests: std::cell::RefCell<Vec<PendingTaskRequest>>,
    result: Rc<Term>,
    constraints: std::cell::RefCell<Vec<Type>>,
}
#[derive(Clone)]
enum Term {
    Scalar(Variable),
    Address(Variable, bool),
    /// Only the finite payload participates in the Copy constraint graph.
    /// The bool distinguishes a carrier expression from a descriptor.
    Resource(Variable, bool),
    Registration(Rc<Term>, bool, bool),
    Closure(usize),
    ClosureArray {
        element: usize,
        length: u32,
    },
    ClosureAddress(usize, bool),
    /// Ordinary owning state retains the source factory identity separately
    /// from its eventual layout/ClosureId.
    ClosurePackage {
        shape: Rc<Type>,
        environment: usize,
    },
    /// Source identities are attached to real field positions, not merged by
    /// their eventual shared concrete ClosureId.
    StatePackage {
        shape: Rc<Type>,
        fields: Vec<(u32, usize)>,
    },
    // Only new value-inferred ordinary calls need to carry a complete source
    // State through wrappers before real ClosureIds exist.
    ValueStore(Rc<Term>, bool),
    ValueAddress(Rc<Term>, bool),
    ValueUnion {
        error: Rc<Term>,
        result: Rc<Term>,
    },
    /// Inference-only original input products before concrete ClosureIds exist.
    /// Ordinary Types and both kernels still prove their actual owned shape.
    InputTuple(Rc<Vec<Term>>),
    CreateFailure(Rc<Vec<Term>>),
    PendingTask(Rc<PendingTask>, bool),
    Function(Rc<Signature>),
    /// Nominal receiver already constrained; only valid immediately as a callee.
    Method(Rc<Signature>),
    /// Internal selector only, never a storable function value.
    SliceLength,
    Module(zil_base::ModuleId),
}
#[derive(Clone)]
struct Binding {
    term: Term,
    owner: Option<usize>,
    mutable: bool,
}
struct Closure {
    parameters: Vec<Variable>,
    result: Variable,
    has_return: bool,
    function_pointer: bool,
    span: Span,
}
struct Solver<'a, 'b, 'c> {
    dyn_interfaces: &'a [DynInterface],
    dyn_declarations: &'a [DynDeclaration],
    graph: Graph,
    scopes: Vec<BTreeMap<String, Binding>>,
    prepared: BTreeSet<String>,
    closures: Vec<Closure>,
    owner: Option<usize>,
    ordinary_result: Type,
    source_function: zil_base::NodeId,
    ordinary_callable_return: Option<CoreCallableReturn>,
    // Source-visible opaque identity belongs to the factory definition, even
    // when its hidden environment is forwarded by another factory.
    factory_terms: BTreeMap<FunctionId, usize>,
    ordinary_return_identity: Option<usize>,
    helper_environments: BTreeMap<String, usize>,
    state_modes: BTreeMap<usize, CallableKind>,
    signatures: &'a BTreeMap<String, Signature>,
    imports: Option<&'a program::Imports<'a>>,
    enums: &'a BTreeMap<String, Type>,
    budget: &'b mut Budget<'c>,
    expression_index: usize,
    statement_index: usize,
    literal_types: BTreeMap<usize, Variable>,
    binding_types: BTreeMap<usize, Term>,
    closure_expressions: BTreeMap<usize, usize>,
    callable_hint: Option<(Vec<Type>, Type)>,
}
impl<'a, 'b, 'c> Solver<'a, 'b, 'c> {
    fn owner_type(&mut self, actual: &Type, owner: Option<&Type>, span: Span) -> R<()> {
        if let Some(expected) = owner {
            validate_type(actual, 0, span, self.budget)?;
            validate_type(expected, 0, span, self.budget)?;
            if member_owner::concrete(actual, &mut || self.budget.work(1, span))?
                && member_owner::concrete(expected, &mut || self.budget.work(1, span))?
            {
                validate_type(expected, 0, span, self.budget)?;
                self.budget.work(2, span)?;
                member_owner::check_member_owner(actual, Some(expected), span)?;
            }
        }
        Ok(())
    }
    fn owner_term(&mut self, actual: &Term, owner: Option<&Type>, span: Span) -> R<()> {
        if owner.is_none() {
            return Ok(());
        }
        match actual {
            Term::Scalar(v) | Term::Address(v, _) | Term::Resource(v, _) => {
                if let Some(ty) = self.graph.ty(*v)? {
                    self.owner_type(&ty, owner, span)?;
                }
            }
            Term::StatePackage { shape, .. } | Term::ClosurePackage { shape, .. } => {
                self.owner_type(shape, owner, span)?
            }
            Term::ValueAddress(inner, _) => self.owner_term(inner, owner, span)?,
            Term::Module(_) => {
                return Err(error(
                    "E-MEMBER-OWNER",
                    span,
                    "a constrained member requires an actual value receiver",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn new(
        function: &CoreFunction,
        signatures: &'a BTreeMap<String, Signature>,
        imports: Option<&'a program::Imports<'a>>,
        catalog: closures::Catalog<'a>,
        budget: &'b mut Budget<'c>,
    ) -> R<Self> {
        let old_result = function.callable_return.is_some()
            || forwarding::bound(function).is_some()
            || function.type_parameters.first().is_some_and(|parameter| {
                store_templates::symbolic_type(&function.return_type, &parameter.name)
            })
            || callable_fields::template(function)
            || function.return_type.is_copy_value()
            || function.return_type.is_scalar_or_address()
            || packages::ordinary(&function.return_type)
            || owned_shapes::sum(&function.return_type)
            || packages::owned_array_supported(&function.return_type)
            || resources::supported(&function.return_type);
        if !old_result {
            // A closure in an ordinary function must not narrow the already
            // admitted owned error boundary. Reserve before the new walk.
            validate_type(&function.return_type, 0, function.span, budget)?;
        }
        if !old_result && !errors::supported(&function.return_type) {
            return Err(unsupported(
                function.span,
                "non-scalar function result in closure inference",
            ));
        }
        let mut this = Self {
            graph: Graph::new(budget.limits),
            scopes: vec![BTreeMap::new()],
            prepared: BTreeSet::new(),
            closures: vec![],
            owner: None,
            ordinary_result: function.return_type.clone(),
            source_function: function.id,
            ordinary_callable_return: function.callable_return.clone(),
            factory_terms: BTreeMap::new(),
            ordinary_return_identity: None,
            helper_environments: BTreeMap::new(),
            state_modes: BTreeMap::new(),
            signatures,
            imports,
            enums: catalog
                .enums
                .expect("source inference always has its enum catalog"),
            dyn_interfaces: catalog.dyn_interfaces,
            dyn_declarations: catalog.dyn_declarations,
            budget,
            expression_index: 0,
            statement_index: 0,
            literal_types: BTreeMap::new(),
            binding_types: BTreeMap::new(),
            closure_expressions: BTreeMap::new(),
            callable_hint: None,
        };
        if let Some(parameter) = function.type_parameters.first() {
            this.graph
                .set_store_parameter(&parameter.name, function.span)?;
        }
        if let Some(state) = function.state_parameters.first() {
            state_origins::validate_shape(function)?;
            for binding in &state.bindings {
                this.seed_state_environment(
                    &binding.generic_name,
                    binding.kind,
                    &binding.parameters,
                    &binding.result,
                    function.id,
                    function.span,
                )?;
            }
        }
        for (index, parameter) in function.parameters.iter().enumerate() {
            let term = if function
                .state_parameters
                .first()
                .is_some_and(|state| state.index as usize == index)
            {
                this.known_term(&parameter.ty, parameter.span)?
            } else if let Some(bound) = function
                .callable_parameters
                .iter()
                .find(|bound| bound.index as usize == index)
            {
                let ty = Type::Opaque {
                    identity: function.id,
                    hidden: Box::new(Type::FunctionPointer {
                        parameters: bound.parameters.clone(),
                        result: Box::new(bound.result.clone()),
                    }),
                };
                let id = if let Some(id) = this.helper_environments.get(&bound.generic_name) {
                    *id
                } else {
                    let Term::Closure(id) = this.known_term(&ty, parameter.span)? else {
                        unreachable!()
                    };
                    this.budget
                        .work(bound.generic_name.len().saturating_add(1), parameter.span)?;
                    this.helper_environments
                        .insert(bound.generic_name.clone(), id);
                    id
                };
                if callable_fields::template(function)
                    && function
                        .callable_parameters
                        .iter()
                        .any(|other| other.generic_index != bound.generic_index)
                {
                    this.budget.work(1, parameter.span)?;
                    this.state_modes.insert(id, bound.kind);
                }
                if registration::environment(&parameter.ty).is_some()
                    && matches!(parameter.ty, Type::Managed(_))
                {
                    Term::Registration(Rc::new(Term::Closure(id)), true, true)
                } else if registration::borrowed_environment(&parameter.ty).is_some() {
                    Term::ValueAddress(
                        Rc::new(Term::Registration(Rc::new(Term::Closure(id)), true, true)),
                        true,
                    )
                } else if callable_fields::placeholder_package(&parameter.ty, &bound.generic_name) {
                    this.state_term(&parameter.ty, &bound.generic_name, id, parameter.span)?
                } else {
                    match helpers::parameter_mode(&parameter.ty, &bound.generic_name) {
                        Some(Some(mutable)) => Term::ClosureAddress(id, mutable),
                        Some(None) => Term::Closure(id),
                        None => {
                            return Err(unsupported(
                                parameter.span,
                                "invalid callable helper parameter",
                            ));
                        }
                    }
                }
            } else {
                this.known_term(&parameter.ty, parameter.span)?
            };
            this.insert(&parameter.name, term, false, parameter.span)?;
        }
        Ok(this)
    }
    fn seed_state_environment(
        &mut self,
        name: &str,
        kind: CallableKind,
        parameters: &[Type],
        result: &Type,
        identity: zil_base::NodeId,
        span: Span,
    ) -> R<()> {
        let ty = Type::Opaque {
            identity,
            hidden: Box::new(Type::FunctionPointer {
                parameters: parameters.to_vec(),
                result: Box::new(result.clone()),
            }),
        };
        let Term::Closure(id) = self.known_term(&ty, span)? else {
            unreachable!()
        };
        self.budget.work(name.len().saturating_add(2), span)?;
        self.helper_environments.insert(name.to_owned(), id);
        self.state_modes.insert(id, kind);
        Ok(())
    }

    fn state_term_many(&mut self, ty: &Type, bindings: &[(String, usize)], span: Span) -> R<Term> {
        validate_type(ty, 0, span, self.budget)?;
        let Type::Package { name, fields } = ty else {
            return Err(unsupported(span, "State requires a complete Package"));
        };
        self.budget.work(
            fields
                .len()
                .saturating_mul(bindings.len().saturating_add(2)),
            span,
        )?;
        let mut sources = Vec::new();
        let mut shape = Vec::new();
        for (index, (name, ty)) in fields.iter().enumerate() {
            let binding = if let Type::Named(name) = ty {
                bindings.iter().find(|(binder, _)| binder == name)
            } else {
                None
            };
            let ty = if let Some((_, source)) = binding {
                sources.push((index as u32, *source));
                Type::Named("$callback".into())
            } else {
                ty.clone()
            };
            shape.push((name.clone(), ty));
        }
        if sources.len() != 2 {
            return Err(unsupported(
                span,
                "State requires both direct callable fields",
            ));
        }
        Ok(Term::StatePackage {
            shape: Rc::new(Type::Package {
                name: callable_fields::nominal_base(name).to_owned(),
                fields: shape,
            }),
            fields: sources,
        })
    }

    fn known_result(&mut self, signature: &Signature, span: Span) -> R<Term> {
        if matches!(signature.result, Type::Opaque { .. }) {
            if let Some(id) = self.factory_terms.get(&signature.concrete_id(span)?) {
                return Ok(Term::Closure(*id));
            }
            let term = self.known_term(&signature.result, span)?;
            if let Term::Closure(id) = term {
                self.factory_terms.insert(signature.concrete_id(span)?, id);
            }
            Ok(term)
        } else {
            self.known_term(&signature.result, span)
        }
    }

    fn state_term(&mut self, ty: &Type, generic: &str, environment: usize, span: Span) -> R<Term> {
        validate_type(ty, 0, span, self.budget)?;
        let Type::Package { name, fields } = ty else {
            return Err(unsupported(
                span,
                "callable State requires an owning Package",
            ));
        };
        if !fields
            .iter()
            .any(|(_, t)| matches!(t,Type::Named(n) if n==generic))
        {
            return Err(unsupported(span, "callable State has no direct F field"));
        }
        let shape = Type::Package {
            name: callable_fields::nominal_base(name).to_owned(),
            fields: fields
                .iter()
                .map(|(n, t)| {
                    (
                        n.clone(),
                        if matches!(t,Type::Named(n) if n==generic) {
                            Type::Named("$callback".into())
                        } else {
                            t.clone()
                        },
                    )
                })
                .collect(),
        };
        Ok(Term::ClosurePackage {
            shape: Rc::new(shape),
            environment,
        })
    }

    fn known_term(&mut self, ty: &Type, span: Span) -> R<Term> {
        if let Type::Address {
            mutable: true,
            inner,
        } = ty
            && registration::borrowed_environment(ty).is_some()
        {
            let carrier = self.known_term(inner, span)?;
            return Ok(Term::ValueAddress(Rc::new(carrier), true));
        }
        if let Some(environment) = registration::environment(ty) {
            let source = self.known_term(environment, span)?;
            return Ok(Term::Registration(
                Rc::new(source),
                matches!(ty, Type::Managed(_)),
                registration::asynchronous(ty),
            ));
        }
        if self.helper_environments.len() > 1
            && let Type::Package { fields, .. } = ty
        {
            self.budget.work(
                fields.len().saturating_mul(self.helper_environments.len()),
                span,
            )?;
            let mut bindings = Vec::new();
            for (name, id) in &self.helper_environments {
                if fields
                    .iter()
                    .any(|(_, ty)| matches!(ty, Type::Named(n) if n == name))
                {
                    self.budget.work(name.len().saturating_add(1), span)?;
                    bindings.push((name.clone(), *id));
                }
            }
            if bindings.len() == 2 {
                return self.state_term_many(ty, &bindings, span);
            }
        }
        for (name, id) in &self.helper_environments {
            if matches!(ty,Type::Named(n) if n==name) {
                return Ok(Term::Closure(*id));
            }
            if callable_fields::placeholder_package(ty, name) {
                let (name, id) = (name.clone(), *id);
                return self.state_term(ty, &name, id, span);
            }
        }

        if let Type::Opaque { hidden, .. } = ty {
            let Type::FunctionPointer { parameters, result } = hidden.as_ref() else {
                return Err(unsupported(
                    span,
                    "opaque context is not a callable signature",
                ));
            };
            self.budget.work(parameters.len().saturating_add(1), span)?;
            let mut variables = Vec::new();
            for ty in parameters {
                let variable = self.fresh(VALUE, false, span)?;
                self.bind(variable, ty, span)?;
                variables.push(variable);
            }
            let output = self.fresh(VALUE, false, span)?;
            self.bind(output, result, span)?;
            let id = self.closures.len();
            self.closures.push(Closure {
                parameters: variables,
                result: output,
                has_return: true,
                function_pointer: false,
                span,
            });
            return Ok(Term::Closure(id));
        }
        let variable = self.fresh(VALUE, false, span)?;
        if let Type::Managed(payload) | Type::Desc(payload) = ty {
            if !resources::supported(ty) {
                return Err(unsupported(span, "resource requires a finite Copy payload"));
            }
            self.bind(variable, payload, span)?;
            Ok(Term::Resource(variable, matches!(ty, Type::Managed(_))))
        } else if let Type::Address { mutable, inner } = ty {
            if !inner.is_address_pointee()
                && !stored::shared_address(ty)
                && !stored::exclusive_address(ty)
                && !ty.is_shared_sync_cell_address()
                && !ty.is_shared_resource_head_address()
                && !ty.is_exclusive_task_head_address()
                && !resources::head_address(ty)
                && !ty.is_package_address()
                && !matches!(inner.as_ref(), Type::Package { .. })
                && !reference_patterns::sum_address(ty)
                && !self.graph.symbolic_store_type(inner)
            {
                return Err(unsupported(span, "nested reference signature"));
            }
            self.bind(variable, inner, span)?;
            Ok(Term::Address(variable, *mutable))
        } else {
            self.bind(variable, ty, span)?;
            Ok(Term::Scalar(variable))
        }
    }
    fn bind_callable_argument(
        &mut self,
        argument: &CoreExpr,
        ty: &Type,
        bound: &CoreCallableParameter,
        depth: usize,
    ) -> R<usize> {
        let owning_registration = matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::AsyncRegistration { .. }));
        if registration::borrowed_environment(ty).is_some() || owning_registration {
            let actual = self.expr(argument, depth, false)?;
            let carrier = if owning_registration {
                Rc::new(actual)
            } else {
                let Term::ValueAddress(carrier, true) = actual else {
                    return Err(error(
                        "E-TYPE-MISMATCH",
                        argument.span,
                        "exclusive registration address required",
                    ));
                };
                carrier
            };
            let Term::Registration(environment, true, true) = carrier.as_ref() else {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    argument.span,
                    "actual asynchronous registration required",
                ));
            };
            let Term::Closure(id) = environment.as_ref() else {
                return Err(unsupported(
                    argument.span,
                    "concrete registration environment required",
                ));
            };
            let id = *id;
            self.bind_source_bound(id, bound, argument.span)?;
            return Ok(id);
        }
        let state = callable_fields::placeholder_package(ty, &bound.generic_name);
        let wanted = if state {
            None
        } else {
            helpers::parameter_mode(ty, &bound.generic_name).ok_or_else(|| {
                unsupported(argument.span, "invalid callable template parameter shape")
            })?
        };
        let previous = self
            .callable_hint
            .replace((bound.parameters.clone(), bound.result.clone()));
        let term = self.expr(argument, depth, false)?;
        self.callable_hint = previous;
        let id = match (wanted, term) {
            (None, Term::ClosurePackage { shape, environment }) if state => {
                let expected =
                    self.state_term(ty, &bound.generic_name, environment, argument.span)?;
                self.unify(
                    &Term::ClosurePackage { shape, environment },
                    &expected,
                    argument.span,
                )?;
                environment
            }
            (None, Term::Closure(id)) if !state => id,
            (Some(want_mut), Term::ClosureAddress(id, actual_mut)) if !want_mut || actual_mut => id,
            _ => {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    argument.span,
                    "helper requires its concrete environment passing mode",
                ));
            }
        };
        self.bind_source_bound(id, bound, argument.span)?;
        Ok(id)
    }
    fn bind_source_bound(&mut self, id: usize, bound: &CoreCallableParameter, span: Span) -> R<()> {
        if !self.state_modes.is_empty() {
            self.budget.work(1, span)?;
            if self
                .state_modes
                .get(&id)
                .is_some_and(|kind| !helpers::compatible(*kind, bound.kind))
            {
                return Err(error(
                    "E-CLOSURE-MODE",
                    span,
                    "the State declaration does not provide the requested callable capability",
                ));
            }
        }
        let parameters = self.closures[id].parameters.clone();
        if parameters.len() != bound.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "closure signature differs from callable bound",
            ));
        }
        for (variable, ty) in parameters.into_iter().zip(&bound.parameters) {
            self.bind(variable, ty, span)?;
        }
        self.bind(self.closures[id].result, &bound.result, span)?;
        Ok(())
    }

    fn state_call(
        &mut self,
        signature: &Signature,
        arguments: &[CoreExpr],
        depth: usize,
        span: Span,
    ) -> R<Term> {
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "State helper argument count differs",
            ));
        }
        self.budget.work(
            signature.callable.len().saturating_add(arguments.len()),
            span,
        )?;
        let mut sources = Vec::new();
        for (index, (argument, ty)) in arguments.iter().zip(&signature.parameters).enumerate() {
            if let Some(state) = &signature.state_parameter
                && state.index as usize == index
            {
                let value = self.expr(argument, depth + 1, false)?;
                let Term::StatePackage { fields, .. } = &value else {
                    return Err(error(
                        "E-TYPE-MISMATCH",
                        argument.span,
                        "both State field origins are required",
                    ));
                };
                for (declaration, bound) in state.bindings.iter().zip(&signature.callable) {
                    let id = fields
                        .iter()
                        .find(|(field, _)| *field == declaration.fields[0])
                        .map(|(_, id)| *id)
                        .ok_or_else(|| {
                            error(
                                "E-TYPE-MISMATCH",
                                argument.span,
                                "State selector has no source callable",
                            )
                        })?;
                    self.bind_source_bound(id, bound, argument.span)?;
                    sources.push((bound.generic_name.clone(), id));
                }
                let expected = self.state_term_many(ty, &sources, argument.span)?;
                self.unify(&value, &expected, argument.span)?;
            } else if let Some(bound) = signature
                .callable
                .iter()
                .find(|bound| bound.index as usize == index)
            {
                let id = self.bind_callable_argument(argument, ty, bound, depth + 1)?;
                sources.push((bound.generic_name.clone(), id));
            } else {
                let value = self.expr_expected(argument, Some(ty), depth + 1, false)?;
                self.bind_term(&value, ty, argument.span)?;
            }
        }
        self.state_result_term(&signature.result, &sources, span)
    }

    fn replacement_call(
        &mut self,
        signature: &Signature,
        arguments: &[CoreExpr],
        depth: usize,
        span: Span,
    ) -> R<Term> {
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "State replacement argument count differs",
            ));
        }
        self.budget.work(
            arguments
                .len()
                .saturating_mul(signature.callable.len().saturating_add(1)),
            span,
        )?;
        let mut environment = None;
        for (index, (argument, ty)) in arguments.iter().zip(&signature.parameters).enumerate() {
            if let Some(bound) = signature
                .callable
                .iter()
                .find(|bound| bound.index as usize == index)
            {
                let id = self.bind_callable_argument(argument, ty, bound, depth + 1)?;
                if let Some(previous) = environment {
                    self.unify(&Term::Closure(previous), &Term::Closure(id), argument.span)?;
                } else {
                    environment = Some(id);
                }
            } else {
                let value = self.expr_expected(argument, Some(ty), depth + 1, false)?;
                self.bind_term(&value, ty, argument.span)?;
            }
        }
        self.state_term(
            &signature.result,
            &signature.callable[0].generic_name,
            environment.ok_or_else(|| unsupported(span, "State replacement source is absent"))?,
            span,
        )
    }
    fn state_result_term(&mut self, ty: &Type, sources: &[(String, usize)], span: Span) -> R<Term> {
        if matches!(ty, Type::Package { .. })
            && sources
                .iter()
                .any(|(name, _)| callable_fields::placeholder_package(ty, name))
        {
            return self.state_term_many(ty, sources, span);
        }
        if let Type::ErrorUnion { error, result } = ty {
            self.budget.work(2, span)?;
            let failure = self.state_result_term(error, sources, span)?;
            let success = self.state_result_term(result, sources, span)?;
            return Ok(Term::ValueUnion {
                error: Rc::new(failure),
                result: Rc::new(success),
            });
        }
        self.known_term(ty, span)
    }

    fn value_store_element(&mut self, term: &Term, span: Span) -> R<Term> {
        self.budget.work(1, span)?;
        match term {
            Term::ValueStore(element, _) => Ok(element.as_ref().clone()),
            Term::Resource(variable, _) => {
                let Some(Type::OwnedStore { element }) = self.graph.ty(*variable)? else {
                    return Err(error(
                        "E-TYPE-APPLICATION",
                        span,
                        "value argument is not a Store",
                    ));
                };
                validate_type(&element, 0, span, self.budget)?;
                self.known_term(&element, span)
            }
            _ => Err(error(
                "E-TYPE-APPLICATION",
                span,
                "value argument is not a Store",
            )),
        }
    }
    fn value_union_parts(&mut self, term: &Term, span: Span) -> R<(Term, Term)> {
        self.budget.work(2, span)?;
        match term {
            Term::ValueUnion { error, result } => {
                Ok((error.as_ref().clone(), result.as_ref().clone()))
            }
            Term::Scalar(variable) => {
                let Type::ErrorUnion { error, result } =
                    self.graph.injection_type(*variable, span)?
                else {
                    return Err(error(
                        "E-TYPE-APPLICATION",
                        span,
                        "value argument is not a complete error union",
                    ));
                };
                Ok((
                    self.known_term(&error, span)?,
                    self.known_term(&result, span)?,
                ))
            }
            _ => Err(error(
                "E-TYPE-APPLICATION",
                span,
                "value argument is not a complete error union",
            )),
        }
    }
    fn product_term_type(&mut self, term: &Term, span: Span) -> R<Type> {
        match term {
            Term::Scalar(variable) => self.graph.injection_type(*variable, span),
            Term::ClosurePackage { shape, .. } | Term::StatePackage { shape, .. } => {
                validate_type(shape, 0, span, self.budget)?;
                Ok(shape.as_ref().clone())
            }
            _ => Err(error(
                "E-TYPE-APPLICATION",
                span,
                "generic Product inference requires a complete actual value type",
            )),
        }
    }
    fn infer_value_term(
        &mut self,
        pattern: &Type,
        value: &Term,
        name: &str,
        actual: &mut Option<Term>,
        span: Span,
    ) -> R<()> {
        validate_type(pattern, 0, span, self.budget)?;
        if matches!(pattern, Type::Named(n) if n == name) {
            if let Some(previous) = actual {
                self.unify(previous, value, span)?;
            } else {
                self.budget.work(1, span)?;
                *actual = Some(value.clone());
            }
            return Ok(());
        }
        if let Some(element) = dynamic_store::element(pattern) {
            let value = self.value_store_element(value, span)?;
            return self.infer_value_term(element, &value, name, actual, span);
        }
        if let Type::ErrorUnion { error, result } = pattern {
            let (failure, success) = self.value_union_parts(value, span)?;
            self.infer_value_term(error, &failure, name, actual, span)?;
            return self.infer_value_term(result, &success, name, actual, span);
        }
        if matches!(pattern, Type::Package { .. }) && self.budget.products.is_some() {
            let ty = self.product_term_type(value, span)?;
            let products = self.budget.products.clone();
            let limits = self.budget.limits;
            if let Some(pairs) =
                aggregate_templates::input_pairs(pattern, &ty, &products, limits, span, &mut |n| {
                    self.budget.work(n, span)
                })?
            {
                for (wanted, got) in pairs {
                    let term = self.known_term(&got, span)?;
                    self.infer_value_term(&wanted, &term, name, actual, span)?;
                }
                return Ok(());
            }
        }
        self.bind_term(value, pattern, span)
    }
    fn applied_value_term(
        &mut self,
        pattern: &Type,
        name: &str,
        actual: &Term,
        span: Span,
    ) -> R<Term> {
        validate_type(pattern, 0, span, self.budget)?;
        if matches!(pattern, Type::Named(n) if n == name) {
            self.budget.work(1, span)?;
            return Ok(actual.clone());
        }
        if let Some(element) = dynamic_store::element(pattern) {
            let element = self.applied_value_term(element, name, actual, span)?;
            self.budget.work(1, span)?;
            return Ok(Term::ValueStore(
                Rc::new(element),
                matches!(pattern, Type::Managed(_)),
            ));
        }
        if let Type::ErrorUnion { error, result } = pattern {
            let failure = self.applied_value_term(error, name, actual, span)?;
            let success = self.applied_value_term(result, name, actual, span)?;
            self.budget.work(2, span)?;
            return Ok(Term::ValueUnion {
                error: Rc::new(failure),
                result: Rc::new(success),
            });
        }
        if matches!(pattern, Type::Package { .. }) && self.budget.products.is_some() {
            let actual = self.product_term_type(actual, span)?;
            let products = self.budget.products.clone();
            let limits = self.budget.limits;
            let ty = store_templates::substitute_type(
                pattern,
                name,
                &actual,
                limits,
                &products,
                span,
                &mut |n| self.budget.work(n, span),
            )?;
            return self.known_term(&ty, span);
        }
        self.known_term(pattern, span)
    }
    fn value_type_call(
        &mut self,
        signature: &Signature,
        arguments: &[CoreExpr],
        depth: usize,
        span: Span,
    ) -> R<Term> {
        let name = &signature
            .type_parameter
            .as_ref()
            .expect("ordinary template")
            .name;
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "ordinary template value argument count differs",
            ));
        }
        self.budget.work(arguments.len().saturating_mul(2), span)?;
        let mut actual = None;
        for (argument, pattern) in arguments.iter().zip(&signature.parameters) {
            let products = self.budget.products.clone();
            let limits = self.budget.limits;
            let parameterized = aggregate_templates::parameter_in(
                pattern,
                name,
                &products,
                limits,
                argument.span,
                &mut |n| self.budget.work(n, argument.span),
            )?;
            let hint = (!parameterized).then_some(pattern);
            let value = self.expr_expected(argument, hint, depth + 1, false)?;
            self.infer_value_term(pattern, &value, name, &mut actual, argument.span)?;
        }
        let actual = actual.ok_or_else(|| error("E-TYPE-APPLICATION", span,
            "ordinary template call has no unique complete type constraint from a value argument"))?;
        match &actual {
            Term::StatePackage { .. } | Term::ClosurePackage { .. } => {}
            Term::Scalar(variable) => {
                let ty = self.graph.injection_type(*variable, span)?;
                validate_type(&ty, 0, span, self.budget)?;
                dynamic_store::charge_pod(&ty, self.budget.limits, span, &mut self.budget.work)?;
                if !(self.graph.is_store_parameter(&ty)
                    || dynamic_store::supported_element(&ty)
                    || dynamic_store::callback_element(
                        &ty,
                        |t| matches!(t, Type::Named(n) if self.helper_environments.contains_key(n)),
                    ))
                {
                    return Err(error(
                        "E-STORE-TYPE-ARGUMENT",
                        span,
                        "inferred value type does not satisfy StoreElement",
                    ));
                }
            }
            _ => {
                return Err(error(
                    "E-STORE-TYPE-ARGUMENT",
                    span,
                    "inferred value type is not a complete Store element",
                ));
            }
        }
        self.applied_value_term(&signature.result, name, &actual, span)
    }

    fn fresh(&mut self, domain: u16, literal: bool, span: Span) -> R<Variable> {
        self.budget.work(1, span)?;
        if self.graph.len() >= self.budget.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source constraint variable budget exceeded",
                Some(span),
            ));
        }
        Ok(self.graph.fresh(domain, literal))
    }
    fn bind(&mut self, variable: Variable, ty: &Type, span: Span) -> R<()> {
        self.budget.work(1, span)?;
        // Only this source function's declared callable binders may occur in
        // an unresolved Package shell. Ordinary T remains a distinct opaque
        // binder, and actual ownership/loans are checked after instantiation.
        self.graph.bind_source(
            variable,
            ty,
            (!self.helper_environments.is_empty()).then_some(&self.helper_environments),
            span,
        )
    }
    fn pending_task(
        &mut self,
        worker: Rc<Signature>,
        call: zil_base::NodeId,
        inputs: Vec<Term>,
        recover_inputs: bool,
        span: Span,
    ) -> R<Term> {
        self.budget.work(inputs.len().saturating_add(8), span)?;
        let result = self.known_term(&worker.result, span)?;
        let inputs = Rc::new(inputs);
        let task = Term::PendingTask(
            Rc::new(PendingTask {
                requests: std::cell::RefCell::new(vec![PendingTaskRequest {
                    caller: self.source_function,
                    closure_scope: self.owner,
                    call,
                    worker,
                    inputs: inputs.clone(),
                    recover_inputs,
                }]),
                result: Rc::new(result),
                constraints: std::cell::RefCell::new(Vec::new()),
            }),
            true,
        );
        let failure = if recover_inputs {
            Term::CreateFailure(inputs)
        } else {
            self.known_term(&tasks::create_error(), span)?
        };
        Ok(Term::ValueUnion {
            error: Rc::new(failure),
            result: Rc::new(task),
        })
    }
    fn bind_pending_task(
        &mut self,
        task: &Rc<PendingTask>,
        owned: bool,
        ty: &Type,
        span: Span,
    ) -> R<()> {
        let payload = match ty {
            Type::Managed(payload) if owned => payload,
            Type::Desc(payload) => payload,
            _ => {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "pending Task carrier mode differs from its complete declared constraint",
                ));
            }
        };
        let Type::Task { result, .. } = payload.as_ref() else {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "pending Task requires an actual Task constraint",
            ));
        };
        self.bind_term(&task.result, result, span)?;
        charge_type_snapshot(ty, span, self.budget.limits, &mut self.budget.work, 1)?;
        self.budget.work(1, span)?;
        if task.constraints.borrow().len() >= self.budget.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "pending Task context constraint inventory exceeded",
                Some(span),
            ));
        }
        task.constraints.borrow_mut().push(ty.clone());
        // The original annotation remains on the Core operation. Only the
        // concrete worker/context producer may check or publish its classes.
        Ok(())
    }
    fn pending_task_operation(&mut self, name: &str, term: &Term, span: Span) -> R<Option<Term>> {
        let (task, owned, address) = match term {
            Term::PendingTask(task, owned) => (task, *owned, None),
            Term::ValueAddress(carrier, mutable) => match carrier.as_ref() {
                Term::PendingTask(task, owned) => (task, *owned, Some(*mutable)),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let allowed = match name {
            "task_valid" => address.is_none(),
            "task_finish_destroy" => address.is_none() && owned,
            "task_start" | "task_poll" | "task_request_cancel" | "task_poll_close" => {
                address == Some(true) && owned
            }
            _ => false,
        };
        if !allowed {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "pending Task operation changes its actual carrier or address mode",
            ));
        }
        let requests = task.requests.borrow();
        self.budget.work(requests.len(), span)?;
        // Keep every real call request. Matching result shapes does not merge
        // workers, captures or the contexts which will be formed later.
        let mut result = None;
        for request in requests.iter() {
            self.budget
                .work(request.inputs.len().saturating_add(3), span)?;
            let returned = match name {
                "task_valid" => Type::Bool,
                "task_poll" => Type::ErrorUnion {
                    error: Box::new(tasks::poll_state()),
                    result: Box::new(tasks::poll_type(&request.worker.result)),
                },
                "task_poll_close" => tasks::poll_type(&Type::Unit),
                _ => Type::Unit,
            };
            let actual = self.known_term(&returned, span)?;
            if let Some(previous) = &result {
                self.unify(previous, &actual, span)?
            } else {
                result = Some(actual)
            }
        }
        result.map(Some).ok_or_else(|| {
            error(
                "E-TASK-WORKER",
                span,
                "pending Task lost every actual worker request",
            )
        })
    }
    fn scalar(&mut self, term: &Term, span: Span) -> R<Variable> {
        match term {
            Term::Scalar(variable) => Ok(*variable),
            Term::Function(signature) => {
                if signature.storage_life.is_some() {
                    for ty in &signature.parameters {
                        charge_type_snapshot(
                            ty,
                            span,
                            self.budget.limits,
                            &mut self.budget.work,
                            4,
                        )?;
                    }
                    charge_type_snapshot(
                        &signature.result,
                        span,
                        self.budget.limits,
                        &mut self.budget.work,
                        5,
                    )?;
                    borrowed_fp::static_pointer_slots(signature, |work| {
                        self.budget.work(work, span)
                    })?;
                } else {
                    for ty in signature
                        .parameters
                        .iter()
                        .chain(std::iter::once(&signature.result))
                    {
                        validate_type(ty, 0, span, self.budget)?;
                    }
                }
                self.budget
                    .work(borrowed_fp::pointer_work(signature), span)?;
                let ty = signature.pointer_type().ok_or_else(|| unsupported(span,
                    "function pointers require pure Copy parameters and result; indirect address contracts are not implemented"))?;
                let variable = self.fresh(VALUE, false, span)?;
                self.bind(variable, &ty, span)?;
                Ok(variable)
            }
            _ => Err(unsupported(
                span,
                "closure values cannot escape through scalar signatures, captures or scalar operations",
            )),
        }
    }
    fn input_pattern(
        &mut self,
        pattern: &CorePattern,
        term: &Term,
        mutable: bool,
        depth: usize,
    ) -> R<()> {
        self.budget.node(depth, pattern.span)?;
        match (&pattern.kind, term) {
            (CorePatternKind::Name(name), _) => {
                self.insert(name, term.clone(), mutable, pattern.span)
            }
            (CorePatternKind::Wildcard, _) => Ok(()),
            (CorePatternKind::Tuple(patterns), Term::InputTuple(fields))
                if patterns.len() == fields.len() =>
            {
                self.budget.work(fields.len(), pattern.span)?;
                for (pattern, field) in patterns.iter().zip(fields.iter()) {
                    self.input_pattern(pattern, field, mutable, depth + 1)?;
                }
                Ok(())
            }
            (
                CorePatternKind::ErrorArm { is_error, pattern },
                Term::ValueUnion { error, result },
            ) => self.input_pattern(
                pattern,
                if *is_error { error } else { result },
                mutable,
                depth + 1,
            ),
            _ => {
                let variable = self.scalar(term, pattern.span)?;
                for (name, variable, span) in self.graph.pattern_bind(pattern, variable)? {
                    let term = self.pattern_term(variable, span)?;
                    self.insert(&name, term, mutable, span)?;
                }
                Ok(())
            }
        }
    }
    fn pattern_term(&mut self, variable: Variable, span: Span) -> R<Term> {
        if let Some((inner, Some(mutable))) = self.graph.address_shape(variable, span)? {
            return Ok(Term::Address(inner, mutable));
        }
        // A concrete pattern payload can be an address without an inferred
        // Address shape node. Keep its original qualifier and complete pointee
        // when exposing it to ordinary projections and direct call contexts.
        if let Some(ty @ (Type::Managed(_) | Type::Desc(_) | Type::Address { .. })) =
            self.graph.ty(variable)?
        {
            return self.known_term(&ty, span);
        }
        Ok(Term::Scalar(variable))
    }
    // A shared aggregate projection constrains its pointee shape only.
    // Ordinary checking still decides place validity and all address permissions.
    fn projection_base(&mut self, term: &Term, span: Span) -> R<Variable> {
        match term {
            Term::Address(variable, _) | Term::Resource(variable, _) => Ok(*variable),
            _ => self.scalar(term, span),
        }
    }
    fn value_term(&mut self, term: Term, span: Span) -> R<Term> {
        if matches!(term, Term::Method(_)) {
            return Err(unsupported(
                span,
                "stored bound method values are not implemented",
            ));
        }
        if matches!(term, Term::Function(_)) {
            Ok(Term::Scalar(self.scalar(&term, span)?))
        } else {
            Ok(term)
        }
    }
    fn bind_term(&mut self, term: &Term, ty: &Type, span: Span) -> R<()> {
        if let Term::InputTuple(fields) = term {
            let Type::Tuple(wanted) = ty else {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "pending input product requires a Tuple constraint",
                ));
            };
            self.budget
                .work(fields.len().saturating_add(wanted.len()), span)?;
            if fields.len() != wanted.len() {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "pending input product changes Tuple arity",
                ));
            }
            for (field, wanted) in fields.iter().zip(wanted) {
                self.bind_term(field, wanted, span)?;
            }
            return Ok(());
        }
        if let Term::PendingTask(task, owned) = term {
            return self.bind_pending_task(task, *owned, ty, span);
        }
        if let Term::ValueAddress(carrier, mutable) = term
            && matches!(carrier.as_ref(), Term::PendingTask(..))
            && let Type::Address {
                mutable: wanted,
                inner,
            } = ty
        {
            if !*mutable && *wanted {
                return Err(error(
                    "E-ADDRESS-PERMISSION",
                    span,
                    "a shared Task address cannot grant exclusive access",
                ));
            }
            return self.bind_term(carrier, inner, span);
        }

        if let Term::ValueAddress(carrier, true) = term
            && let Type::Address {
                mutable: true,
                inner,
            } = ty
            && registration::borrowed_environment(ty).is_some()
        {
            return self.bind_term(carrier, inner, span);
        }
        if let Term::Registration(environment, owned, asynchronous) = term {
            let Some(wanted) = registration::environment(ty) else {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "registration carrier differs from expected type",
                ));
            };
            if *asynchronous != registration::asynchronous(ty)
                || matches!(ty, Type::Managed(_)) && !owned
            {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "registration descriptor does not own its environment",
                ));
            }
            let target = self.known_term(wanted, span)?;
            return self.unify(environment, &target, span);
        }
        if let Term::ValueStore(element, owned) = term
            && let Some(wanted) = dynamic_store::element(ty)
        {
            if matches!(ty, Type::Managed(_)) && !owned {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "Store description is not an owning carrier",
                ));
            }
            return self.bind_term(element, wanted, span);
        }
        if let Term::ValueUnion {
            error: failure,
            result,
        } = term
            && let Type::ErrorUnion {
                error,
                result: wanted,
            } = ty
        {
            self.bind_term(failure, error, span)?;
            return self.bind_term(result, wanted, span);
        }
        if self.helper_environments.keys().any(|name| {
            matches!(ty,Type::Named(n) if n==name) || callable_fields::placeholder_package(ty, name)
        }) {
            let expected = self.known_term(ty, span)?;
            return self.unify(term, &expected, span);
        }
        if let Term::Resource(variable, managed) = term {
            return match ty {
                Type::Desc(payload) => self.bind(*variable, payload, span),
                Type::Managed(payload) if *managed => self.bind(*variable, payload, span),
                _ => Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "resource descriptor/carrier mode differs from the expected type",
                )),
            };
        }
        if let Term::Address(variable, mutable) = term {
            return match ty {
                Type::Address {
                    mutable: wanted,
                    inner,
                } if (inner.is_address_pointee()
                    || stored::shared_address(ty)
                    || ty.is_shared_sync_cell_address()
                    || ty.is_shared_resource_head_address()
                    || ty.is_exclusive_task_head_address()
                    || resources::head_address(ty)
                    || ty.is_package_address()
                    || reference_patterns::sum_address(ty)
                    || self.graph.symbolic_store_type(inner))
                    && (*mutable || !wanted) =>
                {
                    self.bind(*variable, inner, span)
                }
                Type::Address { mutable: true, .. } if !mutable => Err(error(
                    "E-ADDRESS-PERMISSION",
                    span,
                    "a shared address cannot grant exclusive access",
                )),
                Type::Address { .. } => Err(unsupported(
                    span,
                    "only shared finite Copy addresses are implemented",
                )),
                _ => Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "shared address does not match scalar type",
                )),
            };
        }
        let variable = self.scalar(term, span)?;
        self.bind(variable, ty, span)
    }
    fn unify(&mut self, left: &Term, right: &Term, span: Span) -> R<()> {
        self.budget.work(1, span)?;
        if matches!(left, Term::Function(_)) || matches!(right, Term::Function(_)) {
            let a = self.scalar(left, span)?;
            let b = self.scalar(right, span)?;
            return self.graph.unify(a, b, span);
        }
        match (left, right) {
            (Term::InputTuple(a), Term::InputTuple(b)) => {
                self.budget.work(a.len().saturating_add(b.len()), span)?;
                if a.len() != b.len() {
                    return Err(error(
                        "E-TYPE-MISMATCH",
                        span,
                        "pending input products change Tuple arity",
                    ));
                }
                for (a, b) in a.iter().zip(b.iter()) {
                    self.unify(a, b, span)?;
                }
                Ok(())
            }
            (Term::InputTuple(_), Term::Scalar(variable)) => {
                let ty = self.graph.ty(*variable)?.ok_or_else(|| {
                    unsupported(
                        span,
                        "pending input product requires a complete Tuple constraint",
                    )
                })?;
                self.bind_term(left, &ty, span)
            }
            (Term::Scalar(_), Term::InputTuple(_)) => self.unify(right, left, span),
            (Term::PendingTask(a, am), Term::PendingTask(b, bm)) if am == bm => {
                self.unify(&a.result, &b.result, span)?;
                if !Rc::ptr_eq(a, b) {
                    let own = a.requests.borrow();
                    let other = b.requests.borrow();
                    self.budget.work(
                        own.len().saturating_mul(2).saturating_add(other.len()),
                        span,
                    )?;
                    let mut requests = own.clone();
                    for request in other.iter() {
                        self.budget.work(requests.len().saturating_add(1), span)?;
                        if !requests.iter().any(|old| {
                            old.caller == request.caller
                                && old.closure_scope == request.closure_scope
                                && old.call == request.call
                                && old.worker.template == request.worker.template
                                && old.worker.id == request.worker.id
                                && old.recover_inputs == request.recover_inputs
                        }) {
                            if requests.len() >= self.budget.limits.max_nodes {
                                return Err(Diagnostic::limit(
                                    Phase::Types,
                                    "pending Task request inventory exceeded",
                                    Some(span),
                                ));
                            }
                            requests.push(request.clone());
                        }
                    }
                    drop(own);
                    drop(other);
                    let mut constraints = Vec::new();
                    for source in [a, b] {
                        for constraint in source.constraints.borrow().iter() {
                            charge_type_snapshot(
                                constraint,
                                span,
                                self.budget.limits,
                                &mut self.budget.work,
                                2,
                            )?;
                            if constraints.len() >= self.budget.limits.max_nodes {
                                return Err(Diagnostic::limit(
                                    Phase::Types,
                                    "pending Task constraint inventory exceeded",
                                    Some(span),
                                ));
                            }
                            constraints.push(constraint.clone());
                        }
                    }
                    *a.requests.borrow_mut() = requests.clone();
                    *b.requests.borrow_mut() = requests;
                    *a.constraints.borrow_mut() = constraints.clone();
                    *b.constraints.borrow_mut() = constraints;
                }
                Ok(())
            }

            (Term::PendingTask(task, owned), Term::Resource(variable, managed))
                if owned == managed =>
            {
                let payload = self.graph.ty(*variable)?.ok_or_else(|| {
                    unsupported(span, "Task constraint awaits its complete declared payload")
                })?;
                let ty = if *managed {
                    Type::Managed(Box::new(payload))
                } else {
                    Type::Desc(Box::new(payload))
                };
                self.bind_pending_task(task, *owned, &ty, span)
            }
            (Term::Resource(..), Term::PendingTask(..)) => self.unify(right, left, span),
            (Term::Registration(a, am, aa), Term::Registration(b, bm, ba))
                if am == bm && aa == ba =>
            {
                self.unify(a, b, span)
            }
            (Term::ValueStore(a, _), Term::ValueStore(b, _)) => self.unify(a, b, span),
            (Term::ValueStore(a, _), Term::Resource(..)) => {
                let b = self.value_store_element(right, span)?;
                self.unify(a, &b, span)
            }
            (Term::Resource(..), Term::ValueStore(b, _)) => {
                let a = self.value_store_element(left, span)?;
                self.unify(&a, b, span)
            }
            (
                Term::ValueUnion {
                    error: a,
                    result: b,
                },
                _,
            ) => {
                let (c, d) = self.value_union_parts(right, span)?;
                self.unify(a, &c, span)?;
                self.unify(b, &d, span)
            }
            (_, Term::ValueUnion { .. }) => self.unify(right, left, span),
            (Term::ValueAddress(a, am), Term::ValueAddress(b, bm)) if am == bm => {
                self.unify(a, b, span)
            }
            (Term::Scalar(a), Term::Scalar(b)) => self.graph.unify(*a, *b, span),
            (Term::Address(a, am), Term::Address(b, bm)) if am == bm => {
                self.graph.unify(*a, *b, span)
            }
            (Term::Resource(a, am), Term::Resource(b, bm)) if am == bm => {
                self.graph.unify(*a, *b, span)
            }
            (Term::Scalar(_), Term::Address(..)) | (Term::Address(..), Term::Scalar(_)) => {
                Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "shared address and scalar types differ",
                ))
            }
            (
                Term::ClosureArray {
                    element: a,
                    length: an,
                },
                Term::ClosureArray {
                    element: b,
                    length: bn,
                },
            ) if a == b && an == bn => Ok(()),
            (Term::ClosureArray { .. }, Term::ClosureArray { .. }) => Err(error(
                "E-TYPE-MISMATCH",
                span,
                "callback arrays require the same source factory identity and fixed length",
            )),
            (
                Term::ClosurePackage {
                    shape: a,
                    environment: ae,
                },
                Term::ClosurePackage {
                    shape: b,
                    environment: be,
                },
            ) => {
                validate_type(a, 0, span, self.budget)?;
                validate_type(b, 0, span, self.budget)?;
                if ae == be && a == b {
                    Ok(())
                } else {
                    Err(error(
                        "E-TYPE-MISMATCH",
                        span,
                        "callable State requires the same nominal shape and source factory identity",
                    ))
                }
            }
            (
                Term::StatePackage {
                    shape: a,
                    fields: af,
                },
                Term::StatePackage {
                    shape: b,
                    fields: bf,
                },
            ) => {
                validate_type(a, 0, span, self.budget)?;
                validate_type(b, 0, span, self.budget)?;
                self.budget.work(af.len().saturating_add(bf.len()), span)?;
                if a == b && af == bf {
                    Ok(())
                } else {
                    Err(error(
                        "E-TYPE-MISMATCH",
                        span,
                        "State requires the same nominal shape and both field source identities",
                    ))
                }
            }
            (Term::Closure(a), Term::Closure(b)) if a == b => Ok(()),
            (Term::Closure(_), Term::Closure(_)) => Err(error(
                "E-TYPE-MISMATCH",
                span,
                "different closure expressions have different concrete types",
            )),
            _ => Err(unsupported(
                span,
                "higher-order scalar conversion is not implemented",
            )),
        }
    }
    fn insert(&mut self, name: &str, term: Term, mutable: bool, span: Span) -> R<()> {
        let term = self.value_term(term, span)?;
        self.budget.work(name.len(), span)?;
        self.budget.work(1, span)?;
        if !matches!(
            term,
            Term::Scalar(_)
                | Term::Address(..)
                | Term::Resource(..)
                | Term::Registration(..)
                | Term::Closure(_)
                | Term::ClosureArray { .. }
                | Term::ClosurePackage { .. }
                | Term::StatePackage { .. }
                | Term::ValueStore(..)
                | Term::ValueAddress(..)
                | Term::ValueUnion { .. }
                | Term::InputTuple(_)
                | Term::CreateFailure(_)
                | Term::PendingTask(..)
                | Term::ClosureAddress(..)
        ) {
            return Err(unsupported(
                span,
                "stored function/module values are not implemented",
            ));
        }
        let binding = Binding {
            term,
            owner: self.owner,
            mutable,
        };
        if self
            .scopes
            .last_mut()
            .expect("scope")
            .insert(name.into(), binding)
            .is_some()
        {
            return Err(error(
                "E-NAME-DUPLICATE",
                span,
                format!("duplicate local {name}"),
            ));
        }
        Ok(())
    }
    fn name(&mut self, name: &str, write: bool, span: Span) -> R<Term> {
        self.budget.work(name.len(), span)?;
        self.budget.work(1, span)?;
        if let Some(binding) = self.scopes.iter().rev().find_map(|s| s.get(name)).cloned() {
            if let Some(owner) = self.owner
                && binding.owner != self.owner
                && self.closures[owner].function_pointer
            {
                return Err(error(
                    "E-CAPTURED-FN-POINTER",
                    span,
                    "function pointer conversion requires a closure with no captures",
                ));
            }
            if self.owner.is_some()
                && binding.owner != self.owner
                && (matches!(
                    &binding.term,
                    Term::StatePackage { .. }
                        | Term::ValueStore(..)
                        | Term::ValueAddress(..)
                        | Term::ValueUnion { .. }
                ) || matches!(&binding.term, Term::Closure(id) | Term::ClosureAddress(id, _) if self.state_modes.contains_key(id)))
            {
                return Err(unsupported(
                    span,
                    "capturing a new State declaration source is outside the finite preserved capability profile",
                ));
            }
            // The concrete capture checker preserves the complete slice type
            // and its actual address mode. Inference only relates that type;
            // the source/loan kernels independently prove the captured window.
            // This graph collects source type relationships only. The concrete
            // closure checker later admits explicit-move Package captures and
            // independently classifies their responsibility slots.
            if write
                && !binding.mutable
                && !matches!(
                    binding.term,
                    Term::Address(_, true)
                        | Term::Resource(..)
                        | Term::ValueStore(..)
                        | Term::ValueAddress(_, true)
                )
            {
                return Err(error(
                    "E-IMMUTABLE-ASSIGNMENT",
                    span,
                    format!("{name} is not mutable"),
                ));
            }
            return Ok(match binding.term {
                Term::Registration(environment, _, asynchronous) => {
                    Term::Registration(environment, false, asynchronous)
                }
                Term::Resource(variable, _) => Term::Resource(variable, false),
                Term::PendingTask(task, _) => Term::PendingTask(task, false),
                Term::ValueStore(element, _) => Term::ValueStore(element, false),
                other => other,
            });
        }
        if !write {
            if let Some(signature) = self.signatures.get(name) {
                return Ok(Term::Function(Rc::new(context_signature(
                    signature,
                    self.budget,
                    span,
                )?)));
            }
            if let Some(module) = self.imports.and_then(|i| i.aliases.get(name)) {
                return Ok(Term::Module(*module));
            }
        }
        Err(error(
            "E-UNKNOWN-NAME",
            span,
            format!("unknown value {name}"),
        ))
    }
    fn constructor(&mut self, expression: &CoreExpr) -> R<Option<(Type, usize)>> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &expression.kind
        else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        if self
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
        {
            return Ok(None);
        }
        let Some(ty) = enums::constructor_type(self.enums, name, expression.span)? else {
            return Ok(None);
        };
        let Type::Enum { variants, .. } = stored::physical(ty) else {
            return Ok(None);
        };
        if owner.is_some() {
            return Err(error(
                "E-MEMBER-OWNER",
                expression.span,
                "a constrained member cannot denote an enum constructor namespace",
            ));
        }
        validate_type(ty, 0, expression.span, self.budget)?;
        let index = variants
            .iter()
            .position(|(name, _)| name == member)
            .ok_or_else(|| error("E-ENUM-VARIANT", expression.span, "unknown enum variant"))?;
        Ok(Some((ty.clone(), index)))
    }
    fn variant(
        &mut self,
        ty: &Type,
        index: usize,
        arguments: &[CoreExpr],
        depth: usize,
        span: Span,
    ) -> R<Term> {
        let Type::Enum { variants, .. } = stored::physical(ty) else {
            return Err(error(
                "E-ENUM-TYPE",
                span,
                "enum constructor lacks its actual physical variant layout",
            ));
        };
        let fields = &variants[index].1;
        if fields.len() != arguments.len() {
            return Err(error(
                "E-ENUM-ARITY",
                span,
                "enum variant payload arity mismatch",
            ));
        }
        for (argument, expected) in arguments.iter().zip(fields) {
            let actual = self.expr_expected(argument, Some(expected), depth + 1, false)?;
            self.bind_term(&actual, expected, argument.span)?;
        }
        self.known_term(ty, span)
    }
    fn projected_base(&mut self, object: &CoreExpr, depth: usize, write: bool) -> R<Term> {
        // An immutable, unannotated callback parameter can be written through
        // a projection only when its eventual argument is an exclusive address.
        // Ordinary Value bindings still retain their existing mutability rule.
        let mut inferred = None;
        if write
            && let Some(owner) = self.owner
            && let CoreExprKind::Name(name) = &object.kind
        {
            self.budget
                .work(name.len().saturating_add(1), object.span)?;
            let parameter = self
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(name))
                .and_then(|binding| match binding.term {
                    Term::Scalar(variable) if !binding.mutable && binding.owner == Some(owner) => {
                        Some(variable)
                    }
                    _ => None,
                });
            if let Some(variable) = parameter {
                self.budget
                    .work(self.closures[owner].parameters.len(), object.span)?;
                if self.closures[owner].parameters.contains(&variable)
                    && self.graph.ty(variable)?.is_none()
                {
                    inferred = Some(variable);
                }
            }
        }
        let term = self.expr(object, depth, write && inferred.is_none())?;
        if let Some(variable) = inferred {
            let inner = self.fresh(VALUE, false, object.span)?;
            self.graph
                .address(variable, inner, Some(true), object.span)?;
            Ok(Term::Address(inner, true))
        } else {
            Ok(term)
        }
    }

    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.budget.node(depth, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.budget.node(depth + 1, statement.span)?;
            let index = self.statement_index;
            self.statement_index += 1;
            match &statement.kind {
                CoreStatementKind::ForEach { .. }
                | CoreStatementKind::CompoundAssign { .. }
                | CoreStatementKind::Yield(_)
                | CoreStatementKind::Loop(_)
                | CoreStatementKind::With { .. }
                | CoreStatementKind::CompileTime(_) => {
                    return Err(error(
                        "E-CORE-ELABORATION",
                        statement.span,
                        "unelaborated source statement reached executable Core",
                    ));
                }

                CoreStatementKind::TupleBinding {
                    pattern,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let value = self.expr_expected(initializer, ty.as_ref(), depth + 1, false)?;
                    let value = self.value_term(value, statement.span)?;
                    if let Some(ty) = ty {
                        self.bind_term(&value, ty, statement.span)?;
                    }
                    if matches!(value, Term::InputTuple(_)) {
                        self.input_pattern(pattern, &value, *mutable, depth + 1)?;
                        self.binding_types.insert(index, value);
                        continue;
                    }
                    let variable = self.scalar(&value, statement.span)?;
                    self.binding_types.insert(index, value.clone());
                    for (name, variable, span) in self.graph.pattern_bind(pattern, variable)? {
                        let term = self.pattern_term(variable, span)?;
                        self.insert(&name, term, *mutable, span)?;
                    }
                }
                CoreStatementKind::Binding {
                    name,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let Some(initializer) = initializer else {
                        return Err(unsupported(statement.span, "uninitialized source binding"));
                    };
                    let value = self.expr_expected(initializer, ty.as_ref(), depth + 1, false)?;
                    let value = self.value_term(value, statement.span)?;
                    if let Some(ty) = ty {
                        self.bind_term(&value, ty, statement.span)?;
                    }
                    self.binding_types.insert(index, value.clone());
                    self.insert(name, value, *mutable, statement.span)?;
                }
                CoreStatementKind::Assign { target, value } => {
                    // A source name ordinarily reads a descriptor. Assignment
                    // to an owning mutable binding instead constrains the new
                    // carrier; concrete slot lowering and Verify decide safety.
                    let owned_target = if let CoreExprKind::Name(name) = &target.kind {
                        self.scopes
                            .iter()
                            .rev()
                            .find_map(|scope| scope.get(name))
                            .is_some_and(|b| {
                                b.mutable
                                    && matches!(
                                        b.term,
                                        Term::Resource(_, true) | Term::PendingTask(_, true)
                                    )
                            })
                    } else {
                        false
                    };
                    let target_type = self.expr(target, depth + 1, true)?;
                    let hint = if let Term::Scalar(variable) = target_type {
                        self.graph.ty(variable)?
                    } else {
                        None
                    };
                    let value_type = self.expr_expected(value, hint.as_ref(), depth + 1, false)?;
                    if owned_target {
                        match (&target_type, &value_type) {
                            (Term::Resource(a, _), Term::Resource(b, true)) => {
                                self.graph.unify(*a, *b, statement.span)?
                            }
                            (Term::PendingTask(a, _), Term::PendingTask(b, true)) => {
                                self.unify(
                                    &Term::PendingTask(a.clone(), true),
                                    &Term::PendingTask(b.clone(), true),
                                    statement.span,
                                )?;
                            }
                            _ => {
                                return Err(error(
                                    "E-TYPE-MISMATCH",
                                    statement.span,
                                    "managed replacement requires a new owning carrier",
                                ));
                            }
                        }
                    } else if let Some(expected @ Type::Address { .. }) = hint.as_ref() {
                        // A resolved stored field can be represented by one
                        // concrete graph node while `&value` has an Address
                        // term. Compare their complete types, preserving mode.
                        self.bind_term(&value_type, expected, statement.span)?;
                    } else {
                        self.unify(&target_type, &value_type, statement.span)?;
                    }
                }
                CoreStatementKind::Expression(e) => {
                    self.expr(e, depth + 1, false)?;
                }
                CoreStatementKind::Return(e) => {
                    if self.owner.is_none()
                        && let Some(bound) = self.ordinary_callable_return.clone()
                    {
                        let e = e.as_ref().ok_or_else(|| {
                            error(
                                "E-TYPE-MISMATCH",
                                statement.span,
                                "callable factory must return a concrete closure",
                            )
                        })?;
                        let previous = self
                            .callable_hint
                            .replace((bound.parameters.clone(), bound.result.clone()));
                        let term = self.expr_expected(e, None, depth + 1, false)?;
                        self.callable_hint = previous;
                        let Term::Closure(id) = term else {
                            return Err(error(
                                "E-TYPE-MISMATCH",
                                e.span,
                                "opaque callable return requires a closure environment",
                            ));
                        };
                        if self
                            .ordinary_return_identity
                            .is_some_and(|previous| previous != id)
                        {
                            return Err(error(
                                "E-TYPE-MISMATCH",
                                e.span,
                                "all opaque callable returns require the same source-visible concrete identity",
                            ));
                        }
                        self.ordinary_return_identity = Some(id);
                        let parameters = self.closures[id].parameters.clone();
                        if parameters.len() != bound.parameters.len() {
                            return Err(error(
                                "E-CALL-ARITY",
                                e.span,
                                "returned closure signature differs from public bound",
                            ));
                        }
                        for (variable, ty) in parameters.into_iter().zip(&bound.parameters) {
                            self.bind(variable, ty, e.span)?;
                        }
                        self.bind(self.closures[id].result, &bound.result, e.span)?;
                        continue;
                    }
                    let hint = match self.owner {
                        Some(owner) => self.graph.ty(self.closures[owner].result)?,
                        None => {
                            validate_type(&self.ordinary_result, 0, statement.span, self.budget)?;
                            Some(self.ordinary_result.clone())
                        }
                    };
                    let value = if let Some(e) = e {
                        self.expr_expected(e, hint.as_ref(), depth + 1, false)?
                    } else if let Some(union @ Type::ErrorUnion { .. }) = &hint {
                        errors::injection(&Type::Unit, union, statement.span)?;
                        self.known_term(union, statement.span)?
                    } else {
                        Term::Scalar(self.fresh(UNIT, false, statement.span)?)
                    };
                    if let Some(owner) = self.owner {
                        self.closures[owner].has_return = true;
                        if let Term::Address(inner, false) = value {
                            self.graph.shared_address(
                                self.closures[owner].result,
                                inner,
                                statement.span,
                            )?;
                        } else if let Term::Address(inner, true) = value {
                            self.graph.address(
                                self.closures[owner].result,
                                inner,
                                Some(true),
                                statement.span,
                            )?;
                        } else if let Term::Resource(payload, true) = &value
                            && let Some(task @ Type::Task { .. }) = self.graph.ty(*payload)?
                        {
                            // Preserve the actual owning Task head in the
                            // closure result type; do not scalarize its payload
                            // or infer any permission from its result element.
                            self.bind(
                                self.closures[owner].result,
                                &Type::Managed(Box::new(task)),
                                statement.span,
                            )?;
                        } else {
                            self.unify(
                                &value,
                                &Term::Scalar(self.closures[owner].result),
                                statement.span,
                            )?;
                        }
                    } else {
                        self.bind_term(
                            &value,
                            hint.as_ref().expect("ordinary return type"),
                            statement.span,
                        )?;
                    }
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition_type = self.expr(condition, depth + 1, false)?;
                    self.bind_term(&condition_type, &Type::Bool, condition.span)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    let condition_type = self.expr(condition, depth + 1, false)?;
                    self.bind_term(&condition_type, &Type::Bool, condition.span)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let value_type = self.expr(value, depth + 1, false)?;
                    let variable = self.scalar(&value_type, value.span)?;
                    self.scopes.push(BTreeMap::new());
                    for (name, variable, span) in self.graph.pattern_bind(pattern, variable)? {
                        let term = self.pattern_term(variable, span)?;
                        self.insert(&name, term, false, span)?;
                    }
                    self.block(then_body, depth + 1)?;
                    self.scopes.pop();
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let value_type = self.expr(value, depth + 1, false)?;
                    let variable = self.scalar(&value_type, value.span)?;
                    self.scopes.push(BTreeMap::new());
                    for (name, variable, span) in self.graph.pattern_bind(pattern, variable)? {
                        let term = self.pattern_term(variable, span)?;
                        self.insert(&name, term, false, span)?;
                    }
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    let start_type = self.expr(start, depth + 1, false)?;
                    let end_type = self.expr(end, depth + 1, false)?;
                    self.unify(&start_type, &end_type, statement.span)?;
                    let variable = self.scalar(&start_type, start.span)?;
                    self.budget.work(1, start.span)?;
                    self.graph.constrain(variable, INTEGER, start.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.insert(name, start_type, false, statement.span)?;
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    let value_type = self.expr(value, depth + 1, false)?;
                    if matches!(value_type, Term::ValueUnion { .. }) {
                        for arm in arms {
                            self.scopes.push(BTreeMap::new());
                            self.input_pattern(&arm.pattern, &value_type, false, depth + 1)?;
                            self.block(&arm.body, depth + 1)?;
                            self.scopes.pop();
                        }
                        continue;
                    }
                    let variable = self.scalar(&value_type, value.span)?;
                    for arm in arms {
                        self.scopes.push(BTreeMap::new());
                        for (name, variable, span) in
                            self.graph.pattern_bind(&arm.pattern, variable)?
                        {
                            let term = self.pattern_term(variable, span)?;
                            self.insert(&name, term, false, span)?;
                        }
                        self.block(&arm.body, depth + 1)?;
                        self.scopes.pop();
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body)
                | CoreStatementKind::Defer(body) => self.block(body, depth + 1)?,
                CoreStatementKind::Break | CoreStatementKind::Continue => {}
                CoreStatementKind::Unsupported(feature) => {
                    return Err(unsupported(statement.span, feature));
                }
            }
        }
        self.scopes.pop();
        Ok(())
    }
    fn expr(&mut self, expression: &CoreExpr, depth: usize, write: bool) -> R<Term> {
        self.expr_expected(expression, None, depth, write)
    }

    fn record_hints(&mut self, value: &CoreExpr) -> R<BTreeMap<String, Type>> {
        let mut hints = BTreeMap::new();
        let base_type;
        let (ty, fields) = match &value.kind {
            CoreExprKind::Record { ty, fields } => (Some(ty), fields),
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                base_type = if let CoreExprKind::Name(name) = &base.kind {
                    match self.scopes.iter().rev().find_map(|scope| scope.get(name)) {
                        Some(Binding {
                            term: Term::Scalar(variable),
                            ..
                        }) => self.graph.ty(*variable)?,
                        _ => None,
                    }
                } else {
                    None
                };
                (ty.as_ref().or(base_type.as_ref()), fields)
            }
            _ => return Ok(hints),
        };
        if let Some(ty) = ty {
            let ty = match ty {
                Type::Managed(payload) | Type::Desc(payload) => payload.as_ref(),
                _ => ty,
            };
            for (field, expression) in fields {
                if let CoreExprKind::Name(name) = &expression.kind {
                    // Account for the field search and resulting type clone
                    // before reading a possibly large nominal shape.
                    validate_type(ty, 0, expression.span, self.budget)?;
                    self.budget.work(name.len(), expression.span)?;
                    let (_, ty) = aggregates::field(ty, field, expression.span)?;
                    hints.insert(name.clone(), ty);
                }
            }
        }
        Ok(hints)
    }

    fn expr_expected(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
        write: bool,
    ) -> R<Term> {
        if let Some(sum) = expected.filter(|ty| enums::anonymous(ty))
            && !matches!(
                &expression.kind,
                CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            )
        {
            self.graph.reserve_type(sum, 0, expression.span)?;
            self.graph.reserve_type(sum, 0, expression.span)?;
            let term = self.expr_raw(expression, None, depth, write)?;
            let variable = self.scalar(&term, expression.span)?;
            let actual = self.graph.injection_type(variable, expression.span)?;
            enums::anonymous_injection(&actual, sum, expression.span)?;
            return self.known_term(sum, expression.span);
        }
        if let Some(union @ Type::ErrorUnion { .. }) = expected
            && !matches!(
                &expression.kind,
                CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            )
        {
            let constructor_expected = match &expression.kind {
                CoreExprKind::Call { callee, .. }
                    if matches!(&callee.kind, CoreExprKind::TypeApply { callee, .. }
                        if errors::constructor(callee).is_some()) =>
                {
                    Some(union)
                }
                _ => None,
            };
            let term = self.expr_raw(expression, constructor_expected, depth, write)?;
            if matches!(term, Term::ValueUnion { .. }) {
                self.bind_term(&term, union, expression.span)?;
                return Ok(term);
            }
            if let Term::PendingTask(task, true) = &term {
                let Type::ErrorUnion {
                    error: failure,
                    result,
                } = union
                else {
                    unreachable!()
                };
                let error_task = matches!(failure.as_ref(), Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }));
                let result_task = matches!(result.as_ref(), Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }));
                return match (error_task, result_task) {
                    (false, true) => {
                        self.bind_pending_task(task, true, result, expression.span)?;
                        Ok(Term::ValueUnion {
                            error: Rc::new(self.known_term(failure, expression.span)?),
                            result: Rc::new(term),
                        })
                    }
                    (true, false) => {
                        self.bind_pending_task(task, true, failure, expression.span)?;
                        Ok(Term::ValueUnion {
                            error: Rc::new(term),
                            result: Rc::new(self.known_term(result, expression.span)?),
                        })
                    }
                    _ => Err(error(
                        "E-ERROR-INJECTION-AMBIGUOUS",
                        expression.span,
                        "a pending Task needs an explicit tagged value when both union arms are Task carriers",
                    )),
                };
            }
            if let Term::Resource(payload, true) = &term
                && let Some(task @ Type::Task { .. }) = self.graph.ty(*payload)?
            {
                let owner = Type::Managed(Box::new(task));
                charge_type_snapshot(
                    &owner,
                    expression.span,
                    self.budget.limits,
                    &mut self.budget.work,
                    2,
                )?;
                charge_type_snapshot(
                    union,
                    expression.span,
                    self.budget.limits,
                    &mut self.budget.work,
                    2,
                )?;
                errors::injection(&owner, union, expression.span)?;
                return self.known_term(union, expression.span);
            }
            let variable = self.scalar(&term, expression.span)?;
            let actual = self.graph.injection_type(variable, expression.span)?;
            errors::injection(&actual, union, expression.span)?;
            return self.known_term(union, expression.span);
        }
        self.expr_raw(expression, expected, depth, write)
    }
    // Recursive variants have separate frames: a projection must not retain
    // unrelated call/member/closure temporaries on small API worker stacks.
    fn expr_raw(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
        write: bool,
    ) -> R<Term> {
        self.budget.node(depth, expression.span)?;
        let index = self.expression_index;
        self.expression_index += 1;
        let span = expression.span;
        if write
            && !matches!(
                expression.kind,
                CoreExprKind::Name(_)
                    | CoreExprKind::Member { .. }
                    | CoreExprKind::Index { .. }
                    | CoreExprKind::Slice { .. }
                    | CoreExprKind::Unary {
                        op: UnaryOp::Dereference,
                        ..
                    }
            )
        {
            return Err(unsupported(span, "assignment to projected place"));
        }
        match &expression.kind {
            CoreExprKind::Literal(value) => {
                foreign::check_literal(value, span)?;
                self.known_term(&value.ty(), span)
            }
            CoreExprKind::Float(text) => {
                self.budget
                    .work(zil_base::numeric::decimal_work(text.len()), span)?;
                let domain = if text.ends_with("f32") {
                    F32
                } else if text.ends_with("f64") {
                    F64
                } else {
                    FLOAT
                };
                let variable = self.fresh(domain, domain == FLOAT, span)?;
                if let Some(expected) = expected.filter(|ty| ty.is_float()) {
                    self.graph.bind(variable, expected, span)?;
                }
                self.literal_types.insert(index, variable);
                Ok(Term::Scalar(variable))
            }
            CoreExprKind::NumericConvert { value, target } => {
                if !target.is_numeric() {
                    return Err(error(
                        "E-NUMERIC-CONVERSION",
                        span,
                        "numeric target required",
                    ));
                }
                let operand = self.expr(value, depth + 1, false)?;
                let variable = self.scalar(&operand, value.span)?;
                self.graph.constrain(variable, NUMBER, span)?;
                self.known_term(target, span)
            }
            CoreExprKind::IntegerConvert { .. } => self.expr_integer_convert(expression, depth),
            CoreExprKind::Ascribed { .. } => self.expr_ascribed(expression, depth),
            CoreExprKind::Try(..) => self.expr_try(expression, depth),
            CoreExprKind::Integer(text) => {
                self.budget.work(text.len(), span)?;
                let domain = if text.ends_with("u8") {
                    U8
                } else if text.ends_with("u32") {
                    U32
                } else if text.ends_with("i8") {
                    I8
                } else if text.ends_with("i16") {
                    I16
                } else if text.ends_with("isize") {
                    ISIZE
                } else if text.ends_with("u16") {
                    U16
                } else if text.ends_with("u64") {
                    U64
                } else if text.ends_with("usize") {
                    USIZE
                } else if text.ends_with("i64") {
                    I64
                } else if text.ends_with("i32") {
                    I32
                } else {
                    INTEGER
                };
                let variable = self.fresh(domain, domain == INTEGER, span)?;
                self.literal_types.insert(index, variable);
                Ok(Term::Scalar(variable))
            }
            CoreExprKind::String(_) => self.known_term(&utf8_literals::ty(), span),
            CoreExprKind::Bool(_) => Ok(Term::Scalar(self.fresh(BOOL, false, span)?)),
            CoreExprKind::Unit => Ok(Term::Scalar(self.fresh(UNIT, false, span)?)),
            CoreExprKind::Name(name) => {
                let term = self.name(name, write, span)?;
                if matches!(expected, Some(Type::Managed(_)))
                    && self.prepared.contains(name)
                    && let Some(Binding {
                        term: Term::PendingTask(task, true),
                        ..
                    }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                {
                    return Ok(Term::PendingTask(task.clone(), true));
                }
                if matches!(expected, Some(Type::Managed(_)))
                    && self.prepared.contains(name)
                    && let Some(Binding {
                        term: Term::Resource(variable, true),
                        ..
                    }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                {
                    return Ok(Term::Resource(*variable, true));
                }
                Ok(term)
            }
            CoreExprKind::MakeDynOwned {
                interface, source, ..
            } => {
                self.expr(source, depth + 1, false)?;
                self.known_term(&dyn_dispatch::owned_result(*interface), span)
            }
            CoreExprKind::Unary { .. } => self.expr_unary(expression, depth, write),
            CoreExprKind::Binary { .. } => self.expr_binary(expression, depth),
            CoreExprKind::Call { .. } => self.expr_call(expression, expected, depth),
            CoreExprKind::Member { .. } => self.expr_member(expression, depth, write),
            CoreExprKind::Tuple(..) => self.expr_tuple(expression, expected, depth),
            CoreExprKind::Array(..) => self.expr_array(expression, expected, depth),
            CoreExprKind::Record { .. } => self.expr_record(expression, depth),
            CoreExprKind::RecordUpdate { .. } => self.expr_record_update(expression, depth),
            CoreExprKind::Index { .. } => self.expr_index(expression, depth, write),
            CoreExprKind::Slice { .. } => self.expr_slice(expression, depth, write),
            CoreExprKind::Sequence { .. } => self.expr_sequence(expression, expected, depth),
            CoreExprKind::CompileTime(..) => self.expr_compile_time(expression, expected, depth),
            CoreExprKind::Closure { .. } => self.expr_closure(expression, expected, depth, index),
            CoreExprKind::If { .. } => self.expr_if(expression, expected, depth),
            CoreExprKind::Unsupported(feature) => Err(unsupported(span, feature)),
            _ => Err(unsupported(
                span,
                "non-scalar expression in closure inference",
            )),
        }
    }
    #[inline(never)]
    fn expr_integer_convert(&mut self, expression: &CoreExpr, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::IntegerConvert { value, target } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        if !target.is_integer() {
            return Err(error(
                "E-INTEGER-CONVERSION",
                span,
                "conversion target must be an integer",
            ));
        }
        let operand = self.expr(value, depth + 1, false)?;
        let source = self.scalar(&operand, value.span)?;
        self.graph.constrain(source, INTEGER, value.span)?;
        self.known_term(target, span)
    }

    #[inline(never)]
    fn expr_ascribed(&mut self, expression: &CoreExpr, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Ascribed { ty, value } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let term = self.expr_expected(value, Some(ty), depth + 1, false)?;
        self.bind_term(&term, ty, span)?;
        Ok(term)
    }

    #[inline(never)]
    fn expr_try(&mut self, expression: &CoreExpr, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Try(operand) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let value = self.expr(operand, depth + 1, false)?;
        if let Term::ValueUnion {
            error: failure,
            result,
        } = value
        {
            let enclosing = match self.owner {
                Some(owner) => self.graph.ty(self.closures[owner].result)?,
                None => Some(self.ordinary_result.clone()),
            };
            let Some(Type::ErrorUnion {
                error: expected, ..
            }) = enclosing
            else {
                return Err(error(
                    "E-TRY-RETURN-TYPE",
                    span,
                    "? requires an explicitly known enclosing error-union result",
                ));
            };
            self.bind_term(&failure, &expected, span)?;
            self.budget.work(4, span)?;
            return Ok(result.as_ref().clone());
        }
        let variable = self.scalar(&value, span)?;
        let Type::ErrorUnion {
            error: failure,
            result,
        } = self.graph.injection_type(variable, span)?
        else {
            return Err(error(
                "E-TRY-NOT-ERROR",
                span,
                "? requires an error-union operand",
            ));
        };
        let enclosing = match self.owner {
            Some(owner) => self.graph.ty(self.closures[owner].result)?,
            None => Some(self.ordinary_result.clone()),
        };
        let Some(Type::ErrorUnion {
            error: expected, ..
        }) = enclosing
        else {
            return Err(error(
                "E-TRY-RETURN-TYPE",
                span,
                "? requires an explicitly known enclosing error-union result",
            ));
        };
        if failure != expected {
            return Err(error(
                "E-TRY-ERROR-TYPE",
                span,
                "? error type must exactly match the enclosing error type",
            ));
        }
        self.known_term(&result, span)
    }

    #[inline(never)]
    fn expr_unary(&mut self, expression: &CoreExpr, depth: usize, write: bool) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Unary { op, operand } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let value = self.expr(operand, depth + 1, false)?;
        if *op == UnaryOp::Give {
            if let CoreExprKind::Member { object, member, .. } = &operand.kind
                && let CoreExprKind::Name(name) = &object.kind
                && let Some(Binding {
                    term: Term::Scalar(variable),
                    ..
                }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                && let Some(ty) = self.graph.ty(*variable)?
                && matches!(stored::physical(&ty), Type::Package { .. })
            {
                let field = aggregates::field(&ty, member, span)?.1;
                if !matches!(field, Type::Managed(_)) && !packages::product::owned_supported(&field)
                {
                    return Err(error(
                        "E-GIVE-TYPE",
                        span,
                        "give requires a Managed package field",
                    ));
                }
                return self.known_term(&field, span);
            }
            let CoreExprKind::Name(name) = &operand.kind else {
                return Err(unsupported(
                    span,
                    "give requires a managed local or direct Managed package field",
                ));
            };
            return match self.scopes.iter().rev().find_map(|s| s.get(name)) {
                Some(Binding {
                    term: Term::PendingTask(task, true),
                    ..
                }) => Ok(Term::PendingTask(task.clone(), true)),
                Some(Binding {
                    term: Term::PendingTask(_, false),
                    ..
                }) => Err(error(
                    "E-GIVE-DESCRIPTOR",
                    span,
                    "a Task descriptor does not own its carrier",
                )),

                Some(Binding {
                    term: Term::Address(variable, true),
                    ..
                }) => Ok(Term::Address(*variable, true)),
                Some(Binding {
                    term: Term::Registration(environment, true, asynchronous),
                    ..
                }) => Ok(Term::Registration(environment.clone(), true, *asynchronous)),
                Some(Binding {
                    term: Term::ValueStore(element, true),
                    ..
                }) => Ok(Term::ValueStore(element.clone(), true)),
                Some(Binding {
                    term: Term::Resource(variable, true),
                    ..
                }) => Ok(Term::Resource(*variable, true)),
                Some(Binding {
                    term: Term::Resource(_, false),
                    ..
                }) => Err(error(
                    "E-GIVE-DESCRIPTOR",
                    span,
                    "a descriptor does not own the resource carrier",
                )),
                _ => Err(error(
                    "E-GIVE-TYPE",
                    span,
                    "give requires a managed resource carrier",
                )),
            };
        }
        if matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut) {
            if let CoreExprKind::Name(name) = &operand.kind
                && let Some(binding) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                && let Term::PendingTask(task, true) = &binding.term
            {
                return Ok(Term::ValueAddress(
                    Rc::new(Term::PendingTask(task.clone(), true)),
                    *op == UnaryOp::BorrowMut,
                ));
            }

            if *op == UnaryOp::BorrowMut
                && let CoreExprKind::Name(name) = &operand.kind
                && let Some(binding) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                && binding.mutable
                && matches!(binding.term, Term::Registration(_, true, true))
            {
                return Ok(Term::ValueAddress(Rc::new(binding.term.clone()), true));
            }
            if !matches!(
                operand.kind,
                CoreExprKind::Name(_)
                    | CoreExprKind::Member { .. }
                    | CoreExprKind::Index { .. }
                    | CoreExprKind::Slice { .. }
                    | CoreExprKind::Unary {
                        op: UnaryOp::Dereference,
                        ..
                    }
            ) {
                return Err(unsupported(span, "shared borrow requires a place"));
            }
            if let Term::Closure(id) = value {
                return Ok(Term::ClosureAddress(id, *op == UnaryOp::BorrowMut));
            }
            if let Term::ClosureAddress(id, available) = value {
                if *op == UnaryOp::BorrowMut && !available {
                    return Err(error(
                        "E-ADDRESS-PERMISSION",
                        span,
                        "a shared callback field cannot grant exclusive access",
                    ));
                }
                return Ok(Term::ClosureAddress(id, *op == UnaryOp::BorrowMut));
            }
            if matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut)
                && let CoreExprKind::Name(name) = &operand.kind
                && let Some(Binding {
                    term: Term::Resource(variable, true),
                    ..
                }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
            {
                let variable = *variable;
                self.graph.solve()?;
                let payload = self.resolved(variable, span)?;
                let task_head = *op == UnaryOp::BorrowMut
                    && matches!(payload, Type::TaskI32 | Type::Task { .. });
                if !task_head
                    && (*op != UnaryOp::Borrow
                        || !matches!(payload, Type::Record { .. })
                        || !payload.is_copy_value())
                {
                    return Err(unsupported(
                        span,
                        "head borrow requires a finite user-resource payload",
                    ));
                }
                let head = self.fresh(VALUE, false, span)?;
                self.bind(head, &Type::Managed(Box::new(payload)), span)?;
                return Ok(Term::Address(head, *op == UnaryOp::BorrowMut));
            }
            if matches!(value, Term::Address(..) | Term::ClosureAddress(..)) {
                return Err(unsupported(
                    span,
                    "nested shared addresses are not implemented",
                ));
            }
            return Ok(Term::Address(
                self.scalar(&value, span)?,
                *op == UnaryOp::BorrowMut,
            ));
        }
        if *op == UnaryOp::Dereference {
            return match value {
                Term::Address(variable, mutable) if !write || mutable => Ok(Term::Scalar(variable)),
                Term::Scalar(variable) => {
                    let inner = self.fresh(VALUE, false, span)?;
                    self.graph
                        .address(variable, inner, write.then_some(true), span)?;
                    Ok(Term::Scalar(inner))
                }
                _ => Err(error(
                    "E-OPERATOR-TYPE",
                    span,
                    "dereference requires a shared address",
                )),
            };
        }
        let variable = self.scalar(&value, span)?;
        let domain = match op {
            UnaryOp::Not => BOOL,
            UnaryOp::Negate => NUMBER,
            UnaryOp::BitNot => INTEGER,
            _ => {
                return Err(unsupported(
                    span,
                    "address/resource operation in closure inference",
                ));
            }
        };
        self.budget.work(1, span)?;
        self.graph.constrain(variable, domain, span)?;
        Ok(value)
    }

    #[inline(never)]
    fn expr_binary(&mut self, expression: &CoreExpr, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Binary { op, left, right } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let left = self.expr(left, depth + 1, false)?;
        let right = self.expr(right, depth + 1, false)?;
        self.unify(&left, &right, span)?;
        let variable = self.scalar(&left, span)?;
        let domain = if matches!(op, BinaryOp::And | BinaryOp::Or) {
            BOOL
        } else if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) {
            // CT already supports Unit equality. This pass only
            // collects types; runtime operator admission stays with
            // the ordinary checker after CT has been verified/run.
            SCALAR
        } else {
            NUMBER
        };
        self.budget.work(1, span)?;
        self.graph.constrain(variable, domain, span)?;
        if matches!(
            op,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            Ok(Term::Scalar(self.fresh(BOOL, false, span)?))
        } else {
            Ok(left)
        }
    }

    #[inline(never)]
    fn expr_call(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        if let CoreExprKind::TypeApply {
            callee: target,
            type_arguments,
        } = &callee.kind
            && matches!(&target.kind, CoreExprKind::Name(name) if name == "owned_dyn")
        {
            self.budget.node(depth + 1, callee.span)?;
            self.budget.node(depth + 2, target.span)?;
            self.expression_index += 2;
            let [Type::Dyn { interface }] = type_arguments.as_slice() else {
                return Err(error(
                    "E-DYN-OWNER",
                    span,
                    "owned_dyn needs one actual dyn interface",
                ));
            };
            let [source] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "owned_dyn needs one actual provider",
                ));
            };
            self.expr(source, depth + 1, false)?;
            // This private constraint grants no owner authority. Builder checks
            // the complete concrete provider and the exact owning table.
            return self.known_term(&dyn_dispatch::owned_result(*interface), span);
        }
        if let CoreExprKind::TypeApply {
            callee: target,
            type_arguments,
        } = &callee.kind
            && let Some(name) = errors::constructor(target)
        {
            self.budget.node(depth + 1, callee.span)?;
            self.budget.node(depth + 2, target.span)?;
            self.expression_index += 2;
            let [error_type, result_type] = type_arguments.as_slice() else {
                return Err(error(
                    "E-ERROR-CONSTRUCTOR",
                    span,
                    "error constructor needs explicit error and result types",
                ));
            };
            let [payload] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "error constructor needs one complete payload",
                ));
            };
            for ty in type_arguments {
                validate_type(ty, 0, callee.span, self.budget)?;
            }
            if let Some(expected) = expected {
                let work = type_snapshot_work(expected, span, self.budget.limits)?;
                self.budget.work(work.saturating_mul(2), span)?;
            }
            let result_type = errors::constructor_result(error_type, result_type, expected);
            let selected = if name == "error_err" {
                error_type
            } else {
                result_type
            };
            let actual = self.expr_expected(payload, Some(selected), depth + 1, false)?;
            self.bind_term(&actual, selected, payload.span)?;
            return self.known_term(
                &Type::ErrorUnion {
                    error: Box::new(error_type.clone()),
                    result: Box::new(result_type.clone()),
                },
                span,
            );
        }
        if let Some(name) = dynamic_store::primitive(callee) {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let arity = match name {
                "store_new" => 0,
                "store_reserve" | "store_push" | "store_at" | "store_at_mut" => 2,
                _ => 1,
            };
            if arguments.len() != arity {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "Store primitive argument count mismatch",
                ));
            }
            if name == "store_new" {
                let element = expected.and_then(dynamic_store::element).ok_or_else(|| {
                    error(
                        "E-OWNED-STORE-TYPE",
                        span,
                        "store_new requires a concrete Store context",
                    )
                })?;
                validate_type(element, 0, span, self.budget)?;
                return self.known_term(&dynamic_store::result_type(name, element), span);
            }
            let first = self.expr(&arguments[0], depth + 1, false)?;
            if let Term::ValueStore(element, _) = first {
                if arguments.len() == 2 {
                    if name == "store_push" {
                        let value = self.expr(&arguments[1], depth + 1, false)?;
                        self.unify(&value, &element, arguments[1].span)?;
                    } else {
                        let value =
                            self.expr_expected(&arguments[1], Some(&Type::I64), depth + 1, false)?;
                        self.bind_term(&value, &Type::I64, arguments[1].span)?;
                    }
                }
                self.budget.work(4, span)?;
                return match name {
                    "store_pop" => Ok(Term::ValueUnion {
                        error: Rc::new(self.known_term(&Type::Unit, span)?),
                        result: element,
                    }),
                    "store_push" => Ok(Term::ValueUnion {
                        error: element,
                        result: Rc::new(self.known_term(&Type::Unit, span)?),
                    }),
                    "store_at" | "store_at_mut" => {
                        if let Term::Scalar(variable) = element.as_ref() {
                            Ok(Term::Address(*variable, name == "store_at_mut"))
                        } else {
                            Ok(Term::ValueAddress(element, name == "store_at_mut"))
                        }
                    }
                    "store_reserve" => self.known_term(&Type::Bool, span),
                    _ => self.known_term(&Type::I64, span),
                };
            }
            let variable = match first {
                Term::Resource(variable, _) => variable,
                Term::Address(head, _) => {
                    let Some(Type::Managed(payload)) = self.graph.ty(head)? else {
                        return Err(error(
                            "E-OWNED-STORE-TYPE",
                            span,
                            "Store primitive requires a resource description",
                        ));
                    };
                    validate_type(&payload, 0, span, self.budget)?;
                    if !matches!(payload.as_ref(), Type::OwnedStore { element }
                        if dynamic_store::inline_element(element)
                            && dynamic_store::supported_element(element))
                    {
                        return Err(error(
                            "E-OWNED-STORE-TYPE",
                            span,
                            "Store primitive requires a resource description",
                        ));
                    }
                    let variable = self.fresh(VALUE, false, span)?;
                    self.bind(variable, &payload, span)?;
                    variable
                }
                _ => {
                    return Err(error(
                        "E-OWNED-STORE-TYPE",
                        span,
                        "Store primitive requires a resource description",
                    ));
                }
            };
            let Some(Type::OwnedStore { element }) = self.graph.ty(variable)? else {
                return Err(error(
                    "E-OWNED-STORE-TYPE",
                    span,
                    "Store primitive requires a concrete Store element",
                ));
            };
            validate_type(&element, 0, span, self.budget)?;
            if matches!(
                name,
                "store_reserve" | "store_push" | "store_at" | "store_at_mut"
            ) {
                let wanted = if matches!(name, "store_reserve" | "store_at" | "store_at_mut") {
                    Type::I64
                } else {
                    (*element).clone()
                };
                let term = self.expr_expected(&arguments[1], Some(&wanted), depth + 1, false)?;
                self.bind_term(&term, &wanted, arguments[1].span)?;
            }
            return self.known_term(&dynamic_store::result_type(name, &element), span);
        }
        if let Some(name) = tasks::primitive(callee) {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            self.budget.work(12, span)?;
            if matches!(name, "task_create" | "task_create_owned" | "task_await") {
                let Some(worker) = arguments.first() else {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "task_create requires a static worker",
                    ));
                };
                let Term::Function(signature) = self.expr(worker, depth + 1, false)? else {
                    return Err(error(
                        "E-TASK-WORKER",
                        span,
                        "task_create worker must be static",
                    ));
                };
                if arguments.len() != signature.parameters.len() + 1 {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "task_create actual input count mismatch",
                    ));
                }
                if name == "task_create_owned" && !signature.callable.is_empty() {
                    let mut inputs = vec![];
                    for (index, (argument, wanted)) in
                        arguments[1..].iter().zip(&signature.parameters).enumerate()
                    {
                        self.budget
                            .work(signature.callable.len().saturating_add(1), argument.span)?;
                        if registration::environment(wanted).is_none()
                            && let Some(bound) = signature
                                .callable
                                .iter()
                                .find(|role| role.index as usize == index)
                        {
                            let id =
                                self.bind_callable_argument(argument, wanted, bound, depth + 1)?;
                            inputs.push(match helpers::parameter_mode(wanted, &bound.generic_name) {
                                Some(Some(mutable)) => Term::ClosureAddress(id, mutable),
                                Some(None) => Term::Closure(id),
                                None => return Err(unsupported(argument.span,
                                    "Task failure input requires its actual callable carrier mode")),
                            });
                            continue;
                        }
                        let dynamic = signature
                            .callable
                            .iter()
                            .any(|role| role.index as usize == index)
                            && registration::environment(wanted).is_some();
                        let term = self.expr_expected(
                            argument,
                            if dynamic { None } else { Some(wanted) },
                            depth + 1,
                            false,
                        )?;
                        if !dynamic {
                            self.bind_term(&term, wanted, argument.span)?;
                        } else if !matches!(term, Term::Registration(_, true, true)) {
                            return Err(error(
                                "E-TYPE-MISMATCH",
                                argument.span,
                                "Task worker requires a complete actual owning registration input",
                            ));
                        }
                        inputs.push(term);
                    }
                    return self.pending_task(signature.clone(), expression.id, inputs, true, span);
                }
                let mut pending_inputs = Vec::new();
                for (index, (argument, wanted)) in
                    arguments[1..].iter().zip(&signature.parameters).enumerate()
                {
                    if !signature.callable.is_empty() {
                        self.budget
                            .work(signature.callable.len().saturating_add(1), argument.span)?;
                        if let Some(bound) = signature
                            .callable
                            .iter()
                            .find(|bound| bound.index as usize == index)
                        {
                            let id =
                                self.bind_callable_argument(argument, wanted, bound, depth + 1)?;
                            pending_inputs.push(
                                match helpers::parameter_mode(wanted, &bound.generic_name) {
                                    Some(Some(mutable)) => Term::ClosureAddress(id, mutable),
                                    Some(None) => Term::Closure(id),
                                    None => {
                                        return Err(unsupported(
                                            argument.span,
                                            "Task input lost its declared callable mode",
                                        ));
                                    }
                                },
                            );
                            continue;
                        }
                    }
                    let term = self.expr_expected(argument, Some(wanted), depth + 1, false)?;
                    self.bind_term(&term, wanted, argument.span)?;
                    pending_inputs.push(term);
                }
                validate_type(&signature.result, 0, span, self.budget)?;
                if name == "task_create_owned" {
                    for parameter in &signature.parameters {
                        charge_type_snapshot(
                            parameter,
                            span,
                            self.budget.limits,
                            &mut self.budget.work,
                            6,
                        )?;
                    }
                }
                charge_type_snapshot(
                    &signature.result,
                    span,
                    self.budget.limits,
                    &mut self.budget.work,
                    4,
                )?;
                if name == "task_await" {
                    return self.known_term(&signature.result, span);
                }
                return self.pending_task(
                    signature.clone(),
                    expression.id,
                    pending_inputs,
                    name == "task_create_owned",
                    span,
                );
            }
            let offset = usize::from(matches!(name, "task_start" | "task_await"));
            let parameters = tasks::parameters(name);
            if arguments.len() != parameters.len() + offset {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "task primitive argument count mismatch",
                ));
            }
            if offset != 0 {
                let Term::Function(signature) = self.expr(&arguments[0], depth + 1, false)? else {
                    return Err(error(
                        "E-TASK-WORKER",
                        span,
                        "task worker must be a static function",
                    ));
                };
                if !signature.scalar_thread_worker()
                    || signature.parameters.as_slice() != [Type::I32]
                {
                    return Err(error(
                        "E-TASK-WORKER",
                        span,
                        "task worker requires fn(i32)->i32",
                    ));
                }
            }
            let mut returned = tasks::result(name);
            for (argument, wanted) in arguments[offset..].iter().zip(&parameters) {
                let term = self.expr_expected(argument, None, depth + 1, false)?;
                if let Some(result) = self.pending_task_operation(name, &term, argument.span)? {
                    return Ok(result);
                }
                let v = self.projection_base(&term, argument.span)?;
                if let Some(actual) = self.graph.ty(v)?
                    && let Some(task @ Type::Task { .. }) = tasks::payload(&actual)
                {
                    charge_type_snapshot(task, span, self.budget.limits, &mut self.budget.work, 6)?;
                    let parameter = tasks::parameter_for(name, task);
                    self.bind_term(&term, &parameter, argument.span)?;
                    returned = tasks::result_for(name, task);
                } else {
                    self.bind_term(&term, wanted, argument.span)?;
                }
            }
            return self.known_term(&returned, span);
        }
        if let Some(name) = byte_buffer::primitive(callee) {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let parameters = byte_buffer::parameters(name);
            if parameters.len() != arguments.len() {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "byte storage primitive argument count mismatch",
                ));
            }
            for (argument, ty) in arguments.iter().zip(&parameters) {
                let term = self.expr_expected(argument, Some(ty), depth + 1, false)?;
                self.bind_term(&term, ty, argument.span)?;
            }
            return self.known_term(
                &byte_buffer::result_type(name).expect("byte primitive"),
                span,
            );
        }
        if let CoreExprKind::Name(name) = &callee.kind
            && generic_mutex::primitive(name)
        {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let [argument] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "mutex operation requires one argument",
                ));
            };
            let term = self.expr_expected(argument, None, depth + 1, false)?;
            let variable = self.projection_base(&term, argument.span)?;
            if name == "mutex_create"
                && matches!(argument.kind, CoreExprKind::Integer(..))
                && self.graph.ty(variable)?.is_none()
            {
                self.graph.bind(variable, &Type::I32, argument.span)?;
            }
            let actual = self
                .graph
                .ty(variable)?
                .ok_or_else(|| unsupported(span, "mutex operand type is unresolved"))?;
            let result = match name.as_str() {
                "mutex_create" => {
                    let payload = if matches!(term, Term::Resource(_, true)) {
                        Type::Managed(Box::new(actual))
                    } else {
                        actual
                    };
                    Type::ErrorUnion {
                        error: Box::new(generic_mutex::create_error()),
                        result: Box::new(Type::Managed(Box::new(Type::Mutex {
                            payload: Box::new(payload),
                        }))),
                    }
                }
                "mutex_lock" => {
                    let head = match &actual {
                        Type::Desc(inner) | Type::Managed(inner) => inner.as_ref(),
                        other => other,
                    };
                    let Type::Mutex { payload } = head else {
                        return Err(error("E-MUTEX-HEAD", span, "mutex lock requires head"));
                    };
                    generic_mutex::guard(payload)
                }
                "mutex_value" => {
                    let head = match &actual {
                        Type::Address {
                            mutable: true,
                            inner,
                        } => inner.as_ref(),
                        other if matches!(term, Term::Address(_, true)) => other,
                        _ => {
                            return Err(error(
                                "E-MUTEX-GUARD",
                                span,
                                "mutex value requires mutable guard",
                            ));
                        }
                    };
                    let Type::MutexGuard { payload, .. } = head else {
                        return Err(error("E-MUTEX-GUARD", span, "mutex value requires guard"));
                    };
                    Type::Address {
                        mutable: true,
                        inner: payload.clone(),
                    }
                }
                "mutex_unlock" => Type::Unit,
                _ => unreachable!(),
            };
            return self.known_term(&result, span);
        }
        if matches!(&callee.kind,CoreExprKind::Name(name) if name=="atomic_compare_exchange") {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            if arguments.len() != 3 {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "atomic_compare_exchange requires three arguments",
                ));
            }
            let address = scoped_thread::sync_address(SyncCellKind::AtomicI32);
            for (index, argument) in arguments.iter().enumerate() {
                let wanted = if index == 0 { &address } else { &Type::I32 };
                let term = self.expr_expected(argument, Some(wanted), depth + 1, false)?;
                self.bind_term(&term, wanted, argument.span)?;
            }
            return self.known_term(&Type::Tuple(vec![Type::I32, Type::Bool]), span);
        }
        if let CoreExprKind::Name(name) = &callee.kind
            && let Some(kind) = scoped_thread::wide_compare_exchange_kind(name)
        {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            if arguments.len() != 3 {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "wide atomic compare_exchange requires three arguments",
                ));
            }
            let address = scoped_thread::sync_address(kind);
            let integer = scoped_thread::integer_type(kind);
            for (index, argument) in arguments.iter().enumerate() {
                let wanted = if index == 0 { &address } else { &integer };
                let term = self.expr_expected(argument, Some(wanted), depth + 1, false)?;
                self.bind_term(&term, wanted, argument.span)?;
            }
            return self.known_term(&Type::Tuple(vec![integer, Type::Bool]), span);
        }
        if let CoreExprKind::Name(name) = &callee.kind
            && let Some((kind, op)) = scoped_thread::wide_primitive(name)
        {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let address = scoped_thread::sync_address(kind);
            let integer = scoped_thread::integer_type(kind);
            let result = if let Some(op) = op {
                let arity = if op == SyncIntegerOp::Load { 1 } else { 2 };
                if arguments.len() != arity {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "incorrect wide atomic operation arity",
                    ));
                }
                for (index, argument) in arguments.iter().enumerate() {
                    let wanted = if index == 0 { &address } else { &integer };
                    let term = self.expr_expected(argument, Some(wanted), depth + 1, false)?;
                    self.bind_term(&term, wanted, argument.span)?;
                }
                if op == SyncIntegerOp::Store {
                    Type::Unit
                } else {
                    integer
                }
            } else {
                let [initial, body] = arguments.as_slice() else {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "wide atomic scope requires initial integer and body",
                    ));
                };
                let term = self.expr_expected(initial, Some(&integer), depth + 1, false)?;
                self.bind_term(&term, &integer, initial.span)?;
                let Term::Closure(id) = self.expr(body, depth + 1, false)? else {
                    return Err(error(
                        "E-SYNC-BODY",
                        body.span,
                        "wide atomic scope requires concrete closure",
                    ));
                };
                if self.closures[id].parameters.len() != 1 {
                    return Err(error(
                        "E-SYNC-BODY",
                        body.span,
                        "wide atomic scope body takes one sealed cell address",
                    ));
                }
                self.graph
                    .bind(self.closures[id].parameters[0], &address, body.span)?;
                self.graph
                    .bind(self.closures[id].result, &Type::I32, body.span)?;
                Type::Tuple(vec![Type::Bool, Type::I32])
            };
            return self.known_term(&result, span);
        }
        if let CoreExprKind::Name(name) = &callee.kind
            && let Some((kind, op)) = scoped_thread::sync_primitive(name)
        {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let address_ty = scoped_thread::sync_address(kind);
            let result = if let Some(op) = op {
                let arity = if op == SyncI32Op::Load { 1 } else { 2 };
                if arguments.len() != arity {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "incorrect synchronization operation arity",
                    ));
                }
                for (index, argument) in arguments.iter().enumerate() {
                    let ty = if index == 0 { &address_ty } else { &Type::I32 };
                    let term = self.expr_expected(argument, Some(ty), depth + 1, false)?;
                    self.bind_term(&term, ty, argument.span)?;
                }
                if op == SyncI32Op::Store {
                    Type::Unit
                } else {
                    Type::I32
                }
            } else {
                let [initial, body] = arguments.as_slice() else {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "cell scope requires initial i32 and body",
                    ));
                };
                let term = self.expr_expected(initial, Some(&Type::I32), depth + 1, false)?;
                self.bind_term(&term, &Type::I32, initial.span)?;
                let Term::Closure(id) = self.expr(body, depth + 1, false)? else {
                    return Err(error(
                        "E-SYNC-BODY",
                        body.span,
                        "cell scope requires a concrete closure",
                    ));
                };
                if self.closures[id].parameters.len() != 1 {
                    return Err(error(
                        "E-SYNC-BODY",
                        body.span,
                        "cell scope body takes one sealed cell address",
                    ));
                }
                self.graph
                    .bind(self.closures[id].parameters[0], &address_ty, body.span)?;
                self.graph
                    .bind(self.closures[id].result, &Type::I32, body.span)?;
                Type::Tuple(vec![Type::Bool, Type::I32])
            };
            return self.known_term(&result, span);
        }
        if matches!(&callee.kind,CoreExprKind::Name(name) if name=="thread_scope") {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let [worker, argument, body] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "thread_scope requires three arguments",
                ));
            };
            if !matches!(
                worker.kind,
                CoreExprKind::Name(_) | CoreExprKind::Member { .. }
            ) {
                return Err(error(
                    "E-THREAD-WORKER",
                    worker.span,
                    "thread worker must be a static ordinary symbol",
                ));
            }
            let Term::Function(signature) = self.expr(worker, depth + 1, false)? else {
                return Err(error(
                    "E-THREAD-WORKER",
                    worker.span,
                    "thread worker cannot be a stored callable",
                ));
            };
            if !signature.scalar_thread_worker() {
                return Err(error(
                    "E-THREAD-WORKER",
                    worker.span,
                    "thread worker must be a concrete ordinary fn(i32)->i32 or fn(&i32)->i32",
                ));
            }
            let payload_type = &signature.parameters[0];
            let argument_term =
                self.expr_expected(argument, Some(payload_type), depth + 1, false)?;
            self.bind_term(&argument_term, payload_type, argument.span)?;
            let Term::Closure(id) = self.expr(body, depth + 1, false)? else {
                return Err(error(
                    "E-THREAD-BODY",
                    body.span,
                    "thread body must be a concrete closure",
                ));
            };
            if !self.closures[id].parameters.is_empty() {
                return Err(error(
                    "E-THREAD-BODY",
                    body.span,
                    "thread body signature is ()->i32",
                ));
            }
            self.graph
                .bind(self.closures[id].result, &Type::I32, body.span)?;
            return self.known_term(&scoped_thread::scope_result(payload_type), span);
        }
        if let Some(name) = registration::primitive(callee) {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let asynchronous =
                name.starts_with("async_registration_") || name == "c_register_async";
            if matches!(name, "c_register" | "c_register_async") {
                let (providers, callback): (Vec<_>, _) = match arguments.as_slice() {
                    [register, close, callback] if !asynchronous => (
                        vec![
                            (register, ScopedCallbackRole::Register),
                            (close, ScopedCallbackRole::Close),
                        ],
                        callback,
                    ),
                    [register, close, drain, callback] if asynchronous => (
                        vec![
                            (register, ScopedCallbackRole::AsyncRegister),
                            (close, ScopedCallbackRole::AsyncRequestClose),
                            (drain, ScopedCallbackRole::AsyncPollDrain),
                        ],
                        callback,
                    ),
                    _ => {
                        return Err(error(
                            "E-CALL-ARITY",
                            span,
                            "registration constructor requires ordered actual providers and F",
                        ));
                    }
                };
                let mut threaded = false;
                for (index, (expression, role)) in providers.into_iter().enumerate() {
                    let Term::Function(signature) = self.expr(expression, depth + 1, false)? else {
                        return Err(error(
                            "E-FFI-REGISTRATION",
                            span,
                            "registration role requires static foreign function",
                        ));
                    };
                    if index == 0 && asynchronous {
                        threaded = signature.scoped_callback
                            == Some(ScopedCallbackRole::ThreadAsyncRegister);
                    }
                    let role = if threaded {
                        match role {
                            ScopedCallbackRole::AsyncRegister => {
                                ScopedCallbackRole::ThreadAsyncRegister
                            }
                            ScopedCallbackRole::AsyncRequestClose => {
                                ScopedCallbackRole::ThreadAsyncRequestClose
                            }
                            ScopedCallbackRole::AsyncPollDrain => {
                                ScopedCallbackRole::ThreadAsyncPollDrain
                            }
                            other => other,
                        }
                    } else {
                        role
                    };
                    if signature.scoped_callback != Some(role) {
                        return Err(error(
                            "E-FFI-REGISTRATION",
                            span,
                            "registration role differs from actual declaration",
                        ));
                    }
                }
                let previous = self.callable_hint.replace((vec![Type::I32], Type::I32));
                let source = self.expr(callback, depth + 1, false)?;
                self.callable_hint = previous;
                let Term::Closure(id) = source else {
                    return Err(error(
                        "E-REGISTRATION-CALLBACK",
                        span,
                        "registration requires concrete F",
                    ));
                };
                if self.closures[id].parameters.len() != 1 {
                    return Err(error(
                        "E-REGISTRATION-CALLBACK",
                        span,
                        "callback signature is (i32)->i32",
                    ));
                }
                self.graph
                    .bind(self.closures[id].parameters[0], &Type::I32, span)?;
                self.graph
                    .bind(self.closures[id].result, &Type::I32, span)?;
                return Ok(Term::Registration(
                    Rc::new(Term::Closure(id)),
                    true,
                    asynchronous,
                ));
            }
            let [argument] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "registration operation requires one carrier",
                ));
            };
            if asynchronous
                && matches!(
                    name,
                    "async_registration_request_close" | "async_registration_poll_close"
                )
            {
                if let CoreExprKind::Name(local) = &argument.kind
                    && self.scopes.iter().rev().find_map(|s| s.get(local)).is_some_and(|b|
                        matches!(&b.term, Term::ValueAddress(carrier, true) if matches!(carrier.as_ref(), Term::Registration(_, true, true))))
                {
                    self.expr(argument, depth + 1, false)?;
                    let returned = if name == "async_registration_poll_close" { tasks::poll_type(&Type::Unit) } else { Type::Unit };
                    return self.known_term(&returned, span);
                }
                let CoreExprKind::Unary {
                    op: UnaryOp::BorrowMut,
                    operand,
                } = &argument.kind
                else {
                    return Err(error(
                        "E-REGISTRATION-CARRIER",
                        span,
                        "async close requires explicit exclusive owning head address",
                    ));
                };
                let CoreExprKind::Name(local) = &operand.kind else {
                    return Err(error(
                        "E-REGISTRATION-CARRIER",
                        span,
                        "async close requires actual complete local owning head",
                    ));
                };
                self.budget
                    .work(local.len().saturating_add(self.scopes.len()), span)?;
                if !self
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|s| s.get(local))
                    .is_some_and(|b| {
                        b.mutable && matches!(b.term, Term::Registration(_, true, true))
                    })
                {
                    return Err(error(
                        "E-REGISTRATION-CARRIER",
                        span,
                        "async close requires mutable complete local owner",
                    ));
                }
                self.budget.node(depth + 1, argument.span)?;
                self.expression_index += 1;
                self.expr(operand, depth + 2, false)?;
                let returned = if name == "async_registration_poll_close" {
                    tasks::poll_type(&Type::Unit)
                } else {
                    Type::Unit
                };
                return self.known_term(&returned, span);
            }
            let actual = self.expr(argument, depth + 1, false)?;
            if !matches!(actual,Term::Registration(_,owned,a) if a == asynchronous && (matches!(name,"registration_valid"|"async_registration_valid") || owned))
            {
                return Err(error(
                    "E-REGISTRATION-CARRIER",
                    span,
                    "registration close requires actual owner, valid requires descriptor",
                ));
            }
            return self.known_term(
                if matches!(name, "registration_valid" | "async_registration_valid") {
                    &Type::Bool
                } else {
                    &Type::Unit
                },
                span,
            );
        }
        if matches!(&callee.kind,CoreExprKind::Name(name) if name=="c_with_callback") {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let [register, close, callback, body] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "c_with_callback requires four arguments",
                ));
            };
            for (expression, role) in [
                (register, ScopedCallbackRole::Register),
                (close, ScopedCallbackRole::Close),
            ] {
                let Term::Function(signature) = self.expr(expression, depth + 1, false)? else {
                    return Err(error(
                        "E-FFI-SCOPED-ROLE",
                        expression.span,
                        "scoped callback role requires an actual static foreign symbol",
                    ));
                };
                if signature.scoped_callback != Some(role) {
                    return Err(error(
                        "E-FFI-SCOPED-ROLE",
                        expression.span,
                        "scoped callback role differs from its actual declaration",
                    ));
                }
            }
            let Term::ClosureAddress(callback_id, true) = self.expr(callback, depth + 1, false)?
            else {
                return Err(error(
                    "E-FFI-SCOPED-CALLBACK",
                    callback.span,
                    "scoped callback requires an exclusive concrete closure address",
                ));
            };
            if self.closures[callback_id].parameters.len() != 1 {
                return Err(error(
                    "E-FFI-SCOPED-CALLBACK",
                    callback.span,
                    "scoped callback signature is (i32)->i32",
                ));
            }
            self.graph
                .bind(self.closures[callback_id].parameters[0], &Type::I32, span)?;
            self.graph
                .bind(self.closures[callback_id].result, &Type::I32, span)?;
            let Term::Closure(body_id) = self.expr(body, depth + 1, false)? else {
                return Err(error(
                    "E-FFI-SCOPED-BODY",
                    body.span,
                    "scoped body must be a concrete closure",
                ));
            };
            if !self.closures[body_id].parameters.is_empty() {
                return Err(error(
                    "E-FFI-SCOPED-BODY",
                    body.span,
                    "scoped body signature is ()->i32",
                ));
            }
            self.graph
                .bind(self.closures[body_id].result, &Type::I32, span)?;
            return self.known_term(&Type::Tuple(vec![Type::Bool, Type::I32]), span);
        }
        if foreign::callback_constructor(callee) {
            // Keep normalization's expression numbering exact without
            // resolving the intrinsic name as an ordinary function.
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let [target] = arguments.as_slice() else {
                return Err(error(
                    "E-FFI-CALLBACK",
                    span,
                    "c_callback requires one static ordinary function symbol",
                ));
            };
            if !matches!(
                target.kind,
                CoreExprKind::Name(_) | CoreExprKind::Member { .. }
            ) {
                return Err(error(
                    "E-FFI-CALLBACK",
                    target.span,
                    "callback target must be a static symbol",
                ));
            }
            let Term::Function(signature) = self.expr(target, depth + 1, false)? else {
                return Err(error(
                    "E-FFI-CALLBACK",
                    target.span,
                    "stored callables and closures cannot become C callbacks",
                ));
            };
            let ty = signature.foreign_callback_type().ok_or_else(|| {
                error(
                    "E-FFI-CALLBACK",
                    target.span,
                    "C callback target has an unsupported signature or contract",
                )
            })?;
            return self.known_term(&ty, span);
        }
        if let Some((ty, variant)) = self.constructor(callee)? {
            // Normalization still traverses the constructor member and
            // its namespace token; neither is a local value read.
            self.budget.node(depth + 1, callee.span)?;
            self.budget.node(depth + 2, callee.span)?;
            self.expression_index += 2;
            return self.variant(&ty, variant, arguments, depth, span);
        }
        let callee_type = if let CoreExprKind::TypeApply {
            callee: target,
            type_arguments,
        } = &callee.kind
        {
            self.budget.node(depth + 1, callee.span)?;
            self.expression_index += 1;
            let [actual] = type_arguments.as_slice() else {
                return Err(error(
                    "E-TYPE-APPLICATION",
                    callee.span,
                    "one complete type argument is required",
                ));
            };
            validate_type(actual, 0, callee.span, self.budget)?;
            dynamic_store::charge_pod(
                actual,
                self.budget.limits,
                callee.span,
                &mut self.budget.work,
            )?;
            if !(self.graph.is_store_parameter(actual)
                || dynamic_store::supported_element(actual)
                || dynamic_store::callback_element(
                    actual,
                    |ty| matches!(ty, Type::Named(name) if self.helper_environments.contains_key(name)),
                ))
            {
                return Err(error(
                    "E-STORE-TYPE-ARGUMENT",
                    callee.span,
                    "ordinary type applications require the caller's StoreElement binder or a complete finite element type",
                ));
            }
            let Term::Function(signature) = self.expr(target, depth + 2, false)? else {
                return Err(error(
                    "E-TYPE-APPLICATION",
                    callee.span,
                    "ordinary type application requires a static template",
                ));
            };
            let mut signature = context_signature(&signature, self.budget, span)?;
            let bound = signature.type_parameter.take().ok_or_else(|| {
                error(
                    "E-TYPE-APPLICATION",
                    callee.span,
                    "type application target is not an ordinary StoreElement template",
                )
            })?;
            let limits = self.budget.limits;
            let products = self.budget.products.clone();
            for parameter in &mut signature.parameters {
                *parameter = store_templates::substitute_type(
                    parameter,
                    &bound.name,
                    actual,
                    limits,
                    &products,
                    span,
                    &mut |work| self.budget.work(work, span),
                )?;
            }
            signature.result = store_templates::substitute_type(
                &signature.result,
                &bound.name,
                actual,
                limits,
                &products,
                span,
                &mut |work| self.budget.work(work, span),
            )?;
            // This temporary signature is consumed only by this call.
            // Its template has no concrete FunctionId or function value.
            Term::Function(Rc::new(signature))
        } else {
            self.expr(callee, depth + 1, false)?
        };
        let (parameters, result) = match callee_type {
            Term::Method(signature) => {
                let mut state_environment = None;
                if arguments.len() + 1 != signature.parameters.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "method argument count mismatch",
                    ));
                }
                for (index, (argument, ty)) in
                    arguments.iter().zip(&signature.parameters[1..]).enumerate()
                {
                    if let Some(bound) = signature
                        .callable
                        .iter()
                        .find(|bound| bound.index as usize == index + 1)
                    {
                        state_environment =
                            Some(self.bind_callable_argument(argument, ty, bound, depth + 1)?);
                    } else {
                        let value = self.expr_expected(argument, Some(ty), depth + 1, false)?;
                        self.bind_term(&value, ty, argument.span)?;
                    }
                }
                if registration::environment(&signature.result).is_some()
                    && let Some(environment) = state_environment
                {
                    return Ok(Term::Registration(
                        Rc::new(Term::Closure(environment)),
                        matches!(signature.result, Type::Managed(_)),
                        registration::asynchronous(&signature.result),
                    ));
                }
                if forwarding::signature(&signature).is_some() {
                    return Ok(Term::Closure(
                        state_environment
                            .ok_or_else(|| unsupported(span, "forward source absent"))?,
                    ));
                }
                if let (Some(environment), Some(bound)) =
                    (state_environment, signature.callable.first())
                    && callable_fields::placeholder_package(&signature.result, &bound.generic_name)
                {
                    return self.state_term(
                        &signature.result,
                        &bound.generic_name,
                        environment,
                        span,
                    );
                }
                return self.known_result(&signature, span);
            }
            Term::SliceLength => {
                if !arguments.is_empty() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "slice len requires no arguments",
                    ));
                }
                return self.known_term(&Type::I64, span);
            }
            Term::Function(signature) => {
                if state_origins::replacement_signature(&signature) {
                    return self.replacement_call(&signature, arguments, depth, span);
                }
                if callable_fields::signature(&signature)
                    && signature.callable.len() == 2
                    && signature.callable[0].generic_index != signature.callable[1].generic_index
                {
                    return self.state_call(&signature, arguments, depth, span);
                }
                if signature.type_parameter.is_some() {
                    return self.value_type_call(&signature, arguments, depth, span);
                }
                let mut state_environment = None;
                if arguments.len() != signature.parameters.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "function call argument count mismatch",
                    ));
                }
                for (index, (argument, ty)) in
                    arguments.iter().zip(&signature.parameters).enumerate()
                {
                    if let Some(bound) = signature
                        .callable
                        .iter()
                        .find(|b| b.index as usize == index)
                    {
                        state_environment =
                            Some(self.bind_callable_argument(argument, ty, bound, depth + 1)?);
                    } else {
                        let value = self.expr_expected(argument, Some(ty), depth + 1, false)?;
                        self.bind_term(&value, ty, argument.span)?;
                    }
                }
                if registration::environment(&signature.result).is_some()
                    && let Some(environment) = state_environment
                {
                    return Ok(Term::Registration(
                        Rc::new(Term::Closure(environment)),
                        matches!(signature.result, Type::Managed(_)),
                        registration::asynchronous(&signature.result),
                    ));
                }
                if forwarding::signature(&signature).is_some() {
                    return Ok(Term::Closure(
                        state_environment
                            .ok_or_else(|| unsupported(span, "forward source absent"))?,
                    ));
                }
                if let (Some(environment), Some(bound)) =
                    (state_environment, signature.callable.first())
                    && callable_fields::placeholder_package(&signature.result, &bound.generic_name)
                {
                    return self.state_term(
                        &signature.result,
                        &bound.generic_name,
                        environment,
                        span,
                    );
                }
                return self.known_result(&signature, span);
            }
            Term::ClosureAddress(id, _)
                if self.state_modes.get(&id) == Some(&CallableKind::FnOnce) =>
            {
                return Err(error(
                    "E-CLOSURE-MODE",
                    span,
                    "FnOnce State declaration requires an owned callable value",
                ));
            }
            Term::ClosureAddress(id, false)
                if self.state_modes.get(&id) == Some(&CallableKind::FnMut) =>
            {
                return Err(error(
                    "E-CLOSURE-MODE",
                    span,
                    "FnMut State declaration requires an exclusive callback window",
                ));
            }
            Term::Closure(id) | Term::ClosureAddress(id, _) => {
                self.budget.work(self.closures[id].parameters.len(), span)?;
                (
                    self.closures[id].parameters.clone(),
                    self.closures[id].result,
                )
            }
            Term::Scalar(callee) => {
                let known = self.graph.ty(callee)?;
                if let Some(Type::BorrowedFunctionPointer {
                    parameters, result, ..
                }) = &known
                {
                    if parameters.len() != arguments.len() {
                        return Err(error(
                            "E-CALL-ARITY",
                            span,
                            "indirect call arity differs from its signature",
                        ));
                    }
                    for (argument, parameter) in arguments.iter().zip(parameters) {
                        let term =
                            self.expr_expected(argument, Some(parameter), depth + 1, false)?;
                        self.bind_term(&term, parameter, argument.span)?;
                    }
                    return self.known_term(result, span);
                }
                let hints = match &known {
                    Some(
                        Type::FunctionPointer { parameters, .. }
                        | Type::BorrowedFunctionPointer { parameters, .. },
                    ) => Some(parameters),
                    _ => None,
                };
                let mut parameters = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    let value = self.expr_expected(
                        argument,
                        hints.and_then(|p| p.get(index)),
                        depth + 1,
                        false,
                    )?;
                    parameters.push(self.scalar(&value, argument.span)?);
                }
                let result = self.fresh(VALUE, false, span)?;
                self.graph
                    .function_pointer(callee, parameters, result, span)?;
                return Ok(Term::Scalar(result));
            }
            _ => {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    callee.span,
                    "called value is not callable",
                ));
            }
        };
        if arguments.len() != parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "closure call argument count mismatch",
            ));
        }
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let hint = self.graph.ty(parameter)?;
            let value = self.expr_expected(argument, hint.as_ref(), depth + 1, false)?;
            if let Some(ty @ Type::Address { .. }) = hint.as_ref() {
                self.bind_term(&value, ty, argument.span)?;
            } else if let Term::Address(inner, mutable) = value {
                let wanted = self
                    .graph
                    .address_shape(parameter, argument.span)?
                    .and_then(|(_, mutable)| mutable);
                if wanted == Some(true) && !mutable {
                    return Err(error(
                        "E-ADDRESS-PERMISSION",
                        argument.span,
                        "a shared address cannot grant exclusive access",
                    ));
                }
                self.graph
                    .address(parameter, inner, wanted.or(Some(mutable)), argument.span)?;
            } else {
                self.unify(&value, &Term::Scalar(parameter), argument.span)?;
            }
        }
        if let Some(ty) = self.graph.ty(result)?
            && (matches!(ty, Type::Address { .. })
                || matches!(&ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..})))
        {
            self.known_term(&ty, span)
        } else {
            Ok(Term::Scalar(result))
        }
    }

    #[inline(never)]
    fn expr_member(&mut self, expression: &CoreExpr, depth: usize, write: bool) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &expression.kind
        else {
            unreachable!("expression dispatcher variant");
        };
        if !write && let Some((ty, variant)) = self.constructor(expression)? {
            self.budget.node(depth + 1, object.span)?;
            self.expression_index += 1;
            return self.variant(&ty, variant, &[], depth, span);
        }
        if !write
            && let CoreExprKind::Name(name) = &object.kind
            && !self
                .scopes
                .iter()
                .rev()
                .any(|scope| scope.contains_key(name))
            && let Some(signature) = self.signatures.get(&format!("{name}.{member}"))
        {
            if owner.is_some() {
                return Err(error(
                    "E-MEMBER-OWNER",
                    span,
                    "a constrained member requires a value receiver, not a static namespace",
                ));
            }
            // This object is a static namespace, but still occupies an
            // expression position in the normalization traversal.
            self.budget.node(depth + 1, object.span)?;
            self.expression_index += 1;
            return Ok(Term::Function(Rc::new(context_signature(
                signature,
                self.budget,
                span,
            )?)));
        }
        // Mutating a resource payload does not replace its owning
        // Package slot. Stop place mutability at the explicit Managed
        // field projection; its enclosing shell may be a `let`.
        let resource_field = if write
            && let CoreExprKind::Member {
                object: shell,
                member: field,
                owner: field_owner,
            } = &object.kind
            && let CoreExprKind::Name(name) = &shell.kind
            && let Some(Binding {
                term: Term::Scalar(variable),
                ..
            }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
            && let Some(ty) = self.graph.ty(*variable)?
            && matches!(stored::physical(&ty), Type::Package { .. })
        {
            self.owner_type(&ty, field_owner.as_ref(), object.span)?;
            matches!(
                aggregates::field(&ty, field, object.span)?.1,
                Type::Managed(_)
            )
        } else {
            false
        };
        let object_type = self.projected_base(object, depth + 1, write && !resource_field)?;
        self.owner_term(&object_type, owner.as_ref(), span)?;
        if let Term::CreateFailure(inputs) = &object_type {
            self.budget.work(inputs.len().saturating_add(1), span)?;
            return match member.as_str() {
                "inputs" => Ok(Term::InputTuple(inputs.clone())),
                "reason" => self.known_term(&tasks::create_error(), span),
                _ => Err(error(
                    "E-UNKNOWN-FIELD",
                    span,
                    "TaskCreateFailure field absent",
                )),
            };
        }
        if let Term::InputTuple(inputs) = &object_type {
            self.budget.work(inputs.len().saturating_add(1), span)?;
            let index = member.parse::<usize>().map_err(|_| {
                error(
                    "E-TUPLE-FIELD",
                    span,
                    "original input Tuple requires a numeric field",
                )
            })?;
            return inputs
                .get(index)
                .cloned()
                .ok_or_else(|| error("E-TUPLE-FIELD", span, "original input Tuple field absent"));
        }
        if let Term::ValueAddress(inner, mutable) = &object_type {
            let (shape, field_sources, one) = match inner.as_ref() {
                Term::StatePackage { shape, fields } => (shape, Some(fields), None),
                Term::ClosurePackage { shape, environment } => (shape, None, Some(*environment)),
                _ => {
                    return Err(unsupported(
                        span,
                        "this inferred Store window has no Package field shape",
                    ));
                }
            };
            validate_type(shape, 0, span, self.budget)?;
            let (index, field) = aggregates::field(shape, member, span)?;
            let callback = field_sources
                .and_then(|fields| fields.iter().find(|(i, _)| *i == index).map(|(_, id)| *id))
                .or_else(|| {
                    matches!(&field, Type::Named(n) if n == "$callback")
                        .then_some(one)
                        .flatten()
                });
            if let Some(id) = callback {
                return Ok(Term::ClosureAddress(id, *mutable));
            }
            return self.known_term(&packages::projection_type(field), span);
        }
        if let Term::StatePackage { shape, fields } = &object_type {
            validate_type(shape, 0, span, self.budget)?;
            let (index, field) = aggregates::field(shape, member, span)?;
            if let Some((_, source)) = fields.iter().find(|(field, _)| *field == index) {
                return Ok(Term::Closure(*source));
            }
            return self.known_term(
                &if write {
                    field
                } else {
                    packages::projection_type(field)
                },
                span,
            );
        }
        if let Term::ClosurePackage { shape, environment } = &object_type {
            validate_type(shape, 0, span, self.budget)?;
            let field = aggregates::field(shape, member, span)?.1;
            if matches!(&field,Type::Named(n) if n=="$callback") {
                return Ok(Term::Closure(*environment));
            }
            return self.known_term(
                &if write {
                    field
                } else {
                    packages::projection_type(field)
                },
                span,
            );
        }
        let Term::Module(module) = object_type else {
            let base = self.projection_base(&object_type, span)?;
            if member == "len"
                && matches!(object_type, Term::Address(..))
                && self.graph.is_slice(base)
            {
                return Ok(Term::SliceLength);
            }
            if !write {
                self.graph.solve()?;
                let method_type = self.graph.ty(base)?.map(|ty| match ty {
                    Type::Managed(payload) => *payload,
                    other => other,
                });
                if let Some(Type::OwnedDyn { interface }) = &method_type {
                    self.budget.work(self.dyn_interfaces.len(), span)?;
                    if let Some(definition) =
                        self.dyn_interfaces.iter().find(|d| d.id == *interface)
                    {
                        let mut selected = None;
                        for slot in &definition.slots {
                            self.budget
                                .work(self.dyn_declarations.len().saturating_add(1), span)?;
                            if self
                                .dyn_declarations
                                .iter()
                                .find(|d| d.id == slot.member.application.declaration)
                                .and_then(|d| d.members.get(slot.member.ordinal as usize))
                                .is_some_and(|m| m.name == *member)
                            {
                                if selected.is_some() {
                                    return Err(error(
                                        "E-DYN-SLOT",
                                        span,
                                        "ambiguous dyn method in source inference",
                                    ));
                                }
                                charge_type_snapshot(
                                    &slot.result,
                                    span,
                                    self.budget.limits,
                                    &mut self.budget.work,
                                    1,
                                )?;
                                for ty in &slot.parameters {
                                    charge_type_snapshot(
                                        ty,
                                        span,
                                        self.budget.limits,
                                        &mut self.budget.work,
                                        1,
                                    )?;
                                }
                                selected = Some((
                                    slot.receiver.mutable,
                                    slot.parameters.clone(),
                                    slot.result.clone(),
                                ));
                            }
                        }
                        if let Some((mutable, arguments, result)) = selected {
                            // Private inference only. Final Builder resolves the
                            // exact slot and independently proves owner access.
                            let mut parameters = vec![Type::Address {
                                mutable,
                                inner: Box::new(Type::OwnedDyn {
                                    interface: *interface,
                                }),
                            }];
                            parameters.extend(arguments);
                            return Ok(Term::Method(Rc::new(Signature {
                                retained_return_sources: None,
                                storage_life: None,
                                life: None,
                                id: None,
                                callable: vec![],
                                state_parameter: None,
                                callable_return: None,
                                type_parameter: None,
                                template: None,
                                foreign: false,
                                scoped_callback: None,
                                cleanup: false,
                                parameters,
                                result,
                                result_mode: PassingMode::Infer,
                                public: false,
                                receiver: None,
                            })));
                        }
                    }
                }
                if let Some(
                    ty @ (Type::Record { .. }
                    | Type::Package { .. }
                    | Type::Enum { .. }
                    | Type::ByteBuffer),
                ) = method_type
                {
                    validate_type(&ty, 0, span, self.budget)?;
                    if let Some(signature) =
                        aggregates::resolve_type_method(&ty, member, self.signatures, self.imports)
                    {
                        let signature = context_signature(signature, self.budget, span)?;
                        let receiver = &signature.parameters[0];
                        let value = if receiver.is_shared_resource_head_address()
                            && matches!(&object.kind, CoreExprKind::Name(name)
                                if self.scopes.iter().rev().find_map(|scope|scope.get(name))
                                    .is_some_and(|binding| matches!(binding.term, Term::Resource(_, true))))
                        {
                            self.known_term(receiver, object.span)?
                        } else if let (Type::Address { mutable, .. }, Term::Scalar(variable)) =
                            (receiver, &object_type)
                        {
                            Term::Address(*variable, *mutable)
                        } else {
                            object_type.clone()
                        };
                        if matches!(receiver, Type::Address { mutable: true, .. })
                            && matches!(value, Term::Address(_, false))
                        {
                            return Err(error(
                                "E-ADDRESS-PERMISSION",
                                object.span,
                                "a shared receiver cannot grant exclusive access",
                            ));
                        }
                        self.bind_term(&value, receiver, object.span)?;
                        return Ok(Term::Method(Rc::new(signature)));
                    }
                }
            }
            if let Some(ty @ Type::Package { .. }) = self.graph.ty(base)? {
                let field = aggregates::field(&ty, member, span)?.1;
                let field = if write {
                    field
                } else {
                    packages::projection_type(field)
                };
                return self.known_term(&field, span);
            }
            let result = self.fresh(VALUE, false, span)?;
            self.graph
                .project(base, Projection::Field(member.clone()), result, span)?;
            return Ok(Term::Scalar(result));
        };
        let imports = self.imports.expect("module term has imports");
        let signature = imports.signatures[&module].get(member).ok_or_else(|| {
            error(
                "E-MODULE-MEMBER",
                span,
                format!("module has no function {member}"),
            )
        })?;
        if !signature.public {
            return Err(error(
                "E-MODULE-PRIVATE",
                span,
                format!("function {member} is private"),
            ));
        }
        Ok(Term::Function(Rc::new(context_signature(
            signature,
            self.budget,
            span,
        )?)))
    }

    #[inline(never)]
    fn expr_tuple(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Tuple(items) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let hints = match expected {
            Some(Type::Tuple(fields)) => Some(fields),
            _ => None,
        };
        let mut terms = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let term =
                self.expr_expected(item, hints.and_then(|h| h.get(index)), depth + 1, false)?;
            terms.push(term);
        }
        let mut deferred = false;
        for term in &terms {
            deferred |= self.contains_pending_task(term, depth + 1, span)?;
        }
        if deferred {
            // Carry the real component terms. Reconstructing a Type here would
            // publish a Task context before its worker/capture domain exists.
            return Ok(Term::InputTuple(Rc::new(terms)));
        }
        let mut fields = Vec::new();
        for (item, term) in items.iter().zip(terms) {
            let owned_store_element = match &term {
                Term::ValueStore(element, true) => {
                    let Term::Scalar(element) = element.as_ref() else {
                        return Err(unsupported(
                            item.span,
                            "ordinary Tuple Store fields require a concrete inline element",
                        ));
                    };
                    Some(self.graph.injection_type(*element, item.span)?)
                }
                Term::Resource(variable, true) => match self.graph.ty(*variable)? {
                    Some(Type::OwnedStore { element }) => Some(*element),
                    _ => None,
                },
                _ => None,
            };
            let field = if let Some(element) = owned_store_element {
                // A complete ordinary Tuple may own a concrete or generic
                // Store call result. The Builder still proves the real transfer.
                if !dynamic_store::inline_element(&element)
                    || !dynamic_store::supported_element(&element)
                {
                    return Err(unsupported(
                        item.span,
                        "ordinary Tuple Store fields require a finite scalar or POD Record element",
                    ));
                }
                let ty = Type::Managed(Box::new(Type::OwnedStore {
                    element: Box::new(element),
                }));
                let variable = self.fresh(VALUE, false, item.span)?;
                self.bind(variable, &ty, item.span)?;
                variable
            } else {
                self.scalar(&term, item.span)?
            };
            fields.push(field);
        }
        let result = self.fresh(VALUE, false, span)?;
        self.graph.tuple(result, fields, span)?;
        Ok(Term::Scalar(result))
    }

    fn contains_pending_task(&mut self, term: &Term, depth: usize, span: Span) -> R<bool> {
        self.budget.node(depth, span)?;
        match term {
            Term::PendingTask(..) => Ok(true),
            Term::ValueAddress(inner, _)
            | Term::ValueStore(inner, _)
            | Term::Registration(inner, ..) => self.contains_pending_task(inner, depth + 1, span),
            Term::ValueUnion { error, result } => {
                Ok(self.contains_pending_task(error, depth + 1, span)?
                    | self.contains_pending_task(result, depth + 1, span)?)
            }
            Term::InputTuple(fields) | Term::CreateFailure(fields) => {
                let mut found = false;
                for field in fields.iter() {
                    found |= self.contains_pending_task(field, depth + 1, span)?;
                }
                Ok(found)
            }
            _ => Ok(false),
        }
    }

    #[inline(never)]
    fn expr_array(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Array(items) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let length = u32::try_from(items.len())
            .map_err(|_| Diagnostic::limit(Phase::Types, "array length overflow", Some(span)))?;
        let element = self.fresh(VALUE, false, span)?;
        let hint = match expected {
            Some(Type::Array { element, .. }) => Some(element.as_ref()),
            _ => None,
        };
        let mut callback = None;
        for (index, item) in items.iter().enumerate() {
            let term = self.expr_expected(item, hint, depth + 1, false)?;
            if index == 0
                && let Term::Closure(id) = term
            {
                callback = Some(id);
            }
            if let Some(id) = callback {
                self.unify(&Term::Closure(id), &term, item.span)?;
            } else {
                self.unify(&Term::Scalar(element), &term, item.span)?;
            }
        }
        if let Some(element) = callback {
            return Ok(Term::ClosureArray { element, length });
        }
        let result = self.fresh(VALUE, false, span)?;
        self.graph.array(result, element, length, span)?;
        Ok(Term::Scalar(result))
    }

    #[inline(never)]
    fn expr_record(&mut self, expression: &CoreExpr, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Record { ty, fields } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let payload = match ty {
            Type::Managed(payload) => payload.as_ref(),
            _ => ty,
        };
        for (name, value) in fields {
            let expected = aggregates::field(payload, name, value.span)?.1;
            let actual = self.expr_expected(value, Some(&expected), depth + 1, false)?;
            self.bind_term(&actual, &expected, value.span)?;
        }
        self.known_term(ty, span)
    }

    #[inline(never)]
    fn expr_record_update(&mut self, expression: &CoreExpr, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::RecordUpdate { ty, base, fields } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let base = self.expr(base, depth + 1, false)?;
        if let Some(ty) = ty {
            self.bind_term(&base, ty, span)?;
        }
        for (name, value) in fields {
            let result = self.fresh(VALUE, false, value.span)?;
            let base = self.scalar(&base, span)?;
            self.graph
                .project(base, Projection::Field(name.clone()), result, value.span)?;
            let hint = self.graph.ty(result)?;
            let actual = self.expr_expected(value, hint.as_ref(), depth + 1, false)?;
            self.unify(&Term::Scalar(result), &actual, value.span)?;
        }
        Ok(base)
    }

    #[inline(never)]
    fn expr_index(&mut self, expression: &CoreExpr, depth: usize, write: bool) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Index { object, index } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let base = self.projected_base(object, depth + 1, write)?;
        let index_term = self.expr(index, depth + 1, false)?;
        let index_variable = self.scalar(&index_term, index.span)?;
        self.graph.constrain(index_variable, INTEGER, index.span)?;
        if let Term::ClosureArray { element, .. } = base {
            // Ordinary lowering proves the literal exact location;
            // this layer retains only its source opaque identity.
            return Ok(Term::Closure(element));
        }
        let result = self.fresh(VALUE, false, span)?;
        let base = self.projection_base(&base, span)?;
        self.graph
            .project(base, Projection::Element, result, span)?;
        Ok(Term::Scalar(result))
    }

    #[inline(never)]
    fn expr_slice(&mut self, expression: &CoreExpr, depth: usize, write: bool) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Slice { object, start, end } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let base = self.expr(object, depth + 1, write)?;
        for endpoint in [start, end].into_iter().flatten() {
            let term = self.expr(endpoint, depth + 1, false)?;
            let variable = self.scalar(&term, endpoint.span)?;
            self.graph.constrain(variable, INTEGER, endpoint.span)?;
        }
        let base = self.projection_base(&base, span)?;
        let element = self.fresh(VALUE, false, span)?;
        self.graph
            .project(base, Projection::Element, element, span)?;
        let result = self.fresh(VALUE, false, span)?;
        self.graph.slice(result, element, span)?;
        Ok(Term::Scalar(result))
    }

    #[inline(never)]
    fn expr_sequence(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<Term> {
        let CoreExprKind::Sequence { bindings, value } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        self.scopes.push(BTreeMap::new());
        let mut hints = self.record_hints(value)?;
        for (name, expression) in bindings {
            if hints.is_empty() {
                hints = self.record_hints(value)?;
            }
            let term = self.expr_expected(expression, hints.get(name), depth + 1, false)?;
            self.insert(name, term, false, expression.span)?;
            self.budget
                .work(name.len().saturating_add(1), expression.span)?;
            self.prepared.insert(name.clone());
        }
        let result = self.expr_expected(value, expected, depth + 1, false)?;
        self.scopes.pop();
        Ok(result)
    }

    #[inline(never)]
    fn expr_compile_time(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<Term> {
        let CoreExprKind::CompileTime(inner) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        // Only type relationships are collected. No value is read or
        // computed; the original CT expression still enters verify_ct.
        self.expr_expected(inner, expected, depth + 1, false)
    }

    #[inline(never)]
    fn expr_closure(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
        index: usize,
    ) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::Closure {
            move_capture: _,
            parameters,
            return_type,
            body,
        } = &expression.kind
        else {
            unreachable!("expression dispatcher variant");
        };
        for parameter in parameters {
            if let Some(ty) = &parameter.ty {
                self.graph.annotation_context(ty, 0, parameter.span)?;
            }
        }
        if let Some(ty) = return_type {
            self.graph.annotation_context(ty, 0, span)?;
        }
        if return_type.as_ref().is_some_and(|t| {
            !(t.is_copy_value()
                || lending::supported_result(t)
                || lending::transferred_result(t)
                || matches!(t, Type::Address { inner, .. } if inner.is_address_pointee())
                || closures::owned_signature_head(t))
                && !(parameters.is_empty() && closures::task_result_head(t))
        }) || parameters.iter().any(|p| {
            p.ty.as_ref().is_some_and(|t| {
                !closures::signature_parameter(t) && !closures::owned_signature_head(t)
            })
        }) {
            return Err(unsupported(
                span,
                "closure parameters require a finite Copy type or one Copy/slice address layer; address parameters require a Copy result",
            ));
        }
        let callable_hint = self.callable_hint.take();
        let pointer = match expected {
            Some(
                Type::FunctionPointer { parameters, result }
                | Type::BorrowedFunctionPointer {
                    parameters, result, ..
                },
            ) => Some((parameters, result.as_ref())),
            _ => None,
        };
        let shape = pointer.or_else(|| callable_hint.as_ref().map(|(p, r)| (p, r)));
        if let Some((types, _)) = shape
            && types.len() != parameters.len()
        {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "closure and expected function pointer have different arity",
            ));
        }
        let result = self.fresh(VALUE, false, span)?;
        if let Some(ty) = return_type {
            self.bind(result, ty, span)?;
        }
        if let Some((_, ty)) = shape {
            self.bind(result, ty, span)?;
        }
        let id = self.closures.len();
        self.budget.closure(span)?;
        self.closures.push(Closure {
            parameters: vec![],
            result,
            has_return: false,
            function_pointer: pointer.is_some(),
            span,
        });
        self.closure_expressions.insert(index, id);
        let previous_owner = self.owner.replace(id);
        self.scopes.push(BTreeMap::new());
        for (parameter_index, parameter) in parameters.iter().enumerate() {
            if !matches!(
                parameter.mode,
                PassingMode::Infer | PassingMode::Copy | PassingMode::Move
            ) {
                return Err(unsupported(
                    parameter.span,
                    "non-Value closure parameter mode",
                ));
            }
            let variable = self.fresh(VALUE, false, parameter.span)?;
            if let Some(ty) = &parameter.ty {
                self.bind(variable, ty, parameter.span)?;
            }
            if let Some((types, _)) = shape {
                self.bind(variable, &types[parameter_index], parameter.span)?;
            }
            self.closures[id].parameters.push(variable);
            let term = if let Some(ty @ Type::Address { .. }) = self.graph.ty(variable)? {
                let term = self.known_term(&ty, parameter.span)?;
                if let Term::Address(inner, mutable) = term {
                    self.graph
                        .address(variable, inner, Some(mutable), parameter.span)?;
                }
                term
            } else {
                Term::Scalar(variable)
            };
            self.insert(&parameter.name, term, false, parameter.span)?;
        }
        self.block(body, depth + 1)?;
        if !self.closures[id].has_return {
            self.bind(result, &Type::Unit, span)?;
        }
        self.scopes.pop();
        self.owner = previous_owner;
        if pointer.is_some() {
            self.known_term(expected.expect("function pointer context"), span)
        } else {
            Ok(Term::Closure(id))
        }
    }

    #[inline(never)]
    fn expr_if(&mut self, expression: &CoreExpr, expected: Option<&Type>, depth: usize) -> R<Term> {
        let span = expression.span;
        let CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } = &expression.kind
        else {
            unreachable!("expression dispatcher variant");
        };
        let condition_type = self.expr(condition, depth + 1, false)?;
        self.bind_term(&condition_type, &Type::Bool, condition.span)?;
        let then_type = self.expr_expected(then_value, expected, depth + 1, false)?;
        let else_type = self.expr_expected(else_value, expected, depth + 1, false)?;
        self.unify(&then_type, &else_type, span)?;
        Ok(then_type)
    }

    fn resolved(&self, variable: Variable, span: Span) -> R<Type> {
        self.graph.ty(variable)?.ok_or_else(|| error("E-CLOSURE-INFERENCE", span, "closure parameter or result type is not uniquely constrained; add a type annotation or a uniquely typed use"))
    }
    fn finish(mut self, function: &CoreFunction) -> R<CoreFunction> {
        self.budget.work(self.graph.len(), function.span)?;
        self.graph.solve()?;
        self.graph.default_literals();
        self.graph.solve()?;
        for closure in &self.closures {
            self.resolved(closure.result, closure.span)?;
            for parameter in &closure.parameters {
                self.resolved(*parameter, closure.span)?;
            }
        }
        let normalization_work = self
            .expression_index
            .checked_add(self.statement_index)
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "source normalization work overflow",
                    Some(function.span),
                )
            })?;
        self.budget.work(normalization_work, function.span)?;
        let mut result =
            snapshot_context_function(function, self.budget.limits, &mut self.budget.work)?;
        let mut expression = 0;
        let mut statement = 0;
        self.normalize_block(&mut result.body, &mut expression, &mut statement)?;
        debug_assert_eq!(expression, self.expression_index);
        debug_assert_eq!(statement, self.statement_index);
        Ok(result)
    }
    fn normalize_block(
        &self,
        block: &mut CoreBlock,
        expression: &mut usize,
        statement: &mut usize,
    ) -> R<()> {
        for item in &mut block.statements {
            let index = *statement;
            *statement += 1;
            match &mut item.kind {
                CoreStatementKind::TupleBinding {
                    ty, initializer, ..
                } => {
                    self.normalize_expr(initializer, expression, statement)?;
                    if let Some(Term::Scalar(variable)) = self.binding_types.get(&index) {
                        *ty = Some(self.resolved(*variable, item.span)?);
                    }
                }
                CoreStatementKind::Binding {
                    ty, initializer, ..
                } => {
                    if let Some(value) = initializer {
                        self.normalize_expr(value, expression, statement)?;
                    }
                    if let Some(Term::Scalar(variable)) = self.binding_types.get(&index) {
                        *ty = Some(self.resolved(*variable, item.span)?);
                    } else if let Some(Term::Address(variable, mutable)) =
                        self.binding_types.get(&index)
                    {
                        *ty = Some(Type::Address {
                            mutable: *mutable,
                            inner: Box::new(self.resolved(*variable, item.span)?),
                        });
                    } else if let Some(Term::Resource(variable, managed)) =
                        self.binding_types.get(&index)
                    {
                        let payload = Box::new(self.resolved(*variable, item.span)?);
                        *ty = Some(if *managed {
                            Type::Managed(payload)
                        } else {
                            Type::Desc(payload)
                        });
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.normalize_expr(target, expression, statement)?;
                    self.normalize_expr(value, expression, statement)?;
                }
                CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                    self.normalize_expr(value, expression, statement)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.normalize_expr(condition, expression, statement)?;
                    self.normalize_block(then_body, expression, statement)?;
                    if let Some(body) = else_body {
                        self.normalize_block(body, expression, statement)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.normalize_expr(condition, expression, statement)?;
                    self.normalize_block(body, expression, statement)?;
                }
                CoreStatementKind::IfLet {
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.normalize_expr(value, expression, statement)?;
                    self.normalize_block(then_body, expression, statement)?;
                    if let Some(body) = else_body {
                        self.normalize_block(body, expression, statement)?;
                    }
                }
                CoreStatementKind::WhileLet { value, body, .. } => {
                    self.normalize_expr(value, expression, statement)?;
                    self.normalize_block(body, expression, statement)?;
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    self.normalize_expr(start, expression, statement)?;
                    self.normalize_expr(end, expression, statement)?;
                    self.normalize_block(body, expression, statement)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.normalize_expr(value, expression, statement)?;
                    for arm in arms {
                        self.normalize_block(&mut arm.body, expression, statement)?;
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body)
                | CoreStatementKind::Defer(body) => {
                    self.normalize_block(body, expression, statement)?
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn normalize_expr(
        &self,
        value: &mut CoreExpr,
        expression: &mut usize,
        statement: &mut usize,
    ) -> R<()> {
        let index = *expression;
        *expression += 1;
        match &mut value.kind {
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::NumericConvert { value, .. } => {
                self.normalize_expr(value, expression, statement)?
            }
            CoreExprKind::Float(text) => {
                if !text.ends_with("f32") && !text.ends_with("f64") {
                    let ty = self.resolved(self.literal_types[&index], value.span)?;
                    text.push_str(if ty == Type::F32 { "f32" } else { "f64" });
                }
            }
            CoreExprKind::Integer(text) => {
                if !text.ends_with("u8")
                    && !text.ends_with("u16")
                    && !text.ends_with("u32")
                    && !text.ends_with("i8")
                    && !text.ends_with("i16")
                    && !text.ends_with("isize")
                    && !text.ends_with("i32")
                    && !text.ends_with("i64")
                    && !text.ends_with("u64")
                    && !text.ends_with("usize")
                {
                    let ty = self.resolved(self.literal_types[&index], value.span)?;
                    text.push_str(match ty {
                        Type::U8 => "u8",
                        Type::U32 => "u32",
                        Type::I8 => "i8",
                        Type::I16 => "i16",
                        Type::Isize => "isize",
                        Type::U16 => "u16",
                        Type::I64 => "i64",
                        Type::U64 => "u64",
                        Type::Usize => "usize",
                        _ => "i32",
                    });
                }
            }
            CoreExprKind::MakeDynBorrow {
                source: operand, ..
            }
            | CoreExprKind::MakeDynOwned {
                source: operand, ..
            }
            | CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand) => self.normalize_expr(operand, expression, statement)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.normalize_expr(left, expression, statement)?;
                self.normalize_expr(right, expression, statement)?;
            }
            CoreExprKind::TypeApply { callee, .. } => {
                self.normalize_expr(callee, expression, statement)?
            }
            CoreExprKind::Call { callee, arguments } => {
                self.normalize_expr(callee, expression, statement)?;
                for argument in arguments {
                    self.normalize_expr(argument, expression, statement)?;
                }
            }
            CoreExprKind::Member { object, .. } => {
                self.normalize_expr(object, expression, statement)?
            }
            CoreExprKind::Tuple(items) | CoreExprKind::Array(items) => {
                for item in items {
                    self.normalize_expr(item, expression, statement)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.normalize_expr(object, expression, statement)?;
                self.normalize_expr(index, expression, statement)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.normalize_expr(object, expression, statement)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.normalize_expr(endpoint, expression, statement)?;
                }
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.normalize_expr(value, expression, statement)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.normalize_expr(base, expression, statement)?;
                for (_, value) in fields {
                    self.normalize_expr(value, expression, statement)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, binding) in bindings {
                    self.normalize_expr(binding, expression, statement)?;
                }
                self.normalize_expr(value, expression, statement)?;
            }
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
            } => {
                let closure = &self.closures[self.closure_expressions[&index]];
                for (parameter, variable) in parameters.iter_mut().zip(&closure.parameters) {
                    parameter.ty = Some(self.resolved(*variable, parameter.span)?);
                }
                *return_type = Some(self.resolved(closure.result, closure.span)?);
                self.normalize_block(body, expression, statement)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.normalize_expr(condition, expression, statement)?;
                self.normalize_expr(then_value, expression, statement)?;
                self.normalize_expr(else_value, expression, statement)?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Dyn inventory is consumed by the bounded admission pass after concrete
/// signature resolution. An orphan raw table cannot enter inference.
pub(super) fn unconsumed_inventory(_core: &CoreModule) -> R<()> {
    // A project may place the trait declaration, interface and committed
    // provider table in different modules. Admission needs the complete
    // project graph and runs after name/type resolution.
    Ok(())
}
