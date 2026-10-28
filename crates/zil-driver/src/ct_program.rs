//! Raw CT query compilation reuses the ordinary type/CFG pipeline. No operation
//! executes here; the CT crate independently verifies the result before its VM.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_base::{CompileResult, Diagnostic, Limits, ModuleId, NodeId, Phase, TargetProfile};
use zil_ir::*;

const QUERY: &str = "$ct_query";
pub(super) struct Compiler<'a> {
    pub program: &'a CoreProgram,
    pub module: ModuleId,
    pub target: &'a TargetProfile,
}
impl zil_ct::SourceCompiler for Compiler<'_> {
    fn compile(
        &self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        limits: &Limits,
    ) -> CompileResult<CtProgram> {
        let mut collector = Collector {
            program: self.program,
            selected: BTreeSet::new(),
            pending: VecDeque::new(),
            nodes: 0,
            work_left: limits.max_analysis_steps,
            query: true,
            limits,
        };
        if let Some(expected) = expected {
            collector.ty(expected, 0)?;
        }
        collector.expression(self.module, expression, 0)?;
        // Bound borrowed declaration trees before copying them into the CT query.
        for module in &self.program.modules {
            collector.charge(0)?;
            collector.declarations(&module.core)?;
            for import in &module.imports {
                collector.charge(0)?;
                collector.text(&import.alias)?;
            }
            for _ in &module.exports {
                collector.charge(0)?;
            }
        }
        let mut bodies = BTreeMap::new();
        while let Some((module, name)) = collector.pending.pop_front() {
            let Some(function) = find_function(self.program, module, &name) else {
                continue;
            };
            if function.foreign.is_some() {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-FORBIDDEN",
                    Some(function.span),
                    "foreign functions cannot execute in compile-time queries",
                )]);
            }
            collector.header(function)?;
            collector.block(module, &function.body, 0)?;
            let mut function = function.clone();
            strip_block(&mut function.body);
            bodies.insert((module, function.id), function);
        }
        let mut modules = Vec::new();
        for module in &self.program.modules {
            let functions: Vec<_> = module
                .core
                .functions
                .iter()
                .filter_map(|f| bodies.remove(&(module.core.module_id, f.id)))
                .collect();
            let exports: Vec<_> = module
                .exports
                .iter()
                .filter(|id| functions.iter().any(|f| f.id == **id))
                .copied()
                .collect();
            modules.push(ProgramModule {
                core: CoreModule {
                    schema_version: module.core.schema_version,
                    module_id: module.core.module_id,
                    source_fingerprint: module.core.source_fingerprint,
                    functions,
                    // The bounded copies describe types, not runtime cleanup.
                    // Runtime Drop bodies are not selected by a pure query; their
                    // names must not become unresolved executable dependencies.
                    // The original declarations (including these names) were
                    // charged above and remain intact in the runtime program.
                    // Resource operations still require independent CT approval.
                    aggregates: module
                        .core
                        .aggregates
                        .iter()
                        .map(|aggregate| {
                            let mut shape = aggregate.clone();
                            shape.teardown = None;
                            shape
                        })
                        .collect(),
                    enums: module.core.enums.clone(),
                    unsupported: Vec::new(),
                },
                imports: module.imports.clone(),
                exports,
            });
        }
        let program = CoreProgram {
            root: self.module,
            source_fingerprint: self.program.source_fingerprint,
            modules,
        };
        let mut program = zil_types::resolve_program_shapes(&program, limits).map_err(ct_errors)?;
        let normalized = Compiler {
            program: &program,
            module: self.module,
            target: self.target,
        };
        let result_type = match expected {
            Some(expected) => expected.clone(), // bounded before query traversal above
            None => normalized
                .hint(self.module, expression, 0, &mut collector)?
                .unwrap_or(Type::I32),
        };
        let mut query = expression.clone();
        strip_expr(&mut query);
        let function = CoreFunction {
            callable_return: None,
            state_parameters: Vec::new(),
            type_parameters: Vec::new(),
            callable_parameters: Vec::new(),
            result_mode: zil_ir::PassingMode::Infer,
            foreign: None,
            id: NodeId(u32::MAX),
            name: QUERY.into(),
            parameters: Vec::new(),
            return_type: result_type,
            receiver: None,
            span: expression.span,
            body: CoreBlock {
                span: expression.span,
                statements: vec![CoreStatement {
                    id: NodeId(u32::MAX - 1),
                    span: expression.span,
                    kind: CoreStatementKind::Return(Some(query)),
                }],
            },
        };
        let root = program
            .modules
            .iter_mut()
            .find(|m| m.core.module_id == self.module)
            .expect("selected query root exists");
        root.exports.push(function.id);
        root.core.functions.push(function);
        let typed = zil_types::check_program(&program, self.target, limits).map_err(ct_errors)?;
        let entry = typed
            .root_functions
            .iter()
            .find(|f| f.name == QUERY)
            .map(|f| f.function)
            .expect("synthetic query was typed");
        let plan = zil_analyze::analyze(&typed.module, limits).map_err(ct_errors)?;
        Ok(CtProgram {
            module: typed.module,
            plan,
            entry,
            max_steps: limits.max_ct_steps,
            max_call_depth: limits.max_call_depth,
            max_live_slots: limits.max_values.saturating_add(limits.max_locals),
        })
    }
}
fn ct_errors(mut errors: Vec<Diagnostic>) -> Vec<Diagnostic> {
    for d in &mut errors {
        d.phase = Phase::Ct;
        d.notes.push("while compiling a pure #(...) query".into());
    }
    errors
}
impl Compiler<'_> {
    fn enum_hint(
        &self,
        current: ModuleId,
        expression: &CoreExpr,
        depth: usize,
        shape: &mut Collector<'_>,
    ) -> CompileResult<Option<Type>> {
        let CoreExprKind::Member { object, member } = &expression.kind else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        // Raw declaration names are local; normalized project names are already
        // source-inexpressible. This is only a bounded hint: ordinary Types
        // still checks the actual constructor, variant, arity and visibility.
        for module in &self.program.modules {
            shape.charge(depth)?;
            if module.core.module_id != current && !name.starts_with('$') {
                continue;
            }
            for declaration in &module.core.enums {
                shape.charge(depth + 1)?;
                if declaration.name != *name {
                    continue;
                }
                let mut found = false;
                for (variant, fields) in &declaration.variants {
                    shape.charge(depth + 1)?;
                    shape.text(variant)?;
                    found |= variant == member;
                    for ty in fields {
                        shape.ty(ty, depth + 2)?;
                    }
                }
                if !found {
                    return Ok(None);
                }
                let owner = module.core.module_id.0;
                let digits = owner.checked_ilog10().map_or(1, |n| n as usize + 1);
                shape.text("@module:")?;
                shape.text_len(digits)?;
                shape.text(&declaration.name)?;
                let name = format!("@module{owner}:{}", declaration.name);
                return Ok(Some(Type::Enum {
                    name,
                    variants: declaration.variants.clone(),
                }));
            }
        }
        Ok(None)
    }
    fn hint(
        &self,
        module: ModuleId,
        e: &CoreExpr,
        depth: usize,
        shape: &mut Collector<'_>,
    ) -> CompileResult<Option<Type>> {
        fn copied(ty: Option<&Type>, shape: &mut Collector<'_>) -> CompileResult<Option<Type>> {
            if let Some(ty) = ty {
                shape.ty(ty, 0)?;
                Ok(Some(ty.clone()))
            } else {
                Ok(None)
            }
        }
        shape.charge(depth)?;
        Ok(match &e.kind {
            CoreExprKind::IntegerConvert { target, .. } if target.is_integer() => {
                copied(Some(target), shape)?
            }
            CoreExprKind::Ascribed { ty, .. } => copied(Some(ty), shape)?,
            CoreExprKind::Integer(n) if n.ends_with("u8") => Some(Type::U8),
            CoreExprKind::Integer(n) if n.ends_with("u16") => Some(Type::U16),
            CoreExprKind::Integer(n) if n.ends_with("i64") => Some(Type::I64),
            CoreExprKind::Integer(n) if n.ends_with("i32") => Some(Type::I32),
            // Unsuffixed literals take their context from the other operand;
            // choosing i32 here would change ordinary expression inference.
            CoreExprKind::Integer(_) => None,
            CoreExprKind::Unit => Some(Type::Unit),
            CoreExprKind::Bool(_) => Some(Type::Bool),
            CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Some(Type::Bool),
            CoreExprKind::Unary { operand, .. } | CoreExprKind::CompileTime(operand) => {
                self.hint(module, operand, depth + 1, shape)?
            }
            CoreExprKind::Binary {
                op:
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or,
                ..
            } => Some(Type::Bool),
            CoreExprKind::Binary { left, right, .. } => {
                match self.hint(module, left, depth + 1, shape)? {
                    Some(ty) => Some(ty),
                    None => self.hint(module, right, depth + 1, shape)?,
                }
            }
            CoreExprKind::Call { callee, .. } => match copied(
                resolve(self.program, module, callee)
                    .and_then(|(module, name)| find_function(self.program, module, name))
                    .map(|f| &f.return_type),
                shape,
            )? {
                Some(ty) => Some(ty),
                None => self.enum_hint(module, callee, depth + 1, shape)?,
            },
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => match self.hint(module, then_value, depth + 1, shape)? {
                Some(ty) => Some(ty),
                None => self.hint(module, else_value, depth + 1, shape)?,
            },
            CoreExprKind::Sequence { value, .. } => self.hint(module, value, depth + 1, shape)?,
            CoreExprKind::Tuple(items) => Some(Type::Tuple(
                items
                    .iter()
                    .map(|e| Ok(self.hint(module, e, depth + 1, shape)?.unwrap_or(Type::I32)))
                    .collect::<CompileResult<_>>()?,
            )),
            CoreExprKind::Array(items) => {
                let mut element = None;
                for item in items {
                    element = self.hint(module, item, depth + 1, shape)?;
                    if element.is_some() {
                        break;
                    }
                }
                let Some(length) = u32::try_from(items.len()).ok() else {
                    return Ok(None);
                };
                Some(Type::Array {
                    element: Box::new(element.unwrap_or(Type::I32)),
                    length,
                })
            }
            CoreExprKind::Record { ty, .. } => copied(Some(ty), shape)?,
            CoreExprKind::RecordUpdate { ty: Some(ty), .. } => copied(Some(ty), shape)?,
            CoreExprKind::RecordUpdate { base, .. } => self.hint(module, base, depth + 1, shape)?,
            CoreExprKind::Index { object, .. } => {
                match self.hint(module, object, depth + 1, shape)? {
                    Some(Type::Array { element, .. }) => Some(*element),
                    _ => None,
                }
            }
            CoreExprKind::Member { object, member } => {
                if let Some(ty) = self.enum_hint(module, e, depth + 1, shape)? {
                    return Ok(Some(ty));
                }
                match self.hint(module, object, depth + 1, shape)? {
                    Some(Type::Tuple(fields)) => member
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| fields.into_iter().nth(index)),
                    Some(Type::Record { fields, .. }) => fields
                        .into_iter()
                        .find(|(n, _)| n == member)
                        .map(|(_, t)| t),
                    Some(Type::Named(name)) => copied(
                        self.program
                            .modules
                            .iter()
                            .find(|m| m.core.module_id == module)
                            .and_then(|m| m.core.aggregates.iter().find(|a| a.name == name))
                            .and_then(|a| a.fields.iter().find(|f| &f.name == member))
                            .map(|f| &f.ty),
                        shape,
                    )?,
                    _ => None,
                }
            }
            _ => None,
        })
    }
}
fn find_function<'a>(
    program: &'a CoreProgram,
    module: ModuleId,
    name: &str,
) -> Option<&'a CoreFunction> {
    let module = program
        .modules
        .iter()
        .find(|m| m.core.module_id == module)?;
    let mut found = module.core.functions.iter().filter(|f| f.name == name);
    let first = found.next()?;
    found.next().is_none().then_some(first)
}
fn resolve<'a>(
    program: &CoreProgram,
    module: ModuleId,
    callee: &'a CoreExpr,
) -> Option<(ModuleId, &'a str)> {
    match &callee.kind {
        CoreExprKind::Name(name) => Some((module, name)),
        CoreExprKind::Member { object, member } => {
            let CoreExprKind::Name(alias) = &object.kind else {
                return None;
            };
            let module = program
                .modules
                .iter()
                .find(|m| m.core.module_id == module)?;
            let mut imports = module.imports.iter().filter(|i| &i.alias == alias);
            let import = imports.next()?;
            imports.next().is_none().then_some((import.module, member))
        }
        _ => None,
    }
}
// Constant-sized structure only. Names and Type trees are checked for exact
// equality by mixed_state_signature after each caller has prepaid both heads.
fn mixed_state_shape(function: &CoreFunction) -> CompileResult<()> {
    let invalid = || mixed_state_error(function);
    if function.state_parameters.len() != 1
        || function.callable_parameters.len() != 1
        || !function.type_parameters.is_empty()
        || function.callable_return.is_some()
        || function.receiver.is_some()
        || function.foreign.is_some()
    {
        return Err(invalid());
    }
    let owner = &function.state_parameters[0];
    let direct = &function.callable_parameters[0];
    if owner.bindings.len() != 1 || owner.index == direct.index {
        return Err(invalid());
    }
    for index in [owner.index, direct.index] {
        if !function
            .parameters
            .get(index as usize)
            .is_some_and(|parameter| {
                matches!(
                    parameter.mode,
                    PassingMode::Infer | PassingMode::Move | PassingMode::Give
                )
            })
        {
            return Err(invalid());
        }
    }
    let bound = &owner.bindings[0];
    if bound.generic_index != 0
        || direct.generic_index != 0
        || bound.generic_name.is_empty()
        || direct.generic_name.is_empty()
        || bound.kind == CallableKind::FunctionPointer
        || direct.kind != bound.kind
        || !(1..=2).contains(&bound.fields.len())
        || (bound.fields.len() == 2 && bound.fields[0] >= bound.fields[1])
    {
        return Err(invalid());
    }
    Ok(())
}

