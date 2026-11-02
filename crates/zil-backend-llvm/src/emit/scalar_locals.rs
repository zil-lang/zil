//! Eliminate unobservable scalar slots without changing the verified CFG.
//! Mutable locals are confined to one *source* block, even when checked
//! operations split it into several machine blocks. Calls and checks stay put.
use zil_ir::{BlockId, InstructionKind as I, Type, TypedFunction};
use zil_llvm_api::model as m;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operand {
    Argument(usize),
    Value(usize),
}
impl Operand {
    fn machine(self) -> m::Val {
        m::local(match self {
            Self::Argument(id) => format!("arg.{id}"),
            Self::Value(id) => format!("v.{id}"),
        })
    }
    fn matches(self, value: &m::Val) -> bool {
        let m::Val::Local(name) = value else {
            return false;
        };
        match self {
            Self::Argument(id) => index(name, "arg.") == Some(id),
            Self::Value(id) => index(name, "v.") == Some(id),
        }
    }
}

struct Local {
    bits: u32,
    eligible: bool,
    parameter: Option<usize>,
    block: Option<BlockId>,
    current: Option<Operand>,
    loads: usize,
    stores: Vec<Operand>,
}
impl Local {
    fn in_block(&mut self, block: BlockId) {
        if self.block.is_some_and(|previous| previous != block) {
            self.eligible = false;
        }
        self.block = Some(block);
    }
}
#[derive(Clone, Copy)]
struct Alias {
    local: usize,
    operand: Operand,
}
struct Plan {
    locals: Vec<Local>,
    aliases: Vec<Option<Alias>>,
}

fn source_plan(source: &TypedFunction) -> Option<Plan> {
    // Never size a table from the largest raw ID. Noncanonical IDs simply keep
    // their original representation; admission still belongs to Verify.
    if source.environment.is_some()
        || source
            .locals
            .iter()
            .enumerate()
            .any(|(id, local)| local.id.0 as usize != id)
    {
        return None;
    }
    let mut plan = Plan {
        locals: source
            .locals
            .iter()
            .map(|local| {
                let bits = match local.ty {
                    Type::Bool => 1,
                    Type::U8 => 8,
                    Type::U16 => 16,
                    Type::I32 => 32,
                    Type::I64 => 64,
                    _ => 0,
                };
                Local {
                    bits,
                    eligible: bits != 0,
                    parameter: None,
                    block: None,
                    current: None,
                    loads: 0,
                    stores: vec![],
                }
            })
            .collect(),
        aliases: vec![None; source.values.len()],
    };
    for (argument, id) in source.parameters.iter().enumerate() {
        let local = plan.locals.get_mut(id.0 as usize)?;
        if local.parameter.replace(argument).is_some() {
            return None;
        }
        local.current = Some(Operand::Argument(argument));
        local.stores.push(Operand::Argument(argument));
    }
    for block in &source.blocks {
        for instruction in &block.instructions {
            match &instruction.kind {
                I::Load(id) => {
                    let slot = id.0 as usize;
                    let local = plan.locals.get_mut(slot)?;
                    if !local.eligible {
                        continue;
                    }
                    if local.parameter.is_none() {
                        local.in_block(block.id);
                    }
                    let Some(operand) = local.current else {
                        local.eligible = false;
                        continue;
                    };
                    let result = instruction.result?.0 as usize;
                    if source.values.get(result)? != &source.locals.get(slot)?.ty {
                        return None;
                    }
                    let alias = plan.aliases.get_mut(result)?;
                    if alias.is_some() {
                        return None;
                    }
                    *alias = Some(Alias {
                        local: slot,
                        operand,
                    });
                    local.loads += 1;
                }
                I::Store { local: id, value } => {
                    let slot = id.0 as usize;
                    let local = plan.locals.get_mut(slot)?;
                    // Changing parameters can carry state around back edges.
                    // They are outside this first, deliberately local strategy.
                    if local.parameter.is_some() {
                        local.eligible = false;
                    }
                    if !local.eligible {
                        continue;
                    }
                    local.in_block(block.id);
                    let value = value.0 as usize;
                    if source.values.get(value)? != &source.locals.get(slot)?.ty {
                        return None;
                    }
                    let operand = Operand::Value(value);
                    local.current = Some(operand);
                    local.stores.push(operand);
                }
                I::StorageLive(id) | I::StorageDead(id) => {
                    let local = plan.locals.get_mut(id.0 as usize)?;
                    if local.parameter.is_some() {
                        // Parameter StorageDead at each exit has no machine
                        // operation. Verify still checks all source lifetimes.
                        // A restart, unlike an exit, cannot reuse the entry value.
                        if matches!(instruction.kind, I::StorageLive(_)) {
                            local.eligible = false;
                        }
                    } else {
                        local.in_block(block.id);
                        local.current = None;
                    }
                }
                I::BorrowLocal { local } | I::BorrowLocalMut { local } => {
                    plan.locals.get_mut(local.0 as usize)?.eligible = false;
                }
                I::Constant(_)
                | I::ConvertInteger { .. }
                | I::Unary { .. }
                | I::Binary { .. }
                | I::Call { .. }
                | I::ForeignCall { .. }
                | I::FunctionRef { .. }
                | I::ForeignFunctionRef { .. }
                | I::CallIndirect { .. }
                | I::UnsafeEnter { .. }
                | I::UnsafeExit { .. }
                | I::ScopeEnter { .. }
                | I::ScopeExit { .. } => {}
                // Resource, capture, projection and future instructions retain
                // the whole original function. No unknown local use is inferred.
                _ => return None,
            }
        }
    }
    plan.locals
        .iter()
        .any(|local| local.eligible)
        .then_some(plan)
}

