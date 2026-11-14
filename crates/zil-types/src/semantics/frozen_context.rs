//! CT's immutable source interface is not an executable module. This entry
//! validates the original symbolic source world without lowering its bodies.
//! The selected executable must separately discharge its contextual bridge.
use crate::*;
fn bad(message: &str) -> Diagnostic {
    error("E-CT-FROZEN-TYPES", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
fn pay(left: &mut usize, count: usize) -> R<()> {
    *left = left.checked_sub(count).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "frozen source interface work exceeded", None)
    })?;
    Ok(())
}
/// Read-only source facts. Source targets remain Core and are never emitted
/// or reclassified as lexical Scope targets. Runtime lookups use E's ledger.
struct SourceSymbols {
    context: std::rc::Rc<super::original_context::OriginalContext>,
    owner: SemanticInstanceId,
}
impl std::ops::Deref for SourceSymbols {
    type Target = super::original_context::OriginalContext;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}
pub(crate) struct FrozenSource {
    context: std::rc::Rc<super::original_context::OriginalContext>,
    pub(super) owner: SemanticInstanceId,
    pub(super) query: CtQueryOrigin,
    pub(super) wrapper_instance: SemanticInstanceId,
    pub(super) wrapper_definition: SemanticDefinitionId,
}
impl std::ops::Deref for FrozenSource {
    type Target = super::original_context::OriginalContext;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}
