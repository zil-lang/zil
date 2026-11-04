//! Typed scalar const arguments. The internal MetaEval carrier never reaches
//! Core: it is an instance-key/substitution value, not a runtime type.
use super::*;

pub(super) fn parameters(function: &Function) -> impl Iterator<Item = &GenericParam> + Clone {
    function
        .generics
        .iter()
        .filter(|p| !matches!(p.kind, GenericParamKind::Lifetime))
}

pub(super) fn binding(function: &Function, name: &str, local: usize) -> bool {
    function
        .generics
        .iter()
        .filter(|p| matches!(p.kind, GenericParamKind::Const(_)))
        .enumerate()
        .any(|(ordinal, parameter)| {
            parameter.name == name && local == function.params.len() + ordinal
        })
}

pub(super) fn domain(ty: &TypeRef) -> R<&str> {
    match trait_bounds::plain(ty) {
        Some(name @ ("bool" | "u8" | "u16" | "i32" | "i64")) => Ok(name),
        _ => Err(unsupported(
            ty.span,
            "const parameter requires a supported target scalar domain",
        )),
    }
}

pub(super) fn expression(ty: &TypeRef) -> Option<&Expr> {
    if let TypeKind::MetaEval(e) = &ty.kind {
        Some(e)
    } else {
        None
    }
}

pub(super) fn carrier(e: &Expr, budget: &mut Budget<'_>) -> R<TypeRef> {
    budget.const_expression(e)?;
    let ty = TypeRef {
        span: e.span,
        kind: TypeKind::MetaEval(Box::new(e.clone())),
    };
    Ok(ty)
}

pub(super) fn argument(
    arg: &GenericArg,
    parameter: Option<&GenericParam>,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    match arg {
        GenericArg::Const(e) => carrier(e, budget),
        GenericArg::Type(ty)
            if parameter.is_some_and(|p| matches!(p.kind, GenericParamKind::Const(_))) =>
        {
            let name = trait_bounds::plain(ty)
                .ok_or_else(|| error(ty.span, "const argument must be a compile-time value"))?;
            budget.ty(ty)?;
            carrier(
                &Expr {
                    span: ty.span,
                    kind: ExprKind::Name(name.to_owned()),
                },
                budget,
            )
        }
        GenericArg::Type(ty) => {
            budget.ty(ty)?;
            Ok(ty.clone())
        }
    }
}

fn integer(text: &str, span: Span) -> R<(u128, Option<&str>)> {
    let (digits, suffix) = ["u16", "u8", "i32", "i64", "u64", "usize"]
        .into_iter()
        .find_map(|suffix| {
            text.strip_suffix(suffix)
                .map(|digits| (digits, Some(suffix)))
        })
        .unwrap_or((text, None));
    let digits = digits.replace('_', "");
    let (digits, radix) = if let Some(d) = digits.strip_prefix("0x") {
        (d, 16)
    } else if let Some(d) = digits.strip_prefix("0b") {
        (d, 2)
    } else if let Some(d) = digits.strip_prefix("0o") {
        (d, 8)
    } else {
        (digits.as_str(), 10)
    };
    let value =
        u128::from_str_radix(digits, radix).map_err(|_| error(span, "invalid const integer"))?;
    Ok((value, suffix))
}

pub(super) fn normalize(
    parameter: &GenericParam,
    actual: &TypeRef,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    let GenericParamKind::Const(ty) = &parameter.kind else {
        if expression(actual).is_some() {
            return Err(error(actual.span, "value supplied for a type parameter"));
        }
        return Ok(actual.clone());
    };
    let domain = domain(ty)?;
    let actual = expression(actual)
        .ok_or_else(|| error(actual.span, "type supplied for a const parameter"))?;
    budget.node(actual.span, 0)?;
    let kind = match (&actual.kind, domain) {
        (ExprKind::Literal(Literal::Bool(value)), "bool") => {
            ExprKind::Literal(Literal::Bool(*value))
        }
        (ExprKind::Literal(Literal::Integer(text)), domain) if domain != "bool" => {
            budget.text(text, actual.span)?;
            let (value, suffix) = integer(text, actual.span)?;
            if suffix.is_some_and(|suffix| suffix != domain) {
                return Err(error(
                    actual.span,
                    "const argument type differs from its declared target domain",
                ));
            }
            let max = match domain {
                "u8" => u8::MAX as u128,
                "u16" => u16::MAX as u128,
                "i32" => i32::MAX as u128,
                "i64" => i64::MAX as u128,
                _ => unreachable!(),
            };
            if value > max {
                return Err(error(
                    actual.span,
                    "const argument is outside its target integer domain",
                ));
            }
            ExprKind::Literal(Literal::Integer(format!("{value}{domain}")))
        }
        _ => {
            return Err(error(
                actual.span,
                "const argument is not a closed value of its declared domain",
            ));
        }
    };
    carrier(
        &Expr {
            span: actual.span,
            kind,
        },
        budget,
    )
}

