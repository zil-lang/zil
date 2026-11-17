//! Raw analysis products for the explicit trusted semantic cache. The actual
//! original module still receives a fresh complete preflight. Cache input/header
//! Eq and CURRENT callee classes are checked by canonicalize_inner on every hit.
use super::*;
use crate::machine_cache::{Decoder, Encoder};
use std::sync::Arc;
use zil_base::Limits;

const MAGIC: &[u8; 8] = b"ZILCLS01";
impl Cache {
    pub(crate) fn encode_raw(&self) -> Option<Vec<u8>> {
        let mut out = Encoder::new();
        out.out.text(&self.triple)?;
        out.out.word(self.functions.len())?;
        // A shared input body/header carries identical raw observations and
        // normalized data. Symbol names remain independent per logical row.
        let mut owners: HashMap<Option<*const Vec<Block>>, Vec<usize>> = HashMap::new();
        let mut records: Vec<&FunctionReads> = vec![];
        for reads in &self.functions {
            let owner = body_owner(&reads.input);
            let old = owners.get(&owner).and_then(|rows| {
                rows.iter().copied().find(|&i| {
                    let old = records[i];
                    same_class_header(&old.input, &reads.input)
                        && old.input.blocks == reads.input.blocks
                        && old.addresses == reads.addresses
                        && old.dependencies == reads.dependencies
                        && old.cost == reads.cost
                        && old.normalized == reads.normalized
                })
            });
            out.out.text(&reads.input.name)?;
            if let Some(old) = old {
                out.out.word(old)?;
                continue;
            }
            let index = records.len();
            out.out.word(index)?;
            out.function(&reads.input)?;
            out.out.word(reads.cost)?;
            out.out.word(reads.addresses.len())?;
            for address in reads.addresses.iter() {
                out.out.text(address)?;
            }
            out.out.word(usize::from(reads.dependencies.is_some()))?;
            if let Some(deps) = reads.dependencies.as_ref() {
                out.out.word(deps.len())?;
                for &id in deps {
                    out.out.word(id)?;
                }
            }
            out.out.word(usize::from(reads.normalized.is_some()))?;
            if let Some(normalized) = &reads.normalized {
                out.out.word(normalized.callees.len())?;
                for (id, name) in &normalized.callees {
                    out.out.word(*id)?;
                    out.out.text(name)?;
                }
                out.function(&normalized.function)?;
                out.out.put(&normalized.selector.0)?;
                // This selects a bucket/name only; full normalized Eq remains
                // the class proof, and validate_raw checks its full relation.
            }
            owners.entry(owner).or_default().push(index);
            records.push(reads);
        }
        out.finish(MAGIC)
    }
    pub(crate) fn decode_raw(bytes: &[u8], limits: &Limits) -> Option<Self> {
        let mut input = Decoder::new(bytes, MAGIC, limits)?;
        let triple = input.input.text()?;
        let count = input.input.count()?;
        if count > limits.max_functions.min(32_768) {
            return None;
        }
        let mut records: Vec<Arc<FunctionReads>> = vec![];
        let mut functions = Vec::with_capacity(count.min(4096));
        let mut context = Vec::with_capacity(count.min(4096));
        let mut names = BTreeSet::new();
        for _ in 0..count {
            let name = input.input.text()?;
            if !names.insert(name.clone()) {
                return None;
            }
            let index = input.input.word()?;
            if index > records.len() {
                return None;
            }
            let reads = if index < records.len() {
                let mut reads = (*records[index]).clone();
                reads.input.name = name;
                Arc::new(reads)
            } else {
                let mut function = input.function()?;
                function.name = name;
                let cost = input.input.word()?;
                if cost != function.cost() {
                    return None;
                }
                let n = input.input.count()?;
                let mut addresses = BTreeSet::new();
                for _ in 0..n {
                    if !addresses.insert(input.input.text()?) {
                        return None;
                    }
                }
                let dependencies = match input.input.word()? {
                    0 => None,
                    1 => {
                        let n = input.input.count()?;
                        let mut deps = BTreeSet::new();
                        for _ in 0..n {
                            let id = input.input.word()?;
                            if id >= count || !deps.insert(id) {
                                return None;
                            }
                        }
                        Some(deps)
                    }
                    _ => return None,
                };
                let normalized = match input.input.word()? {
                    0 => None,
                    1 => {
                        let n = input.input.count()?;
                        let mut callees = Vec::with_capacity(n.min(4096));
                        for _ in 0..n {
                            let id = input.input.word()?;
                            if id >= count || callees.last().is_some_and(|(last, _)| *last >= id) {
                                return None;
                            }
                            callees.push((id, input.input.text()?));
                        }
                        let function = input.function()?;
                        if !function.name.is_empty() || function.blocks.is_none() {
                            return None;
                        }
                        let selector =
                            zil_base::Fingerprint(input.input.take(32)?.try_into().ok()?);
                        // Stable symbol spelling must equal the original
                        // producer, even for otherwise valid raw products.
                        use std::hash::Hash;
                        let mut hash = crate::object_cache::BufferedHasher::new();
                        b"ZIL-CLOSED-CALL-CLASS-v1".hash(&mut hash);
                        function.hash(&mut hash);
                        if selector != hash.digest() {
                            return None;
                        }
                        Some(Arc::new(Normalized {
                            callees,
                            function,
                            selector,
                        }))
                    }
                    _ => return None,
                };
                let reads = Arc::new(FunctionReads {
                    input: function,
                    addresses: Arc::new(addresses),
                    dependencies: Arc::new(dependencies),
                    cost,
                    normalized,
                });
                records.push(reads.clone());
                reads
            };
            let mut header = reads.input.clone();
            let defined = header.blocks.is_some();
            header.blocks = None;
            context.push((header, defined));
            functions.push(reads);
        }
        if !input.input.finish() {
            return None;
        }
        let cache = Self {
            triple,
            context,
            functions,
            ..Self::default()
        };
        cache.validate_raw()?;
        Some(cache)
    }
}

