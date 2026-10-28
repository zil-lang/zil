//! Bounded concrete instantiation of a retained callable template. Bodies are
//! checked in the defining module; consumers receive a normal direct call.
use super::*;

#[path = "helper_flow.rs"]
mod flow;
#[path = "helper_plan.rs"]
pub(super) mod plan;
use std::collections::BTreeSet;
use std::rc::Rc;
use zil_base::{ModuleId, NodeId};

pub(super) type TemplateKey = (ModuleId, NodeId);
pub(super) type Tables = BTreeMap<ModuleId, BTreeMap<String, Signature>>;
/// The finite two-type profile uses a fixed ordered binding vector. Empty
/// entries are part of identity; runtime values never enter this type key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Bindings(pub [Option<ClosureId>; 2]);
pub(super) type Instance = (TemplateKey, Bindings);
impl Bindings {
    fn bind(&mut self, index: u32, id: ClosureId, span: Span) -> R<()> {
        let slot = self.0.get_mut(index as usize).ok_or_else(|| {
            unsupported(span, "callable type index is outside the finite profile")
        })?;
        if slot.is_some_and(|prior| prior != id) {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "one callable type parameter requires one concrete closure identity",
            ));
        }
        *slot = Some(id);
        Ok(())
    }
    pub(super) fn get(self, index: u32, span: Span) -> R<ClosureId> {
        self.0
            .get(index as usize)
            .and_then(|id| *id)
            .ok_or_else(|| unsupported(span, "concrete callable type binding is absent"))
    }
    fn label(self) -> String {
        self.0
            .iter()
            .flatten()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join("_")
    }
}

#[derive(Clone)]
pub(super) struct Template {
    pub function: Rc<CoreFunction>,
    pub snapshot_work: usize,
    pub store_shapes: bool,
    pub context: Rc<Context>,
    pub public: bool,
}
pub(super) struct Context {
    pub module: ModuleId,
    pub imports: BTreeMap<String, ModuleId>,
    pub enums: BTreeMap<String, Type>,
}
#[derive(Clone)]
pub(super) struct Export {
    pub module: ModuleId,
    pub public: bool,
    pub signature: ScalarFunction,
}

pub(super) fn parameter_mode(ty: &Type, name: &str) -> Option<Option<bool>> {
    match ty {
        Type::Named(n) if n == name => Some(None),
        Type::Address { mutable, inner } if matches!(inner.as_ref(), Type::Named(n) if n == name) => {
            Some(Some(*mutable))
        }
        _ => None,
    }
}

