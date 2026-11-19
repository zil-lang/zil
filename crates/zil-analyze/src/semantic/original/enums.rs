//! Resolve constructor and pattern membership through complete original enum
//! declarations. Names or a selected integer tag alone supply no membership.
use super::*;

pub(super) struct Constructor {
    pub(super) value: Term,
    pub(super) value_member: bool,
    pub(super) variant: u32,
    pub(super) arguments: Vec<Term>,
    pub(super) payload: Vec<Term>,
}
impl Solver<'_, '_> {
    pub(super) fn pattern_reference_mode(
        &self, pattern: &SemanticOriginalPattern, b: &mut Budget, depth: usize,
    ) -> CompileResult<Option<bool>> {
        original_pattern_reference_mode(pattern, depth, &mut |depth| self.step(depth, b))
    }

    pub(super) fn tuple_binding(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        subject: Term,
        source: &pattern_places::PatternSubject,
        mutable: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let mut stack = Vec::new();
        Self::push(&mut stack, pattern, b)?;
        while let Some(pattern) = stack.pop() {
            self.step(depth + 1, b)?;
            match &pattern.kind {
                CorePatternKind::Name(_) | CorePatternKind::Wildcard => {}
                CorePatternKind::Tuple(fields) => {
                    for field in fields {
                        Self::push(&mut stack, field, b)?;
                    }
                }
                _ => {
                    return Err(unknown(
                        "original binding pattern requires its complete refutability and branch relation",
                    ));
                }
            }
        }
        let start = frame.locals.len();
        if !self.pattern_bind(
            frame,
            statement,
            &mut Vec::new(),
            pattern,
            subject,
            source,
            &mut Vec::new(),
            b,
            depth + 1,
        )? {
            return Err(fail("original tuple binding is not irrefutable"));
        }
        for local in &mut frame.locals[start..] {
            b.spend(1, WORK)?;
            local.mutable = mutable;
        }
        Ok(())
    }
    fn enum_variant(
        &self,
        value: Term,
        name: &str,
        b: &mut Budget,
    ) -> CompileResult<(u32, Vec<Term>)> {
        let Kind::Nominal {
            definition, fields, ..
        } = &self.state.arena[self.type_root(value, b)?]
        else {
            return Err(fail(
                "original variant operation requires a nominal enum subject",
            ));
        };
        let declaration = self.inventory.definition(*definition, b)?;
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Enum,
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &declaration.body
        else {
            return Err(fail(
                "original variant selects another nominal declaration family",
            ));
        };
        let mut start = 0usize;
        let mut selected = None;
        for (index, variant) in variants.iter().enumerate() {
            b.spend(
                variant
                    .name
                    .len()
                    .saturating_add(name.len() + variant.fields.len() + 1),
                WORK,
            )?;
            let end = start
                .checked_add(variant.fields.len())
                .ok_or_else(|| resource("original variant field count overflow"))?;
            if variant.name == name {
                if selected.is_some() {
                    return Err(fail("original enum repeats a variant identity"));
                }
                let mut payload = Vec::new();
                for field in fields
                    .get(start..end)
                    .ok_or_else(|| fail("original enum omits complete variant fields"))?
                {
                    Self::push(&mut payload, *field, b)?;
                }
                selected = Some((
                    u32::try_from(index)
                        .map_err(|_| resource("original variant ordinal overflow"))?,
                    payload,
                ));
            }
            start = end;
        }
        if start != fields.len() {
            return Err(fail("original enum adds unowned variant fields"));
        }
        selected.ok_or_else(|| fail("original constructor or pattern names no actual enum member"))
    }

