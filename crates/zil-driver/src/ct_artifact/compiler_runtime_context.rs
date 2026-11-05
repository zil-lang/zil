//! Runtime specialization recipes remain untrusted until the frozen consumer
//! independently resolves the actual original call and complete vector.
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct RuntimeEntrySource {
    pub definition: CtSymbolId,
    pub caller: CtSymbolId,
    pub call_site: source_sites::SourceSite,
    pub expression_site: source_sites::SourceSite,
    pub arguments: Vec<zil_expand::runtime_ct::RuntimeArgument>,
}

impl<'a> Compiler<'a> {
    pub(super) fn bind_runtime_entry(
        &mut self,
        module: usize,
        expression: &s::Expr,
        source: &RuntimeEntrySource,
    ) -> R<()> {
        use zil_expand::runtime_ct::RuntimeArgument as A;
        let span = expression.span;
        let (tree, frozen) = self.frozen_site_module(source.expression_site, span)?;
        let selected = source_sites::select_in_tree(
            frozen,
            tree,
            source_sites::Selection::Site(source.expression_site),
            self.budget.limits.max_call_depth,
            |work| self.budget.work(work, span),
        )?
        .ok_or_else(|| error(span, "runtime CT expression has no immutable source site"))?;
        if !matches!(selected.node, source_sites::SourceNode::Expr(node) if std::ptr::eq(node, expression))
        {
            return Err(error(
                span,
                "runtime CT entry must compile its exact original template expression",
            ));
        }
        let original = if source.definition.generation_path.is_some() {
            source_scope::generated_original_function(
                self.generated_catalog
                    .ok_or_else(|| error(span, "runtime CT source catalog absent"))?,
                &source.definition,
                self.budget.limits.max_call_depth,
                |work| self.budget.work(work, span),
            )?
            .map(|(_, f)| f)
        } else {
            source_scope::original_function(
                self.modules[module].syntax,
                source.definition.declaration,
                self.budget.limits.max_call_depth,
                |work| self.budget.work(work, span),
            )?
        }
        .ok_or_else(|| error(span, "runtime CT source is not an original function"))?;
        if original.generic_host {
            return Err(error(
                span,
                "runtime CT member requires its separately authenticated host context",
            ));
        }
        if selected
            .function
            .is_none_or(|function| !std::ptr::eq(function, original.function))
        {
            return Err(error(
                span,
                "runtime CT expression is outside its actual template owner",
            ));
        }
        let parameters = &original.function.generics;
        if parameters.len() != source.arguments.len() {
            return Err(error(
                span,
                "runtime CT complete source vector arity differs",
            ));
        }
        let (call_tree, call_module) = self.frozen_site_module(source.call_site, span)?;
        let call = source_sites::select_in_tree(
            call_module,
            call_tree,
            source_sites::Selection::Site(source.call_site),
            self.budget.limits.max_call_depth,
            |work| self.budget.work(work, span),
        )?
        .ok_or_else(|| error(span, "runtime CT original call is absent"))?;
        let source_sites::SourceNode::Expr(call_expression) = call.node else {
            return Err(error(span, "runtime CT call site is not an expression"));
        };
        let s::ExprKind::Call { callee, .. } = &call_expression.kind else {
            return Err(error(span, "runtime CT actual source site is not a call"));
        };
        let s::ExprKind::TypeApply { arguments, .. } = &callee.kind else {
            return Err(error(
                span,
                "runtime CT requires an explicit original source vector",
            ));
        };
        if arguments.len()
            != parameters
                .iter()
                .filter(|p| !matches!(p.kind, s::GenericParamKind::Lifetime))
                .count()
        {
            return Err(error(
                span,
                "runtime CT original explicit Type/Const vector arity differs",
            ));
        }
        let caller_module = self.module_index(source.caller.module, span)?;
        self.source_life_scope =
            self.source_life_scope(caller_module, &source.caller, call_expression.span)?;
        let mut actuals = Vec::new();
        Budget::take(
            &mut self.budget.bytes,
            parameters
                .len()
                .saturating_mul(std::mem::size_of::<Option<CtScopeActual>>()),
            span,
            "runtime CT pending actual vector exhausted",
        )?;
        actuals.reserve_exact(parameters.len());
        let mut explicit = arguments.iter();
        for (ordinal, (parameter, recipe)) in parameters.iter().zip(&source.arguments).enumerate() {
            self.budget.node(span)?;
            if let s::GenericParamKind::Lifetime = parameter.kind {
                if !matches!(recipe, A::LifeDefinition { ordinal: selected } if *selected as usize == ordinal)
                {
                    return Err(error(
                        span,
                        "runtime CT lifetime slot does not name its original definition ordinal",
                    ));
                }
                actuals.push(None);
                continue;
            }
            let argument = explicit
                .next()
                .ok_or_else(|| error(span, "runtime CT explicit source vector ended early"))?;
            actuals.push(Some(match (&parameter.kind, argument, recipe) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty), A::Type(_)) => {
                    self.generic_names(std::slice::from_ref(parameter), &[], parameter.span)?;
                    let graph = self.metadata_type(caller_module, ty, 0)?;
                    self.metadata_copy(&graph, 0, span)?;
                    CtScopeActual::Type(graph)
                }
                (s::GenericParamKind::Const(_), argument, A::Const(_)) => {
                    CtScopeActual::Scalar(self.source_scalar_argument(parameter, argument, span)?)
                }
                _ => {
                    return Err(error(
                        span,
                        "runtime CT recipe kind differs from the original full ordinal",
                    ));
                }
            }));
        }
        self.source_life_scope = self.source_life_scope(module, &source.definition, span)?;
        let scope = self
            .source_life_scope
            .as_ref()
            .ok_or_else(|| error(span, "runtime CT template scope is absent"))?;
        let base = CtAmbientLifeScope {
            declaration: source.definition.clone(),
            enclosing: None,
            binder_site: Vec::new(),
            instance: CtScopeInstance::Definition,
        };
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.bytes,
            std::mem::size_of::<CtAmbientLifeScope>(),
            span,
            "runtime CT definition row exhausted",
        )?;
        let base_id = CtAmbientScopeId(
            u32::try_from(self.ambient_life_scopes.len())
                .map_err(|_| error(span, "runtime CT definition row overflow"))?,
        );
        let _ = scope;
        self.ambient_life_scopes.push(base);
        for (ordinal, slot) in actuals.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(CtScopeActual::Lifetime(CtAmbientLifeReference {
                    scope: base_id,
                    parameter: ordinal as u32,
                }));
            }
        }
        Budget::take(
            &mut self.budget.bytes,
            actuals
                .len()
                .saturating_mul(std::mem::size_of::<CtScopeActual>()),
            span,
            "runtime CT complete actual vector exhausted",
        )?;
        let actuals = actuals.into_iter().map(Option::unwrap).collect::<Vec<_>>();
        self.bindings.clear();
        self.metadata_bindings.clear();
        self.scalar_bindings.clear();
        for (parameter, actual) in parameters.iter().zip(&actuals) {
            self.budget.text(&parameter.name, parameter.span)?;
            match actual {
                CtScopeActual::Type(graph) => {
                    self.budget.metadata(graph, span)?;
                    self.metadata_bindings
                        .push((parameter.name.clone(), graph.clone()));
                }
                CtScopeActual::Scalar(value) => {
                    self.budget.work(32, span)?;
                    self.scalar_bindings
                        .push((parameter.name.clone(), value.clone()));
                }
                CtScopeActual::Lifetime(_) => {}
            }
        }
        Self::actual_cost(&mut self.budget, &actuals, span)?;
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.bytes,
            std::mem::size_of::<CtAmbientLifeScope>(),
            span,
            "runtime CT instance row exhausted",
        )?;
        self.ambient_life_scopes.push(CtAmbientLifeScope {
            declaration: source.definition.clone(),
            enclosing: None,
            binder_site: Vec::new(),
            instance: CtScopeInstance::Instantiated(actuals.clone()),
        });
        self.source_life_scope.as_mut().unwrap().instance = CtScopeInstance::Instantiated(actuals);
        Ok(())
    }
}

