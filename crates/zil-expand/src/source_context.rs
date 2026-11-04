//! Source declaration lineage at the Driver -> Expand boundary. Names below
//! are raw identity data; they never establish visibility, builtin or loan facts.
use std::collections::{BTreeMap, BTreeSet};

use crate::generics::Budget;
use zil_base::{Diagnostic, FingerprintBuilder, ModuleId, NodeId, Phase, SourceUnit, Span};
use zil_ir::LifeDeclarationIdentity;

/// One actual output item retains its original source declaration, or an
/// explicitly allocated generated declaration. Vector position is current AST
/// placement, not a source declaration identity. Spans are diagnostics only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationOrigin {
    pub declaration: NodeId,
    pub identity: LifeDeclarationIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDeclarationOrigins {
    pub module: ModuleId,
    /// Aligned with the actual module item vector, including imports until the
    /// same transformation removes both. No spelling/span reconstruction.
    pub items: Vec<DeclarationOrigin>,
}

#[derive(Clone, Copy)]
pub enum DeclarationPlacement<'a> {
    Original,
    Aligned(&'a [ModuleDeclarationOrigins]),
    /// A transforming producer has not supplied lineage. Consumers needing
    /// declaration identity fail closed rather than renumber the resulting AST.
    Unavailable,
}

#[derive(Clone, Copy)]
pub enum SourceIdentityContext<'a> {
    /// One complete closed original request. Its namespace changes with any
    /// source byte/alias-group change and is not independently linkable.
    ClosedSnapshot {
        root: ModuleId,
        aliases: &'a BTreeMap<Vec<String>, ModuleId>,
        declarations: DeclarationPlacement<'a>,
    },
    /// Stable naming supplied explicitly, with one canonical path per actual
    /// module. This is not an alias map and grants no permissions.
    Explicit {
        package: &'a str,
        modules: &'a BTreeMap<ModuleId, Vec<String>>,
        declarations: DeclarationPlacement<'a>,
    },
}

