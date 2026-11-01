//! Request-local evaluation of already bound, independently verified scalar CFGs.
//! A failed/partial evaluation never changes the emitted function.
use super::*;

const MAX_WORK: usize = 1_000_000;
const MAX_DEPTH: usize = 64;

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::I32 | Type::I64 | Type::U8 | Type::U16
    )
}
fn eligible(f: &TypedFunction) -> bool {
    f.foreign.is_none()
        && f.environment.is_none()
        && scalar(&f.return_type)
        && f.locals.iter().all(|local| scalar(&local.ty))
        && f.values.iter().all(scalar)
        && f.blocks.iter().all(|block| {
            block.instructions.iter().all(|i| match &i.kind {
                InstructionKind::Constant(c) => scalar(&c.ty()),
                InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::Call { .. } => true,
                InstructionKind::Unary { op, .. } => matches!(op, UnaryOp::Negate | UnaryOp::Not),
                InstructionKind::Binary { op, .. } => matches!(
                    op,
                    BinaryOp::Add
                        | BinaryOp::Subtract
                        | BinaryOp::Multiply
                        | BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                ),
                _ => false,
            })
        })
}

fn integer(c: &Constant) -> Option<i128> {
    Some(match c {
        Constant::I32(n) => i128::from(*n),
        Constant::I64(n) => i128::from(*n),
        Constant::U8(n) => i128::from(*n),
        Constant::U16(n) => i128::from(*n),
        _ => return None,
    })
}
fn checked(n: i128, ty: &Type) -> Option<Constant> {
    Some(match ty {
        Type::I32 => Constant::I32(n.try_into().ok()?),
        Type::I64 => Constant::I64(n.try_into().ok()?),
        Type::U8 => Constant::U8(n.try_into().ok()?),
        Type::U16 => Constant::U16(n.try_into().ok()?),
        _ => return None,
    })
}
fn binary(op: BinaryOp, left: &Constant, right: &Constant) -> Option<Constant> {
    if left.ty() != right.ty() {
        return None;
    }
    if let (Constant::Bool(l), Constant::Bool(r)) = (left, right) {
        return Some(Constant::Bool(match op {
            BinaryOp::Equal => l == r,
            BinaryOp::NotEqual => l != r,
            BinaryOp::And => *l && *r,
            BinaryOp::Or => *l || *r,
            _ => return None,
        }));
    }
    let l = integer(left)?;
    let r = integer(right)?;
    match op {
        BinaryOp::Add => checked(l.checked_add(r)?, &left.ty()),
        BinaryOp::Subtract => checked(l.checked_sub(r)?, &left.ty()),
        BinaryOp::Multiply => checked(l.checked_mul(r)?, &left.ty()),
        BinaryOp::Equal => Some(Constant::Bool(l == r)),
        BinaryOp::NotEqual => Some(Constant::Bool(l != r)),
        BinaryOp::Less => Some(Constant::Bool(l < r)),
        BinaryOp::LessEqual => Some(Constant::Bool(l <= r)),
        BinaryOp::Greater => Some(Constant::Bool(l > r)),
        BinaryOp::GreaterEqual => Some(Constant::Bool(l >= r)),
        _ => None,
    }
}

