//! Shared independent-V callback. A successful role match retains an actual
//! entry obligation; it never exports class equations or runtime authority.
use super::*;
use crate::semantic::{self, api::*};

#[derive(Default)]
pub(crate) struct Proofs {
    pub(crate) callable_shapes: Vec<requests::CallableShapeRequest>,
    pub(crate) pending_statics: Vec<(StorageBase, StaticOccurrence)>,
}

/// One request-local representation of already checked semantic identities.
/// This map only assigns private outside images. Intrinsic origins are carried
/// separately by Symbols, so two existing intrinsic classes are never merged.
struct Domain {
    symbols: symbols::Symbols,
    owner: terms::Owner,
    frame: FrameToken,
    keys: Vec<ClassKey>,
    scope: DynInstantiationScope,
}
impl Domain {
    fn new(active: &mut semantic::ActiveQueries, b: &mut Budget) -> Check<Self> {
        let frame = active.fresh(b)?;
        Ok(Self {
            symbols: symbols::Symbols::new(terms::Owner::Semantic(frame.0), active.fresh(b)?),
            owner: terms::Owner::Semantic(frame.0),
            frame,
            keys: vec![],
            scope: DynInstantiationScope {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
            },
        })
    }
    fn key(&mut self, key: &ClassKey, quantified: bool, b: &mut Budget) -> Check<()> {
        if let ClassKey::Outer { path, .. } | ClassKey::Intrinsic { path, .. } = key {
            b.spend(path.len())?;
        }
        b.spend(self.keys.len().saturating_add(1))?;
        if self.keys.contains(key) {
            return Ok(());
        }
        let index = u32::try_from(self.keys.len())
            .map_err(|_| limit("semantic callback class table exceeded"))?;
        let image = if quantified {
            let i = self.scope.invocation_binder_count;
            self.scope.invocation_binder_count = i
                .checked_add(1)
                .ok_or_else(|| limit("semantic Callable invocation domain exceeded"))?;
            StorageBinder::Invocation(i)
        } else {
            let i = self.scope.fixed_binder_count;
            self.scope.fixed_binder_count = i
                .checked_add(1)
                .ok_or_else(|| limit("semantic callback Fixed domain exceeded"))?;
            StorageBinder::Fixed(i)
        };
        scopes::bounded(self.scope, b)?;
        self.symbols.bind(
            key.clone(),
            terms::LifeTerm {
                origin: terms::Origin {
                    owner: self.owner,
                    root: terms::Root::Semantic(self.frame.0, index),
                    class: 0,
                },
                image,
            },
            b,
        )?;
        self.keys.push(key.clone());
        Ok(())
    }
    fn value(&mut self, value: &CheckedType, b: &mut Budget) -> Check<()> {
        b.spend(value.classes.len().saturating_add(value.origins.len()))?;
        for key in &value.classes {
            self.key(key, false, b)?;
        }
        for origin in &value.origins {
            if let IntrinsicOrigin::DerivedClass { source } = origin {
                self.key(source, false, b)?;
            }
        }
        Ok(())
    }
}

impl ExternalProofs for Proofs {
    fn query(
        &mut self,
        module: &TypedModule,
        _context: &semantic::CheckedContext,
        _environment: &EvalEnv,
        request: &ExternalRequest,
        mode: RequirementMode,
        active: &mut semantic::ActiveQueries,
        b: &mut Budget,
    ) -> Check<ExternalValue> {
        // An original abstract premise is not a concrete actual implementation.
        // The shared context owns its declaration Γ and caller-side discharge.
        if mode == RequirementMode::DeclarationPremise {
            return Err(bad(
                "original declaration premise requires its checked semantic assumption context",
            ));
        }
        match request {
            ExternalRequest::Structural { receiver, bound } => {
                let mut domain = Domain::new(active, b)?;
                domain.value(receiver, b)?;
                let (receiver, pending_statics) = domain.symbols.incoming_pending(receiver, b)?;
                b.spend(pending_statics.len())?;
                self.pending_statics.extend(
                    pending_statics
                        .into_iter()
                        .map(|row| (StorageBase::Receiver, row)),
                );
                // Structural qualification neither promises empty Context nor
                // discharges any Static constructor/return provenance.
                structural::check(module, &receiver, *bound, b)?;
                Ok(ExternalValue::Proved)
            }
            ExternalRequest::Callable {
                receiver,
                kind,
                signature,
            } => {
                let mut domain = Domain::new(active, b)?;
                // Quantifier keys enter first. A caller Fixed which happens to
                // share an integer/image can never become one of these keys.
                for key in signature.bound.iter().chain(&signature.invocation) {
                    domain.key(key, true, b)?;
                }
                domain.value(receiver, b)?;
                for (parameter, _) in &signature.parameters {
                    domain.value(parameter, b)?;
                }
                domain.value(&signature.result, b)?;
                let (receiver, statics) = domain.symbols.incoming_pending(receiver, b)?;
                let mut pending_statics = statics
                    .into_iter()
                    .map(|row| (StorageBase::Receiver, row))
                    .collect::<Vec<_>>();
                b.spend(signature.parameters.len())?;
                let mut parameters = Vec::with_capacity(signature.parameters.len());
                let mut parameter_modes = Vec::with_capacity(signature.parameters.len());
                for (index, (parameter, mode)) in signature.parameters.iter().enumerate() {
                    let (value, statics) = domain.symbols.incoming_pending(parameter, b)?;
                    b.spend(statics.len())?;
                    pending_statics.extend(
                        statics
                            .into_iter()
                            .map(|row| (StorageBase::Parameter(index as u32), row)),
                    );
                    parameters.push(value);
                    parameter_modes.push(*mode);
                }
                let (result, statics) = domain.symbols.incoming_pending(&signature.result, b)?;
                b.spend(statics.len())?;
                pending_statics.extend(statics.into_iter().map(|row| (StorageBase::Result, row)));
                let shape = callables::Shape {
                    parameters,
                    parameter_modes,
                    result,
                    result_mode: signature.result_mode,
                    call_kind: signature.call_kind,
                };
                let proof = callables::check_shape(
                    module,
                    &receiver,
                    *kind,
                    &shape,
                    domain.scope,
                    true,
                    b,
                )?;
                let mut pending = proof.request(module, b)?;
                pending.pending_statics = pending_statics;
                b.spend(1)?;
                self.callable_shapes.push(pending);
                // The caller still has the outstanding actual-CFG request.
                // No equality or Outlives fact is returned to its Γ.
                Ok(ExternalValue::Proved)
            }
            ExternalRequest::Trait { .. }
            | ExternalRequest::AssociatedType { .. }
            | ExternalRequest::AssociatedConst { .. }
            | ExternalRequest::Dyn { .. } => Err(bad(
                "semantic external query requires the complete original FullW instance graph",
            )),
        }
    }
}
