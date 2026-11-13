//! Ordinary generic inherent methods normalize to ordinary checked templates.
//! This private dispatch inventory selects source declarations, never loans.
use super::*;

pub(super) struct Method {
    host: String,
    member: String,
    template: String,
    // Impl ordinary binder -> actual nominal declaration binder.
    arguments: Vec<(String, String)>,
    receiver: Option<UnaryOp>,
    module: zil_base::ModuleId,
    public: bool,
    scoped: bool,
}
pub(super) struct Prepared {
    pub(super) module: Module,
    pub(super) methods: Vec<Method>,
}
fn selected(item: &Item) -> bool {
    matches!(&item.kind, ItemKind::Impl(i) if i.trait_type.is_none() && !i.generics.is_empty())
}

pub(super) fn prepare(
    module: &Module,
    enabled: bool,
    mut origins: Option<&mut origins::Origins>,
    budget: &mut Budget<'_>,
) -> R<Option<Prepared>> {
    // The caller has already visited every item in the ordinary preflight.
    if !enabled {
        return Ok(None);
    }
    for item in &module.items {
        budget.item(item)?;
    }
    let mut prepared = Module {
        id: module.id,
        span: module.span,
        items: Vec::new(),
    };
    let mut output_origins = Vec::new();
    let mut methods = Vec::<Method>::new();
    for (ordinal, item) in module.items.iter().enumerate() {
        let origin = origins
            .as_ref()
            .ok_or_else(|| error(item.span, "inherent source provenance absent"))?
            .get(ordinal, item.span)?;
        if !selected(item) {
            budget.node(item.span, 0)?;
            prepared.items.push(item.clone());
            output_origins.push(origin);
            continue;
        }
        let ItemKind::Impl(implementation) = &item.kind else {
            unreachable!()
        };
        if !item.attributes.is_empty()
            || implementation.receiver_default.is_some()
            || !implementation.where_clause.is_empty()
        {
            return Err(unsupported(
                item.span,
                "generic inherent attributes, default receiver or where premises",
            ));
        }
        let TypeKind::Named { path, args } = &grouped(&implementation.target).kind else {
            return Err(unsupported(
                item.span,
                "generic inherent target requires a complete nominal application",
            ));
        };
        let [host] = path.as_slice() else {
            return Err(unsupported(
                item.span,
                "generic inherent target requires one resolved nominal name",
            ));
        };
        let mut declaration = None;
        for (index, candidate) in module.items.iter().enumerate() {
            budget.node(candidate.span, 0)?;
            if let ItemKind::Aggregate(aggregate) = &candidate.kind {
                budget.text(host, item.span)?;
                budget.text(&aggregate.name, candidate.span)?;
                if aggregate.name == *host {
                    declaration = Some((index, candidate, aggregate));
                    break;
                }
            }
        }
        let (host_index, host_item, aggregate) = declaration
            .ok_or_else(|| error(item.span, "generic inherent target declaration absent"))?;
        if aggregate.kind != AggregateKind::Struct
            || aggregate.generics.len() != args.len()
            || implementation.generics.len() != args.len()
        {
            return Err(unsupported(
                item.span,
                "generic inherent target requires all struct type/life parameters exactly once",
            ));
        }
        let host_origin = origins
            .as_ref()
            .expect("checked source ledger")
            .get(host_index, host_item.span)?;
        if host_origin.declaration.module != origin.declaration.module {
            return Err(error(
                item.span,
                "inherent impl requires its outer nominal declaration in the current module",
            ));
        }
        let mut names = BTreeSet::new();
        let mut ordinary = BTreeMap::new();
        for (formal, actual) in aggregate.generics.iter().zip(args) {
            budget.node(formal.span, 0)?;
            let name = match (&formal.kind, actual) {
                (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => name,
                (GenericParamKind::Type(_), GenericArg::Type(ty)) => trait_bounds::plain(ty)
                    .ok_or_else(|| {
                        unsupported(
                            ty.span,
                            "inherent target type argument must be a direct impl binder",
                        )
                    })?,
                _ => {
                    return Err(unsupported(
                        item.span,
                        "generic inherent target argument kind or const parameter",
                    ));
                }
            };
            let mut found = None;
            for parameter in &implementation.generics {
                budget.node(parameter.span, 0)?;
                budget.text(name, item.span)?;
                budget.text(&parameter.name, parameter.span)?;
                if parameter.name == name {
                    found = Some(parameter);
                    break;
                }
            }
            let parameter = found.ok_or_else(|| {
                error(item.span, "inherent target argument is not an impl binder")
            })?;
            budget.text(name, item.span)?;
            if !names.insert(name) {
                return Err(unsupported(
                    item.span,
                    "generic inherent target repeats an impl binder",
                ));
            }
            match (&formal.kind, &parameter.kind) {
                (GenericParamKind::Lifetime, GenericParamKind::Lifetime) => {}
                (GenericParamKind::Type(_), GenericParamKind::Type(bounds))
                    if bounds.iter().all(copy::is_bound) =>
                {
                    budget.text(name, item.span)?;
                    budget.text(&formal.name, formal.span)?;
                    ordinary.insert(name.to_owned(), formal.name.clone());
                }
                _ => {
                    return Err(unsupported(
                        item.span,
                        "generic inherent binder requires its declared type/life kind and Copy bounds",
                    ));
                }
            }
        }
        for (member_index, member) in implementation.members.iter().enumerate() {
            budget.item(member)?;
            let ItemKind::Function(function) = &member.kind else {
                return Err(unsupported(
                    member.span,
                    "generic inherent non-function member",
                ));
            };
            if !member.attributes.is_empty()
                || !function.generics.is_empty()
                || !function.where_clause.is_empty()
                || function.params.iter().any(|p| !p.attributes.is_empty())
            {
                return Err(unsupported(
                    member.span,
                    "generic inherent method attributes, own generics or where premises",
                ));
            }
            if matches!(function.body, FunctionBody::Declaration) {
                return Err(error(
                    member.span,
                    "generic inherent method requires a body",
                ));
            }
            for field in &aggregate.fields {
                budget.node(field.span, 0)?;
                budget.text(&field.name, field.span)?;
                budget.text(&function.name, member.span)?;
                if field.name == function.name {
                    return Err(error(
                        member.span,
                        "inherent field and method names must be distinct",
                    ));
                }
            }
            for previous in &methods {
                budget.text(&previous.host, member.span)?;
                budget.text(host, member.span)?;
                budget.text(&previous.member, member.span)?;
                budget.text(&function.name, member.span)?;
                if previous.host == *host && previous.member == function.name {
                    return Err(error(
                        member.span,
                        "duplicate generic inherent method for the same nominal host",
                    ));
                }
            }
            let receiver = function
                .params
                .first()
                .filter(|p| p.name == "self")
                .ok_or_else(|| {
                    unsupported(
                        member.span,
                        "generic inherent method requires an explicit self receiver",
                    )
                })?;
            let (receiver_type, borrow) = match &grouped(&receiver.ty).kind {
                TypeKind::Reference { mutable, inner, .. } => (
                    inner.as_ref(),
                    Some(if *mutable {
                        UnaryOp::BorrowMut
                    } else {
                        UnaryOp::Borrow
                    }),
                ),
                _ => (&receiver.ty, None),
            };
            if trait_bounds::plain(receiver_type) != Some("Self") || receiver.give {
                return Err(unsupported(
                    receiver.span,
                    "generic inherent receiver must be Self, &Self or &mut Self",
                ));
            }
            budget.identity_work(80, member.span)?;
            let template = format!(
                "$inherent_{}_{}_{}",
                origin.declaration.module.0, origin.declaration.ordinal, member_index
            );
            budget.text(&template, member.span)?;
            budget.text("Self", member.span)?;
            budget.ty(&implementation.target)?;
            let substitutions = BTreeMap::from([("Self".into(), implementation.target.clone())]);
            budget.substituted_item(member, &substitutions)?;
            let mut lifted = member.clone();
            // A public factory may return a private nominal, but it cannot
            // make that nominal's inherent members externally callable.
            lifted.public = host_item.public && member.public;
            substitute_item(&mut lifted, &substitutions)?;
            let ItemKind::Function(lifted_function) = &mut lifted.kind else {
                unreachable!()
            };
            for parameter in &implementation.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                if let GenericParamKind::Type(bounds) = &parameter.kind {
                    for bound in bounds {
                        budget.ty(bound)?;
                    }
                }
            }
            lifted_function.generics = implementation.generics.clone();
            lifted_function.name.clone_from(&template);
            // The lifted item is now an ordinary free function. Preserve the
            // actual receiver at parameter zero, but lower its reserved source
            // binding with the existing prepaid AST receiver renamer. Local
            // binding patterns are untouched and still reject source `self`
            // rebinding; member labels and literal contents are not renamed.
            trait_instances::rename_default_receiver(&mut lifted, budget)?;
            let mut arguments = Vec::new();
            for parameter in &implementation.generics {
                if let Some(formal) = ordinary.get(&parameter.name) {
                    budget.text(&parameter.name, parameter.span)?;
                    budget.text(formal, parameter.span)?;
                    arguments.push((parameter.name.clone(), formal.clone()));
                }
            }
            budget.text(host, member.span)?;
            budget.text(&function.name, member.span)?;
            methods.push(Method {
                host: host.clone(),
                member: function.name.clone(),
                template,
                arguments,
                receiver: borrow,
                module: origin.declaration.module,
                public: host_item.public && member.public,
                scoped: implementation
                    .generics
                    .iter()
                    .any(|p| matches!(p.kind, GenericParamKind::Lifetime)),
            });
            prepared.items.push(lifted);
            output_origins.push(origin.member(member_index, member.span)?);
        }
    }
    origins.as_mut().expect("source ledger").entries = output_origins;
    Ok(Some(Prepared {
        module: prepared,
        methods,
    }))
}

