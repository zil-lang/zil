//! Canonical raw function segments plus complete current source bindings.
//! Rebinding precedes every semantic fingerprint check and current Verify.
use super::*;
use std::sync::Arc;
use zil_ir::wire::{
    FunctionEncodingStats, FunctionPlanEncodingStats, RawFunctionEncoding, RawFunctionPlanEncoding,
    WireSegment,
};

const LEGACY_MANIFEST_MAGIC: &[u8; 8] = b"ZILSMS06";
const MANIFEST_MAGIC: &[u8; 8] = b"ZILSMS07";
const MANIFEST_HEADER: usize = 24;
const BINDING_ROW: usize = 25;
const MANIFEST_ROW: usize = 40;

#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticPublicationStats {
    /// Actual current raw function bindings inspected after subtree differencing.
    pub function_bindings_checked: usize,
    /// Complete raw plan comparisons for changed immutable plan owners.
    pub function_plan_bindings_compared: usize,
    pub serialized_functions: usize,
    pub reused_functions: usize,
    pub serialized_function_plans: usize,
    pub reused_function_plans: usize,
    pub serialized_function_plan_bytes: usize,
    pub reused_function_plan_bytes: usize,
    pub written_segments: usize,
    pub existing_segments: usize,
    pub written_bytes: usize,
    pub manifest_bytes: usize,
    pub canonical_functions: usize,
    pub bound_functions: usize,
    pub encode_ns: u128,
    pub guard_ns: u128,
    pub item_io_ns: u128,
    pub manifest_io_ns: u128,
    pub physical_pack_writes: usize,
    pub physical_pack_bytes: usize,
    pub manifest_writes: usize,
    pub compacted_items: usize,
    pub referenced_packs: usize,
    pub referenced_pack_payload_bytes: usize,
}

#[derive(Clone, Default)]
pub(crate) struct SemanticPublicationSession {
    previous: Option<Arc<TypedProgram>>,
    publication_guard: Arc<std::sync::Mutex<()>>,
    slots: zil_ir::PersistentVec<Arc<OnceLock<RawFunctionEncoding>>>,
    canonical: zil_ir::PersistentVec<Option<Arc<zil_ir::TypedFunction>>>,
    products: BTreeMap<FunctionId, zil_types::FunctionProduct>,
    bindings: zil_ir::PersistentVec<FunctionBinding>,
    plan_slots: zil_ir::PersistentVec<Option<Arc<RawFunctionPlanEncoding>>>,
    plan_inputs: zil_ir::PersistentVec<zil_ir::FunctionPlan>,
    canonical_count: usize,
    function_bindings_checked: usize,
    function_plan_bindings_compared: usize,
    plan_stats: FunctionPlanEncodingStats,
    semantic_packs: packed::Epoch,
    function_packs: packed::Epoch,
    memory: Option<Arc<MemoryPublication>>,
}
struct PreparedPublication {
    previous: Arc<TypedProgram>,
    slots: zil_ir::PersistentVec<Arc<OnceLock<RawFunctionEncoding>>>,
    canonical: zil_ir::PersistentVec<Option<Arc<zil_ir::TypedFunction>>>,
    bindings: zil_ir::PersistentVec<FunctionBinding>,
    plan_slots: zil_ir::PersistentVec<Option<Arc<RawFunctionPlanEncoding>>>,
    plan_inputs: zil_ir::PersistentVec<zil_ir::FunctionPlan>,
    plan_stats: FunctionPlanEncodingStats,
    canonical_count: usize,
    function_bindings_checked: usize,
    function_plan_bindings_compared: usize,
    memory: Option<Arc<MemoryPublication>>,
}

/// One current raw encoding epoch, owned by the existing publication/checkpoint.
/// Disk bytes supply comparison inputs only; current complete values select reuse.
struct MemoryPublication {
    typed: Arc<TypedProgram>,
    candidate: CandidatePlan,
    limits: Limits,
    canonical: zil_ir::PersistentVec<Option<Arc<zil_ir::TypedFunction>>>,
    bindings: zil_ir::PersistentVec<FunctionBinding>,
    slots: zil_ir::PersistentVec<Arc<OnceLock<RawFunctionEncoding>>>,
    plan_slots: zil_ir::PersistentVec<Option<Arc<RawFunctionPlanEncoding>>>,
    segments: Vec<WireSegment>,
}
pub(super) struct LoadedPublication {
    packs: packed::Epoch,
    memory: Option<Arc<MemoryPublication>>,
}
#[derive(Default)]
struct PublicationAttempts {
    functions: usize,
    bindings: usize,
    plans: usize,
}

#[derive(Clone, Debug)]
struct FunctionBinding {
    function: FunctionId,
    canonical: bool,
    span: zil_base::Span,
}

impl SemanticPublicationSession {
    /// Carry only raw byte hints across the filesystem probe's Store boundary.
    /// Successful current encoding owners/checkpoints remain transactional.
    pub(crate) fn adopt_loaded(&mut self, loaded: Self) {
        self.memory = loaded.memory;
        self.semantic_packs = loaded.semantic_packs;
    }

    /// A Types raw owner may retain these bounded raw serialization/placement
    /// hints. No directory handle or Verified capability enters the checkpoint.
    pub(crate) fn checkpoint_for(&self, typed: &Arc<TypedProgram>) -> Option<Self> {
        let count = typed.module.functions.len();
        (self
            .previous
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, typed))
            && self.slots.len() == count
            && self.canonical.len() == count
            && self.bindings.len() == count)
            .then(|| self.clone())
    }
    pub(crate) fn function_packs(&mut self) -> &mut packed::Epoch {
        &mut self.function_packs
    }
    /// Public digests are not evidence. A producer handle must match the exact
    /// current raw value; reversible transport equality is checked separately.
    #[cfg(test)]
    pub(crate) fn bind_products(&mut self, products: &[zil_types::FunctionProduct]) {
        self.products = products
            .iter()
            .map(|product| (product.id, product.clone()))
            .collect();
    }
    #[cfg(test)]
    fn encode(
        &mut self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        limits: &Limits,
    ) -> Option<(Vec<WireSegment>, FunctionEncodingStats)> {
        let (segments, stats, prepared) = self.prepare(typed, candidate, limits, false)?;
        self.install(prepared);
        Some((segments, stats))
    }
    fn install(&mut self, prepared: PreparedPublication) {
        self.previous = Some(prepared.previous);
        self.slots = prepared.slots;
        self.canonical = prepared.canonical;
        self.bindings = prepared.bindings;
        self.plan_slots = prepared.plan_slots;
        self.plan_inputs = prepared.plan_inputs;
        self.plan_stats = prepared.plan_stats;
        self.canonical_count = prepared.canonical_count;
        self.function_bindings_checked = prepared.function_bindings_checked;
        self.function_plan_bindings_compared = prepared.function_plan_bindings_compared;
        self.memory = prepared.memory;
    }
    #[cfg(test)]
    fn prepare(
        &self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        limits: &Limits,
        relative_bindings: bool,
    ) -> Option<(Vec<WireSegment>, FunctionEncodingStats, PreparedPublication)> {
        self.prepare_with_products(typed, candidate, limits, relative_bindings, None)
    }
    fn prepare_with_products(
        &self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        limits: &Limits,
        relative_bindings: bool,
        products: Option<&[zil_types::FunctionProduct]>,
    ) -> Option<(Vec<WireSegment>, FunctionEncodingStats, PreparedPublication)> {
        // Checkpoints may share raw OnceLock slots. Serialize aggregate cache
        // admission across them; their raw hints never authorize Verify.
        let _publication = self.publication_guard.lock().ok()?;
        metadata_budget(typed, limits)?;
        // Admission precedes allocation of the publication tables. The raw
        // Writer still performs its original ordered checks on the full stream.
        let count = typed.module.functions.len();
        let plan_count = candidate.functions.len();
        if count > limits.max_functions.min(limits.max_nodes)
            || plan_count > limits.max_functions.min(limits.max_nodes)
        {
            return None;
        }
        let mut attempts = PublicationAttempts::default();
        if let Some(prepared) = self
            .memory
            .as_ref()
            .and_then(|memory| memory.prepare(typed, candidate, limits, &mut attempts))
        {
            return Some(prepared);
        }
        // A restored raw image is only a staged comparison/encoding baseline;
        // failed publication leaves the successful owner/checkpoint unchanged.
        let previous = self
            .memory
            .as_ref()
            .map(|memory| &memory.typed)
            .or(self.previous.as_ref());
        let old_canonical = self
            .memory
            .as_ref()
            .map_or(&self.canonical, |memory| &memory.canonical);
        let old_slots = self
            .memory
            .as_ref()
            .map_or(&self.slots, |memory| &memory.slots);
        let old_bindings = self
            .memory
            .as_ref()
            .map_or(&self.bindings, |memory| &memory.bindings);
        let old_plans = self
            .memory
            .as_ref()
            .map_or(&self.plan_inputs, |memory| &memory.candidate.functions);
        let old_plan_slots = self
            .memory
            .as_ref()
            .map_or(&self.plan_slots, |memory| &memory.plan_slots);
        let mut canonical = publication_prefix(old_canonical, count);
        let mut slots = publication_prefix(old_slots, count);
        let mut bindings = publication_prefix(old_bindings, count);
        let dirty = previous.map_or_else(
            || (0..count).collect(),
            |previous| {
                typed
                    .module
                    .functions
                    .changed_indices(&previous.module.functions)
            },
        );
        let mut canonical_count = old_canonical.iter().filter(|body| body.is_some()).count();
        for removed in old_canonical.iter().skip(count) {
            canonical_count -= usize::from(removed.is_some());
        }
        let mut function_bindings_checked = attempts.bindings;
        for index in dirty.into_iter().filter(|index| *index < count) {
            function_bindings_checked += 1;
            let function = &typed.module.functions[index];
            let prior = old_canonical.get(index).and_then(Option::as_ref);
            let equal_raw = previous
                .and_then(|p| p.module.functions.get(index))
                .is_some_and(|old| old == function);
            // A canonical transport association remains valid for an exact
            // raw value. Changed values need a real producer handle or a
            // reversible span conversion followed by complete function Eq.
            // Successful MemoryPublication reuse above needs no per-function
            // product table at all. A rebuild borrows the actual dense current
            // products only if its normal encoder needs canonical association.
            let product = products
                .and_then(|rows| rows.get(function.id.0 as usize))
                .filter(|product| product.id == function.id)
                .or_else(|| self.products.get(&function.id));
            let body = if equal_raw && prior.is_some() {
                prior.cloned()
            } else {
                product
                    .and_then(|product| product.canonical_function(function))
                    .or_else(|| {
                        relative_bindings
                            .then(|| relative_function(function))
                            .flatten()
                            .map(Arc::new)
                    })
            };
            let (body, slot) = if let Some(body) = body {
                // Full function equality includes every header, local, value,
                // block and relative span. A product digest never selects bytes.
                let reused = prior.filter(|old| old.as_ref() == body.as_ref());
                let slot = reused
                    .and_then(|_| old_slots.get(index))
                    .cloned()
                    .unwrap_or_else(|| Arc::new(OnceLock::new()));
                (Some(body), slot)
            } else {
                // Unproven functions retain complete current raw spans. Only
                // exact raw equality may retain their already-bound cache slot.
                let slot = (prior.is_none() && equal_raw)
                    .then(|| old_slots.get(index))
                    .flatten()
                    .cloned()
                    .unwrap_or_else(|| Arc::new(OnceLock::new()));
                (None, slot)
            };
            canonical_count -= usize::from(prior.is_some());
            canonical_count += usize::from(body.is_some());
            let binding = FunctionBinding {
                function: function.id,
                canonical: body.is_some(),
                span: function.span,
            };
            publication_set(&mut canonical, index, body);
            publication_set(&mut slots, index, slot);
            publication_set(&mut bindings, index, binding);
        }
        let mut plan_slots = publication_prefix(old_plan_slots, plan_count);
        while plan_slots.len() < plan_count {
            plan_slots.push(None);
        }
        let mut function_plan_bindings_compared = attempts.plans;
        for index in candidate
            .functions
            .changed_indices(old_plans)
            .into_iter()
            .filter(|index| *index < plan_count)
        {
            if let Some(raw) = plan_slots[index].as_ref() {
                function_plan_bindings_compared += 1;
                let rebound = raw.bind_owner(candidate.functions.owner(index)?);
                // Result equality keeps the bytes but binds the new exact raw
                // owner. An unequal row cannot retain its old fragment.
                plan_slots.replace(index, rebound);
            }
        }
        let mut stats = FunctionEncodingStats::default();
        let mut plan_stats = FunctionPlanEncodingStats::default();
        let wire = zil_ir::wire::encode_module_candidate_persistent_segments(
            &typed.module,
            candidate,
            || {
                typed
                    .module
                    .functions
                    .iter()
                    .zip(&canonical)
                    .zip(&slots)
                    .map(|((raw, canonical), slot)| {
                        (
                            canonical.as_ref().map_or(raw, |body| body.as_ref()),
                            slot.as_ref(),
                        )
                    })
            },
            &mut plan_slots,
            limits,
            &mut stats,
            &mut plan_stats,
        )
        .ok()?;
        stats.serialized_functions += attempts.functions;
        function_plan_bindings_compared += plan_stats.compared_function_plans;
        let wire_length = wire.iter().try_fold(0usize, |length, segment| {
            length.checked_add(segment.bytes.len())
        })?;
        let tail = encode_metadata(typed, limits)?;
        let total = 12usize.checked_add(wire_length)?.checked_add(tail.len())?;
        if total > limits.max_source_bytes {
            return None;
        }
        let mut prefix = Vec::with_capacity(12);
        prefix.extend_from_slice(MAGIC);
        prefix.extend_from_slice(&u32::try_from(wire_length).ok()?.to_le_bytes());
        let mut segments = Vec::with_capacity(wire.len() + 2);
        segments.push(WireSegment {
            fingerprint: Fingerprint::of(&prefix),
            bytes: prefix.into(),
        });
        segments.extend(wire);
        segments.push(WireSegment {
            fingerprint: Fingerprint::of(&tail),
            bytes: tail.into(),
        });
        let memory = MemoryPublication::new(
            typed.clone(),
            candidate.clone(),
            limits,
            canonical.clone(),
            bindings.clone(),
            slots.clone(),
            plan_slots.clone(),
            segments.clone(),
        )
        .map(Arc::new);
        Some((
            segments,
            stats,
            PreparedPublication {
                previous: typed.clone(),
                slots,
                canonical,
                bindings,
                plan_slots,
                plan_inputs: candidate.functions.clone(),
                plan_stats,
                canonical_count,
                function_bindings_checked,
                function_plan_bindings_compared,
                memory,
            },
        ))
    }
}