impl Cache {
    fn validate_raw(&self) -> Option<()> {
        let functions = self.functions.iter().map(|f| &f.input).collect::<Vec<_>>();
        let names = functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.as_str(), i))
            .collect();
        // The decoded input is later compared in FULL with the current function
        // and entire current header inventory. Derive read dependencies from
        // that raw model, never let serialized omitted edges grant reuse.
        let mut checked = BTreeSet::new();
        for reads in &self.functions {
            let key = (
                Arc::as_ptr(&reads.addresses),
                Arc::as_ptr(&reads.dependencies),
            );
            if !checked.insert(key) {
                continue;
            }
            if function_addresses(&reads.input, &|| false)? != *reads.addresses
                || dependencies(&reads.input, &functions, &names, &|| false) != *reads.dependencies
            {
                return None;
            }
            if let Some(normalized) = &reads.normalized {
                if !valid_normalized(reads, normalized, &names) {
                    return None;
                }
            }
        }
        Some(())
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
    fn raw_recovery_rederives_reads_and_checks_normalization_relation() {
        let limits = Limits::default();
        let raw = cache().encode_raw().unwrap();
        assert!(Cache::decode_raw(&raw, &limits).is_some());
        let mut omitted = cache();
        Arc::make_mut(&mut omitted.functions[1]).dependencies = Arc::new(Some(BTreeSet::new()));
        assert!(Cache::decode_raw(&omitted.encode_raw().unwrap(), &limits).is_none());
        let mut changed = cache();
        let function = Arc::make_mut(
            Arc::make_mut(&mut changed.functions[0])
                .normalized
                .as_mut()
                .unwrap(),
        );
        function.function.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(99))));
        assert!(Cache::decode_raw(&changed.encode_raw().unwrap(), &limits).is_none());
        let mut limited = limits;
        limited.max_nodes = 1;
        assert!(Cache::decode_raw(&raw, &limited).is_none());
        for length in 0..raw.len() {
            assert!(Cache::decode_raw(&raw[..length], &Limits::default()).is_none());
        }
    }
    #[test]
    fn restored_selector_preserves_deterministic_names_and_rejects_forgery() {
        let old = cache();
        let restored = Cache::decode_raw(&old.encode_raw().unwrap(), &Limits::default()).unwrap();
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
        assert!(Cache::decode_raw(&forged.encode_raw().unwrap(), &Limits::default()).is_none());
    }
    #[test]
    fn restarted_class_inputs_and_current_callee_classes_gate_reuse() {
        let old = cache();
        let restored = Cache::decode_raw(&old.encode_raw().unwrap(), &Limits::default()).unwrap();
        let mut actual = source(11);
        let mut expected = actual.clone();
        let (_, _, current) =
            canonicalize_stable_cached(&mut actual, &BTreeSet::new(), &|| false, Some(&restored))
                .unwrap();
        canonicalize_stable_cached(&mut expected, &BTreeSet::new(), &|| false, None).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(current.reused_inputs, 1);
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
        assert!(
            zil_llvm_api::validate_module(&abi).is_err(),
            "cached raw data cannot hide current ABI error"
        );
    }
}
