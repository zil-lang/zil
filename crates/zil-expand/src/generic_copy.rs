//! Finite structural Copy obligations. These are source checks, never IR proof.
//! The concrete storage walk is extracted without changing StoreElement charging.
use super::*;

pub(super) const BOUND: &str = "Copy";

pub(super) fn is_bound(ty: &TypeRef) -> bool {
    matches!(&grouped(ty).kind, TypeKind::Named { path, args }
        if path.as_slice() == [BOUND] && args.is_empty())
}

pub(super) fn reserved_name(name: &str) -> bool {
    name == BOUND
        || name
            .strip_prefix("$module")
            .and_then(|name| name.split_once('_'))
            .is_some_and(|(module, name)| {
                !module.is_empty() && module.bytes().all(|b| b.is_ascii_digit()) && name == BOUND
            })
}

pub(super) fn concrete_storage(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<bool> {
    Shape {
        nominals,
        budget,
        active: BTreeSet::new(),
        depth: 0,
        symbolic: None,
        instantiated: None,
        source: None,
    }
    .copy(ty)
}

pub(super) fn require(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    available: &trait_bounds::Bounds,
    variables: &BTreeSet<String>,
    budget: &mut Budget<'_>,
) -> R<()> {
    require_registered(ty, nominals, None, available, variables, budget)
}

/// Physical items are recorded only at the actual stored specialization step.
/// This directory locates source shapes; it is not a Copy certificate.
pub(super) fn require_registered<'n>(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &'n Item>,
    instantiated: Option<&'n BTreeMap<String, Item>>,
    available: &trait_bounds::Bounds,
    variables: &BTreeSet<String>,
    budget: &mut Budget<'_>,
) -> R<()> {
    require_registered_scoped(
        ty,
        nominals,
        instantiated,
        available,
        variables,
        budget,
        None,
    )
}

pub(super) fn require_registered_scoped<'n>(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &'n Item>,
    instantiated: Option<&'n BTreeMap<String, Item>>,
    available: &trait_bounds::Bounds,
    variables: &BTreeSet<String>,
    budget: &mut Budget<'_>,
    source: Option<&Function>,
) -> R<()> {
    Shape {
        nominals,
        budget,
        active: BTreeSet::new(),
        depth: 0,
        symbolic: Some((available, variables)),
        instantiated,
        source,
    }
    .copy(ty)
    .map(|_| ())
    .map_err(|mut diagnostic| {
        // Preserve the old Store diagnostics on its unchanged concrete path.
        diagnostic.message = diagnostic.message.replace("StoreElement", "Copy bound");
        diagnostic.message = diagnostic.message.replace(
            "finite Copy or explicit direct manages resources",
            "a finite ordinary Copy Value",
        );
        diagnostic
    })
}

struct Shape<'s, 'b, 'n, 'l> {
    nominals: &'s BTreeMap<String, &'n Item>,
    instantiated: Option<&'n BTreeMap<String, Item>>,
    budget: &'b mut Budget<'l>,
    active: BTreeSet<String>,
    depth: usize,
    symbolic: Option<(&'s trait_bounds::Bounds, &'s BTreeSet<String>)>,
    source: Option<&'s Function>,
}

