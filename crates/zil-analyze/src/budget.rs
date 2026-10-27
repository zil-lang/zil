use std::collections::BTreeSet;

use zil_base::{CompileResult, Diagnostic, Limits, Phase, SCHEMA_VERSION, Span};
use zil_ir::{Constant, InstructionKind, LocalId, Terminator, Type, TypedModule};

use crate::bad;

fn exhausted(context: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Analyze,
        format!("analysis budget exceeded during {context}"),
        None,
    )]
}

pub(super) struct Budget {
    remaining: usize,
}

impl Budget {
    pub(super) fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_analysis_steps,
        }
    }

    pub(super) fn spend(&mut self, amount: usize, context: &str) -> CompileResult<()> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| exhausted(context))?;
        Ok(())
    }

    pub(super) fn product(
        &mut self,
        left: usize,
        right: usize,
        multiplier: usize,
        context: &str,
    ) -> CompileResult<()> {
        let amount = left
            .checked_mul(right)
            .and_then(|n| n.checked_mul(multiplier))
            .ok_or_else(|| exhausted(context))?;
        self.spend(amount, context)
    }

    pub(super) fn sets(
        &mut self,
        left: &BTreeSet<LocalId>,
        right: &BTreeSet<LocalId>,
        context: &str,
    ) -> CompileResult<()> {
        self.spend(left.len(), context)?;
        self.spend(right.len(), context)
    }

    pub(super) fn lookup(&mut self, size: usize, context: &str) -> CompileResult<()> {
        // A bounded tree lookup/insertion visits logarithmically many levels.
        self.spend(size.checked_ilog2().map_or(1, |n| n as usize + 2), context)
    }

    pub(super) fn type_work(
        &mut self,
        ty: &Type,
        copy_check: bool,
        context: &str,
    ) -> CompileResult<()> {
        // Input preflight has already bounded nesting. Reserve this walk, one
        // exact type comparison and, when requested, one Copy classification.
        // A mismatch can stop the comparison early; an equal declaration is
        // the upper bound. Arrays compare their length once, not per element.
        self.spend(2 + usize::from(copy_check), context)?;
        match ty {
            Type::Tuple(fields) => {
                for field in fields {
                    self.type_work(field, copy_check, context)?;
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                self.spend(name.len(), context)?;
                for (name, field) in fields {
                    self.spend(name.len(), context)?;
                    self.type_work(field, copy_check, context)?;
                }
            }
            Type::Enum { name, variants } => {
                self.spend(name.len(), context)?;
                for (name, fields) in variants {
                    self.spend(2 + usize::from(copy_check), context)?;
                    self.spend(name.len(), context)?;
                    for field in fields {
                        self.type_work(field, copy_check, context)?;
                    }
                }
            }
            Type::Array { element, .. }
            | Type::OwnedStore { element }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => self.type_work(element, copy_check, context)?,
            Type::ErrorUnion { error, result } => {
                self.type_work(error, copy_check, context)?;
                self.type_work(result, copy_check, context)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.type_work(parameter, copy_check, context)?;
                }
                self.type_work(result, copy_check, context)?;
            }
            Type::Named(name) | Type::Unsupported(name) => {
                self.spend(name.len(), context)?;
            }
            Type::ByteBuffer
            | Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::ForeignHandle
            | Type::CWideString
            | Type::Closure(_) => {}
        }
        Ok(())
    }

    fn input(&mut self, amount: usize, context: &str) -> CompileResult<()> {
        // Reserve both the preflight visit and the later encoding/hash visit.
        self.product(amount, 2, 1, context)
    }
}

fn count(before: usize, add: usize, maximum: usize, name: &str) -> CompileResult<usize> {
    match before.checked_add(add) {
        Some(total) if total <= maximum => Ok(total),
        _ => Err(exhausted(&format!("module {name} count"))),
    }
}

fn scalar(ty: &Type, span: Span) -> CompileResult<()> {
    if ty.is_copy_value() {
        Ok(())
    } else {
        // Do not Debug-format or recursively inspect an unsupported raw type.
        Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "non-Copy aggregate, address, opaque, and resource analysis is not implemented",
            Some(span),
        )])
    }
}

