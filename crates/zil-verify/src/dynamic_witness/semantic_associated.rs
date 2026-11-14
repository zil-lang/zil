//! Original, static trait queries. Provider selection reads declarations and
//! their lexical recipes; no runtime interface or vtable is manufactured.
use super::*;

/// Private intrinsic keys are separate from their outside class images. A
/// receiver with two existing κ and one outside image therefore still gives
/// two independent original Life actuals to an impl, and constructs an N2 RHS.
pub(super) struct LocalDomain {
    scope: ScopeKey,
    rows: Vec<(ClassKey, ClassKey, IntrinsicOrigin)>,
}
impl LocalDomain {
    pub(super) fn new(scope: ScopeKey) -> Self {
        Self {
            scope,
            rows: vec![],
        }
    }
    fn outside(&self, key: &ClassKey, b: &mut Budget) -> Check<ClassKey> {
        b.spend(self.rows.len())?;
        Ok(self
            .rows
            .iter()
            .find(|(local, _, _)| local == key)
            .map_or_else(|| key.clone(), |(_, outside, _)| outside.clone()))
    }
    fn origin(&self, origin: &IntrinsicOrigin, b: &mut Budget) -> Check<IntrinsicOrigin> {
        b.spend(self.rows.len())?;
        if let IntrinsicOrigin::DerivedClass { source } = origin {
            if let Some((_, _, origin)) = self.rows.iter().find(|(key, _, _)| key == source) {
                return Ok(origin.clone());
            }
        }
        Ok(origin.clone())
    }
    fn value(
        &mut self,
        context: &semantic::CheckedContext,
        env: &EvalEnv,
        value: &CheckedType,
        local: bool,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        charge_type(value, b)?;
        if value.classes.len() != value.origins.len()
            || value.classes.len() != value.life.binder_count as usize
        {
            return Err(bad("original query loses a complete intrinsic domain"));
        }
        let mut result = value.clone();
        for (class, origin) in result.classes.iter_mut().zip(&mut result.origins) {
            *origin = self.origin(origin, b)?;
            if local {
                let mut found = None;
                for (key, outside, known) in &self.rows {
                    if context.same_origin(env, known, origin, b)? {
                        let current = self.outside(class, b)?;
                        if !context.same(env, outside, &current, b)? {
                            return Err(bad(
                                "one original intrinsic origin has conflicting outside images",
                            ));
                        }
                        found = Some(key.clone());
                        break;
                    }
                }
                if let Some(key) = found {
                    *class = key;
                } else {
                    if self.rows.len() >= b.limits.max_nodes {
                        return Err(limit(
                            "original query intrinsic key domain exceeds selected bound",
                        ));
                    }
                    let outside = self.outside(class, b)?;
                    let key = ClassKey::Intrinsic {
                        scope: self.scope,
                        operand: OperandKey::Environment {
                            scope: self.scope,
                            ordinal: 0,
                        },
                        path: vec![],
                        class: u32::try_from(self.rows.len())
                            .map_err(|_| limit("original query intrinsic key domain exceeded"))?,
                    };
                    b.spend(1)?;
                    self.rows.push((key.clone(), outside, origin.clone()));
                    *class = key;
                }
            } else {
                *class = self.outside(class, b)?;
            }
        }
        Ok(result)
    }
    fn actual(
        &mut self,
        context: &semantic::CheckedContext,
        env: &EvalEnv,
        value: &CheckedActual,
        local: bool,
        b: &mut Budget,
    ) -> Check<CheckedActual> {
        Ok(match value {
            CheckedActual::Type(v) => CheckedActual::Type(self.value(context, env, v, local, b)?),
            CheckedActual::Const { carrier, value } => CheckedActual::Const {
                carrier: self.value(context, env, carrier, local, b)?,
                value: value.clone(),
            },
            CheckedActual::Life(k) => CheckedActual::Life(if local {
                k.clone()
            } else {
                self.outside(k, b)?
            }),
        })
    }
}

fn map_type(
    proofs: &mut Proofs,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    value: &CheckedType,
    local: bool,
    b: &mut Budget,
) -> Check<CheckedType> {
    let Some(domain) = proofs.original_domain.as_mut() else {
        return Ok(value.clone());
    };
    domain.value(context, env, value, local, b)
}
fn map_actual(
    proofs: &mut Proofs,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    value: &CheckedActual,
    local: bool,
    b: &mut Budget,
) -> Check<CheckedActual> {
    let Some(domain) = proofs.original_domain.as_mut() else {
        return Ok(value.clone());
    };
    domain.actual(context, env, value, local, b)
}
fn outside_life(proofs: &Proofs, key: &ClassKey, b: &mut Budget) -> Check<ClassKey> {
    proofs
        .original_domain
        .as_ref()
        .map_or_else(|| Ok(key.clone()), |domain| domain.outside(key, b))
}

fn declaration<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a DynDeclaration> {
    b.spend(module.dyn_declarations.len())?;
    let mut rows = module.dyn_declarations.iter().filter(|d| d.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("original static query declaration is absent"))?;
    if rows.next().is_some() {
        return Err(bad("original static query declaration is duplicated"));
    }
    Ok(row)
}

pub(super) fn scope<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a SemanticDefinition> {
    let program = module
        .semantic
        .as_ref()
        .ok_or_else(|| bad("original static query has no semantic scopes"))?;
    b.spend(program.dyn_scopes.len())?;
    let owner = SemanticDynOwner::Declaration(id);
    let mut rows = program.dyn_scopes.iter().filter(|r| r.owner == owner);
    let row = rows
        .next()
        .ok_or_else(|| bad("original declaration has no lexical scope binding"))?;
    if rows.next().is_some() {
        return Err(bad("original declaration has duplicate lexical scopes"));
    }
    bridge::owner(module, &owner, row.scope, b)
}

fn actual_equal(
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    a: &CheckedActual,
    z: &CheckedActual,
    b: &mut Budget,
) -> Check<bool> {
    Ok(match (a, z) {
        (CheckedActual::Type(a), CheckedActual::Type(z)) => context.same_type(env, a, z, b)?,
        (CheckedActual::Life(a), CheckedActual::Life(z)) => context.same(env, a, z, b)?,
        (
            CheckedActual::Const {
                carrier: a,
                value: x,
            },
            CheckedActual::Const {
                carrier: z,
                value: y,
            },
        ) => x == y && context.same_type(env, a, z, b)?,
        _ => false,
    })
}

