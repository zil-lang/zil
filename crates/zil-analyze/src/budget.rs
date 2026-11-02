use std::collections::BTreeSet;
use zil_ir::{CallableFlowNext, CallableTraces};

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
    pub(super) helper_nodes: usize,
    pub(super) helper_calls: usize,
    pub(super) helper_edges: usize,
    pub(super) helper_depth: usize,
    pub(super) shared_product_capture: bool,
    // Empty on legacy modules; populated only by the paid ordinary admission.
    pub(super) ordinary_store_functions: Vec<bool>,
    pub(super) sum_functions: Vec<bool>,
    pub(super) owned_callbacks: BTreeSet<zil_ir::ClosureId>,
    pub(super) owned_callback_helpers: BTreeSet<zil_ir::FunctionId>,
    sum_roots: BTreeSet<usize>,
}

impl Budget {
    pub(super) fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_analysis_steps,
            helper_nodes: limits.max_nodes,
            helper_calls: limits.max_instructions,
            helper_edges: limits.max_analysis_steps,
            helper_depth: limits.max_call_depth,
            shared_product_capture: false,
            ordinary_store_functions: Vec::new(),
            sum_functions: Vec::new(),
            owned_callbacks: BTreeSet::new(),
            owned_callback_helpers: BTreeSet::new(),
            sum_roots: BTreeSet::new(),
        }
    }

    pub(super) fn sum_type(&mut self, ty: &Type) -> CompileResult<bool> {
        if self.sum_roots.is_empty() {
            return Ok(false);
        }
        self.lookup(self.sum_roots.len(), "ordinary Sum type inventory")?;
        Ok(self.sum_roots.contains(&(ty as *const Type as usize)))
    }
    pub(super) fn remaining(&self) -> usize {
        self.remaining
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

#[derive(Clone, Copy, Default)]
struct SumContext {
    blocked: bool,
    in_enum: bool,
    in_product: bool,
    in_error: bool,
    error_products: u8,
    product_sum: bool,
}

fn package_fields_supported(ty: &Type, arrays: bool) -> bool {
    matches!(ty, Type::Package { fields, .. }
    if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
    && fields.iter().all(|(_, t)| {
        t.is_copy_value()
            || (arrays && matches!(t, Type::Closure(_)))
            || matches!(t, Type::Managed(p) if **p == Type::ByteBuffer
                || (arrays && matches!(p.as_ref(), Type::OwnedStore { .. }))
                || (matches!(p.as_ref(), Type::Record { .. }) && p.is_copy_value()))
            || (arrays && matches!(t, Type::Array { element, length }
                if *length > 0 && package_fields_supported(element, false)))
    }))
}

pub(super) fn legacy_value(ty: &Type, span: Span, module: &TypedModule) -> CompileResult<()> {
    if matches!(ty, Type::Address { inner, .. }
        if super::resources::product::nested(inner))
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "ordinary Package Product does not grant a borrowed Package boundary",
            Some(span),
        )]);
    }
    if let Type::ErrorUnion { error, result } = ty {
        if matches!(result.as_ref(), Type::Managed(_)) {
            scalar(error, span)?;
            return legacy_value(result, span, module);
        }
        for payload in [error.as_ref(), result.as_ref()] {
            if matches!(payload, Type::Package { .. }) {
                legacy_value(payload, span, module)?;
            } else {
                scalar(payload, span)?;
            }
        }
        Ok(())
    } else if let Type::Array { element, length } = ty
        && *length > 0
        && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
    {
        legacy_value(element, span, module)
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
    } else if matches!(ty, Type::Package { .. }) {
        if package_fields_supported(ty, true) {
            Ok(())
        } else {
            Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "package requires finite Copy/complete array fields and at least one explicit Managed resource",
                Some(span),
            )])
        }
    } else if crate::dynamic_store::is_carrier(ty)
        || matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
        || ty.is_shared_resource_head_address()
        || ty.is_package_address()
        || matches!(ty, Type::Managed(inner) | Type::Desc(inner)
            if **inner == Type::ByteBuffer
                || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()))
    {
        Ok(())
    } else {
        scalar(ty, span)
    }
}

// Ordinary Products never widen the old array, union or capture profile.
fn local_value(
    ty: &Type,
    span: Span,
    module: &TypedModule,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if budget.sum_type(ty)? {
        return super::resources::sums::admit(module, ty, budget);
    }
    if let Type::Address {
        mutable: false,
        inner,
    } = ty
        && super::resources::product::nested(inner)
    {
        return super::resources::product::admit(inner, budget).map(|_| false);
    }
    if super::resources::product::nested(ty) {
        super::resources::product::admit_owned(module, ty, budget)
    } else if let Some(payload) = super::dynamic_store::product_protocol_payload(ty) {
        super::dynamic_store::product_element(payload, budget).map(|_| false)
    } else {
        legacy_value(ty, span, module).map(|_| false)
    }
}