fn local_value(ty: &Type, span: Span, module: &TypedModule) -> CompileResult<()> {
    if let Type::ErrorUnion { error, result } = ty {
        for payload in [error.as_ref(), result.as_ref()] {
            if matches!(payload, Type::Package { .. }) {
                local_value(payload, span, module)?;
            } else {
                scalar(payload, span)?;
            }
        }
        Ok(())
    } else if let Type::Array { element, length } = ty
        && *length > 0
        && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
    {
        local_value(element, span, module)
    } else if let Type::Closure(id) = ty {
        if id.0 as usize >= module.closures.len() {
            return Err(bad("closure type references an absent environment"));
        }
        Ok(())
    } else if let Type::Address { inner, .. } = ty
        && let Type::Closure(id) = inner.as_ref()
    {
        if id.0 as usize >= module.closures.len() {
            return Err(bad("borrowed environment references an absent closure"));
        }
        Ok(())
    } else if let Type::Package { fields, .. } = ty {
        if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
            && fields.iter().all(|(_, t)| {
                t.is_copy_value()
                    || matches!(t, Type::Managed(p) if **p == Type::ByteBuffer
                    || (matches!(p.as_ref(), Type::Record { .. }) && p.is_copy_value()))
            })
        {
            Ok(())
        } else {
            Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "package requires finite Copy fields and at least one explicit Managed resource",
                Some(span),
            )])
        }
    } else if crate::dynamic_store::is_carrier(ty) || matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
        || matches!(ty, Type::Managed(inner) | Type::Desc(inner)
            if **inner == Type::ByteBuffer
                || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()))
    {
        Ok(())
    } else {
        scalar(ty, span)
    }
}