// This optimization knows the v47 field boundaries, not a new wire format.
// Unknown versions/segmentation always use the original encoder.
const MEMORY_WIRE_VERSION: u32 = 47;

fn relative_function(function: &zil_ir::TypedFunction) -> Option<zil_ir::TypedFunction> {
    let origin = function.span;
    let mut relative = function.clone();
    let convert = |span: &mut zil_base::Span| -> Option<()> {
        if span.file != origin.file
            || span.start < origin.start
            || span.start > span.end
            || span.end > origin.end
        {
            return None;
        }
        *span = zil_base::Span::new(
            FileId(0),
            span.start - origin.start,
            span.end - origin.start,
        );
        Some(())
    };
    convert(&mut relative.span)?;
    for local in &mut relative.locals {
        convert(&mut local.span)?;
    }
    for instruction in relative
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        convert(&mut instruction.span)?;
    }
    Some(relative)
}

fn module_header(program: &TypedProgram) -> TypedModule {
    let mut header = program.module.clone();
    header.functions = Default::default();
    header.source_fingerprint = Fingerprint::ZERO;
    header
}

fn plan_byte_length(plan: &zil_ir::FunctionPlan) -> Option<usize> {
    let zil_ir::FunctionPlan {
        function: _,
        blocks,
        holds,
    } = plan;
    let mut bytes = 12usize; // function ID and the two vector lengths
    for block in blocks {
        let zil_ir::BlockPlan {
            block: _,
            initialized_in,
            initialized_out,
        } = block;
        bytes = bytes.checked_add(12)?.checked_add(
            initialized_in
                .len()
                .checked_add(initialized_out.len())?
                .checked_mul(4)?,
        )?;
    }
    for hold in holds {
        let zil_ir::ValueHold { value: _, roots } = hold;
        bytes = bytes.checked_add(8)?;
        for root in roots {
            bytes = bytes.checked_add(match root {
                zil_ir::HoldRoot::Static => 1,
                zil_ir::HoldRoot::Local(_)
                | zil_ir::HoldRoot::Capture(_)
                | zil_ir::HoldRoot::Parameter(_) => 5,
            })?;
        }
    }
    Some(bytes)
}