struct Evaluator<'a> {
    functions: &'a BTreeMap<FunctionId, &'a TypedFunction>,
    eligible: BTreeSet<FunctionId>,
    active: BTreeSet<FunctionId>,
    summaries: BTreeMap<FunctionId, Constant>,
    work: usize,
}
impl Evaluator<'_> {
    fn take(&mut self, n: usize) -> Option<()> {
        self.work = self.work.checked_sub(n)?;
        Some(())
    }
    fn call(&mut self, id: FunctionId, args: &[Constant]) -> Option<Constant> {
        self.take(1)?;
        if !self.eligible.contains(&id) || self.active.len() >= MAX_DEPTH {
            return None;
        }
        let f = *self.functions.get(&id)?;
        if args.len() != f.parameters.len() {
            return None;
        }
        if args.is_empty()
            && let Some(value) = self.summaries.get(&id)
        {
            return Some(value.clone());
        }
        if !self.active.insert(id) {
            return None;
        }
        let result = self.body(f, args);
        self.active.remove(&id);
        let value = result?;
        if value.ty() != f.return_type {
            return None;
        }
        if args.is_empty() {
            self.summaries.insert(id, value.clone());
        }
        Some(value)
    }
    fn body(&mut self, f: &TypedFunction, args: &[Constant]) -> Option<Constant> {
        // Charge before allocating; loops and recursive evaluation share one request budget.
        self.take(f.locals.len().checked_add(f.values.len())?)?;
        let mut locals: Vec<Option<Constant>> = vec![None; f.locals.len()];
        let mut values: Vec<Option<Constant>> = vec![None; f.values.len()];
        for (id, value) in f.parameters.iter().zip(args) {
            if f.locals.get(id.0 as usize)?.ty != value.ty() {
                return None;
            }
            *locals.get_mut(id.0 as usize)? = Some(value.clone());
        }
        let mut block = f.entry;
        loop {
            self.take(1)?;
            let b = f.blocks.get(block.0 as usize)?;
            if b.id != block {
                return None;
            }
            for i in &b.instructions {
                self.take(1)?;
                let get = |id: &ValueId| values.get(id.0 as usize).cloned().flatten();
                let value = match &i.kind {
                    InstructionKind::Constant(c) => Some(c.clone()),
                    InstructionKind::Load(id) => Some(locals.get(id.0 as usize)?.clone()?),
                    InstructionKind::Store { local, value } => {
                        let value = get(value)?;
                        if f.locals.get(local.0 as usize)?.ty != value.ty() {
                            return None;
                        }
                        *locals.get_mut(local.0 as usize)? = Some(value);
                        None
                    }
                    InstructionKind::StorageLive(id) | InstructionKind::StorageDead(id) => {
                        *locals.get_mut(id.0 as usize)? = None;
                        None
                    }
                    InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. } => None,
                    InstructionKind::Unary { op, operand } => {
                        let c = get(operand)?;
                        Some(match op {
                            UnaryOp::Negate => checked(integer(&c)?.checked_neg()?, &c.ty())?,
                            UnaryOp::Not => {
                                let Constant::Bool(b) = c else {
                                    return None;
                                };
                                Constant::Bool(!b)
                            }
                            _ => return None,
                        })
                    }
                    InstructionKind::Binary { op, left, right } => {
                        Some(binary(*op, &get(left)?, &get(right)?)?)
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let args = arguments.iter().map(get).collect::<Option<Vec<_>>>()?;
                        let result = self.call(*function, &args)?;
                        if i.result.is_some() {
                            Some(result)
                        } else if result == Constant::Unit {
                            None
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                };
                match (i.result, value) {
                    (Some(id), Some(value)) if f.values.get(id.0 as usize)? == &value.ty() => {
                        *values.get_mut(id.0 as usize)? = Some(value);
                    }
                    (None, None) => {}
                    _ => return None,
                }
            }
            block = match &b.terminator {
                Terminator::Return(Some(id)) => return values.get(id.0 as usize)?.clone(),
                Terminator::Return(None) => return Some(Constant::Unit),
                Terminator::Jump(next) => *next,
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let Some(Constant::Bool(c)) = values.get(condition.0 as usize)? else {
                        return None;
                    };
                    if *c { *then_block } else { *else_block }
                }
                Terminator::Trap | Terminator::Unreachable => return None,
            };
        }
    }
}

pub(super) fn plan(
    functions: &BTreeMap<FunctionId, &TypedFunction>,
) -> BTreeMap<FunctionId, Constant> {
    let mut evaluator = Evaluator {
        functions,
        eligible: functions
            .iter()
            .filter(|(_, f)| eligible(f))
            .map(|(id, _)| *id)
            .collect(),
        active: BTreeSet::new(),
        summaries: BTreeMap::new(),
        work: MAX_WORK,
    };
    for (&id, f) in functions {
        if f.parameters.is_empty() && f.return_type != Type::Unit {
            let _ = evaluator.call(id, &[]);
        }
    }
    evaluator.summaries.retain(|_, c| c != &Constant::Unit);
    evaluator.summaries
}

pub(super) fn emit(f: &TypedFunction, c: &Constant) -> m::Function {
    let (ty, value) = match c {
        Constant::Bool(b) => (m::Ty::Int(1), m::Val::Bool(*b)),
        Constant::I32(n) => (m::Ty::Int(32), m::Val::Int(i128::from(*n))),
        Constant::I64(n) => (m::Ty::Int(64), m::Val::Int(i128::from(*n))),
        Constant::U8(n) => (m::Ty::Int(8), m::Val::Int(i128::from(*n))),
        Constant::U16(n) => (m::Ty::Int(16), m::Val::Int(i128::from(*n))),
        _ => unreachable!("only scalar-return summaries are published"),
    };
    m::Function {
        name: symbol(f.id).to_string(),
        linkage: m::Linkage::External,
        signature: m::Signature::new(ty.clone(), vec![]),
        attrs: vec![],
        blocks: Some(vec![m::Block {
            name: "entry".into(),
            instructions: vec![m::Inst::new(
                None,
                m::Op::Return(Some(m::Arg::new(ty, value))),
            )],
        }]),
    }
}
