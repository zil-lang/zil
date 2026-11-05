//! Capture from actual original syntax before a transforming producer moves it.
//! The resulting rows name declarations; they carry no safety authority.
use super::*;
use crate::ct_selection::Visitor;
use crate::generics::origins::Origins;
use zil_base::{CompileResult, Limits};
use zil_syntax::ast::{self as s, Module};

struct Discovery<'a, 'l>(&'a mut Budget<'l>);
impl Visitor for Discovery<'_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.0.node(span, depth).map_err(|error| vec![error])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.0.text(text, span).map_err(|error| vec![error])
    }
    fn item(&mut self, item: &s::Item, depth: usize) -> CompileResult<()> {
        if matches!(item.kind, s::ItemKind::Meta(s::MetaItem::Emit(_))) {
            // A future ItemList can introduce nominal/life declarations. Its
            // source namespace must be frozen before the first event runs.
            self.0.note_source_identity();
        }
        if let s::ItemKind::Function(function) = &item.kind
            && function.generics.len() >= 3
        {
            // Full CT factory instances need authentic declaration identities
            // even when no nominal type stores a life. This is inventory
            // discovery only; it does not authorize any CT operation or member.
            let (mut types, mut scalars, mut lives) = (false, false, false);
            for parameter in &function.generics {
                self.node(parameter.span, depth)?;
                match parameter.kind {
                    s::GenericParamKind::Type(_) => types = true,
                    s::GenericParamKind::Const(_) => scalars = true,
                    s::GenericParamKind::Lifetime => lives = true,
                }
            }
            if types && scalars && lives {
                self.0.note_source_identity();
            }
        }
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(aggregate) => aggregate.generics.as_slice(),
            s::ItemKind::Enum(enumeration) => enumeration.generics.as_slice(),
            _ => &[],
        };
        for parameter in parameters {
            self.node(parameter.span, depth)?;
            if matches!(parameter.kind, s::GenericParamKind::Lifetime) {
                self.0.note_source_identity();
            }
        }
        crate::ct_selection::walk_item(self, item, depth)
    }
    fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
        if matches!(ty.kind, s::TypeKind::Dyn { .. }) {
            self.0.note_dynamic();
        }
        crate::ct_selection::walk_type(self, ty, depth)
    }
}

pub(crate) fn capture_original_impl(
    sources: &[SourceUnit],
    modules: &[&Module],
    context: SourceIdentityContext<'_>,
    limits: &Limits,
) -> CompileResult<(Option<Vec<ModuleDeclarationOrigins>>, Limits)> {
    capture_impl(sources, modules, context, limits, false)
}

pub(crate) fn capture_required_impl(
    sources: &[SourceUnit],
    modules: &[&Module],
    context: SourceIdentityContext<'_>,
    limits: &Limits,
) -> CompileResult<(Vec<ModuleDeclarationOrigins>, Limits)> {
    capture_impl(sources, modules, context, limits, true).map(|(rows, remaining)| {
        (
            rows.expect("forced original capture always constructs a namespace"),
            remaining,
        )
    })
}