impl MemoryPublication {
    fn restore(
        logical: &[u8],
        segments: Vec<WireSegment>,
        bytes: &[u8],
        limits: &Limits,
        cancellation: &zil_db::CancellationToken,
    ) -> Option<(TypedProgram, CandidatePlan, Option<Arc<Self>>)> {
        if cancellation.is_cancelled() {
            return None;
        }
        let (total, count, start) = layout(logical, limits)?;
        if segments.len() != count {
            return None;
        }
        let mut checked = 0usize;
        for (row, segment) in logical[MANIFEST_HEADER..start]
            .as_chunks::<MANIFEST_ROW>()
            .0
            .iter()
            .zip(&segments)
        {
            let length = usize::try_from(u64::from_le_bytes(row[32..40].try_into().ok()?)).ok()?;
            checked = checked.checked_add(length)?;
            if length == 0
                || checked > total
                || checked > limits.max_source_bytes
                || segment.bytes.len() != length
                || segment.fingerprint.0 != row[..32]
            {
                return None;
            }
        }
        // assemble checks these declarations before item I/O/allocation too.
        if checked != total || bytes.len() != total {
            return None;
        }
        // The one ordinary decoder establishes every consumed boundary and
        // aggregate node receipt. Unexpected segmentation only disables slots;
        // it never causes a second candidate/function/probe decode.
        let wire_segments = segments
            .get(1..segments.len().saturating_sub(1))
            .unwrap_or(&[]);
        let (mut raw, candidate, encodings) =
            decode_with_encodings(bytes, limits, Some(wire_segments))?;
        let hints = (|| {
            if zil_ir::wire::WIRE_VERSION != MEMORY_WIRE_VERSION
                || logical.get(..8)? != MANIFEST_MAGIC
            {
                return None;
            }
            let encodings = encodings?;
            let count = raw.module.functions.len();
            if count == 0
                || segments.len()
                    != count
                        .checked_add(candidate.functions.len())?
                        .checked_add(6)?
                || encodings.functions.len() != count
                || encodings.plans.len() != candidate.functions.len()
            {
                return None;
            }
            let bindings: zil_ir::PersistentVec<_> = logical[start..]
                .as_chunks::<BINDING_ROW>()
                .0
                .iter()
                .map(binding)
                .collect::<Option<Vec<_>>>()?
                .into();
            if bindings.len() != count {
                return None;
            }
            let mut canonical = zil_ir::PersistentVec::default();
            let mut slots = zil_ir::PersistentVec::default();
            for (index, encoding) in encodings.functions.into_iter().enumerate() {
                if cancellation.is_cancelled() {
                    return None;
                }
                canonical.push(
                    bindings[index]
                        .canonical
                        .then(|| raw.module.functions.owner(index).unwrap().clone()),
                );
                slots.push(Arc::new(OnceLock::from(encoding)));
            }
            let plans = encodings.plans.into_iter().map(Some).collect();
            Some((canonical, bindings, slots, plans))
        })();
        rebind(&mut raw, logical, limits)?;
        if cancellation.is_cancelled() {
            return None;
        }
        let memory = hints.and_then(|(canonical, bindings, slots, plans)| {
            Self::new(
                Arc::new(raw.clone()),
                candidate.clone(),
                limits,
                canonical,
                bindings,
                slots,
                plans,
                segments,
            )
            .map(Arc::new)
        });
        Some((raw, candidate, memory))
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        typed: Arc<TypedProgram>,
        candidate: CandidatePlan,
        limits: &Limits,
        canonical: zil_ir::PersistentVec<Option<Arc<zil_ir::TypedFunction>>>,
        bindings: zil_ir::PersistentVec<FunctionBinding>,
        slots: zil_ir::PersistentVec<Arc<OnceLock<RawFunctionEncoding>>>,
        plan_slots: zil_ir::PersistentVec<Option<Arc<RawFunctionPlanEncoding>>>,
        segments: Vec<WireSegment>,
    ) -> Option<Self> {
        let count = typed.module.functions.len();
        let plans = candidate.functions.len();
        if zil_ir::wire::WIRE_VERSION != MEMORY_WIRE_VERSION
            || count == 0
            || segments.len() != count.checked_add(plans)?.checked_add(6)?
            || canonical.len() != count
            || bindings.len() != count
            || slots.len() != count
            || plan_slots.len() != plans
        {
            return None;
        }
        let prefix = &segments[0].bytes;
        let header = &segments[1].bytes;
        let plan_header = &segments[count + 3].bytes;
        let wire_length = segments[1..segments.len() - 1]
            .iter()
            .try_fold(0usize, |sum, segment| sum.checked_add(segment.bytes.len()))?;
        if prefix.len() != 12
            || prefix.get(..8)? != MAGIC
            || usize::try_from(u32::from_le_bytes(prefix[8..12].try_into().ok()?)).ok()?
                != wire_length
            || header.get(..8)? != b"ZILMODP\0"
            || u32::from_le_bytes(header.get(8..12)?.try_into().ok()?) != MEMORY_WIRE_VERSION
            || header.get(20..52)? != typed.module.source_fingerprint.0
            || u32::from_le_bytes(
                header
                    .get(header.len().checked_sub(4)?..)?
                    .try_into()
                    .ok()?,
            ) as usize
                != count
            || plan_header.len() != 72
            || plan_header[0..4] != candidate.schema_version.to_le_bytes()
            || plan_header[4..36] != candidate.module_fingerprint.0
            || plan_header[36..68] != candidate.target_fingerprint.0
            || u32::from_le_bytes(plan_header[68..72].try_into().ok()?) as usize != plans
        {
            return None;
        }
        for (index, plan) in candidate.functions.iter().enumerate() {
            let segment = &segments[count + 4 + index];
            if let Some(raw) = &plan_slots[index] {
                // Same-decoder receipts already establish the complete row's
                // actual byte range; do not walk its blocks/holds a second time.
                if !Arc::ptr_eq(&raw.shared_bytes(), &segment.bytes) {
                    return None;
                }
            } else if segment.bytes.len() != plan_byte_length(plan)? {
                return None;
            }
        }
        Some(Self {
            typed,
            candidate,
            limits: limits.clone(),
            canonical,
            bindings,
            slots,
            plan_slots,
            segments,
        })
    }

    fn prepare(
        &self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        limits: &Limits,
        attempts: &mut PublicationAttempts,
    ) -> Option<(Vec<WireSegment>, FunctionEncodingStats, PreparedPublication)> {
        if &self.limits != limits
            || module_header(typed) != module_header(&self.typed)
            || typed.module.functions.len() != self.typed.module.functions.len()
        {
            return None;
        }
        // Only the current module fingerprint may differ in an otherwise equal
        // complete plan. This is data rebinding, never digest authorization.
        let CandidatePlan {
            schema_version,
            module_fingerprint: _,
            target_fingerprint,
            functions,
            origins,
            responsibilities,
            unsupported,
        } = candidate;
        if *schema_version != self.candidate.schema_version
            || *target_fingerprint != self.candidate.target_fingerprint
            || origins != &self.candidate.origins
            || responsibilities != &self.candidate.responsibilities
            || unsupported != &self.candidate.unsupported
            || functions.len() != self.candidate.functions.len()
        {
            return None;
        }
        let plan_dirty = functions.changed_indices(&self.candidate.functions);
        let compared_plans = plan_dirty.len();
        let mut plan_slots = self.plan_slots.clone();
        for &index in &plan_dirty {
            attempts.plans += 1;
            if let Some(raw) = plan_slots[index].as_ref() {
                let rebound = raw.bind_owner(functions.owner(index)?)?;
                plan_slots.replace(index, Some(rebound));
            } else if functions[index] != self.candidate.functions[index] {
                return None;
            }
        }
        let count = typed.module.functions.len();
        let mut segments = self.segments.clone();
        let mut slots = self.slots.clone();
        let mut canonical = self.canonical.clone();
        let mut bindings = self.bindings.clone();
        let mut stats = FunctionEncodingStats {
            serialized_functions: 0,
            reused_functions: count,
        };
        let dirty = typed
            .module
            .functions
            .changed_indices(&self.typed.module.functions);
        for &index in &dirty {
            attempts.bindings += 1;
            let function = &typed.module.functions[index];
            let old = &self.typed.module.functions[index];
            let body = if self.bindings[index].canonical {
                Some(Arc::new(relative_function(function)?))
            } else {
                None
            };
            let equal = match (&body, &self.canonical[index]) {
                (Some(current), Some(previous)) => current == previous,
                (None, None) => function == old,
                _ => false,
            };
            if !equal {
                let current = body.as_deref().unwrap_or(function);
                // The slot was sealed by the current encoder or the same full
                // decoder that produced this exact old body. No replay decode
                // is needed on the first private edit after restart either.
                let previous = self.slots[index].get()?;
                if !previous.matches_limits(limits)
                    || !Arc::ptr_eq(&previous.shared_bytes(), &self.segments[index + 2].bytes)
                {
                    return None;
                }
                let old_cost = previous.node_cost();
                attempts.functions += 1;
                let encoding = zil_ir::wire::encode_raw_function(current, limits).ok()?;
                // All other wire fields/plan rows are unchanged. A non-growing
                // actual node receipt preserves the original aggregate quota.
                // Growth/unknown cost returns to the original ordered writer.
                if encoding.node_cost() > old_cost {
                    return None;
                }
                segments[index + 2] = WireSegment {
                    fingerprint: encoding.content_fingerprint(),
                    bytes: encoding.shared_bytes(),
                };
                slots.replace(index, Arc::new(OnceLock::from(encoding)));
                stats.serialized_functions += 1;
                stats.reused_functions -= 1;
            }
            canonical.replace(index, body);
            bindings.replace(
                index,
                FunctionBinding {
                    function: function.id,
                    canonical: self.bindings[index].canonical,
                    span: function.span,
                },
            );
        }
        let mut header = segments[1].bytes.to_vec();
        header[20..52].copy_from_slice(&typed.module.source_fingerprint.0);
        segments[1] = wire_segment(header);
        let mut header = segments[count + 3].bytes.to_vec();
        header[4..36].copy_from_slice(&candidate.module_fingerprint.0);
        segments[count + 3] = wire_segment(header);
        let last = segments.len() - 1;
        segments[last] = wire_segment(encode_metadata(typed, limits)?);
        let wire_length = segments[1..last]
            .iter()
            .try_fold(0usize, |sum, segment| sum.checked_add(segment.bytes.len()))?;
        let total = 12usize
            .checked_add(wire_length)?
            .checked_add(segments[last].bytes.len())?;
        if total > limits.max_source_bytes {
            return None;
        }
        let mut prefix = MAGIC.to_vec();
        prefix.extend_from_slice(&u32::try_from(wire_length).ok()?.to_le_bytes());
        segments[0] = wire_segment(prefix);
        let canonical_count = canonical.iter().filter(|body| body.is_some()).count();
        let reused_plan_bytes = segments[count + 4..count + 4 + candidate.functions.len()]
            .iter()
            .try_fold(0usize, |sum, segment| sum.checked_add(segment.bytes.len()))?;
        let memory = Self::new(
            typed.clone(),
            candidate.clone(),
            limits,
            canonical.clone(),
            bindings.clone(),
            slots.clone(),
            plan_slots.clone(),
            segments.clone(),
        )?;
        Some((
            segments,
            stats,
            PreparedPublication {
                previous: typed.clone(),
                canonical,
                bindings,
                slots,
                plan_slots,
                plan_inputs: candidate.functions.clone(),
                plan_stats: FunctionPlanEncodingStats {
                    reused_function_plans: candidate.functions.len(),
                    reused_function_plan_bytes: reused_plan_bytes,
                    ..Default::default()
                },
                canonical_count,
                function_bindings_checked: dirty.len(),
                function_plan_bindings_compared: compared_plans,
                memory: Some(Arc::new(memory)),
            },
        ))
    }
}

fn wire_segment(bytes: Vec<u8>) -> WireSegment {
    WireSegment {
        fingerprint: Fingerprint::of(&bytes),
        bytes: bytes.into(),
    }
}

/// Count changes are structural edits. A shortened dense sequence rebuilds its
/// surviving prefix; same-size/append updates share the prior root and leaves.
fn publication_prefix<T: Clone>(
    previous: &zil_ir::PersistentVec<T>,
    count: usize,
) -> zil_ir::PersistentVec<T> {
    if count < previous.len() {
        previous.iter().take(count).cloned().collect()
    } else {
        previous.clone()
    }
}
fn publication_set<T>(items: &mut zil_ir::PersistentVec<T>, index: usize, value: T) {
    if index < items.len() {
        items.replace(index, value);
    } else {
        debug_assert_eq!(index, items.len());
        items.push(value);
    }
}

pub(super) fn maximum_segments(limits: &Limits) -> Option<usize> {
    limits.max_functions.checked_mul(2)?.checked_add(6)
}

