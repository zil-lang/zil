//! Bounded trusted raw Analyze results for direct process restart edits.
//! No Complete, Source, Verify, initialization receipt or filesystem stamp is
//! decoded. Current input/context equality and Analyze's original ledger select
//! reuse; all successful compilations still obtain current independent Verify.
use super::*;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::fs::OpenOptions;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_analyze::{RawAnalysisBase, RawAnalysisRow};
use zil_ir::TypedFunction;

const MAGIC: &[u8; 8] = b"ZILANR01";
const MANIFEST: &[u8; 8] = b"ZILANM01";
const MAXIMUM: usize = 16 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);
type Guard = super::super::artifacts::CacheNamespace;
type Plan = super::super::artifacts::ArtifactPlan;

fn private(path: &Path, create: bool) -> bool {
    if create {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        if builder.create(path).is_err() {
            return false;
        }
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return false;
        }
    }
    true
}
fn open(options: &CompileOptions, plan: &Plan, create: bool) -> Option<Guard> {
    if !options.push_reuse
        || options.stop_after < Stage::Verify
        || options.emit_dir.is_some()
        || options.cache_dir.is_some()
        || ["LD_PRELOAD", "LD_AUDIT"]
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    {
        return None;
    }
    let compiler = COMPILER
        .get_or_init(|| zil_backend_llvm::installed_compiler_identity().ok())
        .as_ref()?;
    let semantic = options.semantic_cache.as_ref()?;
    match fs::symlink_metadata(semantic) {
        Ok(_) if !private(semantic, false) => return None,
        Ok(_) => {}
        Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
    }
    let mut guarded = plan.clone();
    let root = guarded
        .cache_directory(
            "raw-analyze-recovery",
            &semantic.join("analyze-recovery-v1").join(compiler.to_hex()),
        )
        .ok()?;
    if !private(&root, create) || !private(semantic, false) {
        return None;
    }
    let namespace = guarded.pin_cache_directory(&root).ok()?;
    namespace.validate().ok()?;
    Some(namespace)
}

fn read(path: &Path, maximum: usize) -> Option<Vec<u8>> {
    let before = fs::symlink_metadata(path).ok()?;
    if !before.is_file() || before.file_type().is_symlink() || before.len() > maximum as u64 {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if before.permissions().mode() & 0o022 != 0 {
            return None;
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0o400000);
    }
    let mut file = options.open(path).ok()?;
    let opened = file.metadata().ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (before.dev(), before.ino()) != (opened.dev(), opened.ino()) {
            return None;
        }
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= maximum && bytes.len() == opened.len() as usize).then_some(bytes)
}

struct Input<'a> {
    bytes: &'a [u8],
    nodes: usize,
}
impl<'a> Input<'a> {
    fn take(&mut self, size: usize) -> Option<&'a [u8]> {
        let (value, rest) = self.bytes.split_at_checked(size)?;
        self.bytes = rest;
        Some(value)
    }
    fn word(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }
    fn cost(&mut self) -> Option<usize> {
        usize::try_from(u64::from_le_bytes(self.take(8)?.try_into().ok()?)).ok()
    }
    fn count(&mut self, maximum: usize) -> Option<usize> {
        let count = self.word()?;
        if count > maximum {
            return None;
        }
        self.nodes = self.nodes.checked_sub(count)?;
        Some(count)
    }
    fn blob(&mut self) -> Option<&'a [u8]> {
        let count = self.word()?;
        self.take(count)
    }
}
struct Template {
    input: Arc<TypedFunction>,
    plan: Arc<zil_ir::FunctionPlan>,
    span_sensitive: bool,
}
struct Encoded {
    input: Arc<[u8]>,
    plan: Arc<[u8]>,
}
type Owners = (usize, usize, usize);
fn owners(function: &zil_ir::TypedFunction) -> Owners {
    (
        std::ptr::from_ref(&*function.locals) as usize,
        std::ptr::from_ref(&*function.values) as usize,
        std::ptr::from_ref(&*function.blocks) as usize,
    )
}
fn template_eq(
    comparisons: &mut HashMap<(Owners, Owners), bool>,
    left: &TypedFunction,
    right: &TypedFunction,
) -> bool {
    if !RawAnalysisBase::same_template_headers(left, right) {
        return false;
    }
    *comparisons
        .entry((owners(left), owners(right)))
        .or_insert_with(|| RawAnalysisBase::same_template_columns(left, right))
}
fn put_blob(output: &mut Writer, bytes: &[u8]) -> Option<()> {
    output.number(bytes.len())?;
    output.put(bytes)
}

