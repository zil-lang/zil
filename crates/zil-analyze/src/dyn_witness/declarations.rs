//! Declaration-scoped syntax is validated before any substitution. Parameter
//! ordinals retain their original Type/Const/Life positions and lexical scopes.
use super::*;

#[derive(Clone)]
struct Scope<'a> {
    declaration: &'a [LifeParameterKind],
    method: &'a [LifeParameterKind],
    quantified: Vec<u32>,
    invocation: u32,
    implicit: usize,
    self_type: bool,
}
impl Scope<'_> {
    fn formal(&self, method: bool, ordinal: u32, wanted: u8) -> CompileResult<()> {
        let parameters = if method {
            self.method
        } else {
            self.declaration
        };
        let actual = parameters.get(ordinal as usize);
        if matches!(
            (wanted, actual),
            (0, Some(LifeParameterKind::Type))
                | (1, Some(LifeParameterKind::Const(_)))
                | (2, Some(LifeParameterKind::Lifetime))
        ) {
            Ok(())
        } else {
            Err(fail(
                "recipe parameter changes its original kind, ordinal or scope",
            ))
        }
    }
    fn life(&self, life: &DynLifePattern) -> CompileResult<()> {
        match life {
            DynLifePattern::DeclarationParameter(i) => self.formal(false, *i, 2),
            DynLifePattern::MethodParameter(i) => self.formal(true, *i, 2),
            DynLifePattern::Bound { depth, index } => {
                if self
                    .quantified
                    .iter()
                    .rev()
                    .nth(*depth as usize)
                    .is_some_and(|count| *index < *count)
                {
                    Ok(())
                } else {
                    Err(fail(
                        "recipe quantified lifetime escapes its lexical binder",
                    ))
                }
            }
            DynLifePattern::ImplicitDeclaration(i) if (*i as usize) < self.implicit => Ok(()),
            DynLifePattern::ImplicitDeclaration(_) => Err(fail(
                "implicit declaration lifetime has no original occurrence table row",
            )),
            DynLifePattern::Invocation(i) if *i < self.invocation => Ok(()),
            DynLifePattern::Invocation(_) => Err(fail(
                "recipe invocation lifetime is outside its method domain",
            )),
        }
    }
}

