//! Session-owned raw native work products. Every entry is bound to immutable
//! input values; current Verify and LIR binding remain outside this cache.
use crate::BuildOptions;
use std::sync::Arc;
use zil_base::Fingerprint;
use zil_ir::{FunctionId, LirView};
use zil_llvm_api::model::Module;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendSession {
    pub(crate) lower: crate::emit::incremental::Session,
    pub(crate) partition: Option<Arc<PartitionProduct>>,
    pub(crate) transformed: Option<Arc<TransformProduct>>,
    pub(crate) keys: Vec<Arc<KeyProduct>>,
    pub(crate) key_partition: Option<Arc<PartitionProduct>>,
    pub(crate) preflights: Vec<Arc<PreflightProduct>>,
    pub(crate) native_products: NativeProductHistory,
    pub(crate) summary: BackendSessionSummary,
    pub(crate) link: Option<Arc<crate::link_cache::Product>>,
    pub(crate) link_tools: crate::object_cache::InstalledToolIdentities,
    diagnostic_items: zil_ir::PersistentVec<DiagnosticItem>,
    diagnostic_fragments: zil_ir::PersistentVec<Arc<String>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendSessionSummary {
    pub reused_machine_module: bool,
    pub whole_machine_input_functions_compared: usize,
    pub lowered_functions: usize,
    pub reused_functions: usize,
    pub reused_partition: bool,
    pub reused_partition_recipe: bool,
    pub reused_partition_units: usize,
    pub rebuilt_partition_units: usize,
    pub reused_machine_transforms: bool,
    pub reused_object_keys: usize,
    pub reused_link: bool,
    pub rendered_diagnostic_items: usize,
    pub reused_diagnostic_items: usize,
    pub source_projection_traversals: usize,
    pub source_projection_instruction_visits: usize,
    pub source_projection_reuses: usize,
    pub native_history_restored_units: usize,
    pub native_history_retained_epochs: usize,
    /// Encoded payload of distinct opaque native products retained by this
    /// history. Sharing inside different products remains conservatively charged.
    /// This is not process RSS or memory held by separate session checkpoints.
    pub native_history_payload_bytes: u64,
}
impl BackendSession {
    pub fn summary(&self) -> &BackendSessionSummary {
        &self.summary
    }
    pub(crate) fn render(&mut self, module: &Module) -> DiagnosticText {
        let header = Module {
            triple: module.triple.clone(),
            fingerprint: module.fingerprint.clone(),
            items: (Vec::new()).into(),
        }
        .to_text();
        let mut fragments = self.diagnostic_fragments.clone();
        let mut next = self.diagnostic_items.clone();
        fragments.truncate(module.items.len());
        next.truncate(module.items.len());
        let mut relocated = None;
        let mut positions = self.diagnostic_items.iter();
        for (index, item) in module.items.iter().enumerate() {
            let positional = positions.next();
            if positional.is_some_and(|old| old.item == *item) {
                self.summary.reused_diagnostic_items += 1;
                continue;
            }
            let old = if positional
                .is_none_or(|old| diagnostic_name(&old.item) != diagnostic_name(item))
            {
                // Ordinary body edits keep their slots: no name formatting,
                // name-map reconstruction or fragment-vector aggregation. Only
                // insertion/deletion/reordering needs this borrowed old index.
                let names = relocated.get_or_insert_with(|| {
                    self.diagnostic_items
                        .iter()
                        .enumerate()
                        .map(|(slot, old)| (diagnostic_name(&old.item), slot))
                        .collect::<std::collections::BTreeMap<_, _>>()
                });
                names
                    .get(&diagnostic_name(item))
                    .and_then(|&slot| self.diagnostic_items.owner(slot))
                    .filter(|old| old.item == *item)
            } else {
                None
            };
            let product = if let Some(old) = old {
                self.summary.reused_diagnostic_items += 1;
                old.clone()
            } else {
                self.summary.rendered_diagnostic_items += 1;
                Arc::new(DiagnosticItem {
                    item: item.clone(),
                    text: Arc::new(zil_llvm_api::model::item_to_text(item)),
                })
            };
            if let Some(old) = fragments.get(index) {
                if !Arc::ptr_eq(old, &product.text) {
                    fragments.replace(index, product.text.clone());
                }
                next.set_owner(index, product);
            } else {
                fragments.push(product.text.clone());
                next.push_owner(product);
            }
        }
        drop(relocated);
        drop(positions);
        self.diagnostic_items = next;
        self.diagnostic_fragments = fragments.clone();
        DiagnosticText {
            text: Arc::new(header),
            fragments,
            fingerprint: None,
        }
    }
    pub(crate) fn begin(&mut self) {
        self.summary = BackendSessionSummary {
            native_history_retained_epochs: self.native_products.retained_epochs(),
            native_history_payload_bytes: self.native_products.payload_bytes(),
            ..BackendSessionSummary::default()
        };
        self.link_tools.begin();
    }
}
fn diagnostic_name(item: &zil_llvm_api::model::Item) -> (u8, &str) {
    match item {
        zil_llvm_api::model::Item::Function(function) => (0, &function.name),
        zil_llvm_api::model::Item::Global(global) => (1, &global.name),
        zil_llvm_api::model::Item::NamedType(name, _) => (2, name),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticItem {
    item: zil_llvm_api::model::Item,
    text: Arc<String>,
}
/// Private identities minted only after complete current machine/source output
/// equality. They are raw scheduling evidence, never checker capabilities.
#[derive(Clone, Debug)]
pub(crate) struct FunctionProductIdentity(pub(crate) Arc<()>, pub(crate) Arc<()>);
impl PartialEq for FunctionProductIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) && Arc::ptr_eq(&self.1, &other.1)
    }
}
impl Eq for FunctionProductIdentity {}
pub(crate) type ScalarProducts = zil_ir::PersistentVec<FunctionProductIdentity>;
pub(crate) type ScalarIdentity = (Arc<()>, ScalarProducts);
pub(crate) fn same_identity(left: &ScalarIdentity, right: &ScalarIdentity) -> bool {
    Arc::ptr_eq(&left.0, &right.0) && same_products(&left.1, &right.1)
}
fn same_products(left: &ScalarProducts, right: &ScalarProducts) -> bool {
    left.len() == right.len()
        && left
            .changed_indices(right)
            .into_iter()
            .all(|index| left[index] == right[index])
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransformProduct {
    pub(crate) input: ScalarIdentity,
    pub(crate) machine: Module,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionInput {
    options: BuildOptions,
    names: Option<Vec<(FunctionId, String)>>,
    collected: bool,
    closures: Vec<FunctionId>,
    raw: Option<RawInput>,
    machine: Option<Module>,
    scalar: Option<ScalarIdentity>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct RawInput {
    functions: zil_ir::PersistentVec<zil_ir::TypedFunction>,
    closures: Vec<zil_ir::ClosureDefinition>,
    resources: Vec<zil_ir::ResourceDefinition>,
    layouts: Vec<zil_ir::ScalarLayout>,
}
impl PartitionInput {
    pub(crate) fn new(
        options: &BuildOptions,
        names: Option<&[(FunctionId, String)]>,
        collected: bool,
        lir: &LirView<'_>,
        machine: &Module,
        scalar: Option<ScalarIdentity>,
    ) -> Self {
        let mut options = options.clone();
        options.output.clear();
        let machine = scalar.is_none().then(|| {
            let mut machine = machine.clone();
            machine.fingerprint.clear(); // diagnostic only, never native Builder input
            machine
        });
        Self {
            options,
            names: names.map(<[_]>::to_vec),
            collected,
            closures: lir
                .closures
                .iter()
                .map(|closure| closure.function)
                .collect(),
            raw: scalar.is_none().then(|| RawInput {
                functions: lir.functions.clone(),
                closures: lir.closures.to_vec(),
                resources: lir.resources.to_vec(),
                layouts: lir.layouts.to_vec(),
            }),
            machine,
            scalar,
        }
    }
}
impl PartitionInput {
    pub(crate) fn same_recipe_environment(&self, other: &Self) -> bool {
        self.options == other.options
            && self.names == other.names
            && self.collected == other.collected
            && self.closures == other.closures
            && matches!((&self.scalar, &other.scalar), (Some((left, _)), Some((right, _))) if Arc::ptr_eq(left, right))
    }
}
impl PartialEq for PartitionInput {
    fn eq(&self, other: &Self) -> bool {
        if self.options != other.options
            || self.names != other.names
            || self.collected != other.collected
            || self.closures != other.closures
        {
            return false;
        }
        match (&self.scalar, &other.scalar) {
            (Some((left_context, left)), Some((right_context, right))) => {
                Arc::ptr_eq(left_context, right_context) && same_products(left, right)
            }
            (None, None) => self.raw == other.raw && self.machine == other.machine,
            _ => false,
        }
    }
}
impl Eq for PartitionInput {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PartitionProduct {
    pub(crate) input: PartitionInput,
    pub(crate) units: Vec<Module>,
    pub(crate) fallback: Option<&'static str>,
    pub(crate) diagnostic: DiagnosticText,
    pub(crate) private_closures: usize,
    pub(crate) recipe: Option<crate::machine_cgu::Recipe>,
}
impl PartitionProduct {
    pub(crate) fn diagnostic(&self, fingerprint: &str) -> DiagnosticText {
        self.diagnostic.current(fingerprint)
    }
}
/// Shared exact diagnostic bytes. A current verification fingerprint can be
/// streamed into the header without cloning the retained function text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticText {
    text: Arc<String>,
    fragments: zil_ir::PersistentVec<Arc<String>>,
    fingerprint: Option<String>,
}
impl From<String> for DiagnosticText {
    fn from(text: String) -> Self {
        Self {
            text: Arc::new(text),
            fragments: Vec::new().into(),
            fingerprint: None,
        }
    }
}
impl DiagnosticText {
    fn current(&self, fingerprint: &str) -> Self {
        Self {
            text: self.text.clone(),
            fragments: self.fragments.clone(),
            fingerprint: Some(fingerprint.to_owned()),
        }
    }
    pub(crate) fn write(&self, path: &std::path::Path) -> std::io::Result<()> {
        self.write_to(std::fs::File::create(path)?)
    }
    fn write_to(&self, mut output: impl std::io::Write) -> std::io::Result<()> {
        use std::io::{Error, ErrorKind, IoSlice};
        // Gather only bounded slice metadata. All payload bytes stay in their
        // immutable fragments; File::write_vectored consumes them directly.
        // No full-text String or 1 MiB payload aggregation buffer is built.
        const VECTORS: usize = 64;
        let prefix = "; ZIL verified LLVM module\n; verified-fingerprint: ";
        let header: [&[u8]; 3] = if let Some(fingerprint) = &self.fingerprint
            && let Some(rest) = self.text.strip_prefix(prefix)
            && let Some(end) = rest.find('\n')
        {
            [
                prefix.as_bytes(),
                fingerprint.as_bytes(),
                &rest.as_bytes()[end..],
            ]
        } else {
            [self.text.as_bytes(), &[], &[]]
        };
        let mut segments = header
            .into_iter()
            .chain(self.fragments.iter().map(|fragment| fragment.as_bytes()))
            .filter(|bytes| !bytes.is_empty());
        let mut slices = Vec::with_capacity(VECTORS);
        loop {
            slices.clear();
            slices.extend(segments.by_ref().take(VECTORS).map(IoSlice::new));
            if slices.is_empty() {
                break;
            }
            let mut pending = slices.as_mut_slice();
            while !pending.is_empty() {
                match output.write_vectored(pending) {
                    Ok(0) => {
                        return Err(Error::new(
                            ErrorKind::WriteZero,
                            "failed to write diagnostic bytes",
                        ));
                    }
                    Ok(written) => IoSlice::advance_slices(&mut pending, written),
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
        }
        output.flush()
    }
    #[cfg(test)]
    pub(crate) fn to_text(&self) -> String {
        let mut text = self.text.as_ref().clone();
        if let Some(fingerprint) = &self.fingerprint {
            let prefix = "; ZIL verified LLVM module\n; verified-fingerprint: ";
            if let Some(rest) = text.strip_prefix(prefix)
                && let Some(end) = rest.find('\n')
            {
                text.replace_range(prefix.len()..prefix.len() + end, fingerprint);
            }
        }
        for fragment in &self.fragments {
            text.push_str(fragment);
        }
        text
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyProduct {
    pub(crate) directory: std::path::PathBuf,
    pub(crate) module: Module,
    pub(crate) key: Fingerprint,
    pub(crate) late_candidates: Option<Vec<String>>,
}

/// Raw work-product bookkeeping only. Equality here is snapshot identity, never
/// checker authority: every use goes back through the LLVM preflight API.
#[derive(Clone, Debug)]
pub(crate) struct PreflightProduct {
    pub(crate) module: Module,
    pub(crate) receipt: Arc<zil_llvm_api::PreflightReceipt>,
}
impl PartialEq for PreflightProduct {
    fn eq(&self, other: &Self) -> bool {
        self.module == other.module && Arc::ptr_eq(&self.receipt, &other.receipt)
    }
}
impl Eq for PreflightProduct {}

/// A stable ordinal candidate slot, not a reuse authorization. A partition
/// change can put another unit in this slot; the native consumer must still
/// bind its complete current configuration and observed input dependencies.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NativeUnitOwner {
    pub(crate) triple: String,
    pub(crate) requested_units: usize,
    pub(crate) ordinal: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeUnitProduct {
    pub(crate) module: Module,
    pub(crate) products: zil_llvm_api::NativeWorkProducts,
}

// Optional raw-native candidates only, with at most four successful owner epochs.
// The payload cap may retain fewer epochs or units, especially across broad edits;
// the independent checker never lives here.
const NATIVE_HISTORY_EPOCHS: usize = 4;
const NATIVE_HISTORY_PAYLOAD_BYTES: u64 = 128 * 1024 * 1024;
type NativeUnits = std::collections::BTreeMap<NativeUnitOwner, Arc<NativeUnitProduct>>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeProductHistory {
    epochs: std::collections::VecDeque<Arc<NativeEpoch>>,
    // Oldest actual native consumption/generation first. This vector belongs
    // to this history clone, never to a shared epoch. It contains only products
    // still referenced by this history's at most 4 * 64 unit entries.
    recency: Vec<zil_llvm_api::NativeWorkProducts>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeEpoch {
    units: NativeUnits,
}

// Identity is an opaque native allocation, never a raw input/content digest.
// Both single-epoch admission and cross-epoch eviction use this same ledger.
// The caller keeps all counted owners alive, so allocation addresses cannot be
// recycled during a comparison. Nothing here authorizes a compilation reuse.
fn admit_native_payload<T: PartialEq>(
    seen: &mut Vec<T>,
    payload_bytes: &mut u64,
    identity: T,
    bytes: u64,
    cap: u64,
) -> bool {
    if seen.contains(&identity) {
        return true;
    }
    if bytes > cap.saturating_sub(*payload_bytes) {
        return false;
    }
    *payload_bytes += bytes;
    seen.push(identity);
    true
}
impl NativeProductHistory {
    pub(crate) fn candidates(
        &self,
        owner: &NativeUnitOwner,
    ) -> (Option<Arc<NativeUnitProduct>>, Vec<Arc<NativeUnitProduct>>) {
        let parent = self
            .epochs
            .front()
            .and_then(|epoch| epoch.units.get(owner))
            .cloned();
        let older = self
            .epochs
            .iter()
            .skip(1)
            .filter_map(|epoch| epoch.units.get(owner).cloned())
            .collect();
        (parent, older)
    }
    pub(crate) fn retained_epochs(&self) -> usize {
        self.epochs.len()
    }
    pub(crate) fn payload_bytes(&self) -> u64 {
        let mut seen = Vec::new();
        let mut bytes = 0;
        for product in self.epochs.iter().flat_map(|epoch| epoch.units.values()) {
            // At most four successful epochs with at most 64 units each.
            // Borrow the immutable products; this does not clone native maps.
            let admitted = admit_native_payload(
                &mut seen,
                &mut bytes,
                &product.products,
                product.products.retained_payload_bytes(),
                u64::MAX,
            );
            debug_assert!(admitted, "bounded native history cannot overflow u64");
        }
        bytes
    }
    pub(crate) fn commit(
        &mut self,
        enabled: bool,
        next: NativeUnits,
        native_activity: Vec<zil_llvm_api::NativeWorkProducts>,
    ) {
        self.commit_with_activity(enabled, next, native_activity, NATIVE_HISTORY_PAYLOAD_BYTES);
    }
    #[cfg(all(test, feature = "native-products"))]
    fn commit_with_cap(&mut self, enabled: bool, next: NativeUnits, cap: u64) {
        // These tests model new native generations; production supplies actual
        // successful worker events, not every object-cache hit in the next map.
        let activity = next.values().map(|p| p.products.clone()).collect();
        self.commit_with_activity(enabled, next, activity, cap);
    }
    fn prune_recency(&mut self) {
        let epochs = &self.epochs;
        self.recency.retain(|product| {
            epochs
                .iter()
                .any(|epoch| epoch.units.values().any(|entry| entry.products == *product))
        });
    }
    fn remove_allocation(&mut self, product: &zil_llvm_api::NativeWorkProducts) {
        for epoch in &mut self.epochs {
            if epoch.units.values().any(|entry| entry.products == *product) {
                // A staging history may share the immutable epoch with the last
                // successful checkpoint. Copy its map of Arc entries before edit.
                Arc::make_mut(epoch)
                    .units
                    .retain(|_, entry| entry.products != *product);
            }
        }
        self.prune_recency();
    }
    fn commit_with_activity(
        &mut self,
        enabled: bool,
        next: NativeUnits,
        native_activity: Vec<zil_llvm_api::NativeWorkProducts>,
        cap: u64,
    ) {
        if !enabled {
            self.epochs.clear();
            self.recency.clear();
            return;
        }
        // Dedup uses existing opaque product identity only, never a content
        // digest or checker authority. A product is paired with another raw unit
        // only after complete current triple/items equality on an object hit.
        let found = self.epochs.iter().position(|epoch| {
            epoch.units.len() == next.len()
                && epoch
                    .units
                    .iter()
                    .zip(&next)
                    .all(|((old_owner, old), (new_owner, new))| {
                        old_owner == new_owner && old.products == new.products
                    })
        });
        let current = found
            .and_then(|index| self.epochs.remove(index))
            .unwrap_or_else(|| Arc::new(NativeEpoch { units: next }));
        self.epochs.push_front(current);
        while self.epochs.len() > NATIVE_HISTORY_EPOCHS {
            self.epochs.pop_back();
        }
        self.prune_recency();
        // Install the full new map before capacity eviction. Otherwise owner
        // order could discard the hottest newly generated product in advance.
        // Unseen candidates start cold; only the actual activity below heats them.
        let mut unseen = Vec::new();
        for product in self.epochs.iter().flat_map(|epoch| epoch.units.values()) {
            if !self.recency.contains(&product.products) && !unseen.contains(&product.products) {
                unseen.push(product.products.clone());
            }
        }
        unseen.append(&mut self.recency);
        self.recency = unseen;
        for product in native_activity {
            if let Some(index) = self.recency.iter().position(|old| *old == product) {
                let current = self.recency.remove(index);
                self.recency.push(current);
            }
        }
        // A single product that cannot fit must not evict every useful smaller
        // product before it is inevitably discarded itself.
        let oversized: Vec<_> = self
            .recency
            .iter()
            .filter(|product| product.retained_payload_bytes() > cap)
            .cloned()
            .collect();
        for product in oversized {
            self.remove_allocation(&product);
        }
        // Evict one allocation from every epoch, not an entire prior version.
        // The union is recomputed after each removal; opaque internal sharing
        // remains conservatively charged and the 128 MiB cap is unchanged.
        while self.payload_bytes() > cap {
            let Some(oldest) = self.recency.first().cloned() else {
                debug_assert_eq!(self.payload_bytes(), 0);
                break;
            };
            self.remove_allocation(&oldest);
        }
        let mut current = true;
        self.epochs.retain(|epoch| {
            let keep = current || !epoch.units.is_empty();
            current = false;
            keep
        });
    }
}

#[cfg(all(test, feature = "native-products"))]
pub(crate) mod native_history_tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Function, Inst, Item, Linkage, Op, Signature, Ty};

    pub(crate) fn module(answer: i64, sibling: i64) -> Module {
        let function = |name: &str, value| {
            Item::Function(Function {
                name: name.into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), Vec::new()),
                attrs: Vec::new(),
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), value))),
                        )],
                    }]
                    .into(),
                ),
            })
        };
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "diagnostic-only".into(),
            items: vec![function("answer", answer), function("sibling", sibling)].into(),
        }
    }
    pub(crate) fn owner(ordinal: usize) -> NativeUnitOwner {
        NativeUnitOwner {
            triple: "x86_64-unknown-linux-gnu".into(),
            requested_units: 16,
            ordinal,
        }
    }
    fn product(value: i64) -> Arc<NativeUnitProduct> {
        let module = module(value, value + 1);
        let (object, _, product) = zil_llvm_api::compile_module_with_native_products(
            &module,
            &zil_llvm_api::Config {
                triple: &module.triple,
                optimization: zil_llvm_api::Optimization::O2,
            },
            crate::FunctionMerging::Disabled,
            &|| false,
            None,
        )
        .unwrap();
        assert_eq!(
            object.receipt.native_products.unwrap().available,
            1,
            "native history acceptance requires the installed extension"
        );
        Arc::new(NativeUnitProduct {
            module,
            products: product.expect("native product"),
        })
    }
    fn units(product: &Arc<NativeUnitProduct>) -> NativeUnits {
        [(owner(0), product.clone())].into()
    }

    #[test]
    fn native_payload_ledger_counts_shared_allocations_once_at_64_mib_boundary() {
        let first = Arc::new(0_u8);
        let second = Arc::new(0_u8);
        let third = Arc::new(0_u8);
        let allocation = |value: &Arc<u8>| Arc::as_ptr(value);
        let unit = 64 * 1024 * 1024;
        let mut seen = Vec::new();
        let mut bytes = 0;
        assert!(admit_native_payload(
            &mut seen,
            &mut bytes,
            allocation(&first),
            unit,
            unit * 2,
        ));
        assert!(admit_native_payload(
            &mut seen,
            &mut bytes,
            allocation(&first.clone()),
            unit,
            unit * 2,
        ));
        assert_eq!(bytes, unit);
        // Equal value and equal payload size do not identify an allocation.
        assert!(admit_native_payload(
            &mut seen,
            &mut bytes,
            allocation(&second),
            unit,
            unit * 2,
        ));
        assert_eq!(bytes, unit * 2);
        assert!(!admit_native_payload(
            &mut seen,
            &mut bytes,
            allocation(&third),
            unit,
            unit * 2,
        ));
        assert_eq!(bytes, unit * 2);
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn native_history_shared_products_survive_partial_epoch_eviction() {
        let first = product(42);
        let second = product(42);
        assert_ne!(first.products, second.products);
        let first_bytes = first.products.retained_payload_bytes();
        let second_bytes = second.products.retained_payload_bytes();
        assert_eq!(
            first_bytes, second_bytes,
            "same raw input, independent allocation"
        );
        let cap = first_bytes + second_bytes;
        let mut history = NativeProductHistory::default();
        history.commit_with_cap(true, units(&first), cap);
        history.commit_with_cap(
            true,
            [(owner(0), first.clone()), (owner(1), second.clone())].into(),
            cap,
        );
        assert_eq!(history.retained_epochs(), 2);
        assert_eq!(history.payload_bytes(), cap);
        // Replacing a current owner does not erase an older retained owner.
        history.commit_with_cap(true, units(&second), cap);
        assert_eq!(history.retained_epochs(), 3);
        assert_eq!(history.payload_bytes(), cap);
        let checkpoint = history.clone();
        history.epochs.pop_back();
        history.prune_recency();
        assert_eq!(
            history.payload_bytes(),
            cap,
            "middle epoch still owns first"
        );
        history.epochs.pop_back();
        history.prune_recency();
        assert_eq!(
            history.payload_bytes(),
            second_bytes,
            "last history reference removed"
        );
        assert_eq!(
            checkpoint.payload_bytes(),
            cap,
            "checkpoint is an independent immutable history"
        );

        // Admission also deduplicates references inside one owner map. A new
        // wrapper around the same opaque product is not a second native payload.
        let wrapper = Arc::new(NativeUnitProduct {
            module: first.module.clone(),
            products: first.products.clone(),
        });
        history.commit_with_cap(
            true,
            [(owner(0), first.clone()), (owner(1), wrapper)].into(),
            first_bytes,
        );
        assert_eq!(history.retained_epochs(), 1);
        assert_eq!(history.epochs.front().unwrap().units.len(), 2);
        assert_eq!(history.payload_bytes(), first_bytes);
        history.commit_with_cap(true, units(&first), first_bytes);
        assert_eq!(history.retained_epochs(), 2);
        assert_eq!(history.payload_bytes(), first_bytes);
        // Both old epochs must be removed before the distinct second product
        // fits this cap, even though its encoded bytes equal the first's.
        history.commit_with_cap(true, units(&second), first_bytes);
        assert_eq!(history.retained_epochs(), 1);
        assert_eq!(history.payload_bytes(), second_bytes);
        assert_eq!(
            history.candidates(&owner(0)).0.unwrap().products,
            second.products
        );
    }

    #[test]
    fn native_history_is_bounded_lru_and_checkpoint_owners_remain_immutable() {
        let values: Vec<_> = (0..5).map(product).collect();
        let mut history = NativeProductHistory::default();
        for value in &values[..4] {
            history.commit(true, units(value), vec![value.products.clone()]);
        }
        assert_eq!(history.retained_epochs(), 4);
        let checkpoint = history.clone();
        let oldest = history.epochs.back().unwrap().clone();
        history.commit(true, units(&values[0]), vec![values[0].products.clone()]);
        assert_eq!(history.retained_epochs(), 4);
        assert!(Arc::ptr_eq(history.epochs.front().unwrap(), &oldest));
        history.commit(true, units(&values[4]), vec![values[4].products.clone()]);
        let (active, previous) = history.candidates(&owner(0));
        assert_eq!(active.unwrap().products, values[4].products);
        assert_eq!(
            previous.iter().map(|p| &p.products).collect::<Vec<_>>(),
            vec![
                &values[0].products,
                &values[3].products,
                &values[2].products
            ]
        );
        assert_eq!(
            checkpoint.candidates(&owner(0)).0.unwrap().products,
            values[3].products
        );
        assert_eq!(checkpoint.retained_epochs(), 4);
        assert_eq!(history.recency.len(), 4);
        // A failed caller discards its cloned staging history. A successful
        // disabled build releases all retained epochs, including historical ones.
        history.commit(false, NativeUnits::new(), Vec::new());
        assert_eq!(history.retained_epochs(), 0);
        assert_eq!(history.payload_bytes(), 0);
        assert!(history.recency.is_empty());
        assert_eq!(checkpoint.retained_epochs(), 4);
    }

    #[test]
    fn native_history_evicts_allocations_by_real_consumption_not_object_hits() {
        let first = product(42);
        let sibling = product(42);
        let edited = product(42);
        let later = product(42);
        let bytes = first.products.retained_payload_bytes();
        assert!(
            [&sibling, &edited, &later]
                .iter()
                .all(|product| product.products.retained_payload_bytes() == bytes)
        );
        let mut history = NativeProductHistory::default();
        history.commit_with_activity(
            true,
            [(owner(0), first.clone()), (owner(1), sibling.clone())].into(),
            vec![first.products.clone(), sibling.products.clone()],
            bytes * 2,
        );
        let checkpoint = history.clone();
        let prior_epoch = checkpoint.epochs.front().unwrap().clone();
        let initial_recency = history.recency.clone();
        // Restoring an object/current owner does not execute native code and
        // must not heat its product, even when it becomes the current epoch.
        history.commit_with_activity(true, units(&first), Vec::new(), bytes * 2);
        assert_eq!(history.recency, initial_recency);
        // The edited unit actually consumes first, then generates edited.
        // Its sibling is merely an object-cache hit: discard only sibling's
        // cold allocation, from every epoch that references it.
        history.commit_with_activity(
            true,
            [(owner(0), edited.clone()), (owner(1), sibling.clone())].into(),
            vec![first.products.clone(), edited.products.clone()],
            bytes * 2,
        );
        assert_eq!(history.payload_bytes(), bytes * 2);
        assert_eq!(
            history.recency,
            vec![first.products.clone(), edited.products.clone()]
        );
        assert_eq!(
            history.candidates(&owner(0)).0.unwrap().products,
            edited.products
        );
        assert!(
            history
                .candidates(&owner(0))
                .1
                .iter()
                .any(|old| old.products == first.products)
        );
        assert!(history.candidates(&owner(1)).0.is_none());
        assert!(history.candidates(&owner(1)).1.is_empty());
        assert_eq!(
            prior_epoch.units.len(),
            2,
            "shared checkpoint map was not edited"
        );
        assert_eq!(
            checkpoint.recency, initial_recency,
            "ages belong to the staging clone"
        );

        let before_hit = history.recency.clone();
        history.commit_with_activity(true, units(&first), Vec::new(), bytes * 2);
        assert_eq!(history.recency, before_hit);
        history.commit_with_activity(
            true,
            [(owner(1), later.clone())].into(),
            vec![later.products.clone()],
            bytes * 2,
        );
        assert_eq!(history.payload_bytes(), bytes * 2);
        assert_eq!(
            history.recency,
            vec![edited.products.clone(), later.products.clone()]
        );
        assert!(history.epochs.iter().all(|epoch| {
            epoch
                .units
                .values()
                .all(|entry| entry.products != first.products)
        }));

        // Single-epoch admission follows the explicit generation order, not
        // owner order. The hot lower ordinal must not be discarded in advance.
        let mut single = NativeProductHistory::default();
        single.commit_with_activity(
            true,
            [(owner(0), first.clone()), (owner(1), sibling.clone())].into(),
            vec![sibling.products.clone(), first.products.clone()],
            bytes,
        );
        assert_eq!(
            single.candidates(&owner(0)).0.unwrap().products,
            first.products
        );
        assert!(single.candidates(&owner(1)).0.is_none());
        assert_eq!(single.recency, vec![first.products.clone()]);
        assert_eq!(single.payload_bytes(), bytes);
    }

    #[test]
    fn native_history_payload_cap_drops_only_optional_candidates() {
        let first = product(42);
        let second = product(43);
        let cap = first
            .products
            .retained_payload_bytes()
            .max(second.products.retained_payload_bytes());
        assert!(cap > 0 && second.products.retained_payload_bytes() > 0);
        let mut history = NativeProductHistory::default();
        history.commit_with_cap(
            true,
            [(owner(0), first.clone()), (owner(1), second.clone())].into(),
            cap,
        );
        assert_eq!(
            history.payload_bytes(),
            second.products.retained_payload_bytes()
        );
        assert_eq!(
            history.candidates(&owner(1)).0.unwrap().products,
            second.products
        );
        assert!(history.candidates(&owner(0)).0.is_none());
        history.commit_with_cap(true, units(&first), cap);
        history.commit_with_cap(true, units(&first), cap);
        assert_eq!(
            history.retained_epochs(),
            1,
            "same opaque owners deduplicate the epoch"
        );
        history.commit_with_cap(true, units(&first), 0);
        assert_eq!(history.payload_bytes(), 0);
        assert!(history.candidates(&owner(0)).0.is_none());
        assert!(history.candidates(&owner(0)).1.is_empty());
    }
}

