use super::*;

impl Expander<'_> {
    pub(super) fn function(
        &mut self,
        f: &s::Function,
        prefix: Option<&str>,
        host: Option<&Type>,
        default: Option<&s::Param>,
    ) {
        if prefix.is_none() && primitive_name(&f.name) {
            self.diagnostics.push(primitive_name_error(&f.name, f.span));
            return;
        }
        let template = match generics::callable::expanded_template(
            f,
            &self.declarations,
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
                    .map(|role| CoreCallableParameter {
                        index: role.index as u32,
                        generic_index: role.generic_index,
                        generic_name: role.generic_name,
                        kind: role.kind,
                        parameters: role.parameters.iter().map(type_ref).collect(),
                        result: type_ref(&role.result),
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
        if (callable_parameters.is_empty()
            && state_parameters.is_empty()
            && type_parameters.is_empty()
            && !f.generics.is_empty())
            || !f.where_clause.is_empty()
        {
            self.unsupported("generic function specialization", f.span);
        }
        let result_mode = self.resource_result_mode(f);
        let saved_self = self
            .self_type
            .replace(host.cloned().unwrap_or(Type::Unsupported(
                "Self outside a nominal declaration".into(),
            )));
        let saved_receiver = self.lexical_receiver.take();
        let saved_return = self.return_type.take();
        let saved_parent = self.parent_map;
        let id = self.id();
        let parent = self.map_node(id, f.span, ExpansionRole::Source, Vec::new());
        self.parent_map = Some(parent);

        let explicit = if host.is_some() {
            f.params.first().filter(|p| p.name == "self")
        } else {
            None
        };
        let selected = explicit.or(default);
        let mut parameters: Vec<_> = f.params.iter().map(|p| self.param(p)).collect();
        let receiver = selected.map(|p| {
            let parameter = self.param(p);
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
        let return_type = callable_return.as_ref().map_or_else(
            || {
                f.return_type
                    .as_ref()
                    .map_or(Type::Unit, |t| self.type_ref(t))
            },
            |bound| Type::Opaque {
                identity: id,
                hidden: Box::new(Type::FunctionPointer {
                    parameters: bound.parameters.clone(),
                    result: Box::new(bound.result.clone()),
                }),
            },
        );
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
        self.functions.push(CoreFunction {
            callable_return,
            type_parameters,
            state_parameters,
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
