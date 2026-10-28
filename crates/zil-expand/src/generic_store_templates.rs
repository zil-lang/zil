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
            callable::StoreCallbacks::from_names(
                self.callable_name.as_deref(),
                self.callable_pair.as_ref(),
            ),
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

impl Engine<'_> {
    /// Register only a real dual-callable producer's returned complete nominal
    /// shape. Reserved generated identities are used, never global F/G names.
    /// This fact only delays specialization: it grants no Store eligibility.
    pub(super) fn note_state_value_result(
        &mut self,
        ty: &TypeRef,
        names: [&str; 2],
        depth: usize,
    ) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &grouped(ty).kind {
            TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
                self.budget.text(&path[0], ty.span)?;
                if let Some(original) = self.instance_types.get(&path[0]) {
                    self.budget.ty(original)?;
                    if callable::StoreCallbacks::Pair(names).active(original)
                        && let TypeKind::Named { path: family, .. } = &grouped(original).kind
                        && family.len() == 1
                        && matches!(self.nominals.get(&family[0]), Some(Item {
                            kind: ItemKind::Aggregate(aggregate), ..
                        }) if aggregate.kind == AggregateKind::Package)
                    {
                        self.budget.text(&path[0], ty.span)?;
                        self.inferred_state_values.insert(path[0].clone());
                    }
                }
            }
            TypeKind::Error { error, result } => {
                self.note_state_value_result(error, names, depth + 1)?;
                self.note_state_value_result(result, names, depth + 1)?;
            }
            // These are only the already admitted complete carrier wrappers.
            TypeKind::Named { .. } if stores::element(ty).is_some() => {
                self.note_state_value_result(stores::element(ty).unwrap(), names, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn inferred_state_value(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        if self.inferred_state_values.is_empty() {
            return Ok(false);
        }
        self.budget.node(ty.span, depth)?;
        match &grouped(ty).kind {
            TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
                self.budget.text(&path[0], ty.span)?;
                Ok(self.inferred_state_values.contains(&path[0]))
            }
            TypeKind::Named { .. } if stores::element(ty).is_some() => {
                self.inferred_state_value(stores::element(ty).unwrap(), depth + 1)
            }
            TypeKind::Error { error, result } => Ok(self.inferred_state_value(error, depth + 1)?
                || self.inferred_state_value(result, depth + 1)?),
            _ => Ok(false),
        }
    }

    pub(super) fn apply_value_store_template(
        &mut self,
        callee: &mut Expr,
        function: &Function,
        item: &Item,
        template: &str,
        actual: &[Option<TypeRef>],
        span: Span,
    ) -> R<Option<TypeRef>> {
        let parameter = parameter(function).ok_or_else(|| {
            error(
                span,
                "value-inferred ordinary call requires one StoreElement binder",
            )
        })?;
        if actual.len() != function.params.len() {
            return Err(error(span, "ordinary value-inferred call arity differs"));
        }
        let mut element = None;
        for (formal, actual) in function.params.iter().zip(actual) {
            if let Some(actual) = actual
                && let Some(found) =
                    value_element_hint(&formal.ty, actual, &parameter.name, &mut self.budget, 0)?
                && element.is_none()
            {
                element = Some(found);
            }
        }
        let element = element.ok_or_else(|| {
            error(
                span,
                "ordinary call has no complete element constraint from a real value argument",
            )
        })?;
        // This is a source hint only. Types will extract every complete actual
        // T (including nominal/phantom data) and reject conflicting arguments.
        // Never manufacture TypeApply from the producer's out-of-scope F/G.
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
        self.budget.node(callee.span, 0)?;
        self.budget.text(template, callee.span)?;
        callee.kind = ExprKind::Name(template.to_owned());
        let Some(result) = &function.return_type else {
            return Ok(None);
        };
        self.budget.node(span, 0)?;
        self.budget.text(&parameter.name, span)?;
        let bindings = BTreeMap::from([(parameter.name.clone(), element)]);
        let mut result = enums::substituted(result, &bindings, &mut self.budget)?;
        self.ty(&mut result, 0)?;
        Ok(Some(result))
    }
}

fn value_element_hint(
    formal: &TypeRef,
    actual: &TypeRef,
    binder: &str,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<Option<TypeRef>> {
    budget.node(formal.span, depth)?;
    if callable::parameter_mode(formal, binder) == Some(None) {
        budget.ty(actual)?;
        return Ok(Some(actual.clone()));
    }
    if let (Some(formal), Some(actual)) = (stores::element(formal), stores::element(actual)) {
        return value_element_hint(formal, actual, binder, budget, depth + 1);
    }
    if let (
        TypeKind::Error {
            error: fe,
            result: fr,
        },
        TypeKind::Error {
            error: ae,
            result: ar,
        },
    ) = (&grouped(formal).kind, &grouped(actual).kind)
    {
        let error = value_element_hint(fe, ae, binder, budget, depth + 1)?;
        let result = value_element_hint(fr, ar, binder, budget, depth + 1)?;
        return Ok(error.or(result));
    }
    Ok(None)
}
