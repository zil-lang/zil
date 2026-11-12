use super::*;

impl Expander<'_> {
    fn source_signature_type(&mut self,item:Option<usize>,parameter:Option<u32>,span:Span)->Option<Type>{
        let item=item?;
        let catalog=self.storage_catalog?;
        if catalog.signature_candidates.is_empty(){return None}
        let limits=Limits{max_analysis_steps:self.member_owner_limit.saturating_sub(self.member_owner_work),
            max_nodes:self.member_owner_nodes,max_call_depth:self.member_owner_depth,..Limits::default()};
        let mut budget=generics::Budget::new(&limits);
        let result=catalog.signature_type(self.module_id,item,parameter,span,&mut budget);
        let left=budget.remaining_limits();
        self.member_owner_work+=limits.max_analysis_steps-left.max_analysis_steps;
        self.member_owner_nodes=left.max_nodes;
        match result{Ok(ty)=>ty,Err(error)=>{self.diagnostics.push(error);None}}
    }

    pub(super) fn function(
        &mut self,
        f: &s::Function,
        prefix: Option<&str>,
        host: Option<&Type>,
        default: Option<&s::Param>,
    ) {
        // Consume the exact immutable Source function emission once. Synthetic
        // defaults and nested lowering helpers never inherit a caller's header.
        let signature_item=self.source_signature_function.take()
            .filter(|(_,function)|*function==std::ptr::from_ref(f)).map(|(ordinal,_)|ordinal);
        if prefix.is_none() && primitive_name(&f.name) {
            self.diagnostics.push(primitive_name_error(&f.name, f.span));
            return;
        }
        let template = match generics::callable::expanded_template(
            f,
            self.declarations,
            &mut self.callable_work,
        ) {
            Ok(template) if prefix.is_none() => template,
            Ok(None) => None,
            Ok(Some(_)) => {
                self.unsupported("callable helper methods are not implemented", f.span);
                None
            }
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                None
            }
        };
        let owned_factory = template.as_ref().is_some_and(|bounds| bounds.owned_factory);
        let source_coordinates = if self
            .storage_catalog
            .is_some_and(|storage| storage.source_helpers)
        {
            let count = template.as_ref().map_or(0, |t| t.roles.len());
            if self
                .member_owner_limit
                .saturating_sub(self.member_owner_work)
                < count
                || self.member_owner_nodes < count
            {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "source helper coordinate snapshot exceeded",
                    Some(f.span),
                ));
                return;
            }
            self.member_owner_work += count;
            self.member_owner_nodes -= count;
            template
                .as_ref()
                .map(|t| {
                    t.roles
                        .iter()
                        .map(|r| (r.index as u32, r.generic_index, r.source_bound_ordinal))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let (callable_parameters, state_parameters) = match template {
            Some(template) => {
                let state_parameters = if let Some(first) = template.state_bindings.first() {
                    vec![zil_ir::CoreStateParameter {
                        index: first.index as u32,
                        bindings: template
                            .state_bindings
                            .into_iter()
                            .map(|binding| zil_ir::CoreStateBinding {
                                generic_index: binding.generic_index,
                                generic_name: binding.generic_name,
                                kind: binding.kind,
                                parameters: binding.parameters.iter().map(type_ref).collect(),
                                result: type_ref(&binding.result),
                                fields: binding.fields,
                            })
                            .collect(),
                    }]
                } else {
                    Vec::new()
                };
                let callable_parameters = template
                    .roles
                    .into_iter()
                    .map(|role| {
                        if let Some((parameters, result, life)) = self.source_rich_role(f, &role) {
                            return CoreCallableParameter {
                                source_life: None,
                                life: Some(life),
                                index: role.index as u32,
                                generic_index: role.generic_index,
                                generic_name: role.generic_name,
                                closed_environment: role.closed_environment,
                                transfer: role.transfer,
                                thread_callable: role.thread_callable,
                                kind: role.kind,
                                parameters,
                                result,
                            };
                        }
                        let parameters = role
                            .parameters
                            .iter()
                            .map(|ty| {
                                if role.life.is_some() {
                                    lifetimes::signature_type(self, ty)
                                } else {
                                    type_ref(ty)
                                }
                            })
                            .collect();
                        let result = if role.life.is_some() {
                            lifetimes::signature_type(self, &role.result)
                        } else {
                            type_ref(&role.result)
                        };
                        let life = role
                            .life
                            .and_then(|life| self.source_flat_life(life, f.span));
                        CoreCallableParameter {
                            source_life: None,
                            life,
                            index: role.index as u32,
                            generic_index: role.generic_index,
                            generic_name: role.generic_name,
                            closed_environment: role.closed_environment,
                            transfer: role.transfer,
                            thread_callable: role.thread_callable,
                            kind: role.kind,
                            parameters,
                            result,
                        }
                    })
                    .collect::<Vec<_>>();
                (callable_parameters, state_parameters)
            }
            None => (Vec::new(), Vec::new()),
        };
        let type_parameters =
            match generics::store_templates::core_parameters(f, &mut self.callable_work) {
                Ok(parameters) if prefix.is_none() => parameters,
                Ok(parameters) if parameters.is_empty() => parameters,
                Ok(_) => {
                    self.unsupported(
                        "ordinary StoreElement template methods are not implemented",
                        f.span,
                    );
                    Vec::new()
                }
                Err(diagnostic) => {
                    self.diagnostics.push(diagnostic);
                    Vec::new()
                }
            };
        // The normalizer records actual lifted member origins. A retained
        // concrete trait premise is allowed only on its life-only provider;
        // the premise has already been checked in the provider definition.
        let life_method_provider = if f.name.starts_with("$generic_method_")
            || f.name.starts_with("$generic_$generic_method_")
        {
            if let Some(providers) = self.method_providers {
                let work = f
                    .name
                    .len()
                    .saturating_add("$generic_$generic_method_".len())
                    .saturating_mul(
                        providers
                            .len()
                            .checked_ilog2()
                            .map_or(1, |n| n as usize + 2),
                    );
                let Some(left) = self.callable_work.checked_sub(work) else {
                    self.diagnostics.push(Diagnostic::limit(
                        Phase::Expand,
                        "method provider lowering lookup exceeded budget",
                        Some(f.span),
                    ));
                    return;
                };
                self.callable_work = left;
                providers.contains(&f.name)
            } else {
                false
            }
        } else {
            false
        };
        if (callable_parameters.is_empty()
            && state_parameters.is_empty()
            && type_parameters.is_empty()
            && !f.generics.is_empty()
            && !lifetimes::only_life(f))
            || (!f.where_clause.is_empty() && !life_method_provider)
        {
            self.unsupported("generic function specialization", f.span);
        }
        let protocol_contracts = self.resource_protocol_contracts(f);
        let result_mode = self.resource_result_mode(f, !protocol_contracts.is_empty());
        let saved_self = self
            .self_type
            .replace(host.cloned().unwrap_or(Type::Unsupported(
                "Self outside a nominal declaration".into(),
            )));
        let saved_receiver = self.lexical_receiver.take();
        let saved_return = self.return_type.take();
        let saved_parent = self.parent_map;
        let id = self.id();
        let saved_function = self.current_function.replace(id);
        for (parameter, generic, bound) in source_coordinates {
            if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "source helper lowering event budget exhausted",
                    Some(f.span),
                ));
                break;
            }
            self.member_owner_work += 1;
            self.member_owner_nodes -= 1;
            self.callable_context_emissions
                .push(CallableContextEmission::Parameter {
                    creator: id,
                    item: self.source_item_ordinal,
                    parameter,
                    generic,
                    bound,
                });
        }
        let parent = self.map_node(id, f.span, ExpansionRole::Source, Vec::new());
        self.parent_map = Some(parent);

        let explicit = if host.is_some() {
            f.params.first().filter(|p| p.name == "self")
        } else {
            None
        };
        let selected = explicit.or(default);
        let life = match lifetimes::contract(
            f,
            selected.filter(|_| explicit.is_none()),
            &mut self.callable_work,
        ) {
            Ok(life) => life,
            Err(error) => {
                self.diagnostics.push(error);
                None
            }
        };
        let mut parameters: Vec<_> = f
            .params
            .iter()
            .map(|p| {
                let mut param = self.param(p);
                if life.is_some() {
                    param.ty = lifetimes::signature_type(self, &p.ty);
                }
                param
            })
            .collect();
        if prefix.is_none() {
            for (index,parameter) in parameters.iter_mut().enumerate(){
                if let Some(ty)=self.source_signature_type(signature_item,Some(index as u32),parameter.span){parameter.ty=ty;}
            }
        }
        let receiver = selected.map(|p| {
            let mut parameter = self.param(p);
            if life.is_some() {
                parameter.ty = lifetimes::signature_type(self, &p.ty);
            }
            if explicit.is_none() {
                parameters.insert(0, parameter.clone());
            }
            self.lexical_receiver = Some(p.span);
            ReceiverShape {
                ty: parameter.ty,
                mode: parameter.mode,
                implicit: explicit.is_none(),
            }
        });
        for (index, parameter) in parameters.iter().enumerate() {
            let role = if index == 0 && selected.is_some() {
                if explicit.is_some() {
                    ExpansionRole::ReceiverExplicit
                } else {
                    ExpansionRole::ReceiverDefault
                }
            } else {
                ExpansionRole::Source
            };
            self.map_target(
                SourceTarget::Parameter {
                    function: id,
                    index: index as u32,
                },
                parameter.span,
                role,
                if role == ExpansionRole::ReceiverDefault {
                    vec![f.span]
                } else {
                    Vec::new()
                },
            );
        }
        let callable_return = f
            .return_type
            .as_ref()
            .and_then(|ty| self.callable_return(ty));
        if callable_return.is_some()
            && prefix.is_none()
            && self
                .storage_catalog
                .is_some_and(|catalog| catalog.source_helpers)
        {
            if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Expand,
                    "opaque return lowering event budget exhausted",
                    Some(f.span),
                ));
            } else {
                self.member_owner_work += 1;
                self.member_owner_nodes -= 1;
                self.callable_context_emissions
                    .push(CallableContextEmission::Return {
                        creator: id,
                        item: self.source_item_ordinal,
                    });
            }
        }
        if callable_return.is_some()
            && ((!callable_parameters.is_empty() && !owned_factory)
                || !state_parameters.is_empty()
                || !type_parameters.is_empty()
                || host.is_some())
        {
            self.unsupported(
                "opaque callable factories must be ordinary nongeneric functions",
                f.span,
            );
        }
        let mut return_type = callable_return.as_ref().map_or_else(
            || {
                f.return_type.as_ref().map_or(Type::Unit, |t| {
                    if life.is_some() {
                        lifetimes::signature_type(self, t)
                    } else {
                        self.dyn_type_ref(t)
                    }
                })
            },
            |bound| Type::Opaque {
                identity: id,
                hidden: Box::new(Type::FunctionPointer {
                    parameters: bound.parameters.clone(),
                    result: Box::new(bound.result.clone()),
                }),
            },
        );
        if prefix.is_none() && let Some(ty)=self.source_signature_type(signature_item,None,f.span){return_type=ty;}
        self.return_type = Some(return_type.clone());
        let body = match &f.body {
            s::FunctionBody::Block(b) => self.block(b),
            s::FunctionBody::Expression(value) => self.arrow_return(value, Some(&return_type)),
            s::FunctionBody::Declaration => {
                self.unsupported("function declaration without implementation", f.span);
                CoreBlock {
                    statements: Vec::new(),
                    span: f.span,
                }
            }
        };
        let (call_kind, resume_bound) = match tasks::selection(f, &mut self.callable_work) {
            Ok(selection) => selection,
            Err(error) => {
                self.diagnostics.push(error);
                (None, zil_ir::TaskResumeBound::Local)
            }
        };
        self.functions.push(CoreFunction {
            source_scope: None,
            dyn_instance: None,
            retained_return_sources: None,
            source_scope: None,
            storage_life: None,
            call_kind,
            resume_bound,
            life,
            callable_return,
            type_parameters,
            state_parameters,
            protocol_contracts,
            callable_parameters,
            result_mode,
            foreign: None,
            id,
            name: prefix.map_or_else(|| f.name.clone(), |p| format!("{p}.{}", f.name)),
            parameters,
            return_type,
            body,
            receiver,
            span: f.span,
        });
        self.current_function = saved_function;
        self.parent_map = saved_parent;
        self.self_type = saved_self;
        self.lexical_receiver = saved_receiver;
        self.return_type = saved_return;
    }

    fn callable_return(&mut self, ty: &s::TypeRef) -> Option<CoreCallableReturn> {
        let s::TypeKind::Impl(bounds) = &ty.kind else {
            return None;
        };
        let invalid = "opaque return requires one finite Fn/FnMut/FnOnce signature";
        let [bound] = bounds.as_slice() else {
            self.unsupported(invalid, ty.span);
            return None;
        };
        let s::TypeKind::Named { path, args } = &bound.kind else {
            self.unsupported(invalid, ty.span);
            return None;
        };
        let kind = match path.as_slice() {
            [name] if name == "Fn" => CallableKind::Fn,
            [name] if name == "FnMut" => CallableKind::FnMut,
            [name] if name == "FnOnce" => CallableKind::FnOnce,
            _ => {
                self.unsupported(invalid, ty.span);
                return None;
            }
        };
        let [s::GenericArg::Type(parameters), s::GenericArg::Type(result)] = args.as_slice() else {
            self.unsupported(invalid, ty.span);
            return None;
        };
        let s::TypeKind::Tuple(parameters) = &parameters.kind else {
            self.unsupported(invalid, ty.span);
            return None;
        };
        Some(CoreCallableReturn {
            life: None,
            source_life: None,
            kind,
            parameters: parameters.iter().map(|t| self.type_ref(t)).collect(),
            result: self.type_ref(result),
        })
    }

    pub(super) fn trait_receiver(
        &mut self,
        trait_type: &s::TypeRef,
        member: &s::Item,
    ) -> Option<s::Param> {
        let s::ItemKind::Function(method) = &member.kind else {
            return None;
        };
        let Some(name) = type_path(trait_type) else {
            self.unsupported("trait receiver requires name resolution", member.span);
            return None;
        };
        let Some(declaration) = self.traits.get(&name) else {
            self.unsupported(
                "trait receiver declaration is not available in this source module",
                member.span,
            );
            return None;
        };
        let signature = declaration
            .members
            .iter()
            .find_map(|item| match &item.kind {
                s::ItemKind::Function(signature) if signature.name == method.name => {
                    Some(signature)
                }
                _ => None,
            });
        let Some(signature) = signature else {
            self.unsupported(
                "trait implementation method has no matching declaration",
                member.span,
            );
            return None;
        };
        // A static trait method deliberately returns None, regardless of body.
        signature
            .params
            .first()
            .filter(|p| p.name == "self")
            .cloned()
    }

    pub(super) fn receiver_member(&mut self, name: &str, span: Span) -> CoreExpr {
        let Some(receiver_origin) = self.lexical_receiver else {
            // Retain the unresolved form for the receiver-specific Types error.
            return self.expr_node(CoreExprKind::ReceiverMember(name.to_owned()), span);
        };
        let object = self.expr_node(CoreExprKind::Name("self".into()), span);
        self.role(
            object.id,
            ExpansionRole::ReceiverShorthand,
            vec![receiver_origin],
        );
        let member = self.expr_node(
            CoreExprKind::Member {
                object: Box::new(object),
                member: name.to_owned(),
                owner: None,
            },
            span,
        );
        self.role(
            member.id,
            ExpansionRole::ReceiverShorthand,
            vec![receiver_origin],
        );
        member
    }

    pub(super) fn type_ref(&self, ty: &s::TypeRef) -> Type {
        substitute_self(type_ref(ty), self.self_type.as_ref())
    }
}

