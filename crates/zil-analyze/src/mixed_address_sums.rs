//! Active tags of actual addressed Sums. This pass consumes raw CFG operations;
//! it never treats a copied enum value or a producer claim as a place proof.
use super::*;
type Path = Vec<(u8, u32, u32)>;
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
                .len()
                .checked_add(self.observed.len())
                .and_then(|n| n.checked_add(1))
                .ok_or_else(exhausted)?,
            "mixed address snapshot scan",
        )?;
        let routes = self.facts.iter().try_fold(0usize, |n, (_, p, _)| {
            n.checked_add(p.len()).ok_or_else(exhausted)
        })?;
        b.spend(routes, "mixed address path clone")?;
        Ok(self.clone())
    }
    fn meet(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        let count = [
            self.facts.len(),
            other.facts.len(),
            self.observed.len(),
            other.observed.len(),
            1,
        ]
        .into_iter()
        .try_fold(0usize, |n, m| n.checked_add(m).ok_or_else(exhausted))?;
        b.spend(count, "mixed address must-join scan")?;
        for (_, p, _) in &self.facts {
            search_fee(b, p.len(), other.facts.len())?;
        }
        // Each lookup is paid before retain may inspect it.
        for _ in &self.observed {
            search_fee(b, 0, other.observed.len())?;
        }
        let old = (self.facts.len(), self.observed.len());
        self.facts.retain(|f| other.facts.contains(f));
        self.observed.retain(|v| other.observed.contains(v));
        Ok(old != (self.facts.len(), self.observed.len()))
    }
}
fn bad_path() -> Vec<Diagnostic> {
    bad("addressed mixed Sum payload has no current actual tag authority")
}
fn endpoint<'a>(
    f: &'a TypedFunction,
    address: ValueId,
    path: &[zil_ir::PlaceProjection],
    b: &mut Budget,
) -> CompileResult<(bool, &'a Type)> {
    let Some(Type::Address { mutable, inner }) = f.values.get(address.0 as usize) else {
        return Err(bad_path());
    };
    let mut ty = inner.as_ref();
    b.spend(path.len() + 1, "mixed address Sum projection")?;
    for part in path {
        ty = match (part, ty) {
            (zil_ir::PlaceProjection::Field(i), Type::Tuple(fields)) => fields.get(*i as usize),
            (
                zil_ir::PlaceProjection::Field(i),
                Type::Record { fields, .. } | Type::Package { fields, .. },
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (zil_ir::PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (zil_ir::PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(bad_path)?;
    }
    Ok((*mutable, ty))
}
fn require(
    state: &State,
    address: ValueId,
    path: &[zil_ir::PlaceProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    let mut prefix = Vec::new();
    for p in path {
        search_fee(b, prefix.len(), state.facts.len())?;
        let tag = match p {
            zil_ir::PlaceProjection::Variant(v) => Some(v.variant),
            zil_ir::PlaceProjection::ErrorArm(e) => Some(u32::from(*e)),
            _ => None,
        };
        if let Some(tag) = tag
            && !state.facts.contains(&(address, prefix.clone(), tag))
        {
            return Err(bad_path());
        }
        prefix.push(step_key(p));
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
    if !b.mixed_address_sums {
        return Ok(());
    }
    let mut present = false;
    b.spend(f.blocks.len(), "mixed address Sum feature scan")?;
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1, "mixed address Sum instruction scan")?;
            present |= matches!(
                i.kind,
                InstructionKind::PlaceTagAddress { .. }
                    | InstructionKind::BorrowPlaceAddress { .. }
            );
        }
    }
    if !present {
        return Ok(());
    }
    b.spend(
        f.values.len() + f.blocks.len() * 2,
        "mixed address Sum tables",
    )?;
    let mut definitions = vec![None; f.values.len()];
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1, "mixed address definitions/type scan")?;
            if let Some(v) = i.result {
                definitions[v.0 as usize] = Some(&i.kind)
            }
            if let InstructionKind::PlaceTagAddress { address, path }
            | InstructionKind::BorrowPlaceAddress { address, path, .. } = &i.kind
            {
                if path.len() > limits.max_call_depth {
                    return Err(vec![Diagnostic::limit(
                        Phase::Analyze,
                        "mixed address Sum path depth",
                        Some(i.span),
                    )]);
                }
                let (available, ty) = endpoint(f, *address, path, b)?;
                b.type_work(ty, true, "mixed address Sum endpoint shape")?;
                let claimed = i.result.and_then(|v| f.values.get(v.0 as usize));
                match &i.kind {
                    InstructionKind::PlaceTagAddress { .. } => {
                        if !matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                            || claimed != Some(&Type::I32)
                        {
                            return Err(bad_path());
                        }
                    }
                    InstructionKind::BorrowPlaceAddress { mutable, .. } => {
                        if path.is_empty()
                            || (*mutable && !available)
                            || !matches!(claimed,Some(Type::Address{mutable:m,inner})if m==mutable&&inner.as_ref()==ty)
                        {
                            return Err(bad_path());
                        }
                        let ordinary_store_leaf = ownership_store::pod_owned(ty);
                        if !ty.is_copy_value()
                            && !matches!(
                                ty,
                                Type::Tuple(_)
                                    | Type::Package { .. }
                                    | Type::Enum { .. }
                                    | Type::ErrorUnion { .. }
                            )
                            && !ordinary_store_leaf
                        {
                            return Err(bad_path());
                        }
                        if ordinary_store_leaf {
                            let Some(Type::Address { inner, .. }) =
                                f.values.get(address.0 as usize)
                            else {
                                return Err(bad_path());
                            };
                            if !resources::sums::admit(module, inner, b)? {
                                return Err(bad_path());
                            }
                            ownership_store::ordinary_owned(module, ty, b)?;
                        }
                        if matches!(
                            ty,
                            Type::Tuple(_)
                                | Type::Package { .. }
                                | Type::Enum { .. }
                                | Type::ErrorUnion { .. }
                        ) {
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
                b.spend(1, "mixed address Sum epoch transition")?;
                match &i.kind {
                    InstructionKind::PlaceTagAddress { address, path } => {
                        if validate {
                            require(state, *address, path, b)?
                        }
                        search_fee(b, 0, state.observed.len())?;
                        state.observed.insert(i.result.unwrap());
                    }
                    InstructionKind::BorrowPlaceAddress {
                        address,
                        path,
                        mutable: _,
                    } => {
                        if validate {
                            require(state, *address, path, b)?
                        }
                    }
                    other if clobbers(other) => {
                        b.spend(
                            state
                                .facts
                                .len()
                                .checked_add(state.observed.len())
                                .ok_or_else(exhausted)?,
                            "mixed address invalidation",
                        )?;
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
                        search_fee(b, 0, output.observed.len())?;
                        if let (
                            Some(InstructionKind::PlaceTagAddress { address, path }),
                            Some(InstructionKind::Constant(Constant::I32(n))),
                        ) = (
                            definitions[tag.0 as usize],
                            definitions[constant.0 as usize],
                        ) && *n >= 0
                            && output.observed.contains(&tag)
                            && matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
                        {
                            selected = Some((*address, path, *n as u32, *op == BinaryOp::Equal));
                        }
                    }
                }
                for (target, truth) in [(then_block, true), (else_block, false)] {
                    let mut edge = output.copy(b)?;
                    if let Some((address, path, tag, equal)) = selected
                        && truth == equal
                    {
                        search_fee(b, path.len(), edge.facts.len())?;
                        let key = path.iter().map(step_key).collect::<Vec<_>>();
                        edge.facts.insert((address, key, tag));
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

fn step_key(p: &zil_ir::PlaceProjection) -> (u8, u32, u32) {
    match p {
        zil_ir::PlaceProjection::Field(i) => (0, *i, 0),
        zil_ir::PlaceProjection::Variant(v) => (1, v.variant, v.index),
        zil_ir::PlaceProjection::ErrorArm(e) => (2, u32::from(*e), 0),
    }
}

fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Analyze,
        "mixed address accounting overflow",
        None,
    )]
}
fn search_fee(b: &mut Budget, path: usize, set: usize) -> CompileResult<()> {
    let bound = path
        .checked_add(1)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_mul(set.checked_ilog2().map_or(1, |n| n as usize + 2)))
        .ok_or_else(exhausted)?;
    b.spend(bound, "mixed address ordered-key comparison")
}
