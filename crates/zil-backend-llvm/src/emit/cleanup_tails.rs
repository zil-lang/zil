//! Intern identical return-cleanup suffixes within one currently verified
//! function. Nodes are keyed by the exact action/local and successor, not by a
//! guessed lifetime. The planner is optional, bounded, and never mutates LIR.
use super::*;
use zil_ir::CallKind;

#[derive(Clone, Copy)]
pub(super) struct Start {
    pub(super) at: usize,
    pub(super) node: usize,
}
pub(super) struct Node<'a> {
    pub(super) instruction: &'a Instruction,
    pub(super) next: usize,
}
pub(super) struct Plan<'a> {
    pub(super) starts: Vec<Option<Start>>,
    // Node zero is the common scalar/unit return, not a drop action.
    pub(super) nodes: Vec<Node<'a>>,
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}
fn action(function: &TypedFunction, instruction: &Instruction) -> Option<Option<(u8, LocalId)>> {
    if instruction.result.is_some() {
        return None;
    }
    let (tag, id) = match instruction.kind {
        InstructionKind::DropClosure { local } => (0, local),
        InstructionKind::DropManaged { local } => (1, local),
        InstructionKind::StorageDead(local) => (2, local),
        InstructionKind::ScopeExit { .. } => return Some(None),
        _ => return None,
    };
    let local = function.locals.get(id.0 as usize)?;
    if local.id != id {
        return None;
    }
    match (&local.ty, tag) {
        (Type::Closure(_), 0 | 2) => Some(Some((tag, id))),
        (Type::Managed(payload), 1 | 2) if **payload == Type::ByteBuffer => Some(Some((tag, id))),
        (ty, 2) if scalar(ty) => Some(None),
        _ => None,
    }
}

pub(super) fn plan<'a>(
    function: &'a TypedFunction,
    limits: &zil_base::Limits,
    task_owned: bool,
) -> Option<Plan<'a>> {
    if task_owned
        || function.call_kind != CallKind::Direct
        || !scalar(&function.return_type)
        || function.blocks.len() < 2
        || function.foreign.is_some()
    {
        return None;
    }
    // This is optional optimization fuel, not verifier authority or a renewed
    // safety budget. Exhaustion discards the whole plan and keeps original code.
    // Every stored key has constant size; no allocation depends on a raw ID.
    let mut fuel = limits.max_analysis_steps.min(1 << 20);
    fuel = fuel.checked_sub(function.blocks.len())?;
    let mut starts = vec![None; function.blocks.len()];
    let mut nodes = Vec::new();
    let mut intern = BTreeMap::<(u8, LocalId, usize), usize>::new();
    let mut occurrences = 0usize;
    let mut exits = 0usize;
    for (index, block) in function.blocks.iter().enumerate() {
        let Terminator::Return(value) = block.terminator else {
            continue;
        };
        if function.return_type != Type::Unit
            && value.and_then(|v| function.values.get(v.0 as usize)) != Some(&function.return_type)
        {
            return None;
        }
        let mut tail = 0;
        let mut at = block.instructions.len();
        for instruction in block.instructions.iter().rev() {
            fuel = fuel.checked_sub(1)?;
            let Some(action) = action(function, instruction) else {
                break;
            };
            at -= 1;
            let Some((tag, local)) = action else {
                continue;
            };
            // Account for ordered-map comparison work before each lookup.
            fuel = fuel.checked_sub(2 + intern.len().checked_ilog2().unwrap_or(0) as usize)?;
            let key = (tag, local, tail);
            tail = if let Some(&existing) = intern.get(&key) {
                existing
            } else {
                if nodes.len() >= limits.max_nodes.min(65536) {
                    return None;
                }
                let id = nodes.len() + 1;
                nodes.push(Node {
                    instruction,
                    next: tail,
                });
                intern.insert(key, id);
                id
            };
            occurrences += 1;
        }
        if tail != 0 {
            starts[index] = Some(Start { at, node: tail });
            exits += 1;
        }
    }
    // Avoid extra branches/return storage for tiny or non-repeated cleanup.
    (exits >= 2 && occurrences >= nodes.len().saturating_add(8)).then_some(Plan { starts, nodes })
}

