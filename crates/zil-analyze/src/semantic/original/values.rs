//! Instantiate an already independently checked original term. This never
//! feeds executable class coincidences back into the rigid original solver.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};

impl Checked {
    fn context<'a>(
        &self,
        owner: SemanticDefinitionId,
        mut context: &'a SemanticInstance,
        inventory: &Inventory<'a>,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticInstance> {
        for _ in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            if context.definition == owner {
                return Ok(context);
            }
            context = inventory.instance(
                context.parent.ok_or_else(|| {
                    fail("original semantic term leaves its actual lexical context")
                })?,
                b,
            )?;
        }
        Err(resource(
            "original actual lexical context exceeds its finite depth",
        ))
    }
    fn life_value(
        &self,
        term: Term,
        context: &SemanticInstance,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        match &self.arena[self.root(term, b)?] {
            Kind::Life(Life::Formal(owner, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                value::class(
                    scope,
                    &SemanticClassExpr::Formal {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: *ordinal,
                    },
                    inventory,
                    b,
                    0,
                )
            }
            Kind::Life(Life::Implicit(owner, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                value::class(
                    scope,
                    &SemanticClassExpr::Implicit {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: *ordinal,
                    },
                    inventory,
                    b,
                    0,
                )
            }
            Kind::Life(Life::Invocation(owner, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                Ok(Class::Invocation(scope.id, *ordinal))
            }
            _ => Err(unknown(
                "original lifetime instantiation is unresolved independently of actual caller images",
            )),
        }
    }
    pub(super) fn argument(
        &self,
        term: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        b.spend(1, WORK)?;
        match &self.arena[self.root(term, b)?] {
            Kind::Life(_) | Kind::LifeVariable(_) => Ok(Argument::Life(
                self.life_value(term, context, inventory, b)?,
            )),
            Kind::ConstValue(constant) => Ok(Argument::Const {
                carrier: constant.ty(),
                value: constant.clone(),
            }),
            Kind::ConstRigid {
                owner,
                ordinal,
                carrier,
            } => {
                let scope = self.context(*owner, context, inventory, b)?;
                let Some(SemanticActual::Const {
                    ty: SemanticTypeValue::Resolved { ty, .. },
                    value,
                }) = scope.actuals.get(*ordinal as usize)
                else {
                    return Err(fail(
                        "original constant lacks its complete actual carrier and value",
                    ));
                };
                crate::dyn_metadata::ty(ty, b)?;
                if self.materialize(*carrier, context, program, inventory, b, depth + 1)? != *ty
                    || value.ty() != *ty
                {
                    return Err(fail(
                        "original constant carrier differs from its actual full ordinal",
                    ));
                }
                Ok(Argument::Const {
                    carrier: ty.clone(),
                    value: value.clone(),
                })
            }
            _ => Ok(Argument::Type(self.value(
                term,
                context,
                program,
                inventory,
                b,
                depth + 1,
            )?)),
        }
    }
    pub(super) fn value(
        &self,
        term: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original complete value instantiation exceeds its depth",
            ));
        }
        match &self.arena[self.root(term, b)?] {
            Kind::Atom(ty) => Ok(value::scalar(ty.clone())),
            Kind::CallResult {
                caller,
                node,
                hidden,
            } => self.call_result(
                *caller,
                *node,
                *hidden,
                context,
                program,
                inventory,
                b,
                depth + 1,
            ),
            Kind::Rigid(owner, ordinal) => {
                let scope = self.context(*owner, context, inventory, b)?;
                let Some(SemanticActual::Type(actual)) = scope.actuals.get(*ordinal as usize)
                else {
                    return Err(fail("original Type formal has no complete actual"));
                };
                value::resolved(scope, actual, inventory, b, depth + 1)
            }
            Kind::Address {
                mutable,
                life,
                inner,
            } => {
                let inner = self.value(*inner, context, program, inventory, b, depth + 1)?;
                let class = self.life_value(*life, context, inventory, b)?;
                value::address(*mutable, class, inner, b)
            }
            Kind::Nominal {
                definition,
                arguments,
                fields,
            } => {
                let mut actuals = Vec::new();
                let mut physical = Vec::new();
                for term in arguments {
                    let actual = self.argument(*term, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut actuals, actual, b)?;
                }
                for term in fields {
                    let field = self.value(*term, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut physical, field, b)?;
                }
                value::nominal(*definition, &actuals, &physical, inventory, b, depth + 1)
            }
            Kind::Tuple(fields) => {
                let mut types = Vec::new();
                for field in fields {
                    let field = self.value(*field, context, program, inventory, b, depth + 1)?;
                    if !field.classes.is_empty() {
                        return Err(unknown(
                            "original tuple needs its complete child class embedding",
                        ));
                    }
                    Solver::push(&mut types, field.ty, b)?;
                }
                Ok(value::scalar(Type::Tuple(types)))
            }
            Kind::Closure {
                definition,
                environment,
                ..
            } => {
                let ty = self.materialize(term, context, program, inventory, b, depth + 1)?;
                let Type::Closure(id) = ty else {
                    return Err(fail("original closure materialization has another type"));
                };
                let entry = inventory
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|entry| entry.id == id)
                    .ok_or_else(|| fail("original closure has no actual inventory"))?;
                let count = entry
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("original closure has no complete intrinsic scope"))?
                    .fixed_binder_count;
                let mut classes = Vec::new();
                for environment in environment {
                    let value =
                        self.value(*environment, context, program, inventory, b, depth + 1)?;
                    for class in value.classes {
                        value::intern(&mut classes, class, b)?;
                    }
                }
                if classes.len() < count as usize {
                    let child = program
                        .instances
                        .iter()
                        .find(|instance| {
                            instance.definition == *definition
                                && instance.environment.as_ref().is_some_and(|environment| {
                                    environment.closure == SemanticClosureTarget::Typed(id)
                                })
                        })
                        .ok_or_else(|| fail("original closure scope has no exact actual child"))?;
                    let source = inventory.definition(child.definition, b)?;
                    for binding in &child.header_embedding {
                        if matches!(binding.header, StorageBinder::Fixed(_)) {
                            if let SemanticClassExpr::Implicit {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            } = binding.actual
                            {
                                if !source
                                    .implicit_lifetimes
                                    .iter()
                                    .any(|row| row.ordinal == ordinal)
                                {
                                    return Err(fail(
                                        "closure head class has no original occurrence",
                                    ));
                                }
                                value::intern(&mut classes, Class::Implicit(child.id, ordinal), b)?;
                            }
                        }
                    }
                }
                if count as usize != classes.len() {
                    return Err(fail(
                        "actual closure scope omits original environment or phantom classes",
                    ));
                }
                Ok(Value {
                    ty: Type::Closure(id),
                    life: StorageLifeShape {
                        binder_count: count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    classes,
                })
            }
            _ => Err(unknown(
                "original complete type remains unresolved independently of executable images",
            )),
        }
    }
}

