//! Finite Copy enums and bounded pattern-matrix usefulness checking.
use super::*;
use std::collections::BTreeSet;

pub(super) fn nominal_name(owner: u32, name: &str, variants: &[(String, Vec<Type>)]) -> String {
    if name.starts_with("$generic_$builtinOption340_")
        && let [(none, empty), (some, payload)] = variants
        && none == "None"
        && empty.is_empty()
        && some == "Some"
        && let [payload] = payload.as_slice()
    {
        return format!("@builtin:Option:{}", payload.fingerprint());
    }
    if name == "$stdTaskCreateError343" {
        return "@std.task:v1:TaskCreateError".into();
    }
    if name == "$stdTaskPollState343" {
        return "@std.task:v1:TaskPollState".into();
    }
    if name.starts_with("$generic_$stdTaskPoll343_")
        && let [(pending, empty), (ready, payload)] = variants
        && pending == "Pending"
        && empty.is_empty()
        && ready == "Ready"
        && let [payload] = payload.as_slice()
    {
        return format!("@std.task:v1:Poll:{}", payload.fingerprint());
    }
    format!("@module{owner}:{name}")
}

pub(super) const ANONYMOUS_SUM: &str = "@anonymous-sum:v1";
pub(super) const TYPE_MEMBER: &str = "@type-member";
pub(super) fn anonymous(ty: &Type) -> bool {
    matches!(ty, Type::Enum { name, .. } if name == ANONYMOUS_SUM)
}
pub(super) fn canonical_anonymous(
    mut members: Vec<Type>,
    remaining: &mut usize,
    limits: &Limits,
    span: Span,
) -> R<Type> {
    let mut work = ANONYMOUS_SUM
        .len()
        .saturating_add(members.len().saturating_mul(32));
    for member in &members {
        work = work.saturating_add(
            inference::type_snapshot_work(member, span, limits)?
                .saturating_mul(members.len().saturating_add(2)),
        );
    }
    *remaining = remaining.checked_sub(work).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "anonymous sum canonical identity work exceeded",
            Some(span),
        )
    })?;
    if members.len() < 2
        || members
            .iter()
            .any(|member| !member.is_copy_value() && !owned_shapes::value(member))
    {
        return Err(error(
            "E-ANONYMOUS-SUM",
            span,
            "anonymous sum requires at least two finite ordinary complete member types",
        ));
    }
    members.sort();
    if members.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(error(
            "E-ANONYMOUS-SUM",
            span,
            "anonymous sum repeats a complete resolved member type",
        ));
    }
    Ok(Type::Enum {
        name: ANONYMOUS_SUM.into(),
        variants: members
            .into_iter()
            .enumerate()
            .map(|(index, member)| (format!("Member{index}"), vec![member]))
            .collect(),
    })
}
/// Callers bound and prepay complete compared types before selecting a member.
pub(super) fn anonymous_injection(actual: &Type, sum: &Type, span: Span) -> R<Option<u32>> {
    if actual == sum {
        return Ok(None);
    }
    let Type::Enum { name, variants } = sum else {
        unreachable!("anonymous sum context")
    };
    debug_assert_eq!(name, ANONYMOUS_SUM);
    let mut found = None;
    for (index, (_, fields)) in variants.iter().enumerate() {
        if fields.as_slice() == std::slice::from_ref(actual)
            && found.replace(index as u32).is_some()
        {
            return Err(error(
                "E-ANONYMOUS-SUM",
                span,
                "anonymous sum has ambiguous repeated member identity",
            ));
        }
    }
    found.map(Some).ok_or_else(|| {
        error(
            "E-ANONYMOUS-SUM",
            span,
            "value is not an exact member of the expected anonymous sum",
        )
    })
}
pub(super) fn pattern_variant<'a>(
    declared: &Type,
    variant: &str,
    expected: &'a Type,
    span: Span,
) -> R<(usize, &'a [Type])> {
    if let Type::ErrorUnion {
        error: error_ty,
        result,
    } = expected
    {
        if variant != TYPE_MEMBER {
            return Err(error(
                "E-MATCH-TYPE",
                span,
                "ErrorUnion member needs an exact payload type",
            ));
        }
        return match (declared == error_ty.as_ref(), declared == result.as_ref()) {
            (true, false) => Ok((1, std::slice::from_ref(error_ty.as_ref()))),
            (false, true) => Ok((0, std::slice::from_ref(result.as_ref()))),
            (true, true) => Err(error(
                "E-ERROR-PATTERN-AMBIGUOUS",
                span,
                "equal error and success types need .ok or .err",
            )),
            _ => Err(error(
                "E-MATCH-TYPE",
                span,
                "pattern type is not an exact ErrorUnion payload",
            )),
        };
    }
    let Type::Enum { variants, .. } = expected else {
        return Err(error(
            "E-MATCH-TYPE",
            span,
            "variant pattern requires an enum or anonymous sum",
        ));
    };
    let index = if variant == TYPE_MEMBER && anonymous(expected) {
        anonymous_injection(declared, expected, span)?.ok_or_else(|| {
            error(
                "E-ANONYMOUS-SUM",
                span,
                "a member pattern must name one exact payload type",
            )
        })? as usize
    } else {
        if declared != expected {
            return Err(error(
                "E-MATCH-TYPE",
                span,
                "pattern has a different complete sum type",
            ));
        }
        variants
            .iter()
            .position(|(name, _)| name == variant)
            .ok_or_else(|| error("E-ENUM-VARIANT", span, "unknown enum variant"))?
    };
    Ok((index, &variants[index].1))
}

