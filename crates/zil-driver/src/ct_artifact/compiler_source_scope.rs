//! Resolve an original function by the exact SourceIndex traversal ordinal.
//! No name or Span lookup establishes lexical ownership.
use super::*;

pub(crate) struct OriginalItem<'a> {
    pub top: usize,
    pub item: &'a s::Item,
    pub generic_host: bool,
}

pub(crate) fn original_item<'a>(
    module: &'a s::Module,
    declaration: zil_base::NodeId,
    max_depth: usize,
    mut pay: impl FnMut(usize) -> R<()>,
) -> R<Option<OriginalItem<'a>>> {
    struct Walk<'a, F> {
        target: u32,
        next: u32,
        max_depth: usize,
        pay: &'a mut F,
    }
    impl<F: FnMut(usize) -> R<()>> Walk<'_, F> {
        fn row(&mut self, span: Span) -> R<bool> {
            (self.pay)(1)?;
            let found = self.next == self.target;
            self.next = self.next.checked_add(1)
                .ok_or_else(|| error(span, "CT original member ordinal overflow"))?;
            Ok(found)
        }
        fn children<'a>(&mut self, item: &'a s::Item, top: usize, generic_host: bool, depth: usize) -> R<Option<OriginalItem<'a>>> {
            (self.pay)(1)?;
            if depth > self.max_depth {
                return Err(vec![Diagnostic::limit(Phase::Ct, "CT original member depth exceeded", Some(item.span))]);
            }
            let members = match &item.kind {
                s::ItemKind::Aggregate(a) => {
                    for field in &a.fields { self.row(field.span)?; }
                    Some((&a.methods, generic_host || !a.generics.is_empty()))
                }
                s::ItemKind::Trait(t) => Some((&t.members, generic_host || !t.generics.is_empty())),
                s::ItemKind::Impl(i) => Some((&i.members, generic_host || !i.generics.is_empty())),
                s::ItemKind::Enum(e) => {
                    for variant in &e.variants {
                        self.row(variant.span)?;
                        match &variant.fields {
                            s::VariantFields::Unit => {},
                            s::VariantFields::Tuple(fields) => { for field in fields { self.row(field.span)?; } },
                            s::VariantFields::Record(fields) => { for field in fields { self.row(field.span)?; } },
                        }
                    }
                    None
                }
                _ => None,
            };
            if let Some((members, generic_host)) = members {
                for member in members {
                    if self.row(member.span)? {
                        return Ok(Some(OriginalItem { top, item: member, generic_host }));
                    }
                    if let Some(found) = self.children(member, top, generic_host, depth + 1)? { return Ok(Some(found)); }
                }
            }
            Ok(None)
        }
    }
    pay(1)?;
    if let Some(item) = module.items.get(declaration.0 as usize) {
        return Ok(Some(OriginalItem { top: declaration.0 as usize, item, generic_host: false }));
    }
    let next = u32::try_from(module.items.len()).map_err(|_| error(module.span, "CT original declaration ordinal overflow"))?;
    let mut walk = Walk { target: declaration.0, next, max_depth, pay: &mut pay };
    for (top, item) in module.items.iter().enumerate() {
        if let Some(found) = walk.children(item, top, false, 0)? { return Ok(Some(found)); }
    }
    Ok(None)
}


pub(crate) struct OriginalFunction<'a> {
    pub top: usize,
    pub item: &'a s::Item,
    pub function: &'a s::Function,
    pub generic_host: bool,
}
pub(crate) fn original_function<'a>(module: &'a s::Module, declaration: zil_base::NodeId, max_depth: usize, pay: impl FnMut(usize) -> R<()>) -> R<Option<OriginalFunction<'a>>> {
    let Some(original) = original_item(module, declaration, max_depth, pay)? else { return Ok(None) };
    let function = match &original.item.kind {
        s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => f,
        _ => return Ok(None),
    };
    Ok(Some(OriginalFunction { top: original.top, item: original.item, function, generic_host: original.generic_host }))
}

/// Select the actual original child of an already selected declaration. The
/// caller owns an aligned clone of that member vector, so its original ordinal
/// is retained even when member spans repeat.
pub(crate) fn original_member(module: &s::Module, parent: zil_base::NodeId, ordinal: usize, max_depth: usize, mut pay: impl FnMut(usize) -> R<()>) -> R<Option<zil_base::NodeId>> {
    let Some(parent_item) = original_item(module, parent, max_depth, &mut pay)? else { return Ok(None) };
    let members = match &parent_item.item.kind {
        s::ItemKind::Aggregate(a) => &a.methods,
        s::ItemKind::Trait(t) => &t.members,
        s::ItemKind::Impl(i) => &i.members,
        _ => return Ok(None),
    };
    let Some(member) = members.get(ordinal) else { return Ok(None) };
    // A fresh paid walk resolves the actual node pointer in THIS immutable AST,
    // never in a cloned or serialized tree. No pointer is retained or exported.
    let mut next = u32::try_from(module.items.len()).map_err(|_| error(module.span, "CT member inventory overflow"))?;
    fn children(item: &s::Item, target: &s::Item, next: &mut u32, depth: usize, max_depth: usize, pay: &mut impl FnMut(usize) -> R<()>) -> R<Option<zil_base::NodeId>> {
        pay(1)?;
        if depth > max_depth { return Err(vec![Diagnostic::limit(Phase::Ct, "CT original member depth exceeded", Some(item.span))]); }
        let (prefix, members) = match &item.kind {
            s::ItemKind::Aggregate(a) => (a.fields.len(), Some(&a.methods)),
            s::ItemKind::Trait(t) => (0, Some(&t.members)),
            s::ItemKind::Impl(i) => (0, Some(&i.members)),
            s::ItemKind::Enum(e) => {
                let mut rows = 0usize;
                for variant in &e.variants {
                    pay(1)?;
                    rows = rows.saturating_add(1).saturating_add(match &variant.fields { s::VariantFields::Unit => 0, s::VariantFields::Tuple(f) => f.len(), s::VariantFields::Record(f) => f.len() });
                }
                (rows, None)
            }
            _ => (0, None),
        };
        pay(prefix)?;
        *next = next.checked_add(u32::try_from(prefix).map_err(|_| error(item.span, "CT member count overflow"))?).ok_or_else(|| error(item.span, "CT member inventory overflow"))?;
        if let Some(members) = members {
            for member in members {
                pay(1)?;
                let id = zil_base::NodeId(*next);
                *next = next.checked_add(1).ok_or_else(|| error(item.span, "CT member inventory overflow"))?;
                if std::ptr::eq(member, target) { return Ok(Some(id)); }
                if let Some(id) = children(member, target, next, depth + 1, max_depth, pay)? { return Ok(Some(id)); }
            }
        }
        Ok(None)
    }
    for item in &module.items {
        if let Some(id) = children(item, member, &mut next, 0, max_depth, &mut pay)? { return Ok(Some(id)); }
    }
    Ok(None)
}
