use super::*;

impl Checker<'_, '_, '_> {
    pub(super) fn block(&mut self, env: &mut Env, block: &s::Block, depth: usize) -> R<bool> {
        self.step(block.span, depth)?;
        let before = env.values.len();
        let mut terminal = false;
        // Even statements after a terminator remain original source obligations.
        for statement in &block.statements {
            terminal |= self.statement(env, statement, depth + 1)?;
        }
        env.values.truncate(before);
        Ok(terminal)
    }
    fn statement(&mut self, env: &mut Env, statement: &s::Stmt, depth: usize) -> R<bool> {
        self.step(statement.span, depth)?;
        let span = statement.span;
        match &statement.kind {
            s::StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
                let expected = ty
                    .as_ref()
                    .map(|ty| self.ty(env, ty, false, depth + 1))
                    .transpose()?;
                let value = self.expression(env, value, expected.as_ref(), depth + 1)?;
                self.pattern(env, pattern, &value, *mutable, depth + 1)?;
                Ok(false)
            }
            s::StmtKind::Return(expr) => {
                if env.yielding.is_some() {
                    return Err(bad(
                        span,
                        "original CT return cannot leave a block expression",
                    ));
                }
                let expected = self.duplicate_type(&env.result, span)?;
                if let Some(expr) = expr {
                    self.expression(env, expr, Some(&expected), depth + 1)?;
                } else {
                    self.same(&expected, &unit(), span)?;
                }
                Ok(true)
            }
            s::StmtKind::Yield(expr) => {
                let expected = match &env.yielding {
                    Some(Some(ty)) => Some(self.duplicate_type(ty, span)?),
                    Some(None) => None,
                    None => {
                        return Err(bad(
                            span,
                            "CT yield has no enclosing original block expression",
                        ));
                    }
                };
                let value = self.expression(env, expr, expected.as_ref(), depth + 1)?;
                if expected.is_none() {
                    env.yielding = Some(Some(value.ty));
                }
                Ok(true)
            }
            s::StmtKind::Expression(expr) => {
                self.expression(env, expr, None, depth + 1)?;
                Ok(false)
            }
            s::StmtKind::Assign { target, op, value } => {
                let expected = self.mutable_place(env, target, depth + 1)?;
                self.expression(env, value, Some(&expected), depth + 1)?;
                if *op != s::AssignOp::Set && !number(&expected) {
                    return Err(bad(
                        span,
                        "original CT compound assignment has no scalar operation",
                    ));
                }
                if matches!(
                    op,
                    s::AssignOp::BitAnd | s::AssignOp::BitOr | s::AssignOp::BitXor
                ) && !integer(&expected)
                {
                    return Err(bad(
                        span,
                        "original CT bit assignment needs integer carrier",
                    ));
                }
                if let s::ExprKind::Name(name) = &target.kind {
                    if let Some(binding) = env.values.iter_mut().rev().find(|row| row.name == *name)
                    {
                        binding.value.constant = None;
                        binding.value.represented = None;
                    }
                }
                Ok(false)
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut yes = self.environment_copy(env, span)?;
                self.condition(&mut yes, condition, depth + 1)?;
                let yes_terminal = self.block(&mut yes, then_body, depth + 1)?;
                let mut no = self.environment_copy(env, span)?;
                let no_terminal = match else_body {
                    Some(s::ElseBranch::Block(block)) => self.block(&mut no, block, depth + 1)?,
                    Some(s::ElseBranch::If(statement)) => {
                        self.statement(&mut no, statement, depth + 1)?
                    }
                    None => false,
                };
                self.merge_yield(env, &yes, span)?;
                self.merge_yield(env, &no, span)?;
                self.invalidate_values(env);
                Ok(yes_terminal && no_terminal)
            }
            s::StmtKind::While { condition, body } => {
                let mut inner = self.environment_copy(env, span)?;
                self.condition(&mut inner, condition, depth + 1)?;
                inner.loops += 1;
                self.block(&mut inner, body, depth + 1)?;
                self.merge_yield(env, &inner, span)?;
                self.invalidate_values(env);
                Ok(false)
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                let mut inner = self.environment_copy(env, span)?;
                let element = self.iterable(env, iterable, depth + 1)?;
                self.pattern(
                    &mut inner,
                    pattern,
                    &Value::typed(element),
                    false,
                    depth + 1,
                )?;
                inner.loops += 1;
                self.block(&mut inner, body, depth + 1)?;
                self.merge_yield(env, &inner, span)?;
                self.invalidate_values(env);
                Ok(false)
            }
            s::StmtKind::Loop(body) => {
                let mut inner = self.environment_copy(env, span)?;
                inner.loops += 1;
                self.block(&mut inner, body, depth + 1)?;
                self.merge_yield(env, &inner, span)?;
                self.invalidate_values(env);
                Ok(!Self::has_break(body))
            }
            s::StmtKind::Break | s::StmtKind::Continue => {
                if env.loops == 0 {
                    return Err(bad(span, "original CT break/continue has no lexical loop"));
                }
                Ok(false)
            }
            s::StmtKind::Match { value, arms } => {
                let value = self.expression(env, value, None, depth + 1)?;
                let mut all = !arms.is_empty();
                let mut catch_all = false;
                let mut variants = BTreeSet::new();
                let mut booleans = BTreeSet::new();
                let mut covered_types = vec![];
                for arm in arms {
                    let mut branch = self.environment_copy(env, span)?;
                    self.pattern(&mut branch, &arm.pattern, &value, false, depth + 1)?;
                    match &arm.pattern.kind {
                        s::PatternKind::Name(_) | s::PatternKind::Wildcard => catch_all = true,
                        s::PatternKind::Constructor { path, .. } => {
                            if matches!(value.ty, Ty::Sum(_)) {
                                let ty = self.named_type(
                                    &mut branch,
                                    path,
                                    &[],
                                    false,
                                    arm.span,
                                    depth + 1,
                                )?;
                                self.append(&mut covered_types, ty, arm.span)?;
                            }
                            if let Some(name) = path.last() {
                                variants.insert(name.as_str());
                            }
                        }
                        s::PatternKind::Literal(s::Literal::Bool(value)) => {
                            booleans.insert(*value);
                        }
                        _ => {}
                    }
                    let terminal = match &arm.body {
                        s::ArrowBody::Block(block) => self.block(&mut branch, block, depth + 1)?,
                        s::ArrowBody::Expression(expr) => {
                            self.expression(&mut branch, expr, None, depth + 1)?;
                            false
                        }
                    };
                    all &= terminal;
                    self.merge_yield(env, &branch, arm.span)?;
                }
                let exhaustive = catch_all
                    || match &value.ty {
                        Ty::Scalar(Scalar::Bool) => booleans.len() == 2,
                        Ty::Nominal(id, _) => match &self.declarations[*id].item.kind {
                            s::ItemKind::Enum(enumeration) => enumeration
                                .variants
                                .iter()
                                .all(|v| variants.contains(v.name.as_str())),
                            _ => false,
                        },
                        Ty::Sum(members) => {
                            members.iter().all(|member| covered_types.contains(member))
                        }
                        _ => false,
                    };
                if !exhaustive {
                    return Err(bad(
                        span,
                        "original CT match does not cover its complete value domain",
                    ));
                }
                self.invalidate_values(env);
                Ok(all)
            }
            s::StmtKind::Meta(meta) => match meta {
                s::MetaStatement::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expression(env, condition, Some(&bool_ty()), depth + 1)?;
                    let mut yes = self.environment_copy(env, span)?;
                    let a = self.block(&mut yes, then_body, depth + 1)?;
                    let mut no = self.environment_copy(env, span)?;
                    let b = if let Some(body) = else_body {
                        self.block(&mut no, body, depth + 1)?
                    } else {
                        false
                    };
                    self.merge_yield(env, &yes, span)?;
                    self.merge_yield(env, &no, span)?;
                    self.invalidate_values(env);
                    Ok(a && b)
                }
                s::MetaStatement::For {
                    binding,
                    iterable,
                    body,
                } => {
                    let element = self.iterable(env, iterable, depth + 1)?;
                    let mut inner = self.environment_copy(env, span)?;
                    self.bind(&mut inner, binding, Value::typed(element), false, span)?;
                    self.block(&mut inner, body, depth + 1)?;
                    self.merge_yield(env, &inner, span)?;
                    self.invalidate_values(env);
                    Ok(false)
                }
                s::MetaStatement::Emit(expr) => {
                    self.expression(
                        env,
                        expr,
                        Some(&Ty::Meta(Meta::Syntax(Category::StmtList))),
                        depth + 1,
                    )?;
                    Ok(false)
                }
            },
            s::StmtKind::Defer(_) | s::StmtKind::With { .. } | s::StmtKind::Unsafe(_) => Err(bad(
                span,
                "pure CT execution cannot acquire runtime resource, defer, or unsafe obligations",
            )),
        }
    }
    pub(super) fn meta_block(
        &mut self,
        env: &mut Env,
        block: &s::Block,
        expected: Option<&Ty>,
        depth: usize,
    ) -> R<Value> {
        let mut inner = self.environment_copy(env, block.span)?;
        inner.yielding = Some(
            expected
                .map(|ty| self.duplicate_type(ty, block.span))
                .transpose()?,
        );
        inner.loops = 0;
        let terminal = self.block(&mut inner, block, depth + 1)?;
        if !terminal {
            return Err(bad(
                block.span,
                "original CT block expression has a path with no yield",
            ));
        }
        let Some(Some(ty)) = inner.yielding else {
            return Err(bad(
                block.span,
                "original CT block expression has no result type",
            ));
        };
        Ok(Value::typed(ty))
    }
    fn merge_yield(&mut self, target: &mut Env, branch: &Env, span: Span) -> R<()> {
        if let Some(Some(ty)) = &branch.yielding {
            if let Some(Some(expected)) = &target.yielding {
                self.same(ty, expected, span)?;
            } else if target.yielding.is_some() {
                target.yielding = Some(Some(self.duplicate_type(ty, span)?));
            }
        }
        Ok(())
    }
    fn invalidate_values(&self, env: &mut Env) {
        for value in &mut env.values {
            if value.mutable {
                value.value.constant = None;
                value.value.represented = None;
            }
        }
    }
    fn condition(&mut self, env: &mut Env, condition: &s::Condition, depth: usize) -> R<()> {
        match condition {
            s::Condition::Expression(expr) => {
                self.expression(env, expr, Some(&bool_ty()), depth + 1)?;
            }
            s::Condition::Let { pattern, value } => {
                let value = self.expression(env, value, None, depth + 1)?;
                self.pattern(env, pattern, &value, false, depth + 1)?;
            }
        }
        Ok(())
    }
    fn iterable(&mut self, env: &mut Env, expr: &s::Expr, depth: usize) -> R<Ty> {
        if let s::ExprKind::Binary {
            op: s::BinaryOp::Range,
            left,
            right,
        } = &expr.kind
        {
            let hint = self.operand_hint(env, right, depth + 1)?;
            let left = self.expression(env, left, hint.as_ref(), depth + 1)?;
            self.expression(env, right, Some(&left.ty), depth + 1)?;
            if !integer(&left.ty) {
                return Err(bad(
                    expr.span,
                    "CT range endpoints require the same integer carrier",
                ));
            }
            return Ok(left.ty);
        }
        let value = self.expression(env, expr, None, depth + 1)?;
        match value.ty {
            Ty::Array(inner, _) | Ty::List(inner) => Ok(*inner),
            _ => Err(bad(
                expr.span,
                "CT for iterable has no homogeneous Array/List domain",
            )),
        }
    }
    fn bind(
        &mut self,
        env: &mut Env,
        name: &str,
        value: Value,
        mutable: bool,
        span: Span,
    ) -> R<()> {
        self.alloc(name.len() + std::mem::size_of::<Binding>(), span)?;
        self.append(
            &mut env.values,
            Binding {
                name: name.to_owned(),
                value,
                mutable,
            },
            span,
        )?;
        Ok(())
    }
    fn pattern(
        &mut self,
        env: &mut Env,
        pattern: &s::Pattern,
        value: &Value,
        mutable: bool,
        depth: usize,
    ) -> R<()> {
        self.step(pattern.span, depth)?;
        let span = pattern.span;
        self.pattern_names(pattern, &mut BTreeSet::new(), depth + 1)?;
        match &pattern.kind {
            s::PatternKind::Name(name) => {
                let value = self.duplicate_value(value, span)?;
                self.bind(env, name, value, mutable, span)
            }
            s::PatternKind::Wildcard => Ok(()),
            s::PatternKind::Literal(literal) => {
                let literal = self.literal(literal, Some(&value.ty), false, span)?;
                self.same(&literal.ty, &value.ty, span)?;
                Ok(())
            }
            s::PatternKind::Tuple(patterns) => {
                let Ty::Tuple(fields) = &value.ty else {
                    return Err(bad(span, "CT tuple pattern has a nontuple original value"));
                };
                if fields.len() != patterns.len() {
                    return Err(bad(
                        span,
                        "CT tuple pattern arity differs from its original type",
                    ));
                }
                for (pattern, ty) in patterns.iter().zip(fields) {
                    let ty = self.duplicate_type(ty, span)?;
                    self.pattern(env, pattern, &Value::typed(ty), mutable, depth + 1)?;
                }
                Ok(())
            }
            s::PatternKind::Constructor { path, arguments } => {
                if let Ty::Sum(members) = &value.ty {
                    let member = self.named_type(env, path, &[], false, span, depth + 1)?;
                    if members.iter().filter(|ty| **ty == member).count() != 1
                        || arguments.len() != 1
                    {
                        return Err(bad(
                            span,
                            "CT sum pattern has no unique exact original member/payload",
                        ));
                    }
                    return self.pattern(
                        env,
                        &arguments[0],
                        &Value::typed(member),
                        mutable,
                        depth + 1,
                    );
                }
                let Ty::Nominal(id, actuals) = &value.ty else {
                    return Err(bad(
                        span,
                        "CT variant pattern has no original enum identity",
                    ));
                };
                let row = self.declarations[*id];
                let s::ItemKind::Enum(enumeration) = &row.item.kind else {
                    return Err(bad(span, "CT variant pattern selects a nonenum original"));
                };
                if path.len() > 1 {
                    let owner = self.enum_owner(env, &path[..path.len() - 1], depth + 1, span)?;
                    if owner.as_ref().map(|(owner, _)| *owner) != Some(*id) {
                        return Err(bad(
                            span,
                            "CT pattern has another nominal declaration identity",
                        ));
                    }
                }
                let name = path
                    .last()
                    .ok_or_else(|| bad(span, "empty CT variant pattern"))?;
                let variant = enumeration
                    .variants
                    .iter()
                    .find(|row| row.name == *name)
                    .ok_or_else(|| bad(span, "CT variant pattern names no original variant"))?;
                let fields = self.variant_fields(&variant.fields, span)?;
                if fields.len() != arguments.len() {
                    return Err(bad(
                        span,
                        "CT variant pattern field count differs from its original variant",
                    ));
                }
                let mut target = self.instantiate(*id, actuals, depth + 1, span)?;
                for (field, pattern) in fields.iter().zip(arguments) {
                    let ty = self.ty(&mut target, field, false, depth + 1)?;
                    self.pattern(env, pattern, &Value::typed(ty), mutable, depth + 1)?;
                }
                Ok(())
            }
            s::PatternKind::RecordConstructor { path, fields } => {
                let Ty::Nominal(_id, _) = &value.ty else {
                    return Err(bad(
                        span,
                        "CT record pattern has no original nominal identity",
                    ));
                };
                let expected = self.named_type(env, path, &[], false, span, depth + 1)?;
                if expected != value.ty {
                    return Err(bad(
                        span,
                        "CT record pattern selects another original nominal",
                    ));
                }
                let mut names = BTreeSet::new();
                for field in fields {
                    if !names.insert(&field.name) {
                        return Err(bad(
                            field.span,
                            "CT record pattern repeats an original field",
                        ));
                    }
                    let ty = self.field(env, &value.ty, &field.name, depth + 1, field.span)?;
                    self.pattern(env, &field.pattern, &Value::typed(ty), mutable, depth + 1)?;
                }
                Ok(())
            }
            s::PatternKind::Reference { .. }
            | s::PatternKind::Deref { .. }
            | s::PatternKind::ErrorArm { .. } => Err(bad(
                span,
                "pure CT patterns cannot carry runtime borrow or ErrorUnion obligations",
            )),
        }
    }
    fn pattern_names<'p>(
        &mut self,
        pattern: &'p s::Pattern,
        names: &mut BTreeSet<&'p str>,
        depth: usize,
    ) -> R<()> {
        self.step(pattern.span, depth)?;
        match &pattern.kind {
            s::PatternKind::Name(name) => {
                if !names.insert(name) {
                    return Err(bad(
                        pattern.span,
                        "original CT pattern repeats a value binder",
                    ));
                }
            }
            s::PatternKind::Tuple(patterns)
            | s::PatternKind::Constructor {
                arguments: patterns,
                ..
            } => {
                for pattern in patterns {
                    self.pattern_names(pattern, names, depth + 1)?;
                }
            }
            s::PatternKind::RecordConstructor { fields, .. } => {
                for field in fields {
                    self.pattern_names(&field.pattern, names, depth + 1)?;
                }
            }
            s::PatternKind::Reference { pattern, .. }
            | s::PatternKind::Deref { pattern, .. }
            | s::PatternKind::ErrorArm { pattern, .. } => {
                self.pattern_names(pattern, names, depth + 1)?
            }
            _ => {}
        }
        Ok(())
    }
    fn mutable_place(&mut self, env: &mut Env, expr: &s::Expr, depth: usize) -> R<Ty> {
        self.step(expr.span, depth)?;
        match &expr.kind {
            s::ExprKind::Name(name) => {
                let binding = env
                    .value(name)
                    .ok_or_else(|| bad(expr.span, "unknown original CT assignment target"))?;
                if !binding.mutable {
                    return Err(bad(expr.span, "original CT assignment target is immutable"));
                }
                self.duplicate_type(&binding.value.ty, expr.span)
            }
            s::ExprKind::Group(inner) => self.mutable_place(env, inner, depth + 1),
            s::ExprKind::Member { object, member } => {
                let ty = self.mutable_place(env, object, depth + 1)?;
                match member {
                    s::MemberName::Name(name) => self.field(env, &ty, name, depth + 1, expr.span),
                    s::MemberName::TupleIndex(index) => match ty {
                        Ty::Tuple(fields) => fields
                            .into_iter()
                            .nth(*index as usize)
                            .ok_or_else(|| bad(expr.span, "CT write tuple ordinal is absent")),
                        _ => Err(bad(expr.span, "CT projected write is not a tuple")),
                    },
                    _ => Err(bad(
                        expr.span,
                        "CT constrained member write requires its actual complete source relation",
                    )),
                }
            }
            s::ExprKind::Index { object, index } => {
                let ty = self.mutable_place(env, object, depth + 1)?;
                let index = self.expression(env, index, None, depth + 1)?;
                if !integer(&index.ty) {
                    return Err(bad(
                        expr.span,
                        "CT projected write index has a noninteger carrier",
                    ));
                }
                match ty {
                    Ty::Array(inner, _) | Ty::List(inner) => Ok(*inner),
                    _ => Err(bad(expr.span, "CT indexed write needs an Array or List")),
                }
            }
            _ => Err(bad(
                expr.span,
                "CT assignment does not select an original mutable data place",
            )),
        }
    }
    fn has_break(block: &s::Block) -> bool {
        block.statements.iter().any(|statement|match &statement.kind{s::StmtKind::Break=>true,s::StmtKind::If{then_body,else_body,..}=>Self::has_break(then_body)||matches!(else_body,Some(s::ElseBranch::Block(body))if Self::has_break(body)),s::StmtKind::Match{arms,..}=>arms.iter().any(|arm|matches!(&arm.body,s::ArrowBody::Block(body)if Self::has_break(body))),_=>false})
    }
}