impl Checked {
    #[allow(clippy::too_many_arguments)]
    fn call_result(
        &self,
        caller: SemanticDefinitionId,
        node: NodeId,
        hidden: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let context = self.context(caller, context, inventory, b)?;
        let source = inventory.definition(caller, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            site_nodes, sites, ..
        } = &source.body
        else {
            return Err(fail("original call result has no complete caller body"));
        };
        let call = self
            .calls
            .get(&(caller, node))
            .ok_or_else(|| fail("original result role has no checked call and actual vector"))?;
        let mut recipe = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            b.spend(sites.len(), WORK)?;
            if link.node == SemanticOriginalNode::Expression(node)
                && sites.iter().any(|site| {
                    site.id == link.site
                        && matches!(
                            site.kind,
                            SemanticSiteRecipeKind::OriginalCall
                                | SemanticSiteRecipeKind::Call { .. }
                                | SemanticSiteRecipeKind::CallableCall { .. }
                        )
                })
            {
                if recipe.replace(link.site).is_some() {
                    return Err(fail("original call result has ambiguous role anchors"));
                }
            }
        }
        let recipe =
            recipe.ok_or_else(|| fail("original call result has no emitted role anchor"))?;
        let function = match &context.target {
            SemanticInstanceTarget::Function(target) => inventory.function(target, b)?,
            _ => return Err(fail("original call result has no real caller function")),
        };
        let mut actual = None;
        for site in &program.sites {
            b.spend(1, WORK)?;
            if site.instance == context.id && site.recipe == recipe {
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge: SemanticSiteEdge::After,
                } = site.at
                else {
                    return Err(fail(
                        "original call result does not name its actual after boundary",
                    ));
                };
                let instruction = function
                    .blocks
                    .get(block.0 as usize)
                    .and_then(|block| block.instructions.get(instruction as usize))
                    .ok_or_else(|| fail("original call role has no actual instruction"))?;
                if actual.replace(instruction).is_some() {
                    return Err(fail("original result role has multiple executable calls"));
                }
            }
        }
        let instruction = actual.ok_or_else(|| fail("original result role has no actual call"))?;
        if let InstructionKind::Call {
            function: target, ..
        }
        | InstructionKind::SuspendCall {
            function: target, ..
        }
        | InstructionKind::ForeignCall {
            function: target, ..
        } = instruction.kind
        {
            let mut selected = None;
            for instance in &program.instances {
                b.spend(1, WORK)?;
                if matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id==target)
                    && selected.replace(instance).is_some()
                {
                    return Err(fail(
                        "original call result has ambiguous actual target instances",
                    ));
                }
            }
            if selected.is_none_or(|instance| instance.definition != call.target) {
                return Err(fail("original result role selects another actual callee"));
            }
        } else if !call.closure
            || !matches!(
                instruction.kind,
                InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
            )
        {
            return Err(fail("original result role changed its actual call family"));
        }
        let result = self.value(hidden, context, program, inventory, b, depth + 1)?;
        let Some(value) = instruction.result else {
            if result.ty == Type::Unit && result.classes.is_empty() {
                return Ok(result);
            }
            return Err(fail("original class-bearing call has no actual result SSA"));
        };
        let ty = function
            .values
            .get(value.0 as usize)
            .ok_or_else(|| fail("original result role has no complete actual SSA type"))?;
        crate::dyn_metadata::ty(ty, b)?;
        if *ty != result.ty {
            return Err(fail(
                "actual call result differs from the complete original result role",
            ));
        }
        Ok(result)
    }
}