// Names are emitted internally. Reject noncanonical spellings instead of
// interpreting e.g. local.00 as the slot named local.0.
fn index(name: &str, prefix: &str) -> Option<usize> {
    let number = name.strip_prefix(prefix)?;
    if number.len() > 1 && number.starts_with('0') {
        return None;
    }
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}
fn slot(value: &m::Val, count: usize) -> Option<usize> {
    let m::Val::Local(name) = value else {
        return None;
    };
    index(name, "local.").filter(|id| *id < count)
}
fn reject_uses(value: &m::Val, locals: &mut [Local]) {
    // Nested constant operands are unusual here, but still cannot hide an
    // escaping slot. This scan does not allocate an operand list.
    match value {
        m::Val::Local(name) => {
            if let Some(local) = index(name, "local.").and_then(|id| locals.get_mut(id)) {
                local.eligible = false;
            }
        }
        m::Val::Array(args) => {
            for arg in args {
                reject_uses(&arg.value, locals);
            }
        }
        m::Val::Gep { ptr, indices, .. } => {
            reject_uses(ptr, locals);
            for arg in indices {
                reject_uses(&arg.value, locals);
            }
        }
        _ => {}
    }
}

fn check_machine(plan: &mut Plan, machine: &m::Function) -> bool {
    let Some(blocks) = &machine.blocks else {
        return false;
    };
    let mut counts = vec![(0usize, 0usize, 0usize); plan.locals.len()];
    let mut seen_loads = vec![false; plan.aliases.len()];
    for block in blocks {
        for instruction in &block.instructions {
            if let Some(alias) = instruction
                .result
                .as_deref()
                .and_then(|name| index(name, "v."))
                .and_then(|id| plan.aliases.get(id))
                .copied()
                .flatten()
                && !matches!(&instruction.op, m::Op::Load { ptr, .. }
                    if slot(ptr, plan.locals.len()) == Some(alias.local))
            {
                // Do not erase a source Load if another machine definition
                // unexpectedly took the same result name.
                plan.locals[alias.local].eligible = false;
            }
            if let Some(id) = instruction
                .result
                .as_deref()
                .and_then(|n| index(n, "local."))
                && let Some(local) = plan.locals.get_mut(id)
            {
                if matches!(&instruction.op, m::Op::Alloca { ty, .. } if *ty == m::Ty::Int(local.bits))
                    && block.name == "entry"
                {
                    counts[id].0 += 1;
                } else {
                    local.eligible = false;
                }
            }
            match &instruction.op {
                m::Op::Load { ty, ptr, .. } => {
                    if let Some(id) = slot(ptr, plan.locals.len()) {
                        counts[id].1 += 1;
                        let result = instruction.result.as_deref().and_then(|n| index(n, "v."));
                        let alias = result.and_then(|v| plan.aliases.get(v)).copied().flatten();
                        if *ty != m::Ty::Int(plan.locals[id].bits)
                            || !alias.is_some_and(|alias| alias.local == id)
                        {
                            plan.locals[id].eligible = false;
                        } else if let Some(seen) = result.and_then(|v| seen_loads.get_mut(v)) {
                            if *seen {
                                return false;
                            }
                            *seen = true;
                        }
                    } else {
                        reject_uses(ptr, &mut plan.locals);
                    }
                }
                m::Op::Store { arg, ptr, .. } => {
                    if let Some(id) = slot(ptr, plan.locals.len()) {
                        let local = &mut plan.locals[id];
                        if arg.ty != m::Ty::Int(local.bits)
                            || !local
                                .stores
                                .get(counts[id].2)
                                .is_some_and(|value| value.matches(&arg.value))
                            || (local.parameter.is_some() && block.name != "entry")
                        {
                            local.eligible = false;
                        }
                        counts[id].2 += 1;
                    } else {
                        reject_uses(ptr, &mut plan.locals);
                    }
                    reject_uses(&arg.value, &mut plan.locals);
                }
                _ => {
                    for value in instruction.op.values() {
                        reject_uses(value, &mut plan.locals);
                    }
                }
            }
        }
    }
    for (local, (allocas, loads, stores)) in plan.locals.iter_mut().zip(counts) {
        local.eligible &= allocas == 1 && loads == local.loads && stores == local.stores.len();
    }
    plan.locals.iter().any(|local| local.eligible)
}

