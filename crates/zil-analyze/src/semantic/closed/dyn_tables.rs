//! Bind each dynamic slot and entry to its checked original member. The
//! transport's witness proves membership only; it cannot name a replacement
//! definition, supply Self's domain, or certify an entry's signature.
use super::*;
use crate::semantic::value::{self, Value};

struct Signature {
    parameters: Vec<Type>,
    modes: Vec<PassingMode>,
    result: Type,
    result_mode: PassingMode,
    call_kind: CallKind,
    life: StorageLifeContract,
}
#[derive(Clone, PartialEq, Eq)]
enum Role {
    Implicit(u32),
    Invocation(u32),
}

impl Check<'_, '_> {
    fn dyn_step(&self, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original dynamic table bridge exceeds its complete depth",
            ));
        }
        Ok(())
    }
    fn dyn_zero_type(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.dyn_step(b, depth)?;
        crate::dyn_metadata::ty(ty, b)?;
        match ty {
            Type::Named(_) => Err(fail(
                "dynamic complete Self/type value retains an unresolved Named declaration",
            )),
            Type::BorrowedStorage { .. }
            | Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedFunctionPointer { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Task { .. }
            | Type::MutexGuard { .. }
            | Type::Closure(_) => Err(unknown(
                "dynamic table needs the complete nonzero original intrinsic domain",
            )),
            Type::Managed(inner) => self.dyn_zero_type(inner, b, depth + 1),
            Type::OwnedStore { element }
            | Type::Array { element, .. }
            | Type::Slice { element } => self.dyn_zero_type(element, b, depth + 1),
            Type::Mutex { payload } => self.dyn_zero_type(payload, b, depth + 1),
            Type::Registration { environment } | Type::AsyncRegistration { environment } => {
                self.dyn_zero_type(environment, b, depth + 1)
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.dyn_zero_type(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, field) in fields {
                    self.dyn_zero_type(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for field in fields {
                        self.dyn_zero_type(field, b, depth + 1)?;
                    }
                }
                Ok(())
            }
            Type::ErrorUnion { error, result } => {
                self.dyn_zero_type(error, b, depth + 1)?;
                self.dyn_zero_type(result, b, depth + 1)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.dyn_zero_type(parameter, b, depth + 1)?;
                }
                self.dyn_zero_type(result, b, depth + 1)
            }
            _ if ty.is_b0_scalar() || matches!(ty, Type::ForeignHandle | Type::ByteBuffer) => {
                Ok(())
            }
            _ => Err(unknown(
                "dynamic closed type needs its exact original intrinsic signature",
            )),
        }
    }
    fn dyn_zero_value(&self, raw: &DynTypeValue, b: &mut Budget) -> CompileResult<Value> {
        crate::dyn_metadata::recipe(raw, b)?;
        self.dyn_zero_type(&raw.ty, b, 0)?;
        if !empty(&raw.life) || !raw.classes.is_empty() {
            return Err(unknown(
                "dynamic complete type value needs its original nonzero class embedding",
            ));
        }
        Ok(value::scalar(raw.ty.clone()))
    }
    fn dyn_zero_recipe(
        &self,
        recipe: &SemanticTypeRecipe,
        method: SemanticDefinitionId,
        self_type: Option<&Type>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        self.dyn_step(b, depth)?;
        let ty=match recipe {
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() || matches!(ty,Type::ForeignHandle|Type::ByteBuffer)=>{ crate::dyn_metadata::ty(ty,b)?; ty.clone() },
            SemanticTypeRecipe::Tuple(fields) if fields.is_empty()=>Type::Unit,
            SemanticTypeRecipe::Tuple(fields)=>{
                let mut result=Vec::new(); for field in fields { self.push_value(&mut result,self.dyn_zero_recipe(field,method,self_type,b,depth+1)?,b)?; } Type::Tuple(result)
            },
            SemanticTypeRecipe::Nominal { definition,arguments } if arguments.is_empty()=>original::nominal::actual(*definition,&[],self.inventory,b,depth+1)?,
            SemanticTypeRecipe::SelfType { scope:SemanticScopeRef::Ancestor(1) }=>{
                let declaration=self.inventory.definition(method,b)?;
                if declaration.kind!=SemanticDefinitionKind::Method || declaration.parent.is_none() { return Err(fail("dynamic original Self has no exact method owner")); }
                let ty=self_type.ok_or_else(||unknown("public dynamic signature exposes an unerased Self occurrence"))?;
                crate::dyn_metadata::ty(ty,b)?; ty.clone()
            },
            SemanticTypeRecipe::Managed(inner)=>Type::Managed(Box::new(self.dyn_zero_recipe(inner,method,self_type,b,depth+1)?)),
            SemanticTypeRecipe::OwnedStore(element)=>Type::Managed(Box::new(Type::OwnedStore { element:Box::new(self.dyn_zero_recipe(element,method,self_type,b,depth+1)?) })),
            SemanticTypeRecipe::Slice(element)=>Type::Slice { element:Box::new(self.dyn_zero_recipe(element,method,self_type,b,depth+1)?) },
            SemanticTypeRecipe::ErrorUnion { error,result }=>Type::ErrorUnion { error:Box::new(self.dyn_zero_recipe(error,method,self_type,b,depth+1)?),result:Box::new(self.dyn_zero_recipe(result,method,self_type,b,depth+1)?) },
            SemanticTypeRecipe::Exact(exact) if empty(&exact.life) && exact.classes.is_empty() && (exact.ty.is_b0_scalar() || crate::dyn_dispatch::is_create_error(&exact.ty) || exact.ty==crate::generic_mutex::error_type())=>{ crate::dyn_metadata::ty(&exact.ty,b)?; exact.ty.clone() },
            _=>return Err(unknown("dynamic signature requires its complete original mixed or retained type substitution")),
        };
        self.dyn_zero_type(&ty, b, depth + 1)?;
        Ok(ty)
    }
    fn dyn_role(
        &self,
        method: SemanticDefinitionId,
        life: &SemanticLifeRecipe,
        roles: &mut Vec<Role>,
        b: &mut Budget,
    ) -> CompileResult<StorageBinder> {
        let definition = self.inventory.definition(method, b)?;
        let role=match life {
            SemanticLifeRecipe::Implicit { scope:SemanticScopeRef::Ancestor(0),ordinal }=>{
                if !definition.implicit_lifetimes.iter().any(|row|row.ordinal==*ordinal) { return Err(fail("dynamic signature lifetime selects no original implicit occurrence")); }
                Role::Implicit(*ordinal)
            },
            SemanticLifeRecipe::Invocation(ordinal)=>{
                let SemanticDefinitionBody::FunctionWithBody { signature,.. }=&definition.body else { return Err(fail("dynamic signature invocation has no original signature")); };
                if *ordinal>=signature.invocation_lifetimes { return Err(fail("dynamic signature lifetime leaves its original Invocation domain")); }
                Role::Invocation(*ordinal)
            },
            _=>return Err(unknown("dynamic signature requires its complete original quantified or persistent class role")),
        };
        b.spend(roles.len().saturating_add(1), WORK)?;
        let ordinal = if let Some(index) = roles.iter().position(|old| old == &role) {
            index
        } else {
            let index = roles.len();
            self.push_value(roles, role, b)?;
            index
        };
        Ok(StorageBinder::Invocation(u32::try_from(ordinal).map_err(
            |_| fail("dynamic signature invocation domain overflow"),
        )?))
    }
    fn dyn_parameter_type(
        &self,
        recipe: &SemanticTypeRecipe,
        method: SemanticDefinitionId,
        self_type: Option<&Type>,
        position: StoragePosition,
        roles: &mut Vec<Role>,
        life: &mut StorageLifeContract,
        b: &mut Budget,
    ) -> CompileResult<Type> {
        if let SemanticTypeRecipe::Address {
            mutable,
            lifetime: SemanticOuterLifeRecipe::Class(class),
            inner,
        } = recipe
        {
            let binder = self.dyn_role(method, class, roles, b)?;
            let inner = self.dyn_zero_recipe(inner, method, self_type, b, 0)?;
            self.push_value(
                &mut life.bindings,
                StorageLifeBinding { position, binder },
                b,
            )?;
            return Ok(Type::Address {
                mutable: *mutable,
                inner: Box::new(inner),
            });
        }
        self.dyn_zero_recipe(recipe, method, self_type, b, 0)
    }
    fn dyn_signature(
        &self,
        method: SemanticDefinitionId,
        signature: &SemanticSignatureRecipe,
        self_type: Option<&Type>,
        b: &mut Budget,
    ) -> CompileResult<Signature> {
        crate::dyn_metadata::recipe(signature, b)?;
        let definition = self.inventory.definition(method, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            receiver_parameter, ..
        } = &definition.body
        else {
            return Err(fail(
                "dynamic method has no full original signature and body inventory",
            ));
        };
        if !definition.parameters.is_empty()
            || !definition.environment.is_empty()
            || signature.bound_lifetimes != 0
        {
            return Err(unknown(
                "dynamic method requires its complete original mixed or quantified specialization",
            ));
        }
        if *receiver_parameter != Some(0) {
            return Err(fail(
                "dynamic method receiver is not original full parameter zero",
            ));
        }
        let receiver = signature
            .parameters
            .first()
            .ok_or_else(|| fail("dynamic method omits its original receiver"))?;
        let SemanticTypeRecipe::Address {
            mutable,
            lifetime: SemanticOuterLifeRecipe::Class(class),
            inner,
        } = &receiver.ty
        else {
            return Err(fail(
                "dynamic method receiver is not its original borrowed Self",
            ));
        };
        if !matches!(
            inner.as_ref(),
            SemanticTypeRecipe::SelfType {
                scope: SemanticScopeRef::Ancestor(1)
            }
        ) || !matches!(
            (*mutable, receiver.mode),
            (false, PassingMode::Borrow) | (true, PassingMode::BorrowMut)
        ) {
            return Err(fail(
                "dynamic receiver changes its exact original Self scope or passing mode",
            ));
        }
        let mut roles = Vec::new();
        let mut life = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            static_bindings: Vec::new(),
            classes: Vec::new(),
        };
        let binder = self.dyn_role(method, class, &mut roles, b)?;
        self.push_value(
            &mut life.bindings,
            StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: Vec::new(),
                },
                binder,
            },
            b,
        )?;
        // The public comparison excludes parameter zero: only its checked
        // address mode and original role survive erasure. Unit here is an
        // internal unused ABI slot, never a Self value or Context theorem.
        let receiver_ty = if let Some(ty) = self_type {
            self.dyn_zero_type(ty, b, 0)?;
            ty.clone()
        } else {
            Type::Unit
        };
        let mut parameters = Vec::new();
        let mut modes = Vec::new();
        self.push_value(
            &mut parameters,
            Type::Address {
                mutable: *mutable,
                inner: Box::new(receiver_ty),
            },
            b,
        )?;
        self.push_value(&mut modes, receiver.mode, b)?;
        for (ordinal, parameter) in signature.parameters.iter().enumerate().skip(1) {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| fail("dynamic signature parameter ordinal overflow"))?;
            let ty = self.dyn_parameter_type(
                &parameter.ty,
                method,
                self_type,
                StoragePosition {
                    base: StorageBase::Parameter(ordinal),
                    path: Vec::new(),
                },
                &mut roles,
                &mut life,
                b,
            )?;
            self.push_value(&mut parameters, ty, b)?;
            self.push_value(&mut modes, parameter.mode, b)?;
        }
        let result = self.dyn_parameter_type(
            &signature.result,
            method,
            self_type,
            StoragePosition {
                base: StorageBase::Result,
                path: Vec::new(),
            },
            &mut roles,
            &mut life,
            b,
        )?;
        life.invocation_binder_count = u32::try_from(roles.len())
            .map_err(|_| fail("dynamic full signature domain overflow"))?;
        crate::signature_life::charge(&life, b)?;
        Ok(Signature {
            parameters,
            modes,
            result,
            result_mode: signature.result_mode,
            call_kind: signature.call_kind,
            life,
        })
    }
    fn dyn_original_signature<'a>(
        &'a self,
        declaration: LifeDeclarationId,
        member: u32,
        b: &mut Budget,
    ) -> CompileResult<(
        SemanticDefinitionId,
        &'a SemanticSignatureRecipe,
        &'a DynSignatureRecipe,
    )> {
        let raw = self.inventory.original_dyn(declaration, b)?;
        let method = original::member_scope(self.program, declaration, member, self.inventory, b)?;
        let original = self.inventory.definition(method, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else {
            return Err(fail(
                "dynamic public member lacks its full original signature",
            ));
        };
        let mut selected = None;
        for row in raw
            .method_signatures
            .as_ref()
            .ok_or_else(|| unknown("dynamic trait method signature inventory is unknown"))?
        {
            b.spend(1, WORK)?;
            if row.member == member && selected.replace(&row.signature).is_some() {
                return Err(fail("dynamic trait repeats its original method signature"));
            }
        }
        let source =
            selected.ok_or_else(|| fail("dynamic slot has no original trait method signature"))?;
        self.dyn_signature_bridge(method, signature, source, b)?;
        Ok((method, signature, source))
    }
    fn dyn_signature_bridge(
        &self,
        method: SemanticDefinitionId,
        original: &SemanticSignatureRecipe,
        raw: &DynSignatureRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        crate::dyn_metadata::recipe(raw, b)?;
        if original.parameters.len() != raw.parameters.len()
            || original.result_mode != raw.result_mode
            || original.call_kind != raw.call_kind
            || original.invocation_lifetimes != raw.invocation_binder_count
        {
            return Err(fail(
                "dynamic raw signature changes the complete original member ABI",
            ));
        }
        for (raw, original) in raw.parameters.iter().zip(&original.parameters) {
            if raw.mode != original.mode
                || !matches!(&raw.ty,DynTypePattern::Semantic { scope,ty } if *scope==method && ty.as_ref()==&original.ty)
            {
                return Err(fail(
                    "dynamic raw signature changes an exact original parameter recipe",
                ));
            }
        }
        if !matches!(&raw.result,DynTypePattern::Semantic { scope,ty } if *scope==method && ty.as_ref()==&original.result)
        {
            return Err(fail(
                "dynamic raw signature changes its exact original result recipe",
            ));
        }
        Ok(())
    }
}

