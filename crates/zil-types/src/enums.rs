//! Finite Copy enums and bounded pattern-matrix usefulness checking.
use super::*;
use std::collections::BTreeSet;

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
                    name: format!("@module{}:{}", core.module_id.0, e.name),
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
                        name: format!("@module{}:{}", module.core.module_id.0, e.name),
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
                if declared != ty {
                    return Err(invalid());
                }
                let Type::Enum { variants, .. } = ty else {
                    return Err(invalid());
                };
                let (index, (_, types)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| name == variant)
                    .ok_or_else(|| error("E-ENUM-VARIANT", raw.span, "unknown enum variant"))?;
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
        let constructors = match &types[0] {
            Type::Unit => vec![Constructor::Unit],
            Type::Bool => vec![Constructor::Bool(false), Constructor::Bool(true)],
            Type::Tuple(_) => vec![Constructor::Tuple],
            Type::Enum { variants, .. } => (0..variants.len())
                .map(|i| Constructor::Variant(i as u32))
                .collect(),
            Type::U8 | Type::U16 | Type::I32 | Type::I64 => {
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
        let mut coverage = Coverage {
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
        let CoreExprKind::Member { object, member } = &expression.kind else {
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
                variant, fields, ..
            } => {
                let Type::Enum { variants, .. } = ty else {
                    unreachable!()
                };
                let (index, (_, types)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| name == variant)
                    .expect("checked variant");
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
