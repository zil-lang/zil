//! The original lexical inventory is untrusted. Validate its exact links before
//! interpreting any class expression; this pass grants no memory permission.
use crate::{bad, budget::Budget};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::CompileResult;
use zil_ir::*;

const WORK: &str = "original semantic program inventory";
fn fail(message: &str) -> Vec<zil_base::Diagnostic> {
    bad(format!("semantic program: {message}"))
}

struct Inventory<'a> {
    definitions: BTreeMap<SemanticDefinitionId, &'a SemanticDefinition>,
    instances: BTreeMap<SemanticInstanceId, &'a SemanticInstance>,
    functions: BTreeMap<FunctionId, &'a TypedFunction>,
}
impl<'a> Inventory<'a> {
    fn new(
        module: &'a TypedModule,
        program: &'a SemanticModule,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        // The generated, bounded visitor includes every type, class, constant,
        // identity, recipe and actual occurrence. No module hash runs first.
        crate::dyn_metadata::recipe(program, b)?;
        let mut inventory = Self {
            definitions: BTreeMap::new(),
            instances: BTreeMap::new(),
            functions: BTreeMap::new(),
        };
        let mut identities = BTreeSet::new();
        for definition in &program.definitions {
            b.spend(
                inventory
                    .definitions
                    .len()
                    .saturating_add(identities.len())
                    .saturating_add(1),
                WORK,
            )?;
            if inventory
                .definitions
                .insert(definition.id, definition)
                .is_some()
                || !identities.insert(&definition.identity)
            {
                return Err(fail("original definition identity is ambiguous"));
            }
        }
        for instance in &program.instances {
            b.spend(inventory.instances.len().saturating_add(1), WORK)?;
            if inventory.instances.insert(instance.id, instance).is_some() {
                return Err(fail("lexical instance identity is ambiguous"));
            }
        }
        for function in &module.functions {
            b.spend(inventory.functions.len().saturating_add(1), WORK)?;
            if inventory.functions.insert(function.id, function).is_some() {
                return Err(fail("actual function identity is ambiguous"));
            }
        }
        Ok(inventory)
    }
    fn definition(
        &self,
        id: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticDefinition> {
        b.spend(self.definitions.len().saturating_add(1), WORK)?;
        self.definitions
            .get(&id)
            .copied()
            .ok_or_else(|| fail("original lexical definition is absent"))
    }
    fn instance(
        &self,
        id: SemanticInstanceId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticInstance> {
        b.spend(self.instances.len().saturating_add(1), WORK)?;
        self.instances
            .get(&id)
            .copied()
            .ok_or_else(|| fail("actual lexical instance is absent"))
    }
    fn function(
        &self,
        target: &SemanticFunctionTarget,
        b: &mut Budget,
    ) -> CompileResult<&'a TypedFunction> {
        let SemanticFunctionTarget::Typed(id) = target else {
            return Err(fail("executable program retains a Core function target"));
        };
        b.spend(self.functions.len().saturating_add(1), WORK)?;
        self.functions
            .get(id)
            .copied()
            .ok_or_else(|| fail("exact actual function target is absent"))
    }
    fn parents(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<()> {
        for definition in &program.definitions {
            let mut active = BTreeSet::new();
            let mut current = Some(definition.id);
            while let Some(id) = current {
                b.spend(active.len().saturating_add(1), WORK)?;
                if !active.insert(id) {
                    return Err(fail("original lexical parent graph is cyclic"));
                }
                current = self.definition(id, b)?.parent;
            }
        }
        for instance in &program.instances {
            let definition = self.definition(instance.definition, b)?;
            if instance.actuals.len() != definition.parameters.len() {
                return Err(fail("instance omits an original mixed formal"));
            }
            match (instance.parent, definition.parent) {
                (None, None) => {}
                (Some(parent), Some(expected))
                    if self.instance(parent, b)?.definition == expected => {}
                _ => return Err(fail("instance changes its original lexical parent")),
            }
            let mut active = BTreeSet::new();
            let mut current = Some(instance.id);
            while let Some(id) = current {
                b.spend(active.len().saturating_add(1), WORK)?;
                if !active.insert(id) {
                    return Err(fail("actual lexical parent graph is cyclic"));
                }
                current = self.instance(id, b)?.parent;
            }
            match (&instance.target, &definition.body) {
                (
                    SemanticInstanceTarget::Function(target),
                    SemanticDefinitionBody::Function { .. },
                ) => {
                    self.function(target, b)?;
                }
                (
                    SemanticInstanceTarget::Scope,
                    SemanticDefinitionBody::Nominal { .. } | SemanticDefinitionBody::Scope(_),
                ) => {
                    if !instance.header_embedding.is_empty() {
                        return Err(fail("nonfunction scope invents runtime header binders"));
                    }
                }
                _ => return Err(fail("instance changes its original definition kind")),
            }
            if let SemanticLifetimeInstantiation::Partition { rigid } =
                instance.lifetime_instantiation
            {
                let original = self.instance(rigid, b)?;
                if original.definition != instance.definition
                    || !matches!(
                        original.lifetime_instantiation,
                        SemanticLifetimeInstantiation::Rigid
                    )
                {
                    return Err(fail("partition has no exact original rigid instance"));
                }
            }
        }
        Ok(())
    }
    fn sites(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<()> {
        let mut emitted = BTreeSet::new();
        for site in &program.sites {
            let instance = self.instance(site.instance, b)?;
            let definition = self.definition(instance.definition, b)?;
            let (
                SemanticInstanceTarget::Function(target),
                SemanticDefinitionBody::Function { sites, .. },
            ) = (&instance.target, &definition.body)
            else {
                return Err(fail("emission site does not belong to an actual function"));
            };
            let function = self.function(target, b)?;
            b.spend(
                sites.len().saturating_add(emitted.len()).saturating_add(1),
                WORK,
            )?;
            let mut recipes = sites.iter().filter(|recipe| recipe.id == site.recipe);
            let recipe = recipes
                .next()
                .ok_or_else(|| fail("actual emission site has no original recipe"))?;
            if recipes.next().is_some() {
                return Err(fail("original emission recipe is ambiguous"));
            }
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge,
            } = site.at
            else {
                return Err(fail("executable program retains a Core emission site"));
            };
            if !emitted.insert((
                site.instance,
                block.0,
                instruction,
                edge == SemanticSiteEdge::After,
            )) {
                return Err(fail("actual emission boundary has competing ghost actions"));
            }
            b.spend(function.blocks.len().saturating_add(1), WORK)?;
            let block = function
                .blocks
                .iter()
                .find(|candidate| candidate.id == block)
                .ok_or_else(|| fail("emission site names an absent actual block"))?;
            let actual = block.instructions.get(instruction as usize);
            if actual.is_none()
                && !(instruction as usize == block.instructions.len()
                    && edge == SemanticSiteEdge::Before
                    && matches!(site.action, SemanticSiteAction::TypeUse { .. }))
            {
                return Err(fail(
                    "emission site is outside its actual instruction boundary",
                ));
            }
            match (&recipe.kind, &site.action) {
                (
                    SemanticSiteRecipeKind::Construct { .. },
                    SemanticSiteAction::Construct {
                        result: SemanticOperand::Value(result),
                        ..
                    },
                ) => {
                    if actual.and_then(|instruction| instruction.result) != Some(*result) {
                        return Err(fail("constructor site changes its actual result SSA"));
                    }
                }
                (SemanticSiteRecipeKind::Call { .. }, SemanticSiteAction::Call { target, .. }) => {
                    let callee = self.function(target, b)?;
                    if !matches!(actual.map(|instruction| &instruction.kind), Some(InstructionKind::Call { function, .. } | InstructionKind::SuspendCall { function, .. } | InstructionKind::ForeignCall { function, .. }) if *function == callee.id)
                    {
                        return Err(fail("call site changes the actual called definition"));
                    }
                }
                (
                    SemanticSiteRecipeKind::DeclareLocal { .. },
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(local),
                        ..
                    },
                ) => {
                    if !matches!(actual.map(|instruction| &instruction.kind), Some(InstructionKind::StorageLive(actual)) if actual == local)
                    {
                        return Err(fail(
                            "local declaration does not name its actual StorageLive boundary",
                        ));
                    }
                }
                (SemanticSiteRecipeKind::TypeUse { .. }, SemanticSiteAction::TypeUse { .. }) => {}
                (
                    SemanticSiteRecipeKind::Closure { .. },
                    SemanticSiteAction::Closure {
                        result: SemanticOperand::Value(result),
                        closure: SemanticClosureTarget::Typed(closure),
                        ..
                    },
                ) => {
                    if actual.and_then(|instruction| instruction.result) != Some(*result)
                        || !matches!(function.values.get(result.0 as usize), Some(Type::Closure(actual)) if actual == closure)
                    {
                        return Err(fail("closure site changes its actual environment identity"));
                    }
                }
                _ => return Err(fail("emission action changes its original recipe kind")),
            }
        }
        Ok(())
    }
}

/// Structural prerequisites only. The final guard remains until original recipe
/// substitution and actual SSA class flow have both discharged every obligation.
pub(super) fn pending(
    module: &TypedModule,
    program: &SemanticModule,
    b: &mut Budget,
) -> CompileResult<()> {
    let inventory = Inventory::new(module, program, b)?;
    inventory.parents(program, b)?;
    inventory.sites(program, b)?;
    Err(vec![zil_base::Diagnostic::unsupported(
        zil_base::Phase::Analyze,
        "lexical semantic program requires independent complete class flow",
        None,
    )])
}
