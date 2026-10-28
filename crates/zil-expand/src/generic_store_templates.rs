//! Preserve one ordinary StoreElement binder until its complete actual type is
//! known. These syntax constraints grant no element or ownership certificate.
use super::*;

pub(super) enum Instantiation {
    Eager(String),
    Deferred { template: String, element: TypeRef },
}

pub(crate) fn parameter(function: &Function) -> Option<&GenericParam> {
    let [parameter] = function.generics.as_slice() else {
        return None;
    };
    let GenericParamKind::Type(bounds) = &parameter.kind else {
        return None;
    };
    let [bound] = bounds.as_slice() else {
        return None;
    };
    (stores::is_bound(bound)
        && !parameter.name.is_empty()
        && function.where_clause.is_empty()
        && !matches!(function.body, FunctionBody::Declaration)
        && function
            .return_type
            .as_ref()
            .is_none_or(|ty| callable::opaque_signature(ty).is_none()))
    .then_some(parameter)
}

pub(crate) fn core_parameters(
    function: &Function,
    remaining: &mut usize,
) -> R<Vec<zil_ir::CoreTypeParameter>> {
    let Some(parameter) = parameter(function) else {
        return Ok(Vec::new());
    };
    *remaining = remaining
        .checked_sub(parameter.name.len().saturating_add(2))
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "ordinary template metadata budget exceeded",
                Some(parameter.span),
            )
        })?;
    Ok(vec![zil_ir::CoreTypeParameter {
        index: 0,
        name: parameter.name.clone(),
        bound: zil_ir::CoreTypeBound::StoreElement,
    }])
}

impl Engine<'_> {
    pub(super) fn retain_store_template(
        &mut self,
        template: &str,
        item: &Item,
        mut arguments: Vec<TypeRef>,
        span: Span,
    ) -> R<Instantiation> {
        let ItemKind::Function(function) = &item.kind else {
            return Err(unsupported(
                span,
                "only a direct ordinary call can retain a type binder",
            ));
        };
        let Some(parameter) = parameter(function) else {
            return Err(unsupported(
                span,
                "deferred ordinary calls require exactly one StoreElement type parameter",
            ));
        };
        let [element] = arguments.as_slice() else {
            return Err(error(
                span,
                "ordinary StoreElement template needs one complete type argument",
            ));
        };

        // This is the caller's lexical bound, not the callee's same-spelled T.
        // Only the new path allocates these symbolic contexts.
        let mut available = trait_bounds::Bounds::new();
        let mut variables = BTreeSet::new();
        if let Some(name) = &self.store_parameter {
            self.budget.node(span, 0)?;
            self.budget.text(name, span)?;
            self.budget.text(name, span)?;
            self.budget.text(stores::BOUND, span)?;
            available.insert(name.clone(), vec![stores::BOUND.to_owned()]);
            variables.insert(name.clone());
        }
        self.traits.store_element(
            element,
            &available,
            &variables,
            &mut self.budget,
            self.callable_name.as_deref(),
        )?;
        // Charge each actual comparison, rather than assuming an ordered
        // container's internal search cost. This list exists only on the new
        // path and is capped with the other generated functions.
        let mut retained = false;
        for previous in &self.retained_stores {
            self.budget.node(span, 0)?;
            self.budget.text(previous, span)?;
            self.budget.text(template, span)?;
            if previous == template {
                retained = true;
                break;
            }
        }
        if !retained {
            if self
                .retained_stores
                .len()
                .saturating_add(self.instances.len())
                >= self.limits.max_functions.min(512)
            {
                return Err(Diagnostic::limit(
                    Phase::Expand,
                    "retained ordinary template budget exceeded",
                    Some(span),
                ));
            }
            self.budget.node(span, 0)?;
            self.budget.text(template, span)?;
            self.budget.item(item)?;
            self.retained_stores.push(template.to_owned());
            self.pending.push_back(item.clone());
        }
        self.budget.text(template, span)?;
        self.budget.text(&parameter.name, span)?;
        let mut element = arguments.pop().expect("exactly one argument checked above");
        // Resolve the nominal family under the caller's still-active F/T scope.
        self.ty(&mut element, 0)?;
        Ok(Instantiation::Deferred {
            template: template.to_owned(),
            element,
        })
    }

    pub(super) fn apply_store_template(
        &mut self,
        callee: &mut Expr,
        function: &Function,
        template: String,
        element: TypeRef,
    ) -> R<Option<TypeRef>> {
        let parameter = parameter(function).expect("retained template header was checked");
        self.budget.node(callee.span, 0)?;
        self.budget.node(callee.span, 0)?;
        self.budget.text(&template, callee.span)?;
        self.budget.ty(&element)?;
        callee.kind = ExprKind::TypeApply {
            callee: Box::new(Expr {
                kind: ExprKind::Name(template),
                span: callee.span,
            }),
            arguments: vec![GenericArg::Type(element.clone())],
        };
        let Some(result) = &function.return_type else {
            return Ok(None);
        };
        self.budget.node(callee.span, 0)?;
        self.budget.text(&parameter.name, callee.span)?;
        let bindings = BTreeMap::from([(parameter.name.clone(), element)]);
        let mut result = enums::substituted(result, &bindings, &mut self.budget)?;
        // Unlike signature_result, this never switches to the callee's scope.
        self.ty(&mut result, 0)?;
        Ok(Some(result))
    }
}