// Optional cache work has one current bounded balance. It never replenishes
// the Analyze ledger or reports a language diagnostic; exhaustion skips save.
struct Fuel(usize);
impl Fuel {
    fn spend(&mut self, amount: usize) -> Option<()> {
        self.0 = self.0.checked_sub(amount)?;
        Some(())
    }
}
struct Representative {
    input: Arc<TypedFunction>,
    plan: Arc<zil_ir::FunctionPlan>,
    sensitive: bool,
    input_work: usize,
    plan_work: usize,
}
const MAX_BUCKET: usize = 16;

#[derive(Default)]
struct Selectors(HashMap<Owners, Vec<(Arc<TypedFunction>, u64, usize)>>);
impl Selectors {
    fn get(
        &mut self,
        function: &Arc<TypedFunction>,
        random: &RandomState,
        fuel: &mut Fuel,
    ) -> Option<(u64, usize)> {
        let rows = self.0.entry(owners(function)).or_default();
        if let Some((_, selector, work)) = rows
            .iter()
            .find(|(old, _, _)| RawAnalysisBase::same_template_headers(old, function))
        {
            // Same current immutable columns and every varying header. Keep
            // the original per-row optional-cache allowance even on this hit.
            fuel.spend(*work)?;
            return Some((*selector, *work));
        }
        let (selector, work) = semantic_selector(function, random, fuel)?;
        rows.push((function.clone(), selector, work));
        Some((selector, work))
    }
}

