//! Source class constraints before each qualified type gets its own local class0.
//! These private facts describe lexical type meaning, never loans or authority.
use super::*;

pub(super) fn qualified_nominal(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<bool> {
    budget.node(ty.span, 0)?;
    let TypeKind::Named { path, args } = &grouped(ty).kind else {
        return Ok(false);
    };
    let [name] = path.as_slice() else {
        return Ok(false);
    };
    budget.text(name, ty.span)?;
    let Some(item) = nominals.get(name) else {
        return Ok(false);
    };
    let formals = match &item.kind {
        ItemKind::Aggregate(a) if a.kind == AggregateKind::Struct => &a.generics,
        ItemKind::Enum(e) => &e.generics,
        _ => return Ok(false),
    };
    if formals.len() != args.len() {
        return Ok(false);
    }
    let mut qualified = false;
    for (formal, actual) in formals.iter().zip(args) {
        budget.node(formal.span, 0)?;
        qualified |= matches!(
            (&formal.kind, actual),
            (GenericParamKind::Lifetime, GenericArg::Lifetime(_))
        );
    }
    Ok(qualified)
}

/// A mixed ordinary helper may compare qualified nominal source classes only
/// when its written signature actually contains that nominal application.
/// The nominal catalog and lexical life checker remain separate proof inputs.
pub(super) fn qualified_signature(
    function: &Function,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<bool> {
    if !budget.has_stored_lifetimes() || !super::super::lifetimes::copy_life(function) {
        return Ok(false);
    }
    struct Qualified<'a, 'b, 'l> {
        nominals: &'a BTreeMap<String, &'a Item>,
        budget: &'b mut Budget<'l>,
        inside_nominal: bool,
        found: bool,
    }
    impl crate::ct_selection::Visitor for Qualified<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            let old = self.inside_nominal;
            if let TypeKind::Named { path, args } = &ty.kind
                && let [name] = path.as_slice()
                && !args.is_empty()
            {
                self.budget.text(name, ty.span).map_err(|e| vec![e])?;
                self.inside_nominal |= self.nominals.contains_key(name);
                if self.inside_nominal {
                    for argument in args {
                        self.budget.node(ty.span, depth).map_err(|e| vec![e])?;
                        self.found |= matches!(argument, GenericArg::Lifetime(_));
                    }
                }
            }
            self.found |= self.inside_nominal
                && matches!(
                    ty.kind,
                    TypeKind::Reference {
                        lifetime: Some(_),
                        ..
                    }
                );
            let result = crate::ct_selection::walk_type(self, ty, depth);
            self.inside_nominal = old;
            result
        }
    }
    use crate::ct_selection::Visitor;
    let mut visit = Qualified {
        nominals,
        budget,
        inside_nominal: false,
        found: false,
    };
    for ty in function
        .params
        .iter()
        .map(|p| &p.ty)
        .chain(function.return_type.iter())
    {
        visit.ty(ty, 0).map_err(|mut errors| errors.remove(0))?;
    }
    Ok(visit.found)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Binder(usize, usize); // actual declaration in this walk, original full ordinal
