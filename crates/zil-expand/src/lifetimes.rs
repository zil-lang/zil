//! Explicit function life declarations survive address layout erasure.
use super::*;

/// Walk every type position, including unused definitions, before generic
/// removal. Header node identities are request-local traversal context only.
pub(super) fn validate(module: &s::Module, budget: &mut generics::Budget<'_>) -> CompileResult<()> {
    struct Walk<'a, 'l> {
        budget: &'a mut generics::Budget<'l>,
        binders: Vec<String>,
        headers: Vec<*const s::TypeRef>,
        trait_scope: bool,
        impl_scope: bool,
        mutable_life: bool,
        stored_fields: bool,
    }
    fn reference(ty: &s::TypeRef) -> &s::TypeRef {
        match &ty.kind {
            s::TypeKind::Group(inner) => reference(inner),
            _ => ty,
        }
    }
    impl ct_selection::Visitor for Walk<'_, '_> {
        fn type_binder(&mut self, name: &str, span: Span) -> CompileResult<()> {
            self.budget
                .note_type_binder(name, span)
                .map_err(|e| vec![e])
        }
        fn enum_bound(&mut self, owner: Span, bound: &s::TypeRef, _: usize) -> CompileResult<()> {
            if generics::record_enums::is_copy_bound(bound) {
                self.budget.note_enum_profile(owner).map_err(|e| vec![e])?;
            }
            Ok(())
        }
        fn record_variant(&mut self, owner: Span, _: usize) -> CompileResult<()> {
            self.budget.note_enum_profile(owner).map_err(|e| vec![e])
        }
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn item(&mut self, item: &s::Item, depth: usize) -> CompileResult<()> {
            let old_binders = std::mem::take(&mut self.binders);
            let old_headers = std::mem::take(&mut self.headers);
            let old_trait = self.trait_scope;
            let old_impl = self.impl_scope;
            let old_mutable_life = self.mutable_life;
            let old_stored_fields = self.stored_fields;
            self.stored_fields = false;
            self.mutable_life = false;
            self.trait_scope |= matches!(item.kind, s::ItemKind::Trait(_));
            self.impl_scope |= matches!(item.kind, s::ItemKind::Impl(_));
            if let s::ItemKind::Aggregate(aggregate) = &item.kind {
                for parameter in &aggregate.generics {
                    if matches!(parameter.kind, s::GenericParamKind::Lifetime) {
                        self.budget
                            .node(parameter.span, depth)
                            .map_err(|e| vec![e])?;
                        self.budget
                            .text(&parameter.name, parameter.span)
                            .map_err(|e| vec![e])?;
                        for name in &self.binders {
                            self.budget
                                .text(name, parameter.span)
                                .map_err(|e| vec![e])?;
                            self.budget
                                .text(&parameter.name, parameter.span)
                                .map_err(|e| vec![e])?;
                        }
                        if self.binders.contains(&parameter.name) {
                            return Err(vec![failure(parameter.span, "duplicate life binder")]);
                        }
                        self.binders.push(parameter.name.clone());
                        self.stored_fields = true;
                        self.mutable_life = true;
                        self.budget.note_stored_lifetimes();
                    }
                }
            }
            if let s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } = &item.kind {
                self.mutable_life = only_life(f) || !self.impl_scope && copy_life(f);
                for p in &f.generics {
                    if matches!(p.kind, s::GenericParamKind::Lifetime) {
                        self.budget.node(p.span, depth).map_err(|e| vec![e])?;
                        self.budget.text(&p.name, p.span).map_err(|e| vec![e])?;
                        for _ in 0..self.binders.len() {
                            self.budget.text(&p.name, p.span).map_err(|e| vec![e])?;
                        }
                        if self.binders.contains(&p.name) {
                            return Err(vec![failure(p.span, "duplicate life binder")]);
                        }
                        self.binders.push(p.name.clone());
                    }
                }
                if !self.binders.is_empty()
                    && (self.trait_scope
                        || matches!(item.kind, s::ItemKind::Extern { .. })
                        || !(self.mutable_life || !self.impl_scope && copy_life(f))
                        || !f.where_clause.is_empty())
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Expand,
                        "life is supported only by ordinary direct functions and concrete inherent methods",
                        Some(f.span),
                    )]);
                }
                if !self.trait_scope && !matches!(item.kind, s::ItemKind::Extern { .. }) {
                    for ty in f.params.iter().map(|p| &p.ty).chain(f.return_type.iter()) {
                        self.budget.node(ty.span, depth).map_err(|e| vec![e])?;
                        self.headers.push(std::ptr::from_ref(reference(ty)));
                    }
                }
            }
            let result = ct_selection::walk_item(self, item, depth);
            self.binders = old_binders;
            self.headers = old_headers;
            self.trait_scope = old_trait;
            self.impl_scope = old_impl;
            self.mutable_life = old_mutable_life;
            self.stored_fields = old_stored_fields;
            result
        }
        fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(ty.kind, s::TypeKind::Dyn { .. }) {
                self.budget.note_dynamic();
            }
            if matches!(ty.kind, s::TypeKind::ForLifetime { .. }) {
                let old_binders = std::mem::take(&mut self.binders);
                let old_headers = std::mem::take(&mut self.headers);
                let old_mutable_life = self.mutable_life;
                self.mutable_life = true;
                let mut inner = ty;
                let mut rank = depth;
                while let s::TypeKind::ForLifetime {
                    lifetime,
                    inner: nested,
                } = &inner.kind
                {
                    self.budget.node(inner.span, rank).map_err(|e| vec![e])?;
                    self.budget
                        .text(lifetime, inner.span)
                        .map_err(|e| vec![e])?;
                    self.binders.push(lifetime.clone());
                    inner = nested;
                    rank += 1;
                }
                let (params, result): (&[s::TypeRef], &s::TypeRef) = match &inner.kind {
                    s::TypeKind::Function { params, result } => (params, result),
                    s::TypeKind::Named { path, args } if matches!(path.as_slice(), [name] if matches!(name.as_str(), "Fn"|"FnMut"|"FnOnce")) =>
                    {
                        let [s::GenericArg::Type(parameters), s::GenericArg::Type(result)] =
                            args.as_slice()
                        else {
                            return Err(vec![failure(
                                ty.span,
                                "higher-rank callable signature is incomplete",
                            )]);
                        };
                        let s::TypeKind::Tuple(params) = &reference(parameters).kind else {
                            return Err(vec![failure(
                                ty.span,
                                "higher-rank callable parameters require a tuple",
                            )]);
                        };
                        (params, result)
                    }
                    _ => {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Expand,
                            "higher-rank life requires a direct function or callable bound signature",
                            Some(ty.span),
                        )]);
                    }
                };
                for header in params.iter().chain(std::iter::once(result)) {
                    self.budget.node(header.span, rank).map_err(|e| vec![e])?;
                    self.headers.push(std::ptr::from_ref(reference(header)));
                }
                let result = ct_selection::walk_type(self, inner, rank);
                self.binders = old_binders;
                self.headers = old_headers;
                self.mutable_life = old_mutable_life;
                return result;
            }
            if let s::TypeKind::Reference {
                lifetime: Some(name),
                mutable,
                ..
            } = &ty.kind
            {
                for _ in 0..self.binders.len().saturating_add(1) {
                    self.budget.text(name, ty.span).map_err(|e| vec![e])?;
                }
                if !self.binders.contains(name) {
                    return Err(vec![failure(ty.span, "undeclared life name")]);
                }
                for _ in 0..self.headers.len() {
                    self.budget.text("h", ty.span).map_err(|e| vec![e])?;
                }
                if (*mutable && !self.mutable_life)
                    || (!self.stored_fields && !self.headers.contains(&std::ptr::from_ref(ty)))
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Expand,
                        "explicit life in this type position is not implemented",
                        Some(ty.span),
                    )]);
                }
            }
            ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut walk = Walk {
        budget,
        binders: Vec::new(),
        headers: Vec::new(),
        trait_scope: false,
        impl_scope: false,
        mutable_life: false,
        stored_fields: false,
    };
    for item in &module.items {
        ct_selection::Visitor::item(&mut walk, item, 0)?;
    }
    Ok(())
}

