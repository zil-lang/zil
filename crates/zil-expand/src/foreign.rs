//! Source-shaped foreign declarations; no ABI correctness is certified here.
use super::*;

impl Expander<'_> {
    pub(super) fn foreign_constructor(
        &mut self,
        callee: &s::Expr,
        arguments: &[s::Argument],
        span: Span,
    ) -> Option<CoreExprKind> {
        Some(match literal_constructor(callee, arguments, span)? {
            Ok(value) => CoreExprKind::Literal(value),
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                CoreExprKind::Unsupported("invalid static foreign constructor".into())
            }
        })
    }

    pub(super) fn foreign(
        &mut self,
        abi: &str,
        function: &s::Function,
        public: bool,
        prefix: Option<&str>,
        host: Option<&Type>,
    ) {
        if primitive_name(&function.name) {
            self.diagnostics
                .push(primitive_name_error(&function.name, function.span));
            return;
        }
        if public || prefix.is_some() || host.is_some() {
            self.unsupported(
                "foreign imports must be private module declarations; export a ZIL wrapper",
                function.span,
            );
            return;
        }
        if !matches!(function.body, s::FunctionBody::Declaration) {
            self.unsupported("extern declarations cannot contain a body", function.span);
            return;
        }
        if !function.generics.is_empty() || !function.where_clause.is_empty() {
            self.unsupported(
                "generic or annotated foreign callable contracts",
                function.span,
            );
            return;
        }
        let (noescape_parameters, static_callbacks) = match noescape_contract(function) {
            Ok(indices) => indices,
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                return;
            }
        };
        let id = self.id();
        self.map_node(id, function.span, ExpansionRole::Source, Vec::new());
        let parameters = function.params.iter().map(|p| self.param(p)).collect();
        let return_type = function
            .return_type
            .as_ref()
            .map_or(Type::Unit, |t| self.type_ref(t));
        self.functions.push(CoreFunction {
            callable_return: None,
            callable_parameter: None,
            result_mode: zil_ir::PassingMode::Infer,
            id,
            name: function.name.clone(),
            // Syntax retains the string token spelling. Only the literal C ABI
            // spelling enters this bounded slice; no opaque token is authority.
            foreign: Some(ForeignDeclaration {
                abi: if abi == "\"C\"" {
                    "C".into()
                } else {
                    abi.into()
                },
                symbol: function.name.clone(),
                noescape_parameters,
                static_callbacks,
            }),
            parameters,
            return_type,
            body: CoreBlock {
                statements: Vec::new(),
                span: function.span,
            },
            receiver: None,
            span: function.span,
        });
    }
}

pub(super) fn bare_name(expression: &s::Expr) -> Option<&str> {
    match &expression.kind {
        s::ExprKind::Name(name) => Some(name),
        s::ExprKind::Group(inner) => bare_name(inner),
        _ => None,
    }
}

pub(super) fn callback_error(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-FFI-CALLBACK", Some(span), message)
}

/// Check the fixed part of a written callback type before aliases or unused
/// templates can disappear. With no scope, names may still denote aliases;
/// after normalization, only actual type parameters may remain unresolved.
pub(super) fn callback_type_formation(
    ty: &s::TypeRef,
    variables: Option<&BTreeSet<String>>,
) -> Result<(), Diagnostic> {
    let s::TypeKind::ForeignFunction {
        abi,
        params,
        result,
    } = &ty.kind
    else {
        return Ok(());
    };
    let admissible = |mut ty: &s::TypeRef, result: bool| {
        while let s::TypeKind::Group(inner) = &ty.kind {
            ty = inner;
        }
        match &ty.kind {
            s::TypeKind::Named { path, args } if args.is_empty() => {
                if let [name] = path.as_slice() {
                    match name.as_str() {
                        "i32" | "i64" | "c_handle" => true,
                        "void" => result,
                        "u8" | "u16" | "bool" | "c_wstr" => false,
                        _ => variables.is_none_or(|names| names.contains(name)),
                    }
                } else {
                    variables.is_none()
                }
            }
            s::TypeKind::Tuple(fields) if fields.is_empty() => result,
            _ => false,
        }
    };
    if abi != "\"C\"" || !params.iter().all(|ty| admissible(ty, false)) || !admissible(result, true)
    {
        return Err(Diagnostic::error(
            Phase::Expand,
            "E-FFI-CALLBACK-TYPE",
            Some(ty.span),
            "C callback types require the literal C ABI, i32/i64/c_handle parameters and an additional unit result; dependent type parameters must satisfy this profile after instantiation",
        ));
    }
    Ok(())
}

