//! Shared independent-V callback. A successful role match retains an actual
//! entry obligation; it never exports class equations or runtime authority.
use super::*;
use crate::semantic::{self, api::*};
#[path = "semantic_associated.rs"]
mod associated;

pub(crate) fn original_scope<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a SemanticDefinition> {
    associated::scope(module, id, b)
}
pub(crate) fn original_type_recipe(
    module: &TypedModule,
    owner: LifeDeclarationId,
    pattern: &DynTypePattern,
    b: &mut Budget,
) -> Check<SemanticTypeRecipe> {
    associated::type_recipe(module, owner, pattern, b, 0)
}
pub(crate) fn original_constant_recipe(
    module: &TypedModule,
    owner: LifeDeclarationId,
    pattern: &DynConstPattern,
    b: &mut Budget,
) -> Check<SemanticConstRecipe> {
    associated::constant_recipe(module, owner, pattern, b, 0)
}
pub(crate) fn original_requirement_recipes(
    module: &TypedModule,
    owner: LifeDeclarationId,
    rows: &[DynObligation],
    b: &mut Budget,
) -> Check<Vec<SemanticRequirementRecipe>> {
    associated::requirement_recipes(module, owner, rows, b)
}

/// An exact selection in one original provider instance. This is metadata and
/// checked signature substitution, never an executable entry/memory authority.
#[derive(Clone)]
pub(crate) struct OriginalMemberReceipt {
    pub(crate) receiver: CheckedType,
    pub(crate) application: CheckedApplication,
    pub(crate) member: u32,
    pub(crate) method_actuals: Vec<CheckedActual>,
    pub(crate) implementation: LifeDeclarationId,
    pub(crate) declaration_actuals: Vec<CheckedActual>,
    pub(crate) definition: DynDefinitionId,
    pub(crate) body: SemanticDefinitionId,
    pub(crate) body_owner: LifeDeclarationId,
    pub(crate) body_member: u32,
    pub(crate) trait_signature: SemanticDefinitionId,
    pub(crate) source: DynDefinitionScope,
    pub(crate) environment: EvalEnv,
    pub(crate) expected_signature: CheckedSignature,
    pub(crate) actual_signature: CheckedSignature,
    pub(crate) trait_contract: DynContractRecipe,
    pub(crate) body_contract: DynContractRecipe,
    pub(crate) trait_requirements: Vec<DynObligation>,
    pub(crate) body_requirements: Vec<DynObligation>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct OriginalMethodRelation {
    pub(crate) trait_declaration: LifeDeclarationId,
    pub(crate) implementation_declaration: LifeDeclarationId,
    pub(crate) trait_signature: SemanticDefinitionId,
    pub(crate) body: SemanticDefinitionId,
    pub(crate) source: DynDefinitionScope,
}

fn charge_type(value: &CheckedType, b: &mut Budget) -> Check<()> {
    dynamic_metadata::ty(&value.ty, b)?;
    if value.classes.len() > b.limits.max_nodes
        || value.origins.len() > b.limits.max_nodes
        || value.statics.len() > b.limits.max_nodes
    {
        return Err(limit(
            "checked original member value exceeds selected bound",
        ));
    }
    b.spend(
        value
            .classes
            .len()
            .saturating_add(value.origins.len())
            .saturating_add(value.life.slots.len())
            .saturating_add(value.life.retained.len())
            .saturating_add(value.life.nominal_arguments.len())
            .saturating_add(value.statics.len()),
    )?;
    for row in &value.life.slots {
        b.spend(row.path.len())?;
    }
    for row in &value.life.retained {
        b.spend(row.path.len())?;
    }
    for row in &value.life.nominal_arguments {
        b.spend(row.path.len())?;
    }
    for key in &value.classes {
        charge_key(key, b)?;
    }
    for origin in &value.origins {
        match origin {
            IntrinsicOrigin::Existing { path, .. } => b.spend(path.len())?,
            IntrinsicOrigin::DerivedClass { source } => charge_key(source, b)?,
        }
    }
    for row in &value.statics {
        match row {
            StaticOccurrence::Direct { path, leaf, .. } => {
                b.spend(path.len())?;
                dynamic_metadata::ty(leaf, b)?;
            }
            StaticOccurrence::Logical { position, leaf, .. } => {
                b.spend(position.path.len())?;
                dynamic_metadata::ty(leaf, b)?;
            }
        }
    }
    Ok(())
}
fn charge_key(key: &ClassKey, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    if let ClassKey::Intrinsic { path, .. } | ClassKey::Outer { path, .. } = key {
        b.spend(path.len())?;
    }
    Ok(())
}
fn charge_actual(value: &CheckedActual, b: &mut Budget) -> Check<()> {
    match value {
        CheckedActual::Type(value) => charge_type(value, b),
        CheckedActual::Life(key) => charge_key(key, b),
        CheckedActual::Const { carrier, value } => {
            charge_type(carrier, b)?;
            if let Constant::WideString(value) = value {
                b.spend(value.len())?;
            }
            Ok(())
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct OriginalMemberKey {
    receiver: CheckedType,
    application: CheckedApplication,
    member: u32,
    method_actuals: Vec<CheckedActual>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn select_original_member(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    environment: &EvalEnv,
    receiver: &CheckedType,
    application: &CheckedApplication,
    member: u32,
    method_actuals: &[CheckedActual],
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<OriginalMemberReceipt> {
    charge_type(receiver, b)?;
    for actual in application.arguments.iter().chain(method_actuals) {
        charge_actual(actual, b)?;
    }
    if proofs.active_members.len() >= b.limits.max_nodes
        || method_actuals.len() > b.limits.max_nodes
        || application.arguments.len() > b.limits.max_nodes
    {
        return Err(limit(
            "original member selection domain exceeds selected bound",
        ));
    }
    b.spend(
        proofs
            .active_members
            .len()
            .saturating_add(method_actuals.len())
            .saturating_add(1),
    )?;
    let key = OriginalMemberKey {
        receiver: receiver.clone(),
        application: application.clone(),
        member,
        method_actuals: method_actuals.to_vec(),
    };
    if proofs.active_members.contains(&key) {
        return Err(bad("complete original member selection forms a cycle"));
    }
    let token = active.fresh(b)?;
    proofs.active_members.push(key);
    let result = active.with_query(
        semantic::DependencyKey {
            kind: semantic::QueryKind::External,
            token,
        },
        b,
        |active, b| {
            associated::select_member(
                proofs,
                module,
                context,
                environment,
                receiver,
                application,
                member,
                method_actuals,
                active,
                b,
            )
        },
    );
    proofs.active_members.pop();
    result
}

#[derive(Default)]
pub(crate) struct Proofs {
    pub(crate) callable_shapes: Vec<requests::CallableShapeRequest>,
    pub(crate) pending_statics: Vec<(StorageBase, StaticOccurrence)>,
    pub(crate) pending_original_methods: Vec<SemanticDefinitionId>,
    pub(crate) pending_original_method_relations: Vec<OriginalMethodRelation>,
    active_original: Vec<ExternalRequest>,
    available_original: Vec<(ExternalRequest, ExternalValue)>,
    original_domain: Option<associated::LocalDomain>,
    active_members: Vec<OriginalMemberKey>,
}

/// One request-local representation of already checked semantic identities.
/// This map only assigns private outside images. Intrinsic origins are carried
/// separately by Symbols, so two existing intrinsic classes are never merged.
struct Domain<'a> {
    context: &'a semantic::CheckedContext,
    environment: &'a EvalEnv,
    symbols: symbols::Symbols,
    owner: terms::Owner,
    frame: FrameToken,
    keys: Vec<ClassKey>,
    scope: DynInstantiationScope,
}
impl<'a> Domain<'a> {
    fn new(
        context: &'a semantic::CheckedContext,
        environment: &'a EvalEnv,
        active: &mut semantic::ActiveQueries,
        b: &mut Budget,
    ) -> Check<Self> {
        let frame = active.fresh(b)?;
        Ok(Self {
            context,
            environment,
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
            // Only the shared context's original exact relations may identify
            // outside Fixed images. Callable/Outlives never enter that Γ.
            let mut established = None;
            for known in &self.keys {
                let term = self.symbols.class(known, b)?;
                if matches!(term.image, StorageBinder::Fixed(_))
                    && self.context.same(self.environment, known, key, b)?
                {
                    established = Some(term);
                    break;
                }
            }
            if let Some(term) = established {
                term.image
            } else {
                let i = self.scope.fixed_binder_count;
                self.scope.fixed_binder_count = i
                    .checked_add(1)
                    .ok_or_else(|| limit("semantic callback Fixed domain exceeded"))?;
                StorageBinder::Fixed(i)
            }
        };
        scopes::bounded(self.scope, b)?;
        let mut origin = terms::Origin {
            owner: self.owner,
            root: terms::Root::Semantic(self.frame.0, index),
            class: 0,
        };
        if !quantified {
            for known in &self.keys {
                let term = self.symbols.class(known, b)?;
                if term.image == image {
                    origin = term.origin;
                    break;
                }
            }
        }
        self.symbols
            .bind(key.clone(), terms::LifeTerm { origin, image }, b)?;
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

pub(crate) enum Constructor {
    Address {
        mode: StoredBorrowMode,
        outer: Option<ClassKey>,
    },
    Managed,
    Tuple,
}

/// Reuse the independent full occurrence constructor. Existing child types and
/// intrinsic partitions remain intact; only a fresh parent is constructed.
pub(crate) fn construct(
    context: &semantic::CheckedContext,
    environment: &EvalEnv,
    kind: Constructor,
    children: &[CheckedType],
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<CheckedType> {
    b.spend(children.len())?;
    let mut domain = Domain::new(context, environment, active, b)?;
    for child in children {
        domain.value(child, b)?;
    }
    if let Constructor::Address {
        outer: Some(key), ..
    } = &kind
    {
        domain.key(key, false, b)?;
    }
    let mut values = Vec::with_capacity(children.len());
    let mut statics = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let (value, pending) = domain.symbols.incoming_pending(child, b)?;
        values.push(value);
        for row in pending {
            let prefix = match kind {
                Constructor::Tuple => {
                    LifeTypeProjection::Place(PlaceProjection::Field(index as u32))
                }
                _ => LifeTypeProjection::Pointee,
            };
            statics.push(match row {
                StaticOccurrence::Direct {
                    mut path,
                    leaf,
                    mode,
                } => {
                    b.spend(path.len().saturating_add(1))?;
                    path.insert(0, prefix);
                    StaticOccurrence::Direct { path, leaf, mode }
                }
                logical @ StaticOccurrence::Logical { .. } => logical,
            });
        }
    }
    let one = |values: &mut Vec<terms::Value>| -> Check<terms::Value> {
        if values.len() != 1 {
            return Err(bad("unary semantic constructor has another child count"));
        }
        Ok(values.pop().unwrap())
    };
    let value = match kind {
        Constructor::Tuple => constructors::tuple(&values, b)?,
        Constructor::Managed => constructors::managed(&one(&mut values)?, b)?,
        Constructor::Address {
            mode,
            outer: Some(key),
        } => constructors::address(&one(&mut values)?, domain.symbols.class(&key, b)?, mode, b)?,
        Constructor::Address { mode, outer: None } => {
            if !matches!(
                mode,
                StoredBorrowMode::SharedAddress | StoredBorrowMode::ReadOnlyDescription
            ) {
                return Err(bad("static type constructor cannot invent mutable storage"));
            }
            let child = one(&mut values)?;
            let mut value = constructors::managed(&child, b)?;
            value.raw.ty = match mode {
                StoredBorrowMode::SharedAddress => Type::Address {
                    mutable: false,
                    inner: Box::new(child.raw.ty),
                },
                _ => Type::Desc(Box::new(child.raw.ty)),
            };
            statics.push(StaticOccurrence::Direct {
                path: vec![],
                leaf: value.raw.ty.clone(),
                mode,
            });
            value
        }
    };
    domain.symbols.constructed(value, statics, b)
}

impl ExternalProofs for Proofs {
    fn query(
        &mut self,
        module: &TypedModule,
        context: &semantic::CheckedContext,
        environment: &EvalEnv,
        request: &ExternalRequest,
        mode: RequirementMode,
        active: &mut semantic::ActiveQueries,
        b: &mut Budget,
    ) -> Check<ExternalValue> {
        if context.domain_only.get()
            && !matches!(
                request,
                ExternalRequest::AssociatedType { .. } | ExternalRequest::AssociatedConst { .. }
            )
        {
            return Err(bad(
                "Task frame metadata cannot discharge an external WF or capability obligation",
            ));
        }
        // An original abstract premise is not a concrete actual implementation.
        // The shared context owns its declaration Γ and caller-side discharge.
        if mode == RequirementMode::DeclarationPremise {
            return Err(bad(
                "original declaration premise requires its checked semantic assumption context",
            ));
        }
        match request {
            ExternalRequest::Builtin {
                receiver,
                trait_kind,
            } => {
                let bound = match trait_kind {
                    SemanticBuiltinTrait::Copy => DynStructuralBound::Copy,
                    SemanticBuiltinTrait::Value => DynStructuralBound::Value,
                    SemanticBuiltinTrait::SyncDrop => DynStructuralBound::SyncDrop,
                    SemanticBuiltinTrait::StoreElement => DynStructuralBound::StoreElement,
                    SemanticBuiltinTrait::Drop
                    | SemanticBuiltinTrait::Transfer
                    | SemanticBuiltinTrait::Share
                    | SemanticBuiltinTrait::ThreadCallable
                    | SemanticBuiltinTrait::ClosedEnvironment
                    | SemanticBuiltinTrait::Send
                    | SemanticBuiltinTrait::Sync => {
                        return Err(bad(
                            "sealed builtin requires its complete actual owner/environment/provider-body proof",
                        ));
                    }
                };
                let mut domain = Domain::new(context, environment, active, b)?;
                domain.value(receiver, b)?;
                let (receiver, pending_statics) = domain.symbols.incoming_pending(receiver, b)?;
                b.spend(pending_statics.len())?;
                self.pending_statics.extend(
                    pending_statics
                        .into_iter()
                        .map(|row| (StorageBase::Receiver, row)),
                );
                structural::check(module, &receiver, bound, b)?;
                Ok(ExternalValue::Proved)
            }
            ExternalRequest::Structural { receiver, bound } => {
                let mut domain = Domain::new(context, environment, active, b)?;
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
                let mut domain = Domain::new(context, environment, active, b)?;
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
            | ExternalRequest::AssociatedConst { .. } => {
                b.spend(self.available_original.len())?;
                if let Some((_, value)) = self
                    .available_original
                    .iter()
                    .rev()
                    .find(|(key, _)| key == request)
                {
                    return Ok(value.clone());
                }
                b.spend(self.active_original.len().saturating_add(1))?;
                if self.active_original.contains(request) {
                    return Err(bad("original trait/associated query forms a cycle"));
                }
                let token = active.fresh(b)?;
                self.active_original.push(request.clone());
                let result = active.with_query(
                    semantic::DependencyKey {
                        kind: semantic::QueryKind::External,
                        token,
                    },
                    b,
                    |active, b| {
                        associated::query(self, module, context, environment, request, active, b)
                    },
                );
                self.active_original.pop();
                result
            }
            ExternalRequest::Dyn { .. } => Err(bad(
                "semantic external query requires the complete original FullW instance graph",
            )),
        }
    }
}