pub(super) fn key(expression: &Expr) -> R<String> {
    match &expression.kind {
        ExprKind::Literal(Literal::Integer(text)) => {
            let (value, domain) = integer(text, expression.span)?;
            Ok(format!("{}:{value}", domain.unwrap_or("integer")))
        }
        ExprKind::Literal(Literal::Bool(value)) => Ok(format!("bool:{value}")),
        ExprKind::Name(name) => Ok(format!("binder:{}:{name}", name.len())),
        ExprKind::Group(inner) => key(inner),
        _ => Err(unsupported(
            expression.span,
            "const expression requires verified CT evaluation",
        )),
    }
}

pub(super) fn length(expression: &Expr) -> R<u32> {
    if let ExprKind::Literal(Literal::Integer(text)) = &expression.kind {
        let (value, _) = integer(text, expression.span)?;
        return u32::try_from(value).map_err(|_| {
            error(
                expression.span,
                "array length exceeds static representation",
            )
        });
    }
    Err(error(
        expression.span,
        "array length is not a closed integer",
    ))
}

pub(super) fn length_key(value: &Expr, found: &BTreeMap<String, TypeRef>) -> R<String> {
    if let ExprKind::Name(name) = &value.kind
        && let Some(value) = found.get(name).and_then(expression)
    {
        return length_key(value, &BTreeMap::new());
    }
    match &value.kind {
        ExprKind::Literal(Literal::Integer(text)) => Ok(integer(text, value.span)?.0.to_string()),
        ExprKind::Name(name) => Ok(format!("binder:{}:{name}", name.len())),
        ExprKind::Group(inner) => length_key(inner, found),
        _ => Err(unsupported(
            value.span,
            "array length expression requires verified CT evaluation",
        )),
    }
}

/// Validate the body with immutable scalar parameters, never with a fortunate
/// concrete value. The original function and call signature remain untouched.
pub(super) fn definition<'a>(
    function: &'a Function,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Function>> {
    if !function
        .generics
        .iter()
        .any(|p| matches!(p.kind, GenericParamKind::Const(_)))
    {
        return Ok(Cow::Borrowed(function));
    }
    budget.const_function(function)?;
    // Charge the borrowed function before cloning it or any additional header.
    use super::super::ct_selection::Visitor;
    struct Preflight<'a, 'b, 'l> {
        budget: &'a mut Budget<'l>,
        parameters: &'b [GenericParam],
    }
    impl Visitor for Preflight<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if let Some(name) = trait_bounds::plain(ty)
                && self
                    .parameters
                    .iter()
                    .any(|p| p.name == name && matches!(p.kind, GenericParamKind::Const(_)))
            {
                return Err(vec![error(ty.span, "const binder used as a value type")]);
            }
            if let TypeKind::Array {
                length: Some(length),
                ..
            } = &ty.kind
                && let ExprKind::Name(name) = &length.kind
            {
                let parameter = self.parameters.iter().find(|p| p.name == *name);
                match parameter.map(|p| &p.kind) {
                    Some(GenericParamKind::Const(ty)) if domain(ty).is_ok_and(|d| d != "bool") => {}
                    _ => {
                        return Err(vec![error(
                            length.span,
                            "symbolic array length requires an integer const binder",
                        )]);
                    }
                }
            }
            if let TypeKind::Named { path, args } = &ty.kind {
                self.node(ty.span, depth)?;
                for part in path {
                    self.text(part, ty.span)?;
                }
                for arg in args {
                    match arg {
                        GenericArg::Type(ty)
                            if trait_bounds::plain(ty).is_some_and(|name| {
                                self.parameters.iter().any(|p| {
                                    p.name == name && matches!(p.kind, GenericParamKind::Const(_))
                                })
                            }) =>
                        {
                            self.node(ty.span, depth + 1)?
                        }
                        GenericArg::Type(ty) => self.ty(ty, depth + 1)?,
                        GenericArg::Const(e) => self.expression(e, depth + 1)?,
                    }
                }
                Ok(())
            } else {
                super::super::ct_selection::walk_type(self, ty, depth)
            }
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            if let ExprKind::TypeApply { callee, arguments } = &expression.kind {
                self.node(expression.span, depth)?;
                self.expression(callee, depth + 1)?;
                for argument in arguments {
                    match argument {
                        GenericArg::Type(ty)
                            if trait_bounds::plain(ty).is_some_and(|name| {
                                self.parameters.iter().any(|p| {
                                    p.name == name && matches!(p.kind, GenericParamKind::Const(_))
                                })
                            }) =>
                        {
                            self.node(ty.span, depth + 1)?
                        }
                        GenericArg::Type(ty) => self.ty(ty, depth + 1)?,
                        GenericArg::Const(value) => self.expression(value, depth + 1)?,
                    }
                }
                Ok(())
            } else {
                super::super::ct_selection::walk_expression(self, expression, depth)
            }
        }
    }
    // Signature/body are already bounded by the item ledger. A second walk
    // prepays this definition-only clone, including const value positions.
    for parameter in &function.params {
        Preflight {
            budget,
            parameters: &function.generics,
        }
        .ty(&parameter.ty, 0)
        .map_err(|mut e| e.remove(0))?;
    }
    if let Some(result) = &function.return_type {
        Preflight {
            budget,
            parameters: &function.generics,
        }
        .ty(result, 0)
        .map_err(|mut e| e.remove(0))?;
    }
    match &function.body {
        FunctionBody::Block(block) => Preflight {
            budget,
            parameters: &function.generics,
        }
        .block(block, 0)
        .map_err(|mut e| e.remove(0))?,
        FunctionBody::Expression(expression) => budget.const_expression(expression)?,
        _ => {}
    }
    for parameter in &function.generics {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        if let GenericParamKind::Const(ty) = &parameter.kind {
            budget.ty(ty)?;
        }
    }
    let mut checked = function.clone();
    for parameter in &function.generics {
        if let GenericParamKind::Const(ty) = &parameter.kind {
            domain(ty)?;
            budget.ty(ty)?;
            budget.text(&parameter.name, parameter.span)?;
            checked.params.push(Param {
                name: parameter.name.clone(),
                ty: ty.clone(),
                give: false,
                attributes: vec![],
                span: parameter.span,
            });
        }
    }
    checked
        .generics
        .retain(|p| !matches!(p.kind, GenericParamKind::Const(_)));
    Ok(Cow::Owned(checked))
}

