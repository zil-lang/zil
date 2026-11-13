//! A selected CT executable is not a replacement for its immutable source Γ.
//! Source definitions are proved universally before the selected actual graph.
use super::*;
use zil_base::Limits;

fn unsupported(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::unsupported(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
fn put<T>(out: &mut Vec<T>, value: T, b: &mut Budget) -> CompileResult<()> {
    b.spend(out.len().saturating_add(1), WORK)?;
    if out.len() >= b.helper_nodes {
        return Err(vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,
            "frozen original inventory node limit exceeded",
            None,
        )]);
    }
    out.try_reserve(1).map_err(|_| {
        vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,
            "frozen original inventory allocation failed",
            None,
        )]
    })?;
    out.push(value);
    Ok(())
}

pub(crate) fn pending(
    executable: &TypedModule,
    source: &CoreProgram,
    binding: &CtFrozenContext,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    // Fixed-size stage and query fields are paid before comparison or hashing.
    b.spend(32 * 3 + 64, "frozen query and source-stage identity")?;
    if binding.stage.source_fingerprint != source.source_fingerprint {
        return Err(fail(
            "selected CT graph has another frozen query or source-stage identity",
        ));
    }
    let Some(selected) = &executable.semantic else {
        return Err(fail(
            "selected CT graph omits its complete semantic inventory",
        ));
    };
    crate::dyn_metadata::recipe(selected, b)?;
    let mut full = SemanticModule {
        definitions: Vec::new(),
        instances: Vec::new(),
        sites: Vec::new(),
        dyn_scopes: Vec::new(),
    };
    let mut module_ids = BTreeSet::new();
    for module in &source.modules {
        b.lookup(module_ids.len(), WORK)?;
        if !module_ids.insert(module.core.module_id)
            || module.core.schema_version != zil_base::SCHEMA_VERSION
        {
            return Err(fail(
                "frozen source module identities or schema are invalid",
            ));
        }
        let Some(original) = &module.core.semantic else {
            return Err(fail(
                "frozen source module omits its original semantic program",
            ));
        };
        crate::dyn_metadata::recipe(original, b)?;
        for definition in &original.definitions {
            let mut existing = None;
            for item in &full.definitions {
                b.spend(1, WORK)?;
                if item.id == definition.id {
                    existing = Some(item);
                }
            }
            if let Some(existing) = existing {
                crate::dyn_metadata::recipe(existing, b)?;
                if existing != definition {
                    return Err(fail("frozen modules disagree on an original declaration"));
                }
            } else {
                put(&mut full.definitions, definition.clone(), b)?;
            }
        }
        for instance in &original.instances {
            b.spend(full.instances.len().saturating_add(1), WORK)?;
            if full.instances.iter().any(|item| item.id == instance.id) {
                return Err(fail("frozen source repeats an actual lexical instance"));
            }
            put(&mut full.instances, instance.clone(), b)?;
        }
        for site in &original.sites {
            put(&mut full.sites, site.clone(), b)?;
        }
        for scope in &original.dyn_scopes {
            put(&mut full.dyn_scopes, scope.clone(), b)?;
        }
    }
    b.lookup(module_ids.len(), WORK)?;
    if !module_ids.contains(&source.root) {
        return Err(fail("frozen source root module is absent"));
    }
    let mut source_inventory = Inventory::new(executable, limits, &full, b)?;
    source_inventory.source = Some(source);
    source_parents(source, &full, &source_inventory, b)?;
    // This check sees every original body and all original branches, including
    // definitions with no selected CT instantiation. E contributes no premise.
    let original = original::check(executable, &full, &source_inventory, b)?;
    let owner = query_anchor(source, binding, &full, &source_inventory, b)?;
    let wrapper = selected_bridge(
        executable,
        selected,
        &full,
        owner,
        binding,
        &source_inventory,
        &original,
        b,
    )?;
    // Carry S only as a read-only original/lexical lookup context. The actual
    // selected instance/site lists and all executable functions remain E's.
    crate::dyn_metadata::recipe(selected, b)?;
    let mut joined = selected.clone();
    for definition in &full.definitions {
        b.spend(joined.definitions.len().saturating_add(1), WORK)?;
        if !joined
            .definitions
            .iter()
            .any(|item| item.id == definition.id)
        {
            put(&mut joined.definitions, definition.clone(), b)?;
        }
    }
    let mut inventory = Inventory::new(executable, limits, &joined, b)?;
    inventory.source = Some(source);
    inventory.frozen_wrapper = Some(wrapper);
    for instance in &full.instances {
        b.lookup(inventory.instances.len(), WORK)?;
        if !inventory.instances.contains_key(&instance.id) {
            inventory.instances.insert(instance.id, instance);
        }
    }
    inventory.parents(&joined, b)?;
    inventory.sites(&joined, b)?;
    let checked = original::check(executable, &joined, &inventory, b)?;
    closed::check(executable, &joined, &inventory, &checked, b)
}

