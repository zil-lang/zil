//! Trusted-local raw frontend products. No parser, producer, Types or Verify
//! seal is serialized. Current parser classification and exact dependency bytes
//! admit candidates; the ordinary stage consumers still replay ordered fees.
use super::*;
use zil_base::SourceMapEntry;

// Counts are bounded before reservation, but an invalid row may still follow.
// Reserve small ordinary lists exactly without letting an encoded count force
// a large typed allocation before its elements have actually decoded.
const INITIAL_ITEMS: usize = 32;

#[derive(Clone, Default)]
pub struct FrontendProducts {
    pub context: Vec<u8>,
    pub modules: BTreeMap<ModuleId, Arc<[u8]>>,
}

impl FrontendProducts {
    /// Borrow the actual source field for reversible storage factoring. Only
    /// the raw prefix is parsed; this does not admit the module, its suffix,
    /// receipts, environment or any current compiler input. Every length is
    /// bounded by the supplied byte slice and no text or body is allocated.
    pub fn source_text(&self, id: ModuleId) -> Option<&[u8]> {
        let bytes = self.modules.get(&id)?;
        let limits = Limits {
            max_source_bytes: bytes.len(),
            ..Limits::default()
        };
        let mut nodes = 0;
        let mut reader = Reader::new(bytes, &limits, &mut nodes)?;
        if reader.take(8)? != b"ZILCORE1" || reader.u32()? != id.0 {
            return None;
        }
        reader.u32()?; // FileId
        std::str::from_utf8(reader.text_bytes()?).ok()?; // source path
        let text = reader.text_bytes()?;
        std::str::from_utf8(text).ok()?;
        Some(text)
    }
}

// One immutable encoding per successful bound environment. Checkpoints share
// it; only the existing scalar-products predicate can carry it to a new epoch.
pub(super) type ContextEncoding = Arc<std::sync::OnceLock<Option<Arc<[u8]>>>>;

fn bound_context<'a>(environment: &Environment, encoding: &'a ContextEncoding) -> Option<&'a [u8]> {
    encoding
        .get_or_init(|| context(environment).map(Arc::from))
        .as_deref()
}

fn context(environment: &Environment) -> Option<Vec<u8>> {
    // Encode exactly the closed namespace dependency of namespace_headers.
    // Local complete source bytes bind signatures/bodies, and the independent
    // current Types context continues to consume every actual signature.
    let mut w = Writer::new(&environment.limits);
    w.put(b"ZILNS02\0")?;
    let Limits {
        max_source_bytes,
        max_tokens,
        max_nodes: _,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_analysis_steps: _,
        max_ct_steps,
        max_call_depth,
    } = &environment.limits;
    for value in [
        *max_source_bytes,
        *max_tokens,
        *max_functions,
        *max_blocks,
        *max_instructions,
        *max_locals,
        *max_values,
        *max_call_depth,
    ] {
        w.u64(u64::try_from(value).ok()?)?;
    }
    w.u64(*max_ct_steps)?;
    let TargetProfile {
        id,
        triple,
        pointer_width,
        endian,
        backend,
        rule_version,
        checked_arithmetic,
    } = &environment.target;
    w.text(id)?;
    w.text(triple)?;
    w.number(usize::from(*pointer_width))?;
    w.byte(match endian {
        zil_base::Endian::Little => 0,
        zil_base::Endian::Big => 1,
    })?;
    w.text(backend)?;
    w.text(rule_version)?;
    w.boolean(*checked_arithmetic)?;
    w.text(&environment.context)?;
    w.boolean(environment.fused_distribution)?;
    w.number(environment.sources.len())?;
    for (id, file, path) in &environment.sources {
        w.number(id.0 as usize)?;
        w.number(file.0 as usize)?;
        w.text(path)?;
    }
    w.number(environment.imports.len())?;
    for imports in &environment.imports {
        w.number(imports.len())?;
        for import in imports {
            w.import(import)?;
        }
    }
    w.number(environment.headers.len())?;
    for header in &environment.headers {
        w.number(header.len())?;
        for item in header.iter() {
            if !scalar_admission::item_shape(item) {
                return None;
            }
            w.boolean(item.public)?;
            w.number(item.attributes.len())?;
            for attribute in &item.attributes {
                let s::Attribute {
                    path,
                    arguments,
                    string_value,
                    span: _,
                } = attribute;
                // Scalar admission permits only argument-free #[test]. Keep
                // the actual full fields rather than a name-derived marker.
                if !arguments.is_empty() {
                    return None;
                }
                w.strings(path)?;
                w.number(arguments.len())?;
                w.optional(string_value, |w, value| w.text(value))?;
            }
            match &item.kind {
                s::ItemKind::Function(f) => {
                    w.byte(0)?;
                    w.text(&f.name)?;
                }
                s::ItemKind::Extern { function, .. } => {
                    w.byte(1)?;
                    w.text(&function.name)?;
                }
                s::ItemKind::Import(import) => {
                    let s::Import {
                        path,
                        alias,
                        syntax,
                    } = import;
                    w.byte(2)?;
                    w.strings(path)?;
                    w.optional(alias, |w, value| w.text(value))?;
                    w.boolean(*syntax)?;
                }
                _ => return None,
            }
        }
    }
    Some(w.bytes)
}

