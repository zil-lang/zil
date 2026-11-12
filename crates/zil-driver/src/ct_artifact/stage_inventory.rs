//! Immutable pre-generation declaration inventory. This is not a Core program:
//! complete original syntax, including bodies and unused formals, stays live.
//! A stored declaration is not a completed Shape, Contract or Layout task.
use std::{fmt::Write, sync::Arc};
use zil_base::{CompileResult, Diagnostic, Fingerprint, FingerprintBuilder, Limits, NodeId, Phase};
use zil_ct::CtExecutionBudget;
use zil_expand::source_context::{
    DeclarationOrigin, GeneratedSourceCatalog, GeneratedSourceTree, ModuleDeclarationOrigins,
    SourceTreeIdentity,
};
use zil_ir::ct_artifact::CtSymbolId;
use zil_syntax::{ParseResult, ast as s};

fn error(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-STAGE-INVENTORY", None, message)]
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Ct, "CT stage inventory budget exhausted", None)]
}

/// Host-owned immutable trees. Original origins address actual root items;
/// generated trees retain the complete verified emission's parse and hygiene.
enum FrozenTree {
    Original { parse: Arc<ParseResult>, origins: usize },
    Generated(Arc<GeneratedSourceTree>),
}
impl FrozenTree {
    fn module(&self) -> &s::Module {
        match self {
            Self::Original { parse, .. } => &parse.module,
            Self::Generated(tree) => &tree.module,
        }
    }
}

struct Declaration {
    symbol: CtSymbolId,
    tree: usize,
    /// Item/member ordinals, never a span-to-identity reconstruction.
    path: Vec<u32>,
    public: bool,
    content: Fingerprint,
}

/// This private value owns immutable AST handles, not copied declarations with
/// invented empty CFGs. Future emissions cannot mutate this epoch's view.
pub(super) struct FrozenCtStageInventory {
    source: Fingerprint,
    epoch: u64,
    trees: Vec<FrozenTree>,
    origins: Arc<Vec<ModuleDeclarationOrigins>>,
    declarations: Vec<Declaration>,
    content: Fingerprint,
    committed: Fingerprint,
}

// Debug here is an internal, compiler-version-bound content encoding, never a
// nominal identity or a semantic Type key. Stream into the digest without an
// intermediate String. Every fragment is paid before hashing; the preceding
// complete syntax walk independently bounds recursive formatter depth.
struct AstDigest<'a> {
    hash: FingerprintBuilder,
    budget: &'a mut CtExecutionBudget,
    failure: Option<Vec<Diagnostic>>,
}
impl std::fmt::Write for AstDigest<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        if let Err(errors) = self.budget.reserve_analysis_work(text.len() as u64 + 1) {
            self.failure = Some(errors);
            return Err(std::fmt::Error);
        }
        self.hash.update(text.as_bytes());
        Ok(())
    }
}
struct SyntaxMeter<'a> {
    budget: &'a mut CtExecutionBudget,
    limits: &'a Limits,
    nodes: usize,
}
impl zil_expand::ct_selection::Visitor for SyntaxMeter<'_> {
    fn node(&mut self, _: zil_base::Span, depth: usize) -> CompileResult<()> {
        self.budget.reserve_analysis_work(1)?;
        self.nodes = self.nodes.checked_add(1).ok_or_else(limit)?;
        if self.nodes > self.limits.max_nodes || depth >= self.limits.max_call_depth.min(128) {
            return Err(limit());
        }
        Ok(())
    }
    fn text(&mut self, text: &str, _: zil_base::Span) -> CompileResult<()> {
        self.budget.reserve_analysis_work(text.len() as u64)
    }
}
fn content<T: std::fmt::Debug>(
    value: &T,
    budget: &mut CtExecutionBudget,
) -> CompileResult<Fingerprint> {
    let mut digest = AstDigest { hash: FingerprintBuilder::new(), budget, failure: None };
    if write!(&mut digest, "zil/ct-complete-ast/v1\0{value:?}").is_err() {
        return Err(digest.failure.unwrap_or_else(limit));
    }
    Ok(digest.hash.digest())
}

fn push<T>(values: &mut Vec<T>, value: T, budget: &mut CtExecutionBudget) -> CompileResult<()> {
    budget.reserve_analysis_work(1)?;
    if values.len() == values.capacity() {
        let growth = values.capacity().max(4);
        budget.reserve_analysis_work(values.len() as u64)?;
        budget.reserve_temporary_bytes(
            growth.checked_mul(std::mem::size_of::<T>()).ok_or_else(limit)? as u64,
        )?;
        values.try_reserve_exact(growth).map_err(|_| limit())?;
    }
    values.push(value);
    Ok(())
}
fn copy_path(path: &[u32], budget: &mut CtExecutionBudget) -> CompileResult<Vec<u32>> {
    budget.reserve_analysis_work(path.len() as u64)?;
    budget.reserve_temporary_bytes(path.len() as u64 * 4)?;
    Ok(path.to_vec())
}

