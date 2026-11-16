//! One bounded function-manifest parse and shared raw decoded owners. The
//! current FunctionSession still checks the complete input/context and replays
//! all original fees; these records confer no Types or Verify authority.
use super::*;
use zil_ir::wire::{RawFunctionDecodingStats, RawFunctionEncoding};

struct ItemDescriptor {
    key: Fingerprint,
    cost: [usize; 6],
    nodes: usize,
    checksum: Fingerprint,
    bytes: usize,
}

impl FunctionSession {
    /// A hint selects a candidate encoding only. Its private decoder owner must
    /// match the complete item bytes (apart from the actual diagnostic name),
    /// complete Limits and remaining aggregate node allowance before reuse.
    /// Missing/incompatible hints run the same original decoder once.
    pub fn restore_item_products_with_decoded(
        &mut self,
        manifest: &[u8],
        load: &mut dyn FnMut(Fingerprint) -> Option<Arc<[u8]>>,
        hint: &mut dyn FnMut(&[u8]) -> Option<RawFunctionEncoding>,
        target: &TargetProfile,
        limits: &Limits,
        stats: &mut RawFunctionDecodingStats,
    ) -> bool {
        *stats = RawFunctionDecodingStats::default();
        let Some(entries) = decode_items(manifest, load, hint, target, limits, stats) else {
            return false;
        };
        self.entries = Arc::new(entries);
        self.projections = Projections::default();
        self.producer_memo = ProducerMemo::default();
        self.products = Arc::default();
        self.changes = FunctionChanges::default();
        self.stats = FunctionStats::default();
        true
    }
}

fn descriptors(
    manifest: &[u8],
    target: &TargetProfile,
    limits: &Limits,
) -> Option<Vec<ItemDescriptor>> {
    if manifest.len() > limits.max_source_bytes
        || manifest.get(..8)? != MAGIC
        || manifest.get(8..40)? != target.fingerprint().0
    {
        return None;
    }
    let count = u32::from_le_bytes(manifest.get(40..44)?.try_into().ok()?) as usize;
    if count == 0
        || count > limits.max_functions
        || HEADER.checked_add(count.checked_mul(ROW)?)? != manifest.len()
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
    let mut nodes = limits.max_nodes.checked_sub(count)?;
    let mut bytes = legacy_bytes(count, target, limits)?;
    let mut prior_key = None;
    let mut result = Vec::with_capacity(count);
    // The complete descriptor table is parsed and preflighted once before any
    // item load/hint. Neither declared counts nor checksums grant more budget.
    for row in manifest[HEADER..].as_chunks::<ROW>().0 {
        let key = Fingerprint(row[..32].try_into().ok()?);
        if prior_key.is_some_and(|prior| prior >= key) {
            return None;
        }
        prior_key = Some(key);
        let mut cost = [0; 6];
        for (i, maximum) in maxima.iter().enumerate() {
            cost[i] = u64_at(row, 32 + i * 8)?;
            if cost[i] > *maximum {
                return None;
            }
        }
        let item_nodes = u64_at(row, 80)?;
        nodes = nodes.checked_sub(item_nodes)?;
        let item_bytes = u64_at(row, 120)?;
        bytes = bytes
            .checked_add(item_bytes)
            .filter(|n| *n <= limits.max_source_bytes)?;
        result.push(ItemDescriptor {
            key,
            cost,
            nodes: item_nodes,
            checksum: Fingerprint(row[88..120].try_into().ok()?),
            bytes: item_bytes,
        });
    }
    Some(result)
}

fn decode_items(
    manifest: &[u8],
    load: &mut dyn FnMut(Fingerprint) -> Option<Arc<[u8]>>,
    hint: &mut dyn FnMut(&[u8]) -> Option<RawFunctionEncoding>,
    target: &TargetProfile,
    limits: &Limits,
    stats: &mut RawFunctionDecodingStats,
) -> Option<BTreeMap<Fingerprint, Arc<Entry>>> {
    let descriptors = descriptors(manifest, target, limits)?;
    let mut remaining_nodes = limits.max_nodes.checked_sub(descriptors.len())?;
    let mut entries = BTreeMap::new();
    for item in descriptors {
        let data = load(item.checksum)?;
        if data.len() != item.bytes || Fingerprint::of(&data) != item.checksum {
            return None;
        }
        let hinted = hint(&data);
        let reused = hinted.as_ref().and_then(|encoding| {
            encoding.reuse_decoded_function(data.clone(), limits, remaining_nodes)
        });
        let (function, raw_encoding) = if let Some((function, encoding)) = reused {
            stats.reused_decoded_functions += 1;
            if hinted.as_ref()?.decoded_function()?.name != function.name {
                stats.relabeled_decoded_functions += 1;
            }
            (function, encoding)
        } else {
            stats.decode_attempts += 1;
            let decoded =
                zil_ir::wire::decode_raw_function_shared_owner(data, limits, remaining_nodes)
                    .ok()?;
            stats.decoded_functions += 1;
            decoded
        };
        let used_nodes = raw_encoding.node_cost();
        if used_nodes != item.nodes
            || !scalar_typed(&function)
            || function.span.file != FileId(0)
            || function.span.start != 0
        {
            return None;
        }
        remaining_nodes = remaining_nodes.checked_sub(used_nodes)?;
        entries.insert(
            item.key,
            Arc::new(Entry {
                restored: true,
                result_fingerprint: scalar_result_fingerprint(&function, function.span)?,
                origin: function.span,
                canonical: OnceLock::from(Some(function.clone())),
                function,
                last_bound: Mutex::new(None),
                cost: item.cost,
                encoding: OnceLock::from(raw_encoding),
                encoding_fingerprint: OnceLock::from(item.checksum),
            }),
        );
    }
    Some(entries)
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
