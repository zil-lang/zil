//! Transport the exact actual impl frame into a table's complete Context.
//! This is a proposal, not a where/associated/provider execution theorem.
use super::*;

fn independent_members(declaration: &DynDeclaration) -> bool {
    declaration.requirements.as_ref().is_some_and(Vec::is_empty)
        && declaration
            .direct_supertraits
            .as_ref()
            .is_some_and(Vec::is_empty)
        && declaration
            .associated_definitions
            .as_ref()
            .is_some_and(Vec::is_empty)
}
fn bind(
    mapping: &mut [Option<StorageBinder>],
    from: StorageBinder,
    to: StorageBinder,
    context: u32,
    left: &mut usize,
) -> R<()> {
    provider_fee(left, 1)?;
    let (StorageBinder::Fixed(from), StorageBinder::Fixed(to_index)) = (from, to) else {
        return Err(bad(
            "Dyn impl Context changes a persistent class into an invocation",
        ));
    };
    if to_index >= context {
        return Err(bad("Dyn impl Context image is outside its complete domain"));
    }
    let slot = mapping
        .get_mut(from as usize)
        .ok_or_else(|| bad("Dyn impl class is outside its complete lexical domain"))?;
    if slot.is_some_and(|old| old != to) {
        return Err(bad("Dyn impl class has contradictory Context images"));
    }
    *slot = Some(to);
    Ok(())
}
fn pair(
    mapping: &mut [Option<StorageBinder>],
    local: &DynGenericActual,
    context: &DynGenericActual,
    count: u32,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(local, limits, left, 0)?;
    crate::dyn_metadata::semantic_operand_snapshot(context, limits, left, 0)?;
    match (local, context) {
        (DynGenericActual::Type(a), DynGenericActual::Type(b)) if a == b => Ok(()),
        (DynGenericActual::Const(a), DynGenericActual::Const(b)) if a == b => Ok(()),
        (DynGenericActual::Life(a), DynGenericActual::Life(b)) => {
            bind(mapping, *a, *b, count, left)
        }
        (DynGenericActual::QualifiedType(a), DynGenericActual::QualifiedType(b))
            if a.ty == b.ty
                && a.life == b.life
                && a.classes.len() == b.classes.len()
                && a.classes.len() == a.life.binder_count as usize =>
        {
            for (a, b) in a.classes.iter().zip(&b.classes) {
                bind(mapping, *a, *b, count, left)?;
            }
            Ok(())
        }
        _ => Err(bad(
            "Dyn impl Context changes the complete mixed actual vector",
        )),
    }
}