fn mixed_state_error(function: &CoreFunction) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Ct,
        "mixed State metadata requires one owned State, one identical F replacement and complete field selectors",
        Some(function.span),
    )]
}

// Transport consistency only: no source move/alias or concrete ownership proof.
// Call only after shape validation and a full bounded traversal of both heads.
fn mixed_state_signature(function: &CoreFunction) -> CompileResult<()> {
    let owner = &function.state_parameters[0];
    let bound = &owner.bindings[0];
    let direct = &function.callable_parameters[0];
    let state = &function.parameters[owner.index as usize].ty;
    if bound.generic_name != direct.generic_name
        || bound.parameters != direct.parameters
        || bound.result != direct.result
        || !matches!(&function.parameters[direct.index as usize].ty,
            Type::Named(name) if name == &bound.generic_name)
        || state != &function.return_type
    {
        return Err(mixed_state_error(function));
    }
    let Type::Package { fields, .. } = state else {
        return Err(mixed_state_error(function));
    };
    let mut selectors = bound.fields.iter();
    for (index, (_, ty)) in fields.iter().enumerate() {
        if matches!(ty, Type::Named(name) if name == &bound.generic_name)
            && selectors.next().copied().map(|field| field as usize) != Some(index)
        {
            return Err(mixed_state_error(function));
        }
    }
    if selectors.next().is_some() {
        return Err(mixed_state_error(function));
    }
    Ok(())
}

