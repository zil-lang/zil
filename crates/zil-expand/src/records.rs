use super::*;

pub(super) fn project_default_name(nominal: &str, field: &str) -> String {
    // Length framing avoids ambiguity even for generated nominal names.
    format!("$record_default_{}_{}_{}", nominal.len(), nominal, field)
}

impl Expander<'_> {
    pub(super) fn record(
        &mut self,
        origin: Span,
        written_type: Option<&s::Expr>,
        base: Option<&s::Expr>,
        fields: &[s::RecordField],
        expected: Option<&Type>,
    ) -> CoreExpr {
        let previous_parent = self.parent_map;
        self.parent_map = Some(self.marker(origin));
        let target = if let Some(written) = written_type {
            expr_path(written).map(|name| {
                if name == "Self" {
                    self.self_type.clone().unwrap_or(Type::Named(name))
                } else {
                    Type::Named(name)
                }
            })
        } else {
            expected.cloned().filter(|t| matches!(t, Type::Named(_)))
        };
        // Generic/aliased nominal identity is a type-resolution task. Never
        // infer a type by searching for a matching set of field names.
        let declaration = match &target {
            Some(Type::Named(name)) => self.declarations.get(name).copied().cloned(),
            _ => None,
        };
        // A source constructor names every required field. Materializing an
        // already verified CT result is a value copy, so construction privacy
        // belongs here, before CT reifies values into ordinary Core records.
        if base.is_none()
            && declaration.as_ref().is_some_and(|d| {
                d.fields
                    .iter()
                    .any(|f| !f.public && f.span.file != origin.file)
            })
        {
            self.diagnostics.push(Diagnostic::error(
                Phase::Expand,
                "E-FIELD-PRIVATE",
                Some(origin),
                "record construction requires access to every private field",
            ));
        }
        let mut bindings = Vec::new();
        let prepared_base = base.map(|base| {
            let value = self.expr_expected(base, target.as_ref());
            self.role(value.id, ExpansionRole::RecordBase, vec![origin]);
            let name = self.temporary("record_base");
            bindings.push((name.clone(), value));
            self.temporary_read(&name, base.span)
        });
        let mut prepared_fields = Vec::new();
        for field in fields {
            let expected = declaration
                .as_ref()
                .and_then(|d| d.fields.iter().find(|f| f.name == field.name))
                .map(|f| receivers::substitute_self(type_ref(&f.ty), target.as_ref()));
            let value = match &field.value {
                Some(value) => self.expr_expected(value, expected.as_ref()),
                None => self.expr_node(CoreExprKind::Name(field.name.clone()), field.span),
            };
            self.role(value.id, ExpansionRole::RecordField, vec![origin]);
            let temporary = self.temporary("record_field");
            bindings.push((temporary.clone(), value));
            let read = self.temporary_read(&temporary, field.span);
            prepared_fields.push((field.name.clone(), read));
        }
        // Defaults are not re-run by an update. Ordinary construction evaluates
        // them only after all explicit fields, in declaration order.
        if base.is_none()
            && let Some(declaration) = &declaration
        {
            for field in &declaration.fields {
                if fields.iter().any(|f| f.name == field.name) {
                    continue;
                }
                let Some(default) = &field.default else {
                    continue;
                };
                if field.manages {
                    self.unsupported("owned package field defaults are not implemented", origin);
                    continue;
                }
                let callee = if default.span.file == origin.file {
                    let helper = self.default_helper(&declaration.name, field);
                    self.expr_node(CoreExprKind::Name(helper), default.span)
                } else {
                    let selected = self
                        .project_defaults
                        .and_then(|defaults| {
                            defaults.get(&(declaration.name.clone(), field.name.clone()))
                        })
                        .cloned();
                    let Some((owner, helper)) = selected else {
                        self.unsupported(
                            "cross-module defaults require a public ordinary record field",
                            origin,
                        );
                        continue;
                    };
                    if owner == self.module_id {
                        self.expr_node(CoreExprKind::Name(helper), default.span)
                    } else {
                        let alias = format!("$record_defaults_module{}", owner.0);
                        self.generated_imports
                            .entry(owner)
                            .or_insert_with(|| CoreImport {
                                alias: alias.clone(),
                                module: owner,
                                span: origin,
                            });
                        let object = self.expr_node(CoreExprKind::Name(alias), origin);
                        self.expr_node(
                            CoreExprKind::Member {
                                object: Box::new(object),
                                member: helper,
                            },
                            origin,
                        )
                    }
                };
                let call = self.expr_node(
                    CoreExprKind::Call {
                        callee: Box::new(callee),
                        arguments: Vec::new(),
                    },
                    default.span,
                );
                self.role(call.id, ExpansionRole::RecordDefault, vec![origin]);
                let temporary = self.temporary("record_default");
                bindings.push((temporary.clone(), call));
                let read = self.temporary_read(&temporary, default.span);
                prepared_fields.push((field.name.clone(), read));
            }
        }
        let kind = if let Some(base) = prepared_base {
            CoreExprKind::RecordUpdate {
                ty: target,
                base: Box::new(base),
                fields: prepared_fields,
            }
        } else {
            CoreExprKind::Record {
                ty: target.unwrap_or_else(|| {
                    Type::Unsupported("record requires a resolved nominal expected type".into())
                }),
                fields: prepared_fields,
            }
        };
        let result = self.expr_node(kind, origin);
        self.role(
            result.id,
            if base.is_some() {
                ExpansionRole::RecordUpdate
            } else {
                ExpansionRole::RecordResult
            },
            Vec::new(),
        );
        let sequence = self.expr_node(
            CoreExprKind::Sequence {
                bindings,
                value: Box::new(result),
            },
            origin,
        );
        self.role(
            sequence.id,
            if base.is_some() {
                ExpansionRole::RecordUpdate
            } else {
                ExpansionRole::RecordResult
            },
            Vec::new(),
        );
        self.parent_map = previous_parent;
        sequence
    }

    pub(super) fn default_helper(&mut self, nominal: &str, field: &s::Field) -> String {
        let key = (nominal.to_owned(), field.name.clone());
        if let Some(name) = self.default_helpers.get(&key) {
            return name.clone();
        }
        let name = self
            .project_defaults
            .and_then(|defaults| defaults.get(&key))
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| self.temporary("field_default"));
        // Register before expanding the body, so recursive default syntax stays
        // a finite recursive call graph rather than recursively cloning ASTs.
        let exported = self.project_defaults.is_some_and(|defaults| {
            defaults
                .get(&key)
                .is_some_and(|(owner, _)| *owner == self.module_id)
        });
        self.default_helpers.insert(key, name.clone());
        let function = s::Function {
            name: name.clone(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: Some(field.ty.clone()),
            where_clause: Vec::new(),
            contract: Vec::new(),
            body: s::FunctionBody::Expression(
                field.default.clone().expect("caller selected a default"),
            ),
            span: field.span,
        };
        let saved_parent = self.parent_map;
        self.parent_map = None;
        self.function(
            &function,
            None,
            Some(&Type::Named(nominal.to_owned())),
            None,
        );
        self.parent_map = saved_parent;
        if let Some(id) = self
            .functions
            .last()
            .filter(|f| f.name == name)
            .map(|f| f.id)
        {
            if exported {
                self.generated_exports.push(id);
            }
            self.role(id, ExpansionRole::RecordDefault, Vec::new());
        }
        name
    }

    pub(super) fn pipeline(&mut self, origin: Span, input: &s::Expr, target: &s::Expr) -> CoreExpr {
        let previous_parent = self.parent_map;
        self.parent_map = Some(self.marker(origin));
        let input_value = self.expr(input);
        self.role(input_value.id, ExpansionRole::PipeInput, vec![origin]);
        let input_name = self.temporary("pipe_input");
        let target_value = self.expr(target);
        self.role(target_value.id, ExpansionRole::PipeTarget, vec![origin]);
        let argument = self.temporary_read(&input_name, input.span);
        // The call evaluates its callee once after the input preparation, then
        // consumes this single prepared argument identity. No placeholder call
        // rewriting, tuple spreading, implicit try, or implicit borrowing occurs.
        let call = self.expr_node(
            CoreExprKind::Call {
                callee: Box::new(target_value),
                arguments: vec![argument],
            },
            origin,
        );
        self.role(call.id, ExpansionRole::PipeCall, vec![target.span]);
        let result = self.expr_node(
            CoreExprKind::Sequence {
                bindings: vec![(input_name, input_value)],
                value: Box::new(call),
            },
            origin,
        );
        self.role(
            result.id,
            ExpansionRole::PipeCall,
            vec![input.span, target.span],
        );
        self.parent_map = previous_parent;
        result
    }
}
