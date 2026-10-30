//! Definition-time Store shapes and modes. These are syntax obligations, not
//! ownership certificates: every instance enters Types and independent Verify.
use super::*;

pub(super) const BOUND: &str = "StoreElement";

pub(super) fn is_bound(ty: &TypeRef) -> bool {
    matches!(&grouped(ty).kind, TypeKind::Named { path, args }
        if path.as_slice() == [BOUND] && args.is_empty())
}

pub(super) fn element(ty: &TypeRef) -> Option<&TypeRef> {
    match &grouped(ty).kind {
        TypeKind::Named { path, args } if path.as_slice() == ["Store"] => match args.as_slice() {
            [GenericArg::Type(element)] => Some(element),
            _ => None,
        },
        _ => None,
    }
}

/// Syntax classification only: an explicit mode must separately establish
/// ownership of this success payload. The union itself remains a Value.
pub(super) fn success(ty: &TypeRef) -> Option<(&TypeRef, &TypeRef)> {
    match &grouped(ty).kind {
        TypeKind::Error { error, result } if element(result).is_some() => Some((error, result)),
        _ => None,
    }
}

pub(super) fn carrier(ty: &TypeRef) -> Option<&TypeRef> {
    if element(ty).is_some() {
        Some(ty)
    } else {
        success(ty).map(|(_, result)| result)
    }
}

/// A union context cannot constrain an arbitrary result variable. Only the
/// written direct Store / Store-success shape determines which context fits.
pub(super) fn call_context<'a>(result: &TypeRef, expected: &'a TypeRef) -> Option<&'a TypeRef> {
    if let Some((_, payload)) = success(expected) {
        if success(result).is_some() {
            Some(expected)
        } else if element(result).is_some() {
            Some(payload)
        } else {
            None
        }
    } else if matches!(grouped(expected).kind, TypeKind::Error { .. })
        && !matches!(grouped(result).kind, TypeKind::Error { .. })
    {
        // The caller may return either payload or the complete union. A
        // concrete payload result, or an unconstrained result variable, must
        // not be unified with the whole union before real arguments bind it.
        // Definition checking still validates the actual returned payload.
        None
    } else {
        Some(expected)
    }
}

pub(super) fn call_name(callee: &Expr) -> Option<&str> {
    match &callee.kind {
        ExprKind::Group(inner) | ExprKind::TypeApply { callee: inner, .. } => call_name(inner),
        _ => super::super::foreign::bare_name(callee),
    }
}

