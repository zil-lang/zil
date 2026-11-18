//! Shared independent-V callback. A successful role match retains an actual
//! entry obligation; it never exports class equations or runtime authority.
use super::*;
use crate::semantic::{self, api::*};
#[path = "semantic_associated.rs"]
mod associated;
#[path = "semantic_completed.rs"]
mod completed;
#[path = "semantic_member_instance.rs"]
mod member_instance;
pub(crate) use member_instance::check as check_static_member_instance;
pub(crate) use member_instance::build_requests as static_member_requests;

pub(crate) fn original_scope<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a SemanticDefinition> {
    associated::scope(module, id, b)
}
pub(crate) fn original_program_scope<'a>(program:&'a SemanticModule,declarations:&[DynDeclaration],id:LifeDeclarationId,b:&mut Budget)->Check<&'a SemanticDefinition>{
    let owner=SemanticDynOwner::Declaration(id);
    if let Some(rows)=dynamic_metadata::program_lexical_scopes(program,declarations,&owner,b)?{
        let [scope]=rows.as_slice()else{return Err(bad(if rows.is_empty(){"original declaration has no lexical scope binding"}else{"original declaration has duplicate lexical scopes"}));};
        return super::bridge::program_owner(program,declarations,&owner,*scope,b);
    }
    b.spend(program.dyn_scopes.len())?;
    let mut rows=program.dyn_scopes.iter().filter(|row|row.owner==owner);
    let row=rows.next().ok_or_else(||bad("original declaration has no lexical scope binding"))?;
    if rows.next().is_some(){return Err(bad("original declaration has duplicate lexical scopes"));}
    super::bridge::program_owner(program,declarations,&owner,row.scope,b)
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
    pub(crate) trait_environment: EvalEnv,
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

