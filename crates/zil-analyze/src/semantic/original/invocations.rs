//! Each original call opens its own Invocation namespace. This substitution
//! never rewrites a declaration, an intrinsic child class or a retained life.
use super::*;
impl Solver<'_, '_> {
    pub(super) fn invocation_actuals(
        &mut self,
        count: u32,
        b: &mut Budget,
    ) -> CompileResult<Vec<Term>> {
        b.spend(count as usize, WORK)?;
        let mut actuals = Vec::new();
        for _ in 0..count {
            let life = self.term(Kind::LifeVariable(None), b)?;
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
                self.term(
                    Kind::Code {
                        kind,
                        parameters: params,
                        result,
                        modes,
                        result_mode,
                        call_kind,
                    },
                    b,
                )
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
    /// Source Name expressions denote the original address value. Reborrowing
    /// it for one call does not consume that reference or equate a new SSA head.
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
            let term = self.borrowed_argument(frame, inner, b, depth + 1)?;
            if let CoreExprKind::Ascribed { ty, .. } = &value.kind {
                let expected = self.recipe(frame, ty, b, depth + 1)?;
                self.unify(term, expected, depth + 1, b)?;
            }
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original reborrowed argument node is reused"));
            }
            return Ok(term);
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
        let actual = if matches!(self.state.arena[expected_root], Kind::Address { .. }) {
            self.borrowed_argument(frame, value, b, depth + 1)?
        } else {
            self.expression(frame, value, b, depth + 1)?
        };
        let actual_root = self.type_root(actual, b)?;
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
