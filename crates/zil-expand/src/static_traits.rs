//! Finite nominal trait implementations for a local concrete Self. Declaration
//! conformance emits untrusted Core, which still follows the
//! ordinary type/analyze/independent-Verify pipeline. No trait grants capability.
use super::*;
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
fn receiver_type(ty: &s::TypeRef) -> bool {
    match &ty.kind {
        s::TypeKind::Named { path, args } => path.as_slice() == ["Self"] && args.is_empty(),
        s::TypeKind::Reference { inner, .. } => plain_name(inner) == Some("Self"),
        _ => false,
    }
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
        if !t.generics.is_empty() || !t.bounds.is_empty() {
            self.unsupported(
                "generic or supertrait declarations require the full trait solver",
                item.span,
            );
            return;
        }
        let mut names = BTreeSet::new();
        let mut valid = true;
        for member in &t.members {
            self.unexecuted_attributes(&member.attributes, "trait member");
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
            if !finite_method(f) || !matches!(f.body, s::FunctionBody::Declaration) {
                self.unsupported("default, generic or contract-bearing trait methods require additional trait evidence", member.span);
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
    ) {
        // Preserve the old no-trait and builtin Drop paths. An actually
        // normalized external trait implementation activates project lookup.
        if !module.items.iter().any(|item| match &item.kind {
            s::ItemKind::Trait(_) => true,
            s::ItemKind::Impl(implementation) => implementation
                .trait_type
                .as_ref()
                .and_then(plain_name)
                .is_some_and(|name| name.starts_with("$module") && !self.traits.contains_key(name)),
            _ => false,
        }) {
            return;
        }
        // Charge the established declaration/member total before constructing
        // local indexes, including modules newly entering through an imported
        // trait. The old path retains exactly the same total cost.
        let mut work = limits.max_analysis_steps;
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
        // The concrete Self must still be defined in this module. Project
        // signatures do not grant foreign-Self or generic implementation rights.
        let locals: BTreeMap<_, _> = module
            .items
            .iter()
            .filter_map(|item| {
                if let s::ItemKind::Aggregate(a) = &item.kind {
                    Some((a.name.as_str(), a))
                } else {
                    None
                }
            })
            .collect();
        let fields: BTreeMap<_, BTreeSet<_>> = locals
            .iter()
            .map(|(name, a)| (*name, a.fields.iter().map(|f| f.name.as_str()).collect()))
            .collect();
        let mut declared = BTreeSet::new();
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
        for a in locals.values() {
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
        for item in &module.items {
            let s::ItemKind::Impl(i) = &item.kind else {
                continue;
            };
            let Some(trait_ty) = &i.trait_type else {
                continue;
            };
            if plain_name(trait_ty) == Some("Drop") {
                continue;
            }
            let (Some(trait_name), Some(host_name)) = (plain_name(trait_ty), plain_name(&i.target))
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
            let Some(host) = locals.get(host_name).filter(|a| {
                matches!(
                    a.kind,
                    s::AggregateKind::Struct | s::AggregateKind::Resource
                ) && a.generics.is_empty()
            }) else {
                self.unsupported(
                    "static local traits require a concrete local value struct or a shared-head resource implementation",
                    item.span,
                );
                continue;
            };
            if !i.generics.is_empty() || !i.where_clause.is_empty() || i.receiver_default.is_some()
            {
                self.unsupported(
                    "generic, conditional or group-default trait impl",
                    item.span,
                );
                continue;
            }
            if !impls.insert((trait_name.to_owned(), host_name.to_owned())) {
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
            if host.kind == s::AggregateKind::Resource
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
            let required: BTreeMap<_, _> = declaration
                .members
                .iter()
                .filter_map(|m| {
                    if let s::ItemKind::Function(f) = &m.kind {
                        Some((f.name.as_str(), f))
                    } else {
                        None
                    }
                })
                .collect();
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
                let host_ty = Type::Named(host_name.into());
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
                if methods
                    .get(&(host_name.into(), f.name.clone()))
                    .copied()
                    .unwrap_or(0)
                    != 1
                    || fields[host_name].contains(f.name.as_str())
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
                self.static_trait_impls.insert(item.span);
                if project_impl {
                    self.project_trait_work = self
                        .project_trait_work
                        .saturating_add(before_project_work - work);
                }
            }
        }
    }
}