fn charge_type(value:&CheckedType,b:&mut Budget)->Check<()>{
    dynamic_metadata::bounds_type(&value.ty,b)?;
    if value.classes.len()>b.limits.max_nodes||value.origins.len()>b.limits.max_nodes||value.statics.len()>b.limits.max_nodes{return Err(limit("checked original member value exceeds selected bound"));}
    value.charge_clone(b)
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
// Cycle identity is exact, rigid equality. It does not use caller premises or
// identify distinct intrinsic domains through equal outside class images.
fn member_key_equal(a:&OriginalMemberKey,z:&OriginalMemberKey,b:&mut Budget)->Check<bool>{
    b.spend(1)?;
    if a.member!=z.member||a.application.declaration!=z.application.declaration||a.application.arguments.len()!=z.application.arguments.len()||a.method_actuals.len()!=z.method_actuals.len(){return Ok(false);}
    for(a,z)in a.application.arguments.iter().zip(&z.application.arguments).chain(a.method_actuals.iter().zip(&z.method_actuals)){
        if !member_actual_equal(a,z,b)?{return Ok(false);}
    }
    member_value_equal(&a.receiver,&z.receiver,b)
}
fn member_actual_equal(a:&CheckedActual,z:&CheckedActual,b:&mut Budget)->Check<bool>{
    b.spend(1)?;
    match(a,z){
        (CheckedActual::Life(a),CheckedActual::Life(z))=>{charge_key(a,b)?;charge_key(z,b)?;Ok(a==z)},
        (CheckedActual::Type(a),CheckedActual::Type(z))=>member_value_equal(a,z,b),
        (CheckedActual::Const{carrier:a,value:x},CheckedActual::Const{carrier:z,value:y})=>{
            dynamic_metadata::constant(x,b)?;dynamic_metadata::constant(y,b)?;
            if x!=y{return Ok(false);}member_value_equal(a,z,b)
        },
        _=>Ok(false),
    }
}
fn member_value_equal(a:&CheckedType,z:&CheckedType,b:&mut Budget)->Check<bool>{
    a.charge_clone(b)?;z.charge_clone(b)?;
    if a.life!=z.life||a.classes!=z.classes||a.origins.len()!=z.origins.len()||a.statics!=z.statics{return Ok(false);}
    for(a,z)in a.origins.iter().zip(&z.origins){
        semantic::charge_intrinsic_origin(a,b)?;semantic::charge_intrinsic_origin(z,b)?;
        if a!=z{return Ok(false);}
    }
    a.same_raw_type(z,b)
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
    select_member_in(proofs,module,context,environment,receiver,application,member,method_actuals,None,active,b)
}
/// Definition execution has an independently checked complete original parent.
/// It supplies exact original actuals, never an inferred equality of Self κ.
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_definition_member(proofs:&mut Proofs,module:&TypedModule,context:&semantic::CheckedContext,environment:&EvalEnv,receiver:&CheckedType,application:&CheckedApplication,member:u32,method_actuals:&[CheckedActual],parent:&EvalEnv,active:&mut semantic::ActiveQueries,b:&mut Budget)->Check<OriginalMemberReceipt>{
    select_member_in(proofs,module,context,environment,receiver,application,member,method_actuals,Some(parent),active,b)
}
#[allow(clippy::too_many_arguments)]
fn select_member_in(proofs:&mut Proofs,module:&TypedModule,context:&semantic::CheckedContext,environment:&EvalEnv,receiver:&CheckedType,application:&CheckedApplication,member:u32,method_actuals:&[CheckedActual],parent:Option<&EvalEnv>,active:&mut semantic::ActiveQueries,b:&mut Budget)->Check<OriginalMemberReceipt>{
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
    for previous in &proofs.active_members{
        if member_key_equal(previous,&key,b)?{
            return Err(bad("complete original member selection forms a cycle"));
        }
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
                parent,
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
    completed_original: completed::Completed,
    pub(crate) callable_shapes: Vec<requests::CallableShapeRequest>,
    pub(crate) pending_statics: Vec<(StorageBase, StaticOccurrence)>,
    pub(crate) pending_original_methods: Vec<SemanticDefinitionId>,
    pub(crate) pending_original_method_relations: Vec<OriginalMethodRelation>,
    active_original: Vec<ExternalRequest>,
    available_original: Vec<(ExternalRequest, ExternalValue)>,
    original_domain: Option<associated::LocalDomain>,
    active_members: Vec<OriginalMemberKey>,
}
impl Proofs{
    pub(crate) fn original_relation_completed(&self,module:&TypedModule,context:&semantic::CheckedContext,relation:&OriginalMethodRelation,active:&semantic::ActiveQueries,b:&mut Budget)->Check<bool>{self.completed_original.relation_known(module,context,relation,active,b)}
    pub(crate) fn remember_original_relation(&mut self,module:&TypedModule,context:&semantic::CheckedContext,relation:&OriginalMethodRelation,active:&semantic::ActiveQueries,b:&mut Budget)->Check<()>{self.completed_original.relation_success(module,context,relation,active,b)}
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
    let mut normalized_children=Vec::with_capacity(children.len());
    for child in children{
        child.charge_clone(b)?;
        let mut child=child.clone();
        for origin in &mut child.origins{*origin=context.normalized_origin(environment,origin,b)?;}
        normalized_children.push(child);
    }
    let children=normalized_children.as_slice();
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
    let mut terms = Vec::with_capacity(children.len());
    let mut statics = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let (value, pending) = domain.symbols.incoming_pending_terms(child, b)?;
        terms.push(value);
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
    b.spend(children.len().saturating_add(terms.iter().map(Vec::len).sum::<usize>()))?;
    let classes:Vec<Vec<StorageBinder>>=terms.iter().map(|terms|terms.iter().map(|term|term.image).collect()).collect();
    let values:Vec<constructors::View<'_>>=children.iter().zip(&terms).zip(&classes).map(|((child,terms),classes)|constructors::View{ty:&child.ty,life:&child.life,terms,classes}).collect();
    use semantic::PhysicalConstructor as K;
    let(physical,head,mode,is_static)=match kind{
        Constructor::Managed=>(K::Managed,None,None,false),
        Constructor::Tuple=>(K::Tuple,None,None,false),
        Constructor::Address{mode,outer}=>{
            let physical=match mode{StoredBorrowMode::SharedAddress=>K::SharedAddress,StoredBorrowMode::ExclusiveAddress=>K::ExclusiveAddress,_=>K::Description};
            let is_static=outer.is_none();
            let head=match outer{Some(key)=>Some(domain.symbols.class(&key,b)?),None=>None};
            (physical,head,Some(mode),is_static)
        },
    };
    let shape=constructors::shape_views(physical,&values,head,mode,b)?;
    let type_cell=context.physical_constructor(physical,children,&shape.life,b)?;
    if is_static{
        dynamic_metadata::ty(&type_cell.ty,b)?;
        statics.push(StaticOccurrence::Direct{path:vec![],leaf:type_cell.ty.clone(),mode:mode.unwrap()});
    }
    domain.symbols.constructed_shared(type_cell,shape,statics,b)
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
                    SemanticBuiltinTrait::Copy => Some(DynStructuralBound::Copy),
                    SemanticBuiltinTrait::Value => Some(DynStructuralBound::Value),
                    SemanticBuiltinTrait::SyncDrop => Some(DynStructuralBound::SyncDrop),
                    SemanticBuiltinTrait::StoreElement => Some(DynStructuralBound::StoreElement),
                    SemanticBuiltinTrait::Transfer
                    | SemanticBuiltinTrait::ThreadCallable
                    | SemanticBuiltinTrait::ClosedEnvironment => None,
                    SemanticBuiltinTrait::Drop
                    | SemanticBuiltinTrait::Share
                    | SemanticBuiltinTrait::Send
                    | SemanticBuiltinTrait::Sync => {
                        return Err(bad(
                            "sealed builtin requires its complete actual owner/environment/provider-body proof",
                        ));
                    }
                };
                let mut domain = Domain::new(context, environment, active, b)?;
                domain.value(receiver, b)?;
                if let Some(bound)=bound{
                    let(terms,pending_statics)=domain.symbols.incoming_pending_terms(receiver,b)?;
                    b.spend(terms.len().saturating_add(pending_statics.len()))?;
                    self.pending_statics.extend(pending_statics.into_iter().map(|row|(StorageBase::Receiver,row)));
                    let classes:Vec<_>=terms.iter().map(|term|term.image).collect();
                    structural::parts(module,&receiver.ty,&receiver.life,&classes,bound,b)?;
                }else{
                    let(receiver,pending_statics)=domain.symbols.incoming_pending(receiver,b)?;
                    b.spend(pending_statics.len())?;
                    self.pending_statics.extend(pending_statics.into_iter().map(|row|(StorageBase::Receiver,row)));
                    super::semantic_capabilities::check(module,&receiver,*trait_kind,b)?;
                }
                Ok(ExternalValue::Proved)
            }
            ExternalRequest::Structural { receiver, bound } => {
                let mut domain = Domain::new(context, environment, active, b)?;
                domain.value(receiver, b)?;
                let(terms,pending_statics)=domain.symbols.incoming_pending_terms(receiver,b)?;
                b.spend(terms.len().saturating_add(pending_statics.len()))?;
                self.pending_statics.extend(pending_statics.into_iter().map(|row|(StorageBase::Receiver,row)));
                let classes:Vec<_>=terms.iter().map(|term|term.image).collect();
                // Structural qualification neither promises empty Context nor
                // discharges any Static constructor/return provenance.
                structural::parts(module,&receiver.ty,&receiver.life,&classes,*bound,b)?;
                Ok(ExternalValue::Proved)
            }
            ExternalRequest::Callable {
                receiver,
                kind,
                signature,
            } => {
                let source=context.callable_source_signature(module,receiver,environment,self,active,b)?;
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
                if let Some(source)=&source{
                    for(value,_)in &source.parameters{domain.value(value,b)?;}
                    domain.value(&source.result,b)?;
                }
                let source=if let Some(source)=source{
                    let mut parameters=Vec::new();let mut parameter_modes=Vec::new();
                    for(value,mode)in &source.parameters{parameters.push(domain.symbols.incoming_pending(value,b)?.0);parameter_modes.push(*mode);}
                    Some(callables::Shape{parameters,parameter_modes,result:domain.symbols.incoming_pending(&source.result,b)?.0,result_mode:source.result_mode,call_kind:source.call_kind})
                }else{None};
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
                let receiver_head=context.callable_receiver_key(signature,b)?.map(|key|domain.symbols.class(key,b)).transpose()?;
                let proof = callables::check_shape_with_source(
                    module,
                    &receiver,
                    *kind,
                    &shape,
                    source.as_ref(),
                    receiver_head,
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
                let epoch=context.completed_original_epoch();
                let reusable=associated::empty_domain(self)&&self.available_original.is_empty()
                    &&epoch.is_some()&&completed::pure_environment(environment,b)?&&completed::pure_request(request,b)?;
                if reusable {
                    if let Some(value)=self.completed_original.get(module,context,environment,request,active,b)?{return Ok(value);}
                }
                let token = active.fresh(b)?;
                self.active_original.push(request.clone());
                let mut members=Vec::new();
                let result = active.with_query(
                    semantic::DependencyKey {
                        kind: semantic::QueryKind::External,
                        token,
                    },
                    b,
                    |active, b| {
                        associated::query(self, module, context, environment, request, &mut members, reusable, active, b)
                    },
                );
                self.active_original.pop();
                let value=result?;
                if reusable&&associated::empty_domain(self)&&epoch==context.completed_original_epoch()&&self.callable_shapes.is_empty()&&self.pending_statics.is_empty(){
                    context.finish_external_methods(module,self,active,b)?;
                    if associated::empty_domain(self)&&epoch==context.completed_original_epoch()&&self.callable_shapes.is_empty()&&self.pending_statics.is_empty()
                        &&self.pending_original_methods.is_empty()&&self.pending_original_method_relations.is_empty(){
                        self.completed_original.insert(module,context,environment,request,&value,active,b)?;
                        for(request,value)in members{
                            self.completed_original.insert(module,context,environment,&request,&value,active,b)?;
                        }
                    }
                }
                Ok(value)
            }
            ExternalRequest::Dyn { .. } => Err(bad(
                "semantic external query requires the complete original FullW instance graph",
            )),
        }
    }
}