#[cfg(test)]
mod diagnostic_stream_tests {
    use super::*;
    use std::io::{self, Write};

    #[derive(Default)]
    struct ObservedWriter {
        bytes: Vec<u8>,
        calls: usize,
        fail_write: bool,
        fail_flush: bool,
        max_vectors: usize,
        pointers: Vec<usize>,
        limit: Option<usize>,
        interruptions: usize,
    }
    impl Write for ObservedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.fail_write {
                return Err(io::Error::other("write rejected"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn write_vectored(&mut self, slices: &[io::IoSlice<'_>]) -> io::Result<usize> {
            self.calls += 1;
            self.max_vectors = self.max_vectors.max(slices.len());
            if self.interruptions != 0 {
                self.interruptions -= 1;
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.fail_write {
                return Err(io::Error::other("write rejected"));
            }
            let mut remaining = self.limit.unwrap_or(usize::MAX);
            let mut written = 0;
            for slice in slices {
                self.pointers.push(slice.as_ptr() as usize);
                let amount = remaining.min(slice.len());
                self.bytes.extend_from_slice(&slice[..amount]);
                written += amount;
                remaining -= amount;
                if remaining == 0 {
                    break;
                }
            }
            Ok(written)
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("flush rejected"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn diagnostic_stream_keeps_current_full_bytes_while_bounding_write_aggregation() {
        let fragment = Arc::new("; exact diagnostic fragment\n".repeat(2400));
        let diagnostic = DiagnosticText {
            text: Arc::new(
                Module {
                    triple: "x86_64-unknown-linux-gnu".into(),
                    fingerprint: "prior-fingerprint".into(),
                    items: Vec::new().into(),
                }
                .to_text(),
            ),
            fragments: vec![fragment.clone(); 20].into(),
            fingerprint: Some("current-fingerprint".into()),
        };
        let expected = diagnostic.to_text();
        assert!(expected.len() > 1024 * 1024);
        let mut observed = ObservedWriter::default();
        diagnostic.write_to(&mut observed).unwrap();
        assert_eq!(observed.bytes, expected.as_bytes());
        assert!(observed.max_vectors >= 20);
        assert_eq!(
            observed
                .pointers
                .iter()
                .filter(|&&pointer| pointer == fragment.as_ptr() as usize)
                .count(),
            20,
            "actual retained payload addresses, not an aggregation buffer"
        );
        assert!(
            observed.calls <= 3,
            "must aggregate the original twenty large fragments"
        );
        for (fail_write, fail_flush) in [(true, false), (false, true)] {
            let mut rejected = ObservedWriter {
                fail_write,
                fail_flush,
                ..ObservedWriter::default()
            };
            assert_eq!(
                diagnostic.write_to(&mut rejected).unwrap_err().kind(),
                io::ErrorKind::Other
            );
        }
    }
    #[test]
    fn vectored_diagnostics_handle_partial_interrupted_zero_and_scalar_writers() {
        let diagnostic = DiagnosticText {
            text: Arc::new("header\n".into()),
            fragments: (0..149)
                .map(|index| {
                    Arc::new(if index % 3 == 0 {
                        String::new()
                    } else {
                        format!("fragment {index}\n")
                    })
                })
                .collect(),
            fingerprint: Some("ignored without matching header".into()),
        };
        let expected = diagnostic.to_text();
        for limit in [1, 7, 65, usize::MAX] {
            let mut writer = ObservedWriter {
                limit: Some(limit),
                interruptions: 3,
                ..Default::default()
            };
            diagnostic.write_to(&mut writer).unwrap();
            assert_eq!(writer.bytes, expected.as_bytes());
            assert_eq!(writer.interruptions, 0);
            assert!(writer.max_vectors <= 64);
        }
        let mut zero = ObservedWriter {
            limit: Some(0),
            ..Default::default()
        };
        assert_eq!(
            diagnostic.write_to(&mut zero).unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
        assert!(zero.bytes.is_empty());
        // A Write implementation with only scalar write() still receives every
        // byte through the standard trait's fallback write_vectored method.
        struct Scalar(Vec<u8>);
        impl Write for Scalar {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let count = bytes.len().min(3);
                self.0.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut scalar = Scalar(Vec::new());
        diagnostic.write_to(&mut scalar).unwrap();
        assert_eq!(scalar.0, expected.as_bytes());
    }

    #[test]
    fn persistent_diagnostic_consumers_keep_equal_roots_and_current_ordered_bytes() {
        use zil_llvm_api::model::{Item, Ty};
        let mut module = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "cold".into(),
            items: (0..129)
                .map(|index| Item::NamedType(format!("type{index}"), vec![Ty::Int(32)]))
                .collect(),
        };
        let mut session = BackendSession::default();
        let cold = session.render(&module);
        assert_eq!(cold.to_text(), module.to_text());
        let cold_items = session.diagnostic_items.clone();
        session.begin();
        module.fingerprint = "current".into();
        let equal = session.render(&module);
        assert!(cold.fragments.ptr_eq(&equal.fragments));
        assert!(cold_items.ptr_eq(&session.diagnostic_items));
        assert_eq!(equal.to_text(), module.to_text());
        assert_eq!(session.summary.rendered_diagnostic_items, 0);
        session.begin();
        module.items[64] = Item::NamedType("type64".into(), vec![Ty::Int(64)]);
        let changed = session.render(&module);
        assert_eq!(changed.to_text(), module.to_text());
        assert_eq!(
            changed.fragments.changed_indices(&equal.fragments),
            vec![64]
        );
        assert_eq!(
            session.diagnostic_items.changed_indices(&cold_items),
            vec![64]
        );
        assert_eq!(session.summary.rendered_diagnostic_items, 1);
        assert_eq!(session.summary.reused_diagnostic_items, 128);
        // Every reordered byte follows the current source; old slots are not
        // authority and unchanged names alone never authorize payload reuse.
        module.items.reverse();
        session.begin();
        let reordered = session.render(&module);
        assert_eq!(reordered.to_text(), module.to_text());
        assert_eq!(session.summary.rendered_diagnostic_items, 0);
        module.items.remove(17);
        module
            .items
            .insert(3, Item::NamedType("new".into(), vec![Ty::Ptr]));
        assert_eq!(session.render(&module).to_text(), module.to_text());
        module.items = vec![
            Item::NamedType("duplicate".into(), vec![Ty::Int(8)]),
            Item::NamedType("duplicate".into(), vec![Ty::Int(64)]),
        ]
        .into();
        assert_eq!(session.render(&module).to_text(), module.to_text());
        module.items.reverse();
        assert_eq!(session.render(&module).to_text(), module.to_text());
        module.items.clear();
        assert_eq!(session.render(&module).to_text(), module.to_text());
        assert!(session.diagnostic_items.is_empty());
        assert!(session.diagnostic_fragments.is_empty());
    }
}