// Recovered modules consume the same ordered, nonnegative stage demands as
// ordinary scalar declarations. Keep only eight totals, rather than rebuilding
// a recipe, closed-header outcome and tree-map rows for every unchanged item.
// These are raw demands, bound by admit() to current parsed inputs, not proof
// authority. A total that does not fit abandons recovery for the original walk.
#[derive(Default)]
pub(super) struct RecoveredStages {
    totals: [Receipt; 8],
    counts: [usize; 8],
}
impl RecoveredStages {
    pub(super) fn receipt(&self, stage: Stage) -> (Receipt, usize) {
        let index = usize::from(stage_tag(stage));
        (self.totals[index], self.counts[index])
    }
}

impl ExpandSession {
    pub fn needs_frontend_recovery(&self) -> bool {
        self.entries.is_empty()
    }
    /// Supply only explicitly trusted local raw candidates. This does not
    /// create any current epoch; admission happens inside expand_parsed.
    pub fn restore_frontend_products(
        &mut self,
        products: FrontendProducts,
        limits: &Limits,
    ) -> bool {
        let size = products
            .modules
            .values()
            .try_fold(products.context.len(), |n, b| n.checked_add(b.len()));
        if !self.entries.is_empty()
            || products.modules.len() > limits.max_functions
            || size.is_none_or(|n| n > limits.max_source_bytes)
        {
            return false;
        }
        self.recovery = Some(products);
        true
    }

    pub fn encode_frontend_products(&self, limits: &Limits) -> Option<FrontendProducts> {
        let environment = self.environment.as_ref()?;
        let context = bound_context(environment, &self.frontend_context)?.to_vec();
        let mut bytes = context.len();
        let mut modules = BTreeMap::new();
        for (id, entry) in &self.entries {
            // Unknown Core shapes simply have no product; other modules still
            // publish. A failed request never installs entries in this session.
            let encoded = if let Some(encoded) = entry.recovery_encoding.get() {
                encoded.clone()
            } else {
                let Some(encoded) = encode(entry, limits) else {
                    continue;
                };
                let encoded: Arc<[u8]> = encoded.into();
                let _ = entry.recovery_encoding.set(encoded.clone());
                encoded
            };
            bytes = bytes.checked_add(encoded.len())?;
            if bytes > limits.max_source_bytes {
                return None;
            }
            modules.insert(*id, encoded);
        }
        (!modules.is_empty()).then_some(FrontendProducts { context, modules })
    }
}

pub(super) fn admit(
    products: Option<&FrontendProducts>,
    environment: &Environment,
    owner: &Arc<Environment>,
    encoding: &ContextEncoding,
    entries: &mut BTreeMap<ModuleId, Entry>,
) -> u64 {
    let Some(products) = products else {
        return 0;
    };
    if bound_context(environment, encoding) != Some(products.context.as_slice()) {
        return 0;
    }
    let mut restored = 0;
    let mut decode_nodes = environment.limits.max_nodes;
    for (id, bytes) in &products.modules {
        let Some(entry) = entries.get_mut(id) else {
            continue;
        };
        if entry.lower.is_some() || !entry.scalar {
            continue;
        }
        // Decode into a tentative entry. No partially decoded fees/owners can
        // escape on malformed input, quota failure, or a source mismatch.
        if let Some(recovered) = decode(bytes, entry, owner, &environment.limits, &mut decode_nodes)
        {
            *entry = recovered;
            let _ = entry.recovery_encoding.set(bytes.clone());
            restored += 1;
        }
    }
    restored
}

