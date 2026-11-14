//! Choose a real executable ABI from the checked original declaration. The
//! Core source's explicit Some/None annotation remains unchanged.
use super::emission::{Ledger, Origin};
use crate::*;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-ABI", Span::default(), message)
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original ABI selection work exceeded", None)
    })?;
    Ok(())
}
type Key = (
    zil_base::ModuleId,
    zil_base::NodeId,
    Option<zil_base::NodeId>,
);
pub(super) type Choices = std::collections::BTreeMap<Key, CallKind>;
pub(super) struct SourceOrigin {
    origin: Origin,
    scope: Option<CoreFunctionSourceScope>,
}
pub(crate) fn prepare(
    ledger: &mut Ledger,
    cores: &[&CoreModule],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if ledger.original.context().is_some() {
        return Err(bad(
            "original ABI context was prepared twice in one Types request",
        ));
    }
    if cores.iter().all(|core| core.semantic.is_none()) {
        return Ok(());
    }
    let mut origins = Vec::new();
    for core in cores.iter().filter(|core| core.semantic.is_some()) {
        pay(left, core.functions.len())?;
        for function in &core.functions {
            inference::charge_function_scope_snapshot(
                function.source_scope.as_ref(),
                function.span,
                limits,
                left,
            )?;
            origins.push(SourceOrigin {
                origin: Origin {
                    module: core.module_id,
                    creator: function.id,
                    closure: None,
                },
                scope: function.source_scope.clone(),
            });
        }
    }
    let mut input = super::original_context::Input::collect(cores, limits, left)?;
    if let Some(source) = ledger.frozen.as_deref() {
        input.include_frozen(source, limits, left)?;
    }
    let proof = if let Some(source) = ledger.frozen.as_deref() {
        super::original::check_with_frozen(&input.semantic, source, limits, left)?
    } else {
        super::original::check(&input.semantic, limits, left)?
    };
    let context = super::original_context::OriginalContext::checked(
        input,
        proof,
        ledger.frozen.clone(),
        left,
    )?;
    let program = &context.semantic;
    let mut choices = Choices::new();
    for instance in &program.instances {
        let key = match &instance.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) => {
                (*module, *function, None)
            }
            SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
                module,
                creator,
                expression,
                occurrence: 0,
            }) => (*module, *creator, Some(*expression)),
            SemanticInstanceTarget::Scope => continue,
            _ => return Err(bad("original ABI selection lacks a real Core origin")),
        };
        let kind = context
            .original
            .fixed_call_kind(program, instance.definition, limits, left)?;
        pay(left, choices.len().saturating_add(1).ilog2() as usize + 1)?;
        if choices.insert(key, kind).is_some_and(|prior| prior != kind) {
            return Err(bad(
                "one actual Core origin has conflicting original ABI candidates",
            ));
        }
    }
    ledger.abi_origins = origins;
    ledger.fixed_abis = Some(choices);
    ledger.original.install(context)?;
    Ok(())
}
pub(crate) fn selected(
    ledger: &Ledger,
    function: &CoreFunction,
    actual: FunctionId,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<CallKind>> {
    let Some(choices) = ledger.fixed_abis.as_ref() else {
        return Ok(None);
    };
    pay(
        left,
        ledger.frames.len().saturating_add(1).ilog2() as usize + 1,
    )?;
    let origin = if let Some(frame) = ledger.frames.get(&actual) {
        frame.origin
    } else {
        let scope = function
            .source_scope
            .as_ref()
            .ok_or_else(|| bad("specialized executable lacks its exact original scope"))?;
        inference::charge_function_scope_snapshot(Some(scope), function.span, limits, left)?;
        pay(left, ledger.abi_origins.len())?;
        let mut found = None;
        for source in &ledger.abi_origins {
            if source.origin.creator != function.id {
                continue;
            }
            inference::charge_function_scope_snapshot(
                source.scope.as_ref(),
                function.span,
                limits,
                left,
            )?;
            if source.scope.as_ref() == Some(scope) {
                if found.replace(source.origin).is_some() {
                    return Err(bad(
                        "specialized executable has ambiguous original ABI origins",
                    ));
                }
            }
        }
        found.ok_or_else(|| {
            bad("specialized executable lacks its exact checked original ABI origin")
        })?
    };
    pay(left, choices.len().saturating_add(1).ilog2() as usize + 1)?;
    let kind = *choices
        .get(&(origin.module, origin.creator, origin.closure))
        .ok_or_else(|| bad("actual executable origin has no checked original ABI candidate"))?;
    if function.call_kind.is_some_and(|declared| declared != kind) {
        return Err(bad(
            "explicit Core ABI selection differs from its checked original declaration",
        ));
    }
    Ok(Some(kind))
}

/// Register an actual specialization at the compiler's real template mint.
/// The immutable template/context pair, rather than a generated label, supplies
/// the origin. Full actual specialization/class proof remains mandatory later.
pub(crate) fn bind_generated(
    ledger: &mut Ledger,
    original: &CoreFunction,
    module: zil_base::ModuleId,
    actual: FunctionId,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let Some(choices) = ledger.fixed_abis.as_ref() else {
        return Ok(());
    };
    let origin = Origin {
        module,
        creator: original.id,
        closure: None,
    };
    pay(left, choices.len().saturating_add(1).ilog2() as usize + 1)?;
    if !choices.contains_key(&(module, original.id, None)) {
        return Err(bad(
            "actual specialization template has no checked original ABI candidate",
        ));
    }
    pay(left, ledger.abi_origins.len())?;
    let mut matched = false;
    for source in &ledger.abi_origins {
        if source.origin != origin {
            continue;
        }
        inference::charge_function_scope_snapshot(
            source.scope.as_ref(),
            original.span,
            limits,
            left,
        )?;
        inference::charge_function_scope_snapshot(
            original.source_scope.as_ref(),
            original.span,
            limits,
            left,
        )?;
        if source.scope != original.source_scope || matched {
            return Err(bad(
                "actual specialization template has no unique immutable Core origin",
            ));
        }
        matched = true;
    }
    if !matched {
        return Err(bad(
            "actual specialization was not minted from a registered Core template",
        ));
    }
    ledger.bind(actual, origin, None, left, original.span)
}
