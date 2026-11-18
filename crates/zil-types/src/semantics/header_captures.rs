//! Complete Fixed images for whole captured values. The original sealed
//! environment and exact creator capture selections bind every header row.
use super::*;
#[path = "header_capture_uses.rs"]
mod first_use;
use super::super::materialize::Formation;

/// Formation is a query over exact actual closure identities. A pending
/// dependency is never a complete directory with an empty class domain.
pub(in crate::semantics) enum HeaderFormation {
    Complete {
        header: StorageLifeContract,
        embedding: Vec<SemanticHeaderBinding>,
        values: Vec<(StorageBase, SemanticTypeValue)>,
    },
    NotReady { dependencies: BTreeSet<ClosureId> },
}


pub(in crate::semantics) fn populate(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &super::super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
    complete: &mut BTreeSet<SemanticInstanceId>,
) -> R<()> {
    let view = super::super::materialize::ActualDirectory::from_module(module);
    loop {
        let mut changes = Vec::new();
        let mut blocked = false;
        pay(left, program.instances.len())?;
        for instance in &program.instances {
            if complete.contains(&instance.id) {
                continue;
            }
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
            let mut domains = BTreeSet::<(u32, u32)>::new();
            let captures = creator_captures(program, &view, ledger, instance, id, limits, left)?;
            let mut ready = true;
            for (_, _, ty) in &captures {
                ready &= capture_dependencies_ready(program, module, ty, limits, left, 0)?;
            }
            if !ready {
                blocked = true;
                continue;
            }
            for (capture, ordinal, actual_ty) in captures {
                let value = match capture_value(
                    program, &view, ledger, instance, original, recipes, ordinal, &actual_ty,
                    limits, left,
                )? {
                    Formation::Complete(value) => value,
                    Formation::NotReady { .. } => return Err(bad("capture domain reads an unsealed actual child header")),
                };
                capture_coordinates(&value, &mut domains, left)?;
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
                        program, ledger, &domains,
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
                        program, ledger, &domains,
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
                        let actual = embedding.get_mut(fixed as usize).ok_or_else(|| bad("whole capture embedding leaves its complete Fixed domain"))?;
                        if actual != row {
                            if actual.header != row.header || !equal_capture_image(program, ledger, original.id, &domains, &actual.actual, &row.actual, limits, left)? {
                                return Err(bad("whole capture embedding differs from its original complete images"));
                            }
                            actual.actual = row.actual.clone();
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
        let progress = !changes.is_empty();
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
        if !blocked {
            return Ok(());
        }
        if !progress {
            return Err(Diagnostic::unsupported(
                Phase::Types,
                "captured closure graph has an unresolved complete Fixed dependency",
                None,
            ));
        }
    }
}

/// A captured child must have its entire actual Fixed directory before an
/// enclosing whole value can be interpreted. The worklist publishes only
/// independently formed rows; dependency order supplies no class equation.
fn capture_dependencies_ready(
    program: &SemanticModule,
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<bool> {
    pay(left, 1)?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "capture dependency depth exceeded",
            None,
        ));
    }
    let mut ready = true;
    match ty {
        Type::Closure(id) => {
            pay(
                left,
                module
                    .closures
                    .len()
                    .saturating_add(program.instances.len()),
            )?;
            let child = module
                .closures
                .iter()
                .find(|row| row.id == *id)
                .ok_or_else(|| bad("capture dependency has no actual closure"))?;
            let header = child
                .storage_life
                .as_ref()
                .ok_or_else(|| bad("capture dependency has no complete actual domain"))?;
            let mut instances = program.instances.iter().filter(|row| {
                row.target
                    == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(
                        child.function,
                    ))
                    && row
                        .environment
                        .as_ref()
                        .is_some_and(|env| env.closure == SemanticClosureTarget::Typed(*id))
            });
            let instance = instances
                .next()
                .ok_or_else(|| bad("capture dependency has no exact actual semantic instance"))?;
            if instances.next().is_some() {
                return Err(bad(
                    "capture dependency has ambiguous actual semantic instances",
                ));
            }
            pay(
                left,
                (header.fixed_binder_count as usize)
                    .saturating_mul(instance.header_embedding.len().saturating_add(1)),
            )?;
            ready = (0..header.fixed_binder_count).all(|fixed| {
                instance
                    .header_embedding
                    .iter()
                    .filter(|row| row.header == StorageBinder::Fixed(fixed))
                    .count()
                    == 1
            });
        }
        Type::Address { inner, .. }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::OwnedStore { element: inner }
        | Type::Mutex { payload: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
        | Type::Slice { element: inner }
        | Type::Array { element: inner, .. }
        | Type::Opaque { hidden: inner, .. }
        | Type::BorrowedStorage { storage: inner, .. } => {
            ready &= capture_dependencies_ready(program, module, inner, limits, left, depth + 1)?;
        }
        Type::Task {
            result,
            retained_life,
        }
        | Type::MutexGuard {
            payload: result,
            retained_life,
        } => {
            ready &= capture_dependencies_ready(program, module, result, limits, left, depth + 1)?;
            for port in &retained_life.ports {
                ready &= capture_dependencies_ready(
                    program,
                    module,
                    &port.leaf,
                    limits,
                    left,
                    depth + 1,
                )?;
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                ready &=
                    capture_dependencies_ready(program, module, field, limits, left, depth + 1)?;
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                ready &=
                    capture_dependencies_ready(program, module, field, limits, left, depth + 1)?;
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    ready &= capture_dependencies_ready(
                        program,
                        module,
                        field,
                        limits,
                        left,
                        depth + 1,
                    )?;
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            ready &= capture_dependencies_ready(program, module, error, limits, left, depth + 1)?;
            ready &= capture_dependencies_ready(program, module, result, limits, left, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                ready &= capture_dependencies_ready(
                    program,
                    module,
                    parameter,
                    limits,
                    left,
                    depth + 1,
                )?;
            }
            ready &= capture_dependencies_ready(program, module, result, limits, left, depth + 1)?;
        }
        _ => {}
    }
    Ok(ready)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn operand_type(
    program: &SemanticModule, module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger, instance: &SemanticInstance,
    base: &str, recipe: &SemanticTypeRecipe, hint: Option<&Type>,
    limits: &Limits, left: &mut usize,
) -> R<Formation<SemanticTypeValue>> {
    match super::super::materialize::form_type_in_view(program, module, ledger,
        instance.id, recipe, hint, limits, left)
    {
        Ok(value) => Ok(value),
        Err(mut error) => {
            crate::dyn_metadata::semantic_operand_snapshot(instance, limits, left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
            if let Some(hint) = hint {
                inference::charge_type_snapshot(hint, Span::default(), limits, left, 1)?;
            }
            let detail = format!("original closure header operand {base:?}: instance {instance:?}; original recipe {recipe:?}; complete physical hint {hint:?}");
            pay(left, detail.len())?;
            error.notes.push(detail);
            Err(error)
        }
    }
}

fn capture_value(
    program: &SemanticModule,
    module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    recipes: &[SemanticSiteRecipe],
    ordinal: u32,
    actual_ty: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<Formation<SemanticTypeValue>> {
    let expected = SemanticTypeRecipe::Environment {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    };
    let value = match operand_type(program, module, ledger, instance,
        "capture environment", &expected, None, limits, left)? {
        Formation::Complete(value) => value,
        Formation::NotReady { dependencies } => return Ok(Formation::NotReady { dependencies }),
    };
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
                let observed = super::super::materialize::symbolic_in_view(
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
        return super::super::materialize::form_type_in_view(
            program,
            module,
            ledger,
            instance.id,
            &recipe,
            Some(actual_ty),
            limits,
            left,
        );
    } else if let Type::Managed(inner) = actual_ty
        && inner.as_ref() == original_ty
    {
        // The exact creator selected an owning carrier of this logical
        // environment value. Transport its entire child domain; owning the
        // carrier itself contributes no borrowed outer occurrence.
        let recipe = SemanticTypeRecipe::Managed(Box::new(expected));
        return super::super::materialize::form_type_in_view(program, module, ledger,
            instance.id, &recipe, Some(actual_ty), limits, left);
    } else {
        return Err(Diagnostic::unsupported(Phase::Types,
            "capture representation has no complete original environment transport", None));
    };
    Ok(Formation::Complete(value))
}

/// Select the immutable environment coordinate of one real Typed capture.
/// The exact creator/MakeClosure/capture ledger proves this correspondence;
/// neither a source name alone nor a same-Type field supplies an ordinal.
pub(in crate::semantics) fn original_capture_ordinal(
    program: &SemanticModule, module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger, instance: &SemanticInstance,
    capture: u32, limits: &Limits, left: &mut usize,
) -> R<u32> {
    let environment = instance.environment.as_ref().ok_or_else(|| bad("capture selection has no original environment"))?;
    let SemanticClosureTarget::Typed(id) = environment.closure else {
        return Err(bad("capture selection retains an unbound creator"));
    };
    let captures = creator_captures(program, module, ledger, instance, id, limits, left)?;
    pay(left, captures.len())?;
    let mut selected = captures.iter().filter(|(index, _, _)| *index == capture);
    let (_, ordinal, _) = selected.next().ok_or_else(|| bad("capture selection has no exact creator input"))?;
    if selected.next().is_some() { return Err(bad("capture selection repeats its actual creator input")); }
    Ok(*ordinal)
}

/// Read the already checked complete source value for one actual capture.
/// The caller cannot supply an environment ordinal or select a same-Type mint.
pub(in crate::semantics) fn original_capture_value(
    program: &SemanticModule,
    module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger,
    instance: &SemanticInstance,
    capture: u32,
    actual_ty: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeValue> {
    let environment = instance.environment.as_ref().ok_or_else(|| bad("capture value has no original environment"))?;
    let SemanticClosureTarget::Typed(id) = environment.closure else {
        return Err(bad("capture value retains an unbound Core creator"));
    };
    let captures = creator_captures(program, module, ledger, instance, id, limits, left)?;
    pay(left, captures.len().saturating_add(program.definitions.len()))?;
    let mut selected = captures.iter().filter(|(index, _, _)| *index == capture);
    let (_, ordinal, ty) = selected.next().ok_or_else(|| bad("capture value has no exact actual creator selection"))?;
    if selected.next().is_some() { return Err(bad("capture value repeats its actual creator selection")); }
    inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
    inference::charge_type_snapshot(actual_ty, Span::default(), limits, left, 1)?;
    if ty != actual_ty { return Err(bad("capture value changes its actual complete capture type")); }
    let original = program.definitions.iter().find(|row| row.id == instance.definition)
        .ok_or_else(|| bad("capture value lost its original definition"))?;
    let (SemanticDefinitionBody::Function { sites, .. } | SemanticDefinitionBody::FunctionWithBody { sites, .. }) = &original.body else {
        return Err(bad("capture value has no original code body"));
    };
    match capture_value(program, module, ledger, instance, original, sites, *ordinal, actual_ty, limits, left)? {
        Formation::Complete(value) => Ok(value),
        Formation::NotReady { .. } => Err(bad("capture query reached an unsealed actual child header")),
    }
}

/// Relate each original environment slot to the actual lexical selection made
/// by this exact creator. A source slot and a physical capture need not have
/// the same ordinal, and equal concrete types never establish this relation.
fn creator_captures(
    program: &SemanticModule,
    module: &super::super::materialize::ActualDirectory<'_>,
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
            .saturating_add(module.function_count())
            .saturating_add(program.sites.len()),
    )?;
    let parent_instance = module
        .original_instance(program, parent)
        .ok_or_else(|| bad("captured environment lost its original creator instance"))?;
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
        parent_instance.target
    else {
        return Err(bad(
            "captured environment creator is not its actual Typed frame",
        ));
    };
    let creator = module
        .function(function)
        .ok_or_else(|| bad("captured environment creator frame is absent"))?;
    pay(
        left,
        module.closures.len().saturating_add(ledger.frames.len()),
    )?;
    let entry = module
        .closures
        .iter()
        .find(|row| row.id == closure)
        .ok_or_else(|| bad("captured environment has no actual closure directory"))?
        .function;
    let frame = ledger
        .frames
        .get(&entry)
        .ok_or_else(|| bad("captured environment has no actual emitter origin"))?;
    if frame.parent != Some((function, closure)) {
        return Err(bad("captured environment changes its actual creator frame"));
    }
    let mut mint = None;
    for site in program.sites.iter().filter(|site| site.instance == parent) {
        let matches = match &site.action {
            SemanticSiteAction::Closure {
                closure: SemanticClosureTarget::Typed(actual),
                ..
            } => *actual == closure,
            SemanticSiteAction::Closure {
                closure:
                    SemanticClosureTarget::Core {
                        module,
                        creator,
                        expression,
                        occurrence: 0,
                    },
                ..
            } => {
                frame.origin.module == *module
                    && frame.origin.creator == *creator
                    && frame.origin.closure == Some(*expression)
            }
            _ => false,
        };
        if matches && mint.replace(site).is_some() {
            return Err(bad(
                "captured environment has multiple original creator mints",
            ));
        }
    }
    let site =
        mint.ok_or_else(|| bad("captured environment has no exact original creator mint"))?;
    let (block, instruction) = match site.at {
        SemanticSiteAddress::Typed {
            block,
            instruction,
            edge: SemanticSiteEdge::After,
        } => (block, instruction as usize),
        SemanticSiteAddress::Core {
            expression,
            occurrence: 0,
            role: SemanticEmissionRole::Closure,
        } => {
            if frame.origin.closure != Some(expression) {
                return Err(bad(
                    "captured environment source changes its exact closure expression",
                ));
            }
            let receipt = ledger
                .frames
                .get(&function)
                .and_then(|frame| frame.expressions.get(&expression))
                .ok_or_else(|| {
                    bad("captured environment source has not reached its actual mint")
                })?;
            pay(left, receipt.instructions.len())?;
            let mut selected = None;
            for point in &receipt.instructions {
                let op = creator
                    .blocks
                    .get(point.block.0 as usize)
                    .and_then(|block| block.instructions.get(point.instruction));
                if op.is_some_and(|op| matches!(op.kind, InstructionKind::MakeClosure { closure: actual, .. } if actual == closure))
                    && selected.replace((point.block, point.instruction)).is_some()
                { return Err(bad("captured environment source has multiple actual closure mints")); }
            }
            selected
                .ok_or_else(|| bad("captured environment source has no exact actual MakeClosure"))?
        }
        _ => {
            return Err(bad(
                "captured environment mint has no actual creation instruction",
            ));
        }
    };
    pay(left, creator.blocks.len())?;
    let op = creator
        .blocks
        .iter()
        .find(|row| row.id == block)
        .and_then(|row| row.instructions.get(instruction))
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
        if !creator_source_matches(
            program, module, ledger, parent,
            &environment.actuals[ordinal as usize], source, limits, left,
        )? {
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

/// The immutable original source and the exact creator establish identity.
/// A ghost actual may still be its Core recipe, or may have been completely
/// materialized by finish. Both stages verify that same source; equality here
/// does not establish a runtime capture, owner, or cleanup fact.
#[allow(clippy::too_many_arguments)]
fn creator_source_matches(
    program: &SemanticModule,
    module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger,
    parent: SemanticInstanceId,
    actual: &SemanticActual,
    source: &SemanticActualRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
    match actual {
        SemanticActual::CoreRecipe { scope, recipe } => {
            Ok(*scope == parent && recipe.as_ref() == source)
        }
        SemanticActual::Type(SemanticTypeValue::Resolved { .. }) => {
            // Use the real creator's original Gamma, not the child's asserted
            // environment and not a hint selected by physical Type equality.
            let expected = super::super::materialize::actual_in_view(
                program, module, ledger, parent, source, limits, left,
            )?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, limits, left, 1)?;
            Ok(actual == &expected)
        }
        // An unmaterialized inference, unresolved Type, or a different actual
        // kind cannot stand in for this original captured Type source.
        _ => Ok(false),
    }
}

#[allow(clippy::too_many_arguments)]
fn put(
    program: &SemanticModule,
    ledger: &super::super::emission::Ledger,
    domains: &BTreeSet<(u32, u32)>,
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
        if prior != &image && !equal_capture_image(program, ledger, original.id, domains, prior, &image, limits, left)? {
            return Err(bad(
                "one actual Fixed class has different original capture images",
            ));
        }
    } else {
        images.insert(fixed, image);
    }
    Ok(())
}

fn capture_coordinates(
    value: &SemanticTypeValue,
    coordinates: &mut BTreeSet<(u32, u32)>,
    left: &mut usize,
) -> R<()> {
    let SemanticTypeValue::Resolved { life, classes, .. } = value else { return Err(bad("capture has no complete original domain")); };
    if life.binder_count as usize != classes.len() { return Err(bad("capture omits its complete original class vector")); }
    for image in classes {
        pay(left, 1)?;
        if let SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Environment(slot), path, class } = image
            && path.is_empty() {
            pay(left, coordinates.len().saturating_add(1).ilog2() as usize + 1)?;
            coordinates.insert((*slot, *class));
        }
    }
    Ok(())
}

/// The original defining equation can relate two sealed Type environments.
/// A coincident actual image, machine Type or loan source is never evidence.
#[allow(clippy::too_many_arguments)]
fn equal_capture_image(
    program: &SemanticModule,
    ledger: &super::super::emission::Ledger,
    owner: SemanticDefinitionId,
    domains: &BTreeSet<(u32, u32)>,
    a: &SemanticClassExpr,
    b: &SemanticClassExpr,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    let (
        SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Environment(a), path: ap, class: ac },
        SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Environment(b), path: bp, class: bc },
    ) = (a, b) else { return Ok(false); };
    pay(left, ap.len().saturating_add(bp.len()).saturating_add(1))?;
    if !ap.is_empty() || !bp.is_empty() || ac != bc { return Ok(false); }
    pay(left, (domains.len().saturating_add(1).ilog2() as usize + 1).saturating_mul(2))?;
    if !domains.contains(&(*a, *ac)) || !domains.contains(&(*b, *bc)) { return Ok(false); }
    let proof = ledger.original.borrow();
    if let Some(proof) = proof.as_ref() {
        let original = ledger.original.context().map_or(program, |context| &context.semantic);
        return proof.equal_capture_types(original, owner, *a, *b, limits, left);
    }
    if let Some(frozen) = ledger.frozen.as_deref() {
        return frozen.original.equal_capture_types(&frozen.semantic, owner, *a, *b, limits, left);
    }
    Ok(false)
}

/// Complete an absent closure directory from its original signature and every
/// actual creator capture. None never supplies a zero-domain premise.
pub(in crate::semantics) fn complete_emitted_headers(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &mut super::super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    pay(left, 1)?;
    let formation_stats = ledger.ordinary_stats.clone();
    let mut complete = BTreeSet::new();
    loop {
    // Deferred nominal applications and closure headers share this existing
    // finite formation loop. A Type actual may need a child header, while a
    // different header may need an already formed nominal application. Publish
    // only complete rows between rounds; neither dependency supplies an N0 fact.
    let before_nominals = module.life_instances.len();
    let (nominal_progress, nominal_state) = stored::OrdinaryStats::step(&formation_stats,
        "finish: deferred nominals", left, |left| {
            super::super::materialize::populate_deferred_nominals(program, module, ledger, limits, left)
        })?;
    let nominal_dependencies = match nominal_state {
        Formation::Complete(()) => BTreeSet::new(),
        Formation::NotReady { dependencies } => dependencies,
    };
    // A representation change invalidates only the affected complete headers
    // and embeddings in the canonicalizer. Do not let this scheduling set
    // preserve a directory which the real actual catalogue has retired.
    pay(left, complete.len().saturating_mul(program.instances.len().saturating_add(module.closures.len()).saturating_add(1)))?;
    let mut retired = Vec::new();
    for id in &complete {
        let instance = program.instances.iter().find(|row| row.id == *id)
            .ok_or_else(|| bad("completed header instance disappeared during representation formation"))?;
        let Some(environment) = &instance.environment else { continue; };
        let SemanticClosureTarget::Typed(closure) = environment.closure else {
            return Err(bad("completed header reverted to an unbound creator"));
        };
        let definition = module.closures.iter().find(|row| row.id == closure)
            .ok_or_else(|| bad("completed header lost its actual closure during canonicalization"))?;
        if definition.storage_life.is_none() { retired.push(*id); }
    }
    for id in retired { complete.remove(&id); }
    let mut pending = Vec::new();
    let mut progress = nominal_progress || module.life_instances.len() != before_nominals;
    pay(left, program.instances.len())?;
    for position in 0..program.instances.len() {
        let instance = &program.instances[position];
        pay(left, complete.len().saturating_add(1).ilog2() as usize + 1)?;
        if complete.contains(&instance.id) { continue; }
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
        let early = ledger.formed_headers.get(&id);
        if definition.storage_life.is_some() && early.is_none() {
            complete.insert(instance.id);
            continue;
        }
        let entry = module
            .functions
            .iter()
            .find(|row| row.id == definition.function)
            .ok_or_else(|| bad("closure directory completion has no actual entry"))?;
        if (entry.storage_life.is_some() && early.is_none()) || entry.environment != Some(id) {
            return Err(bad(
                "closure directory completion changes its actual entry identity",
            ));
        }
        let view = super::super::materialize::ActualDirectory::from_module(module);
        let (directory, mapping, values) = match form_emitted_header(
            program, &view, ledger, instance, id, crate::FunctionRef::from(entry), limits, left,
        )? {
            HeaderFormation::Complete { header, embedding, values } => (header, embedding, values),
            HeaderFormation::NotReady { dependencies } => {
                pay(left, dependencies.len().saturating_add(1))?;
                pending.push((instance.id, id, dependencies));
                continue;
            }
        };
        if let Some((expected, embedding, proposed_values)) = early {
            crate::borrowed_fp::charge_contract(expected, |work| pay(left, work), 2)?;
            crate::borrowed_fp::charge_contract(&directory, |work| pay(left, work), 2)?;
            for row in embedding.iter().chain(mapping.iter()) {
                crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
            }
            pay(left, values.len().saturating_add(proposed_values.len()))?;
            for (_, value) in values.iter().chain(proposed_values.iter()) {
                crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
            }
            if proposed_values != &values
                || expected != &directory
                || embedding != &mapping
                || definition.storage_life.as_ref() != Some(&directory)
                || entry.storage_life.as_ref() != Some(&directory)
            {
                return Err(bad(
                    "early actual Task closure directory differs from its complete finish reconstruction",
                ));
            }
        }
        let (instance, closure, entry, header, embedding) =
            (instance.id, id, entry.id, directory, mapping);
        // Publish this complete candidate immediately inside the sole finish
        // transaction. Later candidates may read it; no Typed program escapes
        // until every actual dependency and original proof has been sealed.
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
        complete.insert(instance);
        progress = true;
    }
    if pending.is_empty() && nominal_dependencies.is_empty() { return Ok(()); }
    if !progress {
        let mut diagnostic = Diagnostic::unsupported(Phase::Types,
            "closure declaration directory has unresolved actual header dependencies", None);
        if !nominal_dependencies.is_empty() {
            let detail = format!("pending complete nominal applications require actual closure headers {nominal_dependencies:?}");
            pay(left, detail.len())?;
            diagnostic.notes.push(detail);
        }
        for (instance, closure, dependencies) in pending {
            let detail = format!("pending original instance {instance:?}, actual closure {closure:?}, requires complete headers {dependencies:?}");
            pay(left, detail.len())?;
            diagnostic.notes.push(detail);
        }
        return Err(diagnostic);
    }
    }
}

/// Complete invocation images for a directory that was emitted before its
/// original P/C/R operands could be interpreted. Actual header positions, not
/// original quantifier ordinals, select these images. Fixed images must already
/// have been independently grounded by the capture passes.
pub(in crate::semantics) fn complete_invocation_embeddings(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &super::super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let view = super::super::materialize::ActualDirectory::from_module(module);
    let mut changes = Vec::new();
    for instance in &program.instances {
        pay(left, 1)?;
        let Some(environment) = &instance.environment else {
            continue;
        };
        let SemanticClosureTarget::Typed(id) = environment.closure else {
            return Err(bad("invocation embedding retains a Core closure target"));
        };
        pay(left, module.closures.len())?;
        let definition = module
            .closures
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| bad("invocation embedding lost its actual closure"))?;
        let Some(header) = &definition.storage_life else {
            continue;
        };
        if header.invocation_binder_count == 0 {
            continue;
        }
        crate::borrowed_fp::charge_contract(header, |work| pay(left, work), 1)?;
        if (header.fixed_binder_count as usize)
            .saturating_add(header.invocation_binder_count as usize)
            > limits.max_nodes
        {
            return Err(Diagnostic::limit(
                Phase::Types,
                "closure header domain node limit exceeded",
                None,
            ));
        }
        let mut existing = BTreeMap::new();
        for row in &instance.header_embedding {
            crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
            let valid = match row.header {
                StorageBinder::Fixed(i) => i < header.fixed_binder_count,
                StorageBinder::Invocation(i) => i < header.invocation_binder_count,
            };
            pay(left, existing.len().saturating_add(1).ilog2() as usize + 1)?;
            if !valid || existing.insert(row.header, &row.actual).is_some() {
                return Err(bad(
                    "invocation embedding duplicates or exceeds its actual binder domain",
                ));
            }
        }
        pay(left, header.fixed_binder_count as usize)?;
        for fixed in 0..header.fixed_binder_count {
            pay(left, existing.len().saturating_add(1).ilog2() as usize + 1)?;
            if !existing.contains_key(&StorageBinder::Fixed(fixed)) {
                return Err(bad(
                    "invocation embedding lacks an independently grounded Fixed image",
                ));
            }
        }
        if existing.len()
            == (header.fixed_binder_count as usize)
                .saturating_add(header.invocation_binder_count as usize)
        {
            // Complete proposals still undergo the independent whole World
            // header proof; this pass supplies only genuinely absent images.
            continue;
        }
        pay(left, module.functions.len())?;
        let entry = module
            .functions
            .iter()
            .find(|row| row.id == definition.function)
            .ok_or_else(|| bad("invocation embedding lost its actual entry"))?;
        if let Some(actual) = &entry.storage_life {
            crate::borrowed_fp::charge_contract(actual, |work| pay(left, work), 1)?;
        }
        if entry.environment != Some(id)
            || entry.storage_life.as_ref() != Some(header)
            || !matches!(instance.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))
                    if function == entry.id)
        {
            return Err(bad(
                "invocation embedding changes the actual closure entry directory",
            ));
        }
        let values = match emitted_operands(
            program,
            &view,
            ledger,
            instance,
            id,
            crate::FunctionRef::from(entry),
            limits,
            left,
        )? {
            Formation::Complete(values) => values,
            Formation::NotReady { dependencies } => {
                // This reader runs after complete_emitted_headers has sealed
                // every actual closure directory inside finish. It cannot
                // consume pending P/C/R operands or invent an empty domain.
                let detail = format!("invocation embedding for original instance {:?}, actual closure {id:?} still requires complete headers {dependencies:?}", instance.id);
                pay(left, detail.len())?;
                let mut diagnostic = bad("invocation embedding reads unsealed original P/C/R operands");
                diagnostic.notes.push(detail);
                return Err(diagnostic);
            }
        };
        let mut bindings = Vec::new();
        let mut classes = Vec::new();
        let mut statics = Vec::new();
        for (base, value) in &values {
            super::super::creator_headers::rows::append(
                value,
                *base,
                &mut bindings,
                &mut classes,
                &mut statics,
                limits,
                left,
            )?;
        }
        let head = receiver_head(program, ledger, instance.definition, limits, left)?;
        receiver_binding(&mut bindings, head, limits, left)?;
        if bindings.len() != header.bindings.len()
            || classes.len() != header.classes.len()
            || statics.len() != header.static_bindings.len()
        {
            return Err(bad(
                "invocation embedding changes complete original header occurrences",
            ));
        }
        let mut images = BTreeMap::new();
        let mut seen_bindings = BTreeSet::new();
        for (position, image) in bindings {
            pay(
                left,
                position
                    .path
                    .len()
                    .saturating_add(1)
                    .saturating_mul(seen_bindings.len().saturating_add(1).ilog2() as usize + 1),
            )?;
            if !seen_bindings.insert(position.clone()) {
                return Err(bad(
                    "original invocation operands repeat a borrow occurrence",
                ));
            }
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
            let row = rows
                .next()
                .ok_or_else(|| bad("invocation embedding omits an original borrow occurrence"))?;
            if rows.next().is_some() {
                return Err(bad(
                    "invocation embedding repeats an actual borrow occurrence",
                ));
            }
            invocation_image(
                &mut images,
                &existing,
                header,
                row.binder,
                image,
                matches!(position.base, StorageBase::Capture(_)),
                limits,
                left,
            )?;
        }
        let mut seen_classes = BTreeSet::new();
        for (position, image) in classes {
            pay(
                left,
                position
                    .path
                    .len()
                    .saturating_add(1)
                    .saturating_mul(seen_classes.len().saturating_add(1).ilog2() as usize + 1),
            )?;
            if !seen_classes.insert(position.clone()) {
                return Err(bad("original invocation operands repeat a whole class"));
            }
            pay(
                left,
                header
                    .classes
                    .len()
                    .saturating_mul(position.path.len().saturating_add(1)),
            )?;
            let mut rows = header.classes.iter().filter(|row| row.position == position);
            let row = rows
                .next()
                .ok_or_else(|| bad("invocation embedding omits an original whole class"))?;
            if rows.next().is_some() {
                return Err(bad("invocation embedding repeats an actual whole class"));
            }
            invocation_image(
                &mut images,
                &existing,
                header,
                row.binder,
                image,
                matches!(position.base, StorageBase::Capture(_)),
                limits,
                left,
            )?;
        }
        let mut seen_statics = BTreeSet::new();
        for position in statics {
            pay(
                left,
                position
                    .path
                    .len()
                    .saturating_add(1)
                    .saturating_mul(seen_statics.len().saturating_add(1).ilog2() as usize + 1),
            )?;
            if !seen_statics.insert(position.clone()) {
                return Err(bad(
                    "original invocation operands repeat a static occurrence",
                ));
            }
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
                    "invocation embedding changes an original static occurrence",
                ));
            }
        }
        if images.len() != header.invocation_binder_count as usize {
            return Err(bad(
                "invocation embedding lacks a complete original invocation image",
            ));
        }
        let mut embedding = Vec::new();
        for row in &instance.header_embedding {
            crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
            embedding.push(row.clone());
        }
        for (index, actual) in images {
            pay(left, existing.len().saturating_add(1).ilog2() as usize + 1)?;
            if !existing.contains_key(&StorageBinder::Invocation(index)) {
                embedding.push(SemanticHeaderBinding {
                    header: StorageBinder::Invocation(index),
                    actual,
                });
            }
        }
        changes.push((instance.id, embedding));
    }
    for (id, embedding) in changes {
        pay(left, program.instances.len())?;
        program
            .instances
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or_else(|| bad("completed invocation instance disappeared"))?
            .header_embedding = embedding;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn invocation_image(
    images: &mut BTreeMap<u32, SemanticClassExpr>,
    existing: &BTreeMap<StorageBinder, &SemanticClassExpr>,
    header: &StorageLifeContract,
    binder: StorageBinder,
    image: SemanticClassExpr,
    persistent: bool,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    // Fixed images are never inferred from invocation inputs. Their original
    // capture proof and the final World row proof remain mandatory.
    let StorageBinder::Invocation(index) = binder else {
        let StorageBinder::Fixed(index) = binder else {
            unreachable!()
        };
        pay(left, existing.len().saturating_add(1).ilog2() as usize + 1)?;
        if index >= header.fixed_binder_count || !existing.contains_key(&binder) {
            return Err(bad("invocation row has no grounded actual Fixed image"));
        }
        return Ok(());
    };
    if persistent || index >= header.invocation_binder_count {
        return Err(bad(
            "persistent capture or out-of-domain row uses an invocation image",
        ));
    }
    crate::dyn_metadata::semantic_operand_snapshot(&image, limits, left, 1)?;
    pay(left, existing.len().saturating_add(1).ilog2() as usize + 1)?;
    if let Some(prior) = existing.get(&binder) {
        crate::dyn_metadata::semantic_operand_snapshot(*prior, limits, left, 1)?;
        if *prior != &image {
            return Err(bad(
                "invocation proposal differs from its complete original operand image",
            ));
        }
    }
    pay(left, images.len().saturating_add(1).ilog2() as usize + 1)?;
    if let Some(prior) = images.get(&index) {
        crate::dyn_metadata::semantic_operand_snapshot(prior, limits, left, 1)?;
        if prior != &image {
            return Err(bad(
                "one actual invocation class has different original operand images",
            ));
        }
    } else {
        images.insert(index, image);
    }
    Ok(())
}

/// Form the same untrusted complete header at a real closure mint or at finish.
#[allow(clippy::too_many_arguments)]
pub(in crate::semantics) fn form_emitted_header(
    program: &SemanticModule,
    module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger,
    instance: &SemanticInstance,
    id: ClosureId,
    entry: crate::FunctionRef<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<HeaderFormation> {
    let values = match emitted_operands(program, module, ledger, instance, id, entry, limits, left)? {
        Formation::Complete(values) => values,
        Formation::NotReady { dependencies } => return Ok(HeaderFormation::NotReady { dependencies }),
    };
    let mut bindings = Vec::new();
    let mut classes = Vec::new();
    let mut statics = Vec::new();
    // Keep the established P/C/R row order and the capture-first value order
    // used by early Task formation; neither order numbers original roles.
    for captures in [false, true] {
        for (base, value) in &values {
            pay(left, 1)?;
            if (*base != StorageBase::Result) && matches!(base, StorageBase::Capture(_)) == captures
            {
                super::super::creator_headers::rows::append(
                    value,
                    *base,
                    &mut bindings,
                    &mut classes,
                    &mut statics,
                    limits,
                    left,
                )?;
            }
        }
    }
    let (_, result) = values
        .last()
        .ok_or_else(|| bad("closure directory lacks its result"))?;
    super::super::creator_headers::rows::append(
        result,
        StorageBase::Result,
        &mut bindings,
        &mut classes,
        &mut statics,
        limits,
        left,
    )?;
    let mut coordinates = BTreeSet::new();
    for (base, value) in &values {
        if matches!(base, StorageBase::Capture(_)) { capture_coordinates(value, &mut coordinates, left)?; }
    }
    let mut equal = |a: &SemanticClassExpr, b: &SemanticClassExpr, left: &mut usize| {
        equal_capture_image(program, ledger, instance.definition, &coordinates, a, b, limits, left)
    };
    let head = receiver_head(program, ledger, instance.definition, limits, left)?;
    let (directory, mut mapping) = complete_rows(bindings, classes, statics, head, &mut equal, limits, left)?;
    if !instance.header_embedding.is_empty() {
        pay(left, 1)?;
        if instance.header_embedding.len() != mapping.len() {
            return Err(bad("completed closure directory changes its original domain embedding"));
        }
        for (actual, existing) in mapping.iter_mut().zip(&instance.header_embedding) {
            crate::dyn_metadata::semantic_operand_snapshot(existing, limits, left, 1)?;
            if actual.header != existing.header
                || actual.actual != existing.actual && !equal(&actual.actual, &existing.actual, left)? {
                return Err(bad("completed closure directory changes its original domain embedding"));
            }
            actual.actual = existing.actual.clone();
        }
    }
    Ok(HeaderFormation::Complete { header: directory, embedding: mapping, values })
}

/// Interpret original operands without deriving or renumbering an actual
/// directory. Existing Fixed axes remain grounded by the capture pass.
#[allow(clippy::too_many_arguments)]
fn emitted_operands(
    program: &SemanticModule,
    module: &super::super::materialize::ActualDirectory<'_>,
    ledger: &super::super::emission::Ledger,
    instance: &SemanticInstance,
    id: ClosureId,
    entry: crate::FunctionRef<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<Formation<Vec<(StorageBase, SemanticTypeValue)>>> {
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
    let mut values = Vec::new();
    for (capture, ordinal, actual_ty) in captures {
        let value = match capture_value(
            program, module, ledger, instance, original, recipes, ordinal, &actual_ty, limits, left,
        )? {
            Formation::Complete(value) => value,
            Formation::NotReady { dependencies } => return Ok(Formation::NotReady { dependencies }),
        };
        values.push((StorageBase::Capture(capture), value));
    }
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
        let value = match operand_type(program, module, ledger, instance,
            "parameter", &recipe, Some(actual), limits, left)? {
            Formation::Complete(value) => value,
            Formation::NotReady { dependencies } => return Ok(Formation::NotReady { dependencies }),
        };
        if !matches!(&value, SemanticTypeValue::Resolved { ty, .. } if ty == actual) {
            return Err(bad(
                "closure directory completion changes an original parameter type",
            ));
        }
        values.push((StorageBase::Parameter(ordinal as u32), value));
    }
    let recipe = SemanticTypeRecipe::OriginalType {
        scope: SemanticScopeRef::Ancestor(0),
        source: SemanticOriginalTypeSource::Result,
    };
    let value = match operand_type(program, module, ledger, instance,
        "result", &recipe, Some(entry.result()?), limits, left)? {
        Formation::Complete(value) => value,
        Formation::NotReady { dependencies } => return Ok(Formation::NotReady { dependencies }),
    };
    if !matches!(&value, SemanticTypeValue::Resolved { ty, .. } if ty == entry.result()?) {
        return Err(bad(
            "closure directory completion changes its original result type",
        ));
    }
    values.push((StorageBase::Result, value));
    Ok(Formation::Complete(values))
}

fn complete_rows<F: FnMut(&SemanticClassExpr, &SemanticClassExpr, &mut usize) -> R<bool>>(
    mut bindings: Vec<(StoragePosition, SemanticClassExpr)>,
    classes: Vec<(SignatureLifeClass, SemanticClassExpr)>,
    statics: Vec<StoragePosition>,
    receiver_head: bool,
    equal: &mut F,
    limits: &Limits,
    left: &mut usize,
) -> R<(StorageLifeContract, Vec<SemanticHeaderBinding>)> {
    receiver_binding(&mut bindings, receiver_head, limits, left)?;
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
                equal,
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
            equal,
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
            equal,
            limits,
            left,
        )?;
        directory
            .classes
            .push(SignatureLifeClassBinding { position, binder });
    }
    Ok((directory, mapping))
}

