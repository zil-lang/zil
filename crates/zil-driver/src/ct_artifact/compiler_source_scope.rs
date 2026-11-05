//! Resolve original nodes by the exact SourceIndex traversal. Names and spans
//! never establish lexical ownership; node pointers are used only within one
//! immutable original AST and are never retained, serialized or reused.
use super::*;

pub(crate) struct OriginalItem<'a> {
    pub top: usize,
    pub declaration: zil_base::NodeId,
    pub item: &'a s::Item,
    pub generic_host: bool,
}
enum Selection<'a> {
    Declaration(zil_base::NodeId),
    Node(&'a s::Item),
}
impl Selection<'_> {
    fn matches(&self, id: zil_base::NodeId, item: &s::Item) -> bool {
        match self {
            Self::Declaration(target) => *target == id,
            Self::Node(target) => std::ptr::eq(*target, item),
        }
    }
}
fn select<'a>(
    module: &'a s::Module,
    selection: Selection<'_>,
    max_depth: usize,
    mut pay: impl FnMut(usize) -> R<()>,
) -> R<Option<OriginalItem<'a>>> {
    struct Walk<'a, F> {
        selection: Selection<'a>,
        next: u32,
        max_depth: usize,
        pay: F,
    }
    impl<F: FnMut(usize) -> R<()>> Walk<'_, F> {
        fn row(&mut self, span: Span) -> R<zil_base::NodeId> {
            (self.pay)(1)?;
            let id = zil_base::NodeId(self.next);
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| error(span, "CT original member ordinal overflow"))?;
            Ok(id)
        }
        fn children<'a>(
            &mut self,
            item: &'a s::Item,
            top: usize,
            generic_host: bool,
            depth: usize,
        ) -> R<Option<OriginalItem<'a>>> {
            (self.pay)(1)?;
            if depth > self.max_depth {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT original member depth exceeded",
                    Some(item.span),
                )]);
            }
            let members = match &item.kind {
                s::ItemKind::Aggregate(a) => {
                    for field in &a.fields {
                        self.row(field.span)?;
                    }
                    Some((&a.methods, generic_host || !a.generics.is_empty()))
                }
                s::ItemKind::Trait(t) => Some((&t.members, generic_host || !t.generics.is_empty())),
                s::ItemKind::Impl(i) => Some((&i.members, generic_host || !i.generics.is_empty())),
                s::ItemKind::Enum(e) => {
                    for variant in &e.variants {
                        self.row(variant.span)?;
                        match &variant.fields {
                            s::VariantFields::Unit => {}
                            s::VariantFields::Tuple(fields) => {
                                for field in fields {
                                    self.row(field.span)?;
                                }
                            }
                            s::VariantFields::Record(fields) => {
                                for field in fields {
                                    self.row(field.span)?;
                                }
                            }
                        }
                    }
                    None
                }
                _ => None,
            };
            if let Some((members, generic_host)) = members {
                for member in members {
                    let declaration = self.row(member.span)?;
                    if self.selection.matches(declaration, member) {
                        return Ok(Some(OriginalItem {
                            top,
                            declaration,
                            item: member,
                            generic_host,
                        }));
                    }
                    if let Some(found) = self.children(member, top, generic_host, depth + 1)? {
                        return Ok(Some(found));
                    }
                }
            }
            Ok(None)
        }
    }
    // Direct top-level ordinal access needs no whole-module scan.
    if let Selection::Declaration(id) = selection {
        pay(1)?;
        if let Some(item) = module.items.get(id.0 as usize) {
            return Ok(Some(OriginalItem {
                top: id.0 as usize,
                declaration: id,
                item,
                generic_host: false,
            }));
        }
    }
    let next = u32::try_from(module.items.len())
        .map_err(|_| error(module.span, "CT original declaration ordinal overflow"))?;
    let mut walk = Walk {
        selection,
        next,
        max_depth,
        pay,
    };
    for (top, item) in module.items.iter().enumerate() {
        if let Some(found) = walk.children(item, top, false, 0)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}
pub(crate) struct OriginalFunction<'a> {
    pub top: usize,
    pub item: &'a s::Item,
    pub function: &'a s::Function,
    pub generic_host: bool,
}
pub(crate) fn original_function<'a>(
    module: &'a s::Module,
    declaration: zil_base::NodeId,
    max_depth: usize,
    pay: impl FnMut(usize) -> R<()>,
) -> R<Option<OriginalFunction<'a>>> {
    let Some(original) = select(module, Selection::Declaration(declaration), max_depth, pay)?
    else {
        return Ok(None);
    };
    let function = match &original.item.kind {
        s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => f,
        _ => return Ok(None),
    };
    Ok(Some(OriginalFunction {
        top: original.top,
        item: original.item,
        function,
        generic_host: original.generic_host,
    }))
}

/// Select a retained original AST node, never its spelling or source span.
pub(crate) fn original_item<'a>(
    module: &'a s::Module,
    item: &'a s::Item,
    max_depth: usize,
    pay: impl FnMut(usize) -> R<()>,
) -> R<Option<OriginalItem<'a>>> {
    select(module, Selection::Node(item), max_depth, pay)
}

/// The caller owns an aligned clone of this original member vector. Its
/// ordinal remains authentic even when sibling spans are equal.
pub(crate) fn original_member(
    module: &s::Module,
    parent: zil_base::NodeId,
    ordinal: usize,
    max_depth: usize,
    mut pay: impl FnMut(usize) -> R<()>,
) -> R<Option<zil_base::NodeId>> {
    let Some(parent_item) = select(module, Selection::Declaration(parent), max_depth, &mut pay)?
    else {
        return Ok(None);
    };
    pay(1)?;
    let members = match &parent_item.item.kind {
        s::ItemKind::Aggregate(a) => &a.methods,
        s::ItemKind::Trait(t) => &t.members,
        s::ItemKind::Impl(i) => &i.members,
        _ => return Ok(None),
    };
    let Some(member) = members.get(ordinal) else {
        return Ok(None);
    };
    Ok(select(module, Selection::Node(member), max_depth, pay)?
        .map(|original| original.declaration))
}
