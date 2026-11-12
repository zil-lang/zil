//! Function-item raw persistence. A manifest is committed only after every item
//! validates against the original whole-epoch quotas; nothing grants authority.
use super::*;

const MAGIC: &[u8; 8] = b"ZILFNI01";
const HEADER: usize = 44;
const ROW: usize = 128;

#[derive(Clone, Debug)]
pub struct FunctionItem {
    pub input_key: Fingerprint,
    pub cost: [usize; 6],
    pub wire_nodes: usize,
    pub content_fingerprint: Fingerprint,
    pub bytes: Arc<[u8]>,
}
#[derive(Clone, Debug)]
pub struct FunctionItemProducts {
    pub manifest: Vec<u8>,
    pub items: Vec<FunctionItem>,
}

fn empty_wire(target: &TargetProfile, limits: &Limits) -> Option<Vec<u8>> {
    let module = TypedModule {
        life_declarations: vec![],
        life_instances: vec![],
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: target.clone(),
        functions: vec![],
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
    zil_ir::wire::encode_module_candidate(&module, &candidate, limits).ok()
}

fn legacy_bytes(count: usize, target: &TargetProfile, limits: &Limits) -> Option<usize> {
    // Keep the original ZILFUN01 aggregate byte quota. Function item storage
    // does not turn each independently bounded fragment into a fresh budget.
    12usize
        .checked_add(count.checked_mul(80)?)?
        .checked_add(empty_wire(target, limits)?.len())
}

impl FunctionSession {
    /// Immutable item buffers are shared with the existing fragment slots.
    /// Only changed items need writing; the manifest is small and binds their
    /// exact ordered input keys, costs, byte identities and current target.
    pub fn encode_item_products(
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
        let mut nodes = limits.max_nodes.checked_sub(count)?;
        let mut bytes = legacy_bytes(count, target, limits)?;
        if bytes > limits.max_source_bytes {
            return None;
        }
        let retained_bytes = self.entries.values().fold(0usize, |total, entry| {
            total.saturating_add(entry.encoding.get().map_or(0, |e| e.shared_bytes().len()))
        });
        let mut cache_bytes = limits.max_source_bytes.saturating_sub(retained_bytes);
        let mut items = Vec::with_capacity(count);
        for (key, entry) in self.entries.iter() {
            let (data, wire_nodes, content_fingerprint) = if let Some(encoding) =
                entry.encoding.get().filter(|e| e.matches_limits(limits))
            {
                stats.reused_functions += 1;
                let data = encoding.shared_bytes();
                let fingerprint = *entry
                    .encoding_fingerprint
                    .get_or_init(|| Fingerprint::of(&data));
                (data, encoding.node_cost(), fingerprint)
            } else {
                stats.serialized_functions += 1;
                let encoding =
                    zil_ir::wire::encode_raw_function(entry.canonical()?.as_ref(), limits).ok()?;
                let data = encoding.shared_bytes();
                let wire_nodes = encoding.node_cost();
                let fingerprint = Fingerprint::of(&data);
                if entry.encoding.get().is_none()
                    && data.len() <= cache_bytes
                    && entry.encoding.set(encoding).is_ok()
                {
                    cache_bytes -= data.len();
                    let _ = entry.encoding_fingerprint.set(fingerprint);
                }
                (data, wire_nodes, fingerprint)
            };
            nodes = nodes.checked_sub(wire_nodes)?;
            bytes = bytes
                .checked_add(data.len())
                .filter(|n| *n <= limits.max_source_bytes)?;
            items.push(FunctionItem {
                input_key: *key,
                cost: entry.cost,
                wire_nodes,
                content_fingerprint,
                bytes: data,
            });
        }
        let size = HEADER.checked_add(count.checked_mul(ROW)?)?;
        if size > limits.max_source_bytes {
            return None;
        }
        let mut manifest = Vec::with_capacity(size);
        manifest.extend_from_slice(MAGIC);
        manifest.extend_from_slice(&target.fingerprint().0);
        manifest.extend_from_slice(&u32::try_from(count).ok()?.to_le_bytes());
        for item in &items {
            manifest.extend_from_slice(&item.input_key.0);
            for cost in item.cost {
                manifest.extend_from_slice(&u64::try_from(cost).ok()?.to_le_bytes());
            }
            manifest.extend_from_slice(&u64::try_from(item.wire_nodes).ok()?.to_le_bytes());
            manifest.extend_from_slice(&item.content_fingerprint.0);
            manifest.extend_from_slice(&u64::try_from(item.bytes.len()).ok()?.to_le_bytes());
        }
        debug_assert_eq!(manifest.len(), size);
        Some(FunctionItemProducts { manifest, items })
    }

    /// Load and validate all raw items into a private epoch before replacing any
    /// state. Missing/truncated/corrupt items and aggregate quota violations are
    /// complete misses, with the prior session left untouched.
    pub fn restore_item_products(
        &mut self,
        manifest: &[u8],
        load: &mut dyn FnMut(Fingerprint) -> Option<Arc<[u8]>>,
        target: &TargetProfile,
        limits: &Limits,
    ) -> bool {
        let Some(entries) = decode_items(manifest, load, target, limits) else {
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

fn u64_at(bytes: &[u8], start: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(
        bytes.get(start..start.checked_add(8)?)?.try_into().ok()?,
    ))
    .ok()
}
fn decode_items(
    manifest: &[u8],
    load: &mut dyn FnMut(Fingerprint) -> Option<Arc<[u8]>>,
    target: &TargetProfile,
    limits: &Limits,
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
    // Validate the entire small descriptor table before opening raw items.
    for row in manifest[HEADER..].as_chunks::<ROW>().0 {
        let key = Fingerprint(row[..32].try_into().ok()?);
        if prior_key.is_some_and(|prior| prior >= key) {
            return None;
        }
        prior_key = Some(key);
        for (i, maximum) in maxima.iter().enumerate() {
            if u64_at(row, 32 + i * 8)? > *maximum {
                return None;
            }
        }
        nodes = nodes.checked_sub(u64_at(row, 80)?)?;
        bytes = bytes
            .checked_add(u64_at(row, 120)?)
            .filter(|n| *n <= limits.max_source_bytes)?;
    }
    // The decoder computes actual node cost independently. Descriptor values
    // never grant extra allowance or authenticate their own raw bytes.
    let mut remaining_nodes = limits.max_nodes.checked_sub(count)?;
    let mut entries = BTreeMap::new();
    for row in manifest[HEADER..].as_chunks::<ROW>().0 {
        let key = Fingerprint(row[..32].try_into().ok()?);
        let checksum = Fingerprint(row[88..120].try_into().ok()?);
        let data = load(checksum)?;
        if data.len() != u64_at(row, 120)? || Fingerprint::of(&data) != checksum {
            return None;
        }
        let (function, raw_encoding) =
            zil_ir::wire::decode_raw_function_shared(data, limits, remaining_nodes).ok()?;
        let used_nodes = raw_encoding.node_cost();
        if used_nodes != u64_at(row, 80)?
            || !scalar_typed(&function)
            || function.span.file != FileId(0)
            || function.span.start != 0
        {
            return None;
        }
        remaining_nodes = remaining_nodes.checked_sub(used_nodes)?;
        let mut cost = [0; 6];
        for (i, value) in cost.iter_mut().enumerate() {
            *value = u64_at(row, 32 + i * 8)?;
        }
        let encoding = OnceLock::new();
        let _ = encoding.set(raw_encoding);
        let encoding_fingerprint = OnceLock::new();
        let _ = encoding_fingerprint.set(checksum);
        entries.insert(
            key,
            Arc::new(Entry {
                result_fingerprint: scalar_result_fingerprint(&function, function.span)?,
                origin: function.span,
                function: Arc::new(function),
                last_bound: Mutex::new(None),
                canonical: OnceLock::new(),
                cost,
                encoding,
                encoding_fingerprint,
            }),
        );
    }
    Some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_syntax::{ParsedSource, attributes::AttributeContext};

    fn prepared(change: u32) -> PreparedProgram {
        let text = format!(
            "fn f(x:i32)->i32{{if x<10{{return x+{change};}}else{{return x*2;}}}} fn g(x:i32)->i32{{if x<10{{return x+3;}}else{{return x*4;}}}}"
        );
        let parsed = ParsedSource::parse(
            Arc::new(zil_base::SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                text,
            )),
            &Limits::default(),
            &AttributeContext::default(),
        );
        assert!(parsed.parsed().is_ok());
        let aliases = BTreeMap::new();
        let produced = zil_expand::ExpandSession::default()
            .expand_parsed(
                &[parsed],
                &[vec![]],
                &Limits::default(),
                &TargetProfile::default(),
                SourceIdentityContext::ClosedSnapshot {
                    root: ModuleId(0),
                    aliases: &aliases,
                    declarations: DeclarationPlacement::Original,
                },
            )
            .unwrap()
            .unwrap()
            .into_program(ModuleId(0), Fingerprint::ZERO)
            .0;
        infer_produced_program_for_ct(&produced, &Limits::default())
            .unwrap()
            .into_prepared()
    }
    fn session() -> FunctionSession {
        let mut session = FunctionSession::default();
        session
            .check_prepared_program(&prepared(1), &TargetProfile::default(), &Limits::default())
            .unwrap();
        session
    }
    fn products(session: &FunctionSession, limits: &Limits) -> FunctionItemProducts {
        session
            .encode_item_products(
                &TargetProfile::default(),
                limits,
                &mut zil_ir::wire::FunctionEncodingStats::default(),
            )
            .unwrap()
    }
    fn loader(items: &[FunctionItem], key: Fingerprint) -> Option<Arc<[u8]>> {
        items
            .iter()
            .find(|item| item.content_fingerprint == key)
            .map(|item| item.bytes.clone())
    }

    #[test]
    fn warm_item_publication_and_restore_share_existing_buffers() {
        let mut session = session();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let mut stats = zil_ir::wire::FunctionEncodingStats::default();
        let cold = session
            .encode_item_products(&target, &limits, &mut stats)
            .unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (2, 0));
        let old_wire = session.encode_work_products(&target, &limits).unwrap();
        let warm = session
            .encode_item_products(&target, &limits, &mut stats)
            .unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (0, 2));
        assert_eq!(cold.manifest, warm.manifest);
        for (a, b) in cold.items.iter().zip(&warm.items) {
            assert!(Arc::ptr_eq(&a.bytes, &b.bytes));
        }
        assert!(session.restore_item_products(
            &cold.manifest,
            &mut |key| loader(&cold.items, key),
            &target,
            &limits
        ));
        let restored = session
            .encode_item_products(&target, &limits, &mut stats)
            .unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (0, 2));
        for (a, b) in cold.items.iter().zip(&restored.items) {
            assert!(Arc::ptr_eq(&a.bytes, &b.bytes));
        }
        assert_eq!(
            session.encode_work_products(&target, &limits).unwrap(),
            old_wire
        );
        assert_eq!(
            session
                .check_prepared_program(&prepared(1), &target, &limits)
                .unwrap(),
            check_program(prepared(1).program(), &target, &limits).unwrap()
        );
    }

    #[test]
    fn a_private_edit_changes_one_item_and_retains_the_other_allocation() {
        let mut session = session();
        let limits = Limits::default();
        let before = products(&session, &limits);
        session
            .check_prepared_program(&prepared(2), &TargetProfile::default(), &limits)
            .unwrap();
        let mut stats = zil_ir::wire::FunctionEncodingStats::default();
        let after = session
            .encode_item_products(&TargetProfile::default(), &limits, &mut stats)
            .unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (1, 1));
        let shared = after
            .items
            .iter()
            .filter(|item| {
                before.items.iter().any(|old| {
                    old.content_fingerprint == item.content_fingerprint
                        && Arc::ptr_eq(&old.bytes, &item.bytes)
                })
            })
            .count();
        assert_eq!(shared, 1);
    }

    #[test]
    fn aggregate_byte_and_node_boundaries_match_legacy_publication() {
        let session = session();
        let target = TargetProfile::default();
        let item_products = products(&session, &Limits::default());
        let exact_bytes = legacy_bytes(item_products.items.len(), &target, &Limits::default())
            .unwrap()
            + item_products
                .items
                .iter()
                .map(|item| item.bytes.len())
                .sum::<usize>();
        let exact_nodes = item_products.items.len()
            + item_products
                .items
                .iter()
                .map(|item| item.wire_nodes)
                .sum::<usize>();
        for (bytes, nodes, accepted) in [
            (exact_bytes, exact_nodes, true),
            (exact_bytes - 1, exact_nodes, false),
            (exact_bytes, exact_nodes - 1, false),
        ] {
            let limits = Limits {
                max_source_bytes: bytes,
                max_nodes: nodes,
                ..Limits::default()
            };
            let actual = session.encode_item_products(
                &target,
                &limits,
                &mut zil_ir::wire::FunctionEncodingStats::default(),
            );
            assert_eq!(actual.is_some(), accepted);
            assert_eq!(
                session.encode_work_products(&target, &limits).is_some(),
                accepted
            );
            let mut restored = FunctionSession::default();
            assert_eq!(
                restored.restore_item_products(
                    &item_products.manifest,
                    &mut |key| loader(&item_products.items, key),
                    &target,
                    &limits
                ),
                accepted
            );
        }
    }

    #[test]
    fn corrupt_or_forged_item_manifest_never_partially_restores() {
        let mut session = session();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let products = products(&session, &limits);
        let previous = session.entries.clone();
        let mut bad = products.manifest.clone();
        let nodes = u64_at(&bad, HEADER + 80).unwrap();
        bad[HEADER + 80..HEADER + 88].copy_from_slice(&((nodes - 1) as u64).to_le_bytes());
        assert!(!session.restore_item_products(
            &bad,
            &mut |key| loader(&products.items, key),
            &target,
            &limits
        ));
        assert!(Arc::ptr_eq(&previous, &session.entries));
        assert!(!session.restore_item_products(
            &products.manifest,
            &mut |key| {
                let mut bytes = loader(&products.items, key)?.to_vec();
                bytes[0] ^= 1;
                Some(bytes.into())
            },
            &target,
            &limits
        ));
        assert!(Arc::ptr_eq(&previous, &session.entries));
        let mut calls = 0;
        let wrong_target = TargetProfile {
            pointer_width: 16,
            ..target
        };
        assert!(!session.restore_item_products(
            &products.manifest,
            &mut |_| {
                calls += 1;
                None
            },
            &wrong_target,
            &limits
        ));
        assert_eq!(calls, 0);
        assert!(Arc::ptr_eq(&previous, &session.entries));
    }
}
