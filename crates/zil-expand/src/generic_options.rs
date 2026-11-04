//! Option syntax enters the existing enum specialization path with a sealed
//! compiler-only template. The generated enum remains ordinary untrusted Core.
use super::*;
use crate::generated_spans::{self, Visitor as _};
pub(super) const TEMPLATE: &str = "$builtinOption340";

struct Probe<'a> {
    budget: Budget<'a>,
    present: bool,
}
impl crate::ct_selection::Visitor for Probe<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.node(span, depth).map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|e| vec![e])
    }
    fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
        if matches!(ty.kind, TypeKind::Optional(_))
            || matches!(&ty.kind, TypeKind::Named{path,..} if path.as_slice()==["Option"])
        {
            self.present = true;
        }
        crate::ct_selection::walk_type(self, ty, depth)
    }
    fn expression(&mut self, e: &Expr, depth: usize) -> CompileResult<()> {
        self.present |= matches!(&e.kind, ExprKind::Name(name) if name=="Option");
        crate::ct_selection::walk_expression(self, e, depth)
    }
}
struct Normalize<'a> {
    budget: Budget<'a>,
}
impl generated_spans::Visitor for Normalize<'_> {
    fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
        self.budget.node(*span, depth).map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|e| vec![e])
    }
    fn ty(&mut self, ty: &mut TypeRef, depth: usize) -> CompileResult<()> {
        if let TypeKind::Optional(inner) = &mut ty.kind {
            self.budget.ty(inner).map_err(|e| vec![e])?;
            ty.kind = TypeKind::Named {
                path: vec![TEMPLATE.into()],
                args: vec![GenericArg::Type(*inner.clone())],
            };
        } else if let TypeKind::Named { path, .. } = &mut ty.kind
            && path.as_slice() == ["Option"]
        {
            path[0] = TEMPLATE.into();
        }
        generated_spans::walk_type(self, ty, depth)
    }
    fn expression(&mut self, e: &mut Expr, depth: usize) -> CompileResult<()> {
        if let ExprKind::Name(name) = &mut e.kind
            && name == "Option"
        {
            *name = TEMPLATE.into();
        }
        generated_spans::walk_expression(self, e, depth)
    }
}

pub(super) fn prepare(module: &Module, limits: &Limits) -> CompileResult<(Option<Module>, Limits)> {
    let mut probe = Probe {
        budget: Budget::new(limits),
        present: false,
    };
    for item in &module.items {
        let reserved = match &item.kind {
            ItemKind::Aggregate(a) => a.name == "Option",
            ItemKind::Enum(e) => e.name == "Option",
            ItemKind::TypeAlias(a) => a.name == "Option",
            ItemKind::Trait(t) => t.name == "Option",
            _ => false,
        };
        if reserved {
            return Err(vec![error(
                item.span,
                "Option is a reserved built-in type family",
            )]);
        }
        crate::ct_selection::Visitor::item(&mut probe, item, 0)?;
    }
    let remaining = probe.budget.remaining_limits();
    if !probe.present {
        return Ok((None, remaining));
    }
    // Clone only after a complete prepaid borrowed scan; normalize with the
    // same remaining ledger, then ordinary specialization pays its own work.
    let mut output = module.clone();
    let mut normal = Normalize {
        budget: probe.budget,
    };
    for item in &mut output.items {
        normal.item(item, 0)?;
    }
    let span = module.span;
    let item = Item {
        public: false,
        attributes: vec![],
        span,
        kind: ItemKind::Enum(EnumDecl {
            name: TEMPLATE.into(),
            generics: vec![GenericParam {
                name: "T".into(),
                kind: GenericParamKind::Type(vec![]),
                span,
            }],
            variants: vec![
                Variant {
                    name: "None".into(),
                    fields: VariantFields::Unit,
                    attributes: vec![],
                    span,
                },
                Variant {
                    name: "Some".into(),
                    fields: VariantFields::Tuple(vec![VariantTupleField {
                        ty: named("T", span),
                        manages: false,
                        span,
                    }]),
                    attributes: vec![],
                    span,
                },
            ],
        }),
    };
    normal.budget.item(&item).map_err(|e| vec![e])?;
    output.items.insert(0, item);
    Ok((Some(output), normal.budget.remaining_limits()))
}
