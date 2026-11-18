//! Candidate Code inputs from the complete lexical Source constraint graph.
//! A candidate only starts an original equation. The original body and real
//! calls must prove that equation before the original context can be published.
use crate::*;
use std::collections::BTreeMap;
use zil_base::{ModuleId, NodeId};

type CoreOrigin = (ModuleId, NodeId, NodeId);
type CoreParameters = Vec<(u32, String, SemanticTypeRecipe)>;

#[derive(Default)]
pub(crate) struct CoreInputs(BTreeMap<CoreOrigin, CoreParameters>);
pub(super) type Goals = BTreeMap<SemanticDefinitionId, BTreeMap<u32, SemanticTypeRecipe>>;

pub(super) const MISSING: &str = "E-ORIGINAL-CALLABLE-CONTEXT";

fn pay(left: &mut usize, work: usize) -> R<()> {
    *left = left.checked_sub(work).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original callable input context work exceeded", None)
    })?;
    Ok(())
}

impl CoreInputs {
    pub(crate) fn record(
        &mut self,
        origin: CoreOrigin,
        parameters: &[CoreClosureParameter],
        span: Span,
        left: &mut usize,
    ) -> R<()> {
        let mut candidates = Vec::new();
        for (ordinal, parameter) in parameters.iter().enumerate() {
            if parameter.mode != PassingMode::Infer {
                continue;
            }
            let Some(ty @ Type::FunctionPointer { .. }) = parameter.ty.as_ref() else {
                continue;
            };
            // This tree has already received the existing Core structural and
            // depth checks. Conversion pays again for each retained component.
            if let Some(recipe) = closed_recipe(ty, left)? {
                pay(left, parameter.name.len().saturating_add(1))?;
                candidates.push((ordinal as u32, parameter.name.clone(), recipe));
            }
        }
        if !candidates.is_empty() {
            pay(left, self.0.len().saturating_add(1).ilog2() as usize + 1)?;
            if self.0.insert(origin, candidates).is_some() {
                return Err(error("E-ORIGINAL-CALLABLE-ORIGIN", span,
                    "one Core closure occurrence names multiple callable input contexts"));
            }
        }
        Ok(())
    }
}

fn closed_recipe(ty: &Type, left: &mut usize) -> R<Option<SemanticTypeRecipe>> {
    pay(left, 1)?;
    Ok(Some(match ty {
        Type::FunctionPointer { parameters, result } => {
            pay(left, parameters.len())?;
            let mut inputs = Vec::new();
            for parameter in parameters {
                let Some(ty) = closed_recipe(parameter, left)? else { return Ok(None); };
                inputs.push(SemanticParameterRecipe { ty, mode: PassingMode::Infer });
            }
            let Some(result) = closed_recipe(result, left)? else { return Ok(None); };
            SemanticTypeRecipe::Code {
                kind: SemanticCodeKind::FunctionPointer,
                signature: Box::new(SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes: 0,
                    parameters: inputs,
                    result,
                    result_mode: PassingMode::Infer,
                    call_kind: CallKind::Direct,
                }),
            }
        }
        Type::Tuple(types) => {
            pay(left, types.len())?;
            let mut fields = Vec::new();
            for ty in types {
                let Some(field) = closed_recipe(ty, left)? else { return Ok(None); };
                fields.push(field);
            }
            SemanticTypeRecipe::Tuple(fields)
        }
        Type::Array { element, length } => {
            let Some(element) = closed_recipe(element, left)? else { return Ok(None); };
            SemanticTypeRecipe::Array {
                element: Box::new(element),
                length: Box::new(SemanticConstRecipe::Value(Constant::U32(*length))),
            }
        }
        ty if ty.is_integer() || ty.is_float() || matches!(ty, Type::Bool | Type::Unit) =>
            SemanticTypeRecipe::Atom(ty.clone()),
        _ => return Ok(None),
    }))
}