struct Inference<'a> {
    module: &'a TypedModule,
    context: &'a semantic::CheckedContext,
    caller: &'a EvalEnv,
    definition: &'a SemanticDefinition,
    actuals: Vec<Option<CheckedActual>>,
    implicit: Vec<Option<ClassKey>>,
    outside: Vec<(ClassKey, ClassKey)>,
}
impl Inference<'_> {
    fn bind(&mut self, ordinal: u32, value: CheckedActual, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        let formal = self
            .definition
            .parameters
            .get(ordinal as usize)
            .ok_or_else(|| bad("original provider selects an absent full formal"))?;
        if !matches!(
            (formal, &value),
            (SemanticFormal::Type { .. }, CheckedActual::Type(_))
                | (SemanticFormal::Lifetime, CheckedActual::Life(_))
                | (SemanticFormal::Const { .. }, CheckedActual::Const { .. })
        ) {
            return Err(bad("original provider changes its mixed formal kind"));
        }
        let slot = self
            .actuals
            .get_mut(ordinal as usize)
            .ok_or_else(|| bad("original provider formal is absent"))?;
        if let Some(old) = slot {
            if let (CheckedActual::Life(a), CheckedActual::Life(z)) = (&*old, &value) {
                b.spend(self.outside.len().saturating_mul(2))?;
                let x = self.outside.iter().find(|(local, _)| local == a);
                let y = self.outside.iter().find(|(local, _)| local == z);
                if x.is_some() && y.is_some() {
                    return Ok(a == z);
                }
                return self.context.same(
                    self.caller,
                    x.map_or(a, |(_, v)| v),
                    y.map_or(z, |(_, v)| v),
                    b,
                );
            }
            actual_equal(self.context, self.caller, old, &value, b)
        } else {
            *slot = Some(value);
            Ok(true)
        }
    }
    fn life(
        &mut self,
        recipe: &SemanticLifeRecipe,
        value: &ClassKey,
        b: &mut Budget,
    ) -> Check<bool> {
        match recipe {
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => self.bind(*ordinal, CheckedActual::Life(value.clone()), b),
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => {
                let slot = self
                    .implicit
                    .get_mut(*ordinal as usize)
                    .ok_or_else(|| bad("original provider implicit lifetime is absent"))?;
                if let Some(old) = slot {
                    self.context.same(self.caller, old, value, b)
                } else {
                    *slot = Some(value.clone());
                    Ok(true)
                }
            }
            _ => Ok(false),
        }
    }
    fn constant(
        &mut self,
        recipe: &SemanticConstRecipe,
        carrier: &CheckedType,
        value: &Constant,
        b: &mut Budget,
    ) -> Check<bool> {
        match recipe {
            SemanticConstRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => self.bind(
                *ordinal,
                CheckedActual::Const {
                    carrier: carrier.clone(),
                    value: value.clone(),
                },
                b,
            ),
            SemanticConstRecipe::Value(expected) => Ok(expected == value),
            _ => Ok(true), // Complete expression is re-evaluated after all anchors.
        }
    }
    fn actual(
        &mut self,
        recipe: &SemanticActualRecipe,
        value: &CheckedActual,
        b: &mut Budget,
        depth: usize,
    ) -> Check<bool> {
        Ok(match (recipe, value) {
            (SemanticActualRecipe::Type(p), CheckedActual::Type(v)) => {
                self.ty(p, v, b, depth + 1)?
            }
            (SemanticActualRecipe::Life(p), CheckedActual::Life(v)) => self.life(p, v, b)?,
            (
                SemanticActualRecipe::Const {
                    carrier: p,
                    value: q,
                },
                CheckedActual::Const {
                    carrier: v,
                    value: w,
                },
            ) => self.ty(p, v, b, depth + 1)? && self.constant(q, v, w, b)?,
            _ => false,
        })
    }
    fn ty(
        &mut self,
        recipe: &SemanticTypeRecipe,
        value: &CheckedType,
        b: &mut Budget,
        depth: usize,
    ) -> Check<bool> {
        b.spend(1)?;
        if depth >= b.limits.max_call_depth.min(128) {
            return Err(limit("original provider pattern depth exceeded"));
        }
        match recipe {
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => self.bind(*ordinal, CheckedActual::Type(value.clone()), b),
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let original = self.context.definition(self.module, *definition, b)?;
                let SemanticDefinitionBody::Nominal { declaration, .. } = &original.body else {
                    return Err(bad(
                        "provider nominal recipe selects a nonnominal definition",
                    ));
                };
                let Some(actuals) = nominal_arguments(
                    self.module,
                    self.context,
                    self.caller,
                    *definition,
                    *declaration,
                    value,
                    b,
                )?
                else {
                    return Ok(false);
                };
                if actuals.len() != arguments.len() {
                    return Ok(false);
                }
                for (p, v) in arguments.iter().zip(&actuals) {
                    if !self.actual(p, v, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime,
                inner,
            } => {
                let Type::Address {
                    mutable: actual,
                    inner: ty,
                } = &value.ty
                else {
                    return Ok(false);
                };
                if mutable != actual {
                    return Ok(false);
                }
                let mode = if *mutable {
                    StoredBorrowMode::ExclusiveAddress
                } else {
                    StoredBorrowMode::SharedAddress
                };
                if !self.outer(lifetime, value, mode, b)? {
                    return Ok(false);
                }
                let child = child(self.module, value, ty, LifeTypeProjection::Pointee, b)?;
                self.ty(inner, &child, b, depth + 1)
            }
            SemanticTypeRecipe::Desc { lifetime, inner } => {
                let Type::Desc(ty) = &value.ty else {
                    return Ok(false);
                };
                if !self.outer(lifetime, value, StoredBorrowMode::ReadOnlyDescription, b)? {
                    return Ok(false);
                }
                let child = child(self.module, value, ty, LifeTypeProjection::Pointee, b)?;
                self.ty(inner, &child, b, depth + 1)
            }
            SemanticTypeRecipe::Managed(inner) => {
                let Type::Managed(ty) = &value.ty else {
                    return Ok(false);
                };
                self.ty(
                    inner,
                    &child(self.module, value, ty, LifeTypeProjection::Pointee, b)?,
                    b,
                    depth + 1,
                )
            }
            SemanticTypeRecipe::Tuple(patterns) => {
                let Type::Tuple(fields) = &value.ty else {
                    return Ok(false);
                };
                if patterns.len() != fields.len() {
                    return Ok(false);
                }
                for (index, (p, v)) in patterns.iter().zip(fields).enumerate() {
                    if !self.ty(
                        p,
                        &child(
                            self.module,
                            value,
                            v,
                            LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                            b,
                        )?,
                        b,
                        depth + 1,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            SemanticTypeRecipe::Atom(ty) => Ok(*ty == value.ty),
            SemanticTypeRecipe::Array { element, length } => {
                let Type::Array {
                    element: ty,
                    length: n,
                } = &value.ty
                else {
                    return Ok(false);
                };
                let carrier = closed(self.module, &Type::Usize, b)?;
                Ok(
                    self.ty(element, &closed(self.module, ty, b)?, b, depth + 1)?
                        && self.constant(length, &carrier, &Constant::Usize(u64::from(*n)), b)?,
                )
            }
            SemanticTypeRecipe::Slice(p) | SemanticTypeRecipe::OwnedStore(p) => {
                let ty = match (&value.ty, recipe) {
                    (Type::Slice { element }, SemanticTypeRecipe::Slice(_))
                    | (Type::OwnedStore { element }, SemanticTypeRecipe::OwnedStore(_)) => element,
                    _ => return Ok(false),
                };
                self.ty(p, &closed(self.module, ty, b)?, b, depth + 1)
            }
            // Deferred equations/qualified shapes do not determine a provider.
            // They must independently agree with the complete reconstructed key.
            _ => Ok(true),
        }
    }
    fn outer(
        &mut self,
        recipe: &SemanticOuterLifeRecipe,
        value: &CheckedType,
        mode: StoredBorrowMode,
        b: &mut Budget,
    ) -> Check<bool> {
        b.spend(value.life.slots.len())?;
        let mut rows = value
            .life
            .slots
            .iter()
            .filter(|r| r.path.is_empty() && r.mode == mode);
        let row = rows.next();
        if rows.next().is_some() {
            return Err(bad("provider receiver repeats its outer occurrence"));
        }
        match (recipe,row){
            (SemanticOuterLifeRecipe::Class(p),Some(r))=>self.life(p,value.classes.get(r.binder as usize).ok_or_else(||bad("provider outer occurrence loses its class"))?,b),
            (SemanticOuterLifeRecipe::Static,None)=>Ok(value.statics.iter().any(|r|matches!(r,StaticOccurrence::Direct{path,mode:m,..} if path.is_empty()&&*m==mode))),
            _=>Ok(false),
        }
    }
}

fn closed(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<CheckedType> {
    values::closed(module, ty, b)?;
    Ok(CheckedType {
        ty: ty.clone(),
        life: StorageLifeShape {
            binder_count: 0,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
        classes: vec![],
        origins: vec![],
        statics: vec![],
    })
}

/// A child keeps the actual parent intrinsic origins, even when outside class
/// images coincide. Full child occurrences select each class exactly once.
fn child(
    module: &TypedModule,
    parent: &CheckedType,
    ty: &Type,
    path: LifeTypeProjection,
    b: &mut Budget,
) -> Check<CheckedType> {
    let count = values::intrinsic(module, ty, b)?.count;
    b.spend(count as usize)?;
    let mut indices = vec![];
    for class in 0..count {
        b.spend(parent.life.retained.len())?;
        let mut rows = parent
            .life
            .retained
            .iter()
            .filter(|r| r.path == [path] && r.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("provider child omits an intrinsic occurrence"))?;
        if rows.next().is_some() {
            return Err(bad("provider child repeats an intrinsic occurrence"));
        }
        indices.push(row.binder);
    }
    let mapped = |index: u32| -> Check<u32> {
        indices
            .iter()
            .position(|i| *i == index)
            .map(|i| i as u32)
            .ok_or_else(|| bad("provider child occurrence has no intrinsic anchor"))
    };
    let mut life = StorageLifeShape {
        binder_count: count,
        slots: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    };
    for row in &parent.life.slots {
        let prefix = match path {
            LifeTypeProjection::Place(p) => vec![p],
            _ => vec![],
        };
        if !prefix.is_empty() && row.path.starts_with(&prefix) {
            let mut r = row.clone();
            r.path.drain(..prefix.len());
            r.binder = mapped(r.binder)?;
            life.slots.push(r);
        }
    }
    for row in &parent.life.retained {
        if row.path.first() == Some(&path) && row.path.len() > 1 {
            let mut r = row.clone();
            r.path.remove(0);
            r.binder = mapped(r.binder)?;
            life.retained.push(r);
        }
    }
    for row in &parent.life.nominal_arguments {
        if row.path.first() == Some(&path) {
            let mut r = row.clone();
            r.path.remove(0);
            r.binder = mapped(r.binder)?;
            life.nominal_arguments.push(r);
        }
    }
    // Stored child metadata is authoritative for its own intrinsic selectors;
    // their outside images still come solely from the parent's whole child map.
    if let Type::BorrowedStorage { life: original, .. } = ty {
        life = original.clone();
        if life.binder_count != count {
            return Err(bad("provider child intrinsic domain differs"));
        }
    }
    let mut classes = vec![];
    let mut origins = vec![];
    for i in indices {
        classes.push(
            parent
                .classes
                .get(i as usize)
                .cloned()
                .ok_or_else(|| bad("provider child class is absent"))?,
        );
        origins.push(
            parent
                .origins
                .get(i as usize)
                .cloned()
                .ok_or_else(|| bad("provider child origin is absent"))?,
        );
    }
    let mut statics = vec![];
    for row in &parent.statics {
        if let StaticOccurrence::Direct {
            path: prefix,
            leaf,
            mode,
        } = row
        {
            if prefix.first() == Some(&path) {
                statics.push(StaticOccurrence::Direct {
                    path: prefix[1..].to_vec(),
                    leaf: leaf.clone(),
                    mode: *mode,
                });
            }
        }
    }
    Ok(CheckedType {
        ty: ty.clone(),
        life,
        classes,
        origins,
        statics,
    })
}

fn nominal_arguments(
    module: &TypedModule,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    definition: SemanticDefinitionId,
    declaration: LifeDeclarationId,
    value: &CheckedType,
    b: &mut Budget,
) -> Check<Option<Vec<CheckedActual>>> {
    b.spend(module.life_instances.len())?;
    let mut selected = None;
    for instance in &module.life_instances {
        if instance.declaration != declaration || instance.ty != value.ty {
            continue;
        }
        let mut actuals = vec![];
        for (ordinal, actual) in instance.arguments.iter().enumerate() {
            b.spend(1)?;
            actuals.push(match actual {
                LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. } => {
                    CheckedActual::Type(child(
                        module,
                        value,
                        ty,
                        LifeTypeProjection::TypeArgument(ordinal as u32),
                        b,
                    )?)
                }
                LifeNominalActual::Const(v) => CheckedActual::Const {
                    carrier: closed(module, &v.ty(), b)?,
                    value: v.clone(),
                },
                LifeNominalActual::Lifetime(class) => {
                    b.spend(value.life.nominal_arguments.len())?;
                    let mut rows = value.life.nominal_arguments.iter().filter(|r| {
                        r.path.is_empty()
                            && r.declaration == declaration
                            && r.argument == ordinal as u32
                    });
                    let row = rows
                        .next()
                        .ok_or_else(|| bad("provider nominal lifetime occurrence is absent"))?;
                    if rows.next().is_some() || row.binder != *class {
                        return Err(bad(
                            "provider nominal lifetime occurrence differs from full instance",
                        ));
                    }
                    CheckedActual::Life(
                        value
                            .classes
                            .get(*class as usize)
                            .cloned()
                            .ok_or_else(|| bad("provider nominal lifetime class is absent"))?,
                    )
                }
            });
        }
        retain_actuals(context, env, &mut selected, actuals, b)?;
    }
    if value.life.binder_count == 0 {
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("provider nominal semantic inventory is absent"))?;
        b.spend(program.instances.len())?;
        for instance in &program.instances {
            if instance.definition != definition || instance.target != SemanticInstanceTarget::Scope
            {
                continue;
            }
            let Some(claim) = &instance.self_type else {
                continue;
            };
            let (ty, life, classes) = semantic::resolved(claim)?;
            if *ty != value.ty || life.binder_count != 0 || !classes.is_empty() {
                continue;
            }
            let mut actuals = vec![];
            for actual in &instance.actuals {
                actuals.push(match actual {
                    SemanticActual::Type(v) => {
                        let (ty, life, classes) = semantic::resolved(v)?;
                        if life.binder_count != 0 || !classes.is_empty() {
                            return Err(bad(
                                "zero nominal actual retains an unanchored intrinsic domain",
                            ));
                        }
                        CheckedActual::Type(closed(module, ty, b)?)
                    }
                    SemanticActual::Const { ty, value } => {
                        let (carrier, life, classes) = semantic::resolved(ty)?;
                        if life.binder_count != 0 || !classes.is_empty() || *carrier != value.ty() {
                            return Err(bad("provider nominal constant carrier differs"));
                        }
                        CheckedActual::Const {
                            carrier: closed(module, carrier, b)?,
                            value: value.clone(),
                        }
                    }
                    _ => return Err(bad("zero nominal key has unresolved or lifetime actual")),
                });
            }
            retain_actuals(context, env, &mut selected, actuals, b)?;
        }
    }
    Ok(selected)
}

fn retain_actuals(
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    selected: &mut Option<Vec<CheckedActual>>,
    value: Vec<CheckedActual>,
    b: &mut Budget,
) -> Check<()> {
    if let Some(previous) = selected {
        if previous.len() != value.len() {
            return Err(bad("actual nominal has conflicting complete original keys"));
        }
        for (a, z) in previous.iter().zip(&value) {
            if !actual_equal(context, env, a, z, b)? {
                return Err(bad("actual nominal has ambiguous complete original keys"));
            }
        }
    } else {
        *selected = Some(value);
    }
    Ok(())
}

pub(super) fn type_recipe(
    module: &TypedModule,
    owner: LifeDeclarationId,
    p: &DynTypePattern,
    b: &mut Budget,
    depth: usize,
) -> Check<SemanticTypeRecipe> {
    scoped_type_recipe(module, &SemanticDynOwner::Declaration(owner), p, b, depth)
}
pub(super) fn constant_recipe(
    module: &TypedModule,
    owner: LifeDeclarationId,
    p: &DynConstPattern,
    b: &mut Budget,
    depth: usize,
) -> Check<SemanticConstRecipe> {
    scoped_constant_recipe(module, &SemanticDynOwner::Declaration(owner), p, b, depth)
}

fn life(owner: &SemanticDynOwner, pattern: DynLifePattern) -> Check<SemanticLifeRecipe> {
    Ok(match pattern {
        DynLifePattern::DeclarationParameter(ordinal) => {
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(
                    if matches!(owner, SemanticDynOwner::Declaration(_)) {
                        0
                    } else {
                        1
                    },
                ),
                ordinal,
            })
        }
        DynLifePattern::ImplicitDeclaration(ordinal) => SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(
                if matches!(owner, SemanticDynOwner::Declaration(_)) {
                    0
                } else {
                    1
                },
            ),
            ordinal,
        },
        DynLifePattern::Bound { depth, index } => SemanticLifeRecipe::Bound { depth, index },
        DynLifePattern::Invocation(index) => SemanticLifeRecipe::Invocation(index),
        DynLifePattern::MethodParameter(ordinal) => {
            if matches!(owner, SemanticDynOwner::Declaration(_)) {
                return Err(bad("declaration recipe refers to a method formal"));
            }
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            })
        }
    })
}
fn nominal_definition(
    module: &TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<SemanticDefinitionId> {
    let program = module
        .semantic
        .as_ref()
        .ok_or_else(|| bad("original nominal recipe has no semantic program"))?;
    b.spend(program.definitions.len())?;
    let mut rows = program.definitions.iter().filter(
        |r| matches!(r.body,SemanticDefinitionBody::Nominal{declaration,..} if declaration==id),
    );
    let row = rows
        .next()
        .ok_or_else(|| bad("original nominal recipe has no exact definition"))?;
    if rows.next().is_some() {
        return Err(bad("original nominal recipe has duplicated definition"));
    }
    Ok(row.id)
}
fn application(
    module: &TypedModule,
    owner: &SemanticDynOwner,
    p: &DynSelfPattern,
    b: &mut Budget,
    depth: usize,
) -> Check<SemanticTraitRecipe> {
    let target = declaration(module, p.declaration, b)?;
    if target.kind != DynDeclarationKind::Trait {
        return Err(bad("original application selects a nontrait declaration"));
    }
    Ok(SemanticTraitRecipe {
        declaration: p.declaration,
        arguments: actual_recipes(
            module,
            owner,
            &target.parameters,
            &p.arguments,
            b,
            depth + 1,
        )?,
    })
}
fn actual_recipes(
    module: &TypedModule,
    owner: &SemanticDynOwner,
    formals: &[LifeParameterKind],
    patterns: &[DynPatternActual],
    b: &mut Budget,
    depth: usize,
) -> Check<Vec<SemanticActualRecipe>> {
    b.spend(patterns.len())?;
    if patterns.len() != formals.len() {
        return Err(bad(
            "original application omits its full mixed argument domain",
        ));
    }
    formals
        .iter()
        .zip(patterns)
        .map(|(kind, p)| {
            Ok(match (kind, p) {
                (LifeParameterKind::Type, DynPatternActual::Type(p)) => {
                    SemanticActualRecipe::Type(scoped_type_recipe(module, owner, p, b, depth + 1)?)
                }
                (LifeParameterKind::Lifetime, DynPatternActual::Life(p)) => {
                    SemanticActualRecipe::Life(life(owner, *p)?)
                }
                (LifeParameterKind::Const(carrier), DynPatternActual::Const(p)) => {
                    SemanticActualRecipe::Const {
                        carrier: SemanticTypeRecipe::Exact(Box::new(SemanticExactTypeRecipe {
                            ty: carrier.clone(),
                            life: StorageLifeShape {
                                binder_count: 0,
                                slots: vec![],
                                retained: vec![],
                                nominal_arguments: vec![],
                            },
                            classes: vec![],
                        })),
                        value: scoped_constant_recipe(module, owner, p, b, depth + 1)?,
                    }
                }
                _ => return Err(bad("original application changes a mixed formal kind")),
            })
        })
        .collect()
}
fn scoped_type_recipe(
    module: &TypedModule,
    owner: &SemanticDynOwner,
    p: &DynTypePattern,
    b: &mut Budget,
    depth: usize,
) -> Check<SemanticTypeRecipe> {
    b.spend(1)?;
    if depth >= b.limits.max_call_depth.min(128) {
        return Err(limit("original associated type recipe depth exceeded"));
    }
    let nested = |p: &DynTypePattern, b: &mut Budget| {
        scoped_type_recipe(module, owner, p, b, depth + 1).map(Box::new)
    };
    Ok(match p {
        DynTypePattern::Semantic { scope, ty } => {
            bridge::owner(module, owner, *scope, b)?;
            dynamic_metadata::recipe(ty.as_ref(), b)?;
            ty.as_ref().clone()
        }
        DynTypePattern::SelfType => SemanticTypeRecipe::SelfType {
            scope: SemanticScopeRef::Ancestor(
                if matches!(owner, SemanticDynOwner::Declaration(_)) {
                    0
                } else {
                    1
                },
            ),
        },
        DynTypePattern::Parameter(ordinal) => SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(
                if matches!(owner, SemanticDynOwner::Declaration(_)) {
                    0
                } else {
                    1
                },
            ),
            ordinal: *ordinal,
        }),
        DynTypePattern::MethodParameter(ordinal) => {
            if matches!(owner, SemanticDynOwner::Declaration(_)) {
                return Err(bad("declaration type refers to a method formal"));
            }
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: *ordinal,
            })
        }
        DynTypePattern::Concrete(ty) => {
            values::closed(module, ty, b)?;
            SemanticTypeRecipe::Exact(Box::new(SemanticExactTypeRecipe {
                ty: ty.clone(),
                life: StorageLifeShape {
                    binder_count: 0,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                classes: vec![],
            }))
        }
        DynTypePattern::Nominal(p) => {
            let id = nominal_definition(module, p.declaration, b)?;
            b.spend(module.life_declarations.len())?;
            let mut rows = module
                .life_declarations
                .iter()
                .filter(|r| r.id == p.declaration);
            let d = rows
                .next()
                .ok_or_else(|| bad("nominal recipe has no original formal directory"))?;
            if rows.next().is_some() {
                return Err(bad("nominal recipe formal directory duplicated"));
            }
            SemanticTypeRecipe::Nominal {
                definition: id,
                arguments: actual_recipes(
                    module,
                    owner,
                    &d.parameters,
                    &p.arguments,
                    b,
                    depth + 1,
                )?,
            }
        }
        DynTypePattern::AssociatedProjection {
            receiver,
            application: p,
            member,
        } => SemanticTypeRecipe::AssociatedType {
            receiver: nested(receiver, b)?,
            application: Box::new(application(module, owner, p, b, depth + 1)?),
            member: *member,
        },
        DynTypePattern::Address {
            mutable,
            lifetime,
            inner,
        } => SemanticTypeRecipe::Address {
            mutable: *mutable,
            lifetime: SemanticOuterLifeRecipe::Class(life(
                owner,
                lifetime.ok_or_else(|| bad("original associated Address lifetime is Unknown"))?,
            )?),
            inner: nested(inner, b)?,
        },
        DynTypePattern::Desc { lifetime, inner } => SemanticTypeRecipe::Desc {
            lifetime: SemanticOuterLifeRecipe::Class(life(
                owner,
                lifetime
                    .ok_or_else(|| bad("original associated Description lifetime is Unknown"))?,
            )?),
            inner: nested(inner, b)?,
        },
        DynTypePattern::Managed(p) => SemanticTypeRecipe::Managed(nested(p, b)?),
        DynTypePattern::OwnedStore(p) => SemanticTypeRecipe::OwnedStore(nested(p, b)?),
        DynTypePattern::Slice(p) => SemanticTypeRecipe::Slice(nested(p, b)?),
        DynTypePattern::Tuple(ps) => SemanticTypeRecipe::Tuple(
            ps.iter()
                .map(|p| scoped_type_recipe(module, owner, p, b, depth + 1))
                .collect::<Check<Vec<_>>>()?,
        ),
        DynTypePattern::Array { element, length } => SemanticTypeRecipe::Array {
            element: nested(element, b)?,
            length: Box::new(scoped_constant_recipe(module, owner, length, b, depth + 1)?),
        },
        DynTypePattern::ErrorUnion { error, result } => SemanticTypeRecipe::ErrorUnion {
            error: nested(error, b)?,
            result: nested(result, b)?,
        },
    })
}
fn scoped_constant_recipe(
    module: &TypedModule,
    owner: &SemanticDynOwner,
    p: &DynConstPattern,
    b: &mut Budget,
    depth: usize,
) -> Check<SemanticConstRecipe> {
    b.spend(1)?;
    if depth >= b.limits.max_call_depth.min(128) {
        return Err(limit("original associated constant recipe depth exceeded"));
    }
    let nested = |p: &DynConstPattern, b: &mut Budget| {
        scoped_constant_recipe(module, owner, p, b, depth + 1).map(Box::new)
    };
    Ok(match p {
        DynConstPattern::Value(v) => SemanticConstRecipe::Value(v.clone()),
        DynConstPattern::IntegerLiteral(v) => SemanticConstRecipe::IntegerLiteral(v.clone()),
        DynConstPattern::FloatLiteral(v) => SemanticConstRecipe::FloatLiteral(v.clone()),
        DynConstPattern::Parameter(ordinal) => SemanticConstRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(
                if matches!(owner, SemanticDynOwner::Declaration(_)) {
                    0
                } else {
                    1
                },
            ),
            ordinal: *ordinal,
        }),
        DynConstPattern::MethodParameter(ordinal) => {
            if matches!(owner, SemanticDynOwner::Declaration(_)) {
                return Err(bad("declaration constant refers to a method formal"));
            }
            SemanticConstRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: *ordinal,
            })
        }
        DynConstPattern::Unary { op, value, ty } => SemanticConstRecipe::Unary {
            op: *op,
            operand_type: Box::new(scoped_type_recipe(module, owner, ty, b, depth + 1)?),
            value: nested(value, b)?,
        },
        DynConstPattern::Binary {
            op,
            left,
            right,
            ty,
        } => SemanticConstRecipe::Binary {
            op: *op,
            operand_type: Box::new(scoped_type_recipe(module, owner, ty, b, depth + 1)?),
            left: nested(left, b)?,
            right: nested(right, b)?,
        },
        DynConstPattern::Convert { .. } => {
            return Err(bad(
                "original associated conversion lacks its complete source carrier",
            ));
        }
        DynConstPattern::AssociatedProjection {
            receiver,
            application: p,
            member,
        } => SemanticConstRecipe::AssociatedConst {
            receiver: Box::new(scoped_type_recipe(module, owner, receiver, b, depth + 1)?),
            application: Box::new(application(module, owner, p, b, depth + 1)?),
            member: *member,
        },
    })
}

