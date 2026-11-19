//! Finite Store qualification is stricter than Value/SyncDrop/Copy. A genuine
//! phantom actual stays in its type even when it contributes no physical byte.
use super::*;
impl CheckedOriginal {
    fn store_field_has_storage(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<bool> {
        let token = active.fresh(b)?;
        active.with_query(
            DependencyKey {
                kind: QueryKind::Type,
                token,
            },
            b,
            |active, b| match self.resolved(ty, b)? {
                Ty::Atom(ty) => Ok(ty != Type::Unit),
                Ty::Managed(payload) => {
                    self.store_field_has_storage(module, context, &payload, env, active, b)
                }
                Ty::Array(child, length) => Ok(
                    matches!(length.value,ConstValue::Value(Constant::Usize(n))if n>0)
                        && self.store_field_has_storage(module, context, &child, env, active, b)?,
                ),
                Ty::Tuple(fields) => {
                    for field in fields {
                        if self.store_field_has_storage(module, context, &field, env, active, b)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                Ty::Nominal(id, args) => {
                    let definition = context.definition(module, id, b)?;
                    let SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } = &definition.body
                    else {
                        return Ok(true);
                    };
                    let mut selected = self.environment(module, context, id, active, b)?;
                    selected.lexical[0].actuals = args;
                    for field in fields {
                        let child = self.ty(module, context, &field.ty, &selected, active, b)?;
                        if self.store_field_has_storage(module, context, &child, env, active, b)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                Ty::Variable(..) | Ty::Projection { .. } | Ty::SelfType(_) => self
                    .structural_premise(
                        module,
                        context,
                        ty,
                        DynStructuralBound::StoreElement,
                        env,
                        active,
                        b,
                    ),
                Ty::Opaque(..)
                | Ty::Closure(..)
                | Ty::Code { .. }
                | Ty::ErrorUnion(..)
                | Ty::Store(_) => Ok(true),
                _ => Ok(false),
            },
        )
    }


    fn store_copy_field(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<bool> {
        let token = active.fresh(b)?;
        active.with_query(
            DependencyKey {
                kind: QueryKind::Type,
                token,
            },
            b,
            |active, b| {
                Ok(match self.resolved(ty, b)? {
                    Ty::Atom(ty) => {
                        ty.is_b0_scalar() || matches!(ty, Type::ForeignHandle | Type::CWideString)
                    }
                    Ty::Code { .. } => self.is_copy(module, context, ty, env, active, b)?,
                    Ty::Array(child, length) => {
                        matches!(length.value, ConstValue::Value(Constant::Usize(_)))
                            && self.store_copy_field(module, context, &child, env, active, b)?
                    }
                    Ty::ErrorUnion(a, z) => {
                        self.store_copy_field(module, context, &a, env, active, b)?
                            && self.store_copy_field(module, context, &z, env, active, b)?
                    }
                    Ty::Tuple(fields) => {
                        for f in fields {
                            if !self.store_copy_field(module, context, &f, env, active, b)? {
                                return Ok(false);
                            }
                        }
                        true
                    }
                    Ty::Nominal(id, args) => {
                        let def = context.definition(module, id, b)?;
                        let SemanticDefinitionBody::Nominal {
                            kind: LifeNominalKind::Record | LifeNominalKind::Enum,
                            body,
                            ..
                        } = &def.body
                        else {
                            return Ok(false);
                        };
                        let mut selected = self.environment(module, context, id, active, b)?;
                        selected.lexical[0].actuals = args;
                        let mut eligible = |recipe: &SemanticTypeRecipe| {
                            let child = self.ty(module, context, recipe, &selected, active, b)?;
                            self.store_copy_field(module, context, &child, env, active, b)
                        };
                        match body {
                            SemanticNominalBody::Product { fields } => {
                                for f in fields {
                                    if !eligible(&f.ty)? {
                                        return Ok(false);
                                    }
                                }
                            }
                            SemanticNominalBody::Enum { variants } => {
                                for row in variants {
                                    for f in &row.fields {
                                        if !eligible(f)? {
                                            return Ok(false);
                                        }
                                    }
                                }
                            }
                        }
                        self.is_copy(module, context, ty, env, active, b)?
                    }
                    _ => false,
                })
            },
        )
    }
    fn pod_field_extent(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Option<zil_ir::PodExtent>> {
        use zil_ir::PodExtent;
        let token = active.fresh(b)?;
        active.with_query(
            DependencyKey {
                kind: QueryKind::Type,
                token,
            },
            b,
            |active, b| match self.resolved(ty, b)? {
                Ty::Atom(ty) => Ok(PodExtent::scalar(&ty)),
                Ty::Variable(..) => Ok((self.structural_premise(
                    module,
                    context,
                    ty,
                    DynStructuralBound::Copy,
                    env,
                    active,
                    b,
                )? && self.structural_premise(
                    module,
                    context,
                    ty,
                    DynStructuralBound::StoreElement,
                    env,
                    active,
                    b,
                )?)
                .then_some(PodExtent::POSITIVE)),
                Ty::Array(child, length) => {
                    let ConstValue::Value(Constant::Usize(count)) = length.value else {
                        return Ok(None);
                    };
                    Ok(self
                        .pod_field_extent(module, context, &child, env, active, b)?
                        .and_then(|field| field.repeat(count)))
                }
                Ty::Tuple(fields) => {
                    let mut extent = PodExtent::EMPTY;
                    for field in fields {
                        let Some(next) = self
                            .pod_field_extent(module, context, &field, env, active, b)?
                            .and_then(|field| extent.append(field))
                        else {
                            return Ok(None);
                        };
                        extent = next;
                    }
                    Ok(extent.finish())
                }
                Ty::Nominal(id, actuals) => {
                    let decl = context.definition(module, id, b)?;
                    let SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Record,
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } = &decl.body
                    else {
                        return Ok(None);
                    };
                    let mut selected = self.environment(module, context, id, active, b)?;
                    selected.lexical[0].actuals = actuals;
                    let mut extent = PodExtent::EMPTY;
                    for field in fields {
                        let child = self.ty(module, context, &field.ty, &selected, active, b)?;
                        let Some(next) = self
                            .pod_field_extent(module, context, &child, env, active, b)?
                            .and_then(|field| extent.append(field))
                        else {
                            return Ok(None);
                        };
                        extent = next;
                    }
                    Ok(extent.finish())
                }
                _ => Ok(None),
            },
        )
    }
    pub(super) fn store_element(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        self.store_element_at(module,context,ty,env,false,active,b)
    }
    fn store_element_at(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,member:bool,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        if self.structural_premise(module,context,ty,DynStructuralBound::StoreElement,env,active,b)?{return Ok(true);}
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|Ok(match self.resolved(ty,b)?{
            Ty::Atom(ty)=>ty.is_b0_scalar()&&ty!=Type::Unit,
            Ty::Tuple(_) | Ty::Array(..) => self.pod_field_extent(module,context,ty,env,active,b)?.is_some_and(zil_ir::PodExtent::positive),
            Ty::Managed(child)=>self.resource_sync_drop(module,context,&child,env,active,b)? && (member || self.store_field_has_storage(module,context,&child,env,active,b)?),
            Ty::Nominal(id,arguments)=>{
                let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("Store element has no exact original nominal body"));};
                if *kind == LifeNominalKind::Record {
                    return Ok(self.pod_field_extent(module,context,ty,env,active,b)?.is_some_and(zil_ir::PodExtent::positive)
                        && self.sync_drop(module,context,ty,env,active,b)?);
                }
                if *kind==LifeNominalKind::Resource{return Ok(false);}
                let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
                let mut nonempty = false;
                let mut has_manager = false;
                let mut uses_copy_field = false;
                let fields=match body{SemanticNominalBody::Product{fields}=>fields.iter().map(|row|&row.ty).collect::<Vec<_>>(),SemanticNominalBody::Enum{variants}=>{if variants.is_empty(){return Ok(false);}nonempty=true;variants.iter().flat_map(|row|&row.fields).collect()}};
                for field in fields {
                    let child = self.ty(module, context, field, &selected, active, b)?;
                    if child == Ty::Atom(Type::Unit) { continue; }
                    nonempty = true;
                    has_manager |= matches!(child, Ty::Managed(_));
                    if !self.store_element_at(module, context, &child, env, *kind == LifeNominalKind::Package, active, b)? {
                        if !self.store_copy_field(module, context, &child, env, active, b)? {
                            return Ok(false);
                        }
                        uses_copy_field = true;
                    }
                }
                nonempty && (!uses_copy_field || *kind == LifeNominalKind::Package && has_manager)
                    && (member || self.store_field_has_storage(module, context, ty, env, active, b)?)
                    && self.sync_drop(module, context, ty, env, active, b)?
            },
            Ty::ErrorUnion(error,result)=>self.store_element(module,context,&error,env,active,b)?&&self.store_element(module,context,&result,env,active,b)?,
            Ty::Closure(..)=>self.closed_value(module,context,ty,env,active,b)?&&self.sync_drop(module,context,ty,env,active,b)?,
            Ty::Opaque(owner,arguments)=>{
                // Sealed structural qualification follows this exact opaque
                // owner's completed original result, not its Callable bound
                // or a same-shaped emitted closure. Keep phantom actuals in
                // the closed-environment obligation as well as the hidden
                // physical element and its ordinary cleanup proof.
                if !self.closed_value(module,context,ty,env,active,b)?{return Ok(false);}
                let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;
                self.store_element(module,context,&hidden,env,active,b)?
            },
            _=>false,
        }))
    }
}
