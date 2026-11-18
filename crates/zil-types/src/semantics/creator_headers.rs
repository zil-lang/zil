//! Ordinary header proposals from complete original P/R queries. Existing
//! binder identities are preserved; caller images never determine a header.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeSet;
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

/// Early physical header proposals can split one declared lifetime into
/// several invocation entries. Rebuild that directory only after every P/R
/// row and every incoming claim has been checked against the original domain.
/// These images are materializer canonical original classes, not caller
/// instantiations. Fixed child coordinates remain distinct even when their
/// outside images coincide.
fn canonical_invocations(
    header: &mut StorageLifeContract,
    embedding: &mut Vec<SemanticHeaderBinding>,
    source_order: &[SemanticClassExpr],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if header.invocation_binder_count <= 1 {
        return Ok(());
    }
    let mut canonical: Vec<SemanticHeaderBinding> = Vec::new();
    let mut count = 0u32;
    pay(
        left,
        embedding
            .len()
            .saturating_add(header.invocation_binder_count as usize),
    )?;
    let mut remap = vec![None; header.invocation_binder_count as usize];
    for row in embedding
        .iter()
        .filter(|row| matches!(row.header, StorageBinder::Fixed(_)))
    {
        crate::dyn_metadata::semantic_operand_snapshot(&row.actual, limits, left, 1)?;
        canonical.push(row.clone());
    }
    // A descriptor inferred from an owning parameter can precede a written
    // address in source. Allocate class numbers at their first P/R occurrence
    // after both have been formed, preserving equalities in the original scope.
    for image in source_order {
        pay(left, embedding.len().saturating_add(1))?;
        let mut selected = None;
        for row in embedding.iter() {
            let StorageBinder::Invocation(old) = row.header else { continue; };
            crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&row.actual, limits, left, 1)?;
            if &row.actual != image { continue; }
            let slot = remap.get_mut(old as usize)
                .ok_or_else(|| bad("original invocation header leaves its declared domain"))?;
            if let Some(ordinal) = *slot {
                selected = Some(ordinal);
                continue;
            }
            let ordinal = match selected {
                Some(ordinal) => ordinal,
                None => {
                    let ordinal = count;
                    count = count.checked_add(1)
                        .ok_or_else(|| bad("canonical invocation header overflows"))?;
                    crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
                    canonical.push(SemanticHeaderBinding {
                        header: StorageBinder::Invocation(ordinal), actual: image.clone(),
                    });
                    selected = Some(ordinal);
                    ordinal
                }
            };
            *slot = Some(ordinal);
        }
    }
    if remap.contains(&None) {
        return Err(bad("invocation class lacks an occurrence in the original complete signature"));
    }
    pay(
        left,
        header.bindings.len().saturating_add(header.classes.len()),
    )?;
    for binder in header
        .bindings
        .iter_mut()
        .map(|row| &mut row.binder)
        .chain(header.classes.iter_mut().map(|row| &mut row.binder))
    {
        if let StorageBinder::Invocation(old) = binder {
            pay(left, 1)?;
            *old = remap.get(*old as usize).copied().flatten()
                .ok_or_else(|| bad("invocation header row leaves its original full directory"))?;
        }
    }
    header.invocation_binder_count = count;
    *embedding = canonical;
    Ok(())
}
pub(super) fn populate(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    populate_proposals(program, module, ledger, limits, left, None).map(|_| ())
}

pub(super) fn populate_closure_results(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
    requested: &BTreeSet<ClosureId>,
) -> R<bool> {
    populate_proposals(program, module, ledger, limits, left, Some(requested))
}

fn populate_proposals(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
    requested: Option<&BTreeSet<ClosureId>>,
) -> R<bool> {
    use super::materialize::Formation;

    let view = if let Some(context) = ledger.original.context() {
        super::materialize::ActualDirectory::indexed(
            module,
            program,
            context,
            "creator_headers",
            limits,
            left,
        )?
    } else {
        super::materialize::ActualDirectory::from_module_at(module, "creator_headers")
    };
    let mut changes = Vec::new();
    'instances: for instance in &program.instances {
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
        if let Some(requested) = requested {
            let Type::Closure(closure) = stored::physical(&function.return_type) else {
                continue;
            };
            pay(left, requested.len().saturating_add(1).ilog2() as usize + 1)?;
            if !requested.contains(closure) {
                continue;
            }
        }
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
        let mut source_order = Vec::new();
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
            let value = super::materialize::form_type_in_view(
                program,
                &view,
                ledger,
                instance.id,
                &recipe,
                Some(ty),
                limits,
                left,
            )?;
            let value = match value {
                Formation::Complete(value) => value,
                Formation::NotReady { .. } if requested.is_some() => continue 'instances,
                Formation::NotReady { .. } => {
                    return Err(bad("ordinary parameter awaits a complete closure header"));
                }
            };
            rows::append(
                &value,
                StorageBase::Parameter(ordinal as u32),
                &mut bindings,
                &mut classes,
                &mut statics,
                limits,
                left,
            )?;
            if let SemanticTypeValue::Resolved { classes, .. } = value {
                pay(left, classes.len())?;
                source_order.extend(classes);
            }
        }
        let recipe = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        };
        let value = super::materialize::form_type_in_view(
            program,
            &view,
            ledger,
            instance.id,
            &recipe,
            Some(&function.return_type),
            limits,
            left,
        )?;
        let value = match value {
            Formation::Complete(value) => value,
            Formation::NotReady { .. } if requested.is_some() => continue,
            Formation::NotReady { .. } => {
                return Err(bad("ordinary result awaits a complete closure header"));
            }
        };
        rows::append(
            &value,
            StorageBase::Result,
            &mut bindings,
            &mut classes,
            &mut statics,
            limits,
            left,
        )?;
        if let SemanticTypeValue::Resolved { classes, .. } = value {
            pay(left, classes.len())?;
            source_order.extend(classes);
        }
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
        canonical_invocations(&mut proposed, &mut embedding, &source_order, limits, left)?;
        if requested.is_none()
            || function.storage_life.as_ref() != Some(&proposed)
            || instance.header_embedding != embedding
        {
            changes.push((instance.id, id, proposed, embedding));
        }
    }
    drop(view);
    let progress = !changes.is_empty();
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
    Ok(progress)
}
