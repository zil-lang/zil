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
    symbolic_parameter(function, name)
        .is_some_and(|(ordinal, _)| local == function.params.len() + ordinal)
}

pub(super) fn symbol(ordinal: usize) -> String {
    format!("$const_{ordinal}")
}

pub(super) fn symbolic_parameter<'a>(
    function: &'a Function,
    name: &str,
) -> Option<(usize, &'a GenericParam)> {
    let ordinal: usize = name.strip_prefix("$const_")?.parse().ok()?;
    let parameter = function
        .generics
        .iter()
        .filter(|p| matches!(p.kind, GenericParamKind::Const(_)))
        .nth(ordinal)?;
    Some((ordinal, parameter))
}

pub(super) fn domain(ty: &TypeRef) -> R<&str> {
    match trait_bounds::plain(ty) {
        Some(name @ ("bool" | "u8" | "u16" | "u64" | "usize" | "i32" | "i64")) => Ok(name),
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
        GenericArg::Lifetime(_) => Err(unsupported(
            Span::default(),
            "lifetime argument requires a scoped storage application",
        )),
        GenericArg::Const(e) => carrier(e, budget),
        GenericArg::Type(ty)
            if parameter.is_some_and(|p| matches!(p.kind, GenericParamKind::Const(_))) =>
        {
            // Instance reconstruction stores every checked slot as a TypeRef;
            // a scalar slot already has this private carrier. Preserve it
            // without wrapping it again or treating it as a type/name. The
            // caller still checks its declared domain and binder provenance.
            if let Some(value) = expression(ty) {
                return carrier(value, budget);
            }
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
                "u64" => u64::MAX as u128,
                "usize" => match budget.target_pointer_width {
                    32 => u32::MAX as u128,
                    64 => u64::MAX as u128,
                    _ => return Err(unsupported(actual.span, "const usize target width")),
                },
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

/// Reify only a value already checked by normalize, using its same integer
/// parser/domain. In particular u64/usize never pass through a signed carrier.
pub(super) fn normalized_value(actual: &TypeRef, budget: &mut Budget<'_>) -> R<zil_ir::Constant> {
    let expression =
        expression(actual).ok_or_else(|| error(actual.span, "normalized const carrier absent"))?;
    budget.node(expression.span, 0)?;
    match &expression.kind {
        ExprKind::Literal(Literal::Bool(value)) => Ok(zil_ir::Constant::Bool(*value)),
        ExprKind::Literal(Literal::Integer(text)) => {
            budget.text(text, expression.span)?;
            let (value, domain) = integer(text, expression.span)?;
            let invalid = || {
                error(
                    expression.span,
                    "normalized const scalar is outside its exact domain",
                )
            };
            Ok(match domain {
                Some("u8") => zil_ir::Constant::U8(u8::try_from(value).map_err(|_| invalid())?),
                Some("u16") => zil_ir::Constant::U16(u16::try_from(value).map_err(|_| invalid())?),
                Some("u64") => zil_ir::Constant::U64(u64::try_from(value).map_err(|_| invalid())?),
                Some("usize") => {
                    zil_ir::Constant::Usize(u64::try_from(value).map_err(|_| invalid())?)
                }
                Some("i32") => zil_ir::Constant::I32(i32::try_from(value).map_err(|_| invalid())?),
                Some("i64") => zil_ir::Constant::I64(i64::try_from(value).map_err(|_| invalid())?),
                _ => {
                    return Err(error(
                        expression.span,
                        "normalized const scalar domain absent",
                    ));
                }
            })
        }
        _ => Err(error(
            expression.span,
            "normalized const scalar value absent",
        )),
    }
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
pub(super) struct Definition<'a> {
    function: Cow<'a, Function>,
    pub(super) source_ids: Option<BTreeMap<usize, usize>>,
}
impl std::ops::Deref for Definition<'_> {
    type Target = Function;
    fn deref(&self) -> &Function {
        &self.function
    }
}
pub(super) fn definition<'a>(
    function: &'a Function,
    budget: &mut Budget<'_>,
    track_calls: bool,
    origins: Option<BTreeMap<usize, usize>>,
) -> R<Definition<'a>> {
    if !function
        .generics
        .iter()
        .any(|p| matches!(p.kind, GenericParamKind::Const(_)))
    {
        return Ok(Definition {
            function: Cow::Borrowed(function),
            source_ids: origins,
        });
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
                        GenericArg::Lifetime(name) => self.text(name, ty.span)?,
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
                        GenericArg::Lifetime(name) => self.text(name, expression.span)?,
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
    // Clone preserves source structure here. Record node origins BEFORE the
    // definition-only const substitution can insert Type-embedded expressions.
    // Only receiver objects (owned Boxes) consume these keys across the move.
    let source_ids = if track_calls {
        let checked_ids = trait_instances::expression_ids(&checked.body, budget)?;
        if let Some(origins) = origins {
            let current_ids = trait_instances::expression_ids(&function.body, budget)?;
            let mut by_ordinal = BTreeMap::new();
            for (pointer, ordinal) in current_ids {
                budget.node(function.span, 0)?;
                if let Some(original) = origins.get(&pointer) {
                    by_ordinal.insert(ordinal, *original);
                }
            }
            let mut mapped = BTreeMap::new();
            for (pointer, ordinal) in checked_ids {
                budget.node(function.span, 0)?;
                if let Some(original) = by_ordinal.get(&ordinal) {
                    mapped.insert(pointer, *original);
                }
            }
            Some(mapped)
        } else {
            Some(checked_ids)
        }
    } else {
        None
    };
    let mut substitutions = BTreeMap::new();
    for (ordinal, parameter) in function
        .generics
        .iter()
        .filter(|p| matches!(p.kind, GenericParamKind::Const(_)))
        .enumerate()
    {
        if let GenericParamKind::Const(ty) = &parameter.kind {
            domain(ty)?;
            budget.ty(ty)?;
            budget.text(&parameter.name, parameter.span)?;
            budget.text("$const_00000000000000000000", parameter.span)?;
            budget.text(&parameter.name, parameter.span)?;
            let name = symbol(ordinal);
            budget.text(&name, parameter.span)?;
            let value = carrier(
                &Expr {
                    span: parameter.span,
                    kind: ExprKind::Name(name.clone()),
                },
                budget,
            )?;
            substitutions.insert(parameter.name.clone(), value);
            checked.params.push(Param {
                name,
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
    let mut item = Item {
        span: function.span,
        public: false,
        attributes: vec![],
        kind: ItemKind::Function(checked),
    };
    budget.substituted_item(&item, &substitutions)?;
    prepay_scopes(&item, &substitutions, budget)?;
    substitute_item(&mut item, &substitutions)?;
    let ItemKind::Function(checked) = item.kind else {
        unreachable!()
    };
    Ok(Definition {
        function: Cow::Owned(checked),
        source_ids,
    })
}

pub(super) fn scoped_name<'a>(
    name: &str,
    substitutions: &'a BTreeMap<String, TypeRef>,
) -> Cow<'a, BTreeMap<String, TypeRef>> {
    if !substitutions
        .get(name)
        .is_some_and(|t| expression(t).is_some())
    {
        return Cow::Borrowed(substitutions);
    }
    let mut scoped = substitutions.clone();
    hide_name(name, &mut scoped);
    Cow::Owned(scoped)
}

fn hide_name(name: &str, substitutions: &mut BTreeMap<String, TypeRef>) {
    let associated = substitutions
        .get(name)
        .is_some_and(trait_bounds::constants::subject);
    if substitutions
        .get(name)
        .is_some_and(|t| expression(t).is_some())
    {
        substitutions.remove(name);
    }
    if associated {
        substitutions.retain(|key, _| {
            !key.strip_prefix("\0constant:")
                .and_then(|key| key.strip_prefix(name))
                .is_some_and(|suffix| suffix.starts_with('.'))
        });
    }
}

pub(super) fn hide(pattern: &Pattern, substitutions: &mut BTreeMap<String, TypeRef>) {
    match &pattern.kind {
        PatternKind::Name(name) => {
            hide_name(name, substitutions);
        }
        PatternKind::Reference { pattern, .. } | PatternKind::ErrorArm { pattern, .. } => {
            hide(pattern, substitutions)
        }
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
            PatternKind::Reference { pattern, .. } | PatternKind::ErrorArm { pattern, .. } => {
                shadows(pattern, substitutions)
            }
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
    let mut present = false;
    let mut associated = false;
    for (name, ty) in substitutions {
        budget.node(item.span, 0)?;
        budget.text(name, item.span)?;
        present |= expression(ty).is_some();
        associated |= trait_bounds::constants::subject(ty);
    }
    if !present {
        return Ok(());
    }
    use super::super::ct_selection::Visitor;
    struct Scopes<'a, 'b, 'l> {
        substitutions: &'a BTreeMap<String, TypeRef>,
        budget: &'b mut Budget<'l>,
        associated: bool,
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
            // Associated namespaces additionally retain-filter their member
            // entries when the root is shadowed. Pay that second full pass.
            if self.associated {
                for (name, ty) in self.substitutions {
                    self.budget
                        .node(span, 0)
                        .and_then(|_| self.budget.text(name, span))
                        .and_then(|_| self.budget.ty(ty))
                        .map_err(|e| vec![e])?;
                }
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
        fn item(&mut self, item: &Item, depth: usize) -> CompileResult<()> {
            if let ItemKind::Function(function) = &item.kind {
                for parameter in &function.params {
                    self.map(parameter.span)?;
                }
            }
            super::super::ct_selection::walk_item(self, item, depth)
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
        associated,
    }
    .item(item, 0)
    .map_err(|mut e| e.remove(0))
}
