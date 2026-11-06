//! Reuse private Copy snapshot storage after its final use in an acyclic CFG.
//! Source locals/addresses and owning slots are never pooled. Unknown operations
//! or cycles retain the original independent snapshot allocations.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Storage {
    pub(super) owner: ValueId,
    pub(super) bytes: u64,
    pub(super) align: u32,
}

fn pay(work: &mut usize, amount: usize, span: Span) -> CompileResult<()> {
    *work = work.checked_sub(amount).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy snapshot storage planning budget exceeded",
            Some(span),
        )]
    })?;
    Ok(())
}

// This explicit whitelist is a physical-use inventory, not a proof visitor.
// New operations must be audited here; the wildcard forbids pooling altogether.
fn operands(
    i: &InstructionKind,
    work: &mut usize,
    span: Span,
    mut visit: impl FnMut(ValueId),
) -> CompileResult<bool> {
    use InstructionKind as I;
    match i {
        I::Call { arguments, .. }
        | I::ForeignCall { arguments, .. }
        | I::MakeAggregate { fields: arguments }
        | I::MakeVariant {
            fields: arguments, ..
        } => {
            pay(work, arguments.len().saturating_add(1), span)?;
            for value in arguments {
                visit(*value);
            }
        }
        I::CallIndirect { callee, arguments } => {
            pay(work, arguments.len().saturating_add(2), span)?;
            visit(*callee);
            for value in arguments {
                visit(*value);
            }
        }
        I::Store { value, .. }
        | I::Unary { operand: value, .. }
        | I::ConvertInteger { value, .. }
        | I::ConvertNumber { value, .. }
        | I::ExtractField {
            aggregate: value, ..
        }
        | I::VariantTag { value }
        | I::ReadVariant { value, .. }
        | I::MakeErrorUnion { value, .. }
        | I::ErrorTag { value }
        | I::ReadErrorUnion { value, .. }
        | I::TakeErrorUnion { value, .. }
        | I::ReadAddress { address: value }
        | I::ReborrowAddress { address: value, .. }
        | I::AddressField { address: value, .. }
        | I::SliceLength { address: value }
        | I::DescribeAddress { address: value }
        | I::ReadDesc { desc: value, .. }
        | I::BorrowPlaceAddress { address: value, .. }
        | I::PlaceTagAddress { address: value, .. }
        | I::VariantTagAddress { address: value, .. }
        | I::BorrowVariantAddress { address: value, .. }
        | I::OwnedStorePop { store: value }
        | I::OwnedStoreLen { store: value }
        | I::OwnedStoreCapacity { store: value }
        | I::ByteBufferLen { buffer: value }
        | I::ByteBufferCapacity { buffer: value }
        | I::ByteBufferBorrow { buffer: value, .. } => visit(*value),
        I::Binary { left, right, .. } => {
            visit(*left);
            visit(*right);
        }
        I::ExtractIndex { aggregate, index } => {
            visit(*aggregate);
            visit(*index);
        }
        I::UpdateField {
            aggregate, value, ..
        } => {
            visit(*aggregate);
            visit(*value);
        }
        I::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            visit(*aggregate);
            visit(*index);
            visit(*value);
        }
        I::WriteAddress { address, value }
        | I::WriteDesc {
            desc: address,
            value,
            ..
        } => {
            visit(*address);
            visit(*value);
        }
        I::AddressIndex { address, index } => {
            visit(*address);
            visit(*index);
        }
        I::AddressSlice {
            address,
            start,
            end,
        } => {
            visit(*address);
            for value in start.iter().chain(end) {
                visit(*value);
            }
        }
        I::OwnedStoreReserve { store, capacity } => {
            visit(*store);
            visit(*capacity);
        }
        I::OwnedStorePush { store, value } => {
            visit(*store);
            visit(*value);
        }
        I::OwnedStoreBorrow { store, index } | I::OwnedStoreBorrowMut { store, index } => {
            visit(*store);
            visit(*index);
        }
        I::ByteBufferReserve { buffer, capacity } => {
            visit(*buffer);
            visit(*capacity);
        }
        I::ByteBufferPush { buffer, value } => {
            visit(*buffer);
            visit(*value);
        }
        I::ByteBufferTruncate { buffer, length } => {
            visit(*buffer);
            visit(*length);
        }
        I::ScopeEnter { .. }
        | I::ScopeExit { .. }
        | I::UnsafeEnter { .. }
        | I::UnsafeExit { .. }
        | I::StorageLive(_)
        | I::StorageDead(_)
        | I::Constant(_)
        | I::StaticUtf8 { .. }
        | I::Load(_)
        | I::TakeLocal { .. }
        | I::BorrowLocal { .. }
        | I::BorrowLocalMut { .. }
        | I::BorrowFieldLocal { .. }
        | I::BorrowPlaceLocal { .. }
        | I::PlaceTagLocal { .. }
        | I::VariantTagLocal { .. }
        | I::ReadVariantLocal { .. }
        | I::BorrowVariantLocal { .. }
        | I::DescribeLocal { .. }
        | I::ReadPackageField { .. }
        | I::DescribePackageField { .. }
        | I::DropManaged { .. }
        | I::DropErrorUnion { .. }
        | I::DropEnum { .. }
        | I::DropTuple { .. }
        | I::DropPackage { .. }
        | I::FunctionRef { .. }
        | I::ForeignFunctionRef { .. }
        | I::OwnedStoreNew { .. }
        | I::ByteBufferNew => {}
        _ => return Ok(false),
    }
    Ok(true)
}