fn encode(entry: &Entry, limits: &Limits) -> Option<Vec<u8>> {
    let (_, lower) = entry.lower.as_ref()?;
    let c = &lower.core;
    if !entry.scalar
        || !c.life_declarations.is_empty()
        || !c.life_instances.is_empty()
        || !c.closure_life_contexts.is_empty()
        || !c.dyn_declarations.is_empty()
        || !c.dyn_interfaces.is_empty()
        || !c.dyn_vtables.is_empty()
        || !c.aggregate_templates.is_empty()
        || !c.aggregate_applications.is_empty()
        || !c.aggregates.is_empty()
        || !c.enums.is_empty()
        || !c.unsupported.is_empty()
        || !lower.generated_imports.is_empty()
        || !lower.generated_exports.is_empty()
        || !lower.imported_public_traits.is_empty()
        || !lower.trait_method_names.is_empty()
        || !lower.dyn_method_emissions.is_empty()
        || !lower.closure_context_emissions.is_empty()
        || !lower.callable_context_emissions.is_empty()
    {
        return None;
    }
    let mut w = Writer::new(limits);
    w.put(b"ZILCORE1")?;
    w.source(entry.parsed.source())?;
    // Bind current placement independently of source bytes (parser numbering
    // and module spans are inputs too); no process-local allocation is saved.
    w.span(entry.parsed.parsed().module.span)?;
    w.number(entry.parsed.parsed().module.items.len())?;
    for item in &entry.parsed.parsed().module.items {
        w.span(item.span)?;
    }
    w.number(entry.receipts.len())?;
    for ((stage, start), receipt) in entry.receipts.iter() {
        w.byte(stage_tag(*stage))?;
        w.number(*start)?;
        w.number(receipt.nodes)?;
        w.number(receipt.work)?;
    }
    w.number(entry.normalized.len())?;
    for (start, normalized) in entry.normalized.iter() {
        let NormalizedCache::Deferred(recipe) = normalized else {
            return None;
        };
        if !entry.scalar_headers.get(start)?.closed() {
            return None;
        }
        w.number(*start)?;
        w.number(recipe.ordinal)?;
        match recipe.declaration() {
            modules::scalar_products::Declaration::Function(name) => {
                w.byte(0)?;
                w.text(name)?;
            }
            modules::scalar_products::Declaration::Extern => w.byte(1)?,
        }
    }
    w.number(entry.output_imports.len())?;
    for edge in &entry.output_imports {
        w.import(edge)?;
    }
    for fee in [
        lower.project_trait_work,
        lower.project_trait_nodes,
        lower.member_owner_work,
        lower.member_owner_nodes,
        lower.lowering_demand.work,
        lower.lowering_demand.nodes,
        lower.source_map_work?,
    ] {
        w.number(fee)?;
    }
    w.number(c.functions.len())?;
    for f in &c.functions {
        w.function(f, 0)?;
    }
    w.number(lower.function_lower_bindings.len())?;
    for (id, b) in lower.function_lower_bindings.iter() {
        w.number(id.0 as usize)?;
        w.number(b.temporary_start as usize)?;
        w.number(b.temporary_end as usize)?;
        w.boolean(b.node_named)?;
    }
    w.number(lower.source_map.entries.len())?;
    for e in &lower.source_map.entries {
        w.map_entry(e)?;
    }
    Some(w.bytes)
}