struct Collector<'a> {
    program: &'a CoreProgram,
    selected: BTreeSet<(ModuleId, String)>,
    pending: VecDeque<(ModuleId, String)>,
    nodes: usize,
    work_left: usize,
    // Runtime source copying needs shape bounds but grants no CT permission.
    // Actual queries always use true and reject foreign/unsafe constructors.
    query: bool,
    limits: &'a Limits,
}
impl Collector<'_> {
    fn charge(&mut self, depth: usize) -> CompileResult<()> {
        if self.nodes >= self.limits.max_nodes || depth > self.limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT reachable source graph budget exhausted",
                None,
            )]);
        }
        self.nodes += 1;
        self.work_left = self.work_left.checked_sub(1).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "CT source work budget exhausted",
                None,
            )]
        })?;
        Ok(())
    }
    fn text(&mut self, text: &str) -> CompileResult<()> {
        self.text_len(text.len())
    }
    fn text_len(&mut self, length: usize) -> CompileResult<()> {
        self.work_left = self.work_left.checked_sub(length).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "CT source text copy budget exhausted",
                None,
            )]
        })?;
        Ok(())
    }
    fn parameters(&mut self, parameters: &[Type], result: &Type) -> CompileResult<()> {
        if parameters.len() > self.limits.max_locals {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT source parameter budget exhausted",
                None,
            )]);
        }
        for parameter in parameters {
            self.ty(parameter, 0)?;
        }
        self.ty(result, 0)
    }
    // Independently preflight all retained State data before the collector
    // clones even an unused declaration into the runtime normalized program.
    fn state_header(&mut self, function: &CoreFunction) -> CompileResult<()> {
        if !function.callable_parameters.is_empty() {
            mixed_state_shape(function)?;
            self.charge(0)?;
            let bound = &function.state_parameters[0].bindings[0];
            self.charge(0)?;
            self.text(&bound.generic_name)?;
            self.parameters(&bound.parameters, &bound.result)?;
            for _ in &bound.fields {
                self.charge(0)?;
            }
            return Ok(());
        }
        let invalid = || {
            vec![Diagnostic::unsupported(
                Phase::Ct,
                "independent State metadata requires one owner and two ordered field bindings",
                Some(function.span),
            )]
        };
        if function.state_parameters.len() != 1
            || !function.type_parameters.is_empty()
            || !function.callable_parameters.is_empty()
            || function.callable_return.is_some()
            || function.receiver.is_some()
            || function.foreign.is_some()
        {
            return Err(invalid());
        }
        let state = &function.state_parameters[0];
        if state.bindings.len() != 2
            || !function
                .parameters
                .get(state.index as usize)
                .is_some_and(|p| {
                    matches!(
                        p.mode,
                        PassingMode::Infer | PassingMode::Move | PassingMode::Give
                    )
                })
        {
            return Err(invalid());
        }
        self.charge(0)?;
        for (index, bound) in state.bindings.iter().enumerate() {
            if bound.generic_index as usize != index
                || bound.generic_name.is_empty()
                || bound.kind == CallableKind::FunctionPointer
                || bound.fields.len() != 1
            {
                return Err(invalid());
            }
            self.charge(0)?;
            self.text(&bound.generic_name)?;
            self.parameters(&bound.parameters, &bound.result)?;
            for _ in &bound.fields {
                self.charge(0)?;
            }
        }
        if state.bindings[0].fields[0] == state.bindings[1].fields[0] {
            return Err(invalid());
        }
        Ok(())
    }
    fn header(&mut self, function: &CoreFunction) -> CompileResult<()> {
        let header_work = self.work_left;
        self.charge(0)?;
        self.text(&function.name)?;
        if !function.state_parameters.is_empty() {
            if self.query {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-FORBIDDEN",
                    Some(function.span),
                    "independent State templates cannot execute in compile-time queries",
                )]);
            }
            self.state_header(function)?;
        }
        if !function.type_parameters.is_empty() {
            if self.query {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-FORBIDDEN",
                    Some(function.span),
                    "ordinary StoreElement templates cannot execute in compile-time queries",
                )]);
            }
            if function.type_parameters.len() != 1
                || function.type_parameters[0].index != 0
                || function.type_parameters[0].name.is_empty()
                || !function.callable_parameters.is_empty()
                || !function.state_parameters.is_empty()
                || function.callable_return.is_some()
                || function.receiver.is_some()
                || function.foreign.is_some()
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "ordinary StoreElement template metadata requires one lexical binder",
                    Some(function.span),
                )]);
            }
            self.charge(0)?;
            self.text(&function.type_parameters[0].name)?;
        }
        if function.parameters.len() > self.limits.max_locals {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT source parameter budget exhausted",
                Some(function.span),
            )]);
        }
        self.ty(&function.return_type, 0)?;
        for parameter in &function.parameters {
            self.text(&parameter.name)?;
            self.ty(&parameter.ty, 0)?;
        }
        if let Some(receiver) = &function.receiver {
            self.ty(&receiver.ty, 0)?;
        }
        if let Some(contract) = &function.callable_return {
            self.charge(0)?;
            self.parameters(&contract.parameters, &contract.result)?;
        }
        for contract in &function.callable_parameters {
            self.charge(0)?;
            self.text(&contract.generic_name)?;
            self.parameters(&contract.parameters, &contract.result)?;
        }
        if !function.state_parameters.is_empty() && !function.callable_parameters.is_empty() {
            // Both raw heads and actual parameter/return types were bounded.
            // Prepay the full equality and selector scan before using them.
            self.text_len(header_work - self.work_left)?;
            mixed_state_signature(function)?;
        }
        if let Some(foreign) = &function.foreign {
            self.charge(0)?;
            self.text(&foreign.abi)?;
            self.text(&foreign.symbol)?;
            for _ in &foreign.noescape_parameters {
                self.charge(0)?;
            }
        }
        Ok(())
    }
    fn declarations(&mut self, core: &CoreModule) -> CompileResult<()> {
        for aggregate in &core.aggregates {
            self.charge(0)?;
            self.text(&aggregate.name)?;
            if let Some(teardown) = &aggregate.teardown {
                self.text(teardown)?;
            }
            for field in &aggregate.fields {
                self.text(&field.name)?;
                self.ty(&field.ty, 1)?;
            }
        }
        for enumeration in &core.enums {
            self.charge(0)?;
            self.text(&enumeration.name)?;
            for (name, fields) in &enumeration.variants {
                self.charge(1)?;
                self.text(name)?;
                for field in fields {
                    self.ty(field, 2)?;
                }
            }
        }
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        self.charge(depth)?;
        match ty {
            Type::Tuple(fields) => {
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                self.text(name)?;
                for (name, field) in fields {
                    self.text(name)?;
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Enum { name, variants } => {
                self.text(name)?;
                for (name, fields) in variants {
                    self.charge(depth + 1)?;
                    self.text(name)?;
                    for field in fields {
                        self.ty(field, depth + 2)?;
                    }
                }
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => self.ty(element, depth + 1)?,
            Type::ErrorUnion { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                if parameters.len() > self.limits.max_locals {
                    return Err(vec![Diagnostic::limit(
                        Phase::Ct,
                        "CT source parameter budget exhausted",
                        None,
                    )]);
                }
                for p in parameters {
                    self.ty(p, depth + 1)?;
                }
                self.ty(result, depth + 1)?;
            }
            Type::Named(name) | Type::Unsupported(name) => self.text(name)?,
            _ => {}
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &CorePattern, depth: usize) -> CompileResult<()> {
        self.charge(depth)?;
        match &pattern.kind {
            CorePatternKind::Tuple(fields) => {
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Variant {
                ty,
                variant,
                fields,
            } => {
                self.ty(ty, depth + 1)?;
                self.text(variant)?;
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Name(name) | CorePatternKind::Integer(name) => self.text(name)?,
            _ => {}
        }
        Ok(())
    }
    fn expression(&mut self, module: ModuleId, e: &CoreExpr, depth: usize) -> CompileResult<()> {
        self.charge(depth)?;
        match &e.kind {
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                if self.query {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-FORBIDDEN",
                        Some(e.span),
                        "type applications cannot execute in compile-time queries",
                    )]);
                }
                let static_callee = matches!(&callee.kind, CoreExprKind::Name(_))
                    || matches!(&callee.kind, CoreExprKind::Member { object, .. }
                        if matches!(&object.kind, CoreExprKind::Name(_)));
                if type_arguments.len() != 1 || !static_callee {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "type application requires one complete type and a static function callee",
                        Some(e.span),
                    )]);
                }
                self.ty(&type_arguments[0], depth + 1)?;
                self.expression(module, callee, depth + 1)?;
            }
            CoreExprKind::Literal(_) if self.query => {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-FORBIDDEN",
                    Some(e.span),
                    "static foreign literals cannot enter a compile-time query",
                )]);
            }
            CoreExprKind::Call { callee, .. }
                if self.query
                    && matches!(&callee.kind, CoreExprKind::Name(name) if matches!(name.as_str(), "c_callback" | "bytes_new" | "bytes_reserve" | "bytes_truncate" | "bytes_push" | "bytes_len" | "bytes_capacity" | "bytes_view" | "bytes_view_mut")) =>
            {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-FORBIDDEN",
                    Some(e.span),
                    "static callback or dynamic byte storage cannot enter a compile-time query",
                )]);
            }
            CoreExprKind::Literal(Constant::WideString(units)) => {
                self.nodes = self
                    .nodes
                    .checked_add(units.len())
                    .filter(|n| *n <= self.limits.max_nodes)
                    .ok_or_else(|| {
                        vec![Diagnostic::limit(
                            Phase::Ct,
                            "static UTF16 source node budget exhausted",
                            Some(e.span),
                        )]
                    })?;
                self.work_left = self.work_left.checked_sub(units.len()).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "static UTF16 source copy budget exhausted",
                        Some(e.span),
                    )]
                })?;
            }
            CoreExprKind::IntegerConvert { target: ty, value }
            | CoreExprKind::Ascribed { ty, value } => {
                self.ty(ty, depth + 1)?;
                self.expression(module, value, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                if self.query
                    && let Some((module, name)) = resolve(self.program, module, callee)
                {
                    self.text(name)?;
                    if self.selected.insert((module, name.into())) {
                        if self.selected.len() >= self.limits.max_functions {
                            return Err(vec![Diagnostic::limit(
                                Phase::Ct,
                                "CT reachable function budget exhausted",
                                Some(e.span),
                            )]);
                        }
                        self.text(name)?;
                        self.pending.push_back((module, name.into()));
                    }
                }
                self.expression(module, callee, depth + 1)?;
                for a in arguments {
                    self.expression(module, a, depth + 1)?;
                }
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand) => self.expression(module, operand, depth + 1)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.expression(module, left, depth + 1)?;
                self.expression(module, right, depth + 1)?;
            }
            CoreExprKind::Member { object, member } => {
                self.text(member)?;
                self.expression(module, object, depth + 1)?;
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for e in values {
                    self.expression(module, e, depth + 1)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.expression(module, object, depth + 1)?;
                self.expression(module, index, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expression(module, object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expression(module, endpoint, depth + 1)?;
                }
            }
            CoreExprKind::Record { ty, fields } => {
                self.ty(ty, depth + 1)?;
                for (name, e) in fields {
                    self.text(name)?;
                    self.expression(module, e, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.ty(ty, depth + 1)?;
                }
                self.expression(module, base, depth + 1)?;
                for (name, e) in fields {
                    self.text(name)?;
                    self.expression(module, e, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (name, e) in bindings {
                    self.text(name)?;
                    self.expression(module, e, depth + 1)?;
                }
                self.expression(module, value, depth + 1)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(module, condition, depth + 1)?;
                self.expression(module, then_value, depth + 1)?;
                self.expression(module, else_value, depth + 1)?;
            }
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
            } => {
                if parameters.len() > self.limits.max_locals {
                    return Err(vec![Diagnostic::limit(
                        Phase::Ct,
                        "CT closure parameter budget exhausted",
                        Some(e.span),
                    )]);
                }
                for parameter in parameters {
                    self.charge(depth + 1)?;
                    self.text(&parameter.name)?;
                    if let Some(ty) = &parameter.ty {
                        self.ty(ty, depth + 1)?;
                    }
                }
                if let Some(ty) = return_type {
                    self.ty(ty, depth + 1)?;
                }
                self.block(module, body, depth + 1)?;
            }
            CoreExprKind::Integer(text)
            | CoreExprKind::Name(text)
            | CoreExprKind::ReceiverMember(text)
            | CoreExprKind::String(text)
            | CoreExprKind::Unsupported(text) => self.text(text)?,
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, module: ModuleId, block: &CoreBlock, depth: usize) -> CompileResult<()> {
        self.charge(depth)?;
        for statement in &block.statements {
            self.charge(depth + 1)?;
            if let CoreStatementKind::IfLet { pattern, .. }
            | CoreStatementKind::WhileLet { pattern, .. } = &statement.kind
            {
                self.pattern(pattern, depth + 1)?;
            }
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    ty,
                    initializer,
                    ..
                } => {
                    self.text(name)?;
                    if let Some(ty) = ty {
                        self.ty(ty, depth + 1)?;
                    }
                    if let Some(e) = initializer {
                        self.expression(module, e, depth + 1)?;
                    }
                }
                CoreStatementKind::Return(Some(e)) => self.expression(module, e, depth + 1)?,
                CoreStatementKind::Assign { target, value } => {
                    self.expression(module, target, depth + 1)?;
                    self.expression(module, value, depth + 1)?;
                }
                CoreStatementKind::Expression(e) => self.expression(module, e, depth + 1)?,
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                }
                | CoreStatementKind::IfLet {
                    value: condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.expression(module, condition, depth + 1)?;
                    self.block(module, then_body, depth + 1)?;
                    if let Some(b) = else_body {
                        self.block(module, b, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body }
                | CoreStatementKind::WhileLet {
                    value: condition,
                    body,
                    ..
                } => {
                    self.expression(module, condition, depth + 1)?;
                    self.block(module, body, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.text(name)?;
                    self.expression(module, start, depth + 1)?;
                    self.expression(module, end, depth + 1)?;
                    self.block(module, body, depth + 1)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expression(module, value, depth + 1)?;
                    for arm in arms {
                        self.pattern(&arm.pattern, depth + 1)?;
                        self.block(module, &arm.body, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(b) => self.block(module, b, depth + 1)?,
                CoreStatementKind::Unsafe(b) if self.query => {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-FORBIDDEN",
                        Some(b.span),
                        "unsafe regions are outside the pure compile-time capability set",
                    )]);
                }
                CoreStatementKind::Unsafe(b) => self.block(module, b, depth + 1)?,
                CoreStatementKind::Unsupported(text) => self.text(text)?,
                _ => {}
            }
        }
        Ok(())
    }
}
// Traversals above established the bounds before these cloning-time rewrites.
// Nested # inside an already compile-time body selects the same pure stage.
fn strip_expr(e: &mut CoreExpr) {
    if let CoreExprKind::CompileTime(inner) = &mut e.kind {
        strip_expr(inner);
        e.kind = std::mem::replace(&mut inner.kind, CoreExprKind::Unit);
        return;
    }
    match &mut e.kind {
        CoreExprKind::TypeApply { callee, .. } => strip_expr(callee),
        CoreExprKind::Ascribed { value, .. } | CoreExprKind::IntegerConvert { value, .. } => {
            strip_expr(value)
        }
        CoreExprKind::Unary { operand, .. } => strip_expr(operand),
        CoreExprKind::Binary { left, right, .. } => {
            strip_expr(left);
            strip_expr(right);
        }
        CoreExprKind::Call { callee, arguments } => {
            strip_expr(callee);
            for e in arguments {
                strip_expr(e);
            }
        }
        CoreExprKind::Member { object, .. } => strip_expr(object),
        CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
            for e in values {
                strip_expr(e);
            }
        }
        CoreExprKind::Index { object, index } => {
            strip_expr(object);
            strip_expr(index);
        }
        CoreExprKind::Slice { object, start, end } => {
            strip_expr(object);
            for endpoint in [start, end].into_iter().flatten() {
                strip_expr(endpoint);
            }
        }
        CoreExprKind::Record { fields, .. } => {
            for (_, e) in fields {
                strip_expr(e);
            }
        }
        CoreExprKind::RecordUpdate { base, fields, .. } => {
            strip_expr(base);
            for (_, e) in fields {
                strip_expr(e);
            }
        }
        CoreExprKind::Sequence { bindings, value } => {
            for (_, e) in bindings {
                strip_expr(e);
            }
            strip_expr(value);
        }
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            strip_expr(condition);
            strip_expr(then_value);
            strip_expr(else_value);
        }
        CoreExprKind::Closure { body, .. } => strip_block(body),
        _ => {}
    }
}
fn strip_block(block: &mut CoreBlock) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            CoreStatementKind::Binding { initializer, .. }
            | CoreStatementKind::Return(initializer) => {
                if let Some(e) = initializer {
                    strip_expr(e);
                }
            }
            CoreStatementKind::Assign { target, value } => {
                strip_expr(target);
                strip_expr(value);
            }
            CoreStatementKind::Expression(e) => strip_expr(e),
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            }
            | CoreStatementKind::IfLet {
                value: condition,
                then_body,
                else_body,
                ..
            } => {
                strip_expr(condition);
                strip_block(then_body);
                if let Some(b) = else_body {
                    strip_block(b);
                }
            }
            CoreStatementKind::While { condition, body }
            | CoreStatementKind::WhileLet {
                value: condition,
                body,
                ..
            } => {
                strip_expr(condition);
                strip_block(body);
            }
            CoreStatementKind::ForRange {
                start, end, body, ..
            } => {
                strip_expr(start);
                strip_expr(end);
                strip_block(body);
            }
            CoreStatementKind::Match { value, arms } => {
                strip_expr(value);
                for arm in arms {
                    strip_block(&mut arm.body);
                }
            }
            CoreStatementKind::Block(b) | CoreStatementKind::Unsafe(b) => strip_block(b),
            _ => {}
        }
    }
}

