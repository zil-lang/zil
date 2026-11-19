//! Relocation from actual emitter receipts. This stage checks emission identity
//! and opcode operands only. Original recipe/class interpretation is a separate
//! mandatory stage before the resulting untrusted program may be published.
use super::emission::{Frame, Ledger, Origin, Point};
use crate::*;
use zil_base::NodeId;
#[path = "emission_instances.rs"]
mod instances;

fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-EMISSION", Span::default(), message)
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "semantic relocation work exceeded", None)
    })?;
    Ok(())
}
fn search(n: usize) -> usize {
    n.saturating_add(1).ilog2() as usize + 1
}

enum OriginalRecipes<'a> {
    Dense(&'a [SemanticSiteRecipe]),
    Sparse(BTreeMap<SemanticSiteId, &'a SemanticSiteRecipe>),
}

impl<'a> OriginalRecipes<'a> {
    fn new(sites: &'a [SemanticSiteRecipe], left: &mut usize) -> R<Self> {
        pay(left, sites.len())?;
        if sites.iter().enumerate().all(|(ordinal, recipe)| recipe.id.0 as usize == ordinal) {
            return Ok(Self::Dense(sites));
        }
        let mut recipes = BTreeMap::new();
        for recipe in sites {
            pay(left, search(recipes.len()).saturating_add(
                std::mem::size_of::<(SemanticSiteId, &SemanticSiteRecipe)>(),
            ))?;
            // The original validation owns recipe uniqueness. Preserve its
            // source order here for sparse recipe coordinates as well.
            recipes.entry(recipe.id).or_insert(recipe);
        }
        Ok(Self::Sparse(recipes))
    }

    fn get(&self, id: SemanticSiteId, left: &mut usize) -> R<Option<&'a SemanticSiteRecipe>> {
        Ok(match self {
            Self::Dense(sites) => {
                pay(left, 1)?;
                sites.get(id.0 as usize)
            }
            Self::Sparse(sites) => {
                pay(left, search(sites.len()))?;
                sites.get(&id).copied()
            }
        })
    }
}