// Resolve every active alias once, including aliases to loads of a different
// promoted local. The iterative walk is bounded by the existing value table;
// it neither assumes increasing IDs nor recurses along a long assignment chain.
fn resolve(plan: &Plan) -> Option<Vec<Option<Operand>>> {
    let mut result = vec![None; plan.aliases.len()];
    let mut state = vec![0u8; plan.aliases.len()];
    let mut path = vec![];
    for start in 0..plan.aliases.len() {
        let Some(alias) = plan.aliases[start].filter(|a| plan.locals[a.local].eligible) else {
            continue;
        };
        if state[start] == 2 {
            continue;
        }
        state[start] = 1;
        path.push(start);
        let mut operand = alias.operand;
        while let Operand::Value(id) = operand {
            let Some(alias) = plan
                .aliases
                .get(id)?
                .filter(|a| plan.locals[a.local].eligible)
            else {
                break;
            };
            match state[id] {
                1 => return None,
                2 => {
                    operand = result[id]?;
                    break;
                }
                _ => {
                    state[id] = 1;
                    path.push(id);
                    operand = alias.operand;
                }
            }
        }
        for id in path.drain(..) {
            result[id] = Some(operand);
            state[id] = 2;
        }
    }
    Some(result)
}

fn replace(value: &mut m::Val, aliases: &[Option<Operand>]) {
    match value {
        m::Val::Local(name) => {
            if let Some(Some(operand)) = index(name, "v.").and_then(|id| aliases.get(id)) {
                *value = operand.machine();
            }
        }
        m::Val::Array(args) => {
            for arg in args {
                replace(&mut arg.value, aliases);
            }
        }
        m::Val::Gep { ptr, indices, .. } => {
            replace(ptr, aliases);
            for arg in indices {
                replace(&mut arg.value, aliases);
            }
        }
        _ => {}
    }
}
fn replace_operands(op: &mut m::Op, aliases: &[Option<Operand>]) {
    match op {
        m::Op::Alloca { .. } | m::Op::Branch(_) | m::Op::Unreachable | m::Op::Return(None) => {}
        m::Op::Load { ptr, .. } => replace(ptr, aliases),
        m::Op::Store { arg, ptr, .. } => {
            replace(&mut arg.value, aliases);
            replace(ptr, aliases);
        }
        m::Op::Binary { left, right, .. } | m::Op::Compare { left, right, .. } => {
            replace(left, aliases);
            replace(right, aliases);
        }
        m::Op::Cast { arg, .. }
        | m::Op::Extract { arg, .. }
        | m::Op::Freeze(arg)
        | m::Op::Return(Some(arg)) => replace(&mut arg.value, aliases),
        m::Op::Insert { arg, field, .. } => {
            replace(&mut arg.value, aliases);
            replace(&mut field.value, aliases);
        }
        m::Op::Gep { ptr, indices, .. } => {
            replace(ptr, aliases);
            for arg in indices {
                replace(&mut arg.value, aliases);
            }
        }
        m::Op::Call { callee, args, .. } => {
            replace(callee, aliases);
            for arg in args {
                replace(&mut arg.value, aliases);
            }
        }
        m::Op::Phi { incoming, .. } => {
            for (value, _) in incoming {
                replace(value, aliases);
            }
        }
        m::Op::CondBranch { condition, .. } => replace(condition, aliases),
    }
}

