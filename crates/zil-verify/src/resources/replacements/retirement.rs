//! Exact private retirement obligations. No frontend rows or name-based apart.
use super::*;
use zil_ir::PlaceProjection;
mod rows;
pub(in crate::resources) use rows::Rows;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(in crate::resources) struct Sources {
    direct: actual::Targets,
    captures: actual::Captures,
}
impl Sources {
    #[cfg(test)]
    pub(in crate::resources) fn field_state_fixture(seed: u32) -> Self {
        let target = actual::Target {
            base: actual::Base::Input(seed),
            steps: vec![PlaceProjection::Field(seed % 2)],
            mutable: true,
        };
        let captured = actual::Target {
            base: actual::Base::InputCapture(seed, 0),
            steps: vec![PlaceProjection::Field(0)],
            mutable: false,
        };
        Self {
            direct: BTreeSet::from([target]),
            captures: BTreeMap::from([(0, BTreeSet::from([captured]))]),
        }
    }
    pub(in crate::resources) fn cost(&self, b: &mut Budget) -> Check<()> {
        actual::charge_set(&self.direct, b)?;
        b.spend(self.captures.len())?;
        for sources in self.captures.values() {
            actual::charge_set(sources, b)?;
        }
        Ok(())
    }
    pub(in crate::resources) fn merge(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        Ok(actual::merge(&mut self.direct, &other.direct, b)?
            | actual::merge_captures(&mut self.captures, &other.captures, b)?)
    }
    fn copy(&self, b: &mut Budget) -> Check<Self> {
        self.cost(b)?;
        Ok(self.clone())
    }
}
pub(super) type Pairs = BTreeSet<(actual::Target, actual::Target)>;
pub(super) struct Call {
    entry: usize,
    arguments: Vec<ValueId>,
    facts: actual::Function,
    captures: actual::Captures,
    span: Span,
    sealed: bool,
}
fn singleton(
    base: actual::Base,
    steps: Vec<PlaceProjection>,
    b: &mut Budget,
) -> Check<actual::Targets> {
    b.spend(steps.len().saturating_add(1))?;
    Ok(BTreeSet::from([actual::Target {
        base,
        steps,
        mutable: false,
    }]))
}
fn env(ty: &Type) -> Option<ClosureId> {
    match ty {
        Type::Closure(id) => Some(*id),
        Type::Address { inner, .. } => {
            if let Type::Closure(id) = inner.as_ref() {
                Some(*id)
            } else {
                None
            }
        }
        _ => None,
    }
}
fn capture_seed(
    module: &TypedModule,
    ty: &Type,
    parameter: u32,
    b: &mut Budget,
) -> Check<actual::Captures> {
    let mut result = actual::Captures::new();
    if let Some(id) = env(ty) {
        let fields = &module.closures[id.0 as usize].captures;
        b.spend(fields.len())?;
        for (index, field) in fields.iter().enumerate() {
            if matches!(field, Type::Desc(_) | Type::Address { .. }) {
                b.spend(result.len().saturating_add(1))?;
                result.insert(
                    index as u32,
                    singleton(
                        actual::Base::InputCapture(parameter, index as u32),
                        vec![],
                        b,
                    )?,
                );
            }
        }
    }
    Ok(result)
}
pub(super) fn seed(
    plan: &Plan,
    module: &TypedModule,
    f: &TypedFunction,
    slots: &Slots,
    state: &mut State,
    b: &mut Budget,
) -> Check<()> {
    if !plan.active_pairs {
        return Ok(());
    }
    b.spend(f.parameters.len())?;
    for (index, local) in f.parameters.iter().enumerate() {
        if let Some(slot) = slots.desc_local[local.0 as usize] {
            let ty = &f.locals[local.0 as usize].ty;
            if matches!(ty, Type::Desc(_) | Type::Address { .. }) {
                let direct = singleton(actual::Base::Input(index as u32), vec![], b)?;
                state.retirement.direct(slot, direct, b)?;
            }
            let captures = capture_seed(module, ty, index as u32, b)?;
            state.retirement.captures(slot, captures, b)?;
        }
    }
    Ok(())
}
fn value(
    plan: &Plan,
    transfer: &Transfer<'_>,
    state: &State,
    v: ValueId,
    b: &mut Budget,
) -> Check<Sources> {
    if let Some(slot) = transfer.slots.desc_value[v.0 as usize] {
        return state.retirement.get(slot, b)?.copy(b);
    }
    let facts = &plan.actual.functions[transfer.f.id.0 as usize];
    actual::charge_set(&facts.values[v.0 as usize], b)?;
    b.spend(facts.captures[v.0 as usize].len())?;
    for row in facts.captures[v.0 as usize].values() {
        actual::charge_set(row, b)?;
    }
    let source = Sources {
        direct: facts.values[v.0 as usize].clone(),
        captures: facts.captures[v.0 as usize].clone(),
    };
    // Charge before cloning variable-width recipes.
    // This fallback is only a checked address/closure producer, never a Desc.
    Ok(source)
}
pub(super) fn step(
    plan: &Plan,
    transfer: &Transfer<'_>,
    state: &mut State,
    i: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    if !plan.active_pairs {
        return Ok(());
    }
    let f = transfer.f;
    let slots = transfer.slots;
    if let Some(output) = i.result.and_then(|v| slots.desc_value[v.0 as usize]) {
        let mut source = Sources::default();
        match &i.kind {
            InstructionKind::DescribeLocal { local } => {
                source.direct = singleton(actual::Base::Local(*local), vec![], b)?
            }
            InstructionKind::DescribePackageField { local, index } => {
                source.direct = singleton(
                    actual::Base::Local(*local),
                    vec![PlaceProjection::Field(*index)],
                    b,
                )?
            }
            InstructionKind::DescribeCapture { index } => {
                source.direct = singleton(actual::Base::Environment(*index), vec![], b)?
            }
            InstructionKind::ReborrowAddress { address, .. } => {
                source = value(plan, transfer, state, *address, b)?;
            }
            InstructionKind::DescribeAddress { address } => {
                source.direct = value(plan, transfer, state, *address, b)?.direct
            }
            InstructionKind::ReadCapture { index }
                if matches!(f.values[i.result.unwrap().0 as usize], Type::Desc(_)) =>
            {
                source.direct = singleton(actual::Base::Environment(*index), vec![], b)?
            }
            InstructionKind::Load(local)
            | InstructionKind::TakeLocal { local }
            | InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local } => {
                if let Some(slot) = slots.desc_local[local.0 as usize] {
                    source = state.retirement.get(slot, b)?.copy(b)?;
                }
                if source.direct.is_empty()
                    && matches!(f.values[i.result.unwrap().0 as usize], Type::Address { .. })
                {
                    let facts = &plan.actual.functions[f.id.0 as usize];
                    actual::merge(
                        &mut source.direct,
                        &facts.values[i.result.unwrap().0 as usize],
                        b,
                    )?;
                }
            }
            InstructionKind::MakeClosure { closure, captures } => {
                let fields = &transfer.module.closures[closure.0 as usize].captures;
                b.spend(fields.len())?;
                for (index, (ty, capture)) in fields.iter().zip(captures).enumerate() {
                    if matches!(ty, Type::Desc(_) | Type::Address { .. }) {
                        let roots = value(plan, transfer, state, *capture, b)?.direct;
                        b.spend(source.captures.len().saturating_add(1))?;
                        source.captures.insert(index as u32, roots);
                    }
                }
            }
            _ => {
                if matches!(f.values[i.result.unwrap().0 as usize], Type::Address { .. }) {
                    let facts = &plan.actual.functions[f.id.0 as usize];
                    actual::merge(
                        &mut source.direct,
                        &facts.values[i.result.unwrap().0 as usize],
                        b,
                    )?;
                    actual::merge_captures(
                        &mut source.captures,
                        &facts.captures[i.result.unwrap().0 as usize],
                        b,
                    )?;
                }
            }
        }
        state.retirement.replace(output, source, b)?;
    }
    match i.kind {
        InstructionKind::Store { local, value: v } => {
            if let Some(slot) = slots.desc_local[local.0 as usize] {
                let src = value(plan, transfer, state, v, b)?;
                state.retirement.replace(slot, src, b)?;
            }
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            if let Some(slot) = slots.desc_local[local.0 as usize] {
                state.retirement.replace(slot, Sources::default(), b)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn known_local(base: actual::Base) -> bool {
    matches!(base, actual::Base::Local(_))
}
fn relation(left: &actual::Target, right: &actual::Target, b: &mut Budget) -> Check<Option<bool>> {
    b.spend(
        left.steps
            .len()
            .saturating_add(right.steps.len())
            .saturating_add(1),
    )?;
    if left.base != right.base {
        if known_local(left.base) || known_local(right.base) {
            return Ok(Some(true));
        }
        return Ok(None);
    }
    for (a, c) in left.steps.iter().zip(&right.steps) {
        if a != c {
            return Ok(Some(
                matches!((a,c),(PlaceProjection::Field(a),PlaceProjection::Field(c)) if a!=c),
            ));
        }
    }
    Ok(Some(false))
}
fn pair(
    rows: &mut Pairs,
    left: &actual::Target,
    right: &actual::Target,
    span: Span,
    b: &mut Budget,
) -> Check<bool> {
    match relation(left, right, b)? {
        Some(true) => Ok(false),
        Some(false) => Err(bad(
            span,
            "possible replacement retirement would invalidate a live resource description",
        )),
        None => {
            if matches!(left.base, actual::Base::Element(_))
                || matches!(right.base, actual::Base::Element(_))
            {
                return Err(unsupported(
                    "retirement separation for an external Store element needs its exact instance relation",
                    Some(span),
                ));
            }
            let width = left
                .steps
                .len()
                .saturating_add(right.steps.len())
                .saturating_add(2);
            b.spend(width.saturating_mul(rows.len().saturating_add(1)))?;
            for (a, c) in rows.iter() {
                b.spend(
                    a.steps
                        .len()
                        .saturating_add(c.steps.len())
                        .saturating_add(2),
                )?;
            }
            let (mut a, mut c) = (left.clone(), right.clone());
            a.mutable = false;
            c.mutable = false;
            if a > c {
                std::mem::swap(&mut a, &mut c)
            }
            Ok(rows.insert((a, c)))
        }
    }
}
pub(super) fn observe(
    plan: &Plan,
    transfer: &Transfer<'_>,
    state: &State,
    target: &actual::Target,
    point: Point,
    b: &mut Budget,
) -> Check<()> {
    if matches!(target.base, actual::Base::Element(_)) {
        return Ok(());
    }
    sums::replacements::at_type(transfer.module, transfer.f, target, b)?;
    let f = transfer.f;
    let slots = transfer.slots;
    for (ty, slot) in f
        .values
        .iter()
        .zip(&slots.desc_value)
        .chain(f.locals.iter().map(|l| &l.ty).zip(&slots.desc_local))
    {
        b.spend(1)?;
        if !(matches!(ty, Type::Desc(_) | Type::Closure(_))
            || matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Closure(_))))
        {
            continue;
        }
        let Some(slot) = *slot else { continue };
        if Some(slot) == point.output
            || !transfer
                .after
                .row(point.block, point.position)
                .get(slot, b)?
        {
            continue;
        }
        let source = state.retirement.get(slot, b)?;
        source.cost(b)?;
        let mut roots = actual::Targets::new();
        if matches!(ty, Type::Desc(_)) {
            actual::merge(&mut roots, &source.direct, b)?;
        }
        if let Some(id) = env(ty) {
            let fields = &transfer.module.closures[id.0 as usize].captures;
            b.spend(fields.len())?;
            for (index, ty) in fields.iter().enumerate() {
                if matches!(ty, Type::Desc(_)) {
                    b.spend(source.captures.len())?;
                    if let Some(row) = source.captures.get(&(index as u32)) {
                        actual::merge(&mut roots, row, b)?;
                    } else {
                        return Err(unsupported(
                            "live descriptor capture lacks an exact retirement source",
                            Some(point.span),
                        ));
                    }
                }
            }
        }
        if roots.is_empty() {
            return Err(bad(
                point.span,
                "live resource description has no exact retirement source",
            ));
        }
        for desc in roots {
            pair(
                &mut plan.pairs.borrow_mut()[f.id.0 as usize],
                target,
                &desc,
                point.span,
                b,
            )?;
        }
    }
    Ok(())
}
pub(super) fn call(
    plan: &Plan,
    transfer: &Transfer<'_>,
    state: &State,
    inv: &Invocation<'_>,
    point: Point,
    b: &mut Budget,
) -> Check<()> {
    b.spend(inv.arguments.len().saturating_mul(3).saturating_add(1))?;
    let mut facts = actual::Function::default();
    let mut args = Vec::with_capacity(inv.arguments.len());
    for (index, v) in inv.arguments.iter().enumerate() {
        let source = value(plan, transfer, state, *v, b)?;
        facts.values.push(source.direct);
        facts.captures.push(source.captures);
        args.push(ValueId(index as u32));
    }
    let mut captures = actual::Captures::new();
    if let Some(actual) = inv.captures {
        actual::merge_captures(&mut captures, actual, b)?;
    }
    if let Some(slot) = inv.sources {
        let exact = &state.retirement.get(slot, b)?.captures;
        b.spend(exact.len())?;
        for (index, roots) in exact {
            actual::charge_set(roots, b)?;
            b.spend(captures.len().saturating_add(1))?;
            captures.insert(*index, roots.clone());
        }
    }
    let mut calls = plan.calls.borrow_mut();
    b.spend(calls[transfer.f.id.0 as usize].len().saturating_add(1))?;
    calls[transfer.f.id.0 as usize].push(Call {
        entry: inv.entry,
        arguments: args,
        facts,
        captures,
        span: point.span,
        sealed: point.sealed,
    });
    Ok(())
}
pub(super) fn close(plan: &Plan, module: &TypedModule, b: &mut Budget) -> Check<()> {
    if !plan.active_pairs {
        return Ok(());
    }
    loop {
        let mut changed = false;
        let calls = plan.calls.borrow();
        b.spend(calls.len())?;
        for (caller, sites) in calls.iter().enumerate() {
            b.spend(sites.len())?;
            for site in sites {
                let rows = {
                    let pairs = plan.pairs.borrow();
                    let rows = &pairs[site.entry];
                    b.spend(rows.len())?;
                    for (a, c) in rows {
                        b.spend(
                            a.steps
                                .len()
                                .saturating_add(c.steps.len())
                                .saturating_add(2),
                        )?;
                    }
                    rows.clone()
                };
                if site.sealed && !rows.is_empty() {
                    return Err(unsupported(
                        "sealed callback retirement needs a checked separation envelope",
                        Some(site.span),
                    ));
                }
                for (a, c) in rows {
                    b.spend(
                        a.steps
                            .len()
                            .saturating_add(c.steps.len())
                            .saturating_add(2),
                    )?;
                    let left = actual::substitute(
                        &BTreeSet::from([a]),
                        &site.arguments,
                        Some(&site.captures),
                        &site.facts,
                        b,
                    )?;
                    let right = actual::substitute(
                        &BTreeSet::from([c]),
                        &site.arguments,
                        Some(&site.captures),
                        &site.facts,
                        b,
                    )?;
                    if left.is_empty() || right.is_empty() {
                        return Err(bad(
                            site.span,
                            "retirement separation has an ungrounded actual call source",
                        ));
                    }
                    for a in &left {
                        for c in &right {
                            changed |=
                                pair(&mut plan.pairs.borrow_mut()[caller], a, c, site.span, b)?;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Private obligations cannot disappear through a pointer or an unchecked
    // public callable envelope. Those profiles keep their existing fence.
    let pairs = plan.pairs.borrow();
    b.spend(module.functions.len())?;
    for f in &module.functions {
        let rows = &pairs[f.id.0 as usize];
        b.spend(rows.len())?;
        let input_pairs = f.callable_contract.is_some()
            && f.call_kind == zil_ir::CallKind::Direct
            && rows.iter().all(|(a, c)| {
                [a, c].iter().all(|target| {
                    matches!(
                        target.base,
                        actual::Base::Input(_) | actual::Base::InputCapture(..)
                    )
                })
            });
        if !rows.is_empty()
            && (f.callable_return_contract.is_some()
                || (f.callable_contract.is_some() && !input_pairs))
        {
            return Err(unsupported(
                "public callable retirement needs a checked separation envelope",
                Some(f.span),
            ));
        }
        b.spend(f.blocks.len())?;
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for i in &block.instructions {
                if let InstructionKind::FunctionRef { function } = i.kind
                    && !pairs[function.0 as usize].is_empty()
                {
                    return Err(unsupported(
                        "retirement separation cannot be erased through a function pointer",
                        Some(i.span),
                    ));
                }
            }
        }
    }
    Ok(())
}
