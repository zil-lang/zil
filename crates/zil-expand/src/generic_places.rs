//! Symbolic place typing. This is neither an alias proof nor a Copy bound.
use super::*;

impl Check<'_, '_, '_> {
    pub(super) fn place(&mut self, expression: &Expr, state: &mut State) -> R<(TypeRef, bool)> {
        self.place_with_use(expression, state, true)
    }

    pub(super) fn assignment_place(
        &mut self,
        expression: &Expr,
        state: &mut State,
    ) -> R<(TypeRef, bool)> {
        self.place_with_use(expression, state, false)
    }

    fn place_with_use(
        &mut self,
        expression: &Expr,
        state: &mut State,
        use_alias: bool,
    ) -> R<(TypeRef, bool)> {
        self.tick(expression.span)?;
        let span = expression.span;
        match &expression.kind {
            ExprKind::Name(name) => {
                let local = state
                    .locals
                    .get(name)
                    .ok_or_else(|| error(span, "unknown generic place"))?;
                if use_alias {
                    self.resource_description_use(local.description_roots.as_deref(), state, span)?;
                }
                if state.moved.contains(&local.id) {
                    return Err(error(span, "generic place used after move"));
                }
                self.budget.ty(&local.ty)?;
                let (id, ty, mutable) = (local.id, local.ty.clone(), local.mutable);
                if use_alias {
                    self.alias_use_local(state, id, span)?;
                }
                Ok((ty, mutable))
            }
            ExprKind::Group(inner) => self.place_with_use(inner, state, use_alias),
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let ty = self.expr(operand, state, None, true)?;
                let TypeKind::Reference { inner, mutable, .. } = &group(&ty).kind else {
                    return Err(error(span, "dereference requires a reference"));
                };
                self.budget.ty(inner)?;
                Ok((*inner.clone(), *mutable))
            }
            ExprKind::Index { object, index } => {
                let (base, mutable) = self.projected_place(object, state)?;
                let index_ty = self.expr(index, state, None, false)?;
                if !integer(&index_ty) {
                    return Err(error(
                        index.span,
                        "array index must have known integer type",
                    ));
                }
                let TypeKind::Array { element, .. } = &group(&base).kind else {
                    return Err(unsupported(
                        span,
                        "index requires an array or borrowed slice",
                    ));
                };
                self.budget.ty(element)?;
                Ok((*element.clone(), mutable))
            }
            ExprKind::Slice { object, start, end } => {
                let (base, mutable) = self.projected_place(object, state)?;
                let TypeKind::Array { element, .. } = &group(&base).kind else {
                    return Err(unsupported(
                        span,
                        "range requires an array or borrowed slice",
                    ));
                };
                for endpoint in [start, end].into_iter().flatten() {
                    let ty = self.expr(endpoint, state, None, false)?;
                    if !integer(&ty) {
                        return Err(error(
                            endpoint.span,
                            "slice endpoint must have known integer type",
                        ));
                    }
                }
                self.budget.ty(element)?;
                Ok((
                    TypeRef {
                        span,
                        kind: TypeKind::Array {
                            element: element.clone(),
                            length: None,
                        },
                    },
                    mutable,
                ))
            }
            ExprKind::Member { object, member } => {
                let (base, mutable) = self.projected_place(object, state)?;
                match (member, &group(&base).kind) {
                    (MemberName::TupleIndex(index), TypeKind::Tuple(fields)) => {
                        let ty = fields
                            .get(*index as usize)
                            .ok_or_else(|| error(span, "tuple place index outside shape"))?;
                        self.budget.ty(ty)?;
                        Ok((ty.clone(), mutable))
                    }
                    (MemberName::Name(name), TypeKind::Named { path, args }) => {
                        let Some(Item {
                            kind: ItemKind::Aggregate(aggregate),
                            ..
                        }) = self.nominals.get(&path.join("."))
                        else {
                            return Err(unsupported(
                                span,
                                "field place requires a declared nominal shape",
                            ));
                        };
                        let field = aggregate
                            .fields
                            .iter()
                            .find(|field| &field.name == name)
                            .ok_or_else(|| error(span, "unknown generic field place"))?;
                        if !field.public && field.span.file != span.file {
                            return Err(Diagnostic::error(
                                Phase::Expand,
                                "E-FIELD-PRIVATE",
                                Some(span),
                                "record field is private to its declaring module",
                            ));
                        }
                        if aggregate.generics.len() != args.len() {
                            return Err(error(span, "generic nominal arity mismatch"));
                        }
                        let mut substitutions = BTreeMap::new();
                        for (parameter, argument) in aggregate.generics.iter().zip(args) {
                            let GenericArg::Type(ty) = argument else {
                                return Err(unsupported(span, "constant generic place"));
                            };
                            self.budget.ty(ty)?;
                            substitutions.insert(parameter.name.clone(), ty.clone());
                        }
                        Ok((
                            substituted_type(&field.ty, &substitutions, self.budget)?,
                            mutable,
                        ))
                    }
                    _ => Err(unsupported(
                        span,
                        "generic projected field requires a known shape",
                    )),
                }
            }
            _ => Err(unsupported(
                span,
                "borrow or assignment requires an addressable place",
            )),
        }
    }

    fn projected_place(&mut self, object: &Expr, state: &mut State) -> R<(TypeRef, bool)> {
        if let ExprKind::Group(inner) = &object.kind {
            self.tick(object.span)?;
            return self.projected_place(inner, state);
        }
        // Address-producing calls are evaluated symbolically once. Ordinary
        // arrays and records must be actual places, never temporary values.
        let (ty, writable) = match object.kind {
            ExprKind::Name(_)
            | ExprKind::Index { .. }
            | ExprKind::Member { .. }
            | ExprKind::Unary {
                op: UnaryOp::Dereference,
                ..
            } => self.place(object, state)?,
            _ => (self.expr(object, state, None, true)?, false),
        };
        if let TypeKind::Reference { inner, mutable, .. } = &group(&ty).kind {
            self.budget.ty(inner)?;
            Ok((*inner.clone(), *mutable))
        } else {
            Ok((ty, writable))
        }
    }
}
