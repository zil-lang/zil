//! Interpret original impl formals through real nominal occurrences. A local
//! Self class number and an impl formal ordinal are distinct namespaces.
use super::*;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-PATTERN", None, message)
}
struct Match<'a> {
    module: &'a TypedModule,
    table: &'a DynVtable,
    root: &'a StorageLifeShape,
}
impl Match<'_> {
    fn nominal(
        &self,
        pattern: &DynSelfPattern,
        actual: &Type,
        path: &[LifeTypeProjection],
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("impl Self recipe recursion exceeds selected depth"));
        }
        b.spend(
            self.module
                .life_instances
                .len()
                .saturating_add(path.len())
                .saturating_add(1),
        )?;
        dynamic_metadata::ty(actual, b)?;
        let mut selected = None;
        for instance in &self.module.life_instances {
            dynamic_metadata::ty(&instance.ty, b)?;
            if instance.ty == *actual && selected.replace(instance).is_some() {
                return Err(bad("Self recipe has ambiguous actual nominal identity"));
            }
        }
        let instance =
            selected.ok_or_else(|| bad("Self recipe lacks its actual nominal instance"))?;
        if instance.declaration != pattern.declaration
            || instance.arguments.len() != pattern.arguments.len()
        {
            return Err(bad(
                "Self recipe declaration or full actual cardinality differs",
            ));
        }
        b.spend(pattern.arguments.len())?;
        for (ordinal, (recipe, actual)) in pattern
            .arguments
            .iter()
            .zip(&instance.arguments)
            .enumerate()
        {
            match (recipe, actual) {
                (
                    DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(formal)),
                    LifeNominalActual::Lifetime(_),
                ) => {
                    b.spend(
                        self.root
                            .nominal_arguments
                            .len()
                            .saturating_mul(path.len().saturating_add(1)),
                    )?;
                    let mut matches = self.root.nominal_arguments.iter().filter(|row| {
                        row.path == path
                            && row.declaration == instance.declaration
                            && row.argument as usize == ordinal
                    });
                    let occurrence = matches
                        .next()
                        .ok_or_else(|| bad("actual Self lifetime occurrence is absent"))?;
                    let expected = self
                        .table
                        .self_classes
                        .get(occurrence.binder as usize)
                        .ok_or_else(|| bad("Self lifetime occurrence has no complete class map"))?;
                    if matches.next().is_some()
                        || !matches!(self.table.actuals.get(*formal as usize), Some(DynGenericActual::Life(class)) if class == expected)
                    {
                        return Err(bad(
                            "ordered impl life actual differs from its real Self occurrence",
                        ));
                    }
                }
                (DynPatternActual::Const(recipe), LifeNominalActual::Const(actual)) => {
                    if const_value(recipe, self.table, b)? != *actual {
                        return Err(bad("ordered impl constant actual differs from Self"));
                    }
                }
                (DynPatternActual::Type(recipe), LifeNominalActual::Type(actual)) => {
                    b.spend(path.len().saturating_add(1))?;
                    let mut nested = path.to_vec();
                    nested.push(LifeTypeProjection::TypeArgument(ordinal as u32));
                    self.ty(recipe, actual, &nested, depth + 1, b)?;
                }
                _ => {
                    return Err(bad(
                        "Self recipe actual kind differs from real nominal argument",
                    ));
                }
            }
        }
        Ok(())
    }
    fn ty(
        &self,
        recipe: &DynTypePattern,
        actual: &Type,
        path: &[LifeTypeProjection],
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("Self type recipe exceeds selected depth"));
        }
        b.spend(path.len().saturating_add(1))?;
        match recipe {
            DynTypePattern::Nominal(pattern) => self.nominal(pattern, actual, path, depth + 1, b),
            DynTypePattern::Concrete(expected) => {
                dynamic_metadata::ty(expected, b)?;
                dynamic_metadata::ty(actual, b)?;
                if !crate::dynamic_ports::closed_targets(self.module, expected, b)?
                    || expected != actual
                {
                    return Err(bad(
                        "concrete Self recipe is not the complete closed actual type",
                    ));
                }
                Ok(())
            }
            DynTypePattern::Parameter(formal) => {
                let Some(DynGenericActual::Type(expected)) =
                    self.table.actuals.get(*formal as usize)
                else {
                    return Err(bad("Self type recipe selects a non-type formal"));
                };
                dynamic_metadata::ty(expected, b)?;
                dynamic_metadata::ty(actual, b)?;
                if expected != actual {
                    return Err(bad("ordered impl type actual differs from Self"));
                }
                // A borrowed type actual needs a separate joint class map;
                // equality of two local canonical numbers cannot supply it.
                if !crate::dynamic_ports::closed_targets(self.module, expected, b)? {
                    return Err(unsupported(
                        "borrowed impl Type formal lacks a complete joint class witness",
                        None,
                    ));
                }
                Ok(())
            }
            _ => {
                let expected = type_value(recipe, self.table, self.module, depth + 1, b)?;
                dynamic_metadata::ty(actual, b)?;
                if expected != *actual {
                    return Err(bad(
                        "structural Self recipe differs from its full actual type",
                    ));
                }
                Ok(())
            }
        }
    }
}
fn const_value(recipe: &DynConstPattern, table: &DynVtable, b: &mut Budget) -> Check<Constant> {
    b.spend(1)?;
    let value = match recipe {
        DynConstPattern::IntegerLiteral(_)
        | DynConstPattern::FloatLiteral(_)
        | DynConstPattern::MethodParameter(_)
        | DynConstPattern::Unary { .. }
        | DynConstPattern::Binary { .. }
        | DynConstPattern::Convert { .. }
        | DynConstPattern::AssociatedProjection { .. } => {
            return Err(bad(
                "constant declaration recipe requires its checked specialization",
            ));
        }
        DynConstPattern::Value(value) => value,
        DynConstPattern::Parameter(index) => match table.actuals.get(*index as usize) {
            Some(DynGenericActual::Const(value)) => value,
            _ => return Err(bad("constant recipe selects a non-constant actual")),
        },
    };
    if let Constant::WideString(bytes) = value {
        b.spend(bytes.len())?;
    }
    Ok(value.clone())
}
fn type_value(
    recipe: &DynTypePattern,
    table: &DynVtable,
    module: &TypedModule,
    depth: usize,
    b: &mut Budget,
) -> Check<Type> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("trait type recipe exceeds selected depth"));
    }
    let child = |r: &DynTypePattern, b: &mut Budget| type_value(r, table, module, depth + 1, b);
    Ok(match recipe {
        DynTypePattern::Semantic(_) => {
            return Err(unsupported(
                "semantic type recipe requires its complete lexical instance proof",
                None,
            ));
        }
        DynTypePattern::SelfType
        | DynTypePattern::MethodParameter(_)
        | DynTypePattern::AssociatedProjection { .. } => {
            return Err(bad(
                "type declaration recipe requires its checked receiver specialization",
            ));
        }
        DynTypePattern::Concrete(ty) => {
            dynamic_metadata::ty(ty, b)?;
            if !crate::dynamic_ports::closed_targets(module, ty, b)? {
                return Err(bad("concrete trait type recipe is not closed"));
            }
            ty.clone()
        }
        DynTypePattern::Parameter(index) => match table.actuals.get(*index as usize) {
            Some(DynGenericActual::Type(ty)) => {
                dynamic_metadata::ty(ty, b)?;
                if !crate::dynamic_ports::closed_targets(module, ty, b)? {
                    return Err(unsupported(
                        "borrowed trait Type actual lacks a complete joint class witness",
                        None,
                    ));
                }
                ty.clone()
            }
            _ => return Err(bad("trait type recipe selects a non-type actual")),
        },
        DynTypePattern::Nominal(_) => {
            return Err(unsupported(
                "trait nominal type recipe requires a complete application instance witness",
                None,
            ));
        }
        DynTypePattern::Address {
            mutable,
            lifetime,
            inner,
        } => {
            if lifetime.is_some() {
                return Err(unsupported(
                    "borrowed structural Type actual lacks its joint class witness",
                    None,
                ));
            }
            Type::Address {
                mutable: *mutable,
                inner: Box::new(child(inner, b)?),
            }
        }
        DynTypePattern::Tuple(fields) => {
            b.spend(fields.len())?;
            Type::Tuple(fields.iter().map(|t| child(t, b)).collect::<Check<_>>()?)
        }
        DynTypePattern::Array { element, length } => {
            let length = match const_value(length, table, b)? {
                Constant::Usize(n) => u32::try_from(n)
                    .map_err(|_| bad("array recipe length exceeds typed array domain"))?,
                _ => return Err(bad("array recipe length has the wrong exact constant type")),
            };
            Type::Array {
                element: Box::new(child(element, b)?),
                length,
            }
        }
        DynTypePattern::Slice(inner) => Type::Slice {
            element: Box::new(child(inner, b)?),
        },
        DynTypePattern::Managed(inner) => Type::Managed(Box::new(child(inner, b)?)),
        DynTypePattern::Desc { inner, .. } => Type::Desc(Box::new(child(inner, b)?)),
        DynTypePattern::OwnedStore(inner) => Type::OwnedStore {
            element: Box::new(child(inner, b)?),
        },
        DynTypePattern::ErrorUnion { error, result } => Type::ErrorUnion {
            error: Box::new(child(error, b)?),
            result: Box::new(child(result, b)?),
        },
    })
}
pub(super) fn provider(
    module: &TypedModule,
    table: &DynVtable,
    declaration: &DynDeclaration,
    iface: &DynInterface,
    b: &mut Budget,
) -> Check<()> {
    match &declaration.self_pattern {
        Some(DynTypePattern::Nominal(pattern)) => {
            let Type::BorrowedStorage { life, .. } = &table.self_type else {
                return Err(bad(
                    "Self recipe has no actual nominal instance class domain",
                ));
            };
            Match {
                module,
                table,
                root: life,
            }
            .nominal(pattern, &table.self_type, &[], 0, b)?;
        }
        Some(_) => {
            return Err(bad(
                "non-nominal Self recipe needs its complete type-domain witness",
            ));
        }
        None if declaration.parameters.is_empty() && table.self_classes.is_empty() => {}
        None => return Err(bad("generic provider Self pattern is Unknown")),
    }
    let pattern = declaration
        .implemented_trait
        .as_ref()
        .ok_or_else(|| bad("provider implemented trait application is Unknown"))?;
    if pattern.declaration != iface.application.declaration
        || pattern.arguments.len() != iface.application.actuals.len()
    {
        return Err(bad(
            "provider implements a different complete trait application",
        ));
    }
    for (recipe, actual) in pattern.arguments.iter().zip(&iface.application.actuals) {
        b.spend(1)?;
        let expected = match recipe {
            DynPatternActual::Life(
                zil_ir::DynLifePattern::MethodParameter(_)
                | zil_ir::DynLifePattern::Bound { .. }
                | zil_ir::DynLifePattern::Invocation(_)
                | DynLifePattern::ImplicitDeclaration(_),
            ) => {
                return Err(bad(
                    "persistent impl trait recipe cannot use a method or invocation lifetime",
                ));
            }
            DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(index)) => {
                match table.actuals.get(*index as usize) {
                    Some(DynGenericActual::Life(class)) => DynGenericActual::Life(*class),
                    _ => return Err(bad("trait life recipe selects a non-life formal actual")),
                }
            }
            DynPatternActual::Const(recipe) => {
                DynGenericActual::Const(const_value(recipe, table, b)?)
            }
            DynPatternActual::Type(recipe) => {
                DynGenericActual::Type(type_value(recipe, table, module, 0, b)?)
            }
        };
        if expected != *actual {
            return Err(bad(
                "provider ordered trait actual differs, including its phantom class or constant bits",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