#[cfg(test)]
mod member_identity_tests{
    use super::*;
    fn key()->OriginalMemberKey{
        let class=ClassKey::Bound{frame:semantic::FrameToken(4),index:0};
        let value=CheckedType{type_cell:semantic::CheckedTypeData::new(Type::Closure(ClosureId(0))),life:StorageLifeShape{binder_count:1,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:vec![class.clone()],origins:vec![IntrinsicOrigin::DerivedClass{source:class}],statics:vec![]};
        OriginalMemberKey{receiver:value.clone(),application:CheckedApplication{declaration:LifeDeclarationId{module:zil_base::ModuleId(0),declaration:zil_base::NodeId(90),compiler:false},arguments:vec![CheckedActual::Type(value)]},member:0,method_actuals:vec![CheckedActual::Life(ClassKey::Invocation{frame:semantic::FrameToken(8),index:0})]}
    }
    #[test]
    fn paid_member_cycle_identity_matches_complete_rigid_equality(){
        let original=key();
        for attack in 0..8{
            let mut changed=original.clone();
            match attack{
                0=>{},
                1=>changed.member+=1,
                2=>changed.application.declaration.declaration=zil_base::NodeId(91),
                3=>changed.receiver.classes[0]=ClassKey::Bound{frame:semantic::FrameToken(5),index:0},
                4=>changed.receiver.origins[0]=IntrinsicOrigin::DerivedClass{source:ClassKey::Invocation{frame:semantic::FrameToken(4),index:0}},
                5=>changed.receiver.replace_type(Type::Closure(ClosureId(1))),
                6=>changed.method_actuals[0]=CheckedActual::Life(ClassKey::Invocation{frame:semantic::FrameToken(9),index:0}),
                7=>changed.receiver.type_cell=semantic::CheckedTypeData::new(original.receiver.ty.clone()),
                _=>unreachable!(),
            }
            let expected=original==changed;
            assert_eq!(member_key_equal(&original,&changed,&mut Budget::new(&Limits::default())).unwrap(),expected,"rigid identity attack {attack}");
        }
    }
}
