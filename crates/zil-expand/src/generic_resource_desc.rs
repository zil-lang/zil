//! Definition-time roots of descriptions extracted from actual managed Enum fields.
//! Root sets are immutable: branch snapshots copy references, never their contents.
use super::*;
use std::rc::Rc;

#[derive(Clone, Default)]
pub(super) struct ResourceFlow {
    pub owns: bool,
    pub declaration: bool,
    pub roots: Option<Rc<[usize]>>,
    pub reference_depth: Option<usize>,
}

impl Check<'_, '_, '_> {
    pub(super) fn resource_description_root(&mut self, owner: usize, span: Span) -> R<Rc<[usize]>> {
        // Reserve both the collection entry and its immutable backing storage.
        self.budget.node(span, 0)?;
        self.budget.node(span, 0)?;
        let token = self.next_local;
        self.next_local += 1;
        self.budget.node(span, 0)?;
        self.resource_root_owners.push((owner, token));
        self.resource_desc_roots = true;
        Ok(Rc::from([token]))
    }

    fn resource_description_union(
        &mut self,
        a: Option<&Rc<[usize]>>,
        b: Option<&Rc<[usize]>>,
        span: Span,
    ) -> R<Option<Rc<[usize]>>> {
        let (Some(a), Some(b)) = (a, b) else {
            return Ok(a.or(b).cloned());
        };
        if Rc::ptr_eq(a, b) {
            return Ok(Some(a.clone()));
        }
        // Pay all visits and both possible backing copies before allocation.
        for _ in 0..a.len().saturating_add(b.len()) {
            self.budget.node(span, 0)?;
            self.budget.node(span, 0)?;
        }
        let mut roots = Vec::with_capacity(a.len().saturating_add(b.len()));
        let (mut i, mut j) = (0, 0);
        while i < a.len() || j < b.len() {
            let root = if j == b.len() || (i < a.len() && a[i] <= b[j]) {
                let root = a[i];
                i += 1;
                root
            } else {
                let root = b[j];
                j += 1;
                root
            };
            if roots.last() != Some(&root) {
                roots.push(root);
            }
        }
        Ok(Some(Rc::from(roots)))
    }

    pub(super) fn resource_description_use(
        &mut self,
        roots: Option<&[usize]>,
        state: &State,
        span: Span,
    ) -> R<()> {
        if let Some(roots) = roots {
            for root in roots {
                self.budget.node(span, 0)?;
                self.defer_use(*root, false, span)?;
                if state.moved.contains(root) {
                    return Err(error(
                        span,
                        "resource description outlived its actual owned local",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn resource_description_invalidate(
        &mut self,
        roots: Option<&[usize]>,
        state: &mut State,
        span: Span,
    ) -> R<()> {
        if let Some(roots) = roots {
            for root in roots {
                self.budget.node(span, 0)?;
                self.defer_held(state, *root, true, span)?;
                self.defer_use(*root, true, span)?;
                state.moved.insert(*root);
            }
        }
        Ok(())
    }

    // A bare Name keeps only its real declaration scope and borrowed roots.
    // Only an actual give result can transfer ownership to a fresh local root.
    pub(super) fn transferred_resource_expression(
        &mut self,
        expression: &Expr,
        state: &State,
    ) -> R<ResourceFlow> {
        self.tick(expression.span)?;
        match &expression.kind {
            ExprKind::Name(name) => {
                Ok(state
                    .locals
                    .get(name)
                    .map_or_else(ResourceFlow::default, |local| ResourceFlow {
                        owns: false,
                        declaration: local.declaration_resource,
                        roots: local.description_roots.clone(),
                        reference_depth: local.description_reference_depth,
                    }))
            }
            ExprKind::Group(inner) => self.transferred_resource_expression(inner, state),
            ExprKind::Tuple(values) | ExprKind::Array(values) => {
                for value in values {
                    let flow = self.transferred_resource_expression(value, state)?;
                    if flow.declaration || flow.roots.is_some() {
                        return Err(unsupported(
                            value.span,
                            "resource descriptions are not finite Copy tuple/array fields",
                        ));
                    }
                }
                Ok(ResourceFlow::default())
            }
            ExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                let mut flow = self.transferred_resource_expression(operand, state)?;
                if let Some(depth) = flow.reference_depth {
                    let depth = depth.checked_add(1).ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Expand,
                            "resource description reference depth overflow",
                            Some(expression.span),
                        )
                    })?;
                    self.budget.node(expression.span, depth)?;
                    flow.reference_depth = Some(depth);
                }
                flow.owns = false;
                flow.declaration = false;
                Ok(flow)
            }
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let mut flow = self.transferred_resource_expression(operand, state)?;
                if let Some(depth) = flow.reference_depth {
                    let depth = depth.checked_sub(1).ok_or_else(|| {
                        error(
                            expression.span,
                            "resource description dereference requires an actual reference",
                        )
                    })?;
                    self.budget.node(expression.span, depth)?;
                    flow.reference_depth = Some(depth);
                    flow.declaration = depth == 0;
                }
                flow.owns = false;
                Ok(flow)
            }
            ExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => {
                let owns = self.owned_resource_expression(expression, state)?;
                let mut operand = operand.as_ref();
                while let ExprKind::Group(inner) = &operand.kind {
                    self.tick(operand.span)?;
                    operand = inner;
                }
                let declaration = matches!(&operand.kind, ExprKind::Name(name)
                    if state.locals.get(name).is_some_and(|local| local.declaration_resource));
                Ok(ResourceFlow {
                    owns,
                    declaration,
                    roots: None,
                    reference_depth: declaration.then_some(0),
                })
            }
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let a = self.transferred_resource_expression(then_value, state)?;
                let b = self.transferred_resource_expression(else_value, state)?;
                self.resource_description_merge(&a, &b, expression.span, 0)
            }
            _ => Ok(ResourceFlow::default()),
        }
    }

    fn resource_description_merge(
        &mut self,
        a: &ResourceFlow,
        b: &ResourceFlow,
        span: Span,
        depth: usize,
    ) -> R<ResourceFlow> {
        self.budget.node(span, depth)?;
        if a.declaration != b.declaration || a.owns != b.owns {
            return Err(error(
                span,
                "conditional resource values require the same actual declaration and ownership mode",
            ));
        }
        let roots = self.resource_description_union(a.roots.as_ref(), b.roots.as_ref(), span)?;
        if a.reference_depth != b.reference_depth {
            return Err(unsupported(
                span,
                "conditional resource description positions differ",
            ));
        }
        Ok(ResourceFlow {
            owns: a.owns,
            declaration: a.declaration,
            roots,
            reference_depth: a.reference_depth,
        })
    }

    pub(super) fn resource_description_join(
        &mut self,
        state: &mut State,
        branches: &[&State],
        span: Span,
    ) -> R<()> {
        if !self.resource_desc_roots {
            return Ok(());
        }
        for (name, local) in &mut state.locals {
            self.budget.node(span, 0)?;
            let mut merged = None;
            let mut owner_complete = local.owned_resource && local.declaration_resource;
            let mut owner_roots = None;
            let mut owner_roots_changed = false;
            for branch in branches {
                self.budget.node(span, 0)?;
                let actual = branch
                    .locals
                    .get(name)
                    .filter(|value| value.id == local.id)
                    .unwrap_or(local);
                owner_complete &= actual.owned_resource
                    && actual.declaration_resource
                    && !branch.moved.contains(&actual.id)
                    && actual.description_roots.is_some();
                if let Some(roots) = &actual.description_roots {
                    if let Some(previous) = &owner_roots {
                        if !Rc::ptr_eq(previous, roots) {
                            for _ in 0..previous.len().saturating_add(roots.len()) {
                                self.budget.node(span, 0)?;
                            }
                            owner_roots_changed |= previous.as_ref() != roots.as_ref();
                        }
                    } else {
                        owner_roots = Some(roots.clone());
                    }
                }
                let flow = ResourceFlow {
                    owns: actual.owned_resource,
                    declaration: actual.declaration_resource,
                    roots: actual.description_roots.clone(),
                    reference_depth: actual.description_reference_depth,
                };
                merged = Some(match merged {
                    None => flow,
                    Some(ref previous) => {
                        self.resource_description_merge(previous, &flow, span, 0)?
                    }
                });
            }
            if let Some(mut flow) = merged {
                if owner_complete && owner_roots_changed {
                    // The current complete owner exists on every continuing
                    // path. Its joint activation is distinct from old borrowed
                    // descriptions, whose may-invalidated tokens remain moved.
                    flow.roots = Some(self.resource_description_root(local.id, span)?);
                }
                local.declaration_resource = flow.declaration;
                local.description_roots = flow.roots;
                local.description_reference_depth = flow.reference_depth;
            }
        }
        Ok(())
    }

    pub(super) fn resource_description_scope_exit(
        &mut self,
        state: &mut State,
        first_local: usize,
        span: Span,
    ) -> R<()> {
        if !self.resource_desc_roots {
            return Ok(());
        }
        // This immutable activation registry also retains hidden/shadowed
        // owned locals. Name lookup alone cannot establish scope cleanup.
        for &(owner, token) in &self.resource_root_owners {
            self.budget.node(span, 0)?;
            if owner >= first_local {
                self.budget.node(span, 0)?;
                state.moved.insert(token);
            }
        }
        Ok(())
    }

    pub(super) fn resource_description_replacement(
        &mut self,
        target: &Expr,
        value: &Expr,
        state: &State,
    ) -> R<Option<(String, ResourceFlow)>> {
        if !self.enum_resource_flow {
            return Ok(None);
        }
        let ExprKind::Name(name) = &target.kind else {
            return Ok(None);
        };
        let Some(local) = state.locals.get(name) else {
            return Ok(None);
        };
        let flow = self.transferred_resource_expression(value, state)?;
        if !local.declaration_resource
            && !flow.declaration
            && local.description_roots.is_none()
            && flow.roots.is_none()
        {
            return Ok(None);
        }
        if local.declaration_resource != flow.declaration || local.owned_resource != flow.owns {
            return Err(error(
                value.span,
                "resource replacement must preserve its actual declaration and ownership mode",
            ));
        }
        self.budget.text(name, target.span)?;
        Ok(Some((name.clone(), flow)))
    }

    pub(super) fn resource_description_replace(
        &mut self,
        name: &str,
        flow: ResourceFlow,
        state: &mut State,
        span: Span,
    ) -> R<()> {
        let old_roots = state.locals[name].description_roots.clone();
        let roots = if flow.owns {
            self.resource_description_invalidate(old_roots.as_deref(), state, span)?;
            Some(self.resource_description_root(state.locals[name].id, span)?)
        } else {
            flow.roots
        };
        let local = state
            .locals
            .get_mut(name)
            .expect("actual replacement local");
        local.description_roots = roots;
        local.description_reference_depth = flow.reference_depth;
        Ok(())
    }
}
