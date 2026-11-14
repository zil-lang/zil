//! Ordinary header proposals from complete original P/R queries. Existing
//! binder identities are preserved; caller images never determine a header.
use super::emission::Ledger;
use crate::*;
#[path = "creator_invocations.rs"]
mod invocations;
#[path = "header_rows.rs"]
pub(super) mod rows;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-CREATOR-HEADER", Span::default(), message)
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "semantic creator header work exceeded", None)
    })?;
    Ok(())
}
fn position_work(header: &StorageLifeContract) -> usize {
    header
        .bindings
        .iter()
        .fold(0usize, |n, row| {
            n.saturating_add(row.position.path.len().saturating_add(1))
        })
        .saturating_add(header.classes.iter().fold(0usize, |n, row| {
            n.saturating_add(row.position.path.len().saturating_add(1))
        }))
        .saturating_add(header.static_bindings.iter().fold(0usize, |n, row| {
            n.saturating_add(row.path.len().saturating_add(1))
        }))
}
fn insert(
    mapping: &mut std::collections::BTreeMap<StorageBinder, SemanticClassExpr>,
    binder: StorageBinder,
    image: &SemanticClassExpr,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    pay(left, mapping.len().saturating_add(1))?;
    crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
    if let Some(old) = mapping.get(&binder) {
        crate::dyn_metadata::semantic_operand_snapshot(old, limits, left, 1)?;
        if old != image {
            return Err(bad(
                "one incoming header binder has different original domain identities",
            ));
        }
    } else {
        mapping.insert(binder, image.clone());
    }
    Ok(())
}
fn choose(
    mapping: &mut std::collections::BTreeMap<StorageBinder, SemanticClassExpr>,
    image: &SemanticClassExpr,
    count: &mut u32,
    limits: &Limits,
    left: &mut usize,
) -> R<StorageBinder> {
    pay(left, mapping.len().saturating_add(1))?;
    let mut selected = None;
    let mut ambiguous = false;
    for (binder, old) in mapping.iter() {
        crate::dyn_metadata::semantic_operand_snapshot(old, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
        if old == image {
            if selected.replace(*binder).is_some() {
                ambiguous = true;
            }
        }
    }
    if !ambiguous {
        if let Some(binder) = selected {
            return Ok(binder);
        }
    }
    let binder = StorageBinder::Invocation(*count);
    *count = count
        .checked_add(1)
        .ok_or_else(|| bad("ordinary header domain overflows"))?;
    insert(mapping, binder, image, limits, left)?;
    Ok(binder)
}
pub(super) fn populate(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    invocations::seed(program, module, ledger, limits, left)?;
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
            .find(|function| function.id == id)
            .ok_or_else(|| bad("ordinary header target is absent"))?;
        let original = program
            .definitions
            .iter()
            .find(|definition| definition.id == instance.definition)
            .ok_or_else(|| bad("ordinary header original is absent"))?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &original.body
        else {
            continue;
        };
        if function.parameters.len() != signature.parameters.len() {
            return Err(bad("ordinary header changes original parameter arity"));
        }
        let mut bindings = Vec::new();
        let mut classes = Vec::new();
        let mut statics = Vec::new();
        pay(left, function.parameters.len().saturating_add(1))?;
        for (ordinal, local) in function.parameters.iter().enumerate() {
            let ty = &function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("ordinary header parameter local is absent"))?
                .ty;
            let recipe = SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Parameter(ordinal as u32),
            };
            let value = super::materialize::type_at(
                program,
                module,
                ledger,
                instance.id,
                &recipe,
                Some(ty),
                limits,
                left,
            )?;
            rows::append(
                &value,
                StorageBase::Parameter(ordinal as u32),
                &mut bindings,
                &mut classes,
                &mut statics,
                limits,
                left,
            )?;
        }
        let recipe = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        };
        let value = super::materialize::type_at(
            program,
            module,
            ledger,
            instance.id,
            &recipe,
            Some(&function.return_type),
            limits,
            left,
        )?;
        rows::append(
            &value,
            StorageBase::Result,
            &mut bindings,
            &mut classes,
            &mut statics,
            limits,
            left,
        )?;
        let mut mapping = std::collections::BTreeMap::new();
        let mut proposed = StorageLifeContract {
            fixed_binder_count: function
                .storage_life
                .as_ref()
                .map_or(0, |header| header.fixed_binder_count),
            invocation_binder_count: function
                .storage_life
                .as_ref()
                .map_or(0, |header| header.invocation_binder_count),
            bindings: Vec::new(),
            static_bindings: statics,
            classes: Vec::new(),
        };
        if let Some(old) = &function.storage_life {
            pay(
                left,
                position_work(old).saturating_mul(
                    bindings
                        .len()
                        .saturating_add(classes.len())
                        .saturating_add(proposed.static_bindings.len())
                        .saturating_add(1),
                ),
            )?;
            for row in &old.bindings {
                let mut matches = bindings
                    .iter()
                    .filter(|(position, _)| position == &row.position);
                let (_, image) = matches.next().ok_or_else(|| {
                    bad("existing header adds a physical occurrence absent from original P/R")
                })?;
                if matches.next().is_some() {
                    return Err(bad("ordinary original physical occurrence is ambiguous"));
                }
                insert(&mut mapping, row.binder, image, limits, left)?;
            }
            for row in &old.classes {
                let mut matches = classes
                    .iter()
                    .filter(|(position, _)| position == &row.position);
                let (_, image) = matches.next().ok_or_else(|| {
                    bad("existing header adds an intrinsic occurrence absent from original P/R")
                })?;
                if matches.next().is_some() {
                    return Err(bad("ordinary original intrinsic occurrence is ambiguous"));
                }
                insert(&mut mapping, row.binder, image, limits, left)?;
            }
            for position in &old.static_bindings {
                if proposed
                    .static_bindings
                    .iter()
                    .filter(|candidate| *candidate == position)
                    .count()
                    != 1
                {
                    return Err(bad(
                        "existing header changes an original program-static occurrence",
                    ));
                }
            }
            for fixed in 0..old.fixed_binder_count {
                if !mapping.contains_key(&StorageBinder::Fixed(fixed)) {
                    return Err(bad(
                        "existing Fixed binder has no complete original occurrence",
                    ));
                }
            }
            for invocation in 0..old.invocation_binder_count {
                if !mapping.contains_key(&StorageBinder::Invocation(invocation)) {
                    return Err(bad(
                        "existing Invocation binder has no complete original occurrence",
                    ));
                }
            }
        }
        pay(left, bindings.len().saturating_add(classes.len()))?;
        for (position, image) in bindings {
            let existing = function
                .storage_life
                .as_ref()
                .and_then(|old| old.bindings.iter().find(|row| row.position == position))
                .map(|row| row.binder);
            let binder = match existing {
                Some(binder) => binder,
                None => choose(
                    &mut mapping,
                    &image,
                    &mut proposed.invocation_binder_count,
                    limits,
                    left,
                )?,
            };
            proposed
                .bindings
                .push(StorageLifeBinding { position, binder });
        }
        for (position, image) in classes {
            let existing = function
                .storage_life
                .as_ref()
                .and_then(|old| old.classes.iter().find(|row| row.position == position))
                .map(|row| row.binder);
            let binder = match existing {
                Some(binder) => binder,
                None => choose(
                    &mut mapping,
                    &image,
                    &mut proposed.invocation_binder_count,
                    limits,
                    left,
                )?,
            };
            proposed
                .classes
                .push(SignatureLifeClassBinding { position, binder });
        }
        if function.storage_life.is_none()
            && mapping.is_empty()
            && proposed.static_bindings.is_empty()
        {
            continue;
        }
        pay(
            left,
            mapping
                .len()
                .saturating_add(instance.header_embedding.len())
                .saturating_add(1),
        )?;
        let mut embedding = Vec::new();
        for (header, actual) in mapping {
            if match header {
                StorageBinder::Fixed(index) => index >= proposed.fixed_binder_count,
                StorageBinder::Invocation(index) => index >= proposed.invocation_binder_count,
            } {
                return Err(bad(
                    "ordinary header binder exceeds its preserved complete domain",
                ));
            }
            embedding.push(SemanticHeaderBinding { header, actual });
        }
        for claim in &instance.header_embedding {
            crate::dyn_metadata::semantic_operand_snapshot(&claim.actual, limits, left, 1)?;
            let mut rows = embedding.iter().filter(|row| row.header == claim.header);
            let row = rows
                .next()
                .ok_or_else(|| bad("claimed header embedding adds an unknown original domain"))?;
            if rows.next().is_some() || row.actual != claim.actual {
                return Err(bad(
                    "claimed header embedding differs from the original P/R identity",
                ));
            }
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
            .find(|row| row.id == function)
            .ok_or_else(|| bad("ordinary header target disappeared"))?
            .storage_life = Some(header);
        program
            .instances
            .iter_mut()
            .find(|row| row.id == instance)
            .ok_or_else(|| bad("ordinary header scope disappeared"))?
            .header_embedding = embedding;
    }
    Ok(())
}
