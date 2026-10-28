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
    Shape {
        nominals,
        budget,
        active: BTreeSet::new(),
        depth: 0,
        symbolic: Some((available, variables)),
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
    budget: &'b mut Budget<'l>,
    active: BTreeSet<String>,
    depth: usize,
    symbolic: Option<(&'s trait_bounds::Bounds, &'s BTreeSet<String>)>,
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
                        "bool" | "i32" | "i64" | "u8" | "u16" | "c_handle" | "c_wstr" => {
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
                    Cow::Owned(type_key(ty, self.budget)?)
                } else {
                    Cow::Borrowed(name.as_str())
                };
                if !self.active.insert(key.to_string()) {
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
