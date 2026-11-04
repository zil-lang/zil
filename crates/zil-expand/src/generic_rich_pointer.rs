//! Closed higher-rank signatures retain qualified classes in their invocation
//! domain. Keys are private scoped structural lookup, never type authority.
use super::*;
use zil_ir::{StorageBase, Type};

pub(crate) fn key(ty: &TypeRef, budget: &mut Budget<'_>) -> R<Option<String>> {
    if let TypeKind::Group(inner) = &ty.kind {
        budget.node(ty.span, 0)?;
        return key(inner, budget);
    }
    if !matches!(ty.kind, TypeKind::ForLifetime { .. }) {
        return Ok(None);
    }
    if !has_life(ty, budget)? {
        return Ok(None);
    }
    let mut names = Vec::new();
    let mut inner = ty;
    while let TypeKind::ForLifetime {
        lifetime,
        inner: next,
    } = &inner.kind
    {
        budget.node(inner.span, 0)?;
        budget.text(lifetime, inner.span)?;
        names.push(lifetime.as_str());
        inner = next;
    }
    if !matches!(inner.kind, TypeKind::Function { .. }) {
        return Err(unsupported(
            ty.span,
            "rich source code requires a function signature",
        ));
    }
    for _ in &names {
        budget.node(ty.span, 0)?;
    }
    let mut render = Key {
        budget,
        names,
        dense: vec![],
        next: 0,
        out: String::new(),
    };
    render.dense.resize(render.names.len(), None);
    render.text("B40:", ty.span)?;
    render.ty(inner, 0)?;
    Ok(Some(render.out))
}
struct Key<'a, 'b, 'l> {
    budget: &'a mut Budget<'l>,
    names: Vec<&'b str>,
    dense: Vec<Option<u32>>,
    next: u32,
    out: String,
}
impl Key<'_, '_, '_> {
    fn text(&mut self, value: &str, span: Span) -> R<()> {
        self.budget
            .identity_work(value.len().saturating_mul(2).saturating_add(1), span)?;
        self.out.push_str(value);
        Ok(())
    }
    fn number(&mut self, value: usize, span: Span) -> R<()> {
        self.budget.identity_work(
            if value == 0 {
                1
            } else {
                value.ilog10() as usize + 1
            },
            span,
        )?;
        self.text(&value.to_string(), span)
    }
    fn label(&mut self, name: Option<&str>, span: Span) -> R<()> {
        let mut found = None;
        if let Some(name) = name {
            for (index, candidate) in self.names.iter().enumerate().rev() {
                self.budget.node(span, 0)?;
                self.budget.text(name, span)?;
                self.budget.text(candidate, span)?;
                if *candidate == name {
                    found = Some(index);
                    break;
                }
            }
            if found.is_none() {
                return Err(unsupported(
                    span,
                    "free Fixed signature classes require their actual enclosing source context",
                ));
            }
        }
        let dense = if let Some(index) = found {
            if let Some(dense) = self.dense[index] {
                dense
            } else {
                let dense = self.next;
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or_else(|| error(span, "signature binder ordinal exhausted"))?;
                self.dense[index] = Some(dense);
                dense
            }
        } else {
            let dense = self.next;
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| error(span, "signature binder ordinal exhausted"))?;
            dense
        };
        self.number(dense as usize, span)
    }
    fn ty(&mut self, ty: &TypeRef, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Group(inner) => self.ty(inner, depth + 1),
            TypeKind::Reference {
                lifetime,
                mutable,
                inner,
            } => {
                self.text(if *mutable { "M" } else { "R" }, ty.span)?;
                self.label(lifetime.as_deref(), ty.span)?;
                self.text("<", ty.span)?;
                self.ty(inner, depth + 1)?;
                self.text(">", ty.span)
            }
            TypeKind::Named { path, args } => {
                if path.len() != 1 {
                    return Err(unsupported(
                        ty.span,
                        "rich code needs the actual resolved nominal symbol",
                    ));
                }
                self.text("N", ty.span)?;
                self.budget.identity_work(path.len(), ty.span)?;
                let bytes = path
                    .iter()
                    .try_fold(path.len().saturating_sub(1), |n, p| n.checked_add(p.len()))
                    .ok_or_else(|| error(ty.span, "signature key overflow"))?;
                self.number(bytes, ty.span)?;
                self.text(":", ty.span)?;
                for (i, name) in path.iter().enumerate() {
                    if i != 0 {
                        self.text(".", ty.span)?;
                    }
                    self.text(name, ty.span)?;
                }
                for arg in args {
                    self.budget.node(ty.span, depth)?;
                    self.text("[", ty.span)?;
                    match arg {
                        GenericArg::Lifetime(name) => {
                            self.text("L", ty.span)?;
                            self.label(Some(name), ty.span)?;
                        }
                        _ => {
                            return Err(unsupported(
                                ty.span,
                                "rich pointer nominal arguments currently require declared life actuals",
                            ));
                        }
                    }
                    self.text("]", ty.span)?;
                }
                Ok(())
            }
            TypeKind::Tuple(fields) => {
                self.text("(", ty.span)?;
                for field in fields {
                    self.ty(field, depth + 1)?;
                    self.text(",", ty.span)?;
                }
                self.text(")", ty.span)
            }
            TypeKind::Function { params, result } => {
                self.text("F(", ty.span)?;
                for p in params {
                    self.ty(p, depth + 1)?;
                    self.text(",", ty.span)?;
                }
                self.text(")->", ty.span)?;
                self.ty(result, depth + 1)
            }
            _ => Err(unsupported(
                ty.span,
                "rich pointer signature needs its complete scoped constructor",
            )),
        }
    }
}