fn decode(
    bytes: &[u8],
    current: &Entry,
    environment: &Arc<Environment>,
    limits: &Limits,
    nodes: &mut usize,
) -> Option<Entry> {
    let mut r = Reader::new(bytes, limits, nodes)?;
    if r.take(8)? != b"ZILCORE1"
        || !r.matches_source(current.parsed.source())?
        || r.span()? != current.parsed.parsed().module.span
    {
        return None;
    }
    let count = r.count()?;
    if count != current.parsed.parsed().module.items.len() {
        return None;
    }
    for item in &current.parsed.parsed().module.items {
        if r.span()? != item.span {
            return None;
        }
    }
    let mut entry = current.clone();
    let module = &current.parsed.parsed().module;
    let declarations = module
        .items
        .iter()
        .filter(|item| !matches!(item.kind, s::ItemKind::Import(_)))
        .count();
    let mut stages = RecoveredStages::default();
    let mut cursors = [0; 8];
    let mut previous = None;
    let mut receipts = BTreeMap::new();
    for _ in 0..r.count()? {
        let stage = read_stage(r.byte()?)?;
        let start = r.number()?;
        let receipt = Receipt {
            nodes: r.number()?,
            work: r.number()?,
        };
        // The writer's canonical order excludes duplicates and ensures that
        // each per-item stage has exactly the complete declaration sequence.
        if previous.is_some_and(|key| key >= (stage, start)) {
            return None;
        }
        previous = Some((stage, start));
        let index = usize::from(stage_tag(stage));
        if matches!(stage, Stage::Preflight | Stage::Restore) {
            if start != module.span.start || stages.counts[index] != 0 {
                return None;
            }
            receipts.insert((stage, start), receipt);
        } else {
            while module
                .items
                .get(cursors[index])
                .is_some_and(|item| matches!(item.kind, s::ItemKind::Import(_)))
            {
                cursors[index] += 1;
            }
            if module.items.get(cursors[index])?.span.start != start {
                return None;
            }
            cursors[index] += 1;
        }
        stages.totals[index].nodes = stages.totals[index].nodes.checked_add(receipt.nodes)?;
        stages.totals[index].work = stages.totals[index].work.checked_add(receipt.work)?;
        stages.counts[index] = stages.counts[index].checked_add(1)?;
    }
    for stage in [
        Stage::Preflight,
        Stage::Normalize,
        Stage::Prepare,
        Stage::Lifetimes,
        Stage::Headers,
        Stage::Distribute,
        Stage::Restore,
        Stage::RestoreItem,
    ] {
        let required = match stage {
            Stage::Preflight | Stage::Restore => 1,
            Stage::Distribute if environment.fused_distribution => 0,
            _ => declarations,
        };
        if stages.receipt(stage).1 != required {
            return None;
        }
    }
    let count = r.count()?;
    if count != declarations {
        return None;
    }
    let mut previous_ordinal = None;
    for _ in 0..count {
        let start = r.number()?;
        let ordinal = r.number()?;
        if previous_ordinal.is_some_and(|previous| previous >= ordinal) {
            return None;
        }
        previous_ordinal = Some(ordinal);
        let item = module.items.get(ordinal)?;
        if item.span.start != start {
            return None;
        }
        match r.byte()? {
            0 if matches!(item.kind, s::ItemKind::Function(_)) => {
                // Validate the same raw text field without allocating a name
                // that no resumed consumer needs on a successful module hit.
                std::str::from_utf8(r.text_bytes()?).ok()?;
            }
            1 if matches!(item.kind, s::ItemKind::Extern { .. }) => {}
            _ => return None,
        }
    }
    let import_count = r.count()?;
    let mut imports = Vec::with_capacity(import_count.min(INITIAL_ITEMS));
    for _ in 0..import_count {
        imports.push(r.import()?);
    }
    let fees = [
        r.number()?,
        r.number()?,
        r.number()?,
        r.number()?,
        r.number()?,
        r.number()?,
        r.number()?,
    ];
    let count = r.count()?;
    if count > limits.max_functions || count != declarations {
        return None;
    }
    let mut functions = Vec::with_capacity(count.min(INITIAL_ITEMS));
    for _ in 0..count {
        functions.push(r.function(0)?);
    }
    for (actual, item) in functions.iter().zip(
        current
            .parsed
            .parsed()
            .module
            .items
            .iter()
            .filter(|item| !matches!(item.kind, s::ItemKind::Import(_))),
    ) {
        let (s::ItemKind::Function(expected)
        | s::ItemKind::Extern {
            function: expected, ..
        }) = &item.kind
        else {
            return None;
        };
        if actual.name != expected.name
            || actual.span != expected.span
            || actual.parameters.len() != expected.params.len()
            || actual
                .parameters
                .iter()
                .zip(&expected.params)
                .any(|(a, b)| a.name != b.name || a.span != b.span)
        {
            return None;
        }
    }
    let mut bindings = BTreeMap::new();
    for _ in 0..r.count()? {
        let id = NodeId(r.u32()?);
        let binding = FunctionLowerBinding {
            temporary_start: r.u32()?,
            temporary_end: r.u32()?,
            node_named: r.boolean()?,
        };
        if binding.temporary_end < binding.temporary_start || bindings.insert(id, binding).is_some()
        {
            return None;
        }
    }
    if bindings.len() != functions.len() || functions.iter().any(|f| !bindings.contains_key(&f.id))
    {
        return None;
    }
    let mut map = SourceMap::new(current.parsed.source().module_id);
    let map_count = r.count()?;
    map.entries.reserve(map_count.min(INITIAL_ITEMS));
    for _ in 0..map_count {
        map.entries.push(r.map_entry()?);
    }
    if !r.bytes.is_empty() {
        return None;
    }
    let core = CoreModule {
        schema_version: SCHEMA_VERSION,
        module_id: current.parsed.source().module_id,
        source_fingerprint: current.parsed.source().fingerprint(),
        functions: functions.into(),
        life_declarations: vec![],
        life_instances: vec![],
        closure_life_contexts: vec![],
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        aggregate_templates: vec![],
        aggregate_applications: vec![],
        aggregates: vec![],
        enums: vec![],
        unsupported: vec![],
    };
    entry.receipts = Arc::new(receipts);
    entry.recovered_stages = Some(Arc::new(stages));
    entry.output_imports = imports;
    entry.lower = Some((
        limits.clone(),
        Arc::new(ExpandedModule {
            core: Arc::new(core),
            source_map: Arc::new(map),
            generated_imports: vec![],
            generated_exports: vec![],
            imported_public_traits: BTreeSet::new(),
            trait_method_names: BTreeMap::new(),
            dyn_method_emissions: vec![],
            closure_context_emissions: vec![],
            callable_context_emissions: vec![],
            project_trait_work: fees[0],
            project_trait_nodes: fees[1],
            member_owner_work: fees[2],
            member_owner_nodes: fees[3],
            lowering_demand: LoweringDemand {
                work: fees[4],
                nodes: fees[5],
            },
            source_map_work: Some(fees[6]),
            function_lower_bindings: Arc::new(bindings),
            item_lower_receipts: Arc::default(),
            reused_lower_items: 0,
            executed_lower_items: 0,
            copied_lower_map_entries: 0,
        }),
    ));
    // Validate the raw map against all current sources at its original Lower
    // allowance; the decoded map has no process-local validation receipt.
    entry.relocated_lower = true;
    entry.recovered_lower = true;
    Some(entry)
}

fn stage_tag(s: Stage) -> u8 {
    match s {
        Stage::Preflight => 0,
        Stage::Normalize => 1,
        Stage::Prepare => 2,
        Stage::Lifetimes => 3,
        Stage::Headers => 4,
        Stage::Distribute => 5,
        Stage::Restore => 6,
        Stage::RestoreItem => 7,
    }
}
fn read_stage(n: u8) -> Option<Stage> {
    Some(match n {
        0 => Stage::Preflight,
        1 => Stage::Normalize,
        2 => Stage::Prepare,
        3 => Stage::Lifetimes,
        4 => Stage::Headers,
        5 => Stage::Distribute,
        6 => Stage::Restore,
        7 => Stage::RestoreItem,
        _ => return None,
    })
}

struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
    nodes: usize,
    depth: usize,
}
impl Writer {
    fn new(l: &Limits) -> Self {
        Self {
            bytes: vec![],
            maximum: l.max_source_bytes,
            nodes: l.max_nodes,
            depth: l.max_call_depth,
        }
    }
    fn put(&mut self, b: &[u8]) -> Option<()> {
        if self.bytes.len().checked_add(b.len())? > self.maximum {
            return None;
        }
        self.bytes.extend_from_slice(b);
        Some(())
    }
    fn byte(&mut self, b: u8) -> Option<()> {
        self.put(&[b])
    }
    fn boolean(&mut self, b: bool) -> Option<()> {
        self.byte(u8::from(b))
    }
    fn number(&mut self, n: usize) -> Option<()> {
        let mut n = u32::try_from(n).ok()?;
        while n >= 128 {
            self.byte((n as u8 & 127) | 128)?;
            n >>= 7;
        }
        self.byte(n as u8)
    }
    fn u64(&mut self, n: u64) -> Option<()> {
        self.put(&n.to_le_bytes())
    }
    fn strings(&mut self, values: &[String]) -> Option<()> {
        self.number(values.len())?;
        for value in values {
            self.text(value)?;
        }
        Some(())
    }
    fn text(&mut self, s: &str) -> Option<()> {
        self.number(s.len())?;
        self.put(s.as_bytes())
    }
    fn node(&mut self, depth: usize) -> Option<()> {
        if depth > self.depth {
            return None;
        }
        self.nodes = self.nodes.checked_sub(1)?;
        Some(())
    }
    fn span(&mut self, s: Span) -> Option<()> {
        self.number(s.file.0 as usize)?;
        self.number(s.start)?;
        self.number(s.end)
    }
    fn source(&mut self, s: &SourceUnit) -> Option<()> {
        self.number(s.module_id.0 as usize)?;
        self.number(s.file_id.0 as usize)?;
        self.text(&s.path)?;
        self.text(&s.text)
    }
    fn import(&mut self, i: &CoreImport) -> Option<()> {
        self.text(&i.alias)?;
        self.number(i.module.0 as usize)?;
        self.span(i.span)
    }
}
struct Reader<'a, 'n> {
    bytes: &'a [u8],
    nodes: &'n mut usize,
    depth: usize,
}
impl<'a, 'n> Reader<'a, 'n> {
    fn new(bytes: &'a [u8], l: &Limits, nodes: &'n mut usize) -> Option<Self> {
        (bytes.len() <= l.max_source_bytes).then_some(Self {
            bytes,
            nodes,
            depth: l.max_call_depth,
        })
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.bytes.get(..n)?;
        self.bytes = &self.bytes[n..];
        Some(b)
    }
    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn boolean(&mut self) -> Option<bool> {
        match self.byte()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn u32(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            if shift == 28 && byte > 15 {
                return None;
            }
            value |= u32::from(byte & 127) << shift;
            if byte & 128 == 0 {
                if shift != 0 && byte == 0 {
                    return None;
                }
                return Some(value);
            }
        }
        None
    }
    fn number(&mut self) -> Option<usize> {
        usize::try_from(self.u32()?).ok()
    }
    fn count(&mut self) -> Option<usize> {
        let n = self.number()?;
        if n > self.bytes.len() || n > *self.nodes {
            return None;
        }
        *self.nodes -= n;
        Some(n)
    }
    fn text_bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.number()?;
        self.take(n)
    }
    fn text(&mut self) -> Option<String> {
        Some(std::str::from_utf8(self.text_bytes()?).ok()?.to_owned())
    }
    fn node(&mut self, depth: usize) -> Option<()> {
        if depth > self.depth {
            return None;
        }
        *self.nodes = self.nodes.checked_sub(1)?;
        Some(())
    }
    fn span(&mut self) -> Option<Span> {
        let file = FileId(self.u32()?);
        let start = self.number()?;
        let end = self.number()?;
        (start <= end).then_some(Span { file, start, end })
    }
    fn matches_source(&mut self, s: &SourceUnit) -> Option<bool> {
        Some(
            self.u32()? == s.module_id.0
                && self.u32()? == s.file_id.0
                && self.text_bytes()? == s.path.as_bytes()
                && self.text_bytes()? == s.text.as_bytes(),
        )
    }
    fn import(&mut self) -> Option<CoreImport> {
        Some(CoreImport {
            alias: self.text()?,
            module: ModuleId(self.u32()?),
            span: self.span()?,
        })
    }
}

