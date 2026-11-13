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
use zil_ir::ct_artifact::{CtStageIdentity, CtSymbolId, CtTypeArgument, CtTypeNode, CtTypeRef};
use zil_syntax::{ParseResult, ast as s};

fn error(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-STAGE-INVENTORY",
        None,
        message,
    )]
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "CT stage inventory budget exhausted",
        None,
    )]
}

/// Host-owned immutable trees. Original origins address actual root items;
/// generated trees retain the complete verified emission's parse and hygiene.
enum FrozenTree {
    Original {
        parse: Arc<ParseResult>,
        origins: usize,
    },
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
    let mut digest = AstDigest {
        hash: FingerprintBuilder::new(),
        budget,
        failure: None,
    };
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
            growth
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(limit)? as u64,
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
    fn row(
        &mut self,
        ordinal: u32,
        item: &s::Item,
        path: &[u32],
        public: bool,
    ) -> CompileResult<()> {
        if self.declarations.len() >= self.limits.max_nodes {
            return Err(limit());
        }
        let declaration = match self.relocation {
            Some(rows) => *rows
                .get(ordinal as usize)
                .ok_or_else(|| error("generated declaration relocation is incomplete"))?,
            None => NodeId(ordinal),
        };
        let row = Declaration {
            symbol: CtSymbolId {
                module: self.module,
                declaration,
                generation_path: self.generation,
            },
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
        self.next = self
            .next
            .checked_add(u32::try_from(count).map_err(|_| limit())?)
            .ok_or_else(limit)?;
        if self.next as usize > self.limits.max_nodes {
            return Err(limit());
        }
        Ok(old)
    }
    fn children(&mut self, item: &s::Item, path: &mut Vec<u32>, public: bool) -> CompileResult<()> {
        if path.len() >= self.limits.max_call_depth.min(128) {
            return Err(limit());
        }
        let members = match &item.kind {
            s::ItemKind::Aggregate(value) => {
                self.allocate(value.fields.len())?;
                &value.methods
            }
            s::ItemKind::Enum(value) => {
                for variant in &value.variants {
                    self.allocate(1)?;
                    match &variant.fields {
                        s::VariantFields::Unit => {}
                        s::VariantFields::Tuple(fields) => {
                            self.allocate(fields.len())?;
                        }
                        s::VariantFields::Record(fields) => {
                            self.allocate(fields.len())?;
                        }
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
            push(
                path,
                u32::try_from(ordinal).map_err(|_| limit())?,
                self.budget,
            )?;
            self.row(id, member, path, public && member.public)?;
            self.children(member, path, public && member.public)?;
            path.pop();
        }
        Ok(())
    }
}

impl FrozenCtStageInventory {
    pub(super) fn identity(&self) -> CtStageIdentity {
        CtStageIdentity {
            source_fingerprint: self.source_fingerprint(),
            epoch: self.epoch(),
            committed_generation_fingerprint: self.committed_fingerprint(),
            declaration_inventory_fingerprint: self.content_fingerprint(),
        }
    }

    /// Resolve raw query Type occurrences in this exact immutable epoch.
    /// A current materializer's pending symbol table is not a frozen subject.
    pub(super) fn query_type(
        &self,
        graph: &CtTypeRef,
        budget: &mut CtExecutionBudget,
        limits: &Limits,
    ) -> CompileResult<Fingerprint> {
        budget.reserve_analysis_work(graph.nodes.len() as u64 + 129)?;
        let mut dependencies = FingerprintBuilder::new();
        dependencies.update(b"zil/ct-frozen-subject-source/v1\0");
        // The subject's body/declaration digest is not an epoch identity. A
        // repeated declaration in another frozen source or generation must
        // never reuse this task dependency merely because its AST is equal.
        dependencies.update(&self.source.0);
        dependencies.update(&self.epoch.to_le_bytes());
        dependencies.update(&self.content.0);
        dependencies.update(&self.committed.0);
        budget.reserve_analysis_work(104)?;
        if let Some(catalogue) = &graph.scoped_catalogue {
            if catalogue.stage != self.identity() {
                return Err(error(
                    "query Type catalogue belongs to another source or generation epoch",
                ));
            }
        }
        for node in &graph.nodes {
            if let CtTypeNode::Nominal { symbol, arguments } = node {
                let actual = self.declaration(symbol, budget, limits)?.ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-PHASE-DEPENDENCY",
                        None,
                        "query subject is not committed in the current immutable generation epoch",
                    )]
                })?;
                if actual.body().is_some()
                    || !matches!(
                        actual.item.kind,
                        s::ItemKind::Aggregate(_)
                            | s::ItemKind::Enum(_)
                            | s::ItemKind::TypeAlias(_)
                    )
                {
                    return Err(error(
                        "nominal query subject is not an actual type declaration",
                    ));
                }
                // Full original formal order includes unused/phantom and
                // intervening Const/Lifetime positions. This is only input
                // formation for a per-subject computation: bounds, complete
                // Const carriers and class substitution remain its separate
                // Shape/Contract tasks, never inferred from this sort check.
                let parameters = match &actual.item.kind {
                    s::ItemKind::Aggregate(declaration) => &declaration.generics,
                    s::ItemKind::Enum(declaration) => &declaration.generics,
                    s::ItemKind::TypeAlias(declaration) => &declaration.generics,
                    _ => unreachable!("nominal declaration kind checked above"),
                };
                budget.reserve_analysis_work(parameters.len() as u64 + 1)?;
                if parameters.len() != arguments.len()
                    || parameters.iter().zip(arguments).any(|(formal, actual)| {
                        !matches!(
                            (&formal.kind, actual),
                            (s::GenericParamKind::Type(_), CtTypeArgument::Type(_))
                                | (s::GenericParamKind::Const(_), CtTypeArgument::Scalar(_))
                                | (s::GenericParamKind::Lifetime, CtTypeArgument::Lifetime(_))
                        )
                    })
                {
                    return Err(error(
                        "query subject actuals do not match its complete original mixed formal order",
                    ));
                }
                budget.reserve_analysis_work(38)?;
                dependencies.update(&actual.content.0);
                dependencies.update(&[u8::from(actual.public)]);
                if let Some(origin) = actual.origin {
                    dependencies.update(&origin.declaration.0.to_le_bytes());
                }
            }
        }
        Ok(dependencies.digest())
    }
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
        budget.reserve_analysis_work(
            parsed.len() as u64 + origins.len() as u64 + committed.trees.len() as u64,
        )?;
        if parsed.len() != origins.len() {
            return Err(error("original declaration inventory is incomplete"));
        }
        let mut trees = Vec::new();
        let mut original = std::collections::BTreeSet::new();
        for parse in parsed {
            budget.reserve_analysis_work(origins.len() as u64 + 1)?;
            let origin = origins
                .iter()
                .position(|row| row.module == parse.module.id)
                .ok_or_else(|| error("original module origins are absent"))?;
            budget.reserve_temporary_bytes(64)?;
            if !original.insert(parse.module.id)
                || !parse.diagnostics.is_empty()
                || parse.module.items.len() != origins[origin].items.len()
            {
                return Err(error("original source parse or placement is not complete"));
            }
            for (ordinal, row) in origins[origin].items.iter().enumerate() {
                budget.reserve_analysis_work(1)?;
                if row.declaration != NodeId(ordinal as u32)
                    || row.generated.is_some()
                    || row.retained_members.is_some()
                {
                    return Err(error(
                        "stage inventory requires original unfiltered declaration placement",
                    ));
                }
            }
            push(
                &mut trees,
                FrozenTree::Original {
                    parse: Arc::clone(parse),
                    origins: origin,
                },
                budget,
            )?;
        }
        for (ordinal, tree) in committed.trees.iter().enumerate() {
            budget.reserve_analysis_work(ordinal as u64 + 1)?;
            if !matches!(tree.identity, SourceTreeIdentity::Generated { .. })
                || committed.trees[..ordinal]
                    .iter()
                    .any(|old| old.identity == tree.identity)
            {
                return Err(error("committed source event is not unique"));
            }
            push(&mut trees, FrozenTree::Generated(Arc::clone(tree)), budget)?;
        }
        // One original declaration/formal/body bridge is shared with the
        // runtime Source producer. Its AST views are complete and immutable;
        // they do not claim that a requested Contract facet has completed.
        let module_bytes = (parsed.len() as u64)
            .checked_mul(std::mem::size_of::<&s::Module>() as u64)
            .ok_or_else(limit)?;
        budget.reserve_analysis_work(parsed.len() as u64)?;
        budget.reserve_temporary_bytes(module_bytes)?;
        let modules = parsed.iter().map(|parse| &parse.module).collect::<Vec<_>>();
        let mut allowance = limits.clone();
        allowance.max_analysis_steps = allowance
            .max_analysis_steps
            .min(usize::try_from(budget.remaining().max_analysis_work).unwrap_or(usize::MAX));
        allowance.max_source_bytes = allowance
            .max_source_bytes
            .min(usize::try_from(budget.remaining().max_temporary_bytes).unwrap_or(usize::MAX));
        let before = allowance.clone();
        let original_rows =
            zil_expand::source_declarations::collect_original_declarations_with_budget(
                &modules,
                &origins,
                &mut allowance,
            );
        budget.reserve_analysis_work(
            (before.max_analysis_steps - allowance.max_analysis_steps) as u64,
        )?;
        let original_bytes = (before.max_source_bytes - allowance.max_source_bytes) as u64;
        budget.reserve_temporary_bytes(original_bytes)?;
        drop(modules);
        budget.release_temporary_bytes(module_bytes);
        let original_rows = match original_rows {
            Ok(rows) => rows,
            Err(errors) => {
                budget.release_temporary_bytes(original_bytes);
                return Err(errors);
            }
        };
        let mut declarations = Vec::new();
        let mut all = FingerprintBuilder::new();
        all.update(b"zil/ct-frozen-original-and-committed/v1\0");
        all.update(&source.0);
        all.update(&epoch.to_le_bytes());
        let mut generated = FingerprintBuilder::new();
        generated.update(b"zil/ct-committed-generation-content/v1\0");
        let mut visited_nodes = before.max_nodes - allowance.max_nodes;
        for (index, tree) in trees.iter().enumerate() {
            let module = tree.module();
            if matches!(tree, FrozenTree::Generated(_)) {
                let mut meter = SyntaxMeter {
                    budget,
                    limits,
                    nodes: visited_nodes,
                };
                for item in &module.items {
                    meter.item(item, 0)?;
                }
                visited_nodes = meter.nodes;
            }
            let digest = content(module, budget)?;
            all.update(&digest.0);
            let (generation, relocation) = match tree {
                FrozenTree::Original { origins: row, .. } => {
                    let bytes =
                        super::invocation_context::origins_cost(&origins[*row..=*row], budget)?;
                    // The complete borrowed origins stay behind an Arc; bytes
                    // are visited/hashed but no second owned copy is created.
                    let _ = bytes;
                    all.update(&content(&origins[*row], budget)?.0);
                    (None, None)
                }
                FrozenTree::Generated(tree) => {
                    let identity = tree
                        .generation_fingerprint()
                        .ok_or_else(|| error("committed source event has no generated identity"))?;
                    generated.update(&identity.0);
                    generated.update(&digest.0);
                    generated.update(&content(&tree.emitter, budget)?.0);
                    generated.update(&content(&tree.declarations, budget)?.0);
                    (Some(identity), Some(tree.declarations.as_slice()))
                }
            };
            if matches!(tree, FrozenTree::Original { .. }) {
                budget.reserve_analysis_work(original_rows.declarations.len() as u64)?;
                for actual in &original_rows.declarations {
                    if actual.module != module.id {
                        continue;
                    }
                    let row = Declaration {
                        symbol: CtSymbolId {
                            module: actual.module,
                            declaration: actual.declaration,
                            generation_path: None,
                        },
                        tree: index,
                        path: copy_path(&actual.path, budget)?,
                        public: actual.public,
                        content: content(actual.item, budget)?,
                    };
                    push(&mut declarations, row, budget)?;
                }
                continue;
            }
            let mut collector = Collector {
                tree: index,
                module: module.id,
                generation,
                relocation,
                next: u32::try_from(module.items.len()).map_err(|_| limit())?,
                declarations: &mut declarations,
                budget,
                limits,
            };
            let mut path = Vec::new();
            for (ordinal, item) in module.items.iter().enumerate() {
                push(
                    &mut path,
                    u32::try_from(ordinal).map_err(|_| limit())?,
                    collector.budget,
                )?;
                collector.row(ordinal as u32, item, &path, item.public)?;
                path.pop();
            }
            for (ordinal, item) in module.items.iter().enumerate() {
                push(&mut path, ordinal as u32, collector.budget)?;
                collector.children(item, &mut path, item.public)?;
                path.pop();
            }
            if relocation.is_some_and(|rows| rows.len() != collector.next as usize) {
                return Err(error(
                    "committed source declaration inventory is not complete",
                ));
            }
        }
        drop(original_rows);
        budget.release_temporary_bytes(original_bytes);
        let committed = generated.digest();
        all.update(&committed.0);
        Ok(Self {
            source,
            epoch,
            trees,
            origins,
            declarations,
            content: all.digest(),
            committed,
        })
    }

    pub(super) fn source_fingerprint(&self) -> Fingerprint {
        self.source
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(super) fn content_fingerprint(&self) -> Fingerprint {
        self.content
    }
    pub(super) fn committed_fingerprint(&self) -> Fingerprint {
        self.committed
    }

    pub(super) fn declaration<'a>(
        &'a self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
        limits: &Limits,
    ) -> CompileResult<Option<FrozenDeclaration<'a>>> {
        budget.reserve_analysis_work(self.declarations.len() as u64)?;
        let Some(row) = self.declarations.iter().find(|row| row.symbol == *symbol) else {
            return Ok(None);
        };
        let tree = &self.trees[row.tree];
        let item = zil_expand::source_context::source_item_at_path(
            tree.module(),
            &row.path,
            limits.max_call_depth,
            |count| budget.reserve_analysis_work(count as u64),
        )?
        .ok_or_else(|| error("frozen declaration structural path is absent"))?;
        let origin = match tree {
            FrozenTree::Original { origins, .. } => {
                Some(&self.origins[*origins].items[row.path[0] as usize])
            }
            FrozenTree::Generated(_) => None,
        };
        Ok(Some(FrozenDeclaration {
            item,
            origin,
            public: row.public,
            content: row.content,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use zil_base::{FileId, ModuleId, SourceUnit};
    use zil_ir::ct_artifact::CtDeclaredBudget;

    fn budget(work: u64) -> CtExecutionBudget {
        CtExecutionBudget::new(CtDeclaredBudget {
            max_analysis_work: work,
            max_temporary_bytes: 16_000_000,
            max_steps: 100_000,
            max_call_depth: 128,
            max_live_cells: 100_000,
            max_generated_nodes: 100_000,
            max_instances: 10_000,
        })
    }
    fn original(
        text: &str,
    ) -> (
        SourceUnit,
        Arc<ParseResult>,
        Arc<Vec<ModuleDeclarationOrigins>>,
    ) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "inventory.zil", text);
        let parse = Arc::new(zil_syntax::parse(&source));
        assert!(parse.diagnostics.is_empty(), "{:?}", parse.diagnostics);
        let (origins, _) = zil_expand::source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parse.module],
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &BTreeMap::new(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
            &Limits::default(),
        )
        .unwrap();
        (source, parse, Arc::new(origins))
    }

    #[test]
    fn complete_original_headers_and_bodies_survive_epoch_freeze() {
        let text = "pub trait Read<life unused,T>{fn read(self:&Self)->i32;}struct Cell<T>{value:T;fn number()->i32{return 42}}fn full<life p,#N:usize,T>(value:T)->T{return value}";
        let (source, parse, origins) = original(text);
        let mut b = budget(1_000_000);
        let frozen = FrozenCtStageInventory::freeze(
            source.fingerprint(),
            7,
            std::slice::from_ref(&parse),
            Arc::clone(&origins),
            &GeneratedSourceCatalog::default(),
            &mut b,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(frozen.source_fingerprint(), source.fingerprint());
        assert_eq!(frozen.epoch(), 7);
        for row in &frozen.declarations {
            let actual = frozen
                .declaration(&row.symbol, &mut b, &Limits::default())
                .unwrap()
                .unwrap();
            assert_eq!(actual.content, row.content);
            assert_eq!(actual.public, row.public);
            assert!(actual.origin.is_some());
            if let s::ItemKind::Function(function) = &actual.item.kind {
                if function.name == "full" {
                    assert_eq!(function.generics.len(), 3);
                    assert!(matches!(
                        function.generics[0].kind,
                        s::GenericParamKind::Lifetime
                    ));
                    assert!(matches!(
                        function.generics[1].kind,
                        s::GenericParamKind::Const(_)
                    ));
                    assert!(matches!(actual.body(), Some(s::FunctionBody::Block(_))));
                } else if function.name == "read" {
                    assert!(actual.body().is_none());
                    assert_eq!(actual.origin.unwrap().declaration, NodeId(0));
                } else if function.name == "number" {
                    assert!(actual.body().is_some());
                    // Top-level declarations 0,1,2; trait member 3;
                    // Cell field 4, then Cell method 5, independent of spans.
                    assert_eq!(row.symbol.declaration, NodeId(5));
                }
            }
        }
        let next = FrozenCtStageInventory::freeze(
            source.fingerprint(),
            8,
            &[parse],
            origins,
            &GeneratedSourceCatalog::default(),
            &mut b,
            &Limits::default(),
        )
        .unwrap();
        assert_ne!(frozen.content_fingerprint(), next.content_fingerprint());
        assert_eq!(frozen.committed_fingerprint(), next.committed_fingerprint());
    }

    #[test]
    fn query_subject_uses_full_mixed_formals_and_the_exact_source_epoch() {
        use zil_ir::{
            Constant,
            ct_artifact::{CtScalarTy, CtTypeNodeId},
        };
        let (source, parse, origins) = original("struct Phantom<T,#N:usize>{value:T}fn main(){}");
        let limits = Limits::default();
        let mut ledger = budget(10_000_000);
        let freeze = |epoch, ledger: &mut CtExecutionBudget| {
            FrozenCtStageInventory::freeze(
                source.fingerprint(),
                epoch,
                std::slice::from_ref(&parse),
                Arc::clone(&origins),
                &GeneratedSourceCatalog::default(),
                ledger,
                &limits,
            )
            .unwrap()
        };
        let first = freeze(7, &mut ledger);
        let next = freeze(8, &mut ledger);
        let symbol = first.declarations[0].symbol.clone();
        let mut ty = CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![
                CtTypeNode::Primitive(CtScalarTy::I32),
                CtTypeNode::Nominal {
                    symbol,
                    arguments: vec![
                        CtTypeArgument::Type(CtTypeNodeId(0)),
                        CtTypeArgument::Scalar(Constant::Usize(42)),
                    ],
                },
            ],
            root: CtTypeNodeId(1),
            ambient_classes: Vec::new(),
        };
        let baseline = first.query_type(&ty, &mut ledger, &limits).unwrap();
        assert_ne!(
            baseline,
            next.query_type(&ty, &mut ledger, &limits).unwrap()
        );
        let CtTypeNode::Nominal { arguments, .. } = &mut ty.nodes[1] else {
            unreachable!()
        };
        let last = arguments.pop().unwrap();
        assert!(first.query_type(&ty, &mut ledger, &limits).is_err());
        let CtTypeNode::Nominal { arguments, .. } = &mut ty.nodes[1] else {
            unreachable!()
        };
        arguments.push(last);
        arguments.swap(0, 1);
        assert!(first.query_type(&ty, &mut ledger, &limits).is_err());
        let CtTypeNode::Nominal { arguments, .. } = &mut ty.nodes[1] else {
            unreachable!()
        };
        arguments.swap(0, 1);
        assert_eq!(
            baseline,
            first.query_type(&ty, &mut ledger, &limits).unwrap()
        );
    }

    #[test]
    fn filtered_inventory_and_missing_body_cannot_be_full_source() {
        let (source, parse, origins) = original("fn full()->i32{return 42}fn other(){} ");
        let mut filtered = (*origins).clone();
        filtered[0].items.pop();
        assert!(
            FrozenCtStageInventory::freeze(
                source.fingerprint(),
                0,
                std::slice::from_ref(&parse),
                Arc::new(filtered),
                &GeneratedSourceCatalog::default(),
                &mut budget(1_000_000),
                &Limits::default()
            )
            .is_err()
        );
        let frozen = FrozenCtStageInventory::freeze(
            source.fingerprint(),
            0,
            &[parse],
            origins,
            &GeneratedSourceCatalog::default(),
            &mut budget(1_000_000),
            &Limits::default(),
        )
        .unwrap();
        let first = frozen
            .declaration(
                &CtSymbolId {
                    module: ModuleId(0),
                    declaration: NodeId(0),
                    generation_path: None,
                },
                &mut budget(1_000_000),
                &Limits::default(),
            )
            .unwrap()
            .unwrap();
        let s::FunctionBody::Block(body) = first.body().unwrap() else {
            panic!("actual body lost");
        };
        assert!(!body.statements.is_empty());
    }

    #[test]
    fn stage_full_source_visit_has_exact_fresh_budget_boundary() {
        let (source, parse, origins) =
            original("struct Phantom<life a>{value:i32}fn full<T>(value:T)->T{return value}");
        let run = |work| {
            let mut b = budget(work);
            let value = FrozenCtStageInventory::freeze(
                source.fingerprint(),
                3,
                std::slice::from_ref(&parse),
                Arc::clone(&origins),
                &GeneratedSourceCatalog::default(),
                &mut b,
                &Limits::default(),
            );
            (value, b.usage().analysis_work)
        };
        let (baseline, spent) = run(1_000_000);
        let baseline = baseline.unwrap();
        for _ in 0..2 {
            let (exact, used) = run(spent);
            assert_eq!(
                exact.unwrap().content_fingerprint(),
                baseline.content_fingerprint()
            );
            assert_eq!(used, spent);
            let (failure, _) = run(spent - 1);
            assert!(
                failure
                    .err()
                    .unwrap()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
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