pub(super) fn validate_template(function: &CoreFunction) -> R<()> {
    let bounds = &function.callable_parameters;
    if bounds.is_empty() {
        return Ok(());
    }
    let reject = || {
        unsupported(
            function.span,
            "one or two finite nonescaping callable roles, a single lending role, or a bounded same-F owning State constructor are implemented",
        )
    };
    if bounds.len() > 2
        || function.foreign.is_some()
        || function.receiver.is_some()
        || bounds.windows(2).any(|pair| pair[0].index >= pair[1].index)
    {
        return Err(reject());
    }
    let owned_state = callable_fields::template(function);
    if !(function.return_type.is_copy_value()
        || (bounds.len() == 1 && lending::supported_result(&function.return_type))
        || (owned_state
            && callable_fields::placeholder_package(
                &function.return_type,
                &bounds[0].generic_name,
            )))
    {
        return Err(reject());
    }
    let mut present = [false; 2];
    for bound in bounds {
        if bound.generic_index >= 2
            || bound.generic_name.is_empty()
            || bound.kind == CallableKind::FunctionPointer
            || !closures::supported_signature(&bound.parameters, &bound.result)
            || (bounds.len() > 1 && !owned_state && !bound.result.is_copy_value())
        {
            return Err(reject());
        }
        present[bound.generic_index as usize] = true;
        let parameter = function
            .parameters
            .get(bound.index as usize)
            .ok_or_else(reject)?;
        let mode = parameter_mode(&parameter.ty, &bound.generic_name);
        if mode.is_none()
            && !(owned_state
                && callable_fields::placeholder_package(&parameter.ty, &bound.generic_name))
        {
            return Err(reject());
        }
        for prior in bounds.iter().take_while(|prior| prior.index < bound.index) {
            if (prior.generic_index == bound.generic_index)
                != (prior.generic_name == bound.generic_name)
                || (prior.generic_index == bound.generic_index
                    && (prior.kind != bound.kind
                        || prior.parameters != bound.parameters
                        || prior.result != bound.result
                        || parameter_mode(
                            &function.parameters[prior.index as usize].ty,
                            &prior.generic_name,
                        ) != mode))
            {
                return Err(reject());
            }
        }
        match (bound.kind, mode.unwrap_or(None)) {
            (CallableKind::FnMut, None) if !owned_state => {
                return Err(unsupported(
                    parameter.span,
                    "by-value FnMut helper needs an explicit mutable binding; use &mut F in this slice",
                ));
            }
            (CallableKind::FnMut, Some(false)) | (CallableKind::FnOnce, Some(_)) => {
                return Err(error(
                    "E-CLOSURE-MODE",
                    parameter.span,
                    "callable bound requires its matching mutable or owned environment passing mode",
                ));
            }
            _ => {}
        }
    }
    if !present[0] {
        return Err(reject());
    }
    if owned_state
        && bounds.len() == 2
        && function.parameters.iter().enumerate().any(|(index, parameter)| {
            !bounds.iter().any(|bound| bound.index as usize == index)
                && !(parameter.ty.is_copy_value()
                    || matches!(&parameter.ty, Type::Managed(_) if resources::supported(&parameter.ty)))
        })
    {
        return Err(unsupported(
            function.span,
            "two-input owning State constructors require finite Copy or direct Managed ordinary parameters",
        ));
    }
    if function.parameters.iter().enumerate().any(|(i,p)|
        !bounds.iter().any(|bound| bound.index as usize == i)
        && !(p.ty.is_copy_value()
            || matches!(&p.ty, Type::Address { inner, .. } if inner.is_copy_value()
                || (function.return_type.is_copy_value() && matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value())))
            || packages::supported(&p.ty)
            || matches!(&p.ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
            || matches!(p.ty, Type::Managed(_))))
    { return Err(reject()); }
    Ok(())
}

pub(super) fn install_single(
    core: &CoreModule,
    signatures: &BTreeMap<String, Signature>,
    enums: &BTreeMap<String, Type>,
    state: &mut closures::State,
    limits: &Limits,
) -> R<()> {
    if core
        .functions
        .iter()
        .all(|f| f.callable_parameters.is_empty() && f.type_parameters.is_empty())
    {
        return Ok(());
    }
    let span = core.functions[0].span;
    reserve_signatures(signatures, state, limits, span)?;
    for (name, ty) in enums {
        let work = inference::type_snapshot_work(ty, span, limits)?;
        state.snapshot(work.saturating_mul(2).saturating_add(name.len()), span)?;
    }
    let context = Rc::new(Context {
        module: core.module_id,
        imports: BTreeMap::new(),
        enums: enums.clone(),
    });
    state.helper_tables = Rc::new(BTreeMap::from([(core.module_id, signatures.clone())]));
    install(core, context, state, limits)
}

