//! Function-scoped raw CFG work products. Nothing here carries Verify authority.
use super::*;
use std::hash::Hash;
use std::sync::Arc;
use zil_base::{FileId, Fingerprint, FingerprintBuilder, ModuleId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FunctionStats {
    pub reused_functions: usize,
    pub executed_functions: usize,
}

#[derive(Clone)]
struct Entry {
    // All spans are relative to the source function; IDs remain explicitly bound.
    function: TypedFunction,
    cost: [usize; 6],
}

/// Holds only the most recent successful scalar Types epoch, not an unbounded
/// edit history. Complete declaration contexts and current checks stay fresh.
#[derive(Clone, Default)]
pub struct FunctionSession {
    entries: Arc<BTreeMap<Fingerprint, Arc<Entry>>>,
    stats: FunctionStats,
}
impl FunctionSession {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn last_stats(&self) -> FunctionStats {
        self.stats
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn check_prepared_program(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<TypedProgram> {
        // Preserve the existing inference/shape path, including all raw limits.
        let inferred;
        let resolved;
        let program = if prepared.identity_inference(limits) {
            resolved = aggregates::resolve_prepared_program(prepared.program(), limits)?;
            &*resolved
        } else {
            inferred = program::resolve_program_shapes(prepared.program(), limits)?;
            &inferred
        };
        let mut next = BTreeMap::new();
        let mut stats = FunctionStats::default();
        let mut reuse = Reuse {
            prior: &self.entries,
            next: &mut next,
            stats: &mut stats,
            keys: None,
        };
        let result =
            program::check_resolved_program_with_reuse(program, target, limits, Some(&mut reuse));
        if result.is_ok() {
            self.entries = Arc::new(next);
        }
        self.stats = stats;
        result
    }

    /// Optional transport of raw, explicitly trusted local work products. The
    /// caller must bind the exact compiler and protect the artifact destination.
    pub fn encode_work_products(&self, target: &TargetProfile, limits: &Limits) -> Option<Vec<u8>> {
        if self.entries.is_empty() || self.entries.len() > limits.max_functions {
            return None;
        }
        let mut bytes = b"ZILFUN01".to_vec();
        bytes.extend_from_slice(&u32::try_from(self.entries.len()).ok()?.to_le_bytes());
        let mut functions = Vec::new();
        for (key, entry) in self.entries.iter() {
            bytes.extend_from_slice(&key.0);
            for cost in entry.cost {
                bytes.extend_from_slice(&u64::try_from(cost).ok()?.to_le_bytes());
            }
            if bytes.len() > limits.max_source_bytes {
                return None;
            }
            functions.push(entry.function.clone());
        }
        let module = TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: target.clone(),
            functions,
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
        };
        let candidate = CandidatePlan {
            schema_version: SCHEMA_VERSION,
            module_fingerprint: Fingerprint::ZERO,
            target_fingerprint: Fingerprint::ZERO,
            functions: vec![],
            origins: vec![],
            responsibilities: vec![],
            unsupported: vec![],
        };
        let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, limits).ok()?;
        if bytes.len().checked_add(wire.len())? > limits.max_source_bytes {
            return None;
        }
        bytes.extend_from_slice(&wire);
        Some(bytes)
    }

    pub fn restore_work_products(
        &mut self,
        bytes: &[u8],
        target: &TargetProfile,
        limits: &Limits,
    ) -> bool {
        let Some(entries) = decode(bytes, target, limits) else {
            return false;
        };
        self.entries = Arc::new(entries);
        self.stats = FunctionStats::default();
        true
    }
}

fn decode(
    bytes: &[u8],
    target: &TargetProfile,
    limits: &Limits,
) -> Option<BTreeMap<Fingerprint, Arc<Entry>>> {
    if bytes.len() > limits.max_source_bytes || bytes.get(..8)? != b"ZILFUN01" {
        return None;
    }
    let count = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?) as usize;
    if count > limits.max_functions {
        return None;
    }
    let boundary = 12usize.checked_add(count.checked_mul(80)?)?;
    let metadata = bytes.get(12..boundary)?;
    let (module, candidate) =
        zil_ir::wire::decode_module_candidate(bytes.get(boundary..)?, limits).ok()?;
    if module.target != *target
        || module.functions.len() != count
        || !module.closures.is_empty()
        || !module.resources.is_empty()
        || !module.unsupported.is_empty()
        || !candidate.functions.is_empty()
        || !candidate.origins.is_empty()
        || !candidate.responsibilities.is_empty()
        || !candidate.unsupported.is_empty()
    {
        return None;
    }
    let maxima = [
        limits.max_blocks,
        limits.max_locals,
        limits.max_values,
        limits.max_instructions,
        limits.max_analysis_steps,
        limits.max_nodes,
    ];
    let mut entries = BTreeMap::new();
    for (record, function) in metadata.as_chunks::<80>().0.iter().zip(module.functions) {
        if !scalar_typed(&function) || function.span.file != FileId(0) || function.span.start != 0 {
            return None;
        }
        let key = Fingerprint(record[..32].try_into().ok()?);
        let mut cost = [0; 6];
        for (i, chunk) in record[32..].as_chunks::<8>().0.iter().enumerate() {
            cost[i] = usize::try_from(u64::from_le_bytes(*chunk)).ok()?;
            if cost[i] > maxima[i] {
                return None;
            }
        }
        if entries
            .insert(key, Arc::new(Entry { function, cost }))
            .is_some()
        {
            return None;
        }
    }
    Some(entries)
}