struct Lower<'a, 'b> {
    ledger: &'a Ledger,
    functions: BTreeMap<FunctionId, &'a TypedFunction>,
    closures: BTreeMap<ClosureId, &'a ClosureDefinition>,
    left: &'b mut usize,
    limits: &'b Limits,
}
impl<'a> Lower<'a, '_> {
    fn actual(&mut self, id: FunctionId) -> R<&'a TypedFunction> {
        pay(self.left, search(self.functions.len()))?;
        self.functions
            .get(&id)
            .copied()
            .ok_or_else(|| bad("emitter receipt has no actual Typed function"))
    }
    fn frame(&mut self, id: FunctionId) -> R<&'a Frame> {
        pay(self.left, search(self.ledger.frames.len()))?;
        self.ledger
            .frames
            .get(&id)
            .ok_or_else(|| bad("semantic target was not emitted with its actual Core owner"))
    }
    fn origin(target: &SemanticFunctionTarget) -> R<Origin> {
        Ok(match target {
            SemanticFunctionTarget::Core { module, function } => Origin {
                module: *module,
                creator: *function,
                closure: None,
            },
            SemanticFunctionTarget::CoreClosure {
                module,
                creator,
                expression,
                occurrence: 0,
            } => Origin {
                module: *module,
                creator: *creator,
                closure: Some(*expression),
            },
            SemanticFunctionTarget::CoreClosure { .. } => {
                return Err(bad("Core closure occurrence is not an actual mint"));
            }
            SemanticFunctionTarget::Typed(_) => {
                return Err(bad(
                    "Core target cannot claim a preselected Typed function identity",
                ));
            }
        })
    }
    fn target(&mut self, target: &SemanticFunctionTarget) -> R<FunctionId> {
        let origin = Self::origin(target)?;
        pay(self.left, search(self.ledger.targets.len()))?;
        let id = match self.ledger.targets.get(&origin).map(Vec::as_slice).unwrap_or_default() {
            [id] => *id,
            [] => return Err(bad("Core semantic target has no actual emitter binding")),
            _ => return Err(bad("one Core owner has ambiguous actual Typed targets")),
        };
        self.actual(id)?;
        if origin.closure.is_some() {
            self.closure_for_entry(id)?;
        }
        Ok(id)
    }
    fn matches_target(&mut self, target: &SemanticFunctionTarget, actual: FunctionId) -> R<bool> {
        let origin = Self::origin(target)?;
        self.actual(actual)?;
        Ok(self.frame(actual)?.origin == origin)
    }
    fn closure_for_entry(&mut self, id: FunctionId) -> R<ClosureId> {
        let frame = self.frame(id)?;
        let (creator, closure) = frame
            .parent
            .ok_or_else(|| bad("closure entry has no actual direct creator"))?;
        let closure = closure.ok_or_else(|| bad("thin function entry has no runtime closure environment"))?;
        let function = self.actual(id)?;
        pay(self.left, search(self.closures.len()))?;
        let actual = self.closures.get(&closure)
            .ok_or_else(|| bad("closure emitter binding has no actual environment definition"))?;
        if actual.function != id || function.environment != Some(closure) {
            return Err(bad(
                "closure definition and entry identities do not match the actual emitter",
            ));
        }
        self.actual(creator)?;
        Ok(closure)
    }
    fn closure(&mut self, target: &SemanticClosureTarget, creator: FunctionId) -> R<ClosureId> {
        match target {
            SemanticClosureTarget::Core {
                module,
                creator: core_creator,
                expression,
                occurrence,
            } => {
                let origin = Self::origin(&SemanticFunctionTarget::CoreClosure {
                    module: *module,
                    creator: *core_creator,
                    expression: *expression,
                    occurrence: *occurrence,
                })?;
                pay(self.left, search(self.ledger.targets.len()))?;
                let targets = self.ledger.targets.get(&origin).map(Vec::as_slice).unwrap_or_default();
                let mut found = None;
                for id in targets {
                    let frame = self.frame(*id)?;
                    if frame.parent.is_some_and(|(parent, _)| parent == creator) && found.replace(*id).is_some() {
                        return Err(bad("one actual creator has ambiguous closure entries"));
                    }
                }
                self.closure_for_entry(
                    found.ok_or_else(|| bad("closure has no actual entry in its exact creator"))?,
                )
            }
            SemanticClosureTarget::Typed(_) => Err(bad(
                "Core closure target cannot claim a preselected environment identity",
            )),
        }
    }
    fn instruction(&mut self, function: FunctionId, point: Point) -> R<&'a Instruction> {
        pay(self.left, 2)?;
        self.actual(function)?
            .blocks
            .get(point.block.0 as usize)
            .and_then(|block| block.instructions.get(point.instruction))
            .ok_or_else(|| bad("semantic emitter point is outside the actual CFG"))
    }
    fn expression(
        &mut self,
        function: FunctionId,
        node: NodeId,
    ) -> R<&'a super::emission::Expression> {
        let frame = self.frame(function)?;
        pay(self.left, search(frame.expressions.len()))?;
        frame.expressions.get(&node).ok_or_else(|| {
            bad(&format!(
                "semantic source expression {node:?} in {function:?} was not actually emitted"
            ))
        })
    }
    // A captureless source closure can emit a real FunctionRef. Its lexical
    // parent and expression still identify the child, even without a runtime
    // environment. A pointer of the same signature cannot stand in for it.
    fn thin_entry(&mut self, parent: FunctionId, expression: NodeId) -> R<Option<FunctionId>> {
        let receipt = self.expression(parent, expression)?;
        pay(self.left, receipt.instructions.len())?;
        let mut child = None;
        for point in &receipt.instructions {
            let op = self.instruction(parent, *point)?;
            let InstructionKind::FunctionRef { function } = op.kind else { continue; };
            if op.result.is_none() || op.result != receipt.value { continue; }
            let frame = self.frame(function)?;
            if frame.parent != Some((parent, None))
                || frame.origin.closure != Some(expression) || self.actual(function)?.environment.is_some()
                || child.replace(function).is_some()
            {
                return Err(bad("thin function reference changes its original creator, expression or entry"));
            }
        }
        Ok(child)
    }
    fn operand(&mut self, function: FunctionId, operand: &mut SemanticOperand) -> R<()> {
        pay(self.left, 1)?;
        match operand {
            SemanticOperand::CoreExpression {
                expression,
                occurrence: 0,
            } => {
                let row = self.expression(function, *expression)?;
                *operand = if let Some(value) = row.value {
                    SemanticOperand::Value(value)
                } else {
                    row.place.clone().ok_or_else(|| {
                        bad("semantic operand expression has no actual value or place")
                    })?
                };
                Ok(())
            }
            SemanticOperand::CoreExpression { .. } => Err(bad(
                "semantic operand occurrence is not an actual Core mint",
            )),
            // These are lexical selectors, not asserted compiler SSA numbers.
            SemanticOperand::Parameter(_)
            | SemanticOperand::Capture(_)
            | SemanticOperand::Receiver
            | SemanticOperand::TypeArgument { .. }
            | SemanticOperand::Environment(_) => Ok(()),
            SemanticOperand::Value(_) | SemanticOperand::Local(_) => Err(bad(
                "Core semantic operand cannot select an invented Typed SSA or local",
            )),
        }
    }
    fn point_for(
        &mut self,
        function: FunctionId,
        node: NodeId,
        action: &mut SemanticSiteAction,
        intrinsic: Option<&super::original::IntrinsicKind>,
    ) -> R<Point> {
        let row = self.expression(function, node)?;
        if matches!(action, SemanticSiteAction::Construct { .. }) {
            let construction = row
                .construction
                .ok_or_else(|| bad("literal constructor has no actual producer receipt"))?;
            let instruction = self.instruction(function, construction.at)?;
            if instruction.result != Some(construction.value)
                || !matches!(
                    instruction.kind,
                    InstructionKind::Constant(Constant::Unit)
                        | InstructionKind::MakeVariant { .. }
                        | InstructionKind::MakeErrorUnion { .. }
                        | InstructionKind::MakeAggregate { .. }
                        | InstructionKind::MakeManaged { .. }
                        | InstructionKind::MakePackage { .. }
                        | InstructionKind::MakeOwnedTuple { .. }
                        | InstructionKind::MakeOwnedArray { .. }
                )
            {
                return Err(bad(
                    "literal constructor receipt is not its actual creation opcode/result",
                ));
            }
            return Ok(construction.at);
        }
        let mut found = None;
        pay(self.left, row.instructions.len())?;
        for point in &row.instructions {
            let instruction = self.instruction(function, *point)?;
            let matches = match action {
                SemanticSiteAction::Construct { .. } => {
                    instruction.result.is_some() && instruction.result == row.value
                }
                SemanticSiteAction::Call { target, .. } => match &instruction.kind {
                    InstructionKind::Call {
                        function: actual, ..
                    }
                    | InstructionKind::SuspendCall {
                        function: actual, ..
                    }
                    | InstructionKind::ForeignCall {
                        function: actual, ..
                    } => self.matches_target(target, *actual)?,
                    _ => false,
                },
                SemanticSiteAction::CoreBodyCall => {
                    if let Some(intrinsic) = intrinsic {
                        super::world::intrinsic_operation_matches(
                            intrinsic,
                            &instruction.kind,
                            self.left,
                        )?
                    } else {
                        matches!(
                            instruction.kind,
                            InstructionKind::Call { .. }
                                | InstructionKind::ForeignCall { .. }
                                | InstructionKind::SuspendCall { .. }
                                | InstructionKind::CallIndirect { .. }
                                | InstructionKind::CallClosure { .. }
                                | InstructionKind::CallClosureOnce { .. }
                                | InstructionKind::CallClosureShared { .. }
                                | InstructionKind::CallClosureMut { .. }
                                | InstructionKind::CallClosureBorrowed { .. }
                        )
                    }
                }
                SemanticSiteAction::BodyCall { .. } => {
                    return Err(bad("Core input claims already materialized body call"));
                }
                SemanticSiteAction::CallableCall { .. } => matches!(
                    instruction.kind,
                    InstructionKind::CallIndirect { .. }
                        | InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureOnce { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. }
                ),
                SemanticSiteAction::TypeUse { .. } => {
                    instruction.result.is_some() && instruction.result == row.value
                }
                SemanticSiteAction::Closure { closure, .. } => matches!(&instruction.kind,
                    InstructionKind::MakeClosure { closure: actual, .. } if self.closure(closure, function)? == *actual),
                SemanticSiteAction::DeclareLocal { .. } => {
                    return Err(bad(
                        "binding must relocate from its actual declaration receipt",
                    ));
                }
            };
            if matches && found.replace(*point).is_some() {
                return Err(bad("semantic site matches multiple actual operations"));
            }
        }
        found.ok_or_else(|| bad("semantic site does not name its actual emitted operation"))
    }
    fn site(
        &mut self,
        function: FunctionId,
        site: &mut SemanticSite,
        intrinsic: Option<&super::original::IntrinsicKind>,
    ) -> R<()> {
        let SemanticSiteAddress::Core {
            expression,
            occurrence: 0,
            role,
        } = site.at
        else {
            return Err(bad(
                "Core semantic site does not name one real Core mint and role",
            ));
        };
        let expected = match site.action {
            SemanticSiteAction::Construct { .. } => SemanticEmissionRole::Construct,
            SemanticSiteAction::Call { .. }
            | SemanticSiteAction::CallableCall { .. }
            | SemanticSiteAction::CoreBodyCall => SemanticEmissionRole::Call,
            SemanticSiteAction::TypeUse { .. } => SemanticEmissionRole::Ascription,
            SemanticSiteAction::DeclareLocal { .. } => SemanticEmissionRole::Binding,
            SemanticSiteAction::Closure { .. } => SemanticEmissionRole::Closure,
            SemanticSiteAction::BodyCall { .. } => {
                return Err(bad("Core site asserts resolved body call"));
            }
        };
        if role != expected {
            return Err(bad(
                "semantic site action disagrees with its actual emission role",
            ));
        }
        let edge = if matches!(site.action, SemanticSiteAction::TypeUse { .. }) {
            SemanticSiteEdge::Before
        } else {
            SemanticSiteEdge::After
        };
        let point = if matches!(site.action, SemanticSiteAction::TypeUse { .. }) {
            self.expression(function, expression)?
                .end
                .ok_or_else(|| bad("type-use has no actual emitter boundary"))?
        } else if let SemanticSiteAction::DeclareLocal { local, .. } = &mut site.action {
            let SemanticLocalTarget::Core {
                statement,
                occurrence: 0,
            } = local
            else {
                return Err(bad("Core binding has no exact statement identity"));
            };
            if *statement != expression {
                return Err(bad("binding source token and statement target disagree"));
            }
            let frame = self.frame(function)?;
            pay(self.left, search(frame.declarations.len()))?;
            let declaration = frame
                .declarations
                .get(statement)
                .ok_or_else(|| bad("semantic binding did not declare an actual local"))?;
            *local = SemanticLocalTarget::Typed(declaration.local);
            declaration.at
        } else {
            self.point_for(function, expression, &mut site.action, intrinsic)?
        };
        match &mut site.action {
            SemanticSiteAction::Construct { result, .. }
            | SemanticSiteAction::Closure { result, .. } => {
                if *result
                    != (SemanticOperand::CoreExpression {
                        expression,
                        occurrence: 0,
                    })
                {
                    return Err(bad(
                        "constructor result does not select its exact original Core occurrence",
                    ));
                }
                let actual = self
                    .instruction(function, point)?
                    .result
                    .ok_or_else(|| bad("actual constructor has no result"))?;
                *result = SemanticOperand::Value(actual);
            }
            SemanticSiteAction::TypeUse { subject, .. } => {
                self.operand(function, subject)?;
                let row = self.expression(function, expression)?;
                let actual = row
                    .value
                    .map(SemanticOperand::Value)
                    .or_else(|| row.place.clone())
                    .ok_or_else(|| bad("type-use has no actual expression result or place"))?;
                if *subject != actual {
                    return Err(bad(
                        "type-use subject differs from its actual emitted expression",
                    ));
                }
            }
            SemanticSiteAction::CallableCall { callee } => {
                let row = self.expression(function, expression)?;
                let Some(source) = row.callee else {
                    return Err(bad("callable site is not an actual Core call"));
                };
                if *callee
                    != (SemanticOperand::CoreExpression {
                        expression: source,
                        occurrence: 0,
                    })
                {
                    return Err(bad(
                        "callable callee does not select the actual source call operand",
                    ));
                }
                *callee = match self.instruction(function, point)?.kind {
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        SemanticOperand::Local(callee)
                    }
                    InstructionKind::CallIndirect { callee, .. }
                    | InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::CallClosureBorrowed { callee, .. } => {
                        SemanticOperand::Value(callee)
                    }
                    _ => return Err(bad("callable site has another actual opcode")),
                };
            }
            SemanticSiteAction::Call { .. }
            | SemanticSiteAction::DeclareLocal { .. }
            | SemanticSiteAction::CoreBodyCall => {}
            SemanticSiteAction::BodyCall { .. } => {
                return Err(bad("Core site asserts resolved body call"));
            }
        }
        if let SemanticSiteAction::Call { target, .. } = &mut site.action {
            let actual = match self.instruction(function, point)?.kind {
                InstructionKind::Call { function, .. }
                | InstructionKind::SuspendCall { function, .. }
                | InstructionKind::ForeignCall { function, .. } => function,
                _ => return Err(bad("static call receipt is not its actual direct call")),
            };
            if !self.matches_target(target, actual)? {
                return Err(bad("static call changes its exact actual Core origin"));
            }
            *target = SemanticFunctionTarget::Typed(actual);
        }
        if let SemanticSiteAction::Closure { closure, .. } = &mut site.action {
            *closure = SemanticClosureTarget::Typed(self.closure(closure, function)?);
        }
        site.at = SemanticSiteAddress::Typed {
            block: point.block,
            instruction: u32::try_from(point.instruction)
                .map_err(|_| bad("actual instruction ordinal overflows"))?,
            edge,
        };
        Ok(())
    }
}
fn recipe_kind(recipe: &SemanticSiteRecipeKind, action: &SemanticSiteAction) -> bool {
    matches!(
        (recipe, action),
        (
            SemanticSiteRecipeKind::OriginalCall,
            SemanticSiteAction::CoreBodyCall
        ) | (
            SemanticSiteRecipeKind::Construct { .. },
            SemanticSiteAction::Construct { .. }
        ) | (
            SemanticSiteRecipeKind::Call { .. },
            SemanticSiteAction::Call { .. }
        ) | (
            SemanticSiteRecipeKind::CallableCall { .. },
            SemanticSiteAction::CallableCall { .. }
        ) | (
            SemanticSiteRecipeKind::TypeUse { .. },
            SemanticSiteAction::TypeUse { .. }
        ) | (
            SemanticSiteRecipeKind::DeclareLocal { .. },
            SemanticSiteAction::DeclareLocal { .. }
        ) | (
            SemanticSiteRecipeKind::Closure { .. },
            SemanticSiteAction::Closure { .. }
        )
    )
}
/// The returned program is still untrusted: original body recipes, every
/// resolved type/const/class, header embedding, W and partitions must also be
/// checked. No caller may publish it on relocation alone.
pub(super) fn program(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    pay(
        left,
        module.functions.len().saturating_mul(2).saturating_add(1),
    )?;
    let mut functions = BTreeMap::new();
    for function in &module.functions {
        pay(left, search(functions.len()))?;
        if functions.insert(function.id, function).is_some() {
            return Err(bad("actual Typed function identity repeats"));
        }
    }
    let mut closures = BTreeMap::new();
    for definition in &module.closures {
        pay(left, search(closures.len()))?;
        if closures.insert(definition.id, definition).is_some() {
            return Err(bad("actual closure definition identity repeats"));
        }
    }
    let mut lower = Lower {
        ledger,
        functions,
        closures,
        left,
        limits,
    };
    let targets = instances::publish(program, &mut lower)?;
    pay(
        lower.left,
        program
            .definitions
            .len()
            .saturating_mul(2)
            .saturating_add(program.instances.len().saturating_mul(2)),
    )?;
    let mut originals = BTreeMap::new();
    for definition in &program.definitions {
        let recipes = match &definition.body {
            SemanticDefinitionBody::Function { sites, .. }
            | SemanticDefinitionBody::FunctionWithBody { sites, .. } =>
                Some(OriginalRecipes::new(sites, lower.left)?),
            _ => None,
        };
        pay(lower.left, search(originals.len()))?;
        if originals.insert(definition.id, (definition, recipes)).is_some() {
            return Err(bad("original semantic definition identity repeats"));
        }
    }
    let mut owners = BTreeMap::new();
    let mut function_worlds = BTreeMap::<(SemanticDefinitionId, FunctionId), Vec<&SemanticInstance>>::new();
    for instance in &program.instances {
        pay(lower.left, search(owners.len()))?;
        if owners.insert(instance.id, instance).is_some() {
            return Err(bad("actual semantic instance identity repeats"));
        }
        if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(target)) = instance.target {
            pay(lower.left, search(function_worlds.len()).saturating_add(1))?;
            function_worlds.entry((instance.definition, target)).or_default().push(instance);
        }
    }
    pay(lower.left, program.sites.len().saturating_mul(2))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut owner_observations = Vec::new();
    let has_dyn_observation = if let Some(proof) = ledger.original.borrow().as_ref() {
        pay(lower.left, proof.intrinsics.len())?;
        proof.intrinsics.values().any(|call| matches!(call.kind, super::original::IntrinsicKind::DynCall { .. }))
    } else {
        false
    };
    // Only synthesized Dyn receiver observations query original sites that
    // have not yet been visited. Other operations use the live seen set.
    let actual_sites = if has_dyn_observation {
        pay(lower.left, program.sites.len()
            .saturating_mul(search(program.sites.len()).saturating_add(1)))?;
        Some(program.sites.iter().map(|site| (site.instance, site.recipe))
            .collect::<std::collections::BTreeSet<_>>())
    } else {
        None
    };
    for site in &mut program.sites {
        pay(
            lower.left,
            search(seen.len())
                .saturating_add(search(owners.len()))
                .saturating_add(search(originals.len())),
        )?;
        if !seen.insert((site.instance, site.recipe)) {
            return Err(bad(
                "original semantic site was consumed twice in one actual instance",
            ));
        }
        let owner = owners
            .get(&site.instance)
            .ok_or_else(|| bad("semantic site has no actual original instance"))?;
        let (definition, recipes) = originals
            .get(&owner.definition)
            .ok_or_else(|| bad("semantic site lost its original definition"))?;
        let recipes = recipes.as_ref().ok_or_else(|| bad(
                "executable semantic site belongs to a nonfunction original scope",
        ))?;
        let original = recipes.get(site.recipe, lower.left)?
            .ok_or_else(|| bad("actual semantic site is absent from its original body"))?;
        if !recipe_kind(&original.kind, &site.action) {
            return Err(bad(
                "actual semantic action changes its original operation kind",
            ));
        }

        pay(lower.left, search(targets.len()))?;
        let function = *targets
            .get(&site.instance)
            .ok_or_else(|| bad("semantic executable site has no actual function instance"))?;
        // A sealed intrinsic is selected only by this checked original
        // definition/site pair. The whole scheme and actual argument/result
        // domains are still checked by World after relocation.
        let proof = ledger.original.borrow();
        let intrinsic = if let Some(proof) = proof.as_ref() {
            pay(lower.left, search(proof.intrinsics.len()))?;
            proof
                .intrinsics
                .get(&(definition.id, site.recipe))
                .map(|call| &call.kind)
        } else {
            None
        };
        if let Some(super::original::IntrinsicKind::DynCall { receiver_node, .. }) = intrinsic
            && let SemanticSiteAddress::Core {
                expression: call,
                occurrence: 0,
                role: SemanticEmissionRole::Call,
            } = site.at
            && let Some(receiver) = lower.expression(function, call)?.dyn_receiver
        {
            let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body
            else {
                return Err(bad(
                    "dynamic owner observation requires its complete original body",
                ));
            };
            pay(lower.left, site_nodes.len())?;
            let mut selected = None;
            for node in site_nodes
                .iter()
                .filter(|node| node.node == SemanticOriginalNode::Expression(*receiver_node))
            {
                let recipe = recipes.get(node.site, lower.left)?
                    .ok_or_else(|| bad("dynamic owner observation lost its original recipe"))?;
                let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                    continue;
                };
                if selected.replace((recipe.id, expected)).is_some() {
                    return Err(bad(
                        "dynamic owner place selects multiple original observation recipes",
                    ));
                }
            }
            let (recipe, expected) = selected.ok_or_else(|| {
                bad("dynamic owner place has no exact original observation recipe")
            })?;
            let actual_sites = actual_sites.as_ref()
                .ok_or_else(|| bad("dynamic owner observation has no actual site directory"))?;
            pay(lower.left, search(actual_sites.len()))?;
            if !actual_sites.contains(&(site.instance, recipe)) {
                crate::dyn_metadata::semantic_operand_snapshot(expected, limits, lower.left, 1)?;
                let mut observation = SemanticSite {
                    instance: site.instance,
                    recipe,
                    at: SemanticSiteAddress::Core {
                        expression: receiver,
                        occurrence: 0,
                        role: SemanticEmissionRole::Ascription,
                    },
                    action: SemanticSiteAction::TypeUse {
                        subject: SemanticOperand::CoreExpression {
                            expression: receiver,
                            occurrence: 0,
                        },
                        expected: SemanticTypeValue::Core {
                            scope: site.instance,
                            source: SemanticCoreTypeSource::OriginalRecipe(Box::new(
                                expected.clone(),
                            )),
                        },
                    },
                };
                lower.site(function, &mut observation, None)?;
                if !seen.insert((observation.instance, observation.recipe)) {
                    return Err(bad(
                        "dynamic owner observation repeats one actual original operation",
                    ));
                }
                pay(lower.left, 1)?;
                owner_observations.push(observation);
            }
        }
        if let (
            SemanticSiteRecipeKind::Closure { definition: expected, environment: declared, .. },
            SemanticSiteAction::Closure { result, environment, .. },
            SemanticSiteAddress::Core { expression, occurrence: 0, role: SemanticEmissionRole::Closure },
        ) = (&original.kind, &site.action, &site.at)
            && let Some(entry) = lower.thin_entry(function, *expression)?
        {
            let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
                return Err(bad("thin function reference lost its original expression"));
            };
            pay(lower.left, site_nodes.len().saturating_add(search(function_worlds.len())))?;
            let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
            let Some(SemanticOriginalNode::Expression(node)) = nodes.next().map(|row| &row.node) else {
                return Err(bad("thin function reference has no original expression"));
            };
            let proof = proof.as_ref().ok_or_else(|| bad("thin function reference lacks its checked source body"))?;
            pay(lower.left, search(proof.function_values.len()))?;
            let children = function_worlds.get(&(*expected, entry)).map(Vec::as_slice).unwrap_or_default();
            pay(lower.left, children.len())?;
            if nodes.next().is_some() || !declared.is_empty() || !environment.is_empty()
                || *result != (SemanticOperand::CoreExpression { expression: *expression, occurrence: 0 })
                || !proof.function_values.get(&(definition.id, *node)).is_some_and(|value| value.target == *expected)
                || !children.iter().any(|child| child.parent == Some(site.instance) && child.environment.is_none())
            {
                return Err(bad("thin function reference changes its checked zero-capture child or creator"));
            }
            let expression = *expression;
            site.at = SemanticSiteAddress::Core { expression, occurrence: 0, role: SemanticEmissionRole::Ascription };
            site.action = SemanticSiteAction::TypeUse {
                subject: SemanticOperand::CoreExpression { expression, occurrence: 0 },
                expected: SemanticTypeValue::Core { scope: site.instance, source: SemanticCoreTypeSource::OriginalRecipe(
                    Box::new(SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Expression(*node) })) },
            };
        }
        lower.site(function, site, intrinsic)?;
        match (&original.kind, &site.action) {
            (
                SemanticSiteRecipeKind::Call {
                    target: expected, ..
                },
                SemanticSiteAction::Call {
                    target: SemanticFunctionTarget::Typed(actual),
                    ..
                },
            ) => {
                pay(lower.left, search(function_worlds.len()))?;
                if !function_worlds.contains_key(&(*expected, *actual)) {
                    return Err(bad("actual static call changes its original definition"));
                }
            }
            (
                SemanticSiteRecipeKind::Closure {
                    definition: expected,
                    ..
                },
                SemanticSiteAction::Closure {
                    closure: SemanticClosureTarget::Typed(actual),
                    ..
                },
            ) => {
                pay(lower.left, search(lower.closures.len()))?;
                let entry = lower.closures.get(actual)
                    .ok_or_else(|| bad("actual closure site has no environment definition"))?
                    .function;
                pay(lower.left, search(function_worlds.len()))?;
                let children = function_worlds.get(&(*expected, entry)).map(Vec::as_slice).unwrap_or_default();
                pay(lower.left, children.len())?;
                if !children.iter().any(|child| child.parent == Some(site.instance)) {
                    return Err(bad(
                        "closure site changes its original definition or actual lexical parent",
                    ));
                }
            }
            _ => {}
        }
    }
    if program.sites.len().saturating_add(owner_observations.len()) > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "actual original owner observation inventory exceeded",
            None,
        ));
    }
    program.sites.extend(owner_observations);
    // Original sites include type-stage operations and unused template bodies.
    // They remain immutable and universally checked. Core mint events and
    // actual owner-place events attached to their original dynamic calls are
    // relocated only from real emitter receipts. No whole-value read is made
    // to observe an owner place; instances::publish covers every real frame.
    Ok(())
}
