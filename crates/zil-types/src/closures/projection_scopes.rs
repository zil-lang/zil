//! Producer snapshots only. Neither this inventory nor the projected candidate
//! supplies a target, phase, owner cleanup or lifetime proof to either kernel.
use super::*;

fn references(ty: &Type, ids: &mut BTreeSet<ClosureId>, left: &mut usize, span: Span) -> R<()> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        *left = left
            .checked_sub(1)
            .ok_or_else(|| limit("Source projection scope traversal exceeded", span))?;
        match ty {
            Type::Closure(id) => {
                *left = left
                    .checked_sub(ids.len().saturating_add(2))
                    .ok_or_else(|| limit("Source projection scope identity work exceeded", span))?;
                ids.insert(*id);
            }
            Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::BorrowedStorage { storage: t, .. }
            | Type::Registration { environment: t }
            | Type::AsyncRegistration { environment: t }
            | Type::Opaque { hidden: t, .. }
            | Type::Array { element: t, .. }
            | Type::Slice { element: t }
            | Type::OwnedStore { element: t }
            | Type::Mutex { payload: t } => pending.push(t),
            Type::Task {
                result,
                retained_life,
            } => {
                pending.push(result);
                pending.extend(retained_life.ports.iter().map(|p| &p.leaf));
            }
            Type::MutexGuard {
                payload,
                retained_life,
            } => {
                pending.push(payload);
                pending.extend(retained_life.ports.iter().map(|p| &p.leaf));
            }
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}
impl State {
    pub(super) fn projection_scope_definitions(
        &mut self,
        parent: &Type,
        span: Span,
    ) -> R<Vec<ClosureDefinition>> {
        self.snapshot_type(parent, 1, 0, span)?;
        let mut needed = BTreeSet::new();
        references(parent, &mut needed, &mut self.work_left, span)?;
        let mut done = BTreeSet::new();
        let mut definitions = Vec::new();
        while let Some(id) = needed.iter().copied().find(|id| !done.contains(id)) {
            self.snapshot(
                needed.len().saturating_add(done.len()).saturating_add(2),
                span,
            )?;
            done.insert(id);
            let pending = self.pending.get(id.0 as usize).ok_or_else(|| {
                unsupported(span, "projection actual closure definition is absent")
            })?;
            let typed = pending.typed.as_ref().ok_or_else(|| {
                unsupported(span, "projection actual closure entry is unresolved")
            })?;
            if typed.environment != Some(id) || typed.id != pending.function {
                return Err(unsupported(
                    span,
                    "projection closure does not match its actual resolved entry",
                ));
            }
            for capture in &pending.captures {
                inference::charge_type_snapshot(
                    &capture.ty,
                    span,
                    &self.limits,
                    &mut self.work_left,
                    3,
                )?;
                references(&capture.ty, &mut needed, &mut self.work_left, span)?;
            }
            for parameter in &typed.parameters {
                let ty = &typed.locals[parameter.0 as usize].ty;
                inference::charge_type_snapshot(ty, span, &self.limits, &mut self.work_left, 3)?;
                references(ty, &mut needed, &mut self.work_left, span)?;
            }
            inference::charge_type_snapshot(
                &typed.return_type,
                span,
                &self.limits,
                &mut self.work_left,
                3,
            )?;
            references(&typed.return_type, &mut needed, &mut self.work_left, span)?;
            if let Some(life) = &typed.storage_life {
                self.work_left = self
                    .work_left
                    .checked_sub(borrowed_fp::metadata_work(life).saturating_mul(3))
                    .ok_or_else(|| {
                        limit(
                            "Source projection actual closure relation snapshot exceeded",
                            span,
                        )
                    })?;
            }
            definitions.push(ClosureDefinition {
                id,
                function: typed.id,
                captures: pending.captures.iter().map(Capture::stored_type).collect(),
                parameters: typed
                    .parameters
                    .iter()
                    .map(|local| typed.locals[local.0 as usize].ty.clone())
                    .collect(),
                result: typed.return_type.clone(),
                storage_life: typed.storage_life.clone(),
                kind: pending.callable_kind(),
                lending_captures: Vec::new(),
            });
        }
        Ok(definitions)
    }
}