pub(super) fn promote(source: &TypedFunction, machine: &mut m::Function) {
    if machine.name != super::symbol(source.id) {
        return;
    }
    let Some(mut plan) = source_plan(source) else {
        return;
    };
    if !check_machine(&mut plan, machine) {
        return;
    }
    let Some(aliases) = resolve(&plan) else {
        return;
    };
    // Everything above is read-only with respect to machine IR. A rejected
    // candidate has never been partially rewritten.
    for block in machine.blocks.iter_mut().flatten() {
        block.instructions.retain_mut(|instruction| {
            let id = match &instruction.op {
                m::Op::Alloca { .. } => instruction
                    .result
                    .as_deref()
                    .and_then(|n| index(n, "local.")),
                m::Op::Load { ptr, .. } | m::Op::Store { ptr, .. } => slot(ptr, plan.locals.len()),
                _ => None,
            };
            if id
                .and_then(|id| plan.locals.get(id))
                .is_some_and(|local| local.eligible)
            {
                return false;
            }
            replace_operands(&mut instruction.op, &aliases);
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::Span;
    use zil_ir::{
        BasicBlock, FunctionId, Instruction, LocalDecl, LocalId, PassingMode, Terminator, ValueId,
    };

    // These are internal transformation guards, not source acceptance evidence.
    // Source/Verify/native controls live in the driver's integration test.
    fn fixture() -> (TypedFunction, m::Function) {
        let source = TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(0),
            name: "read".into(),
            foreign: None,
            environment: None,
            parameters: vec![LocalId(0)],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "x".into(),
                ty: Type::I32,
                mutable: false,
                span: Span::default(),
            }],
            values: vec![Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: I::Load(LocalId(0)),
                    span: Span::default(),
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span: Span::default(),
        };
        let machine = m::Function {
            name: "zil.fn.0".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                m::Ty::Int(32),
                vec![m::Param::new(m::Ty::Int(32), "arg.0")],
            ),
            attrs: vec![],
            blocks: Some(vec![m::Block {
                name: "entry".into(),
                instructions: vec![
                    m::Inst::new(
                        Some("local.0".into()),
                        m::Op::Alloca {
                            ty: m::Ty::Int(32),
                            align: 4,
                        },
                    ),
                    m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(m::Ty::Int(32), m::local("arg.0".into())),
                            ptr: m::local("local.0".into()),
                            align: 4,
                        },
                    ),
                    m::Inst::new(
                        Some("v.0".into()),
                        m::Op::Load {
                            ty: m::Ty::Int(32),
                            ptr: m::local("local.0".into()),
                            align: 4,
                        },
                    ),
                    m::Inst::new(
                        None,
                        m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::local("v.0".into())))),
                    ),
                ],
            }]),
        };
        (source, machine)
    }

    #[test]
    fn unexpected_machine_address_use_preserves_the_whole_slot() {
        for nested in [false, true] {
            let (source, mut machine) = fixture();
            let pointer = m::local("local.0".into());
            let argument = if nested {
                m::Val::Gep {
                    ty: m::Ty::Int(32),
                    ptr: Box::new(pointer),
                    indices: vec![m::Arg::new(m::Ty::Int(32), 0)],
                }
            } else {
                pointer
            };
            machine.blocks.as_mut().unwrap()[0].instructions.insert(
                3,
                m::Inst::new(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global("observe".into()),
                        args: vec![m::Arg::new(m::Ty::Ptr, argument)],
                        vararg: None,
                    },
                ),
            );
            let before = machine.clone();
            promote(&source, &mut machine);
            assert_eq!(machine, before);
        }
    }

    #[test]
    fn sparse_or_out_of_range_source_ids_do_not_allocate_from_the_id() {
        for invalid_local in [false, true] {
            let (mut source, mut machine) = fixture();
            if invalid_local {
                source.locals[0].id = LocalId(u32::MAX);
            } else {
                source.blocks[0].instructions[0].result = Some(ValueId(u32::MAX));
            }
            let before = machine.clone();
            promote(&source, &mut machine);
            assert_eq!(machine, before);
        }
    }

    #[test]
    fn aliases_resolve_without_recursion_and_cycles_keep_original_ir() {
        let (source, _) = fixture();
        let mut plan = source_plan(&source).unwrap();
        plan.aliases = (0..4096)
            .map(|id| {
                Some(Alias {
                    local: 0,
                    operand: if id == 4095 {
                        Operand::Argument(0)
                    } else {
                        Operand::Value(id + 1)
                    },
                })
            })
            .collect();
        assert!(
            resolve(&plan)
                .unwrap()
                .iter()
                .all(|value| *value == Some(Operand::Argument(0)))
        );
        plan.aliases[4095].as_mut().unwrap().operand = Operand::Value(0);
        assert!(resolve(&plan).is_none());
    }

    fn instruction(result: Option<u32>, kind: I) -> Instruction {
        Instruction {
            result: result.map(ValueId),
            kind,
            span: Span::default(),
        }
    }

    #[test]
    fn a_single_source_block_loop_requires_a_definition_before_its_first_load() {
        let (mut source, _) = fixture();
        source.parameters.clear();
        source.locals[0].mutable = true;
        source.values = vec![Type::I32; 2];
        source.blocks[0].terminator = Terminator::Jump(BlockId(0));
        source.blocks[0].instructions = vec![
            instruction(Some(0), I::Load(LocalId(0))),
            instruction(Some(1), I::Constant(zil_ir::Constant::I32(7))),
            instruction(
                None,
                I::Store {
                    local: LocalId(0),
                    value: ValueId(1),
                },
            ),
        ];
        assert!(source_plan(&source).is_none());

        source.blocks[0].instructions = vec![
            instruction(None, I::StorageLive(LocalId(0))),
            instruction(Some(0), I::Constant(zil_ir::Constant::I32(7))),
            instruction(
                None,
                I::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
            ),
            instruction(Some(1), I::Load(LocalId(0))),
            instruction(None, I::StorageDead(LocalId(0))),
        ];
        let plan = source_plan(&source).unwrap();
        assert!(plan.locals[0].eligible);
        assert_eq!(resolve(&plan).unwrap()[1], Some(Operand::Value(0)));
    }

    #[test]
    fn a_new_storage_activation_cannot_inherit_the_previous_value() {
        let (mut source, _) = fixture();
        source.parameters.clear();
        source.values = vec![Type::I32; 2];
        source.blocks[0].instructions = vec![
            instruction(None, I::StorageLive(LocalId(0))),
            instruction(Some(0), I::Constant(zil_ir::Constant::I32(7))),
            instruction(
                None,
                I::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
            ),
            instruction(None, I::StorageDead(LocalId(0))),
            instruction(None, I::StorageLive(LocalId(0))),
            instruction(Some(1), I::Load(LocalId(0))),
        ];
        assert!(source_plan(&source).is_none());
    }

    #[test]
    fn readonly_parameters_do_not_depend_on_source_block_order_or_reachability() {
        let (mut source, _) = fixture();
        source.values = vec![Type::I32; 2];
        source.blocks = vec![
            BasicBlock {
                id: BlockId(2),
                instructions: vec![
                    instruction(Some(0), I::Load(LocalId(0))),
                    instruction(None, I::StorageDead(LocalId(0))),
                ],
                terminator: Terminator::Return(Some(ValueId(0))),
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    instruction(Some(1), I::Load(LocalId(0))),
                    instruction(None, I::StorageDead(LocalId(0))),
                ],
                terminator: Terminator::Return(Some(ValueId(1))),
            },
            BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Jump(BlockId(2)),
            },
        ];
        let plan = source_plan(&source).unwrap();
        assert_eq!(resolve(&plan).unwrap(), vec![Some(Operand::Argument(0)); 2]);
    }

    #[test]
    fn an_alias_to_a_later_disqualified_slot_keeps_that_slots_load() {
        let (mut source, mut machine) = fixture();
        source.locals.push(LocalDecl {
            id: LocalId(1),
            name: "copy".into(),
            ty: Type::I32,
            mutable: false,
            span: Span::default(),
        });
        source.values = vec![
            Type::I32,
            Type::I32,
            Type::Address {
                inner: Box::new(Type::I32),
                mutable: false,
            },
        ];
        source.blocks[0].instructions = vec![
            instruction(Some(0), I::Load(LocalId(0))),
            instruction(
                None,
                I::Store {
                    local: LocalId(1),
                    value: ValueId(0),
                },
            ),
            instruction(Some(1), I::Load(LocalId(1))),
            instruction(Some(2), I::BorrowLocal { local: LocalId(0) }),
        ];
        source.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        let instructions = &mut machine.blocks.as_mut().unwrap()[0].instructions;
        instructions.insert(
            1,
            m::Inst::new(
                Some("local.1".into()),
                m::Op::Alloca {
                    ty: m::Ty::Int(32),
                    align: 4,
                },
            ),
        );
        instructions.pop();
        instructions.extend([
            m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Int(32), m::local("v.0".into())),
                    ptr: m::local("local.1".into()),
                    align: 4,
                },
            ),
            m::Inst::new(
                Some("v.1".into()),
                m::Op::Load {
                    ty: m::Ty::Int(32),
                    ptr: m::local("local.1".into()),
                    align: 4,
                },
            ),
            m::Inst::new(
                Some("v.2".into()),
                m::Op::Gep {
                    ty: m::Ty::Int(8),
                    ptr: m::local("local.0".into()),
                    indices: vec![m::Arg::new(m::Ty::Int(64), 0)],
                },
            ),
            m::Inst::new(
                None,
                m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::local("v.1".into())))),
            ),
        ]);
        promote(&source, &mut machine);
        let instructions = &machine.blocks.as_ref().unwrap()[0].instructions;
        assert_eq!(instructions.len(), 5);
        assert!(
            instructions
                .iter()
                .any(|i| i.result.as_deref() == Some("local.0"))
        );
        assert!(
            instructions
                .iter()
                .any(|i| i.result.as_deref() == Some("v.0") && matches!(i.op, m::Op::Load { .. }))
        );
        assert_eq!(
            instructions.last().unwrap().op,
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::local("v.0".into()))))
        );
    }
}