/// Whether this exact completed original entry proves a receiver result
/// upper, even when its actual result retains a stronger captured Fixed.
pub(in crate::semantics) fn receiver_head(
    program: &SemanticModule, ledger: &super::super::emission::Ledger,
    owner: SemanticDefinitionId, limits: &Limits, left: &mut usize,
) -> R<bool> {
    let proof = ledger.original.borrow();
    let Some(proof) = proof.as_ref() else { return Ok(false); };
    let original = ledger.original.context().map_or(program, |context| &context.semantic);
    proof.receiver_result_upper(original, owner, limits, left)
}

/// Receiver is a synthetic invocation access head, not another captured value.
/// It has no semantic-class rows; capture domains remain on their actual Types.
pub(in crate::semantics) fn receiver_binding(
    bindings: &mut Vec<(StoragePosition, SemanticClassExpr)>,
    proved_head: bool, limits: &Limits, left: &mut usize,
) -> R<bool> {
    pay(left, bindings.len())?;
    let mut receiver = None;
    for (position, image) in bindings.iter() {
        if position.base == StorageBase::Result && position.path.is_empty()
            && matches!(image, SemanticClassExpr::OuterOccurrence {
                operand: SemanticOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
            } if path.is_empty())
        {
            if receiver.replace(image).is_some() {
                return Err(bad("closure receiver has multiple original result occurrences"));
            }
        }
    }
    let image = if let Some(image) = receiver {
        crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
        image.clone()
    } else if proved_head {
        pay(left, 1)?;
        SemanticClassExpr::OuterOccurrence { operand: SemanticOperand::Receiver,
            path: Vec::new(), mode: StoredBorrowMode::SharedAddress }
    } else { return Ok(false); };
    let at = bindings.iter().position(|(position, _)| position.base == StorageBase::Result)
        .unwrap_or(bindings.len());
    pay(left, bindings.len().saturating_add(1))?;
    bindings.insert(at, (StoragePosition { base: StorageBase::Receiver, path: Vec::new() }, image));
    Ok(true)
}

