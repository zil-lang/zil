//! Request-local normalized syntax ownership. Deferred is a sealed recipe,
//! never an Item/Deref and never a restored body pretending to be normalized.
use super::*;
use crate::incremental::{NormalizedRecipe, Reuse, Stage};

/// A successful restored view is syntax ownership, never a new stage receipt.
/// Copies of the lazy owner share one eventual AST, including when a checkpoint
/// or placement candidate is the first consumer that needs ordinary syntax.
#[derive(Clone)]
pub(crate) enum RestoredProduct {
    Shared(Arc<Item>),
    View(Arc<RestoredView>),
}
pub(crate) struct RestoredView {
    body: Arc<super::scalar_walk::BodyView>,
    item: std::sync::OnceLock<Arc<Item>>,
}
impl RestoredProduct {
    #[cfg(test)]
    pub(crate) fn is_materialized(&self) -> bool {
        match self {
            Self::Shared(_) => true,
            Self::View(view) => view.item.get().is_some(),
        }
    }
    pub(crate) fn view(body: Arc<super::scalar_walk::BodyView>) -> Self {
        Self::View(Arc::new(RestoredView {
            body,
            item: std::sync::OnceLock::new(),
        }))
    }
    pub(crate) fn materialize(&self) -> Arc<Item> {
        match self {
            Self::Shared(item) => item.clone(),
            Self::View(view) => view
                .item
                .get_or_init(|| Arc::new(view.body.materialize()))
                .clone(),
        }
    }
}

pub(crate) struct RestoredModuleView {
    id: ModuleId,
    span: Span,
    items: Vec<RestoredProduct>,
    imports: Vec<CoreImport>,
    module: std::sync::OnceLock<Arc<SharedScalarModule>>,
}
impl RestoredModuleView {
    #[cfg(test)]
    pub(crate) fn is_materialized(&self) -> bool {
        self.module.get().is_some()
    }
    pub(crate) fn new(
        id: ModuleId,
        span: Span,
        items: Vec<RestoredProduct>,
        imports: Vec<CoreImport>,
    ) -> Self {
        Self {
            id,
            span,
            items,
            imports,
            module: std::sync::OnceLock::new(),
        }
    }
    pub(crate) fn materialize(&self) -> Arc<SharedScalarModule> {
        self.module
            .get_or_init(|| {
                Arc::new(SharedScalarModule {
                    id: self.id,
                    span: self.span,
                    items: self
                        .items
                        .iter()
                        .map(RestoredProduct::materialize)
                        .collect::<Vec<_>>()
                        .into(),
                    imports: self.imports.clone(),
                })
            })
            .clone()
    }
}

#[derive(Clone)]
pub(crate) enum Declaration {
    Function(String),
    Extern,
}
impl Declaration {
    pub(super) fn from_item(item: &Item) -> Self {
        match &item.kind {
            ItemKind::Function(function) => Self::Function(function.name.clone()),
            ItemKind::Extern { .. } => Self::Extern,
            _ => unreachable!("sealed scalar normalized declaration"),
        }
    }
    fn function_name(&self) -> Option<&str> {
        match self {
            Self::Function(name) => Some(name),
            Self::Extern => None,
        }
    }
}

