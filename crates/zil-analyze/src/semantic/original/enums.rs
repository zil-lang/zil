//! Resolve constructor and pattern membership through complete original enum
//! declarations. Names or a selected integer tag alone supply no membership.
use super::*;

pub(super) struct Constructor {
    pub(super) value: Term,
    pub(super) variant: u32,
    pub(super) arguments: Vec<Term>,
    pub(super) payload: Vec<Term>,
}
impl Solver<'_, '_> {
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
        let declaration = self.inventory.definition(frame.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &declaration.body else {
            return Err(fail(
                "original enum constructor lacks its lexical name inventory",
            ));
        };
        let mut selected = None;
        for binding in names {
            b.spend(
                binding
                    .path
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(path.iter().map(|s| s.len()).sum::<usize>() + 1),
                WORK,
            )?;
            if binding.namespace == SemanticOriginalNamespace::Value
                && binding.path.len() == 1
                && binding.path[0] == path[0]
            {
                return Ok(None);
            }
            if binding.namespace != SemanticOriginalNamespace::Type
                || binding.path.len() != path.len()
                || !binding.path.iter().zip(&path).all(|(a, b)| a == b)
            {
                continue;
            }
            if selected.replace(&binding.value).is_some() {
                return Err(fail("original enum owner has competing type bindings"));
            }
        }
        let Some(selected) = selected else {
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
                for formal in &definition.parameters {
                    let actual = match formal {
                        SemanticFormal::Type { .. } => self.var(b)?,
                        SemanticFormal::Lifetime => self.term(Kind::LifeVariable(None), b)?,
                        SemanticFormal::Const { .. } => {
                            return Err(unknown(
                                "original enum constructor needs its complete const actual inference",
                            ));
                        }
                    };
                    Self::push(&mut actuals, actual, b)?;
                }
                if matches!(
                    object.kind,
                    CoreExprKind::GenericApply { .. } | CoreExprKind::TypeApply { .. }
                ) {
                    return Err(unknown(
                        "original explicit enum actuals require their full mixed substitution",
                    ));
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
        let (variant, payload) = self.enum_variant(value, member, b)?;
        if payload.len() != arguments.len() {
            return Err(fail(
                "original enum constructor changes its declared member arity",
            ));
        }
        for (argument, expected) in arguments.iter().zip(&payload) {
            let actual = self.expression(frame, argument, b, depth + 1)?;
            self.unify(actual, *expected, depth + 1, b)?;
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
    fn pattern_bind(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        path: &mut Vec<u32>,
        pattern: &SemanticOriginalPattern,
        subject: Term,
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
                self.bind(frame, name, subject, false, b)?;
                Ok(true)
            }
            CorePatternKind::Wildcard => {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: subject,
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?;
                Ok(true)
            }
            CorePatternKind::Tuple(patterns) => {
                let Kind::Tuple(fields) = &self.state.arena[self.root(subject, b)?] else {
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
                    irrefutable &=
                        self.pattern_bind(frame, statement, path, pattern, field, b, depth + 1)?;
                    path.pop();
                }
                Ok(irrefutable)
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
                    if !matches!(self.state.arena[self.root(subject,b)?], Kind::Nominal {definition,..} if definition == *expected)
                    {
                        return Err(fail(
                            "original pattern selects another enum declaration identity",
                        ));
                    }
                }
                let (_, payload) = self.enum_variant(subject, variant, b)?;
                if fields.len() != payload.len() {
                    return Err(fail("original enum pattern changes its exact member arity"));
                }
                let mut irrefutable = true;
                for (index, (pattern, field)) in fields.iter().zip(payload).enumerate() {
                    Self::push(path, index as u32, b)?;
                    irrefutable &=
                        self.pattern_bind(frame, statement, path, pattern, field, b, depth + 1)?;
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
                let ty = self.scalar_literal(text, false, b)?;
                self.unify(subject, ty, depth + 1, b)?;
                Ok(false)
            }
            _ => Err(unknown(
                "original pattern needs its complete mode, place and field occurrence proof",
            )),
        }
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
        let subject = self.expression(frame, value, b, depth + 1)?;
        let original_alive = frame.alive;
        let base = self.copy_locals(&frame.locals, b)?;
        let base_len = base.len();
        let mut joined: Option<Vec<Local>> = None;
        let mut coverage = BTreeSet::new();
        let mut total = false;
        let mut bools = BTreeSet::new();
        for (arm_index, arm) in arms.iter().enumerate() {
            frame.locals = self.copy_locals(&base, b)?;
            frame.alive = original_alive;
            let mut path = vec![
                u32::try_from(arm_index)
                    .map_err(|_| resource("original match arm ordinal overflow"))?,
            ];
            let irrefutable = self.pattern_bind(
                frame,
                statement,
                &mut path,
                &arm.pattern,
                subject,
                b,
                depth + 1,
            )?;
            match &arm.pattern.kind {
                CorePatternKind::Name(_) | CorePatternKind::Wildcard => total = true,
                CorePatternKind::Tuple(_) if irrefutable => total = true,
                CorePatternKind::Constructor { variant, .. }
                | CorePatternKind::Variant { variant, .. }
                    if irrefutable =>
                {
                    let (index, _) = self.enum_variant(subject, variant, b)?;
                    b.lookup(coverage.len(), WORK)?;
                    coverage.insert(index);
                }
                CorePatternKind::Bool(value) => {
                    bools.insert(*value);
                }
                _ => {}
            }
            self.block(frame, &arm.body, b, depth + 1)?;
            if frame.alive {
                self.cleanup(&frame.locals[base_len..], b)?;
                frame.locals.truncate(base_len);
                if let Some(previous) = &mut joined {
                    if previous.len() != frame.locals.len() {
                        return Err(fail("original match changes enclosing lexical bindings"));
                    }
                    for (old, new) in previous.iter_mut().zip(&frame.locals) {
                        b.spend(1, WORK)?;
                        old.used |= new.used;
                        old.initialized &= new.initialized;
                    }
                } else {
                    joined = Some(self.copy_locals(&frame.locals, b)?);
                }
            }
        }
        if !total {
            match &self.state.arena[self.root(subject, b)?] {
                Kind::Nominal { definition, .. } => {
                    let declaration = self.inventory.definition(*definition, b)?;
                    let SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Enum,
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } = &declaration.body
                    else {
                        return Err(fail("original match lacks an exhaustive subject family"));
                    };
                    if coverage.len() != variants.len() {
                        return Err(unknown(
                            "original match does not prove complete enum coverage",
                        ));
                    }
                }
                Kind::Atom(Type::Bool) if bools.len() == 2 => {}
                _ => {
                    return Err(unknown(
                        "original match needs its complete exhaustive pattern coverage proof",
                    ));
                }
            }
        }
        frame.alive = joined.is_some();
        frame.locals = joined.unwrap_or(base);
        Ok(())
    }
}

pub(in crate::semantic) struct ActualConstructor {
    pub(in crate::semantic) result: crate::semantic::value::Value,
    pub(in crate::semantic) variant: u32,
    pub(in crate::semantic) arguments: Vec<crate::semantic::value::Argument>,
    pub(in crate::semantic) payload: Vec<crate::semantic::value::Value>,
}
impl Checked {
    pub(in crate::semantic) fn enum_constructor(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<ActualConstructor>> {
        let declaration = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &declaration.body else {
            return Err(fail("actual variant has no complete original definition"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("actual variant selects an original statement"));
                };
                if selected.replace(node).is_some() {
                    return Err(fail("actual variant has competing original node anchors"));
                }
            }
        }
        let node = selected.ok_or_else(|| fail("actual variant has no original node anchor"))?;
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
            result,
            variant: constructor.variant,
            arguments,
            payload,
        }))
    }
}
