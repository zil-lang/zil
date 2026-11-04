//! Contextual type lowering for actual Dyn syntax. This is nominal resolution,
//! not a capability or a replacement for independent dynamic table checking.
use super::*;

impl Expander<'_> {
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
        let Some(catalog) = self.dynamic_catalog.filter(|catalog| !catalog.is_empty()) else {
            return self.type_ref(ty);
        };
        let limits = Limits {
            max_analysis_steps: self
                .member_owner_limit
                .saturating_sub(self.member_owner_work),
            max_nodes: self.member_owner_nodes,
            max_call_depth: self.member_owner_depth,
            ..Limits::default()
        };
        let mut budget = generics::Budget::new(&limits);
        let result = catalog
            .contains_type(ty, &mut budget)
            .and_then(|contains| {
                if contains {
                    catalog.core_type(ty, &mut budget, 0).map(Some)
                } else {
                    Ok(None)
                }
            })
            .and_then(|result| {
                if let Some(lowered) = &result {
                    generics::dynamic::source::prepay_substitute_self(
                        lowered,
                        self.self_type.as_ref(),
                        &mut budget,
                        ty.span,
                        0,
                    )?;
                }
                Ok(result)
            });
        let left = budget.remaining_limits();
        self.member_owner_work = self
            .member_owner_work
            .saturating_add(limits.max_analysis_steps - left.max_analysis_steps);
        self.member_owner_nodes = left.max_nodes;
        match result {
            Ok(Some(ty)) => receivers::substitute_self(ty, self.self_type.as_ref()),
            Ok(None) => self.type_ref(ty),
            Err(error) => {
                self.diagnostics.push(error);
                Type::Unsupported("dyn contextual type resolution failed".into())
            }
        }
    }
}
