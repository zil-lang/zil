//! Raw scalar bodies are stored and decoded once. Each logical row retains
//! its complete input selector, actual outer identity and original six fees.
//! A dictionary is transport only: current Types and independent Verify still
//! decide whether any restored raw result is usable.
use super::*;
use zil_ir::wire::{RawFunctionDecodingStats, RawFunctionEncoding};

pub(super) const MAGIC: &[u8; 8] = b"ZILFND02";
const FIXED_ROW: usize = 136;

impl FunctionSession {
    pub fn encode_dictionary_products(
        &self,
        target: &TargetProfile,
        limits: &Limits,
        stats: &mut zil_ir::wire::FunctionEncodingStats,
    ) -> Option<FunctionItemProducts> {
        *stats = zil_ir::wire::FunctionEncodingStats::default();
        let _publication = self
            .publication_guard
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let count = self.entries.len();
        if count == 0 || count > limits.max_functions {
            return None;
        }
        let mut remaining_nodes = limits.max_nodes.checked_sub(count)?;
        let mut logical_bytes = legacy_bytes(count, target, limits)?;
        let mut retained = self.entries.values().fold(0usize, |n, entry| {
            n.saturating_add(entry.encoding.get().map_or(0, |e| e.byte_len()))
        });
        let mut manifest = Vec::new();
        manifest.extend_from_slice(MAGIC);
        manifest.extend_from_slice(&target.fingerprint().0);
        manifest.extend_from_slice(&u32::try_from(count).ok()?.to_le_bytes());
        let mut bodies: BTreeMap<Fingerprint, FunctionItem> = BTreeMap::new();
        for (input_key, entry) in self.entries.iter() {
            let encoding = if let Some(raw) = entry
                .encoding
                .get()
                .filter(|raw| raw.matches_limits(limits))
            {
                stats.reused_functions += 1;
                raw.clone()
            } else {
                stats.serialized_functions += 1;
                let raw =
                    zil_ir::wire::encode_raw_function(entry.canonical()?.as_ref(), limits).ok()?;
                let length = raw.byte_len();
                if entry.encoding.get().is_none()
                    && retained
                        .checked_add(length)
                        .is_some_and(|n| n <= limits.max_source_bytes)
                    && entry.encoding.set(raw.clone()).is_ok()
                {
                    retained += length;
                }
                raw
            };
            let length = encoding.byte_len();
            remaining_nodes = remaining_nodes.checked_sub(encoding.node_cost())?;
            logical_bytes = logical_bytes
                .checked_add(length)
                .filter(|n| *n <= limits.max_source_bytes)?;
            let (id, name, key, body) = encoding.dictionary_fragment()?;
            if let Some(previous) = bodies.get(&key) {
                if !Arc::ptr_eq(&previous.bytes, &body) && previous.bytes != body {
                    return None;
                }
            } else {
                bodies.insert(
                    key,
                    FunctionItem {
                        input_key: *input_key,
                        cost: entry.cost,
                        wire_nodes: encoding.node_cost(),
                        content_fingerprint: key,
                        bytes: body,
                    },
                );
            }
            let length = manifest
                .len()
                .checked_add(FIXED_ROW)?
                .checked_add(name.len())?;
            if length > limits.max_source_bytes {
                return None;
            }
            manifest.extend_from_slice(&input_key.0);
            for fee in entry.cost {
                manifest.extend_from_slice(&u64::try_from(fee).ok()?.to_le_bytes());
            }
            manifest.extend_from_slice(&u64::try_from(encoding.node_cost()).ok()?.to_le_bytes());
            manifest.extend_from_slice(&key.0);
            manifest.extend_from_slice(&u64::try_from(encoding.byte_len()).ok()?.to_le_bytes());
            manifest.extend_from_slice(&id.0.to_le_bytes());
            manifest.extend_from_slice(&u32::try_from(name.len()).ok()?.to_le_bytes());
            manifest.extend_from_slice(name.as_bytes());
        }
        Some(FunctionItemProducts {
            manifest,
            items: bodies.into_values().collect(),
        })
    }
}

struct Row<'a> {
    key: Fingerprint,
    cost: [usize; 6],
    nodes: usize,
    body: Fingerprint,
    bytes: usize,
    id: FunctionId,
    name: &'a str,
}

