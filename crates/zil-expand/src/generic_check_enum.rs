//! Definition-time enum constructor and match checking, before instantiation.
use super::super::enums as syntax_enum;
use super::*;

#[derive(Clone, PartialEq, Eq)]
enum Ctor {
    Tuple,
    Bool(bool),
    Integer(i64),
    Variant(String),
    Opaque,
}
#[derive(Clone)]
enum Pat {
    Any,
    Ctor(Ctor, Vec<Pat>),
}

impl Check<'_, '_, '_> {
    pub(super) fn enum_expression(
        &mut self,
        e: &Expr,
        state: &mut State,
        expected: Option<&TypeRef>,
    ) -> R<Option<TypeRef>> {
        let (callee, args) = match &e.kind {
            ExprKind::Call { callee, arguments } => (callee.as_ref(), arguments.as_slice()),
            _ => (e, &[][..]),
        };
        let Some((name, variant, explicit)) = syntax_enum::constructor(callee) else {
            return Ok(None);
        };
        if state.locals.contains_key(name) {
            return Ok(None);
        }
        let Some(decl) = self.enums.get(name).copied() else {
            return Ok(None);
        };
        let fields = syntax_enum::fields(decl, variant, e.span)?;
        if args.len() != fields.len() {
            return Err(error(
                e.span,
                "parametric enum constructor payload arity mismatch",
            ));
        }
        let vars = decl
            .generics
            .iter()
            .map(|p| p.name.clone())
            .collect::<BTreeSet<_>>();
        let mut found = BTreeMap::new();
        if let Some(explicit) = explicit {
            if explicit.len() != decl.generics.len() {
                return Err(error(
                    e.span,
                    "parametric enum type argument arity mismatch",
                ));
            }
            for (p, arg) in decl.generics.iter().zip(explicit) {
                let GenericArg::Type(t) = arg else {
                    return Err(unsupported(e.span, "const enum type argument"));
                };
                self.budget.ty(t)?;
                found.insert(p.name.clone(), t.clone());
            }
        }
        syntax_enum::context(decl, expected, &mut found, self.budget)?;
        for (arg, field) in args.iter().zip(fields) {
            if arg.spread {
                return Err(unsupported(arg.span, "enum payload tuple spread"));
            }
            let hint = syntax_enum::substituted(field, &found, self.budget)?;
            let unresolved = vars
                .iter()
                .any(|v| !found.contains_key(v) && syntax_enum::contains(&hint, v));
            let actual = self.expr(&arg.value, state, (!unresolved).then_some(&hint), false)?;
            unify(field, &actual, &vars, &mut found, arg.span, self.budget)?;
        }
        syntax_enum::application(decl, &found, e.span, self.budget).map(Some)
    }
    fn enum_fields(&mut self, ty: &TypeRef, variant: &str) -> R<Vec<TypeRef>> {
        let TypeKind::Named { path, args } = &group(ty).kind else {
            return Err(error(
                ty.span,
                "constructor pattern requires a declared enum",
            ));
        };
        let decl = self
            .enums
            .get(&path.join("."))
            .copied()
            .ok_or_else(|| error(ty.span, "unknown enum in generic pattern"))?;
        if decl.generics.len() != args.len() {
            return Err(error(ty.span, "generic enum pattern type arity mismatch"));
        }
        let mut found = BTreeMap::new();
        for (p, arg) in decl.generics.iter().zip(args) {
            let GenericArg::Type(t) = arg else {
                return Err(unsupported(ty.span, "const enum argument"));
            };
            self.budget.ty(t)?;
            found.insert(p.name.clone(), t.clone());
        }
        syntax_enum::fields(decl, variant, ty.span)?
            .iter()
            .map(|t| syntax_enum::substituted(t, &found, self.budget))
            .collect()
    }
    fn pattern(
        &mut self,
        p: &Pattern,
        ty: &TypeRef,
        state: &mut State,
        seen: &mut BTreeSet<String>,
    ) -> R<Pat> {
        self.tick(p.span)?;
        Ok(match &p.kind {
            PatternKind::Name(name) => {
                if !seen.insert(name.clone()) {
                    return Err(error(p.span, "duplicate generic pattern binding"));
                }
                self.budget.ty(ty)?;
                self.bind_local(state, name.clone(), ty.clone())?;
                Pat::Any
            }
            PatternKind::Wildcard => Pat::Any,
            PatternKind::Literal(Literal::Bool(v)) if boolean(ty) => {
                Pat::Ctor(Ctor::Bool(*v), vec![])
            }
            PatternKind::Literal(Literal::Integer(v)) if integer(ty) => {
                Pat::Ctor(Ctor::Integer(integer_value(v, ty, p.span)?), vec![])
            }
            PatternKind::Tuple(ps) => {
                let TypeKind::Tuple(ts) = &group(ty).kind else {
                    return Err(error(
                        p.span,
                        "tuple pattern requires tuple type before instantiation",
                    ));
                };
                if ps.len() != ts.len() {
                    return Err(error(p.span, "generic tuple pattern arity mismatch"));
                }
                Pat::Ctor(
                    Ctor::Tuple,
                    ps.iter()
                        .zip(ts)
                        .map(|(p, t)| self.pattern(p, t, state, seen))
                        .collect::<R<_>>()?,
                )
            }
            PatternKind::Constructor { path, arguments } => {
                let alias = path.first().and_then(|name| self.pattern_types.get(name));
                if let Some(alias) = alias {
                    same(ty, alias, p.span, self.budget)?;
                }
                let TypeKind::Named { path: base, .. } = &group(ty).kind else {
                    return Err(error(p.span, "enum pattern requires declared enum type"));
                };
                if path.len() != base.len() + 1
                    || (alias.is_none() && path[..path.len() - 1] != base[..])
                {
                    return Err(error(p.span, "generic pattern nominal type mismatch"));
                }
                let variant = path.last().expect("variant");
                let fields = self.enum_fields(ty, variant)?;
                if fields.len() != arguments.len() {
                    return Err(error(p.span, "generic enum pattern payload arity mismatch"));
                }
                self.budget.text(variant, p.span)?;
                Pat::Ctor(
                    Ctor::Variant(variant.clone()),
                    arguments
                        .iter()
                        .zip(&fields)
                        .map(|(p, t)| self.pattern(p, t, state, seen))
                        .collect::<R<_>>()?,
                )
            }
            _ => {
                return Err(unsupported(
                    p.span,
                    "parametric pattern requires a known type constructor",
                ));
            }
        })
    }
    pub(super) fn enum_match(
        &mut self,
        value: &Expr,
        arms: &[MatchArm],
        state: &mut State,
    ) -> R<bool> {
        let ty = self.expr(value, state, None, false)?;
        let mut rows = Vec::new();
        let mut continuing = Vec::new();
        let mut all_return = true;
        for arm in arms {
            let mut branch = self.clone_state(state)?;
            let p = self.pattern(&arm.pattern, &ty, &mut branch, &mut BTreeSet::new())?;
            if !self.useful(
                &rows,
                std::slice::from_ref(&p),
                std::slice::from_ref(&ty),
                0,
                arm.span,
            )? {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-MATCH-UNREACHABLE",
                    Some(arm.span),
                    "generic match arm is already covered",
                ));
            }
            rows.push(vec![p]);
            let returned = match &arm.body {
                ArrowBody::Block(b) => self.block(b, &mut branch)?,
                ArrowBody::Expression(e) => {
                    self.expr(e, &mut branch, None, false)?;
                    false
                }
            };
            if !returned {
                all_return = false;
                continuing.push(branch);
            }
        }
        if self.useful(&rows, &[Pat::Any], std::slice::from_ref(&ty), 0, value.span)? {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-MATCH-NONEXHAUSTIVE",
                Some(value.span),
                "generic match does not cover every possible value",
            ));
        }
        if !all_return {
            self.join_states(state, &continuing.iter().collect::<Vec<_>>(), value.span)?;
        }
        Ok(all_return)
    }
    fn charge_pattern(&mut self, p: &Pat, depth: usize, span: Span) -> R<()> {
        self.budget.node(span, depth)?;
        if let Pat::Ctor(constructor, fields) = p {
            if let Ctor::Variant(name) = constructor {
                self.budget.text(name, span)?;
            }
            for f in fields {
                self.charge_pattern(f, depth + 1, span)?;
            }
        }
        Ok(())
    }
    fn constructors(&mut self, ty: &TypeRef) -> R<Vec<Ctor>> {
        Ok(match &group(ty).kind {
            TypeKind::Tuple(_) => vec![Ctor::Tuple],
            TypeKind::Named { path, .. } if path.as_slice() == ["bool"] => {
                vec![Ctor::Bool(false), Ctor::Bool(true)]
            }
            TypeKind::Named { path, .. } if self.enums.contains_key(&path.join(".")) => {
                let decl = self.enums[&path.join(".")];
                let mut variants = Vec::new();
                for v in &decl.variants {
                    self.budget.node(v.span, 0)?;
                    self.budget.text(&v.name, v.span)?;
                    variants.push(Ctor::Variant(v.name.clone()));
                }
                variants
            }
            _ => vec![Ctor::Opaque],
        })
    }
    fn constructor_fields(&mut self, c: &Ctor, ty: &TypeRef) -> R<Vec<TypeRef>> {
        match (c, &group(ty).kind) {
            (Ctor::Tuple, TypeKind::Tuple(fields)) => {
                for f in fields {
                    self.budget.ty(f)?;
                }
                Ok(fields.clone())
            }
            (Ctor::Variant(v), _) => self.enum_fields(ty, v),
            _ => Ok(vec![]),
        }
    }
    fn specialize(
        &mut self,
        rows: &[Vec<Pat>],
        c: &Ctor,
        arity: usize,
        span: Span,
    ) -> R<Vec<Vec<Pat>>> {
        for row in rows {
            match &row[0] {
                Pat::Any => {
                    for _ in 0..arity {
                        self.budget.node(span, 0)?;
                    }
                }
                Pat::Ctor(actual, fields) if actual == c => {
                    for p in fields {
                        self.charge_pattern(p, 0, span)?;
                    }
                }
                _ => continue,
            }
            for p in &row[1..] {
                self.charge_pattern(p, 0, span)?;
            }
        }
        Ok(specialize(rows, c, arity))
    }
    fn useful(
        &mut self,
        rows: &[Vec<Pat>],
        query: &[Pat],
        types: &[TypeRef],
        depth: usize,
        span: Span,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        for p in rows.iter().flatten().chain(query) {
            self.charge_pattern(p, 0, span)?;
        }
        for ty in types {
            self.budget.ty(ty)?;
        }
        if query.is_empty() {
            return Ok(rows.is_empty());
        }
        if rows.iter().any(|r| r.iter().all(|p| matches!(p, Pat::Any))) {
            return Ok(false);
        }
        if let Pat::Ctor(c, fields) = &query[0] {
            let mut next = fields.clone();
            next.extend_from_slice(&query[1..]);
            let mut ts = self.constructor_fields(c, &types[0])?;
            ts.extend_from_slice(&types[1..]);
            let rows = self.specialize(rows, c, fields.len(), span)?;
            return self.useful(&rows, &next, &ts, depth + 1, span);
        }
        let ctors = self.constructors(&types[0])?;
        for c in ctors {
            if c == Ctor::Opaque {
                let defaults = rows
                    .iter()
                    .filter(|r| matches!(r[0], Pat::Any))
                    .map(|r| r[1..].to_vec())
                    .collect::<Vec<_>>();
                return self.useful(&defaults, &query[1..], &types[1..], depth + 1, span);
            }
            let mut ts = self.constructor_fields(&c, &types[0])?;
            let n = ts.len();
            ts.extend_from_slice(&types[1..]);
            let mut next = vec![Pat::Any; n];
            next.extend_from_slice(&query[1..]);
            let specialized = self.specialize(rows, &c, n, span)?;
            if self.useful(&specialized, &next, &ts, depth + 1, span)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn specialize(rows: &[Vec<Pat>], c: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|row| {
            let fields = match &row[0] {
                Pat::Any => vec![Pat::Any; arity],
                Pat::Ctor(actual, fields) if actual == c => fields.clone(),
                _ => return None,
            };
            Some(fields.into_iter().chain(row[1..].iter().cloned()).collect())
        })
        .collect()
}
fn integer_value(text: &str, ty: &TypeRef, span: Span) -> R<i64> {
    let (actual, value) = checked_integer(text, Some(ty), false, span)?;
    if type_key_bounded(&actual)? != type_key_bounded(ty)? {
        return Err(error(
            span,
            "generic integer pattern suffix disagrees with its type",
        ));
    }
    Ok(value)
}