pub(super) fn coalesce(
    function: &TypedFunction,
    snapshots: &mut copy_aggregates::Plan,
    work: &mut usize,
) -> CompileResult<()> {
    if snapshots.is_empty() {
        return Ok(());
    }
    // Prepay worst-case topological map/queue work, interval sorting, first-fit
    // comparisons and all retained per-value/per-slot data before allocating.
    let blocks = function.blocks.len();
    let count = snapshots.len();
    let cost = blocks
        .saturating_mul(blocks.saturating_add(1))
        .saturating_mul(8)
        .saturating_add(
            count
                .saturating_mul(count.saturating_add(1))
                .saturating_mul(8),
        )
        .saturating_add(count.saturating_mul(32))
        .saturating_add(128);
    pay(work, cost, function.span)?;
    let indices = function
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect::<BTreeMap<_, _>>();
    let mut edges = vec![[None; 2]; blocks];
    let mut incoming = vec![0usize; blocks];
    for (index, block) in function.blocks.iter().enumerate() {
        let targets = match block.terminator {
            Terminator::Jump(target) => [Some(target), None],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => [Some(then_block), Some(else_block)],
            _ => [None, None],
        };
        for (slot, target) in targets.into_iter().enumerate() {
            if let Some(target) = target {
                let Some(&to) = indices.get(&target) else {
                    return Ok(());
                };
                edges[index][slot] = Some(to);
                incoming[to] += 1;
            }
        }
    }
    let mut ready = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(blocks);
    while let Some(index) = ready.pop_first() {
        order.push(index);
        for to in edges[index].into_iter().flatten() {
            incoming[to] -= 1;
            if incoming[to] == 0 {
                ready.insert(to);
            }
        }
    }
    if order.len() != blocks {
        return Ok(());
    }
    let mut intervals = snapshots
        .keys()
        .map(|id| (*id, None::<(usize, usize)>))
        .collect::<BTreeMap<_, _>>();
    let mut point = 0usize;
    for index in order {
        let block = &function.blocks[index];
        pay(
            work,
            block.instructions.len().saturating_mul(8).saturating_add(8),
            function.span,
        )?;
        for instruction in &block.instructions {
            point += 1;
            if let Some(id) = instruction.result
                && let Some(interval) = intervals.get_mut(&id)
            {
                if interval.is_some() {
                    return Ok(());
                }
                *interval = Some((point, point));
            }
            let mut valid = true;
            let supported = operands(&instruction.kind, work, instruction.span, |id| {
                if let Some(interval) = intervals.get_mut(&id) {
                    match interval {
                        Some((_, end)) => *end = point,
                        None => valid = false,
                    }
                }
            })?;
            if !supported || !valid {
                return Ok(());
            }
        }
        point += 1;
        let value = match block.terminator {
            Terminator::Return(value) => value,
            Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        };
        if let Some(value) = value
            && let Some(interval) = intervals.get_mut(&value)
        {
            match interval {
                Some((_, end)) => *end = point,
                None => return Ok(()),
            }
        }
    }
    let Some(mut intervals) = intervals
        .into_iter()
        .map(|(id, interval)| interval.map(|(start, end)| (start, end, id)))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(());
    };
    intervals.sort_unstable();
    let mut slots = Vec::<(Storage, usize)>::new();
    let mut assigned = Vec::with_capacity(count);
    for (start, end, id) in intervals {
        let snapshot = &snapshots[&id];
        // Closed intervals deliberately keep inputs distinct from the result
        // of their final-use instruction: memcpy never receives overlap.
        let reuse = slots
            .iter()
            .enumerate()
            .filter(|(_, (_, last))| *last < start)
            .min_by_key(|(index, (slot, _))| (snapshot.bytes.saturating_sub(slot.bytes), *index))
            .map(|(i, _)| i);
        let index = match reuse {
            Some(index) => {
                let (slot, last) = &mut slots[index];
                slot.owner = slot.owner.min(id);
                slot.bytes = slot.bytes.max(snapshot.bytes);
                slot.align = slot.align.max(snapshot.align);
                *last = end;
                index
            }
            None => {
                slots.push((
                    Storage {
                        owner: id,
                        bytes: snapshot.bytes,
                        align: snapshot.align,
                    },
                    end,
                ));
                slots.len() - 1
            }
        };
        assigned.push((id, index));
    }
    for (id, index) in assigned {
        snapshots.get_mut(&id).unwrap().storage = Some(slots[index].0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Limits, ResultCategory};
    use zil_ir::{BasicBlock, BlockId, CallKind, PassingMode};

    fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
        Instruction {
            result: result.map(ValueId),
            kind,
            span: Span::new(FileId(0), 0, 1),
        }
    }
    fn function(blocks: Vec<BasicBlock>, values: Vec<Type>) -> TypedFunction {
        TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(0),
            name: "private_snapshot_model".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values,
            blocks,
            entry: BlockId(0),
            span: Span::new(FileId(0), 0, 1),
        }
    }
    fn big(bytes: u32) -> Type {
        Type::Array {
            element: Box::new(Type::U8),
            length: bytes,
        }
    }
    fn snapshots(values: &[Type]) -> copy_aggregates::Plan {
        let mut work = usize::MAX;
        values
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                copy_aggregates::classify(t, &mut work, &Limits::default())
                    .unwrap()
                    .map(|s| (ValueId(i as u32), s))
            })
            .collect()
    }
    fn block(id: u32, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            instructions,
            terminator,
        }
    }

    #[test]
    fn g932_pool_keeps_final_use_inputs_distinct_from_output() {
        let f = function(
            vec![block(
                0,
                vec![
                    instruction(Some(0), InstructionKind::Load(LocalId(0))),
                    instruction(Some(1), InstructionKind::Load(LocalId(1))),
                    instruction(
                        Some(2),
                        InstructionKind::UpdateField {
                            aggregate: ValueId(0),
                            index: 0,
                            value: ValueId(1),
                        },
                    ),
                ],
                Terminator::Return(Some(ValueId(2))),
            )],
            vec![big(32768), big(8192), big(32768)],
        );
        let mut p = snapshots(&f.values);
        let mut work = usize::MAX;
        coalesce(&f, &mut p, &mut work).unwrap();
        let owners = p
            .values()
            .map(|s| s.storage.unwrap().owner)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            owners.len(),
            3,
            "output zero/copy must not overwrite either final-use operand"
        );
    }

    #[test]
    fn g932_pool_sorts_actual_definitions_and_retains_maximum_size_alignment() {
        let aligned = Type::Array {
            element: Box::new(Type::U64),
            length: 1024,
        };
        let f = function(
            vec![block(
                0,
                vec![
                    instruction(Some(1), InstructionKind::Load(LocalId(0))),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(1),
                            value: ValueId(1),
                        },
                    ),
                    instruction(Some(0), InstructionKind::Load(LocalId(2))),
                ],
                Terminator::Return(Some(ValueId(0))),
            )],
            vec![big(32768), aligned],
        );
        let mut p = snapshots(&f.values);
        let mut work = usize::MAX;
        coalesce(&f, &mut p, &mut work).unwrap();
        for s in p.values() {
            let physical = s.storage.unwrap();
            assert_eq!(physical.owner, ValueId(0));
            assert_eq!(physical.bytes, 32768);
            assert_eq!(physical.align, 8);
        }
    }

    #[test]
    fn g932_pool_live_across_diamond_cannot_alias_branch_snapshot() {
        let f = function(
            vec![
                block(
                    0,
                    vec![instruction(Some(0), InstructionKind::Load(LocalId(0)))],
                    Terminator::Branch {
                        condition: ValueId(2),
                        then_block: BlockId(2),
                        else_block: BlockId(1),
                    },
                ),
                block(
                    1,
                    vec![
                        instruction(Some(1), InstructionKind::Load(LocalId(1))),
                        instruction(
                            None,
                            InstructionKind::Store {
                                local: LocalId(2),
                                value: ValueId(1),
                            },
                        ),
                    ],
                    Terminator::Jump(BlockId(3)),
                ),
                block(2, vec![], Terminator::Jump(BlockId(3))),
                block(3, vec![], Terminator::Return(Some(ValueId(0)))),
            ],
            vec![big(8192), big(8192), Type::Bool],
        );
        let mut p = snapshots(&f.values);
        let mut work = usize::MAX;
        coalesce(&f, &mut p, &mut work).unwrap();
        assert_ne!(
            p[&ValueId(0)].storage.unwrap().owner,
            p[&ValueId(1)].storage.unwrap().owner
        );
    }

    #[test]
    fn g932_pool_falls_back_for_cycles_unknown_operations_and_missing_definitions() {
        let cases = [
            function(
                vec![block(
                    0,
                    vec![instruction(Some(0), InstructionKind::Load(LocalId(0)))],
                    Terminator::Jump(BlockId(0)),
                )],
                vec![big(8192)],
            ),
            function(
                vec![block(
                    0,
                    vec![
                        instruction(Some(0), InstructionKind::Load(LocalId(0))),
                        instruction(None, InstructionKind::TaskYield),
                    ],
                    Terminator::Return(Some(ValueId(0))),
                )],
                vec![big(8192)],
            ),
            function(
                vec![block(0, vec![], Terminator::Return(Some(ValueId(0))))],
                vec![big(8192)],
            ),
        ];
        for f in cases {
            let mut p = snapshots(&f.values);
            let mut work = usize::MAX;
            coalesce(&f, &mut p, &mut work).unwrap();
            assert!(p.values().all(|s| s.storage.is_none()));
        }
    }

    #[test]
    fn g932_pool_prepays_exact_work_and_preserves_uncommitted_plans_on_failure() {
        let f = function(
            vec![block(
                0,
                vec![
                    instruction(Some(0), InstructionKind::Load(LocalId(0))),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(1),
                            value: ValueId(0),
                        },
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::Call {
                            function: FunctionId(1),
                            arguments: vec![ValueId(0)],
                        },
                    ),
                ],
                Terminator::Return(Some(ValueId(1))),
            )],
            vec![big(8192), big(8192)],
        );
        let original = snapshots(&f.values);
        let mut p = original.clone();
        let mut work = usize::MAX;
        coalesce(&f, &mut p, &mut work).unwrap();
        let cost = usize::MAX - work;
        for _ in 0..2 {
            let mut fresh = original.clone();
            let mut exact = cost;
            coalesce(&f, &mut fresh, &mut exact).unwrap();
            assert_eq!(exact, 0);
        }
        for mut short in [0, cost - 1] {
            let mut fresh = original.clone();
            assert_eq!(
                coalesce(&f, &mut fresh, &mut short).unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
            assert!(fresh.values().all(|s| s.storage.is_none()));
        }
    }
}