// The selector never authorizes an alias. Unsupported/rich data is simply not
// indexed. All varying fields in the closed, non-sensitive vocabulary are
// hashed explicitly; only outer ID/name and diagnostic spans are omitted.
fn semantic_selector(
    function: &TypedFunction,
    random: &RandomState,
    fuel: &mut Fuel,
) -> Option<(u64, usize)> {
    use zil_ir::{CallKind, InstructionKind as I, Terminator, Type};
    let TypedFunction {
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
        id: _,
        name: _,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span: _,
    } = function;
    if *call_kind != CallKind::Direct
        || storage_life.is_some()
        || reference_return_bound.is_some()
        || callable_return_contract.is_some()
        || callable_contract.is_some()
        || protocol_contract.is_some()
        || foreign.is_some()
        || environment.is_some()
    {
        return None;
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
    let before = fuel.0;
    fuel.spend(parameters.len().checked_add(1)?)?;
    if !primitive(return_type) {
        return None;
    }
    let mut hash = random.build_hasher();
    parameters.hash(&mut hash);
    return_type.hash(&mut hash);
    result_mode.hash(&mut hash);
    entry.hash(&mut hash);
    locals.len().hash(&mut hash);
    for local in locals {
        fuel.spend(local.name.len().checked_add(1)?)?;
        if !primitive(&local.ty) {
            return None;
        }
        local.id.hash(&mut hash);
        local.name.hash(&mut hash);
        local.ty.hash(&mut hash);
        local.mutable.hash(&mut hash);
    }
    values.len().hash(&mut hash);
    for ty in values {
        fuel.spend(1)?;
        if !primitive(ty) {
            return None;
        }
        ty.hash(&mut hash);
    }
    blocks.len().hash(&mut hash);
    for block in blocks {
        fuel.spend(1)?;
        block.id.hash(&mut hash);
        std::mem::discriminant(&block.terminator).hash(&mut hash);
        match block.terminator {
            Terminator::Return(value) => value.hash(&mut hash),
            Terminator::Jump(target) => target.hash(&mut hash),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => (condition, then_block, else_block).hash(&mut hash),
            Terminator::Trap | Terminator::Unreachable => {}
        }
        block.instructions.len().hash(&mut hash);
        for instruction in &block.instructions {
            fuel.spend(1)?;
            instruction.result.hash(&mut hash);
            std::mem::discriminant(&instruction.kind).hash(&mut hash);
            match &instruction.kind {
                I::Constant(value) if primitive(&value.ty()) => value.hash(&mut hash),
                I::Load(id) | I::StorageLive(id) | I::StorageDead(id) => id.hash(&mut hash),
                I::Store { local, value } => (local, value).hash(&mut hash),
                I::Unary { op, operand } => (op, operand).hash(&mut hash),
                I::Binary { op, left, right } => (op, left, right).hash(&mut hash),
                I::ConvertInteger { value, target } | I::ConvertNumber { value, target }
                    if primitive(target) =>
                {
                    (value, target).hash(&mut hash)
                }
                I::Call {
                    function,
                    arguments,
                } => {
                    fuel.spend(arguments.len())?;
                    (function, arguments).hash(&mut hash);
                }
                I::ScopeEnter { scope } => scope.hash(&mut hash),
                I::ScopeExit { scope, kind } => (scope, kind).hash(&mut hash),
                _ => return None,
            }
        }
    }
    Some((hash.finish(), before - fuel.0))
}
fn plan_work(plan: &zil_ir::FunctionPlan, fuel: &mut Fuel) -> Option<usize> {
    let mut work = plan
        .blocks
        .len()
        .checked_add(plan.holds.len())?
        .checked_add(1)?;
    fuel.spend(work)?;
    for block in &plan.blocks {
        let count = block
            .initialized_in
            .len()
            .checked_add(block.initialized_out.len())?;
        fuel.spend(count)?;
        work = work.checked_add(count)?;
    }
    for hold in &plan.holds {
        fuel.spend(hold.roots.len())?;
        work = work.checked_add(hold.roots.len())?;
    }
    Some(work)
}
#[allow(clippy::too_many_arguments)] // One current body/plan comparison and its original budget.
fn same_representative(
    old: &Representative,
    function: &TypedFunction,
    plan: &zil_ir::FunctionPlan,
    sensitive: bool,
    input_work: usize,
    plan_work: usize,
    fuel: &mut Fuel,
    comparisons: &mut HashMap<(Owners, Owners), bool>,
) -> Option<bool> {
    // Reserve both sides, including the eligibility walks inside same_template,
    // before exact comparison. Even deliberately colliding buckets stay bounded.
    let body = old
        .input_work
        .checked_add(input_work)?
        .checked_add(old.input_work.min(input_work))?;
    fuel.spend(body.checked_add(old.plan_work)?.checked_add(plan_work)?)?;
    Some(
        old.sensitive == sensitive
            && (if sensitive {
                old.input.as_ref() == function
            } else {
                // Both inputs already passed span_sensitive in this request.
                // Recheck each header, share only the complete column relation;
                // the strong representatives/selector owners outlive this map.
                template_eq(comparisons, &old.input, function)
            })
            && old.plan.blocks == plan.blocks
            && old.plan.holds == plan.holds,
    )
}

pub(crate) fn load(
    options: &CompileOptions,
    artifacts: &Plan,
    program: &Arc<TypedProgram>,
    products: &[zil_types::FunctionProduct],
) -> Option<RawAnalysisBase> {
    let namespace = open(options, artifacts, false)?;
    let context = RawAnalysisBase::context_for(program, &options.limits)?;
    let manifest = read(&namespace.root().join("current.manifest"), 44)?;
    let mut header = Input {
        bytes: &manifest,
        nodes: 8,
    };
    if header.take(8)? != MANIFEST {
        return None;
    }
    let digest = Fingerprint(header.take(32)?.try_into().ok()?);
    let length = header.word()?;
    let maximum = options.limits.max_source_bytes.min(MAXIMUM);
    if !header.bytes.is_empty() || length > maximum {
        return None;
    }
    let bytes = read(
        &namespace.root().join(format!("{}.raw", digest.to_hex())),
        maximum,
    )?;
    if bytes.len() != length || Fingerprint::of(&bytes) != digest {
        return None;
    }
    let raw = decode(&bytes, &context, options, program, products)?;
    if options.cancellation.is_cancelled() || namespace.validate().is_err() {
        return None;
    }
    Some(raw)
}

fn decode(
    bytes: &[u8],
    context: &[u8],
    options: &CompileOptions,
    program: &Arc<TypedProgram>,
    products: &[zil_types::FunctionProduct],
) -> Option<RawAnalysisBase> {
    let limits = &options.limits;
    if bytes.len() > limits.max_source_bytes.min(MAXIMUM) {
        return None;
    }
    let mut input = Input {
        bytes,
        nodes: limits.max_nodes.min(1_000_000),
    };
    if input.take(8)? != MAGIC || input.blob()? != context {
        return None;
    }
    let global_cost = input.cost()?;
    if global_cost > limits.max_analysis_steps {
        return None;
    }
    let count = input.count(limits.max_functions.min(16_384))?;
    let mut encoded = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        encoded.push(Encoded {
            input: input.blob()?.into(),
            plan: input.blob()?.into(),
        });
    }
    let count = input.count(limits.max_functions.min(16_384))?;
    if count != program.module.functions.len() {
        return None;
    }
    let mut rows = Vec::with_capacity(count.min(4096));
    let mut uses = vec![Vec::new(); encoded.len()];
    let mut seen = 0;
    let mut total = global_cost;
    for index in 0..count {
        let template = input.word()?;
        if template >= encoded.len() || template > seen {
            return None;
        }
        if template == seen {
            seen += 1;
        }
        let mut costs = [0; 5];
        for cost in &mut costs {
            *cost = input.cost()?;
            total = total.checked_add(*cost)?;
        }
        if total > limits.max_analysis_steps {
            return None;
        }
        uses[template].push(index);
        rows.push((template, costs));
    }
    if !input.bytes.is_empty() || seen != encoded.len() {
        return None;
    }
    let products = products
        .iter()
        .map(|product| (product.id, product))
        .collect::<BTreeMap<_, _>>();
    let mut templates = Vec::with_capacity(encoded.len());
    for (index, record) in encoded.iter().enumerate() {
        if options.cancellation.is_cancelled() {
            return None;
        }
        // Actual decoder associations from the current Types restore can lend
        // immutable bodies only after COMPLETE raw byte/header equality.
        let mut decoded = None;
        for &id in &uses[index] {
            let current = &program.module.functions[id];
            if let Some(encoding) = products.get(&current.id).and_then(|product| {
                product.canonical_encoding(current, limits, limits.max_source_bytes)
            }) && let Some(value) =
                encoding.reuse_decoded_scalar_header(record.input.clone(), limits, input.nodes)
            {
                decoded = Some(value);
                break;
            }
        }
        let (function, encoding) = decoded.or_else(|| {
            zil_ir::wire::decode_raw_function_shared_owner(
                record.input.clone(),
                limits,
                input.nodes,
            )
            .ok()
        })?;
        input.nodes = input.nodes.checked_sub(encoding.node_cost())?;
        let (plan, plan_encoding) = zil_ir::wire::decode_raw_function_plan_shared_owner(
            record.plan.clone(),
            limits,
            input.nodes,
        )
        .ok()?;
        input.nodes = input.nodes.checked_sub(plan_encoding.node_cost())?;
        let span_sensitive = RawAnalysisBase::span_sensitive(&function)?;
        templates.push(Template {
            input: function,
            plan,
            span_sensitive,
        });
    }
    // Bind each current producer once. These are actual immutable current
    // Typed values, not persisted authority. Retain their strong owners while
    // the request-local column comparison map is alive.
    let canonical = program
        .module
        .functions
        .iter()
        .map(|current| products.get(&current.id)?.canonical_function(current))
        .collect::<Vec<_>>();
    let mut comparisons = HashMap::new();
    let mut expanded = Vec::with_capacity(rows.len());
    for (index, (template_index, costs)) in rows.into_iter().enumerate() {
        if options.cancellation.is_cancelled() {
            return None;
        }
        let template = &templates[template_index];
        let current = &program.module.functions[index];
        let function = if template.span_sensitive {
            // Foreign/unsafe rows keep their actual original outer identity and
            // all diagnostic placement. Relocation is a current changed input.
            if template.input.id != current.id || template.input.name != current.name {
                return None;
            }
            template.input.clone()
        } else {
            // The old body has already passed the real bounded decoder and
            // non-sensitive-domain check. Compare its COMPLETE semantic columns
            // against the current producer value once per immutable body pair;
            // every row still checks its complete current header. Recreating
            // and comparing a full wire fragment for every scalar ID/name would
            // undo the dictionary sharing just established by Types recovery.
            let bound = canonical[index].as_ref().is_some_and(|function| {
                RawAnalysisBase::same_template_headers(function, current)
                    && template_eq(&mut comparisons, &template.input, function)
            });
            if bound {
                program.module.functions.owner(index)?.clone()
            } else {
                let mut function = template.input.as_ref().clone();
                function.id = current.id;
                function.name = current.name.clone();
                Arc::new(function)
            }
        };
        let mut plan = template.plan.as_ref().clone();
        plan.function = current.id;
        expanded.push(RawAnalysisRow {
            input: function,
            plan: Arc::new(plan),
            costs,
        });
    }
    Some(RawAnalysisBase {
        context: context.to_vec(),
        rows: expanded,
        global_cost,
    })
}