pub(super) fn bind(
    program: &SemanticModule,
    inputs: &CoreInputs,
    limits: &Limits,
    left: &mut usize,
) -> R<Goals> {
    let mut goals = Goals::new();
    if inputs.0.is_empty() {
        return Ok(goals);
    }
    let scan = program.instances.len().saturating_add(program.definitions.len()).saturating_add(program.sites.len());
    pay(left, program.instances.len())?;
    for entry in &program.instances {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
            module, creator, expression, occurrence: 0,
        }) = &entry.target else { continue; };
        pay(left, inputs.0.len().saturating_add(1).ilog2() as usize + 1)?;
        let Some(candidates) = inputs.0.get(&(*module, *creator, *expression)) else { continue; };
        if entry.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid || !entry.actuals.is_empty() {
            continue;
        }
        pay(left, scan)?;
        let Some(child) = program.definitions.iter().find(|row| row.id == entry.definition) else { continue; };
        if child.kind != SemanticDefinitionKind::Closure || !child.parameters.is_empty() {
            continue;
        }
        let Some(parent) = program.instances.iter().find(|row| Some(row.id) == entry.parent)
            .filter(|row| Some(row.definition) == child.parent) else { continue; };
        let matching_creator = match &parent.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module: home, function }) =>
                home == module && function == creator,
            SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure { module: home, creator: root, occurrence: 0, .. }) =>
                home == module && root == creator,
            _ => false,
        };
        if !matching_creator || parent.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid {
            continue;
        }
        let Some(parent_definition) = program.definitions.iter().find(|row| row.id == parent.definition)
            else { continue; };
        let mut ancestor = Some(parent_definition);
        let mut closed = true;
        let mut visited = 0usize;
        while let Some(row) = ancestor {
            if !row.parameters.is_empty() || !row.requirements.is_empty() {
                closed = false;
                break;
            }
            visited += 1;
            if visited > program.definitions.len() {
                return Err(error("E-ORIGINAL-CALLABLE-ORIGIN", Span::default(),
                    "original closure creator ancestry contains a cycle"));
            }
            if let Some(next) = row.parent {
                pay(left, program.definitions.len())?;
                ancestor = program.definitions.iter().find(|row| row.id == next);
                if ancestor.is_none() {
                    closed = false;
                }
            } else {
                ancestor = None;
            }
        }
        if !closed {
            continue;
        }
        let SemanticDefinitionBody::FunctionWithBody { sites, site_nodes, .. } = &parent_definition.body
            else { continue; };
        let SemanticDefinitionBody::FunctionWithBody { signature, parameters, .. } = &child.body
            else { continue; };
        let mut sites_at_occurrence = program.sites.iter().filter(|row| row.instance == parent.id
            && matches!(row.at, SemanticSiteAddress::Core {
                expression: at, role: SemanticEmissionRole::Closure, occurrence: 0
            } if at == *expression));
        let Some(site) = sites_at_occurrence.next() else { continue; };
        pay(left, sites.len().saturating_add(site_nodes.len()))?;
        if sites_at_occurrence.next().is_some()
            || !site_nodes.iter().any(|node| node.site == site.recipe
                && matches!(node.node, SemanticOriginalNode::Expression(_)))
            || !sites.iter().any(|row| row.id == site.recipe
                && matches!(&row.kind, SemanticSiteRecipeKind::Closure { definition, .. } if *definition == child.id))
            || !matches!(&site.action, SemanticSiteAction::Closure {
                closure: SemanticClosureTarget::Core { module: home, creator: function, expression: at, occurrence: 0 },
                ..
            } if home == module && function == creator && at == expression) {
            return Err(error("E-ORIGINAL-CALLABLE-ORIGIN", Span::default(),
                "callable input context changes its original lexical creation occurrence"));
        }
        let mut child_goals = BTreeMap::new();
        for (ordinal, name, recipe) in candidates {
            let index = *ordinal as usize;
            let Some(binder) = parameters.get(index) else { continue; };
            let Some(parameter) = signature.parameters.get(index) else { continue; };
            if binder.name == *name && binder.ty.is_none() && binder.mode == PassingMode::Infer
                && parameter.mode == PassingMode::Infer
                && matches!(&parameter.ty, SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(parameter)
                } if parameter == ordinal)
            {
                crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
                child_goals.insert(*ordinal, recipe.clone());
            }
        }
        if !child_goals.is_empty() && goals.insert(child.id, child_goals).is_some() {
            return Err(error("E-ORIGINAL-CALLABLE-ORIGIN", Span::default(),
                "one original callable input has multiple Core creation occurrences"));
        }
    }
    Ok(goals)
}