macro_rules! scalar_tags {
    ($ty:ident, $write:ident, $read:ident, $($variant:ident),+ $(,)?) => {
        impl Writer { fn $write(&mut self, value: &$ty) -> Option<()> {
            #[repr(u8)]
            enum Tag { $($variant),+ }
            $(if matches!(value, $ty::$variant) { return self.byte(Tag::$variant as u8); })+
            None
        } }
        impl Reader<'_, '_> { fn $read(&mut self) -> Option<$ty> {
            // Construct only the selected scalar. In particular, do not build
            // and clone an entire array of the large recursive Type enum for
            // every current parameter, local annotation and conversion.
            #[repr(u8)]
            enum Tag { $($variant),+ }
            let tag = self.byte()?;
            $(if tag == Tag::$variant as u8 { return Some($ty::$variant); })+
            None
        } }
    }
}
scalar_tags!(
    Type, ty, ty, Unit, Bool, I32, I64, U64, Usize, F32, F64, U8, U16, U32, I8, I16, Isize
);
scalar_tags!(
    PassingMode,
    mode,
    mode,
    Copy,
    Move,
    Describe,
    Create,
    Give,
    Borrow,
    BorrowMut,
    Infer
);
scalar_tags!(
    UnaryOp,
    unary,
    unary,
    Negate,
    Not,
    BitNot,
    Dereference,
    Borrow,
    BorrowMut,
    Give
);
scalar_tags!(
    BinaryOp,
    binary,
    binary,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    AddWrapping,
    SubtractWrapping,
    MultiplyWrapping,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight
);
scalar_tags!(
    ExpansionRole,
    role,
    role,
    Source,
    ArrowReturn,
    PipeInput,
    PipeTarget,
    PipeCall,
    RecordBase,
    RecordField,
    RecordDefault,
    RecordResult,
    RecordUpdate,
    ReceiverDefault,
    ReceiverExplicit,
    ReceiverShorthand,
    ScriptMain,
    MetaExpansion,
    SyntaxIsland,
    ClosureBody,
    TemporaryRead
);