pub(crate) fn check_runtime_definition_expression<'a>(
    modules: &'a [SourceModule<'a>],
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
    definition: &CtSymbolId,
    site: source_sites::SourceSite,
    limits: &mut Limits,
) -> R<(s::TypeRef, u64)> {
    let mut compiler = Compiler::indexed(modules, limits)?;
    compiler.generated_catalog = catalog;
    compiler.index_generated()?;
    compiler.checking_generics = true;
    let result = (|| {
        let (tree, source) = compiler.frozen_site_module(site, Span::default())?;
        let location = source_sites::select_in_tree(
            source,
            tree,
            source_sites::Selection::Site(site),
            compiler.budget.limits.max_call_depth,
            |work| compiler.budget.work(work, source.span),
        )?
        .ok_or_else(|| {
            error(
                source.span,
                "runtime CT definition expression source is absent",
            )
        })?;
        let source_sites::SourceNode::Expr(expression) = location.node else {
            return Err(error(
                source.span,
                "runtime CT definition site is not an expression",
            ));
        };
        let mut declaration = None;
        for index in 0..compiler.declarations.len() {
            let row = compiler.declarations[index];
            if compiler.item_symbol(row.module, row.item)? == *definition {
                if declaration.replace(index).is_some() {
                    return Err(error(
                        expression.span,
                        "runtime CT definition identity repeats",
                    ));
                }
            }
        }
        let declaration = declaration.ok_or_else(|| {
            error(
                expression.span,
                "runtime CT definition is not a frozen source function",
            )
        })?;
        let d = compiler.declarations[declaration];
        if location
            .function
            .is_none_or(|function| !std::ptr::eq(function, d.function))
        {
            return Err(error(
                expression.span,
                "runtime CT definition expression belongs to another function",
            ));
        }
        let names =
            compiler.generic_names(&d.function.generics, &d.function.where_clause, d.item.span)?;
        for name in names {
            compiler.budget.node(expression.span)?;
            compiler.budget.text(&name, expression.span)?;
            let ty = CtTy::Record(CtRecordSchemaId(u32::MAX - compiler.symbolic.len() as u32));
            compiler.budget.copy_ty(&ty, 0, expression.span)?;
            compiler.symbolic.push(ty.clone());
            compiler.bindings.push((name, ty));
        }
        compiler.prepare_abstract_scope(declaration)?;
        compiler.bind_abstract_host(declaration)?;
        let mut parameters = Vec::new();
        for parameter in &d.function.generics {
            if let s::GenericParamKind::Const(domain) = &parameter.kind {
                compiler.budget.text(&parameter.name, parameter.span)?;
                let domain = compiler.scalar_argument_type(domain, parameter.span)?;
                compiler.budget.node(parameter.span)?;
                parameters.push((parameter.name.clone(), CtTy::Scalar(domain)));
            }
        }
        compiler.budget.node(expression.span)?;
        let query_id = CtFunctionId(compiler.functions.len() as u32);
        compiler.functions.push(None);
        compiler.current_source_function = Some(query_id);
        let body = cfg::expression(
            &mut compiler,
            query_id,
            d.module,
            definition.clone(),
            expression,
            None,
            &parameters,
        )?;
        let before_output = compiler.budget.bytes;
        let ty = compiler.runtime_computed_type(&body.result_type, expression.span, 0)?;
        Ok((
            ty,
            before_output.saturating_sub(compiler.budget.bytes) as u64,
        ))
    })();
    let remaining = compiler.budget.declared();
    drop(compiler);
    limits.max_analysis_steps = remaining.max_analysis_work as usize;
    limits.max_nodes = remaining.max_generated_nodes as usize;
    limits.max_source_bytes = remaining.max_temporary_bytes as usize;
    limits.max_functions = remaining.max_instances as usize;
    result
}