pub(super) fn reserve_signatures(
    signatures: &BTreeMap<String, Signature>,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<()> {
    state.snapshot(signatures.len(), span)?;
    for (name, signature) in signatures {
        let work = inference::signature_snapshot_work(name, signature, span, limits)?;
        // One bounded borrowed walk and one ensuing deep table clone.
        state.snapshot(work.saturating_mul(2), span)?;
    }
    Ok(())
}

/// Templates can run while opaque factories are being checked, but their lookup
/// snapshot is updated after each dependency-ordered factory completes.
/// Only concrete result slots change; no second deep table clone is required.
pub(super) fn resolve_factory_result(
    state: &mut closures::State,
    module: ModuleId,
    name: &str,
    result: &Type,
    span: Span,
) -> R<()> {
    if state.helper_tables.is_empty() {
        return Ok(());
    }
    if !matches!(result, Type::Closure(_)) {
        return Err(unsupported(span, "factory result identity is unresolved"));
    }
    state.snapshot(name.len().saturating_add(1), span)?;
    // instantiate_helper holds the only extra Rc in its local stack frame.
    // It finishes synchronously before either entry point resolves results;
    // no template, pending closure or typed function stores that Rc.
    let tables = Rc::get_mut(&mut state.helper_tables)
        .expect("helper lookup snapshots are local to synchronous instantiation");
    let signature = tables
        .get_mut(&module)
        .and_then(|table| table.get_mut(name))
        .ok_or_else(|| unsupported(span, "factory signature is absent from helper lookup"))?;
    signature.result = result.clone();
    Ok(())
}

pub(super) fn install(
    core: &CoreModule,
    context: Rc<Context>,
    state: &mut closures::State,
    limits: &Limits,
) -> R<()> {
    for function in &core.functions {
        if !function.callable_parameters.is_empty() || !function.type_parameters.is_empty() {
            validate_template(function)?;
            store_templates::validate_header(function)?;
            let (snapshot_work, store_shapes) = inference::helper_snapshot_work(function, limits)?;
            state.snapshot(snapshot_work.saturating_mul(2), function.span)?;
            state.helper_templates.insert(
                (core.module_id, function.id),
                Template {
                    function: Rc::new(function.clone()),
                    snapshot_work,
                    store_shapes,
                    public: state.helper_tables[&context.module][&function.name].public,
                    context: context.clone(),
                },
            );
        }
    }
    Ok(())
}

pub(super) fn compatible(actual: CallableKind, requested: CallableKind) -> bool {
    matches!(
        (actual, requested),
        (
            CallableKind::Fn,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) | (
            CallableKind::FnMut,
            CallableKind::FnMut | CallableKind::FnOnce
        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
    )
}

impl Builder<'_, '_> {
    pub(super) fn call_helper(
        &mut self,
        signature: &Signature,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if signature.callable.is_empty() {
            return Err(unsupported(
                span,
                "template signature has no callable roles",
            ));
        }
        let owned_state = callable_fields::signature(signature);
        if !owned_state && let Some(expected) = expected {
            self.same(&signature.result, expected, span)?;
        }
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "callable helper argument count differs",
            ));
        }
        let mut staged = Vec::new();
        let mut environments = Bindings::default();
        self.closures.snapshot(
            arguments.len().saturating_add(signature.callable.len()),
            span,
        )?;
        for (index, (argument, ty)) in arguments.iter().zip(&signature.parameters).enumerate() {
            let (value, actual) = if let Some(bound) = signature
                .callable
                .iter()
                .find(|bound| bound.index as usize == index)
            {
                let mode = if owned_state {
                    None
                } else {
                    parameter_mode(ty, &bound.generic_name)
                        .ok_or_else(|| unsupported(span, "invalid callable template parameter"))?
                };
                let (mut value, mut actual) = if owned_state && signature.callable.len() == 1 {
                    self.owned_array_input(argument, None)?
                } else if mode.is_none() {
                    // A constructor receives ordinary values. Structural Copy
                    // environments may be copied into distinct inputs; owning
                    // environments transfer once through the ordinary path.
                    self.transfer_expr(argument, None)?
                } else {
                    self.expr(argument, None)?
                };
                let closure = if owned_state {
                    let pattern_work =
                        inference::type_snapshot_work(ty, argument.span, self.limits)?;
                    let actual_work =
                        inference::type_snapshot_work(&actual, argument.span, self.limits)?;
                    self.closures
                        .snapshot(pattern_work.saturating_add(actual_work), argument.span)?;
                    callable_fields::environment(ty, &actual, &bound.generic_name, argument.span)?
                } else {
                    match (&actual, mode) {
                        (Type::Closure(id), None) => *id,
                        (Type::Address { mutable, inner }, Some(wanted)) if !wanted || *mutable => {
                            let Type::Closure(id) = inner.as_ref() else {
                                return Err(error(
                                    "E-TYPE-MISMATCH",
                                    argument.span,
                                    "helper requires an address to a concrete environment",
                                ));
                            };
                            let id = *id;
                            let wanted_type = Type::Address {
                                mutable: wanted,
                                inner: Box::new(Type::Closure(id)),
                            };
                            value = self.value(
                                InstructionKind::ReborrowAddress {
                                    address: value,
                                    mutable: wanted,
                                },
                                wanted_type.clone(),
                                argument.span,
                            )?;
                            actual = wanted_type;
                            id
                        }
                        _ => {
                            return Err(error(
                                "E-TYPE-MISMATCH",
                                argument.span,
                                "helper requires its declared concrete environment passing mode",
                            ));
                        }
                    }
                };
                if owned_state {
                    let mut work = self.limits.max_analysis_steps;
                    retained::self_contained(
                        &self.closures.pending,
                        closure,
                        &mut work,
                        argument.span,
                    )?;
                    self.closures
                        .snapshot(self.limits.max_analysis_steps - work, argument.span)?;
                }
                let pending = self
                    .closures
                    .pending
                    .get_mut(closure.0 as usize)
                    .ok_or_else(|| unsupported(span, "absent concrete closure"))?;
                if !compatible(pending.callable_kind(), bound.kind) {
                    return Err(error(
                        "E-CLOSURE-MODE",
                        argument.span,
                        "concrete environment does not implement the requested callable mode",
                    ));
                }
                if pending.inference.arity() != bound.parameters.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        argument.span,
                        "concrete environment signature differs from callable bound",
                    ));
                }
                for (i, ty) in bound.parameters.iter().enumerate() {
                    pending.inference.argument(i, ty, argument.span)?;
                }
                pending
                    .inference
                    .expect_result(&bound.result, argument.span)?;
                self.materialize_closure(closure)?;
                environments.bind(bound.generic_index, closure, argument.span)?;
                (value, actual)
            } else {
                let (value, actual) = self.argument_expr(argument, ty)?;
                self.same(&actual, ty, argument.span)?;
                (value, actual)
            };
            let mut prepared = self.stage_argument(value, &actual, argument.span)?;
            if owned_state
                && signature.callable.len() == 2
                && matches!(actual, Type::Closure(_))
                && prepared.1.is_none()
            {
                // A structural Copy environment is first copied to its own
                // real temporary. Publication into an owning State still
                // takes that distinct position; a Load view is not an owned
                // input and the independent verifier must keep rejecting it.
                let local = self.temp(actual.clone(), argument.span)?;
                self.store(local, value, argument.span)?;
                prepared.1 = Some(local);
            }
            staged.push(prepared);
        }
        let instance = self.instantiate_helper(
            signature.template.expect("template key"),
            environments,
            span,
        )?;
        let result = if owned_state {
            self.closures
                .snapshot(self.closures.helper_functions.len(), span)?;
            self.closures
                .helper_functions
                .iter()
                .find(|f| f.id == instance)
                .ok_or_else(|| unsupported(span, "concrete owning helper result is unavailable"))?
                .return_type
                .clone()
        } else {
            signature.result.clone()
        };
        if let Some(expected) = expected {
            self.same(&result, expected, span)?;
        }
        let values = self.commit_arguments(staged, span)?;
        let instruction = InstructionKind::Call {
            function: instance,
            arguments: values,
        };
        if result == Type::Unit {
            self.emit(instruction, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            let value = self.value(instruction, result.clone(), span)?;
            Ok((value, result))
        }
    }

    fn instantiate_helper(
        &mut self,
        key: TemplateKey,
        bindings: Bindings,
        span: Span,
    ) -> R<FunctionId> {
        self.closures
            .snapshot(plan::tree_work(self.closures.helper_instances.len()), span)?;
        if let Some(id) = self.closures.helper_instances.get(&(key, bindings)) {
            return Ok(*id);
        }
        if !self.closures.active_helpers.is_empty() {
            return Err(unsupported(
                span,
                "actual helper instance is not available from the finite dependency plan",
            ));
        }
        self.instantiate_plan(plan::Node::Callable((key, bindings)), span)?;
        self.closures
            .snapshot(plan::tree_work(self.closures.helper_instances.len()), span)?;
        self.closures
            .helper_instances
            .get(&(key, bindings))
            .copied()
            .ok_or_else(|| unsupported(span, "completed helper instance is absent from its plan"))
    }

    pub(super) fn instantiate_plan(&mut self, root: plan::Node, span: Span) -> R<()> {
        if !self.closures.active_helpers.is_empty() {
            return Err(unsupported(
                span,
                "actual template instance is absent from the finite dependency plan",
            ));
        }
        let order = plan::order(root, self.closures, self.limits, span)?;
        for node in order {
            match node {
                plan::Node::Callable((key, bindings)) => {
                    self.instantiate_ready_helper(key, bindings, span)?;
                }
                plan::Node::Store(index) => {
                    self.instantiate_ready_store(index, span)?;
                }
            }
        }
        Ok(())
    }

    fn instantiate_ready_helper(
        &mut self,
        key: TemplateKey,
        bindings: Bindings,
        span: Span,
    ) -> R<FunctionId> {
        let template = self
            .closures
            .helper_templates
            .get(&key)
            .cloned()
            .ok_or_else(|| unsupported(span, "callable template definition is absent"))?;
        self.closures.snapshot(template.snapshot_work, span)?;
        let mut function = template.function.as_ref().clone();
        let bounds = &template.function.callable_parameters;
        let owned_state = callable_fields::template(&function);
        if owned_state {
            let bound = &bounds[0];
            callable_fields::specialize(
                &mut function,
                &bound.generic_name,
                bindings.get(bound.generic_index, span)?,
            );
            function.callable_parameters.clear();
        } else {
            for bound in bounds {
                let mode = parameter_mode(
                    &function.parameters[bound.index as usize].ty,
                    &bound.generic_name,
                )
                .ok_or_else(|| unsupported(span, "invalid concrete helper parameter"))?;
                let concrete = Type::Closure(bindings.get(bound.generic_index, span)?);
                function.parameters[bound.index as usize].ty = if let Some(mutable) = mode {
                    Type::Address {
                        mutable,
                        inner: Box::new(concrete),
                    }
                } else {
                    concrete
                };
            }
        }
        self.closures.snapshot(24, span)?;
        let label = bindings.label();
        let id = self.closures.allocate_function(span)?;
        function.name = format!("{}::<env{}>", function.name, label);
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
                "recursive callable State instantiation requires a finite declared contract",
            ));
        }
        let mut builder = Builder::new(
            &function,
            id,
            &tables[&template.context.module],
            self.limits,
            self.closures,
        )?;
        if owned_state {
            builder.closures.snapshot(1, span)?;
            let bound = &bounds[0];
            builder.state_callable_mode =
                Some((bindings.get(bound.generic_index, span)?, bound.kind));
        }
        builder.imports = Some(&imports);
        builder.enums = Some(&template.context.enums);
        let typed_result = builder.finish(&function.body);
        self.closures.active_helpers.remove(&key);
        let mut typed = typed_result?;
        // Public export names are module-local. The generated implementation
        // label includes definition identity, independently of caller order.
        typed.name = format!(
            "@module{}:helper{}:env{}",
            template.context.module.0, function.id.0, label
        );
        if !owned_state {
            self.closures.snapshot(bounds.len(), span)?;
            let mut roles = Vec::new();
            for bound in bounds {
                let closure = bindings.get(bound.generic_index, span)?;
                let capture_count = self.closures.pending[closure.0 as usize].captures.len();
                self.closures
                    .snapshot(capture_count.saturating_mul(4), span)?;
                let mut hold = Vec::new();
                let mut call = Vec::new();
                let mut writes = Vec::new();
                for index in 0..capture_count {
                    let capture = &self.closures.pending[closure.0 as usize].captures[index];
                    let work = inference::type_snapshot_work(&capture.ty, span, self.limits)?;
                    self.closures.snapshot(work.saturating_mul(2), span)?;
                    let ty =
                        self.closures.pending[closure.0 as usize].captures[index].stored_type();
                    if matches!(ty, Type::Address { .. }) {
                        hold.push(index as u32);
                    }
                    if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
                        call.push(index as u32);
                    }
                    if matches!(ty, Type::Address { mutable: true, .. } | Type::Desc(_)) {
                        writes.push(index as u32);
                    }
                }
                roles.push(CallableRole {
                    parameter: bound.index,
                    closure,
                    kind: bound.kind,
                    hold,
                    call,
                    writes,
                    cleanup: if matches!(
                        &function.parameters[bound.index as usize].ty,
                        Type::Address { .. }
                    ) {
                        CallableCleanup::Borrowed
                    } else {
                        CallableCleanup::Owned
                    },
                });
            }
            typed.callable_contract = Some(CallableParameterContract {
                roles,
                traces: flow::contract(&typed, bounds, self.closures, self.limits, span)?,
            });
        }
        self.closures.helper_exports.push(Export {
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
        self.closures.helper_instances.insert((key, bindings), id);
        Ok(id)
    }

    /// Call a borrowed helper parameter at its actual environment address.
    pub(super) fn call_borrowed_helper_parameter(
        &mut self,
        callee: &CoreExpr,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let (address, ty) = if let CoreExprKind::Name(name) = &callee.kind
            && let Some(local) = self.lookup(name)
            && matches!(&self.function.locals[local.0 as usize].ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)))
        {
            let ty = self.function.locals[local.0 as usize].ty.clone();
            (
                self.value(InstructionKind::Load(local), ty.clone(), span)?,
                ty,
            )
        } else {
            self.expr(callee, None)?
        };
        let Type::Address { mutable, inner } = ty else {
            return Err(unsupported(
                span,
                "borrowed helper environment is not an address",
            ));
        };
        let Type::Closure(id) = *inner else {
            return Err(unsupported(
                span,
                "borrowed helper environment has no concrete identity",
            ));
        };
        let parameter_bound = self
            .helper_bounds
            .iter()
            .find(|bound| {
                matches!(&callee.kind, CoreExprKind::Name(name)
                if self.lookup(name) == self.function.parameters.get(bound.index as usize).copied())
            })
            .cloned();
        let (kind, parameters, result) = if let Some(bound) = parameter_bound {
            (bound.kind, bound.parameters, bound.result)
        } else {
            let pending = &mut self.closures.pending[id.0 as usize];
            let (parameters, result) = pending.inference.resolve(false, span)?;
            (pending.callable_kind(), parameters, result)
        };
        if kind == CallableKind::FnOnce || (kind == CallableKind::FnMut && !mutable) {
            return Err(error(
                "E-CLOSURE-MODE",
                span,
                "borrowed environment lacks the requested call mode",
            ));
        }
        if !compatible(self.closures.pending[id.0 as usize].callable_kind(), kind) {
            return Err(error(
                "E-CLOSURE-MODE",
                span,
                "concrete closure cannot satisfy the helper call mode",
            ));
        }
        if let Some(expected) = expected {
            self.same(&result, expected, span)?;
        }
        if arguments.len() != parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "callback argument count differs",
            ));
        }
        let mut values = Vec::new();
        for (argument, parameter) in arguments.iter().zip(&parameters) {
            let (value, actual) = self.argument_expr(argument, parameter)?;
            self.same(&actual, parameter, argument.span)?;
            values.push(value);
        }
        let instruction = InstructionKind::CallClosureBorrowed {
            callee: address,
            kind,
            arguments: values,
        };
        if result == Type::Unit {
            self.emit(instruction, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            let value = self.value(instruction, result.clone(), span)?;
            Ok((value, result))
        }
    }
}

