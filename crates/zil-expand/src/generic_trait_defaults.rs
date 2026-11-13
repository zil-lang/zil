//! Default bodies are checked with symbolic Self before any implementation is
//! selected. Concrete bodies remain ordinary functions in the trait's module.
use super::*;

pub(crate) fn features_with_methods(
    declaration: &TraitDecl,
    budget: &mut Budget<'_>,
) -> R<(bool, bool, bool)> {
    let mut found = false;
    let mut associated = false;
    let mut methods = false;
    for member in &declaration.members {
        budget.node(member.span, 0)?;
        associated |= matches!(member.kind, ItemKind::TypeAlias(_) | ItemKind::Const(_));
        methods |= matches!(&member.kind, ItemKind::Function(f) if !f.generics.is_empty());
        found |= matches!(&member.kind, ItemKind::Function(f)
            if !matches!(f.body, FunctionBody::Declaration));
    }
    Ok((found, associated, methods))
}

fn contains_reference(ty: &TypeRef, budget: &mut Budget<'_>) -> R<bool> {
    use super::super::ct_selection::Visitor;
    struct References<'a, 'l> {
        budget: &'a mut Budget<'l>,
        found: bool,
    }
    impl Visitor for References<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(
                ty.kind,
                TypeKind::Function { .. } | TypeKind::ForeignFunction { .. }
            ) {
                // A thin function pointer does not carry its signature's
                // argument references as borrowed payload.
                return self.node(ty.span, depth);
            }
            self.found |= matches!(ty.kind, TypeKind::Reference { .. });
            super::super::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut scan = References {
        budget,
        found: false,
    };
    scan.ty(ty, 0).map_err(|mut e| e.remove(0))?;
    Ok(scan.found)
}

fn no_new_borrows(item: &Item, budget: &mut Budget<'_>) -> R<()> {
    use super::super::ct_selection::Visitor;
    struct BorrowCheck<'a, 'l>(&'a mut Budget<'l>);
    impl Visitor for BorrowCheck<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.0.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.0.text(text, span).map_err(|e| vec![e])
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            self.node(expression.span, depth)?;
            if matches!(
                expression.kind,
                ExprKind::Unary {
                    op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                    ..
                }
            ) {
                return Err(vec![unsupported(
                    expression.span,
                    "new borrows in default bodies require symbolic source-flow checking",
                )]);
            }
            super::super::ct_selection::walk_expression(self, expression, depth)
        }
    }
    BorrowCheck(budget)
        .item(item, 0)
        .map_err(|mut e| e.remove(0))
}

fn direct_reference_parameter<'a>(
    function: &'a Function,
    expression: &Expr,
    budget: &mut Budget<'_>,
) -> R<Option<&'a Param>> {
    budget.node(expression.span, 0)?;
    if let ExprKind::Group(inner) = &expression.kind {
        return direct_reference_parameter(function, inner, budget);
    }
    let ExprKind::Name(name) = &expression.kind else {
        return Ok(None);
    };
    for parameter in &function.params {
        budget.node(parameter.span, 0)?;
        budget.text(name, expression.span)?;
        budget.text(&parameter.name, parameter.span)?;
        if parameter.name == *name {
            return Ok(
                matches!(grouped(&parameter.ty).kind, TypeKind::Reference { .. })
                    .then_some(parameter),
            );
        }
    }
    Ok(None)
}