enum Work<'a> {
    Type(&'a DynTypePattern, Scope<'a>, usize),
    Const(&'a DynConstPattern, Scope<'a>, usize),
    Application(&'a DynSelfPattern, bool, Scope<'a>, usize),
    Obligation(&'a DynObligation, Scope<'a>, usize),
    Signature(&'a DynSignatureRecipe, Scope<'a>, usize),
}
struct Walk<'a, 'b> {
    module: &'a TypedModule,
    b: &'b mut Budget,
    pending: Vec<Work<'a>>,
}
impl<'a> Walk<'a, '_> {
    fn add(&mut self, work: Work<'a>) -> CompileResult<()> {
        self.b.spend(1, WORK)?;
        self.pending.push(work);
        Ok(())
    }
    fn obligations(
        &mut self,
        obligations: &'a [DynObligation],
        scope: &Scope<'a>,
        depth: usize,
    ) -> CompileResult<()> {
        self.b.spend(
            obligations
                .len()
                .saturating_mul(scope.quantified.len().saturating_add(1)),
            WORK,
        )?;
        for obligation in obligations {
            self.add(Work::Obligation(obligation, scope.clone(), depth))?;
        }
        Ok(())
    }
    fn application(
        &mut self,
        pattern: &'a DynSelfPattern,
        is_trait: bool,
        scope: Scope<'a>,
        depth: usize,
    ) -> CompileResult<()> {
        self.b.spend(
            self.module
                .dyn_declarations
                .len()
                .saturating_add(self.module.life_declarations.len()),
            WORK,
        )?;
        let parameters = if is_trait {
            let mut matches = self
                .module
                .dyn_declarations
                .iter()
                .filter(|d| d.id == pattern.declaration && d.kind == DynDeclarationKind::Trait);
            let declaration = matches
                .next()
                .ok_or_else(|| fail("application recipe has no actual trait declaration"))?;
            if matches.next().is_some() {
                return Err(fail("application recipe has ambiguous trait identity"));
            }
            &declaration.parameters
        } else {
            let mut matches = self
                .module
                .life_declarations
                .iter()
                .filter(|d| d.id == pattern.declaration);
            let declaration = matches
                .next()
                .ok_or_else(|| fail("nominal recipe has no actual type declaration"))?;
            if matches.next().is_some() {
                return Err(fail("nominal recipe has ambiguous declaration identity"));
            }
            &declaration.parameters
        };
        if parameters.len() != pattern.arguments.len() {
            return Err(fail("recipe omits original ordered generic occurrences"));
        }
        self.b.spend(
            parameters
                .len()
                .saturating_mul(scope.quantified.len().saturating_add(1)),
            WORK,
        )?;
        for (kind, actual) in parameters.iter().zip(&pattern.arguments) {
            match (kind, actual) {
                (LifeParameterKind::Type, DynPatternActual::Type(ty)) => {
                    self.add(Work::Type(ty, scope.clone(), depth + 1))?
                }
                (LifeParameterKind::Const(_), DynPatternActual::Const(value)) => {
                    self.add(Work::Const(value, scope.clone(), depth + 1))?
                }
                (LifeParameterKind::Lifetime, DynPatternActual::Life(life)) => scope.life(life)?,
                _ => return Err(fail("recipe actual changes the original formal kind")),
            }
        }
        Ok(())
    }
    fn run(&mut self) -> CompileResult<()> {
        while let Some(work) = self.pending.pop() {
            let depth = match &work {
                Work::Type(_, _, d)
                | Work::Const(_, _, d)
                | Work::Application(_, _, _, d)
                | Work::Obligation(_, _, d)
                | Work::Signature(_, _, d) => *d,
            };
            let scope = match &work {
                Work::Type(_, scope, _)
                | Work::Const(_, scope, _)
                | Work::Application(_, _, scope, _)
                | Work::Obligation(_, scope, _)
                | Work::Signature(_, scope, _) => scope,
            };
            // Reserve the at-most-three scalar successors before cloning their
            // lexical binder stacks. Variable-arity successors prepay below.
            self.b.spend(
                scope.quantified.len().saturating_mul(3).saturating_add(1),
                WORK,
            )?;
            if depth > self.b.partial_depth.min(128) {
                return Err(limit("dynamic declaration recipe scope depth exceeded"));
            }
            match work {
                Work::Type(pattern, scope, depth) => match pattern {
                    DynTypePattern::Concrete(ty) => {
                        crate::dyn_metadata::ty(ty, self.b)?;
                        if !super::values::known_zero(self.module, ty, self.b)? {
                            return Err(fail("Concrete recipe is not a complete Known0 type"));
                        }
                    }
                    DynTypePattern::Parameter(i) => scope.formal(false, *i, 0)?,
                    DynTypePattern::MethodParameter(i) => scope.formal(true, *i, 0)?,
                    DynTypePattern::SelfType if !scope.self_type => {
                        return Err(fail("Self cannot prove its own implementation pattern"));
                    }
                    DynTypePattern::SelfType => {}
                    DynTypePattern::Nominal(application) => {
                        self.add(Work::Application(application, false, scope, depth + 1))?
                    }
                    DynTypePattern::Address {
                        lifetime, inner, ..
                    } => {
                        let lifetime = lifetime.as_ref().ok_or_else(|| fail("complete signature/self recipe has no explicit elided lifetime identity"))?;
                        scope.life(lifetime)?;
                        self.add(Work::Type(inner, scope, depth + 1))?;
                    }
                    DynTypePattern::Desc { lifetime, inner } => {
                        let lifetime = lifetime
                            .as_ref()
                            .ok_or_else(|| fail("description recipe outer domain is Unknown"))?;
                        scope.life(lifetime)?;
                        self.add(Work::Type(inner, scope, depth + 1))?;
                    }
                    DynTypePattern::Tuple(fields) => {
                        self.b.spend(
                            fields
                                .len()
                                .saturating_mul(scope.quantified.len().saturating_add(1)),
                            WORK,
                        )?;
                        for field in fields {
                            self.add(Work::Type(field, scope.clone(), depth + 1))?;
                        }
                    }
                    DynTypePattern::Array { element, length } => {
                        self.add(Work::Type(element, scope.clone(), depth + 1))?;
                        self.add(Work::Const(length, scope, depth + 1))?;
                    }
                    DynTypePattern::Slice(inner)
                    | DynTypePattern::Managed(inner)
                    | DynTypePattern::OwnedStore(inner) => {
                        self.add(Work::Type(inner, scope, depth + 1))?
                    }
                    DynTypePattern::ErrorUnion { error, result } => {
                        self.add(Work::Type(error, scope.clone(), depth + 1))?;
                        self.add(Work::Type(result, scope, depth + 1))?;
                    }
                    DynTypePattern::AssociatedProjection {
                        receiver,
                        application,
                        member,
                    } => {
                        member_kind(self.module, application.declaration, *member, false, self.b)?;
                        self.add(Work::Type(receiver, scope.clone(), depth + 1))?;
                        self.add(Work::Application(application, true, scope, depth + 1))?;
                    }
                },
                Work::Const(pattern, scope, depth) => match pattern {
                    DynConstPattern::Value(_)
                    | DynConstPattern::IntegerLiteral(_)
                    | DynConstPattern::FloatLiteral(_) => {}
                    DynConstPattern::Parameter(i) => scope.formal(false, *i, 1)?,
                    DynConstPattern::MethodParameter(i) => scope.formal(true, *i, 1)?,
                    DynConstPattern::Unary { value, ty, .. }
                    | DynConstPattern::Convert { value, ty } => {
                        self.add(Work::Const(value, scope.clone(), depth + 1))?;
                        self.add(Work::Type(ty, scope, depth + 1))?;
                    }
                    DynConstPattern::Binary {
                        left, right, ty, ..
                    } => {
                        self.add(Work::Const(left, scope.clone(), depth + 1))?;
                        self.add(Work::Const(right, scope.clone(), depth + 1))?;
                        self.add(Work::Type(ty, scope, depth + 1))?;
                    }
                    DynConstPattern::AssociatedProjection {
                        receiver,
                        application,
                        member,
                    } => {
                        member_kind(self.module, application.declaration, *member, true, self.b)?;
                        self.add(Work::Type(receiver, scope.clone(), depth + 1))?;
                        self.add(Work::Application(application, true, scope, depth + 1))?;
                    }
                },
                Work::Application(application, is_trait, scope, depth) => {
                    self.application(application, is_trait, scope, depth)?
                }
                Work::Obligation(obligation, scope, depth) => match obligation {
                    DynObligation::TypeEqual { left, right } => {
                        self.add(Work::Type(left, scope.clone(), depth + 1))?;
                        self.add(Work::Type(right, scope, depth + 1))?;
                    }
                    DynObligation::ConstEqual { ty, left, right } => {
                        self.add(Work::Type(ty, scope.clone(), depth + 1))?;
                        self.add(Work::Const(left, scope.clone(), depth + 1))?;
                        self.add(Work::Const(right, scope, depth + 1))?;
                    }
                    DynObligation::Structural { receiver, .. } => {
                        self.add(Work::Type(receiver, scope, depth + 1))?
                    }
                    DynObligation::Trait {
                        receiver,
                        application,
                    } => {
                        self.add(Work::Type(receiver, scope.clone(), depth + 1))?;
                        self.add(Work::Application(application, true, scope, depth + 1))?;
                    }
                    DynObligation::Callable {
                        receiver,
                        quantified_lifetimes,
                        signature,
                        ..
                    } => {
                        self.add(Work::Type(receiver, scope.clone(), depth + 1))?;
                        self.b
                            .spend(scope.quantified.len().saturating_add(1), WORK)?;
                        let mut bound = scope;
                        bound.quantified.push(*quantified_lifetimes);
                        self.add(Work::Signature(signature, bound, depth + 1))?;
                    }
                },
                Work::Signature(signature, mut scope, depth) => {
                    scope.invocation = signature.invocation_binder_count;
                    self.b.spend(
                        signature
                            .contract
                            .requires
                            .as_ref()
                            .map_or(0, Vec::len)
                            .saturating_add(
                                signature.contract.guarantees.as_ref().map_or(0, Vec::len),
                            )
                            .saturating_add(signature.contract.effects.as_ref().map_or(0, Vec::len))
                            .saturating_add(
                                signature
                                    .contract
                                    .cleanup
                                    .as_ref()
                                    .map_or(0, |c| c.effects.len()),
                            )
                            .saturating_mul(scope.quantified.len().saturating_add(1)),
                        WORK,
                    )?;
                    self.b.spend(
                        signature
                            .parameters
                            .len()
                            .saturating_mul(scope.quantified.len().saturating_add(1)),
                        WORK,
                    )?;
                    for parameter in &signature.parameters {
                        self.add(Work::Type(&parameter.ty, scope.clone(), depth + 1))?;
                    }
                    self.add(Work::Type(&signature.result, scope.clone(), depth + 1))?;
                    for requirement in signature.contract.requires.iter().flatten() {
                        if let DynRequirementRecipe::State { ty, .. } = requirement {
                            self.add(Work::Type(ty, scope.clone(), depth + 1))?;
                        }
                    }
                    for guarantee in signature.contract.guarantees.iter().flatten() {
                        if let DynGuaranteeRecipe::State { ty, .. } = guarantee {
                            self.add(Work::Type(ty, scope.clone(), depth + 1))?;
                        }
                    }
                    for effect in signature
                        .contract
                        .effects
                        .iter()
                        .flatten()
                        .chain(signature.contract.cleanup.iter().flat_map(|c| &c.effects))
                    {
                        if let DynEffectRecipe::Cleanup(ty) = effect {
                            self.add(Work::Type(ty, scope.clone(), depth + 1))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn member_kind(
    module: &TypedModule,
    declaration: LifeDeclarationId,
    ordinal: u32,
    constant: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(module.dyn_declarations.len(), WORK)?;
    let mut matches = module
        .dyn_declarations
        .iter()
        .filter(|d| d.id == declaration && d.kind == DynDeclarationKind::Trait);
    let declaration = matches
        .next()
        .ok_or_else(|| fail("projection has no actual trait declaration"))?;
    if matches.next().is_some() {
        return Err(fail("projection trait identity is ambiguous"));
    }
    let kind = declaration.members.get(ordinal as usize).map(|m| &m.kind);
    if matches!(
        (constant, kind),
        (false, Some(DynMemberKind::Type)) | (true, Some(DynMemberKind::Constant))
    ) {
        Ok(())
    } else {
        Err(fail("projection changes the actual member kind or ordinal"))
    }
}

pub(super) fn validate(
    module: &TypedModule,
    declaration: &DynDeclaration,
    b: &mut Budget,
) -> CompileResult<()> {
    let (Some(supers), Some(requirements), Some(associated), Some(methods), Some(definitions)) = (
        &declaration.direct_supertraits,
        &declaration.requirements,
        &declaration.associated_definitions,
        &declaration.method_signatures,
        &declaration.method_definitions,
    ) else {
        return Err(fail("complete original declaration recipes are Unknown"));
    };
    if declaration.kind == DynDeclarationKind::Impl && !supers.is_empty() {
        return Err(fail(
            "an impl cannot replace the trait's original direct-super graph",
        ));
    }
    let scope = Scope {
        declaration: &declaration.parameters,
        method: &[],
        quantified: vec![],
        invocation: 0,
        implicit: declaration
            .implicit_lifetimes
            .as_ref()
            .ok_or_else(|| fail("original implicit lifetime occurrence table is Unknown"))?
            .len(),
        self_type: true,
    };
    let mut walk = Walk {
        module,
        b,
        pending: vec![],
    };
    if let Some(pattern) = &declaration.self_pattern {
        if declaration.kind != DynDeclarationKind::Impl {
            return Err(fail("a trait cannot assert an impl Self pattern"));
        }
        let mut self_scope = scope.clone();
        self_scope.self_type = false;
        walk.add(Work::Type(pattern, self_scope, 0))?;
    } else if declaration.kind == DynDeclarationKind::Impl {
        return Err(fail("an impl's complete original Self pattern is Unknown"));
    }
    if let Some(application) = &declaration.implemented_trait {
        if declaration.kind != DynDeclarationKind::Impl {
            return Err(fail("a trait cannot assert implementation membership"));
        }
        walk.add(Work::Application(application, true, scope.clone(), 0))?;
    } else if declaration.kind == DynDeclarationKind::Impl {
        return Err(fail(
            "impl's original complete trait application is Unknown",
        ));
    }
    for application in supers {
        walk.add(Work::Application(application, true, scope.clone(), 0))?;
    }
    walk.obligations(requirements, &scope, 0)?;
    let mut seen = std::collections::BTreeSet::new();
    for definition in associated {
        walk.b
            .spend(declaration.members.len().saturating_add(seen.len()), WORK)?;
        if !seen.insert(definition.member) {
            return Err(fail("associated definition repeats an original member"));
        }
        let kind = declaration
            .members
            .get(definition.member as usize)
            .map(|m| &m.kind);
        match (kind, &definition.carrier, &definition.value) {
            (Some(DynMemberKind::Type), None, Some(DynAssociatedValueRecipe::Type(ty))) => {
                walk.add(Work::Type(ty, scope.clone(), 0))?
            }
            (Some(DynMemberKind::Type), None, None)
                if declaration.kind == DynDeclarationKind::Trait => {}
            (
                Some(DynMemberKind::Constant),
                Some(carrier),
                Some(DynAssociatedValueRecipe::Constant { ty, value }),
            ) => {
                walk.add(Work::Type(carrier, scope.clone(), 0))?;
                walk.add(Work::Type(ty, scope.clone(), 0))?;
                walk.add(Work::Const(value, scope.clone(), 0))?;
            }
            (Some(DynMemberKind::Constant), Some(carrier), None)
                if declaration.kind == DynDeclarationKind::Trait =>
            {
                walk.add(Work::Type(carrier, scope.clone(), 0))?
            }
            _ => {
                return Err(fail(
                    "associated recipe changes its original kind or loses its carrier/definition",
                ));
            }
        }
        walk.obligations(&definition.requirements, &scope, 0)?;
    }
    for (ordinal, member) in declaration.members.iter().enumerate() {
        if !matches!(member.kind, DynMemberKind::Method(_)) && !seen.contains(&(ordinal as u32)) {
            return Err(fail(
                "original associated member has no complete definition row",
            ));
        }
    }
    seen.clear();
    if declaration.kind == DynDeclarationKind::Impl && !methods.is_empty() {
        return Err(fail(
            "impl signatures belong to original definition recipes, not trait member recipes",
        ));
    }
    for method in methods {
        walk.b
            .spend(declaration.members.len().saturating_add(seen.len()), WORK)?;
        let Some(DynMemberDeclaration {
            kind: DynMemberKind::Method(parameters),
            ..
        }) = declaration.members.get(method.member as usize)
        else {
            return Err(fail("signature recipe does not name an original method"));
        };
        if !seen.insert(method.member) {
            return Err(fail("original method signature recipe is duplicated"));
        }
        let mut method_scope = scope.clone();
        method_scope.method = parameters;
        walk.add(Work::Signature(&method.signature, method_scope.clone(), 0))?;
        walk.obligations(&method.requirements, &method_scope, 0)?;
    }
    for (ordinal, member) in declaration.members.iter().enumerate() {
        if declaration.kind == DynDeclarationKind::Trait
            && matches!(member.kind, DynMemberKind::Method(_))
            && !seen.contains(&(ordinal as u32))
        {
            return Err(fail("original method signature is Unknown"));
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    seen.clear();
    for definition in definitions {
        walk.b.spend(definitions.len(), WORK)?;
        if !ids.insert(definition.id)
            || !seen.insert(definition.member)
            || definition.id.module != declaration.id.module
        {
            return Err(fail(
                "method definition identity is duplicated or belongs to another module",
            ));
        }
        let Some(DynMemberDeclaration {
            kind: DynMemberKind::Method(parameters),
            ..
        }) = declaration.members.get(definition.member as usize)
        else {
            return Err(fail("definition recipe does not name an original method"));
        };
        if definition.scope
            != if declaration.kind == DynDeclarationKind::Impl {
                DynDefinitionScope::ImplOverride
            } else {
                DynDefinitionScope::TraitDefault
            }
        {
            return Err(fail("original method definition changes its scope"));
        }
        let mut method_scope = scope.clone();
        method_scope.method = parameters;
        walk.add(Work::Signature(
            &definition.signature,
            method_scope.clone(),
            0,
        ))?;
        walk.obligations(&definition.requirements, &method_scope, 0)?;
    }
    if declaration.kind == DynDeclarationKind::Impl {
        for (ordinal, member) in declaration.members.iter().enumerate() {
            if matches!(member.kind, DynMemberKind::Method(_)) && !seen.contains(&(ordinal as u32))
            {
                return Err(fail("original impl method definition is Unknown"));
            }
        }
    } else {
        for method in methods {
            walk.b.spend(definitions.len(), WORK)?;
            let actual = definitions
                .iter()
                .find(|d| d.member == method.member)
                .map(|d| d.id);
            if method.default_definition != actual {
                return Err(fail(
                    "trait default signature and actual original definition identity differ",
                ));
            }
        }
    }
    walk.run()
}