fn requirements(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    owner: LifeDeclarationId,
    rows: &[DynObligation],
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<()> {
    let recipes = requirement_recipes(module, owner, rows, b)?;
    semantic_requirements(proofs, module, context, env, &recipes, active, b, 0)
}

pub(super) fn requirement_recipes(
    module: &TypedModule,
    owner: LifeDeclarationId,
    rows: &[DynObligation],
    b: &mut Budget,
) -> Check<Vec<SemanticRequirementRecipe>> {
    scoped_requirement_recipes(module, &SemanticDynOwner::Declaration(owner), rows, b)
}

fn scoped_requirement_recipes(
    module: &TypedModule,
    owner: &SemanticDynOwner,
    rows: &[DynObligation],
    b: &mut Budget,
) -> Check<Vec<SemanticRequirementRecipe>> {
    let mut recipes = vec![];
    for row in rows {
        b.spend(1)?;
        let ty = |p: &DynTypePattern, b: &mut Budget| scoped_type_recipe(module, owner, p, b, 0);
        recipes.push(match row {
            DynObligation::Structural { receiver, bound } => {
                SemanticRequirementRecipe::Structural {
                    ty: ty(receiver, b)?,
                    bound: *bound,
                }
            }
            DynObligation::Trait {
                receiver,
                application: p,
            } => SemanticRequirementRecipe::Trait {
                ty: ty(receiver, b)?,
                application: application(module, owner, p, b, 0)?,
            },
            DynObligation::TypeEqual { left, right } => SemanticRequirementRecipe::TypeEqual {
                left: ty(left, b)?,
                right: ty(right, b)?,
            },
            DynObligation::ConstEqual { ty: p, left, right } => {
                SemanticRequirementRecipe::ConstEqual {
                    carrier: ty(p, b)?,
                    left: scoped_constant_recipe(module, owner, left, b, 0)?,
                    right: scoped_constant_recipe(module, owner, right, b, 0)?,
                }
            }
            DynObligation::Callable {
                receiver,
                kind,
                quantified_lifetimes,
                signature,
            } => {
                let signature = SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes: signature.invocation_binder_count,
                    parameters: signature
                        .parameters
                        .iter()
                        .map(|p| {
                            Ok(SemanticParameterRecipe {
                                ty: ty(&p.ty, b)?,
                                mode: p.mode,
                            })
                        })
                        .collect::<Check<Vec<_>>>()?,
                    result: ty(&signature.result, b)?,
                    result_mode: signature.result_mode,
                    call_kind: signature.call_kind,
                };
                let requirement = SemanticRequirementRecipe::Callable {
                    ty: ty(receiver, b)?,
                    kind: *kind,
                    signature: Box::new(signature),
                };
                if *quantified_lifetimes == 0 {
                    requirement
                } else {
                    SemanticRequirementRecipe::ForAll {
                        lifetimes: *quantified_lifetimes,
                        requirements: vec![requirement],
                    }
                }
            }
        });
    }
    Ok(recipes)
}

