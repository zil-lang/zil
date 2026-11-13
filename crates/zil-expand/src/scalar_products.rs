//! Request-local normalized syntax ownership. Deferred is a sealed recipe,
//! never an Item/Deref and never a restored body pretending to be normalized.
use super::*;
use crate::incremental::{NormalizedRecipe, Reuse, Stage};

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

pub(crate) enum NormalizedProduct<'a> {
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
            Self::Owned(item) => item.span,
            Self::Shared(item) => item.span,
            Self::Deferred { recipe, .. } => recipe.span(),
        }
    }
    pub(super) fn function_name(&self) -> Option<&str> {
        match self {
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
            Self::Owned(item) => Ok(item),
            Self::Shared(item) => Ok(item),
            Self::Deferred { .. } => unreachable!("materialized normalized product"),
        }
    }
    pub(super) fn into_owned(mut self, reuse: &mut Reuse<'_>) -> CompileResult<Item> {
        self.ensure(reuse)?;
        match self {
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