impl Engine<'_> {
    pub(super) fn prepare_rich_pointer(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        let Some(key) = key(ty, &mut self.budget)? else {
            return Ok(false);
        };
        self.products
            .storage
            .pointer_lookup_fee(&key, &mut self.budget, ty.span)?;
        if self.products.storage.pointers.contains_key(&key) {
            return Ok(true);
        }
        self.budget.ty(ty)?;
        let mut owned = ty.clone();
        let mut scope = Vec::new();
        let mut inner = ty;
        loop {
            self.budget.node(inner.span, depth)?;
            match &inner.kind {
                TypeKind::Group(next) => inner = next,
                TypeKind::ForLifetime {
                    lifetime,
                    inner: next,
                } => {
                    self.budget.text(lifetime, inner.span)?;
                    scope.push(lifetime.clone());
                    inner = next;
                }
                _ => break,
            }
        }
        let Some((params, result)) = function_parts(&mut owned, &mut self.budget, depth)? else {
            return Err(unsupported(
                ty.span,
                "rich source code requires function parameters",
            ));
        };
        let old_scope = std::mem::replace(&mut self.life_scope, scope);
        let prepared = (|| {
            let mut occurrences = Vec::new();
            let mut stored = false;
            let mut next_elided = u32::try_from(self.life_scope.len())
                .map_err(|_| error(ty.span, "signature scope overflow"))?;
            for (index, parameter) in params.iter_mut().enumerate() {
                self.storage_header(
                    parameter,
                    StorageBase::Parameter(
                        u32::try_from(index)
                            .map_err(|_| error(ty.span, "signature parameter ordinal overflow"))?,
                    ),
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut occurrences,
                    &mut stored,
                    &mut next_elided,
                    depth + 1,
                )?;
            }
            self.storage_header(
                result,
                StorageBase::Result,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next_elided,
                depth + 1,
            )?;
            for occurrence in &mut occurrences {
                self.budget.node(ty.span, depth)?;
                occurrence.fixed_input = false;
            }
            let life = crate::stored_types::contract(occurrences, &mut self.budget, ty.span)?;
            let mut parameters = Vec::new();
            for parameter in params {
                parameters.push(self.products.storage.pointer_parameter_type(
                    parameter,
                    &self.nominals,
                    &mut self.budget,
                    depth + 1,
                )?);
            }
            let result = Box::new(self.products.storage.pointer_parameter_type(
                result,
                &self.nominals,
                &mut self.budget,
                depth + 1,
            )?);
            let mut class_cursor = 0;
            for (index, parameter) in parameters.iter().enumerate() {
                check_classes(
                    parameter,
                    StorageBase::Parameter(index as u32),
                    &mut Vec::new(),
                    &life,
                    &mut class_cursor,
                    &mut self.budget,
                    ty.span,
                    0,
                )?;
            }
            check_classes(
                &result,
                StorageBase::Result,
                &mut Vec::new(),
                &life,
                &mut class_cursor,
                &mut self.budget,
                ty.span,
                0,
            )?;
            if class_cursor != life.classes.len() {
                return Err(error(
                    ty.span,
                    "rich signature has an unmatched qualified class",
                ));
            }
            Ok(Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            })
        })();
        self.life_scope = old_scope;
        let prepared = prepared?;
        self.products
            .storage
            .pointer_lookup_fee(&key, &mut self.budget, ty.span)?;
        self.products.storage.pointers.insert(key, prepared);
        Ok(true)
    }
}

fn function_parts<'a>(
    ty: &'a mut TypeRef,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<Option<(&'a mut Vec<TypeRef>, &'a mut Box<TypeRef>)>> {
    budget.node(ty.span, depth)?;
    match &mut ty.kind {
        TypeKind::ForLifetime { inner, .. } | TypeKind::Group(inner) => {
            function_parts(inner, budget, depth + 1)
        }
        TypeKind::Function { params, result } => Ok(Some((params, result))),
        _ => Ok(None),
    }
}