pub(super) fn hide(pattern: &Pattern, substitutions: &mut BTreeMap<String, TypeRef>) {
    match &pattern.kind {
        PatternKind::Name(name) => {
            if substitutions
                .get(name)
                .is_some_and(|t| expression(t).is_some())
            {
                substitutions.remove(name);
            }
        }
        PatternKind::Reference { pattern, .. } => hide(pattern, substitutions),
        PatternKind::Tuple(patterns)
        | PatternKind::Constructor {
            arguments: patterns,
            ..
        } => {
            for pattern in patterns {
                hide(pattern, substitutions);
            }
        }
        PatternKind::RecordConstructor { fields, .. } => {
            for field in fields {
                hide(&field.pattern, substitutions);
            }
        }
        _ => {}
    }
}

pub(super) fn scoped<'a>(
    pattern: &Pattern,
    substitutions: &'a BTreeMap<String, TypeRef>,
) -> Cow<'a, BTreeMap<String, TypeRef>> {
    fn shadows(pattern: &Pattern, substitutions: &BTreeMap<String, TypeRef>) -> bool {
        match &pattern.kind {
            PatternKind::Name(name) => substitutions
                .get(name)
                .is_some_and(|t| expression(t).is_some()),
            PatternKind::Reference { pattern, .. } => shadows(pattern, substitutions),
            PatternKind::Tuple(ps) | PatternKind::Constructor { arguments: ps, .. } => {
                ps.iter().any(|p| shadows(p, substitutions))
            }
            PatternKind::RecordConstructor { fields, .. } => {
                fields.iter().any(|f| shadows(&f.pattern, substitutions))
            }
            _ => false,
        }
    }
    if !shadows(pattern, substitutions) {
        return Cow::Borrowed(substitutions);
    }
    let mut scoped = substitutions.clone();
    hide(pattern, &mut scoped);
    Cow::Owned(scoped)
}

/// A scope may copy the substitution map only when a value binder shadows a
/// const. Prepay one full map per binding site before mutation starts.
pub(super) fn prepay_scopes(
    item: &Item,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<()> {
    if !substitutions.values().any(|t| expression(t).is_some()) {
        return Ok(());
    }
    use super::super::ct_selection::Visitor;
    struct Scopes<'a, 'b, 'l> {
        substitutions: &'a BTreeMap<String, TypeRef>,
        budget: &'b mut Budget<'l>,
    }
    impl Scopes<'_, '_, '_> {
        fn map(&mut self, span: Span) -> CompileResult<()> {
            for (name, ty) in self.substitutions {
                self.budget
                    .node(span, 0)
                    .and_then(|_| self.budget.text(name, span))
                    .and_then(|_| self.budget.ty(ty))
                    .map_err(|e| vec![e])?;
            }
            Ok(())
        }
    }
    impl Visitor for Scopes<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn statement(&mut self, statement: &Stmt, depth: usize) -> CompileResult<()> {
            self.map(statement.span)?;
            if let StmtKind::Match { arms, .. } = &statement.kind {
                for arm in arms {
                    self.map(arm.span)?;
                }
            }
            super::super::ct_selection::walk_statement(self, statement, depth)
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            if let ExprKind::Closure { params, .. } = &expression.kind {
                for _ in params {
                    self.map(expression.span)?;
                }
            }
            super::super::ct_selection::walk_expression(self, expression, depth)
        }
    }
    Scopes {
        substitutions,
        budget,
    }
    .item(item, 0)
    .map_err(|mut e| e.remove(0))
}