pub(crate) fn save(
    options: &CompileOptions,
    artifacts: &Plan,
    program: &Arc<TypedProgram>,
    raw: &RawAnalysisBase,
    publication: &SemanticPublicationSession,
    products: &[zil_types::FunctionProduct],
) {
    let _ = save_inner(options, artifacts, program, raw, publication, products);
}
fn save_inner(
    options: &CompileOptions,
    artifacts: &Plan,
    program: &Arc<TypedProgram>,
    raw: &RawAnalysisBase,
    publication: &SemanticPublicationSession,
    products: &[zil_types::FunctionProduct],
) -> Option<()> {
    if options.cancellation.is_cancelled()
        || raw.context != RawAnalysisBase::context_for(program, &options.limits)?
    {
        return None;
    }
    if raw.rows.len() != program.module.functions.len()
        || raw.rows.len() > options.limits.max_functions.min(16_384)
    {
        return None;
    }
    let maximum = options.limits.max_source_bytes.min(MAXIMUM);
    let products = products
        .iter()
        .map(|product| (product.id, product))
        .collect::<BTreeMap<_, _>>();
    let mut output = Writer {
        bytes: Vec::new(),
        maximum,
    };
    output.put(MAGIC)?;
    put_blob(&mut output, &raw.context)?;
    output.put(&u64::try_from(raw.global_cost).ok()?.to_le_bytes())?;
    // Owners accelerate actual sharing; semantic selectors also find cold
    // separately allocated equal bodies. Only complete body/plan Eq aliases.
    let mut representatives = Vec::<Representative>::new();
    let mut buckets = HashMap::<Owners, Vec<usize>>::new();
    let mut semantic = HashMap::<u64, Vec<usize>>::new();
    let random = RandomState::new();
    let mut fuel = Fuel(options.limits.max_analysis_steps);
    let mut aliases = Vec::with_capacity(raw.rows.len());
    let mut sensitivities = HashMap::new();
    let mut comparisons = HashMap::new();
    let mut selectors = Selectors::default();
    for (index, row) in raw.rows.iter().enumerate() {
        if options.cancellation.is_cancelled() {
            return None;
        }
        let source = program.module.functions.get(index)?;
        if source != row.input.as_ref() {
            return None;
        }
        let sensitive = *sensitivities
            .entry((owners(source), source.foreign.is_some()))
            .or_insert_with(|| RawAnalysisBase::span_sensitive(source))
            .as_ref()?;
        let function = (!sensitive)
            .then(|| products.get(&source.id)?.canonical_function(source))
            .flatten()
            .filter(|canonical| template_eq(&mut comparisons, source, canonical))
            .unwrap_or_else(|| row.input.clone());
        let (selector, input_work) = if sensitive {
            (None, 0)
        } else {
            let (selector, work) = selectors.get(&function, &random, &mut fuel)?;
            // The complete canonical/current relation was checked above.
            fuel.spend(work)?;
            (Some(selector), work)
        };
        let plan_work = plan_work(&row.plan, &mut fuel)?;
        let owner = owners(&function);
        let mut representative = None;
        // Context requires distinct outer FunctionIds, so sensitive full-Eq
        // rows cannot alias another row. Keep their full original raw input.
        if let Some(selector) = selector {
            for bucket in [buckets.get(&owner), semantic.get(&selector)]
                .into_iter()
                .flatten()
            {
                for &index in bucket.iter().take(MAX_BUCKET) {
                    if same_representative(
                        &representatives[index],
                        &function,
                        &row.plan,
                        sensitive,
                        input_work,
                        plan_work,
                        &mut fuel,
                        &mut comparisons,
                    )? {
                        representative = Some(index);
                        break;
                    }
                }
                if representative.is_some() {
                    break;
                }
            }
        }
        let representative = if let Some(index) = representative {
            index
        } else {
            let next = representatives.len();
            representatives.push(Representative {
                input: function,
                plan: row.plan.clone(),
                sensitive,
                input_work,
                plan_work,
            });
            if let Some(selector) = selector {
                let bucket = semantic.entry(selector).or_default();
                if bucket.len() < MAX_BUCKET {
                    bucket.push(next);
                }
            }
            next
        };
        if !sensitive {
            let bucket = buckets.entry(owner).or_default();
            if bucket.len() < MAX_BUCKET && !bucket.contains(&representative) {
                bucket.push(representative);
            }
        }
        aliases.push(representative);
    }
    output.number(representatives.len())?;
    let mut nodes = options.limits.max_nodes.min(1_000_000);
    for representative in &representatives {
        let function = &representative.input;
        let plan = &representative.plan;
        let index = plan.function.0 as usize;
        let source = program.module.functions.get(index)?;
        let encoding = if representative.sensitive {
            None
        } else {
            publication
                .analyze_function_encoding(program, index)
                .filter(|encoding| encoding.matches_limits(&options.limits))
                .or_else(|| {
                    products
                        .get(&source.id)?
                        .canonical_encoding(source, &options.limits, maximum)
                })
        };
        let encoding = encoding
            .or_else(|| zil_ir::wire::encode_raw_function(function, &options.limits).ok())?;
        let plan_encoding = publication
            .analyze_plan_encoding(program, index, plan)
            .filter(|encoding| encoding.matches_limits(&options.limits))
            .or_else(|| {
                zil_ir::wire::encode_raw_function_plan(plan.clone(), &options.limits).ok()
            })?;
        nodes = nodes
            .checked_sub(encoding.node_cost())?
            .checked_sub(plan_encoding.node_cost())?;
        put_blob(&mut output, &encoding.shared_bytes())?;
        put_blob(&mut output, &plan_encoding.shared_bytes())?;
    }
    output.number(raw.rows.len())?;
    for (row, representative) in raw.rows.iter().zip(aliases) {
        output.number(representative)?;
        for cost in row.costs {
            output.put(&u64::try_from(cost).ok()?.to_le_bytes())?;
        }
    }
    if options.cancellation.is_cancelled() {
        return None;
    }
    let namespace = open(options, artifacts, true)?;
    publish(&namespace, &output.bytes, options)
}