fn environment_type(ty: &Type) -> bool {
    matches!(ty, Type::Closure(_))
        || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)))
}

/// Private value provenance, not hidden type identity. Factory calls may
/// produce the very same ClosureId as the supplied parameter. A monotone union
/// deliberately rejects a slot that can carry both origins.
fn environment_origins(
    function: &TypedFunction,
    bounds: &[CoreCallableParameter],
    state: &mut closures::State,
    span: Span,
) -> R<(Vec<u8>, Vec<u8>)> {
    state.snapshot(
        function.locals.len().saturating_add(function.values.len()),
        span,
    )?;
    let mut locals = vec![0u8; function.locals.len()];
    let mut values = vec![0u8; function.values.len()];
    state.snapshot(bounds.len(), span)?;
    for (role, bound) in bounds.iter().enumerate() {
        let parameter = function.parameters[bound.index as usize];
        locals[parameter.0 as usize] = 1 << role;
    }
    let tables = Rc::clone(&state.helper_tables);
    for block in &function.blocks {
        state.snapshot(1, span)?;
        for instruction in &block.instructions {
            state.snapshot(1, instruction.span)?;
            if matches!(instruction.kind, InstructionKind::MakeClosure { .. }) {
                return Err(unsupported(
                    instruction.span,
                    "helper-local nested environments require an additional contract",
                ));
            }
            let Some(result) = instruction.result else {
                continue;
            };
            let ty = &function.values[result.0 as usize];
            if !environment_type(ty) {
                continue;
            }
            values[result.0 as usize] = match &instruction.kind {
                InstructionKind::Load(_)
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. } => 0,
                InstructionKind::Call {
                    function: callee, ..
                } => {
                    let mut factory = false;
                    'modules: for table in tables.values() {
                        state.snapshot(1, instruction.span)?;
                        for signature in table.values() {
                            state.snapshot(1, instruction.span)?;
                            if signature.id == Some(*callee) {
                                factory = signature.callable_return.is_some()
                                    && matches!(signature.result, Type::Closure(_))
                                    && &signature.result == ty;
                                break 'modules;
                            }
                        }
                    }
                    if factory { 4 } else { 8 }
                }
                _ => 8,
            };
        }
    }
    loop {
        let mut changed = false;
        for block in &function.blocks {
            state.snapshot(1, span)?;
            for instruction in &block.instructions {
                state.snapshot(1, instruction.span)?;
                let origin = match instruction.kind {
                    InstructionKind::Load(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => locals[local.0 as usize],
                    InstructionKind::ReborrowAddress { address, .. } => values[address.0 as usize],
                    InstructionKind::Store { local, value } => {
                        let prior = locals[local.0 as usize];
                        locals[local.0 as usize] |= values[value.0 as usize];
                        changed |= prior != locals[local.0 as usize];
                        0
                    }
                    _ => 0,
                };
                if let Some(result) = instruction.result {
                    let prior = values[result.0 as usize];
                    values[result.0 as usize] |= origin;
                    changed |= prior != values[result.0 as usize];
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok((locals, values))
}

struct Forwarded {
    position: usize,
    /// Child actual parameter position -> current actual parameter position.
    roles: Vec<(u32, u32)>,
}
fn forwarded_contract(
    function: &TypedFunction,
    instruction: &Instruction,
    origins: &[u8],
    bounds: &[CoreCallableParameter],
    state: &mut closures::State,
) -> R<Option<Forwarded>> {
    let InstructionKind::Call {
        function: callee,
        arguments,
    } = &instruction.kind
    else {
        return Ok(None);
    };
    state.snapshot(arguments.len(), instruction.span)?;
    if !arguments
        .iter()
        .any(|value| environment_type(&function.values[value.0 as usize]))
    {
        return Ok(None);
    }
    state.snapshot(state.helper_functions.len(), instruction.span)?;
    let position = state
        .helper_functions
        .iter()
        .position(|f| f.id == *callee)
        .ok_or_else(|| {
            unsupported(
                instruction.span,
                "nested helper has no completed callable contract",
            )
        })?;
    let contract = state.helper_functions[position]
        .callable_contract
        .as_ref()
        .ok_or_else(|| unsupported(instruction.span, "nested helper contract is absent"))?;
    let role_count = contract.roles.len();
    state.snapshot(
        role_count.saturating_mul(3).saturating_add(arguments.len()),
        instruction.span,
    )?;
    let contract = state.helper_functions[position]
        .callable_contract
        .as_ref()
        .unwrap();
    let mut roles = Vec::with_capacity(role_count);
    for (parameter, value) in arguments.iter().enumerate() {
        if !environment_type(&function.values[value.0 as usize]) {
            continue;
        }
        let child = contract
            .roles
            .iter()
            .find(|role| role.parameter as usize == parameter)
            .ok_or_else(|| {
                unsupported(instruction.span, "nested helper environment role is absent")
            })?;
        let mask = origins[value.0 as usize];
        let caller = if mask == 1 {
            bounds.first()
        } else if mask == 2 {
            bounds.get(1)
        } else {
            None
        }
        .ok_or_else(|| {
            unsupported(
                instruction.span,
                "nested helper environment is not one actual supplied role",
            )
        })?;
        if child.kind != caller.kind {
            return Err(unsupported(
                instruction.span,
                "nested helper role has a different declared callable kind",
            ));
        }
        roles.push((child.parameter, caller.index));
    }
    if roles.len() != role_count {
        return Err(unsupported(
            instruction.span,
            "nested helper role mapping is incomplete",
        ));
    }
    Ok(Some(Forwarded { position, roles }))
}
