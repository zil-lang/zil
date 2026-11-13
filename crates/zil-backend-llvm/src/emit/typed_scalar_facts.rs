//! Current-producer-only integer facts. This is not a language capability.
//! Call only after bind_lir matched the complete current VerifiedModule and after
//! the original producer's budget reservation. No facts survive FunctionEmitter.
use std::collections::VecDeque;
use zil_base::{CompileResult, Diagnostic, Phase, TargetProfile};
use zil_ir::{
    BinaryOp, CallKind, Constant, Instruction, InstructionKind as I, PassingMode, Terminator, Type,
    TypedFunction, UnaryOp, ValueId,
};
use zil_llvm_api::model as m;

const FUEL: usize = 1_000_000;
const MAX_BLOCKS: usize = 2048;
const MAX_INSTRUCTIONS: usize = 65_536;
const MAX_VALUES: usize = 65_536;
const MAX_LOCALS: usize = 512;
const MAX_STATE_CELLS: usize = 1_048_576;
struct Fuel(usize);
impl Fuel {
    fn take(&mut self, n: usize) -> Option<()> {
        self.0 = self.0.checked_sub(n)?;
        Some(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Integer {
    bits: u32,
    signed: bool,
}
impl Integer {
    fn of(ty: &Type) -> Option<Self> {
        let (bits, signed) = match ty {
            Type::Bool => (1, false),
            Type::I8 => (8, true),
            Type::I16 => (16, true),
            Type::I32 => (32, true),
            Type::I64 | Type::Isize => (64, true),
            Type::U8 => (8, false),
            Type::U16 => (16, false),
            Type::U32 => (32, false),
            Type::U64 | Type::Usize => (64, false),
            _ => return None,
        };
        Some(Self { bits, signed })
    }
    fn top(self) -> Range {
        Range {
            integer: self,
            lo: if self.signed {
                -(1i128 << (self.bits - 1))
            } else {
                0
            },
            hi: if self.signed {
                (1i128 << (self.bits - 1)) - 1
            } else {
                (1i128 << self.bits) - 1
            },
        }
    }
    fn decode_bits(self, n: u128) -> i128 {
        let n = n & ((1u128 << self.bits) - 1);
        if self.signed && n & (1u128 << (self.bits - 1)) != 0 {
            n as i128 - (1i128 << self.bits)
        } else {
            n as i128
        }
    }
}
fn primitive(ty: &Type) -> bool {
    *ty == Type::Unit || Integer::of(ty).is_some()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    integer: Integer,
    lo: i128,
    hi: i128,
}
impl Range {
    fn singleton(integer: Integer, n: i128) -> Option<Self> {
        let top = integer.top();
        (n >= top.lo && n <= top.hi).then_some(Self {
            integer,
            lo: n,
            hi: n,
        })
    }
    fn union(self, other: Self) -> Option<Self> {
        (self.integer == other.integer).then_some(Self {
            integer: self.integer,
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        })
    }
    fn contains(self, lo: i128, hi: i128) -> bool {
        let top = self.integer.top();
        lo >= top.lo && hi <= top.hi
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    Binary {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    Negate {
        operand: ValueId,
    },
}

/// A private immutable borrow binds the *exact current* CFG, value/local types,
/// modes/signature and target. No hash, Analyze claim, mutable file or cache key
/// can create this value. It is deliberately neither Clone nor serializable.
pub(super) struct Facts<'a> {
    source: &'a TypedFunction,
    target: &'a TargetProfile,
    safe: Vec<Option<Site>>,
    ssa: Option<ScalarSsa>,
}
impl Facts<'_> {
    pub(super) fn binary(
        &self,
        source: &TypedFunction,
        target: &TargetProfile,
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
        result: ValueId,
    ) -> bool {
        std::ptr::eq(self.source, source)
            && std::ptr::eq(self.target, target)
            && self.safe.get(result.0 as usize).copied().flatten()
                == Some(Site::Binary { op, left, right })
    }
    pub(super) fn negate(
        &self,
        source: &TypedFunction,
        target: &TargetProfile,
        operand: ValueId,
        result: ValueId,
    ) -> bool {
        std::ptr::eq(self.source, source)
            && std::ptr::eq(self.target, target)
            && self.safe.get(result.0 as usize).copied().flatten() == Some(Site::Negate { operand })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Operand {
    Argument(usize),
    Value(ValueId),
    Phi { block: usize, local: usize },
    Unit,
}
impl Operand {
    fn machine(&self) -> m::Val {
        match self {
            Self::Argument(index) => m::local(format!("arg.{index}")),
            Self::Value(value) => m::local(format!("v.{}", value.0)),
            Self::Phi { block, local } => m::local(format!("typed.phi.{block}.{local}")),
            Self::Unit => m::Val::Zero,
        }
    }
}
struct SourcePhi {
    local: usize,
    incoming: Vec<(Operand, usize)>,
}
struct ScalarSsa {
    aliases: Vec<Option<Operand>>,
    phis: Vec<Vec<SourcePhi>>,
}
struct SsaBuilder {
    output: ScalarSsa,
    outgoing: Vec<Option<Vec<Option<Operand>>>>,
    current: Vec<Option<Operand>>,
    fuel: Fuel,
}
impl SsaBuilder {
    fn new(source: &TypedFunction, graph: &Graph) -> Option<Self> {
        // No unreachable component may retain an unbound removed Load result.
        if graph.reachable.iter().any(|reachable| !reachable) {
            return None;
        }
        let mut current = vec![None; source.locals.len()];
        for (argument, id) in source.parameters.iter().enumerate() {
            current[id.0 as usize] = Some(if source.locals[id.0 as usize].ty == Type::Unit {
                Operand::Unit
            } else {
                Operand::Argument(argument)
            });
        }
        Some(Self {
            output: ScalarSsa {
                aliases: vec![None; source.values.len()],
                phis: (0..source.blocks.len()).map(|_| vec![]).collect(),
            },
            outgoing: vec![None; source.blocks.len()],
            current,
            fuel: Fuel(FUEL),
        })
    }
    fn enter(&mut self, source: &TypedFunction, graph: &Graph, b: usize) -> Option<()> {
        self.fuel.take(source.locals.len().checked_add(1)?)?;
        if b == source.entry.0 as usize {
            return Some(());
        }
        let first = *graph.predecessors[b].first()?;
        let mut state = self.outgoing[first].as_ref()?.clone();
        for local in 0..state.len() {
            let first = state[local].clone();
            let mut same = true;
            let mut initialized = first.is_some();
            let mut incoming = vec![];
            for &pred in &graph.predecessors[b] {
                self.fuel.take(1)?;
                let value = self.outgoing[pred].as_ref()?.get(local)?.clone();
                same &= value == first;
                initialized &= value.is_some();
                if let Some(value) = value {
                    incoming.push((value, pred));
                }
            }
            if !initialized {
                state[local] = None;
            } else if !same {
                self.output.phis[b].push(SourcePhi { local, incoming });
                state[local] = Some(Operand::Phi { block: b, local });
            }
        }
        self.current = state;
        Some(())
    }
    fn operand(&self, source: &TypedFunction, value: ValueId) -> Option<Operand> {
        if *source.values.get(value.0 as usize)? == Type::Unit {
            return Some(Operand::Unit);
        }
        Some(
            self.output
                .aliases
                .get(value.0 as usize)?
                .clone()
                .unwrap_or(Operand::Value(value)),
        )
    }
    fn instruction(&mut self, source: &TypedFunction, inst: &Instruction) -> Option<()> {
        self.fuel.take(1)?;
        match inst.kind {
            I::Load(local) => {
                let value = self.current.get(local.0 as usize)?.clone()?;
                *self.output.aliases.get_mut(inst.result?.0 as usize)? = Some(value);
            }
            I::Store { local, value } => {
                self.current[local.0 as usize] = Some(self.operand(source, value)?);
            }
            I::StorageLive(local) | I::StorageDead(local) => {
                self.current[local.0 as usize] = None;
            }
            _ => {}
        }
        Some(())
    }
    fn exit(&mut self, b: usize) {
        self.outgoing[b] = Some(std::mem::take(&mut self.current));
    }
}
impl Facts<'_> {
    pub(super) fn scalar_ssa(&self) -> bool {
        self.ssa.is_some()
    }
    pub(super) fn retain_scalar_ssa(&mut self, eligible: bool) {
        if !eligible {
            self.ssa = None;
        }
    }
    fn source_binding(&self, source: &TypedFunction, target: &TargetProfile) -> bool {
        std::ptr::eq(self.source, source) && std::ptr::eq(self.target, target)
    }
    pub(super) fn finish_scalar_ssa(
        &self,
        source: &TypedFunction,
        target: &TargetProfile,
        items: &mut [m::Item],
        exits: &[Option<String>],
    ) -> CompileResult<bool> {
        let Some(plan) = &self.ssa else {
            return Ok(false);
        };
        let fail = || {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-SCALAR-SSA",
                Some(source.span),
                "current scalar producer did not complete its bound SSA plan",
            )]
        };
        if !self.source_binding(source, target)
            || exits.len() != source.blocks.len()
            || exits.iter().any(Option::is_none)
        {
            return Err(fail());
        }
        // This complete primitive producer emits exactly its own function.
        // Helpers/foreign/aggregate/cleanup domains were refused before output.
        if items.len() != 1 {
            return Err(fail());
        }
        let m::Item::Function(machine) = &mut items[0] else {
            return Err(fail());
        };
        if machine.name != super::symbol(source.id) {
            return Err(fail());
        }
        let blocks = machine.blocks.as_mut().ok_or_else(fail)?;
        let labels = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.name.clone(), i))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (b, phis) in plan.phis.iter().enumerate() {
            if phis.is_empty() {
                continue;
            }
            let block = *labels.get(&format!("bb.{b}")).ok_or_else(fail)?;
            let mut prefix = Vec::with_capacity(phis.len() + blocks[block].instructions.len());
            for phi in phis {
                let incoming = phi
                    .incoming
                    .iter()
                    .map(|(value, pred)| {
                        let label = exits[*pred].as_ref().ok_or_else(fail)?;
                        if !labels.contains_key(label) {
                            return Err(fail());
                        }
                        Ok((value.machine(), label.clone()))
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                prefix.push(m::Inst::new(
                    Some(format!("typed.phi.{b}.{}", phi.local)),
                    m::Op::Phi {
                        ty: super::llvm_type(&source.locals[phi.local].ty)?,
                        incoming,
                    },
                ));
            }
            prefix.append(&mut blocks[block].instructions);
            blocks[block].instructions = prefix;
        }
        // Complete operand traversal, including casts, calls, conditions and
        // PHI inputs. Definitions/results are NEVER renamed. The source worklist
        // resolved aliases before emission, so this is one nonrecursive lookup.
        fn rewrite(value: &mut m::Val, aliases: &[Option<Operand>]) {
            match value {
                m::Val::Local(name) => {
                    if let Some(number) = name
                        .strip_prefix("v.")
                        .and_then(|n| n.parse::<usize>().ok())
                        && let Some(Some(alias)) = aliases.get(number)
                    {
                        *value = alias.machine();
                    }
                }
                m::Val::Array(args) => {
                    for arg in args {
                        rewrite(&mut arg.value, aliases);
                    }
                }
                m::Val::Gep { ptr, indices, .. } => {
                    rewrite(ptr, aliases);
                    for arg in indices {
                        rewrite(&mut arg.value, aliases);
                    }
                }
                _ => {}
            }
        }
        for block in blocks.iter_mut() {
            for inst in &mut block.instructions {
                // A leftover slot access cannot be authorized by a partial plan.
                if matches!(
                    inst.op,
                    m::Op::Alloca { .. }
                        | m::Op::Load { .. }
                        | m::Op::Store { .. }
                        | m::Op::AtomicLoad { .. }
                        | m::Op::AtomicStore { .. }
                        | m::Op::AtomicRmw { .. }
                        | m::Op::AtomicCompareExchange { .. }
                ) {
                    return Err(fail());
                }
                super::early_scalar::map_operands(&mut inst.op, &mut |value| {
                    rewrite(value, &plan.aliases)
                });
            }
        }
        Ok(true)
    }
}

struct Graph {
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    reachable: Vec<bool>,
    order: Vec<usize>,
    // Exact source value ownership; never allocate from an untrusted largest ID.
    definitions: Vec<(usize, usize)>,
    dominators: Vec<Vec<u64>>,
}
impl Graph {
    fn new(source: &TypedFunction, fuel: &mut Fuel) -> Option<Self> {
        let count = source.blocks.len();
        let entry = source.entry.0 as usize;
        if count == 0
            || count > MAX_BLOCKS
            || entry >= count
            || source.values.len() > MAX_VALUES
            || source.locals.len() > MAX_LOCALS
            || count.checked_mul(source.locals.len())? > MAX_STATE_CELLS
        {
            return None;
        }
        let mut instructions = 0usize;
        let mut definitions = vec![None; source.values.len()];
        let mut successors = vec![vec![]; count];
        let mut predecessors = vec![vec![]; count];
        for (b, block) in source.blocks.iter().enumerate() {
            fuel.take(1)?;
            if block.id.0 as usize != b {
                return None;
            }
            instructions = instructions.checked_add(block.instructions.len())?;
            if instructions > MAX_INSTRUCTIONS {
                return None;
            }
            for (i, inst) in block.instructions.iter().enumerate() {
                fuel.take(1)?;
                if let Some(id) = inst.result {
                    let definition = definitions.get_mut(id.0 as usize)?;
                    if definition.replace((b, i)).is_some() {
                        return None;
                    }
                }
            }
            let mut edge = |id: zil_ir::BlockId| -> Option<()> {
                let next = id.0 as usize;
                if next >= count {
                    return None;
                }
                successors[b].push(next);
                predecessors[next].push(b);
                Some(())
            };
            match block.terminator {
                Terminator::Jump(next) => edge(next)?,
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    edge(then_block)?;
                    edge(else_block)?;
                }
                Terminator::Return(_) | Terminator::Trap => {}
                // Preserve every unsupported/invalid domain on the old path.
                Terminator::Unreachable => return None,
            }
        }
        let definitions = definitions.into_iter().collect::<Option<Vec<_>>>()?;
        if !predecessors[entry].is_empty() {
            return None;
        }
        // Reject every cycle, including unreachable cycles. No optimistic
        // initialization, loop iteration, widening or partial proof is used.
        let mut remaining = predecessors.iter().map(Vec::len).collect::<Vec<_>>();
        let mut queue = remaining
            .iter()
            .enumerate()
            .filter_map(|(b, &n)| (n == 0).then_some(b))
            .collect::<VecDeque<_>>();
        let mut order = vec![];
        while let Some(b) = queue.pop_front() {
            fuel.take(1)?;
            order.push(b);
            for &next in &successors[b] {
                fuel.take(1)?;
                remaining[next] = remaining[next].checked_sub(1)?;
                if remaining[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        if order.len() != count {
            return None;
        }
        let mut reachable = vec![false; count];
        let mut work = vec![entry];
        while let Some(b) = work.pop() {
            fuel.take(1)?;
            if std::mem::replace(&mut reachable[b], true) {
                continue;
            }
            work.extend(successors[b].iter().copied());
        }
        let words = count.div_ceil(64);
        if count.checked_mul(words)? > MAX_STATE_CELLS {
            return None;
        }
        let mut dominators = vec![vec![0u64; words]; count];
        for &b in &order {
            if !reachable[b] {
                continue;
            }
            if b != entry {
                let mut incoming = predecessors[b].iter().copied().filter(|&p| reachable[p]);
                let first = incoming.next()?;
                dominators[b] = dominators[first].clone();
                fuel.take(words)?;
                for pred in incoming {
                    let (current, inherited) = if pred < b {
                        let (before, after) = dominators.split_at_mut(b);
                        (&mut after[0], &before[pred])
                    } else if pred > b {
                        let (before, after) = dominators.split_at_mut(pred);
                        (&mut before[b], &after[0])
                    } else {
                        return None;
                    };
                    for (word, inherited) in current.iter_mut().zip(inherited) {
                        fuel.take(1)?;
                        *word &= inherited;
                    }
                }
            }
            dominators[b][b / 64] |= 1u64 << (b % 64);
        }
        Some(Self {
            successors,
            predecessors,
            reachable,
            order,
            definitions,
            dominators,
        })
    }
    fn value(&self, id: ValueId, b: usize, position: usize) -> Option<()> {
        let &(owner, defined) = self.definitions.get(id.0 as usize)?;
        if !self.reachable[b] {
            return Some(());
        }
        if owner == b {
            return (defined < position).then_some(());
        }
        (self.reachable[owner] && self.dominators[b][owner / 64] & (1u64 << (owner % 64)) != 0)
            .then_some(())
    }
}
fn uses(inst: &Instruction) -> Option<[Option<ValueId>; 2]> {
    Some(match inst.kind {
        I::Constant(_)
        | I::Load(_)
        | I::StorageLive(_)
        | I::StorageDead(_)
        | I::ScopeEnter { .. }
        | I::ScopeExit { .. } => [None, None],
        I::Store { value, .. } => [Some(value), None],
        I::Unary {
            op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
            operand,
        } => [Some(operand), None],
        I::Binary {
            op:
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::AddWrapping
                | BinaryOp::SubtractWrapping
                | BinaryOp::MultiplyWrapping
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor,
            left,
            right,
        } => [Some(left), Some(right)],
        I::ConvertInteger { value, .. } => [Some(value), None],
        // Includes ALL calls/foreign/indirect/ref/projection/capture/atomic/
        // ownership/async/unknown effects, regardless of their claimed result ty.
        _ => return None,
    })
}
fn value_type(source: &TypedFunction, id: ValueId) -> Option<&Type> {
    source.values.get(id.0 as usize)
}
fn result_type<'a>(source: &'a TypedFunction, inst: &Instruction) -> Option<&'a Type> {
    value_type(source, inst.result?)
}
fn admit(source: &TypedFunction, target: &TargetProfile, fuel: &mut Fuel) -> Option<()> {
    if !target.is_supported_x64_llvm()
        || source.blocks.is_empty()
        || source.blocks.len() > MAX_BLOCKS
        || source.locals.len() > MAX_LOCALS
        || source.values.len() > MAX_VALUES
    {
        return None;
    }
    if source.call_kind != CallKind::Direct
        || source.environment.is_some()
        || source.foreign.is_some()
        || source.storage_life.is_some()
        || source.reference_return_bound.is_some()
        || source.callable_return_contract.is_some()
        || source.callable_contract.is_some()
        || source.protocol_contract.is_some()
        || !matches!(source.result_mode, PassingMode::Infer | PassingMode::Copy)
        || !primitive(&source.return_type)
    {
        return None;
    }
    // Primitive Type is the resolved ordinary by-value ABI. Address, Desc,
    // Managed, BorrowedStorage and all function/environment forms are rejected;
    // there is no per-parameter PassingMode field in current TypedFunction.
    for (id, local) in source.locals.iter().enumerate() {
        fuel.take(1)?;
        if local.id.0 as usize != id || !primitive(&local.ty) {
            return None;
        }
    }
    for ty in &source.values {
        fuel.take(1)?;
        if !primitive(ty) {
            return None;
        }
    }
    let mut parameters = vec![false; source.locals.len()];
    for id in &source.parameters {
        fuel.take(1)?;
        let seen = parameters.get_mut(id.0 as usize)?;
        if std::mem::replace(seen, true) {
            return None;
        }
    }
    let mut count = 0usize;
    for block in &source.blocks {
        fuel.take(1)?;
        count = count.checked_add(block.instructions.len())?;
        if count > MAX_INSTRUCTIONS {
            return None;
        }
        for inst in &block.instructions {
            fuel.take(1)?;
            let _ = uses(inst)?;
        }
    }
    Some(())
}
fn validate(source: &TypedFunction, graph: &Graph, fuel: &mut Fuel) -> Option<()> {
    for (b, block) in source.blocks.iter().enumerate() {
        for (i, inst) in block.instructions.iter().enumerate() {
            fuel.take(1)?;
            for value in uses(inst)?.into_iter().flatten() {
                graph.value(value, b, i)?;
                value_type(source, value)?;
            }
            match &inst.kind {
                I::Constant(value) => {
                    if value.ty() != *result_type(source, inst)? || !primitive(&value.ty()) {
                        return None;
                    }
                }
                I::Load(local) => {
                    if source.locals.get(local.0 as usize)?.ty != *result_type(source, inst)? {
                        return None;
                    }
                }
                I::Store { local, value } => {
                    if inst.result.is_some()
                        || source.locals.get(local.0 as usize)?.ty != *value_type(source, *value)?
                    {
                        return None;
                    }
                }
                I::StorageLive(local) | I::StorageDead(local) => {
                    source.locals.get(local.0 as usize)?;
                    if inst.result.is_some() {
                        return None;
                    }
                }
                I::ScopeEnter { .. } | I::ScopeExit { .. } => {
                    if inst.result.is_some() {
                        return None;
                    }
                }
                I::Unary { op, operand } => {
                    let ty = value_type(source, *operand)?;
                    if ty != result_type(source, inst)? {
                        return None;
                    }
                    match op {
                        UnaryOp::Not if *ty == Type::Bool => {}
                        UnaryOp::Negate | UnaryOp::BitNot if ty.is_integer() => {}
                        _ => return None,
                    }
                }
                I::Binary { op, left, right } => {
                    let ty = value_type(source, *left)?;
                    if ty != value_type(source, *right)? {
                        return None;
                    }
                    let result = result_type(source, inst)?;
                    match op {
                        BinaryOp::Equal | BinaryOp::NotEqual
                            if primitive(ty) && *result == Type::Bool => {}
                        BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                            if ty.is_integer() && *result == Type::Bool => {}
                        BinaryOp::And | BinaryOp::Or
                            if *ty == Type::Bool && *result == Type::Bool => {}
                        BinaryOp::Add
                        | BinaryOp::Subtract
                        | BinaryOp::Multiply
                        | BinaryOp::Divide
                        | BinaryOp::Remainder
                        | BinaryOp::AddWrapping
                        | BinaryOp::SubtractWrapping
                        | BinaryOp::MultiplyWrapping
                        | BinaryOp::BitAnd
                        | BinaryOp::BitOr
                        | BinaryOp::BitXor
                            if ty.is_integer() && result == ty => {}
                        _ => return None,
                    }
                }
                I::ConvertInteger { value, target } => {
                    if !target.is_integer()
                        || !value_type(source, *value)?.is_integer()
                        || result_type(source, inst)? != target
                    {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        match block.terminator {
            Terminator::Return(Some(value)) => {
                graph.value(value, b, usize::MAX)?;
                if value_type(source, value)? != &source.return_type {
                    return None;
                }
            }
            Terminator::Return(None) if source.return_type == Type::Unit => {}
            Terminator::Branch { condition, .. } => {
                graph.value(condition, b, usize::MAX)?;
                if *value_type(source, condition)? != Type::Bool {
                    return None;
                }
            }
            Terminator::Jump(_) | Terminator::Trap => {}
            _ => return None,
        }
    }
    Some(())
}
fn constant(value: &Constant) -> Option<Range> {
    let integer = Integer::of(&value.ty())?;
    let value = match value {
        Constant::Bool(n) => i128::from(*n),
        Constant::I8(n) => i128::from(*n),
        Constant::I16(n) => i128::from(*n),
        Constant::I32(n) => i128::from(*n),
        Constant::I64(n) | Constant::Isize(n) => i128::from(*n),
        Constant::U8(n) => i128::from(*n),
        Constant::U16(n) => i128::from(*n),
        Constant::U32(n) => i128::from(*n),
        Constant::U64(n) | Constant::Usize(n) => i128::from(*n),
        _ => return None,
    };
    Range::singleton(integer, value)
}
fn endpoints(op: BinaryOp, a: Range, b: Range) -> Option<(i128, i128)> {
    let mut lo = i128::MAX;
    let mut hi = i128::MIN;
    for x in [a.lo, a.hi] {
        for y in [b.lo, b.hi] {
            let n = match op {
                BinaryOp::Add | BinaryOp::AddWrapping => x.checked_add(y)?,
                BinaryOp::Subtract | BinaryOp::SubtractWrapping => x.checked_sub(y)?,
                BinaryOp::Multiply | BinaryOp::MultiplyWrapping => x.checked_mul(y)?,
                _ => return None,
            };
            lo = lo.min(n);
            hi = hi.max(n);
        }
    }
    Some((lo, hi))
}
fn arithmetic(op: BinaryOp, a: Range, b: Range) -> (Range, bool) {
    if let Some((lo, hi)) = endpoints(op, a, b)
        && a.contains(lo, hi)
    {
        return (
            Range {
                integer: a.integer,
                lo,
                hi,
            },
            true,
        );
    }
    (a.integer.top(), false)
}
fn binary(op: BinaryOp, a: Range, b: Range) -> Option<(Range, bool)> {
    if a.integer != b.integer {
        return None;
    }
    let integer = a.integer;
    Some(match op {
        BinaryOp::Add
        | BinaryOp::Subtract
        | BinaryOp::Multiply
        | BinaryOp::AddWrapping
        | BinaryOp::SubtractWrapping
        | BinaryOp::MultiplyWrapping => arithmetic(op, a, b),
        BinaryOp::Remainder if b.lo == b.hi && b.lo > 0 => {
            let bound = b.lo - 1;
            (
                Range {
                    integer,
                    lo: if a.lo >= 0 { 0 } else { a.lo.max(-bound) },
                    hi: if a.hi <= 0 { 0 } else { a.hi.min(bound) },
                },
                false,
            )
        }
        BinaryOp::Divide if b.lo == b.hi && b.lo > 0 => (
            Range {
                integer,
                lo: a.lo / b.lo,
                hi: a.hi / b.lo,
            },
            false,
        ),
        BinaryOp::Remainder | BinaryOp::Divide => (integer.top(), false),
        BinaryOp::BitAnd if b.lo == b.hi && b.lo >= 0 => (
            Range {
                integer,
                lo: 0,
                hi: b.hi,
            },
            false,
        ),
        BinaryOp::BitAnd if a.lo == a.hi && a.lo >= 0 => (
            Range {
                integer,
                lo: 0,
                hi: a.hi,
            },
            false,
        ),
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor if a.lo == a.hi && b.lo == b.hi => {
            let (a, b) = (a.lo as u128, b.lo as u128);
            let n = match op {
                BinaryOp::BitAnd => a & b,
                BinaryOp::BitOr => a | b,
                _ => a ^ b,
            };
            (Range::singleton(integer, integer.decode_bits(n))?, false)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => (integer.top(), false),
        BinaryOp::And
        | BinaryOp::Or
        | BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::LessEqual
        | BinaryOp::Greater
        | BinaryOp::GreaterEqual => (Integer::of(&Type::Bool)?.top(), false),
        _ => return None,
    })
}
fn transfer(
    source: &TypedFunction,
    block: usize,
    state: &mut [Option<Range>],
    values: &mut [Option<Range>],
    safe: &mut [Option<Site>],
    fuel: &mut Fuel,
    ssa: &mut Option<SsaBuilder>,
) -> Option<()> {
    for inst in &source.blocks[block].instructions {
        if let Some(builder) = ssa.as_mut()
            && builder.instruction(source, inst).is_none()
        {
            *ssa = None;
        }
        fuel.take(1)?;
        let value = match &inst.kind {
            I::Constant(value) => constant(value),
            I::Load(local) => *state.get(local.0 as usize)?,
            I::Store { local, value } => {
                *state.get_mut(local.0 as usize)? = *values.get(value.0 as usize)?;
                None
            }
            I::StorageLive(local) | I::StorageDead(local) => {
                *state.get_mut(local.0 as usize)? = None;
                None
            }
            I::ScopeEnter { .. } | I::ScopeExit { .. } => None,
            I::Unary { op, operand } => {
                if let Some(input) = *values.get(operand.0 as usize)? {
                    match op {
                        UnaryOp::Negate => {
                            let zero = Range::singleton(input.integer, 0)?;
                            let (output, proved) = arithmetic(BinaryOp::Subtract, zero, input);
                            if proved {
                                *safe.get_mut(inst.result?.0 as usize)? =
                                    Some(Site::Negate { operand: *operand });
                            }
                            Some(output)
                        }
                        UnaryOp::Not => Some(Range {
                            integer: input.integer,
                            lo: 1 - input.hi,
                            hi: 1 - input.lo,
                        }),
                        UnaryOp::BitNot if input.lo == input.hi => Range::singleton(
                            input.integer,
                            input.integer.decode_bits(!(input.lo as u128)),
                        ),
                        UnaryOp::BitNot => Some(input.integer.top()),
                        _ => return None,
                    }
                } else {
                    None
                }
            }
            I::Binary { op, left, right } => {
                if let (Some(a), Some(b)) = (
                    *values.get(left.0 as usize)?,
                    *values.get(right.0 as usize)?,
                ) {
                    let (output, proved) = binary(*op, a, b)?;
                    if proved
                        && matches!(op, BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply)
                    {
                        *safe.get_mut(inst.result?.0 as usize)? = Some(Site::Binary {
                            op: *op,
                            left: *left,
                            right: *right,
                        });
                    }
                    Some(output)
                } else {
                    None
                }
            }
            I::ConvertInteger { value, target } => {
                if let Some(input) = *values.get(value.0 as usize)? {
                    let top = Integer::of(target)?.top();
                    let (lo, hi) = (input.lo.max(top.lo), input.hi.min(top.hi));
                    (lo <= hi).then_some(Range {
                        integer: top.integer,
                        lo,
                        hi,
                    })
                } else {
                    None
                }
            }
            _ => return None,
        };
        if let Some(id) = inst.result {
            *values.get_mut(id.0 as usize)? = value;
        }
    }
    Some(())
}
fn analyze<'a>(
    source: &'a TypedFunction,
    target: &'a TargetProfile,
    remaining: usize,
) -> Option<Facts<'a>> {
    let mut fuel = Fuel(remaining);
    admit(source, target, &mut fuel)?;
    let graph = Graph::new(source, &mut fuel)?;
    validate(source, &graph, &mut fuel)?;
    let count = source.blocks.len();
    let entry = source.entry.0 as usize;
    let mut incoming = vec![None::<Vec<Option<Range>>>; count];
    let mut initial = vec![None; source.locals.len()];
    for id in &source.parameters {
        initial[id.0 as usize] = Integer::of(&source.locals[id.0 as usize].ty).map(Integer::top);
    }
    incoming[entry] = Some(initial);
    let mut remaining = graph
        .predecessors
        .iter()
        .map(|preds| preds.iter().filter(|&&p| graph.reachable[p]).count())
        .collect::<Vec<_>>();
    let mut queue = VecDeque::from([entry]);
    let mut processed = vec![false; count];
    let mut values = vec![None; source.values.len()];
    let mut safe = vec![None; source.values.len()];
    let mut ssa = SsaBuilder::new(source, &graph);
    while let Some(b) = queue.pop_front() {
        fuel.take(1)?;
        if std::mem::replace(&mut processed[b], true) {
            return None;
        }
        let mut state = incoming[b].take()?;
        if let Some(builder) = ssa.as_mut()
            && builder.enter(source, &graph, b).is_none()
        {
            ssa = None;
        }
        transfer(
            source,
            b,
            &mut state,
            &mut values,
            &mut safe,
            &mut fuel,
            &mut ssa,
        )?;
        if let Some(builder) = ssa.as_mut() {
            builder.exit(b);
        }
        for &next in &graph.successors[b] {
            fuel.take(state.len().checked_add(1)?)?;
            if let Some(previous) = &mut incoming[next] {
                for (old, new) in previous.iter_mut().zip(&state) {
                    *old = match (*old, *new) {
                        (Some(a), Some(b)) => Some(a.union(b)?),
                        _ => None,
                    };
                }
            } else {
                incoming[next] = Some(state.clone());
            }
            remaining[next] = remaining[next].checked_sub(1)?;
            if remaining[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if graph
        .order
        .iter()
        .any(|&b| graph.reachable[b] && !processed[b])
    {
        return None;
    }
    Some(Facts {
        source,
        target,
        safe,
        ssa: ssa.map(|builder| builder.output),
    })
}
/// Complete admission and dataflow succeed before any omission is granted.
/// Unsupported shape, malformed identities, loops or optional fuel exhaustion
/// return None; the caller emits every original checked operation unchanged.
pub(super) fn prepare<'a>(
    source: &'a TypedFunction,
    target: &'a TargetProfile,
) -> Option<Facts<'a>> {
    analyze(source, target, FUEL)
}

#[cfg(test)]
#[path = "typed_scalar_facts_tests.rs"]
mod tests;
