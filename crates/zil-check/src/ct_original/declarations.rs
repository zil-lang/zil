use super::*;

impl<'a> Checker<'a, '_, '_> {
    pub(super) fn check(&mut self) -> R<()> {
        for id in 0..self.declarations.len() {
            let row = self.declarations[id];
            let function = match &row.item.kind {
                s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => function,
                _ => continue,
            };
            let env = self.selection_environment(id, 0)?;
            let mut selected = false;
            for parameter in &function.params {
                selected |= self.ct_type(&env, &parameter.ty, &mut BTreeSet::new(), 0)?;
            }
            if let Some(result) = &function.return_type {
                selected |= self.ct_type(&env, result, &mut BTreeSet::new(), 0)?;
            }
            if selected {
                if matches!(row.item.kind, s::ItemKind::Extern { .. }) {
                    return Err(bad(
                        row.item.span,
                        "compile-time Type/Syntax/Ident cannot cross an external runtime ABI",
                    ));
                }
                self.function(id, 0)?;
            }
        }
        Ok(())
    }
    fn ct_type(
        &mut self,
        env: &Env,
        ty: &s::TypeRef,
        active: &mut BTreeSet<Id>,
        depth: usize,
    ) -> R<bool> {
        self.step(ty.span, depth)?;
        match &ty.kind {
            s::TypeKind::Named { path, args } => {
                if path.len() == 1 && env.actual(&path[0]).is_some() {
                    return Ok(false);
                }
                if path.len() == 1 && (meta(&path[0]).is_some() || path[0] == "Syntax") {
                    return Ok(true);
                }
                let mut found = false;
                for arg in args {
                    if let s::GenericArg::Type(ty) = arg {
                        found |= self.ct_type(env, ty, active, depth + 1)?;
                    }
                }
                if found {
                    return Ok(true);
                }
                if path.len() == 1
                    && (scalar(&path[0]).is_some()
                        || matches!(
                            path[0].as_str(),
                            "ByteBuffer"
                                | "ForeignHandle"
                                | "CWideString"
                                | "TaskI32"
                                | "Managed"
                                | "Desc"
                                | "Store"
                                | "Mutex"
                                | "Task"
                                | "Registration"
                                | "AsyncRegistration"
                        ))
                {
                    return Ok(false);
                }
                // Selection does not interpret an ordinary runtime signature.
                // Only an actual alias/nominal can introduce a hidden CT lane.
                let id = match self.resolve(env, path, ty.span) {
                    Ok(id) => id,
                    Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                        return Err(error);
                    }
                    Err(_) => return Ok(false),
                };
                if let Some(result) = self.ct_catalogue.get(&id) {
                    return Ok(*result);
                }
                if !active.insert(id) {
                    return Ok(false);
                }
                let row = self.declarations[id];
                let target = self.selection_environment(id, depth + 1)?;
                let result = match &row.item.kind {
                    s::ItemKind::TypeAlias(alias) => match &alias.value {
                        Some(value) => self.ct_type(&target, value, active, depth + 1)?,
                        None => false,
                    },
                    s::ItemKind::Aggregate(aggregate) => {
                        let mut yes = false;
                        for field in &aggregate.fields {
                            yes |= self.ct_type(&target, &field.ty, active, depth + 1)?;
                        }
                        yes
                    }
                    s::ItemKind::Enum(enumeration) => {
                        let mut yes = false;
                        for variant in &enumeration.variants {
                            match &variant.fields {
                                s::VariantFields::Tuple(fields) => {
                                    for field in fields {
                                        yes |=
                                            self.ct_type(&target, &field.ty, active, depth + 1)?;
                                    }
                                }
                                s::VariantFields::Record(fields) => {
                                    for field in fields {
                                        yes |=
                                            self.ct_type(&target, &field.ty, active, depth + 1)?;
                                    }
                                }
                                _ => {}
                            }
                        }
                        yes
                    }
                    _ => false,
                };
                active.remove(&id);
                self.ct_catalogue.insert(id, result);
                Ok(result)
            }
            s::TypeKind::Group(inner)
            | s::TypeKind::Reference { inner, .. }
            | s::TypeKind::Array { element: inner, .. }
            | s::TypeKind::Optional(inner)
            | s::TypeKind::ForLifetime { inner, .. } => self.ct_type(env, inner, active, depth + 1),
            s::TypeKind::Tuple(fields) | s::TypeKind::Union(fields) => {
                let mut yes = false;
                for field in fields {
                    yes |= self.ct_type(env, field, active, depth + 1)?;
                }
                Ok(yes)
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                let mut yes = self.ct_type(env, result, active, depth + 1)?;
                for field in params {
                    yes |= self.ct_type(env, field, active, depth + 1)?;
                }
                Ok(yes)
            }
            s::TypeKind::Error { error, result } => {
                Ok(self.ct_type(env, error, active, depth + 1)?
                    | self.ct_type(env, result, active, depth + 1)?)
            }
            _ => Ok(false),
        }
    }
    fn selection_environment(&mut self, id: Id, depth: usize) -> R<Env> {
        let row = self.declarations[id];
        self.step(row.item.span, depth)?;
        let mut env = if let Some(parent) = row.parent {
            self.selection_environment(parent, depth + 1)?
        } else {
            Env {
                owner: id,
                module: row.module,
                generics: vec![],
                values: vec![],
                result: unit(),
                yielding: None,
                loops: 0,
            }
        };
        env.owner = id;
        env.module = row.module;
        // Selection records binder names/kinds only. It never substitutes an
        // invented value or checks an ordinary runtime carrier as a CT type.
        for (ordinal, formal) in formals(row.item).iter().enumerate() {
            let binder = Binder { owner: id, ordinal };
            let actual = match formal.kind {
                s::GenericParamKind::Type(_) => Actual::Type(Ty::Parameter(binder)),
                s::GenericParamKind::Lifetime => Actual::Life(binder),
                s::GenericParamKind::Const(_) => Actual::Const(Constant {
                    carrier: Scalar::Unit,
                    term: Term::Parameter(binder),
                }),
            };
            self.alloc(
                formal.name.len() + std::mem::size_of::<Actual>(),
                formal.span,
            )?;
            self.append(
                &mut env.generics,
                (formal.name.clone(), actual),
                formal.span,
            )?;
        }
        Ok(env)
    }
    pub(super) fn environment(&mut self, id: Id, depth: usize) -> R<Env> {
        let row = self.declarations[id];
        self.step(row.item.span, depth)?;
        let mut env = if let Some(parent) = row.parent {
            self.environment(parent, depth + 1)?
        } else {
            Env {
                owner: id,
                module: row.module,
                generics: vec![],
                values: vec![],
                result: unit(),
                yielding: None,
                loops: 0,
            }
        };
        env.owner = id;
        env.module = row.module;
        let parameters = formals(row.item);
        let mut names = BTreeSet::new();
        for (ordinal, parameter) in parameters.iter().enumerate() {
            self.step(parameter.span, depth)?;
            self.alloc(
                parameter.name.len() + std::mem::size_of::<Actual>(),
                parameter.span,
            )?;
            if !names.insert(parameter.name.as_str()) {
                return Err(bad(parameter.span, "duplicate original CT formal binder"));
            }
            let binder = Binder { owner: id, ordinal };
            let actual = match &parameter.kind {
                s::GenericParamKind::Type(_) => Actual::Type(Ty::Parameter(binder)),
                s::GenericParamKind::Lifetime => Actual::Life(binder),
                s::GenericParamKind::Const(ty) => {
                    let Ty::Scalar(carrier) = self.ty(&mut env, ty, false, depth + 1)? else {
                        return Err(bad(
                            parameter.span,
                            "CT Const formal requires its complete original scalar carrier",
                        ));
                    };
                    if carrier == Scalar::Unit {
                        return Err(bad(
                            parameter.span,
                            "CT Const formal cannot have unit carrier",
                        ));
                    }
                    Actual::Const(Constant {
                        carrier,
                        term: Term::Parameter(binder),
                    })
                }
            };
            self.append(
                &mut env.generics,
                (parameter.name.clone(), actual),
                parameter.span,
            )?;
        }
        Ok(env)
    }
    fn copy_bounds(&mut self, id: Id, env: &Env, depth: usize) -> R<()> {
        let row = self.declarations[id];
        self.step(row.item.span, depth)?;
        if let Some(parent) = row.parent {
            self.copy_bounds(parent, env, depth + 1)?;
        }
        let parameters = formals(row.item);
        let clauses = match &row.item.kind {
            s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => {
                f.where_clause.as_slice()
            }
            s::ItemKind::Impl(i) => i.where_clause.as_slice(),
            _ => &[],
        };
        for parameter in parameters {
            if let s::GenericParamKind::Type(bounds) = &parameter.kind {
                let mut has_copy = false;
                for bound in bounds {
                    self.step(bound.span, depth)?;
                    if !matches!(&bound.kind,s::TypeKind::Named{path,args}if path.as_slice()==["Copy"]&&args.is_empty())
                    {
                        return Err(bad(
                            bound.span,
                            "pure CT Type formal has a bound outside the ordinary Copy domain",
                        ));
                    }
                    has_copy = true;
                }
                for clause in clauses {
                    if let s::BoundSubject::Type(subject) = &clause.subject {
                        if matches!(&subject.kind,s::TypeKind::Named{path,args}if path.as_slice()==[parameter.name.as_str()]&&args.is_empty())
                        {
                            for bound in &clause.bounds {
                                self.step(bound.span, depth)?;
                                if !matches!(&bound.kind,s::TypeKind::Named{path,args}if path.as_slice()==["Copy"]&&args.is_empty())
                                {
                                    return Err(bad(
                                        bound.span,
                                        "pure CT where bound is not ordinary Copy",
                                    ));
                                }
                                has_copy = true;
                            }
                        }
                    }
                }
                if !has_copy {
                    return Err(bad(
                        parameter.span,
                        "original CT Type formal has no ordinary Copy premise",
                    ));
                }
            }
        }
        for clause in clauses {
            self.step(clause.span, depth)?;
            let s::BoundSubject::Type(subject) = &clause.subject else {
                return Err(bad(
                    clause.span,
                    "pure CT where clause has no Type formal subject",
                ));
            };
            if clause.lifetime_bound.is_some()
                || clause.bounds.is_empty()
                || !matches!(&subject.kind,s::TypeKind::Named{path,args}if path.len()==1&&args.is_empty()&&parameters.iter().any(|p|p.name==path[0]&&matches!(p.kind,s::GenericParamKind::Type(_))))
            {
                return Err(bad(
                    clause.span,
                    "CT where premise is outside its original Type binder scope",
                ));
            }
        }
        let _ = env;
        Ok(())
    }
    pub(super) fn function(&mut self, id: Id, depth: usize) -> R<()> {
        if self.checked.contains(&id) || self.checking.contains(&id) {
            return Ok(());
        }
        if self.checked.len().saturating_add(self.checking.len()) >= self.limits.max_functions {
            return Err(Diagnostic::limit(
                Phase::Ct,
                "CT original callable inventory exhausted",
                Some(self.declarations[id].item.span),
            ));
        }
        let row = self.declarations[id];
        self.step(row.item.span, depth)?;
        let s::ItemKind::Function(function) = &row.item.kind else {
            return Err(bad(
                row.item.span,
                "CT call cannot execute an external or nonfunction declaration",
            ));
        };
        if !row.item.attributes.is_empty() || !function.contract.is_empty() {
            return Err(bad(
                row.item.span,
                "pure CT original function has runtime declaration attributes or contracts",
            ));
        }
        let mut env = self.environment(id, depth + 1)?;
        self.copy_bounds(id, &env, depth + 1)?;
        self.checking.insert(id);
        let result = (|| {
            let (parameters, result) = self.signature(id, &mut env, depth + 1)?;
            env.result = self.duplicate_type(&result, function.span)?;
            let mut names = BTreeSet::new();
            for (parameter, ty) in function.params.iter().zip(parameters) {
                if !names.insert(parameter.name.as_str())
                    || parameter.give
                    || !parameter.attributes.is_empty()
                {
                    return Err(bad(
                        parameter.span,
                        "CT value parameter has duplicate name, ownership mode, or attributes",
                    ));
                }
                self.alloc(
                    parameter.name.len() + std::mem::size_of::<Binding>(),
                    parameter.span,
                )?;
                self.append(
                    &mut env.values,
                    Binding {
                        name: parameter.name.clone(),
                        value: Value::typed(ty),
                        mutable: false,
                    },
                    parameter.span,
                )?;
            }
            match &function.body {
                s::FunctionBody::Block(body) => {
                    let returned = self.block(&mut env, body, depth + 1)?;
                    if !returned {
                        self.same(&result, &unit(), body.span)?;
                    }
                }
                s::FunctionBody::Expression(expr) => {
                    self.expression(&mut env, expr, Some(&result), depth + 1)?;
                }
                s::FunctionBody::Declaration => {
                    return Err(bad(
                        function.span,
                        "CT callable declaration has no original body",
                    ));
                }
            }
            Ok(())
        })();
        self.checking.remove(&id);
        if result.is_ok() {
            self.checked.insert(id);
        }
        result
    }
    pub(super) fn signature(&mut self, id: Id, env: &mut Env, depth: usize) -> R<(Vec<Ty>, Ty)> {
        let row = self.declarations[id];
        let s::ItemKind::Function(function) = &row.item.kind else {
            return Err(bad(
                row.item.span,
                "CT signature has no actual function definition",
            ));
        };
        let mut parameters = self.vector(function.params.len(), function.span)?;
        for parameter in &function.params {
            let ty = self.ty(env, &parameter.ty, false, depth + 1)?;
            self.copy_type(&ty, parameter.span, depth + 1)?;
            parameters.push(ty);
        }
        let result = match &function.return_type {
            Some(ty) => self.ty(env, ty, false, depth + 1)?,
            None => unit(),
        };
        self.copy_type(&result, function.span, depth + 1)?;
        Ok((parameters, result))
    }
    pub(super) fn actuals(
        &mut self,
        caller: &mut Env,
        id: Id,
        args: &[s::GenericArg],
        depth: usize,
        span: Span,
    ) -> R<Vec<Actual>> {
        let parameters = formals(self.declarations[id].item);
        if parameters.len() != args.len() {
            return Err(bad(
                span,
                "original CT application omits or adds a mixed formal argument",
            ));
        }
        let mut target = self.environment(id, depth + 1)?;
        let mut actuals = self.vector(parameters.len(), span)?;
        for (parameter, arg) in parameters.iter().zip(args) {
            self.step(span, depth)?;
            let actual = match (&parameter.kind, arg) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    let ty = self.ty(caller, ty, true, depth + 1)?;
                    self.copy_argument(&ty, span, depth + 1)?;
                    Actual::Type(ty)
                }
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    match caller.actual(name) {
                        Some(Actual::Life(binder)) => Actual::Life(*binder),
                        _ => {
                            return Err(bad(
                                span,
                                "CT Life actual has no exact original lifetime binder",
                            ));
                        }
                    }
                }
                (s::GenericParamKind::Const(carrier), s::GenericArg::Const(expr)) => {
                    let carrier = self.ty(&mut target, carrier, false, depth + 1)?;
                    let value = self.expression(caller, expr, Some(&carrier), depth + 1)?;
                    Actual::Const(value.constant.ok_or_else(|| {
                        bad(
                            expr.span,
                            "CT Const actual is not an original scalar expression",
                        )
                    })?)
                }
                // The parser uses a type-shaped identifier in ambiguous scalar
                // actual positions. Resolve it in the Const namespace, never
                // accept its spelling as a Type or a lifetime.
                (s::GenericParamKind::Const(carrier), s::GenericArg::Type(ty)) => {
                    let s::TypeKind::Named { path, args } = &ty.kind else {
                        return Err(bad(span, "CT Const actual has another mixed kind"));
                    };
                    if path.len() != 1 || !args.is_empty() {
                        return Err(bad(span, "CT Const actual has another mixed kind"));
                    }
                    let Some(Actual::Const(value)) = caller.actual(&path[0]) else {
                        return Err(bad(span, "CT Const actual has no original scalar binder"));
                    };
                    let value = self.duplicate_constant(value, span)?;
                    let expected = self.ty(&mut target, carrier, false, depth + 1)?;
                    self.same(&expected, &Ty::Scalar(value.carrier), span)?;
                    Actual::Const(value)
                }
                _ => {
                    return Err(bad(
                        span,
                        "original CT argument changes its full-ordinal Type/Const/Life kind",
                    ));
                }
            };
            if let Some((_, slot)) = target
                .generics
                .iter_mut()
                .rev()
                .find(|(n, _)| n == &parameter.name)
            {
                *slot = self.duplicate_actual(&actual, span)?;
            }
            actuals.push(actual);
        }
        Ok(actuals)
    }
    pub(super) fn instantiate(
        &mut self,
        id: Id,
        actuals: &[Actual],
        depth: usize,
        span: Span,
    ) -> R<Env> {
        let mut env = self.environment(id, depth + 1)?;
        let parameters = formals(self.declarations[id].item);
        if parameters.len() != actuals.len() {
            return Err(bad(
                span,
                "CT original full formal substitution is incomplete",
            ));
        }
        for (parameter, actual) in parameters.iter().zip(actuals) {
            if let Some((_, slot)) = env
                .generics
                .iter_mut()
                .rev()
                .find(|(name, _)| name == &parameter.name)
            {
                *slot = self.duplicate_actual(&actual, span)?;
            }
        }
        Ok(env)
    }
    pub(super) fn ty(
        &mut self,
        env: &mut Env,
        source: &s::TypeRef,
        represented: bool,
        depth: usize,
    ) -> R<Ty> {
        self.step(source.span, depth)?;
        let ty = match &source.kind {
            s::TypeKind::Group(inner) => return self.ty(env, inner, represented, depth + 1),
            s::TypeKind::Tuple(fields) => {
                if fields.is_empty() {
                    unit()
                } else {
                    let mut values = self.vector(fields.len(), source.span)?;
                    for field in fields {
                        values.push(self.ty(env, field, represented, depth + 1)?);
                    }
                    Ty::Tuple(values)
                }
            }
            s::TypeKind::Union(fields) => {
                let mut values = vec![];
                for field in fields {
                    let ty = self.ty(env, field, represented, depth + 1)?;
                    match ty {
                        Ty::Sum(ts) => {
                            for ty in ts {
                                self.append(&mut values, ty, source.span)?;
                            }
                        }
                        ty => self.append(&mut values, ty, source.span)?,
                    }
                }
                let mut unique = vec![];
                for ty in values {
                    self.work(unique.len() + 1, source.span)?;
                    if !unique.contains(&ty) {
                        self.append(&mut unique, ty, source.span)?;
                    }
                }
                if unique.len() < 2 {
                    return Err(bad(
                        source.span,
                        "CT anonymous sum requires two distinct original members",
                    ));
                }
                self.work(unique.len().saturating_mul(unique.len()), source.span)?;
                // Canonicalize the complete private identities without a
                // Debug-string allocation or a spelling-based identity key.
                for i in 1..unique.len() {
                    let mut j = i;
                    while j > 0 {
                        self.type_work(&unique[j - 1], source.span, 0)?;
                        self.type_work(&unique[j], source.span, 0)?;
                        if super::memory::type_order(&unique[j - 1], &unique[j])
                            != std::cmp::Ordering::Greater
                        {
                            break;
                        }
                        unique.swap(j - 1, j);
                        j -= 1;
                    }
                }
                Ty::Sum(unique)
            }
            s::TypeKind::Array { element, length } => {
                self.alloc(std::mem::size_of::<Ty>(), source.span)?;
                let element = Box::new(self.ty(env, element, represented, depth + 1)?);
                match length {
                    Some(expr) => {
                        let value = self.expression(env, expr, None, depth + 1)?;
                        if !integer(&value.ty) {
                            return Err(bad(
                                expr.span,
                                "CT array cardinality has a noninteger original carrier",
                            ));
                        }
                        let mut constant = value.constant.ok_or_else(|| {
                            bad(
                                expr.span,
                                "CT array length lacks its exact scalar expression",
                            )
                        })?;
                        if matches!(constant.term,Term::Integer(n)if n<0||n>u32::MAX as i128) {
                            return Err(bad(
                                expr.span,
                                "CT array length is outside its finite cardinality",
                            ));
                        }
                        constant.carrier = Scalar::Usize;
                        Ty::Array(element, constant)
                    }
                    None => Ty::List(element),
                }
            }
            s::TypeKind::Named { path, args } => {
                self.named_type(env, path, args, represented, source.span, depth + 1)?
            }
            s::TypeKind::Reference {
                mutable,
                inner,
                lifetime,
            } if represented => {
                let mut args = self.vector(2, source.span)?;
                args.push(Actual::Type(self.ty(env, inner, true, depth + 1)?));
                if let Some(name) = lifetime {
                    let Some(Actual::Life(binder)) = env.actual(name) else {
                        return Err(bad(
                            source.span,
                            "represented CT Address lifetime has no original binder",
                        ));
                    };
                    args.push(Actual::Life(*binder));
                }
                Ty::Runtime(if *mutable { "AddressMut" } else { "Address" }, args)
            }
            s::TypeKind::Function { params, result } if represented => {
                let mut args = self.vector(params.len().saturating_add(1), source.span)?;
                for ty in params {
                    args.push(Actual::Type(self.ty(env, ty, true, depth + 1)?));
                }
                args.push(Actual::Type(self.ty(env, result, true, depth + 1)?));
                Ty::Runtime("Function", args)
            }
            s::TypeKind::ForLifetime { lifetime, inner } if represented => {
                let ordinal=env.generics.iter().filter(|(_,actual)|matches!(actual,Actual::Life(binder)if binder.owner==usize::MAX)).count();
                self.alloc(lifetime.len(), source.span)?;
                self.append(
                    &mut env.generics,
                    (
                        lifetime.clone(),
                        Actual::Life(Binder {
                            owner: usize::MAX,
                            ordinal,
                        }),
                    ),
                    source.span,
                )?;
                let result = self.ty(env, inner, true, depth + 1);
                env.generics.pop();
                let mut args = self.vector(1, source.span)?;
                args.push(Actual::Type(result?));
                Ty::Runtime("ForLife", args)
            }
            s::TypeKind::Error { error, result } if represented => {
                let mut args = self.vector(2, source.span)?;
                args.push(Actual::Type(self.ty(env, error, true, depth + 1)?));
                args.push(Actual::Type(self.ty(env, result, true, depth + 1)?));
                Ty::Runtime("ErrorUnion", args)
            }
            s::TypeKind::MetaEval(expr) | s::TypeKind::MetaSplice(expr) => {
                let value = self.expression(env, expr, Some(&Ty::Meta(Meta::Type)), depth + 1)?;
                value.represented.ok_or_else(|| {
                    bad(
                        source.span,
                        "original CT dependent type requires its complete symbolic Type result",
                    )
                })?
            }
            s::TypeKind::MetaBlock(block) => {
                let value = self.meta_block(env, block, Some(&Ty::Meta(Meta::Type)), depth + 1)?;
                value.represented.ok_or_else(|| {
                    bad(
                        source.span,
                        "original CT block Type result is not symbolically determined",
                    )
                })?
            }
            _ => {
                return Err(bad(
                    source.span,
                    "runtime ownership, callable, or trait type is not a pure CT value",
                ));
            }
        };
        Ok(ty)
    }
    pub(super) fn variant_fields<'v>(
        &mut self,
        fields: &'v s::VariantFields,
        span: Span,
    ) -> R<Vec<&'v s::TypeRef>> {
        let count = match fields {
            s::VariantFields::Unit => 0,
            s::VariantFields::Tuple(fields) => fields.len(),
            s::VariantFields::Record(fields) => fields.len(),
        };
        let mut output = self.vector(count, span)?;
        match fields {
            s::VariantFields::Unit => {}
            s::VariantFields::Tuple(fields) => output.extend(fields.iter().map(|field| &field.ty)),
            s::VariantFields::Record(fields) => output.extend(fields.iter().map(|field| &field.ty)),
        };
        Ok(output)
    }
    pub(super) fn copy_argument(&mut self, ty: &Ty, span: Span, depth: usize) -> R<()> {
        self.step(span, depth)?;
        match ty {
            Ty::Runtime("Function" | "Address" | "ForeignHandle" | "CWideString", _) => Ok(()),
            Ty::Runtime("ForLife", arguments) => {
                let [Actual::Type(inner)] = arguments.as_slice() else {
                    return Err(bad(span, "CT quantified Type has incomplete original body"));
                };
                self.copy_argument(inner, span, depth + 1)
            }
            Ty::Tuple(fields) | Ty::Sum(fields) => {
                for field in fields {
                    self.copy_argument(field, span, depth + 1)?;
                }
                Ok(())
            }
            Ty::Array(inner, _) | Ty::List(inner) => self.copy_argument(inner, span, depth + 1),
            Ty::Nominal(id, actuals) => {
                if !self.nominal_active.insert(*id) {
                    return Err(bad(
                        span,
                        "original Copy type argument has recursive physical fields",
                    ));
                }
                let result = (|| {
                    let mut target = self.instantiate(*id, actuals, depth + 1, span)?;
                    match &self.declarations[*id].item.kind {
                        s::ItemKind::Aggregate(aggregate)
                            if aggregate.kind == s::AggregateKind::Struct =>
                        {
                            for field in &aggregate.fields {
                                if field.manages {
                                    return Err(bad(
                                        field.span,
                                        "Copy CT argument has an owning field",
                                    ));
                                }
                                let ty = self.ty(&mut target, &field.ty, true, depth + 1)?;
                                self.copy_argument(&ty, span, depth + 1)?;
                            }
                            Ok(())
                        }
                        s::ItemKind::Enum(enumeration) => {
                            for variant in &enumeration.variants {
                                match &variant.fields {
                                    s::VariantFields::Tuple(fields) => {
                                        for field in fields {
                                            if field.manages {
                                                return Err(bad(
                                                    field.span,
                                                    "Copy CT enum argument has an owning field",
                                                ));
                                            }
                                            let ty =
                                                self.ty(&mut target, &field.ty, true, depth + 1)?;
                                            self.copy_argument(&ty, span, depth + 1)?;
                                        }
                                    }
                                    s::VariantFields::Record(fields) => {
                                        for field in fields {
                                            if field.manages {
                                                return Err(bad(
                                                    field.span,
                                                    "Copy CT enum argument has an owning field",
                                                ));
                                            }
                                            let ty =
                                                self.ty(&mut target, &field.ty, true, depth + 1)?;
                                            self.copy_argument(&ty, span, depth + 1)?;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Ok(())
                        }
                        _ => Err(bad(span, "CT Type actual does not prove ordinary Copy")),
                    }
                })();
                self.nominal_active.remove(id);
                result
            }
            _ => self.copy_type(ty, span, depth + 1),
        }
    }
    pub(super) fn copy_type(&mut self, ty: &Ty, span: Span, depth: usize) -> R<()> {
        self.step(span, depth)?;
        match ty {
            Ty::Scalar(_) | Ty::Meta(_) | Ty::Parameter(_) => Ok(()),
            Ty::Tuple(fields) | Ty::Sum(fields) => {
                for field in fields {
                    self.copy_type(field, span, depth + 1)?;
                }
                Ok(())
            }
            Ty::Array(inner, _) | Ty::List(inner) => self.copy_type(inner, span, depth + 1),
            Ty::Nominal(id, actuals) => {
                if !self.nominal_active.insert(*id) {
                    return Err(bad(
                        span,
                        "recursive CT nominal value has no finite Copy representation",
                    ));
                }
                let result = (|| {
                    let mut env = self.instantiate(*id, actuals, depth + 1, span)?;
                    let row = self.declarations[*id];
                    match &row.item.kind {
                        s::ItemKind::Aggregate(aggregate) => {
                            if aggregate.kind != s::AggregateKind::Struct {
                                return Err(bad(
                                    span,
                                    "CT data value cannot own a resource/package",
                                ));
                            }
                            let mut names = BTreeSet::new();
                            for field in &aggregate.fields {
                                if !names.insert(&field.name)
                                    || field.manages
                                    || !field.attributes.is_empty()
                                {
                                    return Err(bad(
                                        field.span,
                                        "CT nominal field duplicates a name or carries runtime obligations",
                                    ));
                                }
                                let ty = self.ty(&mut env, &field.ty, false, depth + 1)?;
                                self.copy_type(&ty, field.span, depth + 1)?;
                                if let Some(value) = &field.default {
                                    self.expression(&mut env, value, Some(&ty), depth + 1)?;
                                }
                            }
                        }
                        s::ItemKind::Enum(enumeration) => {
                            let mut names = BTreeSet::new();
                            for variant in &enumeration.variants {
                                if !names.insert(&variant.name) || !variant.attributes.is_empty() {
                                    return Err(bad(
                                        variant.span,
                                        "CT enum has duplicate variant or runtime attributes",
                                    ));
                                }
                                match &variant.fields {
                                    s::VariantFields::Tuple(fields) => {
                                        for field in fields {
                                            if field.manages {
                                                return Err(bad(
                                                    field.span,
                                                    "CT enum cannot own a managed field",
                                                ));
                                            }
                                            let ty =
                                                self.ty(&mut env, &field.ty, false, depth + 1)?;
                                            self.copy_type(&ty, field.span, depth + 1)?;
                                        }
                                    }
                                    s::VariantFields::Record(fields) => {
                                        for field in fields {
                                            if field.manages {
                                                return Err(bad(
                                                    field.span,
                                                    "CT enum cannot own a managed field",
                                                ));
                                            }
                                            let ty =
                                                self.ty(&mut env, &field.ty, false, depth + 1)?;
                                            self.copy_type(&ty, field.span, depth + 1)?;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {
                            return Err(bad(
                                span,
                                "CT nominal identity is not its original data declaration",
                            ));
                        }
                    }
                    Ok(())
                })();
                self.nominal_active.remove(id);
                result
            }
            Ty::Runtime(..) => Err(bad(
                span,
                "a represented runtime Type cannot become a pure CT value",
            )),
        }
    }
    pub(super) fn named_type(
        &mut self,
        env: &mut Env,
        path: &[String],
        args: &[s::GenericArg],
        represented: bool,
        span: Span,
        depth: usize,
    ) -> R<Ty> {
        self.step(span, depth)?;
        let result = {
            if path.len() == 1 {
                if path[0] == "Self" && args.is_empty() {
                    if let Some(host) = self.declarations[env.owner].parent {
                        if matches!(
                            self.declarations[host].item.kind,
                            s::ItemKind::Aggregate(_) | s::ItemKind::Enum(_)
                        ) {
                            let mut offset = 0;
                            let mut parent = self.declarations[host].parent;
                            while let Some(id) = parent {
                                self.work(1, span)?;
                                offset += formals(self.declarations[id].item).len();
                                parent = self.declarations[id].parent;
                            }
                            let count = formals(self.declarations[host].item).len();
                            let mut actuals = self.vector(count, span)?;
                            for (_, actual) in env.generics.iter().skip(offset).take(count) {
                                actuals.push(self.duplicate_actual(&actual, span)?);
                            }
                            return Ok(Ty::Nominal(host, actuals));
                        }
                    }
                }
                if let Some(actual) = env.actual(&path[0]) {
                    if !args.is_empty() {
                        return Err(bad(span, "original CT formal is not a type constructor"));
                    }
                    let Actual::Type(ty) = actual else {
                        return Err(bad(span, "original CT type selects a Const or Life formal"));
                    };
                    return self.duplicate_type(ty, span);
                }
                if let Some(scalar) = scalar(&path[0]) {
                    if !args.is_empty() {
                        return Err(bad(span, "CT scalar takes no type arguments"));
                    }
                    return Ok(Ty::Scalar(scalar));
                }
                if let Some(meta) = meta(&path[0]) {
                    if !args.is_empty() {
                        return Err(bad(span, "CT atom takes no type arguments"));
                    }
                    return Ok(Ty::Meta(meta));
                }
                if path[0] == "Syntax" {
                    let [
                        s::GenericArg::Type(s::TypeRef {
                            kind: s::TypeKind::Named { path, args },
                            ..
                        }),
                    ] = args
                    else {
                        return Err(bad(span, "CT Syntax requires its exact original category"));
                    };
                    if path.len() != 1 || !args.is_empty() {
                        return Err(bad(span, "invalid CT syntax category"));
                    }
                    return Ok(Ty::Meta(Meta::Syntax(
                        category(&path[0])
                            .ok_or_else(|| bad(span, "unknown CT syntax category"))?,
                    )));
                }
                if path[0] == "List" {
                    let [s::GenericArg::Type(inner)] = args else {
                        return Err(bad(span, "CT List requires one Type argument"));
                    };
                    self.alloc(std::mem::size_of::<Ty>(), span)?;
                    return Ok(Ty::List(Box::new(self.ty(
                        env,
                        inner,
                        represented,
                        depth + 1,
                    )?)));
                }
                if represented {
                    let runtime = match path[0].as_str() {
                        "ByteBuffer" => Some("ByteBuffer"),
                        "ForeignHandle" => Some("ForeignHandle"),
                        "CWideString" => Some("CWideString"),
                        "Managed" => Some("Managed"),
                        "Desc" => Some("Desc"),
                        "Store" => Some("Store"),
                        _ => None,
                    };
                    if let Some(runtime) = runtime {
                        let count = if matches!(runtime, "Managed" | "Desc" | "Store") {
                            1
                        } else {
                            0
                        };
                        if args.len() != count {
                            return Err(bad(
                                span,
                                "runtime Type constructor has incomplete arguments",
                            ));
                        }
                        let mut values = self.vector(args.len(), span)?;
                        for arg in args {
                            let s::GenericArg::Type(ty) = arg else {
                                return Err(bad(span, "runtime Type constructor expects Type"));
                            };
                            values.push(Actual::Type(self.ty(env, ty, true, depth + 1)?));
                        }
                        return Ok(Ty::Runtime(runtime, values));
                    }
                }
            }
            let id = self.resolve(env, path, span)?;
            let row = self.declarations[id];
            match &row.item.kind {
                s::ItemKind::TypeAlias(alias) => {
                    if !self.aliases.insert(id) {
                        return Err(bad(span, "cyclic original CT type alias"));
                    }
                    let result = (|| {
                        let actuals = self.actuals(env, id, args, depth + 1, span)?;
                        let mut target = self.instantiate(id, &actuals, depth + 1, span)?;
                        self.ty(
                            &mut target,
                            alias
                                .value
                                .as_ref()
                                .ok_or_else(|| bad(span, "CT type alias has no original value"))?,
                            represented,
                            depth + 1,
                        )
                    })();
                    self.aliases.remove(&id);
                    return result;
                }
                s::ItemKind::Aggregate(_) | s::ItemKind::Enum(_) => {
                    let actuals = self.actuals(env, id, args, depth + 1, span)?;
                    self.copy_bounds(id, env, depth + 1)?;
                    Ty::Nominal(id, actuals)
                }
                _ => {
                    return Err(bad(span, "CT type path selects a non-type declaration"));
                }
            }
        };
        Ok(result)
    }
}