fn external_obligation(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    request: ExternalRequest,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<()> {
    let request = match request {
        ExternalRequest::Builtin {
            receiver,
            trait_kind,
        } => ExternalRequest::Builtin {
            receiver: map_type(proofs, context, env, &receiver, false, b)?,
            trait_kind,
        },
        ExternalRequest::Structural { receiver, bound } => ExternalRequest::Structural {
            receiver: map_type(proofs, context, env, &receiver, false, b)?,
            bound,
        },
        ExternalRequest::Trait {
            receiver,
            application,
        } => {
            let mut arguments = vec![];
            for a in &application.arguments {
                arguments.push(map_actual(proofs, context, env, a, false, b)?);
            }
            ExternalRequest::Trait {
                receiver: map_type(proofs, context, env, &receiver, false, b)?,
                application: CheckedApplication {
                    declaration: application.declaration,
                    arguments,
                },
            }
        }
        ExternalRequest::Callable {
            receiver,
            kind,
            mut signature,
        } => {
            for (p, _) in &mut signature.parameters {
                *p = map_type(proofs, context, env, p, false, b)?;
            }
            signature.result = map_type(proofs, context, env, &signature.result, false, b)?;
            ExternalRequest::Callable {
                receiver: map_type(proofs, context, env, &receiver, false, b)?,
                kind,
                signature,
            }
        }
        _ => return Err(bad("original requirement has another external query kind")),
    };
    context.external_requirement(
        module,
        env,
        request,
        RequirementMode::ActualObligation,
        proofs,
        active,
        b,
    )
}

/// Equations use the original caller Γ after explicit outside composition;
/// construction remains in the distinct local intrinsic domain throughout.
fn semantic_requirements(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    rows: &[SemanticRequirementRecipe],
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
    depth: usize,
) -> Check<()> {
    // A domain frame is an untrusted metadata result with a mandatory later
    // WF obligation. This entry introduces no assumption and returns no proof.
    if context.domain_only.get() {
        return Ok(());
    }
    b.spend(rows.len().saturating_add(1))?;
    if depth >= b.limits.max_call_depth.min(128) {
        return Err(limit("original where quantifier depth exceeded"));
    }
    for row in rows {
        match row {
            SemanticRequirementRecipe::LifeEqual { left, right }
            | SemanticRequirementRecipe::Outlives {
                longer: left,
                shorter: right,
            } => {
                let left = context.life(module, left, env, b)?;
                let right = context.life(module, right, env, b)?;
                let left = outside_life(proofs, &left, b)?;
                let right = outside_life(proofs, &right, b)?;
                let established = if matches!(row, SemanticRequirementRecipe::LifeEqual { .. }) {
                    context.same(env, &left, &right, b)?
                } else {
                    context.outlives(env, &left, &right, b)?
                };
                if !established {
                    return Err(bad(
                        "actual provider does not prove its original lifetime requirement",
                    ));
                }
            }
            SemanticRequirementRecipe::TypeEqual { left, right } => {
                let left = context.evaluate_type(module, left, env, proofs, active, b)?;
                let right = context.evaluate_type(module, right, env, proofs, active, b)?;
                let left = map_type(proofs, context, env, &left, false, b)?;
                let right = map_type(proofs, context, env, &right, false, b)?;
                if !context.same_type(env, &left, &right, b)? {
                    return Err(bad(
                        "actual provider does not prove its original complete type equation",
                    ));
                }
            }
            SemanticRequirementRecipe::ConstEqual {
                carrier,
                left,
                right,
            } => {
                let carrier = context.evaluate_type(module, carrier, env, proofs, active, b)?;
                let left =
                    context.evaluate_constant(module, left, &carrier, env, proofs, active, b)?;
                let right =
                    context.evaluate_constant(module, right, &carrier, env, proofs, active, b)?;
                if left != right {
                    return Err(bad(
                        "actual provider does not prove its original constant equation",
                    ));
                }
            }
            SemanticRequirementRecipe::Builtin { ty, trait_kind } => {
                let receiver = context.evaluate_type(module, ty, env, proofs, active, b)?;
                external_obligation(
                    proofs,
                    module,
                    context,
                    env,
                    ExternalRequest::Builtin {
                        receiver,
                        trait_kind: *trait_kind,
                    },
                    active,
                    b,
                )?;
            }
            SemanticRequirementRecipe::Structural { ty, bound } => {
                let receiver = context.evaluate_type(module, ty, env, proofs, active, b)?;
                external_obligation(
                    proofs,
                    module,
                    context,
                    env,
                    ExternalRequest::Structural {
                        receiver,
                        bound: *bound,
                    },
                    active,
                    b,
                )?;
            }
            SemanticRequirementRecipe::Trait { ty, application } => {
                let receiver = context.evaluate_type(module, ty, env, proofs, active, b)?;
                let application =
                    context.application(module, application, env, proofs, active, b)?;
                external_obligation(
                    proofs,
                    module,
                    context,
                    env,
                    ExternalRequest::Trait {
                        receiver,
                        application,
                    },
                    active,
                    b,
                )?;
            }
            SemanticRequirementRecipe::Callable {
                ty,
                kind,
                signature,
            } => {
                let receiver = context.evaluate_type(module, ty, env, proofs, active, b)?;
                let signature =
                    context.evaluate_signature(module, signature, env, proofs, active, b)?;
                external_obligation(
                    proofs,
                    module,
                    context,
                    env,
                    ExternalRequest::Callable {
                        receiver,
                        kind: *kind,
                        signature,
                    },
                    active,
                    b,
                )?;
            }
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                let mut env = env.clone();
                if *lifetimes != 0 {
                    b.spend(*lifetimes as usize)?;
                    let token = active.fresh(b)?;
                    env.bounds.insert(
                        0,
                        BoundFrame {
                            token,
                            classes: (0..*lifetimes)
                                .map(|index| ClassKey::Bound {
                                    frame: token,
                                    index,
                                })
                                .collect(),
                        },
                    );
                }
                semantic_requirements(
                    proofs,
                    module,
                    context,
                    &env,
                    requirements,
                    active,
                    b,
                    depth + 1,
                )?;
            }
        }
    }
    Ok(())
}

