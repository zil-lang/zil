//! Source labels are resolved against one declaration before positional IR.
use super::*;

pub(super) fn profile(declaration: &EnumDecl) -> bool {
    declaration.variants.iter().any(|v| matches!(v.fields, VariantFields::Record(_)))
        || declaration.generics.iter().any(|p| matches!(&p.kind,
            GenericParamKind::Type(bounds) if bounds.iter().any(copy::is_bound)))
}

pub(super) fn variant<'a>(
    declaration: &'a EnumDecl,
    name: &str,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<&'a Variant> {
    for variant in &declaration.variants {
        budget.node(variant.span, 0)?;
        budget.text(&variant.name, variant.span)?;
        budget.text(name, span)?;
        if variant.name == name {
            return Ok(variant);
        }
    }
    Err(error(span, "unknown record enum variant"))
}

pub(super) fn fields<'a>(
    declaration: &'a EnumDecl,
    name: &str,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<&'a [Field]> {
    let variant = variant(declaration, name, span, budget)?;
    match &variant.fields {
        VariantFields::Record(fields) => Ok(fields),
        _ => Err(error(span, "named fields require a record enum variant, not a tuple or unit variant")),
    }
}

/// Each written label maps to one declaration position; visits and compared
/// text are charged before the search, including unsuccessful searches.
pub(super) fn order<'a>(
    declared: &[Field],
    supplied: impl IntoIterator<Item = (&'a str, Span)>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<Vec<usize>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (name, at) in supplied {
        let mut position = None;
        for (index, field) in declared.iter().enumerate() {
            budget.node(at, 0)?;
            budget.text(name, at)?;
            budget.text(&field.name, field.span)?;
            if field.name == name {
                position = Some(index);
                break;
            }
        }
        let position = position.ok_or_else(|| error(at, "unknown record enum field label"))?;
        budget.node(at, 0)?;
        if !seen.insert(position) {
            return Err(error(at, "duplicate record enum field label"));
        }
        result.push(position);
    }
    if result.len() != declared.len() {
        return Err(error(span, "record enum requires every declared field exactly once"));
    }
    Ok(result)
}

pub(super) fn validate(
    declaration: &EnumDecl,
    nominals: &BTreeMap<String, &Item>,
    traits: &trait_bounds::Index<'_>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let variables = declaration.generics.iter().map(|p| p.name.clone()).collect::<BTreeSet<_>>();
    let real_bounds = traits.parameter_bounds(&declaration.generics, budget)?;
    // Generic enum payload qualification is conditional on its actual finite
    // Copy fields. These local layout assumptions are never published as a
    // function's capabilities or used to discharge nested declared premises.
    let conditional = variables.iter().map(|name|
        (name.clone(), vec![trait_bounds::Bound::Copy])).collect();
    let mut variants = BTreeSet::new();
    for variant in &declaration.variants {
        budget.node(variant.span, 0)?;
        budget.text(&variant.name, variant.span)?;
        if !variants.insert(variant.name.as_str()) {
            return Err(error(variant.span, "duplicate record enum variant"));
        }
        let mut qualify = |ty: &TypeRef, manages: bool, span: Span| -> R<()> {
            if manages {
                return Err(unsupported(span, "record enum profile requires complete ordinary Copy payloads"));
            }
            traits.store_types(ty, &real_bounds, &variables, budget, callable::StoreCallbacks::None)?;
            copy::require(ty, nominals, &conditional, &variables, budget)
        };
        match &variant.fields {
            VariantFields::Unit => {}
            VariantFields::Tuple(fields) => {
                for field in fields {
                    qualify(&field.ty, field.manages, field.span)?;
                }
            }
            VariantFields::Record(fields) => {
                let mut labels = BTreeSet::new();
                for field in fields {
                    if field.public || field.default.is_some() || field.manages {
                        return Err(unsupported(field.span, "record enum field pub/default/manages is not implemented"));
                    }
                    // Attribute validation also runs in the ordinary collector.
                    if !field.attributes.is_empty() {
                        return Err(unsupported(field.span, "record enum field attributes are not implemented"));
                    }
                    if !labels.insert(field.name.as_str()) {
                        return Err(error(field.span, "duplicate record enum declaration field"));
                    }
                    qualify(&field.ty, false, field.span)?;
                }
            }
        }
    }
    Ok(())
}
