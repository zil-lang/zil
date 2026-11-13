//! Persistent nominal capture-domain proposals from original type-use recipes
//! at the real environment operand. No runtime address or cleanup is created.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeMap;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-GHOST-CAPTURE", Span::default(), message)
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "semantic nominal capture work exceeded", None)
    })?;
    Ok(())
}
pub(super) fn populate(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    pay(
        left,
        program
            .instances
            .len()
            .saturating_add(module.closures.len()),
    )?;
    let mut changes = Vec::new();
    for instance in &program.instances {
        let Some(environment) = &instance.environment else {
            continue;
        };
        let SemanticClosureTarget::Typed(id) = environment.closure else {
            return Err(bad("ghost environment retains a Core closure"));
        };
        pay(
            left,
            module
                .closures
                .len()
                .saturating_add(program.definitions.len()),
        )?;
        let d = module
            .closures
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("ghost environment has no actual closure definition"))?;
        if d.storage_life.is_some() {
            continue;
        }
        let original = program
            .definitions
            .iter()
            .find(|r| r.id == instance.definition)
            .ok_or_else(|| bad("ghost capture lost its original declaration"))?;
        let (SemanticDefinitionBody::Function { sites: recipes, .. }
        | SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. }) = &original.body
        else {
            return Err(bad("ghost capture original is not executable"));
        };
        let mut images = Vec::new();
        let mut classes = Vec::new();
        let mut matched = BTreeMap::new();
        for site in &program.sites {
            pay(left, 1)?;
            if site.instance != instance.id {
                continue;
            }
            let SemanticSiteAction::TypeUse {
                subject: SemanticOperand::Capture(capture),
                ..
            } = site.action
            else {
                continue;
            };
            let Some(ty @ Type::BorrowedStorage { storage, life }) =
                d.captures.get(capture as usize)
            else {
                continue;
            };
            if !matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                || !life.slots.is_empty()
                || !life.retained.is_empty()
            {
                continue;
            }
            pay(left, recipes.len())?;
            let recipe = recipes
                .iter()
                .find(|r| r.id == site.recipe)
                .ok_or_else(|| bad("capture observation has no original recipe"))?;
            let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                return Err(bad("capture observation changes original site kind"));
            };
            let value = super::materialize::type_at(
                program,
                module,
                ledger,
                instance.id,
                expected,
                Some(ty),
                limits,
                left,
            )?;
            let SemanticTypeValue::Resolved {
                life: actual,
                classes: actual_images,
                ..
            } = value
            else {
                return Err(bad("Core type survives nominal capture interpretation"));
            };
            if &actual != life || actual_images.len() != life.binder_count as usize {
                return Err(bad(
                    "capture's nominal domain contradicts its actual complete type",
                ));
            }
            pay(
                left,
                actual_images
                    .len()
                    .saturating_add(original.environment.len()),
            )?;
            for (class, image) in actual_images.into_iter().enumerate() {
                let valid = match &image {
                    SemanticClassExpr::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    } => {
                        matches!(
                            original.environment.get(*ordinal as usize).map(|r| &r.kind),
                            Some(SemanticFormal::Lifetime)
                        )
                    }
                    SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::Environment(ordinal),
                        path,
                        class: child_class,
                    } if path.is_empty() => {
                        if !matches!(
                            original.environment.get(*ordinal as usize).map(|r| &r.kind),
                            Some(SemanticFormal::Type { .. })
                        ) {
                            false
                        } else {
                            let recipe = SemanticTypeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: *ordinal,
                            };
                            let SemanticTypeValue::Resolved { life, .. } =
                                super::materialize::type_at(
                                    program,
                                    module,
                                    ledger,
                                    instance.id,
                                    &recipe,
                                    None,
                                    limits,
                                    left,
                                )?
                            else {
                                return Err(bad("sealed Type environment retains Core"));
                            };
                            *child_class < life.binder_count
                        }
                    }
                    _ => false,
                };
                if !valid {
                    return Err(bad(
                        "nominal capture image does not select its complete sealed environment class",
                    ));
                }
                pay(left, images.len())?;
                let fixed = match images.iter().position(|old| old == &image) {
                    Some(index) => index,
                    None => {
                        images.push(image);
                        images.len() - 1
                    }
                };
                let position = SignatureLifeClass {
                    base: StorageBase::Capture(capture),
                    path: Vec::new(),
                    class: class as u32,
                };
                let binding = SignatureLifeClassBinding {
                    position: position.clone(),
                    binder: StorageBinder::Fixed(fixed as u32),
                };
                pay(left, matched.len().saturating_add(1))?;
                if let Some(old) = matched.insert(position, binding.clone()) {
                    if old != binding {
                        return Err(bad("capture's repeated original observations disagree"));
                    }
                }
            }
        }
        if images.is_empty() {
            continue;
        }
        pay(left, d.captures.len().saturating_add(matched.len()))?;
        let mut complete = true;
        for (capture, ty) in d.captures.iter().enumerate() {
            match ty {
                Type::BorrowedStorage { life, .. } => {
                    for class in 0..life.binder_count {
                        pay(left, matched.len().saturating_add(1))?;
                        if !matched.contains_key(&SignatureLifeClass {
                            base: StorageBase::Capture(capture as u32),
                            path: Vec::new(),
                            class,
                        }) {
                            complete = false;
                        }
                    }
                }
                ty if ty.is_b0_scalar() => {}
                _ => complete = false,
            }
        }
        if !complete {
            continue;
        }
        classes.extend(matched.into_values());
        let life = StorageLifeContract {
            fixed_binder_count: images.len() as u32,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            static_bindings: Vec::new(),
            classes,
        };
        let embedding = images
            .into_iter()
            .enumerate()
            .map(|(fixed, actual)| SemanticHeaderBinding {
                header: StorageBinder::Fixed(fixed as u32),
                actual,
            })
            .collect::<Vec<_>>();
        changes.push((instance.id, id, d.function, life, embedding));
    }
    for (instance, closure, entry, life, embedding) in changes {
        pay(
            left,
            program
                .instances
                .len()
                .saturating_add(module.closures.len())
                .saturating_add(module.functions.len())
                .saturating_add(life.classes.len().saturating_mul(2)),
        )?;
        let instance = program
            .instances
            .iter_mut()
            .find(|r| r.id == instance)
            .ok_or_else(|| bad("actual ghost instance disappeared"))?;
        if !instance.header_embedding.is_empty() && instance.header_embedding != embedding {
            return Err(bad(
                "claimed ghost embedding disagrees with actual nominal observations",
            ));
        }
        instance.header_embedding = embedding;
        let d = module
            .closures
            .iter_mut()
            .find(|r| r.id == closure)
            .ok_or_else(|| bad("actual ghost closure disappeared"))?;
        d.storage_life = Some(life.clone());
        let f = module
            .functions
            .iter_mut()
            .find(|r| r.id == entry)
            .ok_or_else(|| bad("actual ghost entry disappeared"))?;
        if f.storage_life.is_some() && f.storage_life.as_ref() != Some(&life) {
            return Err(bad(
                "actual ghost entry already has a contradictory class directory",
            ));
        }
        f.storage_life = Some(life);
    }
    Ok(())
}
