//! Ordinary header proposals from complete original P/R type expressions.
//! Original images remain distinct from this call's actual substitutions.
use super::emission::Ledger;
use crate::*;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-CREATOR-HEADER", Span::default(), message)
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "semantic creator header work exceeded", None)
    })?;
    Ok(())
}
fn binder(
    images: &mut Vec<SemanticClassExpr>,
    image: SemanticClassExpr,
    left: &mut usize,
) -> R<StorageBinder> {
    pay(left, images.len().saturating_add(1))?;
    let index = match images.iter().position(|old| old == &image) {
        Some(index) => index,
        None => {
            images.push(image);
            images.len() - 1
        }
    };
    Ok(StorageBinder::Invocation(index as u32))
}
pub(super) fn populate(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut changes = Vec::new();
    for instance in &program.instances {
        pay(left, 1)?;
        if instance.environment.is_some() {
            continue;
        }
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
        else {
            continue;
        };
        pay(
            left,
            module
                .functions
                .len()
                .saturating_add(program.definitions.len()),
        )?;
        let function = module
            .functions
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| bad("creator target is absent"))?;
        let original = program
            .definitions
            .iter()
            .find(|d| d.id == instance.definition)
            .ok_or_else(|| bad("creator original is absent"))?;
        let SemanticDefinitionBody::Function { signature, .. } = &original.body else {
            continue;
        };
        if signature.parameters.len() != function.parameters.len() {
            return Err(bad("creator signature changes original arity"));
        }
        let eligible = function.parameters.iter().all(|local| {
            function.locals.get(local.0 as usize).is_some_and(|l| {
                l.ty.is_b0_scalar()
                    || matches!(&l.ty, Type::Address { inner, .. } if inner.is_b0_scalar())
            })
        }) && (function.return_type.is_b0_scalar()
            || matches!(function.return_type, Type::Closure(_)));
        if !eligible {
            continue;
        }
        let mut images = Vec::new();
        let mut proposed = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            static_bindings: Vec::new(),
            classes: Vec::new(),
        };
        for (index, (parameter, recipe)) in function
            .parameters
            .iter()
            .zip(&signature.parameters)
            .enumerate()
        {
            let ty = &function.locals[parameter.0 as usize].ty;
            let SemanticTypeValue::Resolved { life, classes, .. } = super::materialize::type_at(
                program,
                module,
                ledger,
                instance.id,
                &recipe.ty,
                Some(ty),
                limits,
                left,
            )?
            else {
                return Err(bad("creator input retains Core type"));
            };
            if let Type::Address { .. } = ty {
                if life.binder_count != 1
                    || classes.len() != 1
                    || life.slots.len() != 1
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                {
                    return Err(bad(
                        "creator scalar input has an incomplete actual outer occurrence",
                    ));
                }
                let binding = binder(
                    &mut images,
                    classes
                        .into_iter()
                        .next()
                        .ok_or_else(|| bad("creator input loses outer image"))?,
                    left,
                )?;
                proposed.bindings.push(StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(index as u32),
                        path: Vec::new(),
                    },
                    binder: binding,
                });
            } else if life.binder_count != 0 || !classes.is_empty() {
                return Err(bad("scalar creator input invents a free class"));
            }
        }
        if matches!(function.return_type, Type::Closure(_)) {
            let SemanticTypeValue::Resolved { life, classes, .. } = super::materialize::type_at(
                program,
                module,
                ledger,
                instance.id,
                &signature.result,
                Some(&function.return_type),
                limits,
                left,
            )?
            else {
                return Err(bad("creator result retains Core type"));
            };
            if life.binder_count as usize != classes.len() {
                return Err(bad("creator result loses its complete persistent domain"));
            }
            pay(left, classes.len())?;
            for (class, image) in classes.into_iter().enumerate() {
                let binding = binder(&mut images, image, left)?;
                proposed.classes.push(SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Result,
                        path: Vec::new(),
                        class: class as u32,
                    },
                    binder: binding,
                });
            }
        }
        if images.is_empty() {
            continue;
        }
        proposed.invocation_binder_count = images.len() as u32;
        if let Some(old) = &function.storage_life {
            pay(
                left,
                old.bindings
                    .len()
                    .saturating_mul(proposed.bindings.len())
                    .saturating_add(old.classes.len().saturating_mul(proposed.classes.len())),
            )?;
            if old.fixed_binder_count != 0
                || old.invocation_binder_count > proposed.invocation_binder_count
                || !old.static_bindings.is_empty()
                || old
                    .bindings
                    .iter()
                    .any(|row| !proposed.bindings.contains(row))
                || old
                    .classes
                    .iter()
                    .any(|row| !proposed.classes.contains(row))
            {
                return Err(bad(
                    "existing creator header contradicts the complete original P/R images",
                ));
            }
        }
        let embedding = images
            .into_iter()
            .enumerate()
            .map(|(i, actual)| SemanticHeaderBinding {
                header: StorageBinder::Invocation(i as u32),
                actual,
            })
            .collect::<Vec<_>>();
        if !instance.header_embedding.is_empty() && instance.header_embedding != embedding {
            return Err(bad(
                "claimed creator embedding differs from its complete original P/R images",
            ));
        }
        changes.push((instance.id, id, proposed, embedding));
    }
    for (instance, function, header, embedding) in changes {
        pay(
            left,
            module
                .functions
                .len()
                .saturating_add(program.instances.len()),
        )?;
        module
            .functions
            .iter_mut()
            .find(|f| f.id == function)
            .ok_or_else(|| bad("creator disappeared"))?
            .storage_life = Some(header);
        program
            .instances
            .iter_mut()
            .find(|i| i.id == instance)
            .ok_or_else(|| bad("creator scope disappeared"))?
            .header_embedding = embedding;
    }
    Ok(())
}
