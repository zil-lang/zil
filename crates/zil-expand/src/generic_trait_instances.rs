//! Source instances, never trait proofs. Concrete conformance still reaches
//! the ordinary static collector, Types, Analyze and independent Verify.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub declaration: Span,
    pub name: String,
    pub arguments: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub key: Key,
    pub arguments: Vec<TypeRef>,
}
#[derive(Clone, Debug)]
pub(crate) enum Bound {
    Copy,
    StoreElement,
    Nominal(Reference),
}
impl PartialEq<str> for Bound {
    fn eq(&self, other: &str) -> bool {
        matches!(
            (self, other),
            (Self::Copy, "Copy") | (Self::StoreElement, "StoreElement")
        )
    }
}
impl PartialEq<&str> for Bound {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}
pub(crate) struct CallTarget {
    pub reference: Reference,
    pub member: String,
}
pub(crate) type CallTargets = BTreeMap<usize, CallTarget>;
pub(crate) type Calls = BTreeMap<String, CallTargets>;
pub(crate) fn remaining(budget: &Budget<'_>) -> Limits {
    budget.remaining_limits()
}

pub(crate) struct ConcreteSignature {
    pub key: Key,
    pub host: String,
    pub method: Function,
}
pub(crate) type Signatures = BTreeMap<(Key, String, String), ConcreteSignature>;
pub(crate) fn copy_key(key: &Key, budget: &mut Budget<'_>, span: Span) -> R<Key> {
    budget.text(&key.name, span)?;
    for arg in &key.arguments {
        budget.text(arg, span)?;
        budget.node(span, 0)?;
    }
    Ok(key.clone())
}
pub(crate) fn head(ty: &TypeRef) -> Option<&str> {
    match &grouped(ty).kind {
        TypeKind::Named { path, .. } if path.len() == 1 => Some(&path[0]),
        _ => None,
    }
}
pub(crate) fn arguments(
    declaration_span: Span,
    declaration: &TraitDecl,
    application: &TypeRef,
    budget: &mut Budget<'_>,
) -> R<Reference> {
    budget.ty(application)?;
    let TypeKind::Named { args, .. } = &grouped(application).kind else {
        return Err(unsupported(
            application.span,
            "trait application needs a nominal declaration",
        ));
    };
    if args.len() != declaration.generics.len() {
        return Err(error(
            application.span,
            "trait type argument arity differs from its declaration",
        ));
    }
    let mut keys = Vec::new();
    let mut arguments = Vec::new();
    for arg in args {
        let GenericArg::Type(actual) = arg else {
            return Err(unsupported(
                application.span,
                "trait const arguments require their solver",
            ));
        };
        budget.node(actual.span, 0)?;
        keys.push(type_key(actual, budget)?);
        budget.ty(actual)?;
        arguments.push(actual.clone());
    }
    budget.text(&declaration.name, application.span)?;
    Ok(Reference {
        key: Key {
            declaration: declaration_span,
            name: declaration.name.clone(),
            arguments: keys,
        },
        arguments,
    })
}
pub(crate) fn method_name(
    reference: &Reference,
    method: &str,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<String> {
    if reference.arguments.is_empty() {
        budget.text(method, span)?;
        return Ok(method.to_owned());
    }
    use zil_base::FingerprintBuilder;
    budget.text(&reference.key.name, span)?;
    let mut hash = FingerprintBuilder::new();
    hash.update(b"zil/trait-instance-method/v1\0");
    hash.update(reference.key.name.as_bytes());
    hash.update(&(reference.key.declaration.file.0).to_le_bytes());
    hash.update(&(reference.key.declaration.start as u64).to_le_bytes());
    hash.update(&(reference.key.declaration.end as u64).to_le_bytes());
    for key in &reference.key.arguments {
        budget.text(key, span)?;
        hash.update(&(key.len() as u64).to_le_bytes());
        hash.update(key.as_bytes());
    }
    budget.text(method, span)?;
    budget.node(span, 0)?;
    budget.text(
        "$trait_0000000000000000000000000000000000000000000000000000000000000000_",
        span,
    )?;
    budget.text(method, span)?;
    let result = format!("$trait_{}_{}", hash.digest(), method);
    Ok(result)
}
pub(crate) fn signature<'a>(
    declaration: &TraitDecl,
    member: &'a Item,
    reference: &Reference,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Function>> {
    let ItemKind::Function(method) = &member.kind else {
        return Err(error(
            member.span,
            "trait signature is not an ordinary function",
        ));
    };
    if declaration.generics.is_empty() {
        return Ok(Cow::Borrowed(method));
    }
    let mut substitutions = BTreeMap::new();
    for (parameter, actual) in declaration.generics.iter().zip(&reference.arguments) {
        budget.text(&parameter.name, actual.span)?;
        budget.ty(actual)?;
        budget.node(actual.span, 0)?;
        substitutions.insert(parameter.name.clone(), actual.clone());
    }
    budget.substituted_item(member, &substitutions)?;
    let mut result = method.clone();
    let visitor = Substitution {
        substitutions: &substitutions,
    };
    for p in &mut result.params {
        visitor.ty(&mut p.ty);
    }
    if let Some(t) = &mut result.return_type {
        visitor.ty(t);
    }
    Ok(Cow::Owned(result))
}
pub(crate) fn validate_declaration(
    item: &Item,
    declaration: &TraitDecl,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<()> {
    if super::super::static_traits::reserved_trait(&declaration.name) {
        return Err(error(
            item.span,
            "built-in capability traits cannot be redeclared",
        ));
    }
    let mut variables = BTreeSet::from(["Self".to_owned()]);
    for parameter in &declaration.generics {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        if !variables.insert(parameter.name.clone()) {
            return Err(error(parameter.span, "duplicate trait type binder"));
        }
        let GenericParamKind::Type(bounds) = &parameter.kind else {
            return Err(unsupported(
                parameter.span,
                "trait lifetime/const binders require their solver",
            ));
        };
        for bound in bounds {
            budget.ty(bound)?;
            if !copy::is_bound(bound) {
                return Err(unsupported(
                    bound.span,
                    "trait parameter premise currently requires direct Copy",
                ));
            }
        }
    }
    let mut members = BTreeSet::new();
    for member in &declaration.members {
        budget.node(member.span, 0)?;
        let member_name = match &member.kind {
            ItemKind::Function(f) => Some(f.name.as_str()),
            ItemKind::TypeAlias(a) => Some(a.name.as_str()),
            _ => None,
        };
        if let Some(name) = member_name {
            budget.text(name, member.span)?;
            if !members.insert(name) {
                return Err(error(member.span, "duplicate trait member"));
            }
        }
        if let ItemKind::TypeAlias(alias) = &member.kind {
            if !alias.generics.is_empty() || !member.attributes.is_empty() {
                return Err(unsupported(
                    member.span,
                    "generic or attributed associated type",
                ));
            }
            if let Some(value) = &alias.value {
                known_type(value, &variables, nominals, budget, 0)?;
            }
            for bound in &alias.bounds {
                budget.ty(bound)?;
            }
            continue;
        }
        let ItemKind::Function(method) = &member.kind else {
            return Err(unsupported(
                member.span,
                "associated trait items require their solver",
            ));
        };
        if !method.generics.is_empty()
            || !method.where_clause.is_empty()
            || !method.contract.is_empty()
        {
            return Err(unsupported(
                member.span,
                "generic or contract-bearing trait method",
            ));
        }
        for parameter in &method.params {
            known_type(&parameter.ty, &variables, nominals, budget, 0)?;
        }
        if let Some(result) = &method.return_type {
            known_type(result, &variables, nominals, budget, 0)?;
        }
    }
    Ok(())
}
pub(crate) fn known_type(
    ty: &TypeRef,
    variables: &BTreeSet<String>,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    match &ty.kind {
        TypeKind::Named { path, args } => {
            budget.ty(ty)?;
            if let Some((_, host, application)) = trait_bounds::associated::parts(ty) {
                known_type(host, variables, nominals, budget, depth + 1)?;
                let TypeKind::Named { args, .. } = &application.kind else {
                    unreachable!()
                };
                for argument in args {
                    if let GenericArg::Type(argument) = argument {
                        known_type(argument, variables, nominals, budget, depth + 1)?;
                    }
                }
                return Ok(());
            }
            if path.len() != 1 {
                return Err(error(
                    ty.span,
                    "trait signature type is not a resolved nominal identity",
                ));
            }
            let name = &path[0];
            let arity = if variables.contains(name)
                || matches!(
                    name.as_str(),
                    "void"
                        | "bool"
                        | "u8"
                        | "u16"
                        | "i32"
                        | "i64"
                        | "c_handle"
                        | "c_wstr"
                        | "ByteBuffer"
                        | "ForeignHandle"
                        | "CWideString"
                ) {
                0
            } else if name == "Store" {
                1
            } else {
                match nominals.get(name) {
                    Some(Item {
                        kind: ItemKind::Aggregate(a),
                        ..
                    }) => a.generics.len(),
                    Some(Item {
                        kind: ItemKind::Enum(e),
                        ..
                    }) => e.generics.len(),
                    _ => {
                        return Err(error(
                            ty.span,
                            format!("unknown type {name} in trait signature"),
                        ));
                    }
                }
            };
            if arity != args.len() {
                return Err(error(
                    ty.span,
                    "trait signature type argument arity mismatch",
                ));
            }
            for arg in args {
                let GenericArg::Type(t) = arg else {
                    return Err(unsupported(ty.span, "trait signature const argument"));
                };
                known_type(t, variables, nominals, budget, depth + 1)?;
            }
        }
        TypeKind::Reference { inner, .. } | TypeKind::Group(inner) | TypeKind::Optional(inner) => {
            known_type(inner, variables, nominals, budget, depth + 1)?
        }
        TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
            for t in fields {
                known_type(t, variables, nominals, budget, depth + 1)?;
            }
        }
        TypeKind::Array { element, .. } => {
            known_type(element, variables, nominals, budget, depth + 1)?
        }
        TypeKind::Error { error, result } => {
            known_type(error, variables, nominals, budget, depth + 1)?;
            known_type(result, variables, nominals, budget, depth + 1)?;
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for t in params {
                known_type(t, variables, nominals, budget, depth + 1)?;
            }
            known_type(result, variables, nominals, budget, depth + 1)?;
        }
        _ => {
            return Err(unsupported(
                ty.span,
                "trait signature type requires additional resolution",
            ));
        }
    }
    Ok(())
}

/// Expression identities belong to one immutable declaration, in deterministic
/// structural traversal order. Addresses are only a temporary lookup into that
/// declaration; source coordinates never select a dispatch target.
pub(crate) fn expression_ids(
    body: &FunctionBody,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    use super::super::ct_selection::Visitor;
    struct Ordinals<'a, 'l> {
        budget: &'a mut Budget<'l>,
        ids: BTreeMap<usize, usize>,
    }
    impl Visitor for Ordinals<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            self.node(expression.span, depth)?;
            self.ids
                .insert(expression as *const Expr as usize, self.ids.len());
            super::super::ct_selection::walk_expression(self, expression, depth)
        }
    }
    let mut visitor = Ordinals {
        budget,
        ids: BTreeMap::new(),
    };
    match body {
        FunctionBody::Block(block) => visitor.block(block, 0),
        FunctionBody::Expression(expression) => visitor.expression(expression, 0),
        FunctionBody::Declaration => Ok(()),
    }
    .map_err(|mut e| e.remove(0))?;
    Ok(visitor.ids)
}

