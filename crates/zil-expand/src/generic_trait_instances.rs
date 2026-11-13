//! Source instances, never trait proofs. Concrete conformance still reaches
//! the ordinary static collector, Types, Analyze and independent Verify.
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct Key {
    pub declaration: Span,
    pub name: String,
    pub arguments: Vec<String>,
}
// `name` is the unique declaration symbol registered by the actual input
// module normalizer, including its module qualification. Source coordinates
// remain diagnostic data and never participate in semantic equality or hashing.
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.arguments == other.arguments
    }
}
impl Eq for Key {}
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Key {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.name, &self.arguments).cmp(&(&other.name, &other.arguments))
    }
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
    Value,
    SyncDrop,
    Nominal(Reference),
}
impl PartialEq for Bound {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Copy, Self::Copy)
            | (Self::StoreElement, Self::StoreElement)
            | (Self::Value, Self::Value)
            | (Self::SyncDrop, Self::SyncDrop) => true,
            (Self::Nominal(a), Self::Nominal(b)) => a.key == b.key,
            _ => false,
        }
    }
}
impl Eq for Bound {}
impl PartialEq<str> for Bound {
    fn eq(&self, other: &str) -> bool {
        matches!(
            (self, other),
            (Self::Copy, "Copy")
                | (Self::StoreElement, "StoreElement")
                | (Self::Value, "Value")
                | (Self::SyncDrop, "SyncDrop")
        )
    }
}
impl PartialEq<&str> for Bound {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}
pub(crate) type PrivateHosts = std::rc::Rc<BTreeMap<String, BTreeSet<String>>>;
#[derive(Clone)]
pub(crate) struct PrivateReceiver {
    pub host: TypeRef,
    pub owners: PrivateHosts,
}
#[derive(Clone)]
pub(crate) struct CallTarget {
    pub reference: Reference,
    pub member: String,
    pub generic: bool,
    pub static_host: bool,
    /// The symbolic receiver and actual private implementation directory come
    /// from checked declarations, never from a generated member spelling.
    pub private_receiver: Option<PrivateReceiver>,
    // A symbolic receiver retained only when this trait has actual scalar
    // impls. Specialization chooses the exact trait-qualified scalar target.
    pub scalar_host: Option<TypeRef>,
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
    method_name_with(reference, method, false, budget, span)
}
pub(crate) fn method_name_with(
    reference: &Reference,
    method: &str,
    generic: bool,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<String> {
    if reference.arguments.is_empty() && !generic {
        budget.text(method, span)?;
        return Ok(method.to_owned());
    }
    use zil_base::FingerprintBuilder;
    budget.text(&reference.key.name, span)?;
    let mut hash = FingerprintBuilder::new();
    hash.update(b"zil/trait-instance-method/v2\0");
    hash.update(&(reference.key.name.len() as u64).to_le_bytes());
    hash.update(reference.key.name.as_bytes());
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
    if method.generics.is_empty() && method.where_clause.is_empty() {
        let visitor = Substitution {
            substitutions: &substitutions,
        };
        for parameter in &mut result.params {
            visitor.ty(&mut parameter.ty);
        }
        if let Some(ty) = &mut result.return_type {
            visitor.ty(ty);
        }
    } else {
        let mut item = Item {
            span: member.span,
            public: false,
            attributes: vec![],
            kind: ItemKind::Function(result),
        };
        substitute_item(&mut item, &substitutions)?;
        let ItemKind::Function(function) = item.kind else {
            unreachable!()
        };
        result = function;
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
        if matches!(parameter.kind, GenericParamKind::Type(_))
            && matches!(parameter.name.as_str(), "Value" | "SyncDrop")
        {
            return Err(error(
                parameter.span,
                "Value and SyncDrop are reserved type-binder names",
            ));
        }
        if !variables.insert(parameter.name.clone()) {
            return Err(error(parameter.span, "duplicate trait type binder"));
        }
        if matches!(parameter.kind, GenericParamKind::Lifetime) {
            continue;
        }
        let GenericParamKind::Type(bounds) = &parameter.kind else {
            return Err(unsupported(
                parameter.span,
                "trait lifetime/const binders require their solver",
            ));
        };
        for bound in bounds {
            budget.ty(bound)?;
            if !copy::is_bound(bound)
                && !stores::is_bound(bound)
                && !trait_bounds::value::is_bound(bound)
            {
                return Err(unsupported(
                    bound.span,
                    "trait parameter premise requires a supported structural capability",
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
            ItemKind::Const(c) => Some(c.name.as_str()),
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
            if let ItemKind::Const(constant) = &member.kind {
                if !member.attributes.is_empty() {
                    return Err(unsupported(member.span, "associated constant attributes"));
                }
                let ty = constant.ty.as_ref().ok_or_else(|| {
                    error(member.span, "associated constant requires a declared type")
                })?;
                known_type(ty, &variables, nominals, budget, 0)?;
                continue;
            }
            return Err(unsupported(
                member.span,
                "associated trait items require their solver",
            ));
        };
        if !method.contract.is_empty() {
            return Err(unsupported(
                member.span,
                "generic or contract-bearing trait method",
            ));
        }
        let mut method_variables = Cow::Borrowed(&variables);
        let mut lending = false;
        let mut life_only = true;
        for parameter in &method.generics {
            budget.node(parameter.span, 0)?;
            budget.text(&parameter.name, parameter.span)?;
            lending |= matches!(parameter.kind, GenericParamKind::Lifetime);
            life_only &= matches!(parameter.kind, GenericParamKind::Lifetime);
            match &parameter.kind {
                GenericParamKind::Type(bounds) => {
                    if matches!(method_variables, Cow::Borrowed(_)) {
                        for name in &variables {
                            budget.text(name, member.span)?;
                        }
                    }
                    if !method_variables.to_mut().insert(parameter.name.clone()) {
                        return Err(error(parameter.span, "duplicate method type binder"));
                    }
                    for bound in bounds {
                        budget.ty(bound)?;
                    }
                }
                GenericParamKind::Const(ty) => {
                    consts::domain(ty)?;
                    budget.ty(ty)?;
                }
                GenericParamKind::Lifetime => {}
            }
        }
        if lending {
            if !declaration.generics.is_empty()
                || !life_only
                || !method.where_clause.is_empty()
                || !matches!(method.body, FunctionBody::Declaration)
            {
                return Err(unsupported(
                    member.span,
                    "lending trait method requires life-only local binders and an ordinary declaration",
                ));
            }
            let receiver = method
                .params
                .first()
                .filter(|p| p.name == "self")
                .ok_or_else(|| {
                    error(
                        member.span,
                        "lending method requires a receiver in parameter zero",
                    )
                })?;
            budget.ty(&receiver.ty)?;
            if receiver.give
                || !matches!(&grouped(&receiver.ty).kind,
                TypeKind::Reference { inner, .. }
                    if matches!(&grouped(inner).kind, TypeKind::Named { path, args } if path.as_slice() == ["Self"] && args.is_empty()))
            {
                return Err(unsupported(
                    receiver.span,
                    "lending method requires an ordinary borrowed Self receiver",
                ));
            }
            let result = method
                .return_type
                .as_ref()
                .ok_or_else(|| error(member.span, "lending method requires a shared result"))?;
            budget.ty(result)?;
            let payload = match &grouped(result).kind {
                TypeKind::Reference { .. } => result,
                TypeKind::Named { path, args }
                    if path.as_slice() == [super::options::TEMPLATE]
                        || path.as_slice() == ["Option"] =>
                {
                    let [GenericArg::Type(payload)] = args.as_slice() else {
                        return Err(unsupported(
                            result.span,
                            "lending Option requires one shared address payload",
                        ));
                    };
                    payload
                }
                _ => {
                    return Err(unsupported(
                        result.span,
                        "lending result requires a shared address or Option of one",
                    ));
                }
            };
            let referent = match &grouped(payload).kind {
                TypeKind::Reference {
                    mutable: false,
                    lifetime: Some(_),
                    inner,
                } => inner,
                _ => {
                    return Err(unsupported(
                        payload.span,
                        "lending result requires an explicitly related shared address",
                    ));
                }
            };
            let direct_item = matches!(&grouped(referent).kind, TypeKind::Named { path, args }
                if path.as_slice() == ["Self", "Item"] && args.is_empty());
            let projected_item = if let Some((member_name, host, application)) =
                trait_bounds::associated::parts(referent)
            {
                budget.text(member_name, referent.span)?;
                budget.ty(host)?;
                budget.ty(application)?;
                budget.text(&declaration.name, referent.span)?;
                member_name == "Item"
                    && trait_bounds::plain(host) == Some("Self")
                    && trait_bounds::plain(application) == Some(declaration.name.as_str())
            } else {
                false
            };
            if direct_item || projected_item {
                let mut declared_item = false;
                for candidate in &declaration.members {
                    budget.node(candidate.span, 0)?;
                    if let ItemKind::TypeAlias(alias) = &candidate.kind {
                        budget.text(&alias.name, candidate.span)?;
                        if alias.name == "Item" {
                            declared_item = true;
                            for bound in &alias.bounds {
                                budget.ty(bound)?;
                            }
                        }
                    }
                }
                if !declared_item {
                    return Err(error(
                        referent.span,
                        "lending associated Item requires a declared associated type",
                    ));
                }
            } else {
                let mut available = trait_bounds::Bounds::new();
                let mut self_copy = false;
                for bound in &declaration.bounds {
                    budget.ty(bound)?;
                    self_copy |= copy::is_bound(bound);
                }
                if self_copy {
                    budget.lineage_bytes(
                        std::mem::size_of::<(String, Vec<trait_bounds::Bound>)>()
                            .saturating_add(std::mem::size_of::<trait_bounds::Bound>())
                            .saturating_add(4),
                        referent.span,
                    )?;
                    budget.text("Self", referent.span)?;
                    available.insert("Self".into(), vec![trait_bounds::Bound::Copy]);
                }
                copy::require(referent, nominals, &available, &variables, budget)?;
            }
        }
        if !method.generics.is_empty()
            && let Some(receiver) = method.params.first().filter(|p| p.name == "self")
        {
            budget.ty(&receiver.ty)?;
            if receiver.give || !super::super::static_traits::receiver_type(&receiver.ty) {
                return Err(error(
                    receiver.span,
                    "trait receiver requires Self, &Self or &mut Self",
                ));
            }
        }
        for (index, parameter) in method.params.iter().enumerate() {
            if !method.generics.is_empty() && index > 0 && parameter.name == "self" {
                return Err(error(
                    parameter.span,
                    "trait receiver must be parameter zero",
                ));
            }
            known_type(&parameter.ty, &method_variables, nominals, budget, 0)?;
        }
        if let Some(result) = &method.return_type {
            known_type(result, &method_variables, nominals, budget, 0)?;
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
                        | "u32"
                        | "i8"
                        | "i16"
                        | "isize"
                        | "u64"
                        | "usize"
                        | "i32"
                        | "i64"
                        | "f32"
                        | "f64"
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
            let declared = nominals.get(name).and_then(|item| match &item.kind {
                ItemKind::Aggregate(a) => Some(a.generics.as_slice()),
                ItemKind::Enum(e) => Some(e.generics.as_slice()),
                _ => None,
            });
            for (position, arg) in args.iter().enumerate() {
                budget.node(ty.span, depth + 1)?;
                let formal = declared.and_then(|parameters| parameters.get(position));
                match arg {
                    GenericArg::Type(t)
                        if formal.is_none_or(|p| matches!(p.kind, GenericParamKind::Type(_))) =>
                    {
                        known_type(t, variables, nominals, budget, depth + 1)?
                    }
                    GenericArg::Lifetime(life)
                        if formal.is_some_and(|p| matches!(p.kind, GenericParamKind::Lifetime)) =>
                    {
                        budget.text(life, ty.span)?;
                        budget.identity_work(variables.len(), ty.span)?;
                        if !variables.contains(life) && life != "static" {
                            return Err(error(
                                ty.span,
                                "trait signature lifetime has no enclosing declared source scope",
                            ));
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            ty.span,
                            "trait signature argument kind requires its complete solver",
                        ));
                    }
                }
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

/// Transient MEMBER/CALLEE node identities in one owned AST. Substitution
/// retains these root Boxes: leaf replacement has no descendant method site,
/// and ReceiverMember only grows children in place. Never carried across a
/// clone or an arbitrary rewriting pass, or used as a published identity.
#[derive(Default)]
pub(crate) struct CallAnchors {
    sites: BTreeMap<usize, CallTarget>,
}
impl CallAnchors {
    pub(crate) fn relocate(
        &mut self,
        moved: &BTreeMap<usize, usize>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        for (old, new) in moved {
            budget.node(span, 0)?;
            if let Some(target) = self.sites.remove(old)
                && self.sites.insert(*new, target).is_some()
            {
                return Err(error(
                    span,
                    "selected error arm merged two captured call sites",
                ));
            }
        }
        Ok(())
    }
}
pub(crate) fn anchor_resolved_calls(
    item: &Item,
    mut calls: CallTargets,
    budget: &mut Budget<'_>,
) -> R<CallAnchors> {
    if calls.is_empty() {
        return Ok(CallAnchors::default());
    }
    let ItemKind::Function(function) = &item.kind else {
        return Err(error(item.span, "call anchors require a function"));
    };
    let ids = expression_ids(&function.body, budget)?;
    let mut sites = BTreeMap::new();
    for (pointer, ordinal) in ids {
        budget.node(item.span, 0)?;
        if let Some(target) = calls.remove(&ordinal) {
            sites.insert(pointer, target);
        }
    }
    if !calls.is_empty() {
        return Err(error(
            item.span,
            "captured call site is absent before substitution",
        ));
    }
    Ok(CallAnchors { sites })
}
pub(crate) fn rekey_anchored_calls(
    item: &Item,
    anchors: CallAnchors,
    budget: &mut Budget<'_>,
) -> R<CallTargets> {
    if anchors.sites.is_empty() {
        return Ok(CallTargets::new());
    }
    let ItemKind::Function(function) = &item.kind else {
        return Err(error(item.span, "call anchors require a function"));
    };
    let ids = expression_ids(&function.body, budget)?;
    let mut calls = CallTargets::new();
    for (pointer, target) in anchors.sites {
        budget.node(item.span, 0)?;
        let ordinal = *ids.get(&pointer).ok_or_else(|| {
            error(
                item.span,
                "captured call site was removed during substitution",
            )
        })?;
        if calls.insert(ordinal, target).is_some() {
            return Err(error(item.span, "two captured call sites share one node"));
        }
    }
    Ok(calls)
}

pub(crate) fn resolve_generic_calls(
    symbolic: &CallTargets,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<CallTargets> {
    let mut resolved = BTreeMap::new();
    for (ordinal, target) in symbolic {
        budget.node(span, 0)?;
        if !target.generic {
            continue;
        }
        let mut reference = Reference {
            key: copy_key(&target.reference.key, budget, span)?,
            arguments: Vec::new(),
        };
        reference.key.arguments.clear();
        for argument in &target.reference.arguments {
            let argument = super::enums::substituted(argument, substitutions, budget)?;
            reference.key.arguments.push(type_key(&argument, budget)?);
            reference.arguments.push(argument);
        }
        budget.text(&target.member, span)?;
        resolved.insert(
            *ordinal,
            CallTarget {
                reference,
                member: target.member.clone(),
                generic: true,
                static_host: target.static_host,
                private_receiver: target.private_receiver.as_ref().map(|receiver| {
                    PrivateReceiver {
                        host: receiver.host.clone(),
                        owners: std::rc::Rc::clone(&receiver.owners),
                    }
                }),
                scalar_host: target
                    .scalar_host
                    .as_ref()
                    .map(|host| super::enums::substituted(host, substitutions, budget))
                    .transpose()?,
            },
        );
    }
    Ok(resolved)
}

pub(crate) fn rewrite_calls(
    item: &mut Item,
    symbolic: &CallTargets,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<()> {
    rewrite_calls_materialized(item, symbolic, substitutions, substitutions, budget)
}
pub(crate) fn rewrite_calls_materialized(
    item: &mut Item,
    symbolic: &CallTargets,
    substitutions: &BTreeMap<String, TypeRef>,
    host_actuals: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut resolved = BTreeMap::new();
    let mut static_hosts = BTreeSet::new();
    let mut static_direct = BTreeSet::new();
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
        let private_receiver = if let Some(receiver) = &target.private_receiver {
            budget.substituted_type(&receiver.host, substitutions)?;
            let mut host = receiver.host.clone();
            Substitution { substitutions }.ty(&mut host);
            if let Some(name) = super::trait_bounds::plain(&host) {
                budget.text(name, item.span)?;
                receiver
                    .owners
                    .get(&reference.key.name)
                    .is_some_and(|hosts| hosts.contains(name))
            } else {
                false
            }
        } else {
            false
        };
        let scalar = if let Some(host) = &target.scalar_host {
            let host = super::enums::substituted(host, host_actuals, budget)?;
            head(&host).is_some_and(super::super::static_traits::scalar_self)
        } else {
            false
        };
        let name = method_name_with(
            &reference,
            &target.member,
            target.generic || scalar || private_receiver,
            budget,
            item.span,
        )?;
        resolved.insert(*ordinal, name);
        if target.static_host {
            budget.node(item.span, 0)?;
            static_hosts.insert(*ordinal);
            if !target.generic {
                budget.node(item.span, 0)?;
                static_direct.insert(*ordinal);
            }
        }
    }
    rewrite_resolved_calls(
        item,
        &resolved,
        &static_hosts,
        &static_direct,
        host_actuals,
        budget,
    )
}
fn rewrite_resolved_calls(
    item: &mut Item,
    calls: &BTreeMap<usize, String>,
    static_hosts: &BTreeSet<usize>,
    static_direct: &BTreeSet<usize>,
    substitutions: &BTreeMap<String, TypeRef>,
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
        static_hosts: &'a BTreeSet<usize>,
        static_direct: &'a BTreeSet<usize>,
        substitutions: &'a BTreeMap<String, TypeRef>,
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
            if matches!(
                &e.kind,
                ExprKind::Member { .. } | ExprKind::ReceiverMember(_)
            ) && let Some(ordinal) = self.ids.get(&(e as *const Expr as usize))
                && let Some(target) = self.calls.get(ordinal)
            {
                self.text(target, e.span)?;
            }
            if let ExprKind::Member { object, .. } = &e.kind
                && let Some(ordinal) = self.ids.get(&(e as *const Expr as usize))
                && self.static_hosts.contains(ordinal)
                && let Some(name) = static_host_name(object)
                && let Some(actual) = self.substitutions.get(name)
            {
                self.budget.ty(actual).map_err(|e| vec![e])?;
                self.budget
                    .node(object.span, depth + 1)
                    .map_err(|e| vec![e])?;
                if !matches!(&grouped(actual).kind, TypeKind::Named { path, .. } if path.len() == 1)
                {
                    return Err(vec![error(
                        object.span,
                        "static trait host is not a nominal type",
                    )]);
                }
            }
            if let ExprKind::Member { object, .. } = &e.kind
                && let Some(ordinal) = self.ids.get(&(e as *const Expr as usize))
                && self.static_direct.contains(ordinal)
                && let Some(host) = static_host_label(object, self.substitutions)
                && let Some(target) = self.calls.get(ordinal)
            {
                self.budget.node(e.span, depth).map_err(|e| vec![e])?;
                self.text(host, e.span)?;
                self.text(".", e.span)?;
                self.text(target, e.span)?;
            }
            super::super::ct_selection::walk_expression(self, e, depth)
        }
    }
    Prepay {
        calls,
        static_hosts,
        static_direct,
        substitutions,
        ids: &ids,
        budget,
    }
    .item(item, 0)
    .map_err(|mut e| e.remove(0))?;
    let visitor = CallRewrite {
        calls,
        static_hosts,
        static_direct,
        substitutions,
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
fn static_host_name(mut object: &Expr) -> Option<&str> {
    while let ExprKind::Group(inner) = &object.kind {
        object = inner;
    }
    if let ExprKind::Name(name) = &object.kind {
        Some(name)
    } else {
        None
    }
}
fn static_host_label<'a>(
    object: &'a Expr,
    substitutions: &'a BTreeMap<String, TypeRef>,
) -> Option<&'a str> {
    let name = static_host_name(object)?;
    if let Some(actual) = substitutions.get(name) {
        if let TypeKind::Named { path, .. } = &grouped(actual).kind
            && path.len() == 1
        {
            return Some(path[0].as_str());
        }
        return None;
    }
    Some(name)
}
fn static_host_expression(actual: &TypeRef, span: Span) -> Expr {
    let TypeKind::Named { path, args } = &grouped(actual).kind else {
        unreachable!("prepaid static host type")
    };
    let callee = Expr {
        semantic: Default::default(),
        span,
        kind: ExprKind::Name(path[0].clone()),
    };
    if args.is_empty() {
        callee
    } else {
        Expr {
            semantic: Default::default(),
            span,
            kind: ExprKind::TypeApply {
                callee: Box::new(callee),
                arguments: args.clone(),
            },
        }
    }
}
struct CallRewrite<'a> {
    calls: &'a BTreeMap<usize, String>,
    static_hosts: &'a BTreeSet<usize>,
    static_direct: &'a BTreeSet<usize>,
    substitutions: &'a BTreeMap<String, TypeRef>,
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
        static_hosts: &BTreeSet::new(),
        static_direct: &BTreeSet::new(),
        substitutions: &BTreeMap::new(),
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
        let site = e as *const Expr as usize;
        if let ExprKind::Member { object, .. } = &e.kind
            && let Some(ordinal) = self.ids.get(&site)
            && self.static_direct.contains(ordinal)
            && let Some(host) = static_host_label(object, self.substitutions)
            && let Some(target) = self.calls.get(ordinal)
        {
            e.kind = ExprKind::Name(format!("{host}.{target}"));
            return;
        }
        let method = match &mut e.kind {
            ExprKind::Member {
                member: MemberName::Name(name) | MemberName::Constrained { name, .. },
                ..
            }
            | ExprKind::ReceiverMember(name) => Some(name),
            _ => None,
        };
        if let Some(name) = method
            && let Some(ordinal) = self.ids.get(&site)
            && let Some(target) = self.calls.get(ordinal)
        {
            name.clone_from(target);
        }
        if let ExprKind::Member { object, .. } = &mut e.kind
            && let Some(ordinal) = self.ids.get(&site)
            && self.static_hosts.contains(ordinal)
            && let Some(name) = static_host_name(object)
            && let Some(actual) = self.substitutions.get(name)
        {
            **object = static_host_expression(actual, object.span);
        }

        match &mut e.kind {
            ExprKind::Name(name) if name == "self" && self.receiver.is_some() => {
                *name = self.receiver.unwrap().to_owned();
            }
            ExprKind::Ascribed { ty, value } => {
                self.ty(ty);
                self.expr(value);
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
                        GenericArg::AssociatedBinding(binding) => match &mut binding.kind {
                            DynBindingKind::Type { value, .. } => self.ty(value),
                            DynBindingKind::Constant { ty, value, .. } => {
                                self.ty(ty);
                                self.expr(value);
                            }
                            DynBindingKind::Method { .. } => {}
                        },
                        GenericArg::Lifetime(_) => {}
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
            (false, false, false), // No associated declarations or generic methods in this fixture.
            None,                  // This retained-source control has no runtime CT callback.
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

#[cfg(test)]
mod static_trait_request {
    use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};

    const CONCRETE: &str = "trait Factory{fn make<T:Copy>(value:T)->Self;}struct Cell{n:i32}impl Factory for Cell{fn make<T:Copy>(value:T)->Self{return Cell{n:7}}}fn main()->i32{return Cell.make::<i32>(3).n-7}";
    const SYMBOLIC: &str = "trait Factory{fn make<T:Copy>(value:T)->Self;}struct Cell{n:i32}impl Factory for Cell{fn make<T:Copy>(value:T)->Self{return Cell{n:7}}}fn build<F:Factory>()->F{return F.make::<i32>(3)}fn main()->i32{return build::<Cell>().n-7}";
    const MULTI: &str = "trait Measure<Tag:Copy>{fn score<A:Copy,B:Copy>(a:A,b:B,tag:Tag)->i32;}struct Cell{n:i32}impl Measure<i32> for Cell{fn score<X:Copy,Y:Copy>(a:X,b:Y,tag:i32)->i32{return tag+1}}fn run<H:Measure<i32>>(x:i32)->i32{return H.score::<i32,bool>(3,true,x)}fn main()->i32{return run::<Cell>(7)-8}";
    const ZERO: &str = "trait Construct<T:Copy>{fn make(value:T)->Self;}struct Cell<T:Copy>{n:T}impl<T:Copy> Construct<T> for Cell<T>{fn make(value:T)->Self{return Cell::<T>{n:value}}}fn build<H:Construct<i32>>(v:i32)->H{return H.make(v)}fn main()->i32{let row=build::<Cell<i32>>(42);return row.n-42}";

    fn expand(text: &str, work: usize) -> zil_base::CompileResult<crate::ExpandedModule> {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "static242.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        crate::expand_with_map_limits(
            &source,
            &syntax,
            &Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
        )
    }

    #[test]
    fn static_type_hosts_have_exact_request_work_and_fresh_results() {
        for (label, text) in [
            ("concrete", CONCRETE),
            ("symbolic", SYMBOLIC),
            ("multi", MULTI),
            ("zero", ZERO),
        ] {
            let mut low = 0usize;
            let mut high = Limits::default().max_analysis_steps;
            let expected = format!("{:?}", expand(text, high).unwrap().core);
            while low + 1 < high {
                let middle = low + (high - low) / 2;
                match expand(text, middle) {
                    Ok(_) => high = middle,
                    Err(errors)
                        if errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit) =>
                    {
                        low = middle
                    }
                    Err(errors) => panic!("{label} unexpected diagnostic at {middle}: {errors:?}"),
                }
            }
            assert!(high > 0, "{label} must consume request work");
            let rejected = expand(text, high - 1).unwrap_err();
            assert!(!rejected.is_empty());
            assert!(
                rejected
                    .iter()
                    .all(|e| e.category == ResultCategory::ResourceLimit),
                "{label}: {rejected:?}"
            );
            assert_eq!(
                format!("{:?}", expand(text, high).unwrap().core),
                expected,
                "{label} exact result"
            );
            assert_eq!(
                format!("{:?}", expand(text, high).unwrap().core),
                expected,
                "{label} fresh result"
            );
            println!("{label} exact request work N={high}; N-1 ResourceLimit; fresh result equal");
        }
    }

    #[test]
    fn static_type_host_does_not_override_value_shadow_or_unused_body_check() {
        let shadow = SYMBOLIC.replace(
            "return F.make::<i32>(3)",
            "let F=Cell{n:1};return F.make::<i32>(3)",
        );
        assert!(expand(&shadow, Limits::default().max_analysis_steps).is_err());
        let unused = CONCRETE.replace("return Cell{n:7}", "return value+1");
        assert!(expand(&unused, Limits::default().max_analysis_steps).is_err());
    }
}
