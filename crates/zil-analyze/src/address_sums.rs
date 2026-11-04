//! Active tags of actual addressed Sums. This pass consumes raw CFG operations;
//! it never treats a copied enum value or a producer claim as a place proof.
use super::*;
type Path = Vec<(u32, u32)>;
type Fact = (ValueId, Path, u32);
#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    facts: BTreeSet<Fact>,
    observed: BTreeSet<ValueId>,
}
impl State {
    fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(
            self.facts
                .iter()
                .map(|(_, p, _)| p.len() + 1)
                .sum::<usize>()
                + self.observed.len()
                + 1,
            "address Sum state snapshot",
        )?;
        Ok(self.clone())
    }
    fn meet(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.facts.len() + other.facts.len() + self.observed.len() + other.observed.len() + 1,
            "address Sum must join",
        )?;
        b.spend(
            self.facts
                .iter()
                .map(|(_, p, _)| p.len().saturating_add(1))
                .sum::<usize>()
                .saturating_mul(
                    other
                        .facts
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2),
                ),
            "address Sum fact join comparison",
        )?;
        let old = (self.facts.len(), self.observed.len());
        self.facts.retain(|f| other.facts.contains(f));
        self.observed.retain(|v| other.observed.contains(v));
        Ok(old != (self.facts.len(), self.observed.len()))
    }
}
fn bad_path() -> Vec<Diagnostic> {
    bad("addressed Enum payload has no current actual tag authority")
}
fn endpoint<'a>(
    f: &'a TypedFunction,
    address: ValueId,
    path: &[VariantProjection],
    b: &mut Budget,
) -> CompileResult<(bool, &'a Type)> {
    let Some(Type::Address { mutable, inner }) = f.values.get(address.0 as usize) else {
        return Err(bad_path());
    };
    let mut ty = inner.as_ref();
    b.spend(path.len() + 1, "address Sum projection")?;
    for part in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(bad_path());
        };
        ty = variants
            .get(part.variant as usize)
            .and_then(|(_, fs)| fs.get(part.index as usize))
            .ok_or_else(bad_path)?;
    }
    Ok((*mutable, ty))
}
fn require(
    state: &State,
    address: ValueId,
    path: &[VariantProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    let mut prefix = Vec::new();
    for p in path {
        b.spend(
            prefix.len().saturating_add(1).saturating_mul(
                state
                    .facts
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2),
            ),
            "address Sum active prefix",
        )?;
        if !state.facts.contains(&(address, prefix.clone(), p.variant)) {
            return Err(bad_path());
        }
        prefix.push((p.variant, p.index));
    }
    Ok(())
}
fn clobbers(i: &InstructionKind) -> bool {
    !matches!(
        i,
        InstructionKind::Constant(_)
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ConvertInteger { .. }
            | InstructionKind::Load(_)
            | InstructionKind::ReadCapture { .. }
            | InstructionKind::ReadAddress { .. }
            | InstructionKind::AddressField { .. }
            | InstructionKind::AddressIndex { .. }
            | InstructionKind::AddressSlice { .. }
            | InstructionKind::ReborrowAddress { .. }
            | InstructionKind::BorrowLocal { .. }
            | InstructionKind::BorrowVariantLocal { mutable: false, .. }
            | InstructionKind::BorrowFieldLocal { mutable: false, .. }
            | InstructionKind::VariantTagLocal { .. }
            | InstructionKind::ReadVariantLocal { .. }
    )
}

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.addressed_sums {
        return Ok(());
    }
    let mut present = false;
    b.spend(f.blocks.len(), "address Sum feature scan")?;
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1, "address Sum instruction scan")?;
            present |= matches!(
                i.kind,
                InstructionKind::BorrowFieldLocal { .. }
                    | InstructionKind::VariantTagAddress { .. }
                    | InstructionKind::BorrowVariantAddress { .. }
            );
        }
    }
    if !present {
        return Ok(());
    }
    b.spend(f.values.len() + f.blocks.len() * 2, "address Sum tables")?;
    let mut definitions = vec![None; f.values.len()];
    for block in &f.blocks {
        for i in &block.instructions {
            if let Some(v) = i.result {
                definitions[v.0 as usize] = Some(&i.kind)
            }
            if let InstructionKind::BorrowFieldLocal {
                local,
                path,
                mutable,
            } = &i.kind
            {
                let declaration = f.locals.get(local.0 as usize).ok_or_else(bad_path)?;
                let mut ty = &declaration.ty;
                b.type_work(ty, true, "inline field root shape")?;
                if path.is_empty() || !ty.is_copy_value() || (*mutable && !declaration.mutable) {
                    return Err(bad_path());
                }
                for field in path {
                    b.spend(1, "inline field path")?;
                    ty = match ty {
                        Type::Tuple(fs) => fs.get(*field as usize),
                        Type::Record { fields, .. } => fields.get(*field as usize).map(|(_, t)| t),
                        _ => None,
                    }
                    .ok_or_else(bad_path)?;
                }
                let claim = i.result.and_then(|v| f.values.get(v.0 as usize));
                if !matches!(claim,Some(Type::Address{mutable:m,inner}) if m==mutable && inner.as_ref()==ty)
                {
                    return Err(bad_path());
                }
            }
            if let InstructionKind::VariantTagAddress { address, path }
            | InstructionKind::BorrowVariantAddress { address, path, .. } = &i.kind
            {
                if path.len() > limits.max_call_depth {
                    return Err(vec![Diagnostic::limit(
                        Phase::Analyze,
                        "address Sum path depth",
                        Some(i.span),
                    )]);
                }
                let (available, ty) = endpoint(f, *address, path, b)?;
                b.type_work(ty, true, "address Sum endpoint shape")?;
                let claimed = i.result.and_then(|v| f.values.get(v.0 as usize));
                match &i.kind {
                    InstructionKind::VariantTagAddress { .. } => {
                        if !matches!(ty, Type::Enum { .. }) || claimed != Some(&Type::I32) {
                            return Err(bad_path());
                        }
                    }
                    InstructionKind::BorrowVariantAddress { mutable, .. } => {
                        if path.is_empty()
                            || (*mutable && !available)
                            || !matches!(claimed,Some(Type::Address{mutable:m,inner})if m==mutable&&inner.as_ref()==ty)
                        {
                            return Err(bad_path());
                        }
                        if !ty.is_copy_value()
                            && !matches!(ty, Type::Package { .. } | Type::Enum { .. })
                        {
                            return Err(bad_path());
                        }
                        if matches!(ty, Type::Package { .. } | Type::Enum { .. }) {
                            resources::sums::admit(module, ty, b)?;
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    let run =
        |index: usize, state: &mut State, validate: bool, b: &mut Budget| -> CompileResult<()> {
            for i in &f.blocks[index].instructions {
                b.spend(1, "address Sum epoch transition")?;
                match &i.kind {
                    InstructionKind::VariantTagAddress { address, path } => {
                        if validate {
                            require(state, *address, path, b)?
                        }
                        b.spend(1, "address tag observation")?;
                        state.observed.insert(i.result.unwrap());
                    }
                    InstructionKind::BorrowVariantAddress {
                        address,
                        path,
                        mutable,
                    } => {
                        if validate {
                            require(state, *address, path, b)?
                        }
                        let (_, ty) = endpoint(f, *address, path, b)?;
                        if *mutable && !ty.is_b0_scalar() {
                            state.facts.clear();
                            state.observed.clear();
                        }
                    }
                    other if clobbers(other) => {
                        state.facts.clear();
                        state.observed.clear();
                    }
                    _ => {}
                }
            }
            Ok(())
        };
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(State::default());
    let mut queue = VecDeque::from([f.entry.0 as usize]);
    while let Some(index) = queue.pop_front() {
        let mut output = inputs[index].as_ref().unwrap().copy(b)?;
        run(index, &mut output, false, b)?;
        let next = match f.blocks[index].terminator {
            Terminator::Jump(t) => Some(t),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let mut selected = None;
                if let Some(InstructionKind::Binary { op, left, right }) =
                    definitions[condition.0 as usize]
                {
                    for (tag, constant) in [(*left, *right), (*right, *left)] {
                        if let (
                            Some(InstructionKind::VariantTagAddress { address, path }),
                            Some(InstructionKind::Constant(Constant::I32(n))),
                        ) = (
                            definitions[tag.0 as usize],
                            definitions[constant.0 as usize],
                        ) {
                            if *n >= 0
                                && output.observed.contains(&tag)
                                && matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
                            {
                                selected =
                                    Some((*address, path, *n as u32, *op == BinaryOp::Equal));
                            }
                        }
                    }
                }
                for (target, truth) in [(then_block, true), (else_block, false)] {
                    let mut edge = output.copy(b)?;
                    if let Some((address, path, tag, equal)) = selected {
                        if truth == equal {
                            b.spend(
                                path.len().saturating_add(1).saturating_mul(
                                    edge.facts
                                        .len()
                                        .checked_ilog2()
                                        .map_or(1, |n| n as usize + 2),
                                ),
                                "address tag edge fact",
                            )?;
                            let key = path
                                .iter()
                                .map(|p| (p.variant, p.index))
                                .collect::<Vec<_>>();
                            edge.facts.insert((address, key, tag));
                        }
                    }
                    let changed = if let Some(prior) = &mut inputs[target.0 as usize] {
                        prior.meet(&edge, b)?
                    } else {
                        inputs[target.0 as usize] = Some(edge);
                        true
                    };
                    if changed {
                        queue.push_back(target.0 as usize)
                    }
                }
                continue;
            }
            _ => None,
        };
        if let Some(target) = next {
            let changed = if let Some(prior) = &mut inputs[target.0 as usize] {
                prior.meet(&output, b)?
            } else {
                inputs[target.0 as usize] = Some(output.copy(b)?);
                true
            };
            if changed {
                queue.push_back(target.0 as usize)
            }
        }
    }
    for (index, live) in reachable.iter().enumerate() {
        if *live {
            let mut state = inputs[index].as_ref().ok_or_else(bad_path)?.copy(b)?;
            run(index, &mut state, true, b)?;
        }
    }
    Ok(())
}
