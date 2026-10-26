//! Source CT expansion produces ordinary, untrusted Core values. Every executed
//! operation first passes `zil_verify::verify_ct`; this module has no host hooks.
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{CompileResult, Diagnostic, Limits, Phase, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

/// Evaluate the currently supported closed scalar `#(...)` expressions.
/// A driver-supplied raw CFG producer extends calls/control flow through
/// `evaluate_source_constants_with_compiler`; the VM still verifies every query.
pub fn evaluate_source_constants(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<CoreModule> {
    evaluate_source_constants_with_usage(core, target, limits).map(|(core, _)| core)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceEvaluationUsage {
    pub steps: u64,
    pub nodes: usize,
}

/// A bounded index of raw serializable-value headers. These are type hints, not permissions:
/// types still checks visibility, aliases, argument types and arity afterwards.
/// No dependency body is copied or executed while constructing or using it.
pub struct SourceProgramContext<'a> {
    functions: BTreeMap<(zil_base::ModuleId, &'a str), Option<&'a CoreFunction>>,
    imports: BTreeMap<(zil_base::ModuleId, &'a str), Option<zil_base::ModuleId>>,
}
impl<'a> SourceProgramContext<'a> {
    pub fn new(program: &'a CoreProgram, limits: &Limits) -> CompileResult<Self> {
        let limit = || {
            vec![Diagnostic::limit(
                Phase::Ct,
                "program CT header index budget exceeded",
                None,
            )]
        };
        let mut nodes = program.modules.len();
        if nodes > limits.max_nodes {
            return Err(limit());
        }
        let mut work = limits.max_analysis_steps;
        let mut count = limits.max_functions;
        let mut functions = BTreeMap::new();
        let mut imports = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for module in &program.modules {
            let id = module.core.module_id;
            if !ids.insert(id) {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-MODULE-ID",
                    None,
                    "duplicate module identity in CT context",
                )]);
            }
            nodes = nodes
                .checked_add(module.core.functions.len())
                .and_then(|n| n.checked_add(module.imports.len()))
                .filter(|n| *n <= limits.max_nodes)
                .ok_or_else(limit)?;
            count = count
                .checked_sub(module.core.functions.len())
                .ok_or_else(limit)?;
            for function in &module.core.functions {
                work = work
                    .checked_sub(function.name.len())
                    .and_then(|n| n.checked_sub(function.parameters.len()))
                    .ok_or_else(limit)?;
                if function.parameters.len() > limits.max_locals {
                    return Err(limit());
                }
                let mut usable =
                    hint_type(&function.return_type, 0, &mut nodes, &mut work, limits)?;
                for p in &function.parameters {
                    work = work.checked_sub(p.name.len()).ok_or_else(limit)?;
                    usable &= hint_type(&p.ty, 0, &mut nodes, &mut work, limits)?;
                }
                functions
                    .entry((id, function.name.as_str()))
                    .and_modify(|entry| *entry = None)
                    .or_insert(usable.then_some(function));
            }
            for import in &module.imports {
                work = work.checked_sub(import.alias.len()).ok_or_else(limit)?;
                imports
                    .entry((id, import.alias.as_str()))
                    .and_modify(|entry| *entry = None)
                    .or_insert(Some(import.module));
            }
        }
        Ok(Self { functions, imports })
    }

    fn function(&self, module: zil_base::ModuleId, callee: &CoreExpr) -> Option<&'a CoreFunction> {
        let (module, name) = match &callee.kind {
            CoreExprKind::Name(name) => (module, name.as_str()),
            CoreExprKind::Member { object, member } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return None;
                };
                let module = self
                    .imports
                    .get(&(module, alias.as_str()))
                    .copied()
                    .flatten()?;
                (module, member.as_str())
            }
            _ => return None,
        };
        self.functions.get(&(module, name)).copied().flatten()
    }
}

