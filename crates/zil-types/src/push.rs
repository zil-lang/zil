//! Actual sparse raw CFG updates from sealed producer inputs. This owns no
//! verification capability. Unknown state transitions use the current checker.
use super::*;
use std::sync::Arc;
use zil_base::{ModuleId, NodeId};
use zil_expand::{ExpansionEpoch, FunctionOwner, InputContext, ProducedProgram};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypesPushStats {
    pub context_rebuilds: usize,
    pub sparse_batches: usize,
    pub body_checks: usize,
    pub position_rebindings: usize,
    pub retained_functions: usize,
    pub receipt_tree_updates: usize,
}
impl TypesPushStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"context_rebuilds\":{},\"sparse_batches\":{},\"body_checks\":{},\"position_rebindings\":{},\"retained_functions\":{},\"receipt_tree_updates\":{}}}",
            self.context_rebuilds,
            self.sparse_batches,
            self.body_checks,
            self.position_rebindings,
            self.retained_functions,
            self.receipt_tree_updates
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedFunctionChange {
    pub function: FunctionId,
    /// The old dense slot is absent from the complete current snapshot.
    pub removed: bool,
    /// Complete actual relative CFG inequality, conservatively including local
    /// diagnostic positions. Equal public digests never authorize this result.
    pub result_changed: bool,
    pub position_changed: bool,
}
#[derive(Clone, Debug)]
pub struct TypedUpdate {
    pub program: Arc<TypedProgram>,
    pub changes: Vec<TypedFunctionChange>,
    pub stats: TypesPushStats,
}

