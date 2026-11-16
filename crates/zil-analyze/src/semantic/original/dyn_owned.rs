//! Owned erasure consumes its complete original Self on both outcomes. The
//! owner's cleanup contract cannot be exported as a property of that Self.
use super::*;

pub(super) struct Creation {
    provider: SemanticDefinitionId,
    actuals: Vec<Term>,
    source: Term,
    interface: Term,
    trait_actuals: Vec<Term>,
    output: Term,
}
pub(in crate::semantic) struct DynCreation {
    pub(in crate::semantic) provider: SemanticDefinitionId,
    pub(in crate::semantic) actuals: Vec<crate::semantic::value::Argument>,
    pub(in crate::semantic) interface: crate::semantic::value::Value,
    pub(in crate::semantic) trait_actuals: Vec<crate::semantic::value::Argument>,
    pub(in crate::semantic) source: crate::semantic::value::Value,
    pub(in crate::semantic) output: crate::semantic::value::Value,
}
impl Checked {
    pub(in crate::semantic) fn dyn_creation(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<DynCreation> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail(
                "actual owning Dyn creation has no complete original body",
            ));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(node) = link.node else {
                return Err(fail(
                    "owning Dyn construction selects an original statement",
                ));
            };
            if selected.replace(node).is_some() {
                return Err(fail("owning Dyn creation has competing original nodes"));
            }
        }
        b.lookup(self.dyn_creations.len(), WORK)?;
        let creation = self
            .dyn_creations
            .get(&(
                context.definition,
                selected.ok_or_else(|| fail("owning Dyn creation has no original node"))?,
            ))
            .ok_or_else(|| fail("owning Dyn construction has no original provider/source proof"))?;
        let mut actuals = Vec::new();
        for actual in &creation.actuals {
            let actual = self.argument(*actual, context, program, inventory, b, 0)?;
            Solver::push(&mut actuals, actual, b)?;
        }
        let mut trait_actuals = Vec::new();
        for actual in &creation.trait_actuals {
            let actual = self.argument(*actual, context, program, inventory, b, 0)?;
            Solver::push(&mut trait_actuals, actual, b)?;
        }
        Ok(DynCreation {
            interface: self.value(creation.interface, context, program, inventory, b, 0)?,
            trait_actuals,
            provider: creation.provider,
            actuals,
            source: self.value(creation.source, context, program, inventory, b, 0)?,
            output: self.value(creation.output, context, program, inventory, b, 0)?,
        })
    }
}
impl Solver<'_, '_> {
    pub(super) fn owned_dyn_sync(
        &self,
        ty: &dyn_types::DynType,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        if !ty.owned {
            return Ok(false);
        }
        // SyncDrop belongs to the builtin owning carrier, not to the trait
        // or erased Self. Every constructor separately requires Self:SyncDrop;
        // each executable interface and concrete table must prove its complete
        // cleanup upper and real destructor before semantic publication.
        let declaration = self.inventory.original_dyn(ty.declaration, b)?;
        if declaration.kind != DynDeclarationKind::Trait {
            return Err(fail("owning Dyn cleanup has no original trait identity"));
        }
        self.dyn_domain_covered(ty, b, 0)?;
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
        let (interface, context) = match written {
            [CoreGenericArgument::Type(interface)] => (interface, None),
            [
                CoreGenericArgument::Type(interface),
                CoreGenericArgument::Type(context),
            ] => (interface, Some(context)),
            _ => {
                return Err(fail(
                    "owned Dyn construction requires its full trait and optional Context Type arguments",
                ));
            }
        };
        let [source] = arguments else {
            return Err(fail(
                "original owned Dyn construction consumes exactly one complete Self",
            ));
        };
        // The builtin's two written Types form one interface application.
        // Forming the first fragment alone would incorrectly demand Closed
        // before its explicit Context has been supplied.
        let interface = if let Some(context) = context {
            let SemanticTypeRecipe::Dyn {
                owned: false,
                interface,
            } = interface
            else {
                return Err(fail(
                    "owned Dyn Context requires its exact written borrowed trait recipe",
                ));
            };
            if !matches!(interface.context, SemanticDynContextRecipe::Closed) {
                return Err(fail(
                    "owned Dyn construction repeats its dependency Context",
                ));
            }
            crate::dyn_metadata::recipe(interface, b)?;
            crate::dyn_metadata::recipe(context, b)?;
            let mut full = interface.clone();
            full.context = SemanticDynContextRecipe::Type(Box::new(context.clone()));
            self.dyn_type(frame, false, &full, b, depth + 1)?
        } else {
            self.recipe(frame, interface, b, depth + 1)?
        };
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
        self.dyn_domain_covered(&ty, b, depth + 1)?;
        self.owned_dyn_sync(&ty, b)?;
        let source = self.expression(frame, source, b, depth + 1)?;
        self.dyn_source_covered(&ty, source, b, depth + 1)?;
        let (implementation, provider_actuals) =
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
        b.spend(ty.work(), WORK)?;
        let mut borrowed = ty.clone();
        borrowed.owned = false;
        b.spend(borrowed.arguments.len(), WORK)?;
        let trait_actuals = borrowed.arguments.clone();
        let interface = self.term(Kind::Dyn(borrowed), b)?;
        let owner = self.term(Kind::Dyn(ty), b)?;
        let output = self.term(
            Kind::ErrorUnion {
                error,
                result: owner,
            },
            b,
        )?;
        b.lookup(self.state.dyn_creations.len(), WORK)?;
        if self
            .state
            .dyn_creations
            .insert(
                (frame.definition, node),
                Creation {
                    provider: implementation,
                    actuals: provider_actuals,
                    source,
                    interface,
                    trait_actuals,
                    output,
                },
            )
            .is_some()
        {
            return Err(fail(
                "owning Dyn construction repeats its original full provider key",
            ));
        }
        Ok(output)
    }
}