impl Engine<'_> {
    pub(super) fn validate_trait_defaults(&mut self, module: &Module) -> R<()> {
        if !self.traits.has_defaults {
            return Ok(());
        }
        for item in &module.items {
            self.budget.context.origin = self.traits.origin(item, &mut self.budget)?;
            let ItemKind::Trait(declaration) = &item.kind else {
                continue;
            };
            for parameter in &declaration.generics {
                self.budget.node(parameter.span, 0)?;
                self.budget.text(&parameter.name, parameter.span)?;
                if self.nominals.contains_key(&parameter.name) {
                    self.nominal_binder_collision = true;
                    return Ok(());
                }
            }
            for member in &declaration.members {
                self.budget.node(member.span, 0)?;
                let ItemKind::Function(method) = &member.kind else {
                    continue;
                };
                if matches!(method.body, FunctionBody::Declaration) {
                    continue;
                }
                let mut args = Vec::new();
                for parameter in &declaration.generics {
                    self.budget.node(parameter.span, 0)?;
                    self.budget.text(&parameter.name, parameter.span)?;
                    args.push(GenericArg::Type(named(&parameter.name, parameter.span)));
                }
                self.budget.text(&declaration.name, item.span)?;
                let application = TypeRef {
                    kind: TypeKind::Named {
                        path: vec![declaration.name.clone()],
                        args,
                    },
                    span: item.span,
                };
                let reference = trait_instances::arguments(
                    item.span,
                    declaration,
                    &application,
                    &mut self.budget,
                )?;
                let (_, selected) = self.check_default(declaration, member, &reference, true)?;
                if !selected.is_empty() {
                    self.budget.text(&reference.key.name, member.span)?;
                    self.budget.text(&method.name, member.span)?;
                    self.budget.lineage_bytes(
                        std::mem::size_of::<((String, String), error_injection::Choices)>(),
                        member.span,
                    )?;
                    self.default_error_arms
                        .insert((reference.key.name.clone(), method.name.clone()), selected);
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_default(
        &mut self,
        declaration: &TraitDecl,
        member: &Item,
        reference: &trait_instances::Reference,
        symbolic_arguments: bool,
    ) -> R<(trait_instances::CallTargets, error_injection::Choices)> {
        let (owner, _) = self.traits.declaration_item(reference);
        let origin = self.template_origin(owner)?;
        self.with_origin(origin, |engine| {
            engine.check_default_inner(declaration, member, reference, symbolic_arguments)
        })
    }
    fn check_default_inner(
        &mut self,
        declaration: &TraitDecl,
        member: &Item,
        reference: &trait_instances::Reference,
        symbolic_arguments: bool,
    ) -> R<(trait_instances::CallTargets, error_injection::Choices)> {
        self.budget.item(member)?;
        let ItemKind::Function(mut function) = member.kind.clone() else {
            return Err(error(member.span, "default body is not a method"));
        };
        if !function.contract.is_empty() {
            return Err(unsupported(
                member.span,
                "generic or contract-bearing default method",
            ));
        }
        let method_parameters = std::mem::take(&mut function.generics);
        if symbolic_arguments {
            for parameter in &declaration.generics {
                self.budget.node(parameter.span, 0)?;
                self.budget.text(&parameter.name, parameter.span)?;
                if let GenericParamKind::Type(bounds) = &parameter.kind {
                    for bound in bounds {
                        self.budget.ty(bound)?;
                    }
                }
            }
            function.generics = declaration.generics.clone();
        }
        self.budget.node(member.span, 0)?;
        self.budget.text("Self", member.span)?;
        function.generics.push(GenericParam {
            name: "Self".into(),
            kind: GenericParamKind::Type(vec![]),
            span: member.span,
        });
        function.generics.extend(method_parameters);
        let mut bounds = self
            .traits
            .parameter_bounds(&function.generics, &mut self.budget)?;
        let key = trait_instances::copy_key(&reference.key, &mut self.budget, member.span)?;
        let mut arguments = Vec::new();
        for argument in &reference.arguments {
            self.budget.ty(argument)?;
            self.budget.node(argument.span, 0)?;
            arguments.push(argument.clone());
        }
        self.budget.node(member.span, 0)?;
        self.budget.text("Self", member.span)?;
        bounds.insert(
            "Self".into(),
            self.traits.inherited_bounds(
                vec![trait_bounds::Bound::Nominal(trait_instances::Reference {
                    key,
                    arguments,
                })],
                "Self",
                &mut self.budget,
                member.span,
            )?,
        );
        if !function.where_clause.is_empty() {
            let mut variables = BTreeSet::new();
            for parameter in type_parameters(&function) {
                self.budget.node(parameter.span, 0)?;
                self.budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            self.traits.where_assumptions(
                &function.where_clause,
                &mut bounds,
                &variables,
                &mut self.budget,
            )?;
        }
        if self.traits.has_associated {
            self.traits
                .associated_bounds(&mut bounds, &mut self.budget)?;
        }
        no_new_borrows(member, &mut self.budget)?;
        self.check_default_reference_result(&function, &bounds)?;
        let mut calls = trait_instances::Calls::new();
        let checked =
            consts::definition(&function, &mut self.budget, self.traits.has_instances, None)?;
        let selected = check::validate_parametric_with_bounds(
            &checked,
            check::ParametricTables {
                ct_types: None,
                source_definition: Some(&function),
                expression_origins: checked.source_ids.as_ref(),
                definition_equations: None,
                nominals: &self.nominals,
                enums: &self.enums,
                signatures: &self.signatures,
                foreign_signatures: &self.foreign_signatures,
                pattern_types: self.pattern_types,
                traits: &self.traits,
            },
            &mut helper_graph::Dependencies::default(),
            &mut self.budget,
            &mut calls,
            Some(bounds),
        )?;
        Ok((calls.remove(&function.name).unwrap_or_default(), selected))
    }

    fn check_default_reference_result(
        &mut self,
        function: &Function,
        bounds: &trait_bounds::Bounds,
    ) -> R<()> {
        let Some(result) = &function.return_type else {
            return Ok(());
        };
        if !contains_reference(result, &mut self.budget)? {
            return Ok(());
        }
        let fail = || {
            unsupported(
                function.span,
                "default reference results require a direct reference input or a declared method delegation with known reference inputs; local aliases, branches and unknown origins require symbolic source-flow checking",
            )
        };
        if !matches!(grouped(result).kind, TypeKind::Reference { .. }) {
            return Err(fail());
        }
        // With no intervening bindings or assignments, these names are the real
        // written parameters. A same-spelled local cannot inherit this authority.
        let expression = match &function.body {
            FunctionBody::Expression(expression) => expression,
            FunctionBody::Block(block) if block.statements.len() == 1 => {
                self.budget.node(block.span, 0)?;
                let StmtKind::Return(Some(expression)) = &block.statements[0].kind else {
                    return Err(fail());
                };
                expression
            }
            _ => return Err(fail()),
        };
        if direct_reference_parameter(function, expression, &mut self.budget)?.is_some() {
            return Ok(());
        }
        let mut expression = expression;
        while let ExprKind::Group(inner) = &expression.kind {
            self.budget.node(expression.span, 0)?;
            expression = inner;
        }
        let ExprKind::Call { callee, arguments } = &expression.kind else {
            return Err(fail());
        };
        let ExprKind::Member {
            object,
            member: MemberName::Name(name),
        } = &callee.kind
        else {
            return Err(fail());
        };
        let Some(parameter) = direct_reference_parameter(function, object, &mut self.budget)?
        else {
            return Err(fail());
        };
        let TypeKind::Reference { inner, .. } = &grouped(&parameter.ty).kind else {
            return Err(fail());
        };
        if trait_bounds::plain(inner) != Some("Self") {
            return Err(fail());
        }
        let method = self
            .traits
            .method(bounds, "Self", name, &mut self.budget, callee.span)?;
        let Some(receiver) = method.params.first().filter(|p| {
            p.name == "self" && matches!(grouped(&p.ty).kind, TypeKind::Reference { .. })
        }) else {
            return Err(fail());
        };
        self.budget.ty(&receiver.ty)?;
        if arguments.len() + 1 != method.params.len() {
            return Err(fail());
        }
        for (argument, parameter) in arguments.iter().zip(method.params.iter().skip(1)) {
            self.budget.node(argument.span, 0)?;
            if contains_reference(&parameter.ty, &mut self.budget)?
                && (argument.spread
                    || direct_reference_parameter(function, &argument.value, &mut self.budget)?
                        .is_none())
            {
                return Err(fail());
            }
        }
        Ok(())
    }

    pub(super) fn complete_trait_defaults(
        &mut self,
        implementation: &mut ImplBlock,
        span: Span,
    ) -> R<()> {
        if !self.traits.has_defaults {
            return Ok(());
        }
        let Some(application) = &implementation.trait_type else {
            return Ok(());
        };
        if trait_instances::head(application)
            .is_none_or(super::super::static_traits::reserved_trait)
        {
            return Ok(());
        }
        let reference = self.traits.reference(application, &mut self.budget)?;
        let (trait_item, declaration) = self.traits.declaration_item(&reference);
        for (ordinal, member) in declaration.members.iter().enumerate() {
            self.budget.node(member.span, 0)?;
            let ItemKind::Function(method) = &member.kind else {
                continue;
            };
            if !method.generics.is_empty() {
                continue;
            }
            if matches!(method.body, FunctionBody::Declaration) {
                continue;
            }
            let mut overridden = false;
            for actual in &implementation.members {
                self.budget.node(actual.span, 0)?;
                if let ItemKind::Function(function) = &actual.kind {
                    self.budget.text(&function.name, actual.span)?;
                    self.budget.text(&method.name, member.span)?;
                    overridden |= function.name == method.name;
                }
            }
            if overridden {
                continue;
            }
            if self
                .instances
                .len()
                .saturating_add(self.retained_stores.len())
                .saturating_add(self.traits.instance_count())
                .saturating_add(self.default_functions)
                .saturating_add(2)
                > self.limits.max_functions.min(512)
            {
                return Err(Diagnostic::limit(
                    Phase::Expand,
                    "default-method function budget exceeded",
                    Some(span),
                ));
            }
            let mut substitutions = BTreeMap::new();
            for (parameter, argument) in declaration.generics.iter().zip(&reference.arguments) {
                self.budget.node(argument.span, 0)?;
                self.budget.text(&parameter.name, argument.span)?;
                self.budget.ty(argument)?;
                substitutions.insert(parameter.name.clone(), argument.clone());
            }
            self.budget.substituted_item(member, &substitutions)?;
            let mut body = member.clone();
            if let ItemKind::Function(function) = &mut body.kind
                && let Some(selected) = self
                    .default_error_arms
                    .get(&(reference.key.name.clone(), method.name.clone()))
            {
                error_injection::apply(function, selected, &mut self.budget)?;
            }
            substitute_item(&mut body, &substitutions)?;
            let (calls, _) = self.check_default(declaration, &body, &reference, false)?;
            self.budget.text("Self", span)?;
            self.budget.ty(&implementation.target)?;
            substitutions.clear();
            substitutions.insert("Self".into(), implementation.target.clone());
            // Receiver-sensitive targets need the real implementation host
            // while selecting their internal callable. A symbolic Self cannot
            // choose a scalar target or a private nominal provider alias.
            trait_instances::rewrite_calls(&mut body, &calls, &substitutions, &mut self.budget)?;
            let anchors = if self.traits.has_generic_methods {
                let resolved = trait_instances::resolve_generic_calls(
                    &calls,
                    &substitutions,
                    &mut self.budget,
                    member.span,
                )?;
                Some(trait_instances::anchor_resolved_calls(
                    &body,
                    resolved,
                    &mut self.budget,
                )?)
            } else {
                None
            };
            self.budget.substituted_item(&body, &substitutions)?;
            substitute_item(&mut body, &substitutions)?;
            let remapped = anchors
                .map(|anchors| {
                    trait_instances::rekey_anchored_calls(&body, anchors, &mut self.budget)
                })
                .transpose()?;
            let helper =
                self.default_body_name(&reference, &implementation.target, method, span)?;
            self.budget.item(&body)?;
            let mut wrapper = body.clone();
            let ItemKind::Function(wrapper_function) = &mut wrapper.kind else {
                unreachable!()
            };
            let mut args = Vec::new();
            for parameter in &wrapper_function.params {
                self.budget.node(parameter.span, 0)?;
                self.budget.node(parameter.span, 0)?;
                self.budget.text(&parameter.name, parameter.span)?;
                args.push(Argument {
                    spread: false,
                    value: Expr {
                        semantic: Default::default(),
                        kind: ExprKind::Name(parameter.name.clone()),
                        span: parameter.span,
                    },
                    span: parameter.span,
                });
            }
            self.budget.node(member.span, 0)?;
            self.budget.node(member.span, 0)?;
            self.budget.text(&helper, member.span)?;
            wrapper_function.body = FunctionBody::Expression(Expr {
                semantic: Default::default(),
                kind: ExprKind::Call {
                    callee: Box::new(Expr {
                        semantic: Default::default(),
                        kind: ExprKind::Name(helper.clone()),
                        span: member.span,
                    }),
                    arguments: args,
                },
                span: member.span,
            });
            if let Some(resolved) = remapped {
                self.budget.text(&helper, member.span)?;
                self.resolved_method_calls.insert(helper.clone(), resolved);
            }
            trait_instances::rename_default_receiver(&mut body, &mut self.budget)?;
            body.public = trait_item.public;
            self.budget.text(&helper, member.span)?;
            let ItemKind::Function(body_function) = &mut body.kind else {
                unreachable!()
            };
            body_function.name = helper.clone();
            self.budget.item(&body)?;
            self.budget.text(&helper, member.span)?;
            let ItemKind::Function(body_function) = &body.kind else {
                unreachable!()
            };
            self.signatures
                .insert(helper, Cow::Owned(body_function.clone()));
            self.budget.node(member.span, 0)?;
            implementation.members.push(wrapper);
            self.budget.node(member.span, 0)?;
            let origin = self
                .template_origin(trait_item)?
                .map(|origin| origin.member(ordinal, member.span))
                .transpose()?;
            self.enqueue(body, origin)?;
            self.default_functions += 2;
        }
        Ok(())
    }

    fn default_body_name(
        &mut self,
        reference: &trait_instances::Reference,
        host: &TypeRef,
        method: &Function,
        span: Span,
    ) -> R<String> {
        let host = type_key(host, &mut self.budget)?;
        trait_instances::copy_key(&reference.key, &mut self.budget, span)?;
        self.budget.text(&method.name, span)?;
        let mut hash = zil_base::FingerprintBuilder::new();
        hash.update(b"zil/trait-default-body/v2\0");
        for part in std::iter::once(&reference.key.name)
            .chain(reference.key.arguments.iter())
            .chain(std::iter::once(&host))
            .chain(std::iter::once(&method.name))
        {
            hash.update(&(part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        self.budget.node(span, 0)?;
        self.budget.text(
            "$trait_default_0000000000000000000000000000000000000000000000000000000000000000",
            span,
        )?;
        Ok(format!("$trait_default_{}", hash.digest()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};

    fn limited<T>(result: CompileResult<T>) {
        let errors = result
            .err()
            .expect("downstream default expansion received a fresh budget");
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }

    #[test]
    fn no_argument_trait_default_alias_engine_and_collector_share_one_ledger() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "default330.zil",
            "type Number=i32;trait Read{fn read(self:&Self)->Number{return 7}}struct P{}impl Read for P{}fn main()->i32{let p=P{};return p.read()-7}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let mut products = products::Products::default();
        super::super::specialize_products(&syntax, &limits, &mut products).unwrap();
        assert!(
            products.trait_instances,
            "no-argument defaults must activate the shared ledger"
        );
        let remaining = products.allowance.as_ref().unwrap();
        let work = limits.max_analysis_steps - remaining.max_analysis_steps;
        let nodes = limits.max_nodes - remaining.max_nodes;
        let (alias, budget) =
            aliases::normalize_with_budget(&syntax, Budget::new(&limits)).unwrap();
        let alias = alias.unwrap();
        let alias_remaining = budget.remaining_limits();
        let mut engine = products::Products::default();
        super::super::specialize_with_budget(
            &alias.module,
            &limits,
            &alias.pattern_types,
            Budget::new(&limits),
            &mut engine,
            (false, false, false), // No associated declarations or generic methods in this fixture.
            None,                  // This retained-source control has no runtime CT callback.
        )
        .unwrap();
        let engine_remaining = engine.allowance.unwrap();
        assert!(
            work >= limits.max_analysis_steps - alias_remaining.max_analysis_steps
                + limits.max_analysis_steps
                - engine_remaining.max_analysis_steps
        );
        assert!(
            nodes
                >= limits.max_nodes - alias_remaining.max_nodes + limits.max_nodes
                    - engine_remaining.max_nodes
        );
        for selected in [
            Limits {
                max_analysis_steps: work,
                ..limits.clone()
            },
            Limits {
                max_nodes: nodes,
                ..limits.clone()
            },
        ] {
            let mut exact = products::Products::default();
            super::super::specialize_products(&syntax, &selected, &mut exact).unwrap();
            limited(crate::expand_with_map_limits(&source, &syntax, &selected));
        }
        crate::expand_with_map_limits(&source, &syntax, &limits).unwrap();
        println!(
            "330 single default specialization work={work}, nodes={nodes}; exhausted remainder rejects collector/publication"
        );
    }

    #[test]
    fn project_trait_default_keeps_normalization_and_downstream_ledger() {
        let sources = [
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import rules;struct P{}impl rules.Read for P{}fn main()->i32{let p=P{};return p.read()-7}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "rules.zil",
                "fn helper()->i32{return 7}pub trait Read{fn read(self:&Self)->i32{return helper()}}",
            ),
        ];
        let syntax = sources
            .iter()
            .map(|s| zil_syntax::parse(s).into_result().unwrap())
            .collect::<Vec<_>>();
        let modules = syntax.iter().collect::<Vec<_>>();
        let imports = [
            vec![zil_ir::CoreImport {
                alias: "rules".into(),
                module: ModuleId(1),
                span: syntax[0].items[0].span,
            }],
            vec![],
        ];
        let limits = Limits::default();
        let mut products = products::Products::default();
        crate::modules::specialize(&sources, &modules, &imports, &limits, &mut products).unwrap();
        assert!(products.trait_instances);
        let remaining = products.allowance.as_ref().unwrap();
        let work = limits.max_analysis_steps - remaining.max_analysis_steps;
        let nodes = limits.max_nodes - remaining.max_nodes;
        for selected in [
            Limits {
                max_analysis_steps: work,
                ..limits.clone()
            },
            Limits {
                max_nodes: nodes,
                ..limits.clone()
            },
        ] {
            let mut exact = products::Products::default();
            crate::modules::specialize(&sources, &modules, &imports, &selected, &mut exact)
                .unwrap();
            limited(crate::expand_program_with_limits(
                &sources, &modules, &imports, &selected,
            ));
        }
        crate::expand_program_with_limits(&sources, &modules, &imports, &limits).unwrap();
        println!(
            "330 project default specialization work={work}, nodes={nodes}; exhausted remainder rejects collector/publication"
        );
    }
}
