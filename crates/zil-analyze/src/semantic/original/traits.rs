//! Original trait identities and projections. Receiver and all mixed actuals
//! are interpreted in the original domain, never selected from a vtable ABI.
use super::*;
mod conformance;
mod methods;
mod providers;
mod raw;
pub(in crate::semantic) use methods::TraitCall;

#[derive(Default)]
pub(super) struct State {
    scopes: BTreeMap<LifeDeclarationId, SemanticDefinitionId>,
    pub(super) selves: Vec<(SemanticDefinitionId, Term)>,
    projections: Vec<Term>,
    facts: Vec<(Term, Predicate)>,
    active: Vec<(Term, LifeDeclarationId, Vec<Term>, u32)>,
    proving_traits: Vec<(Term, LifeDeclarationId, Vec<Term>)>,
    associated_values: Vec<(Term, LifeDeclarationId, Vec<Term>, u32, Term)>,
}

fn trait_frame(definition: SemanticDefinitionId, formals: Vec<Term>) -> Frame {
    Frame {
        definition,
        formals,
        parameters: Vec::new(),
        result: 0,
        locals: Vec::new(),
        inherited: 0,
        captured: BTreeMap::new(),
        alive: true,
        environment: Vec::new(),
        expressions: BTreeMap::new(),
        bindings: BTreeMap::new(),
    }
}

fn declaration_work(row: &DynDeclaration, b: &mut Budget) -> CompileResult<()> {
    crate::dyn_metadata::recipe_declaration(row, b)?;
    if let Some(pattern) = &row.self_pattern {
        crate::dyn_metadata::recipe(pattern, b)?;
    }
    if let Some(application) = &row.implemented_trait {
        crate::dyn_metadata::recipe(application, b)?;
    }
    b.spend(
        row.identity
            .package
            .len()
            .saturating_add(
                row.identity
                    .module_path
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .saturating_add(
                row.identity
                    .declaration_path
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .saturating_add(row.identity.generation_path.len())
            .saturating_add(row.parameters.len() + row.members.len() + 1),
        WORK,
    )?;
    for kind in
        row.parameters
            .iter()
            .chain(row.members.iter().flat_map(|member| match &member.kind {
                DynMemberKind::Method(parameters) => parameters.as_slice(),
                _ => &[],
            }))
    {
        b.spend(1, WORK)?;
        if let LifeParameterKind::Const(ty) = kind {
            crate::dyn_metadata::ty(ty, b)?;
        }
    }
    for member in &row.members {
        b.spend(member.name.len(), WORK)?;
    }
    Ok(())
}

impl<'a> Inventory<'a> {
    pub(in crate::semantic) fn original_dyn(
        &self,
        id: LifeDeclarationId,
        b: &mut Budget,
    ) -> CompileResult<&'a DynDeclaration> {
        let mut found = None;
        let mut scan = |rows: &'a [DynDeclaration], b: &mut Budget| -> CompileResult<()> {
            for row in rows {
                b.spend(1, WORK)?;
                if row.id != id {
                    continue;
                }
                declaration_work(row, b)?;
                if let Some(old) = found {
                    declaration_work(old, b)?;
                    if old != row {
                        return Err(fail(
                            "original trait has conflicting full declaration inventories",
                        ));
                    }
                } else {
                    found = Some(row);
                }
            }
            Ok(())
        };
        scan(&self.module.dyn_declarations, b)?;
        if let Some(source) = self.source {
            for module in &source.modules {
                scan(&module.core.dyn_declarations, b)?;
            }
        }
        found.ok_or_else(|| {
            fail("original trait declaration is absent from the complete source inventory")
        })
    }
}

