//! Bounded concrete instantiation of a retained callable template. Bodies are
//! checked in the defining module; consumers receive a normal direct call.
use super::*;

#[path = "helper_flow.rs"]
mod flow;
#[path = "helper_plan.rs"]
mod plan;
use std::collections::BTreeSet;
use std::rc::Rc;
use zil_base::{ModuleId, NodeId};

pub(super) type TemplateKey = (ModuleId, NodeId);
pub(super) type Tables = BTreeMap<ModuleId, BTreeMap<String, Signature>>;

#[derive(Clone)]
pub(super) struct Template {
    pub function: Rc<CoreFunction>,
    pub snapshot_work: usize,
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
    let Some(bound) = &function.callable_parameter else {
        return Ok(());
    };
    let reject = || {
        unsupported(
            function.span,
            "one finite Copy callable signature, optionally with Copy-address parameters and a Copy result, and one nonescaping environment parameter are implemented",
        )
    };
    let owned_state = callable_fields::template(function);
    if function.foreign.is_some()
        || function.receiver.is_some()
        || bound.generic_name.is_empty()
        || bound.kind == CallableKind::FunctionPointer
        || !closures::supported_signature(&bound.parameters, &bound.result)
        || !(function.return_type.is_copy_value()
            || lending::supported_result(&function.return_type)
            || (owned_state
                && callable_fields::placeholder_package(
                    &function.return_type,
                    &bound.generic_name,
                )))
    {
        return Err(reject());
    }
    let parameter = function
        .parameters
        .get(bound.index as usize)
        .ok_or_else(reject)?;
    let mode = parameter_mode(&parameter.ty, &bound.generic_name);
    if (mode.is_none()
        && !(owned_state
            && callable_fields::placeholder_package(&parameter.ty, &bound.generic_name)))
        || function.parameters.iter().enumerate().any(|(i, p)| {
            i != bound.index as usize
                && !(p.ty.is_copy_value()
                    || matches!(&p.ty, Type::Address { inner, .. } if inner.is_copy_value())
                    || packages::supported(&p.ty)
                    || matches!(&p.ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
                    || matches!(p.ty, Type::Managed(_)))
        })
    {
        return Err(reject());
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
        .all(|f| f.callable_parameter.is_none())
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
        if function.callable_parameter.is_some() {
            validate_template(function)?;
            let snapshot_work = inference::helper_snapshot_work(function, limits)?;
            state.snapshot(snapshot_work.saturating_mul(2), function.span)?;
            state.helper_templates.insert(
                (core.module_id, function.id),
                Template {
                    function: Rc::new(function.clone()),
                    snapshot_work,
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
        let bound = signature.callable.as_ref().expect("template signature");
        if self
            .helper_bound
            .as_ref()
            .is_some_and(|caller| caller.kind != bound.kind)
        {
            return Err(unsupported(
                span,
                "nested callable helpers require the same declared callable kind",
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
        let mut environment = None;
        for (index, (argument, ty)) in arguments.iter().zip(&signature.parameters).enumerate() {
            let (value, actual) = if index == bound.index as usize {
                let mode = if owned_state {
                    None
                } else {
                    parameter_mode(ty, &bound.generic_name)
                        .ok_or_else(|| unsupported(span, "invalid callable template parameter"))?
                };
                let (mut value, mut actual) = if owned_state {
                    self.owned_array_input(argument, None)?
                } else if mode.is_none() {
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
                environment = Some(closure);
                (value, actual)
            } else {
                let (value, actual) = self.argument_expr(argument, ty)?;
                self.same(&actual, ty, argument.span)?;
                (value, actual)
            };
            staged.push(self.stage_argument(value, &actual, argument.span)?);
        }
        let closure =
            environment.ok_or_else(|| unsupported(span, "missing concrete helper environment"))?;
        let instance =
            self.instantiate_helper(signature.template.expect("template key"), closure, span)?;
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
        closure: ClosureId,
        span: Span,
    ) -> R<FunctionId> {
        self.closures
            .snapshot(plan::tree_work(self.closures.helper_instances.len()), span)?;
        if let Some(id) = self.closures.helper_instances.get(&(key, closure)) {
            return Ok(*id);
        }
        if !self.closures.active_helpers.is_empty() {
            return Err(unsupported(
                span,
                "actual helper instance is not available from the finite dependency plan",
            ));
        }
        let order = plan::order((key, closure), self.closures, self.limits, span)?;
        for (key, closure) in order {
            self.instantiate_ready_helper(key, closure, span)?;
        }
        self.closures
            .snapshot(plan::tree_work(self.closures.helper_instances.len()), span)?;
        self.closures
            .helper_instances
            .get(&(key, closure))
            .copied()
            .ok_or_else(|| unsupported(span, "completed helper instance is absent from its plan"))
    }

    fn instantiate_ready_helper(
        &mut self,
        key: TemplateKey,
        closure: ClosureId,
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
        let bound = function
            .callable_parameter
            .as_ref()
            .expect("validated template")
            .clone();
        let owned_state = callable_fields::template(&function);
        let mode = parameter_mode(
            &function.parameters[bound.index as usize].ty,
            &bound.generic_name,
        )
        .unwrap_or(None);
        if owned_state {
            callable_fields::specialize(&mut function, &bound.generic_name, closure);
            function.callable_parameter = None;
        } else {
            let concrete = Type::Closure(closure);
            function.parameters[bound.index as usize].ty = if let Some(mutable) = mode {
                Type::Address {
                    mutable,
                    inner: Box::new(concrete),
                }
            } else {
                concrete
            };
        }
        let id = self.closures.allocate_function(span)?;
        function.name = format!("{}::<env{}>", function.name, closure.0);
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
        builder.imports = Some(&imports);
        builder.enums = Some(&template.context.enums);
        let typed_result = builder.finish(&function.body);
        self.closures.active_helpers.remove(&key);
        let mut typed = typed_result?;
        // Public export names are module-local. The generated implementation
        // label includes definition identity, independently of caller order.
        typed.name = format!(
            "@module{}:helper{}:env{}",
            template.context.module.0, function.id.0, closure.0
        );
        let capture_count = self.closures.pending[closure.0 as usize].captures.len();
        self.closures.snapshot(capture_count, span)?;
        let mut hold = Vec::new();
        let mut call = Vec::new();
        let mut writes = Vec::new();
        for index in 0..capture_count {
            let capture = &self.closures.pending[closure.0 as usize].captures[index];
            let work = inference::type_snapshot_work(&capture.ty, span, self.limits)?;
            self.closures.snapshot(work.saturating_mul(2), span)?;
            let ty = self.closures.pending[closure.0 as usize].captures[index].stored_type();
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
        if !owned_state {
            typed.callable_contract = Some(CallableParameterContract {
                parameter: bound.index,
                closure,
                kind: bound.kind,
                hold,
                call,
                writes,
                cleanup: if mode.is_some() {
                    CallableCleanup::Borrowed
                } else {
                    CallableCleanup::Owned
                },
                traces: flow::contract(&typed, &bound, self.closures, self.limits, span)?,
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
        self.closures.helper_instances.insert((key, closure), id);
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
            .helper_bound
            .as_ref()
            .filter(|bound| {
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
    parameter: LocalId,
    state: &mut closures::State,
    span: Span,
) -> R<(Vec<u8>, Vec<u8>)> {
    state.snapshot(
        function.locals.len().saturating_add(function.values.len()),
        span,
    )?;
    let mut locals = vec![0u8; function.locals.len()];
    let mut values = vec![0u8; function.values.len()];
    locals[parameter.0 as usize] = 1;
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
                            if signature.id == *callee {
                                factory = signature.callable_return.is_some()
                                    && matches!(signature.result, Type::Closure(_))
                                    && &signature.result == ty;
                                break 'modules;
                            }
                        }
                    }
                    if factory { 2 } else { 4 }
                }
                _ => 4,
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

fn forwarded_contract(
    function: &TypedFunction,
    instruction: &Instruction,
    origins: &[u8],
    kind: CallableKind,
    state: &mut closures::State,
) -> R<Option<usize>> {
    let InstructionKind::Call {
        function: callee,
        arguments,
    } = &instruction.kind
    else {
        return Ok(None);
    };
    state.snapshot(arguments.len(), instruction.span)?;
    let mut environment = None;
    for (index, value) in arguments.iter().enumerate() {
        if environment_type(&function.values[value.0 as usize])
            && environment.replace((index, *value)).is_some()
        {
            return Err(unsupported(
                instruction.span,
                "nested helper forwarding requires exactly one environment",
            ));
        }
    }
    let Some((parameter, value)) = environment else {
        return Ok(None);
    };
    if origins[value.0 as usize] != 1 {
        return Err(unsupported(
            instruction.span,
            "nested helper environment is not the actual supplied parameter",
        ));
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
    if contract.parameter as usize != parameter || contract.kind != kind {
        return Err(unsupported(
            instruction.span,
            "nested helper parameter or declared callable kind differs",
        ));
    }
    Ok(Some(position))
}