pub(super) struct Reuse<'a> {
    prior: &'a BTreeMap<Fingerprint, Arc<Entry>>,
    next: &'a mut BTreeMap<Fingerprint, Arc<Entry>>,
    stats: &'a mut FunctionStats,
    keys: Option<BTreeMap<(ModuleId, String), Fingerprint>>,
}
impl Reuse<'_> {
    pub(super) fn prepare(
        &mut self,
        program: &CoreProgram,
        tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
        target: &TargetProfile,
        limits: &Limits,
    ) {
        self.keys = keys(program, tables, target, limits);
    }
    pub(super) fn get(
        &mut self,
        module: ModuleId,
        function: &CoreFunction,
        state: &mut closures::State,
    ) -> Option<TypedFunction> {
        let key = self.keys.as_ref()?.get(&(module, function.name.clone()))?;
        let entry = self.prior.get(key)?;
        let mut typed = entry.function.clone();
        if !rebase(
            &mut typed,
            Span::new(
                FileId(0),
                0,
                function.span.end.checked_sub(function.span.start)?,
            ),
            function.span,
        ) || !state.replay_accounting(entry.cost)
        {
            return None;
        }
        self.next.insert(*key, entry.clone());
        self.stats.reused_functions += 1;
        Some(typed)
    }
    pub(super) fn save(
        &mut self,
        module: ModuleId,
        source: &CoreFunction,
        function: &TypedFunction,
        before: [usize; 6],
        after: [usize; 6],
    ) {
        self.stats.executed_functions += 1;
        let Some(key) = self
            .keys
            .as_ref()
            .and_then(|keys| keys.get(&(module, source.name.clone())))
        else {
            return;
        };
        if !scalar_typed(function) {
            return;
        }
        let mut function = function.clone();
        let Some(length) = source.span.end.checked_sub(source.span.start) else {
            return;
        };
        if !rebase(&mut function, source.span, Span::new(FileId(0), 0, length)) {
            return;
        }
        let mut cost = [0; 6];
        for i in 0..6 {
            let Some(delta) = after[i].checked_sub(before[i]) else {
                return;
            };
            cost[i] = delta;
        }
        self.next.insert(*key, Arc::new(Entry { function, cost }));
    }
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::I32 | Type::I64 | Type::U8 | Type::U16
    )
}
fn scalar_typed(f: &TypedFunction) -> bool {
    f.foreign.is_none()
        && f.environment.is_none()
        && f.callable_contract.is_none()
        && f.callable_return_contract.is_none()
        && scalar(&f.return_type)
        && f.locals.iter().all(|l| scalar(&l.ty))
        && f.values.iter().all(scalar)
}
fn rebase(f: &mut TypedFunction, old: Span, new: Span) -> bool {
    let convert = |span: &mut Span| -> Option<()> {
        if span.file != old.file
            || span.start < old.start
            || span.end > old.end
            || span.start > span.end
        {
            return None;
        }
        *span = Span::new(
            new.file,
            new.start.checked_add(span.start - old.start)?,
            new.start.checked_add(span.end - old.start)?,
        );
        if span.end > new.end {
            return None;
        }
        Some(())
    };
    convert(&mut f.span).is_some()
        && f.locals.iter_mut().all(|l| convert(&mut l.span).is_some())
        && f.blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .all(|i| convert(&mut i.span).is_some())
}

