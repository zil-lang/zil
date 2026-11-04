//! Private declaration provenance assigned by actual module traversal. Spans
//! and generated spellings are never owner authority.
use super::*;
use crate::source_context::{
    DeclarationOrigin, DeclarationPlacement, ResolvedSourceIdentityContext,
};
use zil_base::ModuleId;
use zil_base::NodeId;
use zil_ir::LifeDeclarationIdentity;

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
    pub(crate) identities: BTreeMap<DeclId, DeclarationOrigin>,
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
        Ok(Self {
            entries,
            identities: BTreeMap::new(),
        })
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

    pub(crate) fn capture_identities(
        &mut self,
        module: &Module,
        inserted_option: bool,
        context: &ResolvedSourceIdentityContext<'_>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        if self.entries.len() != module.items.len() {
            return Err(error(
                module.span,
                "identity provenance is not aligned with syntax",
            ));
        }
        let mut supplied = None;
        match context.declarations() {
            DeclarationPlacement::Unavailable => {
                return Err(unsupported(
                    module.span,
                    "transformed declarations require explicit source lineage",
                ));
            }
            DeclarationPlacement::Original => {}
            DeclarationPlacement::Aligned(modules) => {
                for candidate in modules {
                    budget.node(module.span, 0)?;
                    if candidate.module == module.id {
                        if supplied.is_some() {
                            return Err(error(module.span, "duplicate declaration-origin module"));
                        }
                        supplied = Some(candidate);
                    }
                }
                let actual = supplied
                    .ok_or_else(|| error(module.span, "declaration-origin module absent"))?;
                if actual.items.len().checked_add(usize::from(inserted_option))
                    != Some(module.items.len())
                {
                    return Err(error(module.span, "declaration-origin item count differs"));
                }
            }
        }
        let module_path = context.module_path(module.id, budget, module.span)?;
        if context.package.is_empty() {
            return Err(error(module.span, "source package namespace is empty"));
        }
        for (index, (item, origin)) in module.items.iter().zip(&self.entries).enumerate() {
            budget.node(item.span, 0)?;
            if origin.declaration.module != module.id {
                continue;
            }
            if origin.declaration.compiler {
                continue;
            }
            let ordinal = index
                .checked_sub(usize::from(inserted_option))
                .ok_or_else(|| error(item.span, "source declaration ordinal underflow"))?;
            let captured = if let Some(supplied) = supplied {
                let captured = &supplied.items[ordinal];
                pay_identity(&captured.identity, budget, item.span)?;
                budget.text(&context.package, item.span)?;
                if captured.identity.package != context.package {
                    return Err(error(
                        item.span,
                        "declaration package differs from request context",
                    ));
                }
                captured.clone()
            } else {
                let (kind, actual_name) = match &item.kind {
                    ItemKind::Function(f) => ("function", Some(f.name.as_str())),
                    ItemKind::Extern { function, .. } => ("extern", Some(function.name.as_str())),
                    ItemKind::Aggregate(a) => (
                        match a.kind {
                            AggregateKind::Struct => "struct",
                            AggregateKind::Package => "package",
                            AggregateKind::Resource => "resource",
                        },
                        Some(a.name.as_str()),
                    ),
                    ItemKind::Enum(e) => ("enum", Some(e.name.as_str())),
                    ItemKind::Trait(t) => ("trait", Some(t.name.as_str())),
                    ItemKind::TypeAlias(a) => ("alias", Some(a.name.as_str())),
                    ItemKind::Const(c) => ("const", Some(c.name.as_str())),
                    ItemKind::Impl(_) => ("impl", None),
                    ItemKind::Import(_) => ("import", None),
                    ItemKind::Meta(_) => ("meta", None),
                };
                budget.text(&context.package, item.span)?;
                for part in module_path {
                    budget.node(item.span, 0)?;
                    budget.text(part, item.span)?;
                }
                budget.text(kind, item.span)?;
                let actual_name = if let Some(actual_name) = actual_name {
                    budget.text(actual_name, item.span)?;
                    actual_name.to_owned()
                } else {
                    // An anonymous declaration uses its actual source structural
                    // ordinal. This is not a location/span or a generated name.
                    for _ in 0..=ordinal.checked_ilog10().unwrap_or(0) {
                        budget.node(item.span, 0)?;
                    }
                    ordinal.to_string()
                };
                DeclarationOrigin {
                    declaration: NodeId(
                        u32::try_from(ordinal).map_err(|_| {
                            error(item.span, "source declaration identity exhausted")
                        })?,
                    ),
                    identity: LifeDeclarationIdentity {
                        package: context.package.to_owned(),
                        module_path: module_path.to_vec(),
                        declaration_path: vec![kind.to_owned(), actual_name],
                        generation_path: Vec::new(),
                    },
                }
            };
            for _ in 0..self
                .identities
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
            {
                budget.node(item.span, 0)?;
            }
            self.identities.insert(origin.declaration, captured);
        }
        Ok(())
    }

    pub(crate) fn copy_identities(
        &self,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<BTreeMap<DeclId, DeclarationOrigin>> {
        for identity in self.identities.values() {
            budget.node(span, 0)?;
            pay_identity(&identity.identity, budget, span)?;
        }
        Ok(self.identities.clone())
    }
}

fn pay_identity(identity: &LifeDeclarationIdentity, budget: &mut Budget<'_>, span: Span) -> R<()> {
    budget.text(&identity.package, span)?;
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        budget.node(span, 0)?;
        budget.text(part, span)?;
    }
    for _ in &identity.generation_path {
        budget.node(span, 0)?;
    }
    Ok(())
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
