//! Contextual type lowering for actual Dyn syntax. This is nominal resolution,
//! not a capability or a replacement for independent dynamic table checking.
use super::*;

impl Expander<'_> {
    pub(super) fn source_flat_life(
        &mut self,
        life: zil_ir::FunctionLifeContract,
        span: Span,
    ) -> Option<zil_ir::StorageLifeContract> {
        let limits = Limits {
            max_analysis_steps: self
                .member_owner_limit
                .saturating_sub(self.member_owner_work),
            max_nodes: self.member_owner_nodes,
            max_call_depth: self.member_owner_depth,
            ..Limits::default()
        };
        let mut budget = generics::Budget::new(&limits);
        let result = (|| {
            for binder in &life.parameter_lives {
                budget.node(span, 0)?;
                if binder.is_some() {
                    budget.node(span, 0)?;
                }
            }
            budget.node(span, 0)?;
            if life.result_life.is_some() {
                budget.node(span, 0)?;
            }
            zil_ir::StorageLifeContract::from_flat(life).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "callable relation arity exhausted",
                    Some(span),
                )
            })
        })();
        let left = budget.remaining_limits();
        self.member_owner_work += limits.max_analysis_steps - left.max_analysis_steps;
        self.member_owner_nodes = left.max_nodes;
        match result {
            Ok(life) => Some(life),
            Err(error) => {
                self.diagnostics.push(error);
                None
            }
        }
    }

    pub(super) fn record_dyn_method(&mut self, member: usize, function: NodeId, span: Span) {
        if self
            .dynamic_catalog
            .is_none_or(|catalog| catalog.is_empty())
        {
            return;
        }
        if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Expand,
                "dyn method emission ledger budget exhausted",
                Some(span),
            ));
            return;
        }
        self.member_owner_work += 1;
        self.member_owner_nodes -= 1;
        self.dyn_method_emissions
            .push((self.source_item_ordinal, member, function));
    }

    pub(super) fn dyn_type_ref(&mut self, ty: &s::TypeRef) -> Type {
        let catalog = self.dynamic_catalog.filter(|catalog| !catalog.is_empty());
        let storage = self.storage_catalog.filter(|catalog| {
            !catalog.pointers.is_empty()
                || catalog.borrowed_signatures && !catalog.instances.is_empty()
        });
        if catalog.is_none() && storage.is_none() {
            return self.type_ref(ty);
        }
        let limits = Limits {
            max_analysis_steps: self
                .member_owner_limit
                .saturating_sub(self.member_owner_work),
            max_nodes: self.member_owner_nodes,
            max_call_depth: self.member_owner_depth,
            ..Limits::default()
        };
        let mut budget = generics::Budget::new(&limits);
        let result = (|| {
            if let Some(storage) = storage
                && let Some(pointer) = storage.pointer_type(ty, &mut budget)?
            {
                return Ok(Some((pointer, false)));
            }
            let Some(catalog) = catalog else {
                return Ok(None);
            };
            if !catalog.contains_type(ty, &mut budget)? {
                return Ok(None);
            }
            let lowered = catalog.core_type(ty, &mut budget, 0)?;
            generics::dynamic::source::prepay_substitute_self(
                &lowered,
                self.self_type.as_ref(),
                &mut budget,
                ty.span,
                0,
            )?;
            Ok(Some((lowered, true)))
        })();
        let left = budget.remaining_limits();
        self.member_owner_work = self
            .member_owner_work
            .saturating_add(limits.max_analysis_steps - left.max_analysis_steps);
        self.member_owner_nodes = left.max_nodes;
        match result {
            Ok(Some((ty, true))) => receivers::substitute_self(ty, self.self_type.as_ref()),
            Ok(Some((ty, false))) => ty,
            Ok(None) => self.type_ref(ty),
            Err(error) => {
                self.diagnostics.push(error);
                Type::Unsupported("dyn contextual type resolution failed".into())
            }
        }
    }
}