// Borrowed header preflight precedes any use that clones a type hint. Unknown
// capabilities remain unavailable as hints; ordinary types and CT verify decide
// acceptance independently after query construction.
pub(super) fn hint_type(
    ty: &Type,
    depth: usize,
    nodes: &mut usize,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<bool> {
    let limit = || {
        vec![Diagnostic::limit(
            Phase::Ct,
            "CT header type budget exceeded",
            None,
        )]
    };
    if depth > limits.max_call_depth || *nodes >= limits.max_nodes {
        return Err(limit());
    }
    *nodes += 1;
    *work = work.checked_sub(1).ok_or_else(limit)?;
    match ty {
        Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64 => Ok(true),
        Type::Named(n) => {
            *work = work.checked_sub(n.len()).ok_or_else(limit)?;
            Ok(true)
        }
        Type::Tuple(fields) => {
            let mut usable = true;
            for f in fields {
                usable &= hint_type(f, depth + 1, nodes, work, limits)?;
            }
            Ok(usable)
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            *work = work.checked_sub(name.len()).ok_or_else(limit)?;
            let mut usable = matches!(ty, Type::Record { .. });
            for (name, f) in fields {
                *work = work.checked_sub(name.len()).ok_or_else(limit)?;
                usable &= hint_type(f, depth + 1, nodes, work, limits)?;
            }
            Ok(usable)
        }
        Type::Array { element, .. } => hint_type(element, depth + 1, nodes, work, limits),
        Type::Enum { name, variants } => {
            *work = work.checked_sub(name.len()).ok_or_else(limit)?;
            let mut usable = true;
            for (name, fields) in variants {
                if *nodes >= limits.max_nodes {
                    return Err(limit());
                }
                *nodes += 1;
                *work = work
                    .checked_sub(1)
                    .and_then(|n| n.checked_sub(name.len()))
                    .ok_or_else(limit)?;
                for field in fields {
                    usable &= hint_type(field, depth + 1, nodes, work, limits)?;
                }
            }
            Ok(usable)
        }
        Type::Address { inner, .. }
        | Type::Slice { element: inner }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Opaque { hidden: inner, .. } => {
            hint_type(inner, depth + 1, nodes, work, limits)?;
            Ok(false)
        }
        Type::ErrorUnion { error, result } => {
            hint_type(error, depth + 1, nodes, work, limits)?;
            hint_type(result, depth + 1, nodes, work, limits)?;
            Ok(false)
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            if parameters.len() > limits.max_locals {
                return Err(limit());
            }
            for parameter in parameters {
                hint_type(parameter, depth + 1, nodes, work, limits)?;
            }
            hint_type(result, depth + 1, nodes, work, limits)?;
            Ok(false)
        }
        Type::Unsupported(name) => {
            *work = work.checked_sub(name.len()).ok_or_else(limit)?;
            Ok(false)
        }
        // Ineligible hints still have all their child shapes bounded above.
        Type::ByteBuffer | Type::ForeignHandle | Type::CWideString | Type::Closure(_) => Ok(false),
    }
}

pub fn evaluate_source_constants_with_usage(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<(CoreModule, SourceEvaluationUsage)> {
    evaluate_with_context(core, target, limits, None, None)
}

pub fn evaluate_source_constants_with_context(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
    context: &SourceProgramContext<'_>,
) -> CompileResult<(CoreModule, SourceEvaluationUsage)> {
    evaluate_with_context(core, target, limits, Some(context), None)
}

/// Produces untrusted CFG only. This callback is outside VM execution and
/// cannot grant CT execution authority; the returned candidate is independently
/// verified before any of its operations execute.
pub trait SourceCompiler {
    fn compile(
        &self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        limits: &Limits,
    ) -> CompileResult<CtProgram>;
}

pub fn evaluate_source_constants_with_compiler(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
    context: Option<&SourceProgramContext<'_>>,
    compiler: &dyn SourceCompiler,
) -> CompileResult<(CoreModule, SourceEvaluationUsage)> {
    evaluate_with_context(core, target, limits, context, Some(compiler))
}

fn evaluate_with_context(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
    context: Option<&SourceProgramContext<'_>>,
    compiler: Option<&dyn SourceCompiler>,
) -> CompileResult<(CoreModule, SourceEvaluationUsage)> {
    if core.schema_version != SCHEMA_VERSION {
        return Err(vec![Diagnostic::error(
            Phase::Ct,
            "E-SCHEMA-VERSION",
            None,
            "unknown Core schema",
        )]);
    }
    let mut walker = Walker {
        core,
        target,
        limits,
        nodes: 0,
        work_left: limits.max_analysis_steps,
        next_node: None,
        steps_left: limits.max_ct_steps,
        context,
        compiler,
        scopes: Vec::new(),
    };
    // Every declaration is borrowed and bounded before the first body, query,
    // or forward-reference hint can copy any of its metadata.
    walker.declarations()?;
    let mut functions = Vec::new();
    for f in &core.functions {
        let mut scope = BTreeMap::new();
        for p in &f.parameters {
            walker.text(&p.name, Some(p.span))?;
            walker.pattern_type_shape(&p.ty, 0, p.span)?;
            scope.insert(p.name.clone(), Some(p.ty.clone()));
        }
        walker.scopes.push(scope);
        functions.push(CoreFunction {
            callable_return: f.callable_return.clone(),
            callable_parameter: f.callable_parameter.clone(),
            result_mode: f.result_mode,
            foreign: f.foreign.clone(),
            id: f.id,
            name: f.name.clone(),
            parameters: f.parameters.clone(),
            return_type: f.return_type.clone(),
            body: walker.block(&f.body, Some(&f.return_type), 0)?,
            receiver: f.receiver.clone(),
            span: f.span,
        });
        walker.scopes.pop();
    }
    let usage = SourceEvaluationUsage {
        steps: limits.max_ct_steps - walker.steps_left,
        nodes: walker.nodes,
    };
    Ok((
        CoreModule {
            functions,
            schema_version: core.schema_version,
            module_id: core.module_id,
            source_fingerprint: core.source_fingerprint,
            aggregates: core.aggregates.clone(),
            enums: core.enums.clone(),
            unsupported: core.unsupported.clone(),
        },
        usage,
    ))
}

fn unsupported(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Ct, message, Some(span))]
}
struct Walker<'a> {
    core: &'a CoreModule,
    target: &'a TargetProfile,
    limits: &'a Limits,
    nodes: usize,
    work_left: usize,
    next_node: Option<u32>,
    steps_left: u64,
    context: Option<&'a SourceProgramContext<'a>>,
    compiler: Option<&'a dyn SourceCompiler>,
    // Type hints only. They grant no semantic authority; ordinary types/verify
    // still check the entire rewritten module after CT execution.
    // An unknown closure parameter still shadows an outer type hint.
    scopes: Vec<BTreeMap<String, Option<Type>>>,
}
impl<'a> Walker<'a> {
    fn text(&mut self, text: &str, span: Option<Span>) -> CompileResult<()> {
        self.work_left = self.work_left.checked_sub(text.len()).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "Core metadata copy budget exceeded",
                span,
            )]
        })?;
        Ok(())
    }
    fn parameters(&mut self, parameters: &[Type], result: &Type, span: Span) -> CompileResult<()> {
        if parameters.len() > self.limits.max_locals {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "callable parameter budget exceeded",
                Some(span),
            )]);
        }
        for parameter in parameters {
            self.pattern_type_shape(parameter, 0, span)?;
        }
        self.pattern_type_shape(result, 0, span)
    }
    fn declarations(&mut self) -> CompileResult<()> {
        let core = self.core;
        if core.functions.len() > self.limits.max_functions {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "function budget exceeded",
                None,
            )]);
        }
        for f in &core.functions {
            self.charge(0, f.span)?;
            self.text(&f.name, Some(f.span))?;
            if f.parameters.len() > self.limits.max_locals {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "parameter budget exceeded",
                    Some(f.span),
                )]);
            }
            self.pattern_type_shape(&f.return_type, 0, f.span)?;
            for p in &f.parameters {
                self.text(&p.name, Some(p.span))?;
                self.pattern_type_shape(&p.ty, 0, p.span)?;
            }
            if let Some(receiver) = &f.receiver {
                self.pattern_type_shape(&receiver.ty, 0, f.span)?;
            }
            if let Some(contract) = &f.callable_return {
                self.charge(0, f.span)?;
                self.parameters(&contract.parameters, &contract.result, f.span)?;
            }
            if let Some(contract) = &f.callable_parameter {
                self.charge(0, f.span)?;
                self.text(&contract.generic_name, Some(f.span))?;
                self.parameters(&contract.parameters, &contract.result, f.span)?;
            }
            if let Some(foreign) = &f.foreign {
                self.charge(0, f.span)?;
                self.text(&foreign.abi, Some(f.span))?;
                self.text(&foreign.symbol, Some(f.span))?;
                for _ in &foreign.noescape_parameters {
                    self.charge(0, f.span)?;
                }
            }
        }
        for aggregate in &core.aggregates {
            self.charge(0, aggregate.span)?;
            self.text(&aggregate.name, Some(aggregate.span))?;
            if let Some(teardown) = &aggregate.teardown {
                self.text(teardown, Some(aggregate.span))?;
            }
            for field in &aggregate.fields {
                self.text(&field.name, Some(field.span))?;
                self.pattern_type_shape(&field.ty, 0, field.span)?;
            }
        }
        for enumeration in &core.enums {
            self.charge(0, enumeration.span)?;
            self.text(&enumeration.name, Some(enumeration.span))?;
            for (name, fields) in &enumeration.variants {
                self.charge(0, enumeration.span)?;
                self.text(name, Some(enumeration.span))?;
                for field in fields {
                    self.pattern_type_shape(field, 0, enumeration.span)?;
                }
            }
        }
        for feature in &core.unsupported {
            // Even empty unsupported entries occupy the copied table.
            self.nodes = self
                .nodes
                .checked_add(1)
                .filter(|n| *n <= self.limits.max_nodes)
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "Core metadata node budget exceeded",
                        feature.span,
                    )]
                })?;
            self.text(&feature.feature, feature.span)?;
        }
        Ok(())
    }
    fn charge(&mut self, depth: usize, span: Span) -> CompileResult<()> {
        if depth > self.limits.max_call_depth || self.nodes >= self.limits.max_nodes {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "Core traversal depth/node budget exceeded",
                Some(span),
            )]);
        }
        self.nodes += 1;
        self.work_left = self.work_left.checked_sub(1).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "Core traversal work budget exceeded",
                Some(span),
            )]
        })?;
        Ok(())
    }
    fn block(
        &mut self,
        block: &CoreBlock,
        result: Option<&Type>,
        depth: usize,
    ) -> CompileResult<CoreBlock> {
        self.charge(depth, block.span)?;
        self.scopes.push(BTreeMap::new());
        let mut statements = Vec::new();
        for s in &block.statements {
            self.charge(depth + 1, s.span)?;
            let kind = match &s.kind {
                CoreStatementKind::Binding {
                    name,
                    mutable,
                    ty,
                    initializer,
                } => {
                    self.text(name, Some(s.span))?;
                    if let Some(ty) = ty {
                        self.pattern_type_shape(ty, depth + 1, s.span)?;
                    }
                    let initializer = initializer
                        .as_ref()
                        .map(|e| self.expr(e, ty.as_ref(), depth + 1))
                        .transpose()?;
                    let hint = if let Some(ty) = ty {
                        self.pattern_type_shape(ty, depth + 1, s.span)?;
                        Some(ty.clone())
                    } else if let Some(e) = &initializer {
                        self.hint(e)?
                    } else {
                        None
                    };
                    self.text(name, Some(s.span))?;
                    self.scopes
                        .last_mut()
                        .expect("block scope")
                        .insert(name.clone(), hint);
                    CoreStatementKind::Binding {
                        name: name.clone(),
                        mutable: *mutable,
                        ty: ty.clone(),
                        initializer,
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    let hint = self.hint(target)?;
                    CoreStatementKind::Assign {
                        target: self.expr(target, None, depth + 1)?,
                        value: self.expr(value, hint.as_ref(), depth + 1)?,
                    }
                }
                CoreStatementKind::Expression(e) => {
                    CoreStatementKind::Expression(self.expr(e, None, depth + 1)?)
                }
                CoreStatementKind::Return(e) => CoreStatementKind::Return(
                    e.as_ref()
                        .map(|e| self.expr(e, result, depth + 1))
                        .transpose()?,
                ),
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => CoreStatementKind::If {
                    condition: self.expr(condition, Some(&Type::Bool), depth + 1)?,
                    then_body: self.block(then_body, result, depth + 1)?,
                    else_body: else_body
                        .as_ref()
                        .map(|b| self.block(b, result, depth + 1))
                        .transpose()?,
                },
                CoreStatementKind::While { condition, body } => CoreStatementKind::While {
                    condition: self.expr(condition, Some(&Type::Bool), depth + 1)?,
                    body: self.block(body, result, depth + 1)?,
                },
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let value = self.expr(value, None, depth + 1)?;
                    self.scopes.push(BTreeMap::new());
                    self.bind_pattern(pattern, depth + 1)?;
                    let then_body = self.block(then_body, result, depth + 1)?;
                    self.scopes.pop();
                    let else_body = else_body
                        .as_ref()
                        .map(|body| self.block(body, result, depth + 1))
                        .transpose()?;
                    CoreStatementKind::IfLet {
                        pattern: pattern.clone(),
                        value,
                        then_body,
                        else_body,
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let value = self.expr(value, None, depth + 1)?;
                    self.scopes.push(BTreeMap::new());
                    self.bind_pattern(pattern, depth + 1)?;
                    let body = self.block(body, result, depth + 1)?;
                    self.scopes.pop();
                    CoreStatementKind::WhileLet {
                        pattern: pattern.clone(),
                        value,
                        body,
                    }
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    // Only closed # expressions execute here. Bounds retain
                    // source order and the runtime body is traversed once.
                    self.text(name, Some(s.span))?;
                    let hint = match self.range_hint(start, 0)? {
                        Some(hint) => Some(hint),
                        None => self.range_hint(end, 0)?,
                    };
                    let start = self.expr(start, hint.as_ref(), depth + 1)?;
                    let hint = match hint {
                        Some(hint) => Some(hint),
                        None => self.hint(&start)?,
                    };
                    let end = self.expr(end, hint.as_ref(), depth + 1)?;
                    // Untyped integer expressions default to i32. Always put
                    // the iteration name in this scope, even when both bounds
                    // are negative literals, so an outer same-named i64 cannot
                    // leak into a closed # expression's type context.
                    let hint = match hint {
                        Some(hint) => Some(hint),
                        None => self.range_hint(&start, 0)?,
                    };
                    let iteration_type = match hint {
                        Some(hint) => hint,
                        None => self.range_hint(&end, 0)?.unwrap_or(Type::I32),
                    };
                    self.text(name, Some(s.span))?;
                    self.scopes
                        .push(BTreeMap::from([(name.clone(), Some(iteration_type))]));
                    let body = self.block(body, result, depth + 1)?;
                    self.scopes.pop();
                    CoreStatementKind::ForRange {
                        name: name.clone(),
                        start,
                        end,
                        body,
                    }
                }
                CoreStatementKind::Match { value, arms } => {
                    let value = self.expr(value, None, depth + 1)?;
                    let mut transformed = Vec::new();
                    for arm in arms {
                        self.scopes.push(BTreeMap::new());
                        self.bind_pattern(&arm.pattern, depth + 1)?;
                        let body = self.block(&arm.body, result, depth + 1)?;
                        self.scopes.pop();
                        transformed.push(CoreMatchArm {
                            pattern: arm.pattern.clone(),
                            body,
                            span: arm.span,
                        });
                    }
                    CoreStatementKind::Match {
                        value,
                        arms: transformed,
                    }
                }
                CoreStatementKind::Block(b) => {
                    CoreStatementKind::Block(self.block(b, result, depth + 1)?)
                }
                CoreStatementKind::Unsafe(b) => {
                    CoreStatementKind::Unsafe(self.block(b, result, depth + 1)?)
                }
                CoreStatementKind::Break => CoreStatementKind::Break,
                CoreStatementKind::Continue => CoreStatementKind::Continue,
                CoreStatementKind::Unsupported(text) => {
                    self.text(text, Some(s.span))?;
                    CoreStatementKind::Unsupported(text.clone())
                }
            };
            statements.push(CoreStatement {
                id: s.id,
                kind,
                span: s.span,
            });
        }
        self.scopes.pop();
        Ok(CoreBlock {
            statements,
            span: block.span,
        })
    }
    fn bind_pattern(&mut self, pattern: &CorePattern, depth: usize) -> CompileResult<()> {
        self.charge(depth, pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.text(name, Some(pattern.span))?;
                self.text(name, Some(pattern.span))?;
                self.scopes
                    .last_mut()
                    .expect("match scope")
                    .insert(name.clone(), None);
            }
            CorePatternKind::Tuple(fields) => {
                for field in fields {
                    self.bind_pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Variant {
                ty,
                variant,
                fields,
            } => {
                self.text(variant, Some(pattern.span))?;
                self.pattern_type_shape(ty, depth + 1, pattern.span)?;
                for field in fields {
                    self.bind_pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Integer(text) => self.text(text, Some(pattern.span))?,
            CorePatternKind::Wildcard | CorePatternKind::Bool(_) => {}
        }
        Ok(())
    }
    // Raw Core patterns carry a full nominal type. Bound that tree before the
    // enclosing pattern is cloned; visiting only the payload patterns is insufficient.
    fn pattern_type_shape(&mut self, ty: &Type, depth: usize, span: Span) -> CompileResult<()> {
        hint_type(ty, depth, &mut self.nodes, &mut self.work_left, self.limits)
            .map(|_| ())
            .map_err(|mut errors| {
                for error in &mut errors {
                    error.span = Some(span);
                }
                errors
            })
    }
    fn expr(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> CompileResult<CoreExpr> {
        self.charge(depth, e.span)?;
        let kind = match &e.kind {
            CoreExprKind::Literal(constant) => {
                if let Constant::WideString(units) = constant {
                    self.nodes = self
                        .nodes
                        .checked_add(units.len())
                        .filter(|n| *n <= self.limits.max_nodes)
                        .ok_or_else(|| {
                            vec![Diagnostic::limit(
                                Phase::Ct,
                                "static UTF16 units exhaust source traversal budget",
                                Some(e.span),
                            )]
                        })?;
                    self.work_left = self.work_left.checked_sub(units.len()).ok_or_else(|| {
                        vec![Diagnostic::limit(
                            Phase::Ct,
                            "static UTF16 copy work budget exceeded",
                            Some(e.span),
                        )]
                    })?;
                }
                CoreExprKind::Literal(constant.clone())
            }
            CoreExprKind::IntegerConvert { value, target } => {
                self.pattern_type_shape(target, depth + 1, e.span)?;
                if !target.is_integer() {
                    return Err(unsupported(
                        e.span,
                        "integer conversion target must be u8/u16/i32/i64",
                    ));
                }
                CoreExprKind::IntegerConvert {
                    value: Box::new(self.expr(value, None, depth + 1)?),
                    target: target.clone(),
                }
            }
            CoreExprKind::Ascribed { ty, value } => {
                if !hint_type(
                    ty,
                    depth + 1,
                    &mut self.nodes,
                    &mut self.work_left,
                    self.limits,
                )? {
                    return Err(unsupported(
                        e.span,
                        "CT value ascription requires a serializable value shape",
                    ));
                }
                CoreExprKind::Ascribed {
                    ty: ty.clone(),
                    value: Box::new(self.expr(value, Some(ty), depth + 1)?),
                }
            }
            CoreExprKind::CompileTime(inner) => {
                let (value, ty) = self.query(inner, expected, e.span)?;
                if let crate::CompileTimeValue::Scalar(c) = value {
                    return Ok(CoreExpr {
                        id: e.id,
                        span: e.span,
                        kind: crate::materialize::scalar(c),
                    });
                }
                if self.next_node.is_none() {
                    self.next_node =
                        Some(crate::materialize::first_free_id(self.core, self.limits)?);
                }
                let next = self
                    .next_node
                    .as_mut()
                    .expect("fresh identity range initialized");
                let child = zil_base::NodeId(*next);
                *next = next.checked_add(1).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "CT result identity overflow",
                        Some(e.span),
                    )]
                })?;
                let value = crate::materialize::Reifier {
                    next,
                    nodes: &mut self.nodes,
                    work_left: &mut self.work_left,
                    limits: self.limits,
                }
                .value(value, &ty, child, e.span, depth + 1)?;
                return Ok(CoreExpr {
                    id: e.id,
                    span: e.span,
                    kind: CoreExprKind::Ascribed {
                        ty,
                        value: Box::new(value),
                    },
                });
            }
            CoreExprKind::Try(inner) => {
                CoreExprKind::Try(Box::new(self.expr(inner, None, depth + 1)?))
            }
            CoreExprKind::Unary { op, operand } => CoreExprKind::Unary {
                op: *op,
                operand: Box::new(self.expr(
                    operand,
                    if *op == UnaryOp::Not {
                        Some(&Type::Bool)
                    } else {
                        expected
                    },
                    depth + 1,
                )?),
            },
            CoreExprKind::Binary { op, left, right } => {
                let left_hint = self.hint(left)?;
                let hint = if comparison(*op) {
                    None
                } else {
                    expected.or(left_hint.as_ref())
                };
                let left = self.expr(left, hint, depth + 1)?;
                let right_hint = self.hint(&left)?;
                CoreExprKind::Binary {
                    op: *op,
                    left: Box::new(left),
                    right: Box::new(self.expr(right, hint.or(right_hint.as_ref()), depth + 1)?),
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                let core = self.core;
                let mut hints = Vec::new();
                if let Some(f) = self.function(callee) {
                    for p in f.parameters.iter().take(arguments.len()) {
                        self.pattern_type_shape(&p.ty, depth + 1, e.span)?;
                        hints.push(p.ty.clone());
                    }
                } else {
                    let enum_fields = (|| {
                        let CoreExprKind::Member { object, member } = &callee.kind else {
                            return None;
                        };
                        let CoreExprKind::Name(name) = &object.kind else {
                            return None;
                        };
                        if self
                            .scopes
                            .iter()
                            .rev()
                            .any(|scope| scope.contains_key(name))
                        {
                            return None;
                        }
                        core.enums
                            .iter()
                            .find(|declaration| &declaration.name == name)
                            .and_then(|declaration| {
                                declaration.variants.iter().find(|(name, _)| name == member)
                            })
                            .map(|(_, fields)| fields)
                    })();
                    if let Some(fields) = enum_fields {
                        for ty in fields.iter().take(arguments.len()) {
                            self.pattern_type_shape(ty, depth + 1, e.span)?;
                            hints.push(ty.clone());
                        }
                    }
                }
                let mut args = Vec::new();
                for (i, a) in arguments.iter().enumerate() {
                    args.push(self.expr(a, hints.get(i), depth + 1)?);
                }
                CoreExprKind::Call {
                    callee: Box::new(self.expr(callee, None, depth + 1)?),
                    arguments: args,
                }
            }
            CoreExprKind::Array(items) => CoreExprKind::Array(
                items
                    .iter()
                    .map(|i| {
                        self.expr(
                            i,
                            match expected {
                                Some(Type::Array { element, .. }) => Some(element.as_ref()),
                                _ => None,
                            },
                            depth + 1,
                        )
                    })
                    .collect::<CompileResult<_>>()?,
            ),
            CoreExprKind::Index { object, index } => CoreExprKind::Index {
                object: Box::new(self.expr(object, None, depth + 1)?),
                index: Box::new(self.expr(index, None, depth + 1)?),
            },
            CoreExprKind::Slice { object, start, end } => CoreExprKind::Slice {
                object: Box::new(self.expr(object, None, depth + 1)?),
                start: start
                    .as_ref()
                    .map(|value| self.expr(value, None, depth + 1).map(Box::new))
                    .transpose()?,
                end: end
                    .as_ref()
                    .map(|value| self.expr(value, None, depth + 1).map(Box::new))
                    .transpose()?,
            },
            CoreExprKind::Tuple(items) => CoreExprKind::Tuple(
                items
                    .iter()
                    .map(|i| self.expr(i, None, depth + 1))
                    .collect::<CompileResult<_>>()?,
            ),
            CoreExprKind::Member { object, member } => {
                self.text(member, Some(e.span))?;
                CoreExprKind::Member {
                    object: Box::new(self.expr(object, None, depth + 1)?),
                    member: member.clone(),
                }
            }
            CoreExprKind::Record { ty, fields } => {
                self.pattern_type_shape(ty, depth + 1, e.span)?;
                CoreExprKind::Record {
                    ty: ty.clone(),
                    fields: self.fields(fields, depth + 1)?,
                }
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.pattern_type_shape(ty, depth + 1, e.span)?;
                }
                CoreExprKind::RecordUpdate {
                    ty: ty.clone(),
                    base: Box::new(self.expr(base, None, depth + 1)?),
                    fields: self.fields(fields, depth + 1)?,
                }
            }
            CoreExprKind::Sequence { bindings, value } => CoreExprKind::Sequence {
                bindings: self.fields(bindings, depth + 1)?,
                value: Box::new(self.expr(value, expected, depth + 1)?),
            },
            CoreExprKind::Closure {
                move_capture,
                parameters,
                return_type,
                body,
            } => {
                if parameters.len() > self.limits.max_locals {
                    return Err(vec![Diagnostic::limit(
                        Phase::Ct,
                        "closure parameter budget exceeded",
                        Some(e.span),
                    )]);
                }
                // Bound the complete signature before the scope map starts copying.
                for parameter in parameters {
                    self.charge(depth + 1, parameter.span)?;
                    self.text(&parameter.name, Some(parameter.span))?;
                    if let Some(ty) = &parameter.ty {
                        self.pattern_type_shape(ty, depth + 1, parameter.span)?;
                    }
                }
                if let Some(ty) = return_type {
                    self.pattern_type_shape(ty, depth + 1, e.span)?;
                }
                let mut scope = BTreeMap::new();
                for parameter in parameters {
                    self.text(&parameter.name, Some(parameter.span))?;
                    if let Some(ty) = &parameter.ty {
                        self.pattern_type_shape(ty, depth + 1, parameter.span)?;
                    }
                    scope.insert(parameter.name.clone(), parameter.ty.clone());
                }
                self.scopes.push(scope);
                let body = self.block(body, return_type.as_ref(), depth + 1)?;
                self.scopes.pop();
                CoreExprKind::Closure {
                    move_capture: *move_capture,
                    parameters: parameters.clone(),
                    return_type: return_type.clone(),
                    body,
                }
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => CoreExprKind::If {
                condition: Box::new(self.expr(condition, Some(&Type::Bool), depth + 1)?),
                then_value: Box::new(self.expr(then_value, expected, depth + 1)?),
                else_value: Box::new(self.expr(else_value, expected, depth + 1)?),
            },
            other => {
                match other {
                    CoreExprKind::Integer(text)
                    | CoreExprKind::String(text)
                    | CoreExprKind::Name(text)
                    | CoreExprKind::ReceiverMember(text)
                    | CoreExprKind::Unsupported(text) => self.text(text, Some(e.span))?,
                    _ => {}
                }
                other.clone()
            }
        };
        Ok(CoreExpr {
            id: e.id,
            kind,
            span: e.span,
        })
    }
    fn query(
        &mut self,
        inner: &CoreExpr,
        expected: Option<&Type>,
        span: Span,
    ) -> CompileResult<(crate::CompileTimeValue, Type)> {
        // Select the complete query's compiler before inspecting literal types.
        // A later call can supply i64 context to an earlier unsuffixed literal;
        // partially compiling that literal as i32 would reject valid source.
        if let Some(compiler) = self.compiler
            && !CtBuilder::supports(inner, self.limits)?
        {
            self.reject_runtime_names(inner)?;
            let mut limits = self.limits.clone();
            limits.max_ct_steps = self.steps_left;
            if limits.max_ct_steps == 0 {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "module CT operation budget exceeded",
                    Some(span),
                )]);
            }
            let raw = compiler.compile(inner, expected, &limits)?;
            // Count generated CFG across all queries in this source module;
            // repeated CT calls cannot each reset the node allowance.
            let cost = raw.module.functions.iter().fold(0usize, |total, f| {
                f.blocks.iter().fold(
                    total
                        .saturating_add(1)
                        .saturating_add(f.locals.len())
                        .saturating_add(f.values.len()),
                    |n, b| n.saturating_add(1).saturating_add(b.instructions.len()),
                )
            });
            self.nodes = self
                .nodes
                .checked_add(cost)
                .filter(|n| *n <= self.limits.max_nodes)
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "module CT generated CFG node budget exceeded",
                        Some(span),
                    )]
                })?;
            // A producer cannot retain a private wrapper or bypass this gate.
            let verified = zil_verify::verify_ct_program(raw, self.target, &limits)?;
            let evaluated = crate::evaluate_program(&verified).map_err(|mut errors| {
                for d in &mut errors {
                    if d.span.is_none() {
                        d.span = Some(span);
                    }
                }
                errors
            })?;
            self.steps_left = self
                .steps_left
                .checked_sub(evaluated.steps)
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "module CT operation budget exceeded",
                        Some(span),
                    )]
                })?;
            let ty = &verified.module().functions[verified.entry().0 as usize].return_type;
            // Each query copies its result shape into ordinary untrusted Core.
            // Charge the full shape, including inactive enum members, first.
            hint_type(ty, 0, &mut self.nodes, &mut self.work_left, self.limits)?;
            return Ok((evaluated.value, ty.clone()));
        }
        let mut builder = CtBuilder {
            instructions: Vec::new(),
            limits: self.limits,
            remaining: self.steps_left,
            hints: scalar_hints(inner, self.limits, &mut self.nodes)?,
        };
        let (result, _) = builder.expression(inner, expected, 0)?;
        let count = builder.instructions.len() as u64;
        self.steps_left = self.steps_left.checked_sub(count).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "module CT operation budget exceeded",
                Some(span),
            )]
        })?;
        let raw = CtModule {
            schema_version: SCHEMA_VERSION,
            input_fingerprint: self.core.source_fingerprint,
            target: self.target.clone(),
            instructions: builder.instructions,
            result,
            max_steps: count,
            dependencies: Vec::new(),
        };
        let verified = zil_verify::verify_ct(raw, self.target, self.limits).map_err(|mut ds| {
            for d in &mut ds {
                if d.span.is_none() {
                    d.span = Some(span);
                }
                d.notes.push("while checking source #(...)".into());
            }
            ds
        })?;
        crate::evaluate(&verified)
            .map(|value| {
                let ty = value.ty();
                (crate::CompileTimeValue::Scalar(value), ty)
            })
            .map_err(|mut ds| {
                for d in &mut ds {
                    if d.span.is_none() {
                        d.span = Some(span);
                    }
                }
                ds
            })
    }
    fn reject_runtime_names(&self, expression: &CoreExpr) -> CompileResult<()> {
        let mut work = vec![expression];
        let mut count = 0usize;
        while let Some(e) = work.pop() {
            count += 1;
            if count > self.limits.max_nodes {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT query name traversal exhausted node budget",
                    Some(e.span),
                )]);
            }
            match &e.kind {
                CoreExprKind::Slice { .. } => {
                    return Err(unsupported(e.span, "borrowed slices are runtime-only"));
                }
                CoreExprKind::Literal(_) => {
                    return Err(unsupported(
                        e.span,
                        "static foreign values are runtime-only",
                    ));
                }
                CoreExprKind::Call { callee, .. } if matches!(&callee.kind, CoreExprKind::Name(name) if name == "c_callback") =>
                {
                    return Err(unsupported(e.span, "static C callbacks are runtime-only"));
                }
                CoreExprKind::Name(name)
                    if self.scopes.iter().rev().any(|s| s.contains_key(name)) =>
                {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-RUNTIME",
                        Some(e.span),
                        format!("compile-time query cannot read runtime binding {name}"),
                    )]);
                }
                CoreExprKind::Unary { operand, .. }
                | CoreExprKind::CompileTime(operand)
                | CoreExprKind::Try(operand)
                | CoreExprKind::Ascribed { value: operand, .. }
                | CoreExprKind::IntegerConvert { value: operand, .. } => work.push(operand),
                CoreExprKind::Binary { left, right, .. } => {
                    work.push(left);
                    work.push(right);
                }
                CoreExprKind::Call { callee, arguments } => {
                    work.push(callee);
                    work.extend(arguments);
                }
                CoreExprKind::Member { object, .. } => work.push(object),
                CoreExprKind::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    work.push(condition);
                    work.push(then_value);
                    work.push(else_value);
                }
                CoreExprKind::Sequence { bindings, value } => {
                    work.extend(bindings.iter().map(|(_, e)| e));
                    work.push(value);
                }
                CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => work.extend(values),
                CoreExprKind::Index { object, index } => {
                    work.push(object);
                    work.push(index);
                }
                CoreExprKind::Record { fields, .. } => work.extend(fields.iter().map(|(_, e)| e)),
                CoreExprKind::RecordUpdate { base, fields, .. } => {
                    work.push(base);
                    work.extend(fields.iter().map(|(_, e)| e));
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn fields(
        &mut self,
        fields: &[(String, CoreExpr)],
        depth: usize,
    ) -> CompileResult<Vec<(String, CoreExpr)>> {
        fields
            .iter()
            .map(|(name, e)| {
                self.text(name, Some(e.span))?;
                Ok((name.clone(), self.expr(e, None, depth)?))
            })
            .collect()
    }
    fn function(&self, callee: &CoreExpr) -> Option<&'a CoreFunction> {
        let name = match &callee.kind {
            CoreExprKind::Name(name) => name,
            CoreExprKind::Member { object, .. } => {
                let CoreExprKind::Name(name) = &object.kind else {
                    return None;
                };
                name
            }
            _ => return None,
        };
        if self
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
        {
            return None;
        }
        if let Some(context) = self.context {
            context.function(self.core.module_id, callee)
        } else if matches!(callee.kind, CoreExprKind::Name(_)) {
            self.core.functions.iter().find(|f| &f.name == name)
        } else {
            None
        }
    }
    fn hint(&mut self, e: &CoreExpr) -> CompileResult<Option<Type>> {
        let ty = match &e.kind {
            CoreExprKind::IntegerConvert { target, .. } if target.is_integer() => Some(target),
            CoreExprKind::Ascribed { ty, .. } => Some(ty),
            CoreExprKind::Name(name) => self
                .scopes
                .iter()
                .rev()
                .find_map(|s| s.get(name))
                .and_then(Option::as_ref),
            CoreExprKind::Integer(n) => Some(if n.ends_with("u8") {
                &Type::U8
            } else if n.ends_with("u16") {
                &Type::U16
            } else if n.ends_with("i64") {
                &Type::I64
            } else {
                &Type::I32
            }),
            CoreExprKind::Bool(_) => Some(&Type::Bool),
            CoreExprKind::Unit => Some(&Type::Unit),
            CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Some(&Type::Bool),
            CoreExprKind::Call { callee, .. } => self.function(callee).map(|f| &f.return_type),
            _ => None,
        };
        if let Some(ty) = ty {
            // Hints can be requested before expr reaches an ascription, and may
            // copy the same header many times. Charge each actual copy first.
            hint_type(ty, 0, &mut self.nodes, &mut self.work_left, self.limits)?;
            Ok(Some(ty.clone()))
        } else {
            Ok(None)
        }
    }
    /// Unlike the default literal hint, a range context may flow from either
    /// explicitly typed endpoint into an unsuffixed literal or closed # value.
    fn range_hint(&mut self, e: &CoreExpr, depth: usize) -> CompileResult<Option<Type>> {
        self.charge(depth, e.span)?;
        match &e.kind {
            CoreExprKind::Integer(n)
                if !n.ends_with("i32")
                    && !n.ends_with("i64")
                    && !n.ends_with("u8")
                    && !n.ends_with("u16") =>
            {
                Ok(None)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Ok(Some(Type::Bool)),
            CoreExprKind::Unary { operand, .. } => self.range_hint(operand, depth + 1),
            CoreExprKind::CompileTime(inner) => self.range_hint(inner, depth + 1),
            CoreExprKind::Sequence { value, .. } => self.range_hint(value, depth + 1),
            CoreExprKind::Binary { op, .. }
                if comparison(*op) || matches!(op, BinaryOp::And | BinaryOp::Or) =>
            {
                Ok(Some(Type::Bool))
            }
            CoreExprKind::Binary { left, right, .. } => match self.range_hint(left, depth + 1)? {
                Some(hint) => Ok(Some(hint)),
                None => self.range_hint(right, depth + 1),
            },
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => match self.range_hint(then_value, depth + 1)? {
                Some(hint) => Ok(Some(hint)),
                None => self.range_hint(else_value, depth + 1),
            },
            _ => self.hint(e),
        }
    }
}

