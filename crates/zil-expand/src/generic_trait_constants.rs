//! Associated value atoms carry the exact bound instance. Declared types are
//! available symbolically; impl values are read only from committed witnesses.
use super::*;

pub(crate) const MARKER: &str = "$associated_constant";
pub(crate) const SUBJECT: &str = "$associated_constant_subject";
pub(crate) fn subject(ty: &TypeRef) -> bool {
    matches!(consts::expression(ty).map(|e| &e.kind), Some(ExprKind::Name(name)) if name == SUBJECT)
}
pub(crate) fn identity(expression: &Expr, budget: &mut Budget<'_>) -> R<Option<String>> {
    let ExprKind::TypeApply { arguments, .. } = &expression.kind else {
        return Ok(None);
    };
    let [GenericArg::Type(atom)] = arguments.as_slice() else {
        return Ok(None);
    };
    type_key(atom, budget).map(Some)
}
pub(crate) fn key(subject: &str, member: &str) -> String {
    format!("\0constant:{subject}.{member}")
}
pub(crate) fn host_expression(
    mut expression: &Expr,
    nominals: &BTreeMap<String, &Item>,
    visible: impl Fn(&str) -> bool,
    budget: &mut Budget<'_>,
) -> R<Option<TypeRef>> {
    let mut depth = 0;
    let (name, args) = loop {
        budget.node(expression.span, depth)?;
        match &expression.kind {
            ExprKind::Name(name) => break (name, &[][..]),
            ExprKind::TypeApply { callee, arguments } => {
                budget.node(callee.span, depth + 1)?;
                let ExprKind::Name(name) = &callee.kind else {
                    return Ok(None);
                };
                break (name, arguments.as_slice());
            }
            ExprKind::Group(inner) => {
                expression = inner;
                depth += 1;
            }
            _ => return Ok(None),
        }
    };
    budget.text(name, expression.span)?;
    if !visible(name) || !nominals.contains_key(name) {
        return Ok(None);
    }
    for argument in args {
        match argument {
            GenericArg::Lifetime(name) => budget.text(name, expression.span)?,
            GenericArg::Type(ty) => budget.ty(ty)?,
            GenericArg::Const(value) => budget.const_expression(value)?,
        }
    }
    Ok(Some(TypeRef {
        span: expression.span,
        kind: TypeKind::Named {
            path: vec![name.clone()],
            args: args.to_vec(),
        },
    }))
}
pub(crate) fn expression(
    host: &TypeRef,
    reference: &trait_instances::Reference,
    member: &str,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Expr> {
    let mut atom = associated::atom(host, reference, member, budget, span)?;
    let TypeKind::Named { path, .. } = &mut atom.kind else {
        unreachable!()
    };
    budget.text(MARKER, span)?;
    path[0] = MARKER.into();
    Ok(Expr {
        span,
        kind: ExprKind::TypeApply {
            callee: Box::new(Expr {
                span,
                kind: ExprKind::Name(MARKER.into()),
            }),
            arguments: vec![GenericArg::Type(atom)],
        },
    })
}
pub(crate) fn parts(expression: &Expr) -> Option<(&str, &TypeRef, &TypeRef)> {
    let ExprKind::TypeApply { callee, arguments } = &expression.kind else {
        return None;
    };
    if !matches!(&callee.kind, ExprKind::Name(name) if name == MARKER) {
        return None;
    }
    let [GenericArg::Type(atom)] = arguments.as_slice() else {
        return None;
    };
    let TypeKind::Named { path, args } = &atom.kind else {
        return None;
    };
    let [marker, member] = path.as_slice() else {
        return None;
    };
    let [GenericArg::Type(host), GenericArg::Type(application)] = args.as_slice() else {
        return None;
    };
    (marker == MARKER).then_some((member, host, application))
}
fn closed(mut value: &Expr, budget: &mut Budget<'_>) -> R<bool> {
    let mut depth = 0;
    loop {
        budget.node(value.span, depth)?;
        match &value.kind {
            ExprKind::Literal(Literal::Bool(_) | Literal::Integer(_) | Literal::Float(_)) => {
                return Ok(true);
            }
            ExprKind::Group(inner) => {
                value = inner;
                depth += 1;
            }
            ExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } => {
                budget.node(operand.span, depth + 1)?;
                return Ok(matches!(
                    operand.kind,
                    ExprKind::Literal(Literal::Integer(_) | Literal::Float(_))
                ));
            }
            _ => return Ok(false),
        }
    }
}
fn function(
    name: String,
    ty: TypeRef,
    value: Expr,
    parameters: Vec<GenericParam>,
    span: Span,
) -> Function {
    Function {
        name,
        generics: parameters,
        params: vec![],
        return_type: Some(ty),
        where_clause: vec![],
        contract: vec![],
        body: FunctionBody::Expression(value),
        span,
    }
}
impl<'a> Index<'a> {
    pub(crate) fn constant_declaration(
        &self,
        reference: &trait_instances::Reference,
        name: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<&'a ConstDecl> {
        let (_, declaration) = self.declaration_item(reference);
        for member in &declaration.members {
            budget.node(member.span, 0)?;
            if let ItemKind::Const(constant) = &member.kind {
                budget.text(&constant.name, member.span)?;
                budget.text(name, span)?;
                if constant.name == name {
                    return Ok(constant);
                }
            }
        }
        Err(error(
            span,
            "associated constant is not declared by its trait instance",
        ))
    }
    pub(crate) fn constant_type(
        &self,
        expression: &Expr,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<TypeRef> {
        let (member, host, application) = parts(expression)
            .ok_or_else(|| error(expression.span, "malformed associated constant atom"))?;
        let reference = self.pattern_reference(application, budget)?;
        self.solve(&reference, host, available, variables, budget)?;
        let constant = self.constant_declaration(&reference, member, budget, expression.span)?;
        let ty = constant.ty.as_ref().ok_or_else(|| {
            error(
                expression.span,
                "associated constant requires its declared type",
            )
        })?;
        let (_, declaration) = self.declaration_item(&reference);
        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in declaration.generics.iter().zip(&reference.arguments) {
            budget.text(&parameter.name, parameter.span)?;
            budget.ty(argument)?;
            substitutions.insert(parameter.name.clone(), argument.clone());
        }
        budget.ty(host)?;
        budget.text("Self", host.span)?;
        substitutions.insert("Self".into(), host.clone());
        budget.substituted_type(ty, &substitutions)?;
        let mut ty = ty.clone();
        Substitution {
            substitutions: &substitutions,
        }
        .ty(&mut ty);
        self.normalize_associated(&ty, available, variables, budget)
    }
    pub(crate) fn constant_value(
        &self,
        expression: &Expr,
        budget: &mut Budget<'_>,
    ) -> R<(Expr, TypeRef)> {
        let (member, host, application) = parts(expression)
            .ok_or_else(|| error(expression.span, "malformed associated constant atom"))?;
        let reference = self.pattern_reference(application, budget)?;
        let ty = self.constant_type(expression, &Bounds::new(), &BTreeSet::new(), budget)?;
        // A candidate's value never participates in proving its own premises.
        let witness = self
            .solve(&reference, host, &Bounds::new(), &BTreeSet::new(), budget)?
            .ok_or_else(|| {
                error(
                    expression.span,
                    "associated constant needs a committed concrete impl",
                )
            })?;
        let ItemKind::Impl(implementation) = &witness.item.kind else {
            unreachable!()
        };
        let mut value = None;
        for item in &implementation.members {
            budget.node(item.span, 0)?;
            if let ItemKind::Const(constant) = &item.kind {
                budget.text(member, expression.span)?;
                budget.text(&constant.name, item.span)?;
                if constant.name == member {
                    value = constant.value.as_ref();
                    break;
                }
            }
        }
        let value = value
            .or(self
                .constant_declaration(&reference, member, budget, expression.span)?
                .value
                .as_ref())
            .ok_or_else(|| {
                error(
                    expression.span,
                    "impl is missing its associated constant value",
                )
            })?;
        budget.const_expression(value)?;
        if !closed(value, budget)? {
            return Err(unsupported(
                value.span,
                "associated constant equation needs verified CT evaluation",
            ));
        }
        Ok((value.clone(), ty))
    }
}
impl Engine<'_> {
    pub(crate) fn validate_trait_constants(&mut self, module: &Module) -> R<()> {
        if !self.traits.has_constants {
            return Ok(());
        }
        for item in &module.items {
            self.budget.context.origin = self.traits.origin(item, &mut self.budget)?;
            self.budget.node(item.span, 0)?;
            let ItemKind::Trait(declaration) = &item.kind else {
                continue;
            };
            let mut arguments = Vec::new();
            for parameter in &declaration.generics {
                self.budget.text(&parameter.name, parameter.span)?;
                arguments.push(GenericArg::Type(named(&parameter.name, parameter.span)));
            }
            self.budget.text(&declaration.name, item.span)?;
            let application = TypeRef {
                span: item.span,
                kind: TypeKind::Named {
                    path: vec![declaration.name.clone()],
                    args: arguments,
                },
            };
            let reference = self
                .traits
                .pattern_reference(&application, &mut self.budget)?;
            for member in &declaration.members {
                self.budget.node(member.span, 0)?;
                let ItemKind::Const(constant) = &member.kind else {
                    continue;
                };
                let Some(value) = &constant.value else {
                    continue;
                };
                self.budget.const_expression(value)?;
                if !closed(value, &mut self.budget)? {
                    return Err(unsupported(
                        value.span,
                        "associated constant default needs verified CT evaluation",
                    ));
                }
                let ty = constant.ty.as_ref().ok_or_else(|| {
                    error(member.span, "associated constant requires a declared type")
                })?;
                self.budget.ty(ty)?;
                self.budget.text(&constant.name, member.span)?;
                let checked = Item {
                    span: member.span,
                    public: false,
                    attributes: vec![],
                    kind: ItemKind::Function(function(
                        constant.name.clone(),
                        ty.clone(),
                        value.clone(),
                        vec![],
                        member.span,
                    )),
                };
                self.check_default(declaration, &checked, &reference, true)?;
            }
        }
        Ok(())
    }
    pub(crate) fn validate_impl_constants(
        &mut self,
        implementation: &ImplBlock,
        reference: &trait_instances::Reference,
        parameters: &[GenericParam],
        bounds: &Bounds,
        equations: &BTreeMap<String, TypeRef>,
    ) -> R<()> {
        if !self.traits.has_constants {
            return Ok(());
        }
        let (_, declaration) = self.traits.declaration_item(reference);
        let mut names = BTreeSet::new();
        for member in &implementation.members {
            self.budget.node(member.span, 0)?;
            let name = match &member.kind {
                ItemKind::Function(f) => &f.name,
                ItemKind::Const(c) => &c.name,
                ItemKind::TypeAlias(a) => &a.name,
                _ => continue,
            };
            self.budget.text(name, member.span)?;
            if !names.insert(name.as_str()) {
                return Err(error(member.span, "duplicate impl member"));
            }
            let ItemKind::Const(constant) = &member.kind else {
                continue;
            };
            if !member.attributes.is_empty() {
                return Err(unsupported(member.span, "associated constant attributes"));
            }
            let required =
                self.traits
                    .constant_declaration(reference, name, &mut self.budget, member.span)?;
            let value = constant
                .value
                .as_ref()
                .ok_or_else(|| error(member.span, "impl associated constant requires a value"))?;
            self.budget.const_expression(value)?;
            if !closed(value, &mut self.budget)? {
                return Err(unsupported(
                    value.span,
                    "associated constant equation needs verified CT evaluation",
                ));
            }
            let mut substitutions = BTreeMap::new();
            for (parameter, argument) in declaration.generics.iter().zip(&reference.arguments) {
                self.budget.text(&parameter.name, parameter.span)?;
                self.budget.ty(argument)?;
                substitutions.insert(parameter.name.clone(), argument.clone());
            }
            self.budget.text("Self", member.span)?;
            self.budget.ty(&implementation.target)?;
            substitutions.insert("Self".into(), implementation.target.clone());
            let declared = required
                .ty
                .as_ref()
                .ok_or_else(|| error(member.span, "trait constant requires a declared type"))?;
            self.budget.substituted_type(declared, &substitutions)?;
            let mut ty = declared.clone();
            Substitution {
                substitutions: &substitutions,
            }
            .ty(&mut ty);
            self.budget.substituted_type(&ty, equations)?;
            Substitution {
                substitutions: equations,
            }
            .ty(&mut ty);
            if let Some(actual) = &constant.ty {
                self.budget.ty(actual)?;
                let mut actual = actual.clone();
                let self_substitution =
                    BTreeMap::from([("Self".into(), implementation.target.clone())]);
                self.budget.substituted_type(&actual, &self_substitution)?;
                Substitution {
                    substitutions: &self_substitution,
                }
                .ty(&mut actual);
                self.budget.substituted_type(&actual, equations)?;
                Substitution {
                    substitutions: equations,
                }
                .ty(&mut actual);
                if type_key(&actual, &mut self.budget)? != type_key(&ty, &mut self.budget)? {
                    return Err(error(
                        member.span,
                        "impl constant type differs from its trait declaration",
                    ));
                }
            }
            for parameter in parameters {
                self.budget.node(parameter.span, 0)?;
                self.budget.text(&parameter.name, parameter.span)?;
                if let GenericParamKind::Type(bounds) = &parameter.kind {
                    for bound in bounds {
                        self.budget.ty(bound)?;
                    }
                }
            }
            self.budget.text("$constant_definition_", member.span)?;
            for (name, supplied) in bounds {
                self.budget.node(member.span, 0)?;
                self.budget.text(name, member.span)?;
                for bound in supplied {
                    self.budget.node(member.span, 0)?;
                    if let Bound::Nominal(reference) = bound {
                        trait_instances::copy_key(&reference.key, &mut self.budget, member.span)?;
                        for argument in &reference.arguments {
                            self.budget.ty(argument)?;
                        }
                    }
                }
            }
            let checked = function(
                format!("$constant_definition_{}", constant.name),
                ty,
                value.clone(),
                parameters.to_vec(),
                member.span,
            );
            // Reuse the complete definition checker; no selected i32 instance can
            // add operations or Copy to an unused generic equation.
            check::validate_parametric_with_bounds(
                &checked,
                check::ParametricTables {
                    source_definition: Some(&checked),
                    expression_origins: None,
                    definition_equations: (self.traits.has_generic_methods
                        && !equations.is_empty())
                    .then_some(equations),
                    nominals: &self.nominals,
                    enums: &self.enums,
                    signatures: &self.signatures,
                    foreign_signatures: &self.foreign_signatures,
                    pattern_types: self.pattern_types,
                    traits: &self.traits,
                },
                &mut helper_graph::Dependencies::default(),
                &mut self.budget,
                &mut trait_instances::Calls::new(),
                Some(bounds.clone()),
            )?;
        }
        for member in &declaration.members {
            self.budget.node(member.span, 0)?;
            if let ItemKind::Const(constant) = &member.kind {
                self.budget.text(&constant.name, member.span)?;
                if constant.value.is_none() && !names.contains(constant.name.as_str()) {
                    return Err(error(
                        implementation.target.span,
                        "impl is missing its associated constant",
                    ));
                }
            }
        }
        Ok(())
    }
}