fn manifest<'a, I>(segments: &[WireSegment], bindings: I, limits: &Limits) -> Option<Vec<u8>>
where
    I: IntoIterator<Item = &'a FunctionBinding>,
    I::IntoIter: ExactSizeIterator,
{
    let bindings = bindings.into_iter();
    let maximum_segments = maximum_segments(limits)?;
    if segments.is_empty() || segments.len() > maximum_segments {
        return None;
    }
    if bindings.len() > limits.max_functions.min(limits.max_nodes) {
        return None;
    }
    let size = MANIFEST_HEADER
        .checked_add(segments.len().checked_mul(MANIFEST_ROW)?)?
        .checked_add(bindings.len().checked_mul(BINDING_ROW)?)?;
    if size > limits.max_source_bytes {
        return None;
    }
    let total = segments.iter().try_fold(0usize, |length, segment| {
        length.checked_add(segment.bytes.len())
    })?;
    if total > limits.max_source_bytes {
        return None;
    }
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(MANIFEST_MAGIC);
    bytes.extend_from_slice(&u64::try_from(total).ok()?.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(segments.len()).ok()?.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(bindings.len()).ok()?.to_le_bytes());
    for segment in segments {
        if segment.bytes.is_empty() {
            return None;
        }
        bytes.extend_from_slice(&segment.fingerprint.0);
        bytes.extend_from_slice(&u64::try_from(segment.bytes.len()).ok()?.to_le_bytes());
    }
    for binding in bindings {
        bytes.extend_from_slice(&binding.function.0.to_le_bytes());
        bytes.push(u8::from(binding.canonical));
        bytes.extend_from_slice(&binding.span.file.0.to_le_bytes());
        bytes.extend_from_slice(&u64::try_from(binding.span.start).ok()?.to_le_bytes());
        bytes.extend_from_slice(&u64::try_from(binding.span.end).ok()?.to_le_bytes());
    }
    Some(bytes)
}

fn layout(manifest: &[u8], limits: &Limits) -> Option<(usize, usize, usize)> {
    if manifest.len() > limits.max_source_bytes {
        return None;
    }
    let maximum = match manifest.get(..8)? {
        magic if magic == MANIFEST_MAGIC => maximum_segments(limits)?,
        magic if magic == LEGACY_MANIFEST_MAGIC => limits.max_functions.checked_add(4)?,
        _ => return None,
    };
    let total = usize::try_from(u64::from_le_bytes(manifest.get(8..16)?.try_into().ok()?)).ok()?;
    let count = u32::from_le_bytes(manifest.get(16..20)?.try_into().ok()?) as usize;
    let functions = u32::from_le_bytes(manifest.get(20..24)?.try_into().ok()?) as usize;
    let start = MANIFEST_HEADER.checked_add(count.checked_mul(MANIFEST_ROW)?)?;
    if total > limits.max_source_bytes
        || count == 0
        || count > maximum
        || functions > limits.max_functions.min(limits.max_nodes)
        || start.checked_add(functions.checked_mul(BINDING_ROW)?)? != manifest.len()
    {
        return None;
    }
    for row in manifest[start..].as_chunks::<BINDING_ROW>().0 {
        binding(row)?;
    }
    Some((total, count, start))
}
fn binding(row: &[u8; BINDING_ROW]) -> Option<FunctionBinding> {
    let function = FunctionId(u32::from_le_bytes(row[..4].try_into().ok()?));
    let canonical = match row[4] {
        0 => false,
        1 => true,
        _ => return None,
    };
    let file = FileId(u32::from_le_bytes(row[5..9].try_into().ok()?));
    let start = usize::try_from(u64::from_le_bytes(row[9..17].try_into().ok()?)).ok()?;
    let end = usize::try_from(u64::from_le_bytes(row[17..25].try_into().ok()?)).ok()?;
    if start > end {
        return None;
    }
    Some(FunctionBinding {
        function,
        canonical,
        span: zil_base::Span::new(file, start, end),
    })
}
fn rebind(program: &mut TypedProgram, manifest: &[u8], limits: &Limits) -> Option<()> {
    let (_, _, start) = layout(manifest, limits)?;
    let rows = manifest[start..].as_chunks::<BINDING_ROW>().0;
    if rows.len() != program.module.functions.len() {
        return None;
    }
    for (function, row) in program.module.functions.iter_mut().zip(rows) {
        let binding = binding(row)?;
        if function.id != binding.function {
            return None;
        }
        if !binding.canonical {
            if function.span != binding.span {
                return None;
            }
            continue;
        }
        let length = binding.span.end.checked_sub(binding.span.start)?;
        if function.span != zil_base::Span::new(FileId(0), 0, length) {
            return None;
        }
        let convert = |span: &mut zil_base::Span| -> Option<()> {
            if span.file != FileId(0) || span.start > span.end || span.end > length {
                return None;
            }
            span.file = binding.span.file;
            span.start = binding.span.start.checked_add(span.start)?;
            span.end = binding.span.start.checked_add(span.end)?;
            Some(())
        };
        convert(&mut function.span)?;
        for local in &mut function.locals {
            convert(&mut local.span)?;
        }
        for instruction in function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            convert(&mut instruction.span)?;
        }
    }
    Some(())
}
fn decode_bound(
    bytes: &[u8],
    manifest: &[u8],
    limits: &Limits,
) -> Option<(TypedProgram, CandidatePlan)> {
    // Both ordinary decoders return raw data only; they do not validate module
    // or candidate semantic fingerprints. Rebind before returning to any such
    // check or to independent current Verify.
    let (mut program, candidate) = decode(bytes, limits)?;
    rebind(&mut program, manifest, limits)?;
    Some((program, candidate))
}

fn assemble(
    manifest: &[u8],
    limits: &Limits,
    load: &mut dyn FnMut(Fingerprint) -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    assemble_buffers(manifest, limits, load)
}

fn assemble_shared(
    manifest: &[u8],
    limits: &Limits,
    load: &mut dyn FnMut(Fingerprint) -> Option<Arc<[u8]>>,
) -> Option<Vec<u8>> {
    assemble_buffers(manifest, limits, load)
}

fn assemble_buffers<B: AsRef<[u8]>>(
    manifest: &[u8],
    limits: &Limits,
    load: &mut dyn FnMut(Fingerprint) -> Option<B>,
) -> Option<Vec<u8>> {
    let (total, _count, start) = layout(manifest, limits)?;
    let mut expected = 0usize;
    for row in manifest[MANIFEST_HEADER..start]
        .as_chunks::<MANIFEST_ROW>()
        .0
    {
        let length = usize::try_from(u64::from_le_bytes(row[32..40].try_into().ok()?)).ok()?;
        if length == 0 {
            return None;
        }
        expected = expected.checked_add(length)?;
        if expected > total {
            return None;
        }
    }
    if expected != total {
        return None;
    }
    let mut bytes = Vec::with_capacity(total);
    for row in manifest[MANIFEST_HEADER..start]
        .as_chunks::<MANIFEST_ROW>()
        .0
    {
        let fingerprint = Fingerprint(row[..32].try_into().ok()?);
        let length = usize::try_from(u64::from_le_bytes(row[32..40].try_into().ok()?)).ok()?;
        let loaded = load(fingerprint)?;
        let segment = loaded.as_ref();
        if segment.len() != length || Fingerprint::of(segment) != fingerprint {
            return None;
        }
        bytes.extend_from_slice(segment);
    }
    Some(bytes)
}

