use super::*;

impl Checker<'_, '_, '_> {
    pub(super) fn call(
        &mut self,
        env: &mut Env,
        callee: &s::Expr,
        args: &[s::Argument],
        expected: Option<&Ty>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        self.step(span, depth)?;
        let (callee, explicit) = match &callee.kind {
            s::ExprKind::TypeApply { callee, arguments } => {
                (callee.as_ref(), Some(arguments.as_slice()))
            }
            _ => (callee, None),
        };
        let mut host_actuals = None;
        let path = if let s::ExprKind::Member {
            object,
            member: s::MemberName::Name(member),
        } = &callee.kind
        {
            if let s::ExprKind::TypeApply {
                callee: host,
                arguments,
            } = &object.kind
            {
                let mut path = self
                    .path(host, depth + 1)?
                    .ok_or_else(|| bad(span, "CT static host has no original declaration path"))?;
                let host = self.resolve(env, &path, span)?;
                let actuals = self.actuals(env, host, arguments, depth + 1, span)?;
                host_actuals = Some((host, actuals));
                path.push(member.clone());
                path
            } else {
                self.path(callee, depth + 1)?
                    .ok_or_else(|| bad(span, "CT call target is not an original pure function"))?
            }
        } else {
            self.path(callee, depth + 1)?
                .ok_or_else(|| bad(span, "CT call target is not an original pure function"))?
        };
        if let Some(local) = env.value(&path[0]) {
            let _ = local;
            return Err(bad(
                span,
                "a value binding shadows the original CT call target",
            ));
        }
        if path.len() > 1 && env.actual(&path[0]).is_some() {
            return Err(bad(
                span,
                "an original formal shadows the qualified CT module or static host",
            ));
        }
        if path.len() == 1 {
            if let Some(target) = scalar(&path[0]) {
                if explicit.is_some()
                    || args.len() != 1
                    || matches!(target, Scalar::Unit | Scalar::Bool)
                {
                    return Err(bad(
                        span,
                        "CT scalar conversion requires one numeric value and no type arguments",
                    ));
                }
                let value = self.expression(env, &args[0].value, None, depth + 1)?;
                if !number(&value.ty) || args[0].spread {
                    return Err(bad(
                        span,
                        "CT scalar conversion has a nonnumeric original operand",
                    ));
                }
                let constant = value.constant.map(|value| Constant {
                    carrier: target,
                    term: Term::Convert(Box::new(value)),
                });
                return Ok(Value {
                    ty: Ty::Scalar(target),
                    constant,
                    represented: None,
                });
            }
            if env.actual(&path[0]).is_some() {
                return Err(bad(
                    span,
                    "an original Type/Const/Life formal is not a callable",
                ));
            }
        }
        if path.len() == 2 {
            if let Some(import) = self.modules[env.module]
                .imports
                .iter()
                .find(|row| row.alias == path[0])
            {
                if matches!(
                    import.target,
                    CtOriginalImportTarget::Meta | CtOriginalImportTarget::Syntax
                ) {
                    if explicit.is_some() {
                        return Err(bad(
                            span,
                            "closed CT standard operations have no generic actual list",
                        ));
                    }
                    return self.builtin(env, import.target, &path[1], args, depth + 1, span);
                }
            }
        }
        // A variant is resolved by its exact original enum owner. No shape or
        // first matching spelling can choose its identity.
        if path.len() > 1 {
            let owner_path = &path[..path.len() - 1];
            if let Some((owner, known)) = self.enum_owner(env, owner_path, depth + 1, span)? {
                let nominal = host_actuals
                    .as_ref()
                    .filter(|(id, _)| *id == owner)
                    .map(|(_, actuals)| Ty::Nominal(owner, actuals.clone()))
                    .or(known);
                return self.variant(
                    env,
                    owner,
                    path.last().unwrap(),
                    explicit,
                    args,
                    nominal.as_ref().or(expected),
                    depth + 1,
                    span,
                );
            }
        }
        let id = self.resolve(env, &path, span)?;
        let row = self.declarations[id];
        let s::ItemKind::Function(function) = &row.item.kind else {
            return Err(bad(
                span,
                "CT Call selects an external or nonfunction source declaration",
            ));
        };
        self.function(id, depth + 1)?;
        if args.len() != function.params.len() || args.iter().any(|arg| arg.spread) {
            return Err(bad(
                span,
                "CT Call value arity or spread differs from its original signature",
            ));
        }
        let mut target = self.environment(id, depth + 1)?;
        if let Some((host, actuals)) = host_actuals {
            if row.parent != Some(host) {
                return Err(bad(
                    span,
                    "CT call's explicit host is not its actual lexical parent",
                ));
            }
            for (parameter, actual) in formals(self.declarations[host].item).iter().zip(&actuals) {
                if let Some((_, slot)) = target
                    .generics
                    .iter_mut()
                    .find(|(name, _)| name == &parameter.name)
                {
                    *slot = actual.clone();
                }
            }
        }
        let actuals = if let Some(explicit) = explicit {
            self.actuals(env, id, explicit, depth + 1, span)?
        } else {
            if function
                .generics
                .iter()
                .any(|parameter| !matches!(parameter.kind, s::GenericParamKind::Type(_)))
            {
                return Err(bad(
                    span,
                    "mixed CT Call requires its complete explicit Type/Const/Life actual vector",
                ));
            }
            let (patterns, _) = self.signature(id, &mut target, depth + 1)?;
            let mut solutions = BTreeMap::new();
            for (pattern, arg) in patterns.iter().zip(args) {
                let hint = if Self::contains_formal(pattern, id) {
                    None
                } else {
                    Some(pattern)
                };
                let value = self.expression(env, &arg.value, hint, depth + 1)?;
                self.infer(pattern, &value.ty, id, &mut solutions, depth + 1, arg.span)?;
            }
            let mut actuals = vec![];
            for ordinal in 0..function.generics.len() {
                let value = solutions.remove(&ordinal).ok_or_else(|| {
                    bad(
                        span,
                        "unused CT Type formal requires an explicit original actual",
                    )
                })?;
                self.copy_type(&value, span, depth + 1)?;
                actuals.push(Actual::Type(value));
            }
            actuals
        };
        for (parameter, actual) in function.generics.iter().zip(&actuals) {
            if let Some((_, slot)) = target
                .generics
                .iter_mut()
                .rev()
                .find(|(name, _)| name == &parameter.name)
            {
                *slot = actual.clone();
            }
        }
        let (parameters, result) = self.signature(id, &mut target, depth + 1)?;
        for (parameter, arg) in parameters.iter().zip(args) {
            self.expression(env, &arg.value, Some(parameter), depth + 1)?;
        }
        Ok(Value::typed(result))
    }
    pub(super) fn enum_owner(
        &mut self,
        env: &mut Env,
        path: &[String],
        depth: usize,
        span: Span,
    ) -> R<Option<(Id, Option<Ty>)>> {
        let id = match self.resolve(env, path, span) {
            Ok(id) => id,
            Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                return Err(error);
            }
            Err(_) => return Ok(None),
        };
        match &self.declarations[id].item.kind {
            s::ItemKind::Enum(enumeration) => Ok(Some((
                id,
                if enumeration.generics.is_empty() {
                    Some(Ty::Nominal(id, vec![]))
                } else {
                    None
                },
            ))),
            s::ItemKind::TypeAlias(_) => {
                let source = s::TypeRef {
                    kind: s::TypeKind::Named {
                        path: path.to_vec(),
                        args: vec![],
                    },
                    span,
                };
                let ty = self.ty(env, &source, false, depth + 1)?;
                if let Ty::Nominal(owner, _) = &ty {
                    if matches!(self.declarations[*owner].item.kind, s::ItemKind::Enum(_)) {
                        return Ok(Some((*owner, Some(ty))));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
    fn contains_formal(ty: &Ty, owner: Id) -> bool {
        match ty {
            Ty::Parameter(binder) => binder.owner == owner,
            Ty::Tuple(fields) | Ty::Sum(fields) => {
                fields.iter().any(|ty| Self::contains_formal(ty, owner))
            }
            Ty::Array(inner, _) | Ty::List(inner) => Self::contains_formal(inner, owner),
            Ty::Nominal(_, args) | Ty::Runtime(_, args) => args
                .iter()
                .any(|arg| matches!(arg,Actual::Type(ty)if Self::contains_formal(ty,owner))),
            _ => false,
        }
    }
    fn infer(
        &mut self,
        pattern: &Ty,
        actual: &Ty,
        owner: Id,
        solutions: &mut BTreeMap<usize, Ty>,
        depth: usize,
        span: Span,
    ) -> R<()> {
        self.step(span, depth)?;
        if let Ty::Parameter(binder) = pattern {
            if binder.owner == owner {
                if let Some(old) = solutions.get(&binder.ordinal) {
                    return self.same(old, actual, span);
                }
                solutions.insert(binder.ordinal, actual.clone());
                return Ok(());
            }
        }
        match (pattern, actual) {
            (Ty::Tuple(a), Ty::Tuple(b)) | (Ty::Sum(a), Ty::Sum(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    self.infer(a, b, owner, solutions, depth + 1, span)?;
                }
            }
            (Ty::List(a), Ty::List(b)) => self.infer(a, b, owner, solutions, depth + 1, span)?,
            (Ty::Array(a, n), Ty::Array(b, m)) if n == m => {
                self.infer(a, b, owner, solutions, depth + 1, span)?
            }
            (Ty::Nominal(a, x), Ty::Nominal(b, y)) if a == b && x.len() == y.len() => {
                for (x, y) in x.iter().zip(y) {
                    match (x, y) {
                        (Actual::Type(x), Actual::Type(y)) => {
                            self.infer(x, y, owner, solutions, depth + 1, span)?
                        }
                        _ if x == y => {}
                        _ => {
                            return Err(bad(
                                span,
                                "CT inferred nominal changes an original Const/Life actual",
                            ));
                        }
                    }
                }
            }
            _ => self.same(pattern, actual, span)?,
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn variant(
        &mut self,
        env: &mut Env,
        id: Id,
        name: &str,
        explicit: Option<&[s::GenericArg]>,
        arguments: &[s::Argument],
        expected: Option<&Ty>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let row = self.declarations[id];
        let s::ItemKind::Enum(enumeration) = &row.item.kind else {
            unreachable!()
        };
        let actuals = if let Some(explicit) = explicit {
            self.actuals(env, id, explicit, depth + 1, span)?
        } else if let Some(Ty::Nominal(owner, actuals)) = expected {
            if *owner != id {
                return Err(bad(
                    span,
                    "CT enum context has another original declaration",
                ));
            }
            actuals.clone()
        } else if enumeration.generics.is_empty() {
            vec![]
        } else {
            return Err(bad(
                span,
                "CT generic variant needs its exact original nominal arguments",
            ));
        };
        let ty = Ty::Nominal(id, actuals.clone());
        self.copy_type(&ty, span, depth + 1)?;
        let variant = enumeration
            .variants
            .iter()
            .find(|row| row.name == name)
            .ok_or_else(|| bad(span, "CT constructor names no original enum variant"))?;
        let mut target = self.instantiate(id, &actuals, depth + 1, span)?;
        let fields = match &variant.fields {
            s::VariantFields::Unit => vec![],
            s::VariantFields::Tuple(fields) => fields.iter().map(|field| &field.ty).collect(),
            s::VariantFields::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
        };
        if fields.len() != arguments.len() || arguments.iter().any(|arg| arg.spread) {
            return Err(bad(
                span,
                "CT variant value arity differs from its actual original variant",
            ));
        }
        for (field, arg) in fields.iter().zip(arguments) {
            let field = self.ty(&mut target, field, false, depth + 1)?;
            self.expression(env, &arg.value, Some(&field), depth + 1)?;
        }
        Ok(Value::typed(ty))
    }
}
