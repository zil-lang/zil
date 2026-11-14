//! Complete Fixed images for whole captured values. The original sealed
//! environment and exact creator capture selections bind every header row.
use super::*;
#[path = "header_capture_uses.rs"]
mod first_use;

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
        let captures = creator_captures(program, module, ledger, instance, id, limits, left)?;
        for (capture, ordinal, actual_ty) in captures {
            let Some(value) = capture_value(
                program, module, ledger, instance, original, recipes, ordinal, &actual_ty, limits,
                left,
            )?
            else {
                continue;
            };
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
        let mut embedding: Vec<_> = images
            .into_iter()
            .map(|(fixed, actual)| SemanticHeaderBinding {
                header: StorageBinder::Fixed(fixed),
                actual,
            })
            .collect();
        for row in &instance.header_embedding {
            crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
            match row.header {
                StorageBinder::Fixed(fixed) => {
                    if embedding.get(fixed as usize) != Some(row) {
                        return Err(bad(
                            "whole capture embedding differs from its original complete images",
                        ));
                    }
                }
                StorageBinder::Invocation(index) => {
                    if index >= header.invocation_binder_count {
                        return Err(bad(
                            "whole capture retains an out-of-domain invocation image",
                        ));
                    }
                    embedding.push(row.clone());
                }
            }
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

#[allow(clippy::too_many_arguments)]
fn capture_value(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &super::super::emission::Ledger,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    recipes: &[SemanticSiteRecipe],
    ordinal: u32,
    actual_ty: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<SemanticTypeValue>> {
    let expected = SemanticTypeRecipe::Environment {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    };
    let value = super::super::materialize::type_at(
        program,
        module,
        ledger,
        instance.id,
        &expected,
        None,
        limits,
        left,
    )?;
    let SemanticTypeValue::Resolved {
        ty: original_ty, ..
    } = &value
    else {
        return Err(bad("creator capture retains an unresolved original type"));
    };
    let value = if original_ty == actual_ty {
        value
    } else if let Type::Address { mutable, inner } = actual_ty
        && inner.as_ref() == original_ty
    {
        // The default capture's physical borrow has its own original
        // occurrence. It does not replace the whole sealed child type.
        let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
            source: SemanticOriginalTypeSource::NameAt { name, .. },
            ..
        }) = &original
            .environment
            .get(ordinal as usize)
            .ok_or_else(|| bad("capture head has no original environment slot"))?
            .source
        else {
            return Err(bad(
                "capture head has no exact original lexical name source",
            ));
        };
        let first = first_use::find(original, name, limits, left)?
            .ok_or_else(|| bad("capture head has no actual original first free use"))?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Err(bad(
                "capture head requires its complete original lexical body",
            ));
        };
        let mut implicit = None;
        for occurrence in &original.implicit_lifetimes {
            pay(left, recipes.len())?;
            for recipe in recipes
                .iter()
                .filter(|recipe| recipe.occurrence_path == occurrence.occurrence_path)
            {
                let SemanticSiteRecipeKind::TypeUse { expected: observed } = &recipe.kind else {
                    continue;
                };
                pay(left, site_nodes.len())?;
                let mut nodes = site_nodes.iter().filter(|node| node.site == recipe.id);
                let Some(node) = nodes.next() else { continue };
                if nodes.next().is_some() || node.node != SemanticOriginalNode::Expression(first) {
                    continue;
                }
                let observed = super::super::materialize::symbolic_at(
                    program,
                    module,
                    ledger,
                    instance.id,
                    observed,
                    limits,
                    left,
                )?;
                if observed == expected && implicit.replace(occurrence.ordinal).is_some() {
                    return Err(bad("one capture has ambiguous original borrow occurrences"));
                }
            }
        }
        let ordinal = implicit
            .ok_or_else(|| bad("borrowed capture has no exact original borrow occurrence"))?;
        let recipe = SemanticTypeRecipe::Address {
            mutable: *mutable,
            lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }),
            inner: Box::new(expected),
        };
        super::super::materialize::type_at(
            program,
            module,
            ledger,
            instance.id,
            &recipe,
            Some(actual_ty),
            limits,
            left,
        )?
    } else {
        // Managed capture modes have independent actual outer roles;
        // their owner is still sealed by the environment, and carries
        // no borrowed header rows through this whole-value path.
        return Ok(None);
    };
    Ok(Some(value))
}