fn comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}
struct CtBuilder<'a> {
    instructions: Vec<CtInstruction>,
    limits: &'a Limits,
    remaining: u64,
    // Borrowed expression identity is private to one query, never persisted or
    // dereferenced. Raw Core NodeIds need not be unique for this lookup.
    hints: BTreeMap<*const CoreExpr, Type>,
}

/// One postorder pass supplies explicit scalar context from either operand.
/// Both the descent and reduction are charged to the module's node allowance.
/// This is not a type certificate: the emitted CtModule is independently checked.
fn scalar_hints(
    expression: &CoreExpr,
    limits: &Limits,
    nodes: &mut usize,
) -> CompileResult<BTreeMap<*const CoreExpr, Type>> {
    let mut work = vec![(expression, 0usize, false)];
    let mut hints: BTreeMap<*const CoreExpr, Type> = BTreeMap::new();
    while let Some((expression, depth, reduce)) = work.pop() {
        *nodes = nodes
            .checked_add(1)
            .filter(|n| *n <= limits.max_nodes)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "module CT scalar hint node budget exceeded",
                    Some(expression.span),
                )]
            })?;
        if depth > limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT scalar hint depth exceeded",
                Some(expression.span),
            )]);
        }
        if !reduce {
            work.push((expression, depth, true));
            match &expression.kind {
                CoreExprKind::Unary { operand, .. } | CoreExprKind::CompileTime(operand) => {
                    work.push((operand, depth + 1, false))
                }
                CoreExprKind::IntegerConvert { value, .. } => work.push((value, depth + 1, false)),
                CoreExprKind::Binary { left, right, .. } => {
                    work.push((left, depth + 1, false));
                    work.push((right, depth + 1, false));
                }
                _ => {}
            }
            continue;
        }
        let hint = match &expression.kind {
            CoreExprKind::Integer(text) if text.ends_with("u8") => Some(Type::U8),
            CoreExprKind::Integer(text) if text.ends_with("u16") => Some(Type::U16),
            CoreExprKind::Integer(text) if text.ends_with("i32") => Some(Type::I32),
            CoreExprKind::Integer(text) if text.ends_with("i64") => Some(Type::I64),
            CoreExprKind::Bool(_)
            | CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Some(Type::Bool),
            CoreExprKind::Unit => Some(Type::Unit),
            CoreExprKind::IntegerConvert { target, .. } if target.is_integer() => {
                Some(target.clone())
            }
            CoreExprKind::Unary { operand, .. } | CoreExprKind::CompileTime(operand) => {
                hints.get(&std::ptr::from_ref(operand.as_ref())).cloned()
            }
            CoreExprKind::Binary { op, .. }
                if comparison(*op) || matches!(op, BinaryOp::And | BinaryOp::Or) =>
            {
                Some(Type::Bool)
            }
            CoreExprKind::Binary { left, right, .. } => hints
                .get(&std::ptr::from_ref(left.as_ref()))
                .or_else(|| hints.get(&std::ptr::from_ref(right.as_ref())))
                .cloned(),
            _ => None,
        };
        if let Some(hint) = hint {
            hints.insert(std::ptr::from_ref(expression), hint);
        }
    }
    Ok(hints)
}
impl CtBuilder<'_> {
    fn supports(expression: &CoreExpr, limits: &Limits) -> CompileResult<bool> {
        let mut work = vec![(expression, 0usize)];
        let mut nodes = 0usize;
        while let Some((expression, depth)) = work.pop() {
            nodes += 1;
            if nodes > limits.max_nodes || depth > limits.max_call_depth {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT query classification budget exceeded",
                    Some(expression.span),
                )]);
            }
            match &expression.kind {
                CoreExprKind::Integer(_) | CoreExprKind::Bool(_) | CoreExprKind::Unit => {}
                CoreExprKind::Unary {
                    op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                    operand,
                } => {
                    work.push((operand, depth + 1));
                }
                CoreExprKind::CompileTime(inner) => work.push((inner, depth + 1)),
                CoreExprKind::IntegerConvert { value, target } if target.is_integer() => {
                    work.push((value, depth + 1))
                }
                CoreExprKind::Binary { op, left, right }
                    if !matches!(op, BinaryOp::And | BinaryOp::Or) =>
                {
                    work.push((left, depth + 1));
                    work.push((right, depth + 1));
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    fn emit(
        &mut self,
        ty: Type,
        kind: CtInstructionKind,
        span: Span,
    ) -> CompileResult<(ValueId, Type)> {
        if self.instructions.len() >= self.limits.max_instructions
            || self.instructions.len() >= self.limits.max_values
            || self.instructions.len() as u64 >= self.remaining
        {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT instruction/value budget exceeded",
                Some(span),
            )]);
        }
        let result = ValueId(self.instructions.len() as u32);
        self.instructions.push(CtInstruction {
            result,
            ty: ty.clone(),
            kind,
        });
        Ok((result, ty))
    }
    fn expression(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> CompileResult<(ValueId, Type)> {
        if depth > self.limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT expression depth exceeded",
                Some(e.span),
            )]);
        }
        match &e.kind {
            CoreExprKind::IntegerConvert { value, target } => {
                if !target.is_integer() {
                    return Err(unsupported(
                        e.span,
                        "integer conversion target must be u8/u16/i32/i64",
                    ));
                }
                let (value, _) = self.expression(value, None, depth + 1)?;
                self.emit(
                    target.clone(),
                    CtInstructionKind::ConvertInteger {
                        value,
                        target: target.clone(),
                    },
                    e.span,
                )
            }
            CoreExprKind::Integer(s) => {
                let c = integer(s, expected, false, e.span)?;
                self.emit(c.ty(), CtInstructionKind::Constant(c), e.span)
            }
            CoreExprKind::Bool(v) => self.emit(
                Type::Bool,
                CtInstructionKind::Constant(Constant::Bool(*v)),
                e.span,
            ),
            CoreExprKind::Unit => self.emit(
                Type::Unit,
                CtInstructionKind::Constant(Constant::Unit),
                e.span,
            ),
            CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if matches!(operand.kind, CoreExprKind::Integer(_)) => {
                let CoreExprKind::Integer(text) = &operand.kind else {
                    unreachable!()
                };
                let c = integer(text, expected, true, e.span)?;
                self.emit(c.ty(), CtInstructionKind::Constant(c), e.span)
            }
            CoreExprKind::Unary { op, operand }
                if matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot) =>
            {
                let (operand, ty) = self.expression(
                    operand,
                    if *op == UnaryOp::Not {
                        Some(&Type::Bool)
                    } else {
                        expected
                    },
                    depth + 1,
                )?;
                self.emit(ty, CtInstructionKind::Unary { op: *op, operand }, e.span)
            }
            CoreExprKind::Binary {
                op: BinaryOp::And | BinaryOp::Or,
                ..
            } => Err(unsupported(
                e.span,
                "CT short-circuit control flow is not implemented; it is never lowered to eager evaluation",
            )),
            CoreExprKind::Binary { op, left, right } => {
                let hint = (if comparison(*op) { None } else { expected })
                    .cloned()
                    .or_else(|| self.hints.get(&std::ptr::from_ref(left.as_ref())).cloned())
                    .or_else(|| self.hints.get(&std::ptr::from_ref(right.as_ref())).cloned());
                let (left, ty) = self.expression(left, hint.as_ref(), depth + 1)?;
                let (right, _) = self.expression(right, Some(&ty), depth + 1)?;
                self.emit(
                    if comparison(*op) { Type::Bool } else { ty },
                    CtInstructionKind::Binary {
                        op: *op,
                        left,
                        right,
                    },
                    e.span,
                )
            }
            CoreExprKind::CompileTime(inner) => self.expression(inner, expected, depth + 1),
            _ => Err(unsupported(
                e.span,
                "#(...) currently supports closed scalar arithmetic; names, calls, generators and control flow require later CT support",
            )),
        }
    }
}
fn integer(
    text: &str,
    expected: Option<&Type>,
    negative: bool,
    span: Span,
) -> CompileResult<Constant> {
    let clean = text.replace('_', "");
    let (digits, ty) = if let Some(s) = clean.strip_suffix("i32") {
        (s, Type::I32)
    } else if let Some(s) = clean.strip_suffix("i64") {
        (s, Type::I64)
    } else if let Some(s) = clean.strip_suffix("u16") {
        (s, Type::U16)
    } else if let Some(s) = clean.strip_suffix("u8") {
        (s, Type::U8)
    } else {
        (
            clean.as_str(),
            if expected == Some(&Type::U8) {
                Type::U8
            } else if expected == Some(&Type::U16) {
                Type::U16
            } else if expected == Some(&Type::I64) {
                Type::I64
            } else {
                Type::I32
            },
        )
    };
    let (radix, digits) = if let Some(s) = digits.strip_prefix("0x") {
        (16, s)
    } else if let Some(s) = digits.strip_prefix("0o") {
        (8, s)
    } else if let Some(s) = digits.strip_prefix("0b") {
        (2, s)
    } else {
        (10, digits)
    };
    let error = || {
        vec![Diagnostic::error(
            Phase::Ct,
            "E-INTEGER-RANGE",
            Some(span),
            "compile-time integer does not fit its target type",
        )]
    };
    let value = i128::from_str_radix(digits, radix).map_err(|_| error())?;
    let value = if negative {
        value.checked_neg().ok_or_else(error)?
    } else {
        value
    };
    match ty {
        Type::U8 => u8::try_from(value).map(Constant::U8).map_err(|_| error()),
        Type::U16 => u16::try_from(value).map(Constant::U16).map_err(|_| error()),
        Type::I64 => i64::try_from(value).map(Constant::I64).map_err(|_| error()),
        _ => i32::try_from(value).map(Constant::I32).map_err(|_| error()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, NodeId};
    fn expr(kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: NodeId(1),
            span: Span::new(FileId(0), 0, 12),
            kind,
        }
    }
    fn module(value: CoreExpr, ty: Type) -> CoreModule {
        let span = value.span;
        CoreModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            aggregates: vec![],
            enums: vec![],
            unsupported: vec![],
            functions: vec![CoreFunction {
                callable_return: None,
                callable_parameter: None,
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: NodeId(0),
                name: "main".into(),
                parameters: vec![],
                return_type: ty,
                receiver: None,
                span,
                body: CoreBlock {
                    span,
                    statements: vec![CoreStatement {
                        id: NodeId(2),
                        span,
                        kind: CoreStatementKind::Return(Some(expr(CoreExprKind::CompileTime(
                            Box::new(value),
                        )))),
                    }],
                },
            }],
        }
    }
    fn number(n: &str) -> CoreExpr {
        expr(CoreExprKind::Integer(n.into()))
    }
    fn binary(op: BinaryOp, a: CoreExpr, b: CoreExpr) -> CoreExpr {
        expr(CoreExprKind::Binary {
            op,
            left: Box::new(a),
            right: Box::new(b),
        })
    }
    #[test]
    fn bytes083_standalone_scalar_hints_are_symmetric_and_cumulatively_bounded() {
        let target = TargetProfile::default();
        let limits = Limits::default();
        for value in [
            binary(BinaryOp::Less, number("1"), number("255u8")),
            binary(BinaryOp::Greater, number("255u8"), number("1")),
            binary(
                BinaryOp::Equal,
                binary(BinaryOp::Add, number("1"), number("2u8")),
                number("3u8"),
            ),
        ] {
            // The helper deliberately repeats NodeId(1); hints must be keyed
            // by the actual expression, not a caller-claimed numeric identity.
            let input = module(value, Type::Bool);
            let (result, usage) =
                evaluate_source_constants_with_usage(&input, &target, &limits).unwrap();
            let CoreStatementKind::Return(Some(value)) =
                &result.functions[0].body.statements[0].kind
            else {
                panic!()
            };
            assert_eq!(value.kind, CoreExprKind::Bool(true));
            let exact = Limits {
                max_nodes: usage.nodes,
                ..limits.clone()
            };
            assert!(evaluate_source_constants(&input, &target, &exact).is_ok());
            let below = Limits {
                max_nodes: usage.nodes - 1,
                ..limits.clone()
            };
            assert!(
                evaluate_source_constants(&input, &target, &below)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.code == "E-RESOURCE-LIMIT")
            );
            let mut twice = input.clone();
            twice.functions.push(input.functions[0].clone());
            assert!(
                evaluate_source_constants(&twice, &target, &exact)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.code == "E-RESOURCE-LIMIT")
            );
            println!(
                "bytes083 scalar hints: nodes={} steps={}",
                usage.nodes, usage.steps
            );
        }
        let inner = expr(CoreExprKind::IntegerConvert {
            value: Box::new(number("256")),
            target: Type::U8,
        });
        let errors =
            evaluate_source_constants(&module(inner, Type::U8), &target, &limits).unwrap_err();
        assert_eq!(errors[0].code, "E-CT-ARITHMETIC");
    }
    #[test]
    fn callable_return_metadata_is_preserved_and_bounded_before_clone() {
        let mut m = module(number("7"), Type::I32);
        let contract = CoreCallableReturn {
            kind: CallableKind::Fn,
            parameters: vec![Type::I32],
            result: Type::I32,
        };
        m.functions[0].callable_return = Some(contract.clone());
        let target = TargetProfile::default();
        let output = evaluate_source_constants(&m, &target, &Limits::default()).unwrap();
        assert_eq!(output.functions[0].callable_return, Some(contract));
        let limits = Limits {
            max_locals: 0,
            ..Limits::default()
        };
        let errors = evaluate_source_constants(&m, &target, &limits).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-RESOURCE-LIMIT"));
        let limits = Limits {
            max_call_depth: 1,
            ..Limits::default()
        };
        m.functions[0].callable_return.as_mut().unwrap().result =
            Type::Tuple(vec![Type::Tuple(vec![Type::I32])]);
        let errors = evaluate_source_constants(&m, &target, &limits).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-RESOURCE-LIMIT"));
    }
    #[test]
    fn source_ct_returns_ordinary_core_with_source_identity() {
        let m = module(
            binary(BinaryOp::Multiply, number("6"), number("7")),
            Type::I64,
        );
        let result =
            evaluate_source_constants(&m, &TargetProfile::default(), &Limits::default()).unwrap();
        let CoreStatementKind::Return(Some(e)) = &result.functions[0].body.statements[0].kind
        else {
            panic!()
        };
        assert_eq!(e.kind, CoreExprKind::Integer("42i64".into()));
        assert_eq!(e.id, NodeId(1));
        assert_eq!(e.span, Span::new(FileId(0), 0, 12));
    }
    #[test]
    fn arithmetic_error_does_not_publish_core() {
        let m = module(
            binary(BinaryOp::Divide, number("1"), number("0")),
            Type::I32,
        );
        let errors = evaluate_source_constants(&m, &TargetProfile::default(), &Limits::default())
            .unwrap_err();
        assert_eq!(errors[0].code, "E-CT-ARITHMETIC");
        assert_eq!(errors[0].span.unwrap().file, FileId(0));
    }
    #[test]
    fn source_ct_does_not_eagerly_execute_short_circuit_rhs() {
        let m = module(
            binary(
                BinaryOp::And,
                expr(CoreExprKind::Bool(false)),
                binary(BinaryOp::Divide, number("1"), number("0")),
            ),
            Type::Bool,
        );
        assert_eq!(
            evaluate_source_constants(&m, &TargetProfile::default(), &Limits::default())
                .unwrap_err()[0]
                .category,
            zil_base::ResultCategory::Unsupported
        );
    }
    #[test]
    fn source_ct_verifier_rejects_mixed_types() {
        let m = module(
            binary(BinaryOp::Add, number("1i32"), number("2i64")),
            Type::I32,
        );
        let errors = evaluate_source_constants(&m, &TargetProfile::default(), &Limits::default())
            .unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
    }
    #[test]
    fn source_ct_budget_is_shared_by_all_expressions() {
        let mut m = module(number("1"), Type::I32);
        let mut second = m.functions[0].clone();
        second.name = "second".into();
        m.functions.push(second);
        let limits = Limits {
            max_ct_steps: 1,
            ..Limits::default()
        };
        assert_eq!(
            evaluate_source_constants(&m, &TargetProfile::default(), &limits).unwrap_err()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
    #[test]
    fn minimum_target_integer_is_preserved() {
        let m = module(
            expr(CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand: Box::new(number("9223372036854775808i64")),
            }),
            Type::I64,
        );
        let result =
            evaluate_source_constants(&m, &TargetProfile::default(), &Limits::default()).unwrap();
        let CoreStatementKind::Return(Some(e)) = &result.functions[0].body.statements[0].kind
        else {
            panic!()
        };
        assert_eq!(
            e.kind,
            CoreExprKind::Integer("-9223372036854775808i64".into())
        );
    }
}
