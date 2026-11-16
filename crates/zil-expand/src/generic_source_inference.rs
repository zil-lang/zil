//! Deferred empty-constructor life equations in one original Source body.
//! These variables are never classes or emitted semantic authority.
use super::*;

pub(super) struct Life {
    pub(super) solution: Option<std::rc::Rc<Fact>>,
    span: Span,
}
struct Context {
    expression: usize,
    caller: usize,
    template: TypeRef,
    lives: Vec<(String, Fact)>,
    types: BTreeMap<String, TypeRef>,
}
#[derive(Default)]
pub(super) struct Pending {
    pub(super) lives: Vec<Life>,
    sites: BTreeMap<(usize, usize, Binder), u32>,
    contexts: Vec<Context>,
}
impl Check<'_, '_, '_> {
    pub(super) fn needs_inference(&mut self, function: &Function) -> R<bool> {
        struct Find<'a, 'b, 'c, 'l> {
            check: &'a mut Check<'b, 'c, 'l>,
            found: bool,
        }
        impl crate::ct_selection::Visitor for Find<'_, '_, '_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                self.check.budget.node(span, depth).map_err(|e| vec![e])
            }
            fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
                self.check.budget.text(value, span).map_err(|e| vec![e])
            }
            fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
                if let ExprKind::Call { arguments, .. } = &expression.kind {
                    for argument in arguments {
                        self.found |= self
                            .check
                            .empty_variant(&argument.value, &Facts::new(), depth + 1)
                            .map_err(|e| vec![e])?;
                    }
                }
                crate::ct_selection::walk_expression(self, expression, depth)
            }
        }
        use crate::ct_selection::Visitor;
        let mut find = Find {
            check: self,
            found: false,
        };
        match &function.body {
            FunctionBody::Block(body) => find.block(body, 0),
            FunctionBody::Expression(expression) => find.expression(expression, 0),
            FunctionBody::Declaration => Ok(()),
        }
        .map_err(|mut e| e.remove(0))?;
        Ok(find.found)
    }
    pub(super) fn fresh_life(
        &mut self,
        caller: usize,
        expression: &Expr,
        formal: Binder,
    ) -> R<Fact> {
        let key = (caller, std::ptr::from_ref(expression) as usize, formal);
        self.lookup(self.inference.sites.len(), 4, expression.span)?;
        if let Some(id) = self.inference.sites.get(&key) {
            return Ok(Fact::InferLife(*id));
        }
        let id = u32::try_from(self.inference.lives.len())
            .map_err(|_| error(expression.span, "original call inference domain overflow"))?;
        self.budget.lineage_bytes(
            std::mem::size_of::<Life>() + std::mem::size_of::<((usize, usize, Binder), u32)>(),
            expression.span,
        )?;
        self.inference.lives.push(Life {
            solution: None,
            span: expression.span,
        });
        self.inference.sites.insert(key, id);
        Ok(Fact::InferLife(id))
    }
    pub(super) fn constrain_life(&mut self, id: u32, actual: &Fact, span: Span) -> R<()> {
        self.fact(span, |this| {
            let left = this.resolve_store(&Fact::InferLife(id), span)?;
            let right = this.resolve_store(actual, span)?;
            let Fact::InferLife(root) = left else {
                this.compatible_position(&left, &right, span, true)?;
                // Static is only a directional upper, never an equality.
                return if matches!(right, Fact::Static) {
                    Ok(())
                } else {
                    this.compatible_position(&right, &left, span, true)
                };
            };
            if matches!(right, Fact::InferLife(other) if other == root)
                || matches!(right, Fact::Static)
            {
                return Ok(());
            }
            if !matches!(
                right,
                Fact::InferLife(_) | Fact::Class(_) | Fact::LocalClass(_)
            ) {
                return Err(error(
                    span,
                    "original life inference requires a genuine class constraint",
                ));
            }
            // Both sides are resolved representatives. A life variable can
            // only reference another representative or an atomic original
            // class, so this occurs check cannot create a structural cycle.
            this.budget
                .lineage_bytes(std::mem::size_of::<Fact>(), span)?;
            this.inference.lives[root as usize].solution = Some(std::rc::Rc::new(right));
            Ok(())
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn defer_empty_context(
        &mut self,
        expression: &Expr,
        caller: usize,
        callee: usize,
        template: &TypeRef,
        explicit: Option<&[GenericArg]>,
        map: &Substitutions,
        depth: usize,
    ) -> R<()> {
        self.pay(expression.span, map.len())?;
        if !map
            .values()
            .any(|value| matches!(value, Fact::InferLife(_)))
        {
            return Ok(());
        }
        let mut expression = expression;
        while let ExprKind::Group(inner) = &expression.kind {
            self.budget.node(expression.span, depth)?;
            expression = inner;
        }
        let function = self.functions[callee];
        let mut lives = Vec::new();
        let mut types = BTreeMap::new();
        let full = explicit.is_some_and(|arguments| arguments.len() == function.generics.len());
        let mut nonlife = 0usize;
        for (ordinal, formal) in function.generics.iter().enumerate() {
            self.budget.node(formal.span, depth)?;
            self.budget.text(&formal.name, formal.span)?;
            if matches!(formal.kind, GenericParamKind::Lifetime) {
                let value = map.get(&Binder(callee, ordinal)).ok_or_else(|| {
                    error(
                        expression.span,
                        "original call lifetime substitution is absent",
                    )
                })?;
                let value = self.copy(value, expression.span)?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<(String, Fact)>(), expression.span)?;
                lives.push((formal.name.clone(), value));
            } else {
                let argument = explicit
                    .and_then(|arguments| arguments.get(if full { ordinal } else { nonlife }));
                nonlife += 1;
                let Some(argument) = argument else {
                    return Err(unsupported(
                        expression.span,
                        "deferred enum context needs its complete original non-life actual syntax",
                    ));
                };
                let actual = consts::argument(argument, Some(formal), self.budget)?;
                self.budget.ty(&actual)?;
                types.insert(formal.name.clone(), actual);
            }
        }
        self.budget.ty(template)?;
        self.budget
            .lineage_bytes(std::mem::size_of::<Context>(), expression.span)?;
        self.inference.contexts.push(Context {
            expression: std::ptr::from_ref(expression) as usize,
            caller,
            template: template.clone(),
            lives,
            types,
        });
        Ok(())
    }
    fn life_name(&mut self, caller: usize, value: &Fact, span: Span) -> R<String> {
        let Fact::Class(expected) = self.resolve_store(value, span)? else {
            return Err(unsupported(
                span,
                "deferred life context requires its exact original scoped recipe transport",
            ));
        };
        let function = self.functions[caller];
        let scope = Scope {
            owner: caller,
            parameters: &function.generics,
            host: self.hosts[caller],
            impl_target: self.impl_targets.get(&caller).copied(),
        };
        let mut names = Vec::new();
        for p in &function.generics {
            names.push(p.name.as_str());
        }
        if let Some((_, parameters)) = self.impl_parameters.get(&caller) {
            for p in *parameters {
                names.push(p.name.as_str());
            }
        }
        if let Some(host) = scope.host {
            let record = self.records[host];
            for p in &record.generics {
                names.push(p.name.as_str());
            }
        }
        for name in names {
            if self.binder(&scope, name, span)? == Some((expected, true)) {
                self.budget.text(name, span)?;
                return Ok(name.to_owned());
            }
        }
        Err(error(
            span,
            "inferred lifetime leaves its original caller lexical scope",
        ))
    }
    pub(super) fn finish_inference(&mut self) -> R<BTreeMap<usize, TypeRef>> {
        for index in 0..self.inference.lives.len() {
            let span = self.inference.lives[index].span;
            let actual = self.resolve_store(&Fact::InferLife(index as u32), span)?;
            if matches!(actual, Fact::InferLife(_)) {
                return Err(error(
                    span,
                    "original call lifetime inference is underdetermined",
                ));
            }
        }
        let mut output = BTreeMap::new();
        for mut context in std::mem::take(&mut self.inference.contexts) {
            let mut names = BTreeMap::new();
            for (formal, actual) in &context.lives {
                let name = self.life_name(context.caller, actual, context.template.span)?;
                self.budget.text(formal, context.template.span)?;
                names.insert(formal.clone(), name);
            }
            replace_lives(
                &mut context.template,
                &names,
                &mut Vec::new(),
                self.budget,
                0,
            )?;
            // Insert caller-owned Type/Const syntax only after substituting the
            // original template's lives, so it cannot be captured by a callee
            // lifetime with coincidentally the same spelling.
            self.budget
                .substituted_type(&context.template, &context.types)?;
            Substitution {
                substitutions: &context.types,
            }
            .ty(&mut context.template);
            self.lookup(output.len(), 1, context.template.span)?;
            if output
                .insert(context.expression, context.template)
                .is_some()
            {
                return Err(error(
                    Span::default(),
                    "empty constructor context was produced twice",
                ));
            }
        }
        Ok(output)
    }
}
fn replace_lives(
    ty: &mut TypeRef,
    names: &BTreeMap<String, String>,
    bound: &mut Vec<String>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    fn name(
        value: &mut String,
        names: &BTreeMap<String, String>,
        bound: &[String],
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        budget.text(value, span)?;
        budget.identity_work(bound.len().saturating_add(names.len()), span)?;
        if bound.contains(value) {
            return Ok(());
        }
        if let Some(actual) = names.get(value) {
            if bound.contains(actual) {
                return Err(unsupported(
                    span,
                    "inferred lifetime context would capture an inner binder",
                ));
            }
            budget.text(actual, span)?;
            value.clone_from(actual);
        }
        Ok(())
    }
    match &mut ty.kind {
        TypeKind::ForLifetime { lifetime, inner } => {
            budget.text(lifetime, ty.span)?;
            bound.push(lifetime.clone());
            let result = replace_lives(inner, names, bound, budget, depth + 1);
            bound.pop();
            result?;
        }
        TypeKind::Reference {
            lifetime, inner, ..
        } => {
            if let Some(lifetime) = lifetime {
                name(lifetime, names, bound, budget, ty.span)?;
            }
            replace_lives(inner, names, bound, budget, depth + 1)?;
        }
        TypeKind::Named { args, .. } => {
            for argument in args {
                match argument {
                    GenericArg::Lifetime(lifetime) => {
                        name(lifetime, names, bound, budget, ty.span)?
                    }
                    GenericArg::Type(inner) => {
                        replace_lives(inner, names, bound, budget, depth + 1)?
                    }
                    GenericArg::Const(_) => {}
                    GenericArg::AssociatedBinding(_) => {
                        return Err(unsupported(
                            ty.span,
                            "deferred associated context needs its complete scoped substitution",
                        ));
                    }
                }
            }
        }
        TypeKind::Tuple(fields) | TypeKind::Union(fields) | TypeKind::Impl(fields) => {
            for field in fields {
                replace_lives(field, names, bound, budget, depth + 1)?;
            }
        }
        TypeKind::Group(inner)
        | TypeKind::Optional(inner)
        | TypeKind::Array { element: inner, .. } => {
            replace_lives(inner, names, bound, budget, depth + 1)?
        }
        TypeKind::Error { error, result } => {
            replace_lives(error, names, bound, budget, depth + 1)?;
            replace_lives(result, names, bound, budget, depth + 1)?;
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for parameter in params {
                replace_lives(parameter, names, bound, budget, depth + 1)?;
            }
            replace_lives(result, names, bound, budget, depth + 1)?;
        }
        TypeKind::MetaEval(_) | TypeKind::MetaSplice(_) | TypeKind::MetaBlock(_) => {
            return Err(unsupported(
                ty.span,
                "deferred context requires its evaluated original type",
            ));
        }
        TypeKind::Dyn { .. } => {
            return Err(unsupported(
                ty.span,
                "deferred dyn context needs its complete scoped substitution",
            ));
        }
    }
    Ok(())
}

pub(super) fn materialize<'a>(
    module: &'a Module,
    mut contexts: BTreeMap<usize, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Module>> {
    if contexts.is_empty() {
        return Ok(Cow::Borrowed(module));
    }
    // Coordinates are traversal ordinals of this held original AST, not spans
    // or names. The destination is its paid exact clone, before any rewriting.
    struct Locate<'a, 'b, 'l> {
        contexts: &'a mut BTreeMap<usize, TypeRef>,
        selected: BTreeMap<usize, (Expr, TypeRef)>,
        ordinal: usize,
        budget: &'b mut Budget<'l>,
    }
    impl crate::ct_selection::Visitor for Locate<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            let ordinal = self.ordinal;
            self.ordinal += 1;
            if let Some(context) = self
                .contexts
                .remove(&(std::ptr::from_ref(expression) as usize))
            {
                self.budget
                    .const_expression(expression)
                    .map_err(|e| vec![e])?;
                self.selected.insert(ordinal, (expression.clone(), context));
            }
            crate::ct_selection::walk_expression(self, expression, depth)
        }
    }
    use crate::ct_selection::Visitor;
    let mut locate = Locate {
        contexts: &mut contexts,
        selected: BTreeMap::new(),
        ordinal: 0,
        budget,
    };
    for item in &module.items {
        locate.item(item, 0).map_err(|mut e| e.remove(0))?;
    }
    let total = locate.ordinal;
    let selected = locate.selected;
    if !contexts.is_empty() {
        return Err(error(
            module.span,
            "original empty constructor context is unreachable",
        ));
    }
    let mut cloned = budget.retained_copy(module.span, |budget| {
        for item in &module.items {
            budget.item(item)?;
        }
        Ok(module.clone())
    })?;
    struct Apply<'a, 'l> {
        selected: BTreeMap<usize, (Expr, TypeRef)>,
        ordinal: usize,
        budget: &'a mut Budget<'l>,
    }
    impl crate::generated_spans::Visitor for Apply<'_, '_> {
        fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
            self.budget.node(*span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn expression(&mut self, expression: &mut Expr, depth: usize) -> CompileResult<()> {
            let ordinal = self.ordinal;
            self.ordinal += 1;
            let selected = self.selected.remove(&ordinal);
            if let Some((original, _)) = &selected {
                self.budget
                    .const_expression(expression)
                    .map_err(|e| vec![e])?;
                if &*expression != original
                    || expression.semantic.raw_index() != original.semantic.raw_index()
                {
                    return Err(vec![error(
                        expression.span,
                        "empty constructor context changed its original occurrence",
                    )]);
                }
            }
            crate::generated_spans::walk_expression(self, expression, depth)?;
            if let Some((_, context)) = selected {
                let TypeKind::Named { path, args } = &grouped(&context).kind else {
                    return Err(vec![error(
                        expression.span,
                        "empty constructor needs a complete nominal context",
                    )]);
                };
                let ExprKind::Call { callee, arguments } = &mut expression.kind else {
                    unreachable!()
                };
                let ExprKind::Member { object, .. } = &mut callee.kind else {
                    unreachable!()
                };
                if !arguments.is_empty()
                    || !matches!(&object.kind, ExprKind::Name(name) if path.len() == 1 && path[0] == *name)
                {
                    return Err(vec![error(
                        expression.span,
                        "empty constructor context selects another original nominal",
                    )]);
                }
                self.budget.ty(&context).map_err(|e| vec![e])?;
                let span = object.span;
                self.budget.node(span, depth + 3).map_err(|e| vec![e])?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<Expr>(), span)
                    .map_err(|e| vec![e])?;
                let original = std::mem::replace(
                    object,
                    Box::new(Expr {
                        kind: ExprKind::Tuple(Vec::new()),
                        semantic: Default::default(),
                        span,
                    }),
                );
                **object = Expr {
                    kind: ExprKind::TypeApply {
                        callee: original,
                        arguments: args.clone(),
                    },
                    semantic: Default::default(),
                    span,
                };
            }
            Ok(())
        }
    }
    let mut apply = Apply {
        selected,
        ordinal: 0,
        budget,
    };
    for item in &mut cloned.items {
        crate::generated_spans::Visitor::item(&mut apply, item, 0).map_err(|mut e| e.remove(0))?;
    }
    if apply.ordinal != total || !apply.selected.is_empty() {
        return Err(error(
            module.span,
            "empty constructor source traversal changed during contextualization",
        ));
    }
    Ok(Cow::Owned(cloned))
}