/// Relate each original environment slot to the actual lexical selection made
/// by this exact creator. A source slot and a physical capture need not have
/// the same ordinal, and equal concrete types never establish this relation.
fn creator_captures(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &super::super::emission::Ledger,
    instance: &SemanticInstance,
    closure: ClosureId,
    limits: &Limits,
    left: &mut usize,
) -> R<Vec<(u32, u32, Type)>> {
    let parent = instance
        .parent
        .ok_or_else(|| bad("captured environment has no original creator"))?;
    pay(
        left,
        program
            .instances
            .len()
            .saturating_add(module.functions.len())
            .saturating_add(program.sites.len()),
    )?;
    let parent_instance = program
        .instances
        .iter()
        .find(|row| row.id == parent)
        .ok_or_else(|| bad("captured environment lost its original creator instance"))?;
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
        parent_instance.target
    else {
        return Err(bad(
            "captured environment creator is not its actual Typed frame",
        ));
    };
    let creator = module
        .functions
        .iter()
        .find(|row| row.id == function)
        .ok_or_else(|| bad("captured environment creator frame is absent"))?;
    let mut mint = None;
    for site in program.sites.iter().filter(|site| site.instance == parent) {
        if matches!(site.action, SemanticSiteAction::Closure { closure: SemanticClosureTarget::Typed(actual), .. } if actual == closure)
            && mint.replace(site).is_some()
        {
            return Err(bad(
                "captured environment has multiple original creator mints",
            ));
        }
    }
    let site =
        mint.ok_or_else(|| bad("captured environment has no exact original creator mint"))?;
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge: SemanticSiteEdge::After,
    } = site.at
    else {
        return Err(bad(
            "captured environment mint has no actual creation instruction",
        ));
    };
    pay(left, creator.blocks.len())?;
    let op = creator
        .blocks
        .iter()
        .find(|row| row.id == block)
        .and_then(|row| row.instructions.get(instruction as usize))
        .ok_or_else(|| bad("captured environment mint leaves its actual creator CFG"))?;
    let InstructionKind::MakeClosure {
        closure: actual,
        captures,
    } = &op.kind
    else {
        return Err(bad(
            "captured environment mint is not an actual MakeClosure",
        ));
    };
    if *actual != closure {
        return Err(bad("captured environment mint changes its closure"));
    }
    pay(
        left,
        ledger.closure_captures.len().saturating_add(captures.len()),
    )?;
    let receipts = ledger
        .closure_captures
        .get(&closure)
        .ok_or_else(|| bad("actual MakeClosure lacks its lexical capture selections"))?;
    if receipts.len() != captures.len()
        || receipts
            .iter()
            .zip(captures)
            .any(|((_, value), actual)| value != actual)
    {
        return Err(bad(
            "actual MakeClosure differs from its capture selection receipts",
        ));
    }
    let environment = instance
        .environment
        .as_ref()
        .ok_or_else(|| bad("child lost its original environment"))?;
    pay(left, program.definitions.len())?;
    let original = program
        .definitions
        .iter()
        .find(|row| row.id == instance.definition)
        .ok_or_else(|| bad("child lost its original definition"))?;
    if original.environment.len() != environment.actuals.len() {
        return Err(bad(
            "child environment has a different complete original arity",
        ));
    }
    let mut result = Vec::new();
    for (capture, (name, value)) in receipts.iter().enumerate() {
        pay(
            left,
            original
                .environment
                .len()
                .saturating_mul(name.len().saturating_add(1)),
        )?;
        let mut matched = None;
        for (ordinal, row) in original.environment.iter().enumerate() {
            if matches!(&row.source, SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::NameAt { name: actual, .. },
            }) if actual == name)
                && matched.replace(ordinal as u32).is_some()
            {
                return Err(bad(
                    "one actual lexical capture selects multiple original environment slots",
                ));
            }
        }
        let ordinal = matched
            .ok_or_else(|| bad("actual lexical capture has no exact original environment slot"))?;
        let source = &original.environment[ordinal as usize].source;
        crate::dyn_metadata::semantic_operand_snapshot(source, limits, left, 1)?;
        if !matches!(&environment.actuals[ordinal as usize], SemanticActual::CoreRecipe { scope, recipe }
            if *scope == parent && recipe.as_ref() == source)
        {
            return Err(bad(
                "captured environment actual changes its exact original creator source",
            ));
        }
        let ty = creator
            .values
            .get(value.0 as usize)
            .ok_or_else(|| bad("actual capture selection has no emitted value type"))?;
        pay(left, module.closures.len())?;
        let stored = module
            .closures
            .iter()
            .find(|row| row.id == closure)
            .and_then(|row| row.captures.get(capture))
            .ok_or_else(|| bad("actual MakeClosure has no corresponding stored capture"))?;
        inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
        inference::charge_type_snapshot(stored, Span::default(), limits, left, 1)?;
        if ty != stored {
            return Err(bad(
                "actual MakeClosure capture value changes its stored environment type",
            ));
        }
        result.push((capture as u32, ordinal, ty.clone()));
    }
    Ok(result)
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
        SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } => original
            .implicit_lifetimes
            .iter()
            .any(|row| row.ordinal == *ordinal),
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

