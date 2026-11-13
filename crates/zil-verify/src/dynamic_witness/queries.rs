//! Finite independent obligation interpretation. This module checks source
//! equations and dependency edges; it never takes a candidate-plan fact as proof.
use super::*;

#[derive(Clone)]
struct Application {
    declaration: LifeDeclarationId,
    actuals: Vec<Actual>,
}
#[derive(Clone)]
enum Actual {
    Type(terms::Value),
    Life(terms::LifeTerm),
    Const(Constant),
}
#[derive(Clone)]
enum Associated {
    Type(terms::Value),
    Const(terms::Value, Constant),
}
#[derive(Clone)]
struct Environment<'a> {
    node: usize,
    receiver: terms::Value,
    args: matching::Arguments<'a>,
    scope: DynInstantiationScope,
    contextual: bool,
}
struct Graph<'a> {
    module: &'a TypedModule,
    table: &'a DynVtable,
    nodes: &'a [DynImplementationWitness],
    active: Vec<bool>,
    proven: Vec<bool>,
    eligible: Vec<bool>,
    queries: Vec<(usize, u32)>,
    values: Vec<((usize, u32), Associated)>,
    edges: Vec<(usize, usize)>,
    key_active: Vec<bool>,
    quantified: u32,
    callable_obligations: Vec<callables::Obligation>,
}
fn declaration<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a DynDeclaration> {
    b.spend(module.dyn_declarations.len())?;
    let mut rows = module.dyn_declarations.iter().filter(|d| d.id == id);
    let d = rows
        .next()
        .ok_or_else(|| bad("query original declaration is absent"))?;
    if rows.next().is_some() {
        return Err(bad("query original declaration identity is ambiguous"));
    }
    Ok(d)
}
fn empty(ty: &Type, b: &mut Budget) -> Check<terms::Value> {
    dynamic_metadata::ty(ty, b)?;
    Ok(terms::Value {
        raw: DynTypeValue {
            ty: ty.clone(),
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![],
        },
        terms: vec![],
    })
}
/// A mapping between actual intrinsic occurrences. Unlike a map on lexical
/// images, this can relate target origins 0/1 with image 0 to caller origins
/// 0/1 with different images, provided their complete Context composition agrees.
struct Relation {
    pairs: Vec<(terms::LifeTerm, terms::LifeTerm)>,
}
impl Relation {
    fn pair(&mut self, a: terms::LifeTerm, z: terms::LifeTerm, b: &mut Budget) -> Check<bool> {
        b.spend(self.pairs.len().saturating_add(1))?;
        if let Some((old, to)) = self.pairs.iter().find(|(from, _)| from.origin == a.origin) {
            if old.image != a.image {
                return Err(bad("one query origin has inconsistent local images"));
            }
            return Ok(to == &z);
        }
        // Complete query keys preserve the joint intrinsic equality partition
        // in both directions. This is not injectivity of lexical images or of
        // to_context: two distinct origins may still both have image Lex0.
        if self
            .pairs
            .iter()
            .any(|(from, to)| to.origin == z.origin && from.origin != a.origin)
        {
            return Ok(false);
        }
        self.pairs.push((a, z));
        Ok(true)
    }
    fn value(&mut self, a: &terms::Value, z: &terms::Value, b: &mut Budget) -> Check<bool> {
        dynamic_metadata::recipe(&a.raw, b)?;
        dynamic_metadata::recipe(&z.raw, b)?;
        let Some(map) = external::alignment(&a.raw, &z.raw, b)? else {
            return Ok(false);
        };
        if a.terms.len() != map.len() || z.terms.len() != map.len() {
            return Err(bad(
                "query origin coverage differs from its complete external domain",
            ));
        }
        for (i, j) in map.into_iter().enumerate() {
            if !self.pair(a.terms[i], z.terms[j as usize], b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn actual(&mut self, a: &Actual, z: &Actual, b: &mut Budget) -> Check<bool> {
        Ok(match (a, z) {
            (Actual::Type(a), Actual::Type(z)) => self.value(a, z, b)?,
            (Actual::Life(a), Actual::Life(z)) => self.pair(*a, *z, b)?,
            (Actual::Const(a), Actual::Const(z)) => a == z,
            _ => false,
        })
    }
    fn mapped(&self, value: &terms::Value, b: &mut Budget) -> Check<terms::Value> {
        dynamic_metadata::recipe(&value.raw, b)?;
        b.spend(value.terms.len())?;
        let mut result = value.clone();
        for (term, image) in result.terms.iter_mut().zip(&mut result.raw.classes) {
            b.spend(self.pairs.len())?;
            let (_, to) = self
                .pairs
                .iter()
                .find(|(from, _)| from.origin == term.origin)
                .ok_or_else(|| {
                    bad("associated RHS has an origin not anchored by its actual query key")
                })?;
            *term = *to;
            *image = to.image;
        }
        Ok(result)
    }
    fn context(
        &self,
        from: &[DynLifeEmbedding],
        to: &[DynLifeEmbedding],
        b: &mut Budget,
    ) -> Check<()> {
        for (a, z) in &self.pairs {
            b.spend(from.len().saturating_add(to.len()).saturating_add(1))?;
            let a = from
                .iter()
                .find(|r| r.from == a.image)
                .ok_or_else(|| bad("query source class has no Context image"))?
                .to;
            let z = to
                .iter()
                .find(|r| r.from == z.image)
                .ok_or_else(|| bad("query caller class has no Context image"))?
                .to;
            if a != z {
                return Err(bad(
                    "query intrinsic relation disagrees with the actual Context composition",
                ));
            }
        }
        Ok(())
    }
}
fn nominal_candidate(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    arguments: &[Actual],
    b: &mut Budget,
) -> Check<Option<terms::Value>> {
    dynamic_metadata::ty(&instance.ty, b)?;
    let life = match &instance.ty {
        Type::BorrowedStorage { life, .. } => life.clone(),
        ty => {
            values::closed(module, ty, b)?;
            empty(ty, b)?.raw.life
        }
    };
    if life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("fresh nominal intrinsic key domain exceeded"));
    }
    b.spend(life.binder_count as usize)?;
    let mut assigned = vec![None; life.binder_count as usize];
    fn bind(
        assigned: &mut [Option<terms::LifeTerm>],
        class: u32,
        term: terms::LifeTerm,
    ) -> Check<bool> {
        let slot = assigned
            .get_mut(class as usize)
            .ok_or_else(|| bad("nominal argument class is outside its full actual domain"))?;
        if slot.is_some_and(|previous| previous != term) {
            return Ok(false);
        }
        *slot = Some(term);
        Ok(true)
    }
    for (ordinal, (argument, recorded)) in arguments.iter().zip(&instance.arguments).enumerate() {
        b.spend(1)?;
        match (argument, recorded) {
            (Actual::Life(term), LifeNominalActual::Lifetime(class)) => {
                b.spend(life.nominal_arguments.len())?;
                let mut rows = life.nominal_arguments.iter().filter(|row| {
                    row.path.is_empty()
                        && row.declaration == instance.declaration
                        && row.argument as usize == ordinal
                });
                let row = rows
                    .next()
                    .ok_or_else(|| bad("fresh nominal original lifetime occurrence is absent"))?;
                if rows.next().is_some() || row.binder != *class {
                    return Err(bad(
                        "fresh nominal original lifetime occurrence disagrees with its instance",
                    ));
                }
                if !bind(&mut assigned, *class, *term)? {
                    return Ok(None);
                }
            }
            (Actual::Const(value), LifeNominalActual::Const(recorded)) if value == recorded => {}
            (Actual::Type(value), LifeNominalActual::Type(ty))
            | (Actual::Type(value), LifeNominalActual::QualifiedType { ty, .. }) => {
                let child_life = match recorded {
                    LifeNominalActual::QualifiedType { life, .. } => life.clone(),
                    _ => match ty {
                        Type::BorrowedStorage { life, .. } => life.clone(),
                        ty => {
                            values::closed(module, ty, b)?;
                            empty(ty, b)?.raw.life
                        }
                    },
                };
                let child = DynTypeValue {
                    ty: ty.clone(),
                    life: child_life,
                    classes: vec![],
                };
                let Some(map) = external::alignment(&value.raw, &child, b)? else {
                    return Ok(None);
                };
                if value.terms.len() != map.len() {
                    return Err(bad("nominal whole Type argument loses an intrinsic origin"));
                }
                for (source, child_class) in map.into_iter().enumerate() {
                    b.spend(life.retained.len().saturating_add(1))?;
                    let mut rows = life.retained.iter().filter(|row| {
                        row.path == [LifeTypeProjection::TypeArgument(ordinal as u32)]
                            && row.class == child_class
                    });
                    let row = rows.next().ok_or_else(|| {
                        bad("fresh nominal omits a full TypeArgument class occurrence")
                    })?;
                    if rows.next().is_some() {
                        return Err(bad("fresh nominal repeats a TypeArgument class occurrence"));
                    }
                    if !bind(&mut assigned, row.binder, value.terms[source])? {
                        return Ok(None);
                    }
                }
            }
            _ => return Ok(None),
        }
    }
    let Some(terms) = assigned.into_iter().collect::<Option<Vec<_>>>() else {
        return Err(bad("fresh nominal contains an ungrounded intrinsic class"));
    };
    let (_, distinct) = terms::partition(&terms, b)?;
    if distinct.len() != terms.len() {
        return Ok(None);
    }
    Ok(Some(terms::Value {
        raw: DynTypeValue {
            ty: instance.ty.clone(),
            life,
            classes: terms.iter().map(|term| term.image).collect(),
        },
        terms,
    }))
}

impl<'a> Graph<'a> {
    fn environment(
        &self,
        node: usize,
        trait_scope: bool,
        b: &mut Budget,
    ) -> Check<Environment<'a>> {
        let w = &self.nodes[node];
        let implementation = declaration(self.module, w.implementation, b)?;
        let owner = terms::Owner::Witness(self.table.id, node as u32);
        let mut args = matching::Arguments {
            semantic_owner: Some(SemanticDynOwner::Declaration(implementation.id)),
            owner,
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
            declaration: std::borrow::Cow::Borrowed(&w.actuals),
            method: std::borrow::Cow::Borrowed(&[]),
            invocation: &[],
            implicit: vec![
                None;
                implementation
                    .implicit_lifetimes
                    .as_ref()
                    .ok_or_else(|| bad("query implicit declaration directory is Unknown"))?
                    .len()
            ],
        };
        matching::ty(
            self.module,
            implementation
                .self_pattern
                .as_ref()
                .ok_or_else(|| bad("query Self recipe is Unknown"))?,
            &w.receiver,
            &w.receiver,
            &mut args,
            b,
        )?;
        matching::complete_anchors(
            self.module,
            implementation,
            &w.application,
            &w.receiver,
            &mut args,
            b,
        )?;
        if trait_scope {
            let trait_decl = declaration(self.module, w.application.declaration, b)?;
            let transferred = matching::transfer_application(
                self.module,
                implementation
                    .implemented_trait
                    .as_ref()
                    .ok_or_else(|| bad("query implemented trait is Unknown"))?,
                &w.application,
                &w.receiver,
                &mut args,
                b,
            )?;
            args = matching::Arguments {
                semantic_owner: Some(SemanticDynOwner::Declaration(trait_decl.id)),
                owner,
                terms: transferred,
                projections: vec![],
                constants: vec![],
                collecting_anchors: false,
                declaration: std::borrow::Cow::Borrowed(&w.application.actuals),
                method: std::borrow::Cow::Borrowed(&[]),
                invocation: &[],
                implicit: vec![
                    None;
                    trait_decl
                        .implicit_lifetimes
                        .as_ref()
                        .ok_or_else(|| bad("query trait implicit directory is Unknown"))?
                        .len()
                ],
            };
        }
        Ok(Environment {
            node,
            receiver: terms::Value::anchored(&w.receiver, owner, terms::Root::Receiver, b)?,
            args,
            scope: w.scope,
            contextual: false,
        })
    }
    fn method_environment(
        &self,
        node: usize,
        trait_scope: bool,
        method_actuals: &'a [DynGenericActual],
        invocation_count: u32,
        request_invocation_count: u32,
        b: &mut Budget,
    ) -> Check<Environment<'a>> {
        let mut env = self.environment(node, trait_scope, b)?;
        let rows = &self.nodes[node].to_context;
        let image = |class, b: &mut Budget| -> Check<StorageBinder> {
            b.spend(rows.len())?;
            let mut found = rows.iter().filter(|row| row.from == class);
            let result = found
                .next()
                .ok_or_else(|| bad("method lexical class lacks Context image"))?
                .to;
            if found.next().is_some() {
                return Err(bad("method lexical class repeats its Context image"));
            }
            Ok(result)
        };
        env.args.terms.map_images(image, b)?;
        for class in &mut env.receiver.raw.classes {
            *class = image(*class, b)?;
        }
        for term in &mut env.receiver.terms {
            term.image = image(term.image, b)?;
        }
        b.spend(env.args.declaration.len())?;
        env.args.declaration = std::borrow::Cow::Owned(
            env.args
                .declaration
                .iter()
                .map(|actual| graph::mapped_actual(actual, rows, b))
                .collect::<Check<Vec<_>>>()?,
        );
        env.args.method = std::borrow::Cow::Borrowed(method_actuals);
        let interface = dynamic_ports::interface(self.module, self.table.interface, b)?;
        env.scope = DynInstantiationScope {
            fixed_binder_count: interface.retained_life.binder_count,
            invocation_binder_count: request_invocation_count,
        };
        if invocation_count > request_invocation_count {
            return Err(bad(
                "original method call domain exceeds its actual slot scope",
            ));
        }
        scopes::actuals(self.module, method_actuals, env.scope, b)?;
        b.spend(invocation_count as usize)?;
        for index in 0..invocation_count {
            let binder = StorageBinder::Invocation(index);
            env.args.terms.bind(
                DynLifePattern::Invocation(index),
                terms::LifeTerm {
                    origin: terms::Origin {
                        owner: env.args.owner,
                        root: terms::Root::Lexical(binder),
                        class: 0,
                    },
                    image: binder,
                },
                b,
            )?;
        }
        for (index, actual) in method_actuals.iter().enumerate() {
            if let DynGenericActual::Life(image) = actual {
                env.args.terms.bind(
                    DynLifePattern::MethodParameter(index as u32),
                    terms::LifeTerm {
                        origin: terms::Origin {
                            owner: env.args.owner,
                            root: terms::Root::Lexical(*image),
                            class: 0,
                        },
                        image: *image,
                    },
                    b,
                )?;
            }
        }
        env.contextual = true;
        Ok(env)
    }
    fn signature(
        &mut self,
        env: &mut Environment<'a>,
        recipe: &DynSignatureRecipe,
        b: &mut Budget,
    ) -> Check<signatures::Resolved> {
        let module = self.module;
        signatures::materialize(
            module,
            recipe,
            env.scope,
            |pattern, b| self.value(env, pattern, b),
            b,
        )
    }
    fn value(
        &mut self,
        env: &mut Environment<'a>,
        pattern: &DynTypePattern,
        b: &mut Budget,
    ) -> Check<terms::Value> {
        self.prepare_type(env, pattern, b, 0)?;
        match pattern {
            DynTypePattern::Semantic { scope, .. } => {
                let expected = env.args.semantic_owner.as_ref().ok_or_else(|| {
                    bad("semantic expression has no independently selected original owner")
                })?;
                bridge::owner(self.module, expected, *scope, b)?;
                Err(bad(
                    "semantic type bridge requires its checked lexical owner interpreter",
                ))
            }
            DynTypePattern::SelfType => {
                dynamic_metadata::recipe(&env.receiver.raw, b)?;
                Ok(env.receiver.clone())
            }
            DynTypePattern::Parameter(i) | DynTypePattern::MethodParameter(i) => {
                instantiate::actual_type(
                    self.module,
                    matches!(pattern, DynTypePattern::MethodParameter(_)),
                    *i,
                    &mut env.args,
                    b,
                )
            }
            DynTypePattern::Concrete(ty) => {
                values::closed(self.module, ty, b)?;
                empty(ty, b)
            }
            DynTypePattern::Nominal(nominal) => self.nominal(env, nominal, b),
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            } => {
                let child = self.value(env, inner, b)?;
                let term = env.args.terms.life(
                    lifetime.ok_or_else(|| bad("fresh Address lifetime is Unknown"))?,
                    b,
                )?;
                let mode = if *mutable {
                    StoredBorrowMode::ExclusiveAddress
                } else {
                    StoredBorrowMode::SharedAddress
                };
                constructors::address(&child, term, mode, b)
            }
            DynTypePattern::Desc { lifetime, inner } => {
                let child = self.value(env, inner, b)?;
                let term = env.args.terms.life(
                    lifetime.ok_or_else(|| bad("fresh Description lifetime is Unknown"))?,
                    b,
                )?;
                constructors::address(&child, term, StoredBorrowMode::ReadOnlyDescription, b)
            }
            DynTypePattern::Managed(inner) => {
                let child = self.value(env, inner, b)?;
                constructors::managed(&child, b)
            }
            DynTypePattern::Tuple(fields) => {
                let mut children = Vec::with_capacity(fields.len());
                b.spend(fields.len())?;
                for field in fields {
                    children.push(self.value(env, field, b)?);
                }
                constructors::tuple(&children, b)
            }
            DynTypePattern::Array { element, length } => {
                let element = self.value(env, element, b)?;
                values::closed(self.module, &element.raw.ty, b)?;
                self.prepare_const(env, length, b, 0)?;
                let Constant::Usize(length) =
                    matching::constant(self.module, length, &Type::Usize, &env.args, b)?
                else {
                    return Err(bad("array recipe length is not its exact usize carrier"));
                };
                let length = u32::try_from(length)
                    .map_err(|_| limit("array recipe exceeds the actual Type length domain"))?;
                empty(
                    &Type::Array {
                        element: Box::new(element.raw.ty),
                        length,
                    },
                    b,
                )
            }
            DynTypePattern::Slice(element) => {
                let element = self.value(env, element, b)?;
                values::closed(self.module, &element.raw.ty, b)?;
                empty(
                    &Type::Slice {
                        element: Box::new(element.raw.ty),
                    },
                    b,
                )
            }
            DynTypePattern::OwnedStore(element) => {
                let element = self.value(env, element, b)?;
                structural::check(self.module, &element, DynStructuralBound::StoreElement, b)?;
                values::closed(self.module, &element.raw.ty, b)?;
                empty(
                    &Type::OwnedStore {
                        element: Box::new(element.raw.ty),
                    },
                    b,
                )
            }
            DynTypePattern::ErrorUnion { error, result } => {
                let error = self.value(env, error, b)?;
                let result = self.value(env, result, b)?;
                values::closed(self.module, &error.raw.ty, b)?;
                values::closed(self.module, &result.raw.ty, b)?;
                empty(
                    &Type::ErrorUnion {
                        error: Box::new(error.raw.ty),
                        result: Box::new(result.raw.ty),
                    },
                    b,
                )
            }
            DynTypePattern::AssociatedProjection { .. } => env
                .args
                .projections
                .iter()
                .find(|(p, _)| p == pattern)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| bad("associated type query did not produce a complete value")),
        }
    }
    /// Resolve an already determined declaration/ordered-actual/intrinsic key.
    /// Catalog order and outer image equality never choose the result shape.
    fn nominal(
        &mut self,
        env: &mut Environment<'a>,
        pattern: &DynSelfPattern,
        b: &mut Budget,
    ) -> Check<terms::Value> {
        b.spend(self.module.life_declarations.len())?;
        let mut declarations = self
            .module
            .life_declarations
            .iter()
            .filter(|d| d.id == pattern.declaration);
        let original = declarations
            .next()
            .ok_or_else(|| bad("fresh query nominal declaration is absent"))?;
        if declarations.next().is_some() || original.parameters.len() != pattern.arguments.len() {
            return Err(bad(
                "fresh query nominal declaration or ordered arity is ambiguous",
            ));
        }
        let mut actuals = vec![];
        for (parameter, argument) in original.parameters.iter().zip(&pattern.arguments) {
            b.spend(1)?;
            let actual = match (parameter, argument) {
                (LifeParameterKind::Lifetime, DynPatternActual::Life(p)) => {
                    let term = env.args.terms.life(*p, b)?;
                    Actual::Life(term)
                }
                (LifeParameterKind::Type, DynPatternActual::Type(p)) => {
                    let value = self.value(env, p, b)?;
                    Actual::Type(value)
                }
                (LifeParameterKind::Const(ty), DynPatternActual::Const(p)) => {
                    self.prepare_const(env, p, b, 0)?;
                    Actual::Const(matching::constant(self.module, p, ty, &env.args, b)?)
                }
                _ => {
                    return Err(bad(
                        "fresh query nominal changes an original mixed formal kind",
                    ));
                }
            };
            actuals.push(actual);
        }
        b.spend(self.module.life_instances.len())?;
        let mut selected = None;
        for instance in &self.module.life_instances {
            if instance.declaration != pattern.declaration
                || instance.arguments.len() != actuals.len()
            {
                continue;
            }
            let Some(value) = nominal_candidate(self.module, instance, &actuals, b)? else {
                continue;
            };
            if selected.replace(value).is_some() {
                return Err(bad("fresh query complete nominal key is ambiguous"));
            }
        }
        let value = selected.ok_or_else(|| bad("fresh query complete nominal key is absent"))?;
        values::check(self.module, &value.raw, env.scope, b)?;
        Ok(value)
    }

    fn application(
        &mut self,
        env: &mut Environment<'a>,
        pattern: &DynSelfPattern,
        b: &mut Budget,
    ) -> Check<Application> {
        b.spend(pattern.arguments.len())?;
        let mut actuals = vec![];
        let declaration = declaration(self.module, pattern.declaration, b)?;
        if declaration.parameters.len() != pattern.arguments.len() {
            return Err(bad("query application omits original mixed arguments"));
        }
        for (p, kind) in pattern.arguments.iter().zip(&declaration.parameters) {
            actuals.push(match (p, kind) {
                (DynPatternActual::Life(p), LifeParameterKind::Lifetime) => {
                    Actual::Life(env.args.terms.life(*p, b)?)
                }
                (DynPatternActual::Type(p), LifeParameterKind::Type) => {
                    Actual::Type(self.value(env, p, b)?)
                }
                (DynPatternActual::Const(p), LifeParameterKind::Const(ty)) => {
                    self.prepare_const(env, p, b, 0)?;
                    Actual::Const(matching::constant(self.module, p, ty, &env.args, b)?)
                }
                _ => return Err(bad("query application changes an original formal kind")),
            });
        }
        Ok(Application {
            declaration: pattern.declaration,
            actuals,
        })
    }
    fn prepare_type(
        &mut self,
        env: &mut Environment<'a>,
        p: &DynTypePattern,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("query type expression depth exceeded"));
        }
        match p {
            DynTypePattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                if env.args.projections.iter().any(|(old, _)| old == p) {
                    return Ok(());
                }
                let receiver = self.value(env, receiver, b)?;
                let application = self.application(env, application, b)?;
                let Associated::Type(value) = self.project(
                    env.node,
                    env.contextual,
                    &receiver,
                    &application,
                    *member,
                    b,
                )?
                else {
                    return Err(bad("type query selects a constant"));
                };
                dynamic_metadata::recipe(p, b)?;
                dynamic_metadata::recipe(&value.raw, b)?;
                env.args.projections.push((p.clone(), value));
            }
            DynTypePattern::Nominal(n) => {
                for a in &n.arguments {
                    match a {
                        DynPatternActual::Type(t) => self.prepare_type(env, t, b, depth + 1)?,
                        DynPatternActual::Const(c) => self.prepare_const(env, c, b, depth + 1)?,
                        DynPatternActual::Life(_) => {}
                    }
                }
            }
            DynTypePattern::Address { inner, .. }
            | DynTypePattern::Managed(inner)
            | DynTypePattern::Desc { inner, .. }
            | DynTypePattern::Slice(inner)
            | DynTypePattern::OwnedStore(inner) => self.prepare_type(env, inner, b, depth + 1)?,
            DynTypePattern::Tuple(fields) => {
                for t in fields {
                    self.prepare_type(env, t, b, depth + 1)?;
                }
            }
            DynTypePattern::Array { element, length } => {
                self.prepare_type(env, element, b, depth + 1)?;
                self.prepare_const(env, length, b, depth + 1)?;
            }
            DynTypePattern::ErrorUnion { error, result } => {
                self.prepare_type(env, error, b, depth + 1)?;
                self.prepare_type(env, result, b, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn prepare_const(
        &mut self,
        env: &mut Environment<'a>,
        p: &DynConstPattern,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("query constant expression depth exceeded"));
        }
        match p {
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                if env.args.constants.iter().any(|(old, _, _)| old == p) {
                    return Ok(());
                }
                let receiver = self.value(env, receiver, b)?;
                let application = self.application(env, application, b)?;
                let Associated::Const(ty, value) = self.project(
                    env.node,
                    env.contextual,
                    &receiver,
                    &application,
                    *member,
                    b,
                )?
                else {
                    return Err(bad("constant query selects a type"));
                };
                dynamic_metadata::recipe(p, b)?;
                dynamic_metadata::recipe(&ty.raw, b)?;
                env.args.constants.push((p.clone(), ty, value));
            }
            DynConstPattern::Unary { value, ty, .. } | DynConstPattern::Convert { value, ty } => {
                self.prepare_type(env, ty, b, depth + 1)?;
                self.prepare_const(env, value, b, depth + 1)?;
            }
            DynConstPattern::Binary {
                left, right, ty, ..
            } => {
                self.prepare_type(env, ty, b, depth + 1)?;
                self.prepare_const(env, left, b, depth + 1)?;
                self.prepare_const(env, right, b, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn node_key(&mut self, node: usize, b: &mut Budget) -> Check<(Environment<'a>, Application)> {
        b.spend(self.key_active.len().saturating_add(1))?;
        if self.key_active[node] {
            return Err(bad(
                "implementation key depends on its own unresolved associated query",
            ));
        }
        if self.key_active.iter().filter(|v| **v).count() >= b.limits.max_call_depth.min(128) {
            return Err(limit("implementation key dependency depth exceeded"));
        }
        self.key_active[node] = true;
        let mut env = self.environment(node, false, b)?;
        let implementation = declaration(self.module, self.nodes[node].implementation, b)?;
        let pattern = implementation
            .implemented_trait
            .as_ref()
            .ok_or_else(|| bad("witness implemented trait recipe is Unknown"))?;
        let app = self.application(&mut env, pattern, b)?;
        self.key_active[node] = false;
        Ok((env, app))
    }
    fn select(
        &mut self,
        caller: usize,
        contextual: bool,
        receiver: &terms::Value,
        app: &Application,
        b: &mut Budget,
    ) -> Check<(usize, Relation)> {
        let mut selected = None;
        for index in 0..self.nodes.len() {
            b.spend(1)?;
            let (env, candidate) = self.node_key(index, b)?;
            if candidate.declaration != app.declaration
                || candidate.actuals.len() != app.actuals.len()
            {
                continue;
            }
            let mut relation = Relation { pairs: vec![] };
            if !relation.value(&env.receiver, receiver, b)? {
                continue;
            }
            let mut equal = true;
            for (a, z) in candidate.actuals.iter().zip(&app.actuals) {
                if !relation.actual(a, z, b)? {
                    equal = false;
                    break;
                }
            }
            if !equal {
                continue;
            }
            if contextual {
                for (from, to) in &relation.pairs {
                    b.spend(self.nodes[index].to_context.len())?;
                    let mut rows = self.nodes[index]
                        .to_context
                        .iter()
                        .filter(|r| r.from == from.image);
                    let row = rows
                        .next()
                        .ok_or_else(|| bad("method query source lacks Context image"))?;
                    if rows.next().is_some() || row.to != to.image {
                        return Err(bad("method query changes its complete Context composition"));
                    }
                }
            } else {
                relation.context(
                    &self.nodes[index].to_context,
                    &self.nodes[caller].to_context,
                    b,
                )?;
            }
            if selected.replace((index, relation)).is_some() {
                return Err(bad(
                    "complete query has conflicting or duplicate implementation witnesses",
                ));
            }
        }
        let result =
            selected.ok_or_else(|| bad("complete actual query has no implementation witness"))?;
        b.spend(self.edges.len().saturating_add(1))?;
        if !self.edges.contains(&(caller, result.0)) {
            self.edges.push((caller, result.0));
        }
        Ok(result)
    }
    fn project(
        &mut self,
        caller: usize,
        contextual: bool,
        receiver: &terms::Value,
        app: &Application,
        member: u32,
        b: &mut Budget,
    ) -> Check<Associated> {
        let (node, relation) = self.select(caller, contextual, receiver, app, b)?;
        if !self.eligible[node] {
            self.prove(node, b)?;
        }
        let value = self.rhs(node, member, b)?;
        Ok(match value {
            Associated::Type(v) => Associated::Type(relation.mapped(&v, b)?),
            Associated::Const(ty, value) => Associated::Const(relation.mapped(&ty, b)?, value),
        })
    }
    fn rhs(&mut self, node: usize, member: u32, b: &mut Budget) -> Check<Associated> {
        b.spend(
            self.values
                .len()
                .saturating_add(self.queries.len())
                .saturating_add(1),
        )?;
        if let Some((_, value)) = self.values.iter().find(|(key, _)| *key == (node, member)) {
            return Ok(value.clone());
        }
        if self.queries.contains(&(node, member)) {
            return Err(bad("active complete associated query forms a cycle"));
        }
        if self.queries.len() > b.limits.max_call_depth.min(128) {
            return Err(limit("associated query proof depth exceeded"));
        }
        self.queries.push((node, member));
        let w = &self.nodes[node];
        let resolution = w
            .associated
            .iter()
            .find(|r| r.member.ordinal == member)
            .ok_or_else(|| bad("query original associated result is absent"))?;
        let (owner, own, trait_scope) = match resolution.source {
            DynAssociatedSource::TraitDefault {
                declaration,
                member,
            } => (declaration, member, true),
            DynAssociatedSource::ImplOverride {
                declaration,
                member,
            } => (declaration, member, false),
        };
        let source = declaration(self.module, owner, b)?;
        let definition = source
            .associated_definitions
            .as_ref()
            .ok_or_else(|| bad("query original associated definitions are Unknown"))?
            .iter()
            .find(|d| d.member == own)
            .ok_or_else(|| bad("query original associated source is absent"))?;
        let recipe = definition
            .value
            .as_ref()
            .ok_or_else(|| bad("query associated source is abstract or Unknown"))?;
        let mut env = self.environment(node, trait_scope, b)?;
        let value = match (recipe, &resolution.value) {
            (DynAssociatedValueRecipe::Type(pattern), DynAssociatedValue::Type(actual)) => {
                let value = self.value(&mut env, pattern, b)?;
                Associated::Type(external::in_shape(&value, actual, b)?.ok_or_else(|| {
                    bad("associated RHS differs from its complete constructed type")
                })?)
            }
            (
                DynAssociatedValueRecipe::Constant { ty, value },
                DynAssociatedValue::Constant {
                    ty: actual,
                    value: claimed,
                },
            ) => {
                self.prepare_type(&mut env, ty, b, 0)?;
                let expected = self.value(&mut env, ty, b)?;
                let carrier = external::in_shape(&expected, actual, b)?
                    .ok_or_else(|| bad("associated constant has a different complete carrier"))?;
                let original_carrier = definition
                    .carrier
                    .as_ref()
                    .ok_or_else(|| bad("original impl/default constant carrier is Unknown"))?;
                self.prepare_type(&mut env, original_carrier, b, 0)?;
                instantiate::against(
                    self.module,
                    original_carrier,
                    actual,
                    &w.receiver,
                    &mut env.args,
                    b,
                )?;
                let trait_decl = declaration(self.module, w.application.declaration, b)?;
                let original = trait_decl
                    .associated_definitions
                    .as_ref()
                    .ok_or_else(|| bad("original trait constant declarations are Unknown"))?
                    .iter()
                    .find(|d| d.member == member)
                    .ok_or_else(|| bad("original trait constant declaration is absent"))?;
                let original = original
                    .carrier
                    .as_ref()
                    .ok_or_else(|| bad("original trait constant carrier is Unknown"))?;
                let mut trait_env = self.environment(node, true, b)?;
                self.prepare_type(&mut trait_env, original, b, 0)?;
                instantiate::against(
                    self.module,
                    original,
                    actual,
                    &w.receiver,
                    &mut trait_env.args,
                    b,
                )?;
                self.prepare_const(&mut env, value, b, 0)?;
                let value = matching::constant(self.module, value, &carrier.raw.ty, &env.args, b)?;
                if value != *claimed {
                    return Err(bad(
                        "resolved constant differs from independent original expression evaluation",
                    ));
                }
                Associated::Const(carrier, value)
            }
            _ => return Err(bad("query original associated source kind differs")),
        };
        self.queries.pop();
        self.values.push(((node, member), value.clone()));
        self.requirements(&mut env, &definition.requirements, b)?;
        Ok(value)
    }
    fn requirements(
        &mut self,
        env: &mut Environment<'a>,
        requirements: &[DynObligation],
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(requirements.len())?;
        for requirement in requirements {
            match requirement {
                DynObligation::Trait {
                    receiver,
                    application,
                } => {
                    let receiver = self.value(env, receiver, b)?;
                    let app = self.application(env, application, b)?;
                    let (node, _) = self.select(env.node, env.contextual, &receiver, &app, b)?;
                    self.prove(node, b)?;
                }
                DynObligation::TypeEqual { left, right } => {
                    let left = self.value(env, left, b)?;
                    let right = self.value(env, right, b)?;
                    dynamic_metadata::recipe(&left.raw, b)?;
                    dynamic_metadata::recipe(&right.raw, b)?;
                    if !external::terms_equal(&left, &right, b)? {
                        return Err(bad(
                            "original where TypeEqual premise is false in the complete intrinsic domain",
                        ));
                    }
                }
                DynObligation::ConstEqual { ty, left, right } => {
                    let ty = self.value(env, ty, b)?;
                    self.prepare_const(env, left, b, 0)?;
                    self.prepare_const(env, right, b, 0)?;
                    let left = matching::constant(self.module, left, &ty.raw.ty, &env.args, b)?;
                    let right = matching::constant(self.module, right, &ty.raw.ty, &env.args, b)?;
                    if left != right {
                        return Err(bad(
                            "original where ConstEqual premise has different carrier bits",
                        ));
                    }
                }
                DynObligation::Structural { receiver, bound } => {
                    let value = self.value(env, receiver, b)?;
                    structural::check(self.module, &value, *bound, b)?;
                }
                DynObligation::Callable {
                    receiver,
                    kind,
                    quantified_lifetimes,
                    signature,
                } => {
                    let receiver = self.value(env, receiver, b)?;
                    values::check(self.module, &receiver.raw, env.scope, b)?;
                    dynamic_metadata::recipe(&env.receiver.raw, b)?;
                    b.spend(env.receiver.terms.len())?;
                    let mut nested = Environment {
                        node: env.node,
                        receiver: env.receiver.clone(),
                        args: env.args.charged_clone(b)?,
                        scope: env.scope,
                        contextual: env.contextual,
                    };
                    dynamic_metadata::recipe(&receiver.raw, b)?;
                    let count = quantified_lifetimes
                        .checked_add(signature.invocation_binder_count)
                        .ok_or_else(|| limit("callable fresh class domain overflow"))?;
                    let start = env.scope.invocation_binder_count;
                    nested.scope.invocation_binder_count = start
                        .checked_add(count)
                        .ok_or_else(|| limit("callable actual invocation scope overflow"))?;
                    scopes::bounded(nested.scope, b)?;
                    nested
                        .args
                        .terms
                        .enter_signature(*quantified_lifetimes != 0, b)?;
                    nested.args.projections.clear();
                    nested.args.constants.clear();
                    let token = self.quantified;
                    self.quantified = token
                        .checked_add(1)
                        .ok_or_else(|| limit("callable quantifier identity overflow"))?;
                    for index in 0..count {
                        let pattern = if index < *quantified_lifetimes {
                            DynLifePattern::Bound { depth: 0, index }
                        } else {
                            DynLifePattern::Invocation(index - *quantified_lifetimes)
                        };
                        nested.args.terms.bind(
                            pattern,
                            terms::LifeTerm {
                                origin: terms::Origin {
                                    owner: nested.args.owner,
                                    root: terms::Root::Quantified(token, index),
                                    class: 0,
                                },
                                image: StorageBinder::Invocation(start + index),
                            },
                            b,
                        )?;
                    }
                    let resolved = self.signature(&mut nested, signature, b)?;
                    let pending = callables::check(
                        self.module,
                        &receiver,
                        *kind,
                        &resolved,
                        nested.scope,
                        !env.contextual,
                        b,
                    )?;
                    b.spend(1)?;
                    self.callable_obligations.push(pending);
                }
            }
        }
        Ok(())
    }
    fn methods(&mut self, b: &mut Budget) -> Check<Vec<signatures::MethodObligation>> {
        let interface = dynamic_ports::interface(self.module, self.table.interface, b)?;
        let mut pending = vec![];
        for entry in &self.table.entries {
            b.spend(
                interface
                    .slots
                    .len()
                    .saturating_add(self.nodes.len())
                    .saturating_add(1),
            )?;
            let slot = interface
                .slots
                .get(entry.slot as usize)
                .ok_or_else(|| bad("method entry selects an absent original interface slot"))?;
            let actual = self
                .module
                .functions
                .get(entry.function.0 as usize)
                .filter(|function| function.id == entry.function)
                .ok_or_else(|| bad("selected method exact actual function is absent"))?;
            let actual_instance = actual
                .dyn_instance
                .as_ref()
                .ok_or_else(|| bad("selected method original instance is Unknown"))?;
            let mut candidates = vec![];
            for (index, witness) in self.nodes.iter().enumerate() {
                if external::alignment(&witness.receiver, &actual_instance.receiver, b)?.is_none() {
                    continue;
                }
                let application =
                    graph::application_image(&witness.application, &witness.to_context, b)?;
                if external::applications_equal(&application, &slot.member.application, b)? {
                    candidates.push(index);
                }
            }
            if candidates.len() != 1 {
                return Err(bad(
                    "method slot does not select one complete witness application",
                ));
            }
            let node = candidates[0];
            let witness = &self.nodes[node];
            let trait_decl = declaration(self.module, slot.member.application.declaration, b)?;
            let member = trait_decl
                .members
                .get(slot.member.ordinal as usize)
                .ok_or_else(|| bad("method slot original member is absent"))?;
            let DynMemberKind::Method(formals) = &member.kind else {
                return Err(bad("method slot selects an associated value"));
            };
            graph::kinds(
                self.module,
                formals,
                &slot.actuals,
                DynInstantiationScope {
                    fixed_binder_count: interface.retained_life.binder_count,
                    invocation_binder_count: slot
                        .storage_life
                        .as_ref()
                        .ok_or_else(|| bad("complete dynamic method header relation is Unknown"))?
                        .invocation_binder_count,
                },
                b,
            )?;
            let original = trait_decl
                .method_signatures
                .as_ref()
                .ok_or_else(|| bad("original trait method signatures are Unknown"))?
                .iter()
                .find(|method| method.member == slot.member.ordinal)
                .ok_or_else(|| bad("original trait method signature is absent"))?;
            let implementation = declaration(self.module, witness.implementation, b)?;
            let mut own = implementation
                .members
                .iter()
                .enumerate()
                .filter(|(_, own)| {
                    own.name == member.name && matches!(own.kind, DynMemberKind::Method(_))
                });
            let selected = own.next();
            if own.next().is_some() {
                return Err(bad("implementation method override identity is ambiguous"));
            }
            let (owner, definition, trait_scope) = if let Some((index, _)) = selected {
                let definition = implementation
                    .method_definitions
                    .as_ref()
                    .ok_or_else(|| bad("original implementation method definitions are Unknown"))?
                    .iter()
                    .find(|method| {
                        method.member as usize == index
                            && method.scope == DynDefinitionScope::ImplOverride
                    })
                    .ok_or_else(|| bad("selected implementation override definition is absent"))?;
                (implementation, definition, false)
            } else {
                let id = original
                    .default_definition
                    .ok_or_else(|| bad("abstract method has no implementation override"))?;
                let definition = trait_decl
                    .method_definitions
                    .as_ref()
                    .ok_or_else(|| bad("original trait default definitions are Unknown"))?
                    .iter()
                    .find(|method| {
                        method.id == id
                            && method.member == slot.member.ordinal
                            && method.scope == DynDefinitionScope::TraitDefault
                    })
                    .ok_or_else(|| bad("selected original trait default definition is absent"))?;
                (trait_decl, definition, true)
            };
            let actual = self
                .module
                .functions
                .get(entry.function.0 as usize)
                .filter(|function| function.id == entry.function)
                .ok_or_else(|| bad("selected method exact actual function is absent"))?;
            if !definition
                .instances
                .iter()
                .any(|instance| instance.function == DynFunctionTarget::Typed(actual.id))
            {
                return Err(bad(
                    "table method target is not an instance of the selected original definition",
                ));
            }
            if definition.signature.invocation_binder_count
                != original.signature.invocation_binder_count
            {
                return Err(bad(
                    "method override changes the original invocation domain",
                ));
            }
            let mut trait_env = self.method_environment(
                node,
                true,
                &slot.actuals,
                original.signature.invocation_binder_count,
                slot.storage_life
                    .as_ref()
                    .ok_or_else(|| bad("method slot call domain is Unknown"))?
                    .invocation_binder_count,
                b,
            )?;
            trait_env.args.semantic_owner = Some(SemanticDynOwner::MethodSignature {
                declaration: trait_decl.id,
                member: original.member,
            });
            self.requirements(&mut trait_env, &original.requirements, b)?;
            let declared = self.signature(&mut trait_env, &original.signature, b)?;
            let mut body_env = self.method_environment(
                node,
                trait_scope,
                &slot.actuals,
                definition.signature.invocation_binder_count,
                slot.storage_life
                    .as_ref()
                    .ok_or_else(|| bad("method slot call domain is Unknown"))?
                    .invocation_binder_count,
                b,
            )?;
            body_env.args.semantic_owner = Some(SemanticDynOwner::MethodDefinition(definition.id));
            self.requirements(&mut body_env, &definition.requirements, b)?;
            let body = self.signature(&mut body_env, &definition.signature, b)?;
            declared.same_signature(&body, b)?;
            let (header, embedding) = body.function(self.module, actual, b)?;
            let images = signatures::specialization(
                self.module,
                actual,
                definition.id,
                &body_env.receiver,
                &body_env.args.declaration,
                &slot.actuals,
                &embedding,
                b,
            )?;
            // The two contracts are independent executable obligations. A body
            // recipe cannot replace the trait upper merely because ABI agrees.
            let origin = requests::Origin::TableSlot {
                table: self.table.id,
                slot: entry.slot,
                member: slot.member.clone(),
                definition: definition.id,
            };
            dynamic_metadata::recipe(&body_env.receiver.raw, b)?;
            b.spend(body_env.receiver.terms.len())?;
            pending.push(signatures::MethodObligation {
                receiver: body_env.receiver.clone(),
                scope: body_env.scope,
                origin: origin.clone(),
                function: actual.id,
                signature: declared,
                expected_header: header.clone(),
                header_embedding: embedding.clone(),
                lexical_images: images.clone(),
            });
            pending.push(signatures::MethodObligation {
                receiver: body_env.receiver,
                scope: body_env.scope,
                origin,
                function: actual.id,
                signature: body,
                expected_header: header,
                header_embedding: embedding,
                lexical_images: images,
            });
            let _ = owner;
        }
        Ok(pending)
    }
    fn prove(&mut self, node: usize, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if self.proven[node] {
            return Ok(());
        }
        if self.active[node] {
            return Err(bad(
                "implementation obligation graph forms an unproved cycle",
            ));
        }
        b.spend(self.active.len())?;
        if self.active.iter().filter(|v| **v).count() >= b.limits.max_call_depth.min(128) {
            return Err(limit("implementation obligation dependency depth exceeded"));
        }
        self.active[node] = true;
        let w = &self.nodes[node];
        let implementation = declaration(self.module, w.implementation, b)?;
        let trait_decl = declaration(self.module, w.application.declaration, b)?;
        let mut implementation_env = self.environment(node, false, b)?;
        self.requirements(
            &mut implementation_env,
            implementation
                .requirements
                .as_ref()
                .ok_or_else(|| bad("original impl requirements are Unknown"))?,
            b,
        )?;
        self.eligible[node] = true;
        let mut trait_env = self.environment(node, true, b)?;
        self.requirements(
            &mut trait_env,
            trait_decl
                .requirements
                .as_ref()
                .ok_or_else(|| bad("original trait requirements are Unknown"))?,
            b,
        )?;
        for supertrait in trait_decl
            .direct_supertraits
            .as_ref()
            .ok_or_else(|| bad("original supertrait graph is Unknown"))?
        {
            let app = self.application(&mut trait_env, supertrait, b)?;
            let (dependency, _) = self.select(node, false, &trait_env.receiver, &app, b)?;
            self.prove(dependency, b)?;
        }
        for resolution in &w.associated {
            self.rhs(node, resolution.member.ordinal, b)?;
            let declared = trait_decl
                .associated_definitions
                .as_ref()
                .ok_or_else(|| bad("original associated bounds are Unknown"))?
                .iter()
                .find(|d| d.member == resolution.member.ordinal)
                .ok_or_else(|| bad("original associated bound member is absent"))?;
            self.requirements(&mut trait_env, &declared.requirements, b)?;
        }
        self.active[node] = false;
        self.proven[node] = true;
        Ok(())
    }
}
/// Additional prerequisite only: method requirements/header/CFG and whole-table
/// reachability still have to be joined before a new Some claim is usable.
pub(super) fn check(
    module: &TypedModule,
    table: &DynVtable,
    root: usize,
    b: &mut Budget,
) -> Check<requests::Pending> {
    let Some(nodes) = table.implementation_witnesses.as_ref() else {
        return Ok(requests::Pending::default());
    };
    if nodes.len() > b.limits.max_nodes {
        return Err(limit("implementation witness graph exceeds selected bound"));
    }
    b.spend(nodes.len().saturating_mul(3))?;
    let mut graph = Graph {
        module,
        table,
        nodes,
        active: vec![false; nodes.len()],
        proven: vec![false; nodes.len()],
        eligible: vec![false; nodes.len()],
        queries: vec![],
        values: vec![],
        edges: vec![],
        key_active: vec![false; nodes.len()],
        quantified: 0,
        callable_obligations: vec![],
    };
    for node in 0..nodes.len() {
        graph.prove(node, b)?;
    }
    let pending_methods = graph.methods(b)?;
    // All method dependencies are now joined before exact graph reachability.
    // The obligations still require the independent actual CFG consumer.
    {
        let mut reached = vec![false; nodes.len()];
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            b.spend(graph.edges.len().saturating_add(1))?;
            let seen = reached
                .get_mut(node)
                .ok_or_else(|| bad("witness root selects an absent proof node"))?;
            if std::mem::replace(seen, true) {
                continue;
            }
            pending.extend(
                graph
                    .edges
                    .iter()
                    .filter(|(from, _)| *from == node)
                    .map(|(_, to)| *to),
            );
        }
        if reached.contains(&false) {
            return Err(bad(
                "implementation proof graph contains an unreachable extra witness",
            ));
        }
    }
    let mut pending = requests::Pending::default();
    b.spend(
        pending_methods
            .len()
            .saturating_add(graph.callable_obligations.len()),
    )?;
    for method in pending_methods {
        pending.methods.push(method.request(module, b)?);
    }
    for callable in graph.callable_obligations {
        pending.callables.push(callable.request(module, b)?);
    }
    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn term(function: u32, class: u32, image: u32) -> terms::LifeTerm {
        terms::LifeTerm {
            origin: terms::Origin {
                owner: terms::Owner::Function(FunctionId(function)),
                root: terms::Root::Receiver,
                class,
            },
            image: StorageBinder::Fixed(image),
        }
    }
    fn method_fixture(default: bool) -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        let module_id = module.module_id;
        let trait_id = LifeDeclarationId {
            module: module_id,
            declaration: zil_base::NodeId(910),
            compiler: false,
        };
        let impl_id = LifeDeclarationId {
            declaration: zil_base::NodeId(911),
            ..trait_id
        };
        let definition_id = DynDefinitionId {
            module: module_id,
            source: zil_base::NodeId(912),
        };
        let identity = |name: &str| LifeDeclarationIdentity {
            package: "method".into(),
            module_path: vec![],
            declaration_path: vec![name.into()],
            generation_path: vec![],
        };
        let zero = StorageLifeShape {
            binder_count: 0,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        };
        let counter = Type::Record {
            name: "method.Counter".into(),
            fields: vec![("value".into(), Type::I32)],
        };
        let receiver = DynTypeValue {
            ty: counter.clone(),
            life: zero.clone(),
            classes: vec![],
        };
        let head = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        };
        let access = DynCallAccess {
            reads: vec![head.clone()],
            writes: vec![],
        };
        let empty_access = DynCallAccess {
            reads: vec![],
            writes: vec![],
        };
        let recipe = DynSignatureRecipe {
            parameters: vec![DynParameterRecipe {
                ty: DynTypePattern::Address {
                    mutable: false,
                    lifetime: Some(DynLifePattern::Invocation(0)),
                    inner: Box::new(DynTypePattern::SelfType),
                },
                mode: PassingMode::Infer,
            }],
            result: DynTypePattern::Concrete(Type::I32),
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
            invocation_binder_count: 1,
            contract: DynContractRecipe {
                requires: Some(vec![]),
                guarantees: Some(vec![]),
                hold: Some(vec![]),
                call: Some(access.clone()),
                result: Some(DynResultSourceRecipe::Value),
                cleanup: Some(DynCleanupRecipe {
                    environment: false,
                    positions: vec![],
                    access: empty_access.clone(),
                    effects: vec![],
                }),
                effects: Some(vec![]),
                resume: Some(DynResumeBound::Local),
                threads: Some(DynThreadRequirements {
                    transfer: false,
                    share: false,
                }),
            },
        };
        let definition = DynDefinitionDeclaration {
            requirements: vec![],
            instances: vec![DynDefinitionSpecialization {
                function: DynFunctionTarget::Typed(FunctionId(0)),
                receiver: receiver.clone(),
                declaration_actuals: vec![],
                method_actuals: vec![],
            }],
            id: definition_id,
            identity: identity("read.body"),
            member: 0,
            scope: if default {
                DynDefinitionScope::TraitDefault
            } else {
                DynDefinitionScope::ImplOverride
            },
            signature: recipe.clone(),
        };
        let member = DynMemberDeclaration {
            name: "read".into(),
            kind: DynMemberKind::Method(vec![]),
        };
        module.dyn_declarations = vec![
            DynDeclaration {
                implicit_lifetimes: Some(vec![]),
                direct_supertraits: Some(vec![]),
                requirements: Some(vec![]),
                associated_definitions: Some(vec![]),
                method_signatures: Some(vec![DynMethodRecipe {
                    requirements: vec![DynObligation::TypeEqual {
                        left: DynTypePattern::Concrete(Type::I32),
                        right: DynTypePattern::Concrete(Type::I32),
                    }],
                    member: 0,
                    signature: recipe,
                    default_definition: default.then_some(definition_id),
                }]),
                method_definitions: Some(if default {
                    vec![definition.clone()]
                } else {
                    vec![]
                }),
                self_pattern: None,
                implemented_trait: None,
                id: trait_id,
                identity: identity("Read"),
                kind: DynDeclarationKind::Trait,
                parameters: vec![],
                members: vec![member.clone()],
            },
            DynDeclaration {
                implicit_lifetimes: Some(vec![]),
                direct_supertraits: Some(vec![]),
                requirements: Some(vec![]),
                associated_definitions: Some(vec![]),
                method_signatures: Some(vec![]),
                method_definitions: Some(if default { vec![] } else { vec![definition] }),
                self_pattern: Some(DynTypePattern::Concrete(counter.clone())),
                implemented_trait: Some(DynSelfPattern {
                    declaration: trait_id,
                    arguments: vec![],
                }),
                id: impl_id,
                identity: identity("Counter.Read"),
                kind: DynDeclarationKind::Impl,
                parameters: vec![],
                members: if default { vec![] } else { vec![member] },
            },
        ];
        let relation = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            bindings: vec![StorageLifeBinding {
                position: head,
                binder: StorageBinder::Invocation(0),
            }],
            static_bindings: vec![],
            classes: vec![],
        };
        let function = &mut module.functions[0];
        let span = function.span;
        let address = Type::Address {
            mutable: false,
            inner: Box::new(counter.clone()),
        };
        function.parameters = vec![LocalId(0)];
        function.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "self".into(),
            ty: address.clone(),
            mutable: false,
            span,
        }];
        function.values = vec![
            address,
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            Type::I32,
        ];
        function.blocks[0].instructions = vec![
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Load(LocalId(0)),
                span,
            },
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::AddressField {
                    address: ValueId(0),
                    index: 0,
                },
                span,
            },
            Instruction {
                result: Some(ValueId(2)),
                kind: InstructionKind::ReadAddress {
                    address: ValueId(1),
                },
                span,
            },
        ];
        function.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        function.storage_life = Some(relation.clone());
        function.dyn_instance = Some(DynFunctionInstance {
            scope: DynInstantiationScope {
                fixed_binder_count: 0,
                invocation_binder_count: 1,
            },
            header_embedding: vec![DynLifeEmbedding {
                from: StorageBinder::Invocation(0),
                to: StorageBinder::Invocation(0),
            }],
            definition: definition_id,
            receiver: receiver.clone(),
            declaration_actuals: vec![],
            method_actuals: vec![],
        });
        let application = DynTraitApplication {
            declaration: trait_id,
            actuals: vec![],
        };
        let interface_id = DynInterfaceId {
            module: module_id,
            index: 0,
        };
        module.dyn_interfaces = vec![DynInterface {
            context: None,
            owning_cleanup: None,
            id: interface_id,
            application: application.clone(),
            inherited: vec![],
            associated_types: vec![],
            associated_constants: vec![],
            retained_life: RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            slots: vec![DynSlot {
                member: DynMember {
                    application: application.clone(),
                    ordinal: 0,
                },
                actuals: vec![],
                receiver: DynReceiver {
                    mutable: false,
                    mode: PassingMode::Infer,
                },
                parameters: vec![],
                parameter_modes: vec![],
                result: Type::I32,
                result_mode: PassingMode::Infer,
                call_kind: CallKind::Direct,
                storage_life: Some(relation),
                contract: DynContract {
                    requires: Some(vec![]),
                    guarantees: Some(vec![]),
                    hold: Some(vec![]),
                    call: Some(access),
                    result: Some(DynResultSource::Value),
                    cleanup: Some(DynCleanupContract {
                        environment: false,
                        positions: vec![],
                        access: empty_access,
                        effects: vec![],
                    }),
                    effects: Some(vec![]),
                    resume: Some(DynResumeBound::Local),
                    threads: Some(DynThreadRequirements {
                        transfer: false,
                        share: false,
                    }),
                },
            }],
        }];
        module.dyn_vtables = vec![DynVtable {
            self_life: Some(zero),
            implementation_witnesses: Some(vec![DynImplementationWitness {
                scope: DynInstantiationScope {
                    fixed_binder_count: 0,
                    invocation_binder_count: 0,
                },
                to_context: vec![],
                receiver,
                application,
                implementation: impl_id,
                actuals: vec![],
                associated: vec![],
            }]),
            self_classes: vec![],
            id: DynVtableId {
                module: module_id,
                index: 0,
            },
            interface: interface_id,
            self_type: counter,
            implementation: impl_id,
            actuals: vec![],
            ports: vec![],
            entries: vec![DynEntry {
                slot: 0,
                function: FunctionId(0),
            }],
            cleanup: None,
            owned_storage: None,
        }];
        module
    }
    fn method_check(module: &TypedModule) -> Check<()> {
        let limits = Limits::default();
        let raw = super::super::declaration_graph(module, &mut Budget::new(&limits));
        let plan = CandidatePlan::empty_for(module);
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = super::super::declaration_graph(&decoded, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref().map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map_err(|e| (&e.code, &e.message))
        );
        raw.map(|_| ())
    }
    #[test]
    fn original_where_constructs_closed_array_slice_and_error_types_before_comparison() {
        let mut module = method_fixture(false);
        let requirements = &mut module.dyn_declarations[0]
            .method_signatures
            .as_mut()
            .unwrap()[0]
            .requirements;
        for (pattern, ty) in [
            (
                DynTypePattern::Array {
                    element: Box::new(DynTypePattern::Concrete(Type::U8)),
                    length: DynConstPattern::IntegerLiteral("2".into()),
                },
                Type::Array {
                    element: Box::new(Type::U8),
                    length: 2,
                },
            ),
            (
                DynTypePattern::Slice(Box::new(DynTypePattern::Concrete(Type::U16))),
                Type::Slice {
                    element: Box::new(Type::U16),
                },
            ),
            (
                DynTypePattern::ErrorUnion {
                    error: Box::new(DynTypePattern::Concrete(Type::U8)),
                    result: Box::new(DynTypePattern::Concrete(Type::I64)),
                },
                Type::ErrorUnion {
                    error: Box::new(Type::U8),
                    result: Box::new(Type::I64),
                },
            ),
        ] {
            requirements.push(DynObligation::TypeEqual {
                left: pattern,
                right: DynTypePattern::Concrete(ty),
            });
        }
        method_check(&module).unwrap();
        let DynObligation::TypeEqual {
            left: DynTypePattern::Array { length, .. },
            ..
        } = &mut module.dyn_declarations[0]
            .method_signatures
            .as_mut()
            .unwrap()[0]
            .requirements[1]
        else {
            unreachable!()
        };
        *length = DynConstPattern::IntegerLiteral("3".into());
        assert!(method_check(&module).is_err());
    }
    #[test]
    fn original_method_override_and_default_join_exact_entry_header_and_where_graph() {
        for default in [false, true] {
            let module = method_fixture(default);
            method_check(&module).unwrap();
            let limits = Limits::default();
            let pending =
                super::super::declaration_graph(&module, &mut Budget::new(&limits)).unwrap();
            assert_eq!(
                pending.methods.len(),
                2,
                "trait and body contract are distinct CFG obligations"
            );
            assert!(pending.callables.is_empty());
            for request in &pending.methods {
                assert_eq!(request.function, FunctionId(0));
                assert_eq!(request.specialization, module.functions[0].dyn_instance);
                assert_eq!(request.actual_header, module.functions[0].storage_life);
                assert_eq!(request.expected_header.bindings.len(), 1);
            }
            let mut wrong_where = module.clone();
            let DynObligation::TypeEqual { right, .. } = &mut wrong_where.dyn_declarations[0]
                .method_signatures
                .as_mut()
                .unwrap()[0]
                .requirements[0]
            else {
                unreachable!()
            };
            *right = DynTypePattern::Concrete(Type::U8);
            assert!(method_check(&wrong_where).is_err());
            let mut wrong_target = module.clone();
            let mut other = wrong_target.functions[0].clone();
            other.id = FunctionId(1);
            other.dyn_instance = None;
            wrong_target.functions.push(other);
            wrong_target.dyn_vtables[0].entries[0].function = FunctionId(1);
            assert!(method_check(&wrong_target).is_err());
            let mut unknown_contract = module.clone();
            let owner = if default { 0 } else { 1 };
            unknown_contract.dyn_declarations[owner]
                .method_definitions
                .as_mut()
                .unwrap()[0]
                .signature
                .contract
                .effects = None;
            assert!(method_check(&unknown_contract).is_err());
        }
    }
    #[test]
    fn fresh_nominal_uses_original_occurrences_not_actual_iteration_order() {
        let (module, _) = crate::tests::baseline();
        let id = LifeDeclarationId {
            module: module.module_id,
            declaration: zil_base::NodeId(900),
            compiler: false,
        };
        let child_id = LifeDeclarationId {
            declaration: zil_base::NodeId(901),
            ..id
        };
        let child_life = StorageLifeShape {
            binder_count: 1,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration: child_id,
                argument: 0,
                binder: 0,
            }],
        };
        let child_ty = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Tag".into(),
                fields: vec![("code".into(), Type::I32)],
            }),
            life: child_life.clone(),
        };
        let child = terms::Value {
            raw: DynTypeValue {
                ty: child_ty.clone(),
                life: child_life.clone(),
                classes: vec![StorageBinder::Fixed(0)],
            },
            terms: vec![term(0, 0, 0)],
        };
        let candidate = |merged| {
            let child_class = if merged { 0 } else { 1 };
            let life = StorageLifeShape {
                binder_count: if merged { 1 } else { 2 },
                slots: vec![],
                retained: vec![StorageRetainedBinding {
                    path: vec![LifeTypeProjection::TypeArgument(0)],
                    class: 0,
                    binder: child_class,
                }],
                nominal_arguments: vec![
                    NominalLifeArgument {
                        path: vec![],
                        declaration: id,
                        argument: 1,
                        binder: 0,
                    },
                    NominalLifeArgument {
                        path: vec![LifeTypeProjection::TypeArgument(0)],
                        declaration: child_id,
                        argument: 0,
                        binder: child_class,
                    },
                ],
            };
            LifeNominalInstance {
                declaration: id,
                ty: Type::BorrowedStorage {
                    storage: Box::new(Type::Record {
                        name: "Mixed".into(),
                        fields: vec![("tag".into(), child_ty.clone())],
                    }),
                    life,
                },
                arguments: vec![
                    LifeNominalActual::QualifiedType {
                        ty: child_ty.clone(),
                        life: child_life.clone(),
                    },
                    LifeNominalActual::Lifetime(0),
                ],
            }
        };
        let limits = Limits::default();
        let arguments = vec![Actual::Type(child.clone()), Actual::Life(term(0, 1, 0))];
        let value = nominal_candidate(
            &module,
            &candidate(false),
            &arguments,
            &mut Budget::new(&limits),
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.terms, vec![term(0, 1, 0), term(0, 0, 0)]);
        assert_eq!(value.raw.classes, vec![StorageBinder::Fixed(0); 2]);
        assert!(
            nominal_candidate(
                &module,
                &candidate(true),
                &arguments,
                &mut Budget::new(&limits)
            )
            .unwrap()
            .is_none()
        );
        let repeated = vec![Actual::Type(child), Actual::Life(term(0, 0, 0))];
        assert!(
            nominal_candidate(
                &module,
                &candidate(false),
                &repeated,
                &mut Budget::new(&limits)
            )
            .unwrap()
            .is_none()
        );
        assert!(
            nominal_candidate(
                &module,
                &candidate(true),
                &repeated,
                &mut Budget::new(&limits)
            )
            .unwrap()
            .is_some()
        );
    }
    #[test]
    fn exact_query_partition_is_bidirectional_but_images_remain_many_to_one() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut relation = Relation { pairs: vec![] };
        assert!(
            relation
                .pair(term(0, 0, 0), term(1, 0, 0), &mut budget)
                .unwrap()
        );
        assert!(
            relation
                .pair(term(0, 1, 0), term(1, 1, 1), &mut budget)
                .unwrap()
        );
        relation
            .context(
                &[DynLifeEmbedding {
                    from: StorageBinder::Fixed(0),
                    to: StorageBinder::Fixed(0),
                }],
                &[
                    DynLifeEmbedding {
                        from: StorageBinder::Fixed(0),
                        to: StorageBinder::Fixed(0),
                    },
                    DynLifeEmbedding {
                        from: StorageBinder::Fixed(1),
                        to: StorageBinder::Fixed(0),
                    },
                ],
                &mut budget,
            )
            .unwrap();
        let mut compressed = Relation { pairs: vec![] };
        assert!(
            compressed
                .pair(term(0, 0, 0), term(1, 0, 0), &mut budget)
                .unwrap()
        );
        assert!(
            !compressed
                .pair(term(0, 1, 0), term(1, 0, 0), &mut budget)
                .unwrap()
        );
        assert!(
            !compressed
                .pair(term(0, 0, 0), term(1, 1, 0), &mut budget)
                .unwrap()
        );
    }
}
