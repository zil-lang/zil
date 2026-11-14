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
        use_: Use,
        values: &[Option<CheckedType>],
        environment: &EvalEnv,
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
            if !definition.parameters.is_empty()
                || !definition.environment.is_empty()
                || !definition.requirements.is_empty()
            {
                return Err(unsupported(
                    "Task worker full actual/where instantiation is not yet bridged",
                    Some(instruction.span),
                ));
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
                    actuals: vec![],
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
        // This performs full type WF. A retained result whose exact source
        // relation is not implemented is rejected, never assigned an empty N.
        let value = self.closed_value(module, &raw, b)?;
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
