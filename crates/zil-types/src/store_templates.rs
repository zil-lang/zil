//! Complete, bounded ordinary StoreElement instances. No callable role is
//! manufactured for T, and a retained template has no concrete FunctionId.
use super::*;

pub(super) struct Instance {
    pub key: helpers::TemplateKey,
    pub actual: Type,
    pub work: usize,
    pub span: Span,
    pub function: Option<FunctionId>,
}

pub(super) fn validate_header(function: &CoreFunction) -> R<()> {
    if function.type_parameters.is_empty() {
        return Ok(());
    }
    if function.type_parameters.len() != 1
        || function.type_parameters[0].index != 0
        || function.type_parameters[0].name.is_empty()
        || !function.callable_parameters.is_empty()
        || !function.state_parameters.is_empty()
        || function.callable_return.is_some()
        || function.foreign.is_some()
        || function.receiver.is_some()
        || matches!(function.return_type, Type::Opaque { .. })
    {
        return Err(error(
            "E-TYPE-PARAMETERS",
            function.span,
            "ordinary templates require one index-zero StoreElement parameter and no callable, foreign or receiver interface",
        ));
    }
    Ok(())
}

/// An ordinary binder is opaque in its own definition. This relation permits
/// transfer and the existing direct error alternatives, without any fields,
/// scalar operations, Copy capability, or callable identity for T.
pub(super) fn symbolic_type(ty: &Type, name: &str) -> bool {
    let parameter = |ty: &Type| matches!(ty, Type::Named(actual) if actual == name);
    parameter(ty)
        || matches!(ty, Type::ErrorUnion { error, result }
        if (parameter(error) || error.is_copy_value())
            && (parameter(result) || result.is_copy_value()))
}

/// Only the finite written positions admitted by the value-inferred profile.
/// The caller already reserved the signature/type trees before this query.
pub(super) fn has_parameter(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Named(n) => n == name,
        Type::Managed(inner) | Type::Desc(inner) => match inner.as_ref() {
            Type::OwnedStore { element } => has_parameter(element, name),
            _ => false,
        },
        Type::ErrorUnion { error, result } => {
            has_parameter(error, name) || has_parameter(result, name)
        }
        _ => false,
    }
}

pub(super) fn infer_argument(
    pattern: &Type,
    actual: &Type,
    name: &str,
    inferred: &mut Option<Type>,
    limits: &Limits,
    span: Span,
    charge: &mut impl FnMut(usize) -> R<()>,
) -> R<()> {
    let pattern_work = inference::type_snapshot_work(pattern, span, limits)?;
    let actual_work = inference::type_snapshot_work(actual, span, limits)?;
    charge(pattern_work.saturating_add(actual_work).saturating_mul(2))?;
    if matches!(pattern, Type::Named(n) if n == name) {
        if let Some(previous) = inferred {
            charge(inference::type_snapshot_work(previous, span, limits)?)?;
            if previous != actual {
                return Err(error(
                    "E-TYPE-ARGUMENT-CONFLICT",
                    span,
                    "value arguments constrain the ordinary parameter to different complete types",
                ));
            }
        } else {
            *inferred = Some(actual.clone());
        }
        return Ok(());
    }
    match (pattern, actual) {
        (Type::Managed(wanted) | Type::Desc(wanted), Type::Managed(got) | Type::Desc(got)) => {
            if let (Type::OwnedStore { element: wanted }, Type::OwnedStore { element: got }) =
                (wanted.as_ref(), got.as_ref())
            {
                return infer_argument(wanted, got, name, inferred, limits, span, charge);
            }
        }
        (
            Type::ErrorUnion {
                error: a,
                result: b,
            },
            Type::ErrorUnion {
                error: c,
                result: d,
            },
        ) => {
            infer_argument(a, c, name, inferred, limits, span, charge)?;
            return infer_argument(b, d, name, inferred, limits, span, charge);
        }
        _ => {}
    }
    if has_parameter(pattern, name) {
        return Err(error(
            "E-TYPE-APPLICATION",
            span,
            "value argument does not provide the declared complete type position",
        ));
    }
    Ok(())
}