/// Signature/field check for a template whose aliases have already resolved.
/// The bounded walk preserves unresolved T while rejecting known nominal or
/// structural arguments which can never specialize to a C callback scalar.
pub(super) fn callback_type_tree(
    ty: &s::TypeRef,
    variables: &BTreeSet<String>,
    budget: &mut generics::Budget<'_>,
    depth: usize,
) -> Result<(), Diagnostic> {
    budget.node(ty.span, depth)?;
    match &ty.kind {
        s::TypeKind::Named { args, .. } | s::TypeKind::Dyn { args, .. } => {
            for arg in args {
                if let s::GenericArg::Type(ty) = arg {
                    callback_type_tree(ty, variables, budget, depth + 1)?;
                }
            }
        }
        s::TypeKind::Reference { inner, .. }
        | s::TypeKind::Group(inner)
        | s::TypeKind::Optional(inner)
        | s::TypeKind::ForLifetime { inner, .. }
        | s::TypeKind::Array { element: inner, .. } => {
            callback_type_tree(inner, variables, budget, depth + 1)?;
        }
        s::TypeKind::Tuple(types) | s::TypeKind::Union(types) | s::TypeKind::Impl(types) => {
            for ty in types {
                callback_type_tree(ty, variables, budget, depth + 1)?;
            }
        }
        s::TypeKind::Function { params, result }
        | s::TypeKind::ForeignFunction { params, result, .. } => {
            for ty in params {
                callback_type_tree(ty, variables, budget, depth + 1)?;
            }
            callback_type_tree(result, variables, budget, depth + 1)?;
        }
        s::TypeKind::Error { error, result } => {
            callback_type_tree(error, variables, budget, depth + 1)?;
            callback_type_tree(result, variables, budget, depth + 1)?;
        }
        s::TypeKind::MetaEval(_) | s::TypeKind::MetaBlock(_) => {}
    }
    // Children have passed the depth/work bound before grouped scalar checks.
    callback_type_formation(ty, Some(variables))
}

pub(super) fn literal_constructor(
    callee: &s::Expr,
    arguments: &[s::Argument],
    span: Span,
) -> Option<Result<Constant, Diagnostic>> {
    let name = bare_name(callee)?;
    if !matches!(name, "c_null" | "c_wstr_null" | "c_wstr") {
        return None;
    }
    let value = match (name, arguments) {
        ("c_null", []) => Some(Constant::NullHandle),
        ("c_wstr_null", []) => Some(Constant::NullWideString),
        ("c_wstr", [argument]) if !argument.spread => {
            if let s::ExprKind::Literal(s::Literal::String(spelling)) = &argument.value.kind {
                wide_literal(spelling).map(Constant::WideString)
            } else {
                None
            }
        }
        _ => None,
    };
    Some(value.ok_or_else(|| Diagnostic::error(
        Phase::Expand,
        "E-FFI-CONSTRUCTOR",
        Some(span),
        "c_null()/c_wstr_null() take no arguments; c_wstr takes one literal without embedded NUL",
    )))
}

/// A source signature hint for finite generic normalization, not a callback
/// safety certificate. Types and Verify check the actual resolved function.
pub(super) fn callback_syntax_type(
    function: &s::Function,
    span: Span,
) -> Result<s::TypeRef, Diagnostic> {
    let valid = |ty: &Type| matches!(ty, Type::I32 | Type::I64 | Type::ForeignHandle);
    let result = function.return_type.as_ref().map_or(Type::Unit, type_ref);
    if !function.generics.is_empty()
        || !function.contract.is_empty()
        || matches!(function.body, s::FunctionBody::Declaration)
        || !function.params.iter().all(|p| valid(&type_ref(&p.ty)))
        || !(result == Type::Unit || valid(&result))
    {
        return Err(callback_error(
            span,
            "generic callback expression requires one ordinary static function with a concrete C callback signature",
        ));
    }
    Ok(s::TypeRef {
        span,
        kind: s::TypeKind::ForeignFunction {
            abi: "\"C\"".into(),
            params: function.params.iter().map(|p| p.ty.clone()).collect(),
            result: Box::new(function.return_type.clone().unwrap_or(s::TypeRef {
                span,
                kind: s::TypeKind::Tuple(vec![]),
            })),
        },
    })
}