pub(crate) fn rewrite_calls(
    item: &mut Item,
    symbolic: &CallTargets,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut resolved = BTreeMap::new();
    for (ordinal, target) in symbolic {
        budget.node(item.span, 0)?;
        let mut reference = Reference {
            key: copy_key(&target.reference.key, budget, item.span)?,
            arguments: Vec::new(),
        };
        reference.key.arguments.clear();
        for argument in &target.reference.arguments {
            budget.substituted_type(argument, substitutions)?;
            let mut argument = argument.clone();
            Substitution { substitutions }.ty(&mut argument);
            reference.key.arguments.push(type_key(&argument, budget)?);
            reference.arguments.push(argument);
        }
        let name = method_name(&reference, &target.member, budget, item.span)?;
        resolved.insert(*ordinal, name);
    }
    rewrite_resolved_calls(item, &resolved, budget)
}
fn rewrite_resolved_calls(
    item: &mut Item,
    calls: &BTreeMap<usize, String>,
    budget: &mut Budget<'_>,
) -> R<()> {
    use super::super::ct_selection::Visitor;
    let ids = if calls.is_empty() {
        BTreeMap::new()
    } else if let ItemKind::Function(function) = &item.kind {
        expression_ids(&function.body, budget)?
    } else {
        return Err(error(
            item.span,
            "trait call rewrite requires a function declaration",
        ));
    };
    struct Prepay<'a, 'b, 'l> {
        calls: &'a BTreeMap<usize, String>,
        ids: &'a BTreeMap<usize, usize>,
        budget: &'b mut Budget<'l>,
    }
    impl Visitor for Prepay<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn expression(&mut self, e: &Expr, depth: usize) -> CompileResult<()> {
            if let ExprKind::Member { object, .. } = &e.kind
                && let Some(ordinal) = self.ids.get(&(&**object as *const Expr as usize))
                && let Some(target) = self.calls.get(ordinal)
            {
                self.text(target, e.span)?;
            }
            super::super::ct_selection::walk_expression(self, e, depth)
        }
    }
    Prepay {
        calls,
        ids: &ids,
        budget,
    }
    .item(item, 0)
    .map_err(|mut e| e.remove(0))?;
    let visitor = CallRewrite {
        calls,
        ids: &ids,
        receiver: None,
    };
    if let ItemKind::Function(f) = &mut item.kind {
        match &mut f.body {
            FunctionBody::Block(b) => visitor.block(b),
            FunctionBody::Expression(e) => visitor.expr(e),
            _ => {}
        }
    }
    Ok(())
}
struct CallRewrite<'a> {
    calls: &'a BTreeMap<usize, String>,
    ids: &'a BTreeMap<usize, usize>,
    receiver: Option<&'a str>,
}