pub(super) fn decode(
    manifest: &[u8],
    load: &mut dyn FnMut(Fingerprint) -> Option<Arc<[u8]>>,
    target: &TargetProfile,
    limits: &Limits,
    stats: &mut RawFunctionDecodingStats,
) -> Option<BTreeMap<Fingerprint, Arc<Entry>>> {
    if manifest.len() > limits.max_source_bytes
        || manifest.get(..8)? != MAGIC
        || manifest.get(8..40)? != target.fingerprint().0
    {
        return None;
    }
    let count = u32::from_le_bytes(manifest.get(40..44)?.try_into().ok()?) as usize;
    if count == 0
        || count > limits.max_functions
        || HEADER.checked_add(count.checked_mul(FIXED_ROW)?)? > manifest.len()
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
    let mut position = HEADER;
    let mut nodes = limits.max_nodes.checked_sub(count)?;
    let mut bytes = legacy_bytes(count, target, limits)?;
    let mut prior = None;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let fixed = manifest.get(position..position.checked_add(FIXED_ROW)?)?;
        let key = Fingerprint(fixed[..32].try_into().ok()?);
        if prior.is_some_and(|old| old >= key) {
            return None;
        }
        prior = Some(key);
        let mut cost = [0; 6];
        for (index, maximum) in maxima.iter().enumerate() {
            cost[index] = u64_at(fixed, 32 + 8 * index)?;
            if cost[index] > *maximum {
                return None;
            }
        }
        let used = u64_at(fixed, 80)?;
        nodes = nodes.checked_sub(used)?;
        let logical_bytes = u64_at(fixed, 120)?;
        bytes = bytes
            .checked_add(logical_bytes)
            .filter(|n| *n <= limits.max_source_bytes)?;
        let id = FunctionId(u32::from_le_bytes(fixed[128..132].try_into().ok()?));
        let length = u32::from_le_bytes(fixed[132..136].try_into().ok()?) as usize;
        position = position.checked_add(FIXED_ROW)?;
        let name =
            std::str::from_utf8(manifest.get(position..position.checked_add(length)?)?).ok()?;
        position = position.checked_add(length)?;
        rows.push(Row {
            key,
            cost,
            nodes: used,
            body: Fingerprint(fixed[88..120].try_into().ok()?),
            bytes: logical_bytes,
            id,
            name,
        });
    }
    if position != manifest.len() {
        return None;
    }
    let mut decoded = BTreeMap::<Fingerprint, (RawFunctionEncoding, FingerprintBuilder)>::new();
    let mut entries = BTreeMap::new();
    let mut remaining = limits.max_nodes.checked_sub(count)?;
    for row in rows {
        if let std::collections::btree_map::Entry::Vacant(slot) = decoded.entry(row.body) {
            let data = load(row.body)?;
            if data.len().checked_add(row.name.len())? != row.bytes
                || Fingerprint::of(&data) != row.body
            {
                return None;
            }
            stats.decode_attempts += 1;
            let (function, raw) =
                zil_ir::wire::decode_raw_function_shared_owner(data, limits, remaining).ok()?;
            if function.id != FunctionId(0)
                || !function.name.is_empty()
                || !scalar_typed(&function)
                || function.span.file != FileId(0)
                || function.span.start != 0
            {
                return None;
            }
            let projection = scalar_result_body(&function, function.span)?;
            slot.insert((raw, projection));
            stats.decoded_functions += 1;
        } else {
            stats.reused_decoded_functions += 1;
            stats.reused_result_projections += 1;
        }
        let (raw, projection) = decoded.get(&row.body)?;
        if raw.node_cost() != row.nodes || raw.byte_len().checked_add(row.name.len())? != row.bytes
        {
            return None;
        }
        let (function, encoding) =
            raw.rebind_decoded_scalar_identity(row.id, row.name, limits, remaining)?;
        remaining = remaining.checked_sub(row.nodes)?;
        entries.insert(
            row.key,
            Arc::new(Entry {
                restored: true,
                result_fingerprint: scalar_result_finish(projection.clone(), &function),
                origin: function.span,
                canonical: OnceLock::from(Some(function.clone())),
                function,
                last_bound: Mutex::new(None),
                cost: row.cost,
                encoding: OnceLock::from(encoding),
                encoding_fingerprint: OnceLock::new(),
            }),
        );
    }
    Some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn siblings() -> FunctionSession {
        let limits = Limits::default();
        let mut products = super::super::tests::products(&super::super::tests::session(), &limits);
        let (first, _) =
            zil_ir::wire::decode_raw_function(&products.items[0].bytes, &limits).unwrap();
        let (mut second, _) =
            zil_ir::wire::decode_raw_function(&products.items[1].bytes, &limits).unwrap();
        let id = second.id;
        second = first;
        second.id = id;
        second.name = "different-label".into();
        let raw = zil_ir::wire::encode_raw_function(&second, &limits).unwrap();
        let item = &mut products.items[1];
        item.bytes = raw.shared_bytes();
        item.content_fingerprint = raw.content_fingerprint();
        item.wire_nodes = raw.node_cost();
        let offset = HEADER + ROW;
        products.manifest[offset + 80..offset + 88]
            .copy_from_slice(&(item.wire_nodes as u64).to_le_bytes());
        products.manifest[offset + 88..offset + 120].copy_from_slice(&item.content_fingerprint.0);
        products.manifest[offset + 120..offset + 128]
            .copy_from_slice(&(item.bytes.len() as u64).to_le_bytes());
        let mut session = FunctionSession::default();
        assert!(session.restore_item_products(
            &products.manifest,
            &mut |key| super::super::tests::loader(&products.items, key),
            &TargetProfile::default(),
            &limits
        ));
        session
    }

    #[test]
    fn dictionary_restores_each_body_once_and_republishes_original_rows() {
        let session = siblings();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let products = session
            .encode_dictionary_products(&target, &limits, &mut Default::default())
            .unwrap();
        assert_eq!(products.items.len(), 1);
        let mut restored = FunctionSession::default();
        let mut stats = RawFunctionDecodingStats::default();
        let mut reads = 0;
        assert!(restored.restore_item_products_with_decoded(
            &products.manifest,
            &mut |key| {
                reads += 1;
                super::super::tests::loader(&products.items, key)
            },
            &mut |_| panic!("dictionary rows do not need a complete per-function byte hint"),
            &target,
            &limits,
            &mut stats
        ));
        assert_eq!(
            (
                reads,
                stats.decoded_functions,
                stats.reused_decoded_functions
            ),
            (1, 1, 1)
        );
        for (key, old) in session.entries.iter() {
            let current = &restored.entries[key];
            assert_eq!(old.function, current.function);
            assert_eq!(old.result_fingerprint, current.result_fingerprint);
            assert_eq!(old.cost, current.cost);
        }
        let next = restored
            .encode_dictionary_products(&target, &limits, &mut Default::default())
            .unwrap();
        assert_eq!(products.manifest, next.manifest);
        assert!(Arc::ptr_eq(&products.items[0].bytes, &next.items[0].bytes));
    }

    #[test]
    fn dictionary_keeps_logical_quotas_and_rejects_corrupt_rows_atomically() {
        let session = siblings();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let products = session
            .encode_dictionary_products(&target, &limits, &mut Default::default())
            .unwrap();
        let original = session
            .encode_item_products(&target, &limits, &mut Default::default())
            .unwrap();
        let nodes = original.items.len()
            + original
                .items
                .iter()
                .map(|item| item.wire_nodes)
                .sum::<usize>();
        let bytes = legacy_bytes(original.items.len(), &target, &limits).unwrap()
            + original
                .items
                .iter()
                .map(|item| item.bytes.len())
                .sum::<usize>();
        for tight in [
            Limits {
                max_nodes: nodes - 1,
                ..limits.clone()
            },
            Limits {
                max_source_bytes: bytes - 1,
                ..limits.clone()
            },
        ] {
            let mut restored = siblings();
            let previous = restored.entries.clone();
            assert!(!restored.restore_item_products(
                &products.manifest,
                &mut |_| panic!("logical aggregate preflight must precede load"),
                &target,
                &tight
            ));
            assert!(Arc::ptr_eq(&previous, &restored.entries));
        }
        for offset in [8, HEADER + 80, HEADER + 88, HEADER + 120, HEADER + 132] {
            let mut bad = products.manifest.clone();
            bad[offset] ^= 0x80;
            let mut restored = siblings();
            let previous = restored.entries.clone();
            assert!(!restored.restore_item_products(
                &bad,
                &mut |key| super::super::tests::loader(&products.items, key),
                &target,
                &limits
            ));
            assert!(Arc::ptr_eq(&previous, &restored.entries));
        }
    }
}
