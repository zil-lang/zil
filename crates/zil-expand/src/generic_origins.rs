//! Private declaration provenance assigned by actual module traversal. Spans
//! and generated spellings are never owner authority.
use super::*;
use zil_base::ModuleId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DeclId {
    pub(crate) module: ModuleId,
    pub(crate) ordinal: u32,
    pub(crate) compiler: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MemberId {
    pub(crate) declaration: DeclId,
    pub(crate) ordinal: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) declaration: DeclId,
    pub(crate) member: Option<u32>,
    pub(crate) provider: bool,
    pub(crate) compiler: bool,
}
#[derive(Default)]
pub(crate) struct Origins {
    pub(crate) entries: Vec<Origin>,
}
impl Origin {
    pub(crate) fn source(module: ModuleId, ordinal: usize, span: Span) -> R<Self> {
        Ok(Self {
            declaration: DeclId {
                module,
                compiler: false,
                ordinal: u32::try_from(ordinal).map_err(|_| {
                    Diagnostic::limit(
                        Phase::Expand,
                        "declaration identity ordinal exhausted",
                        Some(span),
                    )
                })?,
            },
            member: None,
            provider: false,
            compiler: false,
        })
    }
    pub(crate) fn member(self, ordinal: usize, span: Span) -> R<Self> {
        Ok(Self {
            member: Some(u32::try_from(ordinal).map_err(|_| {
                Diagnostic::limit(
                    Phase::Expand,
                    "member identity ordinal exhausted",
                    Some(span),
                )
            })?),
            provider: false,
            ..self
        })
    }
    pub(crate) fn id(self, span: Span) -> R<MemberId> {
        Ok(MemberId {
            declaration: self.declaration,
            ordinal: self
                .member
                .ok_or_else(|| error(span, "method source member identity absent"))?,
        })
    }
}
impl Origins {
    pub(crate) fn single(
        module: &Module,
        inserted_option: bool,
        budget: &mut Budget<'_>,
    ) -> R<Self> {
        let mut entries = Vec::new();
        for (index, item) in module.items.iter().enumerate() {
            budget.node(item.span, 0)?;
            let mut origin = Origin::source(
                module.id,
                index.saturating_sub(usize::from(inserted_option)),
                item.span,
            )?;
            if inserted_option && index == 0 {
                origin.compiler = true;
                origin.declaration.compiler = true;
            }
            entries.push(origin);
        }
        Ok(Self { entries })
    }
    pub(crate) fn insert_option(&mut self, module: &Module, budget: &mut Budget<'_>) -> R<()> {
        if self.entries.len() + 1 != module.items.len() {
            return Err(error(module.span, "Option provenance shape differs"));
        }
        for _ in 0..=self.entries.len() {
            budget.node(module.span, 0)?;
        }
        let mut origin = Origin::source(module.id, 0, module.span)?;
        origin.compiler = true;
        origin.declaration.compiler = true;
        self.entries.insert(0, origin);
        Ok(())
    }
    pub(crate) fn aliases(&mut self, module: &Module, budget: &mut Budget<'_>) -> R<()> {
        if self.entries.len() != module.items.len() {
            return Err(error(module.span, "alias provenance shape differs"));
        }
        let mut retained = Vec::new();
        for (item, origin) in module.items.iter().zip(std::mem::take(&mut self.entries)) {
            budget.node(item.span, 0)?;
            if !matches!(item.kind, ItemKind::TypeAlias(_)) {
                retained.push(origin);
            }
        }
        self.entries = retained;
        Ok(())
    }
    pub(crate) fn get(&self, index: usize, span: Span) -> R<Origin> {
        self.entries
            .get(index)
            .copied()
            .ok_or_else(|| error(span, "declaration provenance absent"))
    }
}
pub(crate) struct Pending {
    pub(crate) item: Item,
    pub(crate) origin: Option<Origin>,
    pub(crate) member_calls: BTreeMap<usize, trait_instances::CallTargets>,
}
impl Engine<'_> {
    pub(crate) fn enqueue(&mut self, item: Item, origin: Option<Origin>) -> R<()> {
        if self.origins.is_some() {
            self.budget.node(item.span, 0)?;
            if origin.is_none() {
                return Err(error(item.span, "generated declaration provenance absent"));
            }
        }
        self.pending.push_back(Pending {
            item,
            origin,
            member_calls: BTreeMap::new(),
        });
        Ok(())
    }
    pub(crate) fn template_origin(&mut self, item: &Item) -> R<Option<Origin>> {
        self.traits.origin(item, &mut self.budget)
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct WalkContext {
    pub(crate) origin: Option<Origin>,
}
impl Budget<'_> {
    pub(crate) fn with_origin<T>(
        &mut self,
        origin: Option<Origin>,
        f: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let previous = std::mem::replace(&mut self.context.origin, origin);
        let result = f(self);
        self.context.origin = previous;
        result
    }
}
impl Engine<'_> {
    pub(super) fn with_origin<T>(
        &mut self,
        origin: Option<Origin>,
        f: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let previous = std::mem::replace(&mut self.current_origin, origin);
        let budget_previous = std::mem::replace(&mut self.budget.context.origin, origin);
        let result = f(self);
        self.current_origin = previous;
        self.budget.context.origin = budget_previous;
        result
    }
    pub(super) fn provider(&mut self, name: &str, span: Span) -> R<bool> {
        if self.origins.is_none() {
            return Ok(false);
        }
        let Some(item) = self.templates.get(name).copied() else {
            return Ok(false);
        };
        self.budget.node(span, 0)?;
        Ok(self
            .traits
            .origin(item, &mut self.budget)?
            .is_some_and(|origin| origin.provider))
    }
}
