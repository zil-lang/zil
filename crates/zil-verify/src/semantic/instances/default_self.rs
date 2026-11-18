//! Recover the default method's lexical Self from an executable receiver and
//! the original impl with the same complete trait application.
use super::*;

impl CheckedContext {
    pub(super) fn default_self(
        &self,
        module: &TypedModule,
        scope: &SemanticInstance,
        declaration: LifeDeclarationId,
        arguments: &[CheckedActual],
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("default Self has no original programme"))?;
        let mut receiver = None;
        b.spend(program.instances.len())?;
        for child in program
            .instances
            .iter()
            .filter(|child| child.parent == Some(scope.id))
        {
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = child.target
            else {
                continue;
            };
            let definition = self.definition(module, child.definition, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                receiver_parameter: Some(0),
                ..
            } = &definition.body
            else {
                continue;
            };
            let Some(parameter) = signature.parameters.first() else {
                continue;
            };
            let function = module
                .functions
                .get(id.0 as usize)
                .filter(|f| f.id == id)
                .ok_or_else(|| bad("default Self has no actual executable method"))?;
            let Some(specialization) = &function.dyn_instance else {
                continue;
            };
            b.spend(program.dyn_scopes.len())?;
            if !program.dyn_scopes.iter().any(|row| {
                row.scope == child.definition
                    && row.owner == SemanticDynOwner::MethodDefinition(specialization.definition)
            }) {
                return Err(bad("default receiver entry has another original method"));
            }
            let physical = function
                .parameters
                .first()
                .and_then(|local| function.locals.get(local.0 as usize))
                .ok_or_else(|| bad("default method requires its actual receiver parameter"))?;
            let actual = match (&parameter.ty, &physical.ty) {
                (
                    SemanticTypeRecipe::Address { mutable, inner, .. },
                    Type::Address {
                        mutable: mode,
                        inner: actual,
                    },
                ) if mutable == mode
                    && matches!(
                        inner.as_ref(),
                        SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(1)
                        }
                    ) =>
                {
                    actual.as_ref()
                }
                (SemanticTypeRecipe::Desc { inner, .. }, Type::Desc(actual))
                    if matches!(
                        inner.as_ref(),
                        SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(1)
                        }
                    ) =>
                {
                    actual.as_ref()
                }
                (
                    SemanticTypeRecipe::SelfType {
                        scope: SemanticScopeRef::Ancestor(1),
                    },
                    actual,
                ) => actual,
                _ => {
                    return Err(bad(
                        "default method receiver changes its original Self position or mode",
                    ));
                }
            };
            dynamic_metadata::ty(actual, b)?;
            if receiver.is_some_and(|previous| previous != actual) {
                return Err(bad(
                    "default methods sharing a trait scope have different actual receivers",
                ));
            }
            receiver = Some(actual);
        }
        let physical = receiver
            .ok_or_else(|| bad("trait Self claim has no executable original default receiver"))?;
        let mut external = dynamic_witness::semantic_external::Proofs::default();
        let mut selected: Option<(SemanticDefinitionId, CheckedType)> = None;
        b.spend(program.instances.len())?;
        for implementation in &program.instances {
            if implementation.target != SemanticInstanceTarget::Scope {
                continue;
            }
            let definition = self.definition(module, implementation.definition, b)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                trait_application,
                ..
            }) = &definition.body
            else {
                continue;
            };
            if trait_application.declaration != declaration {
                continue;
            }
            let environment =
                self.environment_for_instance(module, implementation.id, active, b)?;
            let application = self.application(
                module,
                trait_application,
                &environment,
                &mut external,
                active,
                b,
            )?;
            if !nominal::same_actuals(self, &environment, &application.arguments, arguments, b)? {
                continue;
            }
            let actual = environment
                .lexical
                .first()
                .and_then(|frame| frame.self_type.as_ref())
                .ok_or_else(|| bad("default provider has no reconstructed original Self"))?;
            actual.charge_clone(b)?;
            if &actual.ty != physical {
                continue;
            }
            if let Some((owner, previous)) = &selected {
                if *owner != definition.id || !self.same_type(&environment, previous, actual, b)? {
                    return Err(bad(
                        "default method has ambiguous original implementation receivers",
                    ));
                }
            } else {
                selected = Some((definition.id, actual.clone()));
            }
        }
        self.finish_external_methods(module, &mut external, active, b)?;
        if !external.callable_shapes.is_empty() || !external.pending_statics.is_empty() {
            return Err(unsupported(
                "default receiver selection retains an unproved original obligation",
                None,
            ));
        }
        selected.map(|(_,value)| value).ok_or_else(|| bad("actual receiver and ordered trait arguments have no matching original implementation"))
    }
}
