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
fn protocol_push<T>(
    values: &mut Vec<T>,
    value: T,
    storage: &mut impl FnMut(usize, usize) -> Result<(), Diagnostic>,
    span: Span,
) -> Result<(), Diagnostic> {
    if values.len() == values.capacity() {
        let old = values
            .capacity()
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Expand, "protocol allocation overflow", Some(span))
            })?;
        let growth = values.capacity().max(4);
        let new = values
            .capacity()
            .checked_add(growth)
            .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Expand, "protocol allocation overflow", Some(span))
            })?;
        storage(new, old)?;
        values.try_reserve_exact(growth).map_err(|_| {
            Diagnostic::limit(Phase::Expand, "protocol allocation failed", Some(span))
        })?;
    }
    values.push(value);
    Ok(())
}
fn phase_term(
    term: &s::ContractTerm,
    parameter: &str,
    storage: &mut impl FnMut(usize, usize) -> Result<(), Diagnostic>,
) -> Result<Option<Vec<ProtocolState>>, Diagnostic> {
    if term.path.as_slice() != ["state"] || !term.type_args.is_empty() || term.index.is_some() {
        return Ok(None);
    }
    let Some((name, states)) = term.arguments.split_first() else {
        return Ok(None);
    };
    if name.path.as_slice() != [parameter]
        || !name.arguments.is_empty()
        || !name.type_args.is_empty()
        || name.index.is_some()
    {
        return Ok(None);
    }
    let mut out = Vec::new();
    for state in states {
        let Some(state) = protocol_state(state) else {
            return Ok(None);
        };
        protocol_push(&mut out, state, storage, term.span)?;
    }
    if out.is_empty() {
        return Ok(None);
    }
    out.sort_unstable();
    out.dedup();
    Ok(Some(out))
}
fn protocol_pay(work: &mut usize, amount: usize, span: Span) -> Result<(), Diagnostic> {
    *work = work.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "protocol role source work exceeded",
            Some(span),
        )
    })?;
    Ok(())
}
pub(crate) fn protocol_rows(
    f: &s::Function,
    work: &mut usize,
) -> Result<Vec<FunctionProtocolContract>, Diagnostic> {
    protocol_rows_with_storage(f, work, &mut |_, _| Ok(()))
}
/// The allocation observer reports new storage before the previous buffer is
/// retired. This is the same normalizer used by actual Core and the original
/// declaration producer, with independent inputs and accounting contexts.
pub(crate) fn protocol_rows_with_storage(
    f: &s::Function,
    work: &mut usize,
    storage: &mut impl FnMut(usize, usize) -> Result<(), Diagnostic>,
) -> Result<Vec<FunctionProtocolContract>, Diagnostic> {
    let invalid = || {
        Diagnostic::error(
            Phase::Expand,
            "E-PROTOCOL-CONTRACT",
            Some(f.span),
            "finite protocol requires unique actual registration parameters and a complete per-result phase relation for every role",
        )
    };
    let requires = f.contract.iter().filter(|c| c.name == "requires");
    if !requires
        .clone()
        .any(|c| c.terms.iter().any(|t| t.path.as_slice() == ["state"]))
    {
        // A recognized protocol exit without an entry relation is incomplete,
        // not evidence that the source function declared no protocol at all.
        if f.contract.iter().filter(|clause|clause.name == "provides").any(|clause|clause.terms.iter().any(|term|matches!(term.path.as_slice(),[name] if matches!(name.as_str(),"state"|"retired"|"on_ok"|"on_err"|"on_pending"|"on_ready")))) {
            return Err(invalid());
        }
        return Ok(Vec::new());
    }
    protocol_pay(
        work,
        f.contract.len().saturating_add(f.params.len()),
        f.span,
    )?;
    let mut roles: Vec<FunctionProtocolContract> = Vec::new();
    for clause in requires {
        if clause.terms.is_empty() {
            return Err(invalid());
        }
        protocol_pay(
            work,
            clause
                .terms
                .len()
                .saturating_mul(f.params.len().saturating_add(1)),
            f.span,
        )?;
        for term in &clause.terms {
            let name = term
                .arguments
                .first()
                .filter(|t| t.path.len() == 1)
                .and_then(|t| t.path.first())
                .ok_or_else(invalid)?;
            let parameter = f
                .params
                .iter()
                .position(|p| &p.name == name)
                .ok_or_else(invalid)?;
            protocol_pay(work, term.arguments.len(), f.span)?;
            let entry = phase_term(term, name, storage)?.ok_or_else(invalid)?;
            let parameter = u32::try_from(parameter).map_err(|_| invalid())?;
            if roles.iter().any(|role| role.parameter == parameter) {
                return Err(invalid());
            }
            protocol_push(
                &mut roles,
                FunctionProtocolContract {
                    protocol: ProtocolKind::AsyncRegistrationV1,
                    parameter,
                    entry,
                    exits: Vec::new(),
                },
                storage,
                f.span,
            )?;
        }
    }
    for clause in f.contract.iter().filter(|c| c.name == "provides") {
        if clause.terms.is_empty() {
            return Err(invalid());
        }
        protocol_pay(
            work,
            clause
                .terms
                .len()
                .saturating_mul(f.params.len().saturating_add(1)),
            f.span,
        )?;
        for term in &clause.terms {
            if !term.type_args.is_empty() || term.index.is_some() {
                return Err(invalid());
            }
            let exit = match term.path.as_slice() {
                [x] if x == "on_ok" => ProtocolExit::ErrorOk,
                [x] if x == "on_err" => ProtocolExit::ErrorErr,
                [x] if x == "on_pending" => ProtocolExit::PollPending,
                [x] if x == "on_ready" => ProtocolExit::PollReady,
                [x] if x == "state" || x == "retired" => ProtocolExit::Return,
                _ => return Err(invalid()),
            };
            let phase = if exit == ProtocolExit::Return {
                term
            } else {
                term.arguments
                    .first()
                    .filter(|_| term.arguments.len() == 1)
                    .ok_or_else(invalid)?
            };
            let role_name = phase
                .arguments
                .first()
                .filter(|t| {
                    t.path.len() == 1
                        && t.arguments.is_empty()
                        && t.type_args.is_empty()
                        && t.index.is_none()
                })
                .and_then(|t| t.path.first())
                .ok_or_else(invalid)?;
            let parameter = f
                .params
                .iter()
                .position(|p| &p.name == role_name)
                .ok_or_else(invalid)?;
            let role = roles
                .iter_mut()
                .find(|role| role.parameter as usize == parameter)
                .ok_or_else(invalid)?;
            protocol_pay(
                work,
                phase
                    .arguments
                    .len()
                    .saturating_add(role.entry.len().saturating_mul(4)),
                f.span,
            )?;
            let output = if phase.path.as_slice() == ["retired"]
                && phase.type_args.is_empty()
                && phase.index.is_none()
                && phase.arguments.len() == 1
                && f.params[parameter].give
            {
                Vec::new()
            } else {
                phase_term(phase, role_name, storage)?.ok_or_else(invalid)?
            };
            for input in &role.entry {
                storage(std::mem::size_of_val(output.as_slice()), 0)?;
                protocol_push(
                    &mut role.exits,
                    ProtocolTransition {
                        input: *input,
                        exit,
                        output: output.clone(),
                    },
                    storage,
                    f.span,
                )?;
            }
            storage(0, output.capacity() * std::mem::size_of::<ProtocolState>())?;
            drop(output);
        }
    }
    for role in &mut roles {
        protocol_pay(
            work,
            role.exits
                .len()
                .saturating_mul(role.exits.len().saturating_add(1)),
            f.span,
        )?;
        role.exits.sort_unstable_by_key(|r| (r.input, r.exit));
        if role.exits.is_empty()
            || role
                .exits
                .windows(2)
                .any(|w| (w[0].input, w[0].exit) == (w[1].input, w[1].exit))
        {
            return Err(invalid());
        }
    }
    roles.sort_unstable_by_key(|role| role.parameter);
    Ok(roles)
}
impl Expander<'_> {
    pub(super) fn resource_protocol_contracts(
        &mut self,
        f: &s::Function,
    ) -> Vec<FunctionProtocolContract> {
        match protocol_rows(f, &mut self.callable_work) {
            Ok(rows) => rows,
            Err(error) => {
                self.diagnostics.push(error);
                Vec::new()
            }
        }
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
