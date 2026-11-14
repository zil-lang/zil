//! Complete Fixed images for whole captured values. The original Name query
//! and the actual reaching capture must agree before any header row is linked.
use super::*;

pub(in crate::semantics) fn populate(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &super::super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
    complete: &mut BTreeSet<SemanticInstanceId>,
) -> R<()> {
    let mut changes = Vec::new();
    pay(left, program.instances.len())?;
    for instance in &program.instances {
        let Some(environment) = &instance.environment else {
            continue;
        };
        let SemanticClosureTarget::Typed(id) = environment.closure else {
            return Err(bad("whole capture embedding retains a Core closure"));
        };
        pay(
            left,
            module
                .closures
                .len()
                .saturating_add(module.functions.len())
                .saturating_add(program.definitions.len()),
        )?;
        let definition = module
            .closures
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("whole capture has no actual definition"))?;
        let entry = module
            .functions
            .iter()
            .find(|f| f.id == definition.function)
            .ok_or_else(|| bad("whole capture has no actual entry"))?;
        let Some(header) = &definition.storage_life else {
            continue;
        };
        if header.fixed_binder_count == 0 {
            continue;
        }
        crate::borrowed_fp::charge_contract(header, |work| pay(left, work), 2)?;
        if let Some(actual) = &entry.storage_life {
            crate::borrowed_fp::charge_contract(actual, |work| pay(left, work), 2)?;
        }
        if entry.environment != Some(id) || entry.storage_life.as_ref() != Some(header) {
            return Err(bad(
                "whole capture and entry have different complete directories",
            ));
        }
        let original = program
            .definitions
            .iter()
            .find(|d| d.id == instance.definition)
            .ok_or_else(|| bad("whole capture lost its original definition"))?;
        let (SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. }
        | SemanticDefinitionBody::Function { sites: recipes, .. }) = &original.body
        else {
            return Err(bad("whole capture original has no code signature"));
        };
        let mut images = BTreeMap::new();
        for site in &program.sites {
            pay(left, 1)?;
            if site.instance != instance.id {
                continue;
            }
            let SemanticSiteAction::TypeUse { subject, .. } = &site.action else {
                continue;
            };
            let Some(capture) =
                super::super::ghost_capture::capture_at(entry, subject, &site.at, left)?
            else {
                continue;
            };
            let capture_ty = definition
                .captures
                .get(capture as usize)
                .ok_or_else(|| bad("whole capture observation leaves its actual environment"))?;
            let observed = match subject {
                SemanticOperand::Value(value) => entry.values.get(value.0 as usize),
                SemanticOperand::Local(local) => {
                    entry.locals.get(local.0 as usize).map(|local| &local.ty)
                }
                SemanticOperand::Capture(index) => definition.captures.get(*index as usize),
                _ => None,
            }
            .ok_or_else(|| bad("whole capture observation has no actual type"))?;
            inference::charge_type_snapshot(capture_ty, Span::default(), limits, left, 1)?;
            inference::charge_type_snapshot(observed, Span::default(), limits, left, 1)?;
            if observed != capture_ty {
                return Err(bad(
                    "whole capture observation changes its actual complete type",
                ));
            }
            pay(left, recipes.len())?;
            let mut matching = recipes.iter().filter(|recipe| recipe.id == site.recipe);
            let Some(recipe) = matching.next() else {
                return Err(bad("whole capture has no original TypeUse"));
            };
            if matching.next().is_some() {
                return Err(bad("whole capture has multiple original TypeUses"));
            }
            let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                return Err(bad("whole capture observation changes its original role"));
            };
            let value = super::super::materialize::type_at(
                program,
                module,
                ledger,
                instance.id,
                expected,
                Some(capture_ty),
                limits,
                left,
            )?;
            let mut bindings = Vec::new();
            let mut classes = Vec::new();
            let mut statics = Vec::new();
            super::super::creator_headers::rows::append(
                &value,
                StorageBase::Capture(capture),
                &mut bindings,
                &mut classes,
                &mut statics,
                limits,
                left,
            )?;
            for (position, image) in bindings {
                pay(
                    left,
                    header
                        .bindings
                        .len()
                        .saturating_mul(position.path.len().saturating_add(1)),
                )?;
                let mut rows = header
                    .bindings
                    .iter()
                    .filter(|row| row.position == position);
                let Some(row) = rows.next() else {
                    return Err(bad(
                        "whole capture outer occurrence is absent from its actual header",
                    ));
                };
                if rows.next().is_some() {
                    return Err(bad("whole capture outer occurrence is duplicated"));
                }
                put(
                    &mut images,
                    row.binder,
                    image,
                    original,
                    header,
                    limits,
                    left,
                )?;
            }
            for (position, image) in classes {
                pay(
                    left,
                    header
                        .classes
                        .len()
                        .saturating_mul(position.path.len().saturating_add(1)),
                )?;
                let mut rows = header.classes.iter().filter(|row| row.position == position);
                let Some(row) = rows.next() else {
                    return Err(bad(
                        "whole capture intrinsic class is absent from its actual header",
                    ));
                };
                if rows.next().is_some() {
                    return Err(bad("whole capture intrinsic class is duplicated"));
                }
                put(
                    &mut images,
                    row.binder,
                    image,
                    original,
                    header,
                    limits,
                    left,
                )?;
            }
            for position in statics {
                pay(
                    left,
                    header
                        .static_bindings
                        .len()
                        .saturating_mul(position.path.len().saturating_add(1)),
                )?;
                if header
                    .static_bindings
                    .iter()
                    .filter(|row| **row == position)
                    .count()
                    != 1
                {
                    return Err(bad(
                        "whole capture constant occurrence differs from its actual header",
                    ));
                }
            }
        }
        if images.len() != header.fixed_binder_count as usize {
            continue;
        }
        pay(left, header.fixed_binder_count as usize)?;
        if (0..header.fixed_binder_count).any(|fixed| !images.contains_key(&fixed)) {
            continue;
        }
        let embedding: Vec<_> = images
            .into_iter()
            .map(|(fixed, actual)| SemanticHeaderBinding {
                header: StorageBinder::Fixed(fixed),
                actual,
            })
            .collect();
        for row in &instance.header_embedding {
            crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
        }
        if !instance.header_embedding.is_empty() && instance.header_embedding != embedding {
            return Err(bad(
                "whole capture embedding differs from its original complete images",
            ));
        }
        changes.push((instance.id, embedding));
    }
    for (id, embedding) in changes {
        pay(
            left,
            program
                .instances
                .len()
                .saturating_add(complete.len().saturating_add(1).ilog2() as usize + 1),
        )?;
        let instance = program
            .instances
            .iter_mut()
            .find(|instance| instance.id == id)
            .ok_or_else(|| bad("whole capture instance disappeared"))?;
        instance.header_embedding = embedding;
        complete.insert(id);
    }
    Ok(())
}