pub(super) fn substitute_self(ty: Type, host: Option<&Type>) -> Type {
    match ty {
        Type::Named(name) if name == "Self" => host.cloned().unwrap_or(Type::Named(name)),
        Type::Address { mutable, inner } => Type::Address {
            mutable,
            inner: Box::new(substitute_self(*inner, host)),
        },
        Type::Desc(inner) => Type::Desc(Box::new(substitute_self(*inner, host))),
        Type::Managed(inner) => Type::Managed(Box::new(substitute_self(*inner, host))),
        Type::Tuple(values) => Type::Tuple(
            values
                .into_iter()
                .map(|t| substitute_self(t, host))
                .collect(),
        ),
        Type::ErrorUnion { error, result } => Type::ErrorUnion {
            error: Box::new(substitute_self(*error, host)),
            result: Box::new(substitute_self(*result, host)),
        },
        Type::ForeignCallback { parameters, result } => Type::ForeignCallback {
            parameters: parameters
                .into_iter()
                .map(|t| substitute_self(t, host))
                .collect(),
            result: Box::new(substitute_self(*result, host)),
        },
        Type::FunctionPointer { parameters, result } => Type::FunctionPointer {
            parameters: parameters
                .into_iter()
                .map(|t| substitute_self(t, host))
                .collect(),
            result: Box::new(substitute_self(*result, host)),
        },
        other => other,
    }
}
