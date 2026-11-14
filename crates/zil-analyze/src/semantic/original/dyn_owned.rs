//! Owned erasure consumes its complete original Self on both outcomes. The
//! owner's cleanup contract cannot be exported as a property of that Self.
use super::*;
impl Solver<'_, '_> {
    pub(super) fn owned_dyn_sync(
        &self,
        ty: &dyn_types::DynType,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        if !ty.owned {
            return Ok(false);
        }
        if !ty.arguments.is_empty()
            || !ty.associated.is_empty()
            || !matches!(ty.context, dyn_types::Context::Closed)
        {
            return Err(unknown(
                "original owning Dyn cleanup needs its complete scoped canonical bound key",
            ));
        }
        let mut selected = None;
        for interface in &self.inventory.module.dyn_interfaces {
            b.spend(1, WORK)?;
            crate::dyn_metadata::recipe(&interface.application, b)?;
            crate::dyn_metadata::retained_shape(&interface.retained_life, b)?;
            if interface.application.declaration != ty.declaration
                || !interface.application.actuals.is_empty()
                || interface.context.is_some()
                || interface.retained_life.binder_count != 0
                || !interface.retained_life.ports.is_empty()
                || !interface.retained_life.retained.is_empty()
                || !interface.retained_life.nominal_arguments.is_empty()
                || !interface.associated_types.is_empty()
                || !interface.associated_constants.is_empty()
            {
                continue;
            }
            crate::dyn_cleanup::metadata(interface.owning_cleanup.as_ref(), b)?;
            crate::dyn_cleanup::bound(interface, b)?;
            if selected.replace(interface.id).is_some() {
                return Err(unknown(
                    "original owning Dyn cleanup has multiple canonical interface relocations",
                ));
            }
        }
        if selected.is_none() {
            return Err(unknown(
                "original owning Dyn type has no complete canonical synchronous owner bound",
            ));
        }
        // This is a required invariant of the concrete builtin owner type.
        // All actual tables still need exact concrete Direct cleanup proof in
        // the actual bridge; no successful analysis is published before it.
        Ok(true)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn owned_dyn_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let [CoreGenericArgument::Type(interface)] = written else {
            return Err(unknown(
                "original owned Dyn construction needs its full written trait and Context recipe",
            ));
        };
        let [source] = arguments else {
            return Err(fail(
                "original owned Dyn construction consumes exactly one complete Self",
            ));
        };
        let interface = self.recipe(frame, interface, b, depth + 1)?;
        let root = self.type_root(interface, b)?;
        let Kind::Dyn(ty) = &self.state.arena[root] else {
            return Err(fail(
                "original owned Dyn construction has another trait type argument",
            ));
        };
        b.spend(ty.work(), WORK)?;
        let mut ty = ty.clone();
        if ty.owned {
            return Err(fail(
                "original owned Dyn construction expects the borrowed trait type recipe",
            ));
        }
        ty.owned = true;
        self.owned_dyn_sync(&ty, b)?;
        let source = self.expression(frame, source, b, depth + 1)?;
        let (implementation, _) =
            self.original_provider(frame, source, ty.declaration, &ty.arguments, b, depth + 1)?;
        for predicate in [Predicate::Value, Predicate::SyncDrop] {
            Self::push(
                &mut self.needs,
                Need {
                    term: source,
                    predicate,
                },
                b,
            )?;
        }
        if !ty.associated.is_empty() {
            return Err(unknown(
                "original owned Dyn provider must prove each associated equality before erasure",
            ));
        }
        b.lookup(self.state.native_targets.len(), WORK)?;
        if self
            .state
            .native_targets
            .insert((frame.definition, node), vec![implementation])
            .is_some()
        {
            return Err(fail(
                "original owned Dyn construction repeats its exact provider",
            ));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        if self
            .state
            .intrinsic_inputs
            .insert((frame.definition, node), vec![source])
            .is_some()
        {
            return Err(fail(
                "original owned Dyn construction repeats its complete Self input",
            ));
        }
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), sealed::Intrinsic::OwnedDyn)
            .is_some()
        {
            return Err(fail(
                "original owned Dyn construction repeats its sealed identity",
            ));
        }
        let error = self.atom(crate::dyn_dispatch::create_error(), b)?;
        let owner = self.term(Kind::Dyn(ty), b)?;
        self.term(
            Kind::ErrorUnion {
                error,
                result: owner,
            },
            b,
        )
    }
}