impl Store {
    /// Transfer the just-loaded raw serialization hints to the request owner.
    /// Filesystem discovery can carry this alongside its preloaded pair, rather
    /// than asking a new Store to reread/decode the same candidate later.
    pub(crate) fn take_loaded_publication(
        &self,
        options: &CompileOptions,
    ) -> Option<SemanticPublicationSession> {
        let loaded = self.loaded_publication.borrow_mut().take()?;
        if options.cancellation.is_cancelled() {
            return None;
        }
        Some(SemanticPublicationSession {
            memory: loaded
                .memory
                .filter(|memory| memory.limits == options.limits),
            semantic_packs: loaded.packs,
            ..Default::default()
        })
    }
    /// Loaded bytes are still raw. This bounded locator hint grants no trusted
    /// file stamp: publication must recheck each referenced complete pack.
    pub(crate) fn remember_packed_epoch(
        &self,
        options: &CompileOptions,
        publication: &mut SemanticPublicationSession,
    ) {
        if let Some(loaded) = self.take_loaded_publication(options) {
            publication.semantic_packs = loaded.semantic_packs;
            publication.memory = loaded.memory;
            return;
        }
        let Some(max_items) = maximum_segments(&options.limits) else {
            return;
        };
        let bounds = packed::Bounds {
            maximum: self.maximum,
            max_items,
        };
        let Some(maximum) = bounds.manifest_maximum() else {
            return;
        };
        if let Ok(CacheRead::Candidate(bytes)) =
            ArtifactCache::with_limit(&self.root, maximum).inspect_candidate(self.key)
            && let Some(epoch) = packed::Epoch::hint(
                &self.root.join("packs-v1"),
                &bytes,
                bounds,
                &options.cancellation,
            )
        {
            publication.semantic_packs = epoch;
        }
    }
    pub(super) fn load_segments(
        &self,
        manifest: &[u8],
        options: &CompileOptions,
    ) -> Option<(TypedProgram, CandidatePlan)> {
        self.loaded_publication.borrow_mut().take();
        if packed::is_packed(manifest) {
            let bounds = packed::Bounds {
                maximum: self.maximum,
                max_items: maximum_segments(&options.limits)?,
            };
            let mut reader = packed::Reader::new(
                &self.root.join("packs-v1"),
                manifest,
                bounds,
                &options.cancellation,
            )?;
            let logical = reader.take_logical();
            let mut segments = Vec::new();
            let bytes = assemble_shared(&logical, &options.limits, &mut |key| {
                let bytes = reader.load(key)?;
                segments.push(WireSegment {
                    fingerprint: key,
                    bytes: bytes.clone(),
                });
                Some(bytes)
            })?;
            let (program, candidate, memory) = MemoryPublication::restore(
                &logical,
                segments,
                &bytes,
                &options.limits,
                &options.cancellation,
            )?;
            if options.cancellation.is_cancelled() {
                return None;
            }
            *self.loaded_publication.borrow_mut() = Some(LoadedPublication {
                packs: reader.hint_epoch(),
                memory,
            });
            return Some((program, candidate));
        }
        let root = self.root.join("items");
        if !private_directory(&root, false) {
            return None;
        }
        let cache = ArtifactCache::with_limit(&root, self.maximum);
        let bytes = assemble(manifest, &options.limits, &mut |key| match cache
            .inspect_candidate(key)
            .ok()?
        {
            CacheRead::Candidate(bytes) => Some(bytes),
            _ => None,
        })?;
        decode_bound(&bytes, manifest, &options.limits)
    }
    pub(super) fn save_segments(
        &self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        options: &CompileOptions,
        plan: &artifacts::ArtifactPlan,
        publication: &mut SemanticPublicationSession,
        products: Option<&[zil_types::FunctionProduct]>,
    ) -> Option<SemanticPublicationStats> {
        let batch = publication_start(options.profile_compilation);
        if options.cancellation.is_cancelled() || plan.validate().is_err() {
            return None;
        }
        let mut guarded = plan.clone();
        let root = guarded
            .cache_directory("semantic-packs", &self.root.join("packs-v1"))
            .ok()?;
        let mut guard_ns = publication_elapsed(batch);
        let encode = publication_start(options.profile_compilation);
        let (segments, encoding, prepared) = publication.prepare_with_products(
            typed,
            candidate,
            &options.limits,
            options.push_reuse,
            products,
        )?;
        let manifest = manifest(&segments, &prepared.bindings, &options.limits)?;
        let encode_ns = publication_elapsed(encode);
        let pin = publication_start(options.profile_compilation);
        if options.cancellation.is_cancelled()
            || guarded.validate().is_err()
            || !private_directory(&self.root, true)
            || !private_directory(&root, true)
        {
            return None;
        }
        let namespace = guarded.pin_cache_directory(&root).ok()?;
        guard_ns += publication_elapsed(pin);
        let stats = packed::publish(
            packed::PublishContext {
                packs: &namespace,
                manifest_root: &self.root,
                namespace_key: &root,
                manifest_key: self.key,
                cancellation: &options.cancellation,
                profile: options.profile_compilation,
            },
            &manifest,
            segments
                .into_iter()
                .map(|segment| (segment.fingerprint, segment.bytes))
                .collect(),
            packed::Bounds {
                maximum: self.maximum,
                max_items: maximum_segments(&options.limits)?,
            },
            &mut publication.semantic_packs,
        )?;
        // The bounded transport publishes its manifest last and only then
        // advances its pack epoch. Install the matching raw binding epoch now.
        publication.install(prepared);
        let stats = SemanticPublicationStats {
            function_bindings_checked: publication.function_bindings_checked,
            function_plan_bindings_compared: publication.function_plan_bindings_compared,
            serialized_functions: encoding.serialized_functions,
            reused_functions: encoding.reused_functions,
            serialized_function_plans: publication.plan_stats.serialized_function_plans,
            reused_function_plans: publication.plan_stats.reused_function_plans,
            serialized_function_plan_bytes: publication.plan_stats.serialized_function_plan_bytes,
            reused_function_plan_bytes: publication.plan_stats.reused_function_plan_bytes,
            canonical_functions: publication.canonical_count,
            bound_functions: publication.bindings.len() - publication.canonical_count,
            written_segments: stats.written_items,
            existing_segments: stats.existing_items,
            written_bytes: stats.written_bytes,
            manifest_bytes: stats.manifest_bytes,
            encode_ns: encode_ns + stats.encode_ns,
            guard_ns: guard_ns + stats.guard_ns,
            item_io_ns: stats.item_io_ns,
            manifest_io_ns: stats.manifest_io_ns,
            physical_pack_writes: stats.physical_pack_writes,
            physical_pack_bytes: stats.physical_pack_bytes,
            manifest_writes: stats.manifest_writes,
            compacted_items: stats.compacted_items,
            referenced_packs: stats.referenced_packs,
            referenced_pack_payload_bytes: stats.referenced_pack_payload_bytes,
        };
        Some(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    const SOURCE: &str = "pub fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} pub fn main()->i32{return f(2);}";
    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "semantic-items-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn fixture(path: &Path, text: &str) -> (Arc<TypedProgram>, CandidatePlan) {
        fs::write(path, text).unwrap();
        let (typed, candidate) = super::super::tests::raw(path);
        (Arc::new(typed), candidate)
    }
    fn flatten(segments: &[WireSegment]) -> Vec<u8> {
        segments
            .iter()
            .flat_map(|segment| segment.bytes.iter().copied())
            .collect()
    }
    fn legacy_segments(
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        limits: &Limits,
    ) -> (Vec<WireSegment>, Vec<FunctionBinding>) {
        let mut publication = SemanticPublicationSession::default();
        let (current, _) = publication.encode(typed, candidate, limits).unwrap();
        let slots: Vec<_> = typed
            .module
            .functions
            .iter()
            .map(|_| OnceLock::new())
            .collect();
        let functions: Vec<_> = typed.module.functions.iter().zip(&slots).collect();
        let mut segments = vec![current[0].clone()];
        segments.extend(
            zil_ir::wire::encode_module_candidate_segments(
                &typed.module,
                candidate,
                &functions,
                limits,
                &mut FunctionEncodingStats::default(),
            )
            .unwrap(),
        );
        segments.push(current.last().unwrap().clone());
        assert_eq!(flatten(&segments), flatten(&current));
        (segments, publication.bindings.iter().cloned().collect())
    }
    #[test]
    fn segmented_semantics_preserves_legacy_bytes_and_shares_unchanged_functions() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let limits = Limits::default();
        let mut publication = SemanticPublicationSession::default();
        let (cold, stats) = publication.encode(&typed, &candidate, &limits).unwrap();
        assert_eq!(stats.serialized_functions, 2);
        assert_eq!(publication.plan_stats.serialized_function_plans, 2);
        assert_eq!(flatten(&cold), encode(&typed, &candidate, &limits).unwrap());
        let (warm, stats) = publication.encode(&typed, &candidate, &limits).unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (0, 2));
        assert_eq!(
            (
                publication.plan_stats.serialized_function_plans,
                publication.plan_stats.reused_function_plans
            ),
            (0, 2)
        );
        let shared = warm
            .iter()
            .filter(|item| cold.iter().any(|old| Arc::ptr_eq(&item.bytes, &old.bytes)))
            .count();
        assert_eq!(shared, 6);
        let (changed, next_candidate) = fixture(&path, &SOURCE.replace("x+1", "x+2"));
        let (edited, stats) = publication
            .encode(&changed, &next_candidate, &limits)
            .unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (1, 1));
        assert_eq!(publication.plan_stats.reused_function_plans, 2);
        assert_eq!(
            flatten(&edited),
            encode(&changed, &next_candidate, &limits).unwrap()
        );
        let shared = edited
            .iter()
            .filter(|item| warm.iter().any(|old| Arc::ptr_eq(&item.bytes, &old.bytes)))
            .count();
        assert_eq!(shared, 5);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn persistent_publication_consumes_only_changed_function_and_plan_owners() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let limits = Limits::default();
        let mut publication = SemanticPublicationSession::default();
        publication.encode(&typed, &candidate, &limits).unwrap();
        assert_eq!(publication.function_bindings_checked, 2);
        let cold = publication.checkpoint_for(&typed).unwrap();
        publication.encode(&typed, &candidate, &limits).unwrap();
        assert_eq!(publication.function_bindings_checked, 0);
        assert_eq!(publication.function_plan_bindings_compared, 0);
        assert!(publication.slots.ptr_eq(&cold.slots));
        assert!(publication.bindings.ptr_eq(&cold.bindings));
        assert!(publication.canonical.ptr_eq(&cold.canonical));
        assert!(publication.plan_slots.ptr_eq(&cold.plan_slots));

        let (fresh, fresh_candidate) = fixture(&path, &SOURCE.replace("x+1", "x+2"));
        assert_eq!(candidate.functions, fresh_candidate.functions);
        let mut private = (*typed).clone();
        private.module.source_fingerprint = fresh.module.source_fingerprint;
        private
            .module
            .functions
            .replace(0, fresh.module.functions[0].clone());
        let private = Arc::new(private);
        let mut plan = candidate.clone();
        plan.module_fingerprint = private.module.fingerprint();
        let (segments, stats) = publication.encode(&private, &plan, &limits).unwrap();
        assert_eq!(publication.function_bindings_checked, 1);
        assert_eq!(publication.function_plan_bindings_compared, 0);
        assert_eq!((stats.serialized_functions, stats.reused_functions), (1, 1));
        assert_eq!(
            flatten(&segments),
            encode(&private, &plan, &limits).unwrap()
        );
        assert!(Arc::ptr_eq(
            cold.slots.get(1).unwrap(),
            publication.slots.get(1).unwrap()
        ));
        assert!(Arc::ptr_eq(
            cold.bindings.owner(1).unwrap(),
            publication.bindings.owner(1).unwrap()
        ));
        assert!(publication.plan_slots.ptr_eq(&cold.plan_slots));

        // Reexecution may return a different owner with the exact same raw row.
        // Compare just that row once, then retain the rebound owner and bytes.
        let old_bytes = publication.plan_slots[1].as_ref().unwrap().shared_bytes();
        let row = plan.functions[1].clone();
        plan.functions.replace(1, row);
        publication.encode(&private, &plan, &limits).unwrap();
        assert_eq!(publication.function_bindings_checked, 0);
        assert_eq!(publication.function_plan_bindings_compared, 1);
        assert_eq!(publication.plan_stats.serialized_function_plans, 0);
        assert!(Arc::ptr_eq(
            &old_bytes,
            &publication.plan_slots[1].as_ref().unwrap().shared_bytes()
        ));
        let equal = publication.plan_slots.clone();
        publication.encode(&private, &plan, &limits).unwrap();
        assert_eq!(publication.function_plan_bindings_compared, 0);
        assert!(publication.plan_slots.ptr_eq(&equal));

        let checkpoint = publication.checkpoint_for(&private).unwrap();
        let short = Limits {
            max_source_bytes: flatten(&segments).len() - 1,
            ..limits.clone()
        };
        assert!(publication.encode(&private, &plan, &short).is_none());
        assert!(publication.slots.ptr_eq(&checkpoint.slots));
        assert!(publication.plan_slots.ptr_eq(&checkpoint.plan_slots));
        assert!(publication.bindings.ptr_eq(&checkpoint.bindings));
        let changed_limits = Limits {
            max_tokens: limits.max_tokens - 1,
            ..limits.clone()
        };
        let (bytes, stats) = publication
            .encode(&private, &plan, &changed_limits)
            .unwrap();
        assert_eq!(stats.serialized_functions, 2);
        assert_eq!(publication.plan_stats.serialized_function_plans, 2);
        assert_eq!(
            flatten(&bytes),
            encode(&private, &plan, &changed_limits).unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_manifest_does_not_advance_persistent_publication_epoch() {
        let directory = directory();
        let path = directory.join("main.zil");
        let options = CompileOptions {
            producer_reuse: true,
            ..Default::default()
        };
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), SOURCE);
        let artifacts = artifacts::ArtifactPlan::new(&source, &options).unwrap();
        let (typed, candidate) = fixture(&path, SOURCE);
        let store = Store {
            root: directory.join("semantic-v6"),
            key: Fingerprint::of(b"old"),
            maximum: options.limits.max_source_bytes,
            segmented: true,
            loaded_publication: Default::default(),
        };
        let mut publication = SemanticPublicationSession::default();
        store
            .save(&typed, &candidate, &options, &artifacts, &mut publication)
            .unwrap();
        let checkpoint = publication.checkpoint_for(&typed).unwrap();
        let (fresh, _) = fixture(&path, &SOURCE.replace("x+1", "x+2"));
        let mut private = (*typed).clone();
        private.module.source_fingerprint = fresh.module.source_fingerprint;
        private
            .module
            .functions
            .replace(0, fresh.module.functions[0].clone());
        let private = Arc::new(private);
        let mut plan = candidate.clone();
        plan.module_fingerprint = private.module.fingerprint();
        let next = Store {
            root: store.root.clone(),
            key: Fingerprint::of(b"next"),
            maximum: store.maximum,
            segmented: true,
            loaded_publication: Default::default(),
        };
        // All encoding can succeed and packs can be written, but the final
        // atomic manifest rename must fail against this directory.
        fs::create_dir(next.path()).unwrap();
        assert!(
            next.save(&private, &plan, &options, &artifacts, &mut publication)
                .is_none()
        );
        assert!(publication.checkpoint_for(&typed).is_some());
        assert!(publication.checkpoint_for(&private).is_none());
        assert!(publication.slots.ptr_eq(&checkpoint.slots));
        assert!(publication.bindings.ptr_eq(&checkpoint.bindings));
        assert!(publication.plan_slots.ptr_eq(&checkpoint.plan_slots));
        assert_eq!(store.load(&options).unwrap(), ((*typed).clone(), candidate));
        fs::remove_dir(next.path()).unwrap();
        let saved = next
            .save(&private, &plan, &options, &artifacts, &mut publication)
            .unwrap();
        assert_eq!(saved.function_bindings_checked, 1);
        assert_eq!(saved.function_plan_bindings_compared, 0);
        assert_eq!(saved.manifest_writes, 1);
        assert!(publication.checkpoint_for(&private).is_some());
        assert_eq!(next.load(&options).unwrap(), ((*private).clone(), plan));
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn manifest_bounds_precede_reads_and_corruption_is_a_complete_miss() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let limits = Limits::default();
        let (segments, _) = SemanticPublicationSession::default()
            .encode(&typed, &candidate, &limits)
            .unwrap();
        let bindings: Vec<_> = typed
            .module
            .functions
            .iter()
            .map(|f| FunctionBinding {
                function: f.id,
                canonical: false,
                span: f.span,
            })
            .collect();
        let bytes = manifest(&segments, &bindings, &limits).unwrap();
        let assembled = assemble(&bytes, &limits, &mut |key| {
            segments
                .iter()
                .find(|s| s.fingerprint == key)
                .map(|s| s.bytes.to_vec())
        })
        .unwrap();
        assert_eq!(assembled, encode(&typed, &candidate, &limits).unwrap());
        assert_eq!(
            decode(&assembled, &limits).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let mut forged = bytes.clone();
        forged[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut reads = 0;
        assert!(
            assemble(&forged, &limits, &mut |_| {
                reads += 1;
                None
            })
            .is_none()
        );
        assert_eq!(reads, 0);
        let exact_functions = Limits {
            max_functions: typed.module.functions.len(),
            ..limits.clone()
        };
        assert!(layout(&bytes, &exact_functions).is_some());
        let mut wrong_version = bytes.clone();
        wrong_version[..8].copy_from_slice(LEGACY_MANIFEST_MAGIC);
        assert!(layout(&wrong_version, &exact_functions).is_none());
        assert!(
            maximum_segments(&Limits {
                max_functions: usize::MAX,
                ..limits.clone()
            })
            .is_none()
        );
        assert!(
            assemble(&bytes, &limits, &mut |key| {
                let mut data = segments
                    .iter()
                    .find(|s| s.fingerprint == key)?
                    .bytes
                    .to_vec();
                data[0] ^= 1;
                Some(data)
            })
            .is_none()
        );
        let exact = assembled.len();
        for (maximum, accepted) in [(exact, true), (exact - 1, false)] {
            let limits = Limits {
                max_source_bytes: maximum,
                ..Limits::default()
            };
            assert_eq!(
                SemanticPublicationSession::default()
                    .encode(&typed, &candidate, &limits)
                    .is_some(),
                accepted
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn semantic_store_publishes_items_before_manifest_and_repairs_corrupt_items() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            ..Default::default()
        };
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), SOURCE);
        let plan = artifacts::ArtifactPlan::new(&source, &options).unwrap();
        let store = Store {
            root: directory.join("semantic-v6"),
            key: Fingerprint::of(b"request"),
            maximum: options.limits.max_source_bytes,
            segmented: true,
            loaded_publication: Default::default(),
        };
        // The transport version is independent of the original logical format.
        // Old loose-item manifests remain readable before one-time migration.
        let (segments, bindings) = legacy_segments(&typed, &candidate, &options.limits);
        let mut logical = manifest(&segments, &bindings, &options.limits).unwrap();
        logical[..8].copy_from_slice(LEGACY_MANIFEST_MAGIC);
        let loose_root = store.root.join("items");
        assert!(private_directory(&loose_root, true));
        let loose = ArtifactCache::with_limit(&loose_root, store.maximum);
        for segment in &segments {
            loose
                .write_candidate(segment.fingerprint, &segment.bytes)
                .unwrap();
        }
        ArtifactCache::with_limit(&store.root, store.maximum)
            .write_candidate(store.key, &logical)
            .unwrap();
        assert_eq!(
            store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        // A historical packed envelope also carries the original v6 logical
        // manifest independently of current process hints and new segmentation.
        let old_store = Store {
            root: store.root.clone(),
            key: Fingerprint::of(b"historical packed request"),
            maximum: store.maximum,
            segmented: true,
            loaded_publication: Default::default(),
        };
        let mut guarded = plan.clone();
        let pack_root = guarded
            .cache_directory("old-packs", &store.root.join("packs-v1"))
            .unwrap();
        assert!(private_directory(&pack_root, true));
        let namespace = guarded.pin_cache_directory(&pack_root).unwrap();
        packed::publish(
            packed::PublishContext {
                packs: &namespace,
                manifest_root: &store.root,
                namespace_key: &pack_root,
                manifest_key: old_store.key,
                cancellation: &options.cancellation,
                profile: true,
            },
            &logical,
            segments
                .iter()
                .map(|segment| (segment.fingerprint, segment.bytes.clone()))
                .collect(),
            packed::Bounds {
                maximum: store.maximum,
                max_items: options.limits.max_functions + 4,
            },
            &mut packed::Epoch::default(),
        )
        .unwrap();
        assert_eq!(
            old_store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let mut publication = SemanticPublicationSession::default();
        let cold = store
            .save(&typed, &candidate, &options, &plan, &mut publication)
            .unwrap();
        assert_eq!(cold.serialized_functions, 2);
        assert_eq!(cold.serialized_function_plans, 2);
        assert!(cold.written_segments > 0);
        assert_eq!((cold.physical_pack_writes, cold.manifest_writes), (1, 1));
        assert!(
            cold.encode_ns > 0
                && cold.guard_ns > 0
                && cold.item_io_ns > 0
                && cold.manifest_io_ns > 0
        );
        assert_eq!(
            store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let warm = store
            .save(&typed, &candidate, &options, &plan, &mut publication)
            .unwrap();
        assert_eq!(
            (
                warm.serialized_functions,
                warm.reused_functions,
                warm.written_segments
            ),
            (0, 2, 0)
        );
        assert_eq!((warm.physical_pack_writes, warm.manifest_writes), (0, 1));
        assert_eq!(
            (warm.serialized_function_plans, warm.reused_function_plans),
            (0, 2)
        );
        assert_eq!(
            old_store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let mut restored_publication = SemanticPublicationSession::default();
        store.remember_packed_epoch(&options, &mut restored_publication);
        let restored = store
            .save(
                &typed,
                &candidate,
                &options,
                &plan,
                &mut restored_publication,
            )
            .unwrap();
        assert_eq!(
            (restored.physical_pack_writes, restored.written_segments),
            (0, 0)
        );
        assert_eq!(
            restored.existing_segments,
            cold.written_segments + cold.existing_segments
        );
        assert!(publication.checkpoint_for(&typed).is_some());
        assert!(
            publication
                .checkpoint_for(&Arc::new((*typed).clone()))
                .is_none()
        );
        let item_path = publication.semantic_packs.first_pack_path().unwrap();
        let mut bad = fs::read(&item_path).unwrap();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        fs::write(&item_path, bad).unwrap();
        assert!(store.load(&options).is_none());
        let repaired = store
            .save(&typed, &candidate, &options, &plan, &mut publication)
            .unwrap();
        assert_eq!(repaired.written_segments, cold.written_segments);
        assert_eq!(
            (repaired.physical_pack_writes, repaired.manifest_writes),
            (1, 1)
        );
        assert_eq!(
            store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let before = fs::read(store.path()).unwrap();
        options.cancellation.cancel();
        assert!(
            store
                .save(&typed, &candidate, &options, &plan, &mut publication)
                .is_none()
        );
        assert_eq!(before, fs::read(store.path()).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }
    fn canonical_fixture(
        session: &mut CompilationSession,
        path: &Path,
        text: &str,
    ) -> (
        Arc<TypedProgram>,
        Arc<CandidatePlan>,
        Vec<zil_types::FunctionProduct>,
    ) {
        fs::write(path, text).unwrap();
        let report = session.compile_path(
            path,
            &CompileOptions {
                stop_after: Stage::Analyze,
                producer_reuse: true,
                ..Default::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let (key, typed) = session
            .raw
            .iter()
            .find_map(|(key, raw)| match raw {
                crate::session::RawSnapshot::ProjectTyped(typed)
                    if key.phase == Phase::Types
                        && typed.module.source_fingerprint == report.input_fingerprint =>
                {
                    Some((key.clone(), typed.program().clone()))
                }
                _ => None,
            })
            .unwrap();
        let snapshot = session.database.read(&key).unwrap().unwrap();
        let candidate = session.analysis_candidate(&snapshot).unwrap();
        (
            typed,
            candidate,
            session.function_types.last_products().to_vec(),
        )
    }
    const CANONICAL_SOURCE: &str = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn g(x:i32)->i32{if x<10{return x+3;}else{return x*4;}} pub fn main()->i32{let a=f(2);let b=g(a);let c=f(b);return c;}";

    #[test]
    fn equal_plan_rows_reach_packed_storage_without_reencoding_or_dirty_bytes() {
        let directory = directory();
        let path = directory.join("main.zil");
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            ..Default::default()
        };
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            path.display().to_string(),
            CANONICAL_SOURCE,
        );
        let artifacts = artifacts::ArtifactPlan::new(&source, &options).unwrap();
        let mut session = CompilationSession::default();
        let mut publication = SemanticPublicationSession::default();
        let (cold_typed, cold_candidate, products) =
            canonical_fixture(&mut session, &path, CANONICAL_SOURCE);
        publication.bind_products(&products);
        let cold_store = Store {
            root: directory.join("semantic-v6"),
            key: Fingerprint::of(b"cold"),
            maximum: options.limits.max_source_bytes,
            segmented: true,
            loaded_publication: Default::default(),
        };
        let cold = cold_store
            .save(
                &cold_typed,
                &cold_candidate,
                &options,
                &artifacts,
                &mut publication,
            )
            .unwrap();
        assert_eq!(
            (cold.serialized_function_plans, cold.reused_function_plans),
            (3, 0)
        );
        let old_plans = publication.plan_slots.clone();
        let (old_segments, _) = publication
            .encode(&cold_typed, &cold_candidate, &options.limits)
            .unwrap();
        let old_keys: std::collections::BTreeSet<_> =
            old_segments.iter().map(|s| s.fingerprint).collect();

        let text = CANONICAL_SOURCE.replace("x+1", "x+17");
        let (typed, candidate, products) = canonical_fixture(&mut session, &path, &text);
        assert_ne!(
            cold_candidate.module_fingerprint,
            candidate.module_fingerprint
        );
        assert_eq!(cold_candidate.functions, candidate.functions);
        publication.bind_products(&products);
        let private_store = Store {
            key: Fingerprint::of(b"private"),
            root: cold_store.root.clone(),
            maximum: cold_store.maximum,
            segmented: true,
            loaded_publication: Default::default(),
        };
        let private = private_store
            .save(&typed, &candidate, &options, &artifacts, &mut publication)
            .unwrap();
        assert_eq!(
            (private.serialized_functions, private.reused_functions),
            (1, 2)
        );
        assert_eq!(
            (
                private.serialized_function_plans,
                private.reused_function_plans
            ),
            (0, 3)
        );
        assert_eq!(private.serialized_function_plan_bytes, 0);
        assert_eq!(
            private.reused_function_plan_bytes,
            cold.serialized_function_plan_bytes
        );
        assert_eq!(
            (private.physical_pack_writes, private.manifest_writes),
            (1, 1)
        );
        for (old, current) in old_plans.iter().zip(&publication.plan_slots) {
            assert!(Arc::ptr_eq(
                &old.as_ref().unwrap().shared_bytes(),
                &current.as_ref().unwrap().shared_bytes()
            ));
        }
        let (segments, _) = publication
            .encode(&typed, &candidate, &options.limits)
            .unwrap();
        let dirty: BTreeMap<_, _> = segments
            .iter()
            .filter(|segment| !old_keys.contains(&segment.fingerprint))
            .map(|segment| (segment.fingerprint, segment.bytes.len()))
            .collect();
        assert_eq!(private.written_segments, dirty.len());
        assert_eq!(private.written_bytes, dirty.values().sum::<usize>());
        assert_eq!(private.physical_pack_bytes, private.written_bytes + 100);
        // Header/body/metadata change, while every FunctionPlan row stays in an
        // older bounded pack. History remains self-contained after publication.
        assert_eq!(private.written_segments, 4);
        assert_eq!(
            cold_store.load(&options).unwrap(),
            ((*cold_typed).clone(), (*cold_candidate).clone())
        );
        assert_eq!(
            private_store.load(&options).unwrap(),
            ((*typed).clone(), (*candidate).clone())
        );

        let checkpoint = publication.checkpoint_for(&typed).unwrap();
        let mut short = options.limits.clone();
        short.max_source_bytes = flatten(&segments).len() - 1;
        assert!(publication.encode(&typed, &candidate, &short).is_none());
        for (old, current) in checkpoint.plan_slots.iter().zip(&publication.plan_slots) {
            assert!(Arc::ptr_eq(
                &old.as_ref().unwrap().shared_bytes(),
                &current.as_ref().unwrap().shared_bytes()
            ));
        }
        assert!(
            publication
                .encode(&typed, &candidate, &options.limits)
                .is_some()
        );
        assert_eq!(publication.plan_stats.reused_function_plans, 3);

        // Exact row binding prevents old facts from hiding a forged new row;
        // serialization and disk integrity still do not confer Verify authority.
        let mut forged = (*candidate).clone();
        forged.functions[0].function = FunctionId(u32::MAX);
        let (raw, _) = publication
            .encode(&typed, &forged, &options.limits)
            .unwrap();
        assert_eq!(publication.plan_stats.serialized_function_plans, 1);
        let logical = manifest(&raw, &publication.bindings, &options.limits).unwrap();
        let restored = decode_bound(&flatten(&raw), &logical, &options.limits).unwrap();
        assert!(
            zil_verify::verify_module(
                restored.0.module,
                restored.1,
                &typed.module.target,
                &options.limits
            )
            .is_err()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn variable_width_edit_serializes_one_canonical_function_and_rebinds_every_position() {
        let directory = directory();
        let path = directory.join("main.zil");
        let mut session = CompilationSession::default();
        let mut publication = SemanticPublicationSession::default();
        let limits = Limits::default();
        let mut previous_products: Vec<zil_types::FunctionProduct> = vec![];
        for (text, serialized) in [
            (CANONICAL_SOURCE.to_owned(), 3),
            (CANONICAL_SOURCE.replace("x+1", "x+17"), 1),
            (
                format!("\n\n{}", CANONICAL_SOURCE.replace("x+1", "x+17")),
                0,
            ),
        ] {
            let (typed, candidate, mut products) = canonical_fixture(&mut session, &path, &text);
            // A mutable public digest must not be able to force reuse of a
            // changed body. Only the private current-result seal is consulted.
            for product in &mut products {
                if let Some(old) = previous_products.iter().find(|old| old.id == product.id) {
                    product.result_fingerprint = old.result_fingerprint;
                }
            }
            publication.bind_products(&products);
            let (segments, stats) = publication.encode(&typed, &candidate, &limits).unwrap();
            assert_eq!(stats.serialized_functions, serialized);
            assert_eq!(stats.reused_functions, 3 - serialized);
            assert_eq!(
                publication.bindings.iter().filter(|b| b.canonical).count(),
                3
            );
            let manifest = manifest(&segments, &publication.bindings, &limits).unwrap();
            let bytes = assemble(&manifest, &limits, &mut |key| {
                segments
                    .iter()
                    .find(|s| s.fingerprint == key)
                    .map(|s| s.bytes.to_vec())
            })
            .unwrap();
            assert_eq!(
                bytes.len(),
                encode(&typed, &candidate, &limits).unwrap().len()
            );
            let canonical = decode(&bytes, &limits).unwrap();
            assert_ne!(
                canonical.0.module.fingerprint(),
                candidate.module_fingerprint
            );
            let restored = decode_bound(&bytes, &manifest, &limits).unwrap();
            assert_eq!(restored.0, *typed);
            assert_eq!(restored.1, *candidate);
            assert_eq!(
                restored.0.module.fingerprint(),
                candidate.module_fingerprint
            );
            assert!(
                zil_verify::verify_module(
                    restored.0.module.clone(),
                    restored.1.clone(),
                    &typed.module.target,
                    &limits
                )
                .is_ok()
            );
            let mut forged = manifest.clone();
            let (_, _, start) = layout(&forged, &limits).unwrap();
            // Change length while keeping the binding syntactically ordered.
            let end = u64::from_le_bytes(forged[start + 17..start + 25].try_into().unwrap());
            forged[start + 17..start + 25].copy_from_slice(&(end + 1).to_le_bytes());
            assert!(decode_bound(&bytes, &forged, &limits).is_none());
            let exact = bytes.len();
            for maximum in [exact, exact - 1] {
                let limited = Limits {
                    max_source_bytes: maximum,
                    ..limits.clone()
                };
                let mut encoder = SemanticPublicationSession::default();
                encoder.bind_products(&products);
                assert_eq!(
                    encoder.encode(&typed, &candidate, &limited).is_some(),
                    encode(&typed, &candidate, &limited).is_some()
                );
            }
            previous_products = products;
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn stale_products_cannot_retarget_completed_canonical_bytes_or_current_positions() {
        let directory = directory();
        let path = directory.join("main.zil");
        let mut session = CompilationSession::default();
        let (cold, plan, products) = canonical_fixture(&mut session, &path, CANONICAL_SOURCE);
        let mut publication = SemanticPublicationSession::default();
        publication.bind_products(&products);
        publication
            .encode(&cold, &plan, &Limits::default())
            .unwrap();
        let (moved, plan, _) =
            canonical_fixture(&mut session, &path, &format!("\n{CANONICAL_SOURCE}"));
        // Deliberately retain the old proof set: current source positions must
        // never be replaced by old ones, even when relative CFGs compare equal.
        let (segments, stats) = publication
            .encode(&moved, &plan, &Limits::default())
            .unwrap();
        assert_eq!(stats.serialized_functions, 0);
        let logical = manifest(&segments, &publication.bindings, &Limits::default()).unwrap();
        let actual = decode_bound(&flatten(&segments), &logical, &Limits::default()).unwrap();
        assert_eq!(actual, ((*moved).clone(), (*plan).clone()));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn publication_borrows_actual_products_without_building_an_owned_product_table() {
        let directory = directory();
        let path = directory.join("main.zil");
        let mut session = CompilationSession::default();
        let mut publication = SemanticPublicationSession::default();
        let limits = Limits::default();
        for (text, serialized) in [
            (CANONICAL_SOURCE.to_owned(), 3),
            (CANONICAL_SOURCE.replace("x+1", "x+17"), 1),
            (
                format!("\n\n{}", CANONICAL_SOURCE.replace("x+1", "x+17")),
                0,
            ),
        ] {
            let (typed, candidate, products) = canonical_fixture(&mut session, &path, &text);
            let (segments, stats, prepared) = publication
                .prepare_with_products(&typed, &candidate, &limits, false, Some(&products))
                .unwrap();
            publication.install(prepared);
            assert!(publication.products.is_empty());
            assert_eq!(
                (stats.serialized_functions, stats.reused_functions),
                (serialized, 3 - serialized)
            );
            let logical = manifest(&segments, &publication.bindings, &limits).unwrap();
            assert_eq!(
                decode_bound(&flatten(&segments), &logical, &limits).unwrap(),
                ((*typed).clone(), (*candidate).clone())
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn restored_segments_bind_current_values_without_rereading_or_reencoding() {
        let directory = directory();
        let path = directory.join("main.zil");
        let options = CompileOptions {
            push_reuse: true,
            ..Default::default()
        };
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), SOURCE);
        let artifacts = artifacts::ArtifactPlan::new(&source, &options).unwrap();
        let store = Store {
            root: directory.join("semantic-v7-push"),
            key: Fingerprint::of(b"memory-restart"),
            maximum: options.limits.max_source_bytes,
            segmented: true,
            loaded_publication: Default::default(),
        };
        let (typed, candidate) = fixture(&path, SOURCE);
        let mut cold = SemanticPublicationSession::default();
        store
            .save(&typed, &candidate, &options, &artifacts, &mut cold)
            .unwrap();
        let (decoded, claims) = store.load(&options).unwrap();
        let pack = cold.semantic_packs.first_pack_path().unwrap();
        let pack_root = pack.parent().unwrap();
        let held = pack_root.with_extension("held");
        fs::rename(pack_root, &held).unwrap();
        // Neither data packs nor shared locator manifests can be reopened at
        // handoff. Publication still rechecks the complete packs afterward.
        let mut publication = store.take_loaded_publication(&options).unwrap();
        assert!(publication.memory.is_some());
        assert!(publication.previous.is_none());
        let memory = publication.memory.as_ref().unwrap();
        for (index, slot) in memory.slots.iter().enumerate() {
            assert!(Arc::ptr_eq(
                &slot.get().unwrap().shared_bytes(),
                &memory.segments[index + 2].bytes,
            ));
        }
        assert!(memory.plan_slots.iter().all(Option::is_some));
        fs::rename(&held, pack_root).unwrap();
        drop(cold);
        let current = Arc::new(decoded);
        let restored = store
            .save(&current, &claims, &options, &artifacts, &mut publication)
            .unwrap();
        assert_eq!(
            (restored.serialized_functions, restored.reused_functions),
            (0, 2)
        );
        assert_eq!(
            (
                restored.serialized_function_plans,
                restored.reused_function_plans
            ),
            (0, 2)
        );
        assert_eq!(restored.physical_pack_writes, 0);
        assert!(publication.checkpoint_for(&current).is_some());

        for (text, serialized) in [
            (format!("\n\n{SOURCE}"), 0),
            (format!("\n\n{}", SOURCE.replace("x+1", "x+2")), 1),
        ] {
            let (typed, plan) = fixture(&path, &text);
            let stats = store
                .save(&typed, &plan, &options, &artifacts, &mut publication)
                .unwrap();
            assert_eq!(
                (stats.serialized_functions, stats.reused_functions),
                (serialized, 2 - serialized)
            );
            assert_eq!(
                (stats.serialized_function_plans, stats.reused_function_plans),
                (0, 2)
            );
            let (roundtrip, claims) = store.load(&options).unwrap();
            assert_eq!(roundtrip, *typed);
            assert_eq!(claims, plan);
            assert!(
                zil_verify::verify_module(
                    roundtrip.module,
                    claims,
                    &typed.module.target,
                    &options.limits
                )
                .is_ok()
            );
        }
        let checkpoint = publication.clone();
        let before = fs::read(store.path()).unwrap();
        let mut cancelled = options.clone();
        cancelled.cancellation = zil_db::CancellationToken::default();
        cancelled.cancellation.cancel();
        let current = publication.previous.as_ref().unwrap().clone();
        let plan = publication.memory.as_ref().unwrap().candidate.clone();
        assert!(
            store
                .save(&current, &plan, &cancelled, &artifacts, &mut publication)
                .is_none()
        );
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert!(Arc::ptr_eq(
            publication.memory.as_ref().unwrap(),
            checkpoint.memory.as_ref().unwrap()
        ));
        let (segments, _, _) = publication
            .prepare(&current, &plan, &options.limits, true)
            .unwrap();
        // A self-consistent manifest/hash table with shifted segment boundaries
        // is still a raw candidate, but cannot restore any encoding slots.
        let bytes = flatten(&segments);
        let mut shifted = segments.clone();
        let mut header = shifted[1].bytes.to_vec();
        header.push(shifted[2].bytes[0]);
        shifted[1] = wire_segment(header);
        shifted[2] = wire_segment(shifted[2].bytes[1..].to_vec());
        let forged = manifest(&shifted, &publication.bindings, &options.limits).unwrap();
        let (bound, claims, memory) = MemoryPublication::restore(
            &forged,
            shifted,
            &bytes,
            &options.limits,
            &options.cancellation,
        )
        .unwrap();
        assert_eq!(bound, *current);
        assert_eq!(claims, plan);
        assert!(memory.is_none());
        let valid = manifest(&segments, &publication.bindings, &options.limits).unwrap();
        assert!(
            MemoryPublication::restore(
                &valid,
                segments.clone(),
                &bytes,
                &options.limits,
                &cancelled.cancellation,
            )
            .is_none()
        );
        let mut logical = manifest(&segments, &publication.bindings, &options.limits).unwrap();
        logical[8..16].copy_from_slice(&1u64.to_le_bytes());
        let mut read_item = false;
        assert!(
            assemble(&logical, &options.limits, &mut |_| {
                read_item = true;
                None
            })
            .is_none()
        );
        assert!(
            !read_item,
            "aggregate declared lengths must be checked before item I/O"
        );
        assert!(
            MemoryPublication::restore(
                &logical,
                segments,
                &[],
                &options.limits,
                &options.cancellation
            )
            .is_none()
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn canonical_disk_roundtrip_rebinds_imported_files_before_current_verify() {
        let directory = directory();
        let path = directory.join("main.zil");
        let main = "import dep; pub fn main()->i32{let a=dep.f(2);let b=dep.g(a);let c=dep.f(b);return c;}";
        let dependency = "pub fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} pub fn g(x:i32)->i32{if x<10{return x+3;}else{return x*4;}}";
        let mut session = CompilationSession::default();
        let mut publication = SemanticPublicationSession::default();
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            ..Default::default()
        };
        let store = Store {
            root: directory.join("semantic-v6"),
            key: Fingerprint::of(b"canonical-roundtrip"),
            maximum: options.limits.max_source_bytes,
            segmented: true,
            loaded_publication: Default::default(),
        };
        for (source, expected) in [
            (dependency.to_owned(), 3),
            (dependency.replace("x+1", "x+17"), 1),
        ] {
            fs::write(directory.join("dep.zil"), source).unwrap();
            let (typed, candidate, products) = canonical_fixture(&mut session, &path, main);
            publication.bind_products(&products);
            let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), main);
            let plan = artifacts::ArtifactPlan::new(&source, &options).unwrap();
            let stats = store
                .save(&typed, &candidate, &options, &plan, &mut publication)
                .unwrap();
            assert_eq!(stats.serialized_functions, expected);
            assert_eq!(stats.canonical_functions, 3);
            assert_eq!(stats.bound_functions, 0);
            let (restored, claims) = store.load(&options).unwrap();
            assert_eq!(restored, *typed);
            assert_eq!(claims, *candidate);
            assert!(
                restored
                    .module
                    .functions
                    .iter()
                    .any(|f| f.span.file != FileId(0))
            );
            assert_eq!(restored.module.fingerprint(), claims.module_fingerprint);
            assert!(
                zil_verify::verify_module(
                    restored.module,
                    claims,
                    &options.target,
                    &options.limits
                )
                .is_ok()
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