/// Lower only the retained lexical relation. This raw type is still checked
/// by Types and both independent CFG consumers before it grants any access.
pub(super) fn pointer_type(ty: &s::TypeRef) -> Type {
    fn reference(ty: &s::TypeRef) -> &s::TypeRef {
        match &ty.kind {
            s::TypeKind::Group(inner) => reference(inner),
            _ => ty,
        }
    }
    let mut names = Vec::new();
    let mut inner = ty;
    while let s::TypeKind::ForLifetime {
        lifetime,
        inner: nested,
    } = &inner.kind
    {
        names.push(lifetime.as_str());
        inner = nested;
    }
    let s::TypeKind::Function { params, result } = &inner.kind else {
        return Type::Unsupported("higher-rank life requires a direct function signature".into());
    };
    let tag = |ty: &s::TypeRef| match &reference(ty).kind {
        s::TypeKind::Reference {
            lifetime: Some(name),
            ..
        } => names.iter().rposition(|n| *n == name).map(|i| i as u32),
        _ => None,
    };
    let lower = |ty: &s::TypeRef| match &reference(ty).kind {
        s::TypeKind::Reference { mutable, inner, .. } => Type::Address {
            mutable: *mutable,
            inner: Box::new(super::type_ref(inner)),
        },
        _ => super::type_ref(ty),
    };
    let mut parameter_lives: Vec<_> = params.iter().map(tag).collect();
    let mut remap = vec![None; names.len()];
    let mut next = 0;
    let mut result_life = tag(result);
    for label in parameter_lives
        .iter_mut()
        .chain(std::iter::once(&mut result_life))
    {
        if let Some(old) = *label {
            let entry = &mut remap[old as usize];
            let dense = *entry.get_or_insert_with(|| {
                let dense = next;
                next += 1;
                dense
            });
            *label = Some(dense);
        }
    }
    if next == 0 {
        return Type::FunctionPointer {
            parameters: params.iter().map(lower).collect(),
            result: Box::new(lower(result)),
        };
    }
    Type::BorrowedFunctionPointer {
        parameters: params.iter().map(lower).collect(),
        result: Box::new(lower(result)),
        life: zil_ir::FunctionLifeContract {
            binder_count: next,
            parameter_lives,
            result_life,
        },
    }
}

