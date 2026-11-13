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
struct Environment<'a> {
    node: usize,
    receiver: terms::Value,
    args: matching::Arguments<'a>,
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
        self.pairs.push((a, z));
        Ok(true)
    }
    fn value(&mut self, a: &terms::Value, z: &terms::Value, b: &mut Budget) -> Check<bool> {
        dynamic_metadata::recipe(&a.raw, b)?;
        dynamic_metadata::recipe(&z.raw, b)?;
        if a.raw.ty != z.raw.ty || a.raw.life != z.raw.life || a.terms.len() != z.terms.len() {
            return Ok(false);
        }
        for (a, z) in a.terms.iter().zip(&z.terms) {
            if !self.pair(*a, *z, b)? {
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
            owner,
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            declaration: &w.actuals,
            method: &[],
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
                owner,
                terms: transferred,
                projections: vec![],
                constants: vec![],
                declaration: &w.application.actuals,
                method: &[],
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
        })
    }
    fn value(
        &mut self,
        env: &mut Environment<'a>,
        pattern: &DynTypePattern,
        b: &mut Budget,
    ) -> Check<terms::Value> {
        self.prepare_type(env, pattern, b, 0)?;
        match pattern {
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
            DynTypePattern::AssociatedProjection { .. } => env
                .args
                .projections
                .iter()
                .find(|(p, _)| p == pattern)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| bad("associated type query did not produce a complete value")),
            _ => Err(bad(
                "query constructor needs its complete independently derived intrinsic key",
            )),
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
        let mut origins = vec![];
        for (parameter, argument) in original.parameters.iter().zip(&pattern.arguments) {
            b.spend(1)?;
            let actual = match (parameter, argument) {
                (LifeParameterKind::Lifetime, DynPatternActual::Life(p)) => {
                    let term = env.args.terms.life(*p, b)?;
                    origins.push(term);
                    Actual::Life(term)
                }
                (LifeParameterKind::Type, DynPatternActual::Type(p)) => {
                    let value = self.value(env, p, b)?;
                    b.spend(value.terms.len())?;
                    origins.extend_from_slice(&value.terms);
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
        let (_, unique) = terms::partition(&origins, b)?;
        b.spend(self.module.life_instances.len())?;
        let mut selected = None;
        for instance in &self.module.life_instances {
            if instance.declaration != pattern.declaration
                || instance.arguments.len() != actuals.len()
            {
                continue;
            }
            dynamic_metadata::ty(&instance.ty, b)?;
            let mut equal = true;
            for (actual, recorded) in actuals.iter().zip(&instance.arguments) {
                b.spend(unique.len().saturating_add(1))?;
                equal &= match (actual, recorded) {
                    (Actual::Life(term), LifeNominalActual::Lifetime(class)) => {
                        unique
                            .iter()
                            .position(|v| v.origin == term.origin)
                            .map(|v| v as u32)
                            == Some(*class)
                    }
                    (Actual::Const(a), LifeNominalActual::Const(z)) => a == z,
                    (Actual::Type(a), LifeNominalActual::Type(z)) => {
                        a.raw.ty == *z
                            && (a.raw.life.binder_count == 0
                                || matches!(z, Type::BorrowedStorage { life, .. } if *life == a.raw.life))
                    }
                    (Actual::Type(a), LifeNominalActual::QualifiedType { ty, life }) => {
                        a.raw.ty == *ty && a.raw.life == *life
                    }
                    _ => false,
                };
                if !equal {
                    break;
                }
            }
            if !equal {
                continue;
            }
            if selected.replace(instance).is_some() {
                return Err(bad("fresh query complete nominal key is ambiguous"));
            }
        }
        let instance = selected.ok_or_else(|| bad("fresh query complete nominal key is absent"))?;
        let life = match &instance.ty {
            Type::BorrowedStorage { life, .. } => life.clone(),
            ty if unique.is_empty() => {
                values::closed(self.module, ty, b)?;
                empty(ty, b)?.raw.life
            }
            _ => return Err(bad("fresh query nominal has no complete persistent domain")),
        };
        let raw = DynTypeValue {
            ty: instance.ty.clone(),
            life,
            classes: unique.iter().map(|v| v.image).collect(),
        };
        values::check(self.module, &raw, self.nodes[env.node].scope, b)?;
        instantiate::against(
            self.module,
            &DynTypePattern::Nominal(Box::new(pattern.clone())),
            &raw,
            &env.receiver.raw,
            &mut env.args,
            b,
        )
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
                let Associated::Type(value) =
                    self.project(env.node, &receiver, &application, *member, b)?
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
                let Associated::Const(ty, value) =
                    self.project(env.node, &receiver, &application, *member, b)?
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
            relation.context(
                &self.nodes[index].to_context,
                &self.nodes[caller].to_context,
                b,
            )?;
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
        receiver: &terms::Value,
        app: &Application,
        member: u32,
        b: &mut Budget,
    ) -> Check<Associated> {
        let (node, relation) = self.select(caller, receiver, app, b)?;
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
                self.prepare_type(&mut env, pattern, b, 0)?;
                Associated::Type(instantiate::against(
                    self.module,
                    pattern,
                    actual,
                    &w.receiver,
                    &mut env.args,
                    b,
                )?)
            }
            (
                DynAssociatedValueRecipe::Constant { ty, value },
                DynAssociatedValue::Constant {
                    ty: actual,
                    value: claimed,
                },
            ) => {
                self.prepare_type(&mut env, ty, b, 0)?;
                let carrier =
                    instantiate::against(self.module, ty, actual, &w.receiver, &mut env.args, b)?;
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
                    let (node, _) = self.select(env.node, &receiver, &app, b)?;
                    self.prove(node, b)?;
                }
                DynObligation::TypeEqual { left, right } => {
                    let left = self.value(env, left, b)?;
                    let right = self.value(env, right, b)?;
                    dynamic_metadata::recipe(&left.raw, b)?;
                    dynamic_metadata::recipe(&right.raw, b)?;
                    if left.raw != right.raw || left.terms != right.terms {
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
                    match bound {
                        DynStructuralBound::Copy if value.raw.ty.is_copy_value() => {}
                        DynStructuralBound::StoreElement => {
                            dynamic_store::check_store_element(self.module, &value.raw.ty, None, b)?
                        }
                        _ => {
                            return Err(bad(
                                "structural premise requires its complete actual Value/cleanup/Copy qualification",
                            ));
                        }
                    }
                }
                DynObligation::Callable { .. } => {
                    return Err(bad(
                        "callable premise requires its fresh quantified signature and actual entry proof",
                    ));
                }
            }
        }
        Ok(())
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
            let (dependency, _) = self.select(node, &trait_env.receiver, &app, b)?;
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
) -> Check<()> {
    let Some(nodes) = table.implementation_witnesses.as_ref() else {
        return Ok(());
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
    };
    for node in 0..nodes.len() {
        graph.prove(node, b)?;
    }
    // Method-specific dependency edges are joined by the method proof. With no
    // executable slots there can be no such additional roots.
    if table.entries.is_empty() {
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
    Ok(())
}