fn environment(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    caller: &EvalEnv,
    definition: &SemanticDefinition,
    actuals: Vec<CheckedActual>,
    receiver: &CheckedType,
    implicit: &[Option<ClassKey>],
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<EvalEnv> {
    let mut env = caller.clone();
    env.lexical = if let Some(parent) = definition.parent {
        let i = caller
            .lexical
            .iter()
            .position(|r| r.definition == parent)
            .ok_or_else(|| bad("original provider has no actual lexical parent"))?;
        caller.lexical[i..].to_vec()
    } else {
        vec![]
    };
    let mut sealed = vec![];
    for formal in &definition.environment {
        sealed.push(context.evaluate_actual(module, &formal.source, &env, proofs, active, b)?);
    }
    let key = ScopeKey::Witness {
        definition: definition.id,
        frame: active.fresh(b)?,
    };
    for (ordinal, actual) in implicit.iter().enumerate() {
        if let Some(actual) = actual {
            env.images.push((
                ClassKey::Implicit {
                    scope: key,
                    ordinal: ordinal as u32,
                },
                actual.clone(),
            ));
        }
    }
    env.lexical.insert(
        0,
        LexicalFrame {
            definition: definition.id,
            key,
            actuals,
            environment: sealed,
            self_type: Some(receiver.clone()),
        },
    );
    Ok(env)
}

fn application_equal(
    proofs: &mut Proofs,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    a: &CheckedApplication,
    z: &CheckedApplication,
    b: &mut Budget,
) -> Check<bool> {
    if a.declaration != z.declaration || a.arguments.len() != z.arguments.len() {
        return Ok(false);
    }
    for (a, z) in a.arguments.iter().zip(&z.arguments) {
        let equal = match (a, z) {
            (CheckedActual::Life(a), CheckedActual::Life(z)) => context.same(
                env,
                &outside_life(proofs, a, b)?,
                &outside_life(proofs, z, b)?,
                b,
            )?,
            _ => {
                let a = map_actual(proofs, context, env, a, true, b)?;
                let z = map_actual(proofs, context, env, z, true, b)?;
                actual_equal(context, env, &a, &z, b)?
            }
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

fn check_formals(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    definition: &SemanticDefinition,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<()> {
    if context.domain_only.get() {
        return Ok(());
    }
    let frame = env
        .lexical
        .first()
        .ok_or_else(|| bad("original provider has no lexical frame"))?;
    if frame.actuals.len() != definition.parameters.len() {
        return Err(bad("original provider has incomplete mixed actuals"));
    }
    for (formal, actual) in definition.parameters.iter().zip(&frame.actuals) {
        match (formal, actual) {
            (SemanticFormal::Type { requirements }, CheckedActual::Type(_)) => {
                semantic_requirements(proofs, module, context, env, requirements, active, b, 0)?
            }
            (SemanticFormal::Lifetime, CheckedActual::Life(_)) => {}
            (
                SemanticFormal::Const { carrier },
                CheckedActual::Const {
                    carrier: actual,
                    value,
                },
            ) => {
                let carrier = context.evaluate_type(module, carrier, env, proofs, active, b)?;
                if !context.same_type(env, &carrier, actual, b)? || carrier.ty != value.ty() {
                    return Err(bad(
                        "original provider constant actual has a different carrier",
                    ));
                }
            }
            _ => {
                return Err(bad(
                    "original provider actual changes its mixed formal kind",
                ));
            }
        }
    }
    semantic_requirements(
        proofs,
        module,
        context,
        env,
        &definition.requirements,
        active,
        b,
        0,
    )
}

pub(super) fn query(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    caller: &EvalEnv,
    request: &ExternalRequest,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<ExternalValue> {
    let (receiver, application, member) = match request {
        ExternalRequest::Trait {
            receiver,
            application,
        } => (receiver, application, None),
        ExternalRequest::AssociatedType {
            receiver,
            application,
            member,
        } => (receiver, application, Some((*member, false))),
        ExternalRequest::AssociatedConst {
            receiver,
            application,
            member,
        } => (receiver, application, Some((*member, true))),
        _ => return Err(bad("original query received another request kind")),
    };
    let root = proofs.original_domain.is_none();
    let wants_local = proofs.original_domain.as_ref().is_some_and(|domain| {
        receiver
            .classes
            .iter()
            .chain(application.arguments.iter().filter_map(|a| {
                if let CheckedActual::Life(k) = a {
                    Some(k)
                } else {
                    None
                }
            }))
            .any(|key| domain.rows.iter().any(|(local, _, _)| local == key))
    });
    if root {
        let definition = scope(module, application.declaration, b)?.id;
        proofs.original_domain = Some(LocalDomain::new(ScopeKey::Witness {
            definition,
            frame: active.fresh(b)?,
        }));
    }
    let result = (|| {
        let receiver = map_type(proofs, context, caller, receiver, true, b)?;
        let mut arguments = vec![];
        for a in &application.arguments {
            arguments.push(map_actual(proofs, context, caller, a, true, b)?);
        }
        let application = CheckedApplication {
            declaration: application.declaration,
            arguments,
        };
        let request = match member {
            None => ExternalRequest::Trait {
                receiver,
                application,
            },
            Some((member, false)) => ExternalRequest::AssociatedType {
                receiver,
                application,
                member,
            },
            Some((member, true)) => ExternalRequest::AssociatedConst {
                receiver,
                application,
                member,
            },
        };
        let value = query_inner(proofs, module, context, caller, &request, active, b)?;
        Ok(match value {
            ExternalValue::Type(value) => {
                ExternalValue::Type(map_type(proofs, context, caller, &value, wants_local, b)?)
            }
            ExternalValue::Const { carrier, value } => ExternalValue::Const {
                carrier: map_type(proofs, context, caller, &carrier, wants_local, b)?,
                value,
            },
            ExternalValue::Proved => ExternalValue::Proved,
        })
    })();
    if root {
        proofs.original_domain = None;
    }
    result
}

fn query_inner(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    caller: &EvalEnv,
    request: &ExternalRequest,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<ExternalValue> {
    let (receiver, application, requested) = match request {
        ExternalRequest::Trait {
            receiver,
            application,
        } => (receiver, application, None),
        ExternalRequest::AssociatedType {
            receiver,
            application,
            member,
        } => (receiver, application, Some((*member, false))),
        ExternalRequest::AssociatedConst {
            receiver,
            application,
            member,
        } => (receiver, application, Some((*member, true))),
        _ => return Err(bad("original query received another external request kind")),
    };
    let Provider {
        trait_decl,
        implementation,
        trait_env,
        impl_env,
    } = provider(
        proofs,
        module,
        context,
        caller,
        receiver,
        application,
        active,
        b,
    )?;
    if let Some((member, constant)) = requested {
        associated(
            proofs,
            module,
            context,
            trait_decl,
            implementation,
            &trait_env,
            &impl_env,
            member,
            constant,
            active,
            b,
        )
    } else {
        // Every associated member is part of the original trait instance,
        // including unused members and defaults. No ordinal coincidence is used.
        for (ordinal, member) in trait_decl.members.iter().enumerate() {
            match member.kind {
                DynMemberKind::Type => {
                    associated(
                        proofs,
                        module,
                        context,
                        trait_decl,
                        implementation,
                        &trait_env,
                        &impl_env,
                        ordinal as u32,
                        false,
                        active,
                        b,
                    )?;
                }
                DynMemberKind::Constant => {
                    associated(
                        proofs,
                        module,
                        context,
                        trait_decl,
                        implementation,
                        &trait_env,
                        &impl_env,
                        ordinal as u32,
                        true,
                        active,
                        b,
                    )?;
                }
                DynMemberKind::Method(_) => {}
            }
        }
        methods(
            proofs,
            module,
            context,
            trait_decl,
            implementation,
            &trait_env,
            &impl_env,
            active,
            b,
        )?;
        Ok(ExternalValue::Proved)
    }
}

struct Provider<'a> {
    trait_decl: &'a DynDeclaration,
    implementation: &'a DynDeclaration,
    trait_env: EvalEnv,
    impl_env: EvalEnv,
}

#[allow(clippy::too_many_arguments)]
fn provider<'a>(
    proofs: &mut Proofs,
    module: &'a TypedModule,
    context: &semantic::CheckedContext,
    caller: &EvalEnv,
    receiver: &CheckedType,
    application: &CheckedApplication,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<Provider<'a>> {
    let trait_decl = declaration(module, application.declaration, b)?;
    if trait_decl.kind != DynDeclarationKind::Trait {
        return Err(bad("original trait query selects a nontrait declaration"));
    }
    let trait_scope = scope(module, trait_decl.id, b)?;
    if !matches!(trait_scope.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration}) if declaration==trait_decl.id)
    {
        return Err(bad("original trait scope has another body"));
    }
    let mut selected = None;
    b.spend(module.dyn_declarations.len())?;
    for implementation in &module.dyn_declarations {
        if implementation.kind != DynDeclarationKind::Impl {
            continue;
        }
        let definition = scope(module, implementation.id, b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
            self_type,
            trait_application,
            ..
        }) = &definition.body
        else {
            continue;
        };
        if trait_application.declaration != application.declaration {
            continue;
        }
        if trait_application.arguments.len() != application.arguments.len() {
            return Err(bad(
                "original provider trait application has incomplete mixed arguments",
            ));
        }
        b.spend(
            definition
                .parameters
                .len()
                .saturating_add(definition.implicit_lifetimes.len()),
        )?;
        let mut inference = Inference {
            module,
            context,
            caller,
            definition,
            actuals: vec![None; definition.parameters.len()],
            implicit: vec![None; definition.implicit_lifetimes.len()],
            outside: proofs.original_domain.as_ref().map_or_else(Vec::new, |d| {
                d.rows
                    .iter()
                    .map(|(k, v, _)| (k.clone(), v.clone()))
                    .collect()
            }),
        };
        if !inference.ty(self_type, receiver, b, 0)? {
            continue;
        }
        let mut matches = true;
        for (p, v) in trait_application
            .arguments
            .iter()
            .zip(&application.arguments)
        {
            if !inference.actual(p, v, b, 0)? {
                matches = false;
                break;
            }
        }
        if !matches {
            continue;
        }
        let Some(actuals) = inference.actuals.into_iter().collect::<Option<Vec<_>>>() else {
            continue;
        };
        let env = environment(
            proofs,
            module,
            context,
            caller,
            definition,
            actuals,
            receiver,
            &inference.implicit,
            active,
            b,
        )?;
        let self_value = context.evaluate_type(module, self_type, &env, proofs, active, b)?;
        let self_value = map_type(proofs, context, &env, &self_value, true, b)?;
        if !context.same_type(caller, &self_value, receiver, b)? {
            continue;
        }
        let app = context.application(module, trait_application, &env, proofs, active, b)?;
        if !application_equal(proofs, context, caller, &app, application, b)? {
            continue;
        }
        if selected
            .replace((implementation, definition, env))
            .is_some()
        {
            return Err(bad("complete original trait query has multiple providers"));
        }
    }
    let (implementation, definition, impl_env) =
        selected.ok_or_else(|| bad("complete original trait query has no provider"))?;
    check_formals(proofs, module, context, &impl_env, definition, active, b)?;
    requirements(
        proofs,
        module,
        context,
        &impl_env,
        implementation.id,
        implementation
            .requirements
            .as_ref()
            .ok_or_else(|| bad("original provider requirements are Unknown"))?,
        active,
        b,
    )?;
    let trait_env = environment(
        proofs,
        module,
        context,
        caller,
        trait_scope,
        application.arguments.clone(),
        receiver,
        &[],
        active,
        b,
    )?;
    check_formals(proofs, module, context, &trait_env, trait_scope, active, b)?;
    requirements(
        proofs,
        module,
        context,
        &trait_env,
        trait_decl.id,
        trait_decl
            .requirements
            .as_ref()
            .ok_or_else(|| bad("original trait requirements are Unknown"))?,
        active,
        b,
    )?;
    if !context.domain_only.get() {
        for parent in trait_decl
            .direct_supertraits
            .as_ref()
            .ok_or_else(|| bad("original trait supers are Unknown"))?
        {
            let recipe = application_recipe(module, trait_decl.id, parent, b)?;
            let app = context.application(module, &recipe, &trait_env, proofs, active, b)?;
            match proofs.query(
                module,
                context,
                &trait_env,
                &ExternalRequest::Trait {
                    receiver: receiver.clone(),
                    application: app,
                },
                RequirementMode::ActualObligation,
                active,
                b,
            )? {
                ExternalValue::Proved => {}
                _ => return Err(bad("original super query did not establish satisfaction")),
            }
        }
    }
    Ok(Provider {
        trait_decl,
        implementation,
        trait_env,
        impl_env,
    })
}

struct MethodSelection<'a> {
    signature: &'a DynMethodRecipe,
    body: &'a DynDefinitionDeclaration,
    expected: &'a SemanticDefinition,
    actual: &'a SemanticDefinition,
    trait_default: bool,
}

fn method_selection<'a>(
    module: &'a TypedModule,
    trait_decl: &'a DynDeclaration,
    implementation: &'a DynDeclaration,
    ordinal: u32,
    b: &mut Budget,
) -> Check<MethodSelection<'a>> {
    let member = trait_decl
        .members
        .get(ordinal as usize)
        .ok_or_else(|| bad("original method selection member is absent"))?;
    if !matches!(member.kind, DynMemberKind::Method(_)) {
        return Err(bad("original method selection selects another member kind"));
    }
    let signatures = trait_decl
        .method_signatures
        .as_ref()
        .ok_or_else(|| bad("original trait method signatures are Unknown"))?;
    let definitions = implementation
        .method_definitions
        .as_ref()
        .ok_or_else(|| bad("original impl method definitions are Unknown"))?;
    let defaults = trait_decl
        .method_definitions
        .as_ref()
        .ok_or_else(|| bad("original default method definitions are Unknown"))?;
    b.spend(
        signatures
            .len()
            .saturating_add(definitions.len())
            .saturating_add(defaults.len())
            .saturating_add(implementation.members.len()),
    )?;
    let mut rows = signatures.iter().filter(|r| r.member == ordinal);
    let signature = rows
        .next()
        .ok_or_else(|| bad("exact original trait method signature is absent"))?;
    if rows.next().is_some() {
        return Err(bad("exact original trait method signature is duplicated"));
    }
    let mut members = implementation
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.name == member.name);
    let selected = members.next();
    if members.next().is_some() {
        return Err(bad("original impl has duplicate same-name method members"));
    }
    let (body, trait_default) = if let Some((index, actual)) = selected {
        if actual.kind != member.kind {
            return Err(bad(
                "original impl method changes its full mixed formal kinds",
            ));
        }
        let mut rows = definitions.iter().filter(|r| r.member == index as u32);
        let row = rows
            .next()
            .ok_or_else(|| bad("exact original impl method body is absent"))?;
        if rows.next().is_some() || row.scope != DynDefinitionScope::ImplOverride {
            return Err(bad(
                "original impl method body is duplicated or has another scope",
            ));
        }
        (row, false)
    } else {
        let id = signature
            .default_definition
            .ok_or_else(|| bad("original trait method has no actual override or default body"))?;
        let mut rows = defaults
            .iter()
            .filter(|r| r.id == id && r.member == ordinal);
        let row = rows
            .next()
            .ok_or_else(|| bad("exact original trait default body is absent"))?;
        if rows.next().is_some() || row.scope != DynDefinitionScope::TraitDefault {
            return Err(bad(
                "original trait default body is duplicated or has another scope",
            ));
        }
        (row, true)
    };
    let program = module
        .semantic
        .as_ref()
        .ok_or_else(|| bad("original method proof has no semantic program"))?;
    let find = |owner: SemanticDynOwner, b: &mut Budget| -> Check<&SemanticDefinition> {
        b.spend(program.dyn_scopes.len())?;
        let mut rows = program.dyn_scopes.iter().filter(|r| r.owner == owner);
        let row = rows
            .next()
            .ok_or_else(|| bad("exact original method scope binding is absent"))?;
        if rows.next().is_some() {
            return Err(bad("exact original method scope binding is duplicated"));
        }
        bridge::owner(module, &owner, row.scope, b)
    };
    let expected = find(
        SemanticDynOwner::MethodSignature {
            declaration: trait_decl.id,
            member: ordinal,
        },
        b,
    )?;
    let actual = find(SemanticDynOwner::MethodDefinition(body.id), b)?;
    Ok(MethodSelection {
        signature,
        body,
        expected,
        actual,
        trait_default,
    })
}

