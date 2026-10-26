//! A deliberately closed parametric admission check. Unconstrained T is not
//! presumed numeric, Copy, or equipped with members because one instance is.
use super::*;

pub(super) fn validate_parametric<'l>(
    function: &Function,
    nominals: &BTreeMap<String, &'l Item>,
    enums: &BTreeMap<String, &'l EnumDecl>,
    signatures: &BTreeMap<String, Cow<'l, Function>>,
    pattern_types: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'l>,
) -> R<()> {
    // Validate every written signature even if the template remains unused.
    // Type keys reject unresolved lifetime/const forms rather than erasing them.
    let mut parameter_names = BTreeSet::new();
    let variables = function.generics.iter().map(|p| p.name.clone()).collect();
    for p in &function.params {
        if super::super::primitive_name(&p.name) {
            return Err(super::super::primitive_name_error(&p.name, p.span));
        }
        if !parameter_names.insert(&p.name) {
            return Err(error(p.span, "duplicate generic function parameter"));
        }
        type_key(&p.ty, budget)?;
        super::super::foreign::callback_type_tree(&p.ty, &variables, budget, 0)?;
    }
    if let Some(t) = &function.return_type {
        type_key(t, budget)?;
        super::super::foreign::callback_type_tree(t, &variables, budget, 0)?;
    }
    let mut check = Check {
        variables,
        callable: callable::template(function)?,
        next_local: function.params.len(),
        nominals,
        enums,
        signatures,
        pattern_types,
        result: function
            .return_type
            .clone()
            .unwrap_or_else(|| unit(function.span)),
        budget,
    };
    let mut state = State {
        locals: function
            .params
            .iter()
            .enumerate()
            .map(|(id, p)| {
                (
                    p.name.clone(),
                    Local {
                        id,
                        ty: p.ty.clone(),
                        mutable: false,
                    },
                )
            })
            .collect(),
        moved: BTreeSet::new(),
    };
    match &function.body {
        FunctionBody::Block(body) => {
            if !check.block(body, &mut state)? {
                same(&unit(body.span), &check.result, body.span, check.budget)?;
            }
        }
        FunctionBody::Expression(value) => {
            let result = check.result.clone();
            let actual = check.expr(value, &mut state, Some(&result), false)?;
            if check.callable.is_some() {
                same_return(&actual, &result, value.span, check.budget)?;
            } else {
                same(&actual, &result, value.span, check.budget)?;
            }
        }
        FunctionBody::Declaration => {
            return Err(unsupported(
                function.span,
                "generic declaration without body",
            ));
        }
    }
    Ok(())
}