impl Compiler<'_> {
    pub(super) fn runtime_computed_type(
        &mut self,
        ty: &CtTy,
        span: Span,
        depth: usize,
    ) -> R<s::TypeRef> {
        self.budget.depth(depth, span)?;
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.bytes,
            std::mem::size_of::<s::TypeRef>(),
            span,
            "runtime CT computed type AST exhausted",
        )?;
        let kind = match ty {
            CtTy::Scalar(CtScalarTy::Unit) => s::TypeKind::Tuple(Vec::new()),
            CtTy::Scalar(scalar) => {
                let name = match scalar {
                    CtScalarTy::Unit => unreachable!(),
                    CtScalarTy::Bool => "bool",
                    CtScalarTy::U8 => "u8",
                    CtScalarTy::U16 => "u16",
                    CtScalarTy::I32 => "i32",
                    CtScalarTy::I64 => "i64",
                    CtScalarTy::U64 => "u64",
                    CtScalarTy::Usize => "usize",
                    CtScalarTy::F32 => "f32",
                    CtScalarTy::F64 => "f64",
                };
                self.budget.text(name, span)?;
                s::TypeKind::Named {
                    path: vec![name.into()],
                    args: Vec::new(),
                }
            }
            CtTy::Type => {
                self.budget.text("Type", span)?;
                s::TypeKind::Named {
                    path: vec!["Type".into()],
                    args: Vec::new(),
                }
            }
            CtTy::Tuple(parts) => {
                Budget::take(
                    &mut self.budget.bytes,
                    parts
                        .len()
                        .saturating_mul(std::mem::size_of::<s::TypeRef>()),
                    span,
                    "runtime CT tuple type backing exhausted",
                )?;
                let mut fields = Vec::with_capacity(parts.len());
                for part in parts {
                    fields.push(self.runtime_computed_type(part, span, depth + 1)?);
                }
                s::TypeKind::Tuple(fields)
            }
            CtTy::Array { element, length } => {
                Budget::take(
                    &mut self.budget.bytes,
                    40 + std::mem::size_of::<s::Expr>(),
                    span,
                    "runtime CT array length AST exhausted",
                )?;
                s::TypeKind::Array {
                    element: Box::new(self.runtime_computed_type(element, span, depth + 1)?),
                    length: Some(Box::new(s::Expr {
                        span,
                        kind: s::ExprKind::Literal(s::Literal::Integer(length.to_string())),
                    })),
                }
            }
            CtTy::Record(_) if self.is_symbolic(ty) => {
                let mut name = None;
                for (bound, actual) in self.bindings.iter().rev() {
                    self.budget.work(1, span)?;
                    if actual == ty {
                        self.budget.text(bound, span)?;
                        name = Some(bound.clone());
                        break;
                    }
                }
                s::TypeKind::Named {
                    path: vec![name.ok_or_else(|| {
                        error(span, "computed symbolic CT result lost its real binder")
                    })?],
                    args: Vec::new(),
                }
            }
            CtTy::Record(schema) => {
                if let Some(ty) = self.runtime_abstract_nominal_type(*schema, span, depth + 1)? {
                    return Ok(ty);
                }
                return Err(error(
                    span,
                    "computed runtime CT record has no independently derived abstract nominal identity",
                ));
            }
            _ => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "computed runtime CT definition result requires complete nominal source reification",
                    Some(span),
                )]);
            }
        };
        Ok(s::TypeRef { span, kind })
    }
}