/// Recheck the complete actual operand directory at the early metadata consumer.
/// Final publication derives these operands anew from the original program.
#[allow(clippy::too_many_arguments)]
pub(in crate::semantics) fn validate_formed_header(
    program: &SemanticModule,
    ledger: &super::super::emission::Ledger,
    owner: SemanticDefinitionId,
    parameters: &[Type],
    captures: &[Type],
    result: &Type,
    header: &StorageLifeContract,
    embedding: &[SemanticHeaderBinding],
    values: &[(StorageBase, SemanticTypeValue)],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    pay(
        left,
        parameters
            .len()
            .saturating_add(captures.len())
            .saturating_add(values.len())
            .saturating_add(1),
    )?;
    if values.len()
        != parameters
            .len()
            .saturating_add(captures.len())
            .saturating_add(1)
    {
        return Err(bad("formed closure header omits complete P/C/R operands"));
    }
    let mut seen = BTreeSet::new();
    let mut bindings = Vec::new();
    let mut classes = Vec::new();
    let mut statics = Vec::new();
    let mut ordered = values.iter().collect::<Vec<_>>();
    pay(
        left,
        ordered
            .len()
            .saturating_mul(ordered.len().saturating_add(1).ilog2() as usize + 1),
    )?;
    ordered.sort_by_key(|(base, _)| *base);
    for (base, value) in ordered {
        let actual = match *base {
            StorageBase::Parameter(i) => parameters.get(i as usize),
            StorageBase::Capture(i) => captures.get(i as usize),
            StorageBase::Result => Some(result),
            StorageBase::Receiver => {
                return Err(bad(
                    "closure formation P/C/R cannot invent an invocation Receiver operand",
                ));
            }
        }
        .ok_or_else(|| bad("formed closure operand leaves the actual P/C/R inventory"))?;
        if !seen.insert(*base) {
            return Err(bad("formed closure repeats an actual P/C/R operand"));
        }
        let SemanticTypeValue::Resolved { ty, .. } = value else {
            return Err(bad("formed closure retained an unresolved operand"));
        };
        crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
        inference::charge_type_snapshot(actual, Span::default(), limits, left, 1)?;
        if ty != actual {
            return Err(bad(
                "formed closure operand changes its actual complete type",
            ));
        }
        super::super::creator_headers::rows::append(
            value,
            *base,
            &mut bindings,
            &mut classes,
            &mut statics,
            limits,
            left,
        )?;
    }
    let mut coordinates = BTreeSet::new();
    for (base, value) in values {
        if matches!(base, StorageBase::Capture(_)) { capture_coordinates(value, &mut coordinates, left)?; }
    }
    let mut equal = |a: &SemanticClassExpr, b: &SemanticClassExpr, left: &mut usize| {
        equal_capture_image(program, ledger, owner, &coordinates, a, b, limits, left)
    };
    let head = receiver_head(program, ledger, owner, limits, left)?;
    let (expected, mapping) = complete_rows(bindings, classes, statics, head, &mut equal, limits, left)?;
    crate::borrowed_fp::charge_contract(header, |work| pay(left, work), 1)?;
    for row in embedding {
        crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
    }
    pay(left, 1)?;
    if &expected != header || mapping.len() != embedding.len() {
        return Err(bad("formed closure header changes a complete operand row or original Fixed image"));
    }
    for (actual, existing) in mapping.iter().zip(embedding) {
        if actual.header != existing.header
            || actual.actual != existing.actual && !equal(&actual.actual, &existing.actual, left)? {
            return Err(bad("formed closure header changes a complete operand row or original Fixed image"));
        }
    }
    Ok(())
}

fn header_image<F: FnMut(&SemanticClassExpr, &SemanticClassExpr, &mut usize) -> R<bool>>(
    header: &mut StorageLifeContract,
    mapping: &mut Vec<SemanticHeaderBinding>,
    image: &SemanticClassExpr,
    persistent: bool,
    equal: &mut F,
    limits: &Limits,
    left: &mut usize,
) -> R<StorageBinder> {
    pay(left, mapping.len().saturating_add(1))?;
    crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
    for row in mapping.iter() {
        crate::dyn_metadata::semantic_operand_snapshot(&row.actual, limits, left, 1)?;
        if &row.actual == image || equal(&row.actual, image, left)? {
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
