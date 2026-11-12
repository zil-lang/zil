//! Frozen bcfec60 local/observation map transfer, before direct indexing.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
struct OldFacts {
    locals: BTreeMap<LocalId, Arc<Tree>>,
    values: ssa_values::Values,
    captures: BTreeMap<u32, Arc<Tree>>,
    valid: BTreeSet<ValueId>,
    partial: Option<partial_places::State>,
}
struct OldPlan<'a> {
    actual: &'a Plan,
    observations: BTreeMap<ValueId, (LocalId, Vec<VariantProjection>)>,
}
impl std::ops::Deref for OldPlan<'_> {
    type Target = Plan;
    fn deref(&self) -> &Plan {
        self.actual
    }
}
impl<'a> OldPlan<'a> {
    fn new(actual: &'a Plan, f: &TypedFunction) -> Self {
        let mut observations = BTreeMap::new();
        for i in f.blocks.iter().flat_map(|block| &block.instructions) {
            if let (InstructionKind::VariantTagLocal { local, path }, Some(v)) = (&i.kind, i.result)
            {
                observations.insert(v, (*local, path.clone()));
            }
        }
        Self {
            actual,
            observations,
        }
    }
    pub(super) fn edge(
        &self,
        f: &TypedFunction,
        b: usize,
        successor: usize,
        facts: &mut OldFacts,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        if let (Some(meta), Some(state)) = (&self.partial, &mut facts.partial)
            && let Some((local, path, tag, equal)) =
                meta.edge(state, &f.blocks[b].terminator, successor, budget)?
            && !facts
                .locals
                .get_mut(&local)
                .ok_or_else(|| error("partial edge root absent"))?
                .narrow_place(&path, tag, equal, budget)?
        {
            return Ok(false);
        }
        if let Terminator::Branch {
            condition,
            then_block,
            else_block,
        } = f.blocks[b].terminator
            && then_block != else_block
            && let Some((tag, constant, n, equal)) = self.tests.get(&condition)
        {
            budget.lookup(facts.valid.len(), "Sum edge observation lifetime")?;
            if facts.valid.contains(tag)
                && facts.value(*constant, budget)?.complete(budget)?
                && facts.value(condition, budget)?.complete(budget)?
            {
                let (l, path) = &self.observations[tag];
                let want = if then_block.0 as usize == successor {
                    *equal
                } else {
                    !*equal
                };
                return facts
                    .locals
                    .get_mut(l)
                    .ok_or_else(|| error("Sum edge local absent"))?
                    .narrow(path, *n, want, budget);
            }
        }
        Ok(true)
    }
    pub(super) fn validate_unpack(
        &self,
        f: &TypedFunction,
        i: &Instruction,
        facts: &OldFacts,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } = &i.kind
        else {
            return Ok(());
        };
        let Type::Enum { variants, .. } = &f.locals[local.0 as usize].ty else {
            return Err(error("Unpack requires an Enum local"));
        };
        if !budget.sum_type(&f.locals[local.0 as usize].ty)? || i.result.is_some() {
            return Err(error("Unpack requires an owning Enum and no result"));
        }
        let fs = variants
            .get(*variant as usize)
            .ok_or_else(|| error("Unpack variant absent"))?;
        if fields.len() != fs.1.len() {
            return Err(error("Unpack field count differs"));
        }
        let mut seen = BTreeSet::new();
        for (l, t) in fields.iter().zip(&fs.1) {
            budget.lookup(seen.len(), "Unpack target uniqueness")?;
            budget.type_work(t, true, "Unpack exact field type")?;
            if l == local
                || !seen.insert(*l)
                || f.locals.get(l.0 as usize).map(|l| &l.ty) != Some(t)
            {
                return Err(error("Unpack targets alias or have wrong types"));
            }
            if check
                && !facts
                    .locals
                    .get(l)
                    .ok_or_else(|| error("Unpack target absent"))?
                    .missing
            {
                return Err(error("Unpack target is not vacant"));
            }
        }
        let t = &facts.locals[local];
        let Node::Sum(vs) = &t.node else {
            return Err(error("Unpack source is not a Sum"));
        };
        if check && (!t.complete(budget)? || vs.len() != 1 || !vs.contains_key(variant)) {
            return Err(error("Unpack lacks a complete current selected source"));
        }
        Ok(())
    }
    pub(super) fn apply(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        i: &Instruction,
        facts: &mut OldFacts,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<Option<BTreeSet<Birth>>> {
        budget.spend(1, "Sum instruction facts")?;
        // Reserve the fixed number of indexed lookups used by the transfer;
        // variable-width field and subtree work is charged separately below.
        for _ in 0..8 {
            budget.lookup(
                facts.locals.len() + facts.values.len() + self.observations.len(),
                "Sum instruction index reserve",
            )?;
        }
        let mut output = None;
        if check
            && self.partial.is_some()
            && let InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local } = i.kind
            && !facts.local(local, budget)?.complete(budget)?
        {
            return Err(error(
                "whole borrow requires every selected descendant initialized",
            ));
        }
        if let (Some(meta), Some(state)) = (&self.partial, &mut facts.partial) {
            meta.step(state, i, budget)?;
            if partial_places::invalidates(&i.kind) {
                if let Some(locals) = &self.tagged_locals {
                    budget.spend(locals.len(), "mixed tagged local invalidation inventory")?;
                    for local in locals {
                        budget.lookup(facts.locals.len(), "mixed tagged local fact lookup")?;
                        facts
                            .locals
                            .get_mut(local)
                            .ok_or_else(|| error("mixed tagged local fact is absent"))?
                            .forget_addressed_copy_tags(&f.locals[local.0 as usize].ty, budget)?;
                    }
                } else {
                    for (local, tree) in &mut facts.locals {
                        tree.forget_addressed_copy_tags(&f.locals[local.0 as usize].ty, budget)?;
                    }
                }
            }
        }
        match &i.kind {
            InstructionKind::BindPackageCapture { local, index }
                if facts.has_capture(*index, budget)? =>
            {
                let capture_type = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|closure| closure.captures.get(*index as usize));
                if capture_type != Some(&f.locals[local.0 as usize].ty) {
                    return Err(error("Package Sum capture binding type differs"));
                }
                let capture = facts.capture(*index, budget)?;
                if check && !capture.complete(budget)? {
                    return Err(error("Package Sum capture binding is incomplete"));
                }
                budget.lookup(facts.locals.len(), "Package Sum capture local transfer")?;
                facts.locals.insert(*local, capture);
                edit(
                    facts
                        .captures
                        .get_mut(index)
                        .ok_or_else(|| error("Package Sum capture absent"))?,
                    budget,
                )?
                .missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::EndPackageCapture { local, index }
                if facts.has_capture(*index, budget)? =>
            {
                let local_tree = facts.local(*local, budget)?;
                if check && !facts.capture(*index, budget)?.missing {
                    return Err(error("Package Sum capture binding is still occupied"));
                }
                budget.lookup(facts.captures.len(), "Package Sum capture restoration")?;
                facts.captures.insert(*index, local_tree);
                edit(
                    facts
                        .locals
                        .get_mut(local)
                        .ok_or_else(|| error("Package Sum local absent"))?,
                    budget,
                )?
                .missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::TakeCapture { index } => {
                let ty = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|closure| closure.captures.get(*index as usize))
                    .ok_or_else(|| error("TakeCapture Sum environment absent"))?;
                if matches!(
                    ty,
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) && budget.ordinary_store_sum_capture(f.environment.unwrap(), ty)?
                {
                    if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(ty) {
                        return Err(error("TakeCapture Sum result type differs"));
                    }
                    let capture = facts.capture(*index, budget)?;
                    if check && !capture.complete(budget)? {
                        return Err(error("TakeCapture requires a complete Sum capture"));
                    }
                    output = Some(capture);
                    edit(
                        facts
                            .captures
                            .get_mut(index)
                            .ok_or_else(|| error("TakeCapture Sum capture absent"))?,
                        budget,
                    )?
                    .missing = true;
                }
            }
            InstructionKind::WriteCapture { index, value } => {
                let ty = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|closure| closure.captures.get(*index as usize))
                    .ok_or_else(|| error("WriteCapture Sum environment absent"))?;
                if matches!(
                    ty,
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) && budget.ordinary_store_sum_capture(f.environment.unwrap(), ty)?
                {
                    if f.values.get(value.0 as usize) != Some(ty) {
                        return Err(error("WriteCapture Sum source type differs"));
                    }
                    let incoming = facts.value(*value, budget)?;
                    if check
                        && (!facts.capture(*index, budget)?.missing
                            || !incoming.complete(budget)?)
                    {
                        return Err(error(
                            "WriteCapture requires an empty slot and complete Sum",
                        ));
                    }
                    budget.lookup(facts.captures.len(), "Sum capture refill")?;
                    facts.captures.insert(*index, incoming);
                    edit(
                        facts
                            .values
                            .get_mut(value, budget)?
                            .ok_or_else(|| error("Sum capture refill value absent"))?,
                        budget,
                    )?
                    .missing = true;
                }
            }
            InstructionKind::BorrowLocalMut { local }
                if matches!(f.locals[local.0 as usize].ty, Type::Enum { .. }) =>
            {
                facts
                    .locals
                    .get_mut(local)
                    .ok_or_else(|| error("mutable Sum local absent"))?
                    .forget_addressed_copy_tags(&f.locals[local.0 as usize].ty, budget)?;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::Constant(_) | InstructionKind::MakeAggregate { .. } => {}
            InstructionKind::MakeManaged { .. }
            | InstructionKind::ByteBufferNew
            | InstructionKind::OwnedStoreNew { .. } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Leaf(BTreeSet::from([Birth::Created])),
                }));
            }
            InstructionKind::MakeVariant { variant, fields } => {
                let Some(Type::Enum { variants, .. }) =
                    i.result.and_then(|v| f.values.get(v.0 as usize))
                else {
                    return Err(error("MakeVariant result is not Enum"));
                };
                let fs = variants
                    .get(*variant as usize)
                    .ok_or_else(|| error("MakeVariant variant absent"))?;
                if fs.1.len() != fields.len() {
                    return Err(error("MakeVariant field count mismatch"));
                }
                for (v, t) in fields.iter().zip(&fs.1) {
                    budget.type_work(t, true, "MakeVariant exact field type")?;
                    if f.values.get(v.0 as usize) != Some(t) {
                        return Err(error("MakeVariant field type mismatch"));
                    }
                }
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([(
                        *variant,
                        fields
                            .iter()
                            .map(|v| facts.value(*v, budget))
                            .collect::<CompileResult<_>>()?,
                    )])),
                }));
            }
            InstructionKind::MakeErrorUnion { value, is_error } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([(
                        *is_error as u32,
                        vec![facts.value(*value, budget)?],
                    )])),
                }))
            }
            InstructionKind::MakePackage { fields }
            | InstructionKind::MakeOwnedTuple { fields } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Product(
                        fields
                            .iter()
                            .map(|v| facts.value(*v, budget))
                            .collect::<CompileResult<_>>()?,
                    ),
                }))
            }
            InstructionKind::BorrowPlaceLocal { local, path, .. }
            | InstructionKind::PlaceTagLocal { local, path } => {
                let root = &f.locals[local.0 as usize].ty;
                let ty = if path.is_empty() {
                    root
                } else {
                    place_type(f, *local, path, budget)?
                };
                let borrowed = matches!(i.kind, InstructionKind::BorrowPlaceLocal { .. });
                if borrowed {
                    let InstructionKind::BorrowPlaceLocal { mutable, .. } = i.kind else {
                        unreachable!()
                    };
                    if path.is_empty()
                        || (mutable && !f.locals[local.0 as usize].mutable)
                        || (mutable && matches!(ty, Type::Managed(_)))
                    {
                        return Err(error("mixed borrow has no exact actual storage authority"));
                    }
                    let expected = Type::Address {
                        mutable,
                        inner: Box::new(ty.clone()),
                    };
                    if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&expected) {
                        return Err(error("mixed borrow endpoint Type differs"));
                    }
                } else if !matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                    || i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&Type::I32)
                {
                    return Err(error("mixed tag must observe an actual Sum header as i32"));
                }
                if check {
                    facts
                        .partial
                        .as_ref()
                        .ok_or_else(|| error("mixed observation proof absent"))?
                        .require(*local, path, budget)?;
                    facts.locals[local].viewed_place(path, borrowed, budget)?;
                }
                if matches!(
                    i.kind,
                    InstructionKind::BorrowPlaceLocal { mutable: true, .. }
                ) {
                    facts
                        .locals
                        .get_mut(local)
                        .unwrap()
                        .forget_place_copy_tags(path, root, budget)?;
                }
            }
            InstructionKind::TakeTupleField { local, path }
            | InstructionKind::TakeVariantField { local, path } => {
                if check {
                    facts
                        .partial
                        .as_ref()
                        .ok_or_else(|| error("partial source proof absent"))?
                        .require(*local, path, budget)?;
                }
                let root = &f.locals[local.0 as usize].ty;
                budget.type_work(root, true, "partial transfer exact root")?;
                if root.is_copy_value()
                    || !match i.kind {
                        InstructionKind::TakeTupleField { .. } => matches!(root, Type::Tuple(_)),
                        _ => matches!(root, Type::Enum { .. } | Type::ErrorUnion { .. }),
                    }
                {
                    return Err(error(
                        "partial transfer requires exact owning Tuple or Enum root",
                    ));
                }
                let endpoint = place_type(f, *local, path, budget)?;
                budget.type_work(endpoint, true, "partial result exact Type")?;
                if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(endpoint) {
                    return Err(error(
                        "partial transfer result differs from actual subtree Type",
                    ));
                }
                output = Some(
                    facts
                        .locals
                        .get_mut(local)
                        .ok_or_else(|| error("partial source absent"))?
                        .take_place(path, check, budget)?,
                );
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::TakeLocal { local } | InstructionKind::Load(local) => {
                output = Some(facts.local(*local, budget)?);
                if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                    edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                    facts.invalidate(*local, self, budget)?;
                }
            }
            InstructionKind::Store { local, value } => {
                let t = facts.value(*value, budget)?;
                facts.invalidate(*local, self, budget)?;
                facts.locals.insert(*local, t);
            }
            InstructionKind::TakePackageField { local, index }
            | InstructionKind::ReadPackageField { local, index } => {
                let t = facts.local(*local, budget)?;
                let Node::Product(fs) = &t.node else {
                    return Err(error("Sum field source is not Product"));
                };
                output = Some(
                    fs.get(*index as usize)
                        .ok_or_else(|| error("Sum Product field absent"))?
                        .copied(budget)?,
                );
                if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                    if let Node::Product(fs) =
                        &mut edit(facts.locals.get_mut(local).unwrap(), budget)?.node
                    {
                        edit(&mut fs[*index as usize], budget)?.missing = true;
                    }
                    facts.invalidate(*local, self, budget)?;
                }
            }
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            } => {
                let t = facts.value(*value, budget)?;
                if let Node::Product(fs) =
                    &mut edit(facts.locals.get_mut(local).unwrap(), budget)?.node
                {
                    *fs.get_mut(*index as usize)
                        .ok_or_else(|| error("Sum Product store field absent"))? = t;
                }
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::DropPackageField { local, index } => {
                if let Node::Product(fs) =
                    &mut edit(facts.locals.get_mut(local).unwrap(), budget)?.node
                {
                    edit(
                        fs.get_mut(*index as usize)
                            .ok_or_else(|| error("Sum Product drop field absent"))?,
                        budget,
                    )?
                    .missing = true;
                }
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::TakeErrorUnion { value, is_error }
            | InstructionKind::ReadErrorUnion { value, is_error } => {
                let t = facts.value(*value, budget)?;
                let Node::Sum(vs) = &t.node else {
                    return Err(error("Sum union source absent"));
                };
                output = Some(
                    vs.get(&(*is_error as u32))
                        .and_then(|fs| fs.first())
                        .map(|t| t.copied(budget))
                        .transpose()?
                        .unwrap_or(Tree::unknown(
                            i.result
                                .and_then(|v| f.values.get(v.0 as usize))
                                .ok_or_else(|| error("union result absent"))?,
                            budget,
                        )?),
                );
            }
            InstructionKind::VariantTagLocal { local, path }
            | InstructionKind::ReadVariantLocal { local, path }
            | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                let ty = projection(&f.locals[local.0 as usize].ty, path, budget)?;
                let tag = matches!(i.kind, InstructionKind::VariantTagLocal { .. });
                let borrowed = matches!(i.kind, InstructionKind::BorrowVariantLocal { .. });
                let borrowed_ty;
                let expected = if borrowed {
                    let InstructionKind::BorrowVariantLocal { mutable, .. } = i.kind else {
                        unreachable!()
                    };
                    if path.is_empty()
                        || (!ty.is_copy_value()
                            && !matches!(
                                ty,
                                Type::Tuple(_)
                                    | Type::Package { .. }
                                    | Type::Enum { .. }
                                    | Type::ErrorUnion { .. }
                            ))
                        || (mutable && !f.locals[local.0 as usize].mutable)
                    {
                        return Err(error(
                            "payload borrow requires finite Copy and actual mutable authority",
                        ));
                    }
                    budget.type_work(ty, true, "Sum payload address type")?;
                    borrowed_ty = Type::Address {
                        mutable,
                        inner: Box::new(ty.clone()),
                    };
                    &borrowed_ty
                } else if tag {
                    if !matches!(ty, Type::Enum { .. }) {
                        return Err(error("tag endpoint is not Enum"));
                    }
                    &Type::I32
                } else {
                    if path.is_empty() || !ty.is_copy_value() {
                        return Err(error("local read endpoint is not strict Copy"));
                    }
                    ty
                };
                if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(expected) {
                    return Err(error("local Sum observation result type differs"));
                }
                facts.locals[local].projected(path, check, budget)?;
                if matches!(
                    i.kind,
                    InstructionKind::BorrowVariantLocal { mutable: true, .. }
                ) && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                    && ty.is_copy_value()
                {
                    facts
                        .locals
                        .get_mut(local)
                        .unwrap()
                        .forget_copy_endpoint(path, ty, budget)?;
                    budget.spend(facts.valid.len(), "mutable payload tag epoch")?;
                    facts.valid.retain(|v| {
                        self.observations.get(v).is_none_or(|(root, observed)| {
                            *root != *local || !observed.starts_with(path)
                        })
                    });
                }
                if tag {
                    budget.lookup(facts.valid.len(), "Sum live observation insertion")?;
                    facts.valid.insert(i.result.unwrap());
                }
            }
            InstructionKind::UnpackVariant {
                local,
                variant,
                fields,
            } => {
                self.validate_unpack(f, i, facts, check, budget)?;
                let tree = facts.local(*local, budget)?;
                let Node::Sum(vs) = &tree.node else {
                    unreachable!()
                };
                if let Some(fs) = vs.get(variant) {
                    for (l, t) in fields.iter().zip(fs) {
                        budget.lookup(facts.locals.len(), "Unpack fact publication")?;
                        facts.invalidate(*l, self, budget)?;
                        facts.locals.insert(*l, t.copied(budget)?);
                    }
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::UnpackTuple { local, fields } => {
                let tree = facts.local(*local, budget)?;
                let Type::Tuple(types) = &f.locals[local.0 as usize].ty else {
                    return Err(error("UnpackTuple source requires Tuple"));
                };
                if types.len() != fields.len() || i.result.is_some() {
                    return Err(error("UnpackTuple arity or result mismatch"));
                }
                budget.spend(
                    fields.len().saturating_mul(fields.len() + 2),
                    "Tuple atomic destination validation",
                )?;
                let mut targets = BTreeSet::new();
                for (l, t) in fields.iter().zip(types) {
                    if *l == *local || !targets.insert(*l) || f.locals[l.0 as usize].ty != *t {
                        return Err(error("Tuple destination type or alias mismatch"));
                    }
                    if check && facts.locals.get(l).is_some_and(|t| !t.missing) {
                        return Err(error("Tuple destination is initialized"));
                    }
                }
                let Node::Product(children) = &tree.node else {
                    return Err(error("Tuple source tree requires product"));
                };
                for (l, t) in fields.iter().zip(children) {
                    facts.invalidate(*l, self, budget)?;
                    facts.locals.insert(*l, t.copied(budget)?);
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::DropTuple { local } => {
                if !matches!(f.locals[local.0 as usize].ty, Type::Tuple(_))
                    || !budget.sum_type(&f.locals[local.0 as usize].ty)?
                    || i.result.is_some()
                {
                    return Err(error("DropTuple requires owning Tuple and no result"));
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::DropEnum { local } => {
                if !matches!(f.locals[local.0 as usize].ty, Type::Enum { .. })
                    || !budget.sum_type(&f.locals[local.0 as usize].ty)?
                    || i.result.is_some()
                {
                    return Err(error("DropEnum requires owning Enum and no result"));
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local)
            | InstructionKind::DropManaged { local }
            | InstructionKind::DropPackage { local }
            | InstructionKind::DropErrorUnion { local }
            | InstructionKind::DropClosure { local }
            | InstructionKind::DropOwnedArray { local } => {
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::VariantTag { value } | InstructionKind::ReadVariant { value, .. }
                if !f.values[value.0 as usize].is_copy_value()
                    && !crate::stored_flow::shared_enum_observation(module, f, i, budget)? =>
            {
                return Err(error("legacy Enum SSA observation requires Copy"));
            }
            _ => {}
        }
        let mut birth = None;
        if let Some(v) = i.result {
            let mut tree = match output {
                Some(t) => t,
                None => Tree::unknown(&f.values[v.0 as usize], budget)?,
            };
            if matches!(i.kind, InstructionKind::TaskCreate { .. } | InstructionKind::TaskCreateOwned { .. })
                || (matches!(i.kind,InstructionKind::Call{function,..} if module.functions.get(function.0 as usize).is_some_and(|f|f.result_mode==PassingMode::Create)))
            {
                match &mut edit(&mut tree, budget)?.node {
                    Node::Leaf(s) => *s = BTreeSet::from([Birth::Created]),
                    Node::Sum(vs) if managed_success(&f.values[v.0 as usize]) => {
                        if let Some(fs) = vs.get_mut(&0)
                            && let Some(t) = fs.first_mut()
                            && let Node::Leaf(s) = &mut edit(t, budget)?.node
                        {
                            *s = BTreeSet::from([Birth::Created]);
                        }
                    }
                    _ => {}
                }
            }
            if check && !tree.complete(budget)? {
                return Err(error("Sum result is incomplete"));
            }
            if let Some(s) = tree.birth() {
                birth = Some(s.clone());
            } else if managed_success(&f.values[v.0 as usize])
                && let Node::Sum(vs) = &tree.node
            {
                birth = Some(
                    vs.get(&0)
                        .and_then(|fs| fs.first())
                        .and_then(|t| t.birth())
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            facts.values.set(v, tree, budget)?;
        }
        Ok(birth)
    }
}
impl OldFacts {
    fn initial(
        module: &TypedModule,
        f: &TypedFunction,
        plan: &Plan,
        budget: &mut Budget,
    ) -> CompileResult<Self> {
        let mut facts = Self {
            locals: BTreeMap::new(),
            values: ssa_values::Values::default(),
            captures: BTreeMap::new(),
            valid: BTreeSet::new(),
            partial: plan
                .partial
                .as_ref()
                .map(|_| partial_places::State::default()),
        };
        budget.product(
            f.locals.len(),
            f.parameters.len(),
            1,
            "Sum parameter membership",
        )?;
        for l in &f.locals {
            budget.lookup(facts.locals.len(), "Sum local fact allocation")?;
            let mut t = Tree::unknown(&l.ty, budget)?;
            edit(&mut t, budget)?.missing = !f.parameters.contains(&l.id);
            facts.locals.insert(l.id, t);
        }
        facts.values = ssa_values::Values::build(f.values.len(), budget, |v, budget| {
            let mut t = Tree::unknown(&f.values[v], budget)?;
            edit(&mut t, budget)?.missing = true;
            Ok(t)
        })?;
        if let Some(id) = f.environment
            && budget.has_ordinary_store_sum_capture(id)?
        {
            let closure = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| error("Sum entry environment absent"))?;
            for (index, ty) in closure.captures.iter().enumerate() {
                if !budget.ordinary_store_sum_capture(id, ty)? {
                    continue;
                }
                budget.spend(1, "actual Sum capture root inventory")?;
                budget.lookup(facts.captures.len(), "Sum capture fact allocation")?;
                facts
                    .captures
                    .insert(index as u32, Tree::unknown(ty, budget)?);
            }
        }
        Ok(facts)
    }

    fn has_capture(&self, index: u32, budget: &mut Budget) -> CompileResult<bool> {
        if self.captures.is_empty() {
            return Ok(false);
        }
        budget.lookup(self.captures.len(), "Sum capture root membership")?;
        Ok(self.captures.contains_key(&index))
    }
    fn value(&self, v: ValueId, budget: &mut Budget) -> CompileResult<Arc<Tree>> {
        budget.lookup(self.values.len(), "Sum value source")?;
        self.values
            .get(&v)
            .ok_or_else(|| error("Sum value absent"))?
            .copied(budget)
    }
    fn local(&self, l: LocalId, budget: &mut Budget) -> CompileResult<Arc<Tree>> {
        budget.lookup(self.locals.len(), "Sum local source")?;
        self.locals
            .get(&l)
            .ok_or_else(|| error("Sum local absent"))?
            .copied(budget)
    }
    fn capture(&self, index: u32, budget: &mut Budget) -> CompileResult<Arc<Tree>> {
        budget.lookup(self.captures.len(), "Sum capture source")?;
        self.captures
            .get(&index)
            .ok_or_else(|| error("Sum capture absent"))?
            .copied(budget)
    }
    fn invalidate(&mut self, l: LocalId, plan: &OldPlan, budget: &mut Budget) -> CompileResult<()> {
        budget.spend(self.valid.len(), "Sum observation invalidation")?;
        self.valid.retain(|v| {
            plan.observations
                .get(v)
                .is_none_or(|(local, _)| *local != l)
        });
        Ok(())
    }

    fn from_new(facts: &Facts) -> Self {
        Self {
            locals: facts.locals.keyed(),
            values: facts.values.clone(),
            captures: facts.captures.clone(),
            valid: facts.valid.clone(),
            partial: facts.partial.clone(),
        }
    }
    fn assert_new(&self, facts: &Facts) {
        assert_eq!(self, &Self::from_new(facts));
    }
}

fn budget(n: usize, functions: usize) -> Budget {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    });
    b.sum_functions = vec![true; functions];
    b.pod_stores = true;
    b
}
fn compare_trace(module: &TypedModule, f: &TypedFunction) {
    for check in [false, true] {
        let mut setup = budget(10_000_000, module.functions.len());
        let plan = Plan::new(f, &mut setup).unwrap().unwrap();
        let old_plan = OldPlan::new(&plan, f);
        assert_eq!(old_plan.observations, plan.observations.keyed());
        let mut new = plan.initial(module, f, &mut setup).unwrap();
        let mut old = OldFacts::initial(
            module,
            f,
            &plan,
            &mut budget(10_000_000, module.functions.len()),
        )
        .unwrap();
        old.assert_new(&new);
        let mut a = budget(10_000_000, module.functions.len());
        let mut b = budget(10_000_000, module.functions.len());
        for block in &f.blocks {
            for instruction in &block.instructions {
                let before = new.clone();
                let snapshot = old.clone();
                let old_result = old_plan.apply(module, f, instruction, &mut old, check, &mut a);
                let new_result = plan.apply(module, f, instruction, &mut new, check, &mut b);
                assert_eq!(
                    old_result, new_result,
                    "transfer differs: {:?}",
                    instruction.kind
                );
                old.assert_new(&new);
                assert_eq!(
                    (a.tasks, a.registrations, a.generic_mutex),
                    (b.tasks, b.registrations, b.generic_mutex)
                );
                // Shared input snapshots retain their exact missing/birth trees.
                snapshot.assert_new(&before);
                if old_result.is_err() {
                    break;
                }
            }
            for successor in 0..f.blocks.len() {
                let mut left = old.clone();
                let mut right = new.clone();
                let x = old_plan.edge(f, block.id.0 as usize, successor, &mut left, &mut a);
                let y = plan.edge(f, block.id.0 as usize, successor, &mut right, &mut b);
                assert_eq!(x, y);
                left.assert_new(&right);
            }
        }
    }
}
#[test]
fn direct_transfer_matches_frozen_maps_for_ordinary_owner_and_fresh_caller() {
    for fresh in [false, true] {
        let module = crate::place_replacements::fixture::module_with_relay_and_caller(fresh);
        for f in &module.functions {
            compare_trace(&module, f);
        }
    }
}
fn observation_fixture() -> TypedModule {
    use crate::place_replacements::fixture::{instruction as i, module};
    let mut module = module();
    let owner = module.functions[0].locals[1].ty.clone();
    let sum = Type::Enum {
        name: "IndexedTag".into(),
        variants: vec![("A".into(), vec![Type::I32]), ("B".into(), vec![Type::I32])],
    };
    let f = &mut module.functions[0];
    f.parameters.clear();
    f.locals[0].ty = sum.clone();
    f.values = vec![
        Type::I32,
        sum,
        Type::I32,
        Type::I32,
        Type::Bool,
        owner.clone(),
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(owner.clone()),
        },
        owner,
    ];
    f.blocks[0].instructions = vec![
        i(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(9)),
        ),
        i(
            Some(ValueId(1)),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![ValueId(0)],
            },
        ),
        i(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(1),
            },
        ),
        i(
            Some(ValueId(2)),
            InstructionKind::VariantTagLocal {
                local: LocalId(0),
                path: vec![],
            },
        ),
        i(
            Some(ValueId(3)),
            InstructionKind::Constant(Constant::I32(0)),
        ),
        i(
            Some(ValueId(4)),
            InstructionKind::Binary {
                op: BinaryOp::Equal,
                left: ValueId(2),
                right: ValueId(3),
            },
        ),
        i(
            Some(ValueId(5)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
        i(
            Some(ValueId(6)),
            InstructionKind::MakeErrorUnion {
                value: ValueId(5),
                is_error: false,
            },
        ),
        i(
            Some(ValueId(7)),
            InstructionKind::ReadErrorUnion {
                value: ValueId(6),
                is_error: false,
            },
        ),
    ];
    f.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(4),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    for id in [1, 2] {
        f.blocks.push(BasicBlock {
            id: BlockId(id),
            instructions: vec![],
            terminator: Terminator::Return(None),
        });
    }
    module
}
#[test]
fn direct_observation_presence_edges_and_managed_success_match_frozen_maps() {
    let module = observation_fixture();
    compare_trace(&module, &module.functions[0]);
    let mut broken = module.clone();
    // A semantic shape error occurs after the result/operand prefix is valid.
    broken.functions[0].blocks[0].instructions[8].kind = InstructionKind::ReadErrorUnion {
        value: ValueId(0),
        is_error: false,
    };
    compare_trace(&broken, &broken.functions[0]);
}
#[test]
fn direct_local_and_sparse_observation_inventory_never_renumbers_keys() {
    let tree = Arc::new(Tree {
        missing: true,
        node: Node::Copy,
    });
    for len in [0, 1, 31, 32, 33, 64, 65] {
        let keyed: BTreeMap<_, _> = (0..len)
            .map(|id| (LocalId(id as u32), tree.clone()))
            .collect();
        let mut direct = direct_index::Locals::from_keyed(len, keyed.clone()).unwrap();
        assert_eq!(direct.keyed(), keyed);
        assert!(direct.get(&LocalId(len as u32)).is_none());
        assert!(direct.get_mut(&LocalId(len as u32)).is_none());
        let before = direct.clone();
        assert!(direct.set(LocalId(len as u32), tree.clone()).is_err());
        assert_eq!(before, direct);
        if len > 0 {
            let mut wrong = keyed.clone();
            let value = wrong.remove(&LocalId(0)).unwrap();
            wrong.insert(LocalId(len as u32), value);
            assert!(direct_index::Locals::from_keyed(len, wrong).is_err());
            let mut hole = keyed;
            hole.remove(&LocalId(0));
            assert!(direct_index::Locals::from_keyed(len, hole).is_err());
        }
        let mut b = budget(10_000_000, 1);
        let mut observations = direct_index::Observations::new(len, &mut b).unwrap();
        assert_eq!(observations.len(), 0);
        assert_eq!(observations.keyed(), BTreeMap::new());
        assert!(observations.get(&ValueId(len as u32)).is_none());
        assert!(
            observations
                .insert(ValueId(len as u32), (LocalId(0), vec![]))
                .is_err()
        );
        if len > 0 {
            let last = ValueId((len - 1) as u32);
            observations.insert(last, (LocalId(2), vec![])).unwrap();
            observations.insert(last, (LocalId(3), vec![])).unwrap();
            assert_eq!(observations.len(), 1);
            assert_eq!(observations.get(&last), Some(&(LocalId(3), vec![])));
        }
    }
    let mut module = crate::place_replacements::fixture::module();
    module.functions[0].locals[0].id = LocalId(8);
    assert!(
        direct_index::Locals::build(&module.functions[0], &mut budget(10_000, 1), |_, _| Ok(
            tree.clone()
        ))
        .is_err()
    );
}
#[test]
fn direct_simple_transfer_has_exact_request_boundary_and_keeps_complex_reserve() {
    use crate::place_replacements::fixture::instruction as inst;
    let module = observation_fixture();
    let f = &module.functions[0];
    let mut setup = budget(10_000_000, 1);
    let plan = Plan::new(f, &mut setup).unwrap().unwrap();
    let seed = plan.initial(&module, f, &mut setup).unwrap();
    let old_plan = OldPlan::new(&plan, f);
    let simple = inst(None, InstructionKind::StorageDead(LocalId(1)));
    let run = |old: bool, n| {
        let mut b = budget(n, 1);
        if old {
            old_plan.apply(
                &module,
                f,
                &simple,
                &mut OldFacts::from_new(&seed),
                false,
                &mut b,
            )
        } else {
            plan.apply(&module, f, &simple, &mut seed.clone(), false, &mut b)
        }
    };
    let boundary = |old| {
        let mut lo = 0;
        let mut hi = 100_000;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if run(old, mid).is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        lo
    };
    let n = boundary(false);
    let old_n = boundary(true);
    assert!(n < old_n, "{n} >= {old_n}");
    assert!(run(false, n).is_ok());
    assert!(run(false, n).is_ok());
    assert!(
        run(false, n - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );

    // Non-simple MakeVariant retains the same full logarithmic reserve;
    // only the paid direct/fallback classification adds one unit.
    let instruction = &f.blocks[0].instructions[1];
    let mut old = OldFacts::from_new(&seed);
    let mut new = seed.clone();
    let mut a = budget(100_000, 1);
    let mut b = budget(100_000, 1);
    assert_eq!(
        old_plan.apply(&module, f, instruction, &mut old, false, &mut a),
        plan.apply(&module, f, instruction, &mut new, false, &mut b)
    );
    old.assert_new(&new);
    assert_eq!(a.remaining(), b.remaining() + 1);

    // Meta::step still contains its own BTreeMap observer membership. A
    // result-bearing Constant is therefore deliberately NOT a direct branch.
    let mut partial_module = module.clone();
    partial_module.functions[0].values.push(Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    });
    partial_module.functions[0].blocks[0]
        .instructions
        .push(inst(
            Some(ValueId(8)),
            InstructionKind::BorrowPlaceLocal {
                local: LocalId(0),
                path: vec![PlaceProjection::Variant(VariantProjection {
                    variant: 0,
                    index: 0,
                })],
                mutable: false,
            },
        ));
    let f = &partial_module.functions[0];
    let mut setup = budget(10_000_000, 1);
    let plan = Plan::new(f, &mut setup).unwrap().unwrap();
    assert!(plan.partial.is_some());
    let seed = plan.initial(&partial_module, f, &mut setup).unwrap();
    let old_plan = OldPlan::new(&plan, f);
    let instruction = &f.blocks[0].instructions[0];
    let mut old = OldFacts::from_new(&seed);
    let mut new = seed.clone();
    let mut a = budget(100_000, 1);
    let mut b = budget(100_000, 1);
    assert_eq!(
        old_plan.apply(&partial_module, f, instruction, &mut old, false, &mut a),
        plan.apply(&partial_module, f, instruction, &mut new, false, &mut b)
    );
    old.assert_new(&new);
    assert_eq!(a.remaining(), b.remaining() + 1);
    let mut duplicate = partial_module.clone();
    duplicate.functions[0].blocks[0]
        .instructions
        .last_mut()
        .unwrap()
        .result = Some(ValueId(0));
    assert!(Plan::new(&duplicate.functions[0], &mut budget(100_000, 1)).is_err());
}