impl Check<'_, '_> {
    fn dyn_raw_type(
        &self,
        recipe: &DynTypePattern,
        method: SemanticDefinitionId,
        self_type: Option<&Type>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        self.dyn_step(b, depth)?;
        match recipe {
            DynTypePattern::Semantic { scope, ty } if *scope == method => {
                self.dyn_zero_recipe(ty, method, self_type, b, depth + 1)
            }
            DynTypePattern::SelfType => {
                let ty = self_type
                    .ok_or_else(|| unknown("public dynamic contract exposes an unerased Self"))?;
                self.dyn_zero_type(ty, b, depth + 1)?;
                Ok(ty.clone())
            }
            DynTypePattern::Concrete(ty) if ty.is_b0_scalar() || *ty == Type::ForeignHandle => {
                crate::dyn_metadata::ty(ty, b)?;
                Ok(ty.clone())
            }
            DynTypePattern::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    self.push_value(
                        &mut result,
                        self.dyn_raw_type(field, method, self_type, b, depth + 1)?,
                        b,
                    )?;
                }
                Ok(if result.is_empty() {
                    Type::Unit
                } else {
                    Type::Tuple(result)
                })
            }
            _ => Err(unknown(
                "dynamic contract type requires its exact original scoped full recipe",
            )),
        }
    }
    fn dyn_source(
        &self,
        source: &DynStorageSourceRecipe,
        method: SemanticDefinitionId,
        self_type: Option<&Type>,
        b: &mut Budget,
    ) -> CompileResult<StorageSource> {
        crate::dyn_metadata::recipe(source, b)?;
        Ok(match source {
            DynStorageSourceRecipe::Exact(position) => StorageSource::Exact(position.clone()),
            DynStorageSourceRecipe::PlaceAddress { place, mode } => StorageSource::PlaceAddress {
                place: place.clone(),
                mode: *mode,
            },
            DynStorageSourceRecipe::OwnedView { head, leaf, mode } => StorageSource::OwnedView {
                head: head.clone(),
                leaf: self.dyn_raw_type(leaf, method, self_type, b, 0)?,
                mode: *mode,
            },
            DynStorageSourceRecipe::ReborrowedLeaf { source, head } => {
                StorageSource::ReborrowedLeaf {
                    source: source.clone(),
                    head: head.clone(),
                }
            }
            DynStorageSourceRecipe::Static { leaf, mode } => StorageSource::Static {
                leaf: self.dyn_raw_type(leaf, method, self_type, b, 0)?,
                mode: *mode,
            },
        })
    }
    fn dyn_effects(
        &self,
        rows: &[DynEffectRecipe],
        method: SemanticDefinitionId,
        self_type: Option<&Type>,
        b: &mut Budget,
    ) -> CompileResult<Vec<DynEffect>> {
        let mut result = Vec::new();
        for row in rows {
            let effect = match row {
                DynEffectRecipe::Alloc => DynEffect::Alloc,
                DynEffectRecipe::Io => DynEffect::Io,
                DynEffectRecipe::Block => DynEffect::Block,
                DynEffectRecipe::Suspend => DynEffect::Suspend,
                DynEffectRecipe::Spawn => DynEffect::Spawn,
                DynEffectRecipe::Ffi => DynEffect::Ffi,
                DynEffectRecipe::Reenter => DynEffect::Reenter,
                DynEffectRecipe::Panic => DynEffect::Panic,
                DynEffectRecipe::Call(position) => DynEffect::Call(position.clone()),
                DynEffectRecipe::Cleanup(ty) => {
                    DynEffect::Cleanup(self.dyn_raw_type(ty, method, self_type, b, 0)?)
                }
            };
            self.push_value(&mut result, effect, b)?;
        }
        Ok(result)
    }
    fn dyn_contract(
        &self,
        recipe: &DynContractRecipe,
        method: SemanticDefinitionId,
        self_type: Option<&Type>,
        b: &mut Budget,
    ) -> CompileResult<DynContract> {
        crate::dyn_metadata::recipe(recipe, b)?;
        let requires = recipe
            .requires
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method requirement contract is unknown"))?;
        let guarantees = recipe
            .guarantees
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method guarantee contract is unknown"))?;
        let hold = recipe
            .hold
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method hold contract is unknown"))?;
        let call = recipe
            .call
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method call-access contract is unknown"))?;
        let result = recipe
            .result
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method result-source contract is unknown"))?;
        let cleanup = recipe
            .cleanup
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method cleanup contract is unknown"))?;
        let effects = recipe
            .effects
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method effect contract is unknown"))?;
        let resume = recipe
            .resume
            .ok_or_else(|| unknown("original dynamic method resume contract is unknown"))?;
        let threads = recipe
            .threads
            .as_ref()
            .ok_or_else(|| unknown("original dynamic method thread contract is unknown"))?;
        let mut required = Vec::new();
        for row in requires {
            let row = match row {
                DynRequirementRecipe::Read(position) => DynRequirement::Read(position.clone()),
                DynRequirementRecipe::Write(position) => DynRequirement::Write(position.clone()),
                DynRequirementRecipe::Manage(position) => DynRequirement::Manage(position.clone()),
                DynRequirementRecipe::Apart(a, z) => DynRequirement::Apart(a.clone(), z.clone()),
                DynRequirementRecipe::State {
                    position,
                    ty,
                    variant,
                } => DynRequirement::State {
                    position: position.clone(),
                    ty: self.dyn_raw_type(ty, method, self_type, b, 0)?,
                    variant: *variant,
                },
            };
            self.push_value(&mut required, row, b)?;
        }
        let mut guaranteed = Vec::new();
        for row in guarantees {
            let row = match row {
                DynGuaranteeRecipe::StorageSources(row) => {
                    let mut sources = Vec::new();
                    for source in &row.sources {
                        self.push_value(
                            &mut sources,
                            self.dyn_source(source, method, self_type, b)?,
                            b,
                        )?;
                    }
                    DynGuarantee::StorageSources(StorageSourceBinding {
                        position: row.position.clone(),
                        sources,
                    })
                }
                DynGuaranteeRecipe::ManageResult => DynGuarantee::ManageResult,
                DynGuaranteeRecipe::State {
                    position,
                    ty,
                    variant,
                } => DynGuarantee::State {
                    position: position.clone(),
                    ty: self.dyn_raw_type(ty, method, self_type, b, 0)?,
                    variant: *variant,
                },
            };
            self.push_value(&mut guaranteed, row, b)?;
        }
        let result = match result {
            DynResultSourceRecipe::Value => DynResultSource::Value,
            DynResultSourceRecipe::Create => DynResultSource::Create,
            DynResultSourceRecipe::Give => DynResultSource::Give,
            DynResultSourceRecipe::Borrow { sources, mutable } => {
                let mut actual = Vec::new();
                for source in sources {
                    self.push_value(
                        &mut actual,
                        self.dyn_source(source, method, self_type, b)?,
                        b,
                    )?;
                }
                DynResultSource::Borrow {
                    sources: actual,
                    mutable: *mutable,
                }
            }
        };
        Ok(DynContract {
            requires: Some(required),
            guarantees: Some(guaranteed),
            hold: Some(hold.clone()),
            call: Some(call.clone()),
            result: Some(result),
            cleanup: Some(DynCleanupContract {
                environment: cleanup.environment,
                positions: cleanup.positions.clone(),
                access: cleanup.access.clone(),
                effects: self.dyn_effects(&cleanup.effects, method, self_type, b)?,
            }),
            effects: Some(self.dyn_effects(effects, method, self_type, b)?),
            resume: Some(resume),
            threads: Some(threads.clone()),
        })
    }
    fn dyn_public_slot(
        &self,
        interface: &DynInterface,
        slot: &DynSlot,
        b: &mut Budget,
    ) -> CompileResult<()> {
        crate::dyn_metadata::recipe(&slot.member, b)?;
        if slot.member.application != interface.application || !slot.actuals.is_empty() {
            return Err(unknown(
                "dynamic public slot needs its complete inherited or generic method application",
            ));
        }
        let (method, original, raw) = self.dyn_original_signature(
            slot.member.application.declaration,
            slot.member.ordinal,
            b,
        )?;
        let expected = self.dyn_signature(method, original, None, b)?;
        let Type::Address { mutable, .. } = &expected.parameters[0] else {
            return Err(fail(
                "original dynamic receiver interpreter lost its address",
            ));
        };
        if slot.receiver.mutable != *mutable
            || slot.receiver.mode != expected.modes[0]
            || slot.parameters != expected.parameters[1..]
            || slot.parameter_modes != expected.modes[1..]
            || slot.result != expected.result
            || slot.result_mode != expected.result_mode
            || slot.call_kind != expected.call_kind
            || slot.storage_life.as_ref() != Some(&expected.life)
        {
            return Err(fail("dynamic public slot differs from its complete original signature and local class relation"));
        }
        let contract = self.dyn_contract(&raw.contract, method, None, b)?;
        if slot.contract != contract {
            return Err(fail(
                "dynamic public slot changes an original complete contract component",
            ));
        }
        Ok(())
    }
    fn dyn_provider_zero(
        &self,
        receiver: &Value,
        application: &DynTraitApplication,
        implementation: LifeDeclarationId,
        b: &mut Budget,
    ) -> CompileResult<&SemanticInstance> {
        if !application.actuals.is_empty() {
            return Err(unknown(
                "dynamic provider requires its complete original mixed trait application",
            ));
        }
        let provider = self.actual_provider(receiver, application.declaration, &[], b, 0)?;
        let declaration = self.inventory.definition(provider.definition, b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
            declaration: id, ..
        }) = &declaration.body
        else {
            return Err(fail(
                "dynamic provider selects a nonimplementation original scope",
            ));
        };
        if *id != implementation {
            return Err(fail(
                "dynamic table/witness changes the unique original provider declaration",
            ));
        }
        if !provider.actuals.is_empty() {
            return Err(unknown(
                "dynamic provider requires its complete original mixed implementation actuals",
            ));
        }
        let self_value = self.original_scope_self(provider, b, 0)?;
        if self_value != *receiver || !empty(&self_value.life) || !self_value.classes.is_empty() {
            return Err(fail(
                "dynamic provider changes its complete independently computed original Self domain",
            ));
        }
        Ok(provider)
    }
    fn dyn_witnesses(
        &self,
        table: &DynVtable,
        interface: &DynInterface,
        receiver: &Value,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let witnesses = table.implementation_witnesses.as_ref().ok_or_else(|| {
            unknown("dynamic table omits its complete original membership witness graph")
        })?;
        if witnesses.is_empty() {
            return Err(fail("dynamic membership graph omits its root application"));
        }
        let mut roots = 0usize;
        let mut seen = BTreeSet::new();
        for witness in witnesses {
            crate::dyn_metadata::recipe(witness, b)?;
            let actual = self.dyn_zero_value(&witness.receiver, b)?;
            if witness.scope.fixed_binder_count != 0
                || witness.scope.invocation_binder_count != 0
                || !witness.to_context.is_empty()
                || !witness.actuals.is_empty()
                || !witness.associated.is_empty()
            {
                return Err(unknown("dynamic witness requires its full original nonzero or associated specialization"));
            }
            self.dyn_provider_zero(&actual, &witness.application, witness.implementation, b)?;
            if witness.application != interface.application
                || witness.implementation != table.implementation
                || actual != *receiver
            {
                return Err(unknown(
                    "dynamic membership graph needs its complete inherited obligation closure",
                ));
            }
            b.lookup(seen.len(), WORK)?;
            if !seen.insert(witness.implementation) {
                return Err(fail(
                    "dynamic membership graph repeats an original application",
                ));
            }
            roots += 1;
        }
        if roots != 1 {
            return Err(fail(
                "dynamic table has no unique complete original root witness",
            ));
        }
        Ok(())
    }
}