impl FunctionEmitter<'_> {
    pub(super) fn cleanup_return_slot(&mut self) -> CompileResult<()> {
        if self.function.return_type != Type::Unit {
            self.event(m::Event::Instruction(m::Inst::new(
                Some("cleanup.return.slot".into()),
                m::Op::Alloca {
                    ty: llvm_type(&self.function.return_type)?,
                    align: alignment(&self.function.return_type, self.closures)? as u32,
                },
            )));
        }
        Ok(())
    }
    pub(super) fn cleanup_exit(
        &mut self,
        returned: Option<ValueId>,
        node: usize,
    ) -> CompileResult<()> {
        if self.function.return_type != Type::Unit {
            let returned = returned.expect("cleanup plan checked the current scalar result");
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(
                        llvm_type(&self.function.return_type)?,
                        m::local(format!("v.{}", returned.0)),
                    ),
                    ptr: m::local("cleanup.return.slot".into()),
                    align: alignment(&self.function.return_type, self.closures)? as u32,
                },
            )));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("cleanup.tail.{node}")),
        )));
        Ok(())
    }
    pub(super) fn cleanup_nodes(&mut self, plan: &Plan<'_>) -> CompileResult<()> {
        for (index, node) in plan.nodes.iter().enumerate() {
            self.event(m::Event::Label(format!("cleanup.tail.{}", index + 1)));
            // Use the unchanged instruction lowering, including its live flag,
            // capture layout and original destructor. Never reorder actions.
            self.instruction(node.instruction)?;
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Branch(format!("cleanup.tail.{}", node.next)),
            )));
        }
        self.event(m::Event::Label("cleanup.tail.0".into()));
        let returned = if self.function.return_type == Type::Unit {
            None
        } else {
            let ty = llvm_type(&self.function.return_type)?;
            self.event(m::Event::Instruction(m::Inst::new(
                Some("cleanup.return.value".into()),
                m::Op::Load {
                    ty: ty.clone(),
                    ptr: m::local("cleanup.return.slot".into()),
                    align: alignment(&self.function.return_type, self.closures)? as u32,
                },
            )));
            Some(m::Arg::new(ty, m::local("cleanup.return.value".into())))
        };
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(returned),
        )));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_ir::{BasicBlock, BlockId, LocalDecl, PassingMode};
    fn input(sequences: &[Vec<u32>]) -> TypedFunction {
        TypedFunction {
            retained_return_sources: None,
            storage_life: None,
            call_kind: CallKind::Direct,
            resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contracts: Vec::new(),
            id: FunctionId(0),
            name: "cleanup_model".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: (0..8)
                .map(|id| LocalDecl {
                    id: LocalId(id),
                    name: format!("r{id}"),
                    ty: Type::Managed(Box::new(Type::ByteBuffer)),
                    mutable: false,
                    span: Span::default(),
                })
                .collect(),
            values: vec![],
            blocks: sequences
                .iter()
                .enumerate()
                .map(|(id, seq)| BasicBlock {
                    id: BlockId(id as u32),
                    instructions: seq
                        .iter()
                        .map(|&local| Instruction {
                            result: None,
                            kind: InstructionKind::DropManaged {
                                local: LocalId(local),
                            },
                            span: Span::new(
                                zil_base::FileId(id as u32),
                                local as usize,
                                local as usize + 1,
                            ),
                        })
                        .collect(),
                    terminator: Terminator::Return(None),
                })
                .collect(),
            entry: BlockId(0),
            span: Span::default(),
        }
    }
    fn replay(plan: &Plan<'_>, index: usize) -> Vec<u32> {
        let mut node = plan.starts[index].unwrap().node;
        let mut result = Vec::new();
        while node != 0 {
            let n = &plan.nodes[node - 1];
            let InstructionKind::DropManaged { local } = n.instruction.kind else {
                panic!()
            };
            result.push(local.0);
            node = n.next;
            assert!(
                result.len() < 1024,
                "interned successor graph must be acyclic"
            );
        }
        result
    }
    #[test]
    fn cleanup_tail_interning_preserves_every_order_and_repeated_local() {
        // All length-four action strings, including repeated actions: the
        // transform preserves sequences, it does not infer they are valid.
        let seqs = (0..256)
            .map(|mut n| {
                (0..4)
                    .map(|_| {
                        let x = n % 4;
                        n /= 4;
                        x
                    })
                    .collect()
            })
            .collect::<Vec<Vec<_>>>();
        let f = input(&seqs);
        let p = plan(&f, &zil_base::Limits::default(), false).unwrap();
        for (i, expected) in seqs.iter().enumerate() {
            assert_eq!(replay(&p, i), *expected);
        }
        assert!(p.nodes.len() < seqs.iter().map(Vec::len).sum::<usize>());
    }
    #[test]
    fn cleanup_tail_interning_shares_actual_suffix_not_just_resource_set() {
        let seqs = vec![
            vec![0, 1, 2, 3],
            vec![0, 1, 2, 3],
            vec![1, 0, 2, 3],
            vec![1, 0, 2, 3],
        ];
        let f = input(&seqs);
        let p = plan(&f, &zil_base::Limits::default(), false).unwrap();
        assert_ne!(p.starts[0].unwrap().node, p.starts[2].unwrap().node);
        for (i, expected) in seqs.iter().enumerate() {
            assert_eq!(replay(&p, i), *expected);
        }
        assert_eq!(p.nodes.len(), 6);
    }
    #[test]
    fn cleanup_tail_interning_keeps_effect_prefixes_and_distinct_scalar_results() {
        let sequences = vec![vec![0, 1, 2, 3]; 4];
        let mut f = input(&sequences);
        f.return_type = Type::I32;
        f.values = vec![Type::I32; 4];
        for (i, b) in f.blocks.iter_mut().enumerate() {
            b.instructions.insert(
                0,
                Instruction {
                    result: Some(ValueId(i as u32)),
                    kind: InstructionKind::Constant(Constant::I32(i as i32)),
                    span: Span::default(),
                },
            );
            b.terminator = Terminator::Return(Some(ValueId(i as u32)));
        }
        let old = f.clone();
        let p = plan(&f, &zil_base::Limits::default(), false).unwrap();
        assert!(p.starts.iter().all(|s| s.unwrap().at == 1));
        assert_eq!(f, old);
    }
    #[test]
    fn cleanup_tail_interning_declines_nonreturns_tasks_unknown_actions_and_exhaustion() {
        let sequences = vec![vec![0, 1, 2, 3]; 4];
        let f = input(&sequences);
        assert!(plan(&f, &zil_base::Limits::default(), true).is_none());
        assert!(
            plan(
                &f,
                &zil_base::Limits {
                    max_analysis_steps: 0,
                    ..Default::default()
                },
                false
            )
            .is_none()
        );
        assert!(
            plan(
                &f,
                &zil_base::Limits {
                    max_nodes: 0,
                    ..Default::default()
                },
                false
            )
            .is_none()
        );
        let mut g = f.clone();
        g.return_type = Type::ByteBuffer;
        assert!(plan(&g, &zil_base::Limits::default(), false).is_none());
        let mut g = f.clone();
        for b in &mut g.blocks {
            b.terminator = Terminator::Jump(BlockId(0));
        }
        assert!(plan(&g, &zil_base::Limits::default(), false).is_none());
        let mut g = f.clone();
        for b in &mut g.blocks {
            b.instructions.push(Instruction {
                result: None,
                kind: InstructionKind::StorageLive(LocalId(0)),
                span: Span::default(),
            });
        }
        assert!(plan(&g, &zil_base::Limits::default(), false).is_none());
        let mut g = f;
        g.locals[3].id = LocalId(u32::MAX);
        assert!(plan(&g, &zil_base::Limits::default(), false).is_none());
    }
}