pub(crate) struct ResolvedSourceIdentityContext<'a> {
    pub(crate) package: String,
    context: SourceIdentityContext<'a>,
}
type R<T> = Result<T, Diagnostic>;
fn bad(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-SOURCE-IDENTITY", Some(span), message)
}
fn lookup_work(size: usize, budget: &mut Budget<'_>, span: Span) -> R<()> {
    budget.identity_work(
        12usize.saturating_mul(size.checked_ilog2().map_or(1, |n| n as usize + 2)),
        span,
    )
}
fn bytes(
    hash: &mut FingerprintBuilder,
    value: &[u8],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    budget.identity_work(
        value
            .len()
            .checked_add(8)
            .ok_or_else(|| bad(span, "identity byte count overflow"))?,
        span,
    )?;
    hash.update(&(value.len() as u64).to_le_bytes());
    hash.update(value);
    Ok(())
}
struct Group<'a> {
    source: &'a SourceUnit,
    root: bool,
    aliases: Vec<&'a [String]>,
}
fn compare(
    a: &Group<'_>,
    b: &Group<'_>,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<std::cmp::Ordering> {
    budget.node(span, 0)?;
    for aliases in [&a.aliases, &b.aliases] {
        for path in aliases {
            budget.node(span, 0)?;
            for part in *path {
                budget.node(span, 0)?;
                budget.text(part, span)?;
            }
        }
    }
    Ok((!a.root, &a.aliases).cmp(&(!b.root, &b.aliases)))
}
fn snapshot(
    root: ModuleId,
    aliases: &BTreeMap<Vec<String>, ModuleId>,
    sources: &[SourceUnit],
    budget: &mut Budget<'_>,
) -> R<String> {
    let span = Span::default();
    let mut positions = BTreeMap::new();
    for (index, source) in sources.iter().enumerate() {
        budget.node(span, 0)?;
        lookup_work(positions.len(), budget, span)?;
        if positions.insert(source.module_id, index).is_some() {
            return Err(bad(span, "duplicate actual source module"));
        }
    }
    lookup_work(positions.len(), budget, span)?;
    if !positions.contains_key(&root) {
        return Err(bad(span, "closed source root is absent"));
    }
    for _ in sources {
        budget.node(span, 0)?;
    }
    let mut groups = sources
        .iter()
        .map(|source| Group {
            source,
            root: source.module_id == root,
            aliases: Vec::new(),
        })
        .collect::<Vec<_>>();
    for (path, module) in aliases {
        budget.node(span, 0)?;
        lookup_work(positions.len(), budget, span)?;
        let index = *positions
            .get(module)
            .ok_or_else(|| bad(span, "logical alias points outside complete source graph"))?;
        // Amortized pointer-vector growth; no string or AST clone occurs.
        for _ in 0..3 {
            budget.node(span, 0)?;
        }
        groups[index].aliases.push(path.as_slice());
    }
    for group in &groups {
        budget.node(span, 0)?;
        if !group.root && group.aliases.is_empty() {
            return Err(bad(span, "nonroot source module lacks a logical alias"));
        }
    }
    for _ in sources {
        budget.node(span, 0)?;
        budget.node(span, 0)?;
    }
    let mut order = (0..groups.len()).collect::<Vec<_>>();
    let mut scratch = vec![0; groups.len()];
    let mut width = 1usize;
    while width < order.len() {
        let mut left = 0;
        while left < order.len() {
            let middle = left.saturating_add(width).min(order.len());
            let right = middle.saturating_add(width).min(order.len());
            let (mut a, mut b) = (left, middle);
            for slot in &mut scratch[left..right] {
                budget.node(span, 0)?;
                let choose_a = if a == middle {
                    false
                } else if b == right {
                    true
                } else {
                    compare(&groups[order[a]], &groups[order[b]], budget, span)?
                        != std::cmp::Ordering::Greater
                };
                *slot = if choose_a {
                    let value = order[a];
                    a += 1;
                    value
                } else {
                    let value = order[b];
                    b += 1;
                    value
                };
            }
            left = right;
        }
        std::mem::swap(&mut order, &mut scratch);
        width = width.saturating_mul(2);
    }
    let mut hash = FingerprintBuilder::new();
    bytes(&mut hash, b"ZIL-CLOSED-SOURCE-SNAPSHOT-V1", budget, span)?;
    bytes(
        &mut hash,
        &(groups.len() as u64).to_le_bytes(),
        budget,
        span,
    )?;
    for index in order {
        let group = &groups[index];
        bytes(&mut hash, &[u8::from(group.root)], budget, span)?;
        bytes(
            &mut hash,
            &(group.aliases.len() as u64).to_le_bytes(),
            budget,
            span,
        )?;
        for path in &group.aliases {
            bytes(&mut hash, &(path.len() as u64).to_le_bytes(), budget, span)?;
            for part in *path {
                bytes(&mut hash, part.as_bytes(), budget, span)?;
            }
        }
        bytes(&mut hash, group.source.text.as_bytes(), budget, span)?;
    }
    budget.identity_work(64 + "closed-snapshot:v1:".len(), span)?;
    Ok(format!("closed-snapshot:v1:{}", hash.digest()))
}
impl<'a> SourceIdentityContext<'a> {
    pub(crate) fn resolve(
        self,
        sources: &[SourceUnit],
        budget: &mut Budget<'_>,
    ) -> R<ResolvedSourceIdentityContext<'a>> {
        let package = match self {
            Self::ClosedSnapshot { root, aliases, .. } => snapshot(root, aliases, sources, budget)?,
            Self::Explicit {
                package, modules, ..
            } => {
                if package.is_empty() {
                    return Err(bad(Span::default(), "explicit package namespace is empty"));
                }
                if modules.len() != sources.len() {
                    return Err(bad(
                        Span::default(),
                        "canonical module map is not the actual complete source set",
                    ));
                }
                let mut ids = BTreeSet::new();
                for source in sources {
                    budget.node(Span::default(), 0)?;
                    lookup_work(ids.len(), budget, Span::default())?;
                    if !ids.insert(source.module_id) {
                        return Err(bad(Span::default(), "duplicate actual source module"));
                    }
                    lookup_work(modules.len(), budget, Span::default())?;
                    if !modules.contains_key(&source.module_id) {
                        return Err(bad(
                            Span::default(),
                            "explicit canonical source module is absent",
                        ));
                    }
                }
                let mut paths = BTreeSet::new();
                for path in modules.values() {
                    budget.node(Span::default(), 0)?;
                    let repeats = 12usize
                        .saturating_mul(paths.len().checked_ilog2().map_or(1, |n| n as usize + 2));
                    for _ in 0..repeats {
                        for part in path {
                            budget.node(Span::default(), 0)?;
                            budget.text(part, Span::default())?;
                        }
                    }
                    if !paths.insert(path.as_slice()) {
                        return Err(bad(
                            Span::default(),
                            "two actual modules share one canonical logical identity",
                        ));
                    }
                }
                budget.identity_work(package.len().saturating_add(32), Span::default())?;
                format!("explicit:v1:{}:{package}", package.len())
            }
        };
        Ok(ResolvedSourceIdentityContext {
            package,
            context: self,
        })
    }
}
impl ResolvedSourceIdentityContext<'_> {
    pub(crate) fn declarations(&self) -> DeclarationPlacement<'_> {
        match self.context {
            SourceIdentityContext::ClosedSnapshot { declarations, .. }
            | SourceIdentityContext::Explicit { declarations, .. } => declarations,
        }
    }
    pub(crate) fn module_path(
        &self,
        module: ModuleId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<&[String]> {
        match self.context {
            SourceIdentityContext::ClosedSnapshot { root, aliases, .. } => {
                if module == root {
                    return Ok(&[]);
                }
                for (path, owner) in aliases {
                    budget.node(span, 0)?;
                    if *owner == module {
                        return Ok(path);
                    }
                }
                Err(bad(span, "logical module identity absent"))
            }
            SourceIdentityContext::Explicit { modules, .. } => {
                lookup_work(modules.len(), budget, span)?;
                modules
                    .get(&module)
                    .map(Vec::as_slice)
                    .ok_or_else(|| bad(span, "canonical logical module identity absent"))
            }
        }
    }
}