pub(super) fn preflight(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> CompileResult<(Vec<bool>, Vec<bool>)> {
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
    // Carry the outermost Store element through the already paid type walk.
    // Only a real Closure occurrence allocates a new admission entry; old
    // noncallback Store paths receive no new traversal or logical charge.
    let mut callback_store_elements = Vec::new();
    let mut product_store_elements = Vec::new();
    let mut owned_store_capture_roots = Vec::new();
    for (ty, store_element, capture_owner, resource_head) in module
        .functions
        .iter()
        .flat_map(|f| {
            std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(f.values.iter())
                .map(|ty| (ty, None, None, false))
                .chain(
                    f.blocks
                        .iter()
                        .flat_map(|b| b.instructions.iter())
                        .filter_map(|i| match &i.kind {
                            InstructionKind::ConvertInteger { target, .. } => {
                                Some((target, None, None, false))
                            }
                            InstructionKind::OwnedStoreNew { element } => {
                                Some((element, Some(element), None, false))
                            }
                            _ => None,
                        }),
                )
        })
        .chain(
            module
                .resources
                .iter()
                .map(|r| (&r.payload, None, None, true)),
        )
        .chain(module.closures.iter().enumerate().flat_map(|(index, c)| {
            c.captures
                .iter()
                .map(move |ty| (ty, None, Some(zil_ir::ClosureId(index as u32)), false))
                .chain(
                    c.parameters
                        .iter()
                        .chain(std::iter::once(&c.result))
                        .map(|ty| (ty, None, None, false)),
                )
        }))
    {
        if let Some(element) = store_element
            && super::resources::product::nested(element)
        {
            budget.spend(1, "Product Store instruction admission entry")?;
            product_store_elements.push(element);
        }
        // Reuse the already paid structural visit to classify new ordinary
        // positions. No extra recursive eligibility scan runs on legacy types.
        let root = ty as *const Type as usize;
        // The old Store protocol already admitted Unit!ordinary Product with
        // no Store/Sum child; retain that exact profile and fee path.
        let legacy_product_union = matches!(ty,Type::ErrorUnion { error, result }
            if **error == Type::Unit || **result == Type::Unit);
        let mut new_sum = false;
        let mut first = Some((
            ty,
            0usize,
            store_element,
            capture_owner,
            resource_head,
            SumContext::default(),
        ));
        let mut pending = Vec::new();
        while let Some((ty, depth, store_element, capture_owner, resource_head, context)) =
            first.take().or_else(|| pending.pop())
        {
            budget.input(1, "type shape")?;
            if depth > limits.max_call_depth {
                return Err(exhausted("aggregate nesting"));
            }
            if let Some(owner) = capture_owner
                && super::dynamic_store::is_owned_carrier(ty)
            {
                budget.spend(
                    owned_store_capture_roots.len().saturating_add(1),
                    "owned Store capture root entry",
                )?;
                owned_store_capture_roots.push(owner);
            }
            let mut child = context;
            match ty {
                Type::Enum { .. } => child.in_enum = true,
                Type::Package { .. } => {
                    child.in_product = true;
                    if context.in_error {
                        child.error_products = context.error_products.saturating_add(1);
                    }
                }
                Type::ErrorUnion { .. } => {
                    child.in_error = true;
                    child.product_sum |= context.in_product;
                }
                Type::Managed(_) => {
                    let extension = context.in_enum
                        || context.product_sum
                        || (context.error_products >= 2
                            && (!legacy_product_union
                                || super::dynamic_store::is_owned_carrier(ty)));
                    if extension
                        && (context.blocked
                            || store_element.is_some()
                            || capture_owner.is_some()
                            || resource_head)
                    {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "ordinary owned Sum does not grant array, StoreElement, capture, borrowed or resource-head qualification",
                            None,
                        )]);
                    }
                    new_sum |= extension;
                    child = SumContext {
                        blocked: true,
                        ..SumContext::default()
                    };
                }
                Type::Array { .. }
                | Type::OwnedStore { .. }
                | Type::Address { .. }
                | Type::Desc(_)
                | Type::Opaque { .. }
                | Type::FunctionPointer { .. }
                | Type::ForeignCallback { .. } => {
                    child = SumContext {
                        blocked: true,
                        ..SumContext::default()
                    }
                }
                _ => {}
            }
            match ty {
                Type::Array { element, length } => {
                    budget.input(*length as usize, "array type size")?;
                    pending.push((
                        element,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                }
                Type::Slice { element } => {
                    pending.push((
                        element,
                        depth + 1,
                        store_element,
                        None,
                        resource_head,
                        child,
                    ));
                }
                Type::OwnedStore { element } => {
                    if super::resources::product::nested(element) {
                        budget.spend(1, "Product Store type admission entry")?;
                        product_store_elements.push(element);
                    }
                    pending.push((
                        element,
                        depth + 1,
                        store_element.or(Some(element)),
                        None,
                        // Only the top-level sealed Store definition owns an
                        // ordinary element. A Store hidden in a user resource
                        // head cannot erase that head's closed Copy profile.
                        resource_head && depth != 0,
                        child,
                    ));
                }
                Type::Tuple(fields) => {
                    budget.input(fields.len(), "aggregate type shape")?;
                    for t in fields {
                        pending.push((
                            t,
                            depth + 1,
                            store_element,
                            capture_owner,
                            resource_head,
                            child,
                        ));
                    }
                }
                Type::Record { name, fields } | Type::Package { name, fields } => {
                    budget.input(name.len(), "record identity")?;
                    budget.input(fields.len(), "record fields")?;
                    for (name, t) in fields {
                        budget.input(name.len(), "field name")?;
                        // Only ordinary value positions acquire the new Product
                        // profile. Resource heads remain finite Copy, including
                        // unused metadata. Reuse the paid field visit; the old
                        // flat profile has no new table or logical charge.
                        if resource_head
                            && matches!(ty, Type::Package { .. })
                            && matches!(t, Type::Package { .. })
                        {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "ordinary Package Product is not a resource payload profile",
                                None,
                            )]);
                        }
                        pending.push((
                            t,
                            depth + 1,
                            store_element,
                            capture_owner,
                            resource_head,
                            child,
                        ));
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
                            pending.push((
                                ty,
                                depth + 1,
                                store_element,
                                capture_owner,
                                resource_head,
                                child,
                            ));
                        }
                    }
                }
                Type::Managed(inner) => pending.push((
                    inner,
                    depth + 1,
                    store_element,
                    capture_owner,
                    resource_head,
                    child,
                )),
                Type::Address { inner, .. }
                | Type::Desc(inner)
                | Type::Opaque { hidden: inner, .. } => {
                    pending.push((inner, depth + 1, store_element, None, resource_head, child))
                }
                Type::FunctionPointer { parameters, result }
                | Type::ForeignCallback { parameters, result } => {
                    budget.input(parameters.len(), "function pointer signature")?;
                    pending.push((result, depth + 1, store_element, None, false, child));
                    for parameter in parameters {
                        pending.push((parameter, depth + 1, store_element, None, false, child));
                    }
                }
                Type::ErrorUnion { error, result } => {
                    pending.push((
                        error,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                    pending.push((
                        result,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                }
                Type::Closure(_) if store_element.is_some() => {
                    budget.spend(1, "callback Store admission entry")?;
                    callback_store_elements.push(store_element.unwrap());
                }
                _ => {}
            }
        }
        if new_sum {
            budget.lookup(budget.sum_roots.len(), "ordinary Sum root insertion")?;
            budget.spend(1, "ordinary Sum root allocation")?;
            budget.sum_roots.insert(root);
        }
    }
    // Complete shape and depth preflight precedes recursive Product admission.
    // A Store element is a value, not an owning user-resource header.
    for element in product_store_elements {
        super::dynamic_store::product_carrier(module, element, budget)?;
    }
    count(0, module.closures.len(), limits.max_functions, "closure")?;
    budget.input(module.closures.len(), "closure table preflight")?;
    for closure in &module.closures {
        count(0, closure.captures.len(), limits.max_locals, "capture")?;
        count(
            0,
            closure.lending_captures.len(),
            limits.max_locals,
            "closure lending capture",
        )?;
        budget.input(closure.lending_captures.len(), "closure lending metadata")?;
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
            if ty.is_package_address() {
                budget.shared_product_capture = true;
                let Type::Address {
                    mutable: false,
                    inner,
                } = ty
                else {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "mutable Product capture is not implemented",
                        None,
                    )]);
                };
                super::resources::product::admit(inner, budget)?;
                budget.type_work(&closure.result, true, "shared Product capture result")?;
                if !closure.result.is_copy_value() {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "shared Product captures require a finite Copy result",
                        None,
                    )]);
                }
                continue;
            }
            if ty.is_slice_address() || super::resources::owned_error(ty) {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "slice and owned error-union captures are not implemented",
                    None,
                )]);
            }
            if super::resources::product::nested(ty) {
                // Capture admission is independent from ordinary value admission.
                // It rechecks the complete finite Product before granting an environment slot.
                super::resources::product::admit(ty, budget)?;
            } else {
                legacy_value(ty, Span::default(), module)?;
            }
        }
        let mut owned_signature = false;
        for ty in closure.parameters.iter().chain(std::iter::once(&closure.result)) {
            if matches!(ty, Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. })
                && !ty.is_copy_value()
            {
                super::resources::sums::admit(module, ty, budget)?;
                owned_signature = true;
            }
        }
        if owned_signature {
            budget.spend(1, "owned callback signature entry")?;
            super::owned_callbacks::signature(module, closure, budget)?;
            budget.owned_callbacks.insert(closure.id);
        }
        for ty in &closure.parameters {
            if owned_signature {
                super::owned_callbacks::data(module, ty, budget)?;
                continue;
            }
            if let Type::Address { inner, .. } = ty
                && closure.result.is_copy_value()
            {
                // A slice is unsized only at this pointee position; its
                // elements remain independently restricted to finite Copy.
                let pointee = match inner.as_ref() {
                    Type::Slice { element } => element.as_ref(),
                    other => other,
                };
                scalar(pointee, Span::default())?;
            } else {
                scalar(ty, Span::default())?;
            }
        }
        if owned_signature {
            super::owned_callbacks::data(module, &closure.result, budget)?;
        } else if matches!(&closure.result, Type::Address { inner, mutable: false }
            if inner.is_copy_value() || matches!(&**inner, Type::Slice { element } if element.is_copy_value()))
            && matches!(
                closure.kind,
                zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
            )
            && !closure.lending_captures.is_empty()
        {
            legacy_value(&closure.result, Span::default(), module)?;
        } else {
            scalar(&closure.result, Span::default())?;
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
    for (function_index, function) in module.functions.iter().enumerate() {
        if function.id.0 as usize != function_index {
            return Err(bad("noncanonical function ID"));
        }
        blocks = count(blocks, function.blocks.len(), limits.max_blocks, "block")?;
        locals = count(locals, function.locals.len(), limits.max_locals, "local")?;
        values = count(values, function.values.len(), limits.max_values, "value")?;
        budget.input(1, "function preflight")?;
        budget.input(function.name.len(), "function metadata")?;
        if let Some(contract) = &function.callable_return_contract {
            count(
                0,
                contract.lending_captures.len(),
                limits.max_locals,
                "callable return lending capture",
            )?;
            budget.input(
                contract
                    .hold
                    .len()
                    .saturating_add(contract.call.len())
                    .saturating_add(contract.writes.len())
                    .saturating_add(contract.cleanup.len())
                    .saturating_add(contract.lending_captures.len())
                    .saturating_add(2),
                "callable return contract metadata",
            )?;
        }
        if let Some(contract) = &function.callable_contract {
            count(0, contract.roles.len(), limits.max_locals, "callable role")?;
            budget.input(
                contract.roles.len().saturating_add(1),
                "callable role table",
            )?;
            for role in &contract.roles {
                budget.input(
                    role.hold
                        .len()
                        .saturating_add(role.call.len())
                        .saturating_add(role.writes.len())
                        .saturating_add(5),
                    "callable role metadata",
                )?;
            }
            match &contract.traces {
                CallableTraces::Finite(traces) => {
                    budget.input(traces.len(), "callable finite trace count")?;
                    for trace in traces {
                        count(
                            0,
                            trace.calls.len(),
                            limits.max_instructions,
                            "callable finite events",
                        )?;
                        budget.input(
                            trace.calls.len().saturating_mul(2).saturating_add(1),
                            "callable trace metadata",
                        )?;
                    }
                }
                CallableTraces::Flow(flow) => {
                    count(0, flow.nodes.len(), limits.max_nodes, "callable flow node")?;
                    budget.input(
                        flow.nodes.len().saturating_add(1),
                        "callable flow node metadata",
                    )?;
                    let (mut calls, mut edges) = (0, 0);
                    for node in &flow.nodes {
                        calls = count(
                            calls,
                            node.calls.len(),
                            limits.max_instructions,
                            "callable flow call",
                        )?;
                        budget.input(
                            node.calls.len().saturating_mul(2).saturating_add(1),
                            "callable flow calls and next tag",
                        )?;
                        match &node.next {
                            CallableFlowNext::Edges(next) => {
                                edges = count(
                                    edges,
                                    next.len(),
                                    limits.max_analysis_steps,
                                    "callable flow edge",
                                )?;
                                budget
                                    .input(next.len().saturating_add(1), "callable flow edges")?;
                            }
                            CallableFlowNext::Exit(_) => budget.input(1, "callable flow exit")?,
                        }
                    }
                }
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
        let mut sum_function = budget.sum_type(&function.return_type)?;
        let mut product_store = local_value(&function.return_type, function.span, module, budget)?;
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
            product_store |= local_value(
                &function.locals[parameter.0 as usize].ty,
                function.span,
                module,
                budget,
            )?;
        }
        if function
            .environment
            .is_some_and(|id| id.0 as usize >= module.closures.len())
        {
            return Err(bad("function references an absent environment"));
        }
        for (index, local) in function.locals.iter().enumerate() {
            budget.input(1, "local type preflight")?;
            if local.id.0 as usize != index {
                return Err(bad("noncanonical local ID"));
            }
            budget.input(local.name.len(), "local metadata")?;
            sum_function |= budget.sum_type(&local.ty)?;
            product_store |= local_value(&local.ty, local.span, module, budget)?;
            package |= scoped_aggregate(&local.ty);
        }
        for ty in &function.values {
            budget.input(1, "value type preflight")?;
            sum_function |= budget.sum_type(ty)?;
            product_store |= local_value(ty, function.span, module, budget)?;
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
                    InstructionKind::VariantTagLocal { local, path }
                    | InstructionKind::ReadVariantLocal { local, path } => {
                        sum_function = true;
                        if local.0 as usize >= function.locals.len() {
                            return Err(bad("Sum local outside table"));
                        }
                        count(0, path.len(), limits.max_call_depth, "Sum path")?;
                        budget.input(
                            path.len().saturating_mul(2).saturating_add(1),
                            "Sum path preflight",
                        )?;
                    }
                    InstructionKind::UnpackVariant { local, fields, .. } => {
                        sum_function = true;
                        count(0, fields.len(), limits.max_locals, "Unpack fields")?;
                        budget.input(fields.len().saturating_add(1), "Unpack target preflight")?;
                        // Reserve the semantic cache key's owned target vector
                        // on every request, including uncached analysis.
                        budget.spend(fields.len(), "Unpack cache key allocation reserve")?;
                        if local.0 as usize >= function.locals.len()
                            || fields.iter().any(|l| l.0 as usize >= function.locals.len())
                        {
                            return Err(bad("Unpack local outside table"));
                        }
                    }
                    InstructionKind::DropEnum { local } => {
                        sum_function = true;
                        if local.0 as usize >= function.locals.len() {
                            return Err(bad("DropEnum local outside table"));
                        }
                    }
                    InstructionKind::Constant(Constant::WideString(units)) => {
                        budget.input(units.len(), "static UTF-16 units")?;
                    }
                    InstructionKind::ConvertInteger { target, .. } if !target.is_integer() => {
                        return Err(bad("integer conversion target is not a supported integer"));
                    }
                    InstructionKind::Load(local)
                    | InstructionKind::RegisterDefer { local, .. }
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
                    | InstructionKind::BorrowPackageField { local, .. }
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
        if sum_function && function.foreign.is_some() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "ordinary owned Sum does not grant foreign ABI qualification",
                Some(function.span),
            )]);
        }
        if sum_function {
            if budget.sum_functions.is_empty() {
                budget.spend(module.functions.len(), "ordinary Sum function inventory")?;
                budget.sum_functions = vec![false; module.functions.len()];
            }
            budget.sum_functions[function_index] = true;
        }
        package |= sum_function;
        package_functions.push(package);
        if product_store {
            if budget.ordinary_store_functions.is_empty() {
                budget.spend(
                    module.functions.len(),
                    "ordinary Product Store function flags",
                )?;
                budget.ordinary_store_functions = vec![false; module.functions.len()];
            }
            budget.spend(1, "ordinary Product Store function flag")?;
            budget.ordinary_store_functions[function_index] = true;
        }
    }
    // All raw type and capture trees are now bounded. Admission must include
    // resource-only and unused Store occurrences, not just body operations.
    let store_profiles =
        super::dynamic_store::owned_capture_profiles(module, &owned_store_capture_roots, budget)?;
    for element in callback_store_elements {
        super::dynamic_store::callback_element(module, element, budget)?;
    }
    Ok((package_functions, store_profiles))
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