fn put(
    images: &mut BTreeMap<u32, SemanticClassExpr>,
    binder: StorageBinder,
    image: SemanticClassExpr,
    original: &SemanticDefinition,
    header: &StorageLifeContract,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let StorageBinder::Fixed(fixed) = binder else {
        return Err(bad("persistent whole capture uses an invocation class"));
    };
    if fixed >= header.fixed_binder_count {
        return Err(bad("whole capture Fixed row exceeds its complete domain"));
    }
    pay(left, original.environment.len())?;
    let sealed = match &image {
        SemanticClassExpr::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } => matches!(
            original
                .environment
                .get(*ordinal as usize)
                .map(|row| &row.kind),
            Some(SemanticFormal::Lifetime)
        ),
        SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Environment(ordinal),
            path,
            ..
        } if path.is_empty() => matches!(
            original
                .environment
                .get(*ordinal as usize)
                .map(|row| &row.kind),
            Some(SemanticFormal::Type { .. })
        ),
        _ => false,
    };
    if !sealed {
        return Err(Diagnostic::unsupported(
            Phase::Types,
            "whole capture requires its checked sealed environment image",
            None,
        ));
    }
    crate::dyn_metadata::semantic_operand_snapshot(&image, limits, left, 1)?;
    pay(left, images.len().saturating_add(1).ilog2() as usize + 1)?;
    if let Some(prior) = images.get(&fixed) {
        crate::dyn_metadata::semantic_operand_snapshot(prior, limits, left, 1)?;
        if prior != &image {
            return Err(bad(
                "one actual Fixed class has different original capture images",
            ));
        }
    } else {
        images.insert(fixed, image);
    }
    Ok(())
}