#[allow(clippy::too_many_arguments)]
fn methods(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    trait_decl: &DynDeclaration,
    implementation: &DynDeclaration,
    trait_env: &EvalEnv,
    impl_env: &EvalEnv,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<()> {
    if context.domain_only.get() {
        return Ok(());
    }
    for (ordinal, member) in trait_decl.members.iter().enumerate() {
        let DynMemberKind::Method(formals) = &member.kind else {
            continue;
        };
        let selection = method_selection(module, trait_decl, implementation, ordinal as u32, b)?;
        let MethodSelection {
            expected,
            actual,
            trait_default,
            ..
        } = selection;
        let parent = if trait_default { trait_env } else { impl_env };
        pending_method(proofs, trait_decl, implementation, &selection, b)?;
        if !formals.is_empty() {
            // Its original universal relation remains a mandatory final
            // obligation. A concrete member call supplies the complete mixed
            // method actuals to select_member below; no N0 is fabricated here.
            continue;
        }
        fn signature_of(definition: &SemanticDefinition) -> Check<&SemanticSignatureRecipe> {
            match &definition.body {
                SemanticDefinitionBody::FunctionWithBody { signature, .. }
                | SemanticDefinitionBody::Function { signature, .. } => Ok(signature),
                _ => Err(bad("original method scope has no full signature")),
            }
        }
        let expected_signature = signature_of(expected)?;
        let actual_signature = signature_of(actual)?;
        let receiver = trait_env
            .lexical
            .first()
            .and_then(|r| r.self_type.as_ref())
            .ok_or_else(|| bad("original method has no actual receiver"))?;
        let expected_env = environment(
            proofs,
            module,
            context,
            trait_env,
            expected,
            vec![],
            receiver,
            &[],
            active,
            b,
        )?;
        let actual_env = environment(
            proofs,
            module,
            context,
            parent,
            actual,
            vec![],
            receiver,
            &[],
            active,
            b,
        )?;
        let expected_value = context.evaluate_signature(
            module,
            expected_signature,
            &expected_env,
            proofs,
            active,
            b,
        )?;
        let actual_value =
            context.evaluate_signature(module, actual_signature, &actual_env, proofs, active, b)?;
        signature_equal(
            context,
            trait_env,
            &expected_value,
            &actual_value,
            expected_env.lexical[0].key,
            actual_env.lexical[0].key,
            b,
        )?;
        // This request supplies no runtime entry, memory contract or vtable
        // authority. The original full body and every actual function/entry
        // remain independent obligations at their existing final gates.
    }
    Ok(())
}

fn pending_method(
    proofs: &mut Proofs,
    trait_decl: &DynDeclaration,
    implementation: &DynDeclaration,
    selection: &MethodSelection<'_>,
    b: &mut Budget,
) -> Check<()> {
    b.spend(
        proofs
            .pending_original_methods
            .len()
            .saturating_add(proofs.pending_original_method_relations.len())
            .saturating_add(2),
    )?;
    if !proofs
        .pending_original_methods
        .contains(&selection.actual.id)
    {
        if proofs.pending_original_methods.len() >= b.limits.max_nodes {
            return Err(limit("selected original method body domain exceeds bound"));
        }
        proofs.pending_original_methods.push(selection.actual.id);
    }
    let relation = OriginalMethodRelation {
        trait_declaration: trait_decl.id,
        implementation_declaration: implementation.id,
        trait_signature: selection.expected.id,
        body: selection.actual.id,
        source: selection.body.scope,
    };
    if !proofs.pending_original_method_relations.contains(&relation) {
        if proofs.pending_original_method_relations.len() >= b.limits.max_nodes {
            return Err(limit("original method relation domain exceeds bound"));
        }
        proofs.pending_original_method_relations.push(relation);
    }
    Ok(())
}

fn signature_of(definition: &SemanticDefinition) -> Check<&SemanticSignatureRecipe> {
    match &definition.body {
        SemanticDefinitionBody::FunctionWithBody { signature, .. }
        | SemanticDefinitionBody::Function { signature, .. } => Ok(signature),
        _ => Err(bad("original method scope has no complete signature")),
    }
}

#[allow(clippy::too_many_arguments)]
fn raw_method_signature(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    owner: &SemanticDynOwner,
    raw: &DynSignatureRecipe,
    definition: &SemanticDefinition,
    env: &EvalEnv,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<CheckedSignature> {
    let declared = signature_of(definition)?;
    dynamic_metadata::recipe(raw, b)?;
    if declared.bound_lifetimes != 0 {
        return Err(bad(
            "original method raw signature omits its higher-ranked header domain",
        ));
    }
    let mut parameters = vec![];
    for p in &raw.parameters {
        parameters.push(SemanticParameterRecipe {
            ty: scoped_type_recipe(module, owner, &p.ty, b, 0)?,
            mode: p.mode,
        });
    }
    let converted = SemanticSignatureRecipe {
        bound_lifetimes: 0,
        invocation_lifetimes: raw.invocation_binder_count,
        parameters,
        result: scoped_type_recipe(module, owner, &raw.result, b, 0)?,
        result_mode: raw.result_mode,
        call_kind: raw.call_kind,
    };
    let proposed = context.evaluate_signature(module, &converted, env, proofs, active, b)?;
    let original = context.evaluate_signature(module, declared, env, proofs, active, b)?;
    let key = env
        .lexical
        .first()
        .ok_or_else(|| bad("raw method signature lacks its exact lexical frame"))?
        .key;
    signature_equal(context, env, &original, &proposed, key, key, b)?;
    Ok(original)
}

fn outside_environment(
    proofs: &mut Proofs,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    b: &mut Budget,
) -> Check<EvalEnv> {
    b.spend(
        env.lexical
            .len()
            .saturating_add(env.images.len())
            .saturating_add(env.assumptions.len())
            .saturating_add(env.invocations.len())
            .saturating_add(env.bounds.len()),
    )?;
    for frame in &env.lexical {
        for actual in frame.actuals.iter().chain(&frame.environment) {
            charge_actual(actual, b)?;
        }
        if let Some(value) = &frame.self_type {
            charge_type(value, b)?;
        }
    }
    let mut result = env.clone();
    for frame in &mut result.lexical {
        for actual in frame.actuals.iter_mut().chain(&mut frame.environment) {
            *actual = map_actual(proofs, context, env, actual, false, b)?;
        }
        if let Some(value) = &mut frame.self_type {
            *value = map_type(proofs, context, env, value, false, b)?;
        }
    }
    for (_, image) in &mut result.images {
        *image = outside_life(proofs, image, b)?;
    }
    Ok(result)
}

fn outside_signature(
    proofs: &mut Proofs,
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    mut signature: CheckedSignature,
    b: &mut Budget,
) -> Check<CheckedSignature> {
    for (parameter, _) in &mut signature.parameters {
        *parameter = map_type(proofs, context, env, parameter, false, b)?;
    }
    signature.result = map_type(proofs, context, env, &signature.result, false, b)?;
    Ok(signature)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn select_member(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    caller: &EvalEnv,
    receiver: &CheckedType,
    application: &CheckedApplication,
    member: u32,
    method_actuals: &[CheckedActual],
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<OriginalMemberReceipt> {
    let root = proofs.original_domain.is_none();
    if root {
        let definition = scope(module, application.declaration, b)?.id;
        proofs.original_domain = Some(LocalDomain::new(ScopeKey::Witness {
            definition,
            frame: active.fresh(b)?,
        }));
    }
    let result = (|| {
        let receiver = map_type(proofs, context, caller, receiver, true, b)?;
        b.spend(
            application
                .arguments
                .len()
                .saturating_add(method_actuals.len()),
        )?;
        let mut arguments = vec![];
        for a in &application.arguments {
            arguments.push(map_actual(proofs, context, caller, a, true, b)?);
        }
        let application = CheckedApplication {
            declaration: application.declaration,
            arguments,
        };
        let mut method_arguments = vec![];
        for a in method_actuals {
            method_arguments.push(map_actual(proofs, context, caller, a, true, b)?);
        }
        let Provider {
            trait_decl,
            implementation,
            trait_env,
            impl_env,
        } = provider(
            proofs,
            module,
            context,
            caller,
            &receiver,
            &application,
            active,
            b,
        )?;
        let selection = method_selection(module, trait_decl, implementation, member, b)?;
        let expected = selection.expected;
        let actual = selection.actual;
        if method_arguments.len() != expected.parameters.len()
            || method_arguments.len() != actual.parameters.len()
        {
            return Err(bad(
                "selected original member omits its complete mixed method actuals",
            ));
        }
        let parent = if selection.trait_default {
            &trait_env
        } else {
            &impl_env
        };
        let mut expected_env = environment(
            proofs,
            module,
            context,
            &trait_env,
            expected,
            method_arguments.clone(),
            &receiver,
            &[],
            active,
            b,
        )?;
        let mut actual_env = environment(
            proofs,
            module,
            context,
            parent,
            actual,
            method_arguments,
            &receiver,
            &[],
            active,
            b,
        )?;
        let expected_owner = SemanticDynOwner::MethodSignature {
            declaration: trait_decl.id,
            member,
        };
        let actual_owner = SemanticDynOwner::MethodDefinition(selection.body.id);
        let expected_signature = raw_method_signature(
            proofs,
            module,
            context,
            &expected_owner,
            &selection.signature.signature,
            expected,
            &expected_env,
            active,
            b,
        )?;
        let actual_signature = raw_method_signature(
            proofs,
            module,
            context,
            &actual_owner,
            &selection.body.signature,
            actual,
            &actual_env,
            active,
            b,
        )?;
        signature_equal(
            context,
            &trait_env,
            &expected_signature,
            &actual_signature,
            expected_env.lexical[0].key,
            actual_env.lexical[0].key,
            b,
        )?;
        expected_env.invocations = expected_signature.invocation.clone();
        actual_env.invocations = actual_signature.invocation.clone();
        check_formals(proofs, module, context, &expected_env, expected, active, b)?;
        check_formals(proofs, module, context, &actual_env, actual, active, b)?;
        let expected_requirements = scoped_requirement_recipes(
            module,
            &expected_owner,
            &selection.signature.requirements,
            b,
        )?;
        let actual_requirements =
            scoped_requirement_recipes(module, &actual_owner, &selection.body.requirements, b)?;
        semantic_requirements(
            proofs,
            module,
            context,
            &expected_env,
            &expected_requirements,
            active,
            b,
            0,
        )?;
        semantic_requirements(
            proofs,
            module,
            context,
            &actual_env,
            &actual_requirements,
            active,
            b,
            0,
        )?;
        pending_method(proofs, trait_decl, implementation, &selection, b)?;
        // Every existing pending body, Callable and Static request remains in
        // this same Proofs session. Raw contracts travel with the exact body so
        // the actual callsite can construct its memory/entry obligations.
        let expected_signature = outside_signature(proofs, context, caller, expected_signature, b)?;
        let actual_signature = outside_signature(proofs, context, caller, actual_signature, b)?;
        let environment = outside_environment(proofs, context, &actual_env, b)?;
        let parent = outside_environment(proofs, context, parent, b)?;
        let mut arguments = vec![];
        for a in &application.arguments {
            arguments.push(map_actual(proofs, context, caller, a, false, b)?);
        }
        let application = CheckedApplication {
            declaration: application.declaration,
            arguments,
        };
        let declaration_actuals = parent
            .lexical
            .first()
            .ok_or_else(|| bad("selected method has no original provider parent"))?
            .actuals
            .clone();
        let method_actuals = environment
            .lexical
            .first()
            .ok_or_else(|| bad("selected method has no exact original frame"))?
            .actuals
            .clone();
        dynamic_metadata::recipe(&selection.signature.signature.contract, b)?;
        dynamic_metadata::recipe(&selection.body.signature.contract, b)?;
        for row in selection
            .signature
            .requirements
            .iter()
            .chain(&selection.body.requirements)
        {
            dynamic_metadata::recipe(row, b)?;
        }
        Ok(OriginalMemberReceipt {
            receiver: map_type(proofs, context, caller, &receiver, false, b)?,
            application,
            member,
            method_actuals,
            implementation: implementation.id,
            declaration_actuals,
            definition: selection.body.id,
            body: actual.id,
            body_owner: if selection.trait_default {
                trait_decl.id
            } else {
                implementation.id
            },
            body_member: selection.body.member,
            trait_signature: expected.id,
            source: selection.body.scope,
            environment,
            expected_signature,
            actual_signature,
            trait_contract: selection.signature.signature.contract.clone(),
            body_contract: selection.body.signature.contract.clone(),
            trait_requirements: selection.signature.requirements.clone(),
            body_requirements: selection.body.requirements.clone(),
        })
    })();
    if root {
        proofs.original_domain = None;
    }
    result
}

fn signature_equal(
    context: &semantic::CheckedContext,
    env: &EvalEnv,
    a: &CheckedSignature,
    z: &CheckedSignature,
    a_scope: ScopeKey,
    z_scope: ScopeKey,
    b: &mut Budget,
) -> Check<()> {
    if a.bound.len() != z.bound.len()
        || a.invocation.len() != z.invocation.len()
        || a.parameters.len() != z.parameters.len()
        || a.result_mode != z.result_mode
        || a.call_kind != z.call_kind
    {
        return Err(bad(
            "original method changes the complete published signature domain",
        ));
    }
    let mut pairs = a
        .bound
        .iter()
        .zip(&z.bound)
        .chain(a.invocation.iter().zip(&z.invocation))
        .map(|(a, z)| (a.clone(), z.clone()))
        .collect::<Vec<_>>();
    fn fresh(key: &ClassKey, scope: ScopeKey) -> bool {
        matches!(key,ClassKey::Implicit{scope:s,..}|ClassKey::Formal{scope:s,..} if *s==scope)
    }
    fn pair(
        context: &semantic::CheckedContext,
        env: &EvalEnv,
        a: &ClassKey,
        z: &ClassKey,
        a_scope: ScopeKey,
        z_scope: ScopeKey,
        pairs: &mut Vec<(ClassKey, ClassKey)>,
        b: &mut Budget,
    ) -> Check<bool> {
        b.spend(pairs.len())?;
        if let Some((_, known)) = pairs.iter().find(|(known, _)| known == a) {
            return Ok(known == z);
        }
        if context.same(env, a, z, b)? {
            return Ok(true);
        }
        if !fresh(a, a_scope) || !fresh(z, z_scope) || pairs.iter().any(|(_, known)| known == z) {
            return Ok(false);
        }
        pairs.push((a.clone(), z.clone()));
        Ok(true)
    }
    for ((a, a_mode), (z, z_mode)) in a
        .parameters
        .iter()
        .map(|(ty, mode)| (ty, *mode))
        .chain(std::iter::once((&a.result, a.result_mode)))
        .zip(
            z.parameters
                .iter()
                .map(|(ty, mode)| (ty, *mode))
                .chain(std::iter::once((&z.result, z.result_mode))),
        )
    {
        if a_mode != z_mode
            || a.ty != z.ty
            || a.life != z.life
            || a.statics != z.statics
            || a.classes.len() != z.classes.len()
            || a.origins.len() != a.classes.len()
            || z.origins.len() != z.classes.len()
        {
            return Err(bad(
                "original method changes a complete parameter/result type or mode",
            ));
        }
        for (a, z) in a.classes.iter().zip(&z.classes) {
            if !pair(context, env, a, z, a_scope, z_scope, &mut pairs, b)? {
                return Err(bad(
                    "original method changes the joint signature lifetime partition",
                ));
            }
        }
        for (a, z) in a.origins.iter().zip(&z.origins) {
            if context.same_origin(env, a, z, b)? {
                continue;
            }
            let (
                IntrinsicOrigin::DerivedClass { source: a },
                IntrinsicOrigin::DerivedClass { source: z },
            ) = (a, z)
            else {
                return Err(bad(
                    "original method changes an existing intrinsic signature origin",
                ));
            };
            if !pair(context, env, a, z, a_scope, z_scope, &mut pairs, b)? {
                return Err(bad(
                    "original method changes an intrinsic signature partition",
                ));
            }
        }
    }
    Ok(())
}

fn application_recipe(
    module: &TypedModule,
    owner: LifeDeclarationId,
    p: &DynSelfPattern,
    b: &mut Budget,
) -> Check<SemanticTraitRecipe> {
    application(module, &SemanticDynOwner::Declaration(owner), p, b, 0)
}

#[allow(clippy::too_many_arguments)]
fn associated(
    proofs: &mut Proofs,
    module: &TypedModule,
    context: &semantic::CheckedContext,
    trait_decl: &DynDeclaration,
    implementation: &DynDeclaration,
    trait_env: &EvalEnv,
    impl_env: &EvalEnv,
    member: u32,
    constant: bool,
    active: &mut semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<ExternalValue> {
    b.spend(
        trait_decl
            .members
            .len()
            .saturating_add(implementation.members.len()),
    )?;
    let original = trait_decl
        .members
        .get(member as usize)
        .ok_or_else(|| bad("associated query original trait member is absent"))?;
    if !matches!(
        (&original.kind, constant),
        (DynMemberKind::Type, false) | (DynMemberKind::Constant, true)
    ) {
        return Err(bad("associated query changes original member kind"));
    }
    let mut overrides = implementation
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.name == original.name);
    let override_member = overrides.next();
    if overrides.next().is_some() {
        return Err(bad("associated query has duplicate impl members"));
    }
    let original_rows = trait_decl
        .associated_definitions
        .as_ref()
        .ok_or_else(|| bad("original trait associated definitions are Unknown"))?;
    b.spend(original_rows.len())?;
    let mut rows = original_rows.iter().filter(|r| r.member == member);
    let declaration = rows
        .next()
        .ok_or_else(|| bad("original trait associated declaration is absent"))?;
    if rows.next().is_some() {
        return Err(bad("original trait associated declaration is duplicated"));
    }
    let (owner, row, env) = if let Some((ordinal, m)) = override_member {
        if m.kind != original.kind {
            return Err(bad(
                "impl associated member changes the exact trait member kind",
            ));
        }
        let rows = implementation
            .associated_definitions
            .as_ref()
            .ok_or_else(|| bad("original impl associated definitions are Unknown"))?;
        b.spend(rows.len())?;
        let mut rows = rows.iter().filter(|r| r.member == ordinal as u32);
        let row = rows
            .next()
            .ok_or_else(|| bad("exact impl associated override recipe is absent"))?;
        if rows.next().is_some() {
            return Err(bad("impl associated override recipe is duplicated"));
        }
        (implementation.id, row, impl_env)
    } else {
        (trait_decl.id, declaration, trait_env)
    };
    let recipe = row
        .value
        .as_ref()
        .ok_or_else(|| bad("selected original associated member has no RHS"))?;
    let value = match (recipe, constant) {
        (DynAssociatedValueRecipe::Type(pattern), false) => {
            ExternalValue::Type(context.evaluate_type(
                module,
                &type_recipe(module, owner, pattern, b, 0)?,
                env,
                proofs,
                active,
                b,
            )?)
        }
        (DynAssociatedValueRecipe::Constant { ty, value }, true) => {
            let carrier = context.evaluate_type(
                module,
                &type_recipe(module, owner, ty, b, 0)?,
                env,
                proofs,
                active,
                b,
            )?;
            for (owner, row, environment) in
                [(owner, row, env), (trait_decl.id, declaration, trait_env)]
            {
                let expected = row
                    .carrier
                    .as_ref()
                    .ok_or_else(|| bad("original associated constant has no complete carrier"))?;
                let expected = context.evaluate_type(
                    module,
                    &type_recipe(module, owner, expected, b, 0)?,
                    environment,
                    proofs,
                    active,
                    b,
                )?;
                if !context.same_type(environment, &expected, &carrier, b)? {
                    return Err(bad(
                        "associated constant changes its original complete carrier",
                    ));
                }
            }
            let value = context.evaluate_constant(
                module,
                &constant_recipe(module, owner, value, b, 0)?,
                &carrier,
                env,
                proofs,
                active,
                b,
            )?;
            ExternalValue::Const { carrier, value }
        }
        _ => return Err(bad("selected original associated RHS has a different kind")),
    };
    // The completely constructed RHS is available to its own original bounds.
    // This is a value equation, never provisional Trait satisfaction. Publish
    // only after construction, so an RHS that recursively defines itself fails.
    let frame = trait_env
        .lexical
        .first()
        .ok_or_else(|| bad("associated bound has no original trait scope"))?;
    let receiver = frame
        .self_type
        .clone()
        .ok_or_else(|| bad("associated bound has no actual receiver"))?;
    let application = CheckedApplication {
        declaration: trait_decl.id,
        arguments: frame.actuals.clone(),
    };
    let key = if constant {
        ExternalRequest::AssociatedConst {
            receiver,
            application,
            member,
        }
    } else {
        ExternalRequest::AssociatedType {
            receiver,
            application,
            member,
        }
    };
    b.spend(1)?;
    proofs.available_original.push((key, value.clone()));
    let checked = (|| {
        requirements(
            proofs,
            module,
            context,
            env,
            owner,
            &row.requirements,
            active,
            b,
        )?;
        if owner != trait_decl.id {
            requirements(
                proofs,
                module,
                context,
                trait_env,
                trait_decl.id,
                &declaration.requirements,
                active,
                b,
            )?;
        }
        methods(
            proofs,
            module,
            context,
            trait_decl,
            implementation,
            trait_env,
            impl_env,
            active,
            b,
        )?;
        Ok(())
    })();
    proofs.available_original.pop();
    checked?;
    Ok(value)
}