/// Reified Core may name the exact existing nominal identity. The spelling is
/// unavailable to source identifiers; it still resolves through this ordinary
/// declaration map and must match the complete owner/name pair.
pub(super) fn constructor_type<'a>(
    types: &'a BTreeMap<String, Type>,
    name: &str,
    span: Span,
) -> R<Option<&'a Type>> {
    if name.starts_with("@module") {
        let ty = name
            .split_once(':')
            .and_then(|(_, declaration)| types.get(declaration))
            .filter(|ty| matches!(ty, Type::Enum { name: actual, .. } if actual == name))
            .ok_or_else(|| {
                error(
                    "E-UNKNOWN-NAME",
                    span,
                    "canonical enum namespace does not match an existing nominal declaration",
                )
            })?;
        Ok(Some(ty))
    } else {
        Ok(types.get(name))
    }
}

pub(super) fn definitions(core: &CoreModule) -> BTreeMap<String, Type> {
    core.enums
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                Type::Enum {
                    name: nominal_name(core.module_id.0, &e.name, &e.variants),
                    variants: e.variants.clone(),
                },
            )
        })
        .collect()
}

pub(super) fn project_definitions(
    program: &CoreProgram,
    current: &CoreModule,
    remaining: &mut usize,
    limits: &Limits,
) -> CompileResult<BTreeMap<String, Type>> {
    let mut result = BTreeMap::new();
    for module in &program.modules {
        for e in &module.core.enums {
            if module.core.module_id != current.module_id && !aggregates::project_symbol(&e.name) {
                continue;
            }
            let limit = || {
                vec![Diagnostic::limit(
                    Phase::Types,
                    "project enum shape budget exceeded",
                    Some(e.span),
                )]
            };
            *remaining = remaining
                .checked_sub(e.name.len().saturating_add(e.variants.len()))
                .ok_or_else(limit)?;
            let mut pending = e
                .variants
                .iter()
                .flat_map(|(_, fields)| fields)
                .map(|ty| (ty, 0))
                .collect::<Vec<_>>();
            while let Some((ty, depth)) = pending.pop() {
                if depth > limits.max_call_depth {
                    return Err(limit());
                }
                *remaining = remaining.checked_sub(1).ok_or_else(limit)?;
                match ty {
                    Type::Record { name, fields } | Type::Package { name, fields } => {
                        *remaining = remaining
                            .checked_sub(name.len().saturating_add(fields.len()))
                            .ok_or_else(limit)?;
                        pending.extend(fields.iter().map(|(_, t)| (t, depth + 1)));
                    }
                    Type::Enum { name, variants } => {
                        *remaining = remaining
                            .checked_sub(name.len().saturating_add(variants.len()))
                            .ok_or_else(limit)?;
                        pending.extend(
                            variants
                                .iter()
                                .flat_map(|(_, fields)| fields)
                                .map(|t| (t, depth + 1)),
                        );
                    }
                    Type::Managed(inner)
                    | Type::Desc(inner)
                    | Type::OwnedStore { element: inner } => {
                        pending.push((inner, depth + 1));
                    }
                    ty @ Type::ErrorUnion { error, result } if errors::owned(ty) => {
                        *remaining = remaining.checked_sub(2).ok_or_else(limit)?;
                        pending.push((error, depth + 1));
                        pending.push((result, depth + 1));
                    }
                    Type::Tuple(fields) => pending.extend(fields.iter().map(|t| (t, depth + 1))),
                    Type::Array { element, .. }
                    | Type::Slice { element }
                    | Type::Address { inner: element, .. } => pending.push((element, depth + 1)),
                    _ => {}
                }
            }
            if result
                .insert(
                    e.name.clone(),
                    Type::Enum {
                        name: nominal_name(module.core.module_id.0, &e.name, &e.variants),
                        variants: e.variants.clone(),
                    },
                )
                .is_some()
            {
                return Err(vec![error(
                    "E-NAME-DUPLICATE",
                    e.span,
                    "duplicate enum identity in project",
                )]);
            }
        }
    }
    Ok(result)
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Constructor {
    Unit,
    Bool(bool),
    Integer(Constant),
    Tuple,
    Variant(u32),
    Opaque,
}
#[derive(Clone, Debug)]
enum Pattern {
    Any,
    Constructor(Constructor, Vec<Pattern>),
}
struct Coverage<'a> {
    error_arms: bool,
    remaining: &'a mut usize,
    depth_limit: usize,
    span: Span,
}
impl Coverage<'_> {
    fn spend(&mut self, amount: usize, depth: usize) -> R<()> {
        if depth > self.depth_limit {
            return Err(Diagnostic::limit(
                Phase::Types,
                "match pattern depth budget exceeded",
                Some(self.span),
            ));
        }
        *self.remaining = self.remaining.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "match coverage budget exceeded",
                Some(self.span),
            )
        })?;
        Ok(())
    }
    fn pattern(
        &mut self,
        raw: &CorePattern,
        ty: &Type,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<Pattern> {
        self.spend(1, depth)?;
        let invalid = || {
            error(
                "E-MATCH-TYPE",
                raw.span,
                "pattern does not match the scrutinee type",
            )
        };
        Ok(match &raw.kind {
            CorePatternKind::Name(name) => {
                self.spend(name.len(), depth)?;
                if !names.insert(name.clone()) {
                    return Err(error(
                        "E-MATCH-BINDING",
                        raw.span,
                        "pattern binds a name more than once",
                    ));
                }
                Pattern::Any
            }
            CorePatternKind::Wildcard => Pattern::Any,
            CorePatternKind::Reference { pattern, .. } => {
                self.pattern(pattern, ty, names, depth + 1)?
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                self.error_arms = true;
                let Type::ErrorUnion {
                    error: error_ty,
                    result,
                } = ty
                else {
                    return Err(invalid());
                };
                Pattern::Constructor(
                    Constructor::Variant(u32::from(*is_error)),
                    vec![self.pattern(
                        pattern,
                        if *is_error { error_ty } else { result },
                        names,
                        depth + 1,
                    )?],
                )
            }
            CorePatternKind::Bool(value) if *ty == Type::Bool => {
                Pattern::Constructor(Constructor::Bool(*value), Vec::new())
            }
            CorePatternKind::Integer(value) if ty.is_integer() => {
                let value = integer(value, Some(ty), false, raw.span)?;
                if &value.ty() != ty {
                    return Err(invalid());
                }
                Pattern::Constructor(Constructor::Integer(value), Vec::new())
            }
            CorePatternKind::Tuple(fields) if fields.is_empty() && *ty == Type::Unit => {
                Pattern::Constructor(Constructor::Unit, Vec::new())
            }
            CorePatternKind::Tuple(fields) => {
                let Type::Tuple(types) = ty else {
                    return Err(invalid());
                };
                if fields.len() != types.len() {
                    return Err(error(
                        "E-MATCH-ARITY",
                        raw.span,
                        "tuple pattern arity mismatch",
                    ));
                }
                let fields = fields
                    .iter()
                    .zip(types)
                    .map(|(p, t)| self.pattern(p, t, names, depth + 1))
                    .collect::<R<_>>()?;
                Pattern::Constructor(Constructor::Tuple, fields)
            }
            CorePatternKind::Variant {
                ty: declared,
                variant,
                fields,
            } => {
                if anonymous(ty) || matches!(ty, Type::ErrorUnion { .. }) {
                    self.error_arms |= matches!(ty, Type::ErrorUnion { .. });
                    let limits = Limits {
                        max_call_depth: self.depth_limit,
                        ..Limits::default()
                    };
                    inference::charge_type_snapshot(ty, raw.span, &limits, self.remaining, 2)?;
                    if self.error_arms {
                        inference::charge_type_snapshot(
                            declared,
                            raw.span,
                            &limits,
                            self.remaining,
                            1,
                        )?;
                    }
                }
                let (index, types) = pattern_variant(declared, variant, ty, raw.span)?;
                if fields.len() != types.len() {
                    return Err(error(
                        "E-MATCH-ARITY",
                        raw.span,
                        "variant pattern arity mismatch",
                    ));
                }
                let fields = fields
                    .iter()
                    .zip(types)
                    .map(|(p, t)| self.pattern(p, t, names, depth + 1))
                    .collect::<R<_>>()?;
                Pattern::Constructor(Constructor::Variant(index as u32), fields)
            }
            _ => return Err(invalid()),
        })
    }
    fn reserve_copies(
        &mut self,
        rows: &[Vec<Pattern>],
        query: &[Pattern],
        types: &[Type],
        depth: usize,
    ) -> R<()> {
        if !self.error_arms {
            return Ok(());
        }
        fn patterns(this: &mut Coverage<'_>, ps: &[Pattern], depth: usize) -> R<()> {
            this.spend(ps.len() + 1, depth)?;
            for p in ps {
                if let Pattern::Constructor(_, fields) = p {
                    patterns(this, fields, depth + 1)?;
                }
            }
            Ok(())
        }
        let limits = Limits {
            max_call_depth: self.depth_limit,
            ..Limits::default()
        };
        for ty in types {
            inference::charge_type_snapshot(ty, self.span, &limits, self.remaining, 2)?;
        }
        for row in rows {
            patterns(self, row, depth)?;
        }
        patterns(self, query, depth)
    }
    fn useful(
        &mut self,
        rows: &[Vec<Pattern>],
        query: &[Pattern],
        types: &[Type],
        depth: usize,
    ) -> R<bool> {
        self.spend(
            1usize
                .saturating_add(query.len())
                .saturating_add(rows.iter().map(Vec::len).sum::<usize>()),
            depth,
        )?;
        if query.is_empty() {
            return Ok(rows.is_empty());
        }
        if rows
            .iter()
            .any(|row| row.iter().all(|p| matches!(p, Pattern::Any)))
        {
            return Ok(false);
        }
        if let Pattern::Constructor(constructor, fields) = &query[0] {
            self.reserve_copies(rows, query, types, depth)?;
            let field_types = constructor_fields(constructor, &types[0]);
            let rows = specialize(rows, constructor, fields.len());
            let next = fields
                .iter()
                .chain(&query[1..])
                .cloned()
                .collect::<Vec<_>>();
            let types = field_types
                .into_iter()
                .chain(types[1..].iter().cloned())
                .collect::<Vec<_>>();
            return self.useful(&rows, &next, &types, depth + 1);
        }
        self.reserve_copies(&[], &[], types, depth)?;
        let constructors = match &types[0] {
            Type::Unit => vec![Constructor::Unit],
            Type::Bool => vec![Constructor::Bool(false), Constructor::Bool(true)],
            Type::Tuple(_) => vec![Constructor::Tuple],
            Type::ErrorUnion { .. } => vec![Constructor::Variant(0), Constructor::Variant(1)],
            Type::Enum { variants, .. } => (0..variants.len())
                .map(|i| Constructor::Variant(i as u32))
                .collect(),
            Type::U8 | Type::U16 | Type::U64 | Type::Usize | Type::I32 | Type::I64 => {
                let defaults = rows
                    .iter()
                    .filter(|row| matches!(row[0], Pattern::Any))
                    .map(|row| row[1..].to_vec())
                    .collect::<Vec<_>>();
                return self.useful(&defaults, &query[1..], &types[1..], depth + 1);
            }
            _ => vec![Constructor::Opaque],
        };
        for constructor in constructors {
            self.reserve_copies(rows, query, types, depth)?;
            let fields = constructor_fields(&constructor, &types[0]);
            let rows = specialize(rows, &constructor, fields.len());
            let next = std::iter::repeat_n(Pattern::Any, fields.len())
                .chain(query[1..].iter().cloned())
                .collect::<Vec<_>>();
            let types = fields
                .into_iter()
                .chain(types[1..].iter().cloned())
                .collect::<Vec<_>>();
            if self.useful(&rows, &next, &types, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn constructor_fields(constructor: &Constructor, ty: &Type) -> Vec<Type> {
    match (constructor, ty) {
        (Constructor::Tuple, Type::Tuple(fields)) => fields.clone(),
        (Constructor::Variant(index), Type::ErrorUnion { error, result }) => {
            vec![if *index == 1 { error } else { result }.as_ref().clone()]
        }
        (Constructor::Variant(index), Type::Enum { variants, .. }) => {
            variants[*index as usize].1.clone()
        }
        _ => Vec::new(),
    }
}
fn specialize(rows: &[Vec<Pattern>], constructor: &Constructor, arity: usize) -> Vec<Vec<Pattern>> {
    rows.iter()
        .filter_map(|row| {
            let fields = match &row[0] {
                Pattern::Any => vec![Pattern::Any; arity],
                Pattern::Constructor(c, fields) if c == constructor => fields.clone(),
                _ => return None,
            };
            Some(fields.into_iter().chain(row[1..].iter().cloned()).collect())
        })
        .collect()
}

impl Builder<'_, '_> {
    /// Check one conditional pattern without requiring coverage of the input.
    /// The returned flag means no valid input can take its failure edge.
    pub(super) fn conditional_pattern(&mut self, pattern: &CorePattern, ty: &Type) -> R<bool> {
        if !ty.is_copy_value() {
            return Err(unsupported(
                pattern.span,
                "pattern condition requires a finite Copy value",
            ));
        }
        self.irrefutable_pattern(pattern, ty)
    }
    pub(super) fn irrefutable_pattern(&mut self, pattern: &CorePattern, ty: &Type) -> R<bool> {
        let mut coverage = Coverage {
            error_arms: false,
            remaining: &mut self.match_work_left,
            depth_limit: self.limits.max_call_depth,
            span: pattern.span,
        };
        let checked = coverage.pattern(pattern, ty, &mut BTreeSet::new(), 0)?;
        Ok(!coverage.useful(
            &[vec![checked]],
            &[Pattern::Any],
            std::slice::from_ref(ty),
            0,
        )?)
    }
    pub(super) fn enum_constructor(&self, expression: &CoreExpr) -> R<Option<(Type, u32)>> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &expression.kind
        else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        if self.lookup(name).is_some() {
            return Ok(None);
        }
        let Some(types) = self.enums else {
            return Ok(None);
        };
        let Some(ty @ Type::Enum { variants, .. }) =
            constructor_type(types, name, expression.span)?
        else {
            return Ok(None);
        };
        if owner.is_some() {
            return Err(error(
                "E-MEMBER-OWNER",
                expression.span,
                "a constrained member cannot denote an enum constructor namespace",
            ));
        }
        let index = variants
            .iter()
            .position(|(name, _)| name == member)
            .ok_or_else(|| error("E-ENUM-VARIANT", expression.span, "unknown enum variant"))?;
        Ok(Some((ty.clone(), index as u32)))
    }
    pub(super) fn make_variant(
        &mut self,
        ty: &Type,
        variant: u32,
        expressions: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let Type::Enum { variants, .. } = ty else {
            unreachable!()
        };
        let fields = &variants[variant as usize].1;
        if fields.len() != expressions.len() {
            return Err(error(
                "E-ENUM-ARITY",
                span,
                "enum variant payload arity mismatch",
            ));
        }
        if let Some(expected) = expected {
            self.same(ty, expected, span)?;
        }
        if owned_shapes::enumeration(ty) {
            let work = inference::type_snapshot_work(ty, span, self.limits)?;
            self.closures
                .snapshot(work.saturating_mul(4).saturating_add(fields.len()), span)?;
            let mut staged = Vec::new();
            for (expression, expected) in expressions.iter().zip(fields) {
                let (value, actual) = self.argument_expr(expression, expected)?;
                self.same(&actual, expected, expression.span)?;
                staged.push(self.stage_argument(value, &actual, expression.span)?);
            }
            let fields = self.commit_arguments(staged, span)?;
            let value = self.value(
                InstructionKind::MakeVariant { variant, fields },
                ty.clone(),
                span,
            )?;
            return Ok((value, ty.clone()));
        }
        let mut values = Vec::new();
        for (expression, expected) in expressions.iter().zip(fields) {
            let (value, actual) = self.expr(expression, Some(expected))?;
            self.same(&actual, expected, expression.span)?;
            values.push(value);
        }
        Ok((
            self.value(
                InstructionKind::MakeVariant {
                    variant,
                    fields: values,
                },
                ty.clone(),
                span,
            )?,
            ty.clone(),
        ))
    }
    pub(super) fn match_statement(
        &mut self,
        expression: &CoreExpr,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        if let Some(ty) = self.known_type(expression)
            && !matches!(
                expression.kind,
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    ..
                }
            )
            && arms
                .iter()
                .any(|arm| self.partial_pattern_needed(&arm.pattern, &ty))
        {
            return self.partial_pattern_match(expression, arms, span);
        }
        if (matches!(&expression.kind,CoreExprKind::Unary{op:UnaryOp::Dereference,operand} if matches!(self.known_type(operand),Some(Type::Address{inner,..}) if matches!(inner.as_ref(),Type::Enum{..}|Type::ErrorUnion{..}))))
            || arms
                .iter()
                .any(|a| reference_patterns::has_reference(&a.pattern))
        {
            return self.reference_match(expression, arms, span);
        }
        if self
            .known_type(expression)
            .as_ref()
            .is_some_and(owned_shapes::enumeration)
        {
            return self.owned_enum_match(expression, arms, span);
        }
        let (value, ty) = self.expr(expression, None)?;
        if !ty.is_copy_value() {
            return Err(unsupported(span, "match requires a finite Copy value"));
        }
        self.finish_resource_temps(expression.span)?;
        self.check_match_coverage(&ty, arms, span)?;
        let mut ends = Vec::new();
        for arm in arms {
            let fail = self.block_id()?;
            let mut bindings = Vec::new();
            self.pattern_test(&arm.pattern, value, &ty, fail, &mut bindings)?;
            self.push_scope(arm.span)?;
            for (name, value, ty, span) in bindings {
                let local = self.local(&name, ty, false, span)?;
                self.store(local, value, span)?;
            }
            self.block(&arm.body)?;
            if self.current.is_some() {
                self.end_scopes(self.scopes.len() - 1, arm.span)?;
            }
            if let Some(end) = self.current.take() {
                ends.push(end);
            }
            self.pop_scope();
            self.current = Some(fail);
        }
        self.terminate(Terminator::Trap);
        if !ends.is_empty() {
            let join = self.block_id()?;
            for end in ends {
                self.function.blocks[end.0 as usize].terminator = Terminator::Jump(join);
            }
            self.current = Some(join);
        }
        Ok(())
    }
    pub(super) fn check_match_coverage(
        &mut self,
        ty: &Type,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        let mut coverage = Coverage {
            error_arms: false,
            remaining: &mut self.match_work_left,
            depth_limit: self.limits.max_call_depth,
            span,
        };
        let mut rows = Vec::new();
        for arm in arms {
            let pattern = coverage.pattern(&arm.pattern, ty, &mut BTreeSet::new(), 0)?;
            if !coverage.useful(
                &rows,
                std::slice::from_ref(&pattern),
                std::slice::from_ref(ty),
                0,
            )? {
                return Err(error(
                    "E-MATCH-UNREACHABLE",
                    arm.span,
                    "match arm is already covered by previous arms",
                ));
            }
            rows.push(vec![pattern]);
        }
        if coverage.useful(&rows, &[Pattern::Any], std::slice::from_ref(ty), 0)? {
            return Err(error(
                "E-MATCH-NONEXHAUSTIVE",
                span,
                "match does not cover every possible value",
            ));
        }
        Ok(())
    }
    pub(super) fn pattern_branch(
        &mut self,
        value: ValueId,
        expected: Constant,
        fail: BlockId,
        span: Span,
    ) -> R<()> {
        let constant = self.constant(expected, span)?.0;
        let equal = self.value(
            InstructionKind::Binary {
                op: BinaryOp::Equal,
                left: value,
                right: constant,
            },
            Type::Bool,
            span,
        )?;
        let pass = self.block_id()?;
        self.terminate(Terminator::Branch {
            condition: equal,
            then_block: pass,
            else_block: fail,
        });
        self.current = Some(pass);
        Ok(())
    }
    pub(super) fn pattern_test(
        &mut self,
        pattern: &CorePattern,
        value: ValueId,
        ty: &Type,
        fail: BlockId,
        bindings: &mut Vec<(String, ValueId, Type, Span)>,
    ) -> R<()> {
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                bindings.push((name.clone(), value, ty.clone(), pattern.span))
            }
            CorePatternKind::Wildcard => {}
            CorePatternKind::ErrorArm {
                is_error,
                pattern: payload,
            } => {
                let Type::ErrorUnion {
                    error: error_ty,
                    result,
                } = ty
                else {
                    return Err(error(
                        "E-MATCH-TYPE",
                        pattern.span,
                        "error arm requires an ErrorUnion",
                    ));
                };
                let tag = self.value(
                    InstructionKind::ErrorTag { value },
                    Type::Bool,
                    pattern.span,
                )?;
                self.pattern_branch(tag, Constant::Bool(*is_error), fail, pattern.span)?;
                let payload_ty = if *is_error { error_ty } else { result };
                let payload_value = self.value(
                    InstructionKind::ReadErrorUnion {
                        value,
                        is_error: *is_error,
                    },
                    payload_ty.as_ref().clone(),
                    pattern.span,
                )?;
                self.pattern_test(payload, payload_value, payload_ty, fail, bindings)?;
            }
            CorePatternKind::Reference { .. } => {
                return Err(unsupported(
                    pattern.span,
                    "reference patterns require place-based lowering",
                ));
            }
            CorePatternKind::Integer(text) => self.pattern_branch(
                value,
                integer(text, Some(ty), false, pattern.span)?,
                fail,
                pattern.span,
            )?,
            CorePatternKind::Bool(value_pattern) => {
                self.pattern_branch(value, Constant::Bool(*value_pattern), fail, pattern.span)?
            }
            CorePatternKind::Tuple(fields) => {
                if let Type::Tuple(types) = ty {
                    for (index, (field, ty)) in fields.iter().zip(types).enumerate() {
                        let projected = self.value(
                            InstructionKind::ExtractField {
                                aggregate: value,
                                index: index as u32,
                            },
                            ty.clone(),
                            field.span,
                        )?;
                        self.pattern_test(field, projected, ty, fail, bindings)?;
                    }
                }
            }
            CorePatternKind::Variant {
                ty: declared,
                variant,
                fields,
            } => {
                if anonymous(ty) {
                    self.closures.snapshot_type(ty, 2, 0, pattern.span)?;
                }
                let (index, types) = pattern_variant(declared, variant, ty, pattern.span)?;
                let tag = self.value(
                    InstructionKind::VariantTag { value },
                    Type::I32,
                    pattern.span,
                )?;
                self.pattern_branch(tag, Constant::I32(index as i32), fail, pattern.span)?;
                for (field_index, (field, ty)) in fields.iter().zip(types).enumerate() {
                    let projected = self.value(
                        InstructionKind::ReadVariant {
                            value,
                            variant: index as u32,
                            index: field_index as u32,
                        },
                        ty.clone(),
                        field.span,
                    )?;
                    self.pattern_test(field, projected, ty, fail, bindings)?;
                }
            }
        }
        Ok(())
    }
}