// Hashing, rebasing and transporting a CFG can cost more than rebuilding a
// tiny scalar leaf. This cheap source-shape gate is a scheduling choice only:
// excluded functions retain the full original builder and budget behavior.
fn worthwhile(function: &CoreFunction) -> bool {
    function.foreign.is_none()
        && (function.body.statements.len() >= 4
            || function.body.statements.iter().any(|statement| {
                matches!(
                    statement.kind,
                    CoreStatementKind::If { .. }
                        | CoreStatementKind::While { .. }
                        | CoreStatementKind::ForRange { .. }
                )
            }))
}

fn keys(
    program: &CoreProgram,
    tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    target: &TargetProfile,
    limits: &Limits,
) -> Option<BTreeMap<(ModuleId, String), Fingerprint>> {
    if !program
        .modules
        .iter()
        .flat_map(|module| &module.core.functions)
        .any(worthwhile)
    {
        return None;
    }
    let mut context = FingerprintBuilder::new();
    context.update(b"ZIL-FUNCTION-TYPES-1\0");
    target.hash(&mut context);
    // Limits is an explicit complete struct so adding a field forces review.
    let Limits {
        max_source_bytes,
        max_tokens,
        max_nodes,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_analysis_steps,
        max_ct_steps,
        max_call_depth,
    } = limits;
    (
        max_source_bytes,
        max_tokens,
        max_nodes,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_analysis_steps,
        max_ct_steps,
        max_call_depth,
    )
        .hash(&mut context);
    program.root.hash(&mut context);
    program.modules.len().hash(&mut context);
    for module in &program.modules {
        let core = &module.core;
        if core.schema_version != SCHEMA_VERSION
            || !core.aggregate_templates.is_empty()
            || !core.aggregate_applications.is_empty()
            || !core.aggregates.is_empty()
            || !core.enums.is_empty()
            || !core.unsupported.is_empty()
        {
            return None;
        }
        core.module_id.hash(&mut context);
        module.imports.len().hash(&mut context);
        for import in &module.imports {
            import.alias.hash(&mut context);
            import.module.hash(&mut context);
        }
        core.functions.len().hash(&mut context);
        for f in &core.functions {
            if !f.type_parameters.is_empty()
                || !f.state_parameters.is_empty()
                || !f.callable_parameters.is_empty()
                || f.callable_return.is_some()
                || f.receiver.is_some()
                || (f.foreign.is_none()
                    && (!scalar(&f.return_type) || !f.parameters.iter().all(|p| scalar(&p.ty))))
            {
                return None;
            }
            let signature = tables.get(&core.module_id)?.get(&f.name)?;
            signature.id.hash(&mut context);
            signature.public.hash(&mut context);
            f.name.hash(&mut context);
            f.foreign.is_some().hash(&mut context);
            if let Some(foreign) = &f.foreign {
                foreign.abi.hash(&mut context);
                foreign.symbol.hash(&mut context);
                foreign.noescape_parameters.hash(&mut context);
                foreign.static_callbacks.hash(&mut context);
            }
            f.return_type.hash(&mut context);
            f.result_mode.hash(&mut context);
            f.parameters.len().hash(&mut context);
            for p in &f.parameters {
                p.name.hash(&mut context);
                p.ty.hash(&mut context);
                p.mode.hash(&mut context);
            }
        }
    }
    let context = context.digest();
    let mut keys = BTreeMap::new();
    for module in &program.modules {
        for f in &module.core.functions {
            if !worthwhile(f) {
                continue;
            }
            let mut encoder = BodyKey {
                hash: FingerprintBuilder::new(),
                origin: f.span,
                depth: limits.max_call_depth,
                remaining: limits.max_nodes,
                module,
                tables,
            };
            encoder.hash.update(&context.0);
            module.core.module_id.hash(&mut encoder.hash);
            f.name.hash(&mut encoder.hash);
            if encoder.span(f.span).is_none()
                || f.parameters.iter().any(|p| encoder.span(p.span).is_none())
                || encoder.block(&f.body, 0).is_none()
            {
                continue;
            }
            keys.insert(
                (module.core.module_id, f.name.clone()),
                encoder.hash.digest(),
            );
        }
    }
    Some(keys)
}