#[derive(Clone, PartialEq, Eq)]
enum Head {
    // A reference to the actual aggregate declaration in this borrowed source
    // inventory. This is never a name-based semantic identity or cache key.
    Declaration(usize),
    Enum(usize),
    Plain(Vec<String>),
}
#[derive(Clone)]
enum Fact {
    Unknown,
    Class(Binder),
    Parameter(Binder),
    Named(Head, Vec<Fact>),
    Address(Option<Box<Fact>>, Box<Fact>),
    Tuple(Vec<Fact>),
    List(Box<Fact>),
    Function(Vec<Fact>, Box<Fact>),
}
type Facts = BTreeMap<String, Fact>;
type Substitutions = BTreeMap<Binder, Fact>;
struct Scope<'a> {
    owner: usize,
    parameters: &'a [GenericParam],
    host: Option<usize>,
}
struct Check<'a, 'b, 'l> {
    functions: Vec<&'a Function>,
    records: Vec<&'a Aggregate>,
    enums: Vec<&'a EnumDecl>,
    hosts: Vec<Option<usize>>,
    standalone: Vec<bool>,
    fact_depth: usize,
    budget: &'b mut Budget<'l>,
}
impl Fact {
    fn has_class(&self) -> bool {
        match self {
            Self::Class(_) => true,
            Self::Named(_, fs) | Self::Tuple(fs) => fs.iter().any(Self::has_class),
            Self::Address(_, inner) | Self::List(inner) => inner.has_class(),
            Self::Function(params, result) => {
                params.iter().any(Self::has_class) || result.has_class()
            }
            Self::Unknown | Self::Parameter(_) => false,
        }
    }
}
impl Check<'_, '_, '_> {
    fn pay(&mut self, span: Span, n: usize) -> R<()> {
        self.budget.identity_work(n, span)
    }
    fn fact<T>(&mut self, span: Span, visit: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        self.budget.node(span, self.fact_depth)?;
        self.fact_depth += 1;
        let result = visit(self);
        self.fact_depth -= 1;
        result
    }
    fn work(&mut self, f: &Fact, span: Span) -> R<()> {
        self.fact(span, |this| this.work_inner(f, span))
    }
    fn work_inner(&mut self, f: &Fact, span: Span) -> R<()> {
        match f {
            Fact::Named(path, fs) => {
                if let Head::Plain(path) = path {
                    for part in path {
                        self.budget.text(part, span)?;
                    }
                }
                for f in fs {
                    self.work(f, span)?;
                }
            }
            Fact::Tuple(fs) => {
                for f in fs {
                    self.work(f, span)?;
                }
            }
            Fact::Address(life, inner) => {
                if let Some(life) = life {
                    self.work(life, span)?;
                }
                self.work(inner, span)?;
            }
            Fact::List(inner) => self.work(inner, span)?,
            Fact::Function(params, result) => {
                for f in params {
                    self.work(f, span)?;
                }
                self.work(result, span)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn copy(&mut self, f: &Fact, span: Span) -> R<Fact> {
        self.work(f, span)?;
        Ok(f.clone())
    }
    fn lookup(&mut self, entries: usize, width: usize, span: Span) -> R<()> {
        self.pay(
            span,
            entries
                .saturating_add(1)
                .saturating_mul(width.saturating_add(1))
                .saturating_mul(2),
        )
    }
    fn environment(&mut self, env: &Facts, span: Span) -> R<Facts> {
        for (name, f) in env {
            self.budget.text(name, span)?;
            self.work(f, span)?;
        }
        Ok(env.clone())
    }
    fn binder(&mut self, scope: &Scope<'_>, name: &str, span: Span) -> R<Option<(Binder, bool)>> {
        for (ordinal, p) in scope.parameters.iter().enumerate() {
            self.budget.node(p.span, 0)?;
            self.budget.text(name, span)?;
            self.budget.text(&p.name, p.span)?;
            if p.name == name {
                return Ok(Some((
                    Binder(scope.owner, ordinal),
                    matches!(p.kind, GenericParamKind::Lifetime),
                )));
            }
        }
        if let Some(host) = scope.host {
            let parameters = self.records[host].generics.as_slice();
            for (ordinal, p) in parameters.iter().enumerate() {
                self.budget.node(p.span, 0)?;
                self.budget.text(name, span)?;
                self.budget.text(&p.name, p.span)?;
                if p.name == name {
                    return Ok(Some((
                        Binder(self.functions.len() + host, ordinal),
                        matches!(p.kind, GenericParamKind::Lifetime),
                    )));
                }
            }
        }
        Ok(None)
    }
    fn self_type(&mut self, scope: &Scope<'_>, span: Span) -> R<Option<Fact>> {
        let Some(host) = scope.host else {
            return Ok(None);
        };
        let mut args = Vec::new();
        for (ordinal, p) in self.records[host].generics.iter().enumerate() {
            self.budget.node(p.span, 0)?;
            args.push(if matches!(p.kind, GenericParamKind::Lifetime) {
                Fact::Class(Binder(self.functions.len() + host, ordinal))
            } else {
                Fact::Parameter(Binder(self.functions.len() + host, ordinal))
            });
        }
        self.pay(span, 1)?;
        Ok(Some(Fact::Named(Head::Declaration(host), args)))
    }
    fn unknown_type(&mut self, ty: &TypeRef, scope: &Scope<'_>, depth: usize) -> R<Fact> {
        struct Free<'a, 'b, 'c, 'd, 'l> {
            check: &'a mut Check<'b, 'c, 'l>,
            scope: &'d Scope<'d>,
            local: Vec<String>,
            found: bool,
        }
        impl crate::ct_selection::Visitor for Free<'_, '_, '_, '_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                self.check.budget.node(span, depth).map_err(|e| vec![e])
            }
            fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                self.check.budget.text(text, span).map_err(|e| vec![e])
            }
            fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
                if let TypeKind::ForLifetime { lifetime, inner } = &ty.kind {
                    self.check
                        .budget
                        .node(ty.span, depth)
                        .map_err(|e| vec![e])?;
                    self.check
                        .budget
                        .text(lifetime, ty.span)
                        .map_err(|e| vec![e])?;
                    self.local.push(lifetime.clone());
                    self.ty(inner, depth + 1)?;
                    self.local.pop();
                    return Ok(());
                }
                let mut names = Vec::new();
                match &ty.kind {
                    TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                        for arg in args {
                            self.check
                                .budget
                                .node(ty.span, depth)
                                .map_err(|e| vec![e])?;
                            if let GenericArg::Lifetime(name) = arg {
                                names.push(name.as_str());
                            }
                        }
                    }
                    TypeKind::Reference {
                        lifetime: Some(name),
                        ..
                    } => names.push(name.as_str()),
                    _ => {}
                }
                for name in names {
                    self.check
                        .lookup(self.local.len(), name.len(), ty.span)
                        .map_err(|e| vec![e])?;
                    if !self.local.iter().any(|local| local == name)
                        && self
                            .check
                            .binder(self.scope, name, ty.span)
                            .map_err(|e| vec![e])?
                            .is_some_and(|(_, life)| life)
                    {
                        self.found = true;
                    }
                }
                crate::ct_selection::walk_type(self, ty, depth)
            }
        }
        use crate::ct_selection::Visitor;
        let mut visit = Free {
            check: self,
            scope,
            local: Vec::new(),
            found: false,
        };
        visit.ty(ty, depth).map_err(|mut e| e.remove(0))?;
        if visit.found {
            return Err(unsupported(
                ty.span,
                "source lifetime-bearing type constructor requires its complete class consumer",
            ));
        }
        Ok(Fact::Unknown)
    }
    fn ty(&mut self, ty: &TypeRef, scope: &Scope<'_>, depth: usize) -> R<Fact> {
        self.budget.node(ty.span, depth)?;
        Ok(match &ty.kind {
            TypeKind::Group(inner) => self.ty(inner, scope, depth + 1)?,
            TypeKind::Named { path, args } => {
                if path.as_slice() == ["Self"]
                    && args.is_empty()
                    && let Some(f) = self.self_type(scope, ty.span)?
                {
                    return Ok(f);
                }
                if let [name] = path.as_slice()
                    && args.is_empty()
                    && let Some((binder, false)) = self.binder(scope, name, ty.span)?
                {
                    return Ok(Fact::Parameter(binder));
                }
                for part in path {
                    self.budget.text(part, ty.span)?;
                }
                let mut actuals = Vec::new();
                for arg in args {
                    self.budget.node(ty.span, depth)?;
                    actuals.push(match arg {
                        GenericArg::Type(t) => self.ty(t, scope, depth + 1)?,
                        GenericArg::Lifetime(name) => match self.binder(scope, name, ty.span)? {
                            Some((binder, true)) => Fact::Class(binder),
                            _ => return Err(unsupported(ty.span,"nominal lifetime actual requires its genuine enclosing source scope")),
                        },
                        _ => Fact::Unknown,
                    });
                }
                let qualified = actuals.iter().any(Fact::has_class);
                Fact::Named(self.head(path, ty.span, qualified)?, actuals)
            }
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                let life = lifetime
                    .as_ref()
                    .map(|name| self.binder(scope, name, ty.span))
                    .transpose()?
                    .flatten()
                    .map(|(b, _)| Box::new(Fact::Class(b)));
                Fact::Address(life, Box::new(self.ty(inner, scope, depth + 1)?))
            }
            TypeKind::Tuple(fs) => {
                let mut out = Vec::new();
                for f in fs {
                    out.push(self.ty(f, scope, depth + 1)?);
                }
                Fact::Tuple(out)
            }
            TypeKind::Array { element, .. } | TypeKind::Optional(element) => {
                Fact::List(Box::new(self.ty(element, scope, depth + 1)?))
            }
            TypeKind::Function { params, result } => {
                let mut out = Vec::new();
                for p in params {
                    out.push(self.ty(p, scope, depth + 1)?);
                }
                Fact::Function(out, Box::new(self.ty(result, scope, depth + 1)?))
            }
            // Closed quantified Code has a distinct local binder domain.
            _ => self.unknown_type(ty, scope, depth + 1)?,
        })
    }
    fn head(&mut self, path: &[String], span: Span, qualified: bool) -> R<Head> {
        self.pay(span, path.len().saturating_add(1))?;
        for part in path {
            self.budget.text(part, span)?;
        }
        let spelling = path.join(".");
        for (index, declaration) in self.records.iter().enumerate() {
            self.budget.text(&spelling, span)?;
            self.budget.text(&declaration.name, span)?;
            if declaration.name == spelling {
                return Ok(Head::Declaration(index));
            }
        }
        if qualified {
            for (index, declaration) in self.enums.iter().enumerate() {
                self.budget.text(&spelling, span)?;
                self.budget.text(&declaration.name, span)?;
                if declaration.name == spelling {
                    return Ok(Head::Enum(index));
                }
            }
        }
        Ok(Head::Plain(path.to_vec()))
    }
    fn compatible(&mut self, expected: &Fact, actual: &Fact, span: Span) -> R<()> {
        self.fact(span, |this| {
        // Pay both the structural comparison and the class-presence queries.
        this.work(expected, span)?;
        this.work(actual, span)?;
        this.work(expected, span)?;
        this.work(actual, span)?;
        match (expected, actual) {
            (Fact::Class(a), Fact::Class(b)) if a != b => {
                return Err(error(
                    span,
                    "source annotation cannot equate distinct declared lifetime classes",
                ));
            }
            (Fact::Named(Head::Declaration(a), xs), Fact::Named(Head::Declaration(b), ys)) => {
                if expected.has_class() && (a != b || xs.len() != ys.len()) {
                    return Err(error(
                        span,
                        "source class annotation has a different actual nominal declaration or argument arity",
                    ));
                }
                if a == b && xs.len() == ys.len() {
                    for (x, y) in xs.iter().zip(ys) {
                        this.compatible(x, y, span)?;
                    }
                }
            }
            (Fact::Named(Head::Enum(a), xs), Fact::Named(Head::Enum(b), ys)) => {
                if expected.has_class() && (a != b || xs.len() != ys.len()) {
                    return Err(error(span, "source enum class annotation differs from actual declaration"));
                }
                if a == b && xs.len() == ys.len() {
                    for (x, y) in xs.iter().zip(ys) { this.compatible(x, y, span)?; }
                }
            }
            (Fact::Named(Head::Plain(a), xs), Fact::Named(Head::Plain(b), ys))
                if !expected.has_class() && a == b && xs.len() == ys.len() => {}
            (Fact::Tuple(xs), Fact::Tuple(ys)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys) {
                    this.compatible(x, y, span)?;
                }
            }
            (Fact::Function(xs, x), Fact::Function(ys, y)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys) {
                    this.compatible(x, y, span)?;
                }
                this.compatible(x, y, span)?;
            }
            // Ordinary outer borrow compatibility remains the existing lifetime
            // and kernel consumer's responsibility. Pointee classes are invariant.
            (Fact::Address(_, x), Fact::Address(_, y)) | (Fact::List(x), Fact::List(y)) => {
                this.compatible(x, y, span)?
            }
            (Fact::Class(a), Fact::Class(b)) if a == b => {}
            (x, _) if x.has_class() => {
                return Err(unsupported(
                    span,
                    "ambient source annotation requires its actual declaration and class substitution",
                ));
            }
            _ => {}
        }
        Ok(())

        })
    }
    fn substitute(&mut self, f: &Fact, map: &Substitutions, span: Span) -> R<Fact> {
        self.fact(span, |this| {
            this.work(f, span)?;
            Ok(match f {
                Fact::Class(b) | Fact::Parameter(b) => {
                    this.lookup(map.len(), 2, span)?;
                    match map.get(b) {
                        Some(f) => this.copy(f, span)?,
                        None => Fact::Unknown,
                    }
                }
                Fact::Named(path, fs) => {
                    let mut out = Vec::new();
                    for f in fs {
                        out.push(this.substitute(f, map, span)?);
                    }
                    Fact::Named(path.clone(), out)
                }
                Fact::Tuple(fs) => {
                    let mut out = Vec::new();
                    for f in fs {
                        out.push(this.substitute(f, map, span)?);
                    }
                    Fact::Tuple(out)
                }
                Fact::Address(life, inner) => Fact::Address(
                    life.as_ref()
                        .map(|l| this.substitute(l, map, span).map(Box::new))
                        .transpose()?,
                    Box::new(this.substitute(inner, map, span)?),
                ),
                Fact::List(f) => Fact::List(Box::new(this.substitute(f, map, span)?)),
                Fact::Function(params, result) => {
                    let mut out = Vec::new();
                    for p in params {
                        out.push(this.substitute(p, map, span)?);
                    }
                    Fact::Function(out, Box::new(this.substitute(result, map, span)?))
                }
                _ => this.copy(f, span)?,
            })
        })
    }
    fn infer(
        &mut self,
        formal: &Fact,
        actual: &Fact,
        map: &mut Substitutions,
        span: Span,
        invariant: bool,
    ) -> R<()> {
        self.fact(span, |this| {
            this.infer_inner(formal, actual, map, span, invariant)
        })
    }
    fn infer_inner(
        &mut self,
        formal: &Fact,
        actual: &Fact,
        map: &mut Substitutions,
        span: Span,
        invariant: bool,
    ) -> R<()> {
        self.work(formal, span)?;
        self.work(actual, span)?;
        if invariant {
            self.work(formal, span)?;
        }
        match (formal, actual) {
            (f, Fact::Unknown) if invariant && f.has_class() => {
                return Err(unsupported(
                    span,
                    "source class inference requires the complete actual qualified type",
                ));
            }
            (Fact::Class(b), actual) if !invariant => {
                // Ordinary borrow uppers may admit an intersection. Keep an
                // actual single-input map, but never select one of several
                // distinct declared inputs as a retained nominal class.
                self.pay(span, map.len().saturating_add(1))?;
                if let Some(prior) = map.get(b) {
                    let same = matches!((prior,actual),(Fact::Class(a),Fact::Class(b)) if a==b);
                    if !same {
                        map.insert(*b, Fact::Unknown);
                    }
                } else {
                    map.insert(*b, self.copy(actual, span)?);
                }
            }
            (Fact::Class(_), Fact::Unknown) if invariant => {
                return Err(unsupported(
                    span,
                    "source class inference has an unknown actual class",
                ));
            }
            (Fact::Class(b) | Fact::Parameter(b), actual) => {
                self.pay(span, map.len().saturating_add(1))?;
                if let Some(prior) = map.get(b) {
                    self.work(prior, span)?;
                    self.work(actual, span)?;
                    if matches!(prior, Fact::Unknown) && actual.has_class() {
                        return Err(unsupported(
                            span,
                            "source class inference cannot refine an earlier unknown actual",
                        ));
                    }
                    self.compatible(prior, actual, span)?;
                } else {
                    map.insert(*b, self.copy(actual, span)?);
                }
            }
            (Fact::Named(a, fs), Fact::Named(b, xs)) if a == b => {
                for (f, x) in fs.iter().zip(xs) {
                    self.infer(f, x, map, span, invariant)?;
                }
            }
            (Fact::Tuple(fs), Fact::Tuple(xs)) => {
                for (f, x) in fs.iter().zip(xs) {
                    self.infer(f, x, map, span, invariant)?;
                }
            }
            (Fact::Address(f, fi), Fact::Address(x, xi)) => {
                if let (Some(f), Some(x)) = (f, x) {
                    self.infer(f, x, map, span, invariant)?;
                }
                self.infer(fi, xi, map, span, invariant)?;
            }
            (Fact::List(f), Fact::List(x)) => self.infer(f, x, map, span, invariant)?,
            (Fact::Function(fs, fr), Fact::Function(xs, xr)) => {
                for (f, x) in fs.iter().zip(xs) {
                    self.infer(f, x, map, span, invariant)?;
                }
                self.infer(fr, xr, map, span, invariant)?;
            }
            (f, _) if invariant && f.has_class() => {
                return Err(unsupported(
                    span,
                    "source class inference cannot omit a qualified parameter relation",
                ));
            }
            _ => {}
        }
        Ok(())
    }
    fn bind(&mut self, p: &Pattern, fact: &Fact, env: &mut Facts) -> R<()> {
        self.fact(p.span, |this| this.bind_inner(p, fact, env))
    }
    fn bind_inner(&mut self, p: &Pattern, fact: &Fact, env: &mut Facts) -> R<()> {
        match &p.kind {
            PatternKind::Name(name) => {
                self.budget.text(name, p.span)?;
                self.lookup(env.len(), name.len(), p.span)?;
                env.insert(name.clone(), self.copy(fact, p.span)?);
            }
            PatternKind::Tuple(ps) => {
                for (i, p) in ps.iter().enumerate() {
                    let f = if let Fact::Tuple(fs) = fact {
                        fs.get(i).unwrap_or(&Fact::Unknown)
                    } else {
                        &Fact::Unknown
                    };
                    self.bind(p, f, env)?;
                }
            }
            PatternKind::Reference { pattern, .. } => {
                self.bind(
                    pattern,
                    if let Fact::Address(_, f) = fact {
                        f
                    } else {
                        &Fact::Unknown
                    },
                    env,
                )?;
            }
            PatternKind::Constructor { path, arguments }
                if let Fact::Named(Head::Enum(index), actuals) = fact =>
            {
                let declaration = self.enums.get(*index).copied().ok_or_else(|| {
                    unsupported(p.span, "source enum pattern declaration is absent")
                })?;
                if path.len() != 2
                    || path[0] != declaration.name
                    || actuals.len() != declaration.generics.len()
                {
                    return Err(unsupported(
                        p.span,
                        "source enum pattern differs from actual declaration",
                    ));
                }
                let variant = declaration
                    .variants
                    .iter()
                    .find(|v| v.name == path[1])
                    .ok_or_else(|| unsupported(p.span, "source enum pattern variant is absent"))?;
                let fields: &[VariantTupleField] = match &variant.fields {
                    VariantFields::Unit => &[],
                    VariantFields::Tuple(fields) => fields,
                    VariantFields::Record(_) => {
                        return Err(unsupported(
                            p.span,
                            "source enum record pattern needs named payload proof",
                        ));
                    }
                };
                if fields.len() != arguments.len() {
                    return Err(unsupported(
                        p.span,
                        "source enum pattern payload arity differs",
                    ));
                }
                let owner = self.functions.len() + self.records.len() + index;
                let scope = Scope {
                    owner,
                    parameters: &declaration.generics,
                    host: None,
                };
                let mut map = Substitutions::new();
                for (ordinal, actual) in actuals.iter().enumerate() {
                    self.lookup(map.len(), 2, p.span)?;
                    map.insert(Binder(owner, ordinal), self.copy(actual, p.span)?);
                }
                for (child, field) in arguments.iter().zip(fields) {
                    let formal = self.ty(&field.ty, &scope, 0)?;
                    let actual = self.substitute(&formal, &map, p.span)?;
                    self.bind(child, &actual, env)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn field(&mut self, fact: &Fact, member: &MemberName, span: Span, depth: usize) -> R<Fact> {
        let fact = if let Fact::Address(_, f) = fact {
            f.as_ref()
        } else {
            fact
        };
        if let (Fact::Tuple(fs), MemberName::TupleIndex(index)) = (fact, member) {
            return match fs.get(*index as usize) {
                Some(f) => self.copy(f, span),
                None => Ok(Fact::Unknown),
            };
        }
        let (Fact::Named(Head::Declaration(index), actuals), MemberName::Name(name)) =
            (fact, member)
        else {
            return Ok(Fact::Unknown);
        };
        self.pay(span, 1)?;
        let declaration = self.records[*index];
        let scope = Scope {
            owner: self.functions.len() + index,
            parameters: &declaration.generics,
            host: None,
        };
        let mut map = Substitutions::new();
        for (ordinal, actual) in actuals.iter().enumerate() {
            self.lookup(map.len(), 2, span)?;
            map.insert(Binder(scope.owner, ordinal), self.copy(actual, span)?);
        }
        for field in &declaration.fields {
            self.budget.text(name, span)?;
            self.budget.text(&field.name, span)?;
            if &field.name == name {
                let formal = self.ty(&field.ty, &scope, depth + 1)?;
                return self.substitute(&formal, &map, span);
            }
        }
        Ok(Fact::Unknown)
    }
    fn expr(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        self.budget.node(e.span, depth)?;
        let actual = self.expr_value(e, scope, env, wanted, depth)?;
        if let Some(wanted) = wanted {
            self.compatible(wanted, &actual, e.span)?;
        }
        Ok(actual)
    }
    #[inline(never)]
    fn expr_value(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        match &e.kind {
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) | ExprKind::MetaSplice(inner) => {
                self.expr(inner, scope, env, wanted, depth + 1)
            }
            ExprKind::Ascribed { .. } => self.expr_ascribed(e, scope, env, wanted, depth),
            ExprKind::Record { .. } => self.expr_record(e, scope, env, wanted, depth),
            ExprKind::Array(_) => self.expr_array(e, scope, env, wanted, depth),
            ExprKind::Tuple(_) => self.expr_tuple(e, scope, env, wanted, depth),
            ExprKind::Index { .. } => self.expr_index(e, scope, env, wanted, depth),
            ExprKind::Slice { .. } => self.expr_slice(e, scope, env, wanted, depth),
            ExprKind::Try(_) => self.expr_try(e, scope, env, wanted, depth),
            ExprKind::Pipeline { .. } => self.expr_pipeline(e, scope, env, wanted, depth),
            ExprKind::MetaBlock(_) => self.expr_metablock(e, scope, env, wanted, depth),
            ExprKind::Member { .. } => self.expr_member(e, scope, env, wanted, depth),
            ExprKind::Unary { .. } => self.expr_unary(e, scope, env, wanted, depth),
            ExprKind::Call { .. } => self.expr_call(e, scope, env, wanted, depth),
            ExprKind::TypeApply { .. } => self.expr_typeapply(e, scope, env, wanted, depth),
            ExprKind::If { .. } => self.expr_if(e, scope, env, wanted, depth),
            ExprKind::Binary { .. } => self.expr_binary(e, scope, env, wanted, depth),
            ExprKind::Closure { .. } => self.expr_closure(e, scope, env, wanted, depth),
            _ => self.expr_leaf(e, env),
        }
    }

    #[inline(never)]
    fn expr_leaf(&mut self, e: &Expr, env: &mut Facts) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Name(name) => {
                self.budget.text(name, e.span)?;
                self.lookup(env.len(), name.len(), e.span)?;
                match env.get(name) {
                    Some(f) => self.copy(f, e.span)?,
                    None => Fact::Unknown,
                }
            }
            _ => Fact::Unknown,
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_binary(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let ExprKind::Binary { left, right, .. } = &e.kind else {
            unreachable!("source fact binary dispatcher")
        };
        self.expr(left, scope, env, None, depth + 1)?;
        self.expr(right, scope, env, None, depth + 1)?;
        Ok(Fact::Unknown)
    }

    #[inline(never)]
    fn expr_ascribed(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Ascribed { ty, value } => {
                let annotation = self.ty(ty, scope, depth + 1)?;
                self.expr(value, scope, env, Some(&annotation), depth + 1)?;
                annotation
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_record(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Record { ty, base, fields } => {
                let explicit = match ty.as_deref().map(|t| &t.kind) {
                    Some(ExprKind::TypeApply { callee, arguments }) => match &callee.kind {
                        ExprKind::Name(name) => {
                            self.budget.text(name, e.span)?;
                            let mut args = Vec::new();
                            for a in arguments {
                                args.push(match a {
                                    GenericArg::Lifetime(name) => {
                                        match self.binder(scope, name, e.span)? {
                                            Some((b, true)) => Fact::Class(b),
                                            _ => Fact::Unknown,
                                        }
                                    }
                                    GenericArg::Type(t) => self.ty(t, scope, depth + 1)?,
                                    _ => Fact::Unknown,
                                });
                            }
                            let qualified = args.iter().any(Fact::has_class);
                            Fact::Named(
                                self.head(std::slice::from_ref(name), e.span, qualified)?,
                                args,
                            )
                        }
                        _ => Fact::Unknown,
                    },
                    Some(ExprKind::Name(name)) => {
                        self.budget.text(name, e.span)?;
                        if name == "Self" {
                            self.self_type(scope, e.span)?.unwrap_or(Fact::Unknown)
                        } else {
                            Fact::Named(
                                self.head(std::slice::from_ref(name), e.span, false)?,
                                vec![],
                            )
                        }
                    }
                    _ => Fact::Unknown,
                };
                let record = if let Some(base) = base {
                    self.expr(base, scope, env, Some(&explicit), depth + 1)?
                } else {
                    explicit
                };
                for field in fields {
                    if let Some(value) = &field.value {
                        self.budget.text(&field.name, field.span)?;
                        let hint = self.field(
                            &record,
                            &MemberName::Name(field.name.clone()),
                            e.span,
                            depth + 1,
                        )?;
                        self.expr(value, scope, env, Some(&hint), depth + 1)?;
                    }
                }
                record
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_array(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Array(values) => {
                let mut element = None;
                for value in values {
                    let f = self.expr(value, scope, env, element.as_ref(), depth + 1)?;
                    if element.is_none() {
                        element = Some(f);
                    }
                }
                Fact::List(Box::new(element.unwrap_or(Fact::Unknown)))
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_tuple(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Tuple(values) => {
                let mut fs = Vec::new();
                for (i, value) in values.iter().enumerate() {
                    let hint = if let Some(Fact::Tuple(fs)) = wanted {
                        fs.get(i)
                    } else {
                        None
                    };
                    fs.push(self.expr(value, scope, env, hint, depth + 1)?);
                }
                Fact::Tuple(fs)
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_index(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Index { object, index } => {
                let array = self.expr(object, scope, env, None, depth + 1)?;
                self.expr(index, scope, env, None, depth + 1)?;
                if let Fact::List(inner) = array {
                    *inner
                } else {
                    Fact::Unknown
                }
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_slice(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Slice { object, start, end } => {
                let array = self.expr(object, scope, env, None, depth + 1)?;
                for bound in [start, end].into_iter().flatten() {
                    self.expr(bound, scope, env, None, depth + 1)?;
                }
                array
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_try(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Try(inner) => {
                self.expr(inner, scope, env, None, depth + 1)?;
                Fact::Unknown
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_pipeline(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Pipeline { input, target } => {
                self.expr(input, scope, env, None, depth + 1)?;
                self.expr(target, scope, env, None, depth + 1)?;
                Fact::Unknown
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_metablock(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::MetaBlock(block) => {
                self.budget.node(block.span, depth + 1)?;
                let mut local = self.environment(env, block.span)?;
                let mut output = None;
                for statement in &block.statements {
                    if let StmtKind::Yield(value) = &statement.kind {
                        let f = self.expr(
                            value,
                            scope,
                            &mut local,
                            wanted.or(output.as_ref()),
                            depth + 2,
                        )?;
                        if output.is_none() {
                            output = Some(f);
                        }
                    } else {
                        self.statement(statement, scope, &mut local, None, depth + 2)?;
                    }
                }
                output.unwrap_or(Fact::Unknown)
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_member(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Member { object, member } => {
                let object = self.expr(object, scope, env, None, depth + 1)?;
                self.field(&object, member, e.span, depth + 1)?
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_unary(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Unary { op, operand } => {
                let source = self.expr(operand, scope, env, None, depth + 1)?;
                match op {
                    UnaryOp::Borrow | UnaryOp::BorrowMut => {
                        if let ExprKind::Unary {
                            op: UnaryOp::Dereference,
                            operand: parent,
                        } = &operand.kind
                        {
                            self.expr(parent, scope, env, None, depth + 1)?
                        } else {
                            Fact::Address(None, Box::new(source))
                        }
                    }
                    UnaryOp::Dereference => {
                        if let Fact::Address(_, f) = source {
                            *f
                        } else {
                            Fact::Unknown
                        }
                    }
                    _ => Fact::Unknown,
                }
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_call(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Call { callee, arguments } => {
                self.expr(callee, scope, env, None, depth + 1)?;
                let mut actuals = Vec::new();
                for arg in arguments {
                    actuals.push(self.expr(&arg.value, scope, env, None, depth + 1)?);
                }
                let (root, explicit) = match &callee.kind {
                    ExprKind::Name(name) => (Some(name.as_str()), None),
                    ExprKind::TypeApply { callee, arguments } => (
                        if let ExprKind::Name(name) = &callee.kind {
                            Some(name.as_str())
                        } else {
                            None
                        },
                        Some(arguments),
                    ),
                    _ => (None, None),
                };
                let mut output = Fact::Unknown;
                if root.is_none()
                    && let Some(variant) = self.variant_call_result(
                        callee,
                        &actuals,
                        wanted,
                        scope,
                        e.span,
                        depth + 1,
                    )?
                {
                    return Ok(variant);
                }
                if root.is_none()
                    && let Some(member) =
                        self.member_call_result(callee, arguments.len(), scope, e.span, depth + 1)?
                {
                    return Ok(member);
                }
                if let Some(name) = root {
                    self.lookup(env.len(), name.len(), e.span)?;
                }
                if let Some(name) = root.filter(|name| !env.contains_key(*name)) {
                    for owner in 0..self.functions.len() {
                        self.pay(e.span, 1)?;
                        if !self.standalone[owner] {
                            continue;
                        }
                        let f = self.functions[owner];
                        self.budget.text(name, e.span)?;
                        self.budget.text(&f.name, e.span)?;
                        if f.name != name {
                            continue;
                        }
                        let definition = Scope {
                            owner,
                            parameters: &f.generics,
                            host: self.hosts[owner],
                        };
                        let mut map = Substitutions::new();
                        if let Some(args) = explicit {
                            if f.generics
                                .iter()
                                .any(|p| !matches!(p.kind, GenericParamKind::Type(_)))
                                && args.len() == f.generics.len()
                            {
                                for (ordinal, (formal, arg)) in
                                    f.generics.iter().zip(args).enumerate()
                                {
                                    let fact = self.full_free_actual(
                                        formal,
                                        arg,
                                        scope,
                                        e.span,
                                        depth + 1,
                                    )?;
                                    self.lookup(map.len(), 2, e.span)?;
                                    map.insert(Binder(owner, ordinal), fact);
                                }
                            } else {
                                // Keep the old short and all-Type path's work
                                // and ordinal behavior unchanged.
                                for ((ordinal, _), arg) in f
                                    .generics
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, p)| !matches!(p.kind, GenericParamKind::Lifetime))
                                    .zip(args)
                                {
                                    if let GenericArg::Type(t) = arg {
                                        let fact = self.ty(t, scope, depth + 1)?;
                                        self.lookup(map.len(), 2, e.span)?;
                                        map.insert(Binder(owner, ordinal), fact);
                                    }
                                }
                            }
                        }
                        let result = f
                            .return_type
                            .as_ref()
                            .map(|t| self.ty(t, &definition, depth + 1))
                            .transpose()?;
                        let mut formals = Vec::new();
                        let mut invariant = false;
                        for p in &f.params {
                            let f = self.ty(&p.ty, &definition, depth + 1)?;
                            self.work(&f, e.span)?;
                            invariant |= f.has_class();
                            formals.push(f);
                        }
                        for (formal, actual) in formals.iter().zip(&actuals) {
                            self.infer(formal, actual, &mut map, e.span, invariant)?;
                        }
                        if let Some(result) = result {
                            output = self.substitute(&result, &map, e.span)?;
                        }
                        break;
                    }
                }
                output
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    fn variant_call_result(
        &mut self,
        callee: &Expr,
        actuals: &[Fact],
        wanted: Option<&Fact>,
        caller: &Scope<'_>,
        span: Span,
        depth: usize,
    ) -> R<Option<Fact>> {
        let ExprKind::Member {
            object,
            member: MemberName::Name(variant_name),
        } = &callee.kind
        else {
            return Ok(None);
        };
        let (object, explicit) = match &object.kind {
            ExprKind::TypeApply { callee, arguments } => {
                (callee.as_ref(), Some(arguments.as_slice()))
            }
            _ => (object.as_ref(), None),
        };
        let ExprKind::Name(enum_name) = &object.kind else {
            return Ok(None);
        };
        for index in 0..self.enums.len() {
            let declaration = self.enums[index];
            self.budget.text(enum_name, span)?;
            self.budget.text(&declaration.name, span)?;
            if declaration.name != *enum_name {
                continue;
            }
            for variant in &declaration.variants {
                self.budget.text(variant_name, span)?;
                self.budget.text(&variant.name, span)?;
                if variant.name != *variant_name {
                    continue;
                }
                let fields: &[VariantTupleField] = match &variant.fields {
                    VariantFields::Unit => &[],
                    VariantFields::Tuple(fields) => fields,
                    VariantFields::Record(_) => {
                        return Err(unsupported(
                            span,
                            "scoped source enum record constructor needs named payload evidence",
                        ));
                    }
                };
                if fields.len() != actuals.len() {
                    return Err(unsupported(
                        span,
                        "source enum variant payload arity differs",
                    ));
                }
                let owner = self.functions.len() + self.records.len() + index;
                let definition = Scope {
                    owner,
                    parameters: &declaration.generics,
                    host: None,
                };
                let mut map = Substitutions::new();
                if let Some(explicit) = explicit {
                    if explicit.len() != declaration.generics.len() {
                        return Err(unsupported(
                            span,
                            "source enum constructor needs full ordered actuals",
                        ));
                    }
                    for (ordinal, (formal, actual)) in
                        declaration.generics.iter().zip(explicit).enumerate()
                    {
                        let fact =
                            self.full_free_actual(formal, actual, caller, span, depth + 1)?;
                        self.lookup(map.len(), 2, span)?;
                        map.insert(Binder(owner, ordinal), fact);
                    }
                } else if let Some(Fact::Named(Head::Enum(expected), classes)) = wanted
                    && *expected == index
                    && classes.len() == declaration.generics.len()
                {
                    for (ordinal, class) in classes.iter().enumerate() {
                        self.lookup(map.len(), 2, span)?;
                        map.insert(Binder(owner, ordinal), self.copy(class, span)?);
                    }
                }
                for (field, actual) in fields.iter().zip(actuals) {
                    let formal = self.ty(&field.ty, &definition, depth + 1)?;
                    if !map.is_empty() {
                        let expected = self.substitute(&formal, &map, span)?;
                        self.compatible(&expected, actual, span)?;
                    } else {
                        self.infer(&formal, actual, &mut map, span, true)?;
                    }
                }
                let mut classes = Vec::new();
                for ordinal in 0..declaration.generics.len() {
                    self.lookup(map.len(), 2, span)?;
                    let Some(fact) = map.get(&Binder(owner, ordinal)) else {
                        return Err(unsupported(
                            span,
                            "source enum variant cannot infer its complete actuals",
                        ));
                    };
                    classes.push(self.copy(fact, span)?);
                }
                return Ok(Some(Fact::Named(Head::Enum(index), classes)));
            }
        }
        Ok(None)
    }

    fn full_free_actual(
        &mut self,
        formal: &GenericParam,
        actual: &GenericArg,
        scope: &Scope<'_>,
        span: Span,
        depth: usize,
    ) -> R<Fact> {
        self.budget.node(span, depth)?;
        match (&formal.kind, actual) {
            (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                match self.binder(scope, name, span)? {
                    Some((binder, true)) => Ok(Fact::Class(binder)),
                    _ => Err(unsupported(
                        span,
                        "free CT life actual has no original source binder",
                    )),
                }
            }
            (GenericParamKind::Type(_), GenericArg::Type(ty)) => self.ty(ty, scope, depth + 1),
            (GenericParamKind::Const(domain), arg) => {
                let carrier = consts::argument(arg, Some(formal), self.budget)?;
                let value = consts::expression(&carrier).ok_or_else(|| {
                    unsupported(span, "free CT const actual has no source expression")
                })?;
                if let ExprKind::Name(name) = &value.kind {
                    let binder = scope
                        .parameters
                        .iter()
                        .find(|p| p.name == *name && matches!(p.kind, GenericParamKind::Const(_)));
                    let binder = binder.or_else(|| {
                        consts::symbolic_parameter(self.functions[scope.owner], name)
                            .map(|(_, p)| p)
                    });
                    let Some(GenericParam {
                        kind: GenericParamKind::Const(actual_domain),
                        ..
                    }) = binder
                    else {
                        return Err(unsupported(
                            span,
                            "free CT symbolic const has no original source binder",
                        ));
                    };
                    if type_key(domain, self.budget)? != type_key(actual_domain, self.budget)? {
                        return Err(unsupported(span, "free CT symbolic const domain differs"));
                    }
                } else {
                    consts::normalize(formal, &carrier, self.budget)?;
                }
                Ok(Fact::Unknown)
            }
            _ => Err(unsupported(
                span,
                "free CT actual kind differs from full ordinal",
            )),
        }
    }

    // Resolve a CT host/member call from the actual source AST and inventory.
    // The host and member own separate full ordinal domains, including when
    // their lifetime parameters have the same spelling.
    fn member_call_result(
        &mut self,
        callee: &Expr,
        value_arity: usize,
        caller: &Scope<'_>,
        span: Span,
        depth: usize,
    ) -> R<Option<Fact>> {
        let (member, member_args) = match &callee.kind {
            ExprKind::TypeApply { callee, arguments } => {
                (callee.as_ref(), Some(arguments.as_slice()))
            }
            _ => (callee, None),
        };
        let ExprKind::Member {
            object,
            member: MemberName::Name(method),
        } = &member.kind
        else {
            return Ok(None);
        };
        let (host, host_args) = match &object.kind {
            ExprKind::TypeApply { callee, arguments } => {
                (callee.as_ref(), Some(arguments.as_slice()))
            }
            _ => (object.as_ref(), None),
        };
        let ExprKind::Name(host_name) = &host.kind else {
            return Ok(None);
        };
        for host_index in 0..self.records.len() {
            let declaration = self.records[host_index];
            self.budget.text(host_name, span)?;
            self.budget.text(&declaration.name, span)?;
            if declaration.name != *host_name {
                continue;
            }
            for owner in 0..self.functions.len() {
                self.pay(span, 1)?;
                if self.hosts[owner] != Some(host_index) {
                    continue;
                }
                let function = self.functions[owner];
                self.budget.text(method, span)?;
                self.budget.text(&function.name, span)?;
                if function.name != *method {
                    continue;
                }
                if function.params.len() != value_arity {
                    return Err(unsupported(
                        span,
                        "source member value argument arity differs",
                    ));
                }
                let Some(host_args) = host_args else {
                    return Ok(None);
                };
                let Some(member_args) = member_args else {
                    return Ok(None);
                };
                if host_args.len() != declaration.generics.len()
                    || member_args.len() != function.generics.len()
                {
                    return Err(unsupported(
                        span,
                        "source host/member needs its full ordered actuals",
                    ));
                }
                let mut map = Substitutions::new();
                for (ordinal, (formal, actual)) in
                    declaration.generics.iter().zip(host_args).enumerate()
                {
                    let fact = self.member_actual(formal, actual, caller, span, depth + 1)?;
                    self.lookup(map.len(), 2, span)?;
                    map.insert(Binder(self.functions.len() + host_index, ordinal), fact);
                }
                for (ordinal, (formal, actual)) in
                    function.generics.iter().zip(member_args).enumerate()
                {
                    let fact = self.member_actual(formal, actual, caller, span, depth + 1)?;
                    self.lookup(map.len(), 2, span)?;
                    map.insert(Binder(owner, ordinal), fact);
                }
                let definition = Scope {
                    owner,
                    parameters: &function.generics,
                    host: Some(host_index),
                };
                let Some(result) = &function.return_type else {
                    return Ok(Some(Fact::Unknown));
                };
                let formal = self.ty(result, &definition, depth + 1)?;
                return self.substitute(&formal, &map, span).map(Some);
            }
        }
        Ok(None)
    }

    fn member_actual(
        &mut self,
        formal: &GenericParam,
        actual: &GenericArg,
        caller: &Scope<'_>,
        span: Span,
        depth: usize,
    ) -> R<Fact> {
        self.budget.node(span, depth)?;
        match (&formal.kind, actual) {
            (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                match self.binder(caller, name, span)? {
                    Some((binder, true)) => Ok(Fact::Class(binder)),
                    _ => Err(unsupported(
                        span,
                        "source member life actual has no caller binder",
                    )),
                }
            }
            (GenericParamKind::Type(_), GenericArg::Type(ty)) => self.ty(ty, caller, depth + 1),
            (GenericParamKind::Const(domain), actual) => {
                let wanted = consts::domain(domain)?;
                // A written identifier is resolved in the actual caller's
                // member-first, host-second lexical binder scope. A Type
                // binder or same-spelled global is never a scalar actual.
                let name = match actual {
                    GenericArg::Const(Expr {
                        kind: ExprKind::Name(name),
                        ..
                    }) => Some(name.as_str()),
                    GenericArg::Type(ty) => trait_bounds::plain(ty),
                    _ => None,
                };
                if let Some(name) = name {
                    let Some((binder, false)) = self.binder(caller, name, span)? else {
                        return Err(unsupported(
                            span,
                            "source member Const actual has no actual caller binder",
                        ));
                    };
                    let parameter = if binder.0 == caller.owner {
                        caller.parameters.get(binder.1)
                    } else {
                        caller
                            .host
                            .and_then(|host| self.records[host].generics.get(binder.1))
                    }
                    .ok_or_else(|| error(span, "source member Const binder ordinal absent"))?;
                    let GenericParamKind::Const(source_domain) = &parameter.kind else {
                        return Err(unsupported(
                            span,
                            "source member Const actual names another binder kind",
                        ));
                    };
                    if consts::domain(source_domain)? != wanted {
                        return Err(error(
                            span,
                            "source member Const actual domain differs from original formal",
                        ));
                    }
                    return Ok(Fact::Parameter(binder));
                }
                let carrier = consts::argument(actual, Some(formal), self.budget)?;
                consts::normalize(formal, &carrier, self.budget)?;
                // This pass tracks life/class relations only. No Const value
                // identity or namespace permission follows from Unknown; CT's
                // source actual checker still compares the complete value.
                Ok(Fact::Unknown)
            }
            _ => Err(unsupported(
                span,
                "source member actual kind differs from full ordinal",
            )),
        }
    }

    #[inline(never)]
    fn expr_typeapply(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee, scope, env, None, depth + 1)?;
                for argument in arguments {
                    if let GenericArg::Type(ty) = argument {
                        self.ty(ty, scope, depth + 1)?;
                    }
                }
                Fact::Unknown
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_if(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, scope, env, None, depth + 1)?;
                let a = self.expr(then_value, scope, env, wanted, depth + 1)?;
                self.expr(else_value, scope, env, wanted.or(Some(&a)), depth + 1)?;
                a
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    #[inline(never)]
    fn expr_closure(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        _wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        let actual = match &e.kind {
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let mut child = self.environment(env, e.span)?;
                for p in params {
                    let f =
                        p.ty.as_ref()
                            .map(|t| self.ty(t, scope, depth + 1))
                            .transpose()?
                            .unwrap_or(Fact::Unknown);
                    self.budget.text(&p.name, p.span)?;
                    self.lookup(child.len(), p.name.len(), p.span)?;
                    child.insert(p.name.clone(), f);
                }
                let output = return_type
                    .as_ref()
                    .map(|t| self.ty(t, scope, depth + 1))
                    .transpose()?;
                self.arrow(body, scope, &mut child, output.as_ref(), depth + 1)?;
                Fact::Unknown
            }
            _ => unreachable!("source fact expression dispatcher"),
        };
        Ok(actual)
    }

    fn arrow(
        &mut self,
        body: &ArrowBody,
        scope: &Scope<'_>,
        env: &mut Facts,
        result: Option<&Fact>,
        depth: usize,
    ) -> R<()> {
        match body {
            ArrowBody::Block(b) => self.block(b, scope, env, result, depth),
            ArrowBody::Expression(e) => self.expr(e, scope, env, result, depth).map(|_| ()),
        }
    }
    fn block(
        &mut self,
        block: &Block,
        scope: &Scope<'_>,
        env: &mut Facts,
        result: Option<&Fact>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(block.span, depth)?;
        let mut local = self.environment(env, block.span)?;
        for statement in &block.statements {
            self.statement(statement, scope, &mut local, result, depth + 1)?;
        }
        Ok(())
    }
    fn condition(
        &mut self,
        condition: &Condition,
        scope: &Scope<'_>,
        env: &mut Facts,
        depth: usize,
    ) -> R<()> {
        match condition {
            Condition::Expression(e) => {
                self.expr(e, scope, env, None, depth)?;
            }
            Condition::Let { pattern, value } => {
                let f = self.expr(value, scope, env, None, depth)?;
                self.bind(pattern, &f, env)?;
            }
        }
        Ok(())
    }
    fn statement(
        &mut self,
        s: &Stmt,
        scope: &Scope<'_>,
        env: &mut Facts,
        result: Option<&Fact>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(s.span, depth)?;
        match &s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| self.ty(t, scope, depth + 1))
                    .transpose()?;
                let actual = self.expr(value, scope, env, annotation.as_ref(), depth + 1)?;
                self.bind(pattern, annotation.as_ref().unwrap_or(&actual), env)?;
            }
            StmtKind::Assign { target, value, .. } => {
                let target = self.expr(target, scope, env, None, depth + 1)?;
                self.expr(value, scope, env, Some(&target), depth + 1)?;
            }
            StmtKind::Return(Some(e)) => {
                self.expr(e, scope, env, result, depth + 1)?;
            }
            StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, scope, env, None, depth + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut child = self.environment(env, s.span)?;
                self.condition(condition, scope, &mut child, depth + 1)?;
                self.block(then_body, scope, &mut child, result, depth + 1)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, scope, env, result, depth + 1)?,
                    Some(ElseBranch::If(s)) => self.statement(s, scope, env, result, depth + 1)?,
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                let mut child = self.environment(env, s.span)?;
                self.condition(condition, scope, &mut child, depth + 1)?;
                self.block(body, scope, &mut child, result, depth + 1)?;
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => {
                self.block(b, scope, env, result, depth + 1)?
            }
            StmtKind::For {
                iterable,
                body,
                pattern,
            } => {
                let f = self.expr(iterable, scope, env, None, depth + 1)?;
                let mut child = self.environment(env, s.span)?;
                self.bind(pattern, &f, &mut child)?;
                self.block(body, scope, &mut child, result, depth + 1)?;
            }
            StmtKind::Match { value, arms } => {
                let f = self.expr(value, scope, env, None, depth + 1)?;
                for arm in arms {
                    let mut child = self.environment(env, s.span)?;
                    self.bind(&arm.pattern, &f, &mut child)?;
                    self.arrow(&arm.body, scope, &mut child, result, depth + 1)?;
                }
            }
            StmtKind::With { value, body, .. } => {
                self.expr(value, scope, env, None, depth + 1)?;
                self.block(body, scope, env, result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn validate(module: &Module, budget: &mut Budget<'_>) -> R<()> {
    if !budget.has_stored_lifetimes() {
        return Ok(());
    }
    struct Inventory<'a> {
        functions: Vec<&'a Function>,
        records: Vec<&'a Aggregate>,
        enums: Vec<&'a EnumDecl>,
        hosts: Vec<Option<usize>>,
        standalone: Vec<bool>,
    }
    fn inventory<'a>(
        items: &'a [Item],
        found: &mut Inventory<'a>,
        host: Option<usize>,
        top: bool,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        for item in items {
            budget.node(item.span, depth)?;
            match &item.kind {
                ItemKind::Function(f) => {
                    budget.identity_work(3, item.span)?;
                    found.functions.push(f);
                    found.hosts.push(host);
                    found.standalone.push(top);
                }
                ItemKind::Aggregate(a) => {
                    let index = found.records.len();
                    found.records.push(a);
                    inventory(&a.methods, found, Some(index), false, budget, depth + 1)?;
                }
                ItemKind::Enum(e) => {
                    budget.lineage_bytes(std::mem::size_of::<&EnumDecl>(), item.span)?;
                    found.enums.push(e);
                }
                ItemKind::Impl(i) => {
                    budget.identity_work(i.generics.len(), item.span)?;
                    if i.generics
                        .iter()
                        .any(|p| matches!(p.kind, GenericParamKind::Lifetime))
                    {
                        return Err(unsupported(
                            item.span,
                            "ambient generic impl requires its actual enclosing source context",
                        ));
                    }
                    inventory(&i.members, found, host, false, budget, depth + 1)?;
                }
                ItemKind::Trait(t) => {
                    budget.identity_work(t.generics.len(), item.span)?;
                    if t.generics
                        .iter()
                        .any(|p| matches!(p.kind, GenericParamKind::Lifetime))
                    {
                        return Err(unsupported(
                            item.span,
                            "ambient trait requires its actual enclosing source context",
                        ));
                    }
                    inventory(&t.members, found, host, false, budget, depth + 1)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut found = Inventory {
        functions: Vec::new(),
        records: Vec::new(),
        enums: Vec::new(),
        hosts: Vec::new(),
        standalone: Vec::new(),
    };
    inventory(&module.items, &mut found, None, true, budget, 0)?;
    let mut check = Check {
        functions: found.functions,
        records: found.records,
        enums: found.enums,
        hosts: found.hosts,
        standalone: found.standalone,
        fact_depth: 0,
        budget,
    };
    for owner in 0..check.functions.len() {
        let f = check.functions[owner];
        check.pay(f.span, f.generics.len())?;
        let inherited = if let Some(h) = check.hosts[owner] {
            check.pay(f.span, check.records[h].generics.len())?;
            check.records[h]
                .generics
                .iter()
                .any(|p| matches!(p.kind, GenericParamKind::Lifetime))
        } else {
            false
        };
        if !inherited
            && !f
                .generics
                .iter()
                .any(|p| matches!(p.kind, GenericParamKind::Lifetime))
        {
            continue;
        }
        let scope = Scope {
            owner,
            parameters: &f.generics,
            host: check.hosts[owner],
        };
        let mut env = Facts::new();
        for p in &f.params {
            let fact = check.ty(&p.ty, &scope, 0)?;
            check.budget.text(&p.name, p.span)?;
            check.lookup(env.len(), p.name.len(), p.span)?;
            env.insert(p.name.clone(), fact);
        }
        let result = f
            .return_type
            .as_ref()
            .map(|t| check.ty(t, &scope, 0))
            .transpose()?;
        match &f.body {
            FunctionBody::Block(body) => check.block(body, &scope, &mut env, result.as_ref(), 0)?,
            FunctionBody::Expression(e) => {
                check.expr(e, &scope, &mut env, result.as_ref(), 0)?;
            }
            FunctionBody::Declaration => {}
        }
    }
    Ok(())
}