/// Complete an absent closure directory from its original signature and every
/// actual creator capture. None never supplies a zero-domain premise.
pub(in crate::semantics) fn complete_emitted_headers(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &super::super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut changes = Vec::new();
    for instance in &program.instances {
        pay(left, 1)?;
        let Some(environment) = &instance.environment else {
            continue;
        };
        let SemanticClosureTarget::Typed(id) = environment.closure else {
            return Err(bad("closure directory completion retains a Core target"));
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
            .find(|row| row.id == id)
            .ok_or_else(|| bad("closure directory completion has no actual definition"))?;
        if definition.storage_life.is_some() {
            continue;
        }
        let entry = module
            .functions
            .iter()
            .find(|row| row.id == definition.function)
            .ok_or_else(|| bad("closure directory completion has no actual entry"))?;
        if entry.storage_life.is_some() || entry.environment != Some(id) {
            return Err(bad(
                "closure directory completion changes its actual entry identity",
            ));
        }
        let original = program
            .definitions
            .iter()
            .find(|row| row.id == instance.definition)
            .ok_or_else(|| bad("closure directory completion lost its original definition"))?;
        let (SemanticDefinitionBody::FunctionWithBody {
            signature,
            sites: recipes,
            ..
        }
        | SemanticDefinitionBody::Function {
            signature,
            sites: recipes,
        }) = &original.body
        else {
            return Err(bad(
                "closure directory completion has no original signature",
            ));
        };
        if signature.parameters.len() != entry.parameters.len() {
            return Err(bad(
                "closure directory completion changes original parameter arity",
            ));
        }
        let captures = creator_captures(program, module, ledger, instance, id, limits, left)?;
        let mut capture_bindings = Vec::new();
        let mut capture_classes = Vec::new();
        let mut capture_statics = Vec::new();
        let mut complete = true;
        for (capture, ordinal, actual_ty) in captures {
            let Some(value) = capture_value(
                program, module, ledger, instance, original, recipes, ordinal, &actual_ty, limits,
                left,
            )?
            else {
                complete = false;
                break;
            };
            super::super::creator_headers::rows::append(
                &value,
                StorageBase::Capture(capture),
                &mut capture_bindings,
                &mut capture_classes,
                &mut capture_statics,
                limits,
                left,
            )?;
        }
        if !complete {
            continue;
        }
        let mut bindings = Vec::new();
        let mut classes = Vec::new();
        let mut statics = Vec::new();
        for (ordinal, local) in entry.parameters.iter().enumerate() {
            pay(left, 1)?;
            let actual = &entry
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("closure directory completion lost an actual parameter"))?
                .ty;
            let recipe = SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Parameter(ordinal as u32),
            };
            let value = super::super::materialize::type_at(
                program,
                module,
                ledger,
                instance.id,
                &recipe,
                Some(actual),
                limits,
                left,
            )?;
            if !matches!(&value, SemanticTypeValue::Resolved { ty, .. } if ty == actual) {
                return Err(bad(
                    "closure directory completion changes an original parameter type",
                ));
            }
            super::super::creator_headers::rows::append(
                &value,
                StorageBase::Parameter(ordinal as u32),
                &mut bindings,
                &mut classes,
                &mut statics,
                limits,
                left,
            )?;
        }
        bindings.extend(capture_bindings);
        classes.extend(capture_classes);
        statics.extend(capture_statics);
        let recipe = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        };
        let value = super::super::materialize::type_at(
            program,
            module,
            ledger,
            instance.id,
            &recipe,
            Some(&entry.return_type),
            limits,
            left,
        )?;
        if !matches!(&value, SemanticTypeValue::Resolved { ty, .. } if ty == &entry.return_type) {
            return Err(bad(
                "closure directory completion changes its original result type",
            ));
        }
        super::super::creator_headers::rows::append(
            &value,
            StorageBase::Result,
            &mut bindings,
            &mut classes,
            &mut statics,
            limits,
            left,
        )?;
        let mut directory = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            classes: Vec::new(),
            static_bindings: statics,
        };
        let mut mapping = Vec::<SemanticHeaderBinding>::new();
        // Persistent capture images are assigned first. Invocation inputs may
        // share those same exact original images, never equal concrete Types.
        for persistent in [true, false] {
            for (base, image) in bindings
                .iter()
                .map(|(position, image)| (position.base, image))
                .chain(
                    classes
                        .iter()
                        .map(|(position, image)| (position.base, image)),
                )
            {
                if matches!(base, StorageBase::Capture(_)) != persistent {
                    continue;
                }
                header_image(
                    &mut directory,
                    &mut mapping,
                    image,
                    persistent,
                    limits,
                    left,
                )?;
            }
        }
        for (position, image) in bindings {
            let binder = header_image(
                &mut directory,
                &mut mapping,
                &image,
                matches!(position.base, StorageBase::Capture(_)),
                limits,
                left,
            )?;
            directory
                .bindings
                .push(StorageLifeBinding { position, binder });
        }
        for (position, image) in classes {
            let binder = header_image(
                &mut directory,
                &mut mapping,
                &image,
                matches!(position.base, StorageBase::Capture(_)),
                limits,
                left,
            )?;
            directory
                .classes
                .push(SignatureLifeClassBinding { position, binder });
        }
        if !instance.header_embedding.is_empty() && instance.header_embedding != mapping {
            return Err(bad(
                "completed closure directory changes its original domain embedding",
            ));
        }
        changes.push((instance.id, id, entry.id, directory, mapping));
    }
    for (instance, closure, entry, header, embedding) in changes {
        crate::borrowed_fp::charge_contract(&header, |work| pay(left, work), 2)?;
        pay(
            left,
            module
                .closures
                .len()
                .saturating_add(module.functions.len())
                .saturating_add(program.instances.len()),
        )?;
        module
            .closures
            .iter_mut()
            .find(|row| row.id == closure)
            .ok_or_else(|| bad("completed closure definition disappeared"))?
            .storage_life = Some(header.clone());
        module
            .functions
            .iter_mut()
            .find(|row| row.id == entry)
            .ok_or_else(|| bad("completed closure entry disappeared"))?
            .storage_life = Some(header);
        program
            .instances
            .iter_mut()
            .find(|row| row.id == instance)
            .ok_or_else(|| bad("completed closure instance disappeared"))?
            .header_embedding = embedding;
    }
    Ok(())
}

fn header_image(
    header: &mut StorageLifeContract,
    mapping: &mut Vec<SemanticHeaderBinding>,
    image: &SemanticClassExpr,
    persistent: bool,
    limits: &Limits,
    left: &mut usize,
) -> R<StorageBinder> {
    pay(left, mapping.len().saturating_add(1))?;
    crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
    for row in mapping.iter() {
        crate::dyn_metadata::semantic_operand_snapshot(&row.actual, limits, left, 1)?;
        if &row.actual == image {
            return Ok(row.header);
        }
    }
    let count = if persistent {
        &mut header.fixed_binder_count
    } else {
        &mut header.invocation_binder_count
    };
    let binder = if persistent {
        StorageBinder::Fixed(*count)
    } else {
        StorageBinder::Invocation(*count)
    };
    *count = count
        .checked_add(1)
        .ok_or_else(|| bad("completed closure domain overflows"))?;
    mapping.push(SemanticHeaderBinding {
        header: binder,
        actual: image.clone(),
    });
    Ok(binder)
}