impl Writer {
    fn optional<T>(
        &mut self,
        value: &Option<T>,
        write: impl FnOnce(&mut Self, &T) -> Option<()>,
    ) -> Option<()> {
        self.boolean(value.is_some())?;
        if let Some(v) = value {
            write(self, v)?;
        }
        Some(())
    }
    fn function(&mut self, f: &CoreFunction, depth: usize) -> Option<()> {
        self.node(depth)?;
        if f.storage_life.is_some()
            || f.call_kind.is_some_and(|kind| kind != CallKind::Direct)
            || f.life.is_some()
            || !f.type_parameters.is_empty()
            || !f.state_parameters.is_empty()
            || f.protocol_contract.is_some()
            || f.callable_return.is_some()
            || !f.callable_parameters.is_empty()
            || f.receiver.is_some()
        {
            return None;
        }
        self.number(f.id.0 as usize)?;
        self.text(&f.name)?;
        self.span(f.span)?;
        self.boolean(f.call_kind.is_some())?;
        self.optional(&f.foreign, |w, f| {
            if !f.noescape_parameters.is_empty()
                || f.static_callbacks
                || f.scoped_callback.is_some()
            {
                return None;
            }
            w.text(&f.abi)?;
            w.text(&f.symbol)
        })?;
        self.number(f.parameters.len())?;
        for p in &f.parameters {
            self.text(&p.name)?;
            self.ty(&p.ty)?;
            self.mode(&p.mode)?;
            self.span(p.span)?;
        }
        self.ty(&f.return_type)?;
        self.mode(&f.result_mode)?;
        self.block(&f.body, depth + 1)
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> Option<()> {
        self.node(depth)?;
        self.span(block.span)?;
        self.number(block.statements.len())?;
        for stmt in &block.statements {
            self.statement(stmt, depth + 1)?;
        }
        Some(())
    }
    fn statement(&mut self, s: &CoreStatement, depth: usize) -> Option<()> {
        self.node(depth)?;
        self.number(s.id.0 as usize)?;
        self.span(s.span)?;
        match &s.kind {
            CoreStatementKind::Binding {
                name,
                mutable,
                ty,
                initializer,
            } => {
                self.byte(0)?;
                self.text(name)?;
                self.boolean(*mutable)?;
                self.optional(ty, Self::ty)?;
                self.optional(initializer, |w, e| w.expr(e, depth + 1))?;
            }
            CoreStatementKind::Assign { target, value } => {
                self.byte(1)?;
                self.expr(target, depth + 1)?;
                self.expr(value, depth + 1)?;
            }
            CoreStatementKind::Expression(e) => {
                self.byte(2)?;
                self.expr(e, depth + 1)?;
            }
            CoreStatementKind::Return(e) => {
                self.byte(3)?;
                self.optional(e, |w, e| w.expr(e, depth + 1))?;
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.byte(4)?;
                self.expr(condition, depth + 1)?;
                self.block(then_body, depth + 1)?;
                self.optional(else_body, |w, b| w.block(b, depth + 1))?;
            }
            CoreStatementKind::While { condition, body } => {
                self.byte(5)?;
                self.expr(condition, depth + 1)?;
                self.block(body, depth + 1)?;
            }
            CoreStatementKind::Block(b) => {
                self.byte(6)?;
                self.block(b, depth + 1)?;
            }
            CoreStatementKind::Unsafe(b) => {
                self.byte(7)?;
                self.block(b, depth + 1)?;
            }
            CoreStatementKind::Break => self.byte(8)?,
            CoreStatementKind::Continue => self.byte(9)?,
            _ => return None,
        }
        Some(())
    }
    fn expr(&mut self, e: &CoreExpr, depth: usize) -> Option<()> {
        self.node(depth)?;
        self.number(e.id.0 as usize)?;
        self.span(e.span)?;
        match &e.kind {
            CoreExprKind::Integer(s) => {
                self.byte(0)?;
                self.text(s)?;
            }
            CoreExprKind::Float(s) => {
                self.byte(1)?;
                self.text(s)?;
            }
            CoreExprKind::Bool(b) => {
                self.byte(2)?;
                self.boolean(*b)?;
            }
            CoreExprKind::String(s) => {
                self.byte(3)?;
                self.text(s)?;
            }
            CoreExprKind::Unit => self.byte(4)?,
            CoreExprKind::Name(s) => {
                self.byte(5)?;
                self.text(s)?;
            }
            CoreExprKind::Unary { op, operand } => {
                self.byte(6)?;
                self.unary(op)?;
                self.expr(operand, depth + 1)?;
            }
            CoreExprKind::Binary { op, left, right } => {
                self.byte(7)?;
                self.binary(op)?;
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.byte(8)?;
                self.expr(callee, depth + 1)?;
                self.number(arguments.len())?;
                for e in arguments {
                    self.expr(e, depth + 1)?;
                }
            }
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } => {
                self.byte(9)?;
                self.expr(object, depth + 1)?;
                self.text(member)?;
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.byte(10)?;
                self.number(bindings.len())?;
                for (name, e) in bindings {
                    self.text(name)?;
                    self.expr(e, depth + 1)?;
                }
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.byte(11)?;
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            CoreExprKind::NumericConvert { value, target } => {
                self.byte(12)?;
                self.expr(value, depth + 1)?;
                self.ty(target)?;
            }
            CoreExprKind::IntegerConvert { value, target } => {
                self.byte(13)?;
                self.expr(value, depth + 1)?;
                self.ty(target)?;
            }
            CoreExprKind::Ascribed { ty, value } => {
                self.byte(14)?;
                self.ty(ty)?;
                self.expr(value, depth + 1)?;
            }
            _ => return None,
        }
        Some(())
    }
    fn map_entry(&mut self, e: &SourceMapEntry) -> Option<()> {
        self.number(e.id.0 as usize)?;
        match e.target {
            SourceTarget::Node(n) => {
                self.byte(0)?;
                self.number(n.0 as usize)?;
            }
            SourceTarget::Parameter { function, index } => {
                self.byte(1)?;
                self.number(function.0 as usize)?;
                self.number(index as usize)?;
            }
            SourceTarget::Span(s) => {
                self.byte(2)?;
                self.span(s)?;
            }
        }
        self.span(e.origin)?;
        self.role(&e.role)?;
        self.number(e.related.len())?;
        for s in &e.related {
            self.span(*s)?;
        }
        self.optional(&e.parent, |w, p| w.number(p.0 as usize))
    }
}
impl Reader<'_, '_> {
    fn optional<T>(&mut self, read: impl FnOnce(&mut Self) -> Option<T>) -> Option<Option<T>> {
        Some(if self.boolean()? {
            Some(read(self)?)
        } else {
            None
        })
    }
    fn function(&mut self, depth: usize) -> Option<CoreFunction> {
        self.node(depth)?;
        let id = NodeId(self.u32()?);
        let name = self.text()?;
        let span = self.span()?;
        let call_kind = self.boolean()?.then_some(CallKind::Direct);
        let foreign = self.optional(|r| {
            Some(ForeignDeclaration {
                abi: r.text()?,
                symbol: r.text()?,
                noescape_parameters: vec![],
                static_callbacks: false,
                scoped_callback: None,
            })
        })?;
        let count = self.count()?;
        let mut parameters = Vec::with_capacity(count.min(INITIAL_ITEMS));
        for _ in 0..count {
            parameters.push(CoreParameter {
                name: self.text()?,
                ty: self.ty()?,
                mode: self.mode()?,
                span: self.span()?,
            });
        }
        let return_type = self.ty()?;
        let result_mode = self.mode()?;
        let body = self.block(depth + 1)?;
        Some(CoreFunction {
            id,
            name,
            span,
            foreign,
            parameters,
            return_type,
            result_mode,
            body,
            storage_life: None,
            call_kind,
            life: None,
            type_parameters: vec![],
            state_parameters: vec![],
            protocol_contract: None,
            callable_return: None,
            callable_parameters: vec![],
            receiver: None,
        })
    }
    fn block(&mut self, depth: usize) -> Option<CoreBlock> {
        self.node(depth)?;
        let span = self.span()?;
        let count = self.count()?;
        let mut statements = Vec::with_capacity(count.min(INITIAL_ITEMS));
        for _ in 0..count {
            statements.push(self.statement(depth + 1)?);
        }
        Some(CoreBlock { statements, span })
    }
    fn statement(&mut self, depth: usize) -> Option<CoreStatement> {
        self.node(depth)?;
        let id = NodeId(self.u32()?);
        let span = self.span()?;
        let kind = match self.byte()? {
            0 => CoreStatementKind::Binding {
                name: self.text()?,
                mutable: self.boolean()?,
                ty: self.optional(Self::ty)?,
                initializer: self.optional(|r| r.expr(depth + 1))?,
            },
            1 => CoreStatementKind::Assign {
                target: self.expr(depth + 1)?,
                value: self.expr(depth + 1)?,
            },
            2 => CoreStatementKind::Expression(self.expr(depth + 1)?),
            3 => CoreStatementKind::Return(self.optional(|r| r.expr(depth + 1))?),
            4 => CoreStatementKind::If {
                condition: self.expr(depth + 1)?,
                then_body: self.block(depth + 1)?,
                else_body: self.optional(|r| r.block(depth + 1))?,
            },
            5 => CoreStatementKind::While {
                condition: self.expr(depth + 1)?,
                body: self.block(depth + 1)?,
            },
            6 => CoreStatementKind::Block(self.block(depth + 1)?),
            7 => CoreStatementKind::Unsafe(self.block(depth + 1)?),
            8 => CoreStatementKind::Break,
            9 => CoreStatementKind::Continue,
            _ => return None,
        };
        Some(CoreStatement { id, span, kind })
    }
    fn expr(&mut self, depth: usize) -> Option<CoreExpr> {
        self.node(depth)?;
        let id = NodeId(self.u32()?);
        let span = self.span()?;
        let kind = match self.byte()? {
            0 => CoreExprKind::Integer(self.text()?),
            1 => CoreExprKind::Float(self.text()?),
            2 => CoreExprKind::Bool(self.boolean()?),
            3 => CoreExprKind::String(self.text()?),
            4 => CoreExprKind::Unit,
            5 => CoreExprKind::Name(self.text()?),
            6 => CoreExprKind::Unary {
                op: self.unary()?,
                operand: Box::new(self.expr(depth + 1)?),
            },
            7 => CoreExprKind::Binary {
                op: self.binary()?,
                left: Box::new(self.expr(depth + 1)?),
                right: Box::new(self.expr(depth + 1)?),
            },
            8 => {
                let callee = Box::new(self.expr(depth + 1)?);
                let count = self.count()?;
                let mut arguments = Vec::with_capacity(count.min(INITIAL_ITEMS));
                for _ in 0..count {
                    arguments.push(self.expr(depth + 1)?);
                }
                CoreExprKind::Call { callee, arguments }
            }
            9 => CoreExprKind::Member {
                object: Box::new(self.expr(depth + 1)?),
                member: self.text()?,
                owner: None,
            },
            10 => {
                let count = self.count()?;
                let mut bindings = Vec::with_capacity(count.min(INITIAL_ITEMS));
                for _ in 0..count {
                    bindings.push((self.text()?, self.expr(depth + 1)?));
                }
                CoreExprKind::Sequence {
                    bindings,
                    value: Box::new(self.expr(depth + 1)?),
                }
            }
            11 => CoreExprKind::If {
                condition: Box::new(self.expr(depth + 1)?),
                then_value: Box::new(self.expr(depth + 1)?),
                else_value: Box::new(self.expr(depth + 1)?),
            },
            12 => CoreExprKind::NumericConvert {
                value: Box::new(self.expr(depth + 1)?),
                target: self.ty()?,
            },
            13 => CoreExprKind::IntegerConvert {
                value: Box::new(self.expr(depth + 1)?),
                target: self.ty()?,
            },
            14 => CoreExprKind::Ascribed {
                ty: self.ty()?,
                value: Box::new(self.expr(depth + 1)?),
            },
            _ => return None,
        };
        Some(CoreExpr { id, span, kind })
    }
    fn map_entry(&mut self) -> Option<SourceMapEntry> {
        let id = SourceMapId(self.u32()?);
        let target = match self.byte()? {
            0 => SourceTarget::Node(NodeId(self.u32()?)),
            1 => SourceTarget::Parameter {
                function: NodeId(self.u32()?),
                index: self.u32()?,
            },
            2 => SourceTarget::Span(self.span()?),
            _ => return None,
        };
        let origin = self.span()?;
        let role = self.role()?;
        let count = self.count()?;
        let mut related = Vec::with_capacity(count.min(INITIAL_ITEMS));
        for _ in 0..count {
            related.push(self.span()?);
        }
        let parent = self.optional(|r| Some(SourceMapId(r.u32()?)))?;
        Some(SourceMapEntry {
            id,
            target,
            origin,
            role,
            related,
            parent,
        })
    }
}

#[cfg(test)]
#[path = "frontend_recovery_tests.rs"]
mod tests;
