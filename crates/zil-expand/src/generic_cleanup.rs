//! Definition-time cleanup obligations for ordinary Value binders. The concrete
//! cores still compute actual destruction/state/loans at every executable edge.
use super::*;
impl Check<'_, '_, '_> {
    fn needs_value_cleanup(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        Ok(match &group(ty).kind {
            TypeKind::Reference { .. } => false,
            TypeKind::Named { path, args } => {
                let mut needed = path.len() == 1
                    && self.variables.contains(&path[0])
                    && self.bounds.get(&path[0]).is_some_and(|bounds| {
                        bounds
                            .iter()
                            .any(|bound| matches!(bound, trait_bounds::Bound::Value | trait_bounds::Bound::StoreElement | trait_bounds::Bound::SyncDrop))
                    });
                for arg in args {
                    if let GenericArg::Type(ty) = arg {
                        needed |= self.needs_value_cleanup(ty, depth + 1)?;
                    }
                }
                needed
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                let mut needed = false;
                for field in fields {
                    needed |= self.needs_value_cleanup(field, depth + 1)?;
                }
                needed
            }
            TypeKind::Array { element, .. } => self.needs_value_cleanup(element, depth + 1)?,
            TypeKind::Error { error, result } => {
                self.needs_value_cleanup(error, depth + 1)?
                    || self.needs_value_cleanup(result, depth + 1)?
            }
            _ => false,
        })
    }
    pub(super) fn value_cleanup_type(&mut self, ty: &TypeRef) -> R<()> {
        if self.needs_value_cleanup(ty, 0)? {
            let owning;
            let ty = if stores::element(ty).is_some() {
                self.budget.ty(ty)?;
                owning = TypeRef {
                    kind: TypeKind::Named {
                        path: vec!["Managed".into()],
                        args: vec![GenericArg::Type(ty.clone())],
                    },
                    span: ty.span,
                };
                &owning
            } else {
                ty
            };
            self.traits.value_bound(
                ty,
                &trait_bounds::Bound::SyncDrop,
                &self.bounds,
                &self.variables,
                self.budget,
            )?;
        }
        Ok(())
    }
    pub(super) fn value_cleanup_local(&mut self, local: &Local, state: &State) -> R<()> {
        if !self.needs_value_cleanup(&local.ty, 0)? {
            return Ok(());
        }
        if let TypeKind::Named { path, args } = &group(&local.ty).kind
            && let Some(Item {
                kind: ItemKind::Aggregate(aggregate),
                ..
            }) = self.nominals.get(&path.join("."))
            && aggregate.kind == AggregateKind::Package
        {
            for field in &aggregate.fields {
                self.budget.node(field.span, 0)?;
                self.budget.text(&field.name, field.span)?;
                if self
                    .package_slots
                    .get(&(local.id, field.name.clone()))
                    .is_some_and(|slot| state.moved.contains(slot))
                {
                    continue;
                }
                let ty = nominal_field_type(
                    &aggregate.generics,
                    args,
                    &field.ty,
                    self.source_definition,
                    self.budget,
                )?;
                self.value_cleanup_type(&ty)?;
            }
            return Ok(());
        }
        self.value_cleanup_type(&local.ty)
    }
    pub(super) fn value_cleanup_exit(&mut self, state: &State, first: usize, span: Span) -> R<()> {
        for local in state.locals.values() {
            self.budget.node(span, 0)?;
            if local.id >= first && !state.moved.contains(&local.id) {
                self.value_cleanup_local(local, state)?;
            }
        }
        Ok(())
    }
}
