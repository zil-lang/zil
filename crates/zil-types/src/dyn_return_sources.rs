//! Finite typed source uppers for returned dyn dependency envelopes.
//! These are proposals only. Analyze and Verify reconstruct every actual Return
//! and substitute actual call permissions independently.
use super::*;

type Leaf<'a> = (StoragePosition, &'a Type, StoredBorrowMode, StorageBinder);
struct Walk<'a, 'b> {
    interfaces: &'a [DynInterface],
    contract: &'a StorageLifeContract,
    limits: &'a Limits,
    left: &'b mut usize,
    span: Span,
    result_context: bool,
}
impl<'a, 'b> Walk<'a, 'b> {
    fn pay(&mut self, n: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "dyn retained result proposal work exceeded",
                Some(self.span),
            )
        })?;
        Ok(())
    }
    fn class(
        &mut self,
        base: StorageBase,
        path: &[LifeTypeProjection],
        class: u32,
    ) -> R<StorageBinder> {
        self.pay(
            self.contract
                .classes
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let mut rows = self.contract.classes.iter().filter(|row| {
            row.position.base == base && row.position.path == path && row.position.class == class
        });
        let row = rows.next().ok_or_else(|| {
            unsupported(
                self.span,
                "dyn result lacks an actual complete source class relation",
            )
        })?;
        if rows.next().is_some() {
            return Err(unsupported(
                self.span,
                "dyn result has ambiguous source class relation",
            ));
        }
        Ok(row.binder)
    }
    fn inline_places(
        &mut self,
        ty: &Type,
        base: StorageBase,
        wanted: &Type,
        mode: StoredBorrowMode,
        binder: StorageBinder,
        sources: &mut Vec<StorageSource>,
    ) -> R<()> {
        let Type::Address { mutable, inner } = ty else {
            return Ok(());
        };
        let Type::Address {
            mutable: output_mutable,
            inner: output_inner,
        } = wanted
        else {
            return Ok(());
        };
        if mode
            != if *output_mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            }
            || *output_mutable && !mutable
        {
            return Ok(());
        }
        self.pay(self.contract.bindings.len())?;
        let mut bindings = self
            .contract
            .bindings
            .iter()
            .filter(|row| row.position.base == base && row.position.path.is_empty());
        let Some(row) = bindings.next() else {
            return Ok(());
        };
        if bindings.next().is_some() || row.binder != binder {
            return Ok(());
        }
        // An address-of place crosses only real inline fields. It never loads
        // a stored reference or traverses an owned manager/pointer boundary.
        let mut pending = vec![(inner.as_ref(), Vec::<ContractProjection>::new(), 0usize)];
        while let Some((node, path, depth)) = pending.pop() {
            self.pay(path.len().saturating_add(1))?;
            if depth > self.limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "dyn inline place proposal depth exceeded",
                    Some(self.span),
                ));
            }
            inference::charge_type_snapshot(node, self.span, self.limits, self.left, 1)?;
            inference::charge_type_snapshot(output_inner, self.span, self.limits, self.left, 1)?;
            if !path.is_empty()
                && node == output_inner.as_ref()
                && (node.is_b0_scalar() || *node == Type::Unit)
            {
                self.pay(path.len().saturating_add(1))?;
                sources.push(StorageSource::PlaceAddress {
                    place: StoragePosition {
                        base,
                        path: path.clone(),
                    },
                    mode,
                });
            }
            match node {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    self.pay(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                    for (index, (_, child)) in fields.iter().enumerate().rev() {
                        let mut selected = path.clone();
                        selected.push(ContractProjection::Place(PlaceProjection::Field(
                            index as u32,
                        )));
                        pending.push((child, selected, depth + 1));
                    }
                }
                Type::Tuple(fields) => {
                    self.pay(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                    for (index, child) in fields.iter().enumerate().rev() {
                        let mut selected = path.clone();
                        selected.push(ContractProjection::Place(PlaceProjection::Field(
                            index as u32,
                        )));
                        pending.push((child, selected, depth + 1));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn ty(
        &mut self,
        ty: &'a Type,
        base: StorageBase,
        cp: &mut Vec<LifeTypeProjection>,
        path: &mut Vec<ContractProjection>,
        output: bool,
        out: &mut Vec<Leaf<'a>>,
        depth: usize,
    ) -> R<()> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dyn result proposal type depth exceeded",
                Some(self.span),
            ));
        }
        match ty {
            Type::OwnedDyn { interface } | Type::Dyn { interface } => {
                self.pay(self.interfaces.len())?;
                let item = self
                    .interfaces
                    .iter()
                    .find(|item| item.id == *interface)
                    .ok_or_else(|| {
                        unsupported(self.span, "dyn result source interface is absent")
                    })?;
                if item.context.is_none() {
                    return Ok(());
                }
                self.result_context |= output;
                for (index, port) in item.retained_life.ports.iter().enumerate() {
                    let binder = self.class(base, cp, port.binder)?;
                    self.pay(path.len().saturating_add(2))?;
                    let mut position = path.clone();
                    position.push(ContractProjection::RetainedPort(index as u32));
                    out.push((
                        StoragePosition {
                            base,
                            path: position,
                        },
                        &port.leaf,
                        port.mode,
                        binder,
                    ));
                }
            }
            Type::BorrowedStorage { life, .. } if !output => {
                for slot in &life.slots {
                    let binder = self.class(base, cp, slot.binder)?;
                    let mut node = ty;
                    for selector in &slot.path {
                        self.pay(1)?;
                        if let Type::BorrowedStorage { storage, .. } = node {
                            node = storage;
                        }
                        node = match (node, selector) {
                            (
                                Type::Record { fields, .. } | Type::Package { fields, .. },
                                PlaceProjection::Field(i),
                            ) => fields.get(*i as usize).map(|(_, t)| t),
                            (Type::Tuple(fields), PlaceProjection::Field(i)) => {
                                fields.get(*i as usize)
                            }
                            (Type::Array { element, length }, PlaceProjection::Field(i))
                                if *i < *length =>
                            {
                                Some(element.as_ref())
                            }
                            (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                                .get(p.variant as usize)
                                .and_then(|(_, f)| f.get(p.index as usize)),
                            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                                Some(if *e { error.as_ref() } else { result.as_ref() })
                            }
                            _ => None,
                        }
                        .ok_or_else(|| {
                            unsupported(
                                self.span,
                                "dyn source class slot has no actual inline leaf",
                            )
                        })?;
                    }
                    self.pay(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                    let mut position = path.clone();
                    position.extend(slot.path.iter().copied().map(ContractProjection::Place));
                    out.push((
                        StoragePosition {
                            base,
                            path: position,
                        },
                        node,
                        slot.mode,
                        binder,
                    ));
                }
            }
            Type::Address { inner, mutable } => {
                if !output
                    && !matches!(
                        inner.as_ref(),
                        Type::BorrowedStorage { .. } | Type::OwnedDyn { .. } | Type::Dyn { .. }
                    )
                {
                    self.pay(
                        self.contract
                            .bindings
                            .len()
                            .saturating_mul(path.len().saturating_add(1)),
                    )?;
                    if let Some(row) = self
                        .contract
                        .bindings
                        .iter()
                        .find(|row| row.position.base == base && row.position.path == *path)
                    {
                        self.pay(path.len().saturating_add(1))?;
                        out.push((
                            StoragePosition {
                                base,
                                path: path.clone(),
                            },
                            ty,
                            if *mutable {
                                StoredBorrowMode::ExclusiveAddress
                            } else {
                                StoredBorrowMode::SharedAddress
                            },
                            row.binder,
                        ));
                    }
                }
                cp.push(LifeTypeProjection::Pointee);
                self.ty(inner, base, cp, path, output, out, depth + 1)?;
                cp.pop();
            }
            Type::ErrorUnion { error, result } => {
                for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let selector = PlaceProjection::ErrorArm(arm);
                    cp.push(LifeTypeProjection::Place(selector));
                    path.push(ContractProjection::Place(selector));
                    self.ty(child, base, cp, path, output, out, depth + 1)?;
                    path.pop();
                    cp.pop();
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, child)) in fields.iter().enumerate() {
                    let selector = PlaceProjection::Field(index as u32);
                    cp.push(LifeTypeProjection::Place(selector));
                    path.push(ContractProjection::Place(selector));
                    self.ty(child, base, cp, path, output, out, depth + 1)?;
                    path.pop();
                    cp.pop();
                }
            }
            Type::Tuple(fields) => {
                for (index, child) in fields.iter().enumerate() {
                    let selector = PlaceProjection::Field(index as u32);
                    cp.push(LifeTypeProjection::Place(selector));
                    path.push(ContractProjection::Place(selector));
                    self.ty(child, base, cp, path, output, out, depth + 1)?;
                    path.pop();
                    cp.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
pub(super) fn propose<'a>(
    f: &'a TypedFunction,
    interfaces: &'a [DynInterface],
    limits: &Limits,
    left: &mut usize,
) -> R<Option<Vec<StorageSourceBinding>>> {
    // The module's existing paid inventory authenticates every interface. Empty
    // legacy modules incur no new signature walk or proposed authorization.
    if interfaces.is_empty() {
        if f.retained_return_sources.is_some() {
            return Err(unsupported(
                f.span,
                "explicit retained result claim has no dyn Context consumer",
            ));
        }
        return Ok(None);
    }
    *left = left.checked_sub(interfaces.len()).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "dyn result interface inventory work exceeded",
            Some(f.span),
        )
    })?;
    if !interfaces.iter().any(|item| item.context.is_some()) {
        if f.retained_return_sources.is_some() {
            return Err(unsupported(
                f.span,
                "explicit retained result claim has no dyn Context consumer",
            ));
        }
        return Ok(None);
    }
    let Some(contract) = &f.storage_life else {
        if f.retained_return_sources.is_some() {
            return Err(unsupported(
                f.span,
                "dyn retained result requires its complete class signature",
            ));
        }
        return Ok(None);
    };
    let mut walk = Walk {
        interfaces,
        contract,
        limits,
        left,
        span: f.span,
        result_context: false,
    };
    let mut result = Vec::new();
    walk.ty(
        &f.return_type,
        StorageBase::Result,
        &mut vec![],
        &mut vec![],
        true,
        &mut result,
        0,
    )?;
    if result.is_empty() && !walk.result_context {
        if f.retained_return_sources.is_some() {
            return Err(unsupported(
                f.span,
                "explicit retained result claim has no nonempty dyn Context result",
            ));
        }
        return Ok(None);
    }
    let mut parameters = Vec::new();
    for (index, local) in f.parameters.iter().enumerate() {
        let ty = &f
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| unsupported(f.span, "dyn source input local is absent"))?
            .ty;
        walk.ty(
            ty,
            StorageBase::Parameter(index as u32),
            &mut vec![],
            &mut vec![],
            false,
            &mut parameters,
            0,
        )?;
    }
    let mut proposed = Vec::new();
    for (position, leaf, mode, binder) in result {
        let mut sources = Vec::new();
        for (source, source_type, source_mode, source_binder) in &parameters {
            inference::charge_type_snapshot(leaf, f.span, limits, walk.left, 2)?;
            inference::charge_type_snapshot(source_type, f.span, limits, walk.left, 2)?;
            if mode == *source_mode && binder == *source_binder && leaf == *source_type {
                walk.pay(source.path.len().saturating_add(1))?;
                sources.push(StorageSource::Exact(source.clone()));
            }
        }
        for (index, local) in f.parameters.iter().enumerate() {
            walk.inline_places(
                &f.locals[local.0 as usize].ty,
                StorageBase::Parameter(index as u32),
                leaf,
                mode,
                binder,
                &mut sources,
            )?;
        }
        walk.pay(position.path.len().saturating_add(1))?;
        proposed.push(StorageSourceBinding { position, sources });
    }
    // Written raw Core claims are retained exactly and independently checked;
    // only absent producer proposals are synthesized from the typed header.
    if let Some(written) = &f.retained_return_sources {
        inference::charge_return_sources(written, limits, f.span, |n| walk.pay(n))?;
        return Ok(Some(written.clone()));
    }
    Ok(Some(proposed))
}