pub(super) fn single(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<CoreModule> {
    // This adapter used to copy the whole input before the public CT walker
    // could inspect it. Borrow all runtime shapes first; this pass executes and
    // authorizes nothing, and actual queries still use Collector { query: true }.
    let empty = CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: Vec::new(),
    };
    let mut shape = Collector {
        program: &empty,
        selected: BTreeSet::new(),
        pending: VecDeque::new(),
        nodes: 0,
        work_left: limits.max_analysis_steps,
        query: false,
        limits,
    };
    if core.functions.len() > limits.max_functions {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "source function budget exhausted",
            None,
        )]);
    }
    shape.declarations(core)?;
    for feature in &core.unsupported {
        shape.charge(0)?;
        shape.text(&feature.feature)?;
    }
    for function in &core.functions {
        shape.header(function)?;
    }
    for function in &core.functions {
        shape.block(core.module_id, &function.body, 0)?;
    }
    let program = CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core: core.clone(),
            imports: Vec::new(),
            exports: core.functions.iter().map(|f| f.id).collect(),
        }],
    };
    let compiler = Compiler {
        program: &program,
        module: core.module_id,
        target,
    };
    zil_ct::evaluate_source_constants_with_compiler(core, target, limits, None, &compiler)
        .map(|(core, _)| core)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ResultCategory, SourceUnit};
    use zil_ct::SourceCompiler;

    #[test]
    fn ct122_original_teardown_is_bounded_before_query_copy() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "ct122-budget.zil",
            "resource Token{code:i32}impl Drop for Token{fn drop(self:Self){}}fn pure()->i64{return 42}",
        );
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let mut core = zil_expand::expand(&source, &ast).unwrap();
        // Raw declaration input: an unused runtime symbol is still bounded.
        let original_name = "x".repeat(4096);
        core.aggregates[0].teardown = Some(original_name.clone());
        let span = core.aggregates[0].span;
        let query = CoreExpr {
            id: NodeId(800),
            span,
            kind: CoreExprKind::Call {
                callee: Box::new(CoreExpr {
                    id: NodeId(801),
                    span,
                    kind: CoreExprKind::Name("pure".into()),
                }),
                arguments: vec![],
            },
        };
        let program = CoreProgram {
            root: ModuleId(0),
            source_fingerprint: core.source_fingerprint,
            modules: vec![ProgramModule {
                core,
                imports: vec![],
                exports: vec![],
            }],
        };
        let target = TargetProfile::default();
        let compiler = Compiler {
            program: &program,
            module: ModuleId(0),
            target: &target,
        };
        let errors = compiler
            .compile(
                &query,
                Some(&Type::I64),
                &Limits {
                    max_analysis_steps: 256,
                    ..Limits::default()
                },
            )
            .unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].phase, Phase::Ct);
        assert_eq!(errors[0].message, "CT source text copy budget exhausted");
        let raw = compiler
            .compile(&query, Some(&Type::I64), &Limits::default())
            .unwrap();
        let verified = zil_verify::verify_ct_program(raw, &target, &Limits::default()).unwrap();
        assert_eq!(
            zil_ct::evaluate_program(&verified).unwrap().value,
            zil_ct::CompileTimeValue::Scalar(Constant::I64(42))
        );
        assert_eq!(
            program.modules[0].core.aggregates[0].teardown.as_ref(),
            Some(&original_name)
        );
    }

    #[test]
    fn ct_reachable_pattern_trees_are_bounded_before_clone() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "ct-pattern-budget.zil",
            "fn entry()->i32 {if let n=1 {return n} return 0}",
        );
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &ast).unwrap();
        let span = core.functions[0].span;
        let value = CoreExpr {
            id: NodeId(800),
            span,
            kind: CoreExprKind::Integer("1".into()),
        };
        let empty = CoreBlock {
            span,
            statements: vec![],
        };
        let query = CoreExpr {
            id: NodeId(801),
            span,
            kind: CoreExprKind::Call {
                callee: Box::new(CoreExpr {
                    id: NodeId(802),
                    span,
                    kind: CoreExprKind::Name("entry".into()),
                }),
                arguments: vec![],
            },
        };
        for typed in [false, true] {
            let mut pattern = CorePattern {
                kind: CorePatternKind::Wildcard,
                span,
            };
            if typed {
                let mut ty = Type::I32;
                for _ in 0..40 {
                    ty = Type::Tuple(vec![ty]);
                }
                pattern.kind = CorePatternKind::Variant {
                    ty,
                    variant: "N".into(),
                    fields: vec![],
                };
            } else {
                for _ in 0..40 {
                    pattern = CorePattern {
                        kind: CorePatternKind::Tuple(vec![pattern]),
                        span,
                    };
                }
            }
            for kind in [
                CoreStatementKind::IfLet {
                    pattern: pattern.clone(),
                    value: value.clone(),
                    then_body: empty.clone(),
                    else_body: None,
                },
                CoreStatementKind::WhileLet {
                    pattern: pattern.clone(),
                    value: value.clone(),
                    body: empty.clone(),
                },
                CoreStatementKind::Match {
                    value: value.clone(),
                    arms: vec![CoreMatchArm {
                        pattern: pattern.clone(),
                        body: empty.clone(),
                        span,
                    }],
                },
            ] {
                let mut core = core.clone();
                core.functions[0].body.statements[0].kind = kind;
                let program = CoreProgram {
                    root: core.module_id,
                    source_fingerprint: core.source_fingerprint,
                    modules: vec![ProgramModule {
                        core,
                        imports: vec![],
                        exports: vec![],
                    }],
                };
                let target = TargetProfile::default();
                let compiler = Compiler {
                    program: &program,
                    module: ModuleId(0),
                    target: &target,
                };
                let errors = compiler
                    .compile(
                        &query,
                        Some(&Type::I32),
                        &Limits {
                            max_call_depth: 8,
                            ..Limits::default()
                        },
                    )
                    .unwrap_err();
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                assert_eq!(errors[0].phase, Phase::Ct);
                assert_eq!(
                    errors[0].message,
                    "CT reachable source graph budget exhausted"
                );
            }
        }
    }

    #[test]
    fn ct202_collector_rejects_bare_template_and_indirect_wrapper_before_execution() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "template202-ct.zil",
            "fn pure()->i64{return 42}fn wrapper()->i64{return pure()}",
        );
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &ast).unwrap();
        let span = core.functions[0].span;
        let mut program = CoreProgram {
            root: ModuleId(0),
            source_fingerprint: core.source_fingerprint,
            modules: vec![ProgramModule {
                core,
                imports: vec![],
                exports: vec![],
            }],
        };
        let target = TargetProfile::default();
        let query = |name: &str| CoreExpr {
            id: NodeId(800),
            span,
            kind: CoreExprKind::Call {
                callee: Box::new(CoreExpr {
                    id: NodeId(801),
                    span,
                    kind: CoreExprKind::Name(name.into()),
                }),
                arguments: vec![],
            },
        };
        let control = Compiler {
            program: &program,
            module: ModuleId(0),
            target: &target,
        }
        .compile(&query("wrapper"), Some(&Type::I64), &Limits::default())
        .unwrap();
        let verified = zil_verify::verify_ct_program(control, &target, &Limits::default()).unwrap();
        assert_eq!(
            zil_ct::evaluate_program(&verified).unwrap().value,
            zil_ct::CompileTimeValue::Scalar(Constant::I64(42))
        );
        program.modules[0]
            .core
            .functions
            .iter_mut()
            .find(|f| f.name == "pure")
            .unwrap()
            .type_parameters = vec![CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        }];
        for name in ["pure", "wrapper"] {
            let errors = Compiler {
                program: &program,
                module: ModuleId(0),
                target: &target,
            }
            .compile(&query(name), Some(&Type::I64), &Limits::default())
            .unwrap_err();
            assert_eq!(errors[0].phase, Phase::Ct);
            assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
        }
        let mut wrapped = query("pure");
        let CoreExprKind::Call { callee, .. } = &mut wrapped.kind else {
            panic!()
        };
        **callee = CoreExpr {
            id: NodeId(802),
            span,
            kind: CoreExprKind::TypeApply {
                callee: callee.clone(),
                type_arguments: vec![Type::Named("T".into())],
            },
        };
        let errors = Compiler {
            program: &program,
            module: ModuleId(0),
            target: &target,
        }
        .compile(&wrapped, Some(&Type::I64), &Limits::default())
        .unwrap_err();
        assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
    }

    #[test]
    fn ct202_collector_bounds_unused_template_names_before_whole_program_copy() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "template202-shape.zil",
            "fn f()->i32{return 42}",
        );
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let mut core = zil_expand::expand(&source, &ast).unwrap();
        core.functions[0].type_parameters = vec![CoreTypeParameter {
            index: 0,
            name: "T".repeat(8192),
            bound: CoreTypeBound::StoreElement,
        }];
        let errors = single(
            &core,
            &TargetProfile::default(),
            &Limits {
                max_analysis_steps: 256,
                ..Limits::default()
            },
        )
        .unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].phase, Phase::Ct);
        core.functions[0].type_parameters[0].index = 1;
        let errors = single(&core, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::Unsupported);
        assert_eq!(errors[0].phase, Phase::Ct);
    }
}
