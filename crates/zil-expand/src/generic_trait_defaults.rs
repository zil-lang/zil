//! Default bodies are checked with symbolic Self before any implementation is
//! selected. Concrete bodies remain ordinary functions in the trait's module.
use super::*;

pub(crate) fn present(declaration: &TraitDecl, budget: &mut Budget<'_>) -> R<bool> {
    let mut found = false;
    for member in &declaration.members {
        budget.node(member.span, 0)?;
        found |= matches!(&member.kind, ItemKind::Function(f)
            if !matches!(f.body, FunctionBody::Declaration));
    }
    Ok(found)
}

impl Engine<'_> {
    pub(super) fn validate_trait_defaults(&mut self, module: &Module) -> R<()> {
        if !self.traits.has_defaults {
            return Ok(());
        }
        for item in &module.items {
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
                self.check_default(declaration, member, &reference, true)?;
            }
        }
        Ok(())
    }

    fn check_default(
        &mut self,
        declaration: &TraitDecl,
        member: &Item,
        reference: &trait_instances::Reference,
        symbolic_arguments: bool,
    ) -> R<BTreeMap<Span, String>> {
        self.budget.item(member)?;
        let ItemKind::Function(mut function) = member.kind.clone() else {
            return Err(error(member.span, "default body is not a method"));
        };
        if !function.generics.is_empty()
            || !function.where_clause.is_empty()
            || !function.contract.is_empty()
        {
            return Err(unsupported(
                member.span,
                "generic or contract-bearing default method",
            ));
        }
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
        let mut bounds = self
            .traits
            .parameter_bounds(&function.generics, &mut self.budget)?;
        trait_instances::copy_key(&reference.key, &mut self.budget, member.span)?;
        for argument in &reference.arguments {
            self.budget.ty(argument)?;
        }
        self.budget.node(member.span, 0)?;
        self.budget.text("Self", member.span)?;
        bounds.insert(
            "Self".into(),
            vec![trait_bounds::Bound::Nominal(reference.clone())],
        );
        let mut calls = trait_instances::Calls::new();
        check::validate_parametric_with_bounds(
            &function,
            check::ParametricTables {
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
        Ok(calls.remove(&function.name).unwrap_or_default())
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
        for member in &declaration.members {
            self.budget.node(member.span, 0)?;
            let ItemKind::Function(method) = &member.kind else {
                continue;
            };
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
            substitute_item(&mut body, &substitutions)?;
            let calls = self.check_default(declaration, &body, &reference, false)?;
            trait_instances::rewrite_calls(&mut body, &calls, &mut self.budget)?;
            self.budget.text("Self", span)?;
            self.budget.ty(&implementation.target)?;
            substitutions.clear();
            substitutions.insert("Self".into(), implementation.target.clone());
            self.budget.substituted_item(&body, &substitutions)?;
            substitute_item(&mut body, &substitutions)?;
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
                self.budget.text(&parameter.name, parameter.span)?;
                args.push(Argument {
                    spread: false,
                    value: Expr {
                        kind: ExprKind::Name(parameter.name.clone()),
                        span: parameter.span,
                    },
                    span: parameter.span,
                });
            }
            self.budget.node(member.span, 0)?;
            self.budget.text(&helper, member.span)?;
            wrapper_function.body = FunctionBody::Expression(Expr {
                kind: ExprKind::Call {
                    callee: Box::new(Expr {
                        kind: ExprKind::Name(helper.clone()),
                        span: member.span,
                    }),
                    arguments: args,
                },
                span: member.span,
            });
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
            self.pending.push_back(body);
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
        hash.update(b"zil/trait-default-body/v1\0");
        hash.update(&reference.key.declaration.file.0.to_le_bytes());
        hash.update(&(reference.key.declaration.start as u64).to_le_bytes());
        hash.update(&(reference.key.declaration.end as u64).to_le_bytes());
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
