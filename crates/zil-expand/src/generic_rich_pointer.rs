//! Closed higher-rank signatures retain qualified classes in their invocation
//! domain. Keys are private scoped structural lookup, never type authority.
use super::*;
use zil_ir::{StorageBase, Type};

pub(crate) fn key(ty: &TypeRef, budget: &mut Budget<'_>) -> R<Option<String>> {
    if !matches!(ty.kind, TypeKind::ForLifetime { .. }) {
        return Ok(None);
    }
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
    if !find.found {
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
        while let TypeKind::ForLifetime {
            lifetime,
            inner: next,
        } = &inner.kind
        {
            self.budget.node(inner.span, depth)?;
            self.budget.text(lifetime, inner.span)?;
            scope.push(lifetime.clone());
            inner = next;
        }
        let Some((params, result)) = function_parts(&mut owned) else {
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

fn function_parts(ty: &mut TypeRef) -> Option<(&mut Vec<TypeRef>, &mut Box<TypeRef>)> {
    match &mut ty.kind {
        TypeKind::ForLifetime { inner, .. } => function_parts(inner),
        TypeKind::Function { params, result } => Some((params, result)),
        _ => None,
    }
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
