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
    fn enum_fields(
        &mut self,
        ty: &TypeRef,
        variant: &str,
        mut managed: Option<&mut Vec<usize>>,
    ) -> R<Vec<TypeRef>> {
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
            .enumerate()
            .map(|(index, t)| {
                if t.manages {
                    if !decl.generics.is_empty() {
                        return Err(unsupported(t.span, "owned generic enum payload"));
                    }
                    if let Some(managed) = &mut managed {
                        self.budget.node(t.span, 0)?;
                        managed.push(index);
                    }
                }
                syntax_enum::substituted(t, &found, self.budget)
            })
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
                let mut managed = Vec::new();
                let fields = self.enum_fields(ty, variant, Some(&mut managed))?;
                if fields.len() != arguments.len() {
                    return Err(error(p.span, "generic enum pattern payload arity mismatch"));
                }
                self.budget.text(variant, p.span)?;
                Pat::Ctor(
                    Ctor::Variant(variant.clone()),
                    arguments
                        .iter()
                        .zip(&fields)
                        .enumerate()
                        .map(|(index, (p, t))| {
                            let pattern = self.pattern(p, t, state, seen)?;
                            if !managed.is_empty()
                                && managed.binary_search(&index).is_ok()
                                && let PatternKind::Name(name) = &p.kind
                            {
                                self.budget.node(p.span, 0)?;
                                for _ in
                                    0..(state.locals.len().saturating_add(1).ilog2() as usize + 2)
                                {
                                    self.budget.text(name, p.span)?;
                                }
                                // Resolve this real declaration field in its own scope,
                                // not against the helper's possibly same-spelled binders.
                                self.budget.ty(t)?;
                                let declaration_resource = self.declared_resource(t);
                                let roots =
                                    if declaration_resource {
                                        Some(self.resource_description_root(
                                            state.locals[name].id,
                                            p.span,
                                        )?)
                                    } else {
                                        None
                                    };
                                let local =
                                    state.locals.get_mut(name).expect("actual pattern binding");
                                local.owned_resource = true;
                                local.declaration_resource = declaration_resource;
                                local.description_roots = roots;
                                local.description_reference_depth =
                                    declaration_resource.then_some(0);
                                self.enum_resource_flow = true;
                                if stores::element(&local.ty).is_some() {
                                    // Extraction is incoming ownership, never a create producer.
                                    self.budget.node(p.span, 0)?;
                                    state.store_owners.insert(local.id);
                                    state
                                        .store_origins
                                        .insert(local.id, BTreeSet::from([local.id]));
                                }
                            }
                            Ok(pattern)
                        })
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
    ) -> R<BlockFlow> {
        let ty = self.expr(value, state, None, false)?;
        let mut rows = Vec::new();
        let mut continuing = Vec::new();
        let mut flow = BlockFlow {
            fallthrough: false,
            returns: false,
        };
        for arm in arms {
            let mut branch = self.clone_state(state)?;
            let first_resource_local = self.next_local;
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
            let branch_flow = match &arm.body {
                ArrowBody::Block(b) => self.block(b, &mut branch)?,
                ArrowBody::Expression(e) => {
                    self.expr(e, &mut branch, None, false)?;
                    BlockFlow::NEXT
                }
            };
            self.resource_description_scope_exit(&mut branch, first_resource_local, arm.span)?;
            flow.returns |= branch_flow.returns;
            if branch_flow.fallthrough {
                flow.fallthrough = true;
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
        if flow.fallthrough {
            self.join_states(state, &continuing.iter().collect::<Vec<_>>(), value.span)?;
        }
        Ok(flow)
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
            (Ctor::Variant(v), _) => self.enum_fields(ty, v, None),
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