fn publish(namespace: &Guard, bytes: &[u8], options: &CompileOptions) -> Option<()> {
    if options.cancellation.is_cancelled() || namespace.validate_write().is_err() {
        return None;
    }
    let digest = Fingerprint::of(bytes);
    let name = format!("{}.raw", digest.to_hex());
    let previous = read(&namespace.root().join("current.manifest"), 44).and_then(|bytes| {
        let mut input = Input {
            bytes: &bytes,
            nodes: 8,
        };
        if input.take(8)? != MANIFEST {
            return None;
        }
        let digest = Fingerprint(input.take(32)?.try_into().ok()?);
        let size = input.word()?;
        input.bytes.is_empty().then_some((digest, size))
    });
    if previous == Some((digest, bytes.len()))
        && read(
            &namespace.root().join(&name),
            options.limits.max_source_bytes.min(MAXIMUM),
        )
        .is_some_and(|old| old == bytes)
    {
        return Some(());
    }
    atomic(namespace, &name, bytes, options)?;
    let mut manifest = Writer {
        bytes: Vec::new(),
        maximum: 44,
    };
    manifest.put(MANIFEST)?;
    manifest.put(&digest.0)?;
    manifest.number(bytes.len())?;
    if atomic(namespace, "current.manifest", &manifest.bytes, options).is_none() {
        if previous.is_none_or(|(old, _)| old != digest) {
            let _ = fs::remove_file(namespace.root().join(name));
        }
        return None;
    }
    if let Some((old, _)) = previous
        && old != digest
    {
        let _ = fs::remove_file(namespace.root().join(format!("{}.raw", old.to_hex())));
    }
    Some(())
}
fn atomic(namespace: &Guard, name: &str, bytes: &[u8], options: &CompileOptions) -> Option<()> {
    if options.cancellation.is_cancelled() || namespace.validate_write().is_err() {
        return None;
    }
    let target = namespace.root().join(name);
    if let Ok(meta) = fs::symlink_metadata(&target)
        && (!meta.is_file() || meta.file_type().is_symlink())
    {
        return None;
    }
    let temporary = namespace.root().join(format!(
        ".analyze-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut open = OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let result = (|| {
        let mut file = open.open(&temporary).ok()?;
        file.write_all(bytes).ok()?;
        file.sync_all().ok()?;
        if options.cancellation.is_cancelled() || namespace.validate_write().is_err() {
            return None;
        }
        fs::rename(&temporary, target).ok()?;
        Some(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
#[path = "analyze_recovery_tests.rs"]
mod tests;
