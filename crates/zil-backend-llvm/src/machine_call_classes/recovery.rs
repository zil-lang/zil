//! Recover normalized products against the actual CURRENT machine. No old
//! input body, serialized dependency authority or second input model is kept.
use super::*;
use crate::machine_cache::{Decoder, Encoder};
use std::sync::Arc;
#[cfg(test)]
use zil_base::Limits;

#[cfg(test)]
const MAGIC: &[u8; 8] = b"ZILCLS02";

struct Row {
    callees: Vec<(usize, String)>,
    function: crate::machine_cache::FunctionRef,
    selector: zil_base::Fingerprint,
    decoded: Option<Option<Arc<Normalized>>>,
}

/// Index old values without importing an old analysis epoch. The current pass
/// discovers edges first; only a reached node with unchanged callee values can
/// request its normalized body. Current observations and class equality remain
/// the authority, including when old metadata or a selector is corrupt.
pub(crate) struct Candidates<'a, 'd> {
    decoder: &'d mut Decoder<'a>,
    rows: Vec<Row>,
    aliases: Vec<usize>,
    pub(crate) checked: usize,
}
impl<'a, 'd> Candidates<'a, 'd> {
    pub(crate) fn read(decoder: &'d mut Decoder<'a>, current: &Module) -> Option<Self> {
        let views = current
            .items
            .iter()
            .map(|item| {
                let name = match item {
                    Item::Function(f) => f.name.as_str(),
                    Item::Global(g) => g.name.as_str(),
                    Item::NamedType(name, _) => name.as_str(),
                };
                (name, item)
            })
            .collect::<Vec<_>>();
        Self::read_views(decoder, &current.triple, &views)
    }
    pub(crate) fn read_aliases(
        decoder: &'d mut Decoder<'a>,
        current: &crate::machine_output::AliasModule,
    ) -> Option<Self> {
        Self::read_views(decoder, &current.triple, &current.views())
    }
    fn read_views(
        decoder: &'d mut Decoder<'a>,
        triple: &str,
        views: &[(&str, &Item)],
    ) -> Option<Self> {
        if decoder.input.text()? != triple {
            return None;
        }
        let functions = views
            .iter()
            .filter_map(|(name, item)| match item {
                Item::Function(function) => Some((*name, function)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let count = decoder.input.count()?;
        if count != functions.len() || count > 32_768 {
            return None;
        }
        let mut names = HashMap::new();
        for (index, (name, function)) in functions.iter().enumerate() {
            let header = decoder.function()?;
            let defined = match decoder.input.word()? {
                0 => false,
                1 => true,
                _ => return None,
            };
            if header.blocks.is_some()
                || header.name != *name
                || !same_class_header(&header, function)
                || defined != function.blocks.is_some()
                || names.insert(*name, index).is_some()
            {
                return None;
            }
        }
        let n = decoder.input.count()?;
        if n > count {
            return None;
        }
        let mut rows = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let n = decoder.input.count()?;
            let mut callees = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let id = decoder.input.word()?;
                if id >= count || callees.last().is_some_and(|(old, _)| *old >= id) {
                    return None;
                }
                callees.push((id, decoder.input.text()?));
            }
            rows.push(Row {
                callees,
                function: decoder.function_ref()?,
                selector: zil_base::Fingerprint(decoder.input.take(32)?.try_into().ok()?),
                decoded: None,
            });
        }
        let mut aliases = Vec::with_capacity(count);
        for _ in 0..count {
            let id = decoder.input.word()?;
            if id > rows.len() {
                return None;
            }
            aliases.push(id);
        }
        decoder.input.finish().then_some(Self {
            decoder,
            rows,
            aliases,
            checked: 0,
        })
    }
    pub(super) fn request(
        &mut self,
        index: usize,
        reads: &FunctionReads,
        names: &HashMap<&str, usize>,
        current_callees: &[Option<String>],
    ) -> Option<Arc<Normalized>> {
        let id = self.aliases.get(index)?.checked_sub(1)?;
        let row = self.rows.get_mut(id)?;
        let deps = reads.dependencies.as_ref().as_ref()?;
        if row.callees.len() != deps.len()
            || !row.callees.iter().zip(deps).all(|((id, name), dep)| {
                id == dep
                    && current_callees.get(*dep).and_then(Option::as_deref) == Some(name.as_str())
            })
        {
            return None;
        }
        if row.decoded.is_none() {
            row.decoded = Some((|| {
                let function = self.decoder.resolve(&row.function)?;
                if !function.name.is_empty() || function.blocks.is_none() {
                    return None;
                }
                use std::hash::Hash;
                let mut hash = crate::object_cache::BufferedHasher::new();
                b"ZIL-CLOSED-CALL-CLASS-v1".hash(&mut hash);
                function.hash(&mut hash);
                (row.selector == hash.digest()).then(|| {
                    Arc::new(Normalized {
                        callees: row.callees.clone(),
                        function,
                        selector: row.selector,
                    })
                })
            })());
        }
        let normalized = row.decoded.as_ref()?.as_ref()?;
        self.checked += 1;
        valid_normalized(reads, normalized, names).then(|| normalized.clone())
    }
}
impl Cache {
    #[cfg(test)]
    pub(crate) fn encode_raw(&self) -> Option<Vec<u8>> {
        let mut out = Encoder::new();
        self.encode_into(&mut out)?;
        out.finish(MAGIC)
    }
    pub(crate) fn encode_into(&self, out: &mut Encoder<'_>) -> Option<()> {
        out.out.text(&self.triple)?;
        out.out.word(self.context.len())?;
        for (header, defined) in &self.context {
            out.function(header)?;
            out.out.word(usize::from(*defined))?;
        }
        let mut rows = Vec::new();
        let mut ids = HashMap::new();
        let mut aliases = Vec::with_capacity(self.functions.len());
        for reads in &self.functions {
            if let Some(normalized) = &reads.normalized {
                let next = rows.len();
                let index = *ids.entry(Arc::as_ptr(normalized)).or_insert_with(|| {
                    rows.push(normalized);
                    next
                });
                aliases.push(index + 1);
            } else {
                aliases.push(0);
            }
        }
        if aliases.len() != self.context.len() {
            return None;
        }
        out.out.word(rows.len())?;
        for normalized in rows {
            out.out.word(normalized.callees.len())?;
            for (id, name) in &normalized.callees {
                out.out.word(*id)?;
                out.out.text(name)?;
            }
            out.function(&normalized.function)?;
            out.out.put(&normalized.selector.0)?;
        }
        for id in aliases {
            out.out.word(id)?;
        }
        Some(())
    }
    #[cfg(test)]
    pub(crate) fn decode_raw(bytes: &[u8], limits: &Limits, current: &Module) -> Option<Self> {
        let mut input = Decoder::new(bytes, MAGIC, limits)?;
        Self::decode_from(&mut input, current)
    }
    #[cfg(test)]
    pub(crate) fn decode_from(input: &mut Decoder<'_>, current: &Module) -> Option<Self> {
        let triple = input.input.text()?;
        if triple != current.triple {
            return None;
        }
        let functions = current
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some(function),
                _ => None,
            })
            .collect::<Vec<_>>();
        let count = input.input.count()?;
        if count != functions.len() || count > 32_768 {
            return None;
        }
        let mut context = Vec::with_capacity(count.min(4096));
        let mut names = HashMap::new();
        // Complete current header inventory, before any body decoding. Raw rows
        // cannot hide a changed/missing callee, duplicate name or declaration.
        for (index, function) in functions.iter().enumerate() {
            let header = input.function()?;
            let defined = match input.input.word()? {
                0 => false,
                1 => true,
                _ => return None,
            };
            if header.blocks.is_some()
                || header.name != function.name
                || !same_class_header(&header, function)
                || defined != function.blocks.is_some()
                || names.insert(function.name.as_str(), index).is_some()
            {
                return None;
            }
            context.push((header, defined));
        }
        let n = input.input.count()?;
        if n > count {
            return None;
        }
        let mut normalized = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let n = input.input.count()?;
            let mut callees = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let id = input.input.word()?;
                if id >= count || callees.last().is_some_and(|(old, _)| *old >= id) {
                    return None;
                }
                callees.push((id, input.input.text()?));
            }
            let function = input.function()?;
            if !function.name.is_empty() || function.blocks.is_none() {
                return None;
            }
            let selector = zil_base::Fingerprint(input.input.take(32)?.try_into().ok()?);
            use std::hash::Hash;
            let mut hash = crate::object_cache::BufferedHasher::new();
            b"ZIL-CLOSED-CALL-CLASS-v1".hash(&mut hash);
            function.hash(&mut hash);
            if selector != hash.digest() {
                return None;
            }
            normalized.push(Arc::new(Normalized {
                callees,
                function,
                selector,
            }));
        }
        let mut cache = Self {
            triple,
            context,
            ..Self::default()
        };
        let mut bodies: HashMap<Option<*const Vec<Block>>, Vec<usize>> = HashMap::new();
        let mut observations: Vec<Arc<FunctionReads>> = vec![];
        let mut validated = HashMap::new();
        for function in &functions {
            let id = input.input.word()?;
            let candidate = if id == 0 {
                None
            } else {
                Some(normalized.get(id - 1)?.clone())
            };
            let owner = body_owner(function);
            let observed = bodies.get(&owner).and_then(|rows| {
                rows.iter().copied().find(|&i| {
                    same_class_header(function, &observations[i].input)
                        && function.blocks == observations[i].input.blocks
                })
            });
            let index = if let Some(index) = observed {
                index
            } else {
                let index = observations.len();
                observations.push(Arc::new(FunctionReads {
                    input: Arc::new((*function).clone()),
                    addresses: Arc::new(function_addresses(function, &|| false)?),
                    dependencies: Arc::new(dependencies(function, &functions, &names, &|| false)),
                    cost: function.cost(),
                    normalized: None,
                }));
                bodies.entry(owner).or_default().push(index);
                cache.current_recovery_bodies_checked += 1;
                index
            };
            let mut reads = (*observations[index]).clone();
            // Actual current dependencies and the complete normalization
            // relation decide each candidate. An edited body simply runs the
            // original normalizer; its siblings can still use their own rows.
            reads.normalized = candidate.filter(|candidate| {
                *validated
                    .entry((index, id))
                    .or_insert_with(|| valid_normalized(&reads, candidate, &names))
            });
            cache.functions.push(Arc::new(reads));
        }
        input.input.finish().then_some(cache)
    }
}
fn valid_normalized(
    reads: &FunctionReads,
    normalized: &Normalized,
    names: &HashMap<&str, usize>,
) -> bool {
    let Some(deps) = reads.dependencies.as_ref() else {
        return false;
    };
    if normalized.callees.len() != deps.len()
        || !normalized.callees.iter().map(|(id, _)| id).eq(deps)
        || !same_class_header(&reads.input, &normalized.function)
        || !normalized.function.name.is_empty()
    {
        return false;
    }
    let (Some(left), Some(right)) = (&reads.input.blocks, &normalized.function.blocks) else {
        return false;
    };
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right.iter()).all(|(a, b)| {
        a.name == b.name
            && a.instructions.len() == b.instructions.len()
            && a.instructions.iter().zip(&b.instructions).all(|(a, b)| {
                if a.result != b.result {
                    return false;
                }
                match (&a.op, &b.op) {
                    (
                        Op::Call {
                            result: ar,
                            result_attrs: aa,
                            callee: Val::Global(an),
                            args: ax,
                            vararg: av,
                        },
                        Op::Call {
                            result: br,
                            result_attrs: ba,
                            callee: Val::Global(bn),
                            args: bx,
                            vararg: bv,
                        },
                    ) => {
                        let Some(id) = names.get(an.as_str()) else {
                            return false;
                        };
                        let expected = normalized
                            .callees
                            .binary_search_by_key(id, |(id, _)| *id)
                            .ok()
                            .map_or(an.as_str(), |i| normalized.callees[i].1.as_str());
                        expected == bn && ar == br && aa == ba && ax == bx && av == bv
                    }
                    _ => a.op == b.op,
                }
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Inst, Signature};

    fn source(value: i128) -> Module {
        let leaf = Function {
            name: "zil.fn.0".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value)))),
                    )],
                }]
                .into(),
            ),
        };
        let call = Function {
            name: "zil.fn.1".into(),
            linkage: Linkage::External,
            signature: leaf.signature.clone(),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions: vec![
                        Inst::new(
                            Some("v".into()),
                            Op::Call {
                                result: Ty::Int(32),
                                result_attrs: vec![],
                                callee: Val::Global(leaf.name.clone()),
                                args: vec![],
                                vararg: None,
                            },
                        ),
                        Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("v".into())))),
                        ),
                    ],
                }]
                .into(),
            ),
        };
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: String::new(),
            items: vec![Item::Function(leaf), Item::Function(call)].into(),
        }
    }
    fn cache() -> Cache {
        canonicalize_stable_cached(&mut source(7), &BTreeSet::new(), &|| false, None)
            .unwrap()
            .2
    }
    #[test]
    fn demand_checks_current_dependencies_before_loading_normalized_values() {
        let raw = cache().encode_raw().unwrap();
        let mut decoder = Decoder::new(&raw, MAGIC, &Limits::default()).unwrap();
        let mut actual = source(11);
        let mut expected = actual.clone();
        let mut candidates = Candidates::read(&mut decoder, &actual).unwrap();
        let headers = candidates.decoder.decoded;
        let (_, _, current) = canonicalize_recovering(
            &mut actual,
            &BTreeSet::new(),
            &|| false,
            None,
            Some(&mut candidates),
        )
        .unwrap();
        assert_eq!(current.rebuilt_keys, 2);
        assert_eq!(candidates.checked, 1);
        assert_eq!(
            candidates.decoder.decoded,
            headers + 1,
            "changed leaf is compared; its now-stale caller value must not be decoded"
        );
        canonicalize_stable_cached(&mut expected, &BTreeSet::new(), &|| false, None).unwrap();
        assert_eq!(actual, expected);

        let mut decoder = Decoder::new(&raw, MAGIC, &Limits::default()).unwrap();
        let mut actual = source(7);
        let expected = actual.clone();
        let mut candidates = Candidates::read(&mut decoder, &actual).unwrap();
        let headers = candidates.decoder.decoded;
        let protected = BTreeSet::from(["zil.fn.0".into(), "zil.fn.1".into()]);
        canonicalize_recovering(
            &mut actual,
            &protected,
            &|| false,
            None,
            Some(&mut candidates),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(candidates.checked, 0);
        assert_eq!(
            candidates.decoder.decoded, headers,
            "unrequested products stay raw"
        );
    }
    #[test]
    fn demanded_corrupt_selector_falls_back_to_current_producer() {
        let mut old = cache();
        Arc::make_mut(
            Arc::make_mut(&mut old.functions[0])
                .normalized
                .as_mut()
                .unwrap(),
        )
        .selector
        .0[0] ^= 1;
        let raw = old.encode_raw().unwrap();
        let mut decoder = Decoder::new(&raw, MAGIC, &Limits::default()).unwrap();
        let mut actual = source(7);
        let mut expected = actual.clone();
        let mut candidates = Candidates::read(&mut decoder, &actual).unwrap();
        let (_, _, cache) = canonicalize_recovering(
            &mut actual,
            &BTreeSet::new(),
            &|| false,
            None,
            Some(&mut candidates),
        )
        .unwrap();
        assert!(cache.rebuilt_keys > 0);
        canonicalize_stable_cached(&mut expected, &BTreeSet::new(), &|| false, None).unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn shared_alias_inventory_classifies_without_expanding_function_wrappers() {
        let source = source(7);
        let body: SharedVec<_> = vec![source.items[0].clone()].into();
        let mut parts = (0..5105)
            .map(|id| (Some(format!("zil.fn.{id}")), body.clone()))
            .collect::<Vec<_>>();
        parts.push((Some("main".into()), vec![source.items[1].clone()].into()));
        let machine = crate::machine_output::MachineOutput::from_aliases(
            source.triple.clone(),
            String::new(),
            parts,
        );
        assert!(!machine.is_materialized());
        let mut aliases = machine.aliases().unwrap().clone();
        assert!(Arc::ptr_eq(
            &aliases.items[0].value,
            &aliases.items[5104].value
        ));
        let protected = BTreeSet::from(["main".into()]);
        let mut checks = crate::machine_preflight::CurrentPreflight::default();
        checks.check_aliases(&aliases).unwrap();
        let (stats, symbols, cache) =
            canonicalize_aliases(&mut aliases, &protected, &|| false, None, None).unwrap();
        checks.check_aliases(&aliases).unwrap();
        assert_eq!(stats.equivalence_classes, 1);
        assert_eq!(cache.body_owners, 2);
        assert_eq!(symbols.len(), 1);
        assert!(Arc::ptr_eq(
            &cache.functions[0].input,
            &cache.functions[5104].input
        ));
        assert!(Arc::ptr_eq(
            &aliases.items[0].value,
            &aliases.items[5104].value
        ));
        assert!(!machine.is_materialized());
        let mut expected = machine.to_module();
        canonicalize_stable_cached(&mut expected, &protected, &|| false, None).unwrap();
        assert_eq!(aliases.materialize(), expected);
    }
    #[test]
    fn raw_recovery_rederives_reads_and_checks_normalization_relation() {
        let limits = Limits::default();
        let raw = cache().encode_raw().unwrap();
        assert!(Cache::decode_raw(&raw, &limits, &source(7)).is_some());
        let mut omitted = cache();
        Arc::make_mut(
            Arc::make_mut(&mut omitted.functions[1])
                .normalized
                .as_mut()
                .unwrap(),
        )
        .callees
        .clear();
        let checked =
            Cache::decode_raw(&omitted.encode_raw().unwrap(), &limits, &source(7)).unwrap();
        assert!(
            checked.functions[1].normalized.is_none(),
            "omitted dependency cannot grant reuse"
        );
        let mut changed = cache();
        let function = Arc::make_mut(
            Arc::make_mut(&mut changed.functions[0])
                .normalized
                .as_mut()
                .unwrap(),
        );
        function.function.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(99))));
        assert!(Cache::decode_raw(&changed.encode_raw().unwrap(), &limits, &source(7)).is_none());
        let mut limited = limits;
        limited.max_nodes = 1;
        assert!(Cache::decode_raw(&raw, &limited, &source(7)).is_none());
        for length in 0..raw.len() {
            assert!(Cache::decode_raw(&raw[..length], &Limits::default(), &source(7)).is_none());
        }
    }
    #[test]
    fn restored_selector_preserves_deterministic_names_and_rejects_forgery() {
        let old = cache();
        let restored =
            Cache::decode_raw(&old.encode_raw().unwrap(), &Limits::default(), &source(7)).unwrap();
        let mut actual = source(7);
        let mut expected = actual.clone();
        let (_, symbols, _) =
            canonicalize_stable_cached(&mut actual, &BTreeSet::new(), &|| false, Some(&restored))
                .unwrap();
        let (_, expected_symbols, _) =
            canonicalize_stable_cached(&mut expected, &BTreeSet::new(), &|| false, None).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(symbols, expected_symbols);
        let mut forged = old;
        let normalized = Arc::make_mut(
            Arc::make_mut(&mut forged.functions[0])
                .normalized
                .as_mut()
                .unwrap(),
        );
        normalized.selector.0[0] ^= 1;
        // Re-encode valid framing: rejection must come from the naming
        // relation, rather than a transport checksum or malformed input.
        assert!(
            Cache::decode_raw(
                &forged.encode_raw().unwrap(),
                &Limits::default(),
                &source(7)
            )
            .is_none()
        );
    }
    #[test]
    fn restarted_class_inputs_and_current_callee_classes_gate_reuse() {
        let old = cache();
        let restored =
            Cache::decode_raw(&old.encode_raw().unwrap(), &Limits::default(), &source(11)).unwrap();
        let mut actual = source(11);
        let mut expected = actual.clone();
        let (_, _, current) =
            canonicalize_stable_cached(&mut actual, &BTreeSet::new(), &|| false, Some(&restored))
                .unwrap();
        canonicalize_stable_cached(&mut expected, &BTreeSet::new(), &|| false, None).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(current.reused_inputs, 2);
        assert_eq!(
            current.rebuilt_keys, 2,
            "changed leaf class must invalidate its unchanged caller key"
        );
        let mut abi = source(7);
        let Item::Function(leaf) = &mut abi.items[0] else {
            panic!()
        };
        leaf.signature
            .params
            .push(zil_llvm_api::model::Param::unnamed(Ty::Int(32)));
        assert!(Cache::decode_raw(&old.encode_raw().unwrap(), &Limits::default(), &abi).is_none());
        assert!(
            zil_llvm_api::validate_module(&abi).is_err(),
            "cached raw data cannot hide current ABI error"
        );
    }
}