/// A nested scoped signature must never reach the contextless recursive type
/// renderer. Constructors without a contextual adapter fail before fallback.
pub(crate) fn reject_unhandled(
    ty: &TypeRef,
    budget: &mut Budget<'_>,
    instances: &[crate::stored_types::Instance],
) -> R<()> {
    use crate::ct_selection::Visitor;
    struct Check<'a, 's, 'l> {
        budget: &'a mut Budget<'l>,
        instances: &'s [crate::stored_types::Instance],
        forall: bool,
    }
    impl Visitor for Check<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(ty.kind, TypeKind::ForLifetime { .. })
                && key(ty, self.budget).map_err(|e| vec![e])?.is_some()
            {
                return Err(vec![unsupported(
                    ty.span,
                    "rich Code inside this constructor needs its contextual signature adapter",
                )]);
            }
            if self.forall
                && let TypeKind::Named { path, args } = &ty.kind
                && path.len() == 1
                && args.is_empty()
            {
                // An actual substituted qualified type can already have lost
                // its source life-argument spelling. The real catalogue, never
                // a generated-name prefix, identifies that boundary.
                for instance in self.instances {
                    self.node(ty.span, depth)?;
                    if let Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                        && let Type::Record { name, .. } = storage.as_ref()
                    {
                        self.text(name, ty.span)?;
                        self.text(&path[0], ty.span)?;
                        if name == &path[0] {
                            return Err(vec![unsupported(
                                ty.span,
                                "qualified type in a legacy Code signature needs its actual Fixed context",
                            )]);
                        }
                    }
                }
            }
            let old = self.forall;
            self.forall |= matches!(ty.kind, TypeKind::ForLifetime { .. });
            let result = crate::ct_selection::walk_type(self, ty, depth);
            self.forall = old;
            result
        }
    }
    Check {
        budget,
        instances,
        forall: false,
    }
    .ty(ty, 0)
    .map_err(|mut e| e.remove(0))
}

#[allow(clippy::too_many_arguments)]
fn check_classes(
    ty: &Type,
    base: StorageBase,
    path: &mut Vec<zil_ir::LifeTypeProjection>,
    life: &zil_ir::StorageLifeContract,
    cursor: &mut usize,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<()> {
    use zil_ir::{LifeTypeProjection, PlaceProjection};
    budget.node(span, depth)?;
    match ty {
        Type::BorrowedStorage { life: shape, .. } => {
            for class in 0..shape.binder_count {
                budget.node(span, depth)?;
                let row = life
                    .classes
                    .get(*cursor)
                    .ok_or_else(|| error(span, "rich signature omits a qualified class"))?;
                budget.identity_work(
                    path.len()
                        .saturating_add(row.position.path.len())
                        .saturating_add(1),
                    span,
                )?;
                if row.position.base != base
                    || row.position.path != *path
                    || row.position.class != class
                {
                    return Err(error(
                        span,
                        "rich signature qualified class differs from actual boundary",
                    ));
                }
                *cursor += 1;
            }
        }
        Type::Address { inner, .. } => {
            path.push(LifeTypeProjection::Pointee);
            check_classes(inner, base, path, life, cursor, budget, span, depth + 1)?;
            path.pop();
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                let index = u32::try_from(index)
                    .map_err(|_| error(span, "signature field ordinal exhausted"))?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(index)));
                check_classes(field, base, path, life, cursor, budget, span, depth + 1)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

fn has_life(ty: &TypeRef, budget: &mut Budget<'_>) -> R<bool> {
    use crate::ct_selection::Visitor;
    struct Find<'a, 'l> {
        budget: &'a mut Budget<'l>,
        found: bool,
    }
    impl Visitor for Find<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if let TypeKind::Named { args, .. } = &ty.kind {
                for argument in args {
                    self.node(ty.span, depth)?;
                    self.found |= matches!(argument, GenericArg::Lifetime(_));
                }
            }
            crate::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut find = Find {
        budget,
        found: false,
    };
    find.ty(ty, 0).map_err(|mut e| e.remove(0))?;
    Ok(find.found)
}

/// Convert an actual checked callable protocol header to the same scoped
/// signature recipe used by Code. CallKind remains in its owning role.
pub(crate) fn role_code(bound: &TypeRef, budget: &mut Budget<'_>) -> R<Option<TypeRef>> {
    if !matches!(grouped(bound).kind, TypeKind::ForLifetime { .. }) || !has_life(bound, budget)? {
        return Ok(None);
    }
    budget.ty(bound)?;
    let mut code = bound.clone();
    fn convert(ty: &mut TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<()> {
        budget.node(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::ForLifetime { inner, .. } | TypeKind::Group(inner) => {
                convert(inner, budget, depth + 1)
            }
            TypeKind::Named { path, args } if matches!(path.as_slice(),[name] if matches!(name.as_str(),"Fn"|"FnMut"|"FnOnce")) =>
            {
                let [GenericArg::Type(parameters), GenericArg::Type(result)] = args.as_mut_slice()
                else {
                    return Err(error(ty.span, "rich role requires its complete signature"));
                };
                let TypeKind::Tuple(params) = &mut parameters.kind else {
                    return Err(error(ty.span, "rich role parameters require a tuple"));
                };
                let params = std::mem::take(params);
                budget.node(result.span, depth)?;
                let result = Box::new(std::mem::replace(result, check::unit(result.span)));
                ty.kind = TypeKind::Function { params, result };
                Ok(())
            }
            _ => Err(unsupported(
                ty.span,
                "rich role requires its actual callable protocol",
            )),
        }
    }
    convert(&mut code, budget, 0)?;
    Ok(Some(code))
}
