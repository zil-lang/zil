//! Explicit finite resource syntax. These bindings remain untrusted Core.
use super::*;

fn builtin_drop(i: &s::ImplBlock) -> bool {
    matches!(&i.trait_type, Some(s::TypeRef { kind: s::TypeKind::Named { path, args }, .. }) if path.as_slice() == ["Drop"] && args.is_empty())
}
impl Expander<'_> {
    pub(super) fn collect_resource_drops(&mut self, module: &s::Module) {
        let resources = module
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                s::ItemKind::Aggregate(a) if a.kind == s::AggregateKind::Resource => {
                    Some(a.name.as_str())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for item in &module.items {
            let s::ItemKind::Impl(i) = &item.kind else {
                continue;
            };
            if !builtin_drop(i) {
                continue;
            }
            let target = type_path(&i.target);
            let valid_target =
                matches!(&i.target.kind, s::TypeKind::Named { args, .. } if args.is_empty());
            let valid_method = i.members.len() == 1
                && matches!(&i.members[0].kind,
                s::ItemKind::Function(f) if f.name == "drop" && f.generics.is_empty()
                && f.where_clause.is_empty() && f.contract.is_empty()
                && f.params.len() == 1 && f.params[0].name == "self" && !f.params[0].give
                && matches!(&f.params[0].ty.kind, s::TypeKind::Named { path, args } if path.as_slice() == ["Self"] && args.is_empty())
                && f.return_type.as_ref().is_none_or(|t| type_ref(t) == Type::Unit)
                && !matches!(f.body, s::FunctionBody::Declaration));
            if !valid_target
                || target.as_deref().is_none_or(|n| !resources.contains(n))
                || !i.generics.is_empty()
                || !i.where_clause.is_empty()
                || i.receiver_default.is_some()
                || !valid_method
            {
                self.diagnostics.push(Diagnostic::error(
                    Phase::Expand,
                    "E-DROP-SIGNATURE",
                    Some(item.span),
                    "Drop requires a local resource and exactly fn drop(self: Self) returning void",
                ));
                continue;
            }
            let target = target.expect("validated target");
            if self
                .drop_implementations
                .insert(target.clone(), format!("{target}.$drop"))
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    Phase::Expand,
                    "E-DROP-DUPLICATE",
                    Some(item.span),
                    "a resource has one Drop implementation",
                ));
            }
        }
    }
    pub(super) fn resource_drop(&mut self, i: &s::ImplBlock) -> bool {
        if !builtin_drop(i) {
            return false;
        }
        let Some(target) = type_path(&i.target) else {
            return true;
        };
        if !self.drop_implementations.contains_key(&target) {
            return true;
        }
        for item in &i.members {
            self.unexecuted_attributes(&item.attributes, "Drop method");
            if let s::ItemKind::Function(f) = &item.kind {
                let mut hidden = f.clone();
                hidden.name = "$drop".into();
                self.function(
                    &hidden,
                    Some(&target),
                    Some(&Type::Named(target.clone())),
                    None,
                );
            }
        }
        true
    }
    pub(super) fn resource_result_mode(&mut self, f: &s::Function) -> PassingMode {
        let mut mode = PassingMode::Infer;
        for clause in &f.contract {
            let term = clause.terms.first();
            let selected = match term {
                Some(t)
                    if clause.name == "result"
                        && clause.terms.len() == 1
                        && t.type_args.is_empty()
                        && t.arguments.is_empty()
                        && t.index.is_none() =>
                {
                    match t.path.as_slice() {
                        [name] if name == "create" => Some(PassingMode::Create),
                        [name] if name == "give" => Some(PassingMode::Give),
                        _ => None,
                    }
                }
                _ => None,
            };
            match selected {
                Some(selected) if mode == PassingMode::Infer => mode = selected,
                Some(_) => self.diagnostics.push(Diagnostic::error(
                    Phase::Expand,
                    "E-RESOURCE-RESULT-MODE",
                    Some(clause.span),
                    "resource result modes are mutually exclusive",
                )),
                None => self.unsupported(
                    "callable contract clause outside finite resource results",
                    clause.span,
                ),
            }
        }
        mode
    }
}