struct BodyKey<'a> {
    hash: FingerprintBuilder,
    origin: Span,
    depth: usize,
    remaining: usize,
    module: &'a ProgramModule,
    tables: &'a BTreeMap<ModuleId, BTreeMap<String, Signature>>,
}
impl BodyKey<'_> {
    fn visit(&mut self, depth: usize) -> Option<()> {
        self.remaining = self.remaining.checked_sub(1)?;
        (depth <= self.depth).then_some(())
    }
    fn span(&mut self, span: Span) -> Option<()> {
        if span.file != self.origin.file
            || span.start < self.origin.start
            || span.end > self.origin.end
            || span.start > span.end
        {
            return None;
        }
        (span.start - self.origin.start).hash(&mut self.hash);
        (span.end - self.origin.start).hash(&mut self.hash);
        Some(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> Option<()> {
        self.visit(depth)?;
        self.span(block.span)?;
        block.statements.len().hash(&mut self.hash);
        for s in &block.statements {
            self.visit(depth)?;
            self.span(s.span)?;
            match &s.kind {
                CoreStatementKind::Binding {
                    name,
                    mutable,
                    ty,
                    initializer,
                } => {
                    0u8.hash(&mut self.hash);
                    name.hash(&mut self.hash);
                    mutable.hash(&mut self.hash);
                    if ty.as_ref().is_some_and(|t| !scalar(t)) {
                        return None;
                    }
                    ty.hash(&mut self.hash);
                    initializer.is_some().hash(&mut self.hash);
                    if let Some(e) = initializer {
                        self.expr(e, depth + 1)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    1u8.hash(&mut self.hash);
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Expression(e) => {
                    2u8.hash(&mut self.hash);
                    self.expr(e, depth + 1)?;
                }
                CoreStatementKind::Return(e) => {
                    3u8.hash(&mut self.hash);
                    e.is_some().hash(&mut self.hash);
                    if let Some(e) = e {
                        self.expr(e, depth + 1)?;
                    }
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    4u8.hash(&mut self.hash);
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    else_body.is_some().hash(&mut self.hash);
                    if let Some(b) = else_body {
                        self.block(b, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    5u8.hash(&mut self.hash);
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    6u8.hash(&mut self.hash);
                    name.hash(&mut self.hash);
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::Block(b) => {
                    7u8.hash(&mut self.hash);
                    self.block(b, depth + 1)?;
                }
                CoreStatementKind::Break => 8u8.hash(&mut self.hash),
                CoreStatementKind::Continue => 9u8.hash(&mut self.hash),
                _ => return None,
            }
        }
        Some(())
    }
    fn call(&self, callee: &CoreExpr) -> Option<()> {
        match &callee.kind {
            CoreExprKind::Name(name) => {
                if self
                    .tables
                    .get(&self.module.core.module_id)?
                    .get(name)?
                    .foreign
                {
                    return None;
                }
            }
            CoreExprKind::Member { object, member } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return None;
                };
                let import = self.module.imports.iter().find(|i| &i.alias == alias)?;
                if self.tables.get(&import.module)?.get(member)?.foreign {
                    return None;
                }
            }
            _ => return None,
        }
        Some(())
    }
    fn expr(&mut self, e: &CoreExpr, depth: usize) -> Option<()> {
        self.visit(depth)?;
        self.span(e.span)?;
        match &e.kind {
            CoreExprKind::Literal(c) if scalar(&c.ty()) => {
                0u8.hash(&mut self.hash);
                c.hash(&mut self.hash);
            }
            CoreExprKind::Integer(s) => {
                1u8.hash(&mut self.hash);
                s.hash(&mut self.hash);
            }
            CoreExprKind::Bool(b) => {
                2u8.hash(&mut self.hash);
                b.hash(&mut self.hash);
            }
            CoreExprKind::Unit => 3u8.hash(&mut self.hash),
            CoreExprKind::Name(n) => {
                4u8.hash(&mut self.hash);
                n.hash(&mut self.hash);
            }
            CoreExprKind::Unary { op, operand }
                if matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot) =>
            {
                5u8.hash(&mut self.hash);
                op.hash(&mut self.hash);
                self.expr(operand, depth + 1)?;
            }
            CoreExprKind::Binary { op, left, right } => {
                6u8.hash(&mut self.hash);
                op.hash(&mut self.hash);
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::IntegerConvert { value, target } if scalar(target) => {
                7u8.hash(&mut self.hash);
                target.hash(&mut self.hash);
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Ascribed { ty, value } if scalar(ty) => {
                8u8.hash(&mut self.hash);
                ty.hash(&mut self.hash);
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.call(callee)?;
                9u8.hash(&mut self.hash);
                self.expr(callee, depth + 1)?;
                arguments.len().hash(&mut self.hash);
                for a in arguments {
                    self.expr(a, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, member }
                if matches!(object.kind, CoreExprKind::Name(_)) =>
            {
                10u8.hash(&mut self.hash);
                member.hash(&mut self.hash);
                self.expr(object, depth + 1)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                11u8.hash(&mut self.hash);
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            _ => return None,
        }
        Some(())
    }
}