    pub(super) fn enum_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        callee: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let CoreExprKind::Member {
            object,
            member,
            owner: None,
        } = &callee.kind
        else {
            return Ok(None);
        };
        let Some(path) = sealed::path(object, b, depth + 1)? else {
            return Ok(None);
        };
        if self.local(frame, path[0], b)?.is_some() {
            return Ok(None);
        }
        let names = self.inventory.original_index(frame.definition, b)?;
        if names.name(SemanticOriginalNamespace::Value, &path[..1], b)?.is_some() {
            return Ok(None);
        }
        let Some(selected) = names.name(SemanticOriginalNamespace::Type, &path, b)? else {
            return Ok(None);
        };
        let value = match selected {
            SemanticOriginalNameValue::Definition(id) => {
                let definition = self.inventory.definition(*id, b)?;
                if !matches!(
                    definition.body,
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Enum,
                        ..
                    }
                ) {
                    return Ok(None);
                }
                let mut actuals = Vec::new();
                let (types, written) = match &object.kind {
                    CoreExprKind::TypeApply { type_arguments, .. } => {
                        (Some(type_arguments.as_slice()), None)
                    }
                    CoreExprKind::GenericApply { arguments, .. } => {
                        (None, Some(arguments.as_slice()))
                    }
                    _ => (None, None),
                };
                let count = types
                    .map(|rows| rows.len())
                    .or_else(|| written.map(|rows| rows.len()));
                if count.is_some_and(|count| count != definition.parameters.len()) {
                    return Err(fail(
                        "original explicit enum actuals change the full mixed arity",
                    ));
                }
                for (ordinal, formal) in definition.parameters.iter().enumerate() {
                    let actual = match (
                        formal,
                        types.and_then(|rows| rows.get(ordinal)),
                        written.and_then(|rows| rows.get(ordinal)),
                    ) {
                        (SemanticFormal::Type { .. }, Some(ty), None)
                        | (
                            SemanticFormal::Type { .. },
                            None,
                            Some(CoreGenericArgument::Type(ty)),
                        ) => self.recipe(frame, ty, b, depth + 1)?,
                        (
                            SemanticFormal::Lifetime,
                            None,
                            Some(CoreGenericArgument::Lifetime(name)),
                        ) => self.named_life(frame, name, b, depth + 1)?,
                        (
                            SemanticFormal::Const { carrier },
                            None,
                            Some(CoreGenericArgument::Const(value)),
                        ) => {
                            b.spend(actuals.len(), WORK)?;
                            let context = nominal::frame(*id, actuals.clone());
                            let carrier = self.recipe(&context, carrier, b, depth + 1)?;
                            let CoreExprKind::Integer(text) = &value.kind else {
                                return Err(unknown(
                                    "enum Const actual needs its complete original constant expression",
                                ));
                            };
                            b.spend(text.len(), WORK)?;
                            self.constant(
                                frame,
                                &SemanticConstRecipe::IntegerLiteral(text.clone()),
                                carrier,
                                b,
                                depth + 1,
                            )?
                        }
                        (SemanticFormal::Type { .. }, None, None) if count.is_none() => {
                            self.var(b)?
                        }
                        (SemanticFormal::Lifetime, None, None) if count.is_none() => {
                            self.term(Kind::LifeVariable(None), b)?
                        }
                        (SemanticFormal::Const { .. }, None, None) if count.is_none() => {
                            return Err(unknown(
                                "original enum constructor needs its complete const actual inference",
                            ));
                        }
                        _ => {
                            return Err(fail(
                                "original explicit enum actual changes its mixed formal kind",
                            ));
                        }
                    };
                    Self::push(&mut actuals, actual, b)?;
                }
                self.application(*id, actuals, b, depth + 1)?
            }
            SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(recipe)) => {
                self.recipe(frame, recipe, b, depth + 1)?
            }
            _ => {
                return Err(fail(
                    "original enum constructor owner has another semantic kind",
                ));
            }
        };
        // A type namespace also contains abstract Type formals. A static
        // trait method on such a formal is not an enum constructor.
        let Kind::Nominal { definition, .. } = &self.state.arena[self.type_root(value, b)?] else {
            return Ok(None);
        };
        if !matches!(
            self.inventory.definition(*definition, b)?.body,
            SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Enum,
                ..
            }
        ) {
            return Ok(None);
        }
        let declaration = *definition;
        if let Some(wanted) = self.receiving_for(frame, node, b)? {
            let head = self.type_root(wanted, b)?;
            let mut matches = |term| -> CompileResult<bool> {
                Ok(matches!(self.state.arena[self.type_root(term, b)?],
                    Kind::Nominal { definition, .. } if definition == declaration))
            };
            let contextual = match self.state.arena[head] {
                Kind::Nominal { definition, .. } if definition == declaration => Some(wanted),
                Kind::ErrorUnion { error, result } => match (matches(error)?, matches(result)?) {
                    (true, false) => Some(error),
                    (false, true) => Some(result),
                    _ => None,
                },
                _ => None,
            };
            if let Some(contextual) = contextual {
                self.unify(value, contextual, depth + 1, b)?;
            }
        }
        let (variant, mut payload) = self.enum_variant(value, member, b)?;
        if payload.len() != arguments.len() {
            return Err(fail(
                "original enum constructor changes its declared member arity",
            ));
        }
        for (argument, expected) in arguments.iter().zip(&mut payload) {
            let actual = self.expression_expected(frame, argument, *expected, b, depth + 1)?;
            b.lookup(frame.expressions.len(), WORK)?;
            let source = frame.expressions.get(&argument.id).copied().unwrap_or(actual);
            let actual_head = self.type_root(source, b)?;
            let target_head = self.type_root(*expected, b)?;
            let loan = match (&self.state.arena[actual_head], &self.state.arena[target_head]) {
                (Kind::Address { mutable: false, life: from, inner },
                 Kind::Address { mutable: false, life: to, inner: target }) => {
                    Some((*from, *to, *inner, *target))
                }
                _ => None,
            };
            if let Some((from, to, inner, target)) = loan
                && self.view_upper_allows(frame, from, to, &mut BTreeSet::new(), b, depth + 1)?
            {
                self.unify(inner, target, depth + 1, b)?;
                // The operand keeps its own source class. The nominal result keeps
                // the declared field class proved at this construction boundary.
                *expected = source;
            } else {
                self.unify(actual, *expected, depth + 1, b)?;
            }
            self.prepare_call_value(frame.definition, argument.id, actual, b)?;
        }
        let Kind::Nominal {
            arguments: actuals, ..
        } = &self.state.arena[self.root(value, b)?]
        else {
            return Err(fail("original enum constructor lost its nominal value"));
        };
        b.spend(actuals.len(), WORK)?;
        let actuals = actuals.clone();
        b.lookup(self.state.enum_constructors.len(), WORK)?;
        if self
            .state
            .enum_constructors
            .insert(
                (frame.definition, node),
                Constructor {
                    value,
                    value_member: node == callee.id,
                    variant,
                    arguments: actuals,
                    payload,
                },
            )
            .is_some()
        {
            return Err(fail(
                "original enum constructor has competing member resolutions",
            ));
        }
        Ok(Some(value))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_bind(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        path: &mut Vec<u32>,
        pattern: &SemanticOriginalPattern,
        subject: Term,
        source: &pattern_places::PatternSubject,
        physical: &mut Vec<PlaceProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        b.spend(path.len().saturating_add(1), WORK)?;
        let key = (frame.definition, statement, path.clone());
        b.lookup(self.state.patterns.len(), WORK)?;
        if self.state.patterns.insert(key, subject).is_some() {
            return Err(fail("original binding pattern repeats an occurrence path"));
        }
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                if source.retained.is_some() && source.anchor.is_some() && frame.alive
                    && !self.satisfies(subject, Predicate::Copy, b, depth + 1)?
                {
                    let selected = self.pattern_vacancy(source, physical, b)?;
                    self.place_available(frame, source.anchor, &selected, false, b)?;
                    self.retire_lexical_place(frame, source.anchor, &selected, b)?;
                }
                self.bind(frame, name, subject, false, b)?;
                let anchor = self.pattern_anchor(frame.definition, source, physical, None, b)?;
                frame
                    .locals
                    .last_mut()
                    .ok_or_else(|| fail("pattern binding lost its original local"))?
                    .anchor = Some(anchor);
                Ok(true)
            }
            CorePatternKind::Wildcard => {
                if source.retained.is_none() { Self::push(
                    &mut self.needs,
                    Need {
                        term: subject,
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?; }
                Ok(true)
            }
            CorePatternKind::Reference { mutable, pattern: inner } => {
                if !matches!(inner.kind, CorePatternKind::Name(_) | CorePatternKind::Wildcard) {
                    return Err(fail("original reference pattern requires a name or discard"));
                }
                if *mutable && matches!(self.state.arena[self.type_root(subject, b)?], Kind::Wrapped(Wrapper::Managed, _)) {
                    return Err(fail("ordinary original resource head reference must remain shared"));
                }
                let selected = self.pattern_vacancy(source, physical, b)?;
                self.place_available(frame, source.anchor, &selected, false, b)?;
                b.spend(path.len().saturating_add(2), WORK)?;
                let mut occurrence = vec![ORIGINAL_PATTERN_BORROW, statement.0];
                occurrence.extend_from_slice(path);
                let ordinal = self.inventory.implicit_ordinal(frame.definition, &occurrence, b)?
                    .ok_or_else(|| fail("original reference pattern lacks its precise lifetime occurrence"))?;
                let life = self.term(Kind::Life(Life::Implicit(frame.definition, ordinal)), b)?;
                let address = self.term(Kind::Address { mutable: *mutable, life, inner: subject }, b)?;
                if let CorePatternKind::Name(name) = &inner.kind {
                    Self::push(path, 0, b)?;
                    b.spend(path.len(), WORK)?;
                    let key = (frame.definition, statement, path.clone());
                    b.lookup(self.state.patterns.len(), WORK)?;
                    if self.state.patterns.insert(key, address).is_some() {
                        return Err(fail("original reference pattern repeats its binding path"));
                    }
                    path.pop();
                    self.bind(frame, name, address, false, b)?;
                    let anchor = self.pattern_anchor(frame.definition, source, physical,
                        Some(pattern_places::PatternBorrow { mutable: *mutable, ordinal, span: pattern.span }), b)?;
                    frame.locals.last_mut().ok_or_else(|| fail("original reference pattern loses its binder"))?.anchor = Some(anchor);
                }
                Ok(true)
            }
            CorePatternKind::Tuple(patterns) => {
                if patterns.is_empty()
                    && matches!(
                        self.state.arena[self.type_root(subject, b)?],
                        Kind::Atom(Type::Unit)
                    )
                {
                    return Ok(true);
                }
                let Kind::Tuple(fields) = &self.state.arena[self.type_root(subject, b)?] else {
                    return Err(fail(
                        "original tuple pattern has another subject constructor",
                    ));
                };
                b.spend(fields.len(), WORK)?;
                let fields = fields.clone();
                if fields.len() != patterns.len() {
                    return Err(fail("original tuple pattern changes its complete arity"));
                }
                let mut irrefutable = true;
                for (index, (pattern, field)) in patterns.iter().zip(fields).enumerate() {
                    Self::push(path, index as u32, b)?;
                    Self::push(physical, PlaceProjection::Field(index as u32), b)?;
                    irrefutable &= self.pattern_bind(
                        frame,
                        statement,
                        path,
                        pattern,
                        field,
                        source,
                        physical,
                        b,
                        depth + 1,
                    )?;
                    physical.pop();
                    path.pop();
                }
                Ok(irrefutable)
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let Kind::ErrorUnion { error, result } =
                    self.state.arena[self.type_root(subject, b)?]
                else {
                    return Err(fail(
                        "original error/success pattern has another complete subject family",
                    ));
                };
                Self::push(path, 0, b)?;
                Self::push(physical, PlaceProjection::ErrorArm(*is_error), b)?;
                let result = self.pattern_bind(
                    frame,
                    statement,
                    path,
                    pattern,
                    if *is_error { error } else { result },
                    source,
                    physical,
                    b,
                    depth + 1,
                );
                path.pop();
                physical.pop();
                result
            }
            CorePatternKind::Constructor {
                definition: _,
                variant,
                fields,
            }
            | CorePatternKind::Variant {
                ty: _,
                variant,
                fields,
            } => {
                if let CorePatternKind::Variant { ty, .. } = &pattern.kind {
                    let expected = self.recipe(frame, ty, b, depth + 1)?;
                    self.unify(subject, expected, depth + 1, b)?;
                }
                if let CorePatternKind::Constructor {
                    definition: Some(expected),
                    ..
                } = &pattern.kind
                {
                    if !matches!(self.state.arena[self.type_root(subject,b)?], Kind::Nominal {definition,..} if definition == *expected)
                    {
                        return Err(fail(
                            "original pattern selects another enum declaration identity",
                        ));
                    }
                }
                let (variant, payload) = self.enum_variant(subject, variant, b)?;
                if fields.len() != payload.len() {
                    return Err(fail("original enum pattern changes its exact member arity"));
                }
                let mut irrefutable = true;
                for (index, (pattern, field)) in fields.iter().zip(payload).enumerate() {
                    Self::push(path, index as u32, b)?;
                    Self::push(
                        physical,
                        PlaceProjection::Variant(VariantProjection {
                            variant,
                            index: index as u32,
                        }),
                        b,
                    )?;
                    irrefutable &= self.pattern_bind(
                        frame,
                        statement,
                        path,
                        pattern,
                        field,
                        source,
                        physical,
                        b,
                        depth + 1,
                    )?;
                    physical.pop();
                    path.pop();
                }
                // This is irrefutable *within this variant*, used only by the
                // complete declaration's coverage check below.
                Ok(irrefutable)
            }
            CorePatternKind::Bool(_) => {
                let ty = self.atom(Type::Bool, b)?;
                self.unify(subject, ty, depth + 1, b)?;
                Ok(false)
            }
            CorePatternKind::Integer(text) => {
                let ty = self.scalar_literal(frame.definition, text, false, b)?;
                self.unify(subject, ty, depth + 1, b)?;
                Ok(false)
            }
            _ => Err(unknown(
                "original pattern needs its complete mode, place and field occurrence proof",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn if_let_statement(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        value: &SemanticOriginalExpr,
        then_body: &SemanticOriginalBlock,
        else_body: Option<&SemanticOriginalBlock>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let mode = self.pattern_reference_mode(pattern, b, depth + 1)?;
        let (subject, source) = self.observed_pattern_subject(frame, value, mode, b, depth + 1)?;
        let entered = frame.alive;
        let before = self.copy_locals(&frame.locals, b)?;
        let prefix = before.len();
        self.pattern_bind(
            frame,
            statement,
            &mut Vec::new(),
            pattern,
            subject,
            &source,
            &mut Vec::new(),
            b,
            depth + 1,
        )?;
        b.spend(2, WORK)?;
        let irrefutable = self.covers_patterns(&[vec![Some(pattern)]], &[subject], b, depth + 1)?;
        self.block(frame, then_body, b, depth + 1)?;
        if frame.alive {
            self.cleanup(frame.definition, &frame.locals[prefix..], b)?;
        }
        frame.locals.truncate(prefix);
        let then_alive = frame.alive;
        let then_locals = std::mem::replace(&mut frame.locals, before);
        frame.alive = entered;
        // A failed pattern still owns the complete unmatched subject. Until
        // a narrower variant cleanup proof exists, require its full SyncDrop.
        if !irrefutable && source.retained.is_none() {
            Self::push(
                &mut self.needs,
                Need {
                    term: subject,
                    predicate: Predicate::SyncDrop,
                },
                b,
            )?;
        }
        if let Some(body) = else_body {
            self.block(frame, body, b, depth + 1)?;
        }
        let else_alive = frame.alive && !irrefutable;
        if then_locals.len() != frame.locals.len() {
            return Err(fail("original if-let changes enclosing lexical inventory"));
        }
        for (then, other) in then_locals.iter().zip(&mut frame.locals) {
            b.spend(1, WORK)?;
            match (then_alive, else_alive) {
                (true, true) => self.join_vacancies(other, then, b)?,
                (true, false) => {
                    other.vacant = self.copy_vacancies(&then.vacant, b)?;
                    other.maybe_vacant = self.copy_vacancies(&then.maybe_vacant, b)?;
                    other.live = then.live;
                }
                _ => {}
            }
            other.used = match (then_alive, else_alive) {
                (true, true) => then.used || other.used,
                (true, false) => then.used,
                _ => other.used,
            };
            other.initialized = match (then_alive, else_alive) {
                (true, true) => then.initialized && other.initialized,
                (true, false) => then.initialized,
                _ => other.initialized,
            };
        }
        frame.alive = then_alive || else_alive;
        Ok(())
    }

    fn covers_patterns(
        &self,
        rows: &[OriginalPatternRow<'_>],
        columns: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        b.spend(
            rows.len().saturating_mul(columns.len().saturating_add(1)),
            WORK,
        )?;
        if rows.is_empty() {
            return Ok(false);
        }
        if rows
            .iter()
            .any(|row| row.iter().copied().all(original_pattern_any))
        {
            return Ok(true);
        }
        let Some((subject, tail)) = columns.split_first() else {
            return Err(fail(
                "original pattern matrix has inconsistent column arity",
            ));
        };
        if rows.iter().all(|row| original_pattern_any(row[0])) {
            return self.pattern_branch(rows, OriginalPatternHead::Default, &[], tail, b, depth);
        }
        match &self.state.arena[self.type_root(*subject, b)?] {
            Kind::Atom(Type::Bool) => {
                for value in [false, true] {
                    if !self.pattern_branch(
                        rows,
                        OriginalPatternHead::Bool(value),
                        &[],
                        tail,
                        b,
                        depth,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Atom(Type::Unit) => {
                self.pattern_branch(rows, OriginalPatternHead::Tuple, &[], tail, b, depth)
            }
            Kind::Tuple(fields) => {
                self.pattern_branch(rows, OriginalPatternHead::Tuple, fields, tail, b, depth)
            }
            Kind::ErrorUnion { error, result } => {
                for (is_error, payload) in [(false, result), (true, error)] {
                    if !self.pattern_branch(
                        rows,
                        OriginalPatternHead::ErrorArm(is_error),
                        std::slice::from_ref(payload),
                        tail,
                        b,
                        depth,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Nominal {
                definition, fields, ..
            } => {
                let declaration = self.inventory.definition(*definition, b)?;
                let SemanticDefinitionBody::Nominal {
                    kind: LifeNominalKind::Enum,
                    body: SemanticNominalBody::Enum { variants },
                    ..
                } = &declaration.body
                else {
                    return self.pattern_branch(
                        rows,
                        OriginalPatternHead::Default,
                        &[],
                        tail,
                        b,
                        depth,
                    );
                };
                let mut remaining = fields.as_slice();
                for variant in variants {
                    b.spend(1, WORK)?;
                    let Some((payload, rest)) = remaining.split_at_checked(variant.fields.len())
                    else {
                        return Err(fail(
                            "original enum coverage lacks a complete variant payload",
                        ));
                    };
                    remaining = rest;
                    if !self.pattern_branch(
                        rows,
                        OriginalPatternHead::Variant(&variant.name),
                        payload,
                        tail,
                        b,
                        depth,
                    )? {
                        return Ok(false);
                    }
                }
                if !remaining.is_empty() {
                    return Err(fail(
                        "original enum coverage has extra unowned variant payloads",
                    ));
                }
                Ok(!variants.is_empty())
            }
            // Unknown and open scalar type domains supply no finite literal set.
            _ => self.pattern_branch(rows, OriginalPatternHead::Default, &[], tail, b, depth),
        }
    }

    fn pattern_branch(
        &self,
        rows: &[OriginalPatternRow<'_>],
        head: OriginalPatternHead<'_>,
        fields: &[Term],
        tail: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        let width = fields.len().saturating_add(tail.len());
        let mut work = width.saturating_add(rows.len().saturating_mul(width.saturating_add(1)));
        if let OriginalPatternHead::Variant(expected) = head {
            for row in rows {
                if let Some(
                    CorePatternKind::Variant { variant, .. }
                    | CorePatternKind::Constructor { variant, .. },
                ) = row[0].map(|pattern| &pattern.kind)
                {
                    work = work.saturating_add(expected.len().saturating_add(variant.len()));
                }
            }
        }
        b.spend(work, WORK)?;
        let rows = specialize_original_patterns(rows, head, fields.len());
        let columns = fields.iter().chain(tail).copied().collect::<Vec<_>>();
        self.covers_patterns(&rows, &columns, b, depth + 1)
    }

    pub(super) fn match_statement(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        value: &SemanticOriginalExpr,
        arms: &[SemanticOriginalMatchArm],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let mut mode = None;
        for arm in arms {
            if let Some(mutable) = self.pattern_reference_mode(&arm.pattern, b, depth + 1)? {
                mode = Some(mode.unwrap_or(false) || mutable);
            }
        }
        let (subject, source) = self.observed_pattern_subject(frame, value, mode, b, depth + 1)?;
        let original_alive = frame.alive;
        let base = self.copy_locals(&frame.locals, b)?;
        let base_len = base.len();
        let mut joined: Option<Vec<Local>> = None;
        for (arm_index, arm) in arms.iter().enumerate() {
            frame.locals = self.copy_locals(&base, b)?;
            frame.alive = original_alive;
            let mut path = vec![
                u32::try_from(arm_index)
                    .map_err(|_| resource("original match arm ordinal overflow"))?,
            ];
            self.pattern_bind(
                frame,
                statement,
                &mut path,
                &arm.pattern,
                subject,
                &source,
                &mut Vec::new(),
                b,
                depth + 1,
            )?;
            self.block(frame, &arm.body, b, depth + 1)?;
            if frame.alive {
                self.cleanup(frame.definition, &frame.locals[base_len..], b)?;
                frame.locals.truncate(base_len);
                if let Some(previous) = &mut joined {
                    if previous.len() != frame.locals.len() {
                        return Err(fail("original match changes enclosing lexical bindings"));
                    }
                    for (old, new) in previous.iter_mut().zip(&frame.locals) {
                        b.spend(1, WORK)?;
                        self.join_vacancies(old, new, b)?;
                        old.used |= new.used;
                        old.initialized &= new.initialized;
                    }
                } else {
                    joined = Some(self.copy_locals(&frame.locals, b)?);
                }
            }
        }
        b.spend(arms.len(), WORK)?;
        if !arms
            .iter()
            .any(|arm| original_pattern_any(Some(&arm.pattern)))
        {
            b.spend(arms.len().saturating_mul(2), WORK)?;
            let rows = arms
                .iter()
                .map(|arm| vec![Some(&arm.pattern)])
                .collect::<Vec<_>>();
            if !self.covers_patterns(&rows, &[subject], b, depth + 1)? {
                return Err(fail(
                    "original match omits a possible symbolic constructor or payload",
                ));
            }
        }
        frame.alive = joined.is_some();
        frame.locals = joined.unwrap_or(base);
        Ok(())
    }
}

pub(in crate::semantic) struct ActualConstructor {
    pub(in crate::semantic) node: NodeId,
    pub(in crate::semantic) result: crate::semantic::value::Value,
    pub(in crate::semantic) value_member: bool,
    pub(in crate::semantic) variant: u32,
    pub(in crate::semantic) arguments: Vec<crate::semantic::value::Argument>,
    pub(in crate::semantic) payload: Vec<crate::semantic::value::Value>,
}
impl Checked {
    pub(in crate::semantic) fn frozen_enum(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<Option<(SemanticDefinitionId, u32)>> {
        b.lookup(self.enum_constructors.len(), WORK)?;
        let Some(constructor) = self.enum_constructors.get(&(definition, node)) else {
            return Ok(None);
        };
        let Kind::Nominal { definition, arguments, .. } = &self.arena[self.root(constructor.value, b)?] else {
            return Err(fail("original CT constructor has no complete nominal result"));
        };
        Ok(arguments.is_empty().then_some((*definition, constructor.variant)))
    }

    pub(in crate::semantic) fn enum_constructor(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<ActualConstructor>> {
        let selected = inventory.original_index(context.definition, b)?.site_node(site, b)?
            .ok_or_else(|| fail("actual variant has no original node anchor"))?;
        let SemanticOriginalNode::Expression(node) = selected else {
            return Err(fail("actual variant selects an original statement"));
        };
        b.lookup(self.enum_constructors.len(), WORK)?;
        let Some(constructor) = self.enum_constructors.get(&(context.definition, node)) else {
            return Ok(None);
        };
        let result = self.value(constructor.value, context, program, inventory, b, 0)?;
        let mut arguments = Vec::new();
        for term in &constructor.arguments {
            let argument = self.argument(*term, context, program, inventory, b, 0)?;
            Solver::push(&mut arguments, argument, b)?;
        }
        let mut payload = Vec::new();
        for term in &constructor.payload {
            let value = self.value(*term, context, program, inventory, b, 0)?;
            Solver::push(&mut payload, value, b)?;
        }
        Ok(Some(ActualConstructor {
            node,
            result,
            value_member: constructor.value_member,
            variant: constructor.variant,
            arguments,
            payload,
        }))
    }
}
