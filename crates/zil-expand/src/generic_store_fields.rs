//! A mutable reference can replace an actual managed Store field without
//! becoming the owner of its complete Package or permitting a field give.
use super::*;

impl Check<'_, '_, '_> {
    pub(super) fn borrowed_store_field_slots(
        &mut self,
        object: &Expr,
        member: &str,
        span: Span,
        state: &State,
    ) -> R<Option<BTreeSet<usize>>> {
        let mut direct = object;
        while let ExprKind::Group(inner) = &direct.kind {
            direct = inner;
        }
        if let ExprKind::Unary {
            op: UnaryOp::Dereference,
            operand,
        } = &direct.kind
        {
            self.budget.node(direct.span, 0)?;
            direct = operand;
            while let ExprKind::Group(inner) = &direct.kind {
                self.budget.node(direct.span, 0)?;
                direct = inner;
            }
        }
        let ExprKind::Name(name) = &direct.kind else {
            return Ok(None);
        };
        let Some(local) = state.locals.get(name) else {
            return Ok(None);
        };
        let TypeKind::Reference {
            inner,
            mutable: true,
            ..
        } = &group(&local.ty).kind
        else {
            return Ok(None);
        };
        let TypeKind::Named { path, args } = &group(inner).kind else {
            return Ok(None);
        };
        let Some(Item {
            kind: ItemKind::Aggregate(aggregate),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            return Ok(None);
        };
        if aggregate.kind != AggregateKind::Package {
            return Ok(None);
        }
        self.budget.node(span, 0)?;
        for field in &aggregate.fields {
            self.budget.node(field.span, 0)?;
            self.budget.text(&field.name, field.span)?;
            self.budget.text(member, span)?;
        }
        let declared = aggregate
            .fields
            .iter()
            .find(|field| field.name == member)
            .ok_or_else(|| error(span, "unknown borrowed Package field"))?;
        if !declared.public && declared.span.file != span.file {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-FIELD-PRIVATE",
                Some(span),
                "record field is private to its declaring module",
            ));
        }
        if !declared.manages {
            return Ok(None);
        }
        if aggregate.generics.len() != args.len() {
            return Err(error(span, "generic nominal arity mismatch"));
        }
        let actual = nominal_field_type(
            &aggregate.generics,
            args,
            &declared.ty,
            self.source_definition,
            self.budget,
        )?;
        if stores::element(&actual).is_none() {
            return Ok(None);
        }
        // Origins use the referent's existing alias identity, including an
        // alias/reborrow of this address. The pointer local is never an owner.
        self.alias_store_field_slots(object, member, state, span)
            .map(Some)
    }

    pub(super) fn borrowed_store_origins(
        &mut self,
        slots: &BTreeSet<usize>,
        state: &State,
        span: Span,
    ) -> R<BTreeSet<usize>> {
        let mut origins = BTreeSet::new();
        for slot in slots {
            for _ in 0..state
                .store_origins
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
            {
                self.budget.node(span, 0)?;
            }
            if let Some(roots) = state.store_origins.get(slot) {
                for root in roots {
                    for _ in 0..origins.len().checked_ilog2().map_or(1, |n| n as usize + 2) {
                        self.budget.node(span, 0)?;
                    }
                    self.budget.node(span, 0)?;
                    origins.insert(*root);
                }
            } else {
                for _ in 0..origins.len().checked_ilog2().map_or(1, |n| n as usize + 2) {
                    self.budget.node(span, 0)?;
                }
                self.budget.node(span, 0)?;
                origins.insert(*slot);
            }
        }
        Ok(origins)
    }
}