pub(super) fn unit(span: Span) -> TypeRef {
    TypeRef {
        span,
        kind: TypeKind::Tuple(vec![]),
    }
}
fn same(actual: &TypeRef, expected: &TypeRef, span: Span, budget: &mut Budget<'_>) -> R<()> {
    if type_key(actual, budget)? == type_key(expected, budget)? {
        Ok(())
    } else {
        Err(error(
            span,
            "generic body types must agree before concrete instantiation",
        ))
    }
}
fn same_return(actual: &TypeRef, expected: &TypeRef, span: Span, budget: &mut Budget<'_>) -> R<()> {
    let actual_key = type_key(actual, budget)?;
    if actual_key == type_key(expected, budget)? {
        return Ok(());
    }
    if let TypeKind::Error {
        error: error_type,
        result,
    } = &group(expected).kind
    {
        let error_match = actual_key == type_key(error_type, budget)?;
        let result_match = actual_key == type_key(result, budget)?;
        if error_match != result_match {
            return Ok(());
        }
        if error_match {
            return Err(error(span, "error union payload matches both sides"));
        }
    }
    same(actual, expected, span, budget)
}
fn substituted_type(
    ty: &TypeRef,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    budget.substituted_type(ty, substitutions)?;
    let mut result = ty.clone();
    Substitution { substitutions }.ty(&mut result);
    Ok(result)
}
pub(super) fn integer(ty: &TypeRef) -> bool {
    matches!(&group(ty).kind, TypeKind::Named { path, args } if args.is_empty() && matches!(path.as_slice(), [name] if matches!(name.as_str(), "u8" | "u16" | "i32" | "i64")))
}
// An unused template cannot erase an out-of-range literal or an explicit
// suffix mismatch. Concrete instances still enter ordinary Types and Verify.
fn checked_integer(
    text: &str,
    expected: Option<&TypeRef>,
    negative: bool,
    span: Span,
) -> R<(TypeRef, i64)> {
    let clean = text.replace('_', "");
    let suffix = ["u8", "u16", "i32", "i64"]
        .into_iter()
        .find(|name| clean.ends_with(name));
    let expected_name = expected.and_then(|ty| match &group(ty).kind {
        TypeKind::Named { path, args } if args.is_empty() && path.len() == 1 => {
            Some(path[0].as_str())
        }
        _ => None,
    });
    let name = suffix.unwrap_or(match expected_name {
        Some("u8") => "u8",
        Some("u16") => "u16",
        Some("i64") => "i64",
        _ => "i32",
    });
    let digits = suffix.map_or(clean.as_str(), |suffix| clean.strip_suffix(suffix).unwrap());
    let (radix, digits) = if let Some(v) = digits.strip_prefix("0x") {
        (16, v)
    } else if let Some(v) = digits.strip_prefix("0b") {
        (2, v)
    } else if let Some(v) = digits.strip_prefix("0o") {
        (8, v)
    } else {
        (10, digits)
    };
    let value = i128::from_str_radix(digits, radix)
        .map_err(|_| error(span, "generic integer literal is invalid or too large"))?;
    let value = if negative { -value } else { value };
    let value = match name {
        "u8" => u8::try_from(value)
            .map(i64::from)
            .map_err(|_| error(span, "generic integer literal does not fit u8"))?,
        "u16" => u16::try_from(value)
            .map(i64::from)
            .map_err(|_| error(span, "generic integer literal does not fit u16"))?,
        "i32" => i32::try_from(value)
            .map(i64::from)
            .map_err(|_| error(span, "generic integer literal does not fit i32"))?,
        _ => i64::try_from(value)
            .map_err(|_| error(span, "generic integer literal does not fit i64"))?,
    };
    Ok((named(name, span), value))
}
fn boolean(ty: &TypeRef) -> bool {
    matches!(&group(ty).kind, TypeKind::Named { path, args } if args.is_empty() && path.as_slice() == ["bool"])
}
fn group(ty: &TypeRef) -> &TypeRef {
    match &ty.kind {
        TypeKind::Group(t) => group(t),
        _ => ty,
    }
}
fn dependent(ty: &TypeRef, vars: &BTreeSet<String>) -> bool {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            path.len() == 1 && vars.contains(&path[0])
                || args
                    .iter()
                    .any(|a| matches!(a, GenericArg::Type(t) if dependent(t, vars)))
        }
        TypeKind::Reference { inner, .. } | TypeKind::Group(inner) | TypeKind::Optional(inner) => {
            dependent(inner, vars)
        }
        TypeKind::Tuple(ts) => ts.iter().any(|t| dependent(t, vars)),
        TypeKind::Array { element, .. } => dependent(element, vars),
        TypeKind::Error { error, result } => dependent(error, vars) || dependent(result, vars),
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            params.iter().any(|p| dependent(p, vars)) || dependent(result, vars)
        }
        _ => false,
    }
}
fn known_copy(ty: &TypeRef, vars: &BTreeSet<String>) -> bool {
    match &group(ty).kind {
        TypeKind::Reference { mutable: false, .. }
        | TypeKind::Function { .. }
        | TypeKind::ForeignFunction { .. } => true,
        TypeKind::Reference { mutable: true, .. } => false,
        // Opaque callable bounds expose an invocation interface, not Copy.
        // In particular an unused template must not duplicate a FnOnce owner.
        TypeKind::Impl(_) => false,
        TypeKind::Tuple(ts) => ts.iter().all(|t| known_copy(t, vars)),
        _ => !dependent(ty, vars),
    }
}
#[derive(Clone)]
struct Local {
    id: usize,
    ty: TypeRef,
    mutable: bool,
}
#[derive(Clone)]
struct State {
    locals: BTreeMap<String, Local>,
    moved: BTreeSet<usize>,
}
struct Check<'s, 'b, 'l> {
    variables: BTreeSet<String>,
    callable: Option<callable::CallableTemplate>,
    next_local: usize,
    nominals: &'s BTreeMap<String, &'l Item>,
    enums: &'s BTreeMap<String, &'l EnumDecl>,
    signatures: &'s BTreeMap<String, Cow<'l, Function>>,
    pattern_types: &'s BTreeMap<String, TypeRef>,
    result: TypeRef,
    budget: &'b mut Budget<'l>,
}
impl Check<'_, '_, '_> {
    fn tick(&mut self, span: Span) -> R<()> {
        // The borrowed preflight checked full AST depth before any cloning.
        self.budget.node(span, 0)
    }
    fn bind_local(&mut self, state: &mut State, name: String, ty: TypeRef) -> R<()> {
        if super::super::primitive_name(&name) {
            return Err(super::super::primitive_name_error(&name, ty.span));
        }
        let id = self.next_local;
        self.next_local += 1;
        state.locals.insert(
            name,
            Local {
                id,
                ty,
                mutable: false,
            },
        );
        Ok(())
    }
    fn clone_state(&mut self, state: &State) -> R<State> {
        for local in state.locals.values() {
            self.budget.ty(&local.ty)?;
        }
        Ok(state.clone())
    }
    // The concrete argument checker reborrows address parameters. Mirror only
    // that written reference shape here; an unconstrained T still consumes its
    // argument and gains no Copy or address capability from an instance.
    fn argument(
        &mut self,
        expression: &Expr,
        state: &mut State,
        parameter: &TypeRef,
        expected: Option<&TypeRef>,
    ) -> R<TypeRef> {
        let TypeKind::Reference { mutable, .. } = &group(parameter).kind else {
            return self.expr(expression, state, expected, false);
        };
        let mut actual = self.expr(expression, state, expected, true)?;
        let TypeKind::Reference {
            mutable: available, ..
        } = &group(&actual).kind
        else {
            return Err(error(
                expression.span,
                "reference parameter requires a reference",
            ));
        };
        if *mutable && !available {
            return Err(error(
                expression.span,
                "shared reference cannot grant exclusive access",
            ));
        }
        // Coercion changes only permission; later unification still compares
        // the actual referent with the complete symbolic parameter shape.
        self.budget.ty(&actual)?;
        actual = group(&actual).clone();
        let TypeKind::Reference { mutable: mode, .. } = &mut actual.kind else {
            unreachable!()
        };
        *mode = *mutable;
        Ok(actual)
    }
    fn expr(
        &mut self,
        e: &Expr,
        state: &mut State,
        expected: Option<&TypeRef>,
        borrowing: bool,
    ) -> R<TypeRef> {
        self.tick(e.span)?;
        if let Some(ty) = self.enum_expression(e, state, expected)? {
            return Ok(ty);
        }
        let needs_bound = || {
            unsupported(
                e.span,
                "operation on an unconstrained type parameter requires a declared bound",
            )
        };
        match &e.kind {
            ExprKind::Name(name) => {
                let ty = state.locals.get(name).ok_or_else(|| {
                    error(e.span, format!("unknown value {name} in generic body"))
                })?;
                let id = ty.id;
                self.budget.ty(&ty.ty)?;
                let ty = ty.ty.clone();
                if state.moved.contains(&id) {
                    return Err(error(
                        e.span,
                        "generic value used after move; Copy cannot be inferred from a concrete instance",
                    ));
                }
                if !borrowing && !known_copy(&ty, &self.variables) {
                    state.moved.insert(id);
                }
                Ok(ty)
            }
            ExprKind::Literal(Literal::Integer(text)) => {
                if expected.is_some_and(|t| dependent(t, &self.variables)) {
                    return Err(needs_bound());
                }
                Ok(checked_integer(text, expected, false, e.span)?.0)
            }
            ExprKind::Literal(Literal::Bool(_)) => Ok(named("bool", e.span)),
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => {
                self.expr(inner, state, expected, borrowing)
            }
            ExprKind::Tuple(values) => {
                let hints = expected.and_then(|t| match &group(t).kind {
                    TypeKind::Tuple(ts) => Some(ts),
                    _ => None,
                });
                let mut fields = vec![];
                for (i, v) in values.iter().enumerate() {
                    fields.push(self.expr(v, state, hints.and_then(|ts| ts.get(i)), borrowing)?);
                }
                Ok(TypeRef {
                    span: e.span,
                    kind: TypeKind::Tuple(fields),
                })
            }
            ExprKind::Array(values) => {
                let mut element = expected
                    .and_then(|ty| match &group(ty).kind {
                        TypeKind::Array { element, .. } => Some(element.as_ref()),
                        _ => None,
                    })
                    .cloned();
                for value in values {
                    let actual = self.expr(value, state, element.as_ref(), borrowing)?;
                    if let Some(element) = &element {
                        same(&actual, element, value.span, self.budget)?;
                    } else {
                        element = Some(actual);
                    }
                }
                array_type(
                    element.ok_or_else(|| {
                        error(e.span, "empty generic array requires element type context")
                    })?,
                    values.len(),
                    e.span,
                )
            }
            ExprKind::Index { .. } => {
                let (ty, _) = self.place(e, state)?;
                if !borrowing && !known_copy(&ty, &self.variables) {
                    return Err(needs_bound());
                }
                Ok(ty)
            }
            ExprKind::Slice { .. } => {
                if !borrowing {
                    return Err(unsupported(
                        e.span,
                        "slice ranges require an explicit borrow",
                    ));
                }
                Ok(self.place(e, state)?.0)
            }
            ExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                let mutable = matches!(
                    e.kind,
                    ExprKind::Unary {
                        op: UnaryOp::BorrowMut,
                        ..
                    }
                );
                let (inner, writable) = self.place(operand, state)?;
                if mutable && !writable {
                    return Err(error(e.span, "exclusive borrow requires a writable place"));
                }
                Ok(TypeRef {
                    span: e.span,
                    kind: TypeKind::Reference {
                        mutable,
                        lifetime: None,
                        inner: Box::new(inner),
                    },
                })
            }
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let ty = self.expr(operand, state, None, true)?;
                let TypeKind::Reference { inner, .. } = &group(&ty).kind else {
                    return Err(error(e.span, "dereference requires an address"));
                };
                if !borrowing && !known_copy(inner, &self.variables) {
                    return Err(needs_bound());
                }
                Ok(*inner.clone())
            }
            ExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if matches!(operand.kind, ExprKind::Literal(Literal::Integer(_))) => {
                if expected.is_some_and(|t| dependent(t, &self.variables)) {
                    return Err(needs_bound());
                }
                let ExprKind::Literal(Literal::Integer(text)) = &operand.kind else {
                    unreachable!()
                };
                Ok(checked_integer(text, expected, true, e.span)?.0)
            }
            ExprKind::Unary { op, operand } => {
                let ty = self.expr(operand, state, expected, false)?;
                if dependent(&ty, &self.variables) {
                    return Err(needs_bound());
                }
                let valid = match op {
                    UnaryOp::Negate | UnaryOp::BitNot => integer(&ty),
                    UnaryOp::Not => boolean(&ty),
                    UnaryOp::BorrowMut | UnaryOp::Give => {
                        return Err(unsupported(
                            e.span,
                            "generic mutable borrow or managed give requires capability checking",
                        ));
                    }
                    _ => unreachable!(),
                };
                if !valid {
                    return Err(error(
                        e.span,
                        "invalid concrete unary operand type in generic body",
                    ));
                }
                Ok(ty)
            }
            ExprKind::Binary { op, left, right } => {
                let lhs = self.expr(left, state, expected, false)?;
                let rhs = self.expr(right, state, Some(&lhs), false)?;
                if dependent(&lhs, &self.variables) || dependent(&rhs, &self.variables) {
                    return Err(needs_bound());
                }
                same(&rhs, &lhs, e.span, self.budget)?;
                let valid = match op {
                    BinaryOp::And | BinaryOp::Or => boolean(&lhs),
                    BinaryOp::Equal | BinaryOp::NotEqual => {
                        integer(&lhs)
                            || boolean(&lhs)
                            || matches!(&group(&lhs).kind, TypeKind::Tuple(ts) if ts.is_empty())
                    }
                    BinaryOp::Range => return Err(unsupported(e.span, "generic range value")),
                    _ => integer(&lhs),
                };
                if !valid {
                    return Err(error(
                        e.span,
                        "invalid concrete binary operand types in generic body",
                    ));
                }
                if matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                ) {
                    Ok(named("bool", e.span))
                } else {
                    Ok(lhs)
                }
            }
            ExprKind::Call { callee, arguments } => {
                if let Some((parameters, result)) = super::super::foreign::bare_name(callee)
                    .and_then(|name| bytes::signature(name, e.span))
                {
                    if arguments.len() != parameters.len() || arguments.iter().any(|arg| arg.spread)
                    {
                        return Err(error(
                            e.span,
                            "byte primitive argument count or shape differs",
                        ));
                    }
                    for (index, (argument, parameter)) in
                        arguments.iter().zip(&parameters).enumerate()
                    {
                        // Ordinary ByteBuffer observation takes a descriptor;
                        // it does not consume the carrier in this expression.
                        let actual =
                            self.expr(&argument.value, state, Some(parameter), index == 0)?;
                        same(&actual, parameter, argument.span, self.budget)?;
                    }
                    self.budget.ty(&result)?;
                    return Ok(result);
                }
                if let ExprKind::Member {
                    object,
                    member: MemberName::Name(name),
                } = &callee.kind
                    && name == "len"
                {
                    let ty = self.expr(object, state, None, true)?;
                    if matches!(&group(&ty).kind, TypeKind::Reference { inner, .. } if matches!(group(inner).kind, TypeKind::Array { length: None, .. }))
                    {
                        if !arguments.is_empty() {
                            return Err(error(e.span, "slice len requires no arguments"));
                        }
                        return Ok(named("i64", e.span));
                    }
                    return Err(unsupported(e.span, "generic len requires a borrowed slice"));
                }

                if let Some(value) =
                    super::super::foreign::literal_constructor(callee, arguments, e.span)
                {
                    let value = value?;
                    if let zil_ir::Constant::WideString(units) = &value {
                        self.budget.wide_units(units, e.span)?;
                    }
                    return Ok(named(&value.ty().name(), e.span));
                }
                if super::super::foreign::bare_name(callee) == Some("c_callback") {
                    let [argument] = arguments.as_slice() else {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "c_callback requires one static symbol",
                        ));
                    };
                    if argument.spread {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "C callback constructor does not accept spread arguments",
                        ));
                    }
                    let Some(target) = super::super::foreign::bare_name(&argument.value)
                        .filter(|name| !state.locals.contains_key(*name))
                    else {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "c_callback requires one static symbol",
                        ));
                    };
                    let Some(function) = self.signatures.get(target) else {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "unknown callback symbol",
                        ));
                    };
                    for parameter in &function.params {
                        self.budget.ty(&parameter.ty)?;
                    }
                    if let Some(result) = &function.return_type {
                        self.budget.ty(result)?;
                    }
                    return super::super::foreign::callback_syntax_type(function, e.span);
                }
                if let Some(name) = super::super::integer_conversion_name(callee) {
                    if arguments.len() != 1 || arguments[0].spread {
                        return Err(super::super::conversion_error(
                            e.span,
                            "integer conversion requires exactly one non-spread argument",
                        ));
                    }
                    let actual = self.expr(&arguments[0].value, state, None, false)?;
                    if !integer(&actual) {
                        return Err(super::super::conversion_error(
                            e.span,
                            "integer conversion requires a known integer operand",
                        ));
                    }
                    return Ok(named(name, e.span));
                }
                let (target, explicit) = match &callee.kind {
                    ExprKind::Name(n) => (n, None),
                    ExprKind::TypeApply { callee, arguments } => match &callee.kind {
                        ExprKind::Name(n) => (n, Some(arguments)),
                        _ => {
                            return Err(unsupported(
                                e.span,
                                "parametric qualified callable resolution",
                            ));
                        }
                    },
                    _ => {
                        return Err(unsupported(
                            e.span,
                            "parametric indirect callable resolution",
                        ));
                    }
                };
                if let Some(local) = state.locals.get(target) {
                    // A factory's visible signature may itself be large. Pay
                    // its full type before cloning the symbolic local.
                    self.budget.ty(&local.ty)?;
                    let local = local.clone();
                    let Some(bound) = self.callable.clone() else {
                        return Err(unsupported(
                            e.span,
                            "generic callable parameter requires a Fn bound",
                        ));
                    };
                    let (kind, mode, parameters, result, factory_local) = if let Some(mode) =
                        callable::parameter_mode(&local.ty, &bound.generic_name)
                    {
                        (bound.kind, mode, bound.parameters, bound.result, false)
                    } else if let Some((kind, parameters, result)) =
                        callable::opaque_signature(&local.ty)
                    {
                        for parameter in parameters {
                            self.budget.ty(parameter)?;
                        }
                        self.budget.ty(result)?;
                        (kind, None, parameters.to_vec(), result.clone(), true)
                    } else {
                        return Err(error(
                            e.span,
                            "called generic local is neither the callable parameter nor a direct opaque factory result",
                        ));
                    };
                    if explicit.is_some() || arguments.len() != parameters.len() {
                        return Err(error(e.span, "callable bound argument count differs"));
                    }
                    if state.moved.contains(&local.id) {
                        return Err(error(
                            e.span,
                            "generic callable used after its environment was consumed",
                        ));
                    }
                    match kind {
                        zil_ir::CallableKind::Fn => {}
                        zil_ir::CallableKind::FnMut if mode == Some(true) => {}
                        zil_ir::CallableKind::FnMut if factory_local && local.mutable => {}
                        zil_ir::CallableKind::FnMut if factory_local => {
                            return Err(error(
                                e.span,
                                "local FnMut call requires a mutable binding",
                            ));
                        }
                        zil_ir::CallableKind::FnOnce if mode.is_none() => {
                            state.moved.insert(local.id);
                        }
                        _ => {
                            return Err(unsupported(
                                e.span,
                                "callable bound requires its matching environment passing mode",
                            ));
                        }
                    }
                    for (argument, parameter) in arguments.iter().zip(&parameters) {
                        if argument.spread {
                            return Err(unsupported(argument.span, "callable helper tuple spread"));
                        }
                        let actual =
                            self.argument(&argument.value, state, parameter, Some(parameter))?;
                        same(&actual, parameter, argument.span, self.budget)?;
                    }
                    return Ok(result);
                }
                let function = self
                    .signatures
                    .get(target)
                    .ok_or_else(|| error(e.span, "unknown function in parametric body"))?;
                if self.callable.is_some() && callable::template(function)?.is_some() {
                    return Err(unsupported(
                        e.span,
                        "recursive or nested callable helper contracts are not implemented",
                    ));
                }
                if arguments.len() != function.params.len() {
                    return Err(error(e.span, "parametric call arity mismatch"));
                }
                let variables: BTreeSet<_> =
                    function.generics.iter().map(|p| p.name.clone()).collect();
                let mut found = BTreeMap::new();
                if let Some(args) = explicit {
                    if args.len() != function.generics.len() {
                        return Err(error(e.span, "parametric type argument arity mismatch"));
                    }
                    for (param, arg) in function.generics.iter().zip(args) {
                        let GenericArg::Type(ty) = arg else {
                            return Err(unsupported(e.span, "const generic argument"));
                        };
                        found.insert(param.name.clone(), ty.clone());
                    }
                }
                if let (Some(result), Some(expected)) = (&function.return_type, expected) {
                    unify(
                        result,
                        expected,
                        &variables,
                        &mut found,
                        e.span,
                        self.budget,
                    )?;
                }
                for (parameter, argument) in function.params.iter().zip(arguments) {
                    if argument.spread {
                        return Err(unsupported(argument.span, "parametric tuple spread"));
                    }
                    let hint = substituted_type(&parameter.ty, &found, self.budget)?;
                    let actual = self.argument(
                        &argument.value,
                        state,
                        &hint,
                        (!dependent(&hint, &variables)).then_some(&hint),
                    )?;
                    unify(
                        &parameter.ty,
                        &actual,
                        &variables,
                        &mut found,
                        e.span,
                        self.budget,
                    )?;
                }
                if variables.iter().any(|name| !found.contains_key(name)) {
                    return Err(error(e.span, "unconstrained nested generic argument"));
                }
                match &function.return_type {
                    Some(result) => substituted_type(result, &found, self.budget),
                    None => Ok(unit(e.span)),
                }
            }
            ExprKind::Try(operand) if self.callable.is_some() => {
                let union = self.expr(operand, state, None, false)?;
                let TypeKind::Error {
                    error: actual_error,
                    result,
                } = &group(&union).kind
                else {
                    return Err(error(e.span, "propagation requires an error union"));
                };
                let TypeKind::Error {
                    error: expected_error,
                    ..
                } = &group(&self.result).kind
                else {
                    return Err(error(
                        e.span,
                        "propagation requires an error-returning helper",
                    ));
                };
                same(actual_error, expected_error, e.span, self.budget)?;
                Ok(result.as_ref().clone())
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition_type =
                    self.expr(condition, state, Some(&named("bool", e.span)), false)?;
                same(&condition_type, &named("bool", e.span), e.span, self.budget)?;
                let mut a = self.clone_state(state)?;
                let mut b = self.clone_state(state)?;
                let lhs = self.expr(then_value, &mut a, expected, borrowing)?;
                let rhs = self.expr(else_value, &mut b, Some(&lhs), borrowing)?;
                same(&lhs, &rhs, e.span, self.budget)?;
                state.moved.extend(a.moved);
                state.moved.extend(b.moved);
                Ok(lhs)
            }
            ExprKind::Member {
                object,
                member: MemberName::TupleIndex(i),
            } => {
                let value = self.expr(object, state, None, borrowing)?;
                let TypeKind::Tuple(fields) = &group(&value).kind else {
                    return Err(needs_bound());
                };
                fields
                    .get(*i as usize)
                    .cloned()
                    .ok_or_else(|| error(e.span, "generic tuple index outside its static shape"))
            }
            ExprKind::Member {
                object,
                member: MemberName::Name(member),
            } => {
                let value = self.expr(object, state, None, borrowing)?;
                let TypeKind::Named { path, args } = &group(&value).kind else {
                    return Err(needs_bound());
                };
                let Some(Item {
                    kind: ItemKind::Aggregate(aggregate),
                    ..
                }) = self.nominals.get(&path.join("."))
                else {
                    return Err(needs_bound());
                };
                if aggregate.generics.len() != args.len() {
                    return Err(error(e.span, "generic record argument arity mismatch"));
                }
                let field = aggregate
                    .fields
                    .iter()
                    .find(|field| &field.name == member)
                    .ok_or_else(|| error(e.span, "unknown generic record field"))?;
                if !field.public && field.span.file != e.span.file {
                    return Err(Diagnostic::error(
                        Phase::Expand,
                        "E-FIELD-PRIVATE",
                        Some(e.span),
                        "record field is private to its declaring module",
                    ));
                }
                let mut substitutions = BTreeMap::new();
                for (p, arg) in aggregate.generics.iter().zip(args) {
                    let GenericArg::Type(ty) = arg else {
                        return Err(unsupported(e.span, "generic constant field shape"));
                    };
                    substitutions.insert(p.name.clone(), ty.clone());
                }
                substituted_type(&field.ty, &substitutions, self.budget)
            }
            _ => Err(unsupported(
                e.span,
                "this generic expression requires additional symbolic checking",
            )),
        }
    }
    // Return true when every path through this block has returned. Branches
    // keep separate move states, so a returning arm cannot consume the value
    // on a distinct fallthrough path.
    fn block(&mut self, block: &Block, state: &mut State) -> R<bool> {
        self.tick(block.span)?;
        let mut returned = false;
        for statement in &block.statements {
            self.tick(statement.span)?;
            if returned {
                return Err(unsupported(
                    statement.span,
                    "unreachable-statement type checking",
                ));
            }
            match &statement.kind {
                StmtKind::Binding {
                    pattern:
                        Pattern {
                            kind: PatternKind::Name(n),
                            ..
                        },
                    ty,
                    value,
                    mutable,
                } => {
                    let actual = self.expr(value, state, ty.as_ref(), false)?;
                    if let Some(ty) = ty {
                        same(&actual, ty, statement.span, self.budget)?;
                    }
                    self.bind_local(state, n.clone(), ty.clone().unwrap_or(actual))?;
                    state.locals.get_mut(n).expect("just bound").mutable = *mutable;
                }
                StmtKind::Assign { target, op, value } => {
                    let (ty, writable) = self.place(target, state)?;
                    if !writable {
                        return Err(error(target.span, "assignment requires a writable place"));
                    }
                    if *op != AssignOp::Set && (!integer(&ty) || dependent(&ty, &self.variables)) {
                        return Err(unsupported(
                            target.span,
                            "compound assignment requires a known integer place",
                        ));
                    }
                    let actual = self.expr(value, state, Some(&ty), false)?;
                    same(&actual, &ty, value.span, self.budget)?;
                }
                StmtKind::Return(value) => {
                    let expected = self.result.clone();
                    let actual = match value {
                        Some(e) => self.expr(e, state, Some(&expected), false)?,
                        None => unit(statement.span),
                    };
                    if self.callable.is_some() {
                        same_return(&actual, &expected, statement.span, self.budget)?;
                    } else {
                        same(&actual, &expected, statement.span, self.budget)?;
                    }
                    returned = true;
                }
                StmtKind::Expression(e) => {
                    self.expr(e, state, None, false)?;
                }
                StmtKind::If {
                    condition: Condition::Expression(condition),
                    then_body,
                    else_body,
                } => {
                    let actual = self.expr(
                        condition,
                        state,
                        Some(&named("bool", condition.span)),
                        false,
                    )?;
                    same(
                        &actual,
                        &named("bool", condition.span),
                        condition.span,
                        self.budget,
                    )?;
                    let mut a = self.clone_state(state)?;
                    let mut b = self.clone_state(state)?;
                    let returns_a = self.block(then_body, &mut a)?;
                    let returns_b = match else_body {
                        Some(ElseBranch::Block(block)) => self.block(block, &mut b)?,
                        None => false,
                        Some(ElseBranch::If(statement)) => self.block(
                            &Block {
                                span: statement.span,
                                statements: vec![*statement.clone()],
                            },
                            &mut b,
                        )?,
                    };
                    match (returns_a, returns_b) {
                        (true, true) => returned = true,
                        (true, false) => state.moved = b.moved,
                        (false, true) => state.moved = a.moved,
                        (false, false) => {
                            state.moved.extend(a.moved);
                            state.moved.extend(b.moved);
                        }
                    }
                }
                StmtKind::Match { value, arms } => {
                    returned = self.enum_match(value, arms, state)?;
                }
                _ => {
                    return Err(unsupported(
                        statement.span,
                        "this parametric statement requires additional symbolic checking",
                    ));
                }
            }
        }
        Ok(returned)
    }
}

#[path = "generic_check_enum.rs"]
mod enums;

#[path = "generic_places.rs"]
mod places;
