//! The original lexical inventory is untrusted. Validate its exact links before
//! interpreting any class expression; this pass grants no memory permission.
use crate::{bad, budget::Budget};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::CompileResult;
use zil_ir::*;
mod closed;
pub(super) mod frozen;
mod original;
mod original_index;
mod value;

const WORK: &str = "original semantic program inventory";
fn fail(message: &str) -> Vec<zil_base::Diagnostic> {
    bad(format!("semantic program: {message}"))
}

struct Inventory<'a> {
    module: &'a TypedModule,
    limits: &'a zil_base::Limits,
    source: Option<&'a CoreProgram>,
    frozen_wrapper: Option<SemanticDefinitionId>,
    definitions: BTreeMap<SemanticDefinitionId, &'a SemanticDefinition>,
    instances: BTreeMap<SemanticInstanceId, &'a SemanticInstance>,
    functions: BTreeMap<FunctionId, &'a TypedFunction>,
}
impl<'a> Inventory<'a> {
    fn new(
        module: &'a TypedModule,
        limits: &'a zil_base::Limits,
        program: &'a SemanticModule,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        // The generated, bounded visitor includes every type, class, constant,
        // identity, recipe and actual occurrence. No module hash runs first.
        crate::dyn_metadata::recipe(program, b)?;
        let mut inventory = Self {
            module,
            limits,
            source: None,
            frozen_wrapper: None,
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
            let identity = &definition.identity;
            b.spend(
                identity
                    .module_path
                    .len()
                    .saturating_add(identity.declaration_path.len()),
                WORK,
            )?;
            let width = identity
                .module_path
                .iter()
                .chain(&identity.declaration_path)
                .fold(
                    identity
                        .package
                        .len()
                        .saturating_add(identity.generation_path.len())
                        .saturating_add(identity.module_path.len())
                        .saturating_add(identity.declaration_path.len())
                        .saturating_add(1),
                    |size, part| size.saturating_add(part.len()),
                );
            // Comparing qualified strings is not an integer-key lookup. Pay a
            // complete candidate-key upper before any BTree comparison.
            b.spend(identities.len().saturating_mul(width), WORK)?;
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
    fn resolved(value: &SemanticTypeValue, b: &mut Budget) -> CompileResult<()> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(fail("executable program retains a Core type operand"));
        };
        crate::dyn_metadata::ty(ty, b)?;
        b.spend(classes.len().saturating_add(1), WORK)?;
        if classes.len() != life.binder_count as usize {
            return Err(fail("resolved type loses an intrinsic class occurrence"));
        }
        for class in classes {
            let operand = match class {
                SemanticClassExpr::OuterOccurrence { operand, .. }
                | SemanticClassExpr::IntrinsicClass { operand, .. } => Some(operand),
                _ => None,
            };
            if matches!(operand, Some(SemanticOperand::CoreExpression { .. })) {
                return Err(fail("resolved class retains a Core expression selector"));
            }
        }
        // Equality of the complete intrinsic shape and every original recipe is
        // a separate obligation. Count equality alone never supplies that proof.
        Ok(())
    }
    fn actuals(actuals: &[SemanticActual], b: &mut Budget) -> CompileResult<()> {
        b.spend(actuals.len(), WORK)?;
        for actual in actuals {
            match actual {
                SemanticActual::Type(value) | SemanticActual::Const { ty: value, .. } => {
                    Self::resolved(value, b)?
                }
                SemanticActual::CoreRecipe { .. } | SemanticActual::CoreInferred { .. } => {
                    return Err(fail("executable program retains an unresolved Core actual"));
                }
                SemanticActual::Life(
                    SemanticClassExpr::OuterOccurrence {
                        operand: SemanticOperand::CoreExpression { .. },
                        ..
                    }
                    | SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::CoreExpression { .. },
                        ..
                    },
                ) => {
                    return Err(fail("actual lifetime retains a Core expression selector"));
                }
                SemanticActual::Life(_) => {}
            }
        }
        Ok(())
    }
    fn parents(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<()> {
        for definition in &program.definitions {
            let mut implicit_paths = BTreeSet::new();
            for (ordinal, row) in definition.implicit_lifetimes.iter().enumerate() {
                b.spend(
                    implicit_paths
                        .len()
                        .saturating_mul(row.occurrence_path.len().saturating_add(1))
                        .saturating_add(1),
                    WORK,
                )?;
                if row.ordinal as usize != ordinal || !implicit_paths.insert(&row.occurrence_path) {
                    return Err(fail(
                        "original implicit occurrence directory is not exact and dense",
                    ));
                }
            }
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
            Self::actuals(&instance.actuals, b)?;
            if let Some(value) = &instance.self_type {
                Self::resolved(value, b)?;
            }
            if let Some(environment) = &instance.environment {
                Self::actuals(&environment.actuals, b)?;
                if !matches!(environment.closure, SemanticClosureTarget::Typed(_)) {
                    return Err(fail("environment retains a Core closure target"));
                }
            }
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
                    SemanticDefinitionBody::FunctionWithBody { .. },
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
        // Some is a complete original program claim. An empty side table
        // cannot bypass declaration obligations for a real executable entry.
        for function in self.functions.values() {
            b.spend(program.instances.len().saturating_add(1), WORK)?;
            let count = program.instances.iter().filter(|instance| matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function.id)).count();
            if count != 1 {
                return Err(fail(
                    "actual function does not have exactly one complete original instance",
                ));
            }
        }
        Ok(())
    }
    fn sites(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<()> {
        let mut emitted = BTreeSet::new();
        let mut consumed = BTreeSet::new();
        for site in &program.sites {
            let instance = self.instance(site.instance, b)?;
            let definition = self.definition(instance.definition, b)?;
            let (
                SemanticInstanceTarget::Function(target),
                SemanticDefinitionBody::FunctionWithBody { sites, .. },
            ) = (&instance.target, &definition.body)
            else {
                return Err(fail("emission site does not belong to an actual function"));
            };
            let function = self.function(target, b)?;
            b.spend(
                sites
                    .len()
                    .saturating_add(emitted.len())
                    .saturating_add(consumed.len())
                    .saturating_add(1),
                WORK,
            )?;
            if !consumed.insert((site.instance, site.recipe)) {
                return Err(fail(
                    "one original semantic event is claimed by multiple actual emissions",
                ));
            }
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
                site.recipe,
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
            match &site.action {
                SemanticSiteAction::Construct { value, .. } => Self::resolved(value, b)?,
                SemanticSiteAction::TypeUse { expected, .. }
                | SemanticSiteAction::DeclareLocal { expected, .. } => Self::resolved(expected, b)?,
                SemanticSiteAction::Call { actuals, .. }
                | SemanticSiteAction::BodyCall { actuals } => Self::actuals(actuals, b)?,
                SemanticSiteAction::CoreBodyCall => {
                    return Err(fail(
                        "executable semantic site retains an unresolved Core call",
                    ));
                }
                SemanticSiteAction::Closure { environment, .. } => Self::actuals(environment, b)?,
                SemanticSiteAction::CallableCall { .. } => {}
            }
            match (&recipe.kind, &site.action) {
                (SemanticSiteRecipeKind::OriginalCall, SemanticSiteAction::BodyCall { .. }) => {
                    if !actual.is_some_and(|instruction| {
                        matches!(
                            instruction.kind,
                            InstructionKind::Call { .. }
                                | InstructionKind::SuspendCall { .. }
                                | InstructionKind::ForeignCall { .. }
                                | InstructionKind::CallClosure { .. }
                                | InstructionKind::CallClosureOnce { .. }
                                | InstructionKind::CallClosureShared { .. }
                                | InstructionKind::CallClosureMut { .. }
                                | InstructionKind::TaskYield
                        )
                    }) {
                        return Err(fail("original body call has no actual call opcode"));
                    }
                }

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
                    SemanticSiteRecipeKind::CallableCall { .. },
                    SemanticSiteAction::CallableCall { callee },
                ) => {
                    let matches = match (actual.map(|instruction| &instruction.kind), callee) {
                        (
                            Some(
                                InstructionKind::CallClosureShared { callee: actual, .. }
                                | InstructionKind::CallClosureMut { callee: actual, .. },
                            ),
                            SemanticOperand::Local(callee),
                        ) => actual == callee,
                        (
                            Some(
                                InstructionKind::CallIndirect { callee: actual, .. }
                                | InstructionKind::CallClosure { callee: actual, .. }
                                | InstructionKind::CallClosureOnce { callee: actual, .. }
                                | InstructionKind::CallClosureBorrowed { callee: actual, .. },
                            ),
                            SemanticOperand::Value(callee),
                        ) => actual == callee,
                        _ => false,
                    };
                    if !matches {
                        return Err(fail("callable site changes its actual callee operand"));
                    }
                }
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

/// Validate the exact original inventory, then discharge the complete supported
/// instance and operation obligations. Persistent domains remain unknown until
/// their original class relation is interpreted; an empty claim is not a lane.
pub(super) fn pending(
    module: &TypedModule,
    program: &SemanticModule,
    limits: &zil_base::Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    let inventory = Inventory::new(module, limits, program, b)?;
    for definition in &program.definitions {
        b.spend(1, WORK)?;
        if matches!(definition.body, SemanticDefinitionBody::Function { .. }) {
            return Err(vec![zil_base::Diagnostic::unsupported(
                zil_base::Phase::Analyze,
                "original semantic function omits its complete body and value binders",
                None,
            )]);
        }
    }
    inventory.parents(program, b)?;
    inventory.sites(program, b)?;
    let original = original::check(module, program, &inventory, b)?;
    closed::check(module, program, &inventory, &original, b)
}
