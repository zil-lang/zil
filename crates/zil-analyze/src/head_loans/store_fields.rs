//! Exact inline Store headers selected by an address. The normal head loan
//! graph still carries the actual permission and its reborrow ancestry.
use super::*;

pub(crate) fn borrowed_store_field(f: &TypedFunction, address: ValueId, index: u32) -> bool {
    matches!(f.values.get(address.0 as usize), Some(Type::Address { inner, .. })
        if matches!(inner.as_ref(), Type::Package { fields, .. }
            if fields.get(index as usize).is_some_and(|(_, ty)| dynamic_store::is_owned_carrier(ty))))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Shell(HoldRoot),
    Field(HoldRoot, u32),
}
type State = BTreeMap<LocalId, Place>;

fn field(place: Option<Place>) -> Option<Place> {
    place.filter(|p| matches!(p, Place::Field(..)))
}

pub(super) fn solve(
    f: &TypedFunction,
    graph: &(Vec<Vec<usize>>, Vec<bool>),
    bound: &BTreeMap<LocalId, u32>,
    b: &mut Budget,
) -> CompileResult<Vec<Option<LoanRoot>>> {
    fee(b, checked_add(f.values.len(), f.blocks.len())?)?;
    let mut values = vec![None; f.values.len()];
    let mut outputs: Vec<Option<State>> = vec![None; f.blocks.len()];
    let mut seed = State::new();
    fee(b, f.parameters.len())?;
    for (index, local) in f.parameters.iter().enumerate() {
        if matches!(&f.locals[local.0 as usize].ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Package { .. }))
        {
            seed.insert(*local, Place::Shell(HoldRoot::Parameter(index as u32)));
        }
    }

    // A local address or descriptor whose slot is itself addressable can be
    // rebound through an indirect write, including a call. Keep it coarse.
    let mut exposed = BTreeSet::new();
    for block in &f.blocks {
        fee(b, block.instructions.len())?;
        for instruction in &block.instructions {
            if matches!(
                instruction.kind,
                InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::TakeLocal { .. }
            ) {
                continue;
            }
            for local in instruction.kind.local_operands() {
                fee(b, 1)?;
                if matches!(
                    f.locals[local.0 as usize].ty,
                    Type::Address { .. } | Type::Desc(_)
                ) {
                    b.lookup(exposed.len(), "indirectly writable Store address slot")?;
                    exposed.insert(local);
                }
            }
        }
    }
    loop {
        let mut changed = false;
        fee(b, f.blocks.len())?;
        for (index, block) in f.blocks.iter().enumerate() {
            if !graph.1[index] {
                continue;
            }
            let mut incoming = (index == f.entry.0 as usize)
                .then_some(&seed)
                .into_iter()
                .chain(graph.0[index].iter().filter_map(|p| outputs[*p].as_ref()));
            fee(b, graph.0[index].len())?;
            let Some(first) = incoming.next() else {
                continue;
            };
            fee(b, first.len())?;
            let mut locals = first.clone();
            for predecessor in incoming {
                b.lookup_with_width(predecessor.len(), locals.len(), "reaching Store field join")?;
                locals.retain(|local, place| predecessor.get(local) == Some(place));
            }
            fee(b, block.instructions.len())?;
            for instruction in &block.instructions {
                use InstructionKind::*;
                if let Some(result) = instruction.result {
                    let selected = match instruction.kind {
                        DescribePackageField { local, index }
                            if is_owned_store_field(f, local, index) =>
                        {
                            b.lookup(bound.len(), "actual Store field owner")?;
                            Some(Place::Field(owner(local, bound), index))
                        }
                        BorrowLocal { local } | BorrowLocalMut { local }
                            if matches!(f.locals[local.0 as usize].ty, Type::Package { .. }) =>
                        {
                            b.lookup(bound.len(), "actual package capture owner")?;
                            Some(Place::Shell(owner(local, bound)))
                        }
                        Load(local) => {
                            b.lookup(exposed.len(), "addressable Store slot lookup")?;
                            b.lookup(locals.len(), "reaching Store field local")?;
                            if exposed.contains(&local) {
                                None
                            } else {
                                locals.get(&local).copied()
                            }
                        }
                        AddressField { address, index } => match values[address.0 as usize] {
                            Some(Place::Shell(parent))
                                if borrowed_store_field(f, address, index) =>
                            {
                                Some(Place::Field(parent, index))
                            }
                            other => field(other),
                        },
                        ReborrowAddress { address, .. } => values[address.0 as usize],
                        DescribeAddress { address }
                        | AddressSlice { address, .. }
                        | AddressIndex { address, .. } => field(values[address.0 as usize]),
                        OwnedStoreBorrow { store, .. } | OwnedStoreBorrowMut { store, .. } => {
                            field(values[store.0 as usize])
                        }
                        _ => None,
                    };
                    let at = &mut values[result.0 as usize];
                    changed |= *at != selected;
                    *at = selected;
                }
                match instruction.kind {
                    Store { local, value } => {
                        b.lookup(locals.len(), "store actual Store field local")?;
                        if let Some(place) = values[value.0 as usize] {
                            locals.insert(local, place);
                        } else {
                            locals.remove(&local);
                        }
                    }
                    StorageLive(local) | StorageDead(local) | TakeLocal { local } => {
                        b.lookup(locals.len(), "end actual Store field local")?;
                        locals.remove(&local);
                    }
                    _ => {}
                }
            }
            fee(
                b,
                checked_add(outputs[index].as_ref().map_or(0, State::len), locals.len())?,
            )?;
            if outputs[index].as_ref() != Some(&locals) {
                changed = true;
                outputs[index] = Some(locals);
            }
        }
        if !changed {
            break;
        }
    }
    fee(b, values.len())?;
    Ok(values
        .into_iter()
        .map(|place| match place {
            Some(Place::Field(root, index)) => Some(LoanRoot::StoreField(root, index)),
            _ => None,
        })
        .collect())
}