fn capture_impl(
    sources: &[SourceUnit],
    modules: &[&Module],
    context: SourceIdentityContext<'_>,
    limits: &Limits,
    required: bool,
) -> CompileResult<(Option<Vec<ModuleDeclarationOrigins>>, Limits)> {
    let mut budget = Budget::new(limits);
    if required {
        budget.note_source_identity();
    }
    let result =
        (|| {
            if !matches!(
                context,
                SourceIdentityContext::ClosedSnapshot {
                    declarations: DeclarationPlacement::Original,
                    ..
                } | SourceIdentityContext::Explicit {
                    declarations: DeclarationPlacement::Original,
                    ..
                }
            ) {
                return Err(bad(
                    Span::default(),
                    "capture requires actual original declarations",
                ));
            }
            if sources.len() != modules.len() {
                return Err(bad(Span::default(), "original source/module count differs"));
            }
            // This paid traversal also discovers whether an output consumer needs
            // nominal identity. Feature-free inputs avoid constructing a namespace.
            for (source, module) in sources.iter().zip(modules) {
                budget.node(module.span, 0)?;
                if source.module_id != module.id {
                    return Err(bad(module.span, "original source/module owner differs"));
                }
                for item in &module.items {
                    Discovery(&mut budget)
                        .item(item, 0)
                        .map_err(|mut errors| errors.remove(0))?;
                }
            }
            if !budget.needs_source_identity() && !budget.has_stored_lifetimes() {
                return Ok(None);
            }
            let resolved = context.resolve(sources, &mut budget)?;
            let mut captured = Vec::new();
            for module in modules {
                budget.node(module.span, 0)?;
                let mut origins = Origins::single(module, false, &mut budget)?;
                origins.capture_identities(module, false, &resolved, &mut budget)?;
                budget.lineage_bytes(
                    module
                        .items
                        .len()
                        .saturating_mul(std::mem::size_of::<DeclarationOrigin>()),
                    module.span,
                )?;
                let mut items = Vec::with_capacity(module.items.len());
                for (ordinal, item) in module.items.iter().enumerate() {
                    budget.node(item.span, 0)?;
                    let original = origins.get(ordinal, item.span)?.declaration;
                    // Moving the captured row avoids a second clone of every
                    // package/module/name string. Prepay the actual map removal.
                    lookup_work(origins.identities.len(), &mut budget, item.span)?;
                    items.push(origins.identities.remove(&original).ok_or_else(|| {
                        bad(item.span, "captured original declaration is absent")
                    })?);
                }
                captured.push(ModuleDeclarationOrigins {
                    module: module.id,
                    items,
                });
            }
            Ok(Some(captured))
        })();
    result
        .map(|captured| (captured, budget.remaining_limits()))
        .map_err(|error| vec![error])
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ResultCategory};

    #[test]
    fn original_capture_keeps_actual_rows_and_pays_discovery() {
        let source = SourceUnit::new(
            FileId(12),
            ModuleId(7),
            "irrelevant/native/path",
            "fn ct_only(){} struct View<life a>{p:&'a i64} fn main(){}",
        );
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let aliases = BTreeMap::new();
        let context = SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        };
        let limits = Limits::default();
        let (captured, left) = capture_original_impl(
            std::slice::from_ref(&source),
            &[&parsed.module],
            context,
            &limits,
        )
        .unwrap();
        let rows = &captured.unwrap()[0].items;
        assert_eq!(
            rows.iter().map(|row| row.declaration.0).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(rows[1].identity.declaration_path, ["struct", "View"]);
        assert!(left.max_analysis_steps < limits.max_analysis_steps);
        assert!(left.max_nodes < limits.max_nodes);
        assert_eq!(
            limits.max_analysis_steps,
            Limits::default().max_analysis_steps
        );
        for nodes in [false, true] {
            let boundary = if nodes {
                limits.max_nodes - left.max_nodes
            } else {
                limits.max_analysis_steps - left.max_analysis_steps
            };
            let mut tight = limits.clone();
            if nodes {
                tight.max_nodes = boundary;
            } else {
                tight.max_analysis_steps = boundary;
            }
            assert!(
                capture_original_impl(
                    std::slice::from_ref(&source),
                    &[&parsed.module],
                    context,
                    &tight
                )
                .is_ok()
            );
            if nodes {
                tight.max_nodes -= 1;
            } else {
                tight.max_analysis_steps -= 1;
            }
            assert_eq!(
                capture_original_impl(
                    std::slice::from_ref(&source),
                    &[&parsed.module],
                    context,
                    &tight
                )
                .unwrap_err()[0]
                    .category,
                ResultCategory::ResourceLimit
            );
            println!(
                "original-capture {}={boundary}",
                if nodes { "nodes" } else { "work" }
            );
        }
    }
}
