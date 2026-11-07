//! Finite nominal trait implementations for a local concrete Self. Declaration
//! conformance emits untrusted Core, which still follows the
//! ordinary type/analyze/independent-Verify pipeline. No trait grants capability.
use super::*;
use crate::generics::{Budget as TraitBudget, trait_instances};
use crate::receivers::substitute_self;

fn plain_name(ty: &s::TypeRef) -> Option<&str> {
    match &ty.kind {
        s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
            Some(path[0].as_str())
        }
        _ => None,
    }
}
pub(super) fn reserved_trait(name: &str) -> bool {
    // Only the normalizer's exact prefix is ignored for exclusion. Nominal
    // equality and implementation lookup always retain the complete identity.
    let name = name
        .strip_prefix("$module")
        .and_then(|suffix| {
            suffix.split_once('_').filter(|(module, _)| {
                !module.is_empty() && module.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        .map_or(name, |(_, source)| source);
    matches!(
        name,
        "Copy" | "Transfer" | "Share" | "Send" | "Sync" | "Drop" | "Fn" | "FnMut" | "FnOnce"
    )
}
pub(super) fn receiver_type(ty: &s::TypeRef) -> bool {
    match &ty.kind {
        s::TypeKind::Named { path, args } => path.as_slice() == ["Self"] && args.is_empty(),
        s::TypeKind::Reference { inner, .. } => plain_name(inner) == Some("Self"),
        _ => false,
    }
}
// Scalar Self has no user-owned outer nominal declaration. Its ordinary
// trait implementation must therefore belong to the real trait declaration.
pub(super) fn scalar_self(name: &str) -> bool {
    matches!(
        name,
        "void" | "bool" | "u8" | "u16" | "u64" | "usize" | "i32" | "i64" | "f32" | "f64"
    )
}

fn finite_method(f: &s::Function) -> bool {
    f.generics.is_empty() && f.where_clause.is_empty() && f.contract.is_empty()
}

impl<'a> Expander<'a> {
    fn trait_error(&mut self, code: &str, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(Phase::Expand, code, Some(span), message));
    }

    fn collect_trait_declaration(&mut self, item: &'a s::Item, work: &mut usize) {
        let s::ItemKind::Trait(t) = &item.kind else {
            return;
        };
        if t.name.starts_with("$module") {
            let Some(left) = work.checked_sub(t.name.len()) else {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "module trait identity budget exceeded",
                    Some(item.span),
                ));
                return;
            };
            *work = left;
        }
        if reserved_trait(&t.name) {
            self.trait_error(
                "E-TRAIT-RESERVED",
                item.span,
                "built-in capability and cleanup traits cannot be redeclared",
            );
            return;
        }
        let mut names = BTreeSet::new();
        let mut valid = true;
        for member in &t.members {
            self.unexecuted_attributes(&member.attributes, "trait member");
            if matches!(
                member.kind,
                s::ItemKind::TypeAlias(_) | s::ItemKind::Const(_)
            ) {
                continue;
            }
            let s::ItemKind::Function(f) = &member.kind else {
                self.unsupported(
                    "associated trait items are not in the static local trait slice",
                    member.span,
                );
                valid = false;
                continue;
            };
            let cost = f.params.len().saturating_add(1);
            let Some(left) = work.checked_sub(cost) else {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "trait signature budget exceeded",
                    Some(member.span),
                ));
                return;
            };
            *work = left;
            for parameter in &f.params {
                self.unexecuted_attributes(&parameter.attributes, "trait parameter");
            }
            if !names.insert(f.name.clone()) {
                self.trait_error("E-TRAIT-METHOD-SET", member.span, "duplicate trait method");
                valid = false;
            }
            if !finite_method(f) {
                self.unsupported(
                    "generic or contract-bearing trait methods require additional trait evidence",
                    member.span,
                );
                valid = false;
            }
            if f.params
                .first()
                .is_some_and(|p| p.name == "self" && (!receiver_type(&p.ty) || p.give))
                || f.params.iter().skip(1).any(|p| p.name == "self")
            {
                self.trait_error("E-TRAIT-SIGNATURE", member.span, "static value trait receiver must be Self, &Self, or &mut Self in first position");
                valid = false;
            }
        }
        if valid {
            self.static_traits.insert(t.name.clone());
        }
    }

    pub(super) fn collect_static_traits(
        &mut self,
        module: &'a s::Module,
        declarations: &[&'a s::Module],
        imports: &[CoreImport],
        limits: &Limits,
        signatures: &trait_instances::Signatures,
        metered_trait_instances: bool,
    ) {
        // Preserve the old no-trait and builtin Drop paths. An actually
        // normalized external trait implementation activates project lookup.
        if !module.items.iter().any(|item| match &item.kind {
            s::ItemKind::Trait(_) => true,
            s::ItemKind::Impl(implementation) => implementation
                .trait_type
                .as_ref()
                .and_then(trait_instances::head)
                .is_some_and(|name| name.starts_with("$module") && !self.traits.contains_key(name)),
            _ => false,
        }) {
            return;
        }
        // Charge the established declaration/member total before constructing
        // local indexes, including modules newly entering through an imported
        // trait. The old path retains exactly the same total cost.
        let mut work = limits.max_analysis_steps;
        let starting_project_work = self.project_trait_work;
        for item in &module.items {
            let visits = 1usize.saturating_add(match &item.kind {
                s::ItemKind::Aggregate(a) => a.fields.len().saturating_add(a.methods.len()),
                s::ItemKind::Impl(i) => i.members.len(),
                s::ItemKind::Trait(t) => t.members.len(),
                _ => 0,
            });
            let Some(left) = work.checked_sub(visits) else {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "trait declaration scan exceeded budget",
                    None,
                ));
                return;
            };
            work = left;
        }
        // Concrete nominal Self belongs to this module. A scalar has no
        // nominal owner and instead requires the actual local trait below.
        // Project signatures alone grant neither ownership nor an impl.
        let locals: BTreeMap<_, _> = module
            .items
            .iter()
            .filter_map(|item| {
                if let s::ItemKind::Aggregate(a) = &item.kind {
                    Some((a.name.as_str(), (a, item.public)))
                } else {
                    None
                }
            })
            .collect();
        let fields: BTreeMap<_, BTreeSet<_>> = locals
            .iter()
            .map(|(name, (a, _))| (*name, a.fields.iter().map(|f| f.name.as_str()).collect()))
            .collect();
        let mut declared = BTreeSet::new();
        let mut public_traits = BTreeSet::new();
        for item in &module.items {
            let s::ItemKind::Trait(t) = &item.kind else {
                continue;
            };
            if !declared.insert(t.name.clone()) || locals.contains_key(t.name.as_str()) {
                self.trait_error(
                    "E-TRAIT-DUPLICATE",
                    item.span,
                    "duplicate nominal trait name",
                );
                continue;
            }
            if item.public {
                public_traits.insert(t.name.as_str());
            }
            self.collect_trait_declaration(item, &mut work);
            if t.name.starts_with("$module") {
                self.project_trait_work = self.project_trait_work.saturating_add(t.name.len());
            }
        }
        let mut project_nodes = limits.max_nodes;
        let mut impls = BTreeSet::new();
        // A single ordinary function name is the machine target in this slice.
        // Reject unresolved collisions, never silently pick the first impl.
        let mut methods: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (a, _) in locals.values() {
            for member in &a.methods {
                if let s::ItemKind::Function(f) = &member.kind {
                    *methods.entry((a.name.clone(), f.name.clone())).or_default() += 1;
                }
            }
        }
        for item in &module.items {
            if let s::ItemKind::Impl(i) = &item.kind
                && let Some(host) = plain_name(&i.target)
            {
                for member in &i.members {
                    if let s::ItemKind::Function(f) = &member.kind {
                        *methods.entry((host.into(), f.name.clone())).or_default() += 1;
                    }
                }
            }
        }
        for (item_ordinal, item) in module.items.iter().enumerate() {
            let s::ItemKind::Impl(i) = &item.kind else {
                continue;
            };
            let Some(trait_ty) = &i.trait_type else {
                continue;
            };
            if plain_name(trait_ty) == Some("Drop") {
                continue;
            }
            let (Some(trait_name), Some(host_name)) =
                (trait_instances::head(trait_ty), plain_name(&i.target))
            else {
                self.unsupported(
                    "trait impl requires concrete local nominal names",
                    item.span,
                );
                continue;
            };
            let project_impl = !self.traits.contains_key(trait_name)
                || self.imported_public_traits.contains(trait_name);
            let before_project_work = work;
            if !self.traits.contains_key(trait_name) && trait_name.starts_with("$module") {
                match project_traits::declaration(
                    project_traits::Lookup {
                        current: module.id,
                        name: trait_name,
                        span: item.span,
                    },
                    declarations,
                    imports,
                    &mut work,
                    &mut project_nodes,
                    limits,
                ) {
                    Ok(Some(declaration_item)) => {
                        let s::ItemKind::Trait(declaration) = &declaration_item.kind else {
                            unreachable!()
                        };
                        self.traits.insert(trait_name.to_owned(), declaration);
                        self.collect_trait_declaration(declaration_item, &mut work);
                        if self.static_traits.contains(trait_name) {
                            self.imported_public_traits.insert(trait_name.to_owned());
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.diagnostics.push(error);
                        continue;
                    }
                }
            }
            if !self.static_traits.contains(trait_name) {
                self.unsupported(
                    "trait impl declaration is not a supported local trait",
                    item.span,
                );
                continue;
            }
            let host = locals.get(host_name).filter(|(a, _)| {
                matches!(
                    a.kind,
                    s::AggregateKind::Struct | s::AggregateKind::Resource
                ) && a.generics.is_empty()
            });
            let scalar = scalar_self(host_name);
            if scalar && !declared.contains(trait_name) {
                self.trait_error(
                    "E-TRAIT-OWNER",
                    item.span,
                    "a scalar trait implementation requires the actual local trait declaration",
                );
                continue;
            }
            let mut enum_host = false;
            let mut enum_public = None;
            if host.is_none() && !scalar {
                for declaration in &module.items {
                    let cost = 1usize.saturating_add(match &declaration.kind {
                        s::ItemKind::Enum(e) => e.name.len().saturating_add(host_name.len()),
                        _ => 0,
                    });
                    let Some(left) = work.checked_sub(cost) else {
                        self.diagnostics.push(Diagnostic::limit(
                            Phase::Expand,
                            "trait enum host lookup budget exhausted",
                            Some(item.span),
                        ));
                        return;
                    };
                    work = left;
                    if let s::ItemKind::Enum(e) = &declaration.kind
                        && e.generics.is_empty()
                        && e.name == host_name
                    {
                        enum_host = true;
                        enum_public = Some(declaration.public);
                        break;
                    }
                }
            }
            if host.is_none() && !enum_host && !scalar {
                self.unsupported(
                    "static traits require a concrete local value nominal or a scalar with its local trait",
                    item.span,
                );
                continue;
            }
            if !i.generics.is_empty() || !i.where_clause.is_empty() || i.receiver_default.is_some()
            {
                self.unsupported(
                    "generic, conditional or group-default trait impl",
                    item.span,
                );
                continue;
            }
            let declaration = *self
                .traits
                .get(trait_name)
                .expect("collected local declaration");
            let public_host = host.map(|(_, public)| *public).or(enum_public);
            let private_export = public_host == Some(false)
                && (public_traits.contains(trait_name)
                    || self.imported_public_traits.contains(trait_name));
            let mut instance_reference = None;
            let Some(left) = work.checked_sub(declaration.members.len()) else {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "trait member scan budget exhausted",
                    Some(item.span),
                ));
                return;
            };
            work = left;
            if private_export
                || scalar
                || enum_host
                || !declaration.generics.is_empty()
                || declaration
                    .members
                    .iter()
                    .any(|m| matches!(m.kind, s::ItemKind::TypeAlias(_) | s::ItemKind::Const(_)))
            {
                let mut found = None;
                for source in declarations.iter().copied().chain(std::iter::once(module)) {
                    for item in &source.items {
                        let Some(left) = work.checked_sub(1) else {
                            self.diagnostics.push(Diagnostic::limit(
                                Phase::Expand,
                                "trait instance identity work exhausted",
                                Some(item.span),
                            ));
                            return;
                        };
                        work = left;
                        if let s::ItemKind::Trait(trait_declaration) = &item.kind {
                            let Some(left) = work.checked_sub(
                                trait_declaration
                                    .name
                                    .len()
                                    .min(trait_name.len())
                                    .saturating_add(1),
                            ) else {
                                self.diagnostics.push(Diagnostic::limit(
                                    Phase::Expand,
                                    "trait declaration identity comparison budget exhausted",
                                    Some(item.span),
                                ));
                                return;
                            };
                            work = left;
                            if trait_declaration.name == trait_name {
                                found = Some(item.span);
                            }
                        }
                    }
                }
                let Some(declaration_span) = found else {
                    self.unsupported("trait instance declaration identity is absent", item.span);
                    continue;
                };
                let selected = Limits {
                    max_analysis_steps: work,
                    max_nodes: project_nodes,
                    ..limits.clone()
                };
                let mut meter = TraitBudget::new(&selected);
                let result =
                    trait_instances::arguments(declaration_span, declaration, trait_ty, &mut meter);
                let left = trait_instances::remaining(&meter);
                work = left.max_analysis_steps;
                project_nodes = left.max_nodes;
                match result {
                    Ok(reference) => instance_reference = Some(reference),
                    Err(error) => {
                        self.diagnostics.push(error);
                        continue;
                    }
                }
            }
            if metered_trait_instances && let Some(reference) = &instance_reference {
                let cost = reference
                    .key
                    .arguments
                    .iter()
                    .try_fold(0usize, |n, key| n.checked_add(key.len().saturating_add(1)));
                let Some(left) = cost.and_then(|cost| work.checked_sub(cost)) else {
                    self.diagnostics.push(Diagnostic::limit(
                        Phase::Expand,
                        "trait coherence key budget exhausted",
                        Some(item.span),
                    ));
                    return;
                };
                work = left;
                let Some(left) = project_nodes.checked_sub(reference.key.arguments.len()) else {
                    self.diagnostics.push(Diagnostic::limit(
                        Phase::Expand,
                        "trait coherence key node budget exhausted",
                        Some(item.span),
                    ));
                    return;
                };
                project_nodes = left;
            }
            let keys = instance_reference
                .as_ref()
                .map_or_else(Vec::new, |reference| reference.key.arguments.clone());
            if !impls.insert((trait_name.to_owned(), keys, host_name.to_owned())) {
                self.trait_error(
                    "E-TRAIT-DUPLICATE",
                    item.span,
                    "overlapping concrete trait implementation",
                );
                continue;
            }
            let declaration = *self
                .traits
                .get(trait_name)
                .expect("collected local declaration");
            if host.is_some_and(|(host, _)| host.kind == s::AggregateKind::Resource)
                && declaration.members.iter().any(|member| match &member.kind {
                    s::ItemKind::Function(function) => !function.params.first().is_some_and(|parameter|
                        parameter.name == "self" && !parameter.give
                        && matches!(&parameter.ty.kind, s::TypeKind::Reference { mutable: false, inner, .. }
                            if plain_name(inner) == Some("Self"))),
                    _ => true,
                })
            {
                self.unsupported(
                    "resource trait implementations in this slice require shared &Self receivers",
                    item.span,
                );
                continue;
            }
            let Some(left) =
                work.checked_sub(declaration.members.len().saturating_add(i.members.len()))
            else {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "trait implementation signatures exceeded budget",
                    Some(item.span),
                ));
                return;
            };
            work = left;
            let mut required = BTreeMap::new();
            for member in &declaration.members {
                if let s::ItemKind::Function(method) = &member.kind {
                    let signature = if let Some(reference) = &instance_reference {
                        let selected = Limits {
                            max_analysis_steps: work,
                            max_nodes: project_nodes,
                            ..limits.clone()
                        };
                        let mut meter = TraitBudget::new(&selected);
                        let mut actual = None;
                        for candidate in &i.members {
                            if let Err(error) = meter.node(candidate.span, 0) {
                                self.diagnostics.push(error);
                                return;
                            }
                            if let s::ItemKind::Function(function) = &candidate.kind {
                                if let Err(error) = meter
                                    .text(&function.name, candidate.span)
                                    .and_then(|()| meter.text(&method.name, member.span))
                                {
                                    self.diagnostics.push(error);
                                    return;
                                }
                                if function.name == method.name {
                                    actual = Some(function);
                                    break;
                                }
                            }
                        }
                        let identity_key = match trait_instances::copy_key(
                            &reference.key,
                            &mut meter,
                            member.span,
                        ) {
                            Ok(key) => key,
                            Err(error) => {
                                self.diagnostics.push(error);
                                return;
                            }
                        };
                        if let Err(error) = meter.text(host_name, item.span) {
                            self.diagnostics.push(error);
                            return;
                        }
                        if let Some(actual) = actual
                            && let Err(error) = meter.text(&actual.name, actual.span)
                        {
                            self.diagnostics.push(error);
                            return;
                        }
                        let result = if let Some(actual) = actual
                            && let Some(frozen) = signatures.get(&(
                                identity_key,
                                host_name.to_owned(),
                                actual.name.clone(),
                            )) {
                            if frozen.key != reference.key || frozen.host != host_name {
                                Err(Diagnostic::error(
                                    Phase::Expand,
                                    "E-TRAIT-SIGNATURE",
                                    Some(member.span),
                                    "trait required signature has a different source instance",
                                ))
                            } else {
                                for p in &frozen.method.params {
                                    if let Err(error) = meter.ty(&p.ty) {
                                        self.diagnostics.push(error);
                                        return;
                                    }
                                }
                                if let Some(result) = &frozen.method.return_type
                                    && let Err(error) = meter.ty(result)
                                {
                                    self.diagnostics.push(error);
                                    return;
                                }
                                Ok(std::borrow::Cow::Borrowed(&frozen.method))
                            }
                        } else {
                            trait_instances::signature(declaration, member, reference, &mut meter)
                        };
                        let left = trait_instances::remaining(&meter);
                        work = left.max_analysis_steps;
                        project_nodes = left.max_nodes;
                        match result {
                            Ok(signature) => signature,
                            Err(error) => {
                                self.diagnostics.push(error);
                                return;
                            }
                        }
                    } else {
                        std::borrow::Cow::Borrowed(method)
                    };
                    required.insert(method.name.as_str(), signature);
                }
            }
            let mut implemented = BTreeSet::new();
            let mut valid = true;
            for member in &i.members {
                let s::ItemKind::Function(f) = &member.kind else {
                    self.unsupported("associated implementation item", member.span);
                    valid = false;
                    continue;
                };
                if !implemented.insert(f.name.as_str()) {
                    self.trait_error(
                        "E-TRAIT-METHOD-SET",
                        member.span,
                        "duplicate implementation method",
                    );
                    valid = false;
                }
                let Some(signature) = required.get(f.name.as_str()) else {
                    self.trait_error(
                        "E-TRAIT-METHOD-SET",
                        member.span,
                        "method not declared by this trait",
                    );
                    valid = false;
                    continue;
                };
                if !finite_method(f) || matches!(f.body, s::FunctionBody::Declaration) {
                    self.unsupported(
                        "trait implementation must have an ordinary concrete body",
                        member.span,
                    );
                    valid = false;
                }
                let cost = f
                    .params
                    .len()
                    .saturating_add(signature.params.len())
                    .saturating_add(1);
                let Some(left) = work.checked_sub(cost) else {
                    self.diagnostics.push(Diagnostic::limit(
                        Phase::Expand,
                        "trait implementation budget exceeded",
                        Some(member.span),
                    ));
                    return;
                };
                work = left;
                let host_ty = type_ref(&i.target);
                let ty = |t: &s::TypeRef| substitute_self(type_ref(t), Some(&host_ty));
                let mut actual: Vec<_> = f.params.iter().map(|p| (ty(&p.ty), p.give)).collect();
                let inherited = signature.params.first().filter(|p| p.name == "self");
                let explicit = f.params.first().is_some_and(|p| p.name == "self");
                if !explicit && let Some(receiver) = inherited {
                    actual.insert(0, (ty(&receiver.ty), receiver.give));
                }
                let expected: Vec<_> = signature
                    .params
                    .iter()
                    .map(|p| (ty(&p.ty), p.give))
                    .collect();
                let result = f.return_type.as_ref().map_or(Type::Unit, ty);
                let expected_result = signature.return_type.as_ref().map_or(Type::Unit, ty);
                if (explicit && inherited.is_none())
                    || actual != expected
                    || result != expected_result
                {
                    self.trait_error("E-TRAIT-SIGNATURE", member.span, "implementation receiver, parameters, passing modes and return type must match the declared signature");
                    valid = false;
                }
                if instance_reference.is_none()
                    && (methods
                        .get(&(host_name.into(), f.name.clone()))
                        .copied()
                        .unwrap_or(0)
                        != 1
                        || fields
                            .get(host_name)
                            .is_some_and(|fields| fields.contains(f.name.as_str())))
                {
                    self.unsupported("colliding trait, inherent or field name requires explicit trait-qualified resolution", member.span);
                    valid = false;
                }
            }
            if implemented.len() != required.len()
                || required.keys().any(|name| !implemented.contains(name))
            {
                self.trait_error(
                    "E-TRAIT-METHOD-SET",
                    item.span,
                    "implementation must define every declared method exactly once",
                );
                valid = false;
            }
            if valid {
                if let Some(reference) = &instance_reference {
                    for (member_ordinal, member) in i.members.iter().enumerate() {
                        if let s::ItemKind::Function(method) = &member.kind {
                            let selected = Limits {
                                max_analysis_steps: work,
                                max_nodes: project_nodes,
                                ..limits.clone()
                            };
                            let mut meter = TraitBudget::new(&selected);
                            let result = trait_instances::method_name_with(
                                reference,
                                &method.name,
                                scalar || private_export,
                                &mut meter,
                                member.span,
                            );
                            let left = trait_instances::remaining(&meter);
                            work = left.max_analysis_steps;
                            project_nodes = left.max_nodes;
                            match result {
                                Ok(target) => {
                                    if private_export
                                        && reference.arguments.is_empty()
                                        && method.generics.is_empty()
                                        && methods.get(&(host_name.into(), method.name.clone()))
                                            == Some(&1)
                                        && fields.get(host_name).is_none_or(|fields| {
                                            !fields.contains(method.name.as_str())
                                        })
                                    {
                                        self.private_trait_aliases
                                            .insert((item_ordinal, member_ordinal));
                                    }
                                    self.trait_method_names
                                        .insert((item_ordinal, member_ordinal), target);
                                }
                                Err(error) => {
                                    self.diagnostics.push(error);
                                    return;
                                }
                            }
                        }
                    }
                }
                self.static_trait_impls.insert(item_ordinal);
                if project_impl || instance_reference.is_some() {
                    self.project_trait_work = self
                        .project_trait_work
                        .saturating_add(before_project_work - work);
                }
            }
        }
        if metered_trait_instances {
            self.project_trait_work =
                starting_project_work.saturating_add(limits.max_analysis_steps - work);
            self.project_trait_nodes = self
                .project_trait_nodes
                .saturating_add(limits.max_nodes - project_nodes);
        }
    }
}