struct Collector<'a> {
    tree: usize,
    module: zil_base::ModuleId,
    generation: Option<Fingerprint>,
    /// Generated source has an explicit declaration relocation table.
    relocation: Option<&'a [NodeId]>,
    next: u32,
    declarations: &'a mut Vec<Declaration>,
    budget: &'a mut CtExecutionBudget,
    limits: &'a Limits,
}
impl Collector<'_> {
    fn row(&mut self, ordinal: u32, item: &s::Item, path: &[u32], public: bool) -> CompileResult<()> {
        if self.declarations.len() >= self.limits.max_nodes {
            return Err(limit());
        }
        let declaration = match self.relocation {
            Some(rows) => *rows.get(ordinal as usize).ok_or_else(|| error("generated declaration relocation is incomplete"))?,
            None => NodeId(ordinal),
        };
        let row = Declaration {
            symbol: CtSymbolId { module: self.module, declaration, generation_path: self.generation },
            tree: self.tree,
            path: copy_path(path, self.budget)?,
            public,
            content: content(item, self.budget)?,
        };
        push(self.declarations, row, self.budget)
    }
    fn allocate(&mut self, count: usize) -> CompileResult<u32> {
        self.budget.reserve_analysis_work(count as u64 + 1)?;
        let old = self.next;
        self.next = self.next.checked_add(u32::try_from(count).map_err(|_| limit())?).ok_or_else(limit)?;
        if self.next as usize > self.limits.max_nodes { return Err(limit()); }
        Ok(old)
    }
    fn children(&mut self, item: &s::Item, path: &mut Vec<u32>, public: bool) -> CompileResult<()> {
        if path.len() >= self.limits.max_call_depth.min(128) { return Err(limit()); }
        let members = match &item.kind {
            s::ItemKind::Aggregate(value) => {
                self.allocate(value.fields.len())?;
                &value.methods
            }
            s::ItemKind::Enum(value) => {
                for variant in &value.variants {
                    self.allocate(1)?;
                    match &variant.fields {
                        s::VariantFields::Unit => {},
                        s::VariantFields::Tuple(fields) => { self.allocate(fields.len())?; },
                        s::VariantFields::Record(fields) => { self.allocate(fields.len())?; },
                    }
                }
                return Ok(());
            }
            s::ItemKind::Trait(value) => &value.members,
            s::ItemKind::Impl(value) => &value.members,
            _ => return Ok(()),
        };
        for (ordinal, member) in members.iter().enumerate() {
            let id = self.allocate(1)?;
            push(path, u32::try_from(ordinal).map_err(|_| limit())?, self.budget)?;
            self.row(id, member, path, public && member.public)?;
            self.children(member, path, public && member.public)?;
            path.pop();
        }
        Ok(())
    }
}

