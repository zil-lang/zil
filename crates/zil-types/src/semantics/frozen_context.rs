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
fn source_interface(
    source: &CoreProgram,
    binding: &CtFrozenContext,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
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
    let mut program = SemanticModule {
        definitions: Vec::new(),
        instances: Vec::new(),
        sites: Vec::new(),
        dyn_scopes: Vec::new(),
    };
    for module in &modules {
        let Some(original) = &module.semantic else {
            return Err(pending(
                "frozen source module lacks its complete original semantic program",
            ));
        };
        crate::dyn_metadata::semantic_snapshot(original, limits, left, 1)?;
        pay(
            left,
            original
                .definitions
                .len()
                .saturating_add(original.instances.len())
                .saturating_add(original.sites.len())
                .saturating_add(original.dyn_scopes.len()),
        )?;
        let copy = original.clone();
        program.definitions.extend(copy.definitions);
        program.instances.extend(copy.instances);
        program.sites.extend(copy.sites);
        program.dyn_scopes.extend(copy.dyn_scopes);
    }
    // Includes unused definitions, unselected branches, original mixed-formal
    // rigidity, and original where premises. No E Type or caller Mono actual
    // can change this proof's inputs.
    let proof = super::original::check(&program, limits, left)?;
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
    Ok(())
}
pub(crate) fn check(
    executable: &CoreProgram,
    source: &CoreProgram,
    binding: &CtFrozenContext,
    _target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedModule> {
    let mut left = limits.max_analysis_steps;
    source_interface(source, binding, limits, &mut left).map_err(|error| vec![error])?;
    if executable.source_fingerprint != source.source_fingerprint {
        return Err(vec![bad(
            "selected executable belongs to another frozen source",
        )]);
    }
    // No success is published while E's free names, full actuals and sealed
    // environment still lack a checked bridge to this exact frozen Γ. Calling
    // ordinary check_program(E), clearing Some, or lowering S into E here
    // would erase precisely that obligation.
    Err(vec![pending(
        "selected executable requires its complete frozen-source type/environment substitution bridge",
    )])
}