pub(super) fn intern(
    key: helpers::TemplateKey,
    actual: &Type,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<u32> {
    let work = inference::type_snapshot_work(actual, span, limits)?;
    state.snapshot(work, span)?;
    if !dynamic_store::supported_element(actual) {
        return Err(error(
            "E-STORE-TYPE-ARGUMENT",
            span,
            "StoreElement requires one complete concrete finite Store element type",
        ));
    }
    state.snapshot(state.store_instances.len(), span)?;
    for index in 0..state.store_instances.len() {
        if state.store_instances[index].key != key {
            continue;
        }
        let old_work = state.store_instances[index].work;
        state.snapshot(work.saturating_add(old_work), span)?;
        if state.store_instances[index].actual == *actual {
            return Ok(index as u32);
        }
    }
    if state.store_instances.len() >= limits.max_functions {
        return Err(Diagnostic::limit(
            Phase::Types,
            "ordinary template instance count exceeds the function bound",
            Some(span),
        ));
    }
    state.snapshot(work.saturating_add(1), span)?;
    let index = u32::try_from(state.store_instances.len()).map_err(|_| {
        Diagnostic::limit(
            Phase::Types,
            "ordinary template instance overflow",
            Some(span),
        )
    })?;
    state.store_instances.push(Instance {
        key,
        actual: actual.clone(),
        work,
        span,
        function: None,
    });
    Ok(index)
}

pub(super) fn bound_elements(state: &mut closures::State) -> R<Vec<(Type, Span)>> {
    let mut result = Vec::new();
    if state.store_instances.is_empty() {
        return Ok(result);
    }
    state.snapshot(state.store_instances.len(), Span::default())?;
    for index in 0..state.store_instances.len() {
        let record = &state.store_instances[index];
        let (work, span) = (record.work, record.span);
        state.snapshot(work, span)?;
        result.push((state.store_instances[index].actual.clone(), span));
    }
    Ok(result)
}

pub(super) fn resolve_argument(
    argument: &Type,
    bounds: &[CoreCallableParameter],
    bindings: helpers::Bindings,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<Type> {
    let work = inference::type_snapshot_work(argument, span, limits)?;
    state.snapshot(work.saturating_mul(2), span)?;
    let mut actual = argument.clone();
    if bounds
        .iter()
        .any(|bound| bound.generic_index != bounds[0].generic_index)
    {
        state.snapshot(
            work.saturating_mul(2)
                .saturating_add(bounds.len().saturating_mul(3)),
            span,
        )?;
        let replacements = bounds
            .iter()
            .map(|bound| {
                Ok((
                    bound.generic_name.as_str(),
                    bindings.get(bound.generic_index, span)?,
                ))
            })
            .collect::<R<Vec<_>>>()?;
        callable_fields::specialize_type_all(&mut actual, &replacements);
        aggregate_templates::rebind_callable_type(
            &mut actual,
            bounds,
            bindings,
            state,
            limits,
            span,
        )?;
        return Ok(actual);
    }
    for bound in bounds {
        state.snapshot(work.saturating_add(bound.generic_name.len()), span)?;
        callable_fields::specialize_type(
            &mut actual,
            &bound.generic_name,
            bindings.get(bound.generic_index, span)?,
        );
    }
    aggregate_templates::rebind_callable_type(&mut actual, bounds, bindings, state, limits, span)?;
    Ok(actual)
}

/// The original template snapshot pays for the first copy; each actual T tree
/// expansion is charged independently before copying that complete type.
pub(super) fn specialize(
    template: &helpers::Template,
    actual: &Type,
    state: &mut closures::State,
    limits: &Limits,
) -> R<CoreFunction> {
    let span = template.function.span;
    let name = &template.function.type_parameters[0].name;
    let actual_work = inference::type_snapshot_work(actual, span, limits)?;
    state.snapshot(template.snapshot_work.saturating_add(actual_work), span)?;
    let mut result = template.function.as_ref().clone();
    let mut charge = |work| state.snapshot(work, span);
    let mut substitution = Substitution {
        name,
        actual,
        actual_work,
        limits,
        span,
        charge: &mut charge,
    };
    for parameter in &mut result.parameters {
        substitution.ty(&mut parameter.ty, 0)?;
    }
    substitution.ty(&mut result.return_type, 0)?;
    substitution.block(&mut result.body, 0)?;
    let products = state.products.clone();
    aggregate_templates::rebind_function(
        &mut result,
        &[(name, actual)],
        &products,
        limits,
        span,
        &mut |n| state.snapshot(n, span),
    )?;
    result.type_parameters.clear();
    Ok(result)
}

pub(super) fn substitute_type(
    ty: &Type,
    name: &str,
    actual: &Type,
    limits: &Limits,
    products: &aggregate_templates::Products,
    span: Span,
    charge: &mut impl FnMut(usize) -> R<()>,
) -> R<Type> {
    let cost = inference::type_snapshot_work(ty, span, limits)?;
    let actual_work = inference::type_snapshot_work(actual, span, limits)?;
    charge(cost.saturating_add(actual_work))?;
    let mut result = ty.clone();
    Substitution {
        name,
        actual,
        actual_work,
        limits,
        span,
        charge,
    }
    .ty(&mut result, 0)?;
    aggregate_templates::rebind_type(
        &mut result,
        &[(name, actual)],
        products,
        limits,
        span,
        charge,
    )?;
    Ok(result)
}

impl Builder<'_, '_> {
    pub(super) fn call_inferred_type_template(
        &mut self,
        mut signature: Signature,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let work = inference::signature_snapshot_work("", &signature, span, self.limits)?;
        self.closures.snapshot(work, span)?;
        let name = signature
            .type_parameter
            .as_ref()
            .expect("ordinary template")
            .name
            .clone();
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "ordinary template value argument count differs",
            ));
        }
        self.closures
            .snapshot(arguments.len().saturating_mul(3), span)?;
        let mut inferred = None;
        let mut staged = Vec::new();
        let mut actuals = Vec::new();
        for (argument, parameter) in arguments.iter().zip(&signature.parameters) {
            let (value, actual) = if has_parameter(parameter, &name) {
                let (value, actual) = self.expr(argument, None)?;
                if matches!(parameter, Type::Desc(_)) {
                    self.resource_descriptor(value, actual, argument.span)?
                } else {
                    (value, actual)
                }
            } else {
                self.argument_expr(argument, parameter)?
            };
            infer_argument(
                parameter,
                &actual,
                &name,
                &mut inferred,
                self.limits,
                argument.span,
                &mut |work| self.closures.snapshot(work, argument.span),
            )?;
            staged.push(self.stage_argument(value, &actual, argument.span)?);
            actuals.push(actual);
        }
        let actual = inferred.ok_or_else(|| error("E-TYPE-APPLICATION", span,
            "ordinary template call has no unique complete type constraint from a value argument"))?;
        self.complete_type_signature(&mut signature, &actual, span)?;
        if let Some(expected) = expected {
            self.same(&signature.result, expected, span)?;
        }
        for ((actual, parameter), argument) in
            actuals.iter().zip(&signature.parameters).zip(arguments)
        {
            self.same(actual, parameter, argument.span)?;
        }
        let values = self.commit_arguments(staged, span)?;
        let call = self.direct_call(&signature, values, span)?;
        if signature.result == Type::Unit {
            self.emit(call, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            let value = self.value(call, signature.result.clone(), span)?;
            Ok((value, signature.result))
        }
    }

    pub(super) fn call_type_template(
        &mut self,
        target: &CoreExpr,
        type_arguments: &[Type],
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [actual] = type_arguments else {
            return Err(error(
                "E-TYPE-APPLICATION",
                span,
                "one complete type argument is required",
            ));
        };
        let borrowed = self.resolve_callee(target)?;
        let work = inference::signature_snapshot_work("", borrowed, span, self.limits)?;
        self.closures.snapshot(work, span)?;
        let mut signature = self.resolve_callee(target)?.clone();
        self.complete_type_signature(&mut signature, actual, span)?;
        if let Some(expected) = expected {
            self.same(&signature.result, expected, span)?;
        }
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "ordinary template value argument count differs",
            ));
        }
        self.closures.snapshot(arguments.len(), span)?;
        let mut staged = Vec::new();
        for (argument, ty) in arguments.iter().zip(&signature.parameters) {
            let (value, actual) = self.argument_expr(argument, ty)?;
            self.same(&actual, ty, argument.span)?;
            staged.push(self.stage_argument(value, &actual, argument.span)?);
        }
        let values = self.commit_arguments(staged, span)?;
        let call = self.direct_call(&signature, values, span)?;
        if signature.result == Type::Unit {
            self.emit(call, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            let value = self.value(call, signature.result.clone(), span)?;
            Ok((value, signature.result))
        }
    }

    fn complete_type_signature(
        &mut self,
        signature: &mut Signature,
        actual: &Type,
        span: Span,
    ) -> R<()> {
        let bound = signature.type_parameter.as_ref().ok_or_else(|| {
            error(
                "E-TYPE-APPLICATION",
                span,
                "type application target is not an ordinary StoreElement template",
            )
        })?;
        let key = signature.template.ok_or_else(|| {
            error(
                "E-TYPE-APPLICATION",
                span,
                "ordinary template identity is absent",
            )
        })?;
        let index = intern(key, actual, self.closures, self.limits, span)?;
        if self.closures.store_instances[index as usize]
            .function
            .is_none()
        {
            self.instantiate_plan(helpers::plan::Node::Store(index), span)?;
        }
        let id = self.closures.store_instances[index as usize]
            .function
            .ok_or_else(|| {
                unsupported(
                    span,
                    "completed ordinary template is absent from its instance plan",
                )
            })?;
        let products = self.closures.products.clone();
        for parameter in &mut signature.parameters {
            *parameter = substitute_type(
                parameter,
                &bound.name,
                actual,
                self.limits,
                &products,
                span,
                &mut |work| self.closures.snapshot(work, span),
            )?;
        }
        signature.result = substitute_type(
            &signature.result,
            &bound.name,
            actual,
            self.limits,
            &products,
            span,
            &mut |work| self.closures.snapshot(work, span),
        )?;
        signature.type_parameter = None;
        signature.template = None;
        signature.id = Some(id);
        Ok(())
    }

    pub(super) fn instantiate_ready_store(&mut self, index: u32, span: Span) -> R<()> {
        let record = &self.closures.store_instances[index as usize];
        let (key, work) = (record.key, record.work);
        self.closures.snapshot(work.saturating_add(1), span)?;
        let actual = self.closures.store_instances[index as usize].actual.clone();
        let template = self
            .closures
            .helper_templates
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                error(
                    "E-TYPE-APPLICATION",
                    span,
                    "ordinary template definition is absent",
                )
            })?;
        let mut function = specialize(&template, &actual, self.closures, self.limits)?;
        let mut signature_work =
            inference::type_snapshot_work(&function.return_type, span, self.limits)?;
        for parameter in &function.parameters {
            signature_work = signature_work.saturating_add(inference::type_snapshot_work(
                &parameter.ty,
                span,
                self.limits,
            )?);
        }
        self.closures
            .snapshot(signature_work.saturating_mul(8), span)?;
        let mut errors = Vec::new();
        signature_errors(&function, &mut errors);
        if let Some(error) = errors.into_iter().next() {
            return Err(error);
        }
        let id = self.closures.allocate_function(span)?;
        self.closures
            .snapshot(function.name.len().saturating_add(32), span)?;
        function.name = format!("{}::<type{}>", function.name, index);
        let tables = self.closures.helper_tables.clone();
        for alias in template.context.imports.keys() {
            self.closures
                .snapshot(alias.len().saturating_add(1), span)?;
        }
        let imports = program::Imports {
            aliases: template.context.imports.clone(),
            signatures: &tables,
        };
        if !self.closures.active_helpers.insert(key) {
            return Err(unsupported(
                span,
                "recursive ordinary template has no finite instance plan",
            ));
        }
        let result = (|| {
            let mut builder = Builder::new(
                &function,
                id,
                &tables[&template.context.module],
                self.limits,
                self.closures,
            )?;
            builder.imports = Some(&imports);
            builder.enums = Some(&template.context.enums);
            builder.finish(&function.body)
        })();
        self.closures.active_helpers.remove(&key);
        let mut typed = result?;
        self.closures
            .snapshot(signature_work.saturating_add(128), span)?;
        typed.name = format!(
            "@module{}:type-template{}:instance{}",
            key.0.0, key.1.0, index
        );
        // Ordinary complete T is a value type, never a synthetic callable role.
        debug_assert!(typed.callable_contract.is_none());
        self.closures.helper_exports.push(helpers::Export {
            module: template.context.module,
            public: template.public,
            signature: ScalarFunction {
                name: function.name,
                function: id,
                parameters: function
                    .parameters
                    .iter()
                    .map(|p| (p.mode, p.ty.clone()))
                    .collect(),
                result: function.return_type,
                result_mode: function.result_mode,
            },
        });
        self.closures.helper_functions.push(typed);
        self.closures.store_instances[index as usize].function = Some(id);
        Ok(())
    }
}