impl<'n> Shape<'_, '_, 'n, '_> {
    fn nominal(&mut self, name: &str, span: Span) -> R<Option<&'n Item>> {
        if let Some(instantiated) = self.instantiated.filter(|items| !items.is_empty()) {
            self.budget.identity_work(
                (1 + instantiated.len().checked_ilog2().unwrap_or(0) as usize)
                    .saturating_mul(name.len().saturating_add(1)),
                span,
            )?;
            if let Some(item) = instantiated.get(name) {
                return Ok(Some(item));
            }
        }
        Ok(self.nominals.get(name).copied())
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
        }) = self.nominal(&path.join("."), ty.span)?
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
            match (&parameter.kind, argument) {
                (GenericParamKind::Type(bounds), GenericArg::Type(ty)) => {
                    self.budget.ty(ty)?;
                    for bound in bounds {
                        if is_bound(bound) {
                            self.copy(ty)?;
                        }
                    }
                    substitutions.insert(parameter.name.clone(), ty.clone());
                }
                (GenericParamKind::Const(domain), GenericArg::Const(value)) => {
                    let source = self.source.ok_or_else(|| {
                        unsupported(ty.span, "Copy const actual needs source binder proof")
                    })?;
                    let symbolic = if let ExprKind::Name(name) = &value.kind {
                        consts::symbolic_parameter(source, name).map(|(_, parameter)| parameter)
                    } else {
                        None
                    };
                    if let Some(GenericParam {
                        kind: GenericParamKind::Const(actual_domain),
                        ..
                    }) = symbolic
                    {
                        if type_key(actual_domain, self.budget)? != type_key(domain, self.budget)? {
                            return Err(error(
                                value.span,
                                "Copy const actual domain differs from source binder",
                            ));
                        }
                        substitutions
                            .insert(parameter.name.clone(), consts::carrier(value, self.budget)?);
                    } else {
                        let carrier = consts::carrier(value, self.budget)?;
                        substitutions.insert(
                            parameter.name.clone(),
                            consts::normalize(parameter, &carrier, self.budget)?,
                        );
                    }
                }
                (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                    let source = self.source.ok_or_else(|| {
                        unsupported(ty.span, "Copy life actual needs source binder proof")
                    })?;
                    self.budget.text(name, ty.span)?;
                    if !source
                        .generics
                        .iter()
                        .any(|p| p.name == *name && matches!(p.kind, GenericParamKind::Lifetime))
                    {
                        return Err(error(ty.span, "Copy life actual has no source binder"));
                    }
                }
                _ => {
                    return Err(error(
                        ty.span,
                        "Copy actual kind differs from nominal declaration",
                    ));
                }
            }
        }
        Ok((aggregate, substitutions))
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
        if let Some((available, _)) = self.symbolic
            && available
                .first_key_value()
                .is_some_and(|(key, _)| key.starts_with("\0subject:"))
        {
            let key = trait_bounds::subject_key(ty, self.budget)?;
            if available
                .get(&key)
                .is_some_and(|bounds| bounds.iter().any(|bound| bound == BOUND))
            {
                return Ok(true);
            }
        }
        match &grouped(ty).kind {
            TypeKind::Union(fields) => {
                for field in fields {
                    self.copy(field)?;
                }
                Ok(true)
            }
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
            TypeKind::ForLifetime { .. } => {
                // A retained invocation relation belongs to a Copy code pointer;
                // its address parameters are signature metadata, never fields.
                if rich_pointer::key(ty, self.budget)?.is_none() {
                    type_keys::prepay(ty, self.budget)?;
                }
                match super::super::lifetimes::pointer_type(ty) {
                    zil_ir::Type::BorrowedFunctionPointer { .. }
                    | zil_ir::Type::FunctionPointer { .. } => Ok(true),
                    _ => Err(error(
                        ty.span,
                        "Copy borrowed code requires a complete function signature",
                    )),
                }
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
                    if let Some((available, variables)) = self.symbolic
                        && variables.contains(&path[0])
                    {
                        if available
                            .get(&path[0])
                            .is_some_and(|bounds| bounds.iter().any(|b| b == BOUND))
                        {
                            // Symbolic Copy proves eligibility, never nonzero storage.
                            // `require` discards this flag; Store uses concrete only.
                            return Ok(true);
                        }
                        return Err(error(
                            ty.span,
                            "symbolic type does not satisfy its Copy bound",
                        ));
                    }
                    match path[0].as_str() {
                        "void" => return Ok(false),
                        "bool" | "i32" | "i64" | "u64" | "usize" | "f32" | "f64" | "u8" | "u16"
                        | "c_handle" | "c_wstr" => {
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
                let name = path.join(".");
                // Copy qualification distinguishes Box<Box<i32>> from Box<i32>.
                // The complete instance key pays its normal syntax work first.
                // StoreElement retains its prior nominal activity rule and cost.
                let key = if self.symbolic.is_some() {
                    let key = match type_key(ty, self.budget) {
                        Ok(key) => key,
                        Err(diagnostic)
                            if diagnostic.message
                                == "lifetime instance identity requires scoped canonical storage relation" =>
                        {
                            let Some(source) = self.source else {
                                return Err(diagnostic);
                            };
                            check::source_class_key(ty, source, self.budget)?
                        }
                        Err(diagnostic) => return Err(diagnostic),
                    };
                    Cow::Owned(key)
                } else {
                    Cow::Borrowed(name.as_str())
                };
                if !self.active.insert(key.to_string()) {
                    return Err(error(
                        ty.span,
                        "StoreElement requires a finite nonrecursive layout",
                    ));
                }
                let outcome = match self.nominal(&name, ty.span)? {
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
                            if let GenericParamKind::Type(bounds) = &p.kind {
                                for bound in bounds {
                                    if is_bound(bound) {
                                        self.copy(ty)?;
                                    }
                                }
                            }
                            substitutions.insert(p.name.clone(), ty.clone());
                        }
                        for variant in &e.variants {
                            self.budget.node(variant.span, 0)?;
                            match &variant.fields {
                                VariantFields::Unit => {}
                                VariantFields::Tuple(fields) => {
                                    for field in fields {
                                        if field.manages {
                                            return Err(error(
                                                field.span,
                                                "Copy enum field cannot own resources",
                                            ));
                                        }
                                        let field_type = self.substitute(field, &substitutions)?;
                                        self.copy(&field_type)?;
                                    }
                                }
                                VariantFields::Record(fields) => {
                                    for field in fields {
                                        if field.manages {
                                            return Err(error(
                                                field.span,
                                                "Copy enum field cannot own resources",
                                            ));
                                        }
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
                        format!(
                            "StoreElement fields require finite Copy or explicit direct manages resources: unresolved nominal {name}"
                        ),
                    )),
                };
                self.active.remove(key.as_ref());
                outcome
            }
            _ => Err(error(
                ty.span,
                "StoreElement fields cannot contain addresses, borrowed or unknown ownership shapes",
            )),
        }
    }
}
