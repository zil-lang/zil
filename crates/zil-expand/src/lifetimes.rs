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
    }
    fn reference(ty: &s::TypeRef) -> &s::TypeRef {
        match &ty.kind {
            s::TypeKind::Group(inner) => reference(inner),
            _ => ty,
        }
    }
    impl ct_selection::Visitor for Walk<'_, '_> {
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
            self.trait_scope |= matches!(item.kind, s::ItemKind::Trait(_));
            if let s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } = &item.kind {
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
                        || !only_life(f)
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
            result
        }
        fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
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
                if *mutable || !self.headers.contains(&std::ptr::from_ref(ty)) {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Expand,
                        "explicit life in this type position is not implemented",
                        Some(ty.span),
                    )]);
                }
            }
            if matches!(ty.kind, s::TypeKind::ForLifetime { .. }) {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Expand,
                    "higher-rank life is not implemented",
                    Some(ty.span),
                )]);
            }
            ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut walk = Walk {
        budget,
        binders: Vec::new(),
        headers: Vec::new(),
        trait_scope: false,
    };
    for item in &module.items {
        ct_selection::Visitor::item(&mut walk, item, 0)?;
    }
    Ok(())
}

pub(super) fn only_life(function: &s::Function) -> bool {
    !function.generics.is_empty()
        && function
            .generics
            .iter()
            .all(|p| matches!(p.kind, s::GenericParamKind::Lifetime))
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
    if function
        .generics
        .iter()
        .any(|p| !matches!(p.kind, s::GenericParamKind::Lifetime))
        || !function.where_clause.is_empty()
    {
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
    for p in &function.generics {
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
    ) -> Result<Option<u32>, Diagnostic> {
        match &ty.kind {
            s::TypeKind::Group(inner) => tag(inner, binders, remaining),
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
                if *mutable {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "explicit life on mutable addresses is not implemented",
                        Some(ty.span),
                    ));
                }
                Ok(Some(index as u32))
            }
            _ => Ok(None),
        }
    }
    let parameter_lives = parameters
        .map(|p| tag(&p.ty, &binders, remaining))
        .collect::<Result<Vec<_>, _>>()?;
    let result_life = function
        .return_type
        .as_ref()
        .map(|t| tag(t, &binders, remaining))
        .transpose()?
        .flatten();
    if result_life.is_some_and(|result| !parameter_lives.contains(&Some(result))) {
        return Err(failure(
            function.span,
            "returned life must name a nonempty shared input parameter set",
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
