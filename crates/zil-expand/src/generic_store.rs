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
    Shape {
        nominals,
        budget,
        active: BTreeSet::new(),
        depth: 0,
    }
    .copy(ty)
    .map(|_| ())
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
) -> R<()> {
    let mut shape = Shape {
        nominals,
        budget,
        active: BTreeSet::new(),
        depth: 0,
    };
    let (aggregate, substitutions) = shape.aggregate(ty, AggregateKind::Package)?;
    let mut owns = false;
    let mut storage = false;
    for field in &aggregate.fields {
        shape.budget.node(field.span, 0)?;
        let field_type = shape.substitute(&field.ty, &substitutions)?;
        if field.manages {
            owns = true;
            storage |= shape.resource(&field_type)?;
        } else {
            storage |= shape.copy(&field_type)?;
        }
    }
    if !owns || !storage {
        return Err(error(
            ty.span,
            "StoreElement requires a nonzero complete Package with direct managed fields",
        ));
    }
    Ok(())
}

struct Shape<'s, 'b, 'n, 'l> {
    nominals: &'s BTreeMap<String, &'n Item>,
    budget: &'b mut Budget<'l>,
    active: BTreeSet<String>,
    depth: usize,
}

impl<'n> Shape<'_, '_, 'n, '_> {
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

    /// Returns whether the finite Copy layout has any storage. Keeping this
    /// separate avoids granting eligibility to a zero-size managed Package.
    fn copy(&mut self, ty: &TypeRef) -> R<bool> {
        self.depth += 1;
        self.budget.node(ty.span, self.depth)?;
        let result = self.copy_inner(ty);
        self.depth -= 1;
        result
    }

    fn copy_inner(&mut self, ty: &TypeRef) -> R<bool> {
        self.budget.ty(ty)?;
        match &grouped(ty).kind {
            TypeKind::Tuple(fields) => {
                let mut storage = false;
                for field in fields {
                    storage |= self.copy(field)?;
                }
                Ok(storage)
            }
            TypeKind::Array {
                element,
                length: Some(length),
            } => {
                let size = array_length(length)?;
                Ok(self.copy(element)? && size != 0)
            }
            TypeKind::Error { error, result } => {
                self.copy(error)?;
                self.copy(result)?;
                Ok(true)
            }
            TypeKind::ForeignFunction { .. } => {
                // The registered foreign callback shape was checked by the
                // budget preflight. Require its concrete C profile here;
                // a callback is a Copy code pointer, not an owning closure.
                super::super::foreign::callback_type_formation(ty, Some(&BTreeSet::new()))?;
                Ok(true)
            }
            TypeKind::Function { params, result } => {
                for ty in params {
                    self.copy(ty)?;
                }
                self.copy(result)?;
                Ok(true)
            }
            TypeKind::Named { path, args } => {
                if args.is_empty() && path.len() == 1 {
                    match path[0].as_str() {
                        "void" => return Ok(false),
                        "bool" | "i32" | "i64" | "u8" | "u16" | "c_handle" | "c_wstr" => {
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
                let name = path.join(".");
                if !self.active.insert(name.clone()) {
                    return Err(error(
                        ty.span,
                        "StoreElement requires a finite nonrecursive layout",
                    ));
                }
                let outcome = match self.nominals.get(&name) {
                    Some(Item {
                        kind: ItemKind::Aggregate(a),
                        ..
                    }) if a.kind == AggregateKind::Struct => {
                        let (aggregate, substitutions) =
                            self.aggregate(ty, AggregateKind::Struct)?;
                        let mut storage = false;
                        for field in &aggregate.fields {
                            if field.manages {
                                return Err(error(
                                    field.span,
                                    "StoreElement Copy field cannot own resources",
                                ));
                            }
                            let field_type = self.substitute(&field.ty, &substitutions)?;
                            storage |= self.copy(&field_type)?;
                        }
                        Ok(storage)
                    }
                    Some(Item {
                        kind: ItemKind::Enum(e),
                        ..
                    }) => {
                        if e.generics.len() != args.len() {
                            return Err(error(ty.span, "StoreElement enum type arity mismatch"));
                        }
                        let mut substitutions = BTreeMap::new();
                        for (p, arg) in e.generics.iter().zip(args) {
                            let GenericArg::Type(ty) = arg else {
                                return Err(unsupported(ty.span, "StoreElement const enum"));
                            };
                            self.budget.ty(ty)?;
                            substitutions.insert(p.name.clone(), ty.clone());
                        }
                        for variant in &e.variants {
                            self.budget.node(variant.span, 0)?;
                            match &variant.fields {
                                VariantFields::Unit => {}
                                VariantFields::Tuple(fields) => {
                                    for field in fields {
                                        let field_type = self.substitute(field, &substitutions)?;
                                        self.copy(&field_type)?;
                                    }
                                }
                                VariantFields::Record(fields) => {
                                    for field in fields {
                                        let field_type =
                                            self.substitute(&field.ty, &substitutions)?;
                                        self.copy(&field_type)?;
                                    }
                                }
                            }
                        }
                        Ok(true)
                    }
                    _ => Err(error(
                        ty.span,
                        "StoreElement fields require finite Copy or explicit direct manages resources",
                    )),
                };
                self.active.remove(&name);
                outcome
            }
            _ => Err(error(
                ty.span,
                "StoreElement fields cannot contain addresses, borrowed or unknown ownership shapes",
            )),
        }
    }
}