/// Decode the lexer's scalar escapes before UTF-16 encoding. This path also
/// rejects malformed caller-built AST literals; syntax data is not authority.
fn wide_literal(spelling: &str) -> Option<Vec<u16>> {
    let inner = spelling.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    let mut result = Vec::new();
    while let Some(mut ch) = chars.next() {
        if ch == '\\' {
            ch = match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '0' => '\0',
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'u' if chars.next()? == '{' => {
                    let mut digits = String::new();
                    loop {
                        let digit = chars.next()?;
                        if digit == '}' {
                            break;
                        }
                        if !digit.is_ascii_hexdigit() || digits.len() == 6 {
                            return None;
                        }
                        digits.push(digit);
                    }
                    char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?
                }
                _ => return None,
            };
        }
        if ch == '\0' {
            return None;
        }
        let mut encoded = [0; 2];
        result.extend_from_slice(ch.encode_utf16(&mut encoded));
    }
    result.push(0);
    Some(result)
}

fn noescape_contract(function: &s::Function) -> Result<(Vec<u32>, bool), Diagnostic> {
    let bad =
        |span, message| Diagnostic::error(Phase::Expand, "E-FFI-DECLARATION", Some(span), message);
    let mut ffi = false;
    let mut static_callbacks = false;
    let mut parameters = BTreeSet::new();
    let mut positions = BTreeMap::new();
    for (index, parameter) in function.params.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| bad(parameter.span, "foreign parameter index overflow"))?;
        if positions.insert(parameter.name.as_str(), index).is_some() {
            return Err(bad(
                parameter.span,
                "foreign parameter names must be unique",
            ));
        }
    }
    for clause in &function.contract {
        match clause.name.as_str() {
            "effects" if clause.terms.len() == 1 => {
                let term = &clause.terms[0];
                if ffi || term.path.as_slice() != ["ffi"] || !simple(term) {
                    return Err(bad(
                        clause.span,
                        "foreign effects must contain exactly one ffi marker",
                    ));
                }
                ffi = true;
            }
            "provides" if !clause.terms.is_empty() => {
                for term in &clause.terms {
                    if term.path.as_slice() == ["static_callbacks"] && simple(term) {
                        if static_callbacks {
                            return Err(bad(
                                clause.span,
                                "static_callbacks appears more than once",
                            ));
                        }
                        static_callbacks = true;
                        continue;
                    }
                    if term.path.as_slice() != ["noescape"]
                        || !term.type_args.is_empty()
                        || term.index.is_some()
                        || term.arguments.len() != 1
                    {
                        return Err(bad(
                            clause.span,
                            "foreign provides requires noescape(parameter) or static_callbacks() terms",
                        ));
                    }
                    let name = &term.arguments[0];
                    if name.path.len() != 1 || !simple(name) {
                        return Err(bad(
                            clause.span,
                            "noescape target must be one parameter name",
                        ));
                    }
                    let Some(index) = positions.get(name.path[0].as_str()).copied() else {
                        return Err(bad(clause.span, "noescape names an unknown parameter"));
                    };
                    if !parameters.insert(index) {
                        return Err(bad(
                            clause.span,
                            "noescape parameter appears more than once",
                        ));
                    }
                }
            }
            _ => {
                return Err(bad(
                    clause.span,
                    "unsupported or empty foreign contract clause",
                ));
            }
        }
    }
    if (!parameters.is_empty() || static_callbacks) && !ffi {
        return Err(bad(
            function.span,
            "borrowed foreign parameters and static callbacks require effects(ffi)",
        ));
    }
    Ok((parameters.into_iter().collect(), static_callbacks))
}

fn simple(term: &s::ContractTerm) -> bool {
    term.type_args.is_empty() && term.arguments.is_empty() && term.index.is_none()
}