pub(super) fn copy_error(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<()> {
    copy::concrete_storage(ty, nominals, budget).map(|_| ())
}

pub(super) fn primitive(name: &str) -> bool {
    matches!(
        name,
        "store_new"
            | "store_reserve"
            | "store_push"
            | "store_pop"
            | "store_len"
            | "store_capacity"
            | "store_at"
            | "store_at_mut"
    )
}

pub(super) fn arity(name: &str) -> usize {
    match name {
        "store_new" => 0,
        "store_reserve" | "store_push" | "store_at" | "store_at_mut" => 2,
        _ => 1,
    }
}

pub(super) fn result(name: &str, element: &TypeRef, span: Span) -> TypeRef {
    let kind = match name {
        "store_new" => TypeKind::Named {
            path: vec!["Store".into()],
            args: vec![GenericArg::Type(element.clone())],
        },
        "store_reserve" => return named("bool", span),
        "store_len" | "store_capacity" => return named("i64", span),
        "store_push" => TypeKind::Error {
            error: Box::new(element.clone()),
            result: Box::new(check::unit(span)),
        },
        "store_pop" => TypeKind::Error {
            error: Box::new(check::unit(span)),
            result: Box::new(element.clone()),
        },
        "store_at" | "store_at_mut" => TypeKind::Reference {
            mutable: name == "store_at_mut",
            lifetime: None,
            inner: Box::new(element.clone()),
        },
        _ => unreachable!("known Store primitive"),
    };
    TypeRef { kind, span }
}

/// Source result modes are deliberately finite. No arbitrary contract is
/// accepted merely because the function happens to mention a Store.
pub(super) fn result_mode(function: &Function) -> R<zil_ir::PassingMode> {
    use zil_ir::PassingMode;
    let mut mode = PassingMode::Infer;
    for clause in &function.contract {
        let selected = match clause.terms.as_slice() {
            [term]
                if clause.name == "result"
                    && term.type_args.is_empty()
                    && term.arguments.is_empty()
                    && term.index.is_none() =>
            {
                match term.path.as_slice() {
                    [name] if name == "create" => Some(PassingMode::Create),
                    [name] if name == "give" => Some(PassingMode::Give),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(selected) = selected else {
            return Err(unsupported(
                clause.span,
                "generic contract requires a direct Store result(create/give)",
            ));
        };
        if mode != PassingMode::Infer {
            return Err(error(
                clause.span,
                "generic Store result modes are mutually exclusive",
            ));
        }
        if function.return_type.as_ref().and_then(carrier).is_none() {
            return Err(unsupported(
                clause.span,
                "generic resource result mode requires a direct Store result or an explicit Store success result",
            ));
        }
        mode = selected;
    }
    Ok(mode)
}

pub(super) fn concrete_element(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    context: (
        &trait_bounds::Bounds,
        &BTreeSet<String>,
        callable::StoreCallbacks<'_>,
    ),
) -> R<()> {
    let mut shape = Shape { nominals, budget };
    let (aggregate, substitutions) = shape.aggregate(ty, AggregateKind::Package)?;
    // aggregate already charged the complete application before this bounded
    // syntactic classification. F grants neither Copy nor StoreElement: only
    // its direct occurrences in the existing owning State family are admitted.
    let (available, variables, callback) = context;
    let callback = if callback.active(ty) {
        callback
    } else {
        callable::StoreCallbacks::None
    };
    let active = !matches!(callback, callable::StoreCallbacks::None);
    if active {
        shape.budget.ty(ty)?;
    }
    let mut callbacks = 0usize;
    let mut owns = false;
    let mut storage = false;
    for field in &aggregate.fields {
        shape.budget.node(field.span, 0)?;
        let field_type = shape.substitute(&field.ty, &substitutions)?;
        if field.manages {
            owns = true;
            storage |= shape.resource(&field_type)?;
        } else if callback.field(&field_type) {
            shape.budget.ty(&field_type)?;
            callbacks += 1;
            // Concrete environment layout and nonzero size are checked again
            // after F is resolved; a source name is not a layout certificate.
            storage = true;
        } else if active && check::dependent(&field_type, variables) {
            copy::require(&field_type, nominals, available, variables, shape.budget)?;
        } else if !active && shape.package_name(&field_type) {
            // The actual nominal application was bounded by substitute. A
            // child-only Product still owns its recursively declared leaves.
            owns = true;
            storage |= shape.product(&field_type, 1)?;
        } else {
            storage |= shape.copy(&field_type)?;
        }
    }
    if active && !(1..=2).contains(&callbacks) {
        return Err(error(
            ty.span,
            "callback Store elements require one or two direct fields of declared callable types",
        ));
    }
    if !owns || !storage {
        return Err(error(
            ty.span,
            "StoreElement requires a nonzero complete Package with transitive managed leaves",
        ));
    }
    Ok(())
}

struct Shape<'s, 'b, 'n, 'l> {
    nominals: &'s BTreeMap<String, &'n Item>,
    budget: &'b mut Budget<'l>,
}

impl<'n> Shape<'_, '_, 'n, '_> {
    fn package_name(&self, ty: &TypeRef) -> bool {
        matches!(&grouped(ty).kind, TypeKind::Named { path, .. }
            if matches!(self.nominals.get(&path.join(".")),
                Some(Item { kind: ItemKind::Aggregate(a), .. }) if a.kind == AggregateKind::Package))
    }

    fn product(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        // A recursive by-value application eventually reaches the same depth
        // bound as the source type resolver; no unbounded host recursion.
        self.budget.node(ty.span, depth)?;
        let (aggregate, substitutions) = self.aggregate(ty, AggregateKind::Package)?;
        let (mut owns, mut storage) = (false, false);
        for field in &aggregate.fields {
            self.budget.node(field.span, depth)?;
            let field_type = self.substitute(&field.ty, &substitutions)?;
            if field.manages {
                owns = true;
                storage |= self.resource(&field_type)?;
            } else if self.package_name(&field_type) {
                owns = true;
                storage |= self.product(&field_type, depth + 1)?;
            } else {
                storage |= self.copy(&field_type)?;
            }
        }
        if !owns {
            return Err(error(
                ty.span,
                "Product Store child requires a transitive manages leaf",
            ));
        }
        Ok(storage)
    }

    fn substitute(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
    ) -> R<TypeRef> {
        self.budget.substituted_type(ty, substitutions)?;
        let mut result = ty.clone();
        Substitution { substitutions }.ty(&mut result);
        Ok(result)
    }

    fn aggregate(
        &mut self,
        ty: &TypeRef,
        kind: AggregateKind,
    ) -> R<(&'n Aggregate, BTreeMap<String, TypeRef>)> {
        self.budget.ty(ty)?;
        let TypeKind::Named { path, args } = &grouped(ty).kind else {
            return Err(error(
                ty.span,
                "StoreElement requires a complete nominal Package",
            ));
        };
        let Some(Item {
            kind: ItemKind::Aggregate(aggregate),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            return Err(error(
                ty.span,
                "StoreElement requires a complete nominal Package or declared resource",
            ));
        };
        if aggregate.kind != kind || aggregate.generics.len() != args.len() {
            return Err(error(
                ty.span,
                "StoreElement requires the declared Package/resource category and type arguments",
            ));
        }
        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in aggregate.generics.iter().zip(args) {
            let GenericArg::Type(ty) = argument else {
                return Err(unsupported(ty.span, "StoreElement constant type argument"));
            };
            self.budget.ty(ty)?;
            substitutions.insert(parameter.name.clone(), ty.clone());
        }
        Ok((aggregate, substitutions))
    }

    fn resource(&mut self, ty: &TypeRef) -> R<bool> {
        if trait_bounds::plain(ty) == Some("ByteBuffer") {
            self.budget.ty(ty)?;
            return Ok(true);
        }
        let (aggregate, substitutions) = self.aggregate(ty, AggregateKind::Resource)?;
        let mut storage = false;
        for field in &aggregate.fields {
            if field.manages {
                return Err(error(
                    field.span,
                    "StoreElement resource payload must be finite Copy",
                ));
            }
            let ty = self.substitute(&field.ty, &substitutions)?;
            storage |= self.copy(&ty)?;
        }
        Ok(storage)
    }

    fn copy(&mut self, ty: &TypeRef) -> R<bool> {
        copy::concrete_storage(ty, self.nominals, self.budget)
    }
}