impl Solver<'_, '_> {
    pub(super) fn original_trait_facts(
        &mut self,
        frame: &Frame,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let declaration = self.inventory.definition(frame.definition, b)?;
        for requirement in declaration
            .parameters
            .iter()
            .filter_map(|formal| {
                if let SemanticFormal::Type { requirements } = formal {
                    Some(requirements.as_slice())
                } else {
                    None
                }
            })
            .flatten()
            .chain(&declaration.requirements)
        {
            b.spend(1, WORK)?;
            match requirement {
                SemanticRequirementRecipe::Trait { application, .. } => {
                    self.trait_arguments(frame, application, b, 0)?;
                }
                SemanticRequirementRecipe::Structural {
                    ty: ty @ SemanticTypeRecipe::AssociatedType { .. },
                    ..
                }
                | SemanticRequirementRecipe::Builtin {
                    ty: ty @ SemanticTypeRecipe::AssociatedType { .. },
                    ..
                }
                | SemanticRequirementRecipe::Structural {
                    ty: ty @ SemanticTypeRecipe::SelfType { .. },
                    ..
                }
                | SemanticRequirementRecipe::Builtin {
                    ty: ty @ SemanticTypeRecipe::SelfType { .. },
                    ..
                } => {
                    let (_, predicate) = self.requirement(requirement)?;
                    let subject = self.recipe(frame, ty, b, 0)?;
                    Self::push(&mut self.trait_state.facts, (subject, predicate), b)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn original_trait_need(
        &mut self,
        frame: &Frame,
        requirement: &SemanticRequirementRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let SemanticRequirementRecipe::Trait { ty, application } = requirement else {
            return Err(fail(
                "original trait obligation changes its requirement kind",
            ));
        };
        let receiver = self.recipe(frame, ty, b, 0)?;
        let arguments = self.trait_arguments(frame, application, b, 0)?;
        if self.declared_trait(frame, receiver, application.declaration, &arguments, b, 0)? {
            Ok(())
        } else {
            Err(fail(
                "original actual lacks its complete declared receiver trait obligation",
            ))
        }
    }

    pub(super) fn trait_catalog(&mut self, b: &mut Budget) -> CompileResult<()> {
        trait_members::validate(self.program, self.inventory, b)?;
        for definition in self.inventory.definitions.values() {
            b.spend(1, WORK)?;
            let SemanticDefinitionBody::Scope(recipe) = &definition.body else {
                continue;
            };
            let id = match recipe {
                SemanticScopeRecipe::Trait { declaration }
                | SemanticScopeRecipe::TraitImpl { declaration, .. } => *declaration,
                _ => continue,
            };
            let raw = self.inventory.original_dyn(id, b)?;
            crate::dyn_metadata::recipe_declaration(raw, b)?;
            if raw.identity != definition.identity
                || raw.parameters.len() != definition.parameters.len()
                || (raw.kind == DynDeclarationKind::Trait)
                    != matches!(recipe, SemanticScopeRecipe::Trait { .. })
            {
                return Err(fail(
                    "original trait scope changes its declaration identity, kind or mixed arity",
                ));
            }
            for (raw, formal) in raw.parameters.iter().zip(&definition.parameters) {
                if !matches!(
                    (raw, formal),
                    (LifeParameterKind::Type, SemanticFormal::Type { .. })
                        | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                        | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
                ) {
                    return Err(fail(
                        "original trait scope changes a full mixed formal kind",
                    ));
                }
            }
            if raw.implicit_lifetimes.is_none()
                || raw.requirements.is_none()
                || raw.direct_supertraits.is_none()
                || raw.associated_definitions.is_none()
                || raw.method_signatures.is_none()
                || raw.method_definitions.is_none()
            {
                return Err(unknown(
                    "original trait inventory omits a complete declaration recipe",
                ));
            }
            match recipe {
                SemanticScopeRecipe::Trait { .. }
                    if raw.self_pattern.is_none() && raw.implemented_trait.is_none() => {}
                SemanticScopeRecipe::TraitImpl {
                    self_type,
                    trait_application,
                    ..
                } => {
                    let Some(DynTypePattern::Semantic { scope, ty }) = &raw.self_pattern else {
                        return Err(unknown(
                            "original impl Self pattern needs its exact scoped source recipe",
                        ));
                    };
                    if *scope != definition.id
                        || ty.as_ref() != self_type
                        || raw.implemented_trait.as_ref().is_none_or(|app| {
                            app.declaration != trait_application.declaration
                                || app.arguments.len() != trait_application.arguments.len()
                        })
                    {
                        return Err(fail(
                            "original impl scope and declared Self or trait application disagree",
                        ));
                    }
                }
                _ => {
                    return Err(fail(
                        "original trait invents an implementation Self or trait recipe",
                    ));
                }
            }
            b.lookup(self.trait_state.scopes.len(), WORK)?;
            if self.trait_state.scopes.insert(id, definition.id).is_some() {
                return Err(fail(
                    "original trait declaration has multiple semantic owner scopes",
                ));
            }
            let mut seen = BTreeSet::new();
            for member in &raw.members {
                b.spend(member.name.len().saturating_add(seen.len() + 1), WORK)?;
                if !seen.insert(&member.name) {
                    return Err(fail("original trait repeats a member identity"));
                }
            }
            let mut associated = BTreeSet::new();
            for row in raw.associated_definitions.as_ref().expect("checked") {
                let member = raw
                    .members
                    .get(row.member as usize)
                    .ok_or_else(|| fail("original associated definition has no declared member"))?;
                b.lookup(associated.len(), WORK)?;
                if !associated.insert(row.member) {
                    return Err(fail("original associated member definition is repeated"));
                }
                match (&member.kind, &row.carrier, &row.value) {
                    (DynMemberKind::Type, None, None) if raw.kind == DynDeclarationKind::Trait => {}
                    (DynMemberKind::Type, None, Some(DynAssociatedValueRecipe::Type(_))) => {}
                    (DynMemberKind::Constant, Some(_), None)
                        if raw.kind == DynDeclarationKind::Trait => {}
                    (
                        DynMemberKind::Constant,
                        Some(_),
                        Some(DynAssociatedValueRecipe::Constant { .. }),
                    ) => {}
                    _ => {
                        return Err(fail(
                            "original associated definition changes its member kind or omits its RHS",
                        ));
                    }
                }
            }
            for (ordinal, member) in raw.members.iter().enumerate() {
                if matches!(member.kind, DynMemberKind::Type | DynMemberKind::Constant)
                    && !associated.contains(&(ordinal as u32))
                {
                    return Err(fail(
                        "original trait omits an associated member declaration",
                    ));
                }
            }
        }
        // A raw declaration is not an executable interface. Nevertheless each
        // original row must have an exact scope, even when no table is built.
        for declaration in &self.inventory.module.dyn_declarations {
            b.lookup(self.trait_state.scopes.len(), WORK)?;
            if !self.trait_state.scopes.contains_key(&declaration.id) {
                return Err(fail(
                    "original raw trait declaration has no exact semantic owner scope",
                ));
            }
        }
        let mut done = BTreeSet::new();
        let mut active = BTreeSet::new();
        for id in self.trait_state.scopes.keys() {
            let mut pending = vec![(*id, false)];
            while let Some((id, exit)) = pending.pop() {
                b.spend(done.len().saturating_add(active.len() + 1), WORK)?;
                if exit {
                    active.remove(&id);
                    done.insert(id);
                    continue;
                }
                if done.contains(&id) {
                    continue;
                }
                if !active.insert(id) {
                    return Err(fail("original supertrait declaration graph is cyclic"));
                }
                let raw = self.inventory.original_dyn(id, b)?;
                Self::push(&mut pending, (id, true), b)?;
                for application in raw
                    .direct_supertraits
                    .as_ref()
                    .ok_or_else(|| unknown("original supertrait inventory is unknown"))?
                {
                    let target = self.inventory.original_dyn(application.declaration, b)?;
                    if target.kind != DynDeclarationKind::Trait
                        || target.parameters.len() != application.arguments.len()
                    {
                        return Err(fail(
                            "original supertrait edge changes its declaration kind or full mixed arity",
                        ));
                    }
                    Self::push(&mut pending, (application.declaration, false), b)?;
                }
            }
        }
        self.raw_trait_consistency(b)?;
        self.trait_scope_frames(b)?;
        Ok(())
    }

    fn trait_scope(
        &self,
        declaration: LifeDeclarationId,
        b: &mut Budget,
    ) -> CompileResult<SemanticDefinitionId> {
        b.lookup(self.trait_state.scopes.len(), WORK)?;
        self.trait_state
            .scopes
            .get(&declaration)
            .copied()
            .ok_or_else(|| fail("original trait application has no exact declaration scope"))
    }

    pub(super) fn trait_self(
        &mut self,
        frame: &Frame,
        scope: SemanticScopeRef,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let owner = self.original_scope(frame.definition, &scope, b)?;
        for (id, term) in self.trait_state.selves.iter().rev() {
            b.spend(1, WORK)?;
            if *id == owner {
                if *term == usize::MAX {
                    return Err(fail(
                        "original impl Self pattern refers to its own unestablished Self",
                    ));
                }
                return Ok(*term);
            }
        }
        let declaration = self.inventory.definition(owner, b)?;
        match &declaration.body {
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. }) => {
                self.term(Kind::SelfType(owner), b)
            }
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::TraitImpl { self_type, .. }
                | SemanticScopeRecipe::InherentImpl { self_type, .. }
                | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
            ) => {
                let mut args = Vec::new();
                for (ordinal, formal) in declaration.parameters.iter().enumerate() {
                    let arg = self.rigid_formal(owner, ordinal, formal, &args, b)?;
                    Self::push(&mut args, arg, b)?;
                }
                let parent = trait_frame(owner, args);
                // No Self occurrence can establish its own implementation
                // pattern: detect recursion instead of accepting a self link.
                Self::push(&mut self.trait_state.selves, (owner, usize::MAX), b)?;
                let result = self.recipe(&parent, self_type, b, depth + 1);
                self.trait_state.selves.pop();
                result
            }
            _ => Err(fail(
                "original Self reference does not select an original trait or impl scope",
            )),
        }
    }

    pub(super) fn trait_arguments(
        &mut self,
        frame: &Frame,
        application: &SemanticTraitRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Term>> {
        self.step(depth, b)?;
        let scope = self.trait_scope(application.declaration, b)?;
        let declaration = self.inventory.definition(scope, b)?;
        if !matches!(
            declaration.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
        ) || application.arguments.len() != declaration.parameters.len()
        {
            return Err(fail(
                "original trait application changes its declaration kind or full mixed arity",
            ));
        }
        let mut arguments = Vec::new();
        for (formal, recipe) in declaration.parameters.iter().zip(&application.arguments) {
            let argument = match (formal, recipe) {
                (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(ty)) => {
                    self.recipe(frame, ty, b, depth + 1)?
                }
                (SemanticFormal::Lifetime, SemanticActualRecipe::Life(life)) => {
                    self.life(frame, life, b, depth + 1)?
                }
                (
                    SemanticFormal::Const { carrier },
                    SemanticActualRecipe::Const {
                        carrier: written,
                        value,
                    },
                ) => {
                    b.spend(arguments.len(), WORK)?;
                    let target = trait_frame(scope, arguments.clone());
                    let expected = self.recipe(&target, carrier, b, depth + 1)?;
                    let written = self.recipe(frame, written, b, depth + 1)?;
                    self.unify(expected, written, depth + 1, b)?;
                    self.constant(frame, value, expected, b, depth + 1)?
                }
                _ => {
                    return Err(fail(
                        "original trait actual changes its complete mixed formal kind",
                    ));
                }
            };
            Self::push(&mut arguments, argument, b)?;
        }
        Ok(arguments)
    }

    pub(super) fn same_original(
        &self,
        left: Term,
        right: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let left = self.root(left, b)?;
        let right = self.root(right, b)?;
        if left == right {
            return Ok(true);
        }
        let pair = (&self.state.arena[left], &self.state.arena[right]);
        Ok(match pair {
            (Kind::Atom(a), Kind::Atom(z)) => a == z,
            (Kind::Task(a), Kind::Task(z)) => {
                b.spend(a.work().saturating_add(z.work()), WORK)?;
                if a.retained != z.retained {
                    false
                } else {
                    let mut left = a.terms();
                    let mut right = z.terms();
                    let mut equal = true;
                    loop {
                        match (left.next(), right.next()) {
                            (None, None) => break,
                            (Some(a), Some(z)) => {
                                equal &= self.same_original(a, z, b, depth + 1)?;
                            }
                            _ => {
                                equal = false;
                                break;
                            }
                        }
                    }
                    equal
                }
            }
            (Kind::Rigid(a, ai), Kind::Rigid(z, zi)) => a == z && ai == zi,
            (Kind::SelfType(a), Kind::SelfType(z)) => a == z,
            (Kind::Life(a), Kind::Life(z)) => a == z,
            (Kind::ConstValue(a), Kind::ConstValue(z)) => a == z,
            (
                Kind::ConstRigid {
                    owner: a,
                    ordinal: ai,
                    carrier: ac,
                },
                Kind::ConstRigid {
                    owner: z,
                    ordinal: zi,
                    carrier: zc,
                },
            ) => a == z && ai == zi && self.same_original(*ac, *zc, b, depth + 1)?,
            (Kind::CallResult { hidden, .. }, _) => {
                return self.same_original(*hidden, right, b, depth + 1);
            }
            (_, Kind::CallResult { hidden, .. }) => {
                return self.same_original(left, *hidden, b, depth + 1);
            }
            (Kind::Slice(a), Kind::Slice(z))
            | (Kind::Wrapped(Wrapper::Managed, a), Kind::Wrapped(Wrapper::Managed, z)) => {
                self.same_original(*a, *z, b, depth + 1)?
            }
            (Kind::Wrapped(a, ai), Kind::Wrapped(z, zi)) => {
                a == z && self.same_original(*ai, *zi, b, depth + 1)?
            }
            (
                Kind::Address {
                    mutable: a,
                    life: al,
                    inner: ai,
                },
                Kind::Address {
                    mutable: z,
                    life: zl,
                    inner: zi,
                },
            ) => {
                a == z
                    && self.same_original(*al, *zl, b, depth + 1)?
                    && self.same_original(*ai, *zi, b, depth + 1)?
            }
            (
                Kind::Desc {
                    life: al,
                    inner: ai,
                },
                Kind::Desc {
                    life: zl,
                    inner: zi,
                },
            ) => {
                self.same_original(*al, *zl, b, depth + 1)?
                    && self.same_original(*ai, *zi, b, depth + 1)?
            }
            (
                Kind::Array {
                    element: a,
                    length: al,
                },
                Kind::Array {
                    element: z,
                    length: zl,
                },
            ) => {
                self.same_original(*a, *z, b, depth + 1)?
                    && self.same_original(*al, *zl, b, depth + 1)?
            }
            (
                Kind::ErrorUnion {
                    error: a,
                    result: ar,
                },
                Kind::ErrorUnion {
                    error: z,
                    result: zr,
                },
            ) => {
                self.same_original(*a, *z, b, depth + 1)?
                    && self.same_original(*ar, *zr, b, depth + 1)?
            }
            (Kind::Tuple(a), Kind::Tuple(z)) => self.same_original_rows(a, z, b, depth + 1)?,
            (
                Kind::Nominal {
                    definition: a,
                    arguments: aa,
                    ..
                },
                Kind::Nominal {
                    definition: z,
                    arguments: za,
                    ..
                },
            ) => a == z && self.same_original_rows(aa, za, b, depth + 1)?,
            (
                Kind::Associated {
                    receiver: a,
                    declaration: ad,
                    arguments: aa,
                    member: am,
                },
                Kind::Associated {
                    receiver: z,
                    declaration: zd,
                    arguments: za,
                    member: zm,
                },
            ) => {
                ad == zd
                    && am == zm
                    && self.same_original(*a, *z, b, depth + 1)?
                    && self.same_original_rows(aa, za, b, depth + 1)?
            }
            _ => false,
        })
    }
    fn same_original_rows(
        &self,
        left: &[Term],
        right: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        if left.len() != right.len() {
            return Ok(false);
        }
        for (a, z) in left.iter().zip(right) {
            if !self.same_original(*a, *z, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn declared_trait(
        &mut self,
        frame: &Frame,
        receiver: Term,
        declaration: LifeDeclarationId,
        arguments: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let receiver = self.type_root(receiver, b)?;
        let (owner, ordinal) = match self.state.arena[receiver] {
            Kind::Rigid(owner, ordinal) => (owner, Some(ordinal)),
            Kind::SelfType(owner) => {
                let source = self.inventory.definition(owner, b)?;
                if matches!(source.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration: id }) if id == declaration)
                {
                    let context = if frame.definition == owner {
                        frame
                    } else {
                        self.state
                            .frames
                            .get(&owner)
                            .ok_or_else(|| fail("original trait Self has no lexical frame"))?
                    };
                    if self.same_original_rows(&context.formals, arguments, b, depth + 1)? {
                        return Ok(true);
                    }
                }
                (owner, None)
            }
            _ => {
                return self
                    .original_provider(frame, receiver, declaration, arguments, b, depth + 1)
                    .map(|_| true);
            }
        };
        let original = self.inventory.definition(owner, b)?;
        let mut rows = Vec::new();
        if let Some(ordinal) = ordinal {
            let Some(SemanticFormal::Type { requirements }) =
                original.parameters.get(ordinal as usize)
            else {
                return Err(fail("original trait receiver changes its formal kind"));
            };
            rows.extend(requirements.iter());
        }
        rows.extend(original.requirements.iter());
        for requirement in rows {
            b.spend(1, WORK)?;
            let SemanticRequirementRecipe::Trait { ty, application } = requirement else {
                continue;
            };
            if application.declaration != declaration {
                continue;
            }
            let context = if owner == frame.definition {
                frame
            } else {
                self.state
                    .frames
                    .get(&owner)
                    .ok_or_else(|| fail("original trait premise has no checked lexical frame"))?
            };
            // Copy only bounded arena indices. The original frame remains
            // immutable while its declared subject/application are evaluated.
            b.spend(context.formals.len(), WORK)?;
            let context = trait_frame(owner, context.formals.clone());
            let subject = self.recipe(&context, ty, b, depth + 1)?;
            let actuals = self.trait_arguments(&context, application, b, depth + 1)?;
            if self.same_original(receiver, subject, b, depth + 1)?
                && self.same_original_rows(arguments, &actuals, b, depth + 1)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn associated_type(
        &mut self,
        frame: &Frame,
        receiver: Term,
        declaration: LifeDeclarationId,
        arguments: Vec<Term>,
        member: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let raw = self.inventory.original_dyn(declaration, b)?;
        if raw.kind != DynDeclarationKind::Trait
            || !matches!(
                raw.members.get(member as usize).map(|member| &member.kind),
                Some(DynMemberKind::Type)
            )
        {
            return Err(fail(
                "original associated type selects another declaration or member kind",
            ));
        }
        for (old_receiver, old_decl, old_arguments, old_member, value) in
            &self.trait_state.associated_values
        {
            if *old_decl == declaration
                && *old_member == member
                && self.same_original(*old_receiver, receiver, b, depth + 1)?
                && self.same_original_rows(old_arguments, &arguments, b, depth + 1)?
            {
                return Ok(*value);
            }
        }
        if !matches!(
            self.state.arena[self.type_root(receiver, b)?],
            Kind::Rigid(..) | Kind::SelfType(_)
        ) {
            return self.provider_associated(
                frame,
                receiver,
                declaration,
                arguments,
                member,
                b,
                depth + 1,
            );
        }
        if !self.declared_trait(frame, receiver, declaration, &arguments, b, depth + 1)? {
            return Err(fail(
                "original associated query lacks its full receiver trait premise",
            ));
        }
        for term in &self.trait_state.projections {
            let Kind::Associated {
                receiver: old,
                declaration: id,
                arguments: actuals,
                member: ordinal,
            } = &self.state.arena[*term]
            else {
                return Err(fail("original projection inventory has another type kind"));
            };
            if *id == declaration
                && *ordinal == member
                && self.same_original(receiver, *old, b, depth + 1)?
                && self.same_original_rows(&arguments, actuals, b, depth + 1)?
            {
                return Ok(*term);
            }
        }
        let scope = self.trait_scope(declaration, b)?;
        b.spend(arguments.len(), WORK)?;
        let target = trait_frame(scope, arguments.clone());
        let term = self.term(
            Kind::Associated {
                receiver,
                declaration,
                arguments,
                member,
            },
            b,
        )?;
        Self::push(&mut self.trait_state.projections, term, b)?;
        let row = raw
            .associated_definitions
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.member == member))
            .ok_or_else(|| {
                fail("original associated member omits its full definition and bounds row")
            })?;
        Self::push(&mut self.trait_state.selves, (scope, receiver), b)?;
        let result: CompileResult<()> = (|| {
            for requirement in &row.requirements {
                match requirement {
                    DynObligation::Structural { receiver, bound } => {
                        let DynTypePattern::Semantic {
                            scope: original,
                            ty,
                        } = receiver
                        else {
                            return Err(unknown(
                                "associated member bound needs its exact original semantic subject",
                            ));
                        };
                        if *original != scope {
                            return Err(fail(
                                "associated member bound changes its original declaration scope",
                            ));
                        }
                        let subject = self.recipe(&target, ty, b, depth + 1)?;
                        let predicate = match bound {
                            DynStructuralBound::Copy => Predicate::Copy,
                            DynStructuralBound::Value => Predicate::Value,
                            DynStructuralBound::SyncDrop => Predicate::SyncDrop,
                            DynStructuralBound::StoreElement => Predicate::StoreElement,
                        };
                        Self::push(&mut self.trait_state.facts, (subject, predicate), b)?;
                    }
                    _ => {
                        return Err(unknown(
                            "associated member bound needs its complete trait, callable or equality proof",
                        ));
                    }
                }
            }
            Ok(())
        })();
        self.trait_state.selves.pop();
        result?;
        Ok(term)
    }

    pub(super) fn associated_satisfies(
        &self,
        term: Term,
        predicate: Predicate,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        for (subject, available) in &self.trait_state.facts {
            if entails(*available, predicate) && self.same_original(term, *subject, b, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
