//! Private admission and declared-mode transport for the one-F owning factory.
//! No entry is keyed by ClosureId: two same-type runtime positions retain their
//! own declaration. Public ownership is still carried by ordinary IR values.
use super::*;

pub(super) fn template(function: &CoreFunction) -> bool {
    function.callable_return.is_some() && !function.callable_parameters.is_empty()
}

pub(super) fn signature(signature: &Signature) -> bool {
    signature.callable_return.is_some() && !signature.callable.is_empty()
}

pub(super) fn preflight(function: &CoreFunction, work: &mut usize, limits: &Limits) -> R<()> {
    if !template(function) { return Ok(()) }
    for parameter in &function.parameters {
        // All subsequent new-profile header walks are reserved here, before
        // recursive shape predicates run or any retained template is cloned.
        let cost = inference::type_snapshot_work(&parameter.ty, parameter.span, limits)?;
        *work = work.checked_sub(cost.saturating_mul(12)).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "owning factory header work budget exceeded", Some(parameter.span))
        })?;
    }
    Ok(())
}

fn contains_environment(ty: &Type) -> bool {
    match ty {
        Type::Closure(_) | Type::Opaque { .. } => true,
        Type::Tuple(fields) => fields.iter().any(contains_environment),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields.iter().any(|(_, ty)| contains_environment(ty)),
        Type::Enum { variants, .. } => variants.iter().any(|(_, fields)| fields.iter().any(contains_environment)),
        Type::Array { element, .. } | Type::Slice { element } | Type::OwnedStore { element }
        | Type::Managed(element) | Type::Desc(element) | Type::Address { inner: element, .. } => contains_environment(element),
        Type::ErrorUnion { error, result } => contains_environment(error) || contains_environment(result),
        Type::FunctionPointer { parameters, result } | Type::ForeignCallback { parameters, result } =>
            parameters.iter().any(contains_environment) || contains_environment(result),
        _ => false,
    }
}

pub(super) fn validate(function: &CoreFunction) -> R<()> {
    let reject = || unsupported(function.span, "owning callable factory requires one direct by-value self-contained F, finite ordinary inputs and a fresh Copy-result opaque environment");
    let [input] = function.callable_parameters.as_slice() else { return Err(reject()) };
    let Some(output) = &function.callable_return else { return Err(reject()) };
    let Some(parameter) = function.parameters.get(input.index as usize) else { return Err(reject()) };
    if !function.type_parameters.is_empty()
        || !function.state_parameters.is_empty()
        || function.foreign.is_some() || function.receiver.is_some()
        || function.result_mode != PassingMode::Infer
        || input.generic_index != 0 || input.generic_name.is_empty()
        || input.kind == CallableKind::FunctionPointer || output.kind == CallableKind::FunctionPointer
        || !input.result.is_copy_value() || !output.result.is_copy_value()
        || !closures::supported_signature(&input.parameters, &input.result)
        || !closures::supported_signature(&output.parameters, &output.result)
        || helpers::parameter_mode(&parameter.ty, &input.generic_name) != Some(None)
        || !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy | PassingMode::Move)
        || function.parameters.iter().enumerate().any(|(index, parameter)| {
            index != input.index as usize && (contains_environment(&parameter.ty) || !(parameter.ty.is_copy_value()
                || matches!(&parameter.ty, Type::Managed(_) if resources::supported(&parameter.ty))
                || packages::capture_supported(&parameter.ty)))
        })
    { return Err(reject()); }
    let Type::Opaque { identity, hidden } = &function.return_type else { return Err(reject()) };
    if *identity != function.id || !matches!(hidden.as_ref(), Type::FunctionPointer { parameters, result }
        if parameters == &output.parameters && result.as_ref() == &output.result)
    { return Err(reject()); }
    Ok(())
}

#[derive(Default)]
pub(super) struct Modes {
    pub locals: BTreeMap<LocalId, CallableKind>,
    values: BTreeMap<ValueId, CallableKind>,
    pub captures: BTreeMap<u32, CallableKind>,
}

fn weaker(a: CallableKind, b: CallableKind) -> CallableKind {
    if a == CallableKind::FnOnce || b == CallableKind::FnOnce { CallableKind::FnOnce }
    else if a == CallableKind::FnMut || b == CallableKind::FnMut { CallableKind::FnMut }
    else { CallableKind::Fn }
}

impl Modes {
    pub(super) fn record(&mut self, kind: &InstructionKind, result: Option<ValueId>, state: &mut closures::State, span: Span) -> R<()> {
        state.snapshot(helpers::plan::tree_work(self.locals.len())
            .saturating_add(helpers::plan::tree_work(self.values.len()))
            .saturating_add(helpers::plan::tree_work(self.captures.len())).saturating_add(2), span)?;
        let mode = match kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
            | InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => self.locals.get(local).copied(),
            InstructionKind::ReborrowAddress { address, .. } | InstructionKind::ReadAddress { address } => self.values.get(address).copied(),
            InstructionKind::ReadCapture { index } | InstructionKind::TakeCapture { index }
            | InstructionKind::BorrowCapture { index, .. } => self.captures.get(index).copied(),
            InstructionKind::Store { local, value } => {
                if let Some(mode) = self.values.get(value).copied() {
                    self.locals.entry(*local).and_modify(|prior| *prior = weaker(*prior, mode)).or_insert(mode);
                }
                None
            }
            _ => None,
        };
        if let (Some(result), Some(mode)) = (result, mode) { self.values.insert(result, mode); }
        Ok(())
    }
}

impl Builder<'_, '_> {
    pub(super) fn factory_declared_kind(&mut self, expression: &CoreExpr) -> R<Option<CallableKind>> {
        let Some(modes) = &self.factory_modes else { return Ok(None) };
        self.closures.snapshot(helpers::plan::tree_work(modes.locals.len()), expression.span)?;
        if let CoreExprKind::Name(name) = &expression.kind {
            Ok(self.lookup(name).and_then(|local| modes.locals.get(&local)).copied())
        } else { Ok(None) }
    }

    pub(super) fn factory_value_kind(&mut self, value: ValueId, span: Span) -> R<Option<CallableKind>> {
        let Some(modes) = &self.factory_modes else { return Ok(None) };
        self.closures.snapshot(helpers::plan::tree_work(modes.values.len()), span)?;
        Ok(modes.values.get(&value).copied())
    }

    pub(super) fn factory_require_value(&mut self, value: ValueId, requested: CallableKind, span: Span) -> R<()> {
        if self.factory_value_kind(value, span)?.is_some_and(|declared| !helpers::compatible(declared, requested)) {
            return Err(error("E-CLOSURE-MODE", span, "factory input declaration does not provide the requested callable capability"));
        }
        Ok(())
    }
}