fn source_parents(
    source: &CoreProgram,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    for definition in &program.definitions {
        let mut active = BTreeSet::new();
        let mut current = Some(definition.id);
        while let Some(id) = current {
            b.lookup(active.len(), WORK)?;
            if !active.insert(id) {
                return Err(fail("frozen original lexical parent graph is cyclic"));
            }
            current = inventory.definition(id, b)?.parent;
        }
        if matches!(definition.body, SemanticDefinitionBody::Function { .. }) {
            return Err(fail(
                "frozen original function omits its complete universal body",
            ));
        }
    }
    for instance in &program.instances {
        let definition = inventory.definition(instance.definition, b)?;
        match (definition.parent, instance.parent) {
            (None, None) => {}
            (Some(parent), Some(actual)) if inventory.instance(actual, b)?.definition == parent => {
            }
            _ => return Err(fail("frozen instance changes its real lexical parent")),
        }
        match &instance.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) => {
                if !matches!(
                    definition.body,
                    SemanticDefinitionBody::FunctionWithBody { .. }
                ) {
                    return Err(fail(
                        "frozen function target selects a nonfunction original",
                    ));
                }
                core_function(source, *module, *function, b)?;
            }
            SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure { .. }) => {
                if definition.kind != SemanticDefinitionKind::Closure {
                    return Err(fail("frozen closure target changes original kind"));
                }
            }
            SemanticInstanceTarget::Scope
                if matches!(
                    definition.body,
                    SemanticDefinitionBody::Nominal { .. } | SemanticDefinitionBody::Scope(_)
                ) => {}
            _ => {
                return Err(fail(
                    "frozen source retains another stage's executable function target",
                ));
            }
        }
        if instance.actuals.len() != definition.parameters.len() {
            return Err(fail("frozen instance omits full original mixed formals"));
        }
    }
    Ok(())
}

pub(super) fn core_function<'a>(
    source: &'a CoreProgram,
    module: zil_base::ModuleId,
    id: zil_base::NodeId,
    b: &mut Budget,
) -> CompileResult<&'a CoreFunction> {
    let mut found = None;
    for item in &source.modules {
        b.spend(1, WORK)?;
        if item.core.module_id == module {
            for function in &item.core.functions {
                b.spend(1, WORK)?;
                if function.id == id && found.replace(function).is_some() {
                    return Err(fail("frozen function target is ambiguous"));
                }
            }
        }
    }
    found.ok_or_else(|| fail("frozen caller has no exact full Core function"))
}

fn query_anchor<'a>(
    source: &CoreProgram,
    binding: &CtFrozenContext,
    program: &'a SemanticModule,
    inventory: &Inventory<'a>,
    b: &mut Budget,
) -> CompileResult<&'a SemanticInstance> {
    let CtQueryOwner::Instance(owner) = binding.query.owner else {
        return Err(unsupported(
            "frozen module or definition query needs its complete original declaration and lexical context",
        ));
    };
    let instance = inventory.instance(owner, b)?;
    let definition = inventory.definition(instance.definition, b)?;
    let SemanticDefinitionBody::FunctionWithBody {
        sites, site_nodes, ..
    } = &definition.body
    else {
        return Err(fail("frozen query owner omits its original body"));
    };
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) =
        instance.target
    else {
        return Err(unsupported(
            "frozen closure query needs its exact actual Core creator and nested body anchor",
        ));
    };
    core_function(source, module, function, b)?;
    let index = original_index::index(definition, b)?;
    let SemanticOriginalNode::Expression(original) = binding.query.original else {
        return Err(fail("frozen query does not select an original expression"));
    };
    if !matches!(
        index.get_expr(original, b)?.kind,
        CoreExprKind::CompileTime(_)
    ) {
        return Err(fail(
            "frozen query original anchor is not a compile-time expression",
        ));
    }
    let mut matched = None;
    for actual in &program.sites {
        b.spend(sites.len().saturating_add(site_nodes.len() + 1), WORK)?;
        if actual.instance != owner
            || !matches!(actual.at,SemanticSiteAddress::Core {expression,occurrence,role:SemanticEmissionRole::Ascription} if expression==binding.query.expression&&occurrence==binding.query.occurrence)
        {
            continue;
        }
        let recipe = sites
            .iter()
            .find(|recipe| recipe.id == actual.recipe)
            .ok_or_else(|| fail("frozen query actual event has no original recipe"))?;
        if !matches!(recipe.kind, SemanticSiteRecipeKind::TypeUse { .. })
            || !matches!(actual.action, SemanticSiteAction::TypeUse { .. })
            || !site_nodes
                .iter()
                .any(|node| node.site == recipe.id && node.node == binding.query.original)
        {
            return Err(fail(
                "frozen query does not bind its actual expression to the original compile-time node",
            ));
        }
        if matched.replace(actual).is_some() {
            return Err(fail("frozen query has multiple actual source anchors"));
        }
    }
    if matched.is_none() {
        return Err(fail(
            "frozen query has no exact original-to-actual source anchor",
        ));
    }
    Ok(instance)
}