pub(super) fn complete(
    program: &SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut changes = Vec::new();
    provider_fee(left, module.dyn_vtables.len())?;
    for (ti, table) in module.dyn_vtables.iter().enumerate() {
        provider_fee(
            left,
            module
                .dyn_interfaces
                .len()
                .saturating_add(module.dyn_declarations.len()),
        )?;
        let mut interfaces = module
            .dyn_interfaces
            .iter()
            .filter(|row| row.id == table.interface);
        let interface = interfaces
            .next()
            .ok_or_else(|| bad("Dyn impl proposal has no actual interface"))?;
        if interfaces.next().is_some() {
            return Err(bad("Dyn impl proposal interface is ambiguous"));
        }
        let mut declarations = module
            .dyn_declarations
            .iter()
            .filter(|row| row.id == table.implementation);
        let declaration = declarations
            .next()
            .ok_or_else(|| bad("Dyn impl proposal has no original declaration"))?;
        if declarations.next().is_some() {
            return Err(bad("Dyn impl proposal declaration is ambiguous"));
        }
        let mut traits = module
            .dyn_declarations
            .iter()
            .filter(|row| row.id == interface.application.declaration);
        let trait_declaration = traits
            .next()
            .ok_or_else(|| bad("Dyn impl proposal has no original trait"))?;
        if traits.next().is_some() {
            return Err(bad("Dyn impl proposal trait is ambiguous"));
        }
        // More general graphs retain their authored proposals and all their
        // independent obligations. Missing requirements never mean empty.
        if !independent_members(declaration)
            || !independent_members(trait_declaration)
            || !interface.inherited.is_empty()
            || !interface.associated_types.is_empty()
            || !interface.associated_constants.is_empty()
        {
            continue;
        }
        let original = exact_scope(
            program,
            SemanticDynOwner::Declaration(table.implementation),
            left,
        )?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
            declaration: original_declaration,
            trait_application,
            ..
        }) = &original.body
        else {
            return Err(bad("Dyn impl proposal has no original trait application"));
        };
        if *original_declaration != table.implementation
            || trait_application.declaration != interface.application.declaration
            || trait_application.arguments.len() != interface.application.actuals.len()
        {
            return Err(bad(
                "Dyn impl proposal changes its original trait application",
            ));
        }
        // Each table entry selects the original emitted method frame. A
        // same-shaped receiver or a same-numbered binder cannot select it.
        let mut selected = None;
        provider_fee(left, table.entries.len())?;
        for entry in &table.entries {
            provider_fee(left, program.instances.len())?;
            let mut rows = program.instances.iter().filter(|row| matches!(row.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == entry.function));
            let method = rows
                .next()
                .ok_or_else(|| bad("Dyn impl table entry lacks an original emitted instance"))?;
            if rows.next().is_some() {
                return Err(bad("Dyn impl table entry has ambiguous original instances"));
            }
            let parent = exact_instance(
                program,
                method
                    .parent
                    .ok_or_else(|| bad("Dyn impl method lacks its lexical parent"))?,
                left,
            )?;
            if parent.definition != original.id
                || !matches!(parent.target, SemanticInstanceTarget::Scope)
            {
                return Err(bad(
                    "Dyn impl table entry changes its exact original lexical scope",
                ));
            }
            if selected.is_some_and(|id| id != parent.id) {
                return Err(bad(
                    "Dyn impl table combines different actual lexical instances",
                ));
            }
            selected = Some(parent.id);
        }
        let Some(selected) = selected else { continue };
        let instance = exact_instance(program, selected, left)?;
        let value = crate::semantics::materialize::type_at(
            program,
            module,
            ledger,
            instance.id,
            &SemanticTypeRecipe::SelfType {
                scope: SemanticScopeRef::Ancestor(0),
            },
            Some(&table.self_type),
            limits,
            left,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(&value, limits, left, 0)?;
        if instance.self_type.as_ref() != Some(&value)
            || !matches!(&value, SemanticTypeValue::Resolved { ty, .. } if *ty == table.self_type)
        {
            return Err(bad(
                "Dyn impl table Self differs from its independently materialized original scope",
            ));
        }
        let mut domain = domain_for(program, instance.id, false, limits, left)?;
        let SemanticTypeValue::Resolved { classes, .. } = &value else {
            unreachable!()
        };
        for image in classes {
            domain.bind(shifted(image, 0, limits, left)?, false, limits, left)?;
        }
        let receiver = domain.value(&value, 0, limits, left)?;
        let actuals = domain.actuals(instance, 0, limits, left)?;
        if actuals.len() != table.actuals.len() {
            return Err(bad("Dyn impl table omits a full original actual"));
        }
        let mut application_actuals = Vec::new();
        for recipe in &trait_application.arguments {
            let actual = crate::semantics::materialize::actual_at(
                program,
                module,
                ledger,
                instance.id,
                recipe,
                limits,
                left,
            )?;
            push(
                &mut application_actuals,
                domain.actual(&actual, 0, limits, left)?,
                limits,
                left,
            )?;
        }
        let mut mapping = Vec::new();
        for _ in &domain.fixed {
            push(&mut mapping, None, limits, left)?;
        }
        for (local, context) in actuals.iter().zip(&table.actuals).chain(
            application_actuals
                .iter()
                .zip(&interface.application.actuals),
        ) {
            pair(
                &mut mapping,
                local,
                context,
                interface.retained_life.binder_count,
                limits,
                left,
            )?;
        }
        if let Some(life) = &table.self_life {
            provider_fee(left, table.self_classes.len())?;
            if life != &receiver.life || table.self_classes.len() != receiver.classes.len() {
                return Err(bad("Dyn impl table changes the complete Self domain"));
            }
            for (local, context) in receiver.classes.iter().zip(&table.self_classes) {
                bind(
                    &mut mapping,
                    *local,
                    *context,
                    interface.retained_life.binder_count,
                    left,
                )?;
            }
        } else if !table.self_classes.is_empty() {
            return Err(bad(
                "Dyn impl table has classes without a complete Self shape",
            ));
        }
        let mut to_context = Vec::new();
        for (ordinal, image) in mapping.iter().enumerate() {
            let image = image.ok_or_else(|| {
                unknown("Dyn impl proposal needs its complete original-to-Context class image")
            })?;
            push(
                &mut to_context,
                DynLifeEmbedding {
                    from: StorageBinder::Fixed(
                        u32::try_from(ordinal).map_err(|_| bad("Dyn impl domain overflows"))?,
                    ),
                    to: image,
                },
                limits,
                left,
            )?;
        }
        let mut self_classes = Vec::new();
        for class in &receiver.classes {
            let StorageBinder::Fixed(index) = class else {
                return Err(bad("Dyn impl Self contains an invocation class"));
            };
            let image = mapping
                .get(*index as usize)
                .and_then(|image| *image)
                .ok_or_else(|| bad("Dyn impl Self omits a Context image"))?;
            push(&mut self_classes, image, limits, left)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&receiver, limits, left, 1)?;
        let life = receiver.life.clone();
        let witness = DynImplementationWitness {
            scope: DynInstantiationScope {
                fixed_binder_count: u32::try_from(domain.fixed.len())
                    .map_err(|_| bad("Dyn impl Fixed domain overflows"))?,
                invocation_binder_count: 0,
            },
            to_context,
            receiver,
            application: DynTraitApplication {
                declaration: trait_application.declaration,
                actuals: application_actuals,
            },
            implementation: table.implementation,
            actuals,
            associated: Vec::new(),
        };
        crate::dyn_metadata::semantic_operand_snapshot(&witness, limits, left, 1)?;
        let mut witnesses = Vec::new();
        push(&mut witnesses, witness, limits, left)?;
        if let Some(old) = &table.implementation_witnesses {
            for row in old {
                crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 0)?;
            }
            if old != &witnesses {
                return Err(bad(
                    "Dyn impl proposed witness changes its full original lexical instance",
                ));
            }
        }
        push(
            &mut changes,
            (ti, life, self_classes, witnesses),
            limits,
            left,
        )?;
    }
    for (index, life, classes, witnesses) in changes {
        let table = &mut module.dyn_vtables[index];
        table.self_life = Some(life);
        table.self_classes = classes;
        if table.implementation_witnesses.is_none() {
            table.implementation_witnesses = Some(witnesses);
        }
    }
    Ok(())
}