pub(super) fn preflight(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> CompileResult<Vec<bool>> {
    if module.schema_version != SCHEMA_VERSION {
        return Err(bad("unknown typed module schema version"));
    }
    count(0, module.functions.len(), limits.max_functions, "function")?;
    budget.input(1, "module preflight")?;
    if let Some(feature) = module.unsupported.first() {
        budget.spend(feature.feature.len(), "unsupported feature metadata")?;
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            &feature.feature,
            feature.span,
        )]);
    }
    for text in [
        &module.target.id,
        &module.target.triple,
        &module.target.backend,
        &module.target.rule_version,
    ] {
        // Target text appears in both the module and target fingerprints.
        budget.product(text.len(), 3, 1, "target metadata")?;
    }
    count(0, module.resources.len(), limits.max_nodes, "resource")?;
    budget.input(module.resources.len(), "resource table")?;
    for resource in &module.resources {
        budget.input(resource.name.len().saturating_add(1), "resource metadata")?;
    }
    for ty in module
        .functions
        .iter()
        .flat_map(|f| {
            std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(f.values.iter())
                .chain(
                    f.blocks
                        .iter()
                        .flat_map(|b| b.instructions.iter())
                        .filter_map(|i| match &i.kind {
                            InstructionKind::ConvertInteger { target, .. } => Some(target),
                            InstructionKind::OwnedStoreNew { element } => Some(element),
                            _ => None,
                        }),
                )
        })
        .chain(module.resources.iter().map(|r| &r.payload))
        .chain(module.closures.iter().flat_map(|c| {
            c.captures
                .iter()
                .chain(c.parameters.iter())
                .chain(std::iter::once(&c.result))
        }))
    {
        let mut pending = vec![(ty, 0usize)];
        while let Some((ty, depth)) = pending.pop() {
            budget.input(1, "type shape")?;
            if depth > limits.max_call_depth {
                return Err(exhausted("aggregate nesting"));
            }
            match ty {
                Type::Array { element, length } => {
                    budget.input(*length as usize, "array type size")?;
                    pending.push((element, depth + 1));
                }
                Type::Slice { element } | Type::OwnedStore { element } => {
                    pending.push((element, depth + 1));
                }
                Type::Tuple(fields) => {
                    budget.input(fields.len(), "aggregate type shape")?;
                    for t in fields {
                        pending.push((t, depth + 1));
                    }
                }
                Type::Record { name, fields } | Type::Package { name, fields } => {
                    budget.input(name.len(), "record identity")?;
                    budget.input(fields.len(), "record fields")?;
                    for (name, t) in fields {
                        budget.input(name.len(), "field name")?;
                        pending.push((t, depth + 1));
                    }
                }
                Type::Enum { name, variants } => {
                    budget.input(
                        name.len().saturating_add(variants.len()),
                        "enum identity and variants",
                    )?;
                    for (name, fields) in variants {
                        budget.input(name.len().saturating_add(fields.len()), "enum fields")?;
                        for ty in fields {
                            pending.push((ty, depth + 1));
                        }
                    }
                }
                Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => {
                    pending.push((inner, depth + 1))
                }
                Type::FunctionPointer { parameters, result }
                | Type::ForeignCallback { parameters, result } => {
                    budget.input(parameters.len(), "function pointer signature")?;
                    pending.push((result, depth + 1));
                    for parameter in parameters {
                        pending.push((parameter, depth + 1));
                    }
                }
                Type::ErrorUnion { error, result } => {
                    pending.push((error, depth + 1));
                    pending.push((result, depth + 1));
                }
                _ => {}
            }
        }
    }
    count(0, module.closures.len(), limits.max_functions, "closure")?;
    budget.input(module.closures.len(), "closure table preflight")?;
    for closure in &module.closures {
        count(0, closure.captures.len(), limits.max_locals, "capture")?;
        count(
            0,
            closure.parameters.len(),
            limits.max_locals,
            "closure parameter",
        )?;
        budget.input(closure.captures.len(), "capture type preflight")?;
        budget.input(
            closure.parameters.len().saturating_add(1),
            "closure signature preflight",
        )?;
        for ty in &closure.captures {
            if ty.is_slice_address()
                || matches!(ty, Type::Desc(inner) if **inner == Type::ByteBuffer)
                || super::resources::owned_error(ty)
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "slice, borrowed byte storage and owned error-union captures are not implemented",
                    None,
                )]);
            }
            local_value(ty, Span::default(), module)?;
        }
        for ty in closure
            .parameters
            .iter()
            .chain(std::iter::once(&closure.result))
        {
            scalar(ty, Span::default())?;
        }
        if closure.function.0 as usize >= module.functions.len() {
            return Err(bad("closure entry references an absent function"));
        }
    }
    let (mut blocks, mut instructions, mut locals, mut values) = (0, 0, 0, 0);
    // Record strict package admission during the already charged type visits.
    // Each bounded function visit also materializes one flag; no second type
    // scan is needed in the scope checker, including for legacy scalar IR.
    let mut package_functions = Vec::new();
    for function in &module.functions {
        blocks = count(blocks, function.blocks.len(), limits.max_blocks, "block")?;
        locals = count(locals, function.locals.len(), limits.max_locals, "local")?;
        values = count(values, function.values.len(), limits.max_values, "value")?;
        budget.input(1, "function preflight")?;
        budget.input(function.name.len(), "function metadata")?;
        if let Some(contract) = &function.callable_return_contract {
            budget.input(
                contract
                    .hold
                    .len()
                    .saturating_add(contract.call.len())
                    .saturating_add(contract.writes.len())
                    .saturating_add(contract.cleanup.len())
                    .saturating_add(2),
                "callable return contract metadata",
            )?;
        }
        if let Some(contract) = &function.callable_contract {
            budget.input(
                contract
                    .hold
                    .len()
                    .saturating_add(contract.call.len())
                    .saturating_add(contract.writes.len())
                    .saturating_add(contract.traces.len())
                    .saturating_add(4),
                "callable contract metadata",
            )?;
            for trace in &contract.traces {
                budget.input(
                    trace.calls.len().saturating_add(1),
                    "callable trace metadata",
                )?;
            }
        }
        if let Some(foreign) = &function.foreign {
            budget.input(
                foreign
                    .abi
                    .len()
                    .saturating_add(foreign.symbol.len())
                    .saturating_add(foreign.noescape_parameters.len())
                    .saturating_add(1),
                "foreign declaration metadata",
            )?;
        }
        budget.input(1, "return type preflight")?;
        local_value(&function.return_type, function.span, module)?;
        let scoped_aggregate = |ty: &Type| {
            matches!(ty, Type::Package { .. })
                || super::resources::owned_error(ty)
                || matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)))
        };
        let mut package = scoped_aggregate(&function.return_type);
        budget.input(function.parameters.len(), "parameter preflight")?;
        for parameter in &function.parameters {
            if parameter.0 as usize >= function.locals.len() {
                return Err(bad("parameter local outside table"));
            }
            local_value(
                &function.locals[parameter.0 as usize].ty,
                function.span,
                module,
            )?;
        }
        if function
            .environment
            .is_some_and(|id| id.0 as usize >= module.closures.len())
        {
            return Err(bad("function references an absent environment"));
        }
        for local in &function.locals {
            budget.input(1, "local type preflight")?;
            budget.input(local.name.len(), "local metadata")?;
            local_value(&local.ty, local.span, module)?;
            package |= scoped_aggregate(&local.ty);
        }
        for ty in &function.values {
            budget.input(1, "value type preflight")?;
            local_value(ty, function.span, module)?;
            package |= scoped_aggregate(ty);
        }
        for block in &function.blocks {
            instructions = count(
                instructions,
                block.instructions.len(),
                limits.max_instructions,
                "instruction",
            )?;
            budget.input(1, "block preflight")?;
            for instruction in &block.instructions {
                budget.input(1, "instruction preflight")?;
                match &instruction.kind {
                    InstructionKind::Constant(Constant::WideString(units)) => {
                        budget.input(units.len(), "static UTF-16 units")?;
                    }
                    InstructionKind::ConvertInteger { target, .. } if !target.is_integer() => {
                        return Err(bad("integer conversion target is not a supported integer"));
                    }
                    InstructionKind::Load(local)
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DescribeLocal { local }
                    | InstructionKind::ReadPackageField { local, .. }
                    | InstructionKind::DescribePackageField { local, .. }
                    | InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::DropPackageField { local, .. }
                    | InstructionKind::StorePackageField { local, .. }
                    | InstructionKind::ProjectOwnedArrayField { local, .. }
                    | InstructionKind::TakeOwnedArrayElement { local, .. }
                    | InstructionKind::DropOwnedArrayElement { local, .. }
                    | InstructionKind::StoreOwnedArrayElement { local, .. }
                    | InstructionKind::BorrowOwnedArrayElement { local, .. }
                    | InstructionKind::BindPackageCapture { local, .. }
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropErrorUnion { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropOwnedArray { local }
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::CallClosureMut { callee: local, .. }
                    | InstructionKind::CallClosureShared { callee: local, .. }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                        if local.0 as usize >= function.locals.len() =>
                    {
                        return Err(bad("local ID outside table"));
                    }
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::ForeignCall { arguments, .. } => {
                        budget.input(arguments.len(), "call argument preflight")?;
                    }
                    InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. } => {
                        budget.input(
                            arguments.len().saturating_add(1),
                            "closure call operand preflight",
                        )?;
                    }
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeManaged { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeOwnedArray { elements: fields }
                    | InstructionKind::MakeVariant { fields, .. } => {
                        budget.input(fields.len(), "aggregate operands")?;
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        budget.input(captures.len(), "closure capture operand preflight")?;
                    }
                    InstructionKind::Unsupported(feature) => {
                        budget.spend(feature.len(), "instruction metadata")?;
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            feature,
                            Some(instruction.span),
                        )]);
                    }
                    _ => {}
                }
            }
            budget.input(1, "terminator preflight")?;
            let operands = match block.terminator {
                Terminator::Return(Some(_)) | Terminator::Jump(_) => 1,
                Terminator::Branch { .. } => 3,
                Terminator::Return(None) | Terminator::Unreachable | Terminator::Trap => 0,
            };
            budget.input(operands, "terminator operand preflight")?;
        }
        package_functions.push(package);
    }
    Ok(package_functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::ResultCategory;

    #[test]
    fn arithmetic_overflow_is_a_limit_failure() {
        let mut budget = Budget::new(&Limits {
            max_analysis_steps: usize::MAX,
            ..Limits::default()
        });
        assert_eq!(
            budget.product(usize::MAX, 2, 1, "test").unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        assert_eq!(
            count(usize::MAX, 1, usize::MAX, "test").unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
    }
}