fn source_interface(
    source: &CoreProgram,
    binding: &CtFrozenContext,
    limits: &Limits,
    left: &mut usize,
) -> R<SourceSymbols> {
    if binding.stage.source_fingerprint != source.source_fingerprint {
        return Err(bad(
            "frozen source fingerprint differs from the selected query request",
        ));
    }
    if binding.query.occurrence != 0 {
        return Err(bad(
            "frozen query does not identify a real source mint occurrence",
        ));
    }
    if source.modules.len() > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "frozen source module inventory exceeded",
            None,
        ));
    }
    pay(left, source.modules.len().saturating_add(1))?;
    let modules: Vec<_> = source.modules.iter().map(|row| &row.core).collect();
    super::well_formed::originals(&modules, limits, left)?;
    if modules.iter().any(|module| module.semantic.is_none()) {
        return Err(pending(
            "frozen source module lacks its complete original semantic program",
        ));
    }
    let input = super::original_context::Input::collect(&modules, limits, left)?;
    let program = &input.semantic;
    // Includes unused definitions, unselected branches, original mixed-formal
    // rigidity, and original where premises. No E Type or caller Mono actual
    // can change this proof's inputs.
    let instance = match binding.query.owner {
        CtQueryOwner::Instance(id) => {
            pay(left, program.instances.len())?;
            program
                .instances
                .iter()
                .find(|row| row.id == id)
                .ok_or_else(|| bad("frozen query has no actual source instance"))?
        }
        CtQueryOwner::Definition(id) => {
            pay(left, program.definitions.len())?;
            if !program.definitions.iter().any(|row| row.id == id) {
                return Err(bad("frozen query has no original source definition"));
            }
            return Err(pending(
                "definition-owned frozen query needs its exact original emission context",
            ));
        }
        CtQueryOwner::Module(id) => {
            pay(left, modules.len())?;
            if !modules.iter().any(|row| row.module_id == id) {
                return Err(bad("frozen query has no actual source module"));
            }
            return Err(pending(
                "module-owned frozen query needs its immutable original expression directory",
            ));
        }
    };
    pay(left, program.definitions.len())?;
    let definition = program
        .definitions
        .iter()
        .find(|row| row.id == instance.definition)
        .ok_or_else(|| bad("frozen query instance lost its original declaration"))?;
    let proof =
        super::original::check_at(program, definition.id, binding.query.original, limits, left)?;
    if !proof.complete.contains(&definition.id) {
        return Err(bad(
            "frozen query owner has no complete original body proof",
        ));
    }
    let nodes = super::original_nodes::definition(definition, limits, left)?;
    let (node, is_expression) = match binding.query.original {
        SemanticOriginalNode::Expression(node) => (node, true),
        SemanticOriginalNode::Statement(node) => (node, false),
    };
    pay(left, nodes.len().saturating_add(1).ilog2() as usize + 1)?;
    let kind = nodes
        .get(&node)
        .ok_or_else(|| bad("frozen query leaves its actual original body"))?;
    let expression = !matches!(
        kind,
        super::original_nodes::Kind::Binding | super::original_nodes::Kind::Statement
    );
    if expression != is_expression {
        return Err(bad(
            "frozen query switches its original expression/statement namespace",
        ));
    }
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
        return Err(bad(
            "frozen query original omits its complete operation body",
        ));
    };
    pay(left, site_nodes.len().saturating_add(program.sites.len()))?;
    let mut linked = 0;
    for original in site_nodes {
        if original.node != binding.query.original {
            continue;
        }
        pay(left, program.sites.len())?;
        for actual in &program.sites {
            if actual.instance == instance.id
                && actual.recipe == original.site
                && matches!(actual.at,SemanticSiteAddress::Core {expression,occurrence,..}
                    if expression==binding.query.expression&&occurrence==binding.query.occurrence)
            {
                linked += 1;
            }
        }
    }
    if linked == 0 {
        return Err(pending(
            "frozen query requires its actual original-site to Core emission relation",
        ));
    }
    let owner = instance.id;
    let context = super::original_context::OriginalContext::checked(input, proof, None, left)?;
    Ok(SourceSymbols { context, owner })
}
fn wrapper_relation(
    executable: &CoreProgram,
    source: &CoreProgram,
    frozen: &SourceSymbols,
    binding: &CtFrozenContext,
    limits: &Limits,
    left: &mut usize,
) -> R<(SemanticInstanceId, SemanticDefinitionId)> {
    let SemanticFunctionTarget::Core { module, function } = &binding.query.wrapper else {
        return Err(bad(
            "Types frozen wrapper must name its real generated Core function",
        ));
    };
    if executable.modules.len() > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "frozen executable module inventory exceeded",
            None,
        ));
    }
    pay(
        left,
        executable
            .modules
            .len()
            .saturating_add(source.modules.len()),
    )?;
    let mut raw = None;
    let mut wrapper = None;
    let mut definition = None;
    for source_module in &source.modules {
        if source_module.core.module_id != *module {
            continue;
        }
        pay(left, source_module.core.functions.len())?;
        if source_module
            .core
            .functions
            .iter()
            .any(|row| row.id == *function)
        {
            return Err(bad(
                "generated query wrapper aliases a frozen source function",
            ));
        }
    }
    for module_row in &executable.modules {
        if module_row.core.module_id == *module {
            pay(left, module_row.core.functions.len())?;
            for candidate in &module_row.core.functions {
                if candidate.id == *function && raw.replace(candidate).is_some() {
                    return Err(bad("generated Core wrapper identity is duplicated"));
                }
            }
        }
        let Some(semantic) = &module_row.core.semantic else {
            return Err(pending(
                "selected executable lacks its complete semantic program",
            ));
        };
        crate::dyn_metadata::semantic_snapshot(semantic, limits, left, 1)?;
        pay(left, semantic.instances.len())?;
        for instance in &semantic.instances {
            if !matches!(&instance.target,SemanticInstanceTarget::Function(target) if target==&binding.query.wrapper)
            {
                continue;
            }
            if wrapper.replace(instance).is_some() {
                return Err(bad("generated query wrapper has multiple actual instances"));
            }
            pay(left, semantic.definitions.len())?;
            definition = semantic
                .definitions
                .iter()
                .find(|row| row.id == instance.definition);
        }
    }
    let raw =
        raw.ok_or_else(|| bad("selected executable omits its actual generated Core wrapper"))?;
    let wrapper =
        wrapper.ok_or_else(|| bad("selected executable omits its semantic wrapper instance"))?;
    let definition = definition
        .ok_or_else(|| bad("selected executable omits its generated wrapper original body"))?;
    let parent = frozen.context.instance(frozen.owner, left)?;
    let parent_definition = frozen.context.definition(parent.definition, left)?;
    if wrapper.parent != Some(parent.id)
        || definition.parent != Some(parent.definition)
        || definition.kind != SemanticDefinitionKind::Function
        || !definition.parameters.is_empty()
        || !definition.environment.is_empty()
        || !definition.requirements.is_empty()
        || !wrapper.actuals.is_empty()
        || !raw.parameters.is_empty()
        || raw.receiver.is_some()
    {
        return Err(bad(
            "generated wrapper changes its real frozen parent or adds new premises",
        ));
    }
    if frozen
        .semantic
        .definitions
        .iter()
        .any(|row| row.id == definition.id)
        || frozen
            .semantic
            .instances
            .iter()
            .any(|row| row.id == wrapper.id)
    {
        return Err(bad(
            "generated wrapper reuses a frozen declaration/instance identity",
        ));
    }
    let (
        SemanticDefinitionBody::FunctionWithBody { names: actual, .. },
        SemanticDefinitionBody::FunctionWithBody {
            names: original, ..
        },
    ) = (&definition.body, &parent_definition.body)
    else {
        return Err(bad("frozen wrapper/parent lacks its full original body"));
    };
    if actual.len() != original.len() {
        return Err(bad(
            "generated wrapper loses or invents original source names",
        ));
    }
    pay(left, actual.len())?;
    let empty = std::collections::BTreeMap::new();
    let substitution = super::original::substitute::Substitution {
        frames: &[],
        selves: &[],
        environments: &[],
        implicit: &empty,
        invocation: &empty,
        shift: 1,
    };
    for (actual, original) in actual.iter().zip(original) {
        crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
        if actual.path != original.path || actual.namespace != original.namespace {
            return Err(bad(
                "generated wrapper changes its original declaration-name binding",
            ));
        }
        let value = match &original.value {
            SemanticOriginalNameValue::Recipe(recipe) => SemanticOriginalNameValue::Recipe(
                super::original::substitute::actual(recipe, &substitution, limits, left)?,
            ),
            SemanticOriginalNameValue::Definition(id) => SemanticOriginalNameValue::Definition(*id),
        };
        if actual.value != value {
            return Err(bad(
                "generated wrapper rebases a source name into another scope",
            ));
        }
    }
    Ok((wrapper.id, definition.id))
}
pub(crate) fn check(
    executable: &CoreProgram,
    source: &CoreProgram,
    binding: &CtFrozenContext,
    _target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedModule> {
    let mut left = limits.max_analysis_steps;
    let frozen =
        source_interface(source, binding, limits, &mut left).map_err(|error| vec![error])?;
    let (wrapper_instance, wrapper_definition) =
        wrapper_relation(executable, source, &frozen, binding, limits, &mut left)
            .map_err(|error| vec![error])?;
    pay(&mut left, 1).map_err(|error| vec![error])?;
    let frozen = FrozenSource {
        context: frozen.context,
        owner: frozen.owner,
        query: binding.query.clone(),
        wrapper_instance,
        wrapper_definition,
    };
    if executable.source_fingerprint != source.source_fingerprint {
        return Err(vec![bad(
            "selected executable belongs to another frozen source",
        )]);
    }
    // Only E enters inference, shape resolution and actual CFG emission. The
    // immutable S remains a separate private lookup context for every stage.
    let remaining = Limits {
        max_analysis_steps: left,
        ..limits.clone()
    };
    crate::program::check_program_with_frozen(
        executable,
        _target,
        &remaining,
        std::rc::Rc::new(frozen),
    )
    .map(|program| program.module)
}
