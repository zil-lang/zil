//! Exact constructor-world bridge. Metadata supplies a domain cursor only;
//! the ordinary worker, resource and phase passes discharge execution debts.
use super::*;
use crate::semantic::task_domains::{self, FrameKey, ObligationState};
use original::intrinsics::Use;

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_task_operation(
        &self,
        module: &TypedModule,
        caller: &SemanticInstance,
        f: &TypedFunction,
        instruction: &Instruction,
        site: &SemanticSite,
        use_: Use,
        values: &[Option<CheckedType>],
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        let constructor = match &instruction.kind {
            InstructionKind::TaskCreate {
                worker, arguments, ..
            } => Some((*worker, arguments, false)),
            InstructionKind::TaskCreateOwned {
                worker, arguments, ..
            } => Some((*worker, arguments, true)),
            _ => None,
        };
        let mut selected = Vec::<FrameKey>::new();
        if let Some((worker, arguments, _)) = constructor {
            let source = use_
                .worker
                .ok_or_else(|| bad("Task constructor loses its exact original worker"))?;
            let definition = self.definition(module, source, b)?;
            if !definition.environment.is_empty() {
                return Err(bad(
                    "static Task worker cannot manufacture a sealed closure environment",
                ));
            }
            let (proved_source, recipes) = self
                .original
                .borrow()
                .as_ref()
                .ok_or_else(|| bad("Task worker lacks its universal original proof"))?
                .call_recipes(module, caller.definition, site.recipe, environment, b)?;
            if proved_source != source || recipes.len() != definition.parameters.len() {
                return Err(bad(
                    "Task worker source/full-ordinal substitution changes at its actual boundary",
                ));
            }
            let mut checked = Vec::new();
            for recipe in &recipes {
                checked.push(self.evaluate_actual(
                    module,
                    recipe,
                    environment,
                    external,
                    active,
                    b,
                )?);
            }
            let mut lexical = if let Some(parent) = definition.parent {
                let index = environment
                    .lexical
                    .iter()
                    .position(|frame| frame.definition == parent)
                    .ok_or_else(|| bad("Task worker has another original lexical parent"))?;
                environment.lexical[index..].to_vec()
            } else {
                vec![]
            };
            lexical.insert(
                0,
                LexicalFrame {
                    definition: source,
                    key: ScopeKey::Witness {
                        definition: source,
                        frame: active.fresh(b)?,
                    },
                    actuals: checked,
                    environment: vec![],
                    self_type: None,
                },
            );
            let expected = EvalEnv {
                lexical,
                assumptions: environment.assumptions.clone(),
                images: environment.images.clone(),
                bounds: environment.bounds.clone(),
                invocations: vec![],
                at: environment.at,
            };
            self.call_worlds(module, worker, source, &expected, environment, active, b)?;
            self.evaluate_requirements(
                module,
                &definition.requirements,
                &expected,
                RequirementMode::ActualObligation,
                external,
                active,
                b,
            )?;
            for formal in &definition.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    self.evaluate_requirements(
                        module,
                        requirements,
                        &expected,
                        RequirementMode::ActualObligation,
                        external,
                        active,
                        b,
                    )?;
                }
            }
            let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
            else {
                return Err(bad("Task worker loses its complete original signature"));
            };
            let signature =
                self.evaluate_signature(module, signature, &expected, external, active, b)?;
            if signature.parameters.len() != arguments.len() {
                return Err(bad(
                    "Task worker full source signature changes its operand count",
                ));
            }
            let mut boundary = trait_members::Boundary::default();
            for ((parameter, _), argument) in signature.parameters.iter().zip(arguments) {
                boundary.value(
                    self,
                    environment,
                    parameter,
                    value_at(values, *argument)?,
                    b,
                )?;
            }

            for frame in task_domains::metadata_candidates(worker, b)? {
                if frame.definition != Some(source) {
                    continue;
                }
                let instance = frame
                    .key
                    .instance
                    .ok_or_else(|| bad("semantic Task constructor selects a raw worker frame"))?;
                let actual = self.environment_for_instance(module, instance, active, b)?;
                if actual.lexical.len()!=expected.lexical.len()||actual.lexical.iter().skip(1).zip(expected.lexical.iter().skip(1)).any(|(a,z)|a.definition!=z.definition||matches!((a.key,z.key),(ScopeKey::Instance(a),ScopeKey::Instance(z))if a!=z)){continue;}
                if actual.lexical.iter().zip(&expected.lexical).any(|(a, z)| {
                    a.actuals.len() != z.actuals.len()
                        || a.environment.len() != z.environment.len()
                        || !a
                            .actuals
                            .iter()
                            .zip(&z.actuals)
                            .chain(a.environment.iter().zip(&z.environment))
                            .all(|(a, z)| task_actual_shape(a, z))
                }) {
                    continue;
                }
                let mut world_boundary = trait_members::Boundary::default();
                for (a, z) in actual.lexical.iter().zip(&expected.lexical) {
                    for (a, z) in a
                        .actuals
                        .iter()
                        .zip(&z.actuals)
                        .chain(a.environment.iter().zip(&z.environment))
                    {
                        world_boundary.actual(self, environment, a, z, b)?;
                    }
                    match (&a.self_type, &z.self_type) {
                        (Some(a), Some(z)) => world_boundary.value(self, environment, a, z, b)?,
                        (None, None) => {}
                        _ => {
                            return Err(bad("Task worker changes its complete lexical Self domain"));
                        }
                    }
                }
                if frame.parameters.len() != arguments.len() {
                    return Err(bad(
                        "Task constructor changes the complete worker operand list",
                    ));
                }
                let mut boundary = trait_members::Boundary::default();
                for (parameter, argument) in frame.parameters.iter().zip(arguments) {
                    boundary.value(
                        self,
                        environment,
                        parameter,
                        value_at(values, *argument)?,
                        b,
                    )?;
                }
                selected.push(frame.key);
            }
            if selected.is_empty() {
                return Err(bad("Task constructor has no exact compatible worker frame"));
            }
        }
        let raw = crate::tasks::instruction(module, f, instruction, b)?
            .ok_or_else(|| bad("Task operation has no independent result type"))?;
        if let Some(source) = use_.buffer {
            let head = match instruction.kind {
                InstructionKind::TaskValid { task }
                | InstructionKind::TaskPoll { task }
                | InstructionKind::TaskRequestCancel { task }
                | InstructionKind::TaskPollClose { task }
                | InstructionKind::TaskFinishDestroy { task } => task,
                _ => return Err(bad("Task source claim has another opcode")),
            };
            value_at(values, head)?;
            self.intrinsic_buffer_source(module, caller, f, head, source, b)?;
        }
        let caller_definition = self.definition(module, caller.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &caller_definition.body
        else {
            return Err(bad("Task operation has no complete original body"));
        };
        b.spend(site_nodes.len())?;
        let nodes = site_nodes
            .iter()
            .filter(|row| row.site == site.recipe)
            .filter_map(|row| {
                if let SemanticOriginalNode::Expression(node) = row.node {
                    Some(node)
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        if nodes.len() != 1 {
            return Err(bad(
                "Task operation has no unique exact original expression",
            ));
        }
        let source = SemanticOriginalTypeSource::Expression(*nodes.iter().next().unwrap());
        let recipe = self
            .original
            .borrow()
            .as_ref()
            .ok_or_else(|| bad("Task operation loses its original complete result proof"))?
            .type_recipe(module, caller.definition, &source, environment, b)?;
        let value = self.evaluate_type(module, &recipe, environment, external, active, b)?;
        if value.ty != raw {
            return Err(bad(
                "Task actual result differs from its complete original type/domain",
            ));
        }
        if let Some((worker, _, owned)) = constructor {
            let result = instruction
                .result
                .ok_or_else(|| bad("Task constructor has no SSA result"))?;
            b.spend(b.task_constructors.len())?;
            let keys = b
                .task_constructors
                .keys()
                .filter(|key| {
                    key.caller == Some(caller.id)
                        && key.function == f.id
                        && key.result == result
                        && key.worker == worker
                        && key.owned == owned
                })
                .copied()
                .collect::<Vec<_>>();
            if keys.len() != 1 {
                return Err(bad(
                    "Task constructor lacks a unique exact caller-world site",
                ));
            }
            b.spend(selected.len())?;
            let obligation = b.task_constructors.get_mut(&keys[0]).unwrap();
            if !obligation.raw_checked || obligation.state == ObligationState::Checking {
                return Err(bad("Task constructor kernel is unfinished or cyclic"));
            }
            obligation.frames = selected;
            obligation.frame = obligation.frames.first().copied();
            obligation.semantic_checked = true;
            obligation.state = ObligationState::Discharged;
        }
        Ok(value)
    }
}

fn task_actual_shape(a: &CheckedActual, z: &CheckedActual) -> bool {
    let value = |a: &CheckedType, z: &CheckedType| {
        a.ty == z.ty
            && a.life == z.life
            && a.classes.len() == z.classes.len()
            && a.origins.len() == z.origins.len()
            && a.statics == z.statics
    };
    match (a, z) {
        (CheckedActual::Type(a), CheckedActual::Type(z)) => value(a, z),
        (CheckedActual::Life(_), CheckedActual::Life(_)) => true,
        (
            CheckedActual::Const {
                carrier: a,
                value: av,
            },
            CheckedActual::Const {
                carrier: z,
                value: zv,
            },
        ) => av == zv && value(a, z),
        _ => false,
    }
}