impl FrozenCtStageInventory {
    /// Freeze one actual source/committed-generation epoch. No readiness bits
    /// are inferred from the existence of a declaration or an empty class list.
    pub(super) fn freeze(
        source: Fingerprint,
        epoch: u64,
        parsed: &[Arc<ParseResult>],
        origins: Arc<Vec<ModuleDeclarationOrigins>>,
        committed: &GeneratedSourceCatalog,
        budget: &mut CtExecutionBudget,
        limits: &Limits,
    ) -> CompileResult<Self> {
        use zil_expand::ct_selection::Visitor;
        budget.reserve_analysis_work(parsed.len() as u64 + origins.len() as u64 + committed.trees.len() as u64)?;
        if parsed.len() != origins.len() { return Err(error("original declaration inventory is incomplete")); }
        let mut trees = Vec::new();
        let mut original = std::collections::BTreeSet::new();
        for parse in parsed {
            budget.reserve_analysis_work(origins.len() as u64 + 1)?;
            let origin = origins.iter().position(|row| row.module == parse.module.id)
                .ok_or_else(|| error("original module origins are absent"))?;
            budget.reserve_temporary_bytes(64)?;
            if !original.insert(parse.module.id) || !parse.diagnostics.is_empty()
                || parse.module.items.len() != origins[origin].items.len()
            { return Err(error("original source parse or placement is not complete")); }
            for (ordinal, row) in origins[origin].items.iter().enumerate() {
                budget.reserve_analysis_work(1)?;
                if row.declaration != NodeId(ordinal as u32) || row.generated.is_some() || row.retained_members.is_some() {
                    return Err(error("stage inventory requires original unfiltered declaration placement"));
                }
            }
            push(&mut trees, FrozenTree::Original { parse: Arc::clone(parse), origins: origin }, budget)?;
        }
        for (ordinal, tree) in committed.trees.iter().enumerate() {
            budget.reserve_analysis_work(ordinal as u64 + 1)?;
            if !matches!(tree.identity, SourceTreeIdentity::Generated { .. })
                || committed.trees[..ordinal].iter().any(|old| old.identity == tree.identity)
            { return Err(error("committed source event is not unique")); }
            push(&mut trees, FrozenTree::Generated(Arc::clone(tree)), budget)?;
        }
        let mut declarations = Vec::new();
        let mut all = FingerprintBuilder::new();
        all.update(b"zil/ct-frozen-original-and-committed/v1\0");
        all.update(&source.0);
        all.update(&epoch.to_le_bytes());
        let mut generated = FingerprintBuilder::new();
        generated.update(b"zil/ct-committed-generation-content/v1\0");
        for (index, tree) in trees.iter().enumerate() {
            let module = tree.module();
            let mut meter = SyntaxMeter { budget, limits, nodes: 0 };
            for item in &module.items { meter.item(item, 0)?; }
            let digest = content(module, budget)?;
            all.update(&digest.0);
            let (generation, relocation) = match tree {
                FrozenTree::Original { origins: row, .. } => {
                    let bytes = super::invocation_context::origins_cost(&origins[*row..=*row], budget)?;
                    // The complete borrowed origins stay behind an Arc; bytes
                    // are visited/hashed but no second owned copy is created.
                    let _ = bytes;
                    all.update(&content(&origins[*row], budget)?.0);
                    (None, None)
                }
                FrozenTree::Generated(tree) => {
                    let identity = tree.generation_fingerprint().ok_or_else(|| error("committed source event has no generated identity"))?;
                    generated.update(&identity.0);
                    generated.update(&digest.0);
                    generated.update(&content(&tree.emitter, budget)?.0);
                    generated.update(&content(&tree.declarations, budget)?.0);
                    (Some(identity), Some(tree.declarations.as_slice()))
                }
            };
            let mut collector = Collector {
                tree: index, module: module.id, generation, relocation,
                next: u32::try_from(module.items.len()).map_err(|_| limit())?,
                declarations: &mut declarations, budget, limits,
            };
            let mut path = Vec::new();
            for (ordinal, item) in module.items.iter().enumerate() {
                push(&mut path, u32::try_from(ordinal).map_err(|_| limit())?, collector.budget)?;
                collector.row(ordinal as u32, item, &path, item.public)?;
                path.pop();
            }
            for (ordinal, item) in module.items.iter().enumerate() {
                push(&mut path, ordinal as u32, collector.budget)?;
                collector.children(item, &mut path, item.public)?;
                path.pop();
            }
            if relocation.is_some_and(|rows| rows.len() != collector.next as usize) {
                return Err(error("committed source declaration inventory is not complete"));
            }
        }
        let committed = generated.digest();
        all.update(&committed.0);
        Ok(Self { source, epoch, trees, origins, declarations, content: all.digest(), committed })
    }

    pub(super) fn source_fingerprint(&self) -> Fingerprint { self.source }
    pub(super) fn epoch(&self) -> u64 { self.epoch }
    pub(super) fn content_fingerprint(&self) -> Fingerprint { self.content }
    pub(super) fn committed_fingerprint(&self) -> Fingerprint { self.committed }

    pub(super) fn declaration<'a>(
        &'a self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
        limits: &Limits,
    ) -> CompileResult<Option<FrozenDeclaration<'a>>> {
        budget.reserve_analysis_work(self.declarations.len() as u64)?;
        let Some(row) = self.declarations.iter().find(|row| row.symbol == *symbol) else { return Ok(None); };
        let tree = &self.trees[row.tree];
        let item = zil_expand::source_context::source_item_at_path(
            tree.module(), &row.path, limits.max_call_depth,
            |count| budget.reserve_analysis_work(count as u64),
        )?.ok_or_else(|| error("frozen declaration structural path is absent"))?;
        let origin = match tree {
            FrozenTree::Original { origins, .. } => Some(&self.origins[*origins].items[row.path[0] as usize]),
            FrozenTree::Generated(_) => None,
        };
        Ok(Some(FrozenDeclaration { item, origin, public: row.public, content: row.content }))
    }
}

/// A borrowed complete source declaration. The body stays actual syntax; a
/// declaration-only signature is distinguishable from an empty block body.
pub(super) struct FrozenDeclaration<'a> {
    pub(super) item: &'a s::Item,
    pub(super) origin: Option<&'a DeclarationOrigin>,
    pub(super) public: bool,
    pub(super) content: Fingerprint,
}
impl FrozenDeclaration<'_> {
    pub(super) fn body(&self) -> Option<&s::FunctionBody> {
        let function = match &self.item.kind {
            s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => function,
            _ => return None,
        };
        (!matches!(function.body, s::FunctionBody::Declaration)).then_some(&function.body)
    }
}