#[derive(Clone, Default)]
pub struct TypesPushSession {
    current: Option<Arc<Context>>,
}
impl TypesPushSession {
    pub fn clear(&mut self) {
        self.current = None;
    }
    /// Stage the complete request before replacing the successful raw owner.
    /// Wrong history, target, quotas or declaration contexts rebuild current
    /// inputs. Errors leave the prior owner intact, including its shared CFGs.
    pub fn apply(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<TypedUpdate> {
        if let Some(update) = self.try_sparse(prepared, target, limits) {
            let (context, update) = update;
            self.current = Some(Arc::new(context));
            return Ok(update);
        }
        self.rebuild(prepared, target, limits)
    }
    fn rebuild(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<TypedUpdate> {
        let mut recording = Recording::default();
        let result = if prepared.identity_inference(limits) && prepared.produced().is_some() {
            let resolved = aggregates::resolve_prepared_program_recording(
                prepared.program(),
                limits,
                Some(&mut recording.shapes),
            )?;
            let eligible = matches!(resolved, std::borrow::Cow::Borrowed(_));
            program::check_resolved_program_recording(
                &resolved,
                target,
                limits,
                None,
                eligible.then_some(&mut recording),
            )?
        } else {
            check_prepared_program(prepared, target, limits)?
        };
        let program = Arc::new(result);
        let context = prepared
            .produced()
            .and_then(|produced| recording.finish(produced, program.clone(), target, limits));
        let mut changes: Vec<_> = program
            .module
            .functions
            .iter()
            .map(|f| TypedFunctionChange {
                function: f.id,
                removed: false,
                result_changed: true,
                position_changed: true,
            })
            .collect();
        if let Some(previous) = &self.current {
            for function in previous
                .program
                .module
                .functions
                .iter()
                .skip(program.module.functions.len())
            {
                changes.push(TypedFunctionChange {
                    function: function.id,
                    removed: true,
                    result_changed: true,
                    position_changed: true,
                });
            }
        }
        let update = TypedUpdate {
            stats: TypesPushStats {
                context_rebuilds: 1,
                body_checks: program.module.functions.len(),
                ..TypesPushStats::default()
            },
            program,
            changes,
        };
        self.current = context.map(Arc::new);
        Ok(update)
    }
    fn try_sparse(
        &self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> Option<(Context, TypedUpdate)> {
        let prior = self.current.as_ref()?;
        let produced = prepared.produced()?;
        let batch = produced.change_batch();
        if !prepared.identity_inference(limits)
            || &prior.limits != limits
            || &prior.target != target
            || batch.previous_epoch() != Some(&prior.epoch)
            || batch.context() != &prior.input_context
            || prepared.program().root != prior.program.module.module_id
        {
            return None;
        }
        let mut rows = prior.rows.clone();
        let mut program = prior.program.as_ref().clone();
        let mut changes = Vec::with_capacity(batch.changes().len());
        let mut stats = TypesPushStats {
            sparse_batches: 1,
            retained_functions: program.module.functions.len(),
            ..TypesPushStats::default()
        };
        // Changes are in original module/declaration order. We do not emit an
        // error from this speculative path: all failures restart the original
        // ordered current checker with its original allowances and first error.
        for change in batch.changes() {
            let (module, function) = produced.changed_function(change)?;
            let table = prior.tables.get(&module.core.module_id)?;
            let id = table.get(&function.name)?.concrete_id(function.span).ok()?;
            let index = usize::try_from(id.0).ok()?;
            let old = rows.get(index)?;
            if old.id != id
                || old.module != module.core.module_id
                || old.node != function.id
                || old.name != function.name
                || old.owner.as_ref()? != change.owner()
                || change.previous_span() != Some(old.span)
            {
                return None;
            }
            let before = program.module.functions.get(index)?;
            let mut row = old.clone();
            let mut typed;
            let result_changed;
            if change.body_changed() {
                row.shape = scalar_shapes::validate_function(function, limits)?.ok()?;
                row.names = names::check_function(function, limits).ok()?;
                let mut state = closures::State::new(program.module.functions.len(), limits);
                let imports = program::imports_for(module, &prior.tables).ok()?;
                let mut builder = Builder::new(function, id, table, limits, &mut state).ok()?;
                builder.imports = Some(&imports);
                // Current closed context has no enums/dyn/aggregate inventories.
                typed = builder.finish(&function.body).ok()?;
                if !state.push_closed_state() || !closed_function(&typed) {
                    return None;
                }
                row.cfg = state.accounting();
                row.post = PostReceipt::check(&typed, limits)?;
                row.visibility = instruction_count(&typed)?;
                if module.core.module_id != prepared.program().root {
                    typed.name = format!("@module{}:function{}", module.core.module_id.0, id.0);
                }
                result_changed = !relative_equal(before, &typed)?;
                stats.body_checks += 1;
            } else {
                typed = before.clone();
                if !incremental::rebase(&mut typed, old.span, function.span) {
                    return None;
                }
                result_changed = false;
                stats.position_rebindings += 1;
            }
            let position_changed = before.span != function.span;
            row.span = function.span;
            rows = rows.replace(index, row)?;
            stats.receipt_tree_updates += 1;
            // Preserve the old actual leaf when both complete raw values match.
            // This is actual equality after execution, never an asserted hash.
            if typed != *before {
                program.module.functions.replace(index, typed);
            }
            stats.retained_functions = stats.retained_functions.checked_sub(1)?;
            changes.push(TypedFunctionChange {
                function: id,
                removed: false,
                result_changed,
                position_changed,
            });
        }
        let totals = rows.total()?;
        if !prior.fits(totals, limits) {
            return None;
        }
        program.module.source_fingerprint = prepared.program().source_fingerprint;
        for &index in batch.changed_modules() {
            let module = prepared.program().modules.get(index)?;
            let interface_index = *prior.interface_indexes.get(&module.core.module_id)?;
            program
                .interfaces
                .get_mut(interface_index)?
                .implementation_fingerprint = module.core.source_fingerprint;
        }
        let program = Arc::new(program);
        let context = Context {
            epoch: batch.epoch().clone(),
            rows,
            program: program.clone(),
            ..prior.as_ref().clone()
        };
        let update = TypedUpdate {
            program,
            changes,
            stats,
        };
        Some((context, update))
    }
}

#[derive(Clone)]
struct Context {
    epoch: ExpansionEpoch,
    input_context: InputContext,
    target: TargetProfile,
    limits: Limits,
    program: Arc<TypedProgram>,
    tables: Arc<Tables>,
    rows: ReceiptTree,
    context_cost: [usize; 6],
    fixed_tail: usize,
    interface_indexes: Arc<BTreeMap<ModuleId, usize>>,
}
type Tables = BTreeMap<ModuleId, BTreeMap<String, Signature>>;
impl Context {
    fn fits(&self, total: Total, limits: &Limits) -> bool {
        let maxima = [
            limits.max_blocks,
            limits.max_locals,
            limits.max_values,
            limits.max_instructions,
            limits.max_analysis_steps,
            limits.max_nodes,
        ];
        for (i, &max) in maxima.iter().enumerate() {
            let Some(cost) = self.context_cost[i].checked_add(total.cfg[i]) else {
                return false;
            };
            if cost > max {
                return false;
            }
            if i == 4
                && cost
                    .checked_add(total.post.shared)
                    .and_then(|c| c.checked_add(self.fixed_tail))
                    .is_none_or(|c| c > max)
            {
                return false;
            }
        }
        total.names <= limits.max_nodes
            && total.shape <= limits.max_analysis_steps
            && total.visibility <= limits.max_analysis_steps
            && total.post.bytes <= limits.max_analysis_steps
            && total.post.stores <= limits.max_analysis_steps
    }
}

/// One pass through the existing full checker records real successful work.
/// This temporary recording is released once the persistent receipt tree owns
/// the current leaves; no independent unbounded history is retained.
#[derive(Default)]
pub(super) struct Recording {
    pub(super) names: Vec<usize>,
    pub(super) shapes: Vec<usize>,
    pub(super) tables: Option<Arc<Tables>>,
    pub(super) context_cost: [usize; 6],
    pub(super) body_cost: [usize; 6],
    pub(super) closed_state: bool,
    pub(super) remaining: usize,
    rows: Vec<Row>,
}
impl Recording {
    pub(super) fn record(
        &mut self,
        module: ModuleId,
        source: &CoreFunction,
        typed: &TypedFunction,
        before: [usize; 6],
        after: [usize; 6],
    ) {
        let mut cfg = [0; 6];
        for i in 0..6 {
            let Some(cost) = after[i].checked_sub(before[i]) else {
                self.closed_state = false;
                return;
            };
            cfg[i] = cost;
        }
        self.rows.push(Row {
            id: typed.id,
            module,
            node: source.id,
            name: source.name.clone(),
            owner: None,
            span: source.span,
            cfg,
            names: 0,
            shape: 0,
            visibility: 0,
            post: PostReceipt::default(),
        });
    }
    fn finish(
        mut self,
        produced: &ProducedProgram,
        program: Arc<TypedProgram>,
        target: &TargetProfile,
        limits: &Limits,
    ) -> Option<Context> {
        if !self.closed_state
            || self.rows.len() != program.module.functions.len()
            || self.names.len() != self.rows.len()
            || self.shapes.len() != self.rows.len()
            || !closed_module(&program.module)
            || !program.task_interfaces.is_empty()
        {
            return None;
        }
        for (index, ((row, names), shapes)) in self
            .rows
            .iter_mut()
            .zip(&self.names)
            .zip(&self.shapes)
            .enumerate()
        {
            let function = program.module.functions.get(index)?;
            if usize::try_from(function.id.0).ok()? != index
                || row.id != function.id
                || !closed_function(function)
            {
                return None;
            }
            row.owner = Some(produced.function_owner(row.module, &row.name)?.clone());
            row.names = *names;
            row.shape = *shapes;
            row.post = PostReceipt::check(function, limits)?;
            row.visibility = instruction_count(function)?;
        }
        let rows = ReceiptTree::build(self.rows)?;
        let total = rows.total()?;
        // All post-body shared work was really charged by the old full pass.
        // In this closed primitive domain only the independently checked per-
        // function resource scans vary with CFG shape; interface work is fixed.
        let fixed_tail = limits
            .max_analysis_steps
            .checked_sub(self.remaining)?
            .checked_sub(self.body_cost[4])?
            .checked_sub(total.post.shared)?;
        for i in 0..6 {
            if self.context_cost[i].checked_add(total.cfg[i])? != self.body_cost[i] {
                return None;
            }
        }
        let context = Context {
            epoch: produced.change_batch().epoch().clone(),
            input_context: produced.change_batch().context().clone(),
            target: target.clone(),
            limits: limits.clone(),
            tables: self.tables?,
            rows,
            context_cost: self.context_cost,
            fixed_tail,
            interface_indexes: Arc::new(
                program
                    .interfaces
                    .iter()
                    .enumerate()
                    .map(|(i, m)| (m.module, i))
                    .collect(),
            ),
            program,
        };
        context.fits(total, limits).then_some(context)
    }
}
#[derive(Clone)]
struct Row {
    id: FunctionId,
    module: ModuleId,
    node: NodeId,
    name: String,
    owner: Option<FunctionOwner>,
    span: Span,
    cfg: [usize; 6],
    names: usize,
    shape: usize,
    visibility: usize,
    post: PostReceipt,
}
#[derive(Clone, Copy, Default)]
struct PostReceipt {
    shared: usize,
    bytes: usize,
    stores: usize,
}
impl PostReceipt {
    fn check(function: &TypedFunction, limits: &Limits) -> Option<Self> {
        if !closed_function(function) {
            return None;
        }
        let functions = std::slice::from_ref(function);
        let mut definitions = Vec::new();
        let mut remaining = limits.max_analysis_steps;
        if tasks::add_definition(functions, &mut definitions, &mut remaining, limits).ok()? {
            return None;
        }
        registration::add_definitions(functions, &mut definitions, &mut remaining).ok()?;
        byte_buffer::add_definition(functions, &mut definitions, limits).ok()?;
        dynamic_store::add_definitions(
            functions,
            &mut definitions,
            &[],
            &[],
            limits,
            &mut remaining,
        )
        .ok()?;
        if !definitions.is_empty() {
            return None;
        }
        // These two independent ledgers reserve roots before walking. The
        // exhaustive primitive predicate below admits no child visits/outputs.
        let roots = function
            .values
            .len()
            .checked_add(function.locals.len())?
            .checked_add(1)?;
        Some(Self {
            shared: limits.max_analysis_steps - remaining,
            bytes: roots.checked_add(1)?,
            stores: roots,
        })
    }
}
#[derive(Clone, Copy, Default)]
struct Total {
    cfg: [usize; 6],
    names: usize,
    shape: usize,
    visibility: usize,
    post: PostReceipt,
}
impl Total {
    fn row(row: &Row) -> Self {
        Self {
            cfg: row.cfg,
            names: row.names,
            shape: row.shape,
            visibility: row.visibility,
            post: row.post,
        }
    }
    fn add(self, other: Self) -> Option<Self> {
        let mut cfg = [0; 6];
        for (i, value) in cfg.iter_mut().enumerate() {
            *value = self.cfg[i].checked_add(other.cfg[i])?;
        }
        Some(Self {
            cfg,
            names: self.names.checked_add(other.names)?,
            shape: self.shape.checked_add(other.shape)?,
            visibility: self.visibility.checked_add(other.visibility)?,
            post: PostReceipt {
                shared: self.post.shared.checked_add(other.post.shared)?,
                bytes: self.post.bytes.checked_add(other.post.bytes)?,
                stores: self.post.stores.checked_add(other.post.stores)?,
            },
        })
    }
}
#[derive(Clone)]
struct ReceiptTree(Arc<ReceiptNode>);
struct ReceiptNode {
    len: usize,
    total: Option<Total>,
    kind: ReceiptKind,
}
enum ReceiptKind {
    Leaf(Row),
    Branch(ReceiptTree, ReceiptTree),
}
impl ReceiptTree {
    fn build(rows: Vec<Row>) -> Option<Self> {
        if rows.is_empty() {
            return None;
        }
        fn build(mut rows: Vec<Row>) -> ReceiptTree {
            if rows.len() == 1 {
                let row = rows.pop().expect("one receipt");
                return ReceiptTree(Arc::new(ReceiptNode {
                    len: 1,
                    total: Some(Total::row(&row)),
                    kind: ReceiptKind::Leaf(row),
                }));
            }
            let right = rows.split_off(rows.len() / 2);
            ReceiptTree::branch(build(rows), build(right))
        }
        let result = build(rows);
        result.total()?;
        Some(result)
    }
    fn branch(left: Self, right: Self) -> Self {
        Self(Arc::new(ReceiptNode {
            len: left.0.len + right.0.len,
            total: left.total().zip(right.total()).and_then(|(a, b)| a.add(b)),
            kind: ReceiptKind::Branch(left, right),
        }))
    }
    fn total(&self) -> Option<Total> {
        self.0.total
    }
    fn get(&self, index: usize) -> Option<&Row> {
        if index >= self.0.len {
            return None;
        }
        match &self.0.kind {
            ReceiptKind::Leaf(row) => Some(row),
            ReceiptKind::Branch(left, right) => {
                if index < left.0.len {
                    left.get(index)
                } else {
                    right.get(index - left.0.len)
                }
            }
        }
    }
    fn replace(&self, index: usize, row: Row) -> Option<Self> {
        if index >= self.0.len {
            return None;
        }
        Some(match &self.0.kind {
            ReceiptKind::Leaf(_) => Self(Arc::new(ReceiptNode {
                len: 1,
                total: Some(Total::row(&row)),
                kind: ReceiptKind::Leaf(row),
            })),
            ReceiptKind::Branch(left, right) => {
                if index < left.0.len {
                    Self::branch(left.replace(index, row)?, right.clone())
                } else {
                    Self::branch(left.clone(), right.replace(index - left.0.len, row)?)
                }
            }
        })
    }
}
fn primitive(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}
fn closed_function(f: &TypedFunction) -> bool {
    f.call_kind == CallKind::Direct
        && f.storage_life.is_none()
        && f.reference_return_bound.is_none()
        && f.callable_return_contract.is_none()
        && f.callable_contract.is_none()
        && f.protocol_contract.is_none()
        && f.environment.is_none()
        && primitive(&f.return_type)
        && f.locals.iter().all(|l| primitive(&l.ty))
        && f.values.iter().all(primitive)
}
fn closed_module(m: &TypedModule) -> bool {
    m.closures.is_empty()
        && m.resources.is_empty()
        && m.unsupported.is_empty()
        && m.life_declarations.is_empty()
        && m.life_instances.is_empty()
        && m.dyn_declarations.is_empty()
        && m.dyn_interfaces.is_empty()
        && m.dyn_vtables.is_empty()
}
fn instruction_count(f: &TypedFunction) -> Option<usize> {
    f.blocks
        .iter()
        .try_fold(0usize, |n, b| n.checked_add(b.instructions.len()))
}
fn relative_equal(left: &TypedFunction, right: &TypedFunction) -> Option<bool> {
    let mut left = left.clone();
    let mut right = right.clone();
    let a = Span::new(
        zil_base::FileId(0),
        0,
        left.span.end.checked_sub(left.span.start)?,
    );
    let b = Span::new(
        zil_base::FileId(0),
        0,
        right.span.end.checked_sub(right.span.start)?,
    );
    let left_origin = left.span;
    let right_origin = right.span;
    if !incremental::rebase(&mut left, left_origin, a)
        || !incremental::rebase(&mut right, right_origin, b)
    {
        return None;
    }
    Some(left == right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, SourceUnit};
    use zil_expand::{
        ExpandSession,
        source_context::{DeclarationPlacement, SourceIdentityContext},
    };
    use zil_syntax::{ParsedSource, attributes::AttributeContext};

    fn parse(id: u32, text: &str, limits: &Limits) -> ParsedSource {
        let parsed = ParsedSource::parse(
            Arc::new(SourceUnit::new(
                FileId(id),
                ModuleId(id),
                format!("module{id}.zil"),
                text,
            )),
            limits,
            &AttributeContext::default(),
        );
        assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
        parsed
    }
    fn produced(
        session: &mut ExpandSession,
        sources: &[ParsedSource],
        limits: &Limits,
    ) -> ProducedProgram {
        let aliases = BTreeMap::new();
        session
            .expand_parsed(
                sources,
                &vec![vec![]; sources.len()],
                limits,
                &TargetProfile::default(),
                SourceIdentityContext::ClosedSnapshot {
                    root: ModuleId(0),
                    aliases: &aliases,
                    declarations: DeclarationPlacement::Original,
                },
            )
            .unwrap()
            .expect("closed source producer")
            .into_program(ModuleId(0), Fingerprint::ZERO)
            .0
    }
    fn prepared(produced: &ProducedProgram, limits: &Limits) -> PreparedProgram {
        infer_produced_program_for_ct(produced, limits)
            .unwrap()
            .into_prepared()
    }
    fn fresh(prepared: &PreparedProgram, limits: &Limits) -> CompileResult<TypedProgram> {
        check_prepared_program(prepared, &TargetProfile::default(), limits)
    }
    fn apply(
        session: &mut TypesPushSession,
        prepared: &PreparedProgram,
        limits: &Limits,
    ) -> CompileResult<TypedUpdate> {
        session.apply(prepared, &TargetProfile::default(), limits)
    }
    fn scalar_module() -> &'static str {
        "fn first(x:i32)->i32 {let a=x+1;let b=a*2;return b+3;} fn second(x:i32)->i32 {let a=x+2;let b=a*3;return b+4;} fn third(x:i32)->i32 {let a=x+3;let b=a*4;return b+5;}"
    }
    #[test]
    fn actual_sparse_cfg_consumers_share_untouched_leaves_and_match_fresh() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let mut input = [parse(0, scalar_module(), &limits)];
        let initial = produced(&mut expand, &input, &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        assert!(session.current.is_some(), "actual closed context admitted");
        let next = produced(&mut expand, &input, &limits);
        let warm = apply(&mut session, &prepared(&next, &limits), &limits).unwrap();
        assert_eq!(warm.stats.sparse_batches, 1);
        assert_eq!(warm.stats.body_checks, 0);
        assert!(
            cold.program
                .module
                .functions
                .ptr_eq(&warm.program.module.functions)
        );
        assert!(warm.changes.is_empty());
        input[0] = parse(0, &scalar_module().replace("x+1", "x+111"), &limits);
        let current = produced(&mut expand, &input, &limits);
        let current = prepared(&current, &limits);
        let edited = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(edited.stats.body_checks, 1);
        assert_eq!(edited.stats.position_rebindings, 2);
        assert_eq!(edited.stats.context_rebuilds, 0);
        assert_eq!(edited.program.as_ref(), &fresh(&current, &limits).unwrap());
        assert_eq!(
            edited
                .program
                .module
                .functions
                .changed_indices(&warm.program.module.functions),
            [0, 1, 2]
        );
        // Replace a same-width body next: real unchanged CFG leaves, not only a
        // product sidecar, retain their owners through the actual program.
        input[0] = parse(
            0,
            &input[0].source().text.replace("x+111", "x+222"),
            &limits,
        );
        let next = produced(&mut expand, &input, &limits);
        let next = prepared(&next, &limits);
        let same_width = apply(&mut session, &next, &limits).unwrap();
        assert_eq!(same_width.stats.body_checks, 1);
        assert_eq!(same_width.stats.position_rebindings, 0);
        assert_eq!(
            same_width
                .program
                .module
                .functions
                .changed_indices(&edited.program.module.functions),
            [0]
        );
        for index in [1, 2] {
            assert!(Arc::ptr_eq(
                edited.program.module.functions.owner(index).unwrap(),
                same_width.program.module.functions.owner(index).unwrap()
            ));
        }
        assert_eq!(same_width.program.as_ref(), &fresh(&next, &limits).unwrap());
        assert_eq!(
            cold.program.as_ref(),
            &fresh(&prepared(&initial, &limits), &limits).unwrap()
        );
    }
    #[test]
    fn error_history_limits_and_unsealed_inputs_cannot_retarget_current_context() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let cold_source = parse(0, scalar_module(), &limits);
        let cold_input = produced(&mut expand, &[cold_source.clone()], &limits);
        let cold = apply(&mut session, &prepared(&cold_input, &limits), &limits).unwrap();
        let successful = session.current.clone().unwrap();
        let bad_source = parse(
            0,
            &scalar_module().replace("return b+3", "return missing(b)"),
            &limits,
        );
        let bad = produced(&mut expand, &[bad_source], &limits);
        let bad = prepared(&bad, &limits);
        let actual = apply(&mut session, &bad, &limits).unwrap_err();
        assert_eq!(actual, fresh(&bad, &limits).unwrap_err());
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &successful));
        let recovered = produced(&mut expand, &[cold_source], &limits);
        let recovered = prepared(&recovered, &limits);
        let recovered = apply(&mut session, &recovered, &limits).unwrap();
        assert_eq!(
            recovered.stats.context_rebuilds, 1,
            "failed producer epoch is not the last successful Types epoch"
        );
        assert_eq!(recovered.program, cold.program);
        let blocks = cold
            .program
            .module
            .functions
            .iter()
            .map(|f| f.blocks.len())
            .sum::<usize>();
        assert!(blocks > 1);
        let too_small = Limits {
            max_blocks: blocks - 1,
            ..limits.clone()
        };
        let raw = prepared(&cold_input, &too_small);
        assert_eq!(
            apply(&mut session, &raw, &too_small).unwrap_err(),
            fresh(&raw, &too_small).unwrap_err()
        );
        // Materializing an arbitrary Core removes the producer seal. Even a
        // forged source hash and byte-equal copy cannot activate sparse reuse.
        let shared = Arc::new(cold_input.program().as_ref().clone());
        let raw = infer_shared_program_for_ct(&shared, &limits)
            .unwrap()
            .into_prepared();
        let unsealed = apply(&mut session, &raw, &limits).unwrap();
        assert_eq!(unsealed.stats.context_rebuilds, 1);
        assert!(session.current.is_none());
        let weak = Arc::downgrade(&successful);
        drop(successful);
        session.clear();
        assert!(
            weak.upgrade().is_none(),
            "no independent context history survives eviction"
        );
    }
    #[test]
    fn complete_integer_and_foreign_headers_remain_current() {
        let limits = Limits::default();
        let text = [
            "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize", "f32", "f64",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, ty)| {
            format!(
                "fn value{i}(x:{ty})->{ty} {{let y=x+{};return y;}} ",
                if ty.starts_with('f') { "1.0" } else { "1" }
            )
        })
        .collect::<String>()
            + "extern \"C\" fn foreign_value(x:i32)->i32;";
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        assert!(session.current.is_some());
        let next = produced(
            &mut expand,
            &[parse(0, &text.replace("let y=x+1", "let y=x+2"), &limits)],
            &limits,
        );
        let next = prepared(&next, &limits);
        let actual = apply(&mut session, &next, &limits).unwrap();
        assert_eq!(actual.stats.context_rebuilds, 0);
        assert_eq!(actual.stats.body_checks, 12);
        assert_eq!(actual.program.as_ref(), &fresh(&next, &limits).unwrap());
        assert!(Arc::ptr_eq(
            cold.program.module.functions.owner(12).unwrap(),
            actual.program.module.functions.owner(12).unwrap()
        ));
    }
    #[test]
    fn full_425_function_history_updates_actual_cfg_leaves_without_full_types_inventory() {
        let limits = Limits::default();
        let mut input = vec![
            parse(0, "fn main(x:i32)->i32=>x+1;", &limits),
            parse(
                1,
                "extern \"C\" fn io0(x:i32)->i32;extern \"C\" fn io1(x:i32)->i32;extern \"C\" fn io2(x:i32)->i32;extern \"C\" fn io3(x:i32)->i32;",
                &limits,
            ),
        ];
        for module in 2..22 {
            let text = (0..21)
                .map(|function| {
                    let mut body = format!("fn helper{function}(x:i32)->i32 {{let v0=x+11;");
                    for step in 1..16 {
                        body += &format!("let v{step}=v{}*3+{};", step - 1, step + 1);
                    }
                    body + "return v15;}"
                })
                .collect::<String>();
            input.push(parse(module, &text, &limits));
        }
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let cold_input = produced(&mut expand, &input, &limits);
        let cold = apply(&mut session, &prepared(&cold_input, &limits), &limits).unwrap();
        assert_eq!(cold.program.module.functions.len(), 425);
        assert!(session.current.is_some());
        input[2] = parse(
            2,
            &input[2].source().text.replacen("x+11", "x+111", 1),
            &limits,
        );
        let edited = produced(&mut expand, &input, &limits);
        let edited = prepared(&edited, &limits);
        let edited = apply(&mut session, &edited, &limits).unwrap();
        assert_eq!(edited.stats.body_checks, 1);
        assert_eq!(edited.stats.position_rebindings, 20);
        assert_eq!(edited.stats.retained_functions, 404);
        assert_eq!(edited.stats.context_rebuilds, 0);
        assert_eq!(
            edited
                .program
                .module
                .functions
                .changed_indices(&cold.program.module.functions)
                .len(),
            21
        );
        for index in 26..425 {
            assert!(Arc::ptr_eq(
                cold.program.module.functions.owner(index).unwrap(),
                edited.program.module.functions.owner(index).unwrap()
            ));
        }
        for (index, source) in input.iter_mut().enumerate() {
            *source = parse(
                index as u32,
                &("// current positions\n".to_owned() + &source.source().text),
                &limits,
            );
        }
        let positioned = produced(&mut expand, &input, &limits);
        let positioned = prepared(&positioned, &limits);
        let positions = apply(&mut session, &positioned, &limits).unwrap();
        assert_eq!(positions.stats.body_checks, 0);
        assert_eq!(positions.stats.position_rebindings, 425);
        assert_eq!(positions.stats.context_rebuilds, 0);
        assert_eq!(
            positions.program.as_ref(),
            &fresh(&positioned, &limits).unwrap()
        );
    }
}