// Keep the cold Item inline so Normalize can move it into Restore without
// allocating another per-declaration owner. This request-local table is bounded.
#[allow(clippy::large_enum_variant)]
pub(crate) enum NormalizedProduct<'a> {
    View(Arc<super::scalar_walk::BodyView>),
    Owned(Item),
    Shared(Arc<Item>),
    Deferred {
        recipe: Arc<NormalizedRecipe>,
        // This request's allowance immediately before successful Normalize
        // replay, never a later Probe/Lifetime/Header/Restore remainder.
        allowance: Limits,
        // The same immutable Index used by this request's normalizer. It binds
        // all current imports and declarations; no old Index is reconstructed.
        index: &'a Index,
        target_pointer_width: u16,
    },
}
impl NormalizedProduct<'_> {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::View(view) => view.item().span,
            Self::Owned(item) => item.span,
            Self::Shared(item) => item.span,
            Self::Deferred { recipe, .. } => recipe.span(),
        }
    }
    pub(super) fn function_name(&self) -> Option<&str> {
        match self {
            Self::View(view) => Some(&view.name),
            Self::Owned(item) => match &item.kind {
                ItemKind::Function(function) => Some(&function.name),
                _ => None,
            },
            Self::Shared(item) => match &item.kind {
                ItemKind::Function(function) => Some(&function.name),
                _ => None,
            },
            Self::Deferred { recipe, .. } => recipe.declaration().function_name(),
        }
    }
    pub(crate) fn ensure(&mut self, reuse: &mut Reuse<'_>) -> CompileResult<&Item> {
        if let Self::Deferred {
            recipe,
            allowance,
            index,
            target_pointer_width,
        } = self
        {
            let mut budget = Budget::new(allowance);
            budget.target_pointer_width = *target_pointer_width;
            let clock = reuse.clock();
            let mut item = recipe.source_item().clone();
            reuse.normalized_input_cloned();
            reuse.cloned(recipe.span(), Stage::Normalize);
            reuse.assembly(clock);
            let clock = reuse.clock();
            // Scalar admission excludes aggregate/default/binder products.
            // The original per-item entry state is exactly: no nominal,
            // empty const/type/value scopes, top=true and depth=0. There is
            // no ID cursor, import mutation or generated-name cursor here.
            let result = Visitor {
                scalar_file: reuse.fuse_distribution().then_some(recipe.span().file),
                default_nominal: None,
                const_scope: Names::new(),
                mode: Mode::Normalize(index),
                module: recipe.module(),
                budget: &mut budget,
            }
            .item(&mut item, &Names::new(), true, 0);
            reuse.production(clock);
            result.map_err(|error| vec![error])?;
            let after = budget.remaining_limits();
            if !reuse.normalized_reconstruction_matches(recipe.span(), allowance, &after) {
                return Err(vec![error(
                    "E-PRODUCER-NORMALIZE",
                    recipe.span(),
                    "sealed normalized reconstruction demand differs from its current receipt",
                )]);
            }
            reuse.normalized_reconstructed(allowance, &after);
            *self = Self::Owned(item);
        }
        match self {
            Self::View(view) => Ok(view.item()),
            Self::Owned(item) => Ok(item),
            Self::Shared(item) => Ok(item),
            Self::Deferred { .. } => unreachable!("materialized normalized product"),
        }
    }
    pub(super) fn into_owned(mut self, reuse: &mut Reuse<'_>) -> CompileResult<Item> {
        self.ensure(reuse)?;
        match self {
            Self::View(view) => Ok(view.materialize()),
            Self::Owned(item) => {
                reuse.owned_restore_item();
                Ok(item)
            }
            Self::Shared(item) => match Arc::try_unwrap(item) {
                Ok(item) => {
                    reuse.owned_restore_item();
                    Ok(item)
                }
                Err(item) => {
                    let clock = reuse.clock();
                    let item = item.as_ref().clone();
                    reuse.restored_item_cloned();
                    reuse.assembly(clock);
                    Ok(item)
                }
            },
            Self::Deferred { .. } => unreachable!("materialized normalized product"),
        }
    }
}

/// Shared adapter for the existing real Probe and Lifetime visitors. The rich
/// path is always Borrowed. A scalar product is materialized only after that
/// stage's receipt fails, before the unchanged visitor spends formal budget.
pub(crate) enum Input<'i, 'a> {
    Borrowed(&'i Item),
    Product(&'i mut NormalizedProduct<'a>),
}
impl Input<'_, '_> {
    pub(crate) fn view(&self) -> Option<Arc<super::scalar_walk::BodyView>> {
        match self {
            Self::Product(NormalizedProduct::View(view)) => Some(view.clone()),
            _ => None,
        }
    }
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Borrowed(item) => item.span,
            Self::Product(item) => item.span(),
        }
    }
    pub(crate) fn ensure(&mut self, reuse: Option<&mut Reuse<'_>>) -> CompileResult<&Item> {
        match self {
            Self::Borrowed(item) => Ok(item),
            Self::Product(item) => item.ensure(reuse.expect("scalar producer stage")),
        }
    }
}
