//! Real lexical generic hosts retain their own complete instance vectors.
use super::*;

impl<'a> Compiler<'a> {
    fn member_host(&mut self, declaration: usize, span: Span) -> R<Option<&'a s::Item>> {
        self.budget.work(self.member_hosts.len(), span)?;
        Ok(self
            .member_hosts
            .iter()
            .find(|row| row.declaration == declaration)
            .map(|row| row.host))
    }

    pub(super) fn has_generic_host(&mut self, declaration: usize) -> R<bool> {
        let span = self.declarations[declaration].item.span;
        Ok(self.member_host(declaration, span)?.is_some_and(|host|
            matches!(&host.kind, s::ItemKind::Aggregate(value) if !value.generics.is_empty())))
    }

    pub(super) fn source_host_instance(
        &mut self,
        module: usize,
        declaration: usize,
        arguments: Option<&[s::GenericArg]>,
        depth: usize,
        span: Span,
    ) -> R<Option<CtAmbientScopeId>> {
        self.budget.depth(depth, span)?;
        let Some(host) = self.member_host(declaration, span)? else {
            if arguments.is_some() {
                return Err(error(
                    span,
                    "CT host actuals require an actual member declaration",
                ));
            }
            return Ok(None);
        };
        let s::ItemKind::Aggregate(aggregate) = &host.kind else {
            unreachable!()
        };
        if aggregate.generics.is_empty() {
            if arguments.is_some_and(|args| !args.is_empty()) {
                return Err(error(span, "nongeneric CT host takes no actuals"));
            }
            return Ok(None);
        }
        let arguments = arguments.ok_or_else(|| {
            error(
                span,
                "generic CT member requires complete explicit host actuals",
            )
        })?;
        if arguments.len() != aggregate.generics.len() {
            return Err(error(span, "CT host full actual vector arity differs"));
        }
        let mut actuals = Vec::new();
        for (parameter, argument) in aggregate.generics.iter().zip(arguments) {
            self.budget.node(span)?;
            actuals.push(match (&parameter.kind, argument) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    self.generic_names(std::slice::from_ref(parameter), &[], parameter.span)?;
                    let graph = self.metadata_type(module, ty, depth + 1)?;
                    self.metadata_copy(&graph, depth + 1, span)?;
                    CtScopeActual::Type(graph)
                }
                (s::GenericParamKind::Const(_), argument) => {
                    CtScopeActual::Scalar(self.source_scalar_argument(parameter, argument, span)?)
                }
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    CtScopeActual::Lifetime(self.ambient_actual(name, span)?)
                }
                _ => {
                    return Err(error(
                        span,
                        "CT host actual kind differs from original binder",
                    ));
                }
            });
        }
        let owner = self.declarations[declaration].module;
        let raw = CtAmbientLifeScope {
            declaration: self.item_symbol(owner, host)?,
            enclosing: None,
            binder_site: Vec::new(),
            instance: CtScopeInstance::Instantiated(actuals),
        };
        self.budget.work(self.ambient_life_scopes.len(), span)?;
        for old in &self.ambient_life_scopes {
            self.budget.scope_comparison(old, span)?;
        }
        self.budget.scope_comparison(&raw, span)?;
        let index = if let Some(index) = self.ambient_life_scopes.iter().position(|old| old == &raw)
        {
            index
        } else {
            self.budget.node(span)?;
            Budget::take(
                &mut self.budget.bytes,
                std::mem::size_of::<CtAmbientLifeScope>(),
                span,
                "CT host scope inventory bytes exhausted",
            )?;
            self.ambient_life_scopes.push(raw);
            self.ambient_life_scopes.len() - 1
        };
        Ok(Some(CtAmbientScopeId(
            u32::try_from(index).map_err(|_| error(span, "CT host scope ID overflow"))?,
        )))
    }

    pub(super) fn bind_host_instance(&mut self, declaration: usize) -> R<()> {
        let span = self.declarations[declaration].item.span;
        let Some(host) = self.member_host(declaration, span)? else {
            return Ok(());
        };
        let s::ItemKind::Aggregate(aggregate) = &host.kind else {
            unreachable!()
        };
        if aggregate.generics.is_empty() {
            return Ok(());
        }
        let id = self
            .active_host
            .ok_or_else(|| error(span, "CT member instance lost its authentic host context"))?;
        let scope = self
            .ambient_life_scopes
            .get(id.0 as usize)
            .ok_or_else(|| error(span, "CT host scope is outside inventory"))?;
        self.budget.scope_comparison(scope, span)?;
        let expected = self.item_symbol(self.declarations[declaration].module, host)?;
        let scope = &self.ambient_life_scopes[id.0 as usize];
        if scope.declaration != expected {
            return Err(error(
                span,
                "CT instance host differs from actual lexical parent",
            ));
        }
        let CtScopeInstance::Instantiated(actuals) = &scope.instance else {
            return Err(error(
                span,
                "CT concrete member requires an instantiated host",
            ));
        };
        Self::actual_cost(&mut self.budget, actuals, span)?;
        let actuals = actuals.clone();
        if actuals.len() != aggregate.generics.len() {
            return Err(error(span, "CT instance host arity differs"));
        }
        for (parameter, actual) in aggregate.generics.iter().zip(actuals) {
            self.budget.text(&parameter.name, parameter.span)?;
            match (&parameter.kind, actual) {
                (s::GenericParamKind::Type(_), CtScopeActual::Type(graph)) => {
                    self.metadata_bindings.push((parameter.name.clone(), graph))
                }
                (s::GenericParamKind::Const(_), CtScopeActual::Scalar(value)) => {
                    self.scalar_bindings.push((parameter.name.clone(), value))
                }
                (s::GenericParamKind::Lifetime, CtScopeActual::Lifetime(_)) => {}
                _ => return Err(error(span, "CT instance host kind differs")),
            }
        }
        Ok(())
    }

    pub(super) fn bind_abstract_host(&mut self, declaration: usize) -> R<()> {
        let span = self.declarations[declaration].item.span;
        let Some(host) = self.member_host(declaration, span)? else {
            return Ok(());
        };
        let s::ItemKind::Aggregate(aggregate) = &host.kind else {
            unreachable!()
        };
        if aggregate.generics.is_empty() {
            return Ok(());
        }
        let definition = self.item_symbol(self.declarations[declaration].module, host)?;
        let member_parameters = &self.declarations[declaration].function.generics;
        for (ordinal, parameter) in aggregate.generics.iter().enumerate() {
            self.budget.node(parameter.span)?;
            let mut shadowed = false;
            for member in member_parameters {
                self.budget
                    .work(member.name.len().min(parameter.name.len()) + 1, span)?;
                shadowed |= member.name == parameter.name;
            }
            if shadowed {
                continue;
            }
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(span, "CT host original binder ordinal overflow"))?;
            self.budget.text(&parameter.name, span)?;
            self.budget.work(32, span)?;
            match &parameter.kind {
                s::GenericParamKind::Const(domain) => {
                    let domain = self.scalar_argument_type(domain, parameter.span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<(String, symbolic_nominal::ScalarArgument)>(),
                        span,
                        "CT host symbolic scalar bindings exhausted",
                    )?;
                    self.abstract_scalars.push((
                        parameter.name.clone(),
                        symbolic_nominal::ScalarArgument::Parameter {
                            definition: definition.clone(),
                            ordinal,
                            domain,
                        },
                    ));
                }
                s::GenericParamKind::Lifetime => {
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<(String, CtSymbolId, u32)>(),
                        span,
                        "CT host symbolic life bindings exhausted",
                    )?;
                    self.abstract_lives
                        .push((parameter.name.clone(), definition.clone(), ordinal));
                }
                s::GenericParamKind::Type(_) => {}
            }
        }
        let names = self.generic_names(&aggregate.generics, &[], span)?;
        let mut bindings = Vec::new();
        for name in names {
            // A member binder shadows the same spelling in its lexical host.
            // Keep both actual owners; do not let the host's first-match data
            // binding replace the member's abstract Type parameter.
            let mut shadowed = false;
            for parameter in member_parameters {
                self.budget
                    .work(parameter.name.len().min(name.len()) + 1, span)?;
                shadowed |= parameter.name == name;
            }
            if shadowed {
                continue;
            }
            self.budget.node(span)?;
            self.budget.text(&name, span)?;
            let ty = CtTy::Record(CtRecordSchemaId(u32::MAX - self.symbolic.len() as u32));
            self.budget.copy_ty(&ty, 0, span)?;
            self.symbolic.push(ty.clone());
            bindings.push((name, ty));
        }
        bindings.append(&mut self.bindings);
        self.bindings = bindings;
        Ok(())
    }
}