struct Substitution<'a, C> {
    name: &'a str,
    actual: &'a Type,
    actual_work: usize,
    limits: &'a Limits,
    span: Span,
    charge: &'a mut C,
}
impl<C: FnMut(usize) -> R<()>> Substitution<'_, C> {
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "ordinary template substitution depth exceeded",
                Some(self.span),
            ));
        }
        (self.charge)(1)
    }
    fn ty(&mut self, ty: &mut Type, depth: usize) -> R<()> {
        self.node(depth)?;
        match ty {
            Type::Named(name) => {
                (self.charge)(name.len().saturating_add(self.name.len()))?;
                if name == self.name {
                    (self.charge)(self.actual_work.saturating_mul(2))?;
                    // Check the combined wrapper depth before growing the tree.
                    inference::type_snapshot_work(
                        self.actual,
                        self.span,
                        &Limits {
                            max_call_depth: self.limits.max_call_depth.saturating_sub(depth),
                            ..self.limits.clone()
                        },
                    )?;
                    *ty = self.actual.clone();
                }
            }
            Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Address { inner, .. }
            | Type::Opaque { hidden: inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::OwnedStore { element: inner } => self.ty(inner, depth + 1)?,
            Type::Tuple(fields) => {
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, field) in fields {
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for field in fields {
                        self.ty(field, depth + 1)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.ty(parameter, depth + 1)?;
                }
                self.ty(result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn expr(&mut self, expression: &mut CoreExpr, depth: usize) -> R<()> {
        self.node(depth)?;
        match &mut expression.kind {
            CoreExprKind::Ascribed { ty, value }
            | CoreExprKind::IntegerConvert { target: ty, value } => {
                self.ty(ty, depth + 1)?;
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                for argument in type_arguments {
                    self.ty(argument, depth + 1)?;
                }
                self.expr(callee, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.expr(callee, depth + 1)?;
                for argument in arguments {
                    self.expr(argument, depth + 1)?;
                }
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::Try(operand)
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Member {
                object: operand, ..
            } => self.expr(operand, depth + 1)?,
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(endpoint, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for field in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::Record { ty, fields } => {
                self.ty(ty, depth + 1)?;
                for (_, value) in fields {
                    self.expr(value, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.ty(ty, depth + 1)?;
                }
                self.expr(base, depth + 1)?;
                for (_, value) in fields {
                    self.expr(value, depth + 1)?;
                }
            }
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
            } => {
                for parameter in parameters {
                    if let Some(ty) = &mut parameter.ty {
                        self.ty(ty, depth + 1)?;
                    }
                }
                if let Some(ty) = return_type {
                    self.ty(ty, depth + 1)?;
                }
                self.block(body, depth + 1)?;
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, value) in bindings {
                    self.expr(value, depth + 1)?;
                }
                self.expr(value, depth + 1)?;
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
    fn pattern(&mut self, pattern: &mut CorePattern, depth: usize) -> R<()> {
        self.node(depth)?;
        match &mut pattern.kind {
            CorePatternKind::Variant { ty, fields, .. } => {
                self.ty(ty, depth + 1)?;
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
    fn block(&mut self, block: &mut CoreBlock, depth: usize) -> R<()> {
        self.node(depth)?;
        for statement in &mut block.statements {
            self.node(depth + 1)?;
            match &mut statement.kind {
                CoreStatementKind::Binding {
                    ty, initializer, ..
                } => {
                    if let Some(ty) = ty {
                        self.ty(ty, depth + 2)?;
                    }
                    if let Some(value) = initializer {
                        self.expr(value, depth + 2)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target, depth + 2)?;
                    self.expr(value, depth + 2)?;
                }
                CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                    self.expr(value, depth + 2)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 2)?;
                    self.block(then_body, depth + 2)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 2)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, depth + 2)?;
                    self.block(body, depth + 2)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    self.pattern(pattern, depth + 2)?;
                    self.expr(value, depth + 2)?;
                    self.block(then_body, depth + 2)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 2)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    self.pattern(pattern, depth + 2)?;
                    self.expr(value, depth + 2)?;
                    self.block(body, depth + 2)?;
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    self.expr(start, depth + 2)?;
                    self.expr(end, depth + 2)?;
                    self.block(body, depth + 2)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expr(value, depth + 2)?;
                    for arm in arms {
                        self.pattern(&mut arm.pattern, depth + 2)?;
                        self.block(&mut arm.body, depth + 2)?;
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body)
                | CoreStatementKind::Defer(body) => self.block(body, depth + 2)?,
                _ => {}
            }
        }
        Ok(())
    }
}