impl Engine<'_> {
    fn prepare_inherent_signature(&mut self, name: &str, span: Span) -> R<()> {
        self.budget.text(name, span)?;
        self.stored_lookup_fee(self.products.storage.functions.len(), name.len(), span)?;
        if self.products.storage.functions.contains_key(name) {
            return Ok(());
        }
        let mut selected = None;
        for (index, pending) in self.pending.iter().enumerate() {
            self.budget.node(pending.item.span, 0)?;
            if let ItemKind::Function(function) = &pending.item.kind {
                self.budget.text(&function.name, span)?;
                self.budget.text(name, span)?;
                if function.name == name {
                    selected = Some(index);
                    break;
                }
            }
        }
        let index =
            selected.ok_or_else(|| error(span, "inherent concrete function header absent"))?;
        let origin = self.pending[index].origin;
        let ItemKind::Function(source) = &self.pending[index].item.kind else {
            unreachable!()
        };
        self.budget.function_snapshot(source)?;
        let mut function = source.clone();
        let mut scope = Vec::new();
        for parameter in &function.generics {
            self.budget.node(parameter.span, 0)?;
            if matches!(parameter.kind, GenericParamKind::Lifetime) {
                self.budget.text(&parameter.name, parameter.span)?;
                scope.push(parameter.name.clone());
            }
        }
        let previous_scope = std::mem::replace(&mut self.life_scope, scope);
        let previous_result = self.source_enum_return.take();
        let previous_candidate = self.signature_candidate.take();
        let prepared = self.with_origin(origin, |engine| engine.stored_signature(&mut function));
        let source_result = self.source_enum_return.take();
        let signature_candidate = self.signature_candidate.take();
        self.signature_candidate = previous_candidate;
        self.source_enum_return = previous_result;
        self.life_scope = previous_scope;
        if !prepared? {
            return Err(error(
                span,
                "inherent retained receiver requires its actual storage contract",
            ));
        }
        self.budget.function_snapshot(&function)?;
        self.budget.text(name, span)?;
        self.signatures
            .insert(name.into(), Cow::Owned(function.clone()));
        self.pending[index].item.kind = ItemKind::Function(function);
        self.pending[index].source_enum_return = source_result;
        self.pending[index].signature_candidate = signature_candidate;
        Ok(())
    }

    pub(super) fn record_inherent_instance(
        &mut self,
        generated: &str,
        host: &str,
        actuals: &BTreeMap<String, TypeRef>,
        span: Span,
    ) -> R<()> {
        if self.inherent_methods.is_empty() {
            return Ok(());
        }
        let mut used = false;
        for method in self.inherent_methods {
            self.budget.text(&method.host, span)?;
            self.budget.text(host, span)?;
            used |= method.host == host;
        }
        if !used {
            return Ok(());
        }
        self.budget.identity_work(
            self.inherent_instances
                .len()
                .saturating_add(1)
                .saturating_mul(generated.len().saturating_add(1)),
            span,
        )?;
        if self.inherent_instances.contains_key(generated) {
            return Ok(());
        }
        self.budget.text(generated, span)?;
        self.budget.text(host, span)?;
        for (name, actual) in actuals {
            self.budget.text(name, span)?;
            self.budget.ty(actual)?;
        }
        self.inherent_instances
            .insert(generated.into(), (host.into(), actuals.clone()));
        Ok(())
    }

    pub(super) fn inherent_call(
        &mut self,
        expression: &mut Expr,
        env: &Env,
        depth: usize,
    ) -> R<()> {
        if self.inherent_methods.is_empty() {
            return Ok(());
        }
        let ExprKind::Call { callee, .. } = &expression.kind else {
            return Ok(());
        };
        let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &callee.kind
        else {
            return Ok(());
        };
        let Some(actual) = self.value_hint(object, env, depth + 1, false)? else {
            return Ok(());
        };
        let actual = self.original_type(&actual)?;
        let TypeKind::Named { path, args } = &projection_shape(&actual).kind else {
            return Ok(());
        };
        let [physical] = path.as_slice() else {
            return Ok(());
        };
        self.budget.identity_work(
            self.inherent_instances
                .len()
                .saturating_add(1)
                .saturating_mul(physical.len().saturating_add(1)),
            expression.span,
        )?;
        let mut source_arguments = BTreeMap::new();
        let host = if let Some((host, actuals)) = self.inherent_instances.get(physical) {
            self.budget.text(host, expression.span)?;
            for (name, ty) in actuals {
                self.budget.text(name, expression.span)?;
                self.budget.ty(ty)?;
            }
            source_arguments = actuals.clone();
            host.clone()
        } else {
            self.budget.text(physical, expression.span)?;
            let Some(Item {
                kind: ItemKind::Aggregate(aggregate),
                ..
            }) = self.nominals.get(physical).copied()
            else {
                return Ok(());
            };
            if aggregate.generics.len() != args.len() {
                return Ok(());
            }
            for (formal, actual) in aggregate.generics.iter().zip(args) {
                self.budget.node(formal.span, depth)?;
                if let GenericArg::Type(actual) = actual {
                    self.budget.text(&formal.name, expression.span)?;
                    self.budget.ty(actual)?;
                    source_arguments.insert(formal.name.clone(), actual.clone());
                }
            }
            physical.clone()
        };
        let mut selected = None;
        for method in self.inherent_methods {
            self.budget.text(&method.host, expression.span)?;
            self.budget.text(&host, expression.span)?;
            self.budget.text(&method.member, expression.span)?;
            self.budget.text(member, expression.span)?;
            if method.host == host && method.member == *member {
                selected = Some(method);
                break;
            }
        }
        let Some(method) = selected else {
            return Ok(());
        };
        if !method.public
            && self.current_origin.map(|origin| origin.declaration.module) != Some(method.module)
        {
            return Err(error(
                expression.span,
                "generic inherent method is private to its declaring module",
            ));
        }
        let mut types = Vec::new();
        for (_, formal) in &method.arguments {
            self.budget.text(formal, expression.span)?;
            self.budget.identity_work(
                source_arguments
                    .len()
                    .saturating_add(1)
                    .saturating_mul(formal.len().saturating_add(1)),
                expression.span,
            )?;
            let actual = source_arguments.get(formal).ok_or_else(|| {
                error(
                    expression.span,
                    "inherent receiver complete nominal type argument absent",
                )
            })?;
            self.budget.ty(actual)?;
            types.push(GenericArg::Type(actual.clone()));
        }
        self.budget.text(&method.template, expression.span)?;
        let template = method.template.clone();
        let borrow = method.receiver;
        let scoped = method.scoped;
        let concrete = if scoped {
            let actuals = types
                .iter()
                .map(|argument| match argument {
                    GenericArg::Type(ty) => {
                        self.budget.ty(ty)?;
                        Ok(ty.clone())
                    }
                    _ => Err(error(
                        expression.span,
                        "inherent ordinary type argument kind differs",
                    )),
                })
                .collect::<R<Vec<_>>>()?;
            // A life-only member is an ordinary retained function, not a
            // monomorphization template. Its full source scope survives in
            // the pending header exactly as for other ordinary life helpers.
            let concrete = if actuals.is_empty() && !self.templates.contains_key(&template) {
                self.budget.text(&template, expression.span)?;
                template.clone()
            } else {
                self.instantiate(&template, actuals, expression.span)?
            };
            self.prepare_inherent_signature(&concrete, expression.span)?;
            Some(concrete)
        } else {
            None
        };
        let ExprKind::Call { callee, arguments } = &mut expression.kind else {
            unreachable!()
        };
        let ExprKind::Member { object, .. } = &mut callee.kind else {
            unreachable!()
        };
        self.budget.node(object.span, depth)?;
        for argument in arguments.iter() {
            self.budget.node(argument.span, depth)?;
        }
        let mut value = std::mem::replace(
            object,
            Box::new(Expr {
                semantic: Default::default(),
                span: expression.span,
                kind: ExprKind::Name(String::new()),
            }),
        );
        if let Some(op) = borrow
            && !matches!(grouped(&actual).kind, TypeKind::Reference { .. })
        {
            self.budget.node(value.span, depth)?;
            value = Box::new(Expr {
                semantic: Default::default(),
                span: value.span,
                kind: ExprKind::Unary { op, operand: value },
            });
        }
        arguments.insert(
            0,
            Argument {
                span: value.span,
                spread: false,
                value: *value,
            },
        );
        callee.kind = if let Some(concrete) = concrete {
            ExprKind::Name(concrete)
        } else {
            ExprKind::TypeApply {
                callee: Box::new(Expr {
                    semantic: Default::default(),
                    span: callee.span,
                    kind: ExprKind::Name(template),
                }),
                arguments: types,
            }
        };
        Ok(())
    }
}