pub(crate) fn rename_default_receiver(item: &mut Item, budget: &mut Budget<'_>) -> R<()> {
    use super::super::ct_selection::Visitor;
    const NAME: &str = "$trait_default_self";
    struct Prepay<'a, 'l>(&'a mut Budget<'l>);
    impl Visitor for Prepay<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.0.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.0.text(text, span).map_err(|e| vec![e])
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            if matches!(&expression.kind, ExprKind::Name(name) if name == "self") {
                self.text(NAME, expression.span)?;
            }
            super::super::ct_selection::walk_expression(self, expression, depth)
        }
    }
    Prepay(budget).item(item, 0).map_err(|mut e| e.remove(0))?;
    let empty = BTreeMap::new();
    let visitor = CallRewrite {
        calls: &empty,
        ids: &BTreeMap::new(),
        receiver: Some(NAME),
    };
    if let ItemKind::Function(function) = &mut item.kind {
        for parameter in &mut function.params {
            budget.node(parameter.span, 0)?;
            if parameter.name == "self" {
                budget.text(NAME, parameter.span)?;
                parameter.name = NAME.into();
            }
        }
        match &mut function.body {
            FunctionBody::Block(block) => visitor.block(block),
            FunctionBody::Expression(expression) => visitor.expr(expression),
            FunctionBody::Declaration => {}
        }
    }
    Ok(())
}
impl CallRewrite<'_> {
    fn ty(&self, _: &mut TypeRef) {}
    fn condition(&self, c: &mut Condition) {
        match c {
            Condition::Expression(e) | Condition::Let { value: e, .. } => self.expr(e),
        }
    }
    fn block(&self, b: &mut Block) {
        for s in &mut b.statements {
            self.statement(s);
        }
    }
    fn statement(&self, s: &mut Stmt) {
        match &mut s.kind {
            StmtKind::Binding { ty, value, .. } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
            }
            StmtKind::Return(Some(e)) | StmtKind::Expression(e) | StmtKind::Yield(e) => {
                self.expr(e)
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition);
                self.block(then_body);
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b),
                    Some(ElseBranch::If(s)) => self.statement(s),
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition);
                self.block(body);
            }
            StmtKind::For { iterable, body, .. } => {
                self.expr(iterable);
                self.block(body);
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => self.block(b),
            StmtKind::With { value, body, .. } => {
                self.expr(value);
                self.block(body);
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    self.arrow(&mut arm.body);
                }
            }
            _ => {}
        }
    }
    fn arrow(&self, body: &mut ArrowBody) {
        match body {
            ArrowBody::Block(b) => self.block(b),
            ArrowBody::Expression(e) => self.expr(e),
        }
    }
    fn expr(&self, e: &mut Expr) {
        if let ExprKind::Member {
            object,
            member: MemberName::Name(name) | MemberName::Constrained { name, .. },
        } = &mut e.kind
            && let Some(ordinal) = self.ids.get(&(&**object as *const Expr as usize))
            && let Some(target) = self.calls.get(ordinal)
        {
            name.clone_from(target);
        }

        match &mut e.kind {
            ExprKind::Name(name) if name == "self" && self.receiver.is_some() => {
                *name = self.receiver.unwrap().to_owned();
            }
            ExprKind::Group(x)
            | ExprKind::Try(x)
            | ExprKind::MetaEval(x)
            | ExprKind::Unary { operand: x, .. } => self.expr(x),
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                for x in xs {
                    self.expr(x);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Pipeline { input, target } => {
                self.expr(input);
                self.expr(target);
            }
            ExprKind::Member { object, member } => {
                self.expr(object);
                if let MemberName::Constrained { owner, .. } = member {
                    self.ty(owner);
                }
            }
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee);
                for a in arguments {
                    match a {
                        GenericArg::Type(t) => self.ty(t),
                        GenericArg::Const(e) => self.expr(e),
                    }
                }
            }
            ExprKind::Call { callee, arguments } => {
                self.expr(callee);
                for a in arguments {
                    self.expr(&mut a.value);
                }
            }
            ExprKind::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            ExprKind::Slice { object, start, end } => {
                self.expr(object);
                for x in [start, end].into_iter().flatten() {
                    self.expr(x);
                }
            }
            ExprKind::Record { ty, base, fields } => {
                if let Some(t) = ty {
                    self.expr(t);
                }
                if let Some(b) = base {
                    self.expr(b);
                }
                for f in fields {
                    if let Some(v) = &mut f.value {
                        self.expr(v);
                    }
                }
            }
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                for p in params {
                    if let Some(t) = &mut p.ty {
                        self.ty(t);
                    }
                }
                if let Some(t) = return_type {
                    self.ty(t);
                }
                self.arrow(body);
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition);
                self.expr(then_value);
                self.expr(else_value);
            }
            ExprKind::MetaBlock(b) => self.block(b),
            _ => {}
        }
    }
}

