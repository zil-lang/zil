//! Explicit finite resource syntax. These bindings remain untrusted Core.
use super::*;

fn protocol_state(term: &s::ContractTerm) -> Option<ProtocolState> {
    if !term.arguments.is_empty() || !term.type_args.is_empty() || term.index.is_some() {
        return None;
    }
    match term.path.as_slice() {
        [name] if name == "Open" => Some(ProtocolState::Open),
        [name] if name == "Closing" => Some(ProtocolState::Closing),
        [name] if name == "Closed" => Some(ProtocolState::Closed),
        _ => None,
    }
}
fn phase_term(term: &s::ContractTerm, parameter: &str) -> Option<Vec<ProtocolState>> {
    if term.path.as_slice() != ["state"] || !term.type_args.is_empty() || term.index.is_some() {
        return None;
    }
    let (name, states) = term.arguments.split_first()?;
    if name.path.as_slice() != [parameter]
        || !name.arguments.is_empty()
        || !name.type_args.is_empty()
        || name.index.is_some()
    {
        return None;
    }
    let mut out = states
        .iter()
        .map(protocol_state)
        .collect::<Option<Vec<_>>>()?;
    if out.is_empty() {
        return None;
    }
    out.sort();
    out.dedup();
    Some(out)
}
impl Expander<'_> {
    pub(super) fn resource_protocol_contract(
        &mut self,
        f: &s::Function,
    ) -> Option<FunctionProtocolContract> {
        let requires = f.contract.iter().find(|c| {
            c.name == "requires" && c.terms.iter().any(|t| t.path.as_slice() == ["state"])
        })?;
        let invalid = || {
            Diagnostic::error(
                Phase::Expand,
                "E-PROTOCOL-CONTRACT",
                Some(f.span),
                "finite protocol requires one actual AsyncRegistration parameter and a complete per-result phase relation",
            )
        };
        let Some(term) = requires.terms.first() else {
            self.diagnostics.push(invalid());
            return None;
        };
        let Some(name) = term.arguments.first().and_then(|t| t.path.first()) else {
            self.diagnostics.push(invalid());
            return None;
        };
        let Some(parameter) = f.params.iter().position(|p| &p.name == name) else {
            self.diagnostics.push(invalid());
            return None;
        };
        let Some(entry) = phase_term(term, name) else {
            self.diagnostics.push(invalid());
            return None;
        };
        let provides = f
            .contract
            .iter()
            .filter(|c| c.name == "provides")
            .collect::<Vec<_>>();
        if f.contract.iter().filter(|c| c.name == "requires").count() != 1
            || requires.terms.len() != 1
            || term.path.as_slice() != ["state"]
            || provides.len() != 1
        {
            self.diagnostics.push(invalid());
            return None;
        }
        let mut exits = Vec::new();
        for term in &provides[0].terms {
            let exit = match term.path.as_slice() {
                [x] if x == "on_ok" => ProtocolExit::ErrorOk,
                [x] if x == "on_err" => ProtocolExit::ErrorErr,
                [x] if x == "on_pending" => ProtocolExit::PollPending,
                [x] if x == "on_ready" => ProtocolExit::PollReady,
                [x] if x == "state" || x == "retired" => ProtocolExit::Return,
                _ => {
                    self.diagnostics.push(invalid());
                    return None;
                }
            };
            let phase = if exit == ProtocolExit::Return {
                term
            } else {
                let Some(inner) = term.arguments.first().filter(|_| term.arguments.len() == 1)
                else {
                    self.diagnostics.push(invalid());
                    return None;
                };
                inner
            };
            let retired = phase.path.as_slice() == ["retired"]
                && phase.type_args.is_empty()
                && phase.index.is_none()
                && matches!(phase.arguments.as_slice(),[term] if term.path.as_slice()==[name.as_str()] && term.arguments.is_empty() && term.type_args.is_empty() && term.index.is_none());
            let output = if retired && f.params[parameter].give {
                Some(vec![])
            } else {
                phase_term(phase, name)
            };
            let Some(output) = output else {
                self.diagnostics.push(invalid());
                return None;
            };
            for input in &entry {
                exits.push(ProtocolTransition {
                    input: *input,
                    exit,
                    output: output.clone(),
                });
            }
        }
        exits.sort_by_key(|r| (r.input, r.exit));
        if exits.is_empty()
            || exits
                .windows(2)
                .any(|w| (w[0].input, w[0].exit) == (w[1].input, w[1].exit))
        {
            self.diagnostics.push(invalid());
            return None;
        }
        Some(FunctionProtocolContract {
            protocol: ProtocolKind::AsyncRegistrationV1,
            parameter: parameter as u32,
            entry,
            exits,
        })
    }
}

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
    pub(super) fn resource_result_mode(&mut self, f: &s::Function, protocol: bool) -> PassingMode {
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
                None if matches!(clause.name.as_str(), "effects" | "resume")
                    || (protocol && matches!(clause.name.as_str(), "requires" | "provides")) => {}
                None => self.unsupported(
                    "callable contract clause outside finite resource results",
                    clause.span,
                ),
            }
        }
        mode
    }
}
