//! A bounded source constraint pass, before CT evaluates any integer. Its output
//! is still raw Core: neither capture legality nor executable safety is granted.
use super::closures::{
    BOOL, Graph, I32, I64, INTEGER, Projection, SCALAR, U8, U16, UNIT, VALUE, Variable,
};
use super::*;
use std::collections::BTreeSet;
use std::rc::Rc;

struct Budget<'a> {
    limits: &'a Limits,
    work: usize,
    nodes: usize,
    ordinary_functions: usize,
    closures: usize,
}
impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            work: limits.max_analysis_steps,
            nodes: limits.max_nodes,
            ordinary_functions: 0,
            closures: 0,
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
pub(super) fn helper_snapshot_work(function: &CoreFunction, limits: &Limits) -> R<usize> {
    let mut budget = Budget::new(limits);
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
    if let Some(bound) = &function.callable_parameter {
        callable_snapshot(bound, function.span, &mut budget)?;
    }
    if let Some(bound) = &function.callable_return {
        returned_snapshot(bound, function.span, &mut budget)?;
    }
    contains_closure(&function.body, 0, &mut budget)?;
    Ok(limits.max_analysis_steps - budget.work)
}

pub(super) fn signature_snapshot_work(
    name: &str,
    signature: &Signature,
    span: Span,
    limits: &Limits,
) -> R<usize> {
    let mut budget = Budget::new(limits);
    budget.work(name.len().saturating_add(signature.parameters.len()), span)?;
    for ty in &signature.parameters {
        validate_type(ty, 0, span, &mut budget)?;
    }
    validate_type(&signature.result, 0, span, &mut budget)?;
    if let Some(ty) = &signature.receiver {
        validate_type(ty, 0, span, &mut budget)?;
    }
    if let Some(bound) = &signature.callable {
        callable_snapshot(bound, span, &mut budget)?;
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

fn callable_snapshot(bound: &CoreCallableParameter, span: Span, budget: &mut Budget<'_>) -> R<()> {
    budget.work(
        bound
            .generic_name
            .len()
            .saturating_add(bound.parameters.len()),
        span,
    )?;
    for ty in &bound.parameters {
        validate_type(ty, 0, span, budget)?;
    }
    validate_type(&bound.result, 0, span, budget)
}

/// Infer closure-related source types without executing CT or producing a
/// verified artifact. The caller must still run CT, ordinary types and Verify.
pub fn infer_source_closures(core: &CoreModule, limits: &Limits) -> CompileResult<CoreModule> {
    let mut budget = Budget::new(limits);
    budget.ordinary_functions = core.functions.len();
    let table = headers(core, None, 0, &mut budget).map_err(|e| vec![e])?;
    let flags = core
        .functions
        .iter()
        .map(|f| contains_closure(&f.body, 0, &mut budget))
        .collect::<R<Vec<_>>>()
        .map_err(|e| vec![e])?;
    names::check(core, limits).map_err(|e| vec![e])?;
    let normalized = if flags.iter().any(|f| *f) {
        Some(aggregates::resolve(core.clone(), limits)?)
    } else {
        None
    };
    let core = normalized.as_ref().unwrap_or(core);
    let table = if normalized.is_some() {
        headers(core, None, 0, &mut budget).map_err(|e| vec![e])?
    } else {
        table
    };
    validate_module_metadata(core, &mut budget).map_err(|e| vec![e])?;
    let enum_types = enums::definitions(core);
    let mut functions = Vec::new();
    for (function, flag) in core.functions.iter().zip(flags) {
        functions.push(
            prepare(function, &table, None, &enum_types, flag, &mut budget).map_err(|e| vec![e])?,
        );
    }
    Ok(copy_module_metadata(core, functions))
}

/// The project version uses the established ModuleId/alias graph. Generated
/// closure entries are not inserted into the graph or its exported interface.
pub fn infer_program_closures(
    program: &CoreProgram,
    limits: &Limits,
) -> CompileResult<CoreProgram> {
    let mut budget = Budget::new(limits);
    let mut tables = BTreeMap::new();
    let mut count = 0usize;
    if program.modules.len() > limits.max_nodes {
        return Err(vec![Diagnostic::limit(
            Phase::Types,
            "source closure module budget exceeded",
            None,
        )]);
    }
    for module in &program.modules {
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
                .map(|f| contains_closure(&f.body, 0, &mut budget))
                .collect::<R<Vec<_>>>()
                .map_err(|e| vec![e])?,
        );
    }
    names::check_modules(program.modules.iter().map(|m| &m.core), limits).map_err(|e| vec![e])?;
    let normalized = if flags.iter().flatten().any(|f| *f) {
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
        for (function, flag) in module.core.functions.iter().zip(flags) {
            functions.push(
                prepare(
                    function,
                    table,
                    Some(&imports),
                    &enum_types,
                    flag,
                    &mut budget,
                )
                .map_err(|e| vec![e])?,
            );
        }
        modules.push(ProgramModule {
            core: copy_module_metadata(&module.core, functions),
            imports: module.imports.clone(),
            exports: module.exports.clone(),
        });
    }
    Ok(CoreProgram {
        root: program.root,
        source_fingerprint: program.source_fingerprint,
        modules,
    })
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
        if let Some(bound) = &function.callable_parameter {
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
        // These raw headers provide context only. Formal signature/resource
        // checks run afterwards and remain the authority for admission.
        table.insert(
            function.name.clone(),
            Signature {
                callable: function.callable_parameter.clone(),
                callable_return: function.callable_return.clone(),
                template: function
                    .callable_parameter
                    .as_ref()
                    .map(|_| (core.module_id, function.id)),
                foreign: function.foreign.is_some(),
                cleanup: core
                    .aggregates
                    .iter()
                    .any(|a| a.teardown.as_ref() == Some(&function.name)),
                id: FunctionId(id),
                parameters: function.parameters.iter().map(|p| p.ty.clone()).collect(),
                result: function.return_type.clone(),
                public: exports.is_none_or(|e| e.contains(&function.id)),
                receiver: function.receiver.as_ref().map(|r| r.ty.clone()),
            },
        );
    }
    Ok(table)
}

fn prepare(
    function: &CoreFunction,
    signatures: &BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'_>>,
    enums: &BTreeMap<String, Type>,
    has_closure: bool,
    budget: &mut Budget<'_>,
) -> R<CoreFunction> {
    // Scalar-only functions retain their existing inference/default behavior.
    if !has_closure {
        let dependencies = retained::dependencies(
            function,
            signatures,
            imports,
            &mut budget.work,
            budget.limits,
        )?;
        if dependencies.is_empty() {
            return Ok(function.clone());
        }
    }
    let mut solver = Solver::new(function, signatures, imports, enums, budget)?;
    solver.block(&function.body, 0)?;
    solver.finish(function)
}

fn copy_module_metadata(core: &CoreModule, functions: Vec<CoreFunction>) -> CoreModule {
    CoreModule {
        schema_version: core.schema_version,
        module_id: core.module_id,
        source_fingerprint: core.source_fingerprint,
        functions,
        aggregates: core.aggregates.clone(),
        enums: core.enums.clone(),
        unsupported: core.unsupported.clone(),
    }
}
fn validate_module_metadata(core: &CoreModule, budget: &mut Budget<'_>) -> R<()> {
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
    match ty {
        Type::Array { element, length } => {
            budget.work(*length as usize, span)?;
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
        Type::Address { inner, .. }
        | Type::Slice { element: inner }
        | Type::OwnedStore { element: inner }
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
        Type::Named(name) | Type::Unsupported(name) => budget.work(name.len(), span)?,
        _ => {}
    }
    Ok(())
}
fn validate_pattern(pattern: &CorePattern, depth: usize, budget: &mut Budget<'_>) -> R<()> {
    budget.node(depth, pattern.span)?;
    match &pattern.kind {
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
        CorePatternKind::Tuple(fields) => {
            for field in fields {
                validate_pattern(field, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn returned_snapshot(bound: &CoreCallableReturn, span: Span, budget: &mut Budget<'_>) -> R<()> {
    budget.work(bound.parameters.len().saturating_add(1), span)?;
    for ty in &bound.parameters {
        validate_type(ty, 0, span, budget)?;
    }
    validate_type(&bound.result, 0, span, budget)
}

fn context_signature(signature: &Signature, budget: &mut Budget<'_>, span: Span) -> R<Signature> {
    // Repeated uses must reserve the complete shape before cloning a signature.
    // Reuse the existing finite ordinary ABI shapes, including owned error
    // alternatives. This does not grant a closure capture or callable ABI.
    budget.work(signature.parameters.len(), span)?;
    let admitted = |ty: &Type| {
        ty.is_copy_value()
            || ty.is_scalar_or_address()
            || resources::supported(ty)
            || errors::owned(ty)
            || packages::supported(ty)
            || packages::owned_array_supported(ty)
    };
    if (!admitted(&signature.result)
        && signature.callable_return.is_none()
        && !callable_fields::signature(signature))
        || signature.parameters.iter().enumerate().any(|(index, ty)| {
            !admitted(ty)
                && !signature
                    .callable
                    .as_ref()
                    .is_some_and(|bound| bound.index as usize == index)
        })
    {
        return Err(unsupported(
            span,
            "unsupported signature in closure inference",
        ));
    }
    validate_type(&signature.result, 0, span, budget)?;
    for parameter in &signature.parameters {
        validate_type(parameter, 0, span, budget)?;
    }
    if let Some(bound) = &signature.callable {
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
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                found |= contains_closure(body, depth + 1, budget)?
            }
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
    let mut found = false;
    match &e.kind {
        CoreExprKind::Literal(Constant::WideString(units)) => budget.work(units.len(), e.span)?,
        CoreExprKind::Ascribed { ty, value }
        | CoreExprKind::IntegerConvert { target: ty, value } => {
            validate_type(ty, depth + 1, e.span, budget)?;
            found |= expression_has_closure(value, depth + 1, budget)?;
        }
        CoreExprKind::Closure {
            parameters,
            return_type,
            body,
            ..
        } => {
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
            found = true;
        }
        CoreExprKind::Unary { operand, .. }
        | CoreExprKind::CompileTime(operand)
        | CoreExprKind::Try(operand) => {
            found |= expression_has_closure(operand, depth + 1, budget)?
        }
        CoreExprKind::Binary { left, right, .. } => {
            found |= expression_has_closure(left, depth + 1, budget)?;
            found |= expression_has_closure(right, depth + 1, budget)?;
        }
        CoreExprKind::Call { callee, arguments } => {
            found |= expression_has_closure(callee, depth + 1, budget)?;
            for argument in arguments {
                found |= expression_has_closure(argument, depth + 1, budget)?;
            }
        }
        CoreExprKind::Index { object, index } => {
            found |= expression_has_closure(object, depth + 1, budget)?;
            found |= expression_has_closure(index, depth + 1, budget)?;
        }
        CoreExprKind::Slice { object, start, end } => {
            found |= expression_has_closure(object, depth + 1, budget)?;
            for endpoint in [start, end].into_iter().flatten() {
                found |= expression_has_closure(endpoint, depth + 1, budget)?;
            }
        }
        CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
            for value in values {
                found |= expression_has_closure(value, depth + 1, budget)?;
            }
        }
        CoreExprKind::Member { object, member } => {
            budget.work(member.len(), e.span)?;
            found |= expression_has_closure(object, depth + 1, budget)?;
        }
        CoreExprKind::Record { ty, fields } => {
            validate_type(ty, depth + 1, e.span, budget)?;
            for (name, value) in fields {
                budget.work(name.len(), value.span)?;
                found |= expression_has_closure(value, depth + 1, budget)?;
            }
        }
        CoreExprKind::RecordUpdate { ty, base, fields } => {
            if let Some(ty) = ty {
                validate_type(ty, depth + 1, e.span, budget)?;
            }
            found |= expression_has_closure(base, depth + 1, budget)?;
            for (name, value) in fields {
                budget.work(name.len(), value.span)?;
                found |= expression_has_closure(value, depth + 1, budget)?;
            }
        }
        CoreExprKind::Sequence { bindings, value } => {
            for (name, binding) in bindings {
                budget.work(name.len(), binding.span)?;
                found |= expression_has_closure(binding, depth + 1, budget)?;
            }
            found |= expression_has_closure(value, depth + 1, budget)?;
        }
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            found |= expression_has_closure(condition, depth + 1, budget)?;
            found |= expression_has_closure(then_value, depth + 1, budget)?;
            found |= expression_has_closure(else_value, depth + 1, budget)?;
        }
        CoreExprKind::Integer(text)
        | CoreExprKind::String(text)
        | CoreExprKind::Name(text)
        | CoreExprKind::ReceiverMember(text)
        | CoreExprKind::Unsupported(text) => budget.work(text.len(), e.span)?,
        _ => {}
    }
    Ok(found)
}

#[derive(Clone)]
enum Term {
    Scalar(Variable),
    Address(Variable, bool),
    /// Only the finite payload participates in the Copy constraint graph.
    /// The bool distinguishes a carrier expression from a descriptor.
    Resource(Variable, bool),
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
    graph: Graph,
    scopes: Vec<BTreeMap<String, Binding>>,
    prepared: BTreeSet<String>,
    closures: Vec<Closure>,
    owner: Option<usize>,
    ordinary_result: Type,
    ordinary_callable_return: Option<CoreCallableReturn>,
    // Source-visible opaque identity belongs to the factory definition, even
    // when its hidden environment is forwarded by another factory.
    factory_terms: BTreeMap<FunctionId, usize>,
    ordinary_return_identity: Option<usize>,
    helper_environment: Option<(String, usize)>,
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
    fn new(
        function: &CoreFunction,
        signatures: &'a BTreeMap<String, Signature>,
        imports: Option<&'a program::Imports<'a>>,
        enums: &'a BTreeMap<String, Type>,
        budget: &'b mut Budget<'c>,
    ) -> R<Self> {
        if !(function.callable_return.is_some()
            || callable_fields::template(function)
            || function.return_type.is_copy_value()
            || function.return_type.is_scalar_or_address()
            || packages::supported(&function.return_type)
            || packages::owned_array_supported(&function.return_type)
            || resources::supported(&function.return_type))
        {
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
            ordinary_callable_return: function.callable_return.clone(),
            factory_terms: BTreeMap::new(),
            ordinary_return_identity: None,
            helper_environment: None,
            signatures,
            imports,
            enums,
            budget,
            expression_index: 0,
            statement_index: 0,
            literal_types: BTreeMap::new(),
            binding_types: BTreeMap::new(),
            closure_expressions: BTreeMap::new(),
            callable_hint: None,
        };
        for (index, parameter) in function.parameters.iter().enumerate() {
            let term = if let Some(bound) = function
                .callable_parameter
                .as_ref()
                .filter(|bound| bound.index as usize == index)
            {
                let ty = Type::Opaque {
                    identity: function.id,
                    hidden: Box::new(Type::FunctionPointer {
                        parameters: bound.parameters.clone(),
                        result: Box::new(bound.result.clone()),
                    }),
                };
                let Term::Closure(id) = this.known_term(&ty, parameter.span)? else {
                    unreachable!()
                };
                this.helper_environment = Some((bound.generic_name.clone(), id));
                if callable_fields::placeholder_package(&parameter.ty, &bound.generic_name) {
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
    fn known_result(&mut self, signature: &Signature, span: Span) -> R<Term> {
        if matches!(signature.result, Type::Opaque { .. }) {
            if let Some(id) = self.factory_terms.get(&signature.id) {
                return Ok(Term::Closure(*id));
            }
            let term = self.known_term(&signature.result, span)?;
            if let Term::Closure(id) = term {
                self.factory_terms.insert(signature.id, id);
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
        if let Some((name, id)) = self.helper_environment.clone() {
            if matches!(ty,Type::Named(n) if n==&name) {
                return Ok(Term::Closure(id));
            }
            if callable_fields::placeholder_package(ty, &name) {
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
                && !ty.is_shared_resource_head_address()
                && !ty.is_package_address()
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
        let parameters = self.closures[id].parameters.clone();
        if parameters.len() != bound.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                argument.span,
                "closure signature differs from callable bound",
            ));
        }
        for (variable, ty) in parameters.into_iter().zip(&bound.parameters) {
            self.bind(variable, ty, argument.span)?;
        }
        self.bind(self.closures[id].result, &bound.result, argument.span)?;
        Ok(id)
    }
    fn fresh(&mut self, domain: u8, literal: bool, span: Span) -> R<Variable> {
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
        self.graph.bind(variable, ty, span)
    }
    fn scalar(&mut self, term: &Term, span: Span) -> R<Variable> {
        match term {
            Term::Scalar(variable) => Ok(*variable),
            Term::Function(signature) => {
                for ty in signature
                    .parameters
                    .iter()
                    .chain(std::iter::once(&signature.result))
                {
                    validate_type(ty, 0, span, self.budget)?;
                }
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
        if let Some((name, _)) = &self.helper_environment
            && (matches!(ty,Type::Named(n) if n==name)
                || callable_fields::placeholder_package(ty, name))
        {
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
                    || ty.is_shared_resource_head_address()
                    || ty.is_package_address())
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
                | Term::Closure(_)
                | Term::ClosureArray { .. }
                | Term::ClosurePackage { .. }
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
                && let Term::Address(variable, _) = &binding.term
                && self.graph.is_slice(*variable)
            {
                return Err(unsupported(
                    span,
                    "slice capture or use in a closure is not implemented",
                ));
            }
            // This graph collects source type relationships only. The concrete
            // closure checker later admits explicit-move Package captures and
            // independently classifies their responsibility slots.
            if write
                && !binding.mutable
                && !matches!(binding.term, Term::Address(_, true) | Term::Resource(..))
            {
                return Err(error(
                    "E-IMMUTABLE-ASSIGNMENT",
                    span,
                    format!("{name} is not mutable"),
                ));
            }
            return Ok(match binding.term {
                Term::Resource(variable, _) => Term::Resource(variable, false),
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
        let CoreExprKind::Member { object, member } = &expression.kind else {
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
        let Some(ty @ Type::Enum { variants, .. }) =
            enums::constructor_type(self.enums, name, expression.span)?
        else {
            return Ok(None);
        };
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
        let Type::Enum { variants, .. } = ty else {
            unreachable!()
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
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.budget.node(depth, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.budget.node(depth + 1, statement.span)?;
            let index = self.statement_index;
            self.statement_index += 1;
            match &statement.kind {
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
                            .is_some_and(|b| b.mutable && matches!(b.term, Term::Resource(_, true)))
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
                            _ => {
                                return Err(error(
                                    "E-TYPE-MISMATCH",
                                    statement.span,
                                    "managed replacement requires a new owning carrier",
                                ));
                            }
                        }
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
                        self.insert(&name, Term::Scalar(variable), false, span)?;
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
                        self.insert(&name, Term::Scalar(variable), false, span)?;
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
                    let variable = self.scalar(&value_type, value.span)?;
                    for arm in arms {
                        self.scopes.push(BTreeMap::new());
                        for (name, variable, span) in
                            self.graph.pattern_bind(&arm.pattern, variable)?
                        {
                            self.insert(&name, Term::Scalar(variable), false, span)?;
                        }
                        self.block(&arm.body, depth + 1)?;
                        self.scopes.pop();
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
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
        if let Some(union @ Type::ErrorUnion { .. }) = expected
            && !matches!(
                &expression.kind,
                CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            )
        {
            let term = self.expr_raw(expression, None, depth, write)?;
            let variable = self.scalar(&term, expression.span)?;
            let actual = self.graph.injection_type(variable, expression.span)?;
            errors::injection(&actual, union, expression.span)?;
            return self.known_term(union, expression.span);
        }
        self.expr_raw(expression, expected, depth, write)
    }
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
            CoreExprKind::IntegerConvert { value, target } => {
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
            CoreExprKind::Ascribed { ty, value } => {
                let term = self.expr_expected(value, Some(ty), depth + 1, false)?;
                self.bind_term(&term, ty, span)?;
                Ok(term)
            }
            CoreExprKind::Try(operand) => {
                let value = self.expr(operand, depth + 1, false)?;
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
            CoreExprKind::Integer(text) => {
                self.budget.work(text.len(), span)?;
                let domain = if text.ends_with("u8") {
                    U8
                } else if text.ends_with("u16") {
                    U16
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
            CoreExprKind::Bool(_) => Ok(Term::Scalar(self.fresh(BOOL, false, span)?)),
            CoreExprKind::Unit => Ok(Term::Scalar(self.fresh(UNIT, false, span)?)),
            CoreExprKind::Name(name) => {
                let term = self.name(name, write, span)?;
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
            CoreExprKind::Unary { op, operand } => {
                let value = self.expr(operand, depth + 1, false)?;
                if *op == UnaryOp::Give {
                    if let CoreExprKind::Member { object, member } = &operand.kind
                        && let CoreExprKind::Name(name) = &object.kind
                        && let Some(Binding {
                            term: Term::Scalar(variable),
                            ..
                        }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                        && let Some(ty @ Type::Package { .. }) = self.graph.ty(*variable)?
                    {
                        let field = aggregates::field(&ty, member, span)?.1;
                        if !matches!(field, Type::Managed(_)) {
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
                    if *op == UnaryOp::Borrow
                        && let CoreExprKind::Name(name) = &operand.kind
                        && let Some(Binding {
                            term: Term::Resource(variable, true),
                            ..
                        }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                    {
                        let variable = *variable;
                        self.graph.solve()?;
                        let payload = self.resolved(variable, span)?;
                        if !matches!(payload, Type::Record { .. }) || !payload.is_copy_value() {
                            return Err(unsupported(
                                span,
                                "head borrow requires a finite user-resource payload",
                            ));
                        }
                        let head = self.fresh(VALUE, false, span)?;
                        self.bind(head, &Type::Managed(Box::new(payload)), span)?;
                        return Ok(Term::Address(head, false));
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
                        Term::Address(variable, mutable) if !write || mutable => {
                            Ok(Term::Scalar(variable))
                        }
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
                    UnaryOp::Negate | UnaryOp::BitNot => INTEGER,
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
            CoreExprKind::Binary { op, left, right } => {
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
                    INTEGER
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
            CoreExprKind::Call { callee, arguments } => {
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
                        let element =
                            expected.and_then(dynamic_store::element).ok_or_else(|| {
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
                    let Term::Resource(variable, _) = first else {
                        return Err(error(
                            "E-OWNED-STORE-TYPE",
                            span,
                            "Store primitive requires a resource description",
                        ));
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
                        let wanted =
                            if matches!(name, "store_reserve" | "store_at" | "store_at_mut") {
                                Type::I64
                            } else {
                                (*element).clone()
                            };
                        let term =
                            self.expr_expected(&arguments[1], Some(&wanted), depth + 1, false)?;
                        self.bind_term(&term, &wanted, arguments[1].span)?;
                    }
                    return self.known_term(&dynamic_store::result_type(name, &element), span);
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
                let callee_type = self.expr(callee, depth + 1, false)?;
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
                                .as_ref()
                                .filter(|bound| bound.index as usize == index + 1)
                            {
                                state_environment = Some(self.bind_callable_argument(
                                    argument,
                                    ty,
                                    bound,
                                    depth + 1,
                                )?);
                            } else {
                                let value =
                                    self.expr_expected(argument, Some(ty), depth + 1, false)?;
                                self.bind_term(&value, ty, argument.span)?;
                            }
                        }
                        if let (Some(environment), Some(bound)) =
                            (state_environment, signature.callable.as_ref())
                            && callable_fields::placeholder_package(
                                &signature.result,
                                &bound.generic_name,
                            )
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
                                .as_ref()
                                .filter(|b| b.index as usize == index)
                            {
                                state_environment = Some(self.bind_callable_argument(
                                    argument,
                                    ty,
                                    bound,
                                    depth + 1,
                                )?);
                            } else {
                                let value =
                                    self.expr_expected(argument, Some(ty), depth + 1, false)?;
                                self.bind_term(&value, ty, argument.span)?;
                            }
                        }
                        if let (Some(environment), Some(bound)) =
                            (state_environment, signature.callable.as_ref())
                            && callable_fields::placeholder_package(
                                &signature.result,
                                &bound.generic_name,
                            )
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
                    Term::Closure(id) | Term::ClosureAddress(id, _) => {
                        self.budget.work(self.closures[id].parameters.len(), span)?;
                        (
                            self.closures[id].parameters.clone(),
                            self.closures[id].result,
                        )
                    }
                    Term::Scalar(callee) => {
                        let known = self.graph.ty(callee)?;
                        let hints = match &known {
                            Some(Type::FunctionPointer { parameters, .. }) => Some(parameters),
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
                        self.graph.address(
                            parameter,
                            inner,
                            wanted.or(Some(mutable)),
                            argument.span,
                        )?;
                    } else {
                        self.unify(&value, &Term::Scalar(parameter), argument.span)?;
                    }
                }
                if let Some(ty @ Type::Address { .. }) = self.graph.ty(result)? {
                    self.known_term(&ty, span)
                } else {
                    Ok(Term::Scalar(result))
                }
            }
            CoreExprKind::Member { object, member } => {
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
                    } = &object.kind
                    && let CoreExprKind::Name(name) = &shell.kind
                    && let Some(Binding {
                        term: Term::Scalar(variable),
                        ..
                    }) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
                    && let Some(ty @ Type::Package { .. }) = self.graph.ty(*variable)?
                {
                    matches!(
                        aggregates::field(&ty, field, object.span)?.1,
                        Type::Managed(_)
                    )
                } else {
                    false
                };
                let object_type = self.expr(object, depth + 1, write && !resource_field)?;
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
                        if let Some(
                            ty @ (Type::Record { .. } | Type::Package { .. } | Type::ByteBuffer),
                        ) = method_type
                        {
                            validate_type(&ty, 0, span, self.budget)?;
                            if let Some(signature) = aggregates::resolve_type_method(
                                &ty,
                                member,
                                self.signatures,
                                self.imports,
                            ) {
                                let signature = context_signature(signature, self.budget, span)?;
                                let receiver = &signature.parameters[0];
                                let value = if receiver.is_shared_resource_head_address()
                                    && matches!(&object.kind, CoreExprKind::Name(name)
                                        if self.scopes.iter().rev().find_map(|scope|scope.get(name))
                                            .is_some_and(|binding| matches!(binding.term, Term::Resource(_, true))))
                                {
                                    self.known_term(receiver, object.span)?
                                } else if let (
                                    Type::Address { mutable, .. },
                                    Term::Scalar(variable),
                                ) = (receiver, &object_type)
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
            CoreExprKind::Tuple(items) => {
                let mut fields = Vec::new();
                let hints = match expected {
                    Some(Type::Tuple(fields)) => Some(fields),
                    _ => None,
                };
                for (index, item) in items.iter().enumerate() {
                    let term = self.expr_expected(
                        item,
                        hints.and_then(|h| h.get(index)),
                        depth + 1,
                        false,
                    )?;
                    fields.push(self.scalar(&term, item.span)?);
                }
                let result = self.fresh(VALUE, false, span)?;
                self.graph.tuple(result, fields, span)?;
                Ok(Term::Scalar(result))
            }
            CoreExprKind::Array(items) => {
                let length = u32::try_from(items.len()).map_err(|_| {
                    Diagnostic::limit(Phase::Types, "array length overflow", Some(span))
                })?;
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
            CoreExprKind::Record { ty, fields } => {
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
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                let base = self.expr(base, depth + 1, false)?;
                if let Some(ty) = ty {
                    self.bind_term(&base, ty, span)?;
                }
                for (name, value) in fields {
                    let result = self.fresh(VALUE, false, value.span)?;
                    let base = self.scalar(&base, span)?;
                    self.graph.project(
                        base,
                        Projection::Field(name.clone()),
                        result,
                        value.span,
                    )?;
                    let hint = self.graph.ty(result)?;
                    let actual = self.expr_expected(value, hint.as_ref(), depth + 1, false)?;
                    self.unify(&Term::Scalar(result), &actual, value.span)?;
                }
                Ok(base)
            }
            CoreExprKind::Index { object, index } => {
                let base = self.expr(object, depth + 1, write)?;
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
            CoreExprKind::Slice { object, start, end } => {
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
            CoreExprKind::Sequence { bindings, value } => {
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
            CoreExprKind::CompileTime(inner) => {
                // Only type relationships are collected. No value is read or
                // computed; the original CT expression still enters verify_ct.
                self.expr_expected(inner, expected, depth + 1, false)
            }
            CoreExprKind::Closure {
                move_capture: _,
                parameters,
                return_type,
                body,
            } => {
                if return_type
                    .as_ref()
                    .is_some_and(|t| !(t.is_copy_value() || lending::supported_result(t)))
                    || parameters.iter().any(|p| {
                        p.ty.as_ref()
                            .is_some_and(|t| !closures::signature_parameter(t))
                    })
                {
                    return Err(unsupported(
                        span,
                        "closure parameters require a finite Copy type or one Copy address layer; address parameters require a Copy result",
                    ));
                }
                let callable_hint = self.callable_hint.take();
                let pointer = match expected {
                    Some(Type::FunctionPointer { parameters, result }) => {
                        Some((parameters, result.as_ref()))
                    }
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
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition_type = self.expr(condition, depth + 1, false)?;
                self.bind_term(&condition_type, &Type::Bool, condition.span)?;
                let then_type = self.expr_expected(then_value, expected, depth + 1, false)?;
                let else_type = self.expr_expected(else_value, expected, depth + 1, false)?;
                self.unify(&then_type, &else_type, span)?;
                Ok(then_type)
            }
            CoreExprKind::Unsupported(feature) => Err(unsupported(span, feature)),
            _ => Err(unsupported(
                span,
                "non-scalar expression in closure inference",
            )),
        }
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
        let mut result = function.clone();
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
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
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
            CoreExprKind::Ascribed { value, .. } | CoreExprKind::IntegerConvert { value, .. } => {
                self.normalize_expr(value, expression, statement)?
            }
            CoreExprKind::Integer(text) => {
                if !text.ends_with("u8")
                    && !text.ends_with("u16")
                    && !text.ends_with("i32")
                    && !text.ends_with("i64")
                {
                    let ty = self.resolved(self.literal_types[&index], value.span)?;
                    text.push_str(match ty {
                        Type::U8 => "u8",
                        Type::U16 => "u16",
                        Type::I64 => "i64",
                        _ => "i32",
                    });
                }
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand) => self.normalize_expr(operand, expression, statement)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.normalize_expr(left, expression, statement)?;
                self.normalize_expr(right, expression, statement)?;
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