impl Check<'_, '_> {
    fn dyn_selected_definition<'a>(
        &'a self,
        provider: &SemanticInstance,
        slot: &DynSlot,
        b: &mut Budget,
    ) -> CompileResult<&'a DynDefinitionDeclaration> {
        let trait_raw = self
            .inventory
            .original_dyn(slot.member.application.declaration, b)?;
        let trait_member = trait_raw
            .members
            .get(slot.member.ordinal as usize)
            .ok_or_else(|| fail("dynamic entry selects no original trait member"))?;
        let provider_definition = self.inventory.definition(provider.definition, b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration, .. }) =
            &provider_definition.body
        else {
            return Err(fail(
                "dynamic entry provider is not its original implementation",
            ));
        };
        let implementation = self.inventory.original_dyn(*declaration, b)?;
        let definitions = implementation.method_definitions.as_ref().ok_or_else(|| {
            unknown("dynamic entry provider omits its complete original definition inventory")
        })?;
        let mut selected = None;
        let mut named = false;
        for (ordinal, member) in implementation.members.iter().enumerate() {
            b.spend(
                member
                    .name
                    .len()
                    .saturating_add(trait_member.name.len())
                    .saturating_add(1),
                WORK,
            )?;
            if member.name != trait_member.name {
                continue;
            }
            if named || member.kind != trait_member.kind {
                return Err(fail(
                    "dynamic original override repeats or changes the complete member declaration",
                ));
            }
            named = true;
            for definition in definitions {
                b.spend(1, WORK)?;
                if definition.member as usize == ordinal && selected.replace(definition).is_some() {
                    return Err(fail(
                        "dynamic original override repeats its implementation definition",
                    ));
                }
            }
        }
        if let Some(definition) = selected {
            if definition.scope != DynDefinitionScope::ImplOverride {
                return Err(fail(
                    "dynamic implementation relabels an override as a trait default",
                ));
            }
            return Ok(definition);
        }
        let mut default = None;
        for row in trait_raw
            .method_signatures
            .as_ref()
            .ok_or_else(|| unknown("dynamic trait default signature inventory is unknown"))?
        {
            if row.member == slot.member.ordinal
                && default.replace(row.default_definition).is_some()
            {
                return Err(fail("dynamic trait repeats its default selection"));
            }
        }
        let id = default.flatten().ok_or_else(|| {
            fail("dynamic provider omits the original implementation and default body")
        })?;
        let mut selected = None;
        for definition in trait_raw
            .method_definitions
            .as_ref()
            .ok_or_else(|| unknown("dynamic trait default definition inventory is unknown"))?
        {
            b.spend(1, WORK)?;
            if definition.id == id && selected.replace(definition).is_some() {
                return Err(fail("dynamic trait repeats its exact default definition"));
            }
        }
        let selected = selected
            .ok_or_else(|| fail("dynamic trait default selects no actual original body"))?;
        if selected.scope != DynDefinitionScope::TraitDefault
            || selected.member != slot.member.ordinal
        {
            return Err(fail(
                "dynamic default selection changes its owner or member",
            ));
        }
        Ok(selected)
    }
    fn dyn_registry_target(
        &self,
        target: DynFunctionTarget,
        function: FunctionId,
        definition: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        match target {
            DynFunctionTarget::Typed(id) => Ok(id == function),
            DynFunctionTarget::Core { module, source } => {
                let program=self.inventory.source.ok_or_else(||unknown("dynamic Core specialization target needs its complete original Source inventory"))?;
                let original = self.inventory.definition(definition, b)?;
                let mut found = None;
                for unit in &program.modules {
                    b.spend(unit.core.functions.len().saturating_add(1), WORK)?;
                    if unit.core.module_id != module {
                        continue;
                    }
                    for candidate in &unit.core.functions {
                        if candidate.id != source {
                            continue;
                        }
                        if found.replace(candidate).is_some() {
                            return Err(fail(
                                "dynamic original specialization target is ambiguous",
                            ));
                        }
                    }
                }
                let candidate = found
                    .ok_or_else(|| fail("dynamic original specialization target is absent"))?;
                if let Some(scope) = &candidate.source_scope {
                    b.spend(
                        scope
                            .identity
                            .package
                            .len()
                            .saturating_add(scope.identity.generation_path.len())
                            .saturating_add(1),
                        WORK,
                    )?;
                    for part in scope
                        .identity
                        .module_path
                        .iter()
                        .chain(&scope.identity.declaration_path)
                    {
                        b.spend(part.len().saturating_add(1), WORK)?;
                    }
                    if scope.identity != original.identity {
                        return Ok(false);
                    }
                    if !scope.parameters.is_empty() {
                        return Err(unknown(
                            "dynamic source specialization needs its complete mixed scope bridge",
                        ));
                    }
                }
                let mut linked = false;
                for unit in &program.modules {
                    let Some(semantic) = &unit.core.semantic else {
                        continue;
                    };
                    b.spend(
                        semantic
                            .instances
                            .len()
                            .saturating_add(semantic.definitions.len()),
                        WORK,
                    )?;
                    for world in &semantic.instances {
                        if !matches!(world.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module:actual,function:target }) if actual==module && target==source)
                        {
                            continue;
                        }
                        if world.definition != definition
                            || !world.actuals.is_empty()
                            || world.environment.is_some()
                        {
                            return Err(fail("dynamic original specialization target has another exact source world"));
                        }
                        let source_definition = semantic
                            .definitions
                            .iter()
                            .find(|row| row.id == definition)
                            .ok_or_else(|| {
                                fail("dynamic original source world omits its authored definition")
                            })?;
                        crate::dyn_metadata::recipe(source_definition, b)?;
                        crate::dyn_metadata::recipe(original, b)?;
                        if source_definition != original {
                            return Err(fail("dynamic original specialization target changes its complete authored definition"));
                        }
                        let parent=semantic.instances.iter().find(|row|Some(row.id)==world.parent).ok_or_else(||fail("dynamic original source method omits its real lexical parent world"))?;
                        if Some(parent.definition) != original.parent
                            || !parent.actuals.is_empty()
                            || !matches!(parent.target, SemanticInstanceTarget::Scope)
                        {
                            return Err(fail("dynamic original source method changes its original complete parent scope"));
                        }
                        linked = true;
                    }
                }
                if !linked {
                    return Err(unknown("dynamic Core specialization target needs its exact original semantic Source world"));
                }
                // The actual function's worlds, checked below, must all name
                // this exact original definition; same ABI alone is irrelevant.
                Ok(true)
            }
        }
    }
    fn dyn_entry(
        &self,
        table: &DynVtable,
        slot: &DynSlot,
        entry: &DynEntry,
        provider: &SemanticInstance,
        receiver: &Value,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let definition = self.dyn_selected_definition(provider, slot, b)?;
        let expected_definition =
            original::definition_scope(self.program, definition.id, self.inventory, b)?;
        let original = self.inventory.definition(expected_definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature, foreign, ..
        } = &original.body
        else {
            return Err(fail(
                "dynamic concrete entry has no complete original implementation signature",
            ));
        };
        if foreign.is_some() {
            return Err(fail("dynamic concrete entry substitutes a foreign function for its original method body"));
        }
        self.dyn_signature_bridge(expected_definition, signature, &definition.signature, b)?;
        let expected = self.dyn_signature(expected_definition, signature, Some(&receiver.ty), b)?;
        let (public_method, public_signature, public_raw) = self.dyn_original_signature(
            slot.member.application.declaration,
            slot.member.ordinal,
            b,
        )?;
        let public = self.dyn_signature(public_method, public_signature, Some(&receiver.ty), b)?;
        if expected.parameters != public.parameters
            || expected.modes != public.modes
            || expected.result != public.result
            || expected.result_mode != public.result_mode
            || expected.call_kind != public.call_kind
            || expected.life != public.life
        {
            return Err(fail("dynamic implementation changes its original trait method signature or class relation"));
        }
        let promised =
            self.dyn_contract(&public_raw.contract, public_method, Some(&receiver.ty), b)?;
        let implementation = self.dyn_contract(
            &definition.signature.contract,
            expected_definition,
            Some(&receiver.ty),
            b,
        )?;
        if promised != implementation {
            return Err(unknown(
                "dynamic implementation requires its complete original contract refinement theorem",
            ));
        }
        let function = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(entry.function), b)?;
        let instance = function.dyn_instance.as_ref().ok_or_else(|| {
            fail("dynamic entry omits its exact original method-definition specialization")
        })?;
        crate::dyn_metadata::recipe(instance, b)?;
        if instance.definition != definition.id
            || self.dyn_zero_value(&instance.receiver, b)? != *receiver
            || !instance.declaration_actuals.is_empty()
            || !instance.method_actuals.is_empty()
        {
            return Err(fail("dynamic entry changes its exact original definition, complete Self, or mixed specialization"));
        }
        if instance.scope.fixed_binder_count != 0
            || instance.scope.invocation_binder_count != expected.life.invocation_binder_count
        {
            return Err(fail(
                "dynamic entry specialization changes its independently derived full class domain",
            ));
        }
        if instance.header_embedding.len() != expected.life.invocation_binder_count as usize {
            return Err(fail(
                "dynamic entry specialization omits a complete header-class image",
            ));
        }
        let mut embedded = BTreeSet::new();
        for embedding in &instance.header_embedding {
            b.lookup(embedded.len(), WORK)?;
            if !matches!(embedding.from,StorageBinder::Invocation(index) if index<expected.life.invocation_binder_count)
                || embedding.from != embedding.to
                || !embedded.insert(embedding.from)
            {
                return Err(fail(
                    "dynamic entry changes a canonical original local invocation occurrence",
                ));
            }
        }
        if function.foreign.is_some()
            || function.environment.is_some()
            || function.call_kind != expected.call_kind
            || function.parameters.len() != expected.parameters.len()
            || function.return_type != expected.result
            || !(function.result_mode == expected.result_mode
                || expected.result_mode == PassingMode::Infer)
            || function.storage_life.as_ref() != Some(&expected.life)
        {
            return Err(fail("dynamic concrete function entry differs from its original instantiated ABI and class signature"));
        }
        for (local, ty) in function.parameters.iter().zip(&expected.parameters) {
            crate::dyn_metadata::ty(ty, b)?;
            if function.locals.get(local.0 as usize).map(|row| &row.ty) != Some(ty) {
                return Err(fail(
                    "dynamic concrete entry parameter differs from its complete original type",
                ));
            }
        }
        let worlds = self.function_worlds(entry.function, b)?;
        if worlds.is_empty() {
            return Err(fail(
                "dynamic concrete entry has no independently checked original semantic world",
            ));
        }
        for world in worlds {
            crate::dyn_metadata::recipe(world, b)?;
            if world.definition != expected_definition
                || !world.actuals.is_empty()
                || world.environment.is_some()
            {
                return Err(fail("dynamic concrete function has a world for another original definition or specialization"));
            }
            let parent = self.inventory.instance(
                world.parent.ok_or_else(|| {
                    fail("dynamic concrete method world omits its true lexical Self scope")
                })?,
                b,
            )?;
            if original.parent != Some(parent.definition) {
                return Err(fail(
                    "dynamic concrete method world changes its original lexical parent",
                ));
            }
            if definition.scope == DynDefinitionScope::ImplOverride
                && parent.definition != provider.definition
            {
                return Err(fail(
                    "dynamic concrete override selects another original implementation scope",
                ));
            }
            if !parent.actuals.is_empty() || self.original_scope_self(parent, b, 0)? != *receiver {
                return Err(fail("dynamic method parent changes its independently computed complete original Self"));
            }
            let actual = world
                .self_type
                .as_ref()
                .ok_or_else(|| fail("dynamic method world omits its original Self value"))?;
            if self.resolved(world, actual, b, 0)? != *receiver {
                return Err(fail(
                    "dynamic method world changes the exact original Self domain",
                ));
            }
            self.header_relation(world, function, signature, b)?;
        }
        let mut registered = false;
        for specialization in &definition.instances {
            crate::dyn_metadata::recipe(specialization, b)?;
            if specialization.receiver == instance.receiver
                && specialization.declaration_actuals == instance.declaration_actuals
                && specialization.method_actuals == instance.method_actuals
                && self.dyn_registry_target(
                    specialization.function,
                    entry.function,
                    expected_definition,
                    b,
                )?
            {
                registered = true;
            }
        }
        if !registered {
            return Err(fail("dynamic concrete entry is absent from its exact original full-specialization registry"));
        }
        if table.implementation != self.inventory.original_dyn(table.implementation, b)?.id {
            return Err(fail(
                "dynamic entry loses its actual original provider identity",
            ));
        }
        Ok(())
    }
    pub(super) fn dyn_tables(&self, b: &mut Budget) -> CompileResult<()> {
        let mut interfaces = BTreeSet::new();
        for interface in &self.module.dyn_interfaces {
            self.dyn_interface_metadata(interface, b)?;
            b.lookup(interfaces.len(), WORK)?;
            if !interfaces.insert(interface.id) {
                return Err(fail("dynamic interface identity is repeated"));
            }
            if interface.context.is_some()
                || interface.retained_life.binder_count != 0
                || !interface.retained_life.ports.is_empty()
                || !interface.retained_life.retained.is_empty()
                || !interface.retained_life.nominal_arguments.is_empty()
                || !interface.application.actuals.is_empty()
                || !interface.inherited.is_empty()
                || !interface.associated_types.is_empty()
                || !interface.associated_constants.is_empty()
            {
                return Err(unknown("dynamic interface needs its complete original mixed, inherited, associated and retained Context domain"));
            }
            let declaration = self
                .inventory
                .original_dyn(interface.application.declaration, b)?;
            if declaration.kind != DynDeclarationKind::Trait {
                return Err(fail("dynamic interface selects an implementation rather than an original trait declaration"));
            }
            if !declaration.parameters.is_empty()
                || !declaration
                    .direct_supertraits
                    .as_ref()
                    .ok_or_else(|| {
                        unknown("dynamic interface original supertrait inventory is unknown")
                    })?
                    .is_empty()
            {
                return Err(unknown(
                    "dynamic interface needs its original mixed and inherited application closure",
                ));
            }
            let mut members = BTreeSet::new();
            for slot in &interface.slots {
                b.lookup(members.len(), WORK)?;
                if !members.insert(slot.member.ordinal) {
                    return Err(fail(
                        "dynamic public interface repeats an original method slot",
                    ));
                }
                self.dyn_public_slot(interface, slot, b)?;
            }
            for (ordinal, member) in declaration.members.iter().enumerate() {
                b.spend(1, WORK)?;
                match &member.kind {
                    DynMemberKind::Method(parameters) if parameters.is_empty()=>{
                        if !members.contains(&(ordinal as u32)) { return Err(fail("dynamic public interface omits an original nongeneric method slot")); }
                    },
                    _=>return Err(unknown("dynamic interface needs its complete original associated or finite generic member selection")),
                }
            }
        }
        let mut tables = BTreeSet::new();
        let mut functions = BTreeSet::new();
        for table in &self.module.dyn_vtables {
            crate::dyn_metadata::recipe_table(table, b)?;
            b.lookup(tables.len(), WORK)?;
            if !tables.insert(table.id) {
                return Err(fail("dynamic vtable identity is repeated"));
            }
            let mut interface = None;
            for candidate in &self.module.dyn_interfaces {
                b.spend(1, WORK)?;
                if candidate.id == table.interface {
                    interface = Some(candidate);
                }
            }
            let interface = interface
                .ok_or_else(|| fail("dynamic table selects no actual public interface"))?;
            self.dyn_zero_type(&table.self_type, b, 0)?;
            let self_life = table
                .self_life
                .as_ref()
                .ok_or_else(|| unknown("dynamic table omits the complete original Self domain"))?;
            if !empty(self_life)
                || !table.self_classes.is_empty()
                || !table.actuals.is_empty()
                || !table.ports.is_empty()
                || table.cleanup.is_some()
            {
                return Err(unknown("dynamic table needs its complete original retained, mixed or cleanup specialization"));
            }
            let receiver = value::scalar(table.self_type.clone());
            let provider =
                self.dyn_provider_zero(&receiver, &interface.application, table.implementation, b)?;
            self.dyn_witnesses(table, interface, &receiver, b)?;
            let mut slots = BTreeSet::new();
            for entry in &table.entries {
                b.lookup(slots.len(), WORK)?;
                if !slots.insert(entry.slot) {
                    return Err(fail("dynamic table repeats a concrete public-slot entry"));
                }
                let slot = interface
                    .slots
                    .get(entry.slot as usize)
                    .ok_or_else(|| fail("dynamic table entry selects no actual public slot"))?;
                self.dyn_entry(table, slot, entry, provider, &receiver, b)?;
                b.lookup(functions.len(), WORK)?;
                functions.insert(entry.function);
            }
            if slots.len() != interface.slots.len() {
                return Err(fail(
                    "dynamic table omits a complete concrete entry for its public interface",
                ));
            }
        }
        for function in &self.module.functions {
            b.spend(1, WORK)?;
            if function.dyn_instance.is_some() && !functions.contains(&function.id) {
                return Err(unknown("unselected dynamic function specialization needs its complete original standalone definition bridge"));
            }
        }
        Ok(())
    }
}

