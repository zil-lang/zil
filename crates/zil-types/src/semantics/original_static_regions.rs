//! Static inputs satisfy one directed call boundary. Literal identities come
//! from the immutable original occurrence directory, never from result types
//! or an arbitrary numeric class. No static source creates a LifeEqual fact.
use super::*;

impl Check<'_, '_> {
    pub(super) fn static_call_input<'a, 'e>(
        &mut self,
        owner: SemanticDefinitionId,
        actual: &'a SemanticTypeRecipe,
        expected: &'e SemanticTypeRecipe,
        depth: usize,
    ) -> R<
        Option<(
            &'e SemanticOuterLifeRecipe,
            &'a SemanticTypeRecipe,
            &'e SemanticTypeRecipe,
        )>,
    > {
        self.pay(1, depth)?;
        if let (
            SemanticTypeRecipe::Address {
                mutable: false,
                lifetime,
                inner: actual,
            },
            SemanticTypeRecipe::Address {
                mutable: false,
                lifetime: expected_life,
                inner: expected,
            },
        ) = (actual, expected)
        {
            if self.static_outer_source(owner, lifetime, depth + 1)? {
                return Ok(Some((expected_life, actual, expected)));
            }
        }
        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn solve_static_call_region(
        &mut self,
        owner: SemanticDefinitionId,
        argument: &SemanticOriginalExpr,
        expected: &SemanticOuterLifeRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(u32::MAX),
            ordinal,
        })) = expected
        {
            if !matches!(
                formals.get(*ordinal as usize),
                Some(SemanticFormal::Lifetime)
            ) {
                return Err(bad("static input selects a non-Life mixed formal"));
            }
            if let Some(actual) = solutions.get(*ordinal as usize).and_then(Option::as_ref) {
                if !matches!(actual, SemanticActualRecipe::Life(_)) {
                    return Err(bad("static input changes its solved mixed formal kind"));
                }
                // The real other inputs/written actuals already selected the
                // region. Static may be shortened to it without changing it.
                return Ok(());
            }
            let mut occurrence = argument;
            while let CoreExprKind::Group(inner) = &occurrence.kind {
                self.pay(1, depth)?;
                occurrence = inner;
            }
            // borrow_life authenticates the original TypeUse site and its
            // full occurrence_path against the original implicit directory.
            // The expression itself keeps its independently checked Static
            // type/source; only this fresh callee variable takes the region.
            let region = self.borrow_life(owner, occurrence.id)?;
            let SemanticOuterLifeRecipe::Class(region) = region else {
                return Err(bad(
                    "static input lacks its real shortening region occurrence",
                ));
            };
            self.bind_actual(
                *ordinal,
                SemanticActualRecipe::Life(region),
                solutions,
                depth + 1,
            )?;
            return Ok(());
        }
        if matches!(
            expected,
            SemanticOuterLifeRecipe::Class(
                SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ..
                } | SemanticLifeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ..
                }
            )
        ) {
            // An elided raw outer role already transports a Static endpoint;
            // it is not an original generic Life actual or persistent class.
            return self.unify_outer(
                expected,
                &SemanticOuterLifeRecipe::Static,
                formals,
                solutions,
                implicit,
                invocation,
                depth + 1,
            );
        }
        // A rigid caller/written endpoint is only an upper bound here. The
        // static source was proved before entering this routine; neither the
        // endpoint nor its inner/nominal class domain is rewritten.
        Ok(())
    }
}