#[allow(clippy::too_many_arguments)]
fn selected_bridge(
    executable: &TypedModule,
    selected: &SemanticModule,
    source: &SemanticModule,
    owner: &SemanticInstance,
    binding: &CtFrozenContext,
    inventory: &Inventory<'_>,
    _original: &original::Checked,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    let SemanticFunctionTarget::Typed(wrapper_id) = &binding.query.wrapper else {
        return Err(fail("CT executable query wrapper retains a Core target"));
    };
    let mut wrapper = None;
    for instance in &selected.instances {
        b.spend(1, WORK)?;
        if matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id==*wrapper_id)
        {
            if wrapper.replace(instance).is_some() {
                return Err(fail(
                    "selected CT query wrapper has multiple original instances",
                ));
            }
        }
    }
    let wrapper =
        wrapper.ok_or_else(|| fail("selected CT graph has no exact query wrapper instance"))?;
    if wrapper.parent != Some(owner.id) {
        return Err(fail(
            "selected CT wrapper is not a child of its actual source caller",
        ));
    }
    let definition = selected
        .definitions
        .iter()
        .find(|definition| definition.id == wrapper.definition)
        .ok_or_else(|| fail("selected CT wrapper omits its real compiler definition"))?;
    if definition.parent != Some(owner.definition)
        || !matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        )
        || inventory.definitions.contains_key(&definition.id)
    {
        return Err(fail(
            "selected CT wrapper changes or replaces an original declaration",
        ));
    }
    let function = executable
        .functions
        .get(wrapper_id.0 as usize)
        .filter(|function| function.id == *wrapper_id)
        .ok_or_else(|| fail("selected CT query wrapper has no real executable function"))?;
    if function.foreign.is_some() || function.environment.is_some() {
        return Err(fail("selected CT query wrapper has another execution role"));
    }
    let original_definition = inventory.definition(owner.definition, b)?;
    let index = original_index::index(original_definition, b)?;
    let SemanticOriginalNode::Expression(node) = binding.query.original else {
        return Err(fail("query wrapper has no original expression anchor"));
    };
    let original_expression = index.get_expr(node, b)?;
    let CoreExprKind::CompileTime(payload) = &original_expression.kind else {
        return Err(fail(
            "query wrapper is not anchored at an original compile-time operation",
        ));
    };
    let SemanticDefinitionBody::FunctionWithBody {
        parameters,
        names,
        body: SemanticOriginalFunctionBody::Defined(body),
        ..
    } = &definition.body
    else {
        return Err(fail("query wrapper omits its full generated original body"));
    };
    if !parameters.is_empty()
        || !definition.parameters.is_empty()
        || !definition.environment.is_empty()
        || !definition.requirements.is_empty()
    {
        return Err(unsupported(
            "query wrapper needs its complete declared argument, environment and premise bridge",
        ));
    }
    let SemanticDefinitionBody::FunctionWithBody {
        names: original_names,
        ..
    } = &original_definition.body
    else {
        return Err(fail(
            "query wrapper parent omits its complete original name table",
        ));
    };
    b.spend(names.len().saturating_add(original_names.len()), WORK)?;
    if names != original_names {
        return Err(unsupported(
            "query wrapper names need their complete lexical recipe relocation proof",
        ));
    }
    let [statement] = body.statements.as_slice() else {
        return Err(fail(
            "query wrapper must have its exact generated return body",
        ));
    };
    let CoreStatementKind::Return(Some(actual)) = &statement.kind else {
        return Err(fail("query wrapper has another original operation"));
    };
    crate::dyn_metadata::recipe(payload.as_ref(), b)?;
    crate::dyn_metadata::recipe(actual, b)?;
    let mut expected = payload.as_ref().clone();
    while let CoreExprKind::CompileTime(inner) = expected.kind {
        expected = *inner;
    }
    let mut actual = actual.clone();
    original::normalize_expr(&mut expected, b, 0)?;
    original::normalize_expr(&mut actual, b, 0)?;
    if expected != actual {
        return Err(fail("query wrapper changes the original selected payload"));
    }
    for instance in &selected.instances {
        if instance.id == wrapper.id {
            continue;
        }
        let original = inventory.instance(instance.id, b)?;
        if original.definition != instance.definition
            || original.parent != instance.parent
            || original.actuals != instance.actuals
            || original.environment != instance.environment
            || original.self_type != instance.self_type
        {
            return Err(unsupported(
                "selected source instance needs its complete independently resolved Core actual context",
            ));
        }
        if !matches!(
            (&original.target, &instance.target),
            (
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { .. }),
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))
            )
        ) {
            return Err(fail(
                "selected CT function changes its original Core target role",
            ));
        }
    }
    for item in &selected.definitions {
        if item.id == wrapper.definition {
            continue;
        }
        b.spend(source.definitions.len(), WORK)?;
        let original = source
            .definitions
            .iter()
            .find(|original| original.id == item.id)
            .ok_or_else(|| fail("selected CT graph introduces an unrelated original definition"))?;
        crate::dyn_metadata::recipe(item, b)?;
        crate::dyn_metadata::recipe(original, b)?;
        if item != original {
            return Err(fail(
                "selected CT graph changes an original body, requirement, name or branch",
            ));
        }
    }
    Ok(wrapper.definition)
}