#[cfg(test)]
mod trait_request_ledger {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};

    const SOURCE: &str = "type Number=i32;trait Read<R>{fn read(self:&Self)->R;}struct P{n:Number}impl Read<Number> for P{fn read()->Number{return .n}}fn main()->i32{let p=P{n:7};return p.read()-7}";

    fn resource_limit<T>(result: CompileResult<T>) {
        let errors = match result {
            Ok(_) => panic!("downstream expansion unexpectedly received a fresh allowance"),
            Err(errors) => errors,
        };
        assert!(
            !errors.is_empty(),
            "ResourceLimit must carry an actual diagnostic"
        );
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }

    #[test]
    fn single_source_alias_and_lowering_consume_one_ledger() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "ledger323.zil", SOURCE);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let mut products = products::Products::default();
        super::super::specialize_products(&syntax, &limits, &mut products).unwrap();
        assert!(products.trait_instances);
        let remaining = products.allowance.as_ref().unwrap();
        let specialized_work = limits.max_analysis_steps - remaining.max_analysis_steps;
        let specialized_nodes = limits.max_nodes - remaining.max_nodes;

        // Measure real alias and real engine passes over the same source-derived
        // syntax. The integrated ledger includes their sum, not their maximum.
        let (alias, alias_budget) =
            aliases::normalize_with_budget(&syntax, Budget::new(&limits)).unwrap();
        let alias = alias.unwrap();
        let alias_left = alias_budget.remaining_limits();
        let mut engine_products = products::Products::default();
        super::super::specialize_with_budget(
            &alias.module,
            &limits,
            &alias.pattern_types,
            Budget::new(&limits),
            &mut engine_products,
            false, // This retained source fixture has no associated declarations.
        )
        .unwrap();
        let engine_left = engine_products.allowance.unwrap();
        assert!(
            specialized_work
                >= (limits.max_analysis_steps - alias_left.max_analysis_steps)
                    + (limits.max_analysis_steps - engine_left.max_analysis_steps)
        );
        assert!(
            specialized_nodes
                >= (limits.max_nodes - alias_left.max_nodes)
                    + (limits.max_nodes - engine_left.max_nodes)
        );

        // These exact allowances suffice for specialization itself. They must
        // not buy free downstream static conformance or source-map construction.
        for selected in [
            Limits {
                max_analysis_steps: specialized_work,
                ..limits.clone()
            },
            Limits {
                max_nodes: specialized_nodes,
                ..limits.clone()
            },
        ] {
            let mut exact = products::Products::default();
            super::super::specialize_products(&syntax, &selected, &mut exact).unwrap();
            resource_limit(crate::expand_with_map_limits(&source, &syntax, &selected));
        }
        crate::expand_with_map_limits(&source, &syntax, &limits).unwrap();
        println!(
            "323 single ledger specialization work={specialized_work}, nodes={specialized_nodes}; both exhausted balances reject downstream expansion"
        );
    }

    #[test]
    fn module_normalization_and_lowering_share_the_remainder() {
        let sources = [
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import rules;struct P{n:i32}impl rules.Read<i32> for P{fn read()->i32{return .n}}fn main()->i32{let p=P{n:7};return p.read()-7}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "rules.zil",
                "pub trait Read<R>{fn read(self:&Self)->R;}",
            ),
        ];
        let syntax = sources
            .iter()
            .map(|s| zil_syntax::parse(s).into_result().unwrap())
            .collect::<Vec<_>>();
        let modules = syntax.iter().collect::<Vec<_>>();
        let imports = [
            vec![zil_ir::CoreImport {
                alias: "rules".into(),
                module: ModuleId(1),
                span: syntax[0].items[0].span,
            }],
            Vec::new(),
        ];
        let limits = Limits::default();
        let mut products = products::Products::default();
        crate::modules::specialize(&sources, &modules, &imports, &limits, &mut products).unwrap();
        let remaining = products.allowance.as_ref().unwrap();
        let work = limits.max_analysis_steps - remaining.max_analysis_steps;
        let nodes = limits.max_nodes - remaining.max_nodes;
        for selected in [
            Limits {
                max_analysis_steps: work,
                ..limits.clone()
            },
            Limits {
                max_nodes: nodes,
                ..limits.clone()
            },
        ] {
            let mut exact = products::Products::default();
            crate::modules::specialize(&sources, &modules, &imports, &selected, &mut exact)
                .unwrap();
            resource_limit(crate::expand_program_with_limits(
                &sources, &modules, &imports, &selected,
            ));
        }
        crate::expand_program_with_limits(&sources, &modules, &imports, &limits).unwrap();
        println!(
            "323 project ledger specialization work={work}, nodes={nodes}; both exhausted balances reject downstream expansion"
        );
    }
}
