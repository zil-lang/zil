//! Each original call opens its own Invocation namespace. This substitution
//! never rewrites a declaration, an intrinsic child class or a retained life.
use super::*;
impl Solver<'_, '_> {
    // Solve only Type variables exposed by a real input's complete original
    // constructor/argument directory before forming a dependent nominal or
    // associated output. This is inference, never a provider/where proof.
    pub(super) fn input_recipe_hint(&mut self, frame: &Frame, recipe: &SemanticTypeRecipe, actual: Term,
        b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.step(depth, b)?;
        let actual = self.type_root(actual, b)?;
        match recipe {
            SemanticTypeRecipe::Formal(reference) if reference.scope == SemanticScopeRef::Ancestor(0) => {
                if matches!(self.inventory.definition(frame.definition, b)?.parameters.get(reference.ordinal as usize),
                    Some(SemanticFormal::Type { .. }))
                {
                    let target = *frame.formals.get(reference.ordinal as usize)
                        .ok_or_else(|| fail("input inference omits its exact Type formal"))?;
                    self.input_hint(target, actual, b, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Nominal { definition, arguments } => {
                if let Kind::Nominal { definition: source, arguments: actuals, .. } = &self.state.arena[actual]
                    && source == definition && actuals.len() == arguments.len()
                {
                    b.spend(actuals.len(), WORK)?;
                    let actuals = actuals.clone();
                    for (recipe, actual) in arguments.iter().zip(actuals) {
                        if let SemanticActualRecipe::Type(recipe) = recipe {
                            self.input_recipe_hint(frame, recipe, actual, b, depth + 1)?;
                        }
                    }
                }
            }
            SemanticTypeRecipe::Address { inner, .. } => {
                if let Kind::Address { inner: actual, .. } = self.state.arena[actual] {
                    self.input_recipe_hint(frame, inner, actual, b, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Desc { inner, .. } => {
                if let Kind::Desc { inner: actual, .. } = self.state.arena[actual] {
                    self.input_recipe_hint(frame, inner, actual, b, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner) | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                if let Kind::Wrapped(wrapper, actual) = self.state.arena[actual] {
                    let same = matches!((recipe, wrapper),
                        (SemanticTypeRecipe::Managed(_), Wrapper::Managed)
                        | (SemanticTypeRecipe::OwnedStore(_), Wrapper::Store)
                        | (SemanticTypeRecipe::Mutex(_), Wrapper::Mutex)
                        | (SemanticTypeRecipe::Registration(_), Wrapper::Registration)
                        | (SemanticTypeRecipe::AsyncRegistration(_), Wrapper::AsyncRegistration));
                    if same { self.input_recipe_hint(frame, inner, actual, b, depth + 1)?; }
                }
            }
            SemanticTypeRecipe::Tuple(fields) => {
                if let Kind::Tuple(actuals) = &self.state.arena[actual] && actuals.len() == fields.len() {
                    b.spend(actuals.len(), WORK)?;
                    let actuals = actuals.clone();
                    for (field, actual) in fields.iter().zip(actuals) {
                        self.input_recipe_hint(frame, field, actual, b, depth + 1)?;
                    }
                }
            }
            SemanticTypeRecipe::Array { element, .. } => {
                if let Kind::Array { element: actual, .. } = self.state.arena[actual] {
                    self.input_recipe_hint(frame, element, actual, b, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Qualified { storage, .. } => self.input_recipe_hint(frame, storage, actual, b, depth + 1)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn invocation_actuals(
        &mut self,
        count: u32,
        b: &mut Budget,
    ) -> CompileResult<Vec<Term>> {
        b.spend(count as usize, WORK)?;
        let mut actuals = Vec::new();
        for ordinal in 0..count {
            let life = self.call_variable(call_regions::Role::Invocation(ordinal), b)?;
            Self::push(&mut actuals, life, b)?;
        }
        Ok(actuals)
    }
    pub(super) fn open_invocations(
        &mut self,
        term: Term,
        owner: SemanticDefinitionId,
        actuals: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        let width = match &self.state.arena[term] {
            Kind::Tuple(fields) => fields.len(),
            Kind::Task(task) => task.work(),
            Kind::Dyn(ty) => ty.work(),
            Kind::Nominal {
                arguments, fields, ..
            } => arguments.len().saturating_add(fields.len()),
            Kind::Closure {
                parameters,
                environment,
                ..
            } => parameters.len().saturating_add(environment.len() + 1),
            Kind::Code {
                parameters, modes, ..
            } => parameters.len().saturating_add(modes.len() + 1),
            _ => 1,
        };
        b.spend(width, WORK)?;
        match self.state.arena[term].clone() {
            Kind::Task(task) => self.task_open_invocations(task, owner, actuals, b, depth + 1),
            Kind::Dyn(ty) => self.dyn_open_invocations(ty, owner, actuals, b, depth + 1),
            Kind::MutexGuard { payload, life } => {
                let payload = self.open_invocations(payload, owner, actuals, b, depth + 1)?;
                let life = self.open_invocations(life, owner, actuals, b, depth + 1)?;
                self.term(Kind::MutexGuard { payload, life }, b)
            }
            Kind::Life(Life::Invocation(target, ordinal)) if target == owner => {
                actuals.get(ordinal as usize).copied().ok_or_else(|| {
                    fail("original call omits an Invocation occurrence in its own frame")
                })
            }
            Kind::Address {
                mutable,
                life,
                inner,
            } => {
                let life = self.open_invocations(life, owner, actuals, b, depth + 1)?;
                let inner = self.open_invocations(inner, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::Address {
                        mutable,
                        life,
                        inner,
                    },
                    b,
                )
            }
            Kind::Desc { life, inner } => {
                let life = self.open_invocations(life, owner, actuals, b, depth + 1)?;
                let inner = self.open_invocations(inner, owner, actuals, b, depth + 1)?;
                self.term(Kind::Desc { life, inner }, b)
            }
            Kind::Wrapped(wrapper, inner) => {
                let inner = self.open_invocations(inner, owner, actuals, b, depth + 1)?;
                self.term(Kind::Wrapped(wrapper, inner), b)
            }
            Kind::Slice(inner) => {
                let inner = self.open_invocations(inner, owner, actuals, b, depth + 1)?;
                self.term(Kind::Slice(inner), b)
            }
            Kind::Array { element, length } => {
                let element = self.open_invocations(element, owner, actuals, b, depth + 1)?;
                self.term(Kind::Array { element, length }, b)
            }
            Kind::ErrorUnion { error, result } => {
                let error = self.open_invocations(error, owner, actuals, b, depth + 1)?;
                let result = self.open_invocations(result, owner, actuals, b, depth + 1)?;
                self.term(Kind::ErrorUnion { error, result }, b)
            }
            Kind::Tuple(fields) => {
                let mut replaced = Vec::new();
                for field in fields {
                    let field = self.open_invocations(field, owner, actuals, b, depth + 1)?;
                    Self::push(&mut replaced, field, b)?;
                }
                self.term(Kind::Tuple(replaced), b)
            }
            Kind::Nominal {
                definition,
                arguments,
                fields,
            } => {
                let mut args = Vec::new();
                let mut values = Vec::new();
                for argument in arguments {
                    let argument = self.open_invocations(argument, owner, actuals, b, depth + 1)?;
                    Self::push(&mut args, argument, b)?;
                }
                for field in fields {
                    let field = self.open_invocations(field, owner, actuals, b, depth + 1)?;
                    Self::push(&mut values, field, b)?;
                }
                self.term(
                    Kind::Nominal {
                        definition,
                        arguments: args,
                        fields: values,
                    },
                    b,
                )
            }
            Kind::Closure {
                definition,
                parameters,
                result,
                environment,
            } => {
                let mut params = Vec::new();
                let mut env = Vec::new();
                for parameter in parameters {
                    let parameter =
                        self.open_invocations(parameter, owner, actuals, b, depth + 1)?;
                    Self::push(&mut params, parameter, b)?;
                }
                for field in environment {
                    let field = self.open_invocations(field, owner, actuals, b, depth + 1)?;
                    Self::push(&mut env, field, b)?;
                }
                let result = self.open_invocations(result, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::Closure {
                        definition,
                        parameters: params,
                        result,
                        environment: env,
                    },
                    b,
                )
            }
            Kind::Code {
                kind,
                parameters,
                result,
                modes,
                result_mode,
                call_kind,
            } => {
                let mut params = Vec::new();
                for parameter in parameters {
                    let parameter =
                        self.open_invocations(parameter, owner, actuals, b, depth + 1)?;
                    Self::push(&mut params, parameter, b)?;
                }
                let result = self.open_invocations(result, owner, actuals, b, depth + 1)?;
                let opened = self.term(
                    Kind::Code {
                        kind,
                        parameters: params,
                        result,
                        modes,
                        result_mode,
                        call_kind,
                    },
                    b,
                )?;
                self.opened_code(opened, term, b)
            }
            Kind::CallResult {
                caller,
                node,
                hidden,
            } => {
                let hidden = self.open_invocations(hidden, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::CallResult {
                        caller,
                        node,
                        hidden,
                    },
                    b,
                )
            }
            _ => Ok(term),
        }
    }
    /// Propagate only inference variables through identical result structure.
    /// Concrete lifetimes still meet at the ordinary argument/return checks;
    /// an expected result supplies no outlives or ownership premise.
    pub(super) fn call_result_context(
        &mut self,
        result: Term,
        expected: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let result = self.type_root(result, b)?;
        let expected = self.type_root(expected, b)?;
        if result == expected {
            return Ok(());
        }
        if matches!(
            self.state.arena[result],
            Kind::Infer(None) | Kind::LifeVariable(None)
        ) {
            return self.unify(result, expected, depth + 1, b);
        }
        let mut children = Vec::new();
        match (&self.state.arena[result], &self.state.arena[expected]) {
            (
                Kind::Nominal {
                    definition: a,
                    arguments: aa,
                    fields: af,
                },
                Kind::Nominal {
                    definition: z,
                    arguments: za,
                    fields: zf,
                },
            ) if a == z && aa.len() == za.len() && af.len() == zf.len() => {
                for pair in aa
                    .iter()
                    .copied()
                    .zip(za.iter().copied())
                    .chain(af.iter().copied().zip(zf.iter().copied()))
                {
                    Self::push(&mut children, pair, b)?;
                }
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
            ) if a == z => {
                Self::push(&mut children, (*al, *zl), b)?;
                Self::push(&mut children, (*ai, *zi), b)?;
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
                Self::push(&mut children, (*al, *zl), b)?;
                Self::push(&mut children, (*ai, *zi), b)?;
            }
            (Kind::Wrapped(a, ai), Kind::Wrapped(z, zi)) if a == z => {
                Self::push(&mut children, (*ai, *zi), b)?;
            }
            (Kind::Slice(a), Kind::Slice(z)) => Self::push(&mut children, (*a, *z), b)?,
            (
                Kind::Array {
                    element: a,
                    length: an,
                },
                Kind::Array {
                    element: z,
                    length: zn,
                },
            ) => {
                let (a, z, an, zn) = (*a, *z, *an, *zn);
                if self.same_original(an, zn, b, depth + 1)? {
                    Self::push(&mut children, (a, z), b)?;
                }
            }
            (Kind::Tuple(a), Kind::Tuple(z)) if a.len() == z.len() => {
                for pair in a.iter().copied().zip(z.iter().copied()) {
                    Self::push(&mut children, pair, b)?;
                }
            }
            (
                Kind::ErrorUnion {
                    error: ae,
                    result: ar,
                },
                Kind::ErrorUnion {
                    error: ze,
                    result: zr,
                },
            ) => {
                Self::push(&mut children, (*ae, *ze), b)?;
                Self::push(&mut children, (*ar, *zr), b)?;
            }
            // Different rigid heads and implicit union injections are checked
            // at their actual operation, never selected by a contextual guess.
            _ => {}
        }
        for (result, expected) in children {
            self.call_result_context(result, expected, b, depth + 1)?;
        }
        Ok(())
    }
    /// Read-only receiving adaptation changes the outer access of one use.
    /// The original expression sigma and all inner nominal classes remain
    /// intact; the actual reborrow and its parents are checked at emission.
    pub(super) fn shared_receiving_value(
        &mut self, frame: &mut Frame, expression: &SemanticOriginalExpr,
        actual: Term, expected: Term, b: &mut Budget, depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let (Kind::Address { mutable, life: source, inner },
            Kind::Address { mutable: false, life: target, inner: wanted }) =
            (&self.state.arena[self.type_root(actual, b)?], &self.state.arena[self.type_root(expected, b)?])
        else { return Ok(None); };
        let (mutable, source, inner, target, wanted) = (*mutable, *source, *inner, *target, *wanted);
        self.unify(inner, wanted, depth + 1, b)?;
        if !mutable && self.same_original(source, target, b, depth + 1)? {
            return Ok(None);
        }
        let root = self.root(target, b)?;
        if matches!(self.state.arena[root], Kind::LifeVariable(None)) {
            self.unify(target, source, depth + 1, b)?;
        } else if !self.call_source_allows(frame, source, target, b, depth + 1)? {
            // A saved shared pointer read through an actual borrowed input can
            // be received for that input's head lifetime. This is a directed
            // field view: neither its source sigma nor any child class changes.
            let Some(place) = self.place(frame, expression, false, b, depth + 1)? else {
                if mutable { return Err(fail("original shared receiving view exceeds its complete source lifetime bound")); }
                return Ok(None);
            };
            let Some(LocalAnchor::Parameter(parameter)) = place.anchor else { return Ok(None); };
            if mutable || place.path.is_empty()
                || !self.same_original(place.selected, actual, b, depth + 1)? {
                return Ok(None);
            }
            let Some(input) = frame.parameters.get(parameter as usize).copied() else {
                return Err(fail("original receiving field loses its genuine input"));
            };
            if !self.same_original(input, place.root, b, depth + 1)? { return Ok(None); }
            let Kind::Address { life: head, .. } = self.state.arena[self.type_root(input, b)?] else {
                return Ok(None);
            };
            if !self.same_original(head, target, b, depth + 1)? { return Ok(None); }
            let received = self.term(Kind::Address { mutable: false, life: target, inner }, b)?;
            b.lookup(self.state.receiving_places.len(), WORK)?;
            if self.state.receiving_places.insert((frame.definition, expression.id), ReceivingPlace {
                parameter, path: place.path, root: place.root, source: actual, target: received,
            }).is_some() {
                return Err(fail("original receiving field repeats its exact use occurrence"));
            }
            return Ok(Some(received));
        }
        if let (Kind::Life(Life::Implicit(source_owner,from)),Kind::Life(Life::Implicit(target_owner,to))) =
            (&self.state.arena[self.root(source,b)?],&self.state.arena[self.root(target,b)?])
            && *source_owner==frame.definition && *target_owner==frame.definition
            && self.inventory.definition(frame.definition,b)?.elided_result_ordinal()==Some(*to)
        {
            b.lookup(self.state.received_result_roles.len(),WORK)?;
            self.state.received_result_roles.insert((frame.definition,*from,*to));
        }
        Ok(Some(self.term(Kind::Address { mutable: false, life: target, inner }, b)?))
    }

    /// Source Name expressions denote the original address value. Reborrowing
    /// it for one call does not consume that reference or equate a new SSA head.
    pub(super) fn borrowed_argument_expected(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        expected: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        Self::push(
            &mut self.receiving,
            (frame.definition, value.id, expected),
            b,
        )?;
        let result = self.borrowed_argument(frame, value, b, depth);
        self.receiving
            .pop()
            .ok_or_else(|| fail("original borrowed argument loses its exact receiving frame"))?;
        result
    }
    pub(super) fn borrowed_argument(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } =
            &value.kind
        {
            let wanted = match &value.kind {
                CoreExprKind::Ascribed { ty, .. } => Some(self.recipe(frame, ty, b, depth + 1)?),
                _ => self.receiving_for(frame, value.id, b)?,
            };
            let term = match wanted {
                Some(expected) => {
                    self.borrowed_argument_expected(frame, inner, expected, b, depth + 1)?
                }
                None => self.borrowed_argument(frame, inner, b, depth + 1)?,
            };
            if let CoreExprKind::Ascribed { ty, .. } = &value.kind {
                let expected = self.recipe(frame, ty, b, depth + 1)?;
                let term = self.shared_receiving_value(frame, inner, term, expected, b, depth + 1)?.unwrap_or(term);
                self.unify(term, expected, depth + 1, b)?;
                b.lookup(frame.expressions.len(), WORK)?;
                if frame.expressions.insert(value.id, expected).is_some() {
                    return Err(fail("original reborrowed argument node is reused"));
                }
                return Ok(expected);
            }
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original reborrowed argument node is reused"));
            }
            return Ok(term);
        }
        if matches!(value.kind, CoreExprKind::Member { .. })
            && let Some(place) = self.place(frame, value, false, b, depth + 1)?
            && matches!(
                self.state.arena[self.type_root(place.selected, b)?],
                Kind::Address { .. }
            )
        {
            // A head stored in an inline field is observed in its real place.
            // Selecting it for an access is not a Copy or an owning move.
            let place = self
                .place(frame, value, true, b, depth + 1)?
                .ok_or_else(|| fail("original address observation loses its exact field place"))?;
            return Ok(place.selected);
        }
        if let CoreExprKind::Name(name) = &value.kind {
            b.spend(
                frame
                    .locals
                    .iter()
                    .map(|local| local.name.len().saturating_add(name.len() + 1))
                    .sum::<usize>(),
                WORK,
            )?;
            let index = frame
                .locals
                .iter()
                .rposition(|local| local.name == *name)
                .ok_or_else(|| fail("original borrowed argument has no lexical binder"))?;
            let local = &frame.locals[index];
            if !local.initialized {
                return Err(fail(
                    "original borrowed argument reads an uninitialized reference",
                ));
            }
            let term = local.ty;
            if local.used {
                Self::push(
                    &mut self.needs,
                    Need {
                        term,
                        predicate: Predicate::Copy,
                    },
                    b,
                )?;
            }
            if !matches!(
                self.state.arena[self.type_root(term, b)?],
                Kind::Address { .. }
            ) {
                return self.expression(frame, value, b, depth + 1);
            }
            self.record_environment_read(frame, index, value.id, b)?;
            if index < frame.inherited {
                b.lookup(frame.captured.len(), WORK)?;
                frame.captured.insert(index, term);
            }
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original borrowed argument node is reused"));
            }
            return Ok(term);
        }
        self.expression(frame, value, b, depth + 1)
    }
    pub(super) fn call_input_value(&mut self, frame: &mut Frame, value: &SemanticOriginalExpr,
        expected: Term, b: &mut Budget, depth: usize) -> CompileResult<Term> {
        // A result context can resolve a call input to the caller's declared
        // region before arguments are read. Check the explicit borrow against
        // that region and its real source upper before settling the call.
        let mut operand = value;
        while let CoreExprKind::Group(inner) = &operand.kind {
            b.spend(1, WORK)?;
            operand = inner;
        }
        let contextual = matches!(operand.kind, CoreExprKind::Unary {
            op: UnaryOp::Borrow | UnaryOp::BorrowMut, ..
        }) && matches!(self.state.arena[self.type_root(expected, b)?],
            Kind::Address { life, .. } if matches!(self.state.arena[self.root(life, b)?],
                Kind::Life(Life::Formal(owner, _)) if owner == frame.definition));
        let receiving = if contextual { expected } else { self.deferred_call_type(expected, b)? };
        self.call_argument(frame, value, receiving, b, depth + 1)?;
        b.lookup(frame.expressions.len(), WORK)?;
        let actual = frame.expressions.get(&value.id).copied()
            .ok_or_else(|| fail("original call argument has no checked expression occurrence"))?;
        self.prepare_call_value(frame.definition, value.id, actual, b)?;
        Ok(actual)
    }
    pub(super) fn call_argument(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        expected: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let expected_root = self.type_root(expected, b)?;
        if let Kind::Desc {
            life: wanted_life,
            inner: payload,
        } = self.state.arena[expected_root]
            && let Some(place) = self.place(frame, value, false, b, depth + 1)?
            && let Kind::Wrapped(Wrapper::Managed, actual_payload) =
                self.state.arena[self.type_root(place.selected, b)?]
        {
            // This one argument occurrence observes the owner as a weak
            // description. The lexical variable itself stays an owning value.
            self.unify(actual_payload, payload, depth + 1, b)?;
            let place = self
                .place(frame, value, true, b, depth + 1)?
                .ok_or_else(|| fail("original description loses its owning place"))?;
            let anchor = place.anchor.ok_or_else(|| {
                unknown("original description needs its complete owning source anchor")
            })?;
            let _ = anchor;
            let described = self.describe_argument_role(
                frame,
                value,
                &place,
                place.selected,
                Some(wanted_life),
                b,
                depth + 1,
            )?;
            return self.unify(described, expected, depth + 1, b);
        }
        let actual = if matches!(self.state.arena[expected_root], Kind::Address { .. }) {
            self.borrowed_argument_expected(frame, value, expected, b, depth + 1)?
        } else {
            self.expression_expected(frame, value, expected, b, depth + 1)?
        };
        self.call_argument_value(actual, expected, b, depth + 1)
    }
    pub(super) fn call_argument_value(&mut self, actual: Term, expected: Term,
        b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.step(depth, b)?;
        let expected_root = self.type_root(expected, b)?;
        let actual_root = self.type_root(actual, b)?;
        if self.static_byte_role(actual, expected, b, depth + 1)? {
            return Ok(());
        }
        if let Kind::ErrorUnion { error, result } = self.state.arena[expected_root]
            && !self.same_original(actual_root, expected_root, b, depth + 1)?
        {
            // A direct operand can select one exact original arm. Keep the
            // checked expression's own term and classes as the raw payload.
            let matches_error = self.same_original(actual_root, error, b, depth + 1)?;
            let matches_result = self.same_original(actual_root, result, b, depth + 1)?;
            match (matches_error, matches_result) {
                (true, false) => return self.unify(actual, error, depth + 1, b),
                (false, true) => return self.unify(actual, result, depth + 1, b),
                (true, true) => return Err(fail("implicit original call injection matches both error alternatives")),
                (false, false) => {}
            }
        }
        let actual = match (
            &self.state.arena[expected_root],
            &self.state.arena[actual_root],
        ) {
            (
                Kind::Address { mutable: false, .. },
                Kind::Address {
                    mutable: true,
                    life,
                    inner,
                },
            ) => {
                let (life, inner) = (*life, *inner);
                self.term(
                    Kind::Address {
                        mutable: false,
                        life,
                        inner,
                    },
                    b,
                )?
            }
            _ => actual,
        };
        self.unify(actual, expected, depth + 1, b)
    }
}

impl Checked {
    pub(in crate::semantic) fn receiving_field(
        &self, context: &SemanticInstance, node: NodeId, program: &SemanticModule,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<Option<(u32, Vec<u32>, crate::semantic::value::Value,
        crate::semantic::value::Value, crate::semantic::value::Value)>> {
        b.lookup(self.receiving_places.len(), WORK)?;
        let Some(row) = self.receiving_places.get(&(context.definition, node)) else { return Ok(None); };
        b.spend(row.path.len(), WORK)?;
        Ok(Some((row.parameter, row.path.clone(),
            self.value(row.root, context, program, inventory, b, 0)?,
            self.value(row.source, context, program, inventory, b, 0)?,
            self.value(row.target, context, program, inventory, b, 0)?)))
    }
}