pub(super) fn only_life(function: &s::Function) -> bool {
    !function.generics.is_empty()
        && function
            .generics
            .iter()
            .all(|p| matches!(p.kind, s::GenericParamKind::Lifetime))
}

/// The type namespace and the life namespace are independent. This finite
/// profile introduces exactly one ordinary Copy binder, never a callable,
/// StoreElement or unconstrained type parameter.
pub(super) fn copy_life(function: &s::Function) -> bool {
    let mut types = 0;
    let mut lives = 0;
    for parameter in &function.generics {
        match &parameter.kind {
            s::GenericParamKind::Lifetime => lives += 1,
            s::GenericParamKind::Type(bounds)
                if matches!(bounds.as_slice(), [s::TypeRef { kind: s::TypeKind::Named { path, args }, .. }]
                    if path.as_slice() == ["Copy"] && args.is_empty()) =>
            {
                types += 1
            }
            _ => return false,
        }
    }
    types == 1 && lives > 0
}

fn failure(span: Span, text: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-LIFE-DECLARATION", Some(span), text)
}

fn spend(remaining: &mut usize, work: usize, span: Span) -> Result<(), Diagnostic> {
    *remaining = remaining.checked_sub(work).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "life declaration budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

/// Called on the actual source header, after receiver selection but before any
/// Reference is lowered. None is the unchanged body-inferred legacy lane.
pub(super) fn contract(
    function: &s::Function,
    implicit_receiver: Option<&s::Param>,
    remaining: &mut usize,
) -> Result<Option<CoreLifeContract>, Diagnostic> {
    fn outer(ty: &s::TypeRef) -> Option<&str> {
        match &ty.kind {
            s::TypeKind::Group(inner) => outer(inner),
            s::TypeKind::Reference { lifetime, .. } => lifetime.as_deref(),
            _ => None,
        }
    }
    let parameters = implicit_receiver.into_iter().chain(function.params.iter());
    let present = function
        .generics
        .iter()
        .any(|p| matches!(p.kind, s::GenericParamKind::Lifetime))
        || parameters.clone().any(|p| outer(&p.ty).is_some())
        || function
            .return_type
            .as_ref()
            .is_some_and(|t| outer(t).is_some());
    if !present {
        return Ok(None);
    }
    let mutable_life = only_life(function) || copy_life(function);
    if !(mutable_life || copy_life(function)) || !function.where_clause.is_empty() {
        return Err(Diagnostic::unsupported(
            Phase::Expand,
            "life with type/callable/const parameters or where clauses is not implemented",
            Some(function.span),
        ));
    }
    spend(
        remaining,
        function
            .generics
            .len()
            .saturating_add(function.params.len())
            .saturating_add(2),
        function.span,
    )?;
    let mut binders = Vec::new();
    for p in function
        .generics
        .iter()
        .filter(|p| matches!(p.kind, s::GenericParamKind::Lifetime))
    {
        spend(
            remaining,
            p.name.len().saturating_mul(binders.len().saturating_add(2)),
            p.span,
        )?;
        if binders.contains(&p.name) {
            return Err(failure(p.span, "duplicate life binder"));
        }
        binders.push(p.name.clone());
    }
    fn tag(
        ty: &s::TypeRef,
        binders: &[String],
        remaining: &mut usize,
        mutable_life: bool,
    ) -> Result<Option<u32>, Diagnostic> {
        match &ty.kind {
            s::TypeKind::Group(inner) => tag(inner, binders, remaining, mutable_life),
            s::TypeKind::Reference {
                lifetime: Some(name),
                mutable,
                ..
            } => {
                spend(
                    remaining,
                    name.len().saturating_mul(binders.len().saturating_add(1)),
                    ty.span,
                )?;
                let index = binders
                    .iter()
                    .position(|b| b == name)
                    .ok_or_else(|| failure(ty.span, "undeclared life name"))?;
                if *mutable && !mutable_life {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "mutable life with ordinary type parameters is not implemented",
                        Some(ty.span),
                    ));
                }
                Ok(Some(index as u32))
            }
            _ => Ok(None),
        }
    }
    let parameter_lives = parameters
        .map(|p| tag(&p.ty, &binders, remaining, mutable_life))
        .collect::<Result<Vec<_>, _>>()?;
    let result_life = function
        .return_type
        .as_ref()
        .map(|t| tag(t, &binders, remaining, mutable_life))
        .transpose()?
        .flatten();
    if result_life.is_some_and(|result| !parameter_lives.contains(&Some(result))) {
        return Err(failure(
            function.span,
            "returned life must name a nonempty address input parameter set",
        ));
    }
    Ok(Some(CoreLifeContract {
        binders,
        parameter_lives,
        result_life,
    }))
}

/// Explicit labels may be erased only in the enclosing checked signature.
/// Other type positions retain an Unsupported marker instead of losing life.
pub(super) fn signature_type(expander: &Expander<'_>, ty: &s::TypeRef) -> Type {
    match &ty.kind {
        s::TypeKind::Group(inner) => signature_type(expander, inner),
        s::TypeKind::Reference {
            lifetime: Some(_),
            mutable,
            inner,
        } => Type::Address {
            mutable: *mutable,
            inner: Box::new(expander.type_ref(inner)),
        },
        _ => expander.type_ref(ty),
    }
}
