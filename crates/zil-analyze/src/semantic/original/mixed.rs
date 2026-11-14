//! Mixed original actuals keep their full declaration ordinals and rigid
//! lifetime identities. A particular executable image never equates them.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Life {
    Formal(SemanticDefinitionId, u32),
    Implicit(SemanticDefinitionId, u32),
    Invocation(SemanticDefinitionId, u32),
    Role(SemanticDefinitionId, u32, u32),
}

impl Solver<'_, '_> {
    pub(super) fn rigid_formal(
        &mut self,
        owner: SemanticDefinitionId,
        ordinal: usize,
        formal: &SemanticFormal,
        previous: &[Term],
        b: &mut Budget,
    ) -> CompileResult<Term> {
        let ordinal =
            u32::try_from(ordinal).map_err(|_| fail("original mixed formal ordinal overflow"))?;
        match formal {
            SemanticFormal::Type { requirements } => {
                self.premises_well_formed(owner, requirements, b)?;
                self.term(Kind::Rigid(owner, ordinal), b)
            }
            SemanticFormal::Lifetime => self.term(Kind::Life(Life::Formal(owner, ordinal)), b),
            SemanticFormal::Const { carrier } => {
                b.spend(previous.len(), WORK)?;
                let context = Frame {
                    definition: owner,
                    formals: previous.to_vec(),
                    parameters: Vec::new(),
                    result: 0,
                    locals: Vec::new(),
                    inherited: 0,
                    alive: true,
                    environment: Vec::new(),
                    expressions: BTreeMap::new(),
                    bindings: BTreeMap::new(),
                };
                let carrier = self.recipe(&context, carrier, b, 0)?;
                self.term(
                    Kind::ConstRigid {
                        owner,
                        ordinal,
                        carrier,
                    },
                    b,
                )
            }
        }
    }
    pub(super) fn life(
        &mut self,
        frame: &Frame,
        recipe: &SemanticLifeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match recipe {
            SemanticLifeRecipe::Formal(reference) => {
                let owner = self.original_scope(frame.definition, &reference.scope, b)?;
                if !matches!(
                    self.inventory
                        .definition(owner, b)?
                        .parameters
                        .get(reference.ordinal as usize),
                    Some(SemanticFormal::Lifetime)
                ) {
                    return Err(fail("original Life reference changes its full formal kind"));
                }
                let context = if owner == frame.definition {
                    frame
                } else {
                    self.state
                        .frames
                        .get(&owner)
                        .ok_or_else(|| fail("original lifetime lacks its checked lexical parent"))?
                };
                context
                    .formals
                    .get(reference.ordinal as usize)
                    .copied()
                    .ok_or_else(|| fail("original lifetime formal is absent"))
            }
            SemanticLifeRecipe::Implicit { scope, ordinal } => {
                let owner = self.original_scope(frame.definition, scope, b)?;
                b.spend(self.role_frames.len(), WORK)?;
                for role in self.role_frames.iter().rev() {
                    if role.owner == owner && owner == frame.definition {
                        b.lookup(role.implicits.len(), WORK)?;
                        if let Some(life) = role.implicits.get(ordinal) {
                            return Ok(*life);
                        }
                    }
                }
                if !self
                    .inventory
                    .definition(owner, b)?
                    .implicit_lifetimes
                    .iter()
                    .any(|row| row.ordinal == *ordinal)
                {
                    return Err(fail("original implicit lifetime occurrence is absent"));
                }
                self.term(Kind::Life(Life::Implicit(owner, *ordinal)), b)
            }
            SemanticLifeRecipe::Invocation(ordinal) => {
                if let Some(role) = self
                    .role_frames
                    .last()
                    .filter(|role| role.owner == frame.definition)
                {
                    return role
                        .lifetimes
                        .get(*ordinal as usize)
                        .copied()
                        .ok_or_else(|| {
                            fail("original callable Invocation exceeds its own role frame")
                        });
                }
                let definition = self.inventory.definition(frame.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(fail("original invocation lifetime has no real signature"));
                };
                if *ordinal >= signature.invocation_lifetimes {
                    return Err(fail(
                        "original invocation lifetime exceeds its signature domain",
                    ));
                }
                self.term(Kind::Life(Life::Invocation(frame.definition, *ordinal)), b)
            }
            SemanticLifeRecipe::Environment { scope, ordinal } => {
                let owner = self.original_scope(frame.definition, scope, b)?;
                let declaration = self.inventory.definition(owner, b)?;
                if !matches!(
                    declaration
                        .environment
                        .get(*ordinal as usize)
                        .map(|formal| &formal.kind),
                    Some(SemanticFormal::Lifetime)
                ) {
                    return Err(fail("original Life environment changes its declared kind"));
                }
                let context = if owner == frame.definition {
                    frame
                } else {
                    self.state
                        .frames
                        .get(&owner)
                        .ok_or_else(|| fail("original Life environment lacks its sealed parent"))?
                };
                context
                    .environment
                    .get(*ordinal as usize)
                    .copied()
                    .ok_or_else(|| fail("original Life environment slot is absent"))
            }
            _ => Err(unknown(
                "original derived or quantified lifetime needs its exact occurrence and scoped relation proof",
            )),
        }
    }
    pub(super) fn actual(
        &mut self,
        frame: &Frame,
        formal: &SemanticFormal,
        recipe: &SemanticActualRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match (formal, recipe) {
            (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(ty)) => {
                self.recipe(frame, ty, b, depth + 1)
            }
            (SemanticFormal::Lifetime, SemanticActualRecipe::Life(life)) => {
                self.life(frame, life, b, depth + 1)
            }
            (
                SemanticFormal::Const { carrier },
                SemanticActualRecipe::Const {
                    carrier: written,
                    value,
                },
            ) => {
                let expected = self.recipe(frame, carrier, b, depth + 1)?;
                let written = self.recipe(frame, written, b, depth + 1)?;
                self.unify(expected, written, depth + 1, b)?;
                self.constant(frame, value, expected, b, depth + 1)
            }
            _ => Err(fail(
                "original actual changes its complete mixed formal kind",
            )),
        }
    }
    fn constant(
        &mut self,
        frame: &Frame,
        recipe: &SemanticConstRecipe,
        carrier: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match recipe {
            SemanticConstRecipe::Value(value) => {
                let ty = self.atom(value.ty(), b)?;
                self.unify(carrier, ty, depth + 1, b)?;
                self.term(Kind::ConstValue(value.clone()), b)
            }
            SemanticConstRecipe::IntegerLiteral(text) => {
                let root = self.root(carrier, b)?;
                let Kind::Atom(ty) = &self.state.arena[root] else {
                    return Err(unknown(
                        "original integer constant needs its complete nondependent carrier",
                    ));
                };
                let value = integer_constant(text, ty, b)?;
                self.term(Kind::ConstValue(value), b)
            }
            SemanticConstRecipe::Formal(reference) => {
                let owner = self.original_scope(frame.definition, &reference.scope, b)?;
                if !matches!(
                    self.inventory
                        .definition(owner, b)?
                        .parameters
                        .get(reference.ordinal as usize),
                    Some(SemanticFormal::Const { .. })
                ) {
                    return Err(fail(
                        "original Const reference changes its mixed formal kind",
                    ));
                }
                let context = if owner == frame.definition {
                    frame
                } else {
                    self.state
                        .frames
                        .get(&owner)
                        .ok_or_else(|| fail("original Const lacks its checked lexical parent"))?
                };
                let term = *context
                    .formals
                    .get(reference.ordinal as usize)
                    .ok_or_else(|| fail("original Const formal is absent"))?;
                let root = self.root(term, b)?;
                let expected = match &self.state.arena[root] {
                    Kind::ConstRigid { carrier, .. } => *carrier,
                    Kind::ConstValue(value) => self.atom(value.ty(), b)?,
                    _ => return Err(fail("original Const term has another kind")),
                };
                self.unify(carrier, expected, depth + 1, b)?;
                Ok(term)
            }
            _ => Err(unknown(
                "original constant expression needs its complete carrier-aware symbolic arithmetic proof",
            )),
        }
    }
    pub(super) fn borrow_occurrence(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<u32> {
        let declaration = self.inventory.definition(definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &declaration.body
        else {
            return Err(fail("original borrow has no complete body"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(sites.len().saturating_add(1), WORK)?;
            if link.node != SemanticOriginalNode::Expression(node) {
                continue;
            }
            for site in sites {
                if site.id != link.site
                    || !matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. })
                {
                    continue;
                }
                for row in &declaration.implicit_lifetimes {
                    b.spend(
                        row.occurrence_path
                            .len()
                            .saturating_add(site.occurrence_path.len() + 1),
                        WORK,
                    )?;
                    if row.occurrence_path == site.occurrence_path
                        && selected.replace(row.ordinal).is_some()
                    {
                        return Err(fail(
                            "original borrow has ambiguous declaration occurrences",
                        ));
                    }
                }
            }
        }
        selected.ok_or_else(|| unknown("original borrow lacks its explicit declaration occurrence"))
    }
}

impl Solver<'_, '_> {
    pub(super) fn named_life(
        &mut self,
        frame: &Frame,
        name: &str,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let definition = self.inventory.definition(frame.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &definition.body else {
            return Err(fail("written lifetime has no complete lexical name table"));
        };
        let name = name.strip_prefix('\'').unwrap_or(name);
        let mut selected = None;
        for binding in names {
            b.spend(
                name.len()
                    .saturating_add(binding.path.iter().map(String::len).sum::<usize>() + 1),
                WORK,
            )?;
            if binding.namespace == SemanticOriginalNamespace::Lifetime
                && binding.path.as_slice() == [name]
            {
                let SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Life(life)) =
                    &binding.value
                else {
                    return Err(fail("written lifetime name has another original kind"));
                };
                if selected.replace(life).is_some() {
                    return Err(fail("written lifetime name is ambiguous"));
                }
            }
        }
        self.life(
            frame,
            selected.ok_or_else(|| {
                fail("written lifetime is absent from its original lexical context")
            })?,
            b,
            depth + 1,
        )
    }
    pub(super) fn instantiate(
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
            Kind::Code {
                parameters, modes, ..
            } => parameters.len().saturating_add(modes.len() + 1),
            Kind::Nominal {
                arguments, fields, ..
            } => arguments.len().saturating_add(fields.len()),
            Kind::Closure {
                parameters,
                environment,
                ..
            } => parameters.len().saturating_add(environment.len() + 1),
            _ => 1,
        };
        b.spend(width, WORK)?;
        let kind = self.state.arena[term].clone();
        match kind {
            Kind::Rigid(target, ordinal) | Kind::Life(Life::Formal(target, ordinal))
                if target == owner =>
            {
                actuals
                    .get(ordinal as usize)
                    .copied()
                    .ok_or_else(|| fail("original instantiation omits a full mixed formal"))
            }
            Kind::ConstRigid {
                owner: target,
                ordinal,
                ..
            } if target == owner => actuals
                .get(ordinal as usize)
                .copied()
                .ok_or_else(|| fail("original constant instantiation omits a full mixed formal")),
            Kind::CallResult {
                caller,
                node,
                hidden,
            } => {
                let hidden = self.instantiate(hidden, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::CallResult {
                        caller,
                        node,
                        hidden,
                    },
                    b,
                )
            }
            Kind::Array { element, length } => {
                let element = self.instantiate(element, owner, actuals, b, depth + 1)?;
                let length = self.instantiate(length, owner, actuals, b, depth + 1)?;
                self.term(Kind::Array { element, length }, b)
            }
            Kind::ErrorUnion { error, result } => {
                let error = self.instantiate(error, owner, actuals, b, depth + 1)?;
                let result = self.instantiate(result, owner, actuals, b, depth + 1)?;
                self.term(Kind::ErrorUnion { error, result }, b)
            }
            Kind::Code {
                kind,
                parameters,
                result,
                modes,
                result_mode,
                call_kind,
            } => {
                let mut replaced = Vec::new();
                for parameter in parameters {
                    let term = self.instantiate(parameter, owner, actuals, b, depth + 1)?;
                    Self::push(&mut replaced, term, b)?;
                }
                let result = self.instantiate(result, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::Code {
                        kind,
                        parameters: replaced,
                        result,
                        modes,
                        result_mode,
                        call_kind,
                    },
                    b,
                )
            }
            Kind::Wrapped(wrapper, inner) => {
                let inner = self.instantiate(inner, owner, actuals, b, depth + 1)?;
                self.term(Kind::Wrapped(wrapper, inner), b)
            }
            Kind::Desc { life, inner } => {
                let life = self.instantiate(life, owner, actuals, b, depth + 1)?;
                let inner = self.instantiate(inner, owner, actuals, b, depth + 1)?;
                self.term(Kind::Desc { life, inner }, b)
            }
            Kind::Slice(element) => {
                let element = self.instantiate(element, owner, actuals, b, depth + 1)?;
                self.term(Kind::Slice(element), b)
            }
            Kind::Address {
                mutable,
                life,
                inner,
            } => {
                let life = self.instantiate(life, owner, actuals, b, depth + 1)?;
                let inner = self.instantiate(inner, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::Address {
                        mutable,
                        life,
                        inner,
                    },
                    b,
                )
            }
            Kind::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let field = self.instantiate(field, owner, actuals, b, depth + 1)?;
                    Self::push(&mut result, field, b)?;
                }
                self.term(Kind::Tuple(result), b)
            }
            Kind::Nominal {
                definition,
                arguments,
                fields,
            } => {
                let mut a = Vec::new();
                let mut f = Vec::new();
                for argument in arguments {
                    let term = self.instantiate(argument, owner, actuals, b, depth + 1)?;
                    Self::push(&mut a, term, b)?;
                }
                for field in fields {
                    let term = self.instantiate(field, owner, actuals, b, depth + 1)?;
                    Self::push(&mut f, term, b)?;
                }
                self.term(
                    Kind::Nominal {
                        definition,
                        arguments: a,
                        fields: f,
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
                let mut e = Vec::new();
                for value in environment {
                    let term = self.instantiate(value, owner, actuals, b, depth + 1)?;
                    Self::push(&mut e, term, b)?;
                }
                let mut p = Vec::new();
                for parameter in parameters {
                    let term = self.instantiate(parameter, owner, actuals, b, depth + 1)?;
                    Self::push(&mut p, term, b)?;
                }
                let result = self.instantiate(result, owner, actuals, b, depth + 1)?;
                self.term(
                    Kind::Closure {
                        definition,
                        parameters: p,
                        result,
                        environment: e,
                    },
                    b,
                )
            }
            Kind::Infer(None) => Err(unknown(
                "original result has an unresolved type variable before actual selection",
            )),
            _ => Ok(term),
        }
    }
    pub(super) fn callable_need(
        &mut self,
        frame: &Frame,
        term: Term,
        kind: CallableKind,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.role_need(frame, term, kind, signature, b, depth)
    }
    pub(super) fn check_callables(&mut self, b: &mut Budget) -> CompileResult<()> {
        self.check_roles(b)
    }
}

pub(in crate::semantic) fn integer_constant(
    text: &str,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<Constant> {
    validate_literal(text, ty, false, false, b)?;
    b.spend(text.len().saturating_mul(2), WORK)?;
    let clean = text.replace('_', "");
    let mut digits = clean.as_str();
    for (suffix, carrier) in [
        ("usize", Type::Usize),
        ("isize", Type::Isize),
        ("u64", Type::U64),
        ("i64", Type::I64),
        ("u32", Type::U32),
        ("i32", Type::I32),
        ("u16", Type::U16),
        ("i16", Type::I16),
        ("u8", Type::U8),
        ("i8", Type::I8),
    ] {
        if let Some(rest) = digits.strip_suffix(suffix) {
            if carrier != *ty {
                return Err(fail(
                    "original constant literal suffix differs from its declared carrier",
                ));
            }
            digits = rest;
            break;
        }
    }
    let negative = digits.starts_with('-');
    if negative {
        digits = &digits[1..];
    }
    let (radix, digits) = if let Some(rest) = digits.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = digits.strip_prefix("0b") {
        (2, rest)
    } else if let Some(rest) = digits.strip_prefix("0o") {
        (8, rest)
    } else {
        (10, digits)
    };
    let value = u64::from_str_radix(digits, radix)
        .map_err(|_| fail("original integer constant spelling or width is invalid"))?;
    let signed = if negative {
        -(value as i128)
    } else {
        value as i128
    };
    Ok(match ty {
        Type::I8 => Constant::I8(signed as i8),
        Type::I16 => Constant::I16(signed as i16),
        Type::I32 => Constant::I32(signed as i32),
        Type::I64 => Constant::I64(signed as i64),
        Type::Isize => Constant::Isize(signed as i64),
        Type::U8 => Constant::U8(value as u8),
        Type::U16 => Constant::U16(value as u16),
        Type::U32 => Constant::U32(value as u32),
        Type::U64 => Constant::U64(value),
        Type::Usize => Constant::Usize(value),
        _ => return Err(fail("original integer constant has a noninteger carrier")),
    })
}