impl Check<'_, '_> {
    fn dyn_interface_metadata(
        &self,
        interface: &DynInterface,
        b: &mut Budget,
    ) -> CompileResult<()> {
        crate::dyn_metadata::recipe(&interface.application, b)?;
        crate::dyn_metadata::retained_shape(&interface.retained_life, b)?;
        if let Some(context) = &interface.context {
            crate::dyn_metadata::ty(context, b)?;
        }
        for application in &interface.inherited {
            crate::dyn_metadata::recipe(application, b)?;
        }
        for associated in &interface.associated_types {
            crate::dyn_metadata::recipe(&associated.member, b)?;
            crate::dyn_metadata::ty(&associated.ty, b)?;
            crate::dyn_metadata::shape(&associated.life, b)?;
            b.spend(associated.classes.len(), WORK)?;
        }
        for associated in &interface.associated_constants {
            crate::dyn_metadata::recipe(&associated.member, b)?;
            crate::dyn_metadata::ty(&associated.ty, b)?;
            b.spend(1, WORK)?;
        }
        for slot in &interface.slots {
            crate::dyn_metadata::recipe(&slot.member, b)?;
            for actual in &slot.actuals {
                crate::dyn_metadata::recipe(actual, b)?;
            }
            for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
                crate::dyn_metadata::ty(ty, b)?;
            }
            b.spend(slot.parameter_modes.len().saturating_add(1), WORK)?;
            if let Some(life) = &slot.storage_life {
                crate::signature_life::charge(life, b)?;
            }
            // The runtime inventory independently traverses every actual
            // contract. Prepay the original contract before its equality here;
            // exact arity/value checks above bound the compared type leaves.
            self.dyn_contract_metadata(&slot.contract, b)?;
        }
        if let Some(cleanup) = &interface.owning_cleanup {
            b.spend(
                cleanup
                    .positions
                    .len()
                    .saturating_add(cleanup.access.reads.len())
                    .saturating_add(cleanup.access.writes.len())
                    .saturating_add(1),
                WORK,
            )?;
            for position in cleanup
                .positions
                .iter()
                .chain(&cleanup.access.reads)
                .chain(&cleanup.access.writes)
            {
                b.spend(position.path.len(), WORK)?;
            }
            for effect in &cleanup.effects {
                self.dyn_effect_metadata(effect, b)?;
            }
        }
        Ok(())
    }
    fn dyn_effect_metadata(&self, effect: &DynEffect, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, WORK)?;
        match effect {
            DynEffect::Call(position) => b.spend(position.path.len(), WORK),
            DynEffect::Cleanup(ty) => crate::dyn_metadata::ty(ty, b),
            _ => Ok(()),
        }
    }
    fn dyn_source_metadata(&self, source: &StorageSource, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, WORK)?;
        match source {
            StorageSource::Exact(position) => b.spend(position.path.len(), WORK),
            StorageSource::PlaceAddress { place, .. } => b.spend(place.path.len(), WORK),
            StorageSource::OwnedView { head, leaf, .. } => {
                b.spend(head.path.len(), WORK)?;
                crate::dyn_metadata::ty(leaf, b)
            }
            StorageSource::ReborrowedLeaf { source, head } => {
                b.spend(source.path.len().saturating_add(head.path.len()), WORK)
            }
            StorageSource::Static { leaf, .. } => crate::dyn_metadata::ty(leaf, b),
        }
    }
    fn dyn_contract_metadata(&self, contract: &DynContract, b: &mut Budget) -> CompileResult<()> {
        b.spend(9, WORK)?;
        for row in contract.requires.iter().flatten() {
            b.spend(1, WORK)?;
            match row {
                DynRequirement::Read(position)
                | DynRequirement::Write(position)
                | DynRequirement::Manage(position) => b.spend(position.path.len(), WORK)?,
                DynRequirement::Apart(a, z) => {
                    b.spend(a.path.len().saturating_add(z.path.len()), WORK)?
                }
                DynRequirement::State { position, ty, .. } => {
                    b.spend(position.path.len(), WORK)?;
                    crate::dyn_metadata::ty(ty, b)?;
                }
            }
        }
        for row in contract.guarantees.iter().flatten() {
            b.spend(1, WORK)?;
            match row {
                DynGuarantee::StorageSources(row) => {
                    b.spend(row.position.path.len(), WORK)?;
                    for source in &row.sources {
                        self.dyn_source_metadata(source, b)?;
                    }
                }
                DynGuarantee::ManageResult => {}
                DynGuarantee::State { position, ty, .. } => {
                    b.spend(position.path.len(), WORK)?;
                    crate::dyn_metadata::ty(ty, b)?;
                }
            }
        }
        for position in contract.hold.iter().flatten() {
            b.spend(position.path.len().saturating_add(1), WORK)?;
        }
        if let Some(call) = &contract.call {
            for position in call.reads.iter().chain(&call.writes) {
                b.spend(position.path.len().saturating_add(1), WORK)?;
            }
        }
        if let Some(DynResultSource::Borrow { sources, .. }) = &contract.result {
            for source in sources {
                self.dyn_source_metadata(source, b)?;
            }
        }
        if let Some(cleanup) = &contract.cleanup {
            for position in cleanup
                .positions
                .iter()
                .chain(&cleanup.access.reads)
                .chain(&cleanup.access.writes)
            {
                b.spend(position.path.len().saturating_add(1), WORK)?;
            }
            for effect in &cleanup.effects {
                self.dyn_effect_metadata(effect, b)?;
            }
        }
        for effect in contract.effects.iter().flatten() {
            self.dyn_effect_metadata(effect, b)?;
        }
        Ok(())
    }
}
