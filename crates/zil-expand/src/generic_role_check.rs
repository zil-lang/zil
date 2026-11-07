//! Declaration-local symbolic class checking. These keys never enter generic
//! instance caches, and class equations never supply actual target permissions.
use super::*;

fn push(out: &mut String, value: &str, budget: &mut Budget<'_>, span: Span) -> R<()> {
    budget.identity_work(value.len().saturating_mul(2).saturating_add(1), span)?;
    out.push_str(value);
    Ok(())
}
fn number(out: &mut String, value: usize, budget: &mut Budget<'_>, span: Span) -> R<()> {
    budget.identity_work(
        if value == 0 {
            1
        } else {
            value.ilog10() as usize + 1
        },
        span,
    )?;
    push(out, &value.to_string(), budget, span)
}
fn declared(name: &str, source: &Function, budget: &mut Budget<'_>, span: Span) -> R<usize> {
    for (ordinal, formal) in source.generics.iter().enumerate() {
        budget.node(span, 0)?;
        if matches!(formal.kind, GenericParamKind::Lifetime) {
            budget.text(&formal.name, span)?;
            budget.text(name, span)?;
            if formal.name == name {
                return Ok(ordinal);
            }
        }
    }
    Err(error(
        span,
        "qualified class has no declared source lifetime binder",
    ))
}
#[derive(Clone, Copy)]
pub(super) struct Scope<'a> {
    pub(super) source: &'a Function,
    pub(super) active: bool,
}
impl<'a> Scope<'a> {
    pub(super) fn new(source: &'a Function, callable: bool, budget: &mut Budget<'_>) -> R<Self> {
        // A declaration without a callable role cannot have a role-local
        // class scope. Do not scan or charge its generic list for that query.
        if !callable {
            return Ok(Self {
                source,
                active: false,
            });
        }
        budget.identity_work(source.generics.len(), source.span)?;
        let has_life = source
            .generics
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime));
        Ok(Self {
            source,
            active: callable && has_life,
        })
    }
}
pub(super) fn key(ty: &TypeRef, scope: Scope<'_>, budget: &mut Budget<'_>) -> R<String> {
    if !scope.active {
        return type_key(ty, budget);
    }
    let source = scope.source;
    fn render(
        ty: &TypeRef,
        source: &Function,
        budget: &mut Budget<'_>,
        out: &mut String,
        depth: usize,
    ) -> R<()> {
        budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Group(inner) => render(inner, source, budget, out, depth + 1),
            TypeKind::Reference { mutable, inner, .. } => {
                push(out, if *mutable { "M[" } else { "R[" }, budget, ty.span)?;
                render(inner, source, budget, out, depth + 1)?;
                push(out, "]", budget, ty.span)
            }
            TypeKind::Named { path, args } => {
                push(out, "N", budget, ty.span)?;
                number(out, path.len(), budget, ty.span)?;
                for name in path {
                    number(out, name.len(), budget, ty.span)?;
                    push(out, ":", budget, ty.span)?;
                    push(out, name, budget, ty.span)?;
                }
                push(out, "<", budget, ty.span)?;
                for arg in args {
                    budget.node(ty.span, depth)?;
                    match arg {
                        GenericArg::Lifetime(name) => {
                            push(out, "L", budget, ty.span)?;
                            let ordinal = declared(name, source, budget, ty.span)?;
                            number(out, ordinal, budget, ty.span)?;
                        }
                        GenericArg::Type(inner) => {
                            push(out, "T", budget, ty.span)?;
                            render(inner, source, budget, out, depth + 1)?;
                        }
                        GenericArg::Const(value) => {
                            budget.const_expression_depth(value, depth + 1)?;
                            let key = consts::key(value)?;
                            push(out, "C", budget, ty.span)?;
                            number(out, key.len(), budget, ty.span)?;
                            push(out, &key, budget, ty.span)?;
                        }
                    }
                    push(out, ";", budget, ty.span)?;
                }
                push(out, ">", budget, ty.span)
            }
            TypeKind::Tuple(fields) => {
                push(out, "(", budget, ty.span)?;
                for field in fields {
                    render(field, source, budget, out, depth + 1)?;
                    push(out, ",", budget, ty.span)?;
                }
                push(out, ")", budget, ty.span)
            }
            TypeKind::Error { error: err, result } => {
                push(out, "E[", budget, ty.span)?;
                render(err, source, budget, out, depth + 1)?;
                push(out, "|", budget, ty.span)?;
                render(result, source, budget, out, depth + 1)?;
                push(out, "]", budget, ty.span)
            }
            _ => {
                let key = type_key(ty, budget)?;
                push(out, "X", budget, ty.span)?;
                number(out, key.len(), budget, ty.span)?;
                push(out, &key, budget, ty.span)
            }
        }
    }
    let mut out = String::new();
    render(ty, source, budget, &mut out, 0)?;
    Ok(out)
}
pub(super) fn same(
    actual: &TypeRef,
    expected: &TypeRef,
    span: Span,
    scope: Scope<'_>,
    budget: &mut Budget<'_>,
) -> R<()> {
    if !scope.active {
        return super::same(actual, expected, span, budget);
    }
    if key(actual, scope, budget)? == key(expected, scope, budget)? {
        Ok(())
    } else {
        Err(error(
            span,
            "generic body types and declared qualified classes must agree before instantiation",
        ))
    }
}
pub(super) fn same_return(
    actual: &TypeRef,
    expected: &TypeRef,
    span: Span,
    scope: Scope<'_>,
    budget: &mut Budget<'_>,
) -> R<()> {
    same_return_selected(actual, expected, span, scope, budget).map(|_| ())
}
pub(super) fn same_return_selected(
    actual: &TypeRef,
    expected: &TypeRef,
    span: Span,
    scope: Scope<'_>,
    budget: &mut Budget<'_>,
) -> R<Option<bool>> {
    if !scope.active {
        return super::same_return_selected(actual, expected, span, budget);
    }
    let actual_key = key(actual, scope, budget)?;
    if actual_key == key(expected, scope, budget)? {
        return Ok(None);
    }
    if let TypeKind::Error { error: err, result } = &group(expected).kind {
        let error_match = actual_key == key(err, scope, budget)?;
        let result_match = actual_key == key(result, scope, budget)?;
        if error_match != result_match {
            return Ok(Some(error_match));
        }
    }
    same(actual, expected, span, scope, budget).map(|_| None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rigid {
    Declared(usize),
    Elided(usize),
}
pub(super) struct Frame {
    names: Vec<String>,
    bindings: Vec<Option<Rigid>>,
}
impl Frame {
    pub(super) fn nominal(
        parameters: &[GenericParam],
        arguments: &[GenericArg],
        source: &Function,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Self> {
        if parameters.len() != arguments.len() {
            return Err(error(span, "generic record argument arity mismatch"));
        }
        let mut frame = Self {
            names: Vec::new(),
            bindings: Vec::new(),
        };
        for (parameter, argument) in parameters.iter().zip(arguments) {
            budget.node(span, 0)?;
            match (&parameter.kind, argument) {
                (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                    let ordinal = declared(name, source, budget, span)?;
                    budget.text(&parameter.name, span)?;
                    budget.lineage_bytes(std::mem::size_of::<(String, Option<Rigid>)>(), span)?;
                    frame.names.push(parameter.name.clone());
                    frame.bindings.push(Some(Rigid::Declared(ordinal)));
                }
                (GenericParamKind::Type(_), GenericArg::Type(_))
                | (GenericParamKind::Const(_), GenericArg::Const(_)) => {}
                _ => {
                    return Err(error(
                        span,
                        "generic record actual kind differs from its declared binder",
                    ));
                }
            }
        }
        Ok(frame)
    }
    pub(super) fn new(
        generic: &str,
        source: &Function,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<Self>> {
        for formal in &source.generics {
            budget.node(span, 0)?;
            budget.text(&formal.name, span)?;
            budget.text(generic, span)?;
            if formal.name != generic {
                continue;
            }
            let GenericParamKind::Type(bounds) = &formal.kind else {
                continue;
            };
            for bound in bounds {
                budget.node(span, 0)?;
                let Some(code) = rich_pointer::role_code(bound, budget)? else {
                    continue;
                };
                let mut inner = &code;
                let mut names = Vec::new();
                loop {
                    budget.node(inner.span, 0)?;
                    match &inner.kind {
                        TypeKind::Group(next) => inner = next,
                        TypeKind::ForLifetime {
                            lifetime,
                            inner: next,
                        } => {
                            budget.text(lifetime, span)?;
                            names.push(lifetime.clone());
                            inner = next;
                        }
                        _ => break,
                    }
                }
                for _ in &names {
                    budget.node(span, 0)?;
                }
                return Ok(Some(Self {
                    bindings: vec![None; names.len()],
                    names,
                }));
            }
        }
        Ok(None)
    }
    fn variable(&self, name: &str, budget: &mut Budget<'_>, span: Span) -> R<usize> {
        for (index, candidate) in self.names.iter().enumerate().rev() {
            budget.node(span, 0)?;
            budget.text(name, span)?;
            budget.text(candidate, span)?;
            if candidate == name {
                return Ok(index);
            }
        }
        Err(error(span, "role lifetime has no invocation binder"))
    }
    fn bind(&mut self, name: &str, actual: Rigid, budget: &mut Budget<'_>, span: Span) -> R<()> {
        let index = self.variable(name, budget, span)?;
        if let Some(prior) = self.bindings[index] {
            if prior != actual {
                return Err(error(
                    span,
                    "role invocation cannot equate distinct declared lifetime classes",
                ));
            }
        } else {
            self.bindings[index] = Some(actual)
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn argument(
        &mut self,
        expected: &TypeRef,
        actual: &TypeRef,
        expression: &Expr,
        state: &State,
        source: &Function,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(expected.span, depth)?;
        budget.node(actual.span, depth)?;
        if let TypeKind::Group(inner) = &expected.kind {
            return self.argument(inner, actual, expression, state, source, budget, depth + 1);
        }
        if let TypeKind::Group(inner) = &actual.kind {
            return self.argument(
                expected,
                inner,
                expression,
                state,
                source,
                budget,
                depth + 1,
            );
        }
        match (&expected.kind, &actual.kind) {
            (
                TypeKind::Reference {
                    lifetime: Some(name),
                    inner: want,
                    ..
                },
                TypeKind::Reference {
                    lifetime,
                    inner: have,
                    ..
                },
            ) => {
                let rigid = if let Some(name) = lifetime {
                    Rigid::Declared(declared(name, source, budget, actual.span)?)
                } else {
                    let mut expression = expression;
                    while let ExprKind::Group(inner) = &expression.kind {
                        budget.node(expression.span, depth)?;
                        expression = inner;
                    }
                    let ExprKind::Name(name) = &expression.kind else {
                        return Err(unsupported(
                            expression.span,
                            "elided rich role input needs its actual source place context",
                        ));
                    };
                    budget.text(name, expression.span)?;
                    let local = state
                        .locals
                        .get(name)
                        .ok_or_else(|| error(expression.span, "role input local is absent"))?;
                    Rigid::Elided(local.id)
                };
                self.bind(name, rigid, budget, actual.span)?;
                self.argument(want, have, expression, state, source, budget, depth + 1)
            }
            (TypeKind::Named { args: want, .. }, TypeKind::Named { args: have, .. }) => {
                if want.len() != have.len() {
                    return Err(error(actual.span, "role nominal actual arity differs"));
                }
                for (want, have) in want.iter().zip(have) {
                    budget.node(actual.span, depth)?;
                    match (want, have) {
                        (GenericArg::Lifetime(name), GenericArg::Lifetime(actual_name)) => {
                            let actual = Rigid::Declared(declared(
                                actual_name,
                                source,
                                budget,
                                actual.span,
                            )?);
                            self.bind(name, actual, budget, expected.span)?;
                        }
                        (GenericArg::Type(want), GenericArg::Type(have)) => {
                            self.argument(want, have, expression, state, source, budget, depth + 1)?
                        }
                        (GenericArg::Const(_), GenericArg::Const(_)) => {}
                        _ => return Err(error(actual.span, "role nominal actual kind differs")),
                    }
                }
                Ok(())
            }
            (TypeKind::Tuple(want), TypeKind::Tuple(have)) if want.len() == have.len() => {
                for (want, have) in want.iter().zip(have) {
                    self.argument(want, have, expression, state, source, budget, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub(super) fn substitute(
        &self,
        ty: &TypeRef,
        source: &Function,
        budget: &mut Budget<'_>,
    ) -> R<TypeRef> {
        budget.ty(ty)?;
        let mut ty = ty.clone();
        fn label(
            frame: &Frame,
            name: &str,
            source: &Function,
            budget: &mut Budget<'_>,
            span: Span,
        ) -> R<Option<String>> {
            let index = frame.variable(name, budget, span)?;
            match frame.bindings[index] {
                Some(Rigid::Declared(ordinal)) => {
                    let name = &source.generics[ordinal].name;
                    budget.text(name, span)?;
                    Ok(Some(name.clone()))
                }
                Some(Rigid::Elided(_)) => Ok(None),
                None => Err(error(span, "role result class has no actual input binding")),
            }
        }
        fn walk(
            frame: &Frame,
            ty: &mut TypeRef,
            source: &Function,
            budget: &mut Budget<'_>,
            depth: usize,
        ) -> R<()> {
            budget.node(ty.span, depth)?;
            match &mut ty.kind {
                TypeKind::Reference {
                    lifetime, inner, ..
                } => {
                    if let Some(name) = lifetime {
                        *lifetime = label(frame, name, source, budget, ty.span)?;
                    }
                    walk(frame, inner, source, budget, depth + 1)
                }
                TypeKind::Named { args, .. } => {
                    for arg in args {
                        budget.node(ty.span, depth)?;
                        match arg {
                            GenericArg::Lifetime(name) => {
                                *name=label(frame,name,source,budget,ty.span)?.ok_or_else(||unsupported(ty.span,"qualified role class requires a declared actual lifetime"))?;
                            }
                            GenericArg::Type(inner) => {
                                walk(frame, inner, source, budget, depth + 1)?
                            }
                            GenericArg::Const(_) => {}
                        }
                    }
                    Ok(())
                }
                TypeKind::Group(inner) => walk(frame, inner, source, budget, depth + 1),
                TypeKind::Tuple(fields) => {
                    for field in fields {
                        walk(frame, field, source, budget, depth + 1)?;
                    }
                    Ok(())
                }
                TypeKind::ForLifetime { .. }
                | TypeKind::Function { .. }
                | TypeKind::ForeignFunction { .. }
                | TypeKind::Error { .. }
                | TypeKind::Array { .. }
                | TypeKind::Optional(_)
                | TypeKind::Union(_)
                | TypeKind::Impl(_)
                | TypeKind::Dyn { .. } => Err(unsupported(
                    ty.span,
                    "rich role substitution needs this constructor's scoped adapter",
                )),
                _ => Ok(()),
            }
        }
        walk(self, &mut ty, source, budget, 0)?;
        Ok(ty)
    }
}

pub(super) fn reject_callee_context(function: &Function, budget: &mut Budget<'_>) -> R<()> {
    use crate::ct_selection::Visitor;
    struct Check<'a, 'l> {
        budget: &'a mut Budget<'l>,
    }
    impl Visitor for Check<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(ty.kind, TypeKind::ForLifetime { .. }) {
                return self.budget.ty_depth(ty, depth).map_err(|e| vec![e]);
            }
            if let TypeKind::Named { args, .. } = &ty.kind {
                for arg in args {
                    self.node(ty.span, depth)?;
                    if matches!(arg, GenericArg::Lifetime(_)) {
                        return Err(vec![unsupported(
                            ty.span,
                            "qualified helper call needs its actual callee lifetime substitution",
                        )]);
                    }
                }
            }
            crate::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut check = Check { budget };
    for ty in function
        .params
        .iter()
        .map(|p| &p.ty)
        .chain(function.return_type.iter())
    {
        check.ty(ty, 0).map_err(|mut e| e.remove(0))?;
    }
    for formal in &function.generics {
        check.node(formal.span, 0).map_err(|mut e| e.remove(0))?;
        if let GenericParamKind::Type(bounds) = &formal.kind {
            for bound in bounds {
                check.node(bound.span, 0).map_err(|mut e| e.remove(0))?;
                if rich_pointer::role_code(bound, check.budget)?.is_some() {
                    return Err(unsupported(
                        bound.span,
                        "rich role forwarding needs one complete universal substitution",
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_storage_role_binders_survive_without_a_direct_address_relation() {
        let source = zil_base::SourceUnit::new(
            zil_base::FileId(0),
            zil_base::ModuleId(0),
            "role-binders.zil",
            "fn invoke<F:for<life a>Fn(Tag<'a>)->i32>(f:&F,value:i32)->i32{return value}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let ItemKind::Function(function) = &syntax.items[0].kind else {
            panic!("source function");
        };
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let classified = callable::template(function, &mut budget).unwrap().unwrap();
        let role = &classified.roles[0];
        assert!(role.life.is_none());
        assert!(role.has_role_binders);
        // The actual role-frame consumer must still discover this binder.
        assert!(
            Frame::new(&role.generic_name, function, &mut budget, function.span)
                .unwrap()
                .is_some()
        );
    }
}
